//! Infer ambient effect requirements while preserving explicit provider boundaries.
use super::{
    data::{Catalog, backend_type},
    ir,
    traits::Registry,
};
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{EffectId, SetSummary, Type, TypeId},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct Mode {
    pub allowed: Option<Vec<TypeId>>,
}

#[cfg(test)]
#[path = "effects_tests.rs"]
mod tests;
type Requirements = HashSet<TypeId>;
fn sorted(requirements: Requirements) -> Vec<TypeId> {
    let mut values = requirements.into_iter().collect::<Vec<_>>();
    values.sort();
    values
}

pub(crate) fn infer(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
) -> Result<(), Vec<Diagnostic>> {
    phase(program, modes, catalog, None, None)
}
pub(crate) fn constrain(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
) -> Result<(), Vec<Diagnostic>> {
    phase(program, modes, catalog, Some(solver), Some(parameters))
}
pub(crate) fn incoming(
    expression: &ir::Expr,
    functions: &[ir::Function],
    registry: &Registry,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
    source: &str,
) -> Result<Vec<TypeId>, Diagnostic> {
    invocation(
        expression,
        functions,
        registry,
        catalog,
        solver,
        parameters,
        source,
        &HashSet::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn latent(
    expression: &ir::Expr,
    functions: &[ir::Function],
    registry: &Registry,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
    source: &str,
    preserved_rows: &HashSet<TypeId>,
) -> Result<Vec<TypeId>, Diagnostic> {
    invocation(
        expression,
        functions,
        registry,
        catalog,
        solver,
        parameters,
        source,
        preserved_rows,
    )
}

#[allow(clippy::too_many_arguments)]
fn invocation(
    expression: &ir::Expr,
    functions: &[ir::Function],
    registry: &Registry,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
    source: &str,
    preserved_rows: &HashSet<TypeId>,
) -> Result<Vec<TypeId>, Diagnostic> {
    let index = functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.id, index))
        .collect();
    let mut context = Context {
        catalog,
        functions,
        index: &index,
        registry,
        source,
        solver: Some(solver),
        parameters: Some(parameters),
        preserved_rows: preserved_rows.clone(),
    };
    Ok(sorted(context.expression(expression)?))
}
fn phase(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
    mut solver: Option<&mut super::infer::Solver>,
    parameters: Option<&[Vec<TypeId>]>,
) -> Result<(), Vec<Diagnostic>> {
    let index = program
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.id, index))
        .collect();
    for round in 0..1024 {
        let mut changed = false;
        for i in 0..program.functions.len() {
            let function = program.functions[i].clone();
            let mut context = Context {
                catalog,
                functions: &program.functions,
                index: &index,
                registry: &program.registry,
                source: &function.source_name,
                solver: solver.as_deref_mut(),
                parameters,
                preserved_rows: HashSet::new(),
            };
            let needed = match &function.body {
                Some(body) => context.block(body),
                None => context.members(&function.signature.sets.effects, function.span),
            }
            .map_err(|error| vec![error])?;
            let declaration = modes
                .get(&function.id)
                .and_then(|mode| mode.allowed.as_ref());
            let effects = if let Some(allowed) = declaration {
                // After normalization the signature contains position-based parameters;
                // the source Mode still refers to its declaration's original identities.
                if context.solver.is_some() {
                    allowed.clone()
                } else {
                    function.signature.sets.effects.clone()
                }
            } else {
                sorted(needed.clone())
            };
            if declaration.is_some() || function.constant {
                let allowed = if function.constant {
                    &[][..]
                } else {
                    effects.as_slice()
                };
                context
                    .includes(&needed, allowed, function.span)
                    .map_err(|error| vec![error])?;
            }
            if effects != program.functions[i].signature.sets.effects {
                program.functions[i].signature.sets.effects = effects;
                changed = true;
            }
        }
        if !changed {
            program.core = catalog.core.clone();
            return Ok(());
        }
        if round == 1023 {
            return Err(vec![Diagnostic::new(
                DiagnosticKind::Type,
                Span::new(0, 0),
                "effect inference does not converge; annotate an expanding recursive signature",
            )]);
        }
    }
    unreachable!("bounded effect fixed point")
}

struct Context<'a> {
    catalog: &'a mut Catalog,
    functions: &'a [ir::Function],
    index: &'a HashMap<ir::DefinitionId, usize>,
    registry: &'a Registry,
    source: &'a str,
    solver: Option<&'a mut super::infer::Solver>,
    parameters: Option<&'a [Vec<TypeId>]>,
    preserved_rows: HashSet<TypeId>,
}
impl Context<'_> {
    fn escaped_resume(&self, operation: &ir::Expr) -> bool {
        let ir::ExprKind::Lambda { body, .. } = &operation.kind else {
            return false;
        };
        let Some(ir::Statement::Expr(ir::Expr {
            kind: ir::ExprKind::Call { target, .. },
            ..
        })) = body.statements.last()
        else {
            return false;
        };
        let Some(&index) = self.index.get(target) else {
            // A recursive operation body may still be under construction during
            // source constraints. The complete fixed point revisits it later.
            return false;
        };
        let function = &self.functions[index];
        if function.immediate {
            return false;
        }
        let (Some(resume), Some(body)) = (function.params.last(), &function.body) else {
            return false;
        };
        super::check::suspend::resume_escapes(body, resume.id)
    }
    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(DiagnosticKind::Type, span, message).in_source(self.source)
    }
    fn resolve(&mut self, ty: TypeId, span: Span) -> Result<TypeId, Diagnostic> {
        let Some(solver) = self.solver.as_deref_mut() else {
            return Ok(ty);
        };
        let ty = solver
            .resolve(&mut self.catalog.core.types, ty)
            .map_err(|error| self.error(span, error.message))?;
        if !self.preserved_rows.contains(&ty)
            && self.solver.as_ref().is_some_and(|solver| {
                solver.row_kind(ty) == Some(super::sets::SetKind::Effect) && solver.is_meta(ty)
            })
        {
            return self
                .solver
                .as_deref_mut()
                .unwrap()
                .resolve_row(&mut self.catalog.core.types, ty)
                .map_err(|error| {
                    Diagnostic::new(DiagnosticKind::Type, span, error.message)
                        .in_source(self.source)
                });
        }
        if let Type::Handler(effect) = self.catalog.core.ty(ty) {
            let arguments = self.catalog.core.effects[*effect].args.clone();
            let mut replacements = HashMap::new();
            for argument in arguments {
                let resolved = self
                    .solver
                    .as_deref_mut()
                    .unwrap()
                    .resolve(&mut self.catalog.core.types, argument)
                    .map_err(|error| self.error(span, error.message))?;
                replacements.insert(argument, resolved);
            }
            return Ok(self.catalog.effects.substitute_map(
                &mut self.catalog.core,
                ty,
                &replacements,
            ));
        }
        Ok(ty)
    }
    fn instantiate(
        &mut self,
        target: ir::DefinitionId,
        ty: TypeId,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<TypeId, Diagnostic> {
        if let (Some(parameters), Some(_)) = (self.parameters, self.solver.as_ref()) {
            let parameters = parameters
                .get(target.0)
                .ok_or_else(|| self.error(span, "missing effect signature scheme"))?;
            if arguments.len() > parameters.len() {
                return Err(self.error(span, "call supplies too many effect signature arguments"));
            }
            let mut substitutions = HashMap::new();
            for (&parameter, argument) in parameters.iter().zip(
                arguments
                    .iter()
                    .copied()
                    .chain(parameters[arguments.len()..].iter().copied()),
            ) {
                substitutions.insert(parameter, self.resolve(argument, span)?);
            }
            let ty =
                self.catalog
                    .effects
                    .substitute_map(&mut self.catalog.core, ty, &substitutions);
            self.resolve(ty, span)
        } else {
            Ok(self
                .catalog
                .effects
                .substitute_type(&mut self.catalog.core, ty, arguments))
        }
    }
    fn members(&mut self, values: &[TypeId], span: Span) -> Result<Requirements, Diagnostic> {
        let mut result = HashSet::new();
        let mut pending = values.to_vec();
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            let ty = self.resolve(ty, span)?;
            if !seen.insert(ty) {
                continue;
            }
            // Rigid set parameters remain indivisible. Inferred rows expose their
            // current lower bound so an outer provider can satisfy a known residual.
            match self.catalog.core.ty(ty).clone() {
                Type::Set(summary) => pending.extend(summary.params),
                Type::Param(_) => {
                    result.insert(ty);
                }
                Type::Handler(_) => {
                    result.insert(self.resolve(ty, span)?);
                }
                _ => {
                    return Err(self.error(
                        span,
                        "an effect set member must be an effect or a set parameter",
                    ));
                }
            }
        }
        Ok(result)
    }
    fn same_effect(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        match (self.catalog.core.ty(left), self.catalog.core.ty(right)) {
            (Type::Handler(left), Type::Handler(right)) => {
                let left = &self.catalog.core.effects[*left];
                let right = &self.catalog.core.effects[*right];
                left.decl == right.decl && left.args == right.args
            }
            _ => false,
        }
    }
    fn without(
        &mut self,
        mut needed: Requirements,
        supplied: &[EffectId],
        span: Span,
    ) -> Result<Requirements, Diagnostic> {
        let mut providers = Vec::new();
        for &effect in supplied {
            let ty = self.catalog.core.types.intern(Type::Handler(effect));
            providers.push(self.resolve(ty, span)?);
        }
        needed.retain(|&member| {
            !providers
                .iter()
                .any(|&provider| self.same_effect(member, provider))
        });
        Ok(needed)
    }
    fn includes(
        &mut self,
        needed: &Requirements,
        allowed: &[TypeId],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let allowed = self.members(allowed, span)?;
        if let Some(solver) = self.solver.as_deref_mut() {
            solver.register_effects(&self.catalog.core);
            let actual = self.catalog.core.types.intern(Type::Set(SetSummary {
                params: sorted(needed.clone()),
                ..Default::default()
            }));
            let expected = self.catalog.core.types.intern(Type::Set(SetSummary {
                params: sorted(allowed),
                ..Default::default()
            }));
            return solver
                .include_sets(
                    &mut self.catalog.core.types,
                    actual,
                    expected,
                    super::sets::SetKind::Effect,
                )
                .map_err(|error| self.error(span, error.message));
        }
        if needed.iter().any(|&actual| {
            !allowed
                .iter()
                .any(|&expected| self.same_effect(actual, expected))
        }) {
            return Err(self.error(span, "function requires an undeclared effect; add a use annotation or provide its handler"));
        }
        Ok(())
    }
    fn callback(&mut self, expression: &ir::Expr) -> Result<Requirements, Diagnostic> {
        let ty = self.resolve(backend_type(expression.ty), expression.span)?;
        let Type::Fn(signature) = self.catalog.core.ty(ty).clone() else {
            return Err(self.error(
                expression.span,
                "effect inference requires a callable value",
            ));
        };
        match signature.sets {
            Some(sets) => self.members(&[sets.effects], expression.span),
            None if !signature.ev => Ok(HashSet::new()),
            None => Err(self.error(
                expression.span,
                "effectful function value has lost its source effect identities",
            )),
        }
    }
    fn handler_operation(&mut self, operation: &ir::Expr) -> Result<Requirements, Diagnostic> {
        match &operation.kind {
            ir::ExprKind::Lambda { body, .. } => self.block(body),
            ir::ExprKind::Coerce(value) => self.handler_operation(value),
            _ => {
                let mut needed = self.expression(operation)?;
                needed.extend(self.callback(operation)?);
                Ok(needed)
            }
        }
    }
    fn pattern(&mut self, pattern: &ir::Pattern) -> Result<Requirements, Diagnostic> {
        let mut needed = HashSet::new();
        match &pattern.kind {
            ir::PatternKind::Constant { value, .. } => needed.extend(self.expression(value)?),
            ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
                for field in fields {
                    needed.extend(self.pattern(field)?);
                }
            }
            ir::PatternKind::ListCons { head, tail } => {
                needed.extend(self.pattern(head)?);
                needed.extend(self.pattern(tail)?);
            }
            _ => {}
        }
        Ok(needed)
    }
    fn block(&mut self, block: &ir::Block) -> Result<Requirements, Diagnostic> {
        let mut needed = HashSet::new();
        for statement in &block.statements {
            if let ir::Statement::Pattern { pattern, .. } = statement {
                needed.extend(self.pattern(pattern)?);
            }
            let (ir::Statement::Expr(value) | ir::Statement::Pattern { value, .. }) = statement;
            needed.extend(self.expression(value)?);
        }
        Ok(needed)
    }
    fn expression(&mut self, expression: &ir::Expr) -> Result<Requirements, Diagnostic> {
        use ir::ExprKind as E;
        let mut needed = HashSet::new();
        match &expression.kind {
            E::Literal(_) | E::Local(_) | E::FnRef { .. } | E::Lambda { .. } => {}
            E::Use { effect } => {
                let ty = self.catalog.core.types.intern(Type::Handler(*effect));
                needed.insert(self.resolve(ty, expression.span)?);
            }
            E::OpCall { handler, args, .. } => {
                needed.extend(self.expression(handler)?);
                for argument in args {
                    needed.extend(self.expression(argument)?);
                }
            }
            E::Handler {
                effect, operations, ..
            } => {
                for (index, operation) in operations.iter().enumerate() {
                    needed.extend(self.handler_operation(operation)?);
                    if self.catalog.core.effects[*effect].ops[index].suspend
                        && self.escaped_resume(operation)
                    {
                        let async_effect = self.catalog.core.effects.iter().find_map(|(id, _)| {
                            super::function_abi::is_async(&self.catalog.core, id).then_some(id)
                        });
                        if let Some(async_effect) = async_effect {
                            needed.insert(
                                self.catalog.core.types.intern(Type::Handler(async_effect)),
                            );
                        }
                    }
                }
            }
            E::Provide { bindings, body } => {
                // Provide constructs all bound values in the incoming environment.
                for (_, value) in bindings {
                    needed.extend(self.expression(value)?);
                }
                let body = self.block(body)?;
                needed.extend(
                    self.without(
                        body,
                        &bindings
                            .iter()
                            .map(|(effect, _)| *effect)
                            .collect::<Vec<_>>(),
                        expression.span,
                    )?,
                );
            }
            E::With { bindings, body } => {
                let mut supplied = Vec::new();
                for binding in bindings {
                    let requirements = self.expression(&binding.value)?;
                    needed.extend(self.without(requirements, &supplied, binding.value.span)?);
                    // A release can use previously acquired capabilities. The current
                    // capability has already been retired when its release is invoked.
                    if let Some(cleanup) = &binding.cleanup {
                        let requirements = self.expression(cleanup)?;
                        needed.extend(self.without(requirements, &supplied, cleanup.span)?);
                    }
                    supplied.extend(&binding.effects);
                }
                let body = self.block(body)?;
                needed.extend(self.without(body, &supplied, expression.span)?);
            }
            E::Apply { callee, args, .. } => {
                needed.extend(self.expression(callee)?);
                for argument in args {
                    needed.extend(self.expression(argument)?);
                }
                needed.extend(self.callback(callee)?);
            }
            E::Call {
                target,
                type_arguments,
                args,
                ..
            } => {
                for argument in args {
                    needed.extend(self.expression(argument)?);
                }
                let Some(&index) = self.index.get(target) else {
                    if self.solver.is_some() {
                        return Ok(needed);
                    }
                    return Err(self.error(
                        expression.span,
                        "unknown definition while inferring effects",
                    ));
                };
                for effect in self.functions[index].signature.sets.effects.clone() {
                    let effect =
                        self.instantiate(*target, effect, type_arguments, expression.span)?;
                    needed.extend(self.members(&[effect], expression.span)?);
                }
            }
            E::TraitCall {
                trait_id,
                method,
                self_type,
                method_type_arguments,
                args,
                ..
            } => {
                for argument in args {
                    needed.extend(self.expression(argument)?);
                }
                let mut owners = vec![*trait_id];
                owners.extend(
                    self.registry
                        .supertraits(*trait_id)
                        .map_err(|error| self.error(expression.span, error.message))?,
                );
                for owner in owners {
                    let declaration = self
                        .registry
                        .trait_data(owner)
                        .map_err(|error| self.error(expression.span, error.message))?;
                    if let Some(method) = declaration
                        .methods
                        .iter()
                        .find(|candidate| candidate.name == *method)
                    {
                        let Type::Fn(signature) = self.catalog.core.ty(method.signature).clone()
                        else {
                            return Err(self
                                .error(expression.span, "trait method has no function signature"));
                        };
                        if let Some(sets) = signature.sets {
                            let arguments = [*self_type]
                                .into_iter()
                                .chain(method_type_arguments.iter().copied())
                                .collect::<Vec<_>>();
                            let ty = self.catalog.effects.substitute_type(
                                &mut self.catalog.core,
                                sets.effects,
                                &arguments,
                            );
                            needed.extend(self.members(&[ty], expression.span)?);
                        }
                        break;
                    }
                }
            }
            E::Tuple(values)
            | E::Concat(values)
            | E::Make { args: values, .. }
            | E::Builtin { args: values, .. }
            | E::Fiber { args: values, .. } => {
                for value in values {
                    needed.extend(self.expression(value)?);
                }
            }
            E::List { elements, tail } => {
                for value in elements {
                    needed.extend(self.expression(value)?);
                }
                if let Some(tail) = tail {
                    needed.extend(self.expression(tail)?);
                }
            }
            E::Fail(value)
            | E::Coerce(value)
            | E::Field { base: value, .. }
            | E::Tag(value)
            | E::Assert(value)
            | E::Unary { value, .. } => needed.extend(self.expression(value)?),
            E::Binary { left, right, .. } => {
                needed.extend(self.expression(left)?);
                needed.extend(self.expression(right)?);
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                needed.extend(self.expression(condition)?);
                needed.extend(self.block(then)?);
                needed.extend(self.block(else_)?);
            }
            E::Block(body) => needed.extend(self.block(body)?),
            E::Match { value, arms } | E::Escape { value, arms, .. } => {
                needed.extend(self.expression(value)?);
                for arm in arms {
                    needed.extend(self.pattern(&arm.pattern)?);
                    if let Some(guard) = &arm.guard {
                        needed.extend(self.expression(guard)?);
                    }
                    needed.extend(self.expression(&arm.value)?);
                }
            }
        }
        Ok(needed)
    }
}
