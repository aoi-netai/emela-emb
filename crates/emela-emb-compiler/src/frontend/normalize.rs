//! Freeze solved inference variables before monomorphization and backend lowering.
use super::{
    data::{backend_type, frontend_type},
    infer::Solver,
    ir,
    types::ValueType,
};
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{Definitions, EffectId, Type, TypeId},
};
use std::collections::{HashMap, HashSet};
pub(crate) fn normalize(
    program: &mut ir::Program,
    solver: &mut Solver,
    parameters: &[Vec<TypeId>],
) -> Result<(), Vec<Diagnostic>> {
    solver
        .finish_defaults(&mut program.core.types)
        .and_then(|()| solver.finish_unused(&mut program.core.types))
        .map_err(|e| {
            vec![Diagnostic::new(
                DiagnosticKind::Type,
                Span::new(0, 0),
                e.message,
            )]
        })?;
    let mut errors = Vec::new();
    for function in &mut program.functions {
        // A recursive component shares inference variables, but each member only
        // quantifies its own signature. A variable quantified by a sibling and
        // unused in this member's signature has no required runtime representation.
        let mut substitutions: HashMap<_, _> = solver
            .quantified_parameters()
            .map(|parameter| (parameter, TypeId::UNIT))
            .collect();
        for (index, &parameter) in parameters[function.id.0].iter().enumerate() {
            substitutions.insert(parameter, program.core.types.intern(Type::Param(index)));
        }
        let mut context = Context {
            solver,
            core: &mut program.core,
            effect_ids: HashMap::new(),
            effect_types: HashMap::new(),
            source: &function.source_name,
            parameters: function.type_parameters,
            substitutions,
            schemes: parameters,
        };
        let result = (|| {
            for parameter in &mut function.signature.params {
                context.ty(parameter, function.span)?;
            }
            for error in function
                .signature
                .sets
                .errors
                .iter_mut()
                .chain(&mut function.signature.sets.error_constraints)
                .chain(&mut function.signature.sets.effects)
            {
                let mut ty = frontend_type(*error);
                context.ty(&mut ty, function.span)?;
                *error = backend_type(ty);
            }
            if let Some(ret) = &mut function.signature.ret {
                context.ty(ret, function.span)?;
            }
            for parameter in &mut function.params {
                context.ty(&mut parameter.ty, function.span)?;
            }
            if let Some(body) = &mut function.body {
                context.block(body)?;
            }
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
struct Context<'a> {
    solver: &'a mut Solver,
    core: &'a mut Definitions,
    effect_ids: HashMap<EffectId, EffectId>,
    effect_types: HashMap<TypeId, TypeId>,
    source: &'a str,
    parameters: usize,
    substitutions: HashMap<TypeId, TypeId>,
    schemes: &'a [Vec<TypeId>],
}
impl Context<'_> {
    fn ty(&mut self, ty: &mut ValueType, span: Span) -> Result<(), Diagnostic> {
        let resolved = self
            .solver
            .substitute(&mut self.core.types, backend_type(*ty), &self.substitutions)
            .map_err(|e| {
                Diagnostic::new(DiagnosticKind::Type, span, e.message).in_source(self.source)
            })?;
        let resolved = self.effect_type(resolved, span)?;
        self.validate(resolved, &mut HashSet::new(), span)?;
        *ty = frontend_type(resolved);
        Ok(())
    }
    fn validate(
        &self,
        ty: TypeId,
        seen: &mut HashSet<TypeId>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if !seen.insert(ty) {
            return Ok(());
        }
        let children = match &self.core.types[ty] {
            Type::Param(index) if *index >= self.parameters => {
                return Err(Diagnostic::new(
                    DiagnosticKind::Type,
                    span,
                    "type cannot be inferred; add an explicit signature",
                )
                .in_source(self.source));
            }
            Type::Tuple(fields) | Type::Enum(_, fields) => fields.clone(),
            Type::List(element) | Type::Cell(element) => vec![*element],
            Type::Fn(function) => function
                .params
                .iter()
                .copied()
                .chain([function.ret])
                .chain(
                    function
                        .sets
                        .into_iter()
                        .flat_map(|sets| [sets.errors, sets.effects]),
                )
                .collect(),
            Type::Set(summary) => summary.params.clone(),
            Type::Handler(effect) => self.core.effects[*effect].args.clone(),
            _ => Vec::new(),
        };
        for child in children {
            self.validate(child, seen, span)?;
        }
        Ok(())
    }
    fn block(&mut self, block: &mut ir::Block) -> Result<(), Diagnostic> {
        self.ty(&mut block.ty, block.span)?;
        for statement in &mut block.statements {
            match statement {
                ir::Statement::Pattern { pattern, value } => {
                    self.expression(value)?;
                    self.pattern(pattern)?;
                }
                ir::Statement::Expr(value) => self.expression(value)?,
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &mut ir::Expr) -> Result<(), Diagnostic> {
        self.ty(&mut expression.ty, expression.span)?;
        use ir::ExprKind as E;
        match &mut expression.kind {
            E::Use { effect } => {
                *effect = self.effect(*effect, expression.span)?;
            }
            E::OpCall {
                handler,
                effect,
                args,
                ..
            } => {
                *effect = self.effect(*effect, expression.span)?;
                self.expression(handler)?;
                for argument in args {
                    self.expression(argument)?;
                }
            }
            E::Handler {
                effect, operations, ..
            } => {
                *effect = self.effect(*effect, expression.span)?;
                for operation in operations {
                    self.expression(operation)?;
                }
            }
            E::Provide { bindings, body } => {
                for (effect, value) in bindings {
                    *effect = self.effect(*effect, expression.span)?;
                    self.expression(value)?;
                }
                self.block(body)?;
            }
            E::With { bindings, body } => {
                for binding in bindings {
                    self.ty(&mut binding.local.ty, expression.span)?;
                    for effect in &mut binding.effects {
                        *effect = self.effect(*effect, expression.span)?;
                    }
                    self.expression(&mut binding.value)?;
                    if let Some(cleanup) = &mut binding.cleanup {
                        self.expression(cleanup)?;
                    }
                }
                self.block(body)?;
            }
            E::Literal(_) | E::Local(_) => {}
            E::FnRef { type_arguments, .. } => {
                for argument in type_arguments {
                    let mut ty = frontend_type(*argument);
                    self.ty(&mut ty, expression.span)?;
                    *argument = backend_type(ty);
                }
            }
            E::Lambda { params, body, .. } => {
                for parameter in params {
                    self.ty(&mut parameter.ty, expression.span)?;
                }
                self.block(body)?;
            }
            E::Apply { callee, args, .. } => {
                self.expression(callee)?;
                for argument in args {
                    self.expression(argument)?;
                }
            }
            E::List { elements, tail } => {
                for value in elements {
                    self.expression(value)?;
                }
                if let Some(tail) = tail {
                    self.expression(tail)?;
                }
            }
            E::Concat(values)
            | E::Tuple(values)
            | E::Make { args: values, .. }
            | E::Builtin { args: values, .. } => {
                for value in values {
                    self.expression(value)?;
                }
            }
            E::Fail(value)
            | E::Unary { value, .. }
            | E::Coerce(value)
            | E::Assert(value)
            | E::Field { base: value, .. }
            | E::Tag(value) => self.expression(value)?,
            E::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)?;
            }
            E::TraitCall {
                self_type,
                method_type_arguments,
                args,
                ..
            } => {
                let mut ty = frontend_type(*self_type);
                self.ty(&mut ty, expression.span)?;
                *self_type = backend_type(ty);
                for argument in method_type_arguments {
                    let mut ty = frontend_type(*argument);
                    self.ty(&mut ty, expression.span)?;
                    *argument = backend_type(ty);
                }
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::Call {
                args,
                type_arguments,
                target,
                ..
            } => {
                // Calls within an unfinished recursive component share its inferred
                // variables until the entire component has been generalized.
                type_arguments.extend_from_slice(&self.schemes[target.0][type_arguments.len()..]);
                for arg in args {
                    self.expression(arg)?;
                }
                for argument in type_arguments {
                    let mut ty = frontend_type(*argument);
                    self.ty(&mut ty, expression.span)?;
                    *argument = backend_type(ty);
                }
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                self.expression(condition)?;
                self.block(then)?;
                self.block(else_)?;
            }
            E::Block(block) => self.block(block)?,
            E::Escape { value, error, arms } => {
                self.expression(value)?;
                self.ty(&mut error.ty, expression.span)?;
                for arm in arms {
                    self.pattern(&mut arm.pattern)?;
                    if let Some(guard) = &mut arm.guard {
                        self.expression(guard)?;
                    }
                    self.expression(&mut arm.value)?;
                }
            }
            E::Match { value, arms } => {
                self.expression(value)?;
                for arm in arms {
                    self.pattern(&mut arm.pattern)?;
                    if let Some(guard) = &mut arm.guard {
                        self.expression(guard)?;
                    }
                    self.expression(&mut arm.value)?;
                }
            }
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &mut ir::Pattern) -> Result<(), Diagnostic> {
        self.ty(&mut pattern.ty, pattern.span)?;
        match &mut pattern.kind {
            ir::PatternKind::Constant {
                value,
                coverage,
                comparison,
            } => {
                self.expression(value)?;
                if let Some(coverage) = coverage {
                    self.pattern(coverage)?;
                }
                if let Some(ir::ConstantEquality::Structural(raw_type)) = comparison {
                    let mut ty = frontend_type(*raw_type);
                    self.ty(&mut ty, pattern.span)?;
                    *raw_type = backend_type(ty);
                }
            }

            ir::PatternKind::Bind(local) => self.ty(&mut local.ty, pattern.span)?,
            ir::PatternKind::ListCons { head, tail } => {
                self.pattern(head)?;
                self.pattern(tail)?;
            }
            ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(field)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[path = "normalize/effects.rs"]
mod effects;
