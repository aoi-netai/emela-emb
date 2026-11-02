//! Recoverable failures leave an escape expression through its enclosing result block.
use super::*;

pub(super) fn allocate_raises(
    input: &ir::Program,
    program: &mut b::Program,
) -> HashMap<b::TypeId, b::FnId> {
    fn collect(expression: &ir::Expr, types: &mut std::collections::HashSet<b::TypeId>) {
        use ir::ExprKind as E;
        match &expression.kind {
            E::FnRef { .. } => {}
            E::Lambda { body, .. } => block(body, types),
            E::Apply { callee, args, .. } => {
                collect(callee, types);
                for argument in args {
                    collect(argument, types);
                }
            }
            E::List { elements, tail } => {
                for element in elements {
                    collect(element, types);
                }
                if let Some(tail) = tail {
                    collect(tail, types);
                }
            }
            E::Fail(value) => {
                types.insert(ty(value.ty));
                collect(value, types);
            }
            E::Escape { value, arms, .. } => {
                types.insert(b::TypeId::ANY);
                collect(value, types);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        collect(guard, types);
                    }
                    collect(&arm.value, types);
                }
            }
            E::Call { args, .. }
            | E::Make { args, .. }
            | E::TraitCall { args, .. }
            | E::Builtin { args, .. }
            | E::Concat(args)
            | E::Tuple(args) => {
                for arg in args {
                    collect(arg, types);
                }
            }
            E::Field { base, .. }
            | E::Coerce(base)
            | E::Tag(base)
            | E::Unary { value: base, .. }
            | E::Assert(base) => collect(base, types),
            E::Binary { left, right, .. } => {
                collect(left, types);
                collect(right, types);
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                collect(condition, types);
                block(then, types);
                block(else_, types);
            }
            E::Match { value, arms } => {
                collect(value, types);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        collect(guard, types);
                    }
                    collect(&arm.value, types);
                }
            }
            E::Block(value) => block(value, types),
            E::Local(_) | E::Literal(_) => {}
        }
    }
    fn block(block: &ir::Block, types: &mut std::collections::HashSet<b::TypeId>) {
        for statement in &block.statements {
            match statement {
                ir::Statement::Pattern { value, .. } | ir::Statement::Expr(value) => {
                    collect(value, types)
                }
            }
        }
    }
    let mut types = std::collections::HashSet::new();
    for function in &input.functions {
        if let Some(body) = &function.body {
            block(body, &mut types);
        }
    }
    let mut types: Vec<_> = types.into_iter().collect();
    types.sort_by_key(|ty| ty.index());
    types
        .into_iter()
        .map(|error_type| {
            let mut function = b::Function::new(&format!("__raise_{}", error_type.index()));
            function.ret = b::TypeId::NEVER;
            function.fails = true;
            let parameter = function.vars.alloc(b::VarData {
                hint: "error".into(),
                ty: error_type,
                repr: repr(super::super::data::frontend_type(error_type), &program.core),
            });
            function.params.push(parameter);
            function.body = Some(function.seqs.alloc(b::Seq {
                stmts: vec![],
                term: b::Term::Fail(parameter.into()),
            }));
            let id = program.functions.alloc(function);
            (error_type, id)
        })
        .collect()
}

impl Context<'_> {
    pub(super) fn failure_edge(&self) -> b::OnFail {
        match self.escape {
            Some((error, body)) => b::OnFail::Handle { error, body },
            None => b::OnFail::Propagate,
        }
    }
    pub(super) fn raise(
        &mut self,
        builder: &mut Builder,
        error: b::Operand,
        error_type: b::TypeId,
    ) {
        let call = b::Value::Call {
            callee: b::Callee::Direct(self.raises[&error_type]),
            args: vec![error],
            suspends: false,
            on_fail: Some(self.failure_edge()),
        };
        self.value(builder, call, Ty::Never);
    }
    fn unmatched_error(&mut self, error: b::VarId) -> b::SeqId {
        if self.escape.is_some() || self.function.fails {
            let mut builder = Builder {
                statements: vec![],
                term: None,
            };
            self.raise(&mut builder, error.into(), b::TypeId::ANY);
            self.function.seqs.alloc(b::Seq {
                stmts: builder.statements,
                term: builder.term.expect("raise is terminal"),
            })
        } else {
            // Exhaustiveness is established by the checker. An unexpected error from an
            // invalid foreign boundary cannot become an unchecked propagation edge.
            self.function.seqs.alloc(b::Seq {
                stmts: vec![],
                term: b::Term::Defect(b::Const::String("unhandled error".into()).into()),
            })
        }
    }
    pub(super) fn escape_expression(
        &mut self,
        builder: &mut Builder,
        value: &ir::Expr,
        error: &ir::Local,
        arms: &[ir::Arm],
        result_type: Ty,
    ) -> Option<b::Operand> {
        let label = self.function.labels.alloc(b::LabelData {
            hint: "escape_result".into(),
        });
        let dst = if matches!(result_type, Ty::Unit | Ty::Never) {
            None
        } else {
            Some(self.var("escaped", result_type))
        };
        let error_var = self.var("caught_error", Ty::Any);
        self.locals.insert(error.id, b::Operand::Var(error_var));
        let mut failure = self.unmatched_error(error_var);
        // Handler arms are lowered before installing this escape's edge, so errors in
        // a handler go to an enclosing escape or the function's declared failure channel.
        for arm in arms.iter().rev() {
            let mut steps = Vec::new();
            self.prepare_pattern(&arm.pattern, b::Operand::Var(error_var), &mut steps);
            let mut arm_builder = Builder {
                statements: vec![],
                term: None,
            };
            let result = self.expression(&mut arm_builder, &arm.value, false);
            let mut success = self.function.seqs.alloc(b::Seq {
                stmts: arm_builder.statements,
                term: arm_builder.term.unwrap_or(b::Term::Break {
                    label,
                    value: result,
                }),
            });
            if let Some(guard) = &arm.guard {
                let mut guard_builder = Builder {
                    statements: vec![],
                    term: None,
                };
                let condition = self.expression(&mut guard_builder, guard, false);
                if let Some(term) = guard_builder.term {
                    success = self.function.seqs.alloc(b::Seq {
                        stmts: guard_builder.statements,
                        term,
                    });
                } else {
                    let choice = self.choose(
                        condition.expect("Bool escape guard"),
                        success,
                        failure,
                        Ty::Never,
                        false,
                    );
                    guard_builder.statements.reverse();
                    success = self.prepend(&mut guard_builder.statements, choice);
                }
            }
            failure = self.pattern_steps(steps, success, failure, Ty::Never, false);
        }
        let previous = self.escape.replace((error_var, failure));
        let mut value_builder = Builder {
            statements: vec![],
            term: None,
        };
        let normal = self.expression(&mut value_builder, value, false);
        self.escape = previous;
        let body = self.function.seqs.alloc(b::Seq {
            stmts: value_builder.statements,
            term: value_builder.term.unwrap_or(b::Term::Yield(normal)),
        });
        self.statement(builder, b::Stmt::Block { dst, label, body });
        if result_type == Ty::Never {
            builder.term = Some(b::Term::Unreachable);
        }
        dst.map(b::Operand::Var)
    }
}
