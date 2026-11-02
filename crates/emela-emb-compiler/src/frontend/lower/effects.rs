//! Evidence is an ordinary managed value; lexical providers only select the table.
use super::*;

impl Context<'_> {
    pub(super) fn current_evidence(&mut self, builder: &mut Builder) -> b::VarId {
        if let Some(evidence) = self.evidence {
            return evidence;
        }
        self.extend_evidence(builder, None, &[])
    }
    fn extend_evidence(
        &mut self,
        builder: &mut Builder,
        base: Option<b::VarId>,
        bindings: &[(b::EffectId, b::VarId)],
    ) -> b::VarId {
        let evidence = self.var("provided_evidence", Ty::Compound(b::TypeId::EVIDENCE));
        self.statement(
            builder,
            b::Stmt::Let {
                dst: Some(evidence),
                value: b::Value::EvExtend {
                    base,
                    bindings: bindings.to_vec(),
                },
                on_defect: None,
                on_suspend: None,
            },
        );
        evidence
    }
    pub(super) fn use_effect(
        &mut self,
        builder: &mut Builder,
        effect: b::EffectId,
        result: Ty,
    ) -> Option<b::Operand> {
        let ev = self.current_evidence(builder);
        self.value(builder, b::Value::EvLookup { ev, effect }, result)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn operation_call(
        &mut self,
        builder: &mut Builder,
        handler: &ir::Expr,
        effect: b::EffectId,
        op: usize,
        args: &[ir::Expr],
        order: &[usize],
        result: Ty,
    ) -> Option<b::Operand> {
        let b::Operand::Var(handler) = self.expression(builder, handler, false)? else {
            unreachable!("handler is a variable")
        };
        let mut evaluated = Vec::new();
        for argument in args {
            let value = self.expression(builder, argument, false);
            if builder.term.is_some() {
                return None;
            }
            evaluated.push(value);
        }
        let operation = &self.core.effects[effect].ops[op];
        self.value(
            builder,
            b::Value::Call {
                callee: b::Callee::Op {
                    handler,
                    effect,
                    op,
                },
                args: order
                    .iter()
                    .filter_map(|&index| evaluated[index].clone())
                    .collect(),
                suspends: operation.suspend && self.current_suspends,
                on_fail: operation.fails.then(|| self.failure_edge()),
            },
            result,
        )
    }
    pub(super) fn handler_value(
        &mut self,
        builder: &mut Builder,
        name: &str,
        effect: b::EffectId,
        operations: &[ir::Expr],
        result: Ty,
    ) -> Option<b::Operand> {
        let mut ops = Vec::new();
        for operation in operations {
            let b::Operand::Var(operation) = self.expression(builder, operation, false)? else {
                unreachable!("handler operation is a function value")
            };
            ops.push(operation);
        }
        self.value(
            builder,
            b::Value::Handler {
                name: name.into(),
                effect,
                ops,
            },
            result,
        )
    }
    pub(super) fn provide(
        &mut self,
        builder: &mut Builder,
        base: Option<&ir::Expr>,
        bindings: &[(b::EffectId, ir::Expr)],
        body: &ir::Block,
        tail: bool,
    ) -> Option<b::Operand> {
        let saved = self.evidence;
        let base = if let Some(base) = base {
            let value = self.expression(builder, base, false);
            if builder.term.is_some() {
                return None;
            }
            let Some(b::Operand::Var(value)) = value else {
                unreachable!("provided base is evidence")
            };
            Some(value)
        } else {
            saved
        };
        let mut provided = Vec::new();
        for (effect, value) in bindings {
            let value = self.expression(builder, value, false);
            if builder.term.is_some() {
                self.evidence = saved;
                return None;
            }
            let Some(b::Operand::Var(handler)) = value else {
                unreachable!("provider is a handler variable")
            };
            provided.push((*effect, handler));
        }
        self.evidence = Some(self.extend_evidence(builder, base, &provided));
        let result = self.block_into(builder, body, tail);
        self.evidence = saved;
        result
    }
    pub(super) fn with_expression(
        &mut self,
        builder: &mut Builder,
        bindings: &[ir::WithBinding],
        body: &ir::Block,
        result: Ty,
    ) -> Option<b::Operand> {
        let Some((error, handler)) = self.escape else {
            return self.with_bindings(builder, bindings, body, result);
        };
        if !bindings
            .iter()
            .any(|binding| binding.cleanup.is_some() || binding.retire.is_some())
        {
            return self.with_bindings(builder, bindings, body, result);
        }
        // Leave every acquired capability before entering an enclosing escape arm. A
        // per-call Handle edge would otherwise execute that arm inside this Protect.
        let failed = self.function.labels.alloc(b::LabelData {
            hint: "with_failed".into(),
        });
        let success = self.function.labels.alloc(b::LabelData {
            hint: "with_result".into(),
        });
        let landing = self.function.seqs.alloc(b::Seq {
            stmts: vec![],
            term: b::Term::Break {
                label: failed,
                value: Some(error.into()),
            },
        });
        self.escape = Some((error, landing));
        let mut protected = Builder {
            statements: vec![],
            term: None,
        };
        let value = self.with_bindings(&mut protected, bindings, body, result);
        self.escape = Some((error, handler));
        let protected = self.function.seqs.alloc(b::Seq {
            stmts: protected.statements,
            term: protected.term.unwrap_or(b::Term::Break {
                label: success,
                value,
            }),
        });
        let failure = self.function.stmts.alloc(b::Stmt::Block {
            dst: Some(error),
            label: failed,
            body: protected,
        });
        let mut after_cleanup = self.function.seqs[handler].clone();
        after_cleanup.stmts.insert(0, failure);
        let body = self.function.seqs.alloc(after_cleanup);
        let dst = (!matches!(result, Ty::Unit | Ty::Never))
            .then(|| self.var("with_escaped_result", result));
        self.statement(
            builder,
            b::Stmt::Block {
                dst,
                label: success,
                body,
            },
        );
        if result == Ty::Never {
            builder.term = Some(b::Term::Unreachable);
        }
        dst.map(b::Operand::Var)
    }
    pub(super) fn with_bindings(
        &mut self,
        builder: &mut Builder,
        bindings: &[ir::WithBinding],
        body: &ir::Block,
        result: Ty,
    ) -> Option<b::Operand> {
        let Some((binding, remaining)) = bindings.split_first() else {
            return self.block_into(builder, body, false);
        };
        let value = self.expression(builder, &binding.value, false);
        if builder.term.is_some() {
            return None;
        }
        let value = value.unwrap_or(b::Const::Unit.into());
        self.locals.insert(binding.local.id, value.clone());
        let saved = self.evidence;
        // Release dependencies use the acquisition's surrounding evidence, even when this
        // capability shadows its own effect inside the body.
        let mut cleanup_builder = Builder {
            statements: vec![],
            term: None,
        };
        if let Some(name) = &binding.retire {
            self.statement(
                &mut cleanup_builder,
                b::Stmt::Let {
                    dst: None,
                    value: b::Value::Runtime {
                        op: b::RuntimeOp::Retire,
                        args: vec![value.clone(), b::Const::String(name.clone()).into()],
                    },
                    on_defect: None,
                    on_suspend: None,
                },
            );
        }
        if let Some(cleanup) = &binding.cleanup {
            self.expression(&mut cleanup_builder, cleanup, false);
        }
        if !binding.effects.is_empty() {
            let b::Operand::Var(handler) = value else {
                unreachable!("provided binding is a handler")
            };
            let provided: Vec<_> = binding
                .effects
                .iter()
                .map(|&effect| (effect, handler))
                .collect();
            self.evidence = Some(self.extend_evidence(builder, self.evidence, &provided));
        }
        if binding.cleanup.is_none() && binding.retire.is_none() {
            let value = self.with_bindings(builder, remaining, body, result);
            self.evidence = saved;
            return value;
        }
        let mut body_builder = Builder {
            statements: vec![],
            term: None,
        };
        let value = self.with_bindings(&mut body_builder, remaining, body, result);
        self.evidence = saved;
        let body = self.function.seqs.alloc(b::Seq {
            stmts: body_builder.statements,
            term: body_builder.term.unwrap_or(b::Term::Yield(value)),
        });
        let cleanup = self.function.seqs.alloc(b::Seq {
            stmts: cleanup_builder.statements,
            term: cleanup_builder.term.unwrap_or(b::Term::Yield(None)),
        });
        let dst =
            (!matches!(result, Ty::Unit | Ty::Never)).then(|| self.var("with_result", result));
        self.statement(builder, b::Stmt::Protect { dst, body, cleanup });
        if result == Ty::Never {
            builder.term = Some(b::Term::Unreachable);
        }
        dst.map(b::Operand::Var)
    }
}
