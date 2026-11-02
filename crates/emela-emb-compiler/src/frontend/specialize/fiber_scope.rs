//! Scope captures the calling evidence; each child adds the capability that forked it.
use super::*;

impl Specializer {
    pub(in crate::frontend::specialize) fn scope_effect(&mut self) -> b::EffectId {
        if let Some(id) = self.core.effects.iter().find_map(|(id, effect)| {
            (effect.decl == "Std.Fiber.Scope" && effect.args == [b::TypeId::ANY, b::TypeId::ANY])
                .then_some(id)
        }) {
            return id;
        }
        let callback = self.core.types.intern(b::Type::Fn(b::FnType {
            params: vec![b::TypeId::EVIDENCE],
            ret: b::TypeId::ANY,
            fails: false,
            suspends: true,
            ev: false,
            sets: None,
        }));
        self.core.effects.alloc(b::EffectDef {
            name: "Std.Fiber.Scope[_,_]".into(),
            decl: "Std.Fiber.Scope".into(),
            args: vec![b::TypeId::ANY, b::TypeId::ANY],
            label_of: None,
            ops: vec![
                b::OpDef {
                    name: "fork".into(),
                    suspend: false,
                    type_params: 0,
                    params: vec![callback],
                    ret: b::TypeId::ANY,
                    fails: false,
                },
                b::OpDef {
                    name: "deferred".into(),
                    suspend: false,
                    type_params: 0,
                    params: vec![],
                    ret: b::TypeId::ANY,
                    fails: false,
                },
            ],
        })
    }
    pub(super) fn scope_value(
        &mut self,
        caller: &ir::Function,
        body: &ir::Local,
        errors: Vec<b::TypeId>,
        span: Span,
    ) -> Result<ir::Expr> {
        let effect = self.scope_effect();
        let signature = self.callable(caller, body.ty, span)?;
        let fails = !errors.is_empty();
        let snapshot = self.state_local(
            caller,
            "scope_evidence",
            frontend_type(b::TypeId::EVIDENCE),
            span,
        )?;
        let scope =
            self.state_local(caller, "scope", frontend_type(b::TypeId::FIBER_SCOPE), span)?;
        let child_type = self.core.types.intern(b::Type::Fn(b::FnType {
            params: vec![b::TypeId::EVIDENCE],
            ret: b::TypeId::ANY,
            fails,
            suspends: true,
            ev: false,
            sets: None,
        }));
        let child = self.state_local(caller, "child", frontend_type(child_type), span)?;
        let applied = apply(&child, vec![local(&snapshot, span)], ValueType::Any, span);
        let child_wrapper = self.lambda(
            vec![],
            vec![child.id, snapshot.id],
            applied,
            b::TypeId::ANY,
            fails,
            true,
        );
        let fork_body = fiber(
            b::FiberOp::Fork,
            vec![local(&scope, span), child_wrapper],
            vec![],
            ValueType::Any,
            span,
        );
        let fork = self.lambda(
            vec![child],
            vec![scope.id, snapshot.id],
            fork_body,
            b::TypeId::ANY,
            false,
            false,
        );
        let deferred_body = fiber(
            b::FiberOp::Deferred,
            vec![local(&scope, span)],
            vec![],
            ValueType::Any,
            span,
        );
        let deferred = self.lambda(
            vec![],
            vec![scope.id],
            deferred_body,
            b::TypeId::ANY,
            false,
            false,
        );
        let capability_type = frontend_type(self.core.types.intern(b::Type::Handler(effect)));
        let capability = ir::Expr {
            kind: ir::ExprKind::Handler {
                name: "Scope".into(),
                effect,
                operations: vec![fork, deferred],
            },
            ty: capability_type,
            span,
        };
        let applied = apply(body, vec![], frontend_type(signature.ret), span);
        let provided = ir::Expr {
            kind: ir::ExprKind::Provide {
                base: Some(Box::new(local(&snapshot, span))),
                bindings: vec![(effect, capability)],
                body: single(applied),
            },
            ty: frontend_type(signature.ret),
            span,
        };
        let inner = self.lambda(
            vec![scope],
            vec![body.id, snapshot.id],
            provided,
            signature.ret,
            signature.fails,
            true,
        );
        let value = fiber(
            b::FiberOp::Scope { fails },
            vec![inner],
            errors,
            frontend_type(signature.ret),
            span,
        );
        Ok(sequence(
            vec![
                bind(
                    snapshot,
                    ir::Expr {
                        kind: ir::ExprKind::Evidence,
                        ty: frontend_type(b::TypeId::EVIDENCE),
                        span,
                    },
                    span,
                ),
                ir::Statement::Expr(value),
            ],
            frontend_type(signature.ret),
            span,
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::frontend::specialize) fn scope_fork(
        &mut self,
        caller: &ir::Function,
        handler: ir::Expr,
        effect: b::EffectId,
        op: usize,
        args: Vec<ir::Expr>,
        result: ValueType,
        span: Span,
    ) -> Result<ir::Expr> {
        let capability = self.state_local(caller, "fork_capability", handler.ty, span)?;
        let mut statements = vec![bind(capability.clone(), handler, span)];
        let mut evaluated = Vec::new();
        for argument in args {
            let value = self.state_local(caller, "fork_argument", argument.ty, span)?;
            statements.push(bind(value.clone(), argument, span));
            evaluated.push(value);
        }
        let [callback]: [ir::Local; 1] = evaluated
            .try_into()
            .map_err(|_| diagnostic(caller, span, "Scope.fork expects one callback"))?;
        let signature = self.callable(caller, callback.ty, span)?;
        let evidence = self.state_local(
            caller,
            "child_evidence",
            frontend_type(b::TypeId::EVIDENCE),
            span,
        )?;
        let applied = apply(&callback, vec![], frontend_type(signature.ret), span);
        let boxed = ir::Expr {
            ty: ValueType::Any,
            span,
            kind: ir::ExprKind::Coerce(Box::new(applied)),
        };
        let provided = ir::Expr {
            kind: ir::ExprKind::Provide {
                base: Some(Box::new(local(&evidence, span))),
                bindings: vec![(effect, local(&capability, span))],
                body: single(boxed),
            },
            ty: ValueType::Any,
            span,
        };
        let wrapper = self.lambda(
            vec![evidence],
            vec![callback.id, capability.id],
            provided,
            b::TypeId::ANY,
            signature.fails,
            true,
        );
        statements.push(ir::Statement::Expr(ir::Expr {
            kind: ir::ExprKind::OpCall {
                handler: Box::new(local(&capability, span)),
                effect,
                op,
                errors: vec![],
                args: vec![wrapper],
                order: vec![0],
            },
            ty: result,
            span,
        }));
        Ok(sequence(statements, result, span))
    }
}
