//! Scalar normalization from the source-aware IR to the independent backend contract.
use super::{
    ast::{Binary, Unary},
    ir::{self, DefinitionId, LocalId},
    types::ValueType as Ty,
};
use crate::{contract::ir as b, sources::Intrinsic};
use std::collections::HashMap;

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
            debug_assert!(
                function.signature.sets.errors.is_empty()
                    && function.signature.sets.effects.is_empty()
            );
            let mut target = b::Function::new(&function.name);
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
        let mut context = Context {
            function: target,
            current: function.id,
            ids: &ids,
            intrinsics: &intrinsics,
            locals: HashMap::new(),
            core: &program.core,
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
        if let Some(body) = &function.body {
            context.function.body = Some(context.block(body, true));
        }
    }
    program
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
                ir::Statement::Bind { local, value } => {
                    let value = self.expression(builder, value, false);
                    self.locals
                        .insert(local.id, value.unwrap_or(b::Operand::Const(b::Const::Unit)));
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
                let operands = order
                    .iter()
                    .filter_map(|&index| evaluated[index].clone())
                    .collect();
                if tail && *target == self.current {
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
                        suspends: false,
                        on_fail: None,
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
        Ty::Compound(id) => id,
    }
}
fn repr(value: Ty, core: &b::Definitions) -> b::Repr {
    match value {
        Ty::Int | Ty::Bool => b::Repr::I32,
        Ty::Int64 => b::Repr::I64,
        Ty::Float => b::Repr::F64,
        Ty::String => b::Repr::Ref,
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
    }
}
