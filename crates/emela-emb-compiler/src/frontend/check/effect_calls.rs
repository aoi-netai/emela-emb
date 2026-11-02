//! Capabilities select operations by their exact effect instance, including label identity.
use super::*;
use crate::contract::ir::Type;

impl Checker {
    pub(super) fn use_effect(
        &mut self,
        module: usize,
        name: &ast::Text,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let effect = self
            .catalog
            .resolve_effect(&self.modules, module, name, &self.current_parameters)
            .map_err(|error| error.in_source(&self.modules.modules[module].source_name))?;
        let declaration = self.catalog.effects.declaration(effect).ok_or_else(|| {
            self.error(
                module,
                DK::Type,
                name.span,
                "effect has no source declaration",
            )
        })?;
        let source = &self.modules.modules[declaration.module];
        if source.builtin && source.name == "Prelude" && declaration.ast.name.value == "Async" {
            return Err(self.error(
                module,
                DK::Type,
                span,
                "Async is supplied by the runtime and has no capability value",
            ));
        }
        let ty = data::frontend_type(self.catalog.core.types.intern(Type::Handler(effect)));
        Ok(ir::Expr {
            kind: ir::ExprKind::Use { effect },
            ty,
            span,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn operation_call(
        &mut self,
        module: usize,
        mut receiver: ir::Expr,
        name: &ast::Text,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        receiver.ty = self.resolve_type(module, receiver.ty, receiver.span)?;
        let Type::Handler(effect) = *self.catalog.core.ty(data::backend_type(receiver.ty)) else {
            return Err(self.error(
                module,
                DK::Type,
                receiver.span,
                "operation receiver must be a capability value",
            ));
        };
        let op = self.catalog.core.effects[effect]
            .ops
            .iter()
            .position(|operation| operation.name == name.value)
            .ok_or_else(|| {
                self.error(
                    module,
                    DK::Name,
                    name.span,
                    format!(
                        "effect '{}' has no operation '{}'",
                        self.catalog.core.effects[effect].name, name.value
                    ),
                )
            })?;
        // A label retains its own evidence key, while its operation signature and argument
        // labels come from the base declaration. Substitute effect and operation parameters
        // together so the caller's Param(0) cannot be confused with an operation's Param(0).
        let base = self.catalog.core.effects[effect].label_of.unwrap_or(effect);
        let declaration = self
            .catalog
            .effects
            .declaration(base)
            .ok_or_else(|| {
                self.error(
                    module,
                    DK::Type,
                    name.span,
                    "operation has no source declaration",
                )
            })?
            .clone();
        let operation = self.catalog.core.effects[declaration.id].ops[op].clone();
        let labels = declaration.ast.operations[op]
            .params
            .iter()
            .map(|param| param.name.value.clone())
            .collect::<Vec<_>>();
        let mut substitutions = self.catalog.core.effects[base].args.clone();
        for _ in 0..operation.type_params {
            substitutions.push(
                self.solver
                    .try_fresh(
                        &mut self.catalog.core.types,
                        super::super::infer::Constraint::Any,
                    )
                    .map_err(|error| self.error(module, DK::Type, name.span, error.message))?,
            );
        }
        let parameters = operation
            .params
            .into_iter()
            .map(|ty| {
                self.catalog
                    .effects
                    .substitute_type(&mut self.catalog.core, ty, &substitutions)
            })
            .collect::<Vec<_>>();
        let ret = self.catalog.effects.substitute_type(
            &mut self.catalog.core,
            operation.ret,
            &substitutions,
        );
        let failures = self
            .catalog
            .effects
            .operation_errors(declaration.id, op)
            .ok_or_else(|| {
                self.error(
                    module,
                    DK::Type,
                    name.span,
                    "operation failure signature has not been resolved",
                )
            })?
            .to_vec();
        for failure in failures {
            let failure = self.catalog.effects.substitute_type(
                &mut self.catalog.core,
                failure,
                &substitutions,
            );
            if !matches!(self.catalog.core.ty(failure), Type::Param(_))
                && !matches!(self.catalog.core.ty(failure), Type::Enum(id, _) if self.catalog.core.enums[*id].error)
            {
                return Err(self.error(
                    module,
                    DK::Type,
                    name.span,
                    "effect arguments used in fails must be single error types",
                ));
            }
        }
        if let Some(expected) = expected {
            self.expect_type(module, data::frontend_type(ret), expected, span)?;
        }
        let mut order = vec![usize::MAX; parameters.len()];
        let mut named = false;
        let mut checked = Vec::new();
        for (index, argument) in args.iter().enumerate() {
            let parameter = if let Some(label) = &argument.label {
                named = true;
                labels
                    .iter()
                    .position(|name| *name == label.value)
                    .ok_or_else(|| {
                        self.error(
                            module,
                            DK::Type,
                            label.span,
                            format!("unknown operation argument label '{}'", label.value),
                        )
                    })?
            } else {
                if named {
                    return Err(self.error(
                        module,
                        DK::Type,
                        argument.value.span,
                        "positional arguments must precede named arguments",
                    ));
                }
                if index >= parameters.len() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        format!(
                            "operation '{}' expects {} arguments, found {}",
                            name.value,
                            parameters.len(),
                            args.len()
                        ),
                    ));
                }
                index
            };
            if order[parameter] != usize::MAX {
                return Err(self.error(
                    module,
                    DK::Type,
                    argument.value.span,
                    format!(
                        "argument '{}' is supplied more than once",
                        labels[parameter]
                    ),
                ));
            }
            order[parameter] = index;
            checked.push(self.expr(
                module,
                &argument.value,
                locals,
                Some(data::frontend_type(parameters[parameter])),
            )?);
        }
        if order.contains(&usize::MAX) {
            return Err(self.error(
                module,
                DK::Type,
                span,
                format!(
                    "operation '{}' expects {} arguments, found {}",
                    name.value,
                    parameters.len(),
                    args.len()
                ),
            ));
        }
        let ty = self.resolve_type(module, data::frontend_type(ret), span)?;
        Ok(ir::Expr {
            kind: ir::ExprKind::OpCall {
                handler: Box::new(receiver),
                effect,
                op,
                args: checked,
                order,
            },
            ty,
            span,
        })
    }
}
