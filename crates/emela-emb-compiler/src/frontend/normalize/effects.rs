//! Freeze type arguments carried by capability identities as well as ordinary type nodes.
use super::*;

impl Context<'_> {
    fn raw_type(&mut self, ty: TypeId, span: Span) -> Result<TypeId, Diagnostic> {
        let resolved = self
            .solver
            .substitute(&mut self.core.types, ty, &self.substitutions)
            .map_err(|error| {
                Diagnostic::new(DiagnosticKind::Type, span, error.message).in_source(self.source)
            })?;
        self.effect_type(resolved, span)
    }
    pub(super) fn effect_type(&mut self, ty: TypeId, span: Span) -> Result<TypeId, Diagnostic> {
        if let Some(&mapped) = self.effect_types.get(&ty) {
            return Ok(mapped);
        }
        self.effect_types.insert(ty, ty);
        let mut node = self.core.types[ty].clone();
        match &mut node {
            Type::Handler(effect) => *effect = self.effect(*effect, span)?,
            Type::Tuple(fields) | Type::Enum(_, fields) => {
                for field in fields {
                    *field = self.effect_type(*field, span)?;
                }
            }
            Type::List(element) | Type::Cell(element) => {
                *element = self.effect_type(*element, span)?
            }
            Type::Fn(function) => {
                for param in &mut function.params {
                    *param = self.effect_type(*param, span)?;
                }
                function.ret = self.effect_type(function.ret, span)?;
                if let Some(sets) = &mut function.sets {
                    sets.errors = self.effect_type(sets.errors, span)?;
                    sets.effects = self.effect_type(sets.effects, span)?;
                }
            }
            Type::Set(summary) => {
                for param in &mut summary.params {
                    *param = self.effect_type(*param, span)?;
                }
            }
            _ => {}
        }
        let mapped = self.core.types.intern(node);
        self.effect_types.insert(ty, mapped);
        Ok(mapped)
    }
    pub(super) fn effect(&mut self, effect: EffectId, span: Span) -> Result<EffectId, Diagnostic> {
        if let Some(&mapped) = self.effect_ids.get(&effect) {
            return Ok(mapped);
        }
        let Some(mut definition) = self
            .core
            .effects
            .iter()
            .find_map(|(id, definition)| (id == effect).then(|| definition.clone()))
        else {
            return Err(Diagnostic::new(
                DiagnosticKind::Type,
                span,
                "effect identity is outside the catalog before normalization",
            )
            .in_source(self.source));
        };
        // Reserve recursive references. Concrete instances with different arguments receive
        // a fresh identity before their operation signatures are visited.
        self.effect_ids.insert(effect, effect);
        for arg in &mut definition.args {
            *arg = self.raw_type(*arg, span)?;
        }
        if let Some(base) = &mut definition.label_of {
            *base = self.effect(*base, span)?;
        }
        let existing = self.core.effects.iter().find_map(|(id, candidate)| {
            (candidate.decl == definition.decl
                && candidate.args == definition.args
                && candidate.label_of == definition.label_of)
                .then_some(id)
        });
        let target = existing.unwrap_or_else(|| self.core.effects.alloc(definition.clone()));
        self.effect_ids.insert(effect, target);
        for operation in &mut definition.ops {
            for param in &mut operation.params {
                *param = self.raw_type(*param, span)?;
            }
            operation.ret = self.raw_type(operation.ret, span)?;
        }
        self.core.effects[target] = definition;
        Ok(target)
    }
}
