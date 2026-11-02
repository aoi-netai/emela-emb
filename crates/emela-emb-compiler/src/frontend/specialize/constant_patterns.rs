use super::*;
use crate::frontend::traits::{BuiltinTrait, ParamBounds};
impl Specializer {
    pub(super) fn constant_equality(
        &mut self,
        function: &ir::Function,
        ty: b::TypeId,
        span: Span,
        depth: usize,
    ) -> Result<ir::ConstantEquality> {
        if let Some(eq) = self.registry.builtin_id(BuiltinTrait::Eq) {
            if self
                .registry
                .satisfies(&self.core, ty, eq, &ParamBounds::new())
                .map_err(|error| diagnostic(function, span, error.message))?
            {
                let target = self.trait_target(function, ty, eq, "eq", false, span, depth)?;
                return Ok(ir::ConstantEquality::Function(target));
            }
        }
        if !crate::semantics::derived::supported(&self.core, ty, crate::DerivedOperation::Eq)
            || !self.only_structural_parts(function, ty, &mut HashSet::new(), 0)?
        {
            return Err(diagnostic(
                function,
                span,
                "constant pattern requires Eq when its data contains custom equality or non-structural values",
            ));
        }
        Ok(ir::ConstantEquality::Structural(ty))
    }
    fn only_structural_parts(
        &mut self,
        function: &ir::Function,
        ty: b::TypeId,
        seen: &mut HashSet<b::TypeId>,
        depth: usize,
    ) -> Result<bool> {
        if !seen.insert(ty) {
            return Ok(true);
        }
        if depth >= self.limits.type_depth {
            return Ok(false);
        }
        if let Some(eq) = self.registry.builtin_id(BuiltinTrait::Eq) {
            if self
                .registry
                .satisfies(&self.core, ty, eq, &ParamBounds::new())
                .map_err(|error| diagnostic(function, function.span, error.message))?
            {
                let resolved = self
                    .registry
                    .resolve(&self.core, ty, eq, "eq", &ParamBounds::new())
                    .map_err(|error| diagnostic(function, function.span, error.message))?;
                if let Some(definition) = resolved.definition {
                    if self
                        .templates
                        .get(&definition)
                        .is_none_or(|definition| definition.derivation != Some(BuiltinTrait::Eq))
                    {
                        return Ok(false);
                    }
                }
            }
        }
        let node = self
            .core
            .types
            .types
            .get(ty.index())
            .cloned()
            .ok_or_else(|| {
                diagnostic(
                    function,
                    function.span,
                    "constant pattern type is outside the catalog",
                )
            })?;
        let fields = match node {
            b::Type::Enum(id, args) => {
                let fields: Vec<_> = self.core.enums[id]
                    .variants
                    .iter()
                    .flat_map(|variant| variant.tys.iter().copied())
                    .collect();
                fields
                    .into_iter()
                    .map(|ty| {
                        self.substitute(
                            function,
                            ty,
                            &args,
                            function.span,
                            &mut HashMap::new(),
                            &mut HashSet::new(),
                            0,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            b::Type::Tuple(fields) => fields,
            b::Type::List(element) => vec![element],
            _ => vec![],
        };
        for field in fields {
            if !self.only_structural_parts(function, field, seen, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
