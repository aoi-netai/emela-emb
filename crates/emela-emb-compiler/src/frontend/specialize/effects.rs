//! A generic effect instance is specialized together with its function's type arguments.
use super::*;

impl Specializer {
    /// Recursive handler construction can precede inference of the operation
    /// implementation's requirements. Its wrapper uses the final concrete ABI.
    pub(super) fn handler_operation_abi(&mut self, operation: &mut ir::Expr) {
        let ir::ExprKind::Lambda { body, .. } = &operation.kind else {
            return;
        };
        let Some(ir::Statement::Expr(ir::Expr {
            kind: ir::ExprKind::Call { target, .. },
            ..
        })) = body.statements.last()
        else {
            return;
        };
        let Some(callee) = self
            .functions
            .iter()
            .find(|function| function.id == *target)
        else {
            return;
        };
        let suspends = !callee.immediate
            && super::super::function_abi::effects_abi(&self.core, &callee.signature.sets.effects)
                .1;
        let b::Type::Fn(mut signature) = self.core.types[backend_type(operation.ty)].clone() else {
            return;
        };
        if let Some(sets) = &mut signature.sets {
            sets.effects = self.core.types.intern(b::Type::Set(b::SetSummary {
                suspends,
                ..Default::default()
            }));
        }
        signature.ev = false;
        signature.suspends = suspends;
        operation.ty = frontend_type(self.core.types.intern(b::Type::Fn(signature)));
    }
    pub(super) fn effect(
        &mut self,
        function: &ir::Function,
        effect: b::EffectId,
        arguments: &[b::TypeId],
        span: Span,
        depth: usize,
    ) -> Result<b::EffectId> {
        let key = (effect, arguments.to_vec());
        if let Some(&mapped) = self.effect_instances.get(&key) {
            return Ok(mapped);
        }
        if depth >= self.limits.type_depth {
            return Err(diagnostic(
                function,
                span,
                "effect capability nesting exceeds the specialization limit",
            ));
        }
        let Some(mut definition) = self
            .core
            .effects
            .iter()
            .find_map(|(id, definition)| (id == effect).then(|| definition.clone()))
        else {
            return Err(diagnostic(
                function,
                span,
                "effect identity is outside the catalog before specialization",
            ));
        };
        self.effect_instances.insert(key.clone(), effect);
        for arg in &mut definition.args {
            *arg = self.substitute(
                function,
                *arg,
                arguments,
                span,
                &mut HashMap::new(),
                &mut HashSet::new(),
                depth + 1,
            )?;
        }
        if let Some(base) = &mut definition.label_of {
            *base = self.effect(function, *base, arguments, span, depth + 1)?;
        }
        let existing = self.core.effects.iter().find_map(|(id, candidate)| {
            (candidate.decl == definition.decl
                && candidate.args == definition.args
                && candidate.label_of == definition.label_of)
                .then_some(id)
        });
        let target = existing.unwrap_or_else(|| self.core.effects.alloc(definition.clone()));
        self.effect_instances.insert(key, target);
        for operation in &mut definition.ops {
            for param in &mut operation.params {
                *param = self.substitute(
                    function,
                    *param,
                    arguments,
                    span,
                    &mut HashMap::new(),
                    &mut HashSet::new(),
                    depth + 1,
                )?;
            }
            operation.ret = self.substitute(
                function,
                operation.ret,
                arguments,
                span,
                &mut HashMap::new(),
                &mut HashSet::new(),
                depth + 1,
            )?;
        }
        self.core.effects[target] = definition;
        Ok(target)
    }
}
