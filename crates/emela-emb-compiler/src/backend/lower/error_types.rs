//! Give each concrete generic error its own erased payload slot before ABI planning.
//! Type IDs stay stable so nested aggregates and error-pattern metadata remain connected.
use super::*;

pub(super) fn normalize(program: &mut Program) -> Result<()> {
    let original_types = program.core.types.types.clone();
    let mut replacements = HashMap::new();
    for (index, node) in original_types.iter().enumerate() {
        let Type::Enum(id, args) = node else { continue };
        let definition = program
            .core
            .enums
            .iter()
            .find_map(|(key, value)| (key == *id).then_some(value))
            .ok_or_else(|| {
                invalid(
                    "<module>",
                    "error specialization references an invalid enum",
                )
            })?;
        if !definition.error
            || definition.params == 0
            || has_parameter(
                &original_types,
                TypeId::from_index(index),
                &mut HashSet::new(),
            )?
        {
            continue;
        }
        if definition.params != args.len() {
            return Err(invalid(
                &definition.name,
                "generic error argument count differs from declaration",
            ));
        }
        let mut concrete = definition.clone();
        concrete.name = format!("{}$error{index}", concrete.name);
        concrete.params = 0;
        for variant in &mut concrete.variants {
            for field in &mut variant.tys {
                *field = program.core.types.subst(*field, args);
            }
        }
        let concrete_id = program.core.enums.alloc(concrete);
        if program.core.expanding_enums.contains(id) {
            program.core.expanding_enums.insert(concrete_id);
        }
        replacements.insert(TypeId::from_index(index), concrete_id);
    }
    for (_, function) in program.functions.iter_mut() {
        for (_, statement) in function.stmts.iter_mut() {
            let Stmt::Let { dst, value, .. } = statement else {
                continue;
            };
            match value {
                Value::Make { ctor, .. } => {
                    if let Some(ty) = dst.and_then(|dst| {
                        function
                            .vars
                            .iter()
                            .find_map(|(id, var)| (id == dst).then_some(var.ty))
                    }) {
                        if let Some(&id) = replacements.get(&ty) {
                            if !matches!(original_types.get(ty.index()), Some(Type::Enum(original, _)) if *original == ctor.enum_id)
                            {
                                return Err(invalid(
                                    &function.name,
                                    "generic error constructor differs from its destination type",
                                ));
                            }
                            ctor.enum_id = id;
                        }
                    }
                }
                Value::Field { base, ctor, .. } => {
                    if let Some(ty) = function
                        .vars
                        .iter()
                        .find_map(|(id, var)| (id == *base).then_some(var.ty))
                    {
                        if let Some(&id) = replacements.get(&ty) {
                            if !matches!(original_types.get(ty.index()), Some(Type::Enum(original, _)) if *original == ctor.enum_id)
                            {
                                return Err(invalid(
                                    &function.name,
                                    "generic error projection differs from its base type",
                                ));
                            }
                            ctor.enum_id = id;
                        }
                    }
                }
                Value::IsError {
                    enum_id,
                    error_type,
                    ..
                }
                | Value::AsError {
                    enum_id,
                    error_type,
                    ..
                } => {
                    if !matches!(original_types.get(error_type.index()), Some(Type::Enum(id, _)) if id == enum_id)
                    {
                        return Err(invalid(
                            &function.name,
                            "error operation type differs from its declared enum",
                        ));
                    }
                    if let Some(&id) = replacements.get(error_type) {
                        *enum_id = id;
                    }
                }
                _ => {}
            }
        }
    }
    for (error, show) in &mut program.error_shows {
        let ty = program
            .functions
            .iter()
            .find_map(|(id, function)| {
                (id == *show).then(|| {
                    function.params.first().and_then(|param| {
                        function
                            .vars
                            .iter()
                            .find_map(|(id, var)| (id == *param).then_some(var.ty))
                    })
                })
            })
            .flatten();
        if let Some(id) = ty.and_then(|ty| replacements.get(&ty)) {
            *error = *id;
        }
    }
    for (ty, id) in replacements {
        program.core.types.types[ty.index()] = Type::Enum(id, vec![]);
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

fn has_parameter(types: &[Type], ty: TypeId, seen: &mut HashSet<TypeId>) -> Result<bool> {
    if !seen.insert(ty) {
        return Ok(false);
    }
    let node = types
        .get(ty.index())
        .ok_or_else(|| invalid("<module>", "error type argument is out of bounds"))?;
    let children = match node {
        Type::Param(_) => return Ok(true),
        Type::Enum(_, args) | Type::Tuple(args) => args.clone(),
        Type::List(element) | Type::Cell(element) => vec![*element],
        Type::Fn(ft) => ft
            .params
            .iter()
            .copied()
            .chain([ft.ret])
            .chain(
                ft.sets
                    .into_iter()
                    .flat_map(|sets| [sets.errors, sets.effects]),
            )
            .collect(),
        Type::Set(summary) => summary.params.clone(),
        _ => vec![],
    };
    for child in children {
        if has_parameter(types, child, seen)? {
            return Ok(true);
        }
    }
    Ok(false)
}
