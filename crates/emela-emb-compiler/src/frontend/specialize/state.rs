//! Expand the trusted State.run declaration using concrete Cell and handler values.
use super::*;

impl Specializer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn state_run_expression(
        &mut self,
        caller: &ir::Function,
        runtime: &ir::Function,
        args: Vec<ir::Expr>,
        order: &[usize],
        result_type: ValueType,
        span: Span,
    ) -> Result<ir::Expr> {
        if args.len() != 2 || order.len() != 2 || runtime.signature.params.len() != 2 {
            return Err(diagnostic(
                caller,
                span,
                "State.run requires initial and body arguments",
            ));
        }
        let state = backend_type(runtime.signature.params[0]);
        let b::Type::Fn(body_type) = self
            .core
            .ty(backend_type(runtime.signature.params[1]))
            .clone()
        else {
            return Err(diagnostic(
                caller,
                span,
                "State.run body must have a function type",
            ));
        };
        let effect = state_effect(&self.core, &body_type, state).ok_or_else(|| {
            diagnostic(
                caller,
                span,
                "State.run body is missing its concrete State effect",
            )
        })?;
        let operation_names = self.core.effects[effect]
            .ops
            .iter()
            .map(|op| op.name.clone())
            .collect::<Vec<_>>();
        if operation_names.len() != 3
            || ["get", "set", "update"].iter().any(|name| {
                operation_names
                    .iter()
                    .filter(|op| op.as_str() == *name)
                    .count()
                    != 1
            })
        {
            return Err(diagnostic(
                caller,
                span,
                "State requires get, set and update operations",
            ));
        }

        // Evaluate each argument once in source order, including named arguments.
        // Allocation and handler creation begin only after both arguments succeed.
        let mut statements = Vec::new();
        let mut evaluated = Vec::new();
        for argument in args {
            let local = self.state_local(caller, "argument", argument.ty, span)?;
            statements.push(bind(local.clone(), argument, span));
            evaluated.push(local);
        }
        let initial = evaluated
            .get(order[0])
            .ok_or_else(|| diagnostic(caller, span, "invalid State.run argument order"))?;
        let callback = evaluated
            .get(order[1])
            .ok_or_else(|| diagnostic(caller, span, "invalid State.run argument order"))?;
        let cell_type = frontend_type(self.core.types.intern(b::Type::Cell(state)));
        let cell = self.state_local(caller, "cell", cell_type, span)?;
        statements.push(bind(
            cell.clone(),
            builtin(
                b::Builtin::CellNew,
                vec![local(initial, span)],
                cell_type,
                span,
            ),
            span,
        ));

        let state_type = frontend_type(state);
        let pure_update = self
            .core
            .types
            .intern(b::Type::Fn(pure(vec![state], state)));
        let value = self.state_local(caller, "value", state_type, span)?;
        let update = self.state_local(caller, "update", frontend_type(pure_update), span)?;
        let read = || {
            builtin(
                b::Builtin::CellGet,
                vec![local(&cell, span)],
                state_type,
                span,
            )
        };
        let write = |value| {
            builtin(
                b::Builtin::CellSet,
                vec![local(&cell, span), value],
                ValueType::Unit,
                span,
            )
        };
        let changed = ir::Expr {
            kind: ir::ExprKind::Apply {
                callee: Box::new(local(&update, span)),
                args: vec![read()],
                order: vec![0],
            },
            ty: state_type,
            span,
        };
        let get_type = self.core.types.intern(b::Type::Fn(pure(vec![], state)));
        let set_type = self
            .core
            .types
            .intern(b::Type::Fn(pure(vec![state], b::TypeId::UNIT)));
        let update_type = self
            .core
            .types
            .intern(b::Type::Fn(pure(vec![pure_update], b::TypeId::UNIT)));
        let mut operations = Vec::new();
        for name in operation_names {
            let (params, ty, body) = match name.as_str() {
                "get" => (vec![], get_type, read()),
                "set" => (vec![value.clone()], set_type, write(local(&value, span))),
                "update" => (vec![update.clone()], update_type, write(changed.clone())),
                _ => unreachable!("validated State operation names"),
            };
            operations.push(ir::Expr {
                kind: ir::ExprKind::Lambda {
                    params,
                    captures: vec![cell.id],
                    body: block(body, span),
                },
                ty: frontend_type(ty),
                span,
            });
        }
        let handler_type = frontend_type(self.core.types.intern(b::Type::Handler(effect)));
        let handler = self.state_local(caller, "handler", handler_type, span)?;
        statements.push(bind(
            handler.clone(),
            ir::Expr {
                kind: ir::ExprKind::Handler {
                    name: "State".into(),
                    effect,
                    operations,
                },
                ty: handler_type,
                span,
            },
            span,
        ));
        let body = ir::Expr {
            kind: ir::ExprKind::Apply {
                callee: Box::new(local(callback, span)),
                args: vec![],
                order: vec![],
            },
            ty: frontend_type(body_type.ret),
            span,
        };
        let result = self.state_local(caller, "result", body.ty, span)?;
        statements.push(bind(
            result.clone(),
            ir::Expr {
                ty: body.ty,
                span,
                // State has no release or retirement: returned closures may retain
                // the shared cell through the handler, as in the fixed source contract.
                kind: ir::ExprKind::Provide {
                    bindings: vec![(effect, local(&handler, span))],
                    body: block(body, span),
                },
            },
            span,
        ));
        statements.push(ir::Statement::Expr(ir::Expr {
            kind: ir::ExprKind::Tuple(vec![local(&result, span), read()]),
            ty: result_type,
            span,
        }));
        Ok(ir::Expr {
            kind: ir::ExprKind::Block(ir::Block {
                statements,
                ty: result_type,
                span,
            }),
            ty: result_type,
            span,
        })
    }

    fn state_local(
        &mut self,
        caller: &ir::Function,
        name: &str,
        ty: ValueType,
        span: Span,
    ) -> Result<ir::Local> {
        // Share the generated-local counter with dbg lowering, including nested calls.
        let next = match self.debug_locals.get(&caller.id) {
            Some(&next) => next,
            None => super::debug::maximum_local(caller)
                .map_or(Some(0), |id| id.checked_add(1))
                .ok_or_else(|| diagnostic(caller, span, "local ID namespace is exhausted"))?,
        };
        self.debug_locals.insert(
            caller.id,
            next.checked_add(1)
                .ok_or_else(|| diagnostic(caller, span, "local ID namespace is exhausted"))?,
        );
        Ok(ir::Local {
            id: ir::LocalId(next),
            name: format!("$state_{name}"),
            ty,
        })
    }
}

fn state_effect(core: &b::Definitions, body: &b::FnType, state: b::TypeId) -> Option<b::EffectId> {
    let mut pending = vec![body.sets?.effects];
    let mut seen = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match core.ty(ty) {
            b::Type::Set(summary) => pending.extend(&summary.params),
            b::Type::Handler(effect) => {
                let declaration = &core.effects[*effect];
                if declaration.decl == "Std.State.State" && declaration.args == [state] {
                    return Some(*effect);
                }
            }
            _ => {}
        }
    }
    None
}
fn local(value: &ir::Local, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Local(value.id),
        ty: value.ty,
        span,
    }
}
fn builtin(op: b::Builtin, args: Vec<ir::Expr>, ty: ValueType, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Builtin { op, args },
        ty,
        span,
    }
}
fn block(value: ir::Expr, span: Span) -> ir::Block {
    ir::Block {
        ty: value.ty,
        statements: vec![ir::Statement::Expr(value)],
        span,
    }
}
fn bind(binding: ir::Local, value: ir::Expr, span: Span) -> ir::Statement {
    ir::Statement::Pattern {
        pattern: ir::Pattern {
            ty: binding.ty,
            kind: ir::PatternKind::Bind(binding),
            span,
        },
        value,
    }
}

fn pure(params: Vec<b::TypeId>, ret: b::TypeId) -> b::FnType {
    b::FnType {
        params,
        ret,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }
}
