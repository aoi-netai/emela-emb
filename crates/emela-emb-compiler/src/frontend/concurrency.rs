//! A scope cannot return fiber/deferred handles through its visible result type.
use super::{data, ir, types::ValueType};
use crate::{Diagnostic, DiagnosticKind, contract::ir as b, sources::Intrinsic};
use std::collections::HashSet;

pub(crate) fn contains_handle(core: &b::Definitions, ty: ValueType) -> bool {
    let mut pending = vec![data::backend_type(ty)];
    let mut seen = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match core.ty(ty) {
            b::Type::Enum(id, arguments) => {
                if matches!(
                    core.enums[*id].name.as_str(),
                    "Std.Fiber.Fiber" | "Std.Deferred.Deferred"
                ) {
                    return true;
                }
                pending.extend(arguments);
            }
            b::Type::Tuple(fields) => pending.extend(fields),
            b::Type::List(item) | b::Type::Cell(item) => pending.push(*item),
            b::Type::Fn(signature) => {
                pending.extend(&signature.params);
                pending.push(signature.ret);
                if let Some(sets) = signature.sets {
                    pending.extend([sets.errors, sets.effects]);
                }
            }
            b::Type::Set(row) => pending.extend(&row.params),
            b::Type::Handler(effect) => pending.extend(&core.effects[*effect].args),
            _ => {}
        }
    }
    false
}
pub(crate) fn audit(program: &ir::Program) -> Result<(), Vec<Diagnostic>> {
    let scopes = program
        .functions
        .iter()
        .filter_map(|function| {
            (function.intrinsic == Some(Intrinsic::FiberScope)).then_some(function.id)
        })
        .collect::<HashSet<_>>();
    for function in &program.functions {
        let mut pending = Vec::new();
        if let Some(body) = &function.body {
            pending.extend(body.statements.iter().map(|statement| match statement {
                ir::Statement::Pattern { value, .. } | ir::Statement::Expr(value) => value,
            }));
        }
        while let Some(expression) = pending.pop() {
            if matches!(expression.kind, ir::ExprKind::Call {target, ..} if scopes.contains(&target))
                && contains_handle(&program.core, expression.ty)
            {
                return Err(vec![
                    Diagnostic::new(
                        DiagnosticKind::Type,
                        expression.span,
                        "a fiber handle or Deferred cannot escape its scope result",
                    )
                    .in_source(&function.source_name),
                ]);
            }
            super::check::suspend::push_children(expression, &mut pending);
        }
    }
    Ok(())
}
