//! Project source failure/effect identities to the concrete function value ABI.
use super::ir;
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{Definitions, FnSets, SetSummary, Type, TypeId},
};
use std::collections::HashSet;

pub(crate) fn is_async(core: &Definitions, effect: crate::contract::ir::EffectId) -> bool {
    let effect = &core.effects[effect];
    effect.decl == "Async" && effect.args.is_empty() && effect.label_of.is_none()
}
pub(crate) fn effect_flags(
    core: &Definitions,
    effect: crate::contract::ir::EffectId,
) -> (bool, bool) {
    if is_async(core, effect) {
        (false, true)
    } else {
        (
            true,
            core.effects[effect]
                .ops
                .iter()
                .any(|operation| operation.suspend),
        )
    }
}
pub(crate) fn effects_abi(core: &Definitions, members: &[TypeId]) -> (bool, bool) {
    let mut pending = members.to_vec();
    let mut seen = HashSet::new();
    let (mut evidence, mut suspends) = (false, false);
    while let Some(member) = pending.pop() {
        if !seen.insert(member) {
            continue;
        }
        match core.ty(member) {
            Type::Handler(effect) => {
                let flags = effect_flags(core, *effect);
                evidence |= flags.0;
                suspends |= flags.1;
            }
            Type::Set(row) => {
                evidence |= row.ev;
                suspends |= row.suspends;
                pending.extend(&row.params);
            }
            Type::Param(_) => {
                evidence = true;
                suspends = true;
            }
            _ => {}
        }
    }
    (evidence, suspends)
}

pub(crate) fn project(program: &mut ir::Program) -> Result<(), Vec<Diagnostic>> {
    project_concurrency_rows(&mut program.core);
    let mut updates = Vec::new();
    let initial = program.core.types.types.len();
    for index in 0..initial {
        let Type::Fn(mut function) = program.core.types.types[index].clone() else {
            continue;
        };
        let Some(sets) = function.sets else { continue };
        let errors =
            summary(&program.core, sets.errors, false, &mut HashSet::new()).map_err(|message| {
                vec![Diagnostic::new(
                    DiagnosticKind::Type,
                    Span::new(0, 0),
                    message,
                )]
            })?;
        let effects =
            summary(&program.core, sets.effects, true, &mut HashSet::new()).map_err(|message| {
                vec![Diagnostic::new(
                    DiagnosticKind::Type,
                    Span::new(0, 0),
                    message,
                )]
            })?;
        if errors.params.is_empty() && effects.params.is_empty() {
            function.fails = errors.fails;
            function.ev = effects.ev;
            function.suspends = effects.suspends;
            function.sets = None;
        } else {
            function.fails |= errors.fails;
            function.ev |= effects.ev;
            function.suspends |= effects.suspends;
            function.sets = Some(FnSets {
                errors: program.core.types.intern(Type::Set(errors)),
                effects: program.core.types.intern(Type::Set(effects)),
            });
        }
        updates.push((index, Type::Fn(function)));
    }
    for (index, ty) in updates {
        program.core.types.types[index] = ty;
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
    Ok(())
}
fn project_concurrency_rows(core: &mut Definitions) {
    let cause = core
        .enums
        .iter()
        .find_map(|(id, definition)| (definition.name == "Std.Cause.Cause").then_some(id));
    let exit = core.enums.iter().find_map(|(id, definition)| {
        (definition.name == "Std.Fiber.Exit" && definition.params == 2).then_some(id)
    });
    for node in &mut core.types.types {
        if let Type::Enum(id, arguments) = node {
            if Some(*id) == cause {
                arguments.clear();
            } else if Some(*id) == exit {
                arguments.truncate(1);
            }
        }
    }
    if let Some(cause) = cause {
        core.enums[cause].params = 0;
        core.enums[cause].external = true;
        core.enums[cause].variants.clear();
    }
    if let Some(exit) = exit {
        core.enums[exit].params = 1;
    }
}
fn summary(
    core: &Definitions,
    ty: TypeId,
    effect: bool,
    active: &mut HashSet<TypeId>,
) -> Result<SetSummary, String> {
    if !active.insert(ty) {
        return Err("cyclic function invocation set".into());
    }
    let Some(Type::Set(written)) = core.types.types.get(ty.index()) else {
        return Err("function invocation metadata must be a set".into());
    };
    let mut result = SetSummary {
        params: vec![],
        ..written.clone()
    };
    for &parameter in &written.params {
        match core.ty(parameter) {
            Type::Param(_) => result.params.push(parameter),
            Type::Set(_) => result.merge(&summary(core, parameter, effect, active)?),
            Type::Enum(id, _) if !effect && core.enums[*id].error => result.fails = true,
            Type::Handler(id) if effect => {
                let (ev, suspends) = effect_flags(core, *id);
                result.ev |= ev;
                result.suspends |= suspends;
            }
            _ => return Err("invalid member in a function invocation set".into()),
        }
    }
    active.remove(&ty);
    result.params.sort();
    result.params.dedup();
    Ok(result)
}
