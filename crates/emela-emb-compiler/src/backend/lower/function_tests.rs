use super::*;

fn reference_var(function: &mut Function, ty: TypeId) -> VarId {
    function.vars.alloc(VarData {
        hint: "function".into(),
        repr: Repr::Ref,
        ty,
    })
}

#[test]
fn returned_function_references_remain_reachable_and_indirect_calls_use_their_abi() {
    let mut p = program(vec![]);
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![TypeId::UNIT, TypeId::INT],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut add = Function::new("add_one");
    add.ret = TypeId::INT;
    let input = variable(&mut add, TypeId::INT);
    add.params.push(input);
    let result = variable(&mut add, TypeId::INT);
    let statement = binding(
        &mut add,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![input.into(), Const::Int(1).into()],
        },
    );
    add.body = Some(add.seqs.alloc(Seq {
        stmts: vec![statement],
        term: Term::Return(Some(result.into())),
    }));
    let add_id = p.functions.alloc(add);
    let mut factory = Function::new("factory");
    factory.ret = ft;
    let reference = reference_var(&mut factory, ft);
    let statement = binding(&mut factory, Some(reference), Value::FnRef(add_id));
    factory.body = Some(factory.seqs.alloc(Seq {
        stmts: vec![statement],
        term: Term::Return(Some(reference.into())),
    }));
    let factory_id = p.functions.alloc(factory);
    let mut answer = simple();
    let reference = reference_var(&mut answer, ft);
    let result = variable(&mut answer, TypeId::INT);
    let make = binding(
        &mut answer,
        Some(reference),
        Value::Call {
            callee: Callee::Direct(factory_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(reference),
            args: vec![Const::Int(41).into()],
            suspends: false,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![make, call],
        term: Term::Return(Some(result.into())),
    };
    let answer_id = p.functions.alloc(answer);
    let output = crate::backend::compile_program(p.clone(), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42)return 1;return 0;}\n",
        0,
    );
    let mut bad = p.clone();
    if let Stmt::Let {
        value: Value::Call { args, .. },
        ..
    } = &mut bad.functions[answer_id].stmts[call]
    {
        args.clear();
    }
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("indirect call argument count")
    );
    let mut bad = p;
    if let Stmt::Let {
        value: Value::Call { on_fail, .. },
        ..
    } = &mut bad.functions[answer_id].stmts[call]
    {
        *on_fail = Some(OnFail::Propagate);
    }
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("indirect call channels")
    );
}

#[test]
fn returned_closures_own_captured_string_views_and_release_every_call() {
    let mut p = program(vec![]);
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::STRING,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut maker = Function::new("make_reader");
    maker.ret = ft;
    let text = reference_var(&mut maker, TypeId::STRING);
    maker.params.push(text);
    let closure = reference_var(&mut maker, ft);
    let closure_body = maker.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(text.into())),
    });
    let make = binding(
        &mut maker,
        Some(closure),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![text],
            body: closure_body,
            suspends: false,
        })),
    );
    maker.body = Some(maker.seqs.alloc(Seq {
        stmts: vec![make],
        term: Term::Return(Some(closure.into())),
    }));
    let maker_id = p.functions.alloc(maker);
    let mut smash = Function::new("smash");
    smash.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("smash".into()),
        }],
    });
    let smash_id = p.functions.alloc(smash);
    let mut answer = simple();
    answer.ret = TypeId::BOOL;
    let input = reference_var(&mut answer, TypeId::STRING);
    answer.params.push(input);
    let closure = reference_var(&mut answer, ft);
    let text = reference_var(&mut answer, TypeId::STRING);
    let result = variable(&mut answer, TypeId::BOOL);
    let make = binding(
        &mut answer,
        Some(closure),
        Value::Call {
            callee: Callee::Direct(maker_id),
            args: vec![input.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let smash = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(smash_id),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let call = binding(
        &mut answer,
        Some(text),
        Value::Call {
            callee: Callee::Indirect(closure),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let compare = binding(
        &mut answer,
        Some(result),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::String,
            args: vec![text.into(), Const::String("abc".into()).into()],
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![make, smash, call, compare],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    output.files.iter_mut().find(|file| file.path=="emela_embedded.c").unwrap().contents.push_str("\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic char input[3];\nint test_heap_empty(void);\nvoid smash(void){memset(input,'x',3);}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){memcpy(input,\"abc\",3);if(!answer((EmelaString){input,3,NULL})||!test_heap_empty())return 1;}return 0;}\n",
        0,
    );
}

#[test]
fn loop_variable_captures_use_the_value_at_closure_creation() {
    let mut p = program(vec![]);
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut f = simple();
    let count = variable(&mut f, TypeId::INT);
    let initial = binding(&mut f, Some(count), Value::Const(Const::Int(0)));
    let done = variable(&mut f, TypeId::BOOL);
    let next = variable(&mut f, TypeId::INT);
    let result = variable(&mut f, TypeId::INT);
    let closure = reference_var(&mut f, ft);
    let closure_body = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(count.into())),
    });
    let make = binding(
        &mut f,
        Some(closure),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![count],
            body: closure_body,
            suspends: false,
        })),
    );
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(closure),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let yes = f.seqs.alloc(Seq {
        stmts: vec![make, call],
        term: Term::Yield(Some(result.into())),
    });
    let increment = binding(
        &mut f,
        Some(next),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(1).into()],
        },
    );
    let no = f.seqs.alloc(Seq {
        stmts: vec![increment],
        term: Term::Recur(vec![next.into()]),
    });
    let condition = binding(
        &mut f,
        Some(done),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(42).into()],
        },
    );
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
    let loop_statement = f.stmts.alloc(Stmt::Loop {
        dst: Some(result),
        vars: vec![count],
        inits: vec![count.into()],
        body,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![initial, loop_statement],
        term: Term::Return(Some(result.into())),
    };
    let function = p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p.clone(), &Options::default().with_heap_bytes(4096))
            .unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42)return 1;return 0;}\n",
        0,
    );
    let mut bad = p.clone();
    if let Stmt::Let {
        value: Value::Closure(closure),
        ..
    } = &mut bad.functions[function].stmts[make]
    {
        closure.captures.clear();
    }
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("before its definition")
    );
    let mut bad = p;
    if let Stmt::Let {
        value: Value::Closure(closure),
        ..
    } = &mut bad.functions[function].stmts[make]
    {
        closure.body = body;
    }
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("cyclic sequence")
    );
}

#[test]
fn function_coercion_adapts_unit_erasure_evidence_and_failure_abi() {
    let mut p = program(vec![]);
    let pure = FnType {
        params: vec![TypeId::UNIT, TypeId::INT],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    };
    let pure_type = p.core.types.intern(Type::Fn(pure.clone()));
    let adapted_type = p.core.types.intern(Type::Fn(FnType {
        fails: true,
        ev: true,
        ..pure
    }));
    let mut identity = Function::new("identity");
    identity.ret = TypeId::INT;
    let input = variable(&mut identity, TypeId::INT);
    identity.params.push(input);
    identity.body = Some(identity.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(input.into())),
    }));
    let identity = p.functions.alloc(identity);
    let mut f = simple();
    let source = reference_var(&mut f, pure_type);
    let destination = reference_var(&mut f, adapted_type);
    let evidence = reference_var(&mut f, TypeId::EVIDENCE);
    let error = reference_var(&mut f, TypeId::ANY);
    let result = variable(&mut f, TypeId::INT);
    let reference = binding(&mut f, Some(source), Value::FnRef(identity));
    let coercion = binding(&mut f, Some(destination), Value::Coerce(source));
    let evidence_binding = binding(
        &mut f,
        Some(evidence),
        Value::EvExtend {
            base: None,
            bindings: vec![],
        },
    );
    let handler = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(-1).into())),
    });
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(destination),
            args: vec![Const::Int(42).into(), evidence.into()],
            suspends: false,
            on_fail: Some(OnFail::Handle {
                error,
                body: handler,
            }),
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![reference, coercion, evidence_binding, call],
        term: Term::Return(Some(result.into())),
    };
    let function = p.functions.alloc(f);
    let mut output =
        crate::backend::compile_program(p.clone(), &Options::default().with_heap_bytes(4096))
            .unwrap();
    output.files.iter_mut().find(|file| file.path=="emela_embedded.c").unwrap().contents.push_str("\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
    let mut bad = p;
    let function = &mut bad.functions[function];
    let narrowed = reference_var(function, pure_type);
    let coercion = binding(function, Some(narrowed), Value::Coerce(destination));
    function.seqs[function.body.unwrap()]
        .stmts
        .insert(2, coercion);
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("function coercion")
    );
}

#[test]
fn never_returning_function_values_widen_results_without_losing_failure_channels() {
    for fails in [false, true] {
        let mut p = program(vec![]);
        let bottom_type = p.core.types.intern(Type::Fn(FnType {
            params: vec![],
            ret: TypeId::NEVER,
            fails,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let integer_type = p.core.types.intern(Type::Fn(FnType {
            params: vec![],
            ret: TypeId::INT,
            fails,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let mut bottom = Function::new("bottom");
        bottom.ret = TypeId::NEVER;
        bottom.fails = fails;
        let term = if fails {
            let error = p.core.enums.alloc(EnumDef {
                name: "Stopped".into(),
                params: 0,
                error: true,
                external: false,
                variants: vec![VariantDef {
                    name: "Stopped".into(),
                    fields: VariantFields::Unit,
                    tys: vec![],
                }],
            });
            let error_type = p.core.types.intern(Type::Enum(error, vec![]));
            let value = reference_var(&mut bottom, error_type);
            let make = binding(
                &mut bottom,
                Some(value),
                Value::Make {
                    ctor: CtorRef {
                        enum_id: error,
                        variant: 0,
                    },
                    args: vec![],
                },
            );
            bottom.body = Some(bottom.seqs.alloc(Seq {
                stmts: vec![make],
                term: Term::Fail(value.into()),
            }));
            None
        } else {
            Some(Term::Defect(Const::String("bottom".into()).into()))
        };
        if let Some(term) = term {
            bottom.body = Some(bottom.seqs.alloc(Seq {
                stmts: vec![],
                term,
            }));
        }
        let bottom = p.functions.alloc(bottom);
        let mut f = simple();
        let source = reference_var(&mut f, bottom_type);
        let destination = reference_var(&mut f, integer_type);
        let result = variable(&mut f, TypeId::INT);
        let reference = binding(&mut f, Some(source), Value::FnRef(bottom));
        let coercion = binding(&mut f, Some(destination), Value::Coerce(source));
        let edge = if fails {
            let error = reference_var(&mut f, TypeId::ANY);
            let body = f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Yield(Some(Const::Int(42).into())),
            });
            Some(OnFail::Handle { error, body })
        } else {
            None
        };
        let call = binding(
            &mut f,
            Some(result),
            Value::Call {
                callee: Callee::Indirect(destination),
                args: vec![],
                suspends: false,
                on_fail: edge,
            },
        );
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![reference, coercion, call],
            term: Term::Return(Some(result.into())),
        };
        p.functions.alloc(f);
        let output =
            crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
        if fails {
            execute(&output, DEFECT_HARNESS, 0);
        } else {
            execute(
                &output,
                "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){exit(k==EMELA_DEFECT_PANIC&&n==6&&memcmp(m,\"bottom\",6)==0?77:78);}\nint main(void){(void)answer();return 1;}\n",
                77,
            );
        }
    }
}
