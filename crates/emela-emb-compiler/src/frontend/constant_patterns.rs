//! Pure syntax coverage for constant patterns, independent of runtime getter evaluation.
use super::{ast::Unary, ir};
use std::collections::HashSet;

pub(crate) fn coverage(
    value: &ir::Expr,
    functions: &[Option<ir::Function>],
) -> Option<ir::Pattern> {
    fn expand(
        value: &ir::Expr,
        functions: &[Option<ir::Function>],
        active: &mut HashSet<ir::DefinitionId>,
        depth: usize,
    ) -> Option<ir::Pattern> {
        if depth >= 128 {
            return None;
        }
        let kind = match &value.kind {
            ir::ExprKind::Literal(literal) => ir::PatternKind::Literal(literal.clone()),
            ir::ExprKind::Unary {
                op: Unary::Neg,
                value,
            } => {
                let ir::PatternKind::Literal(literal) =
                    expand(value, functions, active, depth + 1)?.kind
                else {
                    return None;
                };
                ir::PatternKind::Literal(match literal {
                    ir::Literal::Int(value) => ir::Literal::Int(value.wrapping_neg()),
                    ir::Literal::Int64(value) => ir::Literal::Int64(value.wrapping_neg()),
                    ir::Literal::Float(value) => ir::Literal::Float(-value),
                    _ => return None,
                })
            }
            ir::ExprKind::Tuple(fields) => ir::PatternKind::Tuple(
                fields
                    .iter()
                    .map(|field| expand(field, functions, active, depth + 1))
                    .collect::<Option<_>>()?,
            ),
            ir::ExprKind::Make { ctor, args, order } => {
                if order.len() != args.len()
                    || order.iter().any(|&index| index >= args.len())
                    || order.iter().copied().collect::<HashSet<_>>().len() != args.len()
                {
                    return None;
                }
                ir::PatternKind::Constructor {
                    ctor: *ctor,
                    fields: order
                        .iter()
                        .map(|&index| expand(&args[index], functions, active, depth + 1))
                        .collect::<Option<_>>()?,
                }
            }
            ir::ExprKind::Call {
                target,
                args,
                type_arguments,
                ..
            } if args.is_empty() && type_arguments.is_empty() => {
                let function = functions
                    .iter()
                    .flatten()
                    .find(|function| function.id == *target)?;
                if !function.constant || function.type_parameters != 0 || !active.insert(*target) {
                    return None;
                }
                let result = function
                    .body
                    .as_ref()
                    .and_then(|body| one_expression(body))
                    .and_then(|body| expand(body, functions, active, depth + 1));
                active.remove(target);
                let mut result = result?;
                result.ty = value.ty;
                result.span = value.span;
                return Some(result);
            }
            ir::ExprKind::Block(body) => {
                return one_expression(body)
                    .and_then(|body| expand(body, functions, active, depth + 1));
            }
            _ => return None,
        };
        Some(ir::Pattern {
            kind,
            ty: value.ty,
            span: value.span,
        })
    }
    fn one_expression(block: &ir::Block) -> Option<&ir::Expr> {
        match block.statements.as_slice() {
            [ir::Statement::Expr(value)] => Some(value),
            _ => None,
        }
    }
    expand(value, functions, &mut HashSet::new(), 0)
}
