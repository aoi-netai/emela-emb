//! Monomorphize resolved, source-aware function templates before backend normalization.
use super::{
    data::{backend_type, frontend_type},
    ir,
    types::ValueType,
};
use crate::{Diagnostic, DiagnosticKind, Span, contract::ir as b};
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, Vec<Diagnostic>>;
#[derive(Clone, Copy)]
struct Limits {
    instances: usize,
    depth: usize,
    type_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            instances: 4096,
            depth: 128,
            type_depth: 256,
        }
    }
}
fn diagnostic(function: &ir::Function, span: Span, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::new(DiagnosticKind::Type, span, message).in_source(&function.source_name)]
}

pub(crate) fn specialize(input: ir::Program) -> Result<ir::Program> {
    specialize_with_limits(input, Limits::default())
}
fn specialize_with_limits(input: ir::Program, limits: Limits) -> Result<ir::Program> {
    let mut templates = HashMap::new();
    let mut functions = Vec::new();
    let mut maximum = None;
    for function in input.functions {
        if templates.contains_key(&function.id) {
            return Err(diagnostic(
                &function,
                function.span,
                "duplicate definition ID before specialization",
            ));
        }
        maximum =
            Some(maximum.map_or(function.id.0, |previous: usize| previous.max(function.id.0)));
        if function.type_parameters == 0 {
            functions.push(function.clone());
        } else if function.exported {
            return Err(diagnostic(
                &function,
                function.span,
                "a generic C export requires a concrete wrapper function",
            ));
        }
        templates.insert(function.id, function);
    }
    let next_id = maximum.map_or(Some(0), |maximum| maximum.checked_add(1));
    let mut context = Specializer {
        core: input.core,
        registry: input.registry,
        templates,
        functions,
        cache: HashMap::new(),
        builtin_helpers: HashMap::new(),
        next_id,
        instances: 0,
        limits,
    };
    let roots = context.functions.len();
    for index in 0..roots {
        let mut function = context.functions[index].clone();
        context.function(&mut function, &[], 0)?;
        context.functions[index] = function;
    }
    Ok(ir::Program {
        core: context.core,
        registry: context.registry,
        functions: context.functions,
    })
}
struct Specializer {
    core: b::Definitions,
    registry: super::traits::Registry,
    templates: HashMap<ir::DefinitionId, ir::Function>,
    functions: Vec<ir::Function>,
    cache: HashMap<(ir::DefinitionId, Vec<b::TypeId>), ir::DefinitionId>,
    builtin_helpers: HashMap<(super::traits::BuiltinTrait, b::TypeId, bool), ir::DefinitionId>,
    next_id: Option<usize>,
    instances: usize,
    limits: Limits,
}
impl Specializer {
    fn instantiate(
        &mut self,
        caller: &ir::Function,
        target: ir::DefinitionId,
        args: Vec<b::TypeId>,
        span: Span,
        depth: usize,
    ) -> Result<ir::DefinitionId> {
        let template = self.templates.get(&target).cloned().ok_or_else(|| {
            diagnostic(
                caller,
                span,
                "call refers to an unknown definition before specialization",
            )
        })?;
        if args.len() != template.type_parameters {
            return Err(diagnostic(
                caller,
                span,
                format!(
                    "generic call supplies {} type arguments but `{}` requires {}",
                    args.len(),
                    template.name,
                    template.type_parameters
                ),
            ));
        }
        if template.type_parameters == 0 {
            return Ok(target);
        }
        let key = (target, args.clone());
        if let Some(&id) = self.cache.get(&key) {
            return Ok(id);
        }
        if self.instances >= self.limits.instances {
            return Err(diagnostic(
                caller,
                span,
                format!(
                    "function specialization limit ({}) exceeded",
                    self.limits.instances
                ),
            ));
        }
        if depth >= self.limits.depth {
            return Err(diagnostic(
                caller,
                span,
                format!(
                    "function specialization depth limit ({}) exceeded; recursive type arguments may be growing",
                    self.limits.depth
                ),
            ));
        }
        let id = ir::DefinitionId(
            self.next_id
                .ok_or_else(|| diagnostic(caller, span, "definition ID namespace is exhausted"))?,
        );
        self.next_id = id.0.checked_add(1);
        self.instances += 1;
        let mut function = template;
        function.id = id;
        function.type_parameters = 0;
        function.exported = false;
        function.export_symbol = None;
        function.name = format!("{}$specialized{}", function.name, id.0);
        // Reserve the identity before visiting the body so ordinary self and mutual
        // recursion reuse the same concrete instance instead of recursing forever.
        self.cache.insert(key, id);
        let slot = self.functions.len();
        self.functions.push(function.clone());
        self.function(&mut function, &args, depth + 1)?;
        self.functions[slot] = function;
        Ok(id)
    }
    fn ty(
        &mut self,
        function: &ir::Function,
        ty: ValueType,
        args: &[b::TypeId],
        span: Span,
    ) -> Result<ValueType> {
        Ok(frontend_type(self.substitute(
            function,
            backend_type(ty),
            args,
            span,
            &mut HashMap::new(),
            &mut HashSet::new(),
            0,
        )?))
    }
    #[allow(clippy::too_many_arguments)]
    fn substitute(
        &mut self,
        function: &ir::Function,
        ty: b::TypeId,
        args: &[b::TypeId],
        span: Span,
        memo: &mut HashMap<b::TypeId, b::TypeId>,
        active: &mut HashSet<b::TypeId>,
        depth: usize,
    ) -> Result<b::TypeId> {
        if let Some(&mapped) = memo.get(&ty) {
            return Ok(mapped);
        }
        if depth >= self.limits.type_depth {
            return Err(diagnostic(
                function,
                span,
                "concrete type nesting exceeds the specialization limit",
            ));
        }
        if !active.insert(ty) {
            return Err(diagnostic(
                function,
                span,
                "cyclic type catalog before specialization",
            ));
        }
        let node = self
            .core
            .types
            .types
            .get(ty.index())
            .cloned()
            .ok_or_else(|| {
                diagnostic(
                    function,
                    span,
                    "type ID is outside the catalog before specialization",
                )
            })?;
        let mapped = match node {
            b::Type::Param(index) => {
                let concrete = args.get(index).copied().ok_or_else(|| {
                    diagnostic(
                        function,
                        span,
                        format!("unresolved type parameter {index} before backend lowering"),
                    )
                })?;
                // Incoming instance arguments have already been normalized in the caller.
                concrete
            }
            b::Type::Tuple(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|field| {
                        self.substitute(function, field, args, span, memo, active, depth + 1)
                    })
                    .collect::<Result<_>>()?;
                self.core.types.intern(b::Type::Tuple(fields))
            }
            b::Type::Enum(id, arguments) => {
                let arguments = arguments
                    .into_iter()
                    .map(|arg| self.substitute(function, arg, args, span, memo, active, depth + 1))
                    .collect::<Result<_>>()?;
                self.core.types.intern(b::Type::Enum(id, arguments))
            }
            b::Type::List(element) => {
                let element =
                    self.substitute(function, element, args, span, memo, active, depth + 1)?;
                self.core.types.intern(b::Type::List(element))
            }
            b::Type::Cell(element) => {
                let element =
                    self.substitute(function, element, args, span, memo, active, depth + 1)?;
                self.core.types.intern(b::Type::Cell(element))
            }
            b::Type::Fn(mut signature) => {
                signature.params = signature
                    .params
                    .into_iter()
                    .map(|param| {
                        self.substitute(function, param, args, span, memo, active, depth + 1)
                    })
                    .collect::<Result<_>>()?;
                signature.ret =
                    self.substitute(function, signature.ret, args, span, memo, active, depth + 1)?;
                if let Some(sets) = &mut signature.sets {
                    sets.errors = self.substitute(
                        function,
                        sets.errors,
                        args,
                        span,
                        memo,
                        active,
                        depth + 1,
                    )?;
                    sets.effects = self.substitute(
                        function,
                        sets.effects,
                        args,
                        span,
                        memo,
                        active,
                        depth + 1,
                    )?;
                }
                self.core.types.intern(b::Type::Fn(signature))
            }
            b::Type::Set(mut summary) => {
                summary.params = summary
                    .params
                    .into_iter()
                    .map(|param| {
                        self.substitute(function, param, args, span, memo, active, depth + 1)
                    })
                    .collect::<Result<_>>()?;
                self.core.types.intern(b::Type::Set(summary))
            }
            _ => ty,
        };
        active.remove(&ty);
        memo.insert(ty, mapped);
        Ok(mapped)
    }
    fn function(
        &mut self,
        function: &mut ir::Function,
        args: &[b::TypeId],
        depth: usize,
    ) -> Result<()> {
        let source = function.clone();
        for ty in &mut function.signature.params {
            *ty = self.ty(&source, *ty, args, source.span)?;
        }
        let ret = function.signature.ret.ok_or_else(|| {
            diagnostic(
                &source,
                source.span,
                "function return type is unresolved before specialization",
            )
        })?;
        function.signature.ret = Some(self.ty(&source, ret, args, source.span)?);
        for error_type in function
            .signature
            .sets
            .errors
            .iter_mut()
            .chain(&mut function.signature.sets.error_constraints)
        {
            *error_type = self.substitute(
                &source,
                *error_type,
                args,
                source.span,
                &mut HashMap::new(),
                &mut HashSet::new(),
                0,
            )?;
            let valid = match self.core.types.types.get(error_type.index()) {
                Some(b::Type::Enum(id, _)) => self
                    .core
                    .enums
                    .iter()
                    .any(|(key, definition)| key == *id && definition.error),
                _ => false,
            };
            if !valid {
                return Err(diagnostic(
                    &source,
                    source.span,
                    "failure type parameter must be a concrete error type",
                ));
            }
        }
        function.signature.sets.errors.sort_by_key(|ty| ty.index());
        function.signature.sets.errors.dedup();
        function
            .signature
            .sets
            .error_constraints
            .sort_by_key(|ty| ty.index());
        function.signature.sets.error_constraints.dedup();
        for local in &mut function.params {
            local.ty = self.ty(&source, local.ty, args, source.span)?;
        }
        if let Some(kind) = function.derivation.take() {
            function.body = Some(self.derive_body(function, kind, false)?);
        }
        if let Some(body) = &mut function.body {
            self.block(&source, body, args, depth)?;
        }
        Ok(())
    }
    fn block(
        &mut self,
        function: &ir::Function,
        block: &mut ir::Block,
        args: &[b::TypeId],
        depth: usize,
    ) -> Result<()> {
        block.ty = self.ty(function, block.ty, args, block.span)?;
        for statement in &mut block.statements {
            match statement {
                ir::Statement::Pattern { pattern, value } => {
                    self.pattern(function, pattern, args, depth)?;
                    self.expression(function, value, args, depth)?;
                }
                ir::Statement::Expr(expression) => {
                    self.expression(function, expression, args, depth)?
                }
            }
        }
        Ok(())
    }
    fn pattern(
        &mut self,
        function: &ir::Function,
        pattern: &mut ir::Pattern,
        args: &[b::TypeId],
        depth: usize,
    ) -> Result<()> {
        pattern.ty = self.ty(function, pattern.ty, args, pattern.span)?;
        match &mut pattern.kind {
            ir::PatternKind::Constant {
                value,
                coverage,
                comparison,
            } => {
                self.expression(function, value, args, depth)?;
                if let Some(coverage) = coverage {
                    self.pattern(function, coverage, args, depth)?;
                }
                *comparison = Some(self.constant_equality(
                    function,
                    backend_type(pattern.ty),
                    pattern.span,
                    0,
                )?);
            }

            ir::PatternKind::Bind(local) => {
                local.ty = self.ty(function, local.ty, args, pattern.span)?;
            }
            ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(function, field, args, depth)?;
                }
            }
            ir::PatternKind::Any | ir::PatternKind::Literal(_) => {}
        }
        Ok(())
    }
    fn order(function: &ir::Function, span: Span, order: &[usize], count: usize) -> Result<()> {
        if order.len() != count
            || order.iter().any(|&index| index >= count)
            || order.iter().copied().collect::<HashSet<_>>().len() != count
        {
            return Err(diagnostic(
                function,
                span,
                "argument evaluation order is not a complete permutation",
            ));
        }
        Ok(())
    }
    fn expression(
        &mut self,
        function: &ir::Function,
        expression: &mut ir::Expr,
        arguments: &[b::TypeId],
        depth: usize,
    ) -> Result<()> {
        expression.ty = self.ty(function, expression.ty, arguments, expression.span)?;
        match &mut expression.kind {
            ir::ExprKind::Literal(_) | ir::ExprKind::Local(_) => {}
            ir::ExprKind::TraitCall {
                trait_id,
                method,
                self_type,
                args,
                order,
                nested_show,
            } => {
                Self::order(function, expression.span, order, args.len())?;
                for argument in args.iter_mut() {
                    self.expression(function, argument, arguments, depth)?;
                }
                let concrete = self.substitute(
                    function,
                    *self_type,
                    arguments,
                    expression.span,
                    &mut HashMap::new(),
                    &mut HashSet::new(),
                    0,
                )?;
                let target = self.trait_target(
                    function,
                    concrete,
                    *trait_id,
                    method,
                    *nested_show,
                    expression.span,
                    depth,
                )?;
                expression.kind = ir::ExprKind::Call {
                    target,
                    args: std::mem::take(args),
                    order: std::mem::take(order),
                    type_arguments: vec![],
                };
            }
            ir::ExprKind::Concat(values)
            | ir::ExprKind::Tuple(values)
            | ir::ExprKind::Builtin { args: values, .. } => {
                for value in values {
                    self.expression(function, value, arguments, depth)?;
                }
            }
            ir::ExprKind::Make { args, order, .. } => {
                Self::order(function, expression.span, order, args.len())?;
                for value in args {
                    self.expression(function, value, arguments, depth)?;
                }
            }
            ir::ExprKind::Fail(base)
            | ir::ExprKind::Field { base, .. }
            | ir::ExprKind::Unary { value: base, .. }
            | ir::ExprKind::Assert(base)
            | ir::ExprKind::Tag(base) => self.expression(function, base, arguments, depth)?,
            ir::ExprKind::Binary { left, right, .. } => {
                self.expression(function, left, arguments, depth)?;
                self.expression(function, right, arguments, depth)?;
            }
            ir::ExprKind::Block(block) => self.block(function, block, arguments, depth)?,
            ir::ExprKind::If {
                condition,
                then,
                else_,
            } => {
                self.expression(function, condition, arguments, depth)?;
                self.block(function, then, arguments, depth)?;
                self.block(function, else_, arguments, depth)?;
            }
            ir::ExprKind::Escape { value, error, arms } => {
                self.expression(function, value, arguments, depth)?;
                error.ty = self.ty(function, error.ty, arguments, expression.span)?;
                for arm in arms {
                    self.pattern(function, &mut arm.pattern, arguments, depth)?;
                    if let Some(guard) = &mut arm.guard {
                        self.expression(function, guard, arguments, depth)?;
                    }
                    self.expression(function, &mut arm.value, arguments, depth)?;
                }
            }
            ir::ExprKind::Match { value, arms } => {
                self.expression(function, value, arguments, depth)?;
                for arm in arms {
                    self.pattern(function, &mut arm.pattern, arguments, depth)?;
                    if let Some(guard) = &mut arm.guard {
                        self.expression(function, guard, arguments, depth)?;
                    }
                    self.expression(function, &mut arm.value, arguments, depth)?;
                }
            }
            ir::ExprKind::Call {
                target,
                args,
                order,
                type_arguments,
            } => {
                Self::order(function, expression.span, order, args.len())?;
                for value in args {
                    self.expression(function, value, arguments, depth)?;
                }
                let concrete = type_arguments
                    .iter()
                    .map(|&ty| {
                        self.substitute(
                            function,
                            ty,
                            arguments,
                            expression.span,
                            &mut HashMap::new(),
                            &mut HashSet::new(),
                            0,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                *target = self.instantiate(function, *target, concrete, expression.span, depth)?;
                type_arguments.clear();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "specialize/tests.rs"]
mod tests;

#[path = "specialize/derive.rs"]
mod derive;

#[path = "specialize/constant_patterns.rs"]
mod constant_patterns;
