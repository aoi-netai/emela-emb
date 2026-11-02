//! Preserve one erased initializer for every multiply instantiated source const.
use super::*;

impl Specializer {
    pub(super) fn canonical_constants(&mut self) -> Result<()> {
        let mut canonical = BTreeMap::new();
        loop {
            let mut counts = BTreeMap::new();
            for function in &self.functions {
                if function.constant {
                    if let Some(origin) = function.origin {
                        *counts.entry(origin).or_insert(0usize) += 1;
                    }
                }
            }
            let needed = counts
                .into_iter()
                .filter_map(|(origin, count)| {
                    (count > 1 && !canonical.contains_key(&origin)).then_some(origin)
                })
                .collect::<Vec<_>>();
            if needed.is_empty() {
                break;
            }
            for origin in needed {
                let template = self.templates[&origin].clone();
                let arguments = self.uniform_const_arguments(&template);
                let id = self.instantiate(&template, origin, arguments, template.span, 0)?;
                canonical.insert(origin, id);
            }
        }
        for function in &mut self.functions {
            if function.constant {
                if let Some(origin) = function.origin {
                    function.canonical_const = canonical.get(&origin).copied();
                    // Const contexts always report the source name, including a single instance.
                    function.name = self.templates[&origin].name.clone();
                }
            }
        }
        Ok(())
    }

    fn uniform_const_arguments(&mut self, function: &ir::Function) -> Vec<b::TypeId> {
        fn visit(
            core: &b::Definitions,
            ty: b::TypeId,
            row: bool,
            seen: &mut HashSet<(b::TypeId, bool)>,
            sets: &mut HashSet<usize>,
        ) {
            if !seen.insert((ty, row)) {
                return;
            }
            match core.ty(ty) {
                b::Type::Param(index) if row => {
                    sets.insert(*index);
                }
                b::Type::Fn(function) => {
                    for &ty in function.params.iter().chain([&function.ret]) {
                        visit(core, ty, false, seen, sets);
                    }
                    if let Some(invocation) = &function.sets {
                        visit(core, invocation.errors, true, seen, sets);
                        visit(core, invocation.effects, true, seen, sets);
                    }
                }
                b::Type::Tuple(values) | b::Type::Enum(_, values) => {
                    for &ty in values {
                        visit(core, ty, false, seen, sets);
                    }
                }
                b::Type::List(value) | b::Type::Cell(value) => {
                    visit(core, *value, false, seen, sets)
                }
                b::Type::Set(summary) => {
                    for &ty in &summary.params {
                        visit(core, ty, true, seen, sets);
                    }
                }
                b::Type::Handler(effect) => {
                    for &ty in &core.effects[*effect].args {
                        visit(core, ty, false, seen, sets);
                    }
                }
                _ => {}
            }
        }
        let mut sets = HashSet::new();
        let mut seen = HashSet::new();
        for ty in function
            .signature
            .params
            .iter()
            .copied()
            .chain(function.signature.ret)
        {
            visit(&self.core, backend_type(ty), false, &mut seen, &mut sets);
        }
        for &ty in function
            .signature
            .sets
            .errors
            .iter()
            .chain(&function.signature.sets.effects)
        {
            visit(&self.core, ty, true, &mut seen, &mut sets);
        }
        let opaque = self.core.types.intern(b::Type::Set(b::SetSummary {
            fails: true,
            ev: true,
            suspends: false,
            params: vec![],
        }));
        (0..function.type_parameters)
            .map(|index| {
                if sets.contains(&index) {
                    opaque
                } else {
                    b::TypeId::ANY
                }
            })
            .collect()
    }
}
