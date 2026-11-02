use super::*;
use crate::{Options, Output};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "cell_tests.rs"]
mod cells;
#[path = "fiber_tests.rs"]
mod fibers;
#[path = "function_tests.rs"]
mod functions;
#[path = "handler_tests.rs"]
mod handlers;
#[path = "isr_tests.rs"]
mod isr;
#[path = "memory_tests.rs"]
mod memory;
#[path = "p7_aggregate_failure_tests.rs"]
mod p7_aggregate_failures;
#[path = "p7_binding_tests.rs"]
mod p7_bindings;
#[path = "p7_cause_source_seam_tests.rs"]
mod p7_cause_source_seams;
#[path = "p7_cleanup_source_seam_tests.rs"]
mod p7_cleanup_source_seams;
#[path = "p7_const_source_tests.rs"]
mod p7_const_source;
#[path = "p7_contract_tests.rs"]
mod p7_contract;
#[path = "p7_data_tests.rs"]
mod p7_data;
#[path = "p7_discarded_tests.rs"]
mod p7_discarded;
#[path = "p7_evidence_tests.rs"]
mod p7_evidence;
#[path = "p7_input_tests.rs"]
mod p7_input;
#[path = "p7_integration_tests.rs"]
mod p7_integration;
#[path = "p7_list_alias_tests.rs"]
mod p7_list_aliases;
#[path = "p7_misc_value_tests.rs"]
mod p7_misc_values;
#[path = "p7_recursive_source_tests.rs"]
mod p7_recursive_source;
#[path = "p7_source_seams.rs"]
mod p7_source_seams;
#[path = "p7_string_frame_tests.rs"]
mod p7_string_frames;
#[path = "p7_switch_frame_tests.rs"]
mod p7_switch_frames;
#[path = "service_tests.rs"]
mod services;
#[path = "source_builtin_tests.rs"]
mod source_builtins;
#[path = "suspend_tests.rs"]
mod suspend;

fn lower_program(program: Program) -> Result<LoweredModule> {
    super::lower_program(program, &Options::default())
}
fn program(functions: Vec<Function>) -> Program {
    let mut program = Program {
        core: Definitions::default(),
        functions: Default::default(),
        error_shows: vec![],
        const_function_groups: vec![],
        has_scopes: false,
    };
    program.functions.extend(functions);
    program
}
fn variable(f: &mut Function, ty: TypeId) -> VarId {
    let repr = match ty {
        TypeId::INT | TypeId::BOOL => Repr::I32,
        TypeId::INT64 => Repr::I64,
        TypeId::FLOAT => Repr::F64,
        _ => Repr::Void,
    };
    f.vars.alloc(VarData {
        hint: String::new(),
        ty,
        repr,
    })
}
fn simple() -> Function {
    let mut f = Function::new("answer");
    f.exported = true;
    f.export_symbol = Some("answer".into());
    f.ret = TypeId::INT;
    f.body = Some(f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    }));
    f
}
fn binding(f: &mut Function, dst: Option<VarId>, value: Value) -> StmtId {
    f.stmts.alloc(Stmt::Let {
        dst,
        value,
        on_defect: None,
        on_suspend: None,
    })
}

#[test]
fn rejects_invalid_ids_types_and_sequence_cycles_before_planning() {
    let mut f = simple();
    let out_of_bounds = f.vars.alloc(VarData {
        hint: String::new(),
        ty: TypeId::INT,
        repr: Repr::I32,
    });
    f.vars = Default::default();
    f.seqs[f.body.unwrap()].term = Term::Return(Some(out_of_bounds.into()));
    assert!(
        lower_program(program(vec![f])).unwrap_err()[0]
            .message
            .contains("out of bounds")
    );
    let mut f = simple();
    f.seqs[f.body.unwrap()].term = Term::Return(Some(Const::Bool(true).into()));
    assert!(
        lower_program(program(vec![f])).unwrap_err()[0]
            .message
            .contains("type mismatch")
    );
    let mut f = simple();
    let body = f.body.unwrap();
    let branch = f.stmts.alloc(Stmt::If {
        dst: None,
        cond: Const::Bool(true).into(),
        then: body,
        else_: body,
    });
    f.seqs[body].stmts.push(branch);
    assert!(
        lower_program(program(vec![f])).unwrap_err()[0]
            .message
            .contains("cyclic sequence")
    );
}

#[test]
fn rejects_calls_with_wrong_arity_and_unbound_variables() {
    let mut p = program(vec![simple()]);
    let callee = p.functions.iter().next().unwrap().0;
    let mut caller = Function::new("caller");
    caller.ret = TypeId::INT;
    let result = variable(&mut caller, TypeId::INT);
    let stmt = binding(
        &mut caller,
        Some(result),
        Value::Call {
            callee: Callee::Direct(callee),
            args: vec![Const::Int(1).into()],
            suspends: false,
            on_fail: None,
        },
    );
    caller.body = Some(caller.seqs.alloc(Seq {
        stmts: vec![stmt],
        term: Term::Return(Some(result.into())),
    }));
    p.functions.alloc(caller);
    assert!(
        lower_program(p).unwrap_err()[0]
            .message
            .contains("argument count")
    );
    let mut f = simple();
    let unbound = variable(&mut f, TypeId::INT);
    f.seqs[f.body.unwrap()].term = Term::Return(Some(unbound.into()));
    assert!(
        lower_program(program(vec![f])).unwrap_err()[0]
            .message
            .contains("before its definition")
    );
}

#[test]
fn keeps_only_functions_reachable_from_exports_and_constructs_positions() {
    let mut hidden = simple();
    hidden.exported = false;
    hidden.export_symbol = None;
    hidden.name = "unused".into();
    let module = lower_program(program(vec![simple(), hidden])).unwrap();
    assert_eq!(module.emitted_functions.len(), 1);
    let f = &module.lowered.functions[module.emitted_functions.iter().copied().next().unwrap()];
    assert_eq!(f.sequence_positions.len(), 1);
    assert!(f.active_statements.is_empty());
}

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn execute(output: &Output, harness: &str, expected: i32) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = TempDir(std::env::temp_dir().join(format!(
        "emela-emb-lower-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&temp.0).unwrap();
    for file in &output.files {
        std::fs::write(temp.0.join(&file.path), &file.contents).unwrap();
    }
    std::fs::write(temp.0.join("harness.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "emela_embedded.c",
                "harness.c",
                "-lm",
                "-o",
                "program",
            ])
            .current_dir(&temp.0)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(temp.0.join("program")).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn tail_recursion_uses_parallel_parameter_assignment_and_scalar_external_calls() {
    let mut sum = Function::new("sum");
    sum.ret = TypeId::INT;
    let n = variable(&mut sum, TypeId::INT);
    let acc = variable(&mut sum, TypeId::INT);
    sum.params = vec![n, acc];
    let done = variable(&mut sum, TypeId::BOOL);
    let next = variable(&mut sum, TypeId::INT);
    let total = variable(&mut sum, TypeId::INT);
    let condition = binding(
        &mut sum,
        Some(done),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![n.into(), Const::Int(0).into()],
        },
    );
    let decrement = binding(
        &mut sum,
        Some(next),
        Value::Prim {
            op: PrimOp::Sub,
            ty: PrimTy::I32,
            args: vec![n.into(), Const::Int(1).into()],
        },
    );
    let add = binding(
        &mut sum,
        Some(total),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![acc.into(), n.into()],
        },
    );
    let yes = sum.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(acc.into())),
    });
    let no = sum.seqs.alloc(Seq {
        stmts: vec![decrement, add],
        term: Term::Recur(vec![next.into(), total.into()]),
    });
    let branch = sum.stmts.alloc(Stmt::If {
        dst: None,
        cond: done.into(),
        then: yes,
        else_: no,
    });
    sum.body = Some(sum.seqs.alloc(Seq {
        stmts: vec![condition, branch],
        term: Term::Unreachable,
    }));
    let mut p = program(vec![sum]);
    let sum_id = p.functions.iter().next().unwrap().0;
    let mut external = Function::new("increment");
    external.ret = TypeId::INT;
    let param = variable(&mut external, TypeId::INT);
    external.params.push(param);
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("increment".into()),
        }],
    });
    let external_id = p.functions.alloc(external);
    let mut answer = simple();
    let result = variable(&mut answer, TypeId::INT);
    let incremented = variable(&mut answer, TypeId::INT);
    let sum_call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Direct(sum_id),
            args: vec![Const::Int(100_000).into(), Const::Int(0).into()],
            suspends: false,
            on_fail: None,
        },
    );
    let increment_call = binding(
        &mut answer,
        Some(incremented),
        Value::Call {
            callee: Callee::Direct(external_id),
            args: vec![result.into()],
            suspends: false,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![sum_call, increment_call],
        term: Term::Return(Some(incremented.into())),
    };
    p.functions.alloc(answer);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    assert!(output.c_source.contains("emela_recur_tmp_"));
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint32_t increment(int32_t n) { return n + 1; }\n_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; exit(9); }\nint main(void) { return answer() == INT32_C(705082705) ? 0 : 1; }\n",
        0,
    );
}

#[test]
fn unused_checked_builtin_still_reports_a_defect() {
    let mut f = simple();
    let shift = binding(
        &mut f,
        None,
        Value::Builtin {
            op: Builtin::IntShiftLeft,
            args: vec![Const::Int(1).into(), Const::Int(-1).into()],
        },
    );
    f.seqs[f.body.unwrap()].stmts.push(shift);
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)m; (void)n; exit(k == EMELA_DEFECT_NEGATIVE_SHIFT ? 7 : 8); }\nint main(void) { return answer(); }\n",
        7,
    );
}

const DEFECT_HARNESS: &str = "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; exit(9); }\nint main(void) { return answer() == INT32_C(42) ? 0 : 1; }\n";

#[test]
fn block_yield_and_break_to_enclosing_block_execute() {
    for breaks in [false, true] {
        let mut f = simple();
        let result = variable(&mut f, TypeId::INT);
        let outer_label = f.labels.alloc(LabelData {
            hint: "outer".into(),
        });
        let block_body = if breaks {
            let inner_label = f.labels.alloc(LabelData {
                hint: "inner".into(),
            });
            let inner_body = f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Break {
                    label: outer_label,
                    value: Some(Const::Int(42).into()),
                },
            });
            let inner_block = f.stmts.alloc(Stmt::Block {
                dst: None,
                label: inner_label,
                body: inner_body,
            });
            f.seqs.alloc(Seq {
                stmts: vec![inner_block],
                term: Term::Unreachable,
            })
        } else {
            f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Yield(Some(Const::Int(42).into())),
            })
        };
        let block = f.stmts.alloc(Stmt::Block {
            dst: Some(result),
            label: outer_label,
            body: block_body,
        });
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![block],
            term: Term::Return(Some(result.into())),
        };
        let output =
            crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
        execute(&output, DEFECT_HARNESS, 0);
    }
}

#[test]
fn loop_initial_values_yield_and_multiple_recur_variables_execute() {
    let mut f = simple();
    let result = variable(&mut f, TypeId::INT);
    let left = variable(&mut f, TypeId::INT);
    let right = variable(&mut f, TypeId::INT);
    let count = variable(&mut f, TypeId::INT);
    let done = variable(&mut f, TypeId::BOOL);
    let next = variable(&mut f, TypeId::INT);
    let product = variable(&mut f, TypeId::INT);
    let total = variable(&mut f, TypeId::INT);
    let condition = binding(
        &mut f,
        Some(done),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(0).into()],
        },
    );
    let decrement = binding(
        &mut f,
        Some(next),
        Value::Prim {
            op: PrimOp::Sub,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(1).into()],
        },
    );
    let multiply = binding(
        &mut f,
        Some(product),
        Value::Prim {
            op: PrimOp::Mul,
            ty: PrimTy::I32,
            args: vec![left.into(), Const::Int(10).into()],
        },
    );
    let add = binding(
        &mut f,
        Some(total),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![product.into(), right.into()],
        },
    );
    let yes = f.seqs.alloc(Seq {
        stmts: vec![multiply, add],
        term: Term::Yield(Some(total.into())),
    });
    let no = f.seqs.alloc(Seq {
        stmts: vec![decrement],
        term: Term::Recur(vec![right.into(), left.into(), next.into()]),
    });
    let branch = f.stmts.alloc(Stmt::If {
        dst: Some(result),
        cond: done.into(),
        then: yes,
        else_: no,
    });
    let body = f.seqs.alloc(Seq {
        stmts: vec![condition, branch],
        term: Term::Yield(Some(result.into())),
    });
    let statement = f.stmts.alloc(Stmt::Loop {
        dst: Some(result),
        vars: vec![left, right, count],
        inits: vec![
            Const::Int(2).into(),
            Const::Int(4).into(),
            Const::Int(3).into(),
        ],
        body,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![statement],
        term: Term::Return(Some(result.into())),
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
}

#[test]
fn private_entry_remains_reachable_beside_an_export() {
    let mut entry = Function::new("main");
    entry.body = Some(entry.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(None),
    }));
    let output =
        crate::backend::compile_program(program(vec![simple(), entry]), &Options::default())
            .unwrap();
    assert_eq!(output.exports.len(), 1);
    assert!(output.c_source.contains("void emela_embedded_start(void)"));
    let manifest = output
        .files
        .iter()
        .find(|file| file.path == crate::artifact::MANIFEST_FILE)
        .unwrap();
    assert_eq!(
        crate::artifact::Manifest::from_json(&manifest.contents)
            .unwrap()
            .metadata
            .entry
            .as_deref(),
        Some("emela_embedded_start")
    );
}

#[test]
fn unreachable_external_sequences_do_not_trigger_function_recur_wrapping() {
    let mut external = Function::new("external");
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("external".into()),
        }],
    });
    external.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Recur(vec![]),
    });
    let module = lower_program(program(vec![simple(), external])).unwrap();
    assert_eq!(module.emitted_functions.len(), 1);
}

#[test]
fn rejects_explicit_export_and_external_symbol_collisions() {
    let mut external = Function::new("foreign_answer");
    external.ret = TypeId::INT;
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("answer".into()),
        }],
    });
    let mut p = program(vec![external]);
    let callee = p.functions.iter().next().unwrap().0;
    let mut answer = simple();
    let dst = variable(&mut answer, TypeId::INT);
    let call = binding(
        &mut answer,
        Some(dst),
        Value::Call {
            callee: Callee::Direct(callee),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(dst.into())),
    };
    p.functions.alloc(answer);
    let errors = crate::backend::compile_program(p, &Options::default()).unwrap_err();
    assert!(
        errors[0]
            .message
            .contains("conflicts with a generated function")
    );
}

#[test]
fn float_constant_bits_and_non_finite_values_execute() {
    let values = [
        1.25,
        1e100,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    let functions: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let mut f = Function::new(&format!("float_value{index}"));
            f.exported = true;
            f.export_symbol = Some(f.name.clone());
            f.ret = TypeId::FLOAT;
            f.body = Some(f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Return(Some(Const::Float(value).into())),
            }));
            f
        })
        .collect();
    let output = crate::backend::compile_program(program(functions), &Options::default()).unwrap();
    execute(
        &output,
        "#include <math.h>\n#include \"emela_embedded.h\"\nint main(void) {\n if (float_value0() != 1.25 || float_value1() != 1e100) return 1;\n if (float_value2() != 0.0 || !signbit(float_value2())) return 2;\n if (!isinf(float_value3()) || signbit(float_value3())) return 3;\n if (!isinf(float_value4()) || !signbit(float_value4())) return 4;\n return isnan(float_value5()) ? 0 : 5;\n}\n",
        0,
    );
}

#[test]
fn strings_unicode_comparisons_and_float_rounding_execute() {
    let mut f = simple();
    let joined = f.vars.alloc(VarData {
        hint: "joined".into(),
        ty: TypeId::STRING,
        repr: Repr::Ref,
    });
    let count = variable(&mut f, TypeId::INT);
    let rounded = variable(&mut f, TypeId::INT);
    let result = variable(&mut f, TypeId::INT);
    let text = binding(
        &mut f,
        Some(joined),
        Value::Builtin {
            op: Builtin::StringAppend,
            args: vec![
                Const::String("a\u{0301}".into()).into(),
                Const::String("Z".into()).into(),
            ],
        },
    );
    let length = binding(
        &mut f,
        Some(count),
        Value::Builtin {
            op: Builtin::StringLength,
            args: vec![joined.into()],
        },
    );
    let floor = binding(
        &mut f,
        Some(rounded),
        Value::Builtin {
            op: Builtin::FloatFloor,
            args: vec![Const::Float(40.9).into()],
        },
    );
    let add = binding(
        &mut f,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![count.into(), rounded.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![text, length, floor, add],
        term: Term::Return(Some(result.into())),
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
    let mut f = simple();
    f.ret = TypeId::BOOL;
    let result = variable(&mut f, TypeId::BOOL);
    let compare = binding(
        &mut f,
        Some(result),
        Value::Prim {
            op: PrimOp::Lt,
            ty: PrimTy::String,
            args: vec![
                Const::String("a\0x".into()).into(),
                Const::String("a\0y".into()).into(),
            ],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![compare],
        term: Term::Return(Some(result.into())),
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(
        &output,
        &DEFECT_HARNESS.replace("INT32_C(42)", "INT32_C(1)"),
        0,
    );
}

#[test]
fn discarded_string_results_keep_capacity_checks() {
    let mut f = simple();
    let input = f.vars.alloc(VarData {
        hint: "input".into(),
        ty: TypeId::STRING,
        repr: Repr::Ref,
    });
    f.params.push(input);
    let append = binding(
        &mut f,
        None,
        Value::Builtin {
            op: Builtin::StringAppend,
            args: vec![input.into(), Const::String("bb".into()).into()],
        },
    );
    f.seqs[f.body.unwrap()].stmts.push(append);
    let output = crate::backend::compile_program(
        program(vec![f]),
        &Options::default().with_string_capacity(3),
    )
    .unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)m; (void)n; exit(k == EMELA_DEFECT_STRING_CAPACITY ? 7 : 8); }\nint main(void) { return answer((EmelaString){ \"aa\", UINT16_C(2), NULL }); }\n",
        7,
    );
}

fn aggregate_variable(f: &mut Function, ty: TypeId) -> VarId {
    f.vars.alloc(VarData {
        hint: String::new(),
        ty,
        repr: Repr::Ref,
    })
}
#[test]
fn tuple_constructor_projection_and_enum_switch_execute() {
    let mut p = program(vec![]);
    let tuple_ty = p
        .core
        .types
        .intern(Type::Tuple(vec![TypeId::INT, TypeId::BOOL]));
    let enum_id = p.core.enums.alloc(EnumDef {
        name: "Choice".into(),
        params: 0,
        variants: vec![
            VariantDef {
                name: "Number".into(),
                fields: VariantFields::Positional(1),
                tys: vec![TypeId::INT],
            },
            VariantDef {
                name: "Empty".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
        error: false,
        external: false,
    });
    let enum_ty = p.core.types.intern(Type::Enum(enum_id, vec![]));
    let mut f = simple();
    let pair = aggregate_variable(&mut f, tuple_ty);
    let number = variable(&mut f, TypeId::INT);
    let choice = aggregate_variable(&mut f, enum_ty);
    let payload = variable(&mut f, TypeId::INT);
    let result = variable(&mut f, TypeId::INT);
    let tuple = binding(
        &mut f,
        Some(pair),
        Value::Tuple(vec![Const::Int(42).into(), Const::Bool(true).into()]),
    );
    let project = binding(
        &mut f,
        Some(number),
        Value::TupleGet {
            base: pair,
            index: 0,
        },
    );
    let make = binding(
        &mut f,
        Some(choice),
        Value::Make {
            ctor: CtorRef {
                enum_id,
                variant: 0,
            },
            args: vec![number.into()],
        },
    );
    let field = binding(
        &mut f,
        Some(payload),
        Value::Field {
            base: choice,
            ctor: CtorRef {
                enum_id,
                variant: 0,
            },
            index: 0,
        },
    );
    let number_case = f.seqs.alloc(Seq {
        stmts: vec![field],
        term: Term::Yield(Some(payload.into())),
    });
    let empty_case = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(0).into())),
    });
    let switch = f.stmts.alloc(Stmt::Switch {
        dst: Some(result),
        on: SwitchOn::Tag(choice),
        cases: vec![
            Case {
                key: CaseKey::Variant(0),
                body: number_case,
            },
            Case {
                key: CaseKey::Variant(1),
                body: empty_case,
            },
        ],
        default: None,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![tuple, project, make, switch],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
}

#[test]
fn failure_payload_recovery_runs_cleanup_once() {
    let mut p = program(vec![]);
    let error_id = p.core.enums.alloc(EnumDef {
        name: "Failure".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Failure".into(),
            fields: VariantFields::Positional(1),
            tys: vec![TypeId::INT],
        }],
        error: true,
        external: false,
    });
    let error_ty = p.core.types.intern(Type::Enum(error_id, vec![]));
    let mut tick = Function::new("tick");
    tick.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("tick".into()),
        }],
    });
    let tick_id = p.functions.alloc(tick);
    let mut fail = Function::new("fail");
    fail.fails = true;
    fail.ret = TypeId::INT;
    let error_value = aggregate_variable(&mut fail, error_ty);
    let error_make = binding(
        &mut fail,
        Some(error_value),
        Value::Make {
            ctor: CtorRef {
                enum_id: error_id,
                variant: 0,
            },
            args: vec![Const::Int(42).into()],
        },
    );
    let body = fail.seqs.alloc(Seq {
        stmts: vec![error_make],
        term: Term::Fail(error_value.into()),
    });
    let cleanup_call = binding(
        &mut fail,
        None,
        Value::Call {
            callee: Callee::Direct(tick_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let cleanup = fail.seqs.alloc(Seq {
        stmts: vec![cleanup_call],
        term: Term::Yield(None),
    });
    let protect = fail.stmts.alloc(Stmt::Protect {
        dst: None,
        body,
        cleanup,
    });
    fail.body = Some(fail.seqs.alloc(Seq {
        stmts: vec![protect],
        term: Term::Unreachable,
    }));
    let fail_id = p.functions.alloc(fail);
    let mut answer = simple();
    let caught = aggregate_variable(&mut answer, TypeId::ANY);
    let unpacked = aggregate_variable(&mut answer, error_ty);
    let code = variable(&mut answer, TypeId::INT);
    let result = variable(&mut answer, TypeId::INT);
    let unpack = binding(
        &mut answer,
        Some(unpacked),
        Value::AsError {
            error: caught,
            enum_id: error_id,
            error_type: error_ty,
        },
    );
    let field = binding(
        &mut answer,
        Some(code),
        Value::Field {
            base: unpacked,
            ctor: CtorRef {
                enum_id: error_id,
                variant: 0,
            },
            index: 0,
        },
    );
    let handler = answer.seqs.alloc(Seq {
        stmts: vec![unpack, field],
        term: Term::Yield(Some(code.into())),
    });
    let call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Direct(fail_id),
            args: vec![],
            suspends: false,
            on_fail: Some(OnFail::Handle {
                error: caught,
                body: handler,
            }),
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    let answer_id = p.functions.alloc(answer);
    let mut malformed = p.clone();
    if let Stmt::Let {
        value: Value::AsError { error_type, .. },
        ..
    } = &mut malformed.functions[answer_id].stmts[unpack]
    {
        *error_type = TypeId::INT;
    }
    assert!(
        lower_program(malformed).unwrap_err()[0]
            .message
            .contains("error operation type")
    );
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int ticks;\nvoid tick(void) { ++ticks; }\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { (void)k;(void)m;(void)n;exit(9); }\nint main(void) { int value=answer(); return value==42 && ticks==1 ? 0 : 1; }\n",
        0,
    );
}

#[test]
fn rejects_malformed_generic_fields_and_unhandled_failure_channels() {
    let mut p = program(vec![simple()]);
    let param = p.core.types.intern(Type::Param(3));
    let tuple = p.core.types.intern(Type::Tuple(vec![param]));
    p.core.enums.alloc(EnumDef {
        name: "Invalid".into(),
        params: 1,
        variants: vec![VariantDef {
            name: "Invalid".into(),
            fields: VariantFields::Positional(1),
            tys: vec![tuple],
        }],
        error: false,
        external: false,
    });
    assert!(
        lower_program(p).unwrap_err()[0]
            .message
            .contains("undeclared type parameter")
    );
    let mut p = program(vec![]);
    let mut fail = Function::new("fails");
    fail.fails = true;
    fail.body = Some(fail.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(None),
    }));
    let fail_id = p.functions.alloc(fail);
    let mut f = simple();
    let call = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(fail_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()].stmts.push(call);
    p.functions.alloc(f);
    assert!(
        lower_program(p).unwrap_err()[0]
            .message
            .contains("failure edge")
    );
}

#[test]
fn string_parameter_tail_recursion_tracks_loop_ownership() {
    let mut f = simple();
    f.ret = TypeId::BOOL;
    let left = aggregate_variable(&mut f, TypeId::STRING);
    let right = aggregate_variable(&mut f, TypeId::STRING);
    let count = variable(&mut f, TypeId::INT);
    f.params = vec![left, right, count];
    let done = variable(&mut f, TypeId::BOOL);
    let equal = variable(&mut f, TypeId::BOOL);
    let next = variable(&mut f, TypeId::INT);
    let condition = binding(
        &mut f,
        Some(done),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(0).into()],
        },
    );
    let compare = binding(
        &mut f,
        Some(equal),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::String,
            args: vec![left.into(), right.into()],
        },
    );
    let decrement = binding(
        &mut f,
        Some(next),
        Value::Prim {
            op: PrimOp::Sub,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(1).into()],
        },
    );
    let yes = f.seqs.alloc(Seq {
        stmts: vec![compare],
        term: Term::Return(Some(equal.into())),
    });
    let no = f.seqs.alloc(Seq {
        stmts: vec![decrement],
        term: Term::Recur(vec![right.into(), left.into(), next.into()]),
    });
    let branch = f.stmts.alloc(Stmt::If {
        dst: None,
        cond: done.into(),
        then: yes,
        else_: no,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![condition, branch],
        term: Term::Unreachable,
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { (void)k;(void)m;(void)n;exit(9); }\nint main(void) { EmelaString a={\"same\",UINT16_C(4),NULL}; EmelaString b={\"other\",UINT16_C(5),NULL}; if (!answer(a,a,100000)) return 1; return answer(a,b,100001) ? 2 : 0; }\n",
        0,
    );
}

fn runtime_const_program(defect: bool) -> Program {
    let mut p = program(vec![]);
    let mut external = Function::new("read_once");
    external.ret = TypeId::INT;
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("read_once".into()),
        }],
    });
    let external_id = p.functions.alloc(external);
    let mut constant = Function::new("Once");
    constant.ret = TypeId::INT;
    constant.constant = true;
    let value = variable(&mut constant, TypeId::INT);
    let call = binding(
        &mut constant,
        Some(value),
        Value::Call {
            callee: Callee::Direct(external_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let mut statements = vec![call];
    let result = if defect {
        let result = variable(&mut constant, TypeId::INT);
        statements.push(binding(
            &mut constant,
            Some(result),
            Value::Prim {
                op: PrimOp::Div,
                ty: PrimTy::I32,
                args: vec![value.into(), Const::Int(0).into()],
            },
        ));
        result
    } else {
        value
    };
    constant.body = Some(constant.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Return(Some(result.into())),
    }));
    let constant_id = p.functions.alloc(constant);
    let mut answer = simple();
    let first = variable(&mut answer, TypeId::INT);
    let second = variable(&mut answer, TypeId::INT);
    let result = variable(&mut answer, TypeId::INT);
    let a = binding(
        &mut answer,
        Some(first),
        Value::Call {
            callee: Callee::Direct(constant_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let b = binding(
        &mut answer,
        Some(second),
        Value::Call {
            callee: Callee::Direct(constant_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let add = binding(
        &mut answer,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![first.into(), second.into()],
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![a, b, add],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    p
}
#[test]
fn monomorphic_runtime_const_initializes_once() {
    let output =
        crate::backend::compile_program(runtime_const_program(false), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int reads;\nint32_t read_once(void) { ++reads;return INT32_C(21); }\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { (void)k;(void)m;(void)n;exit(9); }\nint main(void) { return answer()==42 && answer()==42 && reads==1 ? 0 : 1; }\n",
        0,
    );
}
#[test]
fn evaluated_const_does_not_emit_unused_initializer_helpers() {
    let mut p = runtime_const_program(false);
    let helper_id = p.functions.iter().next().unwrap().0;
    let helper = &mut p.functions[helper_id];
    helper.external = None;
    helper.body = Some(helper.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(21).into())),
    }));
    let lowered = lower_program(p.clone()).unwrap();
    assert!(!lowered.emitted_functions.contains(&helper_id));
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
}
#[test]
fn runtime_const_defect_is_cached_with_context() {
    let output =
        crate::backend::compile_program(runtime_const_program(true), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <setjmp.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int reads; static jmp_buf target;\nint32_t read_once(void) { ++reads;return INT32_C(21); }\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { const char expected[]=\"while evaluating const Once: division by zero\"; if (k!=EMELA_DEFECT_DIVISION_BY_ZERO || n!=sizeof expected-1 || memcmp(m,expected,n)!=0) exit(9); longjmp(target,1); }\nint main(void) { for (int i=0;i<2;i++) { if (setjmp(target)==0) { (void)answer();return 1; } } return reads==1 ? 0 : 2; }\n",
        0,
    );
}
#[test]
fn static_const_and_aggregate_runtime_const_execute() {
    let mut p = program(vec![]);
    let tuple_ty = p
        .core
        .types
        .intern(Type::Tuple(vec![TypeId::INT, TypeId::BOOL]));
    let mut aggregate = Function::new("Pair");
    aggregate.ret = tuple_ty;
    aggregate.constant = true;
    let tuple = aggregate_variable(&mut aggregate, tuple_ty);
    let make = binding(
        &mut aggregate,
        Some(tuple),
        Value::Tuple(vec![Const::Int(42).into(), Const::Bool(true).into()]),
    );
    aggregate.body = Some(aggregate.seqs.alloc(Seq {
        stmts: vec![make],
        term: Term::Return(Some(tuple.into())),
    }));
    let aggregate_id = p.functions.alloc(aggregate);
    let mut answer = simple();
    let returned = aggregate_variable(&mut answer, tuple_ty);
    let result = variable(&mut answer, TypeId::INT);
    let call = binding(
        &mut answer,
        Some(returned),
        Value::Call {
            callee: Callee::Direct(aggregate_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let get = binding(
        &mut answer,
        Some(result),
        Value::TupleGet {
            base: returned,
            index: 0,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call, get],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
    let mut answer = simple();
    answer.constant = true;
    let result = variable(&mut answer, TypeId::INT);
    let add = binding(
        &mut answer,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![Const::Int(i32::MAX).into(), Const::Int(1).into()],
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![add],
        term: Term::Return(Some(result.into())),
    };
    let output =
        crate::backend::compile_program(program(vec![answer]), &Options::default()).unwrap();
    execute(
        &output,
        &DEFECT_HARNESS.replace("INT32_C(42)", "INT32_MIN"),
        0,
    );
}

#[test]
fn structural_equality_and_show_use_materialized_tuple_layouts() {
    let mut p = program(vec![]);
    let tuple_ty = p
        .core
        .types
        .intern(Type::Tuple(vec![TypeId::INT, TypeId::BOOL]));
    let mut eq = Function::new("tuple_eq");
    eq.ret = TypeId::BOOL;
    eq.derivation = Some(crate::DerivedOperation::Eq);
    eq.external = Some(External { bindings: vec![] });
    let a = aggregate_variable(&mut eq, tuple_ty);
    let b = aggregate_variable(&mut eq, tuple_ty);
    eq.params = vec![a, b];
    let eq_id = p.functions.alloc(eq);
    let mut show = Function::new("tuple_show");
    show.ret = TypeId::STRING;
    show.derivation = Some(crate::DerivedOperation::Show);
    show.external = Some(External { bindings: vec![] });
    let value = aggregate_variable(&mut show, tuple_ty);
    show.params.push(value);
    let show_id = p.functions.alloc(show);
    let mut answer = simple();
    let tuple = aggregate_variable(&mut answer, tuple_ty);
    let equals = variable(&mut answer, TypeId::BOOL);
    let displayed = aggregate_variable(&mut answer, TypeId::STRING);
    let length = variable(&mut answer, TypeId::INT);
    let result = variable(&mut answer, TypeId::INT);
    let make = binding(
        &mut answer,
        Some(tuple),
        Value::Tuple(vec![Const::Int(42).into(), Const::Bool(true).into()]),
    );
    let compare = binding(
        &mut answer,
        Some(equals),
        Value::Call {
            callee: Callee::Direct(eq_id),
            args: vec![tuple.into(), tuple.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let display = binding(
        &mut answer,
        Some(displayed),
        Value::Call {
            callee: Callee::Direct(show_id),
            args: vec![tuple.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let size = binding(
        &mut answer,
        Some(length),
        Value::Builtin {
            op: Builtin::StringByteSize,
            args: vec![displayed.into()],
        },
    );
    let yes = answer.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(length.into())),
    });
    let no = answer.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(0).into())),
    });
    let branch = answer.stmts.alloc(Stmt::If {
        dst: Some(result),
        cond: equals.into(),
        then: yes,
        else_: no,
    });
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![make, compare, display, size, branch],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(
        &output,
        &DEFECT_HARNESS.replace("INT32_C(42)", "INT32_C(10)"),
        0,
    );
}

#[test]
fn unit_enum_uses_the_scalar_tag_representation() {
    let mut p = program(vec![]);
    let enum_id = p.core.enums.alloc(EnumDef {
        name: "Marker".into(),
        params: 0,
        variants: vec![
            VariantDef {
                name: "First".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
            VariantDef {
                name: "Second".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
        error: false,
        external: false,
    });
    let ty = p.core.types.intern(Type::Enum(enum_id, vec![]));
    let mut f = simple();
    let tag = f.vars.alloc(VarData {
        hint: "tag".into(),
        ty,
        repr: Repr::I32,
    });
    let result = variable(&mut f, TypeId::INT);
    let make = binding(
        &mut f,
        Some(tag),
        Value::Make {
            ctor: CtorRef {
                enum_id,
                variant: 1,
            },
            args: vec![],
        },
    );
    let first = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(0).into())),
    });
    let second = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(42).into())),
    });
    let switch = f.stmts.alloc(Stmt::Switch {
        dst: Some(result),
        on: SwitchOn::Tag(tag),
        cases: vec![
            Case {
                key: CaseKey::Variant(0),
                body: first,
            },
            Case {
                key: CaseKey::Variant(1),
                body: second,
            },
        ],
        default: None,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![make, switch],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
}

fn audit_owned_heap(output: &mut Output) {
    if !output.c_source.contains("emela_heap_storage") {
        output
            .files
            .iter_mut()
            .find(|file| file.path == "emela_embedded.c")
            .unwrap()
            .contents
            .push_str("\nint test_heap_empty(void){return 1;}\n");
        return;
    }
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
}
