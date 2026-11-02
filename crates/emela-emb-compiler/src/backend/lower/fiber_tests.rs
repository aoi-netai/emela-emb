use super::*;

#[path = "fiber_deferred_tests.rs"]
mod deferred;
#[path = "fiber_scope_tests.rs"]
mod scope_abi;
#[path = "fiber_source_tests.rs"]
mod source_observations;
#[path = "fiber_unit_tests.rs"]
mod unit;

#[test]
fn raw_interrupt_masks_are_unit_operations_and_are_rejected_from_isr_entries() {
    let mut f = simple();
    let hold = binding(
        &mut f,
        None,
        Value::Runtime {
            op: RuntimeOp::Hold,
            args: vec![],
        },
    );
    let unhold = binding(
        &mut f,
        None,
        Value::Runtime {
            op: RuntimeOp::Unhold,
            args: vec![],
        },
    );
    f.seqs[f.body.unwrap()].stmts = vec![hold, unhold];
    let p = program(vec![f]);
    let options = Options::default();
    let output = crate::backend::compile_program(p.clone(), &options).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
    assert!(!output.c_source.contains("emela_frame_storage"));
    let errors =
        crate::backend::compile_program(p.clone(), &options.with_isr_entry("answer")).unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("fiber interrupt control reachable from ISR entry")
    }));
    for destination in [false, true] {
        let mut invalid = p.clone();
        let function = invalid.functions.iter_mut().next().unwrap().1;
        let dst = destination.then(|| variable(function, TypeId::UNIT));
        if let Stmt::Let {
            dst: target,
            value: Value::Runtime { args, .. },
            ..
        } = &mut function.stmts[hold]
        {
            *target = dst;
            if !destination {
                args.push(Const::Unit.into());
            }
        }
        assert!(lower_program(invalid).unwrap_err().iter().any(|error| {
            error
                .message
                .contains("Hold/Unhold require no arguments or destination")
        }));
    }
}

fn function_type(p: &mut Program, params: Vec<TypeId>, ret: TypeId, fails: bool) -> TypeId {
    p.core.types.intern(Type::Fn(FnType {
        params,
        ret,
        fails,
        suspends: true,
        ev: false,
        sets: None,
    }))
}
fn handle_type(p: &mut Program, name: &str, value: TypeId) -> TypeId {
    let id = p.core.enums.alloc(EnumDef {
        name: name.into(),
        params: 1,
        variants: vec![],
        error: false,
        external: true,
    });
    p.core.types.intern(Type::Enum(id, vec![value]))
}
fn audit_heap(output: &mut Output) {
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
}
fn execute_answer(mut output: Output) {
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);if(result!=42||!test_heap_empty())return 2;}return 0;}\n",
        0,
    );
}
fn scope_function(p: &mut Program, f: &mut Function, scope_body: SeqId, scope: VarId) {
    let ty = function_type(p, vec![TypeId::FIBER_SCOPE], TypeId::INT, false);
    let body = aggregate_variable(f, ty);
    let result = variable(f, TypeId::INT);
    let make = binding(
        f,
        Some(body),
        Value::Closure(Box::new(Closure {
            params: vec![scope],
            captures: vec![],
            body: scope_body,
            suspends: true,
        })),
    );
    let enter = binding(
        f,
        Some(result),
        Value::Fiber {
            op: FiberOp::Scope { fails: false },
            args: vec![body.into()],
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![make, enter],
        term: Term::Return(Some(result.into())),
    };
}

#[test]
fn scope_fork_and_join_reclaim_children_and_their_erased_values() {
    let mut p = program(vec![]);
    let child_type = function_type(&mut p, vec![], TypeId::ANY, false);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::INT);
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let value = variable(&mut f, TypeId::INT);
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let make_value = binding(&mut f, Some(value), Value::Const(Const::Int(42)));
    let box_value = binding(&mut f, Some(boxed), Value::Coerce(value));
    let child_body = f.seqs.alloc(Seq {
        stmts: vec![make_value, box_value],
        term: Term::Return(Some(boxed.into())),
    });
    let child = aggregate_variable(&mut f, child_type);
    let handle = aggregate_variable(&mut f, handle_type);
    let joined = variable(&mut f, TypeId::INT);
    let make_child = binding(
        &mut f,
        Some(child),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![],
            body: child_body,
            suspends: true,
        })),
    );
    let fork = binding(
        &mut f,
        Some(handle),
        Value::Fiber {
            op: FiberOp::Fork,
            args: vec![scope.into(), child.into()],
            on_fail: None,
        },
    );
    let join = binding(
        &mut f,
        Some(joined),
        Value::Fiber {
            op: FiberOp::Join,
            args: vec![handle.into()],
            on_fail: None,
        },
    );
    let scope_body = f.seqs.alloc(Seq {
        stmts: vec![make_child, fork, join],
        term: Term::Return(Some(joined.into())),
    });
    scope_function(&mut p, &mut f, scope_body, scope);
    p.functions.alloc(f);
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(1024),
    )
    .unwrap();
    execute_answer(output);
}

fn check_bool(f: &mut Function, value: VarId, expected: bool) -> StmtId {
    let good = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(None),
    });
    let bad = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Defect(Const::String("unexpected Deferred completion".into()).into()),
    });
    f.stmts.alloc(Stmt::If {
        dst: None,
        cond: value.into(),
        then: if expected { good } else { bad },
        else_: if expected { bad } else { good },
    })
}

fn check_option(f: &mut Function, value: VarId, option: EnumId, expected: Option<i32>) -> StmtId {
    let bad = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Defect(Const::String("unexpected Deferred poll".into()).into()),
    });
    let mut stmts = vec![];
    if let Some(expected) = expected {
        let payload = variable(f, TypeId::INT);
        let equal = variable(f, TypeId::BOOL);
        stmts.push(binding(
            f,
            Some(payload),
            Value::Field {
                base: value,
                ctor: CtorRef {
                    enum_id: option,
                    variant: 0,
                },
                index: 0,
            },
        ));
        stmts.push(binding(
            f,
            Some(equal),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::I32,
                args: vec![payload.into(), Const::Int(expected).into()],
            },
        ));
        stmts.push(check_bool(f, equal, true));
    }
    let good = f.seqs.alloc(Seq {
        stmts,
        term: Term::Yield(None),
    });
    f.stmts.alloc(Stmt::Switch {
        dst: None,
        on: SwitchOn::Tag(value),
        cases: vec![Case {
            key: CaseKey::Variant(if expected.is_some() { 0 } else { 1 }),
            body: good,
        }],
        default: Some(bad),
    })
}

#[test]
fn deferred_complete_poll_and_await_share_one_typed_value_until_scope_closes() {
    let mut p = program(vec![]);
    let deferred_type = handle_type(&mut p, "Std.Deferred.Deferred", TypeId::INT);
    let param = p.core.types.intern(Type::Param(0));
    let option = p.core.enums.alloc(EnumDef {
        name: "Option".into(),
        params: 1,
        variants: vec![
            VariantDef {
                name: "Some".into(),
                fields: VariantFields::Positional(1),
                tys: vec![param],
            },
            VariantDef {
                name: "None".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
        error: false,
        external: false,
    });
    let option_type = p.core.types.intern(Type::Enum(option, vec![TypeId::INT]));
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let deferred = aggregate_variable(&mut f, deferred_type);
    let empty = aggregate_variable(&mut f, option_type);
    let filled = aggregate_variable(&mut f, option_type);
    let first = variable(&mut f, TypeId::BOOL);
    let again = variable(&mut f, TypeId::BOOL);
    let value = variable(&mut f, TypeId::INT);
    let make = binding(
        &mut f,
        Some(deferred),
        Value::Fiber {
            op: FiberOp::Deferred,
            args: vec![scope.into()],
            on_fail: None,
        },
    );
    let poll_empty = binding(
        &mut f,
        Some(empty),
        Value::Fiber {
            op: FiberOp::Poll,
            args: vec![deferred.into()],
            on_fail: None,
        },
    );
    let complete = binding(
        &mut f,
        Some(first),
        Value::Fiber {
            op: FiberOp::Complete,
            args: vec![deferred.into(), Const::Int(42).into()],
            on_fail: None,
        },
    );
    let duplicate = binding(
        &mut f,
        Some(again),
        Value::Fiber {
            op: FiberOp::Complete,
            args: vec![deferred.into(), Const::Int(99).into()],
            on_fail: None,
        },
    );
    let poll = binding(
        &mut f,
        Some(filled),
        Value::Fiber {
            op: FiberOp::Poll,
            args: vec![deferred.into()],
            on_fail: None,
        },
    );
    let first_ok = check_bool(&mut f, first, true);
    let duplicate_ok = check_bool(&mut f, again, false);
    let empty_ok = check_option(&mut f, empty, option, None);
    let filled_ok = check_option(&mut f, filled, option, Some(42));
    let await_ = binding(
        &mut f,
        Some(value),
        Value::Fiber {
            op: FiberOp::DeferredAwait,
            args: vec![deferred.into()],
            on_fail: None,
        },
    );
    let scope_body = f.seqs.alloc(Seq {
        stmts: vec![
            make,
            poll_empty,
            empty_ok,
            complete,
            duplicate,
            poll,
            filled_ok,
            first_ok,
            duplicate_ok,
            await_,
        ],
        term: Term::Return(Some(value.into())),
    });
    scope_function(&mut p, &mut f, scope_body, scope);
    p.functions.alloc(f);
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_frame_bytes(1024)
        .with_deferred_capacity(1);
    let output = crate::backend::compile_program(p.clone(), &options).unwrap();
    execute_answer(output);
    assert!(
        crate::backend::compile_program(p, &options.with_deferred_capacity(0))
            .unwrap_err()
            .iter()
            .any(|error| error.kind == crate::ErrorKind::InvalidOptions)
    );
}

fn exit_types(p: &mut Program, value: TypeId) -> (TypeId, EnumId, TypeId, EnumId, TypeId) {
    let cause = p.core.enums.alloc(EnumDef {
        name: "Std.Cause.Cause".into(),
        params: 0,
        variants: vec![],
        error: false,
        external: true,
    });
    let cause_type = p.core.types.intern(Type::Enum(cause, vec![]));
    let parameter = p.core.types.intern(Type::Param(0));
    let exit = p.core.enums.alloc(EnumDef {
        name: "Exit".into(),
        params: 1,
        variants: vec![
            VariantDef {
                name: "Success".into(),
                fields: VariantFields::Positional(1),
                tys: vec![parameter],
            },
            VariantDef {
                name: "Failure".into(),
                fields: VariantFields::Positional(1),
                tys: vec![cause_type],
            },
        ],
        error: false,
        external: false,
    });
    let exit_type = p.core.types.intern(Type::Enum(exit, vec![value]));
    let kind = p.core.enums.alloc(EnumDef {
        name: "CauseKind".into(),
        params: 0,
        variants: ["Fail", "Unused", "Interrupt"]
            .map(|name| VariantDef {
                name: name.into(),
                fields: VariantFields::Unit,
                tys: vec![],
            })
            .to_vec(),
        error: false,
        external: false,
    });
    let kind_type = p.core.types.intern(Type::Enum(kind, vec![]));
    (cause_type, exit, exit_type, kind, kind_type)
}

#[test]
fn attempts_preserve_success_and_typed_cause_failures_across_raise() {
    for suspends in [false, true] {
        for fails in [false, true] {
            let mut p = program(vec![]);
            let (cause_type, exit, exit_type, kind, kind_type) = exit_types(&mut p, TypeId::INT);
            let error = p.core.enums.alloc(EnumDef {
                name: "Failed".into(),
                params: 0,
                variants: vec![VariantDef {
                    name: "Failed".into(),
                    fields: VariantFields::Positional(1),
                    tys: vec![TypeId::INT],
                }],
                error: true,
                external: false,
            });
            let error_type = p.core.types.intern(Type::Enum(error, vec![]));
            let mut show = Function::new("show_failed");
            show.ret = TypeId::STRING;
            let shown = aggregate_variable(&mut show, error_type);
            show.params.push(shown);
            show.body = Some(show.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Return(Some(Const::String("Failed(42)".into()).into())),
            }));
            let show = p.functions.alloc(show);
            p.error_shows.push((error, show));
            let body_type = p.core.types.intern(Type::Fn(FnType {
                params: vec![],
                ret: TypeId::INT,
                fails,
                suspends,
                ev: false,
                sets: None,
            }));
            let mut f = simple();
            f.suspends = suspends;
            let payload = aggregate_variable(&mut f, error_type);
            let make_error = binding(
                &mut f,
                Some(payload),
                Value::Make {
                    ctor: CtorRef {
                        enum_id: error,
                        variant: 0,
                    },
                    args: vec![Const::Int(42).into()],
                },
            );
            let body = f.seqs.alloc(Seq {
                stmts: if fails { vec![make_error] } else { vec![] },
                term: if fails {
                    Term::Fail(payload.into())
                } else {
                    Term::Return(Some(Const::Int(42).into()))
                },
            });
            let closure = aggregate_variable(&mut f, body_type);
            let make_body = binding(
                &mut f,
                Some(closure),
                Value::Closure(Box::new(Closure {
                    params: vec![],
                    captures: vec![],
                    body,
                    suspends,
                })),
            );
            let outcome = aggregate_variable(&mut f, exit_type);
            let attempt = binding(
                &mut f,
                Some(outcome),
                Value::Fiber {
                    op: FiberOp::Attempt { exit, suspends },
                    args: vec![closure.into()],
                    on_fail: None,
                },
            );
            let result = variable(&mut f, TypeId::INT);
            let value = binding(
                &mut f,
                Some(result),
                Value::Field {
                    base: outcome,
                    ctor: CtorRef {
                        enum_id: exit,
                        variant: 0,
                    },
                    index: 0,
                },
            );
            let success = f.seqs.alloc(Seq {
                stmts: vec![value],
                term: Term::Return(Some(result.into())),
            });
            let cause = aggregate_variable(&mut f, cause_type);
            let cause_kind = f.vars.alloc(VarData {
                hint: "kind".into(),
                ty: kind_type,
                repr: Repr::I32,
            });
            let extract_cause = binding(
                &mut f,
                Some(cause),
                Value::Field {
                    base: outcome,
                    ctor: CtorRef {
                        enum_id: exit,
                        variant: 1,
                    },
                    index: 0,
                },
            );
            let get_kind = binding(
                &mut f,
                Some(cause_kind),
                Value::Fiber {
                    op: FiberOp::CauseKind { kind },
                    args: vec![cause.into()],
                    on_fail: None,
                },
            );
            let rendered = aggregate_variable(&mut f, TypeId::STRING);
            let render = binding(
                &mut f,
                Some(rendered),
                Value::Fiber {
                    op: FiberOp::CauseToString,
                    args: vec![cause.into()],
                    on_fail: None,
                },
            );
            let correct_text = variable(&mut f, TypeId::BOOL);
            let compare_text = binding(
                &mut f,
                Some(correct_text),
                Value::Prim {
                    op: PrimOp::Eq,
                    ty: PrimTy::String,
                    args: vec![
                        rendered.into(),
                        Const::String("error: Failed(42)".into()).into(),
                    ],
                },
            );
            let text_ok = check_bool(&mut f, correct_text, true);
            let caught = aggregate_variable(&mut f, TypeId::ANY);
            let unpacked = aggregate_variable(&mut f, error_type);
            let unpack = binding(
                &mut f,
                Some(unpacked),
                Value::AsError {
                    error: caught,
                    enum_id: error,
                    error_type,
                },
            );
            let field = binding(
                &mut f,
                Some(result),
                Value::Field {
                    base: unpacked,
                    ctor: CtorRef {
                        enum_id: error,
                        variant: 0,
                    },
                    index: 0,
                },
            );
            let handler = f.seqs.alloc(Seq {
                stmts: vec![unpack, field],
                term: Term::Return(Some(result.into())),
            });
            let raise = binding(
                &mut f,
                None,
                Value::Fiber {
                    op: FiberOp::CauseRaise,
                    args: vec![cause.into()],
                    on_fail: Some(OnFail::Handle {
                        error: caught,
                        body: handler,
                    }),
                },
            );
            let raised = f.seqs.alloc(Seq {
                stmts: vec![raise],
                term: Term::Unreachable,
            });
            let unexpected = f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Defect(Const::String("unexpected Cause kind".into()).into()),
            });
            let check_kind = f.stmts.alloc(Stmt::Switch {
                dst: None,
                on: SwitchOn::Tag(cause_kind),
                cases: vec![Case {
                    key: CaseKey::Variant(CAUSE_FAIL),
                    body: raised,
                }],
                default: Some(unexpected),
            });
            let failure = f.seqs.alloc(Seq {
                stmts: vec![
                    extract_cause,
                    get_kind,
                    render,
                    compare_text,
                    text_ok,
                    check_kind,
                ],
                term: Term::Unreachable,
            });
            let choose = f.stmts.alloc(Stmt::Switch {
                dst: None,
                on: SwitchOn::Tag(outcome),
                cases: vec![Case {
                    key: CaseKey::Variant(usize::from(fails)),
                    body: if fails { failure } else { success },
                }],
                default: Some(unexpected),
            });
            f.seqs[f.body.unwrap()] = Seq {
                stmts: vec![make_body, attempt, choose],
                term: Term::Unreachable,
            };
            p.functions.alloc(f);
            let mut output = crate::backend::compile_program(
                p,
                &Options::default()
                    .with_heap_bytes(8192)
                    .with_frame_bytes(1024),
            )
            .unwrap();
            if suspends {
                execute_answer(output);
            } else {
                audit_heap(&mut output);
                execute(
                    &output,
                    "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
                    0,
                );
            }
        }
    }
}

fn wait_task(p: &mut Program) -> FnId {
    let mut f = Function::new("wait");
    f.suspends = true;
    f.ret = TypeId::UNIT;
    let argument = variable(&mut f, TypeId::INT);
    f.params.push(argument);
    f.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("wait".into()),
        }],
    });
    p.functions.alloc(f)
}

fn wait_at(f: &mut Function, wait: FnId, marker: i32) -> StmtId {
    binding(
        f,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(marker).into()],
            suspends: true,
            on_fail: None,
        },
    )
}

#[test]
fn interruption_waits_for_nested_suspending_cleanup_before_await_reports_its_cause() {
    let mut p = program(vec![]);
    let wait = wait_task(&mut p);
    let child_type = function_type(&mut p, vec![], TypeId::ANY, false);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::STRING);
    let (cause_type, exit, exit_type, kind, kind_type) = exit_types(&mut p, TypeId::STRING);
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let captured = aggregate_variable(&mut f, TypeId::STRING);
    let make_text = binding(
        &mut f,
        Some(captured),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![Const::Int(42).into()],
        },
    );
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let wait_body = wait_at(&mut f, wait, 1);
    let box_value = binding(&mut f, Some(boxed), Value::Coerce(captured));
    let body = f.seqs.alloc(Seq {
        stmts: vec![wait_body, box_value],
        term: Term::Return(Some(boxed.into())),
    });
    let wait_inner = wait_at(&mut f, wait, 98);
    let inner_cleanup = f.seqs.alloc(Seq {
        stmts: vec![wait_inner],
        term: Term::Yield(None),
    });
    let inner = f.stmts.alloc(Stmt::Protect {
        dst: None,
        body,
        cleanup: inner_cleanup,
    });
    let outer_body = f.seqs.alloc(Seq {
        stmts: vec![inner],
        term: Term::Unreachable,
    });
    let wait_outer = wait_at(&mut f, wait, 99);
    let outer_cleanup = f.seqs.alloc(Seq {
        stmts: vec![wait_outer],
        term: Term::Yield(None),
    });
    let outer = f.stmts.alloc(Stmt::Protect {
        dst: None,
        body: outer_body,
        cleanup: outer_cleanup,
    });
    let child_body = f.seqs.alloc(Seq {
        stmts: vec![outer],
        term: Term::Unreachable,
    });
    let child = aggregate_variable(&mut f, child_type);
    let make_child = binding(
        &mut f,
        Some(child),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![captured],
            body: child_body,
            suspends: true,
        })),
    );
    let handle = aggregate_variable(&mut f, handle_type);
    let fork = binding(
        &mut f,
        Some(handle),
        Value::Fiber {
            op: FiberOp::Fork,
            args: vec![scope.into(), child.into()],
            on_fail: None,
        },
    );
    let wait_parent = wait_at(&mut f, wait, 2);
    let interrupt = binding(
        &mut f,
        None,
        Value::Fiber {
            op: FiberOp::Interrupt,
            args: vec![handle.into()],
            on_fail: None,
        },
    );
    let outcome = aggregate_variable(&mut f, exit_type);
    let await_ = binding(
        &mut f,
        Some(outcome),
        Value::Fiber {
            op: FiberOp::Await { exit },
            args: vec![handle.into()],
            on_fail: None,
        },
    );
    let cause = aggregate_variable(&mut f, cause_type);
    let extract = binding(
        &mut f,
        Some(cause),
        Value::Field {
            base: outcome,
            ctor: CtorRef {
                enum_id: exit,
                variant: EXIT_FAILURE,
            },
            index: 0,
        },
    );
    let cause_kind = f.vars.alloc(VarData {
        hint: "kind".into(),
        ty: kind_type,
        repr: Repr::I32,
    });
    let get_kind = binding(
        &mut f,
        Some(cause_kind),
        Value::Fiber {
            op: FiberOp::CauseKind { kind },
            args: vec![cause.into()],
            on_fail: None,
        },
    );
    let rendered = aggregate_variable(&mut f, TypeId::STRING);
    let render = binding(
        &mut f,
        Some(rendered),
        Value::Fiber {
            op: FiberOp::CauseToString,
            args: vec![cause.into()],
            on_fail: None,
        },
    );
    let same = variable(&mut f, TypeId::BOOL);
    let compare = binding(
        &mut f,
        Some(same),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::String,
            args: vec![rendered.into(), Const::String("interrupted".into()).into()],
        },
    );
    let text_ok = check_bool(&mut f, same, true);
    let done = f.seqs.alloc(Seq {
        stmts: vec![render, compare, text_ok],
        term: Term::Return(Some(Const::Int(42).into())),
    });
    let bad = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Defect(Const::String("expected interrupted child".into()).into()),
    });
    let check_kind = f.stmts.alloc(Stmt::Switch {
        dst: None,
        on: SwitchOn::Tag(cause_kind),
        cases: vec![Case {
            key: CaseKey::Variant(CAUSE_INTERRUPT),
            body: done,
        }],
        default: Some(bad),
    });
    let failed = f.seqs.alloc(Seq {
        stmts: vec![extract, get_kind, check_kind],
        term: Term::Unreachable,
    });
    let check_exit = f.stmts.alloc(Stmt::Switch {
        dst: None,
        on: SwitchOn::Tag(outcome),
        cases: vec![Case {
            key: CaseKey::Variant(EXIT_FAILURE),
            body: failed,
        }],
        default: Some(bad),
    });
    let scope_body = f.seqs.alloc(Seq {
        stmts: vec![
            make_text,
            make_child,
            fork,
            wait_parent,
            interrupt,
            await_,
            check_exit,
        ],
        term: Term::Unreachable,
    });
    scope_function(&mut p, &mut f, scope_body, scope);
    p.functions.alloc(f);
    let mut output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(16384)
            .with_frame_bytes(2048)
            .with_task_adapter("wait", "board_wait_start"),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask tasks[100];static int seen[100];\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t marker){if(marker<0||marker>=100)abort();tasks[marker]=t;seen[marker]++;}\nint main(void){for(int i=0;i<1000;i++){memset(seen,0,sizeof seen);int32_t result=0;EmelaFiber root=answer_start(&result);while(emela_scheduler_run(1)){}if(seen[1]!=1||seen[2]!=1||seen[98]||seen[99]||emela_fiber_done(root)||result)return 1;if(!emela_task_complete_unit(tasks[2]))return 2;while(emela_scheduler_run(1)){}if(seen[98]!=1||seen[99]||emela_fiber_done(root)||result||emela_task_complete_unit(tasks[1]))return 3;if(!emela_task_complete_unit(tasks[98]))return 4;while(emela_scheduler_run(1)){}if(seen[99]!=1||emela_fiber_done(root)||result)return 5;if(!emela_task_complete_unit(tasks[99]))return 6;while(!emela_fiber_done(root))if(!emela_scheduler_run(1))return 7;emela_fiber_release(root);if(result!=42||!test_heap_empty())return 8;}return 0;}\n",
        0,
    );
}
