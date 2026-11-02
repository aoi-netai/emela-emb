//! Remove conservative generic Scope failures only for a closed, failure-free program.
//! The proof boundary follows the fixed upstream fiber_normalize pass.
use super::*;
use crate::contract::lowered::ClosureMetadata;

pub(super) fn erase(
    program: &mut Program,
    closures: &mut ClosureMetadata,
    options: &crate::Options,
) -> Result<()> {
    let reachable = closure::runtime_reachable(program, options);
    let has_scope = reachable.iter().any(|id| {
        program.functions[*id].stmts.iter().any(|(_, statement)| {
            matches!(
                statement,
                Stmt::Let {
                    value: Value::Fiber {
                        op: FiberOp::Scope { .. }
                            | FiberOp::Fork
                            | FiberOp::Deferred
                            | FiberOp::Complete
                            | FiberOp::Poll
                            | FiberOp::Join
                            | FiberOp::Await { .. }
                            | FiberOp::Interrupt
                            | FiberOp::DeferredAwait,
                        ..
                    },
                    ..
                }
            )
        })
    });
    if !has_scope {
        return Ok(());
    }
    for &id in &reachable {
        let function = &program.functions[id];
        if ((function.exported || function.external.is_some()) && function.fails)
            || function
                .seqs
                .iter()
                .any(|(_, sequence)| matches!(sequence.term, Term::Fail(_) | Term::Unwind))
            || function.stmts.iter().any(|(_, statement)| {
                matches!(
                    statement,
                    Stmt::Let {
                        value: Value::Fiber {
                            op: FiberOp::CauseRaise,
                            ..
                        },
                        ..
                    }
                )
            })
        {
            return Ok(());
        }
        let boundary = if function.external.is_some() {
            function
                .params
                .iter()
                .map(|id| function.vars[*id].ty)
                .chain([function.ret])
                .collect::<Vec<_>>()
        } else if function.exported {
            function
                .params
                .iter()
                .map(|id| function.vars[*id].ty)
                .collect()
        } else {
            vec![]
        };
        if boundary
            .into_iter()
            .any(|ty| unknown_failure(&program.core, ty, &mut HashSet::new()))
        {
            return Ok(());
        }
    }
    // Type IDs stay stable while projecting all function-valued fields and captures.
    // The common graph normalizer then deduplicates equivalent runtime types.
    for node in &mut program.core.types.types {
        if let Type::Fn(function) = node {
            function.fails = false;
        }
    }
    program.core.types.ids = program
        .core
        .types
        .types
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, ty)| (ty, TypeId::from_index(index)))
        .collect();
    for &id in &reachable {
        let function = &mut program.functions[id];
        function.fails = false;
        for (_, statement) in function.stmts.iter_mut() {
            if let Stmt::Let { value, .. } = statement {
                match value {
                    Value::Call { on_fail, .. } => *on_fail = None,
                    Value::Fiber { op, on_fail, .. } => {
                        *on_fail = None;
                        if let FiberOp::Scope { fails } = op {
                            *fails = false;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for (_, effect) in program.core.effects.iter_mut() {
        for operation in &mut effect.ops {
            operation.fails = false;
        }
    }
    let mapped = effects::normalize(program)?;
    for environment in &mut closures.environments {
        environment.function_type = mapped[environment.function_type.index()];
        for capture in &mut environment.captures {
            capture.ty = mapped[capture.ty.index()];
        }
    }
    Ok(())
}

fn unknown_failure(core: &Definitions, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    match core.ty(ty) {
        Type::Fn(function) => {
            function.fails
                || function
                    .params
                    .iter()
                    .chain([&function.ret])
                    .any(|&ty| unknown_failure(core, ty, seen))
        }
        Type::List(value) | Type::Cell(value) => unknown_failure(core, *value, seen),
        Type::Tuple(values) => values.iter().any(|&ty| unknown_failure(core, ty, seen)),
        Type::Enum(id, _) if core.enums[*id].name == "Std.Cause.Cause" => false,
        Type::Enum(id, arguments) => {
            arguments.iter().any(|&ty| unknown_failure(core, ty, seen))
                || core.enums[*id]
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.tys)
                    .any(|&ty| unknown_failure(core, ty, seen))
        }
        Type::Any | Type::Handler(_) | Type::Evidence => true,
        _ => false,
    }
}
