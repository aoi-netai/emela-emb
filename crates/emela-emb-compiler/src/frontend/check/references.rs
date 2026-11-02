//! Function values use the same instantiation and checking rules as direct calls.
use super::*;
use crate::contract::ir::{self as b, Type, TypeId};

impl Checker {
    pub(super) fn function_value(
        &mut self,
        module: usize,
        id: DefinitionId,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        self.validate_external_reference(module, id, span)?;
        let (signature, type_arguments) = self.function_instance(module, id, span)?;
        let ret = match signature.ret {
            Some(ret) => ret,
            None => self.function(id)?,
        };
        let ty = self.signature_type(&signature, ret);
        if let Some(expected) = expected {
            self.expect_type(module, ty, expected, span)?;
        }
        self.validate_call_bounds(module, id, &type_arguments, span)?;
        if self.definitions[id.0].intrinsic.is_some() {
            // Intrinsics have no emitted function body. Check an ordinary call in
            // a wrapper so inspect, panic and other source rewrites still apply.
            // '$' cannot occur in source names; the alias never escapes checking.
            let name = format!("$intrinsic_reference_{}", id.0);
            let previous = self.scopes[module]
                .values
                .insert(name.clone(), Symbol::Function(id));
            let mut wrapper_locals = Locals {
                names: HashMap::new(),
                next: locals.next,
            };
            let params = signature
                .params
                .iter()
                .enumerate()
                .map(|(index, &ty)| wrapper_locals.bind(format!("$reference_argument_{index}"), ty))
                .collect::<Vec<_>>();
            let call = ast::Expr {
                kind: ast::ExprKind::Call {
                    callee: Box::new(ast::Expr {
                        kind: ast::ExprKind::Name(vec![ast::Text::new(&name, span)]),
                        span,
                    }),
                    args: params
                        .iter()
                        .map(|param| ast::Argument {
                            label: None,
                            value: ast::Expr {
                                kind: ast::ExprKind::Name(vec![ast::Text::new(&param.name, span)]),
                                span,
                            },
                        })
                        .collect(),
                },
                span,
            };
            let body = self.expr(module, &call, &mut wrapper_locals, Some(ret));
            locals.next = wrapper_locals.next;
            match previous {
                Some(symbol) => {
                    self.scopes[module].values.insert(name, symbol);
                }
                None => {
                    self.scopes[module].values.remove(&name);
                }
            }
            return Ok(reference_lambda(
                params,
                body?,
                self.resolve_type(module, ty, span)?,
                span,
            ));
        }
        Ok(ir::Expr {
            kind: ir::ExprKind::FnRef {
                target: id,
                type_arguments,
            },
            ty: self.resolve_type(module, ty, span)?,
            span,
        })
    }

    pub(super) fn constructor_value(
        &mut self,
        module: usize,
        ctor: b::CtorRef,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let declaration = self.catalog.declaration(ctor.enum_id);
        let mut bindings = vec![None; declaration.ast.params.len()];
        let raw = self.catalog.core.enums[ctor.enum_id].variants[ctor.variant]
            .tys
            .clone();
        if raw.is_empty() {
            return self.construct(module, ctor, &[], locals, expected, span);
        }
        let arguments = self.constructor_arguments(module, &mut bindings, span)?;
        let params = raw
            .iter()
            .map(|&ty| {
                data::frontend_type(data::substitute(
                    &mut self.catalog.core.types,
                    ty,
                    &arguments,
                ))
            })
            .collect::<Vec<_>>();
        let ret = data::frontend_type(self.catalog.core.types.enum_type(ctor.enum_id, arguments));
        let ty = self.signature_type(
            &Signature {
                params: params.clone(),
                ret: Some(ret),
                sets: InvocationSets::default(),
            },
            ret,
        );
        if let Some(expected) = expected {
            self.expect_type(module, ty, expected, span)?;
        }
        let ret = self.resolve_type(module, ret, span)?;
        self.validate_type_bounds(
            module,
            data::backend_type(ret),
            &self.current_bounds.clone(),
            span,
        )?;
        let params = reference_params(locals, &params);
        let body = ir::Expr {
            kind: ir::ExprKind::Make {
                ctor,
                args: reference_arguments(&params, span),
                order: (0..params.len()).collect(),
            },
            ty: ret,
            span,
        };
        Ok(reference_lambda(
            params,
            body,
            self.resolve_type(module, ty, span)?,
            span,
        ))
    }

    pub(super) fn trait_function_value(
        &mut self,
        module: usize,
        trait_id: super::super::traits::TraitId,
        method: &str,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let (owner, signature, _) = self.trait_method(module, trait_id, method, span)?;
        let method_parameters = self
            .traits
            .registry
            .trait_data(owner)
            .map_err(|error| self.error(module, DK::Type, span, error.message))?
            .methods
            .iter()
            .find(|item| item.name == method)
            .expect("resolved trait method")
            .type_parameters
            .len();
        let substitutions = (0..=method_parameters)
            .map(|_| {
                self.solver.try_fresh(
                    &mut self.catalog.core.types,
                    super::super::infer::Constraint::Any,
                )
            })
            .collect::<Result<Vec<TypeId>, _>>()
            .map_err(|error| self.error(module, DK::Type, span, error.message))?;
        let function = self.catalog.core.types.intern(Type::Fn(signature));
        let ty = data::frontend_type(data::substitute(
            &mut self.catalog.core.types,
            function,
            &substitutions,
        ));
        if let Some(expected) = expected {
            self.expect_type(module, ty, expected, span)?;
        }
        let ty = self.resolve_type(module, ty, span)?;
        let Type::Fn(signature) = self.catalog.core.ty(data::backend_type(ty)).clone() else {
            unreachable!("instantiation preserves a function signature")
        };
        let params = reference_params(
            locals,
            &signature
                .params
                .iter()
                .copied()
                .map(data::frontend_type)
                .collect::<Vec<_>>(),
        );
        let substitutions = substitutions
            .into_iter()
            .map(|ty| self.solver.resolve(&mut self.catalog.core.types, ty))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| self.error(module, DK::Type, span, error.message))?;
        let body = self.checked_generic_trait_call(
            module,
            trait_id,
            method,
            substitutions[0],
            substitutions[1..].to_vec(),
            reference_arguments(&params, span),
            (0..params.len()).collect(),
            false,
            data::frontend_type(signature.ret),
            span,
        )?;
        Ok(reference_lambda(params, body, ty, span))
    }
}

fn reference_params(locals: &mut Locals, types: &[Ty]) -> Vec<Local> {
    types
        .iter()
        .enumerate()
        .map(|(index, &ty)| {
            let id = LocalId(locals.next);
            locals.next += 1;
            Local {
                id,
                name: format!("$reference_argument_{index}"),
                ty,
            }
        })
        .collect()
}

fn reference_arguments(params: &[Local], span: Span) -> Vec<ir::Expr> {
    params
        .iter()
        .map(|param| ir::Expr {
            kind: ir::ExprKind::Local(param.id),
            ty: param.ty,
            span,
        })
        .collect()
}

fn reference_lambda(params: Vec<Local>, body: ir::Expr, ty: Ty, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Lambda {
            params,
            captures: vec![],
            body: ir::Block {
                ty: body.ty,
                statements: vec![ir::Statement::Expr(body)],
                span,
            },
        },
        ty,
        span,
    }
}
