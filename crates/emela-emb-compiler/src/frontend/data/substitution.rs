//! Source substitutions retain invocation identities until ABI projection.
use crate::contract::ir::{Type, TypeId, TypeTable};

pub(crate) fn substitute(types: &mut TypeTable, ty: TypeId, arguments: &[TypeId]) -> TypeId {
    let node = match types[ty].clone() {
        Type::Param(index) => return arguments.get(index).copied().unwrap_or(ty),
        Type::Tuple(fields) => Type::Tuple(substitute_all(types, fields, arguments)),
        Type::Enum(id, fields) => Type::Enum(id, substitute_all(types, fields, arguments)),
        Type::List(item) => Type::List(substitute(types, item, arguments)),
        Type::Cell(item) => Type::Cell(substitute(types, item, arguments)),
        Type::Fn(mut function) => {
            function.params = substitute_all(types, function.params, arguments);
            function.ret = substitute(types, function.ret, arguments);
            if let Some(sets) = &mut function.sets {
                sets.errors = substitute(types, sets.errors, arguments);
                sets.effects = substitute(types, sets.effects, arguments);
            }
            Type::Fn(function)
        }
        Type::Set(mut row) => {
            let members = substitute_all(types, row.params, arguments);
            row.params = Vec::new();
            for member in members {
                match types[member].clone() {
                    Type::Set(other) => row.merge(&other),
                    _ => row.params.push(member),
                }
            }
            row.params.sort();
            row.params.dedup();
            Type::Set(row)
        }
        _ => return ty,
    };
    types.intern(node)
}

fn substitute_all(types: &mut TypeTable, fields: Vec<TypeId>, arguments: &[TypeId]) -> Vec<TypeId> {
    fields
        .into_iter()
        .map(|ty| substitute(types, ty, arguments))
        .collect()
}
