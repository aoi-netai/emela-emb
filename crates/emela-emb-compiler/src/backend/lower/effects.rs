//! Keep labeled evidence keys distinct while sharing one capability ABI and lifetime.
use super::*;

pub(super) fn normalize(program: &mut Program) -> Result<Vec<TypeId>> {
    let mut bases = HashMap::new();
    for (effect, definition) in program.core.effects.iter() {
        let base = crate::semantics::effect_base(&program.core, effect).ok_or_else(|| {
            invalid(
                &definition.name,
                "effect label refers to an invalid or cyclic base",
            )
        })?;
        bases.insert(effect, base);
    }
    let original = &program.core.types;
    let mut nodes = original.types.clone();
    for node in &mut nodes {
        if let Type::Handler(effect) = node {
            *effect = *bases
                .get(effect)
                .ok_or_else(|| invalid("<module>", "handler effect ID is out of bounds"))?;
        }
    }
    let mut representatives: Vec<_> = (0..nodes.len()).map(TypeId::from_index).collect();
    // Replacing a child with its earliest equivalent ID only merges classes.
    // This fixed point also handles guarded cycles without recursive traversal.
    loop {
        for node in &mut nodes {
            remap_children(node, &representatives)?;
        }
        let mut first = HashMap::new();
        let next: Vec<_> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                *first
                    .entry(node.clone())
                    .or_insert(TypeId::from_index(index))
            })
            .collect();
        if next == representatives {
            break;
        }
        representatives = next;
    }
    let mut compact = HashMap::new();
    for (index, &representative) in representatives.iter().enumerate() {
        if representative.index() == index {
            let next = TypeId::from_index(compact.len());
            compact.insert(representative, next);
        }
    }
    let mapped: Vec<_> = representatives
        .iter()
        .map(|representative| compact[representative])
        .collect();
    let mut types = TypeTable {
        types: vec![],
        has_param: vec![],
        ids: HashMap::new(),
    };
    for (index, mut node) in nodes.into_iter().enumerate() {
        if representatives[index].index() != index {
            continue;
        }
        remap_children(&mut node, &mapped)?;
        let id = TypeId::from_index(types.types.len());
        types.ids.insert(node.clone(), id);
        types.types.push(node);
        types.has_param.push(original.has_param[index]);
    }
    let map = |ty: &mut TypeId| -> Result<()> {
        *ty = *mapped
            .get(ty.index())
            .ok_or_else(|| invalid("<module>", "type ID is out of bounds"))?;
        Ok(())
    };
    for (_, definition) in program.core.enums.iter_mut() {
        for variant in &mut definition.variants {
            for ty in &mut variant.tys {
                map(ty)?;
            }
        }
    }
    for (_, definition) in program.core.effects.iter_mut() {
        for ty in &mut definition.args {
            map(ty)?;
        }
        for operation in &mut definition.ops {
            for ty in &mut operation.params {
                map(ty)?;
            }
            map(&mut operation.ret)?;
        }
    }
    for (effect, definition) in program.core.effects.iter() {
        let base = bases[&effect];
        if definition.ops != program.core.effects[base].ops {
            return Err(invalid(
                &definition.name,
                "effect label operations differ from its base",
            ));
        }
    }
    for (_, function) in program.functions.iter_mut() {
        map(&mut function.ret)?;
        if let Some(environment) = &mut function.env {
            for ty in environment {
                map(ty)?;
            }
        }
        for (_, variable) in function.vars.iter_mut() {
            map(&mut variable.ty)?;
        }
        for (_, statement) in function.stmts.iter_mut() {
            let Stmt::Let { value, .. } = statement else {
                continue;
            };
            match value {
                Value::Handler { effect, .. }
                | Value::Call {
                    callee: Callee::Op { effect, .. },
                    ..
                } => {
                    *effect = *bases.get(effect).ok_or_else(|| {
                        invalid(&function.name, "operation effect ID is out of bounds")
                    })?;
                }
                Value::IsError { error_type, .. } | Value::AsError { error_type, .. } => {
                    map(error_type)?
                }
                // EvExtend/EvLookup deliberately keep the exact labeled key.
                _ => {}
            }
        }
    }
    program.core.types = types;
    Ok(mapped)
}

fn remap_children(node: &mut Type, map: &[TypeId]) -> Result<()> {
    let remap = |ty: &mut TypeId| -> Result<()> {
        *ty = *map
            .get(ty.index())
            .ok_or_else(|| invalid("<module>", "type ID is out of bounds"))?;
        Ok(())
    };
    match node {
        Type::Tuple(fields) | Type::Enum(_, fields) => {
            for ty in fields {
                remap(ty)?;
            }
        }
        Type::List(element) | Type::Cell(element) => remap(element)?,
        Type::Fn(ft) => {
            for ty in &mut ft.params {
                remap(ty)?;
            }
            remap(&mut ft.ret)?;
            if let Some(sets) = &mut ft.sets {
                remap(&mut sets.errors)?;
                remap(&mut sets.effects)?;
            }
        }
        Type::Set(summary) => {
            for ty in &mut summary.params {
                remap(ty)?;
            }
        }
        _ => {}
    }
    Ok(())
}
