//! Freeze solved inference variables before monomorphization and backend lowering.
use super::{
    data::{backend_type, frontend_type},
    infer::Solver,
    ir,
    types::ValueType,
};
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{Type, TypeId, TypeTable},
};
use std::collections::HashSet;
pub(crate) fn normalize(
    program: &mut ir::Program,
    solver: &mut Solver,
) -> Result<(), Vec<Diagnostic>> {
    solver
        .finish_defaults(&mut program.core.types)
        .map_err(|e| {
            vec![Diagnostic::new(
                DiagnosticKind::Type,
                Span::new(0, 0),
                e.message,
            )]
        })?;
    let mut errors = Vec::new();
    for function in &mut program.functions {
        let mut context = Context {
            solver,
            types: &mut program.core.types,
            source: &function.source_name,
            parameters: function.type_parameters,
        };
        let result = (|| {
            for parameter in &mut function.signature.params {
                context.ty(parameter, function.span)?;
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
    types: &'a mut TypeTable,
    source: &'a str,
    parameters: usize,
}
impl Context<'_> {
    fn ty(&mut self, ty: &mut ValueType, span: Span) -> Result<(), Diagnostic> {
        let resolved = self
            .solver
            .resolve(self.types, backend_type(*ty))
            .map_err(|e| {
                Diagnostic::new(DiagnosticKind::Type, span, e.message).in_source(self.source)
            })?;
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
        let children = match &self.types[ty] {
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
                .collect(),
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
            E::Literal(_) | E::Local(_) => {}
            E::Concat(values) | E::Tuple(values) | E::Make { args: values, .. } => {
                for value in values {
                    self.expression(value)?;
                }
            }
            E::Unary { value, .. } | E::Assert(value) | E::Field { base: value, .. } => {
                self.expression(value)?
            }
            E::Binary { left, right, .. } => {
                self.expression(left)?;
                self.expression(right)?;
            }
            E::Call {
                args,
                type_arguments,
                ..
            } => {
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
            ir::PatternKind::Bind(local) => self.ty(&mut local.ty, pattern.span)?,
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
