//! Runtime suspension is separate from evidence-bearing capability requirements.
use super::*;

impl Checker {
    pub(super) fn async_type(
        &mut self,
        module: usize,
        span: Span,
    ) -> Result<crate::contract::ir::TypeId, Diagnostic> {
        let effect = self
            .catalog
            .effects
            .declarations
            .iter()
            .find(|declaration| {
                let source = &self.modules.modules[declaration.module];
                source.builtin && source.name == "Prelude" && declaration.ast.name.value == "Async"
            })
            .map(|declaration| declaration.id)
            .ok_or_else(|| {
                self.error(
                    module,
                    DK::Type,
                    span,
                    "the compiler-owned Async requirement is unavailable",
                )
            })?;
        Ok(self
            .catalog
            .core
            .types
            .intern(crate::contract::ir::Type::Handler(effect)))
    }
    pub(super) fn validate_external_reference(
        &self,
        module: usize,
        id: DefinitionId,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let definition = &self.definitions[id.0];
        if definition.external.is_some()
            && definition.ast.suspends
            && !matches!(
                self.current_effect_method,
                Some(super::super::effect_collect::MethodKind::Operation { .. })
            )
        {
            return Err(self.error(
                module,
                DK::Type,
                span,
                "an external suspend function may only be referenced inside a handler operation",
            ));
        }
        Ok(())
    }
}
