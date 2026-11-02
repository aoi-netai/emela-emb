//! Trusted concurrency declarations expand after their source type/row checks.
use super::*;
use crate::sources::Intrinsic;

impl Specializer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fiber_expression(
        &mut self,
        caller: &ir::Function,
        runtime: &ir::Function,
        intrinsic: Intrinsic,
        args: Vec<ir::Expr>,
        order: &[usize],
        result: ValueType,
        span: Span,
    ) -> Result<ir::Expr> {
        let mut statements = Vec::new();
        let mut locals = Vec::new();
        for argument in args {
            let local = self.state_local(caller, "fiber_argument", argument.ty, span)?;
            statements.push(bind(local.clone(), argument, span));
            locals.push(local);
        }
        let mut args = order
            .iter()
            .map(|&index| local(&locals[index], span))
            .collect::<Vec<_>>();
        let errors = if matches!(intrinsic, Intrinsic::CauseRaise | Intrinsic::FiberScope) {
            flatten_errors(&self.core, &runtime.signature.sets.errors)
        } else {
            vec![]
        };
        if intrinsic == Intrinsic::FiberScope {
            if super::super::concurrency::contains_handle(&self.core, result) {
                return Err(diagnostic(
                    caller,
                    span,
                    "a fiber handle or Deferred cannot escape its scope result",
                ));
            }
            let value = self.scope_value(caller, &locals[order[0]], errors, span)?;
            statements.push(ir::Statement::Expr(value));
            return Ok(sequence(statements, result, span));
        }
        let op = match intrinsic {
            Intrinsic::FiberJoin => b::FiberOp::Join,
            Intrinsic::FiberAwait => b::FiberOp::Await {
                exit: self.enum_id(caller, result, span)?,
            },
            Intrinsic::FiberInterrupt => b::FiberOp::Interrupt,
            Intrinsic::FiberAttempt => {
                let callback = locals
                    .get(order[0])
                    .ok_or_else(|| diagnostic(caller, span, "attempt expects one callback"))?;
                let signature = self.callable(caller, callback.ty, span)?;
                let suspends = callable_suspends(&self.core, &signature);
                let applied = apply(callback, vec![], frontend_type(signature.ret), span);
                let wrapper = self.lambda(
                    vec![],
                    vec![callback.id],
                    applied,
                    signature.ret,
                    signature.fails,
                    suspends,
                );
                args = vec![wrapper];
                b::FiberOp::Attempt {
                    exit: self.enum_id(caller, result, span)?,
                    suspends,
                }
            }
            Intrinsic::DeferredComplete => b::FiberOp::Complete,
            Intrinsic::DeferredAwait => b::FiberOp::DeferredAwait,
            Intrinsic::DeferredPoll => b::FiberOp::Poll,
            Intrinsic::CauseKind => b::FiberOp::CauseKind {
                kind: self.enum_id(caller, result, span)?,
            },
            Intrinsic::CauseToString => b::FiberOp::CauseToString,
            Intrinsic::CauseRaise => b::FiberOp::CauseRaise,
            _ => unreachable!("only trusted concurrency declarations select this path"),
        };
        let value = ir::Expr {
            kind: ir::ExprKind::Fiber { op, args, errors },
            ty: if intrinsic == Intrinsic::CauseRaise {
                ValueType::Never
            } else {
                result
            },
            span,
        };
        statements.push(ir::Statement::Expr(value));
        Ok(ir::Expr {
            kind: ir::ExprKind::Block(ir::Block {
                statements,
                ty: result,
                span,
            }),
            ty: result,
            span,
        })
    }
    fn enum_id(&self, caller: &ir::Function, ty: ValueType, span: Span) -> Result<b::EnumId> {
        match self.core.ty(backend_type(ty)) {
            b::Type::Enum(id, _) => Ok(*id),
            _ => Err(diagnostic(
                caller,
                span,
                "a concurrency result must retain its nominal identity",
            )),
        }
    }
    fn callable(&self, caller: &ir::Function, ty: ValueType, span: Span) -> Result<b::FnType> {
        match self.core.ty(backend_type(ty)) {
            b::Type::Fn(signature) => {
                let mut signature = signature.clone();
                if let Some(sets) = signature.sets {
                    signature.fails = !flatten_errors(&self.core, &[sets.errors]).is_empty();
                }
                Ok(signature)
            }
            _ => Err(diagnostic(
                caller,
                span,
                "a concurrency callback must be a function",
            )),
        }
    }
    fn lambda(
        &mut self,
        params: Vec<ir::Local>,
        captures: Vec<ir::LocalId>,
        value: ir::Expr,
        ret: b::TypeId,
        fails: bool,
        suspends: bool,
    ) -> ir::Expr {
        let span = value.span;
        let ty = frontend_type(self.core.types.intern(b::Type::Fn(b::FnType {
            params: params.iter().map(|param| backend_type(param.ty)).collect(),
            ret,
            fails,
            suspends,
            ev: false,
            sets: None,
        })));
        ir::Expr {
            kind: ir::ExprKind::Lambda {
                params,
                captures,
                body: ir::Block {
                    ty: value.ty,
                    statements: vec![ir::Statement::Expr(value)],
                    span,
                },
            },
            ty,
            span,
        }
    }
}
fn flatten_errors(core: &b::Definitions, errors: &[b::TypeId]) -> Vec<b::TypeId> {
    let mut pending = errors.to_vec();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match core.ty(ty) {
            b::Type::Set(row) => pending.extend(&row.params),
            _ => result.push(ty),
        }
    }
    result.sort();
    result
}
fn callable_suspends(core: &b::Definitions, signature: &b::FnType) -> bool {
    signature.suspends
        || signature
            .sets
            .is_some_and(|sets| super::super::function_abi::effects_abi(core, &[sets.effects]).1)
}
fn bind(binding: ir::Local, value: ir::Expr, span: Span) -> ir::Statement {
    ir::Statement::Pattern {
        pattern: ir::Pattern {
            ty: binding.ty,
            kind: ir::PatternKind::Bind(binding),
            span,
        },
        value,
    }
}
fn local(binding: &ir::Local, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Local(binding.id),
        ty: binding.ty,
        span,
    }
}
fn apply(callback: &ir::Local, args: Vec<ir::Expr>, ty: ValueType, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Apply {
            callee: Box::new(local(callback, span)),
            order: (0..args.len()).collect(),
            args,
        },
        ty,
        span,
    }
}

fn sequence(statements: Vec<ir::Statement>, ty: ValueType, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Block(ir::Block {
            statements,
            ty,
            span,
        }),
        ty,
        span,
    }
}
fn single(value: ir::Expr) -> ir::Block {
    ir::Block {
        ty: value.ty,
        span: value.span,
        statements: vec![ir::Statement::Expr(value)],
    }
}
fn fiber(
    op: b::FiberOp,
    args: Vec<ir::Expr>,
    errors: Vec<b::TypeId>,
    ty: ValueType,
    span: Span,
) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Fiber { op, args, errors },
        ty,
        span,
    }
}
#[path = "fiber_scope.rs"]
mod scope;
