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

#[test]
fn interrupt_masks_are_rejected_from_nested_isr_entries_with_or_without_unwind_runtime() {
    for op in [RuntimeOp::Hold, RuntimeOp::Unhold] {
        for via_callee in [false, true] {
            for with_unwind in [false, true] {
                let mut p = program(vec![]);
                let mut action = simple();
                if via_callee {
                    action.exported = false;
                    action.export_symbol = None;
                    action.name = "action".into();
                }
                let mask = binding(&mut action, None, Value::Runtime { op, args: vec![] });
                action.seqs[action.body.unwrap()].stmts = vec![mask];
                let action = p.functions.alloc(action);
                if via_callee {
                    let mut entry = simple();
                    let call = binding(
                        &mut entry,
                        None,
                        Value::Call {
                            callee: Callee::Direct(action),
                            args: vec![],
                            suspends: false,
                            on_fail: None,
                        },
                    );
                    entry.seqs[entry.body.unwrap()].stmts = vec![call];
                    p.functions.alloc(entry);
                }
                if with_unwind {
                    let mut f = Function::new("other");
                    f.exported = true;
                    f.export_symbol = Some("other".into());
                    f.suspends = true;
                    let body = f.seqs.alloc(Seq {
                        stmts: vec![],
                        term: Term::Yield(None),
                    });
                    let cleanup = f.seqs.alloc(Seq {
                        stmts: vec![],
                        term: Term::Yield(None),
                    });
                    let protect = f.stmts.alloc(Stmt::Protect {
                        dst: None,
                        body,
                        cleanup,
                    });
                    f.body = Some(f.seqs.alloc(Seq {
                        stmts: vec![protect],
                        term: Term::Return(None),
                    }));
                    p.functions.alloc(f);
                }
                let errors = crate::backend::compile_program(
                    p,
                    &Options::default().with_isr_entry("answer"),
                )
                .unwrap_err();
                assert!(
                    errors.iter().any(|error| error
                        .message
                        .contains("fiber interrupt control reachable from ISR entry")),
                    "{op:?}/{via_callee}/{with_unwind}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn structured_fiber_waits_accept_suspending_and_reject_native_callers() {
    for case in ["scope", "join", "await", "interrupt", "deferred_await"] {
        let mut p = program(vec![]);
        let child = p.core.enums.alloc(EnumDef {
            name: "Std.Fiber.Fiber".into(),
            params: 1,
            variants: vec![],
            error: false,
            external: true,
        });
        let child = p.core.types.intern(Type::Enum(child, vec![TypeId::INT]));
        let deferred = p.core.enums.alloc(EnumDef {
            name: "Std.Deferred.Deferred".into(),
            params: 1,
            variants: vec![],
            error: false,
            external: true,
        });
        let deferred = p.core.types.intern(Type::Enum(deferred, vec![TypeId::INT]));
        let cause = p.core.enums.alloc(EnumDef {
            name: "Std.Cause.Cause".into(),
            params: 0,
            variants: vec![],
            error: false,
            external: true,
        });
        let cause = p.core.types.intern(Type::Enum(cause, vec![]));
        let param = p.core.types.intern(Type::Param(0));
        let exit = p.core.enums.alloc(EnumDef {
            name: "Std.Fiber.Exit".into(),
            params: 1,
            variants: vec![
                VariantDef {
                    name: "Success".into(),
                    fields: VariantFields::Positional(1),
                    tys: vec![param],
                },
                VariantDef {
                    name: "Failure".into(),
                    fields: VariantFields::Positional(1),
                    tys: vec![cause],
                },
            ],
            error: false,
            external: false,
        });
        let callback = p.core.types.intern(Type::Fn(FnType {
            params: vec![TypeId::FIBER_SCOPE],
            ret: TypeId::UNIT,
            fails: false,
            suspends: true,
            ev: false,
            sets: None,
        }));
        let mut f = simple();
        f.ret = TypeId::UNIT;
        f.suspends = true;
        let ty = match case {
            "scope" => callback,
            "deferred_await" => deferred,
            _ => child,
        };
        let argument = aggregate_variable(&mut f, ty);
        f.params.push(argument);
        let op = match case {
            "scope" => FiberOp::Scope { fails: false },
            "join" => FiberOp::Join,
            "await" => FiberOp::Await { exit },
            "interrupt" => FiberOp::Interrupt,
            _ => FiberOp::DeferredAwait,
        };
        let wait = binding(
            &mut f,
            None,
            Value::Fiber {
                op,
                args: vec![argument.into()],
                on_fail: None,
            },
        );
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![wait],
            term: Term::Return(None),
        };
        let id = p.functions.alloc(f);
        lower_program(p.clone()).unwrap();
        p.functions[id].suspends = false;
        let errors = lower_program(p).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == ErrorKind::InvalidFlowIr
                    && error
                        .message
                        .contains("fiber wait requires a suspending caller")),
            "{case}: {errors:?}"
        );
    }
}

#[test]
fn cause_raise_requires_a_failure_edge_and_a_propagation_channel() {
    for suspends in [false, true] {
        for propagate in [false, true] {
            let mut p = program(vec![]);
            let cause = p.core.enums.alloc(EnumDef {
                name: "Std.Cause.Cause".into(),
                params: 0,
                variants: vec![],
                error: false,
                external: true,
            });
            let cause = p.core.types.intern(Type::Enum(cause, vec![]));
            let mut f = simple();
            f.ret = TypeId::NEVER;
            f.suspends = suspends;
            let value = aggregate_variable(&mut f, cause);
            f.params.push(value);
            let raise = binding(
                &mut f,
                None,
                Value::Fiber {
                    op: FiberOp::CauseRaise,
                    args: vec![value.into()],
                    on_fail: propagate.then_some(OnFail::Propagate),
                },
            );
            f.seqs[f.body.unwrap()] = Seq {
                stmts: vec![raise],
                term: Term::Unreachable,
            };
            p.functions.alloc(f);
            let errors = lower_program(p).unwrap_err();
            let expected = if propagate {
                "failure propagation has no function failure channel"
            } else {
                "explicit failure edge"
            };
            assert!(
                errors.iter().any(|error| error.message.contains(expected)),
                "{suspends}/{propagate}: {errors:?}"
            );
        }
    }
}
