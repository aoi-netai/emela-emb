//! Runtime suspension is separate from evidence-bearing capability requirements.
use super::*;

impl Checker {
    pub(super) fn async_type(
        &mut self,
        module: usize,
        span: Span,
    ) -> Result<crate::contract::ir::TypeId, Diagnostic> {
        let effect = self
            .catalog
            .effects
            .declarations
            .iter()
            .find(|declaration| {
                let source = &self.modules.modules[declaration.module];
                source.builtin && source.name == "Prelude" && declaration.ast.name.value == "Async"
            })
            .map(|declaration| declaration.id)
            .ok_or_else(|| {
                self.error(
                    module,
                    DK::Type,
                    span,
                    "the compiler-owned Async requirement is unavailable",
                )
            })?;
        Ok(self
            .catalog
            .core
            .types
            .intern(crate::contract::ir::Type::Handler(effect)))
    }
    pub(super) fn validate_external_reference(
        &self,
        module: usize,
        id: DefinitionId,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let definition = &self.definitions[id.0];
        if definition.external.is_some()
            && definition.ast.suspends
            && !matches!(
                self.current_effect_method,
                Some(super::super::effect_collect::MethodKind::Operation { .. })
            )
        {
            return Err(self.error(
                module,
                DK::Type,
                span,
                "an external suspend function may only be referenced inside a handler operation",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_sync_operations(
        &mut self,
        program: &ir::Program,
    ) -> Result<(), Vec<Diagnostic>> {
        for function in &program.functions {
            if !function.immediate
                || !super::super::function_abi::effects_abi(
                    &program.core,
                    &function.signature.sets.effects,
                )
                .1
            {
                continue;
            }
            let Some(body) = &function.body else {
                continue;
            };
            let mut pending = Vec::new();
            push_block(body, &mut pending);
            while let Some(expression) = pending.pop() {
                let suspends = match &expression.kind {
                    ir::ExprKind::Call {
                        target,
                        type_arguments,
                        ..
                    } => {
                        let Some(callee) =
                            program.functions.iter().find(|callee| callee.id == *target)
                        else {
                            continue;
                        };
                        let mut requirements = Vec::new();
                        for &requirement in &callee.signature.sets.effects {
                            // Suspension belongs to the effect declaration; its value
                            // arguments do not change the operation markers. Only row
                            // parameters need substitution for this ABI query.
                            requirements
                                .push(self.catalog.core.types.subst(requirement, type_arguments));
                        }
                        !callee.immediate
                            && super::super::function_abi::effects_abi(
                                &self.catalog.core,
                                &requirements,
                            )
                            .1
                    }
                    ir::ExprKind::Apply { callee, .. } => {
                        match self.catalog.core.ty(data::backend_type(callee.ty)) {
                            crate::contract::ir::Type::Fn(function) => {
                                function.suspends
                                    || function.sets.is_some_and(|sets| {
                                        super::super::function_abi::effects_abi(
                                            &self.catalog.core,
                                            &[sets.effects],
                                        )
                                        .1
                                    })
                            }
                            _ => false,
                        }
                    }
                    ir::ExprKind::OpCall { effect, op, .. } => {
                        self.catalog.core.effects[*effect].ops[*op].suspend
                    }
                    _ => false,
                };
                if suspends {
                    return Err(vec![Diagnostic::new(DK::Type, expression.span, "a synchronous effect operation cannot make a suspending call when its body requires a non-Immediate effect").in_source(&function.source_name)]);
                }
                if !matches!(expression.kind, ir::ExprKind::Lambda { .. }) {
                    push_children(expression, &mut pending);
                }
            }
        }
        Ok(())
    }
}

pub(in crate::frontend) fn resume_escapes(body: &ir::Block, resume: LocalId) -> bool {
    let mut pending = Vec::new();
    push_block(body, &mut pending);
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            ir::ExprKind::Local(id) if *id == resume => return true,
            ir::ExprKind::Lambda { captures, .. } if captures.contains(&resume) => return true,
            ir::ExprKind::Apply { callee, args, .. } if matches!(callee.kind, ir::ExprKind::Local(id) if id == resume) => {
                pending.extend(args)
            }
            ir::ExprKind::Lambda { .. } => {}
            _ => push_children(expression, &mut pending),
        }
    }
    false
}
fn push_block<'a>(block: &'a ir::Block, pending: &mut Vec<&'a ir::Expr>) {
    pending.extend(block.statements.iter().map(|statement| match statement {
        ir::Statement::Pattern { value, .. } | ir::Statement::Expr(value) => value,
    }));
}
fn push_children<'a>(expression: &'a ir::Expr, pending: &mut Vec<&'a ir::Expr>) {
    use ir::ExprKind as E;
    match &expression.kind {
        E::Literal(_) | E::Local(_) | E::FnRef { .. } | E::Use { .. } => {}
        E::Lambda { body, .. } | E::Block(body) => push_block(body, pending),
        E::Apply { callee, args, .. }
        | E::OpCall {
            handler: callee,
            args,
            ..
        } => {
            pending.push(callee);
            pending.extend(args);
        }
        E::Concat(args)
        | E::Tuple(args)
        | E::Make { args, .. }
        | E::Builtin { args, .. }
        | E::Call { args, .. }
        | E::TraitCall { args, .. }
        | E::Handler {
            operations: args, ..
        } => pending.extend(args),
        E::List { elements, tail } => {
            pending.extend(elements);
            pending.extend(tail.as_deref());
        }
        E::Provide { bindings, body } => {
            pending.extend(bindings.iter().map(|(_, value)| value));
            push_block(body, pending);
        }
        E::With { bindings, body } => {
            for binding in bindings {
                pending.push(&binding.value);
                pending.extend(binding.cleanup.as_ref());
            }
            push_block(body, pending);
        }
        E::Fail(value)
        | E::Coerce(value)
        | E::Tag(value)
        | E::Assert(value)
        | E::Unary { value, .. }
        | E::Field { base: value, .. } => pending.push(value),
        E::Binary { left, right, .. } => {
            pending.push(left);
            pending.push(right);
        }
        E::Match { value, arms } | E::Escape { value, arms, .. } => {
            pending.push(value);
            for arm in arms {
                pending.extend(arm.guard.as_ref());
                pending.push(&arm.value);
            }
        }
        E::If {
            condition,
            then,
            else_,
        } => {
            pending.push(condition);
            push_block(then, pending);
            push_block(else_, pending);
        }
    }
}
