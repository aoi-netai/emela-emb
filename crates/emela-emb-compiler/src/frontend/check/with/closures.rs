//! Bind lexical providers inside anonymous functions before their invocation ABI.
use super::*;

impl Checker {
    pub(super) fn with_lexical_closures(
        &mut self,
        module: usize,
        body: &mut ir::Block,
        provided: &[(EffectId, Local)],
    ) -> Result<HashSet<LocalId>, Diagnostic> {
        let mut captures = HashSet::new();
        for statement in &mut body.statements {
            let value = match statement {
                ir::Statement::Pattern { value, .. } | ir::Statement::Expr(value) => value,
            };
            captures.extend(self.with_lexical_expression(module, value, provided)?);
        }
        Ok(captures)
    }
    fn with_lexical_expression(
        &mut self,
        module: usize,
        expression: &mut ir::Expr,
        provided: &[(EffectId, Local)],
    ) -> Result<HashSet<LocalId>, Diagnostic> {
        use ir::ExprKind as E;
        let mut added = HashSet::new();
        match &mut expression.kind {
            E::Lambda { captures, body, .. } => {
                added.extend(self.with_lexical_closures(module, body, provided)?);
                let required = self.with_expression_effects(
                    module,
                    &ir::Expr {
                        kind: E::Block(body.clone()),
                        ty: body.ty,
                        span: body.span,
                    },
                )?;
                let mut members = HashSet::new();
                self.with_effect_members(module, &required, expression.span, &mut members)?;
                let unknown = required
                    .iter()
                    .any(|ty| matches!(self.catalog.core.ty(*ty), Type::Param(_)));
                let selected = provided
                    .iter()
                    .filter(|(effect, _)| unknown || members.contains(effect))
                    .cloned()
                    .collect::<Vec<_>>();
                if !selected.is_empty() {
                    let previous = body.clone();
                    let value = ir::Expr {
                        kind: E::Provide {
                            bindings: selected
                                .iter()
                                .map(|(effect, local)| {
                                    (*effect, with_reference(local, expression.span))
                                })
                                .collect(),
                            body: previous,
                        },
                        ty: body.ty,
                        span: expression.span,
                    };
                    body.statements = vec![ir::Statement::Expr(value)];
                    added.extend(selected.iter().map(|(_, local)| local.id));
                }
                for &capture in &added {
                    if !captures.contains(&capture) {
                        captures.push(capture);
                    }
                }
                captures.sort_by_key(|id| id.0);
            }
            E::OpCall { handler, args, .. }
            | E::Apply {
                callee: handler,
                args,
                ..
            } => {
                added.extend(self.with_lexical_expression(module, handler, provided)?);
                for argument in args {
                    added.extend(self.with_lexical_expression(module, argument, provided)?);
                }
            }
            E::Handler { operations, .. }
            | E::Tuple(operations)
            | E::Concat(operations)
            | E::Make {
                args: operations, ..
            }
            | E::Call {
                args: operations, ..
            }
            | E::TraitCall {
                args: operations, ..
            }
            | E::Builtin {
                args: operations, ..
            }
            | E::Fiber {
                args: operations, ..
            } => {
                for value in operations {
                    added.extend(self.with_lexical_expression(module, value, provided)?);
                }
            }
            E::Provide { bindings, body } => {
                for (_, value) in bindings {
                    added.extend(self.with_lexical_expression(module, value, provided)?);
                }
                added.extend(self.with_lexical_closures(module, body, provided)?);
            }
            E::With { bindings, body } => {
                for binding in bindings {
                    added.extend(self.with_lexical_expression(
                        module,
                        &mut binding.value,
                        provided,
                    )?);
                    if let Some(cleanup) = &mut binding.cleanup {
                        added.extend(self.with_lexical_expression(module, cleanup, provided)?);
                    }
                }
                added.extend(self.with_lexical_closures(module, body, provided)?);
            }
            E::List { elements, tail } => {
                for value in elements {
                    added.extend(self.with_lexical_expression(module, value, provided)?);
                }
                if let Some(tail) = tail {
                    added.extend(self.with_lexical_expression(module, tail, provided)?);
                }
            }
            E::Fail(value)
            | E::Coerce(value)
            | E::Field { base: value, .. }
            | E::Tag(value)
            | E::Assert(value)
            | E::Unary { value, .. } => {
                added.extend(self.with_lexical_expression(module, value, provided)?)
            }
            E::Binary { left, right, .. } => {
                added.extend(self.with_lexical_expression(module, left, provided)?);
                added.extend(self.with_lexical_expression(module, right, provided)?);
            }
            E::Block(body) => added.extend(self.with_lexical_closures(module, body, provided)?),
            E::If {
                condition,
                then,
                else_,
            } => {
                added.extend(self.with_lexical_expression(module, condition, provided)?);
                added.extend(self.with_lexical_closures(module, then, provided)?);
                added.extend(self.with_lexical_closures(module, else_, provided)?);
            }
            E::Escape { value, arms, .. } | E::Match { value, arms } => {
                added.extend(self.with_lexical_expression(module, value, provided)?);
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        added.extend(self.with_lexical_expression(module, guard, provided)?);
                    }
                    added.extend(self.with_lexical_expression(module, &mut arm.value, provided)?);
                }
            }
            E::Literal(_) | E::Local(_) | E::FnRef { .. } | E::Use { .. } => {}
        }
        Ok(added)
    }
}
