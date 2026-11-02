//! Error diagnostics use written/derived Show when available, otherwise only the name.
use super::*;
use crate::frontend::{
    traits::{BuiltinTrait, ParamBounds},
    types::{InvocationSets, Signature},
};

impl Specializer {
    pub(super) fn error_displays(&mut self) -> Result<()> {
        let mut visited = HashSet::new();
        while let Some((ty, owner)) = self.error_types.pop_first() {
            if !visited.insert(ty) {
                continue;
            }
            let b::Type::Enum(error, _) = self.core.types[ty] else {
                unreachable!("only concrete error types are registered")
            };
            let caller = self
                .functions
                .iter()
                .find(|function| function.id == owner)
                .expect("error type belongs to a specialized function")
                .clone();
            let show = self.registry.builtin_id(BuiltinTrait::Show);
            let supports_show = show
                .map(|show| {
                    self.registry
                        .satisfies(&self.core, ty, show, &ParamBounds::new())
                })
                .transpose()
                .map_err(|failure| diagnostic(&caller, caller.span, failure.message))?
                .unwrap_or(false);
            let target = if supports_show {
                let show = show.expect("satisfied Show has a registered trait identity");
                self.trait_target(&caller, ty, show, "show", &[], false, caller.span, 0)?
            } else {
                self.error_name_helper(&caller, ty, error)?
            };
            self.functions
                .iter_mut()
                .find(|function| function.id == target)
                .expect("error display target has a concrete definition")
                .error_show = Some(error);
        }
        Ok(())
    }

    fn error_name_helper(
        &mut self,
        caller: &ir::Function,
        ty: b::TypeId,
        error: b::EnumId,
    ) -> Result<ir::DefinitionId> {
        if self.instances >= self.limits.instances {
            return Err(diagnostic(
                caller,
                caller.span,
                "error display specialization limit exceeded",
            ));
        }
        let id = ir::DefinitionId(self.next_id.ok_or_else(|| {
            diagnostic(caller, caller.span, "definition ID namespace is exhausted")
        })?);
        self.next_id = id.0.checked_add(1);
        self.instances += 1;
        let name = self.core.enums[error]
            .variants
            .first()
            .map(|variant| variant.name.clone())
            .unwrap_or_else(|| self.core.enums[error].name.clone());
        let param = ir::Local {
            id: ir::LocalId(0),
            name: "error".into(),
            ty: frontend_type(ty),
        };
        self.functions.push(ir::Function {
            id,
            type_parameters: 0,
            immediate: false,
            constant: false,
            origin: None,
            derivation: None,
            error_show: Some(error),
            name: format!("__error_name_{}_{}", ty.index(), id.0),
            source_name: caller.source_name.clone(),
            span: caller.span,
            exported: false,
            export_symbol: None,
            signature: Signature {
                params: vec![param.ty],
                ret: Some(ValueType::String),
                sets: InvocationSets::default(),
            },
            params: vec![param],
            external: None,
            intrinsic: None,
            body: Some(ir::Block {
                ty: ValueType::String,
                span: caller.span,
                statements: vec![ir::Statement::Expr(ir::Expr {
                    kind: ir::ExprKind::Literal(ir::Literal::String(name)),
                    ty: ValueType::String,
                    span: caller.span,
                })],
            }),
        });
        Ok(id)
    }
}
