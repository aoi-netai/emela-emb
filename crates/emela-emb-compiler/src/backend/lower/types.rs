use super::*;

#[path = "expanding.rs"]
mod expanding;

pub(super) fn validate_catalog(program: &Program) -> Result<()> {
    let canonical = [
        Type::Int,
        Type::Int64,
        Type::Float,
        Type::Bool,
        Type::String,
        Type::Unit,
        Type::Never,
        Type::Evidence,
        Type::FiberScope,
        Type::Any,
        Type::Bytes,
    ];
    if !program.core.types.types.starts_with(&canonical) {
        return Err(invalid(
            "<module>",
            "canonical builtin type IDs have been changed",
        ));
    }
    for (_, definition) in program.core.enums.iter() {
        for variant in &definition.variants {
            if variant.fields.len() != variant.tys.len() {
                return Err(invalid(
                    &definition.name,
                    "enum field names and field types differ in length",
                ));
            }
            for &ty in &variant.tys {
                validate_declared_type(
                    program,
                    &definition.name,
                    ty,
                    definition.params,
                    &mut HashSet::new(),
                )?;
            }
        }
    }
    for &(enum_id, show) in &program.error_shows {
        if !program
            .core
            .enums
            .iter()
            .any(|(id, definition)| id == enum_id && definition.error)
            || !program.functions.iter().any(|(id, _)| id == show)
        {
            return Err(invalid(
                "<module>",
                "error display metadata references an invalid definition",
            ));
        }
    }
    Ok(())
}

fn validate_declared_type(
    program: &Program,
    name: &str,
    ty: TypeId,
    parameters: usize,
    seen: &mut HashSet<TypeId>,
) -> Result<()> {
    if ty.index() >= program.core.types.len() {
        return Err(invalid(name, "enum field type ID is out of bounds"));
    }
    if !seen.insert(ty) {
        return Ok(());
    }
    let children = match program.core.ty(ty) {
        Type::Param(index) if *index >= parameters => {
            return Err(invalid(
                name,
                "enum field references an undeclared type parameter",
            ));
        }
        Type::Tuple(fields) => fields.clone(),
        Type::Enum(id, args) => {
            let definition = program
                .core
                .enums
                .iter()
                .find_map(|(key, value)| (key == *id).then_some(value))
                .ok_or_else(|| invalid(name, "enum field references an invalid enum ID"))?;
            if definition.params != args.len() {
                return Err(invalid(
                    name,
                    "enum field type argument count differs from its declaration",
                ));
            }
            args.clone()
        }
        Type::List(element) | Type::Cell(element) => vec![*element],
        Type::Fn(ft) => ft.params.iter().copied().chain([ft.ret]).collect(),
        Type::Set(summary) => summary.params.clone(),
        _ => vec![],
    };
    for child in children {
        validate_declared_type(program, name, child, parameters, seen)?;
    }
    Ok(())
}

/// Materialize substituted field types before any stage indexes the shared catalog.
/// IDs already supplied by the frontend remain stable because interning only appends.
pub(super) fn normalize_catalog(program: &mut Program) -> Result<()> {
    program.core.expanding_enums = expanding::classify(program);
    let mut pending: Vec<_> = program
        .functions
        .iter()
        .flat_map(|(_, f)| {
            std::iter::once(f.ret)
                    .chain(f.vars.iter().map(|(_, var)| var.ty))
                    .chain(
                        f.stmts.iter().filter_map(|(_, statement)| match statement {
                            Stmt::Let {
                                value:
                                    Value::IsError { error_type, .. }
                                    | Value::AsError { error_type, .. },
                                ..
                            } => Some(*error_type),
                            _ => None,
                        }),
                    )
        })
        .collect();
    let mut seen = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if seen.len() > 16384 {
            return Err(unsupported("<module>", "expanding concrete type graph"));
        }
        if ty.index() >= program.core.types.len() {
            return Err(invalid("<module>", "type ID is out of bounds"));
        }
        match program.core.types[ty].clone() {
            Type::Tuple(fields) => pending.extend(fields),
            Type::Enum(id, args) => {
                let definition = program
                    .core
                    .enums
                    .iter()
                    .find_map(|(key, value)| (key == id).then_some(value))
                    .ok_or_else(|| invalid("<module>", "enum ID is out of bounds"))?;
                if definition.params != args.len() {
                    return Err(invalid(
                        "<module>",
                        "enum type argument count differs from declaration",
                    ));
                }
                for &arg in &args {
                    validate_declared_type(
                        program,
                        "<module>",
                        arg,
                        usize::MAX,
                        &mut HashSet::new(),
                    )?;
                }
                let fields: Vec<_> = definition
                    .variants
                    .iter()
                    .flat_map(|variant| variant.tys.iter().copied())
                    .collect();
                pending.extend(args.iter().copied());
                let fields: Vec<_> = fields
                    .into_iter()
                    .map(|field| program.core.types.subst(field, &args))
                    .collect();
                for field in fields {
                    if !program.core.expanding_enums.contains(&id)
                        || !is_reference_type(program, field)
                    {
                        pending.push(field);
                    }
                }
            }
            Type::List(element) | Type::Cell(element) => pending.push(element),
            Type::Fn(ft) => pending.extend(ft.params.into_iter().chain([ft.ret])),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn validate_type(program: &Program, function: &str, ty: TypeId) -> Result<()> {
    fn visit(
        program: &Program,
        function: &str,
        ty: TypeId,
        seen: &mut HashSet<TypeId>,
        active: &mut HashMap<TypeId, usize>,
        indirections: usize,
    ) -> Result<()> {
        if ty.index() >= program.core.types.len() {
            return Err(invalid(function, "type ID is out of bounds"));
        }
        if seen.contains(&ty) {
            return Ok(());
        }
        if let Some(&entered) = active.get(&ty) {
            return if indirections > entered {
                // Recursive enum and List edges are represented by owned heap pointers.
                Ok(())
            } else {
                Err(invalid(
                    function,
                    "value type has an unguarded structural cycle",
                ))
            };
        }
        active.insert(ty, indirections);
        match program.core.ty(ty) {
            Type::Int
            | Type::Int64
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Bytes
            | Type::Unit
            | Type::Never
            | Type::Any => {}
            Type::Tuple(fields) => {
                for &field in fields {
                    visit(program, function, field, seen, active, indirections)?;
                }
            }
            Type::Enum(id, args) => {
                let definition = program
                    .core
                    .enums
                    .iter()
                    .find_map(|(key, value)| (key == *id).then_some(value))
                    .ok_or_else(|| invalid(function, "enum ID is out of bounds"))?;
                if args.len() != definition.params {
                    return Err(invalid(
                        function,
                        "enum type argument count differs from declaration",
                    ));
                }
                for &arg in args {
                    visit(program, function, arg, seen, active, indirections + 1)?;
                }
                for variant in 0..definition.variants.len() {
                    for field in enum_fields(program, function, ty, variant)? {
                        if program.core.expanding_enums.contains(id)
                            && is_reference_type(program, field)
                        {
                            continue;
                        }
                        visit(program, function, field, seen, active, indirections + 1)?;
                    }
                }
            }
            Type::List(element) => {
                visit(program, function, *element, seen, active, indirections + 1)?
            }
            Type::Fn(ft) => {
                if ft.suspends || ft.sets.is_some() {
                    return Err(unsupported(function, "unresolved or suspending function value"));
                }
                for &child in ft.params.iter().chain([&ft.ret]) {
                    visit(program, function, child, seen, active, indirections + 1)?;
                }
            }
            Type::Evidence => {}
            _ => return Err(unsupported(function, "value type")),
        }
        active.remove(&ty);
        seen.insert(ty);
        Ok(())
    }
    visit(
        program,
        function,
        ty,
        &mut HashSet::new(),
        &mut HashMap::new(),
        0,
    )
}

fn is_reference_type(program: &Program, ty: TypeId) -> bool {
    match program.core.ty(ty) {
        Type::Int
        | Type::Int64
        | Type::Float
        | Type::Bool
        | Type::Unit
        | Type::Never
        | Type::Set(_) => false,
        Type::Enum(id, _) => {
            let definition = &program.core.enums[*id];
            definition.error
                || definition.external
                || definition.variants.is_empty()
                || definition
                    .variants
                    .iter()
                    .any(|variant| !variant.tys.is_empty())
        }
        _ => true,
    }
}

pub(super) fn enum_fields(
    program: &Program,
    function: &str,
    ty: TypeId,
    variant: usize,
) -> Result<Vec<TypeId>> {
    let Type::Enum(id, args) = program.core.ty(ty) else {
        return Err(invalid(function, "value is not an enum"));
    };
    let definition = program
        .core
        .enums
        .iter()
        .find_map(|(key, value)| (key == *id).then_some(value))
        .ok_or_else(|| invalid(function, "enum ID is out of bounds"))?;
    let fields = &definition
        .variants
        .get(variant)
        .ok_or_else(|| invalid(function, "enum variant index is out of bounds"))?
        .tys;
    let mut types = program.core.types.clone();
    let fields: Vec<_> = fields
        .iter()
        .map(|&field| types.subst(field, args))
        .collect();
    if fields
        .iter()
        .any(|ty| ty.index() >= program.core.types.len())
    {
        return Err(invalid(
            function,
            "instantiated enum field type is missing from the type catalog",
        ));
    }
    Ok(fields)
}

pub(super) fn option_type(
    program: &Program,
    function: &str,
    payload: TypeId,
    expected: Option<TypeId>,
) -> Result<TypeId> {
    for (index, ty) in program.core.types.types.iter().enumerate() {
        let Type::Enum(id, _) = ty else {
            continue;
        };
        let definition = program
            .core
            .enums
            .iter()
            .find_map(|(key, value)| (key == *id).then_some(value))
            .ok_or_else(|| invalid(function, "Option enum ID is out of bounds"))?;
        if definition.name != "Option" {
            continue;
        }
        let ty = TypeId::from_index(index);
        if expected.is_some_and(|expected| expected != ty) {
            continue;
        }
        let Some(some) = definition
            .variants
            .iter()
            .position(|variant| variant.name == "Some")
        else {
            continue;
        };
        if enum_fields(program, function, ty, some)? == [payload] {
            return Ok(ty);
        }
    }
    Err(invalid(
        function,
        "builtin result requires an instantiated Option payload type",
    ))
}
