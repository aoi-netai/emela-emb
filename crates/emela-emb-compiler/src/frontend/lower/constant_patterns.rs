use super::*;

pub(super) fn allocate_structural_equality(
    input: &ir::Program,
    program: &mut b::Program,
) -> HashMap<b::TypeId, b::FnId> {
    fn pattern(pat: &ir::Pattern, types: &mut HashSet<b::TypeId>) {
        match &pat.kind {
            ir::PatternKind::Constant {
                value, comparison, ..
            } => {
                if let Some(ir::ConstantEquality::Structural(ty)) = comparison {
                    types.insert(*ty);
                }
                expression(value, types);
            }
            ir::PatternKind::ListCons { head, tail } => {
                pattern(head, types);
                pattern(tail, types);
            }
            ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
                for field in fields {
                    pattern(field, types);
                }
            }
            _ => {}
        }
    }
    fn expression(value: &ir::Expr, types: &mut HashSet<b::TypeId>) {
        use ir::ExprKind as E;
        match &value.kind {
            E::List { elements, tail } => {
                for element in elements {
                    expression(element, types);
                }
                if let Some(tail) = tail {
                    expression(tail, types);
                }
            }
            E::Call { args, .. }
            | E::Make { args, .. }
            | E::TraitCall { args, .. }
            | E::Builtin { args, .. }
            | E::Concat(args)
            | E::Tuple(args) => {
                for arg in args {
                    expression(arg, types);
                }
            }
            E::Field { base, .. }
            | E::Tag(base)
            | E::Fail(base)
            | E::Unary { value: base, .. }
            | E::Assert(base) => expression(base, types),
            E::Binary { left, right, .. } => {
                expression(left, types);
                expression(right, types);
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                expression(condition, types);
                block(then, types);
                block(else_, types);
            }
            E::Match { value, arms } | E::Escape { value, arms, .. } => {
                expression(value, types);
                for arm in arms {
                    pattern(&arm.pattern, types);
                    if let Some(guard) = &arm.guard {
                        expression(guard, types);
                    }
                    expression(&arm.value, types);
                }
            }
            E::Block(value) => block(value, types),
            E::Local(_) | E::Literal(_) => {}
        }
    }
    fn block(value: &ir::Block, types: &mut HashSet<b::TypeId>) {
        for statement in &value.statements {
            match statement {
                ir::Statement::Pattern {
                    pattern: value_pattern,
                    value,
                } => {
                    pattern(value_pattern, types);
                    expression(value, types);
                }
                ir::Statement::Expr(value) => expression(value, types),
            }
        }
    }
    let mut types = HashSet::new();
    for function in &input.functions {
        if let Some(body) = &function.body {
            block(body, &mut types);
        }
    }
    let mut types: Vec<_> = types.into_iter().collect();
    types.sort_by_key(|ty| ty.index());
    types
        .into_iter()
        .map(|ty| {
            let mut function = b::Function::new(&format!("__constant_pattern_eq_{}", ty.index()));
            function.ret = b::TypeId::BOOL;
            function.derivation = Some(crate::DerivedOperation::Eq);
            function.external = Some(b::External { bindings: vec![] });
            for name in ["left", "right"] {
                let param = function.vars.alloc(b::VarData {
                    hint: name.into(),
                    ty,
                    repr: repr(super::super::data::frontend_type(ty), &program.core),
                });
                function.params.push(param);
            }
            (ty, program.functions.alloc(function))
        })
        .collect()
}
impl Context<'_> {
    pub(super) fn constant_pattern(
        &mut self,
        pattern: &ir::Pattern,
        value: &ir::Expr,
        comparison: &Option<ir::ConstantEquality>,
        subject: b::Operand,
        steps: &mut Vec<PatternStep>,
    ) {
        let mut builder = Builder {
            statements: vec![],
            term: None,
        };
        let expected = self.expression(&mut builder, value, false);
        steps.extend(builder.statements.into_iter().map(PatternStep::Statement));
        if let Some(term) = builder.term {
            steps.push(PatternStep::Terminal(term));
            return;
        }
        let mut subject = subject;
        if let b::Operand::Var(error) = subject {
            if self.function.vars[error].ty == b::TypeId::ANY && pattern.ty != Ty::Any {
                if let b::Type::Enum(enum_id, _) = self.core.ty(ty(pattern.ty)) {
                    let condition = self.pattern_value(
                        steps,
                        b::Value::IsError {
                            error,
                            enum_id: *enum_id,
                            error_type: ty(pattern.ty),
                        },
                        Ty::Bool,
                    );
                    steps.push(PatternStep::Test(condition));
                    subject = self.pattern_value(
                        steps,
                        b::Value::AsError {
                            error,
                            enum_id: *enum_id,
                            error_type: ty(pattern.ty),
                        },
                        pattern.ty,
                    );
                }
            }
        }
        let operands = if pattern.ty == Ty::Unit {
            vec![]
        } else {
            vec![subject, expected.expect("non-Unit constant value")]
        };
        let call = match comparison
            .as_ref()
            .expect("constant pattern equality was specialized")
        {
            ir::ConstantEquality::Structural(ty) => b::Value::Call {
                callee: b::Callee::Direct(self.structural_equality[ty]),
                args: operands,
                suspends: false,
                on_fail: None,
            },
            ir::ConstantEquality::Function(target) => match self.intrinsics.get(target) {
                Some(op) => b::Value::Builtin {
                    op: builtin(*op),
                    args: operands,
                },
                None => b::Value::Call {
                    callee: b::Callee::Direct(self.ids[target]),
                    args: operands,
                    suspends: false,
                    on_fail: self.failing.contains(target).then(|| self.failure_edge()),
                },
            },
        };
        let condition = self.pattern_value(steps, call, Ty::Bool);
        steps.push(PatternStep::Test(condition));
    }
}
