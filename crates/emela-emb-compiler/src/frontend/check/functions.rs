//! Function signatures shared by direct calls, function references and closures.
use super::*;
use crate::contract::ir::{self as b, Type, TypeId};
pub(super) struct CaptureScope {
    pub boundary: usize,
    pub captures: HashSet<LocalId>,
}
impl Checker {
    fn infer_parameter_pattern(
        &mut self,
        module: usize,
        pattern: &ast::Pattern,
        ty: Ty,
    ) -> Result<Ty, Diagnostic> {
        let ty = self.resolve_type(module, ty, pattern.span)?;
        if !self.solver.is_meta(data::backend_type(ty)) {
            return Ok(ty);
        }
        let inferred = match &pattern.kind {
            ast::PatternKind::Tuple(fields) => {
                let fields = fields
                    .iter()
                    .map(|_| {
                        self.solver.try_fresh(
                            &mut self.catalog.core.types,
                            super::super::infer::Constraint::Any,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| self.error(module, DK::Type, pattern.span, error.message))?;
                Some(self.catalog.core.types.intern(Type::Tuple(fields)))
            }
            ast::PatternKind::Constructor { path, .. } => match self.resolve(module, path)? {
                Symbol::Constructor(ctor) => {
                    let arity = self.catalog.declaration(ctor.enum_id).ast.params.len();
                    let arguments = (0..arity)
                        .map(|_| {
                            self.solver.try_fresh(
                                &mut self.catalog.core.types,
                                super::super::infer::Constraint::Any,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| {
                            self.error(module, DK::Type, pattern.span, error.message)
                        })?;
                    Some(self.catalog.core.types.enum_type(ctor.enum_id, arguments))
                }
                _ => None,
            },
            ast::PatternKind::List { .. } => {
                let element = self
                    .solver
                    .try_fresh(
                        &mut self.catalog.core.types,
                        super::super::infer::Constraint::Any,
                    )
                    .map_err(|error| self.error(module, DK::Type, pattern.span, error.message))?;
                Some(self.catalog.core.types.intern(Type::List(element)))
            }
            _ => None,
        };
        if let Some(inferred) = inferred {
            self.expect_type(module, ty, data::frontend_type(inferred), pattern.span)?;
        }
        self.resolve_type(module, ty, pattern.span)
    }
    pub(super) fn classify_parameter_kinds(
        &mut self,
        module: usize,
        function: &ast::Function,
        signature: &Signature,
        parameters: &[TypeId],
    ) -> Result<(), Diagnostic> {
        let mut value = HashSet::new();
        let mut errors = HashSet::new();
        let mut effects = HashSet::new();
        let mut pending = signature
            .params
            .iter()
            .copied()
            .chain(signature.ret)
            .map(|ty| (data::backend_type(ty), None))
            .collect::<Vec<_>>();
        pending.extend(
            signature
                .sets
                .errors
                .iter()
                .copied()
                .map(|ty| (ty, Some(super::super::sets::SetKind::Error))),
        );
        pending.extend(
            signature
                .sets
                .effects
                .iter()
                .copied()
                .map(|ty| (ty, Some(super::super::sets::SetKind::Effect))),
        );
        let mut seen = HashSet::new();
        while let Some((ty, kind)) = pending.pop() {
            if !seen.insert((
                ty,
                kind.map(|kind| kind == super::super::sets::SetKind::Effect),
            )) {
                continue;
            }
            match self.catalog.core.ty(ty) {
                Type::Param(_) => match kind {
                    None => {
                        value.insert(ty);
                    }
                    Some(super::super::sets::SetKind::Error) => {
                        errors.insert(ty);
                    }
                    Some(super::super::sets::SetKind::Effect) => {
                        effects.insert(ty);
                    }
                },
                Type::Tuple(fields) | Type::Enum(_, fields) => {
                    pending.extend(fields.iter().copied().map(|ty| (ty, None)))
                }
                Type::List(ty) | Type::Cell(ty) => pending.push((*ty, None)),
                Type::Handler(id) => pending.extend(
                    self.catalog.core.effects[*id]
                        .args
                        .iter()
                        .copied()
                        .map(|ty| (ty, None)),
                ),
                Type::Fn(function) => {
                    pending.extend(
                        function
                            .params
                            .iter()
                            .copied()
                            .chain([function.ret])
                            .map(|ty| (ty, None)),
                    );
                    if let Some(sets) = function.sets {
                        pending.extend([
                            (sets.errors, Some(super::super::sets::SetKind::Error)),
                            (sets.effects, Some(super::super::sets::SetKind::Effect)),
                        ]);
                    }
                }
                Type::Set(row) => pending.extend(row.params.iter().copied().map(|ty| (ty, kind))),
                _ => {}
            }
        }
        for (&ty, parameter) in parameters.iter().zip(&function.type_params) {
            let kind = if effects.contains(&ty) {
                if errors.contains(&ty) || value.contains(&ty) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        parameter.name.span,
                        "an effect-set parameter cannot also be a value type or error set",
                    ));
                }
                Some(super::super::sets::SetKind::Effect)
            } else if errors.contains(&ty) && !value.contains(&ty) {
                Some(super::super::sets::SetKind::Error)
            } else {
                None
            };
            if let Some(kind) = kind {
                self.solver
                    .register_row(ty, kind, super::super::sets::Origin::OpaqueParameter)
                    .map_err(|error| {
                        self.error(module, DK::Type, parameter.name.span, error.message)
                    })?;
            }
            for bound in &parameter.bounds {
                if bound.value == "Immediate" {
                    self.solver
                        .declare_row_immediate(ty)
                        .map_err(|error| self.error(module, DK::Type, bound.span, error.message))?;
                } else if kind.is_some() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        bound.span,
                        "set parameters support only the Immediate effect bound",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn coerce_function(
        &mut self,
        module: usize,
        value: ir::Expr,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let Some(expected) = expected else {
            return Ok(value);
        };
        let actual = self.resolve_type(module, value.ty, span)?;
        let expected = self.resolve_type(module, expected, span)?;
        if actual != expected
            && matches!(
                self.catalog.core.ty(data::backend_type(actual)),
                Type::Fn(_)
            )
            && matches!(
                self.catalog.core.ty(data::backend_type(expected)),
                Type::Fn(_)
            )
        {
            Ok(ir::Expr {
                kind: ir::ExprKind::Coerce(Box::new(value)),
                ty: expected,
                span,
            })
        } else {
            Ok(value)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lambda(
        &mut self,
        module: usize,
        parameters: &[ast::LambdaParameter],
        body: &ast::Block,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let expected = expected
            .map(|ty| self.resolve_type(module, ty, span))
            .transpose()?;
        let expected_fn =
            expected.and_then(|ty| match self.catalog.core.ty(data::backend_type(ty)) {
                Type::Fn(signature) => Some(signature.clone()),
                _ => None,
            });
        if expected_fn
            .as_ref()
            .is_some_and(|signature| signature.params.len() != parameters.len())
        {
            return Err(self.error(
                module,
                DK::Type,
                span,
                "anonymous function parameter count differs from the expected function type",
            ));
        }
        let mut lambda_locals = Locals {
            names: locals.names.clone(),
            next: locals.next,
        };
        self.closures.push(CaptureScope {
            boundary: locals.next,
            captures: HashSet::new(),
        });
        let checked = (|| {
            let mut params = Vec::new();
            let mut bindings = Vec::new();
            let mut names = HashSet::new();
            for (index, parameter) in parameters.iter().enumerate() {
                let expected = expected_fn
                    .as_ref()
                    .map(|signature| data::frontend_type(signature.params[index]));
                let annotation = parameter
                    .ty
                    .as_ref()
                    .map(|ty| {
                        self.catalog.resolve_type_with_self(
                            &self.modules,
                            module,
                            ty,
                            &self.current_parameters,
                            self.current_self_type.map(data::frontend_type),
                        )
                    })
                    .transpose()
                    .map_err(|error| error.in_source(&self.modules.modules[module].source_name))?;
                let ty = match (annotation, expected) {
                    (Some(annotation), Some(expected)) => {
                        self.expect_type(module, annotation, expected, parameter.pattern.span)?;
                        annotation
                    }
                    (Some(annotation), None) | (None, Some(annotation)) => annotation,
                    (None, None) => data::frontend_type(
                        self.solver
                            .try_fresh(
                                &mut self.catalog.core.types,
                                super::super::infer::Constraint::Any,
                            )
                            .map_err(|error| {
                                self.error(module, DK::Type, parameter.pattern.span, error.message)
                            })?,
                    ),
                };
                let ty = self.infer_parameter_pattern(module, &parameter.pattern, ty)?;
                let local = lambda_locals.bind(format!("$lambda_parameter_{index}"), ty);
                let pattern = self.pattern(
                    module,
                    &parameter.pattern,
                    ty,
                    &mut lambda_locals,
                    &mut names,
                )?;
                if !super::super::patterns::exhaustive(
                    std::slice::from_ref(&pattern),
                    ty,
                    &mut self.catalog,
                ) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        parameter.pattern.span,
                        "anonymous function parameter pattern must always match",
                    ));
                }
                bindings.push(ir::Statement::Pattern {
                    pattern,
                    value: ir::Expr {
                        kind: ir::ExprKind::Local(local.id),
                        ty,
                        span: parameter.pattern.span,
                    },
                });
                params.push(local);
            }
            let result = expected_fn
                .as_ref()
                .map(|signature| data::frontend_type(signature.ret));
            let mut body = self.block(module, body, &mut lambda_locals, result)?;
            bindings.append(&mut body.statements);
            body.statements = bindings;
            let errors = self.known_errors(
                module,
                &ir::Expr {
                    kind: ir::ExprKind::Block(body.clone()),
                    ty: body.ty,
                    span: body.span,
                },
            )?;
            let signature = Signature {
                params: params.iter().map(|parameter| parameter.ty).collect(),
                ret: Some(result.unwrap_or(body.ty)),
                sets: InvocationSets {
                    errors,
                    effects: {
                        let functions = self.checked.iter().flatten().cloned().collect::<Vec<_>>();
                        super::super::effects::incoming(
                            &ir::Expr {
                                kind: ir::ExprKind::Block(body.clone()),
                                ty: body.ty,
                                span: body.span,
                            },
                            &functions,
                            &self.traits.registry,
                            &mut self.catalog,
                            &mut self.solver,
                            &self.schemes.parameters,
                            &self.modules.modules[module].source_name,
                        )?
                    },
                    ..Default::default()
                },
            };
            let ty = self.signature_type(&signature, signature.ret.unwrap());
            if let Some(expected) = expected {
                self.expect_type(module, ty, expected, span)?;
            }
            Ok((params, body, ty))
        })();
        let scope = self.closures.pop().expect("active closure capture scope");
        locals.next = lambda_locals.next;
        let (params, body, ty) = checked?;
        let mut captures = scope.captures.into_iter().collect::<Vec<_>>();
        captures.sort_by_key(|id| id.0);
        Ok(ir::Expr {
            kind: ir::ExprKind::Lambda {
                params,
                captures,
                body,
            },
            ty,
            span,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(
        &mut self,
        module: usize,
        callee: &ast::Expr,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let callee = self.expr(module, callee, locals, None)?;
        self.apply_value(module, callee, args, locals, expected, span)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_value(
        &mut self,
        module: usize,
        mut callee: ir::Expr,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        if let Some(label) = args.iter().find_map(|argument| argument.label.as_ref()) {
            return Err(self.error(
                module,
                DK::Type,
                label.span,
                "function values accept positional arguments only",
            ));
        }
        callee.ty = self.resolve_type(module, callee.ty, span)?;
        if self.solver.is_meta(data::backend_type(callee.ty)) {
            let mut parameter_types = Vec::new();
            for _ in args {
                parameter_types.push(data::frontend_type(
                    self.solver
                        .try_fresh(
                            &mut self.catalog.core.types,
                            super::super::infer::Constraint::Any,
                        )
                        .map_err(|error| self.error(module, DK::Type, span, error.message))?,
                ));
            }
            let ret = match expected {
                Some(ty) => ty,
                None => data::frontend_type(
                    self.solver
                        .try_fresh(
                            &mut self.catalog.core.types,
                            super::super::infer::Constraint::Any,
                        )
                        .map_err(|error| self.error(module, DK::Type, span, error.message))?,
                ),
            };
            let function_type = self.signature_type(
                &Signature {
                    params: parameter_types,
                    ret: Some(ret),
                    sets: Default::default(),
                },
                ret,
            );
            self.expect_type(module, callee.ty, function_type, span)?;
            callee.ty = self.resolve_type(module, callee.ty, span)?;
        }
        let Type::Fn(signature) = self.catalog.core.ty(data::backend_type(callee.ty)).clone()
        else {
            return Err(self.error(
                module,
                DK::Type,
                callee.span,
                "only a value of function type can be called",
            ));
        };
        if signature.params.len() != args.len() {
            return Err(self.error(
                module,
                DK::Type,
                span,
                "function value argument count differs from its type",
            ));
        }
        let ret = data::frontend_type(signature.ret);
        if let Some(expected) = expected {
            self.expect_type(module, ret, expected, span)?;
        }
        let mut checked = Vec::new();
        for (argument, parameter) in args.iter().zip(&signature.params) {
            checked.push(self.expr(
                module,
                &argument.value,
                locals,
                Some(data::frontend_type(*parameter)),
            )?);
        }
        Ok(ir::Expr {
            kind: ir::ExprKind::Apply {
                callee: Box::new(callee),
                args: checked,
                order: (0..args.len()).collect(),
            },
            ty: ret,
            span,
        })
    }
    pub(super) fn function_instance(
        &mut self,
        module: usize,
        id: DefinitionId,
        span: Span,
    ) -> Result<(Signature, Vec<TypeId>), Diagnostic> {
        self.function(id)?;
        let mut signature = self.definitions[id.0].signature.clone();
        let type_arguments = (0..self.schemes.parameters[id.0].len())
            .map(|_| {
                self.solver.try_fresh(
                    &mut self.catalog.core.types,
                    super::super::infer::Constraint::Any,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        let substitutions: HashMap<_, _> = self.schemes.parameters[id.0]
            .iter()
            .copied()
            .zip(type_arguments.iter().copied())
            .collect();
        for (&source, &destination) in &substitutions {
            self.solver
                .copy_row_kind(source, destination)
                .map_err(|error| self.error(module, DK::Type, span, error.message))?;
        }
        if !type_arguments.is_empty() {
            if let Some(ret) = signature.ret {
                let resolved = self
                    .solver
                    .resolve(&mut self.catalog.core.types, data::backend_type(ret))
                    .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                if self.solver.is_meta(resolved) {
                    let instance = self
                        .solver
                        .try_fresh(
                            &mut self.catalog.core.types,
                            super::super::infer::Constraint::Any,
                        )
                        .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                    self.schemes
                        .defer_return(id, resolved, instance, substitutions.clone());
                    signature.ret = Some(data::frontend_type(instance));
                }
            }
        }
        for parameter in signature.params.iter_mut().chain(signature.ret.iter_mut()) {
            let ty = self.catalog.effects.substitute_map(
                &mut self.catalog.core,
                data::backend_type(*parameter),
                &substitutions,
            );
            *parameter = data::frontend_type(
                self.solver
                    .substitute(&mut self.catalog.core.types, ty, &substitutions)
                    .map_err(|e| self.error(module, DK::Type, span, e.message))?,
            );
        }
        for ty in signature
            .sets
            .errors
            .iter_mut()
            .chain(&mut signature.sets.error_constraints)
            .chain(&mut signature.sets.effects)
        {
            *ty = self
                .catalog
                .effects
                .substitute_map(&mut self.catalog.core, *ty, &substitutions);
            *ty = self
                .solver
                .substitute(&mut self.catalog.core.types, *ty, &substitutions)
                .map_err(|error| self.error(module, DK::Type, span, error.message))?;
        }
        Ok((signature, type_arguments))
    }
    pub(super) fn signature_type(&mut self, signature: &Signature, ret: Ty) -> Ty {
        let fails = !signature.sets.errors.is_empty();
        let errors = self.catalog.core.types.intern(Type::Set(b::SetSummary {
            params: signature.sets.errors.clone(),
            ..Default::default()
        }));
        let effects = self.catalog.core.types.intern(Type::Set(b::SetSummary {
            params: signature.sets.effects.clone(),
            ..Default::default()
        }));
        Ty::Compound(
            self.catalog.core.types.intern(Type::Fn(b::FnType {
                params: signature
                    .params
                    .iter()
                    .copied()
                    .map(data::backend_type)
                    .collect(),
                ret: data::backend_type(ret),
                fails,
                suspends: false,
                ev: !signature.sets.effects.is_empty(),
                sets: Some(b::FnSets { errors, effects }),
            })),
        )
    }
}
