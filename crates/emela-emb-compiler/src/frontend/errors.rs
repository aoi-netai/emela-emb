//! Recoverable failure inference is separate from value typing and backend control lowering.
use super::{
    data::{Catalog, backend_type},
    ir, patterns,
};
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{Type, TypeId},
};
use std::collections::{HashMap, HashSet};
#[derive(Clone)]
pub(crate) struct Mode {
    pub allowed: Option<Vec<TypeId>>,
}
#[derive(Default)]
struct Analysis {
    errors: HashSet<TypeId>,
    constraints: HashSet<TypeId>,
}
impl Analysis {
    fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
        self.constraints.extend(other.constraints);
    }
}
fn sorted(set: HashSet<TypeId>) -> Vec<TypeId> {
    let mut values: Vec<_> = set.into_iter().collect();
    values.sort();
    values
}

pub(crate) fn infer(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
) -> Result<(), Vec<Diagnostic>> {
    infer_phase(program, modes, catalog, None, None, true)
}
/// Run while value metas are still live, before an SCC is generalized. This supplies
/// inferred failure payload types to escape patterns and includes error-only metas
/// in the same signature scheme as the function's value types.
pub(crate) fn constrain(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
) -> Result<(), Vec<Diagnostic>> {
    infer_phase(
        program,
        modes,
        catalog,
        Some(solver),
        Some(parameters),
        false,
    )
}
pub(crate) fn incoming(
    expression: &ir::Expr,
    functions: &[ir::Function],
    registry: &super::traits::Registry,
    catalog: &mut Catalog,
    solver: &mut super::infer::Solver,
    parameters: &[Vec<TypeId>],
    source: &str,
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
        source,
        registry,
        solver: Some(solver),
        parameters: Some(parameters),
    };
    Ok(sorted(
        context.expression(expression, &HashSet::new())?.errors,
    ))
}
fn infer_phase(
    program: &mut ir::Program,
    modes: &HashMap<ir::DefinitionId, Mode>,
    catalog: &mut Catalog,
    mut solver: Option<&mut super::infer::Solver>,
    parameters: Option<&[Vec<TypeId>]>,
    validate: bool,
) -> Result<(), Vec<Diagnostic>> {
    let index: HashMap<_, _> = program
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect();
    for round in 0..1024 {
        let mut changed = false;
        for i in 0..program.functions.len() {
            let function = program.functions[i].clone();
            let mut context = Context {
                catalog,
                functions: &program.functions,
                index: &index,
                source: &function.source_name,
                registry: &program.registry,
                solver: solver.as_deref_mut(),
                parameters,
            };
            let analysis = match &function.body {
                Some(body) => context.block(body, &HashSet::new()),
                None => Ok(Analysis::default()),
            }
            .map_err(|e| vec![e])?;
            let declared = modes.get(&function.id).and_then(|m| m.allowed.as_ref());
            let errors = declared.cloned().unwrap_or_else(|| sorted(analysis.errors));
            let mut constraints = analysis.constraints;
            constraints.extend(errors.iter().copied());
            let constraints = sorted(constraints);
            if errors != program.functions[i].signature.sets.errors
                || constraints != program.functions[i].signature.sets.error_constraints
            {
                changed = true;
                program.functions[i].signature.sets.errors = errors;
                program.functions[i].signature.sets.error_constraints = constraints;
            }
        }
        if !changed {
            break;
        }
        if round == 1023 {
            return Err(vec![Diagnostic::new(
                DiagnosticKind::Type,
                Span::new(0, 0),
                "failure inference does not converge; an expanding recursive signature needs an explicit finite annotation",
            )]);
        }
    }
    if !validate {
        program.core = catalog.core.clone();
        return Ok(());
    }
    let mut errors = Vec::new();
    for function in &program.functions {
        let mut context = Context {
            catalog,
            functions: &program.functions,
            index: &index,
            source: &function.source_name,
            registry: &program.registry,
            solver: None,
            parameters: None,
        };
        let analysis = match &function.body {
            Some(body) => context.block(body, &HashSet::new()),
            None => Ok(Analysis::default()),
        };
        match analysis {
            Err(error) => errors.push(error),
            Ok(analysis) => {
                if let Some(allowed) = modes.get(&function.id).and_then(|m| m.allowed.as_ref()) {
                    if analysis.errors.iter().any(|error| !allowed.contains(error)) {
                        errors.push(context.error(function.span,if function.constant { "const initializer must be infallible; recover or remove its failure" } else { "function has an undeclared failure; add a fails annotation or handle it with escape" }));
                    }
                }
            }
        }
    }
    // Coverage may materialize generic payload types, preserving all existing catalog IDs.
    program.core = catalog.core.clone();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
struct Context<'a> {
    catalog: &'a mut Catalog,
    functions: &'a [ir::Function],
    index: &'a HashMap<ir::DefinitionId, usize>,
    source: &'a str,
    registry: &'a super::traits::Registry,
    solver: Option<&'a mut super::infer::Solver>,
    parameters: Option<&'a [Vec<TypeId>]>,
}
impl Context<'_> {
    fn resolve(&mut self, ty: TypeId, span: Span) -> Result<TypeId, Diagnostic> {
        if let Some(solver) = self.solver.as_deref_mut() {
            solver
                .resolve(&mut self.catalog.core.types, ty)
                .map_err(|error| {
                    Diagnostic::new(DiagnosticKind::Type, span, error.message)
                        .in_source(self.source)
                })
        } else {
            Ok(ty)
        }
    }
    fn instantiate(
        &mut self,
        target: ir::DefinitionId,
        ty: TypeId,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<TypeId, Diagnostic> {
        if let (Some(solver), Some(parameters)) = (self.solver.as_deref_mut(), self.parameters) {
            let parameters = &parameters[target.0];
            if arguments.len() > parameters.len() {
                return Err(self.error(
                    span,
                    "call has more type arguments than its signature scheme",
                ));
            }
            // A recursive call made before the SCC closes shares its inferred metas.
            let substitutions = parameters
                .iter()
                .copied()
                .zip(
                    arguments
                        .iter()
                        .copied()
                        .chain(parameters[arguments.len()..].iter().copied()),
                )
                .collect();
            solver
                .substitute(&mut self.catalog.core.types, ty, &substitutions)
                .map_err(|error| {
                    Diagnostic::new(DiagnosticKind::Type, span, error.message)
                        .in_source(self.source)
                })
        } else {
            Ok(self.catalog.core.types.subst(ty, arguments))
        }
    }
    fn constrain_pattern(
        &mut self,
        pattern: &ir::Pattern,
        error: TypeId,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let pattern_type = self.resolve(backend_type(pattern.ty), span)?;
        let same_tag = matches!((self.catalog.core.ty(pattern_type),self.catalog.core.ty(error)),(Type::Enum(left,_),Type::Enum(right,_)) if left==right);
        if same_tag {
            if let Some(solver) = self.solver.as_deref_mut() {
                solver
                    .unify(&mut self.catalog.core.types, error, pattern_type)
                    .map_err(|error| {
                        Diagnostic::new(DiagnosticKind::Type, span, error.message)
                            .in_source(self.source)
                    })?;
            }
        }
        Ok(())
    }
    fn error(&self, span: Span, message: &str) -> Diagnostic {
        Diagnostic::new(DiagnosticKind::Type, span, message).in_source(self.source)
    }
    fn require_error(&self, ty: TypeId, span: Span) -> Result<(), Diagnostic> {
        if matches!(self.catalog.core.ty(ty), Type::Param(_))
            || matches!(self.catalog.core.ty(ty),Type::Enum(id,_) if self.catalog.core.enums[*id].error)
        {
            Ok(())
        } else {
            Err(self.error(span, "a failure parameter must denote a single error type"))
        }
    }
    fn block(
        &mut self,
        body: &ir::Block,
        dynamic: &HashSet<TypeId>,
    ) -> Result<Analysis, Diagnostic> {
        let mut analysis = Analysis::default();
        for statement in &body.statements {
            analysis.merge(self.expression(
                match statement {
                    ir::Statement::Pattern { value, .. } | ir::Statement::Expr(value) => value,
                },
                dynamic,
            )?);
        }
        Ok(analysis)
    }
    fn expression(
        &mut self,
        expression: &ir::Expr,
        dynamic: &HashSet<TypeId>,
    ) -> Result<Analysis, Diagnostic> {
        use ir::ExprKind as E;
        let mut analysis = Analysis::default();
        match &expression.kind {
            E::Literal(_) | E::Local(_) | E::FnRef { .. } | E::Lambda { .. } => {}
            E::Apply { callee, args, .. } => {
                analysis.merge(self.expression(callee, dynamic)?);
                for argument in args {
                    analysis.merge(self.expression(argument, dynamic)?);
                }
                let ty = self.resolve(backend_type(callee.ty), callee.span)?;
                if let Type::Fn(ft) = self.catalog.core.ty(ty).clone() {
                    if let Some(sets) = ft.sets {
                        if let Type::Set(errors) = self.catalog.core.ty(sets.errors).clone() {
                            for error in errors.params {
                                let error = self.resolve(error, expression.span)?;
                                self.require_error(error, expression.span)?;
                                analysis.errors.insert(error);
                                analysis.constraints.insert(error);
                            }
                        }
                    }
                }
            }
            E::Concat(args) | E::Tuple(args) | E::Make { args, .. } | E::Builtin { args, .. } => {
                for arg in args {
                    analysis.merge(self.expression(arg, dynamic)?);
                }
            }
            E::List { elements, tail } => {
                for element in elements {
                    analysis.merge(self.expression(element, dynamic)?);
                }
                if let Some(tail) = tail {
                    analysis.merge(self.expression(tail, dynamic)?);
                }
            }
            E::Unary { value, .. }
            | E::Assert(value)
            | E::Tag(value)
            | E::Field { base: value, .. } => analysis.merge(self.expression(value, dynamic)?),
            E::Binary { left, right, .. } => {
                analysis.merge(self.expression(left, dynamic)?);
                analysis.merge(self.expression(right, dynamic)?);
            }
            E::TraitCall {
                trait_id,
                method,
                self_type,
                method_type_arguments,
                args,
                ..
            } => {
                for arg in args {
                    analysis.merge(self.expression(arg, dynamic)?);
                }
                let mut owners = vec![*trait_id];
                owners.extend(
                    self.registry
                        .supertraits(*trait_id)
                        .map_err(|e| self.error(expression.span, &e.message))?,
                );
                for owner in owners {
                    let data = self
                        .registry
                        .trait_data(owner)
                        .map_err(|e| self.error(expression.span, &e.message))?;
                    if let Some(method) = data.methods.iter().find(|m| m.name == *method) {
                        let Type::Fn(signature) = self.catalog.core.ty(method.signature).clone()
                        else {
                            return Err(self
                                .error(expression.span, "trait method signature is not callable"));
                        };
                        if let Some(sets) = signature.sets {
                            if let Type::Set(summary) = self.catalog.core.ty(sets.errors).clone() {
                                for error in summary.params {
                                    let mut arguments = vec![*self_type];
                                    arguments.extend_from_slice(method_type_arguments);
                                    let error = self.catalog.core.types.subst(error, &arguments);
                                    let error = self.resolve(error, expression.span)?;
                                    self.require_error(error, expression.span)?;
                                    analysis.errors.insert(error);
                                    analysis.constraints.insert(error);
                                }
                            }
                        }
                        break;
                    }
                }
            }
            E::Call {
                target,
                args,
                type_arguments,
                ..
            } => {
                for arg in args {
                    analysis.merge(self.expression(arg, dynamic)?);
                }
                let Some(&index) = self.index.get(target) else {
                    if self.solver.is_some() {
                        // A reference into the active SCC can precede that member's body.
                        // Its errors are shared at the SCC fixed point before generalization.
                        return Ok(analysis);
                    }
                    return Err(self.error(
                        expression.span,
                        "call references an unknown definition while inferring failures",
                    ));
                };
                let signature = self.functions[index].signature.clone();
                for error in signature.sets.errors {
                    let error =
                        self.instantiate(*target, error, type_arguments, expression.span)?;
                    self.require_error(error, expression.span)?;
                    analysis.errors.insert(error);
                    analysis.constraints.insert(error);
                }
                for constraint in signature.sets.error_constraints {
                    let constraint =
                        self.instantiate(*target, constraint, type_arguments, expression.span)?;
                    self.require_error(constraint, expression.span)?;
                    analysis.constraints.insert(constraint);
                }
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                analysis.merge(self.expression(condition, dynamic)?);
                analysis.merge(self.block(then, dynamic)?);
                analysis.merge(self.block(else_, dynamic)?);
            }
            E::Block(body) => analysis.merge(self.block(body, dynamic)?),
            E::Match { value, arms } => {
                analysis.merge(self.expression(value, dynamic)?);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        analysis.merge(self.expression(guard, dynamic)?);
                    }
                    analysis.merge(self.expression(&arm.value, dynamic)?);
                }
            }
            E::Fail(value) => {
                analysis.merge(self.expression(value, dynamic)?);
                let ty = self.resolve(backend_type(value.ty), value.span)?;
                if ty == TypeId::ANY {
                    analysis.errors.extend(dynamic);
                    analysis.constraints.extend(dynamic);
                } else {
                    self.require_error(ty, value.span)?;
                    analysis.errors.insert(ty);
                    analysis.constraints.insert(ty);
                }
            }
            E::Escape { value, arms, .. } => {
                analysis.merge(self.expression(value, dynamic)?);
                let incoming = analysis.errors.clone();
                let mut handled = HashSet::new();
                for &error in &incoming {
                    let mut resolved = self.resolve(error, expression.span)?;
                    for arm in arms {
                        self.constrain_pattern(&arm.pattern, resolved, arm.pattern.span)?;
                        resolved = self.resolve(error, expression.span)?;
                    }
                    let mut covering = Vec::new();
                    for arm in arms.iter().filter(|arm| arm.guard.is_none()) {
                        let pattern_type =
                            self.resolve(backend_type(arm.pattern.ty), arm.pattern.span)?;
                        if matches!(
                            arm.pattern.kind,
                            ir::PatternKind::Any | ir::PatternKind::Bind(_)
                        ) || pattern_type == resolved
                        {
                            let mut pattern = arm.pattern.clone();
                            pattern.ty = super::data::frontend_type(resolved);
                            covering.push(pattern);
                        }
                    }
                    if patterns::exhaustive(
                        &covering,
                        super::data::frontend_type(resolved),
                        self.catalog,
                    ) {
                        handled.insert(error);
                    }
                }
                analysis.errors.retain(|error| !handled.contains(error));
                for arm in arms {
                    let narrowed = if matches!(
                        arm.pattern.kind,
                        ir::PatternKind::Any | ir::PatternKind::Bind(_)
                    ) {
                        incoming.clone()
                    } else {
                        HashSet::from([
                            self.resolve(backend_type(arm.pattern.ty), arm.pattern.span)?
                        ])
                    };
                    if let Some(guard) = &arm.guard {
                        analysis.merge(self.expression(guard, &narrowed)?);
                    }
                    analysis.merge(self.expression(&arm.value, &narrowed)?);
                }
            }
        }
        Ok(analysis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Source,
        frontend::{check, modules},
    };
    fn checked(source: &str) -> Result<ir::Program, Vec<Diagnostic>> {
        let mut modules = modules::collect(&[Source::new("main.emel", source)], "main.emel")?;
        crate::frontend::consts::expand(&mut modules)?;
        check::check(modules)
    }
    #[test]
    fn explicit_value_signatures_are_pure_but_omitted_signatures_infer_failures() {
        assert!(checked("error Oops\nfn bad() -> Int { fail Oops }").is_err());
        let program = checked(
            "error Oops\nfn bad() { fail Oops }\nfn good() -> Int { bad() escape { Oops -> 42 } }",
        )
        .unwrap();
        assert_eq!(
            program
                .functions
                .iter()
                .find(|f| f.name == "bad")
                .unwrap()
                .signature
                .sets
                .errors
                .len(),
            1
        );
        assert!(
            program
                .functions
                .iter()
                .find(|f| f.name == "good")
                .unwrap()
                .signature
                .sets
                .errors
                .is_empty()
        );
    }
    #[test]
    fn escape_removes_only_jointly_exhaustive_unguarded_error_payloads() {
        checked("error Flag(value: Bool)\nfn bad(value: Bool) -> Int fails Flag { fail Flag(value:) }\nfn good() -> Int { bad(True) escape { Flag(True) -> 1\n Flag(False) -> 2 } }").unwrap();
        for arms in [
            "Flag(True) -> 1",
            "Flag(value:) if value -> 1\n Flag(False) -> 2",
        ] {
            let source = format!(
                "error Flag(value: Bool)\nfn bad(value: Bool) -> Int fails Flag {{ fail Flag(value:) }}\nfn good() -> Int {{ bad(True) escape {{ {arms} }} }}"
            );
            assert!(checked(&source).is_err(), "{source}");
        }
    }
    #[test]
    fn generic_failure_parameters_require_an_error_even_when_fully_caught() {
        let base = "enum Option[A] { Some(A), None }\nerror Oops\nfn or_fail[A,E](value: Option[A], err: E) -> A fails E { value match { Some(value) -> value\n None -> fail err } }\n";
        checked(&format!(
            "{base}fn main() -> Int {{ or_fail(None, Oops) escape {{ Oops -> 42 }} }}"
        ))
        .unwrap();
        assert!(
            checked(&format!(
                "{base}fn main() -> Int {{ or_fail(Some(1), 0) escape {{ _ -> 42 }} }}"
            ))
            .is_err()
        );
    }
    #[test]
    fn const_initializers_remain_infallible() {
        let source = "error Oops\nfn bad() -> Int fails Oops { fail Oops }\nconst BAD=bad()\nfn main() -> Int { BAD }";
        let mut modules =
            modules::collect(&[Source::new("main.emel", source)], "main.emel").unwrap();
        crate::frontend::consts::expand(&mut modules).unwrap();
        assert!(check::check(modules).is_err());
    }
}
