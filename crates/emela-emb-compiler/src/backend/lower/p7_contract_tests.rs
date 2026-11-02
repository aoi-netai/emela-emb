//! Input residuals remain representable, while local failure handlers are normalized.
use super::*;

#[test]
fn unwind_environment_values_and_refcount_statements_are_rejected_as_input() {
    for suspends in [false, true] {
        for variant in [
            "Unwind",
            "MakeClosure",
            "EnvGet",
            "Dup",
            "Drop",
            "Function.env",
        ] {
            let mut p = program(vec![]);
            let mut target = Function::new("target");
            let env = aggregate_variable(&mut target, TypeId::ANY);
            target.params.push(env);
            target.body = Some(target.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Return(None),
            }));
            let target = p.functions.alloc(target);
            let mut f = simple();
            f.ret = TypeId::UNIT;
            f.suspends = suspends;
            let env = aggregate_variable(&mut f, TypeId::ANY);
            f.params.push(env);
            let statement = match variant {
                "Unwind" => None,
                "MakeClosure" => Some(binding(
                    &mut f,
                    None,
                    Value::MakeClosure {
                        function: target,
                        captures: vec![],
                    },
                )),
                "EnvGet" => {
                    let dst = variable(&mut f, TypeId::INT);
                    Some(binding(&mut f, Some(dst), Value::EnvGet { env, index: 0 }))
                }
                "Dup" => Some(f.stmts.alloc(Stmt::Dup(env))),
                "Drop" => Some(f.stmts.alloc(Stmt::Drop(env))),
                "Function.env" => {
                    f.env = Some(vec![TypeId::INT]);
                    None
                }
                _ => unreachable!(),
            };
            f.seqs[f.body.unwrap()] = Seq {
                stmts: statement.into_iter().collect(),
                term: if variant == "Unwind" {
                    Term::Unwind
                } else {
                    Term::Return(None)
                },
            };
            p.functions.alloc(f);
            let errors = crate::backend::compile_program(p, &Options::default()).unwrap_err();
            let expected = match variant {
                "Unwind" => "terminal variant",
                "MakeClosure" | "EnvGet" => "value variant",
                "Function.env" => "explicit closure environment",
                _ => "statement variant",
            };
            assert!(
                errors
                    .iter()
                    .any(|error| error.kind == ErrorKind::Unsupported
                        && error.message.contains(expected)),
                "{variant}/{suspends}: {errors:?}"
            );
        }
    }
}

#[test]
fn local_failure_handlers_are_validated_and_normalized_in_sync_and_suspend_calls() {
    for suspends in [false, true] {
        let mut p = program(vec![]);
        let error = p.core.enums.alloc(EnumDef {
            name: "LocalFailure".into(),
            params: 0,
            variants: vec![VariantDef {
                name: "LocalFailure".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            }],
            error: true,
            external: false,
        });
        let error_ty = p.core.types.intern(Type::Enum(error, vec![]));
        let mut callee = Function::new("callee");
        callee.ret = TypeId::INT;
        callee.fails = true;
        callee.suspends = suspends;
        let bad = variable(&mut callee, TypeId::BOOL);
        callee.params.push(bad);
        let payload = aggregate_variable(&mut callee, error_ty);
        let make = binding(
            &mut callee,
            Some(payload),
            Value::Make {
                ctor: CtorRef {
                    enum_id: error,
                    variant: 0,
                },
                args: vec![],
            },
        );
        let fail = callee.seqs.alloc(Seq {
            stmts: vec![make],
            term: Term::Fail(payload.into()),
        });
        let good = callee.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(Const::Int(42).into())),
        });
        let choose = callee.stmts.alloc(Stmt::If {
            dst: None,
            cond: bad.into(),
            then: fail,
            else_: good,
        });
        callee.body = Some(callee.seqs.alloc(Seq {
            stmts: vec![choose],
            term: Term::Unreachable,
        }));
        let callee = p.functions.alloc(callee);
        let mut mark = Function::new("handled");
        mark.external = Some(External {
            bindings: vec![ExternalBinding {
                target: "c".into(),
                host: HostRef::Global("handled".into()),
            }],
        });
        let mark = p.functions.alloc(mark);
        let mut f = simple();
        f.suspends = suspends;
        let bad = variable(&mut f, TypeId::BOOL);
        f.params.push(bad);
        let caught = aggregate_variable(&mut f, TypeId::ANY);
        let result = variable(&mut f, TypeId::INT);
        let mark = binding(
            &mut f,
            None,
            Value::Call {
                callee: Callee::Direct(mark),
                args: vec![],
                suspends: false,
                on_fail: None,
            },
        );
        let handler = f.seqs.alloc(Seq {
            stmts: vec![mark],
            term: Term::Yield(Some(Const::Int(42).into())),
        });
        let call = binding(
            &mut f,
            Some(result),
            Value::Call {
                callee: Callee::Direct(callee),
                args: vec![bad.into()],
                suspends,
                on_fail: Some(OnFail::Handle {
                    error: caught,
                    body: handler,
                }),
            },
        );
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![call],
            term: Term::Return(Some(result.into())),
        };
        let answer = p.functions.alloc(f);
        for malformed in 0..3 {
            let mut invalid = p.clone();
            let f = &mut invalid.functions[answer];
            match malformed {
                0 => f.vars[caught].ty = TypeId::INT,
                1 => f.seqs[handler].term = Term::Yield(Some(Const::String("wrong".into()).into())),
                _ => f.seqs[f.body.unwrap()].term = Term::Return(Some(caught.into())),
            };
            assert!(
                super::lower_program(invalid).is_err(),
                "malformed={malformed},suspends={suspends}"
            );
        }
        let lowered = super::lower_program(p.clone()).unwrap();
        assert!(
            lowered
                .lowered
                .functions
                .iter()
                .all(|(_, f)| f.stmts.iter().all(|(_, stmt)| !matches!(
                    stmt,
                    Stmt::Let {
                        value: Value::Call {
                            on_fail: Some(OnFail::Handle { .. }),
                            ..
                        } | Value::Fiber {
                            on_fail: Some(OnFail::Handle { .. }),
                            ..
                        },
                        ..
                    }
                )))
        );
        let mut output =
            crate::backend::compile_program(p, &Options::default().with_heap_bytes(2048)).unwrap();
        audit_owned_heap(&mut output);
        let invoke = if suspends {
            "int32_t result=0;EmelaFiber f=answer_start(bad,&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))abort();emela_fiber_release(f);if(result!=42)abort();"
        } else {
            "if(answer(bad)!=42)abort();"
        };
        let harness = format!(
            "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);static int calls;void handled(void){{calls++;}}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;(void)m;(void)n;abort();}}\nint main(void){{for(int i=0;i<1000;i++){{for(int bad=0;bad<2;bad++){{{invoke}if(calls!=i+bad||!test_heap_empty())return 1;}}}}return calls!=1000;}}\n"
        );
        execute(&output, &harness, 0);
    }
}
