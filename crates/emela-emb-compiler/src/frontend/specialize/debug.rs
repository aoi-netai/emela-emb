//! Debug display permits structural fallback without weakening ordinary Show bounds.
use super::*;
use crate::frontend::{
    traits::{BuiltinTrait, ParamBounds},
    types::{InvocationSets, Signature},
};

impl Specializer {
    pub(super) fn debug_expression(
        &mut self,
        caller: &ir::Function,
        value: ir::Expr,
        span: Span,
        depth: usize,
    ) -> Result<ir::Expr> {
        if value.ty == ValueType::Never {
            return Ok(value);
        }
        let ty = backend_type(value.ty);
        let show = self
            .registry
            .builtin_id(BuiltinTrait::Show)
            .ok_or_else(|| {
                diagnostic(caller, span, "compiler-owned Show trait is missing for dbg")
            })?;
        let display = if self
            .registry
            .satisfies(&self.core, ty, show, &ParamBounds::new())
            .map_err(|error| diagnostic(caller, span, error.message))?
        {
            self.trait_target(caller, ty, show, "show", &[], true, span, depth)?
        } else {
            self.debug_show_helper(caller, ty, true, span, depth)?
        };
        let next = if let Some(&next) = self.debug_locals.get(&caller.id) {
            next
        } else {
            maximum_local(caller)
                .map_or(Some(0), |id| id.checked_add(1))
                .ok_or_else(|| diagnostic(caller, span, "local ID namespace is exhausted"))?
        };
        self.debug_locals.insert(
            caller.id,
            next.checked_add(1)
                .ok_or_else(|| diagnostic(caller, span, "local ID namespace is exhausted"))?,
        );
        let local = ir::Local {
            id: ir::LocalId(next),
            name: "debug_value".into(),
            ty: value.ty,
        };
        let reference = ir::Expr {
            kind: ir::ExprKind::Local(local.id),
            ty: local.ty,
            span,
        };
        let display = ir::Expr {
            kind: ir::ExprKind::Call {
                target: display,
                args: vec![reference.clone()],
                order: vec![0],
                type_arguments: vec![],
            },
            ty: ValueType::String,
            span,
        };
        let print = ir::Expr {
            kind: ir::ExprKind::Builtin {
                op: b::Builtin::DebugPrint,
                args: vec![display],
            },
            ty: ValueType::Unit,
            span,
        };
        // Keep the original operand in the caller's scope. Returning an aggregate containing
        // borrowed String views from a separate formatting helper would require owning copies.
        Ok(ir::Expr {
            ty: local.ty,
            span,
            kind: ir::ExprKind::Block(ir::Block {
                ty: local.ty,
                span,
                statements: vec![
                    ir::Statement::Pattern {
                        pattern: ir::Pattern {
                            ty: local.ty,
                            span,
                            kind: ir::PatternKind::Bind(local),
                        },
                        value,
                    },
                    ir::Statement::Expr(print),
                    ir::Statement::Expr(reference),
                ],
            }),
        })
    }
    pub(super) fn debug_show_helper(
        &mut self,
        caller: &ir::Function,
        ty: b::TypeId,
        nested: bool,
        span: Span,
        depth: usize,
    ) -> Result<ir::DefinitionId> {
        let nested = nested && ty == b::TypeId::STRING;
        if let Some(&id) = self.debug_show_helpers.get(&(ty, nested)) {
            return Ok(id);
        }
        if self.instances >= self.limits.instances || depth >= self.limits.depth {
            return Err(diagnostic(
                caller,
                span,
                "debug display specialization limit exceeded",
            ));
        }
        let id = ir::DefinitionId(
            self.next_id
                .ok_or_else(|| diagnostic(caller, span, "definition ID namespace is exhausted"))?,
        );
        self.next_id = id.0.checked_add(1);
        self.instances += 1;
        let param = ir::Local {
            id: ir::LocalId(0),
            name: "value".into(),
            ty: frontend_type(ty),
        };
        let mut function = ir::Function {
            id,
            type_parameters: 0,
            constant: false,
            origin: None,
            derivation: None,
            name: format!("__debug_show_{}_{}", ty.index(), id.0),
            source_name: caller.source_name.clone(),
            span,
            exported: false,
            export_symbol: None,
            signature: Signature {
                params: vec![param.ty],
                ret: Some(ValueType::String),
                sets: InvocationSets::default(),
            },
            params: vec![param],
            external: None,
            intrinsic: None,
            body: None,
        };
        self.debug_show_helpers.insert((ty, nested), id);
        self.debug_fallback.insert(id);
        let slot = self.functions.len();
        self.functions.push(function.clone());
        function.body = Some(self.derive_body(&function, BuiltinTrait::Show, nested, depth)?);
        self.function(&mut function, &[], depth + 1)?;
        self.functions[slot] = function;
        Ok(id)
    }
}
fn maximum_local(function: &ir::Function) -> Option<usize> {
    fn record(id: ir::LocalId, maximum: &mut Option<usize>) {
        *maximum = Some(maximum.map_or(id.0, |previous| previous.max(id.0)));
    }
    fn pattern(value: &ir::Pattern, maximum: &mut Option<usize>) {
        match &value.kind {
            ir::PatternKind::Bind(local) => record(local.id, maximum),
            ir::PatternKind::ListCons { head, tail } => {
                pattern(head, maximum);
                pattern(tail, maximum);
            }
            ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
                for field in fields {
                    pattern(field, maximum);
                }
            }
            ir::PatternKind::Constant {
                value, coverage, ..
            } => {
                expression(value, maximum);
                if let Some(coverage) = coverage {
                    pattern(coverage, maximum);
                }
            }
            _ => {}
        }
    }
    fn block(value: &ir::Block, maximum: &mut Option<usize>) {
        for statement in &value.statements {
            match statement {
                ir::Statement::Pattern {
                    pattern: value_pattern,
                    value,
                } => {
                    pattern(value_pattern, maximum);
                    expression(value, maximum);
                }
                ir::Statement::Expr(value) => expression(value, maximum),
            }
        }
    }
    fn expression(value: &ir::Expr, maximum: &mut Option<usize>) {
        use ir::ExprKind as E;
        match &value.kind {
            E::Local(id) => record(*id, maximum),
            E::FnRef { .. } => {}
            E::Lambda {
                params,
                captures,
                body,
            } => {
                for parameter in params {
                    record(parameter.id, maximum);
                }
                for capture in captures {
                    record(*capture, maximum);
                }
                block(body, maximum);
            }
            E::Apply { callee, args, .. } => {
                expression(callee, maximum);
                for argument in args {
                    expression(argument, maximum);
                }
            }
            E::List { elements, tail } => {
                for value in elements {
                    expression(value, maximum);
                }
                if let Some(tail) = tail {
                    expression(tail, maximum);
                }
            }
            E::Call { args, .. }
            | E::Make { args, .. }
            | E::TraitCall { args, .. }
            | E::Builtin { args, .. }
            | E::Concat(args)
            | E::Tuple(args) => {
                for value in args {
                    expression(value, maximum);
                }
            }
            E::Field { base, .. }
            | E::Coerce(base)
            | E::Tag(base)
            | E::Fail(base)
            | E::Unary { value: base, .. }
            | E::Assert(base) => expression(base, maximum),
            E::Binary { left, right, .. } => {
                expression(left, maximum);
                expression(right, maximum);
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                expression(condition, maximum);
                block(then, maximum);
                block(else_, maximum);
            }
            E::Escape { value, error, arms } => {
                record(error.id, maximum);
                expression(value, maximum);
                for arm in arms {
                    pattern(&arm.pattern, maximum);
                    if let Some(guard) = &arm.guard {
                        expression(guard, maximum);
                    }
                    expression(&arm.value, maximum);
                }
            }
            E::Match { value, arms } => {
                expression(value, maximum);
                for arm in arms {
                    pattern(&arm.pattern, maximum);
                    if let Some(guard) = &arm.guard {
                        expression(guard, maximum);
                    }
                    expression(&arm.value, maximum);
                }
            }
            E::Block(value) => block(value, maximum),
            E::Literal(_) => {}
        }
    }
    let mut maximum = None;
    for local in &function.params {
        record(local.id, &mut maximum);
    }
    if let Some(body) = &function.body {
        block(body, &mut maximum);
    }
    maximum
}
