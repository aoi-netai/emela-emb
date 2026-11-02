//! Fixed-old list aliases and captured failures, with the original small heap budgets.
use super::*;

fn export(name: &str, ret: TypeId) -> Function {
    let mut f = Function::new(name);
    f.exported = true;
    f.export_symbol = Some(name.into());
    f.ret = ret;
    f
}
fn assign(f: &mut Function, stmts: &mut Vec<StmtId>, ty: TypeId, value: Value) -> VarId {
    let dst = if matches!(ty, TypeId::INT | TypeId::BOOL) {
        variable(f, ty)
    } else {
        aggregate_variable(f, ty)
    };
    stmts.push(binding(f, Some(dst), value));
    dst
}
fn finish(f: &mut Function, stmts: Vec<StmtId>, result: Operand) {
    f.body = Some(f.seqs.alloc(Seq {
        stmts,
        term: Term::Return(Some(result)),
    }));
}

#[test]
fn returned_lists_shared_tails_and_inline_list_owners_reuse_the_old_1024_byte_heap() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::INT));
    let units = p.core.types.intern(Type::List(TypeId::UNIT));
    let tuple = p.core.types.intern(Type::Tuple(vec![list]));
    let tuples = p.core.types.intern(Type::List(tuple));
    let mut f = export("make_list", list);
    let mut body = vec![];
    let value = assign(
        &mut f,
        &mut body,
        list,
        Value::List {
            elems: vec![Const::Int(4).into(), Const::Int(5).into()],
            tail: None,
        },
    );
    finish(&mut f, body, value.into());
    p.functions.alloc(f);
    let mut identity = Function::new("identity_list");
    identity.ret = list;
    let parameter = aggregate_variable(&mut identity, list);
    identity.params.push(parameter);
    finish(&mut identity, vec![], parameter.into());
    let identity = p.functions.alloc(identity);
    let mut f = export("sum_first_two", TypeId::INT);
    let mut body = vec![];
    let values = assign(
        &mut f,
        &mut body,
        list,
        Value::List {
            elems: [10, 20, 30].map(|n| Const::Int(n).into()).to_vec(),
            tail: None,
        },
    );
    let alias = assign(
        &mut f,
        &mut body,
        list,
        Value::Call {
            callee: Callee::Direct(identity),
            args: vec![values.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let first = assign(&mut f, &mut body, TypeId::INT, Value::ListHead(values));
    let tail = assign(&mut f, &mut body, list, Value::ListTail(alias));
    let second = assign(&mut f, &mut body, TypeId::INT, Value::ListHead(tail));
    let result = assign(
        &mut f,
        &mut body,
        TypeId::INT,
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![first.into(), second.into()],
        },
    );
    finish(&mut f, body, result.into());
    p.functions.alloc(f);
    let mut f = export("unit_head", TypeId::BOOL);
    let mut body = vec![];
    let value = assign(
        &mut f,
        &mut body,
        units,
        Value::List {
            elems: vec![Const::Unit.into()],
            tail: None,
        },
    );
    body.push(binding(&mut f, None, Value::ListHead(value)));
    finish(&mut f, body, Const::Bool(true).into());
    p.functions.alloc(f);
    for (name, n, nested) in [
        ("tuple_list_head", 42, false),
        ("list_tuple_head", 73, true),
        ("tuple_value", 5, false),
    ] {
        let ret = if name == "tuple_value" {
            tuple
        } else {
            TypeId::INT
        };
        let mut f = export(name, ret);
        let mut body = vec![];
        let inner = assign(
            &mut f,
            &mut body,
            list,
            Value::List {
                elems: vec![Const::Int(n).into()],
                tail: None,
            },
        );
        let mut value = assign(&mut f, &mut body, tuple, Value::Tuple(vec![inner.into()]));
        if nested {
            let outer = assign(
                &mut f,
                &mut body,
                tuples,
                Value::List {
                    elems: vec![value.into()],
                    tail: None,
                },
            );
            value = assign(&mut f, &mut body, tuple, Value::ListHead(outer));
        }
        let result = if ret == tuple {
            value
        } else {
            let extracted = assign(
                &mut f,
                &mut body,
                list,
                Value::TupleGet {
                    base: value,
                    index: 0,
                },
            );
            assign(&mut f, &mut body, TypeId::INT, Value::ListHead(extracted))
        };
        finish(&mut f, body, result.into());
        p.functions.alloc(f);
    }
    let c_type = crate::semantics::c::type_name(tuple);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(1024)).unwrap();
    audit_owned_heap(&mut output);
    let release = output
        .header
        .lines()
        .find(|line| {
            line.starts_with("void emela_embedded_release_value_")
                && line.contains(&format!("{c_type} *"))
        })
        .unwrap()
        .split('(')
        .next()
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap();
    let harness = format!(
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;(void)m;(void)n;abort();}}\nint main(void){{for(int i=0;i<1000;i++){{void*p=make_list();emela_embedded_release(p);if(sum_first_two()!=30||!unit_head()||tuple_list_head()!=42||list_tuple_head()!=73)return 1;{c_type} value=tuple_value();if(value.f0==NULL)return 2;{release}(&value);if(!test_heap_empty())return 3;}}return 0;}}\n"
    );
    execute(&output, &harness, 0);
}

#[test]
fn empty_lists_keep_the_null_c_boundary_without_requiring_a_heap() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::INT));
    for name in ["empty_list", "check_list", "empty_value"] {
        let mut f = export(
            name,
            if name == "empty_value" {
                list
            } else {
                TypeId::BOOL
            },
        );
        let mut body = vec![];
        let value = if name == "check_list" {
            let input = aggregate_variable(&mut f, list);
            f.params.push(input);
            input
        } else {
            assign(
                &mut f,
                &mut body,
                list,
                Value::List {
                    elems: vec![],
                    tail: None,
                },
            )
        };
        let result = if name == "empty_value" {
            value
        } else {
            assign(&mut f, &mut body, TypeId::BOOL, Value::ListIsEmpty(value))
        };
        finish(&mut f, body, result.into());
        p.functions.alloc(f);
    }
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    assert!(!output.c_source.contains("emela_heap_storage"));
    execute(
        &output,
        "#include <stddef.h>\n#include \"emela_embedded.h\"\nint main(void){int n=0;for(int i=0;i<1000;i++)if(!empty_list()||!check_list(NULL)||check_list(&n)||empty_value()!=NULL)return 1;return 0;}\n",
        0,
    );
}

#[test]
fn returning_a_borrowed_list_alias_requires_reference_management_capacity() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::INT));
    let mut f = export("identity_list", list);
    let input = aggregate_variable(&mut f, list);
    f.params.push(input);
    finish(&mut f, vec![], input.into());
    p.functions.alloc(f);
    let errors = crate::backend::compile_program(p, &Options::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == ErrorKind::Unsupported && e.message.contains("heap_bytes = 0")),
        "{errors:?}"
    );
}

#[test]
fn captured_closure_success_and_propagated_failure_use_the_original_indirect_call_abi() {
    let mut p = program(vec![]);
    let error = p.core.enums.alloc(EnumDef {
        name: "ClosureError".into(),
        params: 0,
        error: true,
        external: false,
        variants: vec![VariantDef {
            name: "ClosureError".into(),
            fields: VariantFields::Positional(1),
            tys: vec![TypeId::INT],
        }],
    });
    let error_type = p.core.types.enum_type(error, vec![]);
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![TypeId::INT],
        ret: TypeId::INT,
        fails: true,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut f = export("apply", TypeId::INT);
    f.fails = true;
    let captured = variable(&mut f, TypeId::INT);
    let input = variable(&mut f, TypeId::INT);
    f.params = vec![captured, input];
    let closure_input = variable(&mut f, TypeId::INT);
    let mut body = vec![];
    let zero = assign(
        &mut f,
        &mut body,
        TypeId::BOOL,
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![closure_input.into(), Const::Int(0).into()],
        },
    );
    let mut failed = vec![];
    let failure = assign(
        &mut f,
        &mut failed,
        error_type,
        Value::Make {
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            args: vec![Const::Int(73).into()],
        },
    );
    let failed = f.seqs.alloc(Seq {
        stmts: failed,
        term: Term::Fail(failure.into()),
    });
    let mut success = vec![];
    let sum = assign(
        &mut f,
        &mut success,
        TypeId::INT,
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![captured.into(), closure_input.into()],
        },
    );
    let success = f.seqs.alloc(Seq {
        stmts: success,
        term: Term::Yield(Some(sum.into())),
    });
    let chosen = variable(&mut f, TypeId::INT);
    body.push(f.stmts.alloc(Stmt::If {
        dst: Some(chosen),
        cond: zero.into(),
        then: failed,
        else_: success,
    }));
    let closure_body = f.seqs.alloc(Seq {
        stmts: body,
        term: Term::Return(Some(chosen.into())),
    });
    let mut body = vec![];
    let closure = assign(
        &mut f,
        &mut body,
        ft,
        Value::Closure(Box::new(Closure {
            params: vec![closure_input],
            captures: vec![captured],
            body: closure_body,
            suspends: false,
        })),
    );
    let result = assign(
        &mut f,
        &mut body,
        TypeId::INT,
        Value::Call {
            callee: Callee::Indirect(closure),
            args: vec![input.into()],
            suspends: false,
            on_fail: Some(OnFail::Propagate),
        },
    );
    finish(&mut f, body, result.into());
    p.functions.alloc(f);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(2048)).unwrap();
    audit_owned_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic int armed;\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)m;(void)n;exit(armed&&k==EMELA_DEFECT_UNHANDLED_ERROR?0:3);}\nint main(void){for(int i=0;i<1000;i++)if(apply(40,2)!=42||!test_heap_empty())return 1;armed=1;(void)apply(40,0);return 2;}\n",
        0,
    );
}
