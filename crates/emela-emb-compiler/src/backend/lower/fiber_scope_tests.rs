use super::*;

#[test]
fn scope_handlers_merge_callback_failure_abis_and_box_typed_child_results() {
    let mut p = program(vec![]);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::INT);
    let callbacks = [false, true].map(|fails| function_type(&mut p, vec![], TypeId::ANY, fails));
    let child_type = function_type(&mut p, vec![], TypeId::INT, false);
    let effect = p.core.effects.alloc(EffectDef {
        name: "Std.Fiber.Scope[_, _]".into(),
        decl: "Std.Fiber.Scope".into(),
        args: vec![],
        label_of: None,
        ops: vec![OpDef {
            name: "fork".into(),
            suspend: false,
            type_params: 1,
            params: vec![callbacks[0]],
            ret: TypeId::ANY,
            fails: false,
        }],
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
        statements.push(binding(
            &mut f,
            Some(handler),
            Value::Handler {
                name: "Scope".into(),
                effect,
                ops: vec![operation],
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
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(16384)
            .with_frame_bytes(2048),
    )
    .unwrap();
    execute_answer(output);
}
