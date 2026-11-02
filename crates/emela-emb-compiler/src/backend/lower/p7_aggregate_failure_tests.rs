use super::*;

#[test]
fn owned_any_nested_in_error_payloads_survives_indirect_success_and_failure() {
    let mut p = program(vec![]);
    let pair = p
        .core
        .types
        .intern(Type::Tuple(vec![TypeId::ANY, TypeId::BOOL]));
    let error = p.core.enums.alloc(EnumDef {
        name: "Missing".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Missing".into(),
            fields: VariantFields::Positional(1),
            tys: vec![pair],
        }],
        error: true,
        external: false,
    });
    let error_ty = p.core.types.intern(Type::Enum(error, vec![]));
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![pair, TypeId::BOOL],
        ret: pair,
        fails: true,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut checked = Function::new("checked");
    checked.ret = pair;
    checked.fails = true;
    let value = aggregate_variable(&mut checked, pair);
    let bad = variable(&mut checked, TypeId::BOOL);
    checked.params = vec![value, bad];
    let payload = aggregate_variable(&mut checked, error_ty);
    let make = binding(
        &mut checked,
        Some(payload),
        Value::Make {
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            args: vec![value.into()],
        },
    );
    let fail = checked.seqs.alloc(Seq {
        stmts: vec![make],
        term: Term::Fail(payload.into()),
    });
    let good = checked.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(value.into())),
    });
    let branch = checked.stmts.alloc(Stmt::If {
        dst: None,
        cond: bad.into(),
        then: fail,
        else_: good,
    });
    checked.body = Some(checked.seqs.alloc(Seq {
        stmts: vec![branch],
        term: Term::Unreachable,
    }));
    let checked = p.functions.alloc(checked);
    let mut f = simple();
    let bad = variable(&mut f, TypeId::BOOL);
    f.params.push(bad);
    let number = variable(&mut f, TypeId::INT);
    let erased = aggregate_variable(&mut f, TypeId::ANY);
    let value = aggregate_variable(&mut f, pair);
    let callback = aggregate_variable(&mut f, ft);
    let result = aggregate_variable(&mut f, pair);
    let caught = aggregate_variable(&mut f, TypeId::ANY);
    let error_value = aggregate_variable(&mut f, error_ty);
    let recovered = aggregate_variable(&mut f, pair);
    let constant = binding(&mut f, Some(number), Value::Const(Const::Int(42)));
    let box_ = binding(&mut f, Some(erased), Value::Coerce(number));
    let make = binding(
        &mut f,
        Some(value),
        Value::Tuple(vec![erased.into(), Const::Bool(true).into()]),
    );
    let reference = binding(&mut f, Some(callback), Value::FnRef(checked));
    let exit = f.labels.alloc(LabelData {
        hint: "result".into(),
    });
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(callback),
            args: vec![value.into(), bad.into()],
            suspends: false,
            on_fail: Some(OnFail::Break(exit)),
        },
    );
    let boxed_result = aggregate_variable(&mut f, TypeId::ANY);
    let box_result = binding(&mut f, Some(boxed_result), Value::Coerce(result));
    let call_body = f.seqs.alloc(Seq {
        stmts: vec![call, box_result],
        term: Term::Break {
            label: exit,
            value: Some(boxed_result.into()),
        },
    });
    let block = f.stmts.alloc(Stmt::Block {
        dst: Some(caught),
        label: exit,
        body: call_body,
    });
    let is_error = variable(&mut f, TypeId::BOOL);
    let check = binding(
        &mut f,
        Some(is_error),
        Value::IsError {
            error: caught,
            enum_id: error,
            error_type: error_ty,
        },
    );
    let unpack = binding(
        &mut f,
        Some(error_value),
        Value::AsError {
            error: caught,
            enum_id: error,
            error_type: error_ty,
        },
    );
    let field = binding(
        &mut f,
        Some(recovered),
        Value::Field {
            base: error_value,
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            index: 0,
        },
    );
    let fail_body = f.seqs.alloc(Seq {
        stmts: vec![unpack, field],
        term: Term::Yield(Some(recovered.into())),
    });
    let normal_pair = aggregate_variable(&mut f, pair);
    let out = aggregate_variable(&mut f, pair);
    let unbox_pair = binding(&mut f, Some(normal_pair), Value::Coerce(caught));
    let success_body = f.seqs.alloc(Seq {
        stmts: vec![unbox_pair],
        term: Term::Yield(Some(normal_pair.into())),
    });
    let choose = f.stmts.alloc(Stmt::If {
        dst: Some(out),
        cond: is_error.into(),
        then: fail_body,
        else_: success_body,
    });
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let number = variable(&mut f, TypeId::INT);
    let first = binding(
        &mut f,
        Some(boxed),
        Value::TupleGet {
            base: out,
            index: 0,
        },
    );
    let unbox = binding(&mut f, Some(number), Value::Coerce(boxed));
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![
            constant, box_, make, reference, block, check, choose, first, unbox,
        ],
        term: Term::Return(Some(number.into())),
    };
    p.functions.alloc(f);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(2048)).unwrap();
    audit_owned_heap(&mut output);

    execute(
        &output,
        "#include <stdlib.h>\n#include <stdio.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,\"defect %d: %.*s\\n\",k,n,m);abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer(i%2)!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}

#[test]
fn growing_generic_arguments_through_function_results_keep_owned_lazy_payloads() {
    let mut p = program(vec![]);
    let grow = p.core.enums.alloc(EnumDef {
        name: "Grow".into(),
        params: 1,
        variants: vec![],
        error: false,
        external: false,
    });
    let a = p.core.types.intern(Type::Param(0));
    let list_a = p.core.types.intern(Type::List(a));
    let larger_a = p.core.types.intern(Type::Enum(grow, vec![list_a]));
    let ft_a = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: larger_a,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    p.core.enums[grow].variants = vec![
        VariantDef {
            name: "Step".into(),
            fields: VariantFields::Positional(1),
            tys: vec![ft_a],
        },
        VariantDef {
            name: "End".into(),
            fields: VariantFields::Unit,
            tys: vec![],
        },
    ];
    let root = p.core.types.intern(Type::Enum(grow, vec![TypeId::INT]));
    let list_int = p.core.types.intern(Type::List(TypeId::INT));
    let larger = p.core.types.intern(Type::Enum(grow, vec![list_int]));
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: larger,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut identity = Function::new("identity");
    identity.exported = true;
    identity.export_symbol = Some("keep".into());
    identity.ret = root;
    let input = aggregate_variable(&mut identity, root);
    identity.params.push(input);
    identity.body = Some(identity.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(input.into())),
    }));
    let identity = p.functions.alloc(identity);
    let mut stop = Function::new("stop");
    stop.ret = larger;
    let end = aggregate_variable(&mut stop, larger);
    let make = binding(
        &mut stop,
        Some(end),
        Value::Make {
            ctor: CtorRef {
                enum_id: grow,
                variant: 1,
            },
            args: vec![],
        },
    );
    stop.body = Some(stop.seqs.alloc(Seq {
        stmts: vec![make],
        term: Term::Return(Some(end.into())),
    }));
    let stop = p.functions.alloc(stop);
    let mut f = simple();
    let callback = aggregate_variable(&mut f, ft);
    let step = aggregate_variable(&mut f, root);
    let projected = aggregate_variable(&mut f, ft);
    let result = aggregate_variable(&mut f, larger);
    let tag = variable(&mut f, TypeId::INT);
    let answer = variable(&mut f, TypeId::INT);
    let a = binding(&mut f, Some(callback), Value::FnRef(stop));
    let b = binding(
        &mut f,
        Some(step),
        Value::Make {
            ctor: CtorRef {
                enum_id: grow,
                variant: 0,
            },
            args: vec![callback.into()],
        },
    );
    let copy = aggregate_variable(&mut f, root);
    let copy_node = binding(
        &mut f,
        Some(copy),
        Value::Call {
            callee: Callee::Direct(identity),
            args: vec![step.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let c = binding(
        &mut f,
        Some(projected),
        Value::Field {
            base: copy,
            ctor: CtorRef {
                enum_id: grow,
                variant: 0,
            },
            index: 0,
        },
    );
    let d = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(projected),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let e = binding(&mut f, Some(tag), Value::Tag(result));
    let out = binding(
        &mut f,
        Some(answer),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![tag.into(), Const::Int(41).into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![a, b, copy_node, c, d, e, out],
        term: Term::Return(Some(answer.into())),
    };
    p.functions.alloc(f);
    let lowered = lower_program(p.clone()).unwrap();
    assert!(lowered.lowered.core.expanding_enums.contains(&grow));
    assert!(lowered.lowered.core.types.len() < 64);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    audit_owned_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}
