use super::*;

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
    let mut pending: Vec<_> = program
        .functions
        .iter()
        .flat_map(|(_, f)| std::iter::once(f.ret).chain(f.vars.iter().map(|(_, var)| var.ty)))
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
                pending.extend(
                    fields
                        .into_iter()
                        .map(|field| program.core.types.subst(field, &args)),
                );
            }
            Type::List(element) | Type::Cell(element) => pending.push(element),
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
        active: &mut HashSet<TypeId>,
    ) -> Result<()> {
        if ty.index() >= program.core.types.len() {
            return Err(invalid(function, "type ID is out of bounds"));
        }
        if seen.contains(&ty) {
            return Ok(());
        }
        if !active.insert(ty) {
            return Err(unsupported(function, "recursive value type"));
        }
        match program.core.ty(ty) {
            Type::Int
            | Type::Int64
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Unit
            | Type::Never
            | Type::Any => {}
            Type::Tuple(fields) => {
                for &field in fields {
                    visit(program, function, field, seen, active)?;
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
                    visit(program, function, arg, seen, active)?;
                }
                for variant in 0..definition.variants.len() {
                    for field in enum_fields(program, function, ty, variant)? {
                        visit(program, function, field, seen, active)?;
                    }
                }
            }
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
        &mut HashSet::new(),
    )
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
