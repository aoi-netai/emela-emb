use super::*;

fn scope_program() -> Program {
    let mut p = program(vec![]);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::INT);
    let callbacks = [false, true].map(|fails| function_type(&mut p, vec![], TypeId::ANY, fails));
    let child_type = function_type(&mut p, vec![], TypeId::INT, false);
    let effect = p.core.effects.alloc(EffectDef {
        name: "Std.Fiber.Scope[_, _]".into(),
        decl: "Std.Fiber.Scope".into(),
        args: vec![],
        label_of: None,
        ops: vec![
            OpDef {
                name: "fork".into(),
                suspend: false,
                type_params: 1,
                params: vec![callbacks[0]],
                ret: TypeId::ANY,
                fails: false,
            },
            OpDef {
                name: "deferred".into(),
                suspend: false,
                type_params: 1,
                params: vec![],
                ret: TypeId::ANY,
                fails: false,
            },
        ],
    });
    let handler_type = p.core.types.intern(Type::Handler(effect));
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let child = aggregate_variable(&mut f, child_type);
    let child_body = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    });
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
    let mut statements = vec![make_child];
    let mut handles = vec![];
    for callback_type in callbacks {
        let callback = aggregate_variable(&mut f, callback_type);
        let forked = aggregate_variable(&mut f, TypeId::ANY);
        let fork = binding(
            &mut f,
            Some(forked),
            Value::Fiber {
                op: FiberOp::Fork,
                args: vec![scope.into(), callback.into()],
                on_fail: None,
            },
        );
        let body = f.seqs.alloc(Seq {
            stmts: vec![fork],
            term: Term::Return(Some(forked.into())),
        });
        let operation_type = p.core.types.intern(Type::Fn(FnType {
            params: vec![callback_type],
            ret: TypeId::ANY,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let operation = aggregate_variable(&mut f, operation_type);
        statements.push(binding(
            &mut f,
            Some(operation),
            Value::Closure(Box::new(Closure {
                params: vec![callback],
                captures: vec![scope],
                body,
                suspends: false,
            })),
        ));
        let handler = aggregate_variable(&mut f, handler_type);
        let deferred_result = aggregate_variable(&mut f, TypeId::ANY);
        let deferred_create = binding(
            &mut f,
            Some(deferred_result),
            Value::Fiber {
                op: FiberOp::Deferred,
                args: vec![scope.into()],
                on_fail: None,
            },
        );
        let deferred_body = f.seqs.alloc(Seq {
            stmts: vec![deferred_create],
            term: Term::Return(Some(deferred_result.into())),
        });
        let deferred_type = p.core.types.intern(Type::Fn(FnType {
            params: vec![],
            ret: TypeId::ANY,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let deferred_op = aggregate_variable(&mut f, deferred_type);
        statements.push(binding(
            &mut f,
            Some(deferred_op),
            Value::Closure(Box::new(Closure {
                params: vec![],
                captures: vec![scope],
                body: deferred_body,
                suspends: false,
            })),
        ));
        statements.push(binding(
            &mut f,
            Some(handler),
            Value::Handler {
                name: "Scope".into(),
                effect,
                ops: vec![operation, deferred_op],
            },
        ));
        let handle = aggregate_variable(&mut f, handle_type);
        statements.push(binding(
            &mut f,
            Some(handle),
            Value::Call {
                callee: Callee::Op {
                    handler,
                    effect,
                    op: 0,
                },
                args: vec![child.into()],
                suspends: false,
                on_fail: None,
            },
        ));
        handles.push(handle);
    }
    for handle in handles {
        let result = variable(&mut f, TypeId::INT);
        statements.push(binding(
            &mut f,
            Some(result),
            Value::Fiber {
                op: FiberOp::Join,
                args: vec![handle.into()],
                on_fail: None,
            },
        ));
        let equal = variable(&mut f, TypeId::BOOL);
        statements.push(binding(
            &mut f,
            Some(equal),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::I32,
                args: vec![result.into(), Const::Int(42).into()],
            },
        ));
        statements.push(check_bool(&mut f, equal, true));
    }
    let body = f.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Return(Some(Const::Int(42).into())),
    });
    scope_function(&mut p, &mut f, body, scope);
    p.functions.alloc(f);
    p
}

#[test]
fn scope_handlers_merge_callback_failure_abis_and_box_typed_child_results() {
    let lowered = lower_program(scope_program()).unwrap();
    assert!(
        lowered
            .emitted_functions
            .iter()
            .all(|id| !lowered.lowered.functions[*id].fails)
    );
    assert!(lowered.emitted_functions.iter().all(|id| lowered.lowered.functions[*id].vars.iter()
        .all(|(_, variable)| !matches!(lowered.lowered.core.ty(variable.ty), Type::Fn(ft) if ft.fails))));
    let output = crate::backend::compile_program(
        scope_program(),
        &Options::default()
            .with_heap_bytes(16384)
            .with_frame_bytes(2048)
            .with_deferred_capacity(0),
    )
    .unwrap();
    assert!(!output.c_source.contains("#define EMELA_FAILURE_RUNTIME 1"));
    execute_answer(output);
}

#[test]
fn conservative_scope_failures_remain_when_a_c_boundary_can_supply_failing_values() {
    for boundary in [0, 1, 2] {
        let mut p = scope_program();
        let failing = function_type(&mut p, vec![], TypeId::INT, true);
        let ty = match boundary {
            0 => TypeId::ANY,
            1 => failing,
            _ => p.core.types.intern(Type::Tuple(vec![TypeId::INT, failing])),
        };
        let (_, function) = p.functions.iter_mut().next().unwrap();
        let param = aggregate_variable(function, ty);
        function.params.push(param);
        let lowered = lower_program(p).unwrap();
        assert!(lowered.emitted_functions.iter().any(|id| lowered.lowered.functions[*id].vars.iter()
            .any(|(_, variable)| matches!(lowered.lowered.core.ty(variable.ty), Type::Fn(ft) if ft.fails))));
    }
}

#[test]
fn ordinary_error_values_in_a_pure_scope_do_not_keep_unused_display_functions() {
    let mut p = scope_program();
    let error = p.core.enums.alloc(EnumDef {
        name: "OrdinaryErrorValue".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "OrdinaryErrorValue".into(),
            fields: VariantFields::Unit,
            tys: vec![],
        }],
        error: true,
        external: false,
    });
    let error_type = p.core.types.intern(Type::Enum(error, vec![]));
    let mut show = Function::new("show_ordinary_error");
    show.ret = TypeId::STRING;
    let argument = aggregate_variable(&mut show, error_type);
    show.params.push(argument);
    show.body = Some(show.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::String("ordinary error".into()).into())),
    }));
    let show = p.functions.alloc(show);
    p.error_shows.push((error, show));
    let (_, function) = p.functions.iter_mut().next().unwrap();
    let value = aggregate_variable(function, error_type);
    let make = binding(
        function,
        Some(value),
        Value::Make {
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            args: vec![],
        },
    );
    function.seqs[function.body.unwrap()].stmts.insert(0, make);
    execute_answer(
        crate::backend::compile_program(
            p,
            &Options::default()
                .with_heap_bytes(16384)
                .with_frame_bytes(2048),
        )
        .unwrap(),
    );
}

#[test]
fn scope_normalization_rejects_missing_operations_and_cyclic_generated_bodies() {
    for cycle in [false, true] {
        let mut p = scope_program();
        let (_, function) = p.functions.iter_mut().next().unwrap();
        let handler = function
            .stmts
            .iter()
            .find_map(|(id, statement)| {
                matches!(
                    statement,
                    Stmt::Let {
                        value: Value::Handler { .. },
                        ..
                    }
                )
                .then_some(id)
            })
            .unwrap();
        let Stmt::Let {
            value: Value::Handler { ops, .. },
            ..
        } = &mut function.stmts[handler]
        else {
            unreachable!()
        };
        let operation = ops[0];
        if cycle {
            let body = function
                .stmts
                .iter()
                .find_map(|(_, statement)| match statement {
                    Stmt::Let {
                        dst: Some(dst),
                        value: Value::Closure(closure),
                        ..
                    } if *dst == operation => Some(closure.body),
                    _ => None,
                })
                .unwrap();
            let recur = function.stmts.alloc(Stmt::Block {
                dst: None,
                label: function.labels.alloc(LabelData {
                    hint: "invalid".into(),
                }),
                body,
            });
            function.seqs[body].stmts.push(recur);
        } else {
            ops.clear();
        }
        let errors = lower_program(p).unwrap_err();
        assert_eq!(errors[0].kind, crate::ErrorKind::InvalidFlowIr);
        assert!(errors[0].message.contains(if cycle {
            "sequence cycle"
        } else {
            "operation count"
        }));
    }
}

#[test]
fn calling_the_generated_scope_deferred_operation_requires_its_pool() {
    let mut p = scope_program();
    let deferred_type = handle_type(&mut p, "Std.Deferred.Deferred", TypeId::INT);
    let (_, function) = p.functions.iter_mut().next().unwrap();
    let (handler, effect) = function
        .stmts
        .iter()
        .find_map(|(_, statement)| match statement {
            Stmt::Let {
                dst: Some(dst),
                value: Value::Handler { effect, .. },
                ..
            } => Some((*dst, *effect)),
            _ => None,
        })
        .unwrap();
    let body = function
        .stmts
        .iter()
        .find_map(|(_, statement)| match statement {
            Stmt::Let {
                value: Value::Closure(closure),
                ..
            } if closure.params.len() == 1
                && function.vars[closure.params[0]].ty == TypeId::FIBER_SCOPE =>
            {
                Some(closure.body)
            }
            _ => None,
        })
        .unwrap();
    let value = aggregate_variable(function, deferred_type);
    let call = binding(
        function,
        Some(value),
        Value::Call {
            callee: Callee::Op {
                handler,
                effect,
                op: 1,
            },
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    function.seqs[body].stmts.push(call);
    let options = Options::default()
        .with_heap_bytes(16384)
        .with_frame_bytes(2048);
    let error =
        crate::backend::compile_program(p.clone(), &options.clone().with_deferred_capacity(0))
            .unwrap_err();
    assert!(
        error
            .iter()
            .any(|error| error.kind == crate::ErrorKind::InvalidOptions)
    );
    execute_answer(crate::backend::compile_program(p, &options.with_deferred_capacity(1)).unwrap());
}
