//! Scalar normalization from the source-aware IR to the independent backend contract.
use super::{
    ast::{Binary, Unary},
    ir::{self, DefinitionId, LocalId},
    types::ValueType as Ty,
};
use crate::{contract::ir as b, sources::Intrinsic};
use std::collections::{HashMap, HashSet};

#[path = "lower/constant_patterns.rs"]
mod constant_patterns;
#[path = "lower/effects.rs"]
mod effects;
#[path = "lower/failures.rs"]
mod failures;

pub(crate) fn lower(input: &ir::Program) -> b::Program {
    let mut program = b::Program {
        core: input.core.clone(),
        functions: Default::default(),
        error_shows: vec![],
        const_function_groups: vec![],
        has_scopes: false,
    };
    let mut ids = HashMap::new();
    for function in &input.functions {
        if function.intrinsic.is_none() {
            debug_assert!(function.derivation.is_none());
            debug_assert!(function.origin.is_none() || function.constant);
            let mut target = b::Function::new(&function.name);
            target.constant = function.constant;
            target.fails = !function.signature.sets.errors.is_empty();
            target.suspends = !function.immediate
                && super::function_abi::effects_abi(&input.core, &function.signature.sets.effects)
                    .1;
            target.exported = function.exported;
            target.export_symbol = function
                .exported
                .then(|| function.export_symbol.clone())
                .flatten();
            target.ret = ty(function.signature.ret.expect("checked return type"));
            if let Some(symbol) = &function.external {
                target.external = Some(b::External {
                    bindings: vec![b::ExternalBinding {
                        target: "c".into(),
                        host: b::HostRef::Global(symbol.clone()),
                    }],
                });
            }
            ids.insert(function.id, program.functions.alloc(target));
        }
    }
    program.error_shows = input
        .functions
        .iter()
        .filter_map(|function| function.error_show.map(|error| (error, ids[&function.id])))
        .collect();
    let raises = failures::allocate_raises(input, &mut program);
    let structural_equality = constant_patterns::allocate_structural_equality(input, &mut program);
    let failing: HashSet<_> = input
        .functions
        .iter()
        .filter_map(|function| (!function.signature.sets.errors.is_empty()).then_some(function.id))
        .collect();
    let effectful: HashSet<_> = input
        .functions
        .iter()
        .filter_map(|function| {
            super::function_abi::effects_abi(&input.core, &function.signature.sets.effects)
                .0
                .then_some(function.id)
        })
        .collect();
    let suspending = input
        .functions
        .iter()
        .filter_map(|function| {
            (!function.immediate
                && super::function_abi::effects_abi(&input.core, &function.signature.sets.effects)
                    .1)
                .then_some(function.id)
        })
        .collect();
    let intrinsics = input
        .functions
        .iter()
        .filter_map(|f| f.intrinsic.map(|op| (f.id, op)))
        .collect();
    for function in &input.functions {
        if function.intrinsic.is_some() {
            continue;
        }
        let id = ids[&function.id];
        let target = &mut program.functions[id];
        let current_suspends = target.suspends;
        let mut context = Context {
            function: target,
            current: function.id,
            ids: &ids,
            intrinsics: &intrinsics,
            locals: HashMap::new(),
            core: &program.core,
            failing: &failing,
            effectful: &effectful,
            suspending: &suspending,
            current_suspends,
            evidence: None,
            raises: &raises,
            structural_equality: &structural_equality,
            escape: None,
            closure_depth: 0,
        };
        for param in &function.params {
            if param.ty == Ty::Unit {
                context
                    .locals
                    .insert(param.id, b::Operand::Const(b::Const::Unit));
                continue;
            }
            let var = context.var(&param.name, param.ty);
            context.function.params.push(var);
            context.locals.insert(param.id, b::Operand::Var(var));
        }
        if effectful.contains(&function.id) {
            let evidence = context.var("evidence", Ty::Compound(b::TypeId::EVIDENCE));
            context.function.params.push(evidence);
            context.evidence = Some(evidence);
        }
        if let Some(body) = &function.body {
            context.function.body = Some(context.block(body, true));
        }
    }
    program
}
enum PatternStep {
    Statement(b::StmtId),
    Test(b::Operand),
    Terminal(b::Term),
}
struct Builder {
    statements: Vec<b::StmtId>,
    term: Option<b::Term>,
}
struct Context<'a> {
    function: &'a mut b::Function,
    current: DefinitionId,
    ids: &'a HashMap<DefinitionId, b::FnId>,
    intrinsics: &'a HashMap<DefinitionId, Intrinsic>,
    locals: HashMap<LocalId, b::Operand>,
    core: &'a b::Definitions,
    failing: &'a HashSet<DefinitionId>,
    effectful: &'a HashSet<DefinitionId>,
    suspending: &'a HashSet<DefinitionId>,
    current_suspends: bool,
    evidence: Option<b::VarId>,
    raises: &'a failures::RaisedFunctions,
    structural_equality: &'a HashMap<b::TypeId, b::FnId>,
    escape: Option<(b::VarId, b::SeqId)>,
    closure_depth: usize,
}
impl Context<'_> {
    fn var(&mut self, name: &str, value_type: Ty) -> b::VarId {
        self.function.vars.alloc(b::VarData {
            hint: name.into(),
            repr: repr(value_type, self.core),
            ty: ty(value_type),
        })
    }
    fn statement(&mut self, builder: &mut Builder, statement: b::Stmt) {
        builder
            .statements
            .push(self.function.stmts.alloc(statement));
    }
    fn value(
        &mut self,
        builder: &mut Builder,
        value: b::Value,
        value_type: Ty,
    ) -> Option<b::Operand> {
        let dst = if matches!(value_type, Ty::Unit | Ty::Never) {
            None
        } else {
            Some(self.var("value", value_type))
        };
        self.statement(
            builder,
            b::Stmt::Let {
                dst,
                value,
                on_defect: None,
                on_suspend: None,
            },
        );
        if value_type == Ty::Never {
            builder.term = Some(b::Term::Unreachable);
        }
        dst.map(b::Operand::Var)
    }
    fn block(&mut self, block: &ir::Block, tail: bool) -> b::SeqId {
        debug_assert!(block.span.start <= block.span.end);
        let mut builder = Builder {
            statements: Vec::new(),
            term: None,
        };
        let value = self.block_into(&mut builder, block, tail);
        let term = builder.term.unwrap_or({
            if tail {
                b::Term::Return(value)
            } else {
                b::Term::Yield(value)
            }
        });
        self.function.seqs.alloc(b::Seq {
            stmts: builder.statements,
            term,
        })
    }
    fn block_into(
        &mut self,
        builder: &mut Builder,
        block: &ir::Block,
        tail: bool,
    ) -> Option<b::Operand> {
        let mut result = None;
        for (index, statement) in block.statements.iter().enumerate() {
            if builder.term.is_some() {
                break;
            }
            match statement {
                ir::Statement::Pattern { pattern, value } => {
                    let value = self.expression(builder, value, false);
                    if builder.term.is_some() {
                        break;
                    }
                    let mut steps = Vec::new();
                    self.prepare_pattern(
                        pattern,
                        value.unwrap_or(b::Operand::Const(b::Const::Unit)),
                        &mut steps,
                    );
                    for step in steps {
                        match step {
                            PatternStep::Statement(id) => builder.statements.push(id),
                            PatternStep::Test(_) => unreachable!("irrefutable binding pattern"),
                            PatternStep::Terminal(term) => {
                                builder.term = Some(term);
                                break;
                            }
                        }
                    }
                    result = None;
                }
                ir::Statement::Expr(expression) => {
                    result = self.expression(
                        builder,
                        expression,
                        tail && index + 1 == block.statements.len(),
                    );
                }
            }
        }
        result
    }
    fn expression(
        &mut self,
        builder: &mut Builder,
        expression: &ir::Expr,
        tail: bool,
    ) -> Option<b::Operand> {
        use ir::ExprKind as E;
        match &expression.kind {
            E::Evidence => Some(self.current_evidence(builder).into()),
            E::Fiber { op, args, errors } => {
                let mut values = Vec::new();
                for argument in args {
                    let value = self.expression(builder, argument, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    values.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                if *op == b::FiberOp::CauseRaise && errors.is_empty() {
                    return self.raise_empty_cause(builder, values);
                }
                self.value(
                    builder,
                    b::Value::Fiber {
                        op: *op,
                        args: values,
                        on_fail: (!errors.is_empty()).then(|| self.failure_edge()),
                    },
                    expression.ty,
                )
            }
            E::Use { effect } => self.use_effect(builder, *effect, expression.ty),
            E::OpCall {
                handler,
                effect,
                op,
                args,
                order,
                ..
            } => self.operation_call(builder, handler, *effect, *op, args, order, expression.ty),
            E::Handler {
                name,
                effect,
                operations,
                ..
            } => self.handler_value(builder, name, *effect, operations, expression.ty),
            E::Provide {
                base,
                bindings,
                body,
            } => self.provide(builder, base.as_deref(), bindings, body, tail),
            E::With { bindings, body } => {
                self.with_expression(builder, bindings, body, expression.ty)
            }
            E::Coerce(value) => {
                let operand = self.expression(builder, value, false)?;
                if expression.ty == value.ty {
                    return Some(operand);
                }
                let source = match operand {
                    b::Operand::Var(var) => var,
                    b::Operand::Const(constant) => {
                        let var = self.var("coercion_source", value.ty);
                        self.statement(
                            builder,
                            b::Stmt::Let {
                                dst: Some(var),
                                value: b::Value::Const(constant),
                                on_defect: None,
                                on_suspend: None,
                            },
                        );
                        var
                    }
                };
                self.value(builder, b::Value::Coerce(source), expression.ty)
            }
            E::FnRef { target, .. } => {
                self.value(builder, b::Value::FnRef(self.ids[target]), expression.ty)
            }
            E::Lambda {
                params,
                captures,
                body,
            } => {
                let mut captured: Vec<_> = captures
                    .iter()
                    .filter_map(|id| match self.locals.get(id).expect("checked capture") {
                        b::Operand::Var(var) => Some(*var),
                        b::Operand::Const(_) => None,
                    })
                    .collect();
                let saved_locals = self.locals.clone();
                let saved_escape = self.escape.take();
                let saved_evidence = self.evidence;
                let b::Type::Fn(ft) = self.core.ty(ty(expression.ty)) else {
                    unreachable!("checked lambda type")
                };
                let ft = ft.clone();
                let saved_suspends = std::mem::replace(&mut self.current_suspends, ft.suspends);
                // Ordinary effectful closures receive the caller's table. Handler operation
                // closures keep the table of the surrounding acquisition instead.
                if !ft.ev {
                    if let Some(evidence) = saved_evidence {
                        if !captured.contains(&evidence) {
                            captured.push(evidence);
                        }
                    }
                }
                self.closure_depth += 1;
                let mut parameters = Vec::new();
                for parameter in params {
                    let value = if parameter.ty == Ty::Unit {
                        b::Operand::Const(b::Const::Unit)
                    } else {
                        let var = self.var(&parameter.name, parameter.ty);
                        parameters.push(var);
                        b::Operand::Var(var)
                    };
                    self.locals.insert(parameter.id, value);
                }
                if ft.ev {
                    let evidence = self.var("lambda_evidence", Ty::Compound(b::TypeId::EVIDENCE));
                    parameters.push(evidence);
                    self.evidence = Some(evidence);
                }
                let body = self.block(body, true);
                self.current_suspends = saved_suspends;
                self.evidence = saved_evidence;
                self.closure_depth -= 1;
                self.locals = saved_locals;
                self.escape = saved_escape;
                self.value(
                    builder,
                    b::Value::Closure(Box::new(b::Closure {
                        params: parameters,
                        captures: captured,
                        body,
                        suspends: ft.suspends,
                    })),
                    expression.ty,
                )
            }
            E::Apply {
                callee,
                args,
                order,
            } => {
                let b::Operand::Var(target) = self.expression(builder, callee, false)? else {
                    unreachable!("function value is a variable")
                };
                let mut evaluated = Vec::new();
                for argument in args {
                    let value = self.expression(builder, argument, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    evaluated.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                let b::Type::Fn(ft) = self.core.ty(ty(callee.ty)) else {
                    unreachable!("checked callable type")
                };
                let (ev, suspends, fails) = (ft.ev, ft.suspends, ft.fails);
                let mut operands: Vec<_> = order
                    .iter()
                    .filter(|&&index| args[index].ty != Ty::Unit)
                    .map(|&index| evaluated[index].clone())
                    .collect();
                if ev {
                    operands.push(self.current_evidence(builder).into());
                }
                self.value(
                    builder,
                    b::Value::Call {
                        callee: b::Callee::Indirect(target),
                        args: operands,
                        suspends,
                        on_fail: fails.then(|| self.failure_edge()),
                    },
                    expression.ty,
                )
            }
            E::Fail(value) => {
                let error = self.expression(builder, value, false);
                if builder.term.is_some() {
                    return None;
                }
                self.raise(builder, error.expect("checked error value"), ty(value.ty));
                None
            }
            E::Escape { value, error, arms } => {
                self.escape_expression(builder, value, error, arms, expression.ty)
            }
            E::TraitCall { .. } => {
                unreachable!("trait methods are resolved before backend lowering")
            }
            E::Tag(value) => {
                let b::Operand::Var(value) = self.expression(builder, value, false)? else {
                    unreachable!("tagged enum is a variable")
                };
                self.value(builder, b::Value::Tag(value), expression.ty)
            }
            E::Builtin { op, args } => {
                let mut values = Vec::new();
                for arg in args {
                    let value = self.expression(builder, arg, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    values.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                if *op == b::Builtin::Panic {
                    builder.term = Some(b::Term::Defect(values[0].clone()));
                    return None;
                }
                if *op == b::Builtin::Todo {
                    builder.term = Some(b::Term::Defect(b::Operand::Const(b::Const::String(
                        "not yet implemented".into(),
                    ))));
                    return None;
                }
                let value = match op {
                    b::Builtin::ShowNested | b::Builtin::ShowString => b::Value::Runtime {
                        op: b::RuntimeOp::ShowString,
                        args: values,
                    },
                    b::Builtin::DebugPrint => b::Value::Runtime {
                        op: b::RuntimeOp::DebugPrint,
                        args: values,
                    },
                    op => b::Value::Builtin {
                        op: *op,
                        args: values,
                    },
                };
                self.value(builder, value, expression.ty)
            }
            E::Match { value, arms } => {
                let value = self.expression(builder, value, false);
                if builder.term.is_some() {
                    return None;
                }
                let value = value.unwrap_or(b::Operand::Const(b::Const::Unit));
                let mut failure = self.function.seqs.alloc(b::Seq {
                    stmts: vec![],
                    term: b::Term::Unreachable,
                });
                for arm in arms.iter().rev() {
                    let mut steps = Vec::new();
                    self.prepare_pattern(&arm.pattern, value.clone(), &mut steps);
                    let body = ir::Block {
                        statements: vec![ir::Statement::Expr(arm.value.clone())],
                        ty: arm.value.ty,
                        span: arm.value.span,
                    };
                    let mut success = self.block(&body, tail);
                    if let Some(guard) = &arm.guard {
                        let mut guard_builder = Builder {
                            statements: Vec::new(),
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
                                condition.expect("Bool guard"),
                                success,
                                failure,
                                expression.ty,
                                tail,
                            );
                            guard_builder.statements.reverse();
                            success = self.prepend(&mut guard_builder.statements, choice);
                        }
                    }
                    failure = self.pattern_steps(steps, success, failure, expression.ty, tail);
                }
                let sequence = self.function.seqs[failure].clone();
                builder.statements.extend(sequence.stmts);
                match sequence.term {
                    b::Term::Yield(value) => value,
                    term => {
                        builder.term = Some(term);
                        None
                    }
                }
            }
            E::List { elements, tail } => {
                let mut elems = Vec::new();
                for element in elements {
                    let value = self.expression(builder, element, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    elems.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                let tail = if let Some(tail) = tail {
                    let tail = self.expression(builder, tail, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    tail
                } else {
                    None
                };
                self.value(builder, b::Value::List { elems, tail }, expression.ty)
            }
            E::Tuple(elements) => {
                let mut values = Vec::new();
                for element in elements {
                    let value = self.expression(builder, element, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    values.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                self.value(builder, b::Value::Tuple(values), expression.ty)
            }
            E::Make { ctor, args, order } => {
                let mut evaluated = Vec::new();
                for argument in args {
                    let value = self.expression(builder, argument, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    evaluated.push(value.unwrap_or(b::Operand::Const(b::Const::Unit)));
                }
                self.value(
                    builder,
                    b::Value::Make {
                        ctor: *ctor,
                        args: order
                            .iter()
                            .map(|&index| evaluated[index].clone())
                            .collect(),
                    },
                    expression.ty,
                )
            }
            E::Field { base, ctor, index } => {
                let b::Operand::Var(base) = self.expression(builder, base, false)? else {
                    unreachable!("constructed record is bound to a variable")
                };
                self.value(
                    builder,
                    b::Value::Field {
                        base,
                        ctor: *ctor,
                        index: *index,
                    },
                    expression.ty,
                )
            }
            E::Concat(parts) => {
                let mut combined = b::Operand::Const(b::Const::String(String::new()));
                for part in parts {
                    let value = self.expression(builder, part, false)?;
                    combined = self
                        .value(
                            builder,
                            b::Value::Builtin {
                                op: b::Builtin::StringAppend,
                                args: vec![combined, value],
                            },
                            Ty::String,
                        )
                        .expect("String result");
                }
                Some(combined)
            }
            E::Literal(value) => {
                if expression.ty == Ty::Unit {
                    None
                } else {
                    Some(b::Operand::Const(literal(value)))
                }
            }
            E::Local(id) => {
                if expression.ty == Ty::Unit {
                    None
                } else {
                    Some(self.locals[id].clone())
                }
            }
            E::Unary { op, value } => {
                let argument = self.expression(builder, value, false)?;
                self.value(
                    builder,
                    b::Value::Prim {
                        op: if *op == Unary::Neg {
                            b::PrimOp::Neg
                        } else {
                            b::PrimOp::Not
                        },
                        ty: prim_ty(value.ty),
                        args: vec![argument],
                    },
                    expression.ty,
                )
            }
            E::Binary {
                op: Binary::And | Binary::Or,
                left,
                right,
            } => {
                let is_and = matches!(
                    &expression.kind,
                    E::Binary {
                        op: Binary::And,
                        ..
                    }
                );
                let condition = self.expression(builder, left, false)?;
                let right_block = ir::Block {
                    statements: vec![ir::Statement::Expr(right.as_ref().clone())],
                    ty: right.ty,
                    span: right.span,
                };
                let constant_block = ir::Block {
                    statements: vec![ir::Statement::Expr(ir::Expr {
                        kind: E::Literal(ir::Literal::Bool(!is_and)),
                        ty: Ty::Bool,
                        span: expression.span,
                    })],
                    ty: Ty::Bool,
                    span: expression.span,
                };
                let (then, else_) = if is_and {
                    (right_block, constant_block)
                } else {
                    (constant_block, right_block)
                };
                self.branch(builder, condition, &then, &else_, expression.ty, tail)
            }
            E::Binary { op, left, right } => {
                let left_value = self.expression(builder, left, false);
                if builder.term.is_some() {
                    return None;
                }
                let right_value = self.expression(builder, right, false);
                if builder.term.is_some() {
                    return None;
                }
                if left.ty == Ty::Unit {
                    return Some(b::Operand::Const(b::Const::Bool(*op == Binary::Eq)));
                }
                self.value(
                    builder,
                    b::Value::Prim {
                        op: prim_op(*op),
                        ty: prim_ty(left.ty),
                        args: vec![
                            left_value.expect("checked scalar operand"),
                            right_value.expect("checked scalar operand"),
                        ],
                    },
                    expression.ty,
                )
            }
            E::Call {
                target,
                args,
                order,
                ..
            } => {
                let mut evaluated = Vec::new();
                for argument in args {
                    let value = self.expression(builder, argument, false);
                    if builder.term.is_some() {
                        return None;
                    }
                    // Evaluate even Unit arguments before omitting their C values.
                    evaluated.push(value);
                }
                let mut operands: Vec<_> = order
                    .iter()
                    .filter_map(|&index| evaluated[index].clone())
                    .collect();
                if self.effectful.contains(target) {
                    operands.push(self.current_evidence(builder).into());
                }
                if tail
                    && *target == self.current
                    && !self.function.constant
                    && self.escape.is_none()
                    && self.closure_depth == 0
                {
                    builder.term = Some(b::Term::Recur(operands));
                    return None;
                }
                let value = match self.intrinsics.get(target) {
                    Some(op) => b::Value::Builtin {
                        op: builtin(*op),
                        args: operands,
                    },
                    None => b::Value::Call {
                        callee: b::Callee::Direct(self.ids[target]),
                        args: operands,
                        suspends: self.suspending.contains(target),
                        on_fail: self.failing.contains(target).then(|| self.failure_edge()),
                    },
                };
                self.value(builder, value, expression.ty)
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                let condition = self.expression(builder, condition, false)?;
                self.branch(builder, condition, then, else_, expression.ty, tail)
            }
            E::Block(block) => self.block_into(builder, block, tail),
            E::Assert(condition) => {
                let condition = self.expression(builder, condition, false)?;
                let then = self.function.seqs.alloc(b::Seq {
                    stmts: vec![],
                    term: b::Term::Yield(None),
                });
                let else_ = self.function.seqs.alloc(b::Seq {
                    stmts: vec![],
                    term: b::Term::Defect(b::Operand::Const(b::Const::String(
                        "assertion failed".into(),
                    ))),
                });
                self.statement(
                    builder,
                    b::Stmt::If {
                        dst: None,
                        cond: condition,
                        then,
                        else_,
                    },
                );
                None
            }
        }
    }
    fn pattern_value(
        &mut self,
        steps: &mut Vec<PatternStep>,
        value: b::Value,
        value_type: Ty,
    ) -> b::Operand {
        let dst = self.var("pattern", value_type);
        steps.push(PatternStep::Statement(self.function.stmts.alloc(
            b::Stmt::Let {
                dst: Some(dst),
                value,
                on_defect: None,
                on_suspend: None,
            },
        )));
        b::Operand::Var(dst)
    }
    fn prepare_pattern(
        &mut self,
        pattern: &ir::Pattern,
        value: b::Operand,
        steps: &mut Vec<PatternStep>,
    ) {
        use ir::PatternKind as P;
        match &pattern.kind {
            P::Constant {
                value: expected,
                comparison,
                ..
            } => self.constant_pattern(pattern, expected, comparison, value, steps),
            P::Any => {}
            P::Bind(local) => {
                self.locals.insert(local.id, value);
            }
            P::Literal(ir::Literal::Unit) => {}
            P::Literal(lit) => {
                let condition = self.pattern_value(
                    steps,
                    b::Value::Prim {
                        op: b::PrimOp::Eq,
                        ty: prim_ty(pattern.ty),
                        args: vec![value, b::Operand::Const(literal(lit))],
                    },
                    Ty::Bool,
                );
                steps.push(PatternStep::Test(condition));
            }
            P::ListNil | P::ListCons { .. } => {
                let b::Operand::Var(base) = value else {
                    unreachable!("list values are bound to variables")
                };
                let empty = self.pattern_value(steps, b::Value::ListIsEmpty(base), Ty::Bool);
                let condition = if matches!(pattern.kind, P::ListCons { .. }) {
                    self.pattern_value(
                        steps,
                        b::Value::Prim {
                            op: b::PrimOp::Not,
                            ty: b::PrimTy::I32,
                            args: vec![empty],
                        },
                        Ty::Bool,
                    )
                } else {
                    empty
                };
                steps.push(PatternStep::Test(condition));
                if let P::ListCons { head, tail } = &pattern.kind {
                    let value = if head.ty == Ty::Unit {
                        b::Operand::Const(b::Const::Unit)
                    } else {
                        self.pattern_value(steps, b::Value::ListHead(base), head.ty)
                    };
                    self.prepare_pattern(head, value, steps);
                    let value = self.pattern_value(steps, b::Value::ListTail(base), tail.ty);
                    self.prepare_pattern(tail, value, steps);
                }
            }
            P::Tuple(fields) => {
                let base = match value {
                    b::Operand::Var(var) => var,
                    b::Operand::Const(value) => {
                        let b::Operand::Var(var) =
                            self.pattern_value(steps, b::Value::Const(value), pattern.ty)
                        else {
                            unreachable!()
                        };
                        var
                    }
                };
                for (index, field) in fields.iter().enumerate() {
                    let value = if field.ty == Ty::Unit {
                        b::Operand::Const(b::Const::Unit)
                    } else {
                        self.pattern_value(steps, b::Value::TupleGet { base, index }, field.ty)
                    };
                    self.prepare_pattern(field, value, steps);
                }
            }
            P::Constructor { ctor, fields } => {
                let value = if let b::Operand::Var(error) = value {
                    if self.function.vars[error].ty == b::TypeId::ANY {
                        let condition = self.pattern_value(
                            steps,
                            b::Value::IsError {
                                error,
                                enum_id: ctor.enum_id,
                                error_type: ty(pattern.ty),
                            },
                            Ty::Bool,
                        );
                        steps.push(PatternStep::Test(condition));
                        self.pattern_value(
                            steps,
                            b::Value::AsError {
                                error,
                                enum_id: ctor.enum_id,
                                error_type: ty(pattern.ty),
                            },
                            pattern.ty,
                        )
                    } else {
                        b::Operand::Var(error)
                    }
                } else {
                    value
                };
                let base = match value {
                    b::Operand::Var(var) => var,
                    b::Operand::Const(value) => {
                        let b::Operand::Var(var) =
                            self.pattern_value(steps, b::Value::Const(value), pattern.ty)
                        else {
                            unreachable!()
                        };
                        var
                    }
                };
                if self.core.enums[ctor.enum_id].variants.len() > 1 {
                    let tag = self.pattern_value(steps, b::Value::Tag(base), Ty::Int);
                    let condition = self.pattern_value(
                        steps,
                        b::Value::Prim {
                            op: b::PrimOp::Eq,
                            ty: b::PrimTy::I32,
                            args: vec![tag, b::Operand::Const(b::Const::Int(ctor.variant as i32))],
                        },
                        Ty::Bool,
                    );
                    steps.push(PatternStep::Test(condition));
                }
                for (index, field) in fields.iter().enumerate() {
                    let value = if field.ty == Ty::Unit {
                        b::Operand::Const(b::Const::Unit)
                    } else {
                        self.pattern_value(
                            steps,
                            b::Value::Field {
                                base,
                                ctor: *ctor,
                                index,
                            },
                            field.ty,
                        )
                    };
                    self.prepare_pattern(field, value, steps);
                }
            }
        }
    }
    fn prepend(&mut self, statements: &mut Vec<b::StmtId>, body: b::SeqId) -> b::SeqId {
        if statements.is_empty() {
            return body;
        }
        let mut seq = self.function.seqs[body].clone();
        let mut prefix: Vec<_> = statements.drain(..).rev().collect();
        prefix.append(&mut seq.stmts);
        seq.stmts = prefix;
        self.function.seqs.alloc(seq)
    }
    fn choose(
        &mut self,
        condition: b::Operand,
        then: b::SeqId,
        else_: b::SeqId,
        value_type: Ty,
        tail: bool,
    ) -> b::SeqId {
        let dst = if tail || matches!(value_type, Ty::Unit | Ty::Never) {
            None
        } else {
            Some(self.var("matched", value_type))
        };
        let statement = self.function.stmts.alloc(b::Stmt::If {
            dst,
            cond: condition,
            then,
            else_,
        });
        self.function.seqs.alloc(b::Seq {
            stmts: vec![statement],
            term: if tail || value_type == Ty::Never {
                b::Term::Unreachable
            } else {
                b::Term::Yield(dst.map(b::Operand::Var))
            },
        })
    }
    fn pattern_steps(
        &mut self,
        steps: Vec<PatternStep>,
        success: b::SeqId,
        failure: b::SeqId,
        value_type: Ty,
        tail: bool,
    ) -> b::SeqId {
        let mut result = success;
        let mut statements = Vec::new();
        for step in steps.into_iter().rev() {
            match step {
                PatternStep::Statement(id) => statements.push(id),
                PatternStep::Terminal(term) => {
                    statements.clear();
                    result = self.function.seqs.alloc(b::Seq {
                        stmts: vec![],
                        term,
                    });
                }
                PatternStep::Test(condition) => {
                    result = self.prepend(&mut statements, result);
                    result = self.choose(condition, result, failure, value_type, tail);
                }
            }
        }
        self.prepend(&mut statements, result)
    }

    fn branch(
        &mut self,
        builder: &mut Builder,
        condition: b::Operand,
        then: &ir::Block,
        else_: &ir::Block,
        value_type: Ty,
        tail: bool,
    ) -> Option<b::Operand> {
        let then = self.block(then, tail);
        let else_ = self.block(else_, tail);
        let dst = if tail || matches!(value_type, Ty::Unit | Ty::Never) {
            None
        } else {
            Some(self.var("branch", value_type))
        };
        self.statement(
            builder,
            b::Stmt::If {
                dst,
                cond: condition,
                then,
                else_,
            },
        );
        if tail || value_type == Ty::Never {
            builder.term = Some(b::Term::Unreachable);
        }
        dst.map(b::Operand::Var)
    }
}
fn ty(value: Ty) -> b::TypeId {
    match value {
        Ty::Int => b::TypeId::INT,
        Ty::Int64 => b::TypeId::INT64,
        Ty::Float => b::TypeId::FLOAT,
        Ty::String => b::TypeId::STRING,
        Ty::Bool => b::TypeId::BOOL,
        Ty::Unit => b::TypeId::UNIT,
        Ty::Never => b::TypeId::NEVER,
        Ty::Any => b::TypeId::ANY,
        Ty::Compound(id) => id,
    }
}
fn repr(value: Ty, core: &b::Definitions) -> b::Repr {
    match value {
        Ty::Int | Ty::Bool => b::Repr::I32,
        Ty::Int64 => b::Repr::I64,
        Ty::Float => b::Repr::F64,
        Ty::String | Ty::Any => b::Repr::Ref,
        Ty::Unit | Ty::Never => b::Repr::Void,
        Ty::Compound(id) => match core.ty(id) {
            b::Type::Enum(id, _)
                if !core.enums[*id].error
                    && !core.enums[*id].external
                    && !core.enums[*id].variants.is_empty()
                    && core.enums[*id].variants.iter().all(|v| v.tys.is_empty()) =>
            {
                b::Repr::I32
            }
            _ => b::Repr::Ref,
        },
    }
}
fn prim_ty(value: Ty) -> b::PrimTy {
    match value {
        Ty::Int | Ty::Bool => b::PrimTy::I32,
        Ty::Int64 => b::PrimTy::I64,
        Ty::Float => b::PrimTy::F64,
        Ty::String => b::PrimTy::String,
        _ => unreachable!("non-numeric primitive in checked IR"),
    }
}
fn prim_op(op: Binary) -> b::PrimOp {
    match op {
        Binary::Add => b::PrimOp::Add,
        Binary::Sub => b::PrimOp::Sub,
        Binary::Mul => b::PrimOp::Mul,
        Binary::Div => b::PrimOp::Div,
        Binary::Rem => b::PrimOp::Rem,
        Binary::Eq => b::PrimOp::Eq,
        Binary::Ne => b::PrimOp::Ne,
        Binary::Lt => b::PrimOp::Lt,
        Binary::Le => b::PrimOp::Le,
        Binary::Gt => b::PrimOp::Gt,
        Binary::Ge => b::PrimOp::Ge,
        _ => unreachable!("short-circuit and pipe normalized separately"),
    }
}
fn literal(value: &ir::Literal) -> b::Const {
    match value {
        ir::Literal::Int(n) => b::Const::Int(*n),
        ir::Literal::Int64(n) => b::Const::Int64(*n),
        ir::Literal::Float(n) => b::Const::Float(*n),
        ir::Literal::String(text) => b::Const::String(text.clone()),
        ir::Literal::Bool(n) => b::Const::Bool(*n),
        ir::Literal::Unit => b::Const::Unit,
    }
}
fn builtin(op: Intrinsic) -> b::Builtin {
    match op {
        Intrinsic::StateRun => unreachable!("State.run is expanded before backend lowering"),
        Intrinsic::Panic => b::Builtin::Panic,
        Intrinsic::Todo => b::Builtin::Todo,
        Intrinsic::Dbg => b::Builtin::Dbg,
        Intrinsic::Inspect | Intrinsic::ShowNested => b::Builtin::ShowNested,
        Intrinsic::IntBitAnd => b::Builtin::IntBitAnd,
        Intrinsic::IntBitOr => b::Builtin::IntBitOr,
        Intrinsic::IntBitXor => b::Builtin::IntBitXor,
        Intrinsic::IntBitNot => b::Builtin::IntBitNot,
        Intrinsic::IntShiftLeft => b::Builtin::IntShiftLeft,
        Intrinsic::IntShiftRight => b::Builtin::IntShiftRight,
        Intrinsic::IntShiftRightUnsigned => b::Builtin::IntShiftRightUnsigned,
        Intrinsic::Int64FromInt => b::Builtin::Int64FromInt,
        Intrinsic::Int64ToInt => b::Builtin::Int64ToInt,
        Intrinsic::Int64BitAnd => b::Builtin::Int64BitAnd,
        Intrinsic::Int64BitOr => b::Builtin::Int64BitOr,
        Intrinsic::Int64BitXor => b::Builtin::Int64BitXor,
        Intrinsic::Int64BitNot => b::Builtin::Int64BitNot,
        Intrinsic::Int64ShiftLeft => b::Builtin::Int64ShiftLeft,
        Intrinsic::Int64ShiftRight => b::Builtin::Int64ShiftRight,
        Intrinsic::Int64ShiftRightUnsigned => b::Builtin::Int64ShiftRightUnsigned,
        Intrinsic::FloatFromInt => b::Builtin::FloatFromInt,
        Intrinsic::IntToString => b::Builtin::IntToString,
        Intrinsic::Int64ToString => b::Builtin::Int64ToString,
        Intrinsic::FloatToString => b::Builtin::FloatToString,
        Intrinsic::FloatFloor => b::Builtin::FloatFloor,
        Intrinsic::FloatCeil => b::Builtin::FloatCeil,
        Intrinsic::FloatRound => b::Builtin::FloatRound,
        Intrinsic::FloatTruncate => b::Builtin::FloatTruncate,
        Intrinsic::StringLength => b::Builtin::StringLength,
        Intrinsic::StringByteSize => b::Builtin::StringByteSize,
        Intrinsic::StringAppend => b::Builtin::StringAppend,
        Intrinsic::StringContains => b::Builtin::StringContains,
        Intrinsic::StringStartsWith => b::Builtin::StringStartsWith,
        Intrinsic::StringEndsWith => b::Builtin::StringEndsWith,
        Intrinsic::StringTrim => b::Builtin::StringTrim,
        Intrinsic::IntParse => b::Builtin::IntParse,
        Intrinsic::IntCheckedAdd => b::Builtin::IntCheckedAdd,
        Intrinsic::FloatParse => b::Builtin::FloatParse,
        Intrinsic::StringFromCodePoint => b::Builtin::StringFromCodePoint,
        Intrinsic::ListJoin => b::Builtin::StringJoin,
        Intrinsic::StringSplit => b::Builtin::StringSplit,
        Intrinsic::StringChars => b::Builtin::StringChars,
        Intrinsic::StringCodePoints => b::Builtin::StringCodePoints,
        Intrinsic::BytesFromList => b::Builtin::BytesFromList,
        Intrinsic::BytesToList => b::Builtin::BytesToList,
        Intrinsic::BytesSize => b::Builtin::BytesSize,
        Intrinsic::BytesGet => b::Builtin::BytesGet,
        Intrinsic::BytesSlice => b::Builtin::BytesSlice,
        Intrinsic::BytesConcat => b::Builtin::BytesConcat,
        Intrinsic::BytesEncodeUtf8 => b::Builtin::BytesEncodeUtf8,
        Intrinsic::BytesDecodeValidUtf8 => b::Builtin::BytesDecodeValidUtf8,
    }
}
