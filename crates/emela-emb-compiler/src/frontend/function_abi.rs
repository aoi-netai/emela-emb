//! Project source failure/effect identities to the concrete function value ABI.
use super::ir;
use crate::{
    Diagnostic, DiagnosticKind, Span,
    contract::ir::{Definitions, FnSets, SetSummary, Type, TypeId},
};
use std::collections::HashSet;

pub(crate) fn project(program: &mut ir::Program) -> Result<(), Vec<Diagnostic>> {
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
                result.ev = true;
                result.suspends |= core.effects[*id]
                    .ops
                    .iter()
                    .any(|operation| operation.suspend);
            }
            _ => return Err("invalid member in a function invocation set".into()),
        }
    }
    active.remove(&ty);
    result.params.sort();
    result.params.dedup();
    Ok(result)
}
