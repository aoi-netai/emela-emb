//! Validate frontend-owned IR and construct the backend's lowering metadata.
//!
//! The frontend supplies typed definitions, never cached reachability or layout state.
use crate::contract::ir::*;
use crate::contract::lowered::LoweredModule;
use crate::{Error, ErrorKind};
use std::collections::{HashMap, HashSet};

type Result<T> = std::result::Result<T, Vec<Error>>;
fn invalid(name: &str, message: impl Into<String>) -> Vec<Error> {
    vec![Error {
        kind: ErrorKind::InvalidFlowIr,
        message: format!(
            "invalid backend IR in function `{name}`: {}",
            message.into()
        ),
    }]
}
fn unsupported(name: &str, message: &str) -> Vec<Error> {
    vec![Error {
        kind: ErrorKind::Unsupported,
        message: format!(
            "function `{name}`: {message} is not connected to independent lowering yet"
        ),
    }]
}

pub(crate) fn lower_program(
    mut program: Program,
    options: &crate::Options,
) -> Result<LoweredModule> {
    let mut symbols = HashSet::new();
    for (_, function) in program.functions.iter() {
        if function.exported {
            let symbol = function.export_symbol.as_deref().unwrap_or(&function.name);
            if !symbols.insert(symbol) {
                return Err(invalid(
                    &function.name,
                    format!("duplicate export `{symbol}`"),
                ));
            }
            if function.export_symbol.as_ref().is_some_and(|symbol| {
                !crate::semantics::c::is_c_identifier(symbol)
                    || crate::semantics::c::is_c_keyword(symbol)
                    || crate::semantics::c::is_reserved_alias(symbol)
            }) {
                return Err(invalid(
                    &function.name,
                    "invalid or reserved explicit C export name",
                ));
            }
        } else if function.export_symbol.is_some() {
            return Err(invalid(
                &function.name,
                "internal function has an explicit C export name",
            ));
        }
        validate_function(&program, function)?;
    }
    // Numeric builtins still execute their checks when their result is unused. Their
    // renderers require a destination, so create an immediately discarded scalar slot.
    let mut discarded_results: HashMap<FnId, HashSet<VarId>> = HashMap::new();
    let mut discarded = Vec::new();
    for (function_id, function) in program.functions.iter() {
        let available = function.vars.iter().map(|(id, _)| id).collect();
        for (statement, stmt) in function.stmts.iter() {
            if let Stmt::Let {
                dst: None,
                value: Value::Builtin { .. },
                ..
            } = stmt
            {
                let Stmt::Let { value, .. } = stmt else {
                    unreachable!()
                };
                discarded.push((
                    function_id,
                    statement,
                    value_type(&program, function, value, &available)?,
                ));
            }
        }
    }
    for (function_id, statement, ty) in discarded {
        let function = &mut program.functions[function_id];
        let repr = match ty {
            TypeId::INT | TypeId::BOOL => Repr::I32,
            TypeId::INT64 => Repr::I64,
            TypeId::FLOAT => Repr::F64,
            _ => unreachable!("validated scalar builtin result"),
        };
        let variable = function.vars.alloc(VarData {
            hint: "discarded_result".into(),
            repr,
            ty,
        });
        let Stmt::Let { dst, .. } = &mut function.stmts[statement] else {
            unreachable!()
        };
        *dst = Some(variable);
        discarded_results
            .entry(function_id)
            .or_default()
            .insert(variable);
    }
    for (_, function) in program.functions.iter_mut() {
        // A self tail call is represented by a loop over the original parameters. Recur's
        // emitter copies all arguments to temporaries before replacing the parameter values.
        if function
            .seqs
            .iter()
            .any(|(_, seq)| matches!(seq.term, Term::Recur(_)))
        {
            let body = function
                .body
                .expect("validated function with a Recur has a body");
            let statement = function.stmts.alloc(Stmt::Loop {
                dst: None,
                vars: function.params.clone(),
                inits: function.params.iter().copied().map(Operand::Var).collect(),
                body,
            });
            function.body = Some(function.seqs.alloc(Seq {
                stmts: vec![statement],
                term: Term::Unreachable,
            }));
        }
        let (statements, sequences) = positions(function);
        function.active_statements = statements.keys().copied().collect();
        function.statement_positions = statements;
        function.sequence_positions = sequences;
    }
    let mut pending: Vec<_> = program
        .functions
        .iter()
        .filter_map(|(id, f)| (f.exported || f.name == options.entry()).then_some(id))
        .collect();
    if pending.is_empty() {
        pending.extend(program.functions.iter().map(|(id, _)| id));
    }
    let mut emitted_functions = HashSet::new();
    while let Some(id) = pending.pop() {
        if !emitted_functions.insert(id) {
            continue;
        }
        let function = &program.functions[id];
        for (_, stmt) in function
            .stmts
            .iter()
            .filter(|(id, _)| function.active_statements.contains(id))
        {
            if let Stmt::Let {
                value:
                    Value::Call {
                        callee: Callee::Direct(id),
                        ..
                    },
                ..
            } = stmt
            {
                pending.push(*id);
            }
        }
    }
    Ok(LoweredModule {
        lowered: program,
        suspensions: HashMap::new(),
        closures: Default::default(),
        const_groups: vec![],
        runtime_consts: HashSet::new(),
        deferred_pool: false,
        emitted_functions,
        discarded_results,
        const_outcomes: HashMap::new(),
    })
}

fn validate_function(program: &Program, f: &Function) -> Result<()> {
    if f.constant
        || f.suspends
        || f.fails
        || f.env.is_some()
        || f.derivation.is_some()
        || f.infallible_cause_fail
    {
        return Err(unsupported(
            &f.name,
            "const, failure, suspension, closure or derived function",
        ));
    }
    let check_type = |ty: TypeId| -> Result<()> {
        if ty.index() >= program.core.types.len() {
            return Err(invalid(&f.name, "type ID is out of bounds"));
        }
        match program.core.ty(ty) {
            Type::Int
            | Type::Int64
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Unit
            | Type::Never => Ok(()),
            _ => Err(unsupported(&f.name, "non-scalar value type")),
        }
    };
    check_type(f.ret)?;
    for (_, var) in f.vars.iter() {
        check_type(var.ty)?;
        let expected = match program.core.ty(var.ty) {
            Type::Int | Type::Bool => Repr::I32,
            Type::Int64 => Repr::I64,
            Type::Float => Repr::F64,
            Type::Unit | Type::Never => Repr::Void,
            _ => Repr::Ref,
        };
        if var.repr != expected {
            return Err(invalid(
                &f.name,
                "variable representation does not match its type",
            ));
        }
    }
    let mut available = HashSet::new();
    for &param in &f.params {
        variable(f, param)?;
        if !available.insert(param) {
            return Err(invalid(&f.name, "duplicate parameter"));
        }
    }
    match (&f.external, f.body) {
        (Some(_), Some(_)) => Err(invalid(&f.name, "external function has a body")),
        (Some(_), None) => Ok(()),
        (None, None) => Err(invalid(&f.name, "function has no body")),
        (None, Some(body)) => validate_sequence(
            program,
            f,
            body,
            &available,
            &mut HashSet::new(),
            None,
            &f.params,
            &HashMap::new(),
        ),
    }
}
fn variable(f: &Function, var: VarId) -> Result<TypeId> {
    f.vars
        .iter()
        .find_map(|(id, value)| (id == var).then_some(value))
        .map(|v| v.ty)
        .ok_or_else(|| invalid(&f.name, "variable ID is out of bounds"))
}
fn operand(f: &Function, value: &Operand, available: &HashSet<VarId>) -> Result<TypeId> {
    if let Operand::Var(var) = value {
        let ty = variable(f, *var)?;
        if !available.contains(var) {
            return Err(invalid(&f.name, "variable is used before its definition"));
        }
        return Ok(ty);
    }
    Ok(
        crate::semantics::operands::operand_type(f, value)
            .expect("every scalar operand has a type"),
    )
}
fn expect_type(f: &Function, actual: TypeId, expected: TypeId) -> Result<()> {
    if actual == expected || actual == TypeId::NEVER {
        Ok(())
    } else {
        Err(invalid(
            &f.name,
            format!("type mismatch: expected {expected:?}, found {actual:?}"),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_sequence(
    program: &Program,
    f: &Function,
    id: SeqId,
    outer: &HashSet<VarId>,
    visiting: &mut HashSet<SeqId>,
    yielded: Option<TypeId>,
    recur_vars: &[VarId],
    blocks: &HashMap<LabelId, TypeId>,
) -> Result<()> {
    let seq = f
        .seqs
        .iter()
        .find_map(|(key, value)| (key == id).then_some(value))
        .ok_or_else(|| invalid(&f.name, "sequence ID is out of bounds"))?;
    if !visiting.insert(id) {
        return Err(invalid(&f.name, "cyclic sequence graph; use Loop/Recur"));
    }
    let mut available = outer.clone();
    for &id in &seq.stmts {
        let stmt = f
            .stmts
            .iter()
            .find_map(|(key, value)| (key == id).then_some(value))
            .ok_or_else(|| invalid(&f.name, "statement ID is out of bounds"))?;
        let dst = stmt.dst();
        let dst_type = dst.map(|var| variable(f, var)).transpose()?;
        match stmt {
            Stmt::Let {
                value,
                on_defect: None,
                on_suspend: None,
                ..
            } => {
                let result = value_type(program, f, value, &available)?;
                if let Some(ty) = dst_type {
                    expect_type(f, result, ty)?;
                }
            }
            Stmt::If {
                cond, then, else_, ..
            } => {
                expect_type(f, operand(f, cond, &available)?, TypeId::BOOL)?;
                validate_sequence(
                    program,
                    f,
                    *then,
                    &available,
                    visiting,
                    Some(dst_type.unwrap_or(TypeId::UNIT)),
                    recur_vars,
                    blocks,
                )?;
                validate_sequence(
                    program,
                    f,
                    *else_,
                    &available,
                    visiting,
                    Some(dst_type.unwrap_or(TypeId::UNIT)),
                    recur_vars,
                    blocks,
                )?;
            }
            Stmt::Loop {
                vars, inits, body, ..
            } => {
                if vars.len() != inits.len() {
                    return Err(invalid(
                        &f.name,
                        "loop variable and initializer counts differ",
                    ));
                }
                let mut loop_available = available.clone();
                for (&var, init) in vars.iter().zip(inits) {
                    expect_type(f, operand(f, init, &available)?, variable(f, var)?)?;
                    loop_available.insert(var);
                }
                validate_sequence(
                    program,
                    f,
                    *body,
                    &loop_available,
                    visiting,
                    Some(dst_type.unwrap_or(TypeId::UNIT)),
                    vars,
                    blocks,
                )?;
            }
            Stmt::Block { label, body, .. } => {
                if !f.labels.iter().any(|(id, _)| id == *label) {
                    return Err(invalid(&f.name, "block label ID is out of bounds"));
                }
                let mut nested = blocks.clone();
                if nested
                    .insert(*label, dst_type.unwrap_or(TypeId::UNIT))
                    .is_some()
                {
                    return Err(invalid(&f.name, "block label is already active"));
                }
                validate_sequence(
                    program,
                    f,
                    *body,
                    &available,
                    visiting,
                    Some(dst_type.unwrap_or(TypeId::UNIT)),
                    recur_vars,
                    &nested,
                )?;
            }
            _ => return Err(unsupported(&f.name, "statement variant")),
        }
        if let Some(dst) = dst {
            available.insert(dst);
        }
    }
    match &seq.term {
        Term::Return(value) => {
            expect_type(
                f,
                value
                    .as_ref()
                    .map(|v| operand(f, v, &available))
                    .transpose()?
                    .unwrap_or(TypeId::UNIT),
                f.ret,
            )?;
        }
        Term::Yield(value) => {
            let expected =
                yielded.ok_or_else(|| invalid(&f.name, "Yield appears at function scope"))?;
            expect_type(
                f,
                value
                    .as_ref()
                    .map(|v| operand(f, v, &available))
                    .transpose()?
                    .unwrap_or(TypeId::UNIT),
                expected,
            )?;
        }
        Term::Recur(args) => {
            if args.len() != recur_vars.len() {
                return Err(invalid(
                    &f.name,
                    "Recur arity does not match loop parameters",
                ));
            }
            for (arg, &var) in args.iter().zip(recur_vars) {
                expect_type(f, operand(f, arg, &available)?, variable(f, var)?)?;
            }
        }
        Term::Break { label, value } => {
            let expected = blocks
                .get(label)
                .copied()
                .ok_or_else(|| invalid(&f.name, "Break refers to an inactive block label"))?;
            let actual = value
                .as_ref()
                .map(|value| operand(f, value, &available))
                .transpose()?
                .unwrap_or(TypeId::UNIT);
            expect_type(f, actual, expected)?;
        }
        Term::Defect(message) => {
            expect_type(f, operand(f, message, &available)?, TypeId::STRING)?;
        }
        Term::Unreachable => {}
        _ => return Err(unsupported(&f.name, "terminal variant")),
    }
    visiting.remove(&id);
    Ok(())
}

fn value_type(
    program: &Program,
    f: &Function,
    value: &Value,
    available: &HashSet<VarId>,
) -> Result<TypeId> {
    match value {
        Value::Const(value) => operand(f, &Operand::Const(value.clone()), available),
        Value::Call {
            callee: Callee::Direct(id),
            args,
            suspends: false,
            on_fail: None,
        } => {
            let target = program
                .functions
                .iter()
                .find_map(|(key, value)| (key == *id).then_some(value))
                .ok_or_else(|| invalid(&f.name, "callee ID is out of bounds"))?;
            if args.len() != target.params.len() {
                return Err(invalid(
                    &f.name,
                    "call argument count does not match callee parameters",
                ));
            }
            for (arg, &param) in args.iter().zip(&target.params) {
                expect_type(f, operand(f, arg, available)?, variable(target, param)?)?;
            }
            Ok(target.ret)
        }
        Value::Prim { op, ty, args } => {
            let ty = match ty {
                PrimTy::I32 => TypeId::INT,
                PrimTy::I64 => TypeId::INT64,
                PrimTy::F64 => TypeId::FLOAT,
                PrimTy::String => return Err(unsupported(&f.name, "String primitive")),
            };
            let unary = matches!(op, PrimOp::Neg | PrimOp::Not);
            if args.len() != if unary { 1 } else { 2 } {
                return Err(invalid(&f.name, "primitive argument count is invalid"));
            }
            let mut first_type = None;
            for arg in args {
                let actual = operand(f, arg, available)?;
                if first_type.is_some_and(|first| first != actual) {
                    return Err(invalid(&f.name, "primitive operands have different types"));
                }
                first_type = Some(actual);
                // Bool comparisons use the I32 primitive representation.
                if !(ty == TypeId::INT
                    && actual == TypeId::BOOL
                    && matches!(op, PrimOp::Not | PrimOp::Eq | PrimOp::Ne))
                {
                    expect_type(f, actual, ty)?;
                }
            }
            Ok(
                if matches!(
                    op,
                    PrimOp::Not
                        | PrimOp::Eq
                        | PrimOp::Ne
                        | PrimOp::Lt
                        | PrimOp::Le
                        | PrimOp::Gt
                        | PrimOp::Ge
                ) {
                    TypeId::BOOL
                } else {
                    ty
                },
            )
        }
        Value::Builtin { op, args } => {
            let (input, result, arity) = match op {
                Builtin::IntBitNot => (TypeId::INT, TypeId::INT, 1),
                Builtin::IntBitAnd
                | Builtin::IntBitOr
                | Builtin::IntBitXor
                | Builtin::IntShiftLeft
                | Builtin::IntShiftRight
                | Builtin::IntShiftRightUnsigned => (TypeId::INT, TypeId::INT, 2),
                Builtin::Int64BitNot => (TypeId::INT64, TypeId::INT64, 1),
                Builtin::Int64BitAnd | Builtin::Int64BitOr | Builtin::Int64BitXor => {
                    (TypeId::INT64, TypeId::INT64, 2)
                }
                Builtin::Int64ShiftLeft
                | Builtin::Int64ShiftRight
                | Builtin::Int64ShiftRightUnsigned => {
                    if args.len() != 2 {
                        return Err(invalid(&f.name, "shift argument count is invalid"));
                    }
                    expect_type(f, operand(f, &args[0], available)?, TypeId::INT64)?;
                    expect_type(f, operand(f, &args[1], available)?, TypeId::INT)?;
                    return Ok(TypeId::INT64);
                }
                Builtin::Int64FromInt => (TypeId::INT, TypeId::INT64, 1),
                Builtin::Int64ToInt => (TypeId::INT64, TypeId::INT, 1),
                Builtin::FloatFromInt => (TypeId::INT, TypeId::FLOAT, 1),
                Builtin::FloatFloor
                | Builtin::FloatCeil
                | Builtin::FloatRound
                | Builtin::FloatTruncate => (TypeId::FLOAT, TypeId::FLOAT, 1),
                _ => return Err(unsupported(&f.name, "builtin")),
            };
            if args.len() != arity {
                return Err(invalid(&f.name, "builtin argument count is invalid"));
            }
            for arg in args {
                expect_type(f, operand(f, arg, available)?, input)?;
            }
            Ok(result)
        }
        _ => Err(unsupported(&f.name, "value variant")),
    }
}

fn positions(f: &Function) -> (HashMap<StmtId, usize>, HashMap<SeqId, usize>) {
    fn visit(
        f: &Function,
        id: SeqId,
        next: &mut usize,
        stmts: &mut HashMap<StmtId, usize>,
        seqs: &mut HashMap<SeqId, usize>,
    ) {
        if seqs.contains_key(&id) {
            return;
        }
        seqs.insert(id, *next);
        for &id in &f.seqs[id].stmts {
            *next += 1;
            stmts.insert(id, *next);
            match &f.stmts[id] {
                Stmt::If { then, else_, .. } => {
                    visit(f, *then, next, stmts, seqs);
                    visit(f, *else_, next, stmts, seqs);
                }
                Stmt::Loop { body, .. } | Stmt::Block { body, .. } => {
                    visit(f, *body, next, stmts, seqs)
                }
                _ => {}
            }
        }
        *next += 1;
        seqs.insert(id, *next);
    }
    let mut statements = HashMap::new();
    let mut sequences = HashMap::new();
    if let Some(body) = f.body {
        visit(f, body, &mut 0, &mut statements, &mut sequences);
    }
    (statements, sequences)
}

#[cfg(test)]
#[path = "lower/tests.rs"]
mod tests;
