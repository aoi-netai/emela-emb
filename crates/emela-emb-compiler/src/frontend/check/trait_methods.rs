use super::*;
use crate::contract::ir::{self as b, Type};
use crate::frontend::traits::{BuiltinTrait, TraitId};
pub(super) struct Obligation {
    module: usize,
    self_type: b::TypeId,
    trait_id: TraitId,
    method: String,
    method_type_arguments: Vec<b::TypeId>,
    bounds: super::super::traits::ParamBounds,
    span: Span,
}
impl Checker {
    pub(super) fn validate_trait_obligations(&mut self) -> Result<(), Vec<Diagnostic>> {
        self.solver
            .finish_defaults(&mut self.catalog.core.types)
            .map_err(|error| vec![Diagnostic::new(DK::Type, Span::new(0, 0), error.message)])?;
        let mut errors = Vec::new();
        for obligation in std::mem::take(&mut self.trait_obligations) {
            let result = (|| {
                let receiver = self
                    .solver
                    .resolve(&mut self.catalog.core.types, obligation.self_type)
                    .map_err(|e| {
                        self.error(obligation.module, DK::Type, obligation.span, e.message)
                    })?;
                let arguments = obligation
                    .method_type_arguments
                    .iter()
                    .map(|&ty| self.solver.resolve(&mut self.catalog.core.types, ty))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| {
                        self.error(obligation.module, DK::Type, obligation.span, e.message)
                    })?;
                self.traits
                    .registry
                    .resolve_generic(
                        &self.catalog.core,
                        receiver,
                        obligation.trait_id,
                        &obligation.method,
                        &arguments,
                        &obligation.bounds,
                    )
                    .map_err(|e| {
                        self.error(obligation.module, DK::Type, obligation.span, e.message)
                    })?;
                Ok(())
            })();
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    pub(super) fn validate_data_declarations(&mut self) -> Result<(), Diagnostic> {
        for declaration in self.catalog.declarations.clone() {
            let mut bounds = super::super::traits::ParamBounds::new();
            for (index, parameter) in declaration.ast.params.iter().enumerate() {
                let ty = self.catalog.core.types.intern(Type::Param(index));
                let mut required = Vec::new();
                for bound in &parameter.bounds {
                    required.push(
                        self.traits
                            .find_trait(declaration.module, &bound.value)
                            .ok_or_else(|| {
                                self.error(
                                    declaration.module,
                                    DK::Name,
                                    bound.span,
                                    "undefined data type trait bound",
                                )
                            })?,
                    );
                }
                bounds.insert(ty, required);
            }
            let variants = self.catalog.core.enums[declaration.id].variants.clone();
            for (variant, source) in variants.iter().zip(&declaration.ast.variants) {
                for (&ty, field) in variant.tys.iter().zip(&source.fields) {
                    self.validate_type_bounds(declaration.module, ty, &bounds, field.ty.span)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn validate_type_bounds(
        &self,
        module: usize,
        ty: b::TypeId,
        bounds: &crate::frontend::traits::ParamBounds,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match self.catalog.core.ty(ty) {
                Type::Tuple(fields) => pending.extend(fields),
                Type::List(element) => pending.push(*element),
                Type::Enum(id, args) => {
                    let declaration = self.catalog.declaration(*id);
                    for (argument, parameter) in args.iter().zip(&declaration.ast.params) {
                        for bound in &parameter.bounds {
                            let trait_id = self
                                .traits
                                .find_trait(declaration.module, &bound.value)
                                .ok_or_else(|| {
                                    self.error(
                                        module,
                                        DK::Name,
                                        bound.span,
                                        "undefined data type trait bound",
                                    )
                                })?;
                            if !self
                                .traits
                                .registry
                                .satisfies(&self.catalog.core, *argument, trait_id, bounds)
                                .map_err(|e| self.error(module, DK::Type, span, e.message))?
                            {
                                return Err(self.error(
                                    module,
                                    DK::Type,
                                    span,
                                    "data type argument does not satisfy its trait bound",
                                ));
                            }
                        }
                    }
                    pending.extend(args);
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn validate_call_bounds(
        &mut self,
        module: usize,
        id: DefinitionId,
        type_arguments: &[b::TypeId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        for (parameter, bounds) in self.definitions[id.0].bounds.clone() {
            let instantiated = self.catalog.core.types.subst(parameter, type_arguments);
            let receiver = self
                .solver
                .resolve(&mut self.catalog.core.types, instantiated)
                .map_err(|e| self.error(module, DK::Type, span, e.message))?;
            for bound in bounds {
                if !self
                    .traits
                    .registry
                    .satisfies(&self.catalog.core, receiver, bound, &self.current_bounds)
                    .map_err(|e| self.error(module, DK::Type, span, e.message))?
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "type argument does not satisfy its trait bound",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn trait_method(
        &self,
        module: usize,
        trait_id: TraitId,
        method: &str,
        span: Span,
    ) -> Result<(TraitId, b::FnType, Vec<String>), Diagnostic> {
        let mut owners = vec![trait_id];
        owners.extend(
            self.traits
                .registry
                .supertraits(trait_id)
                .map_err(|e| self.error(module, DK::Type, span, e.message))?,
        );
        for owner in owners {
            let declaration = self
                .traits
                .registry
                .trait_data(owner)
                .map_err(|e| self.error(module, DK::Type, span, e.message))?;
            if let Some(method_data) = declaration.methods.iter().find(|m| m.name == method) {
                let Type::Fn(signature) = self.catalog.core.ty(method_data.signature) else {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "trait method has no function signature",
                    ));
                };
                let names = self.modules.modules[declaration.owner_module]
                    .ast
                    .traits
                    .iter()
                    .find(|t| t.name.value == declaration.name)
                    .and_then(|t| t.methods.iter().find(|m| m.name.value == method))
                    .map(|f| f.params.iter().map(|p| p.name.value.clone()).collect())
                    .unwrap_or_else(|| {
                        if signature.params.len() == 2 {
                            vec!["self".into(), "other".into()]
                        } else {
                            vec!["self".into()]
                        }
                    });
                return Ok((owner, signature.clone(), names));
            }
        }
        Err(self.error(
            module,
            DK::Name,
            span,
            format!("trait has no method '{method}'"),
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn source_trait_call(
        &mut self,
        module: usize,
        trait_id: TraitId,
        method: &str,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let (owner, signature, names) = self.trait_method(module, trait_id, method, span)?;
        let self_type = self
            .solver
            .try_fresh(
                &mut self.catalog.core.types,
                super::super::infer::Constraint::Any,
            )
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        let method_parameters = self
            .traits
            .registry
            .trait_data(owner)
            .map_err(|e| self.error(module, DK::Type, span, e.message))?
            .methods
            .iter()
            .find(|item| item.name == method)
            .unwrap()
            .type_parameters
            .len();
        let method_type_arguments = (0..method_parameters)
            .map(|_| {
                self.solver.try_fresh(
                    &mut self.catalog.core.types,
                    super::super::infer::Constraint::Any,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        let substitutions = [self_type]
            .into_iter()
            .chain(method_type_arguments.iter().copied())
            .collect::<Vec<_>>();
        let params = signature
            .params
            .iter()
            .map(|&ty| data::frontend_type(self.catalog.core.types.subst(ty, &substitutions)))
            .collect::<Vec<_>>();
        let ret = data::frontend_type(self.catalog.core.types.subst(signature.ret, &substitutions));
        if let Some(expected) = expected {
            self.expect_type(module, ret, expected, span)?;
        }
        let mut order = vec![usize::MAX; params.len()];
        let mut named = false;
        let mut checked = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let parameter = if let Some(label) = &arg.label {
                named = true;
                names
                    .iter()
                    .position(|n| *n == label.value)
                    .ok_or_else(|| {
                        self.error(module, DK::Type, label.span, "unknown trait argument label")
                    })?
            } else {
                if named {
                    return Err(self.error(
                        module,
                        DK::Type,
                        arg.value.span,
                        "positional arguments must precede named arguments",
                    ));
                }
                index
            };
            if parameter >= params.len() {
                return Err(self.error(module, DK::Type, span, "too many trait method arguments"));
            }
            if order[parameter] != usize::MAX {
                return Err(self.error(
                    module,
                    DK::Type,
                    arg.value.span,
                    "trait argument is supplied twice",
                ));
            }
            order[parameter] = index;
            checked.push(self.expr(module, &arg.value, locals, Some(params[parameter]))?);
        }
        if order.contains(&usize::MAX) {
            return Err(self.error(module, DK::Type, span, "missing trait method arguments"));
        }
        let receiver = self
            .solver
            .resolve(&mut self.catalog.core.types, self_type)
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        let method_type_arguments = method_type_arguments
            .into_iter()
            .map(|ty| self.solver.resolve(&mut self.catalog.core.types, ty))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        self.checked_generic_trait_call(
            module,
            trait_id,
            method,
            receiver,
            method_type_arguments,
            checked,
            order,
            false,
            ret,
            span,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn checked_trait_call(
        &mut self,
        module: usize,
        trait_id: TraitId,
        method: &str,
        self_type: b::TypeId,
        args: Vec<ir::Expr>,
        order: Vec<usize>,
        nested_show: bool,
        ret: Ty,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        self.checked_generic_trait_call(
            module,
            trait_id,
            method,
            self_type,
            vec![],
            args,
            order,
            nested_show,
            ret,
            span,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn checked_generic_trait_call(
        &mut self,
        module: usize,
        trait_id: TraitId,
        method: &str,
        self_type: b::TypeId,
        method_type_arguments: Vec<b::TypeId>,
        args: Vec<ir::Expr>,
        order: Vec<usize>,
        nested_show: bool,
        ret: Ty,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        self.trait_obligations.push(Obligation {
            module,
            self_type,
            trait_id,
            method: method.into(),
            method_type_arguments: method_type_arguments.clone(),
            bounds: self.current_bounds.clone(),
            span,
        });
        Ok(ir::Expr {
            kind: ir::ExprKind::TraitCall {
                trait_id,
                method: method.into(),
                self_type,
                args,
                order,
                method_type_arguments,
                nested_show,
            },
            ty: ret,
            span,
        })
    }
    pub(super) fn show(
        &mut self,
        module: usize,
        value: ir::Expr,
        nested: bool,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let trait_id = self.traits.builtin_traits[&BuiltinTrait::Show];
        let ty = data::backend_type(value.ty);
        self.checked_trait_call(
            module,
            trait_id,
            "show",
            ty,
            vec![value],
            vec![0],
            nested,
            Ty::String,
            span,
        )
    }
    pub(super) fn comparison(
        &mut self,
        module: usize,
        op: ast::Binary,
        left: ir::Expr,
        right: ir::Expr,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        use ast::Binary as Op;
        let receiver = self.resolve_type(module, left.ty, span)?;
        if matches!(receiver, Ty::Never)
            || matches!(right.ty, Ty::Never)
            || (receiver == Ty::Float && !matches!(op, Op::Eq | Op::Ne))
        {
            return Ok(ir::Expr {
                kind: ir::ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty: Ty::Bool,
                span,
            });
        }
        if matches!(op, Op::Eq | Op::Ne) {
            let id = self.traits.builtin_traits[&BuiltinTrait::Eq];
            let value = self.checked_trait_call(
                module,
                id,
                "eq",
                data::backend_type(receiver),
                vec![left, right],
                vec![0, 1],
                false,
                Ty::Bool,
                span,
            )?;
            return Ok(if op == Op::Eq {
                value
            } else {
                ir::Expr {
                    kind: ir::ExprKind::Unary {
                        op: ast::Unary::Not,
                        value: Box::new(value),
                    },
                    ty: Ty::Bool,
                    span,
                }
            });
        }
        let id = self.traits.builtin_traits[&BuiltinTrait::Ord];
        let (_, signature, _) = self.trait_method(module, id, "compare", span)?;
        let ordering = data::frontend_type(signature.ret);
        let call = self.checked_trait_call(
            module,
            id,
            "compare",
            data::backend_type(receiver),
            vec![left, right],
            vec![0, 1],
            false,
            ordering,
            span,
        )?;
        let Type::Enum(enum_id, _) = self.catalog.core.ty(signature.ret) else {
            return Err(self.error(module, DK::Type, span, "Ord.compare must return Ordering"));
        };
        let variant = if matches!(op, Op::Lt | Op::Ge) {
            "Less"
        } else {
            "Greater"
        };
        let index = self.catalog.core.enums[*enum_id]
            .variants
            .iter()
            .position(|v| v.name == variant)
            .ok_or_else(|| self.error(module, DK::Type, span, "Ordering constructor is missing"))?;
        let tag = ir::Expr {
            kind: ir::ExprKind::Tag(Box::new(call)),
            ty: Ty::Int,
            span,
        };
        Ok(ir::Expr {
            kind: ir::ExprKind::Binary {
                op: if matches!(op, Op::Lt | Op::Gt) {
                    Op::Eq
                } else {
                    Op::Ne
                },
                left: Box::new(tag),
                right: Box::new(ir::Expr {
                    kind: ir::ExprKind::Literal(ir::Literal::Int(index as i32)),
                    ty: Ty::Int,
                    span,
                }),
            },
            ty: Ty::Bool,
            span,
        })
    }
}
