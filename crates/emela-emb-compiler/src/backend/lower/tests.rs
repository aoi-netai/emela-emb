use super::*;
use crate::{Options, Output};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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
