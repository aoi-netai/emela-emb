//! Validate frontend-owned IR and construct the backend's lowering metadata.
//!
//! The frontend supplies typed definitions, never cached reachability or layout state.
use crate::contract::ir::*;
use crate::contract::lowered::LoweredModule;
use crate::{Error, ErrorKind};
use std::collections::{HashMap, HashSet};

mod closure;
mod const_eval;
mod effects;
mod error_types;
mod failures;
mod handler;
mod types;
mod values;
use values::value_type;

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
    types::validate_catalog(&program)?;
    effects::normalize(&mut program)?;
    types::normalize_catalog(&mut program)?;
    error_types::normalize(&mut program)?;
    let mut symbols = HashSet::new();
    for (id, function) in program.functions.iter() {
        if function.exported {
            let symbol = crate::semantics::c::function_name(id, function);
            if !symbols.insert(symbol.clone()) {
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
    let (mut program, closures) = closure::lower(&program, options)?;
    handler::lower(&mut program, options);
    let mut const_outcomes = HashMap::new();
    let mut runtime_consts = HashSet::new();
    for (id, function) in program.functions.iter() {
        if !function.constant {
            continue;
        }
        if !function.params.is_empty()
            || function.fails
            || function.suspends
            || function.external.is_some()
        {
            return Err(invalid(
                &function.name,
                "const must be a zero-argument, infallible synchronous definition",
            ));
        }
        match const_eval::evaluate(&program, id) {
            Ok(outcome) => {
                let outcome = match outcome {
                    const_eval::Outcome::Value(value) => {
                        crate::contract::lowered::ConstOutcome::Value(value)
                    }
                    const_eval::Outcome::Defect { kind, message } => {
                        crate::contract::lowered::ConstOutcome::Defect { kind, message }
                    }
                    const_eval::Outcome::SelfReference { message } => {
                        crate::contract::lowered::ConstOutcome::SelfReference { message }
                    }
                };
                const_outcomes.insert(id, Ok(outcome));
            }
            Err(_) => {
                runtime_consts.insert(id);
            }
        }
    }
    for &id in &runtime_consts {
        program.functions[id].constant = false;
    }
    failures::normalize(&mut program);
    // Checked builtins and List construction retain allocation/defect behavior even when
    // unused. A temporary destination also lets ownership planning release their results.
    let mut discarded_results: HashMap<FnId, HashSet<VarId>> = HashMap::new();
    let mut discarded = Vec::new();
    for (function_id, function) in program.functions.iter() {
        let available = function.vars.iter().map(|(id, _)| id).collect();
        for (statement, stmt) in function.stmts.iter() {
            if let Stmt::Let {
                dst: None,
                value: Value::Builtin { .. } | Value::List { .. },
                ..
            } = stmt
            {
                let Stmt::Let { value, .. } = stmt else {
                    unreachable!()
                };
                let ty = value_type(&program, function, value, &available, None)?;
                if !matches!(ty, TypeId::UNIT | TypeId::NEVER) {
                    discarded.push((function_id, statement, ty));
                }
            }
        }
    }
    for (function_id, statement, ty) in discarded {
        let repr = match program.core.ty(ty) {
            Type::Int | Type::Bool => Repr::I32,
            Type::Int64 => Repr::I64,
            Type::Float => Repr::F64,
            Type::Unit | Type::Never => Repr::Void,
            _ => Repr::Ref,
        };
        let function = &mut program.functions[function_id];
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
        if has_function_recur(function) {
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
    // Source visibility does not create C roots. A module with neither an export
    // nor the selected entry still gets validated, but emits no reachable functions.
    let mut emitted_functions = HashSet::new();
    while let Some(id) = pending.pop() {
        if !emitted_functions.insert(id) {
            continue;
        }
        // A resolved constant emits its value or defect directly. Its initializer no longer
        // calls helpers; retaining those edges can emit unused static C functions.
        if matches!(const_outcomes.get(&id), Some(Ok(_))) {
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
                    }
                    | Value::FnRef(id),
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
        closures,
        const_groups: vec![],
        runtime_consts,
        deferred_pool: false,
        emitted_functions,
        discarded_results,
        const_outcomes,
    })
}

fn validate_function(program: &Program, f: &Function) -> Result<()> {
    if f.suspends || f.env.is_some() || f.infallible_cause_fail {
        return Err(unsupported(
            &f.name,
            "const, failure, suspension, closure or derived function",
        ));
    }
    let check_type = |ty| types::validate_type(program, &f.name, ty);
    check_type(f.ret)?;
    for (_, var) in f.vars.iter() {
        check_type(var.ty)?;
        let expected = match program.core.ty(var.ty) {
            Type::Int | Type::Bool => Repr::I32,
            Type::Int64 => Repr::I64,
            Type::Float => Repr::F64,
            Type::Unit | Type::Never => Repr::Void,
            Type::Enum(id, _)
                if {
                    let definition = &program.core.enums[*id];
                    !definition.error
                        && !definition.external
                        && !definition.variants.is_empty()
                        && definition
                            .variants
                            .iter()
                            .all(|variant| variant.fields.is_empty())
                } =>
            {
                Repr::I32
            }
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
    if let Some(kind) = f.derivation {
        let valid = !f.exported
            && f.external.is_some()
            && f.body.is_none()
            && !f.fails
            && !f.suspends
            && !f.constant
            && match kind {
                crate::DerivedOperation::Eq => {
                    f.params.len() == 2
                        && f.vars[f.params[0]].ty == f.vars[f.params[1]].ty
                        && f.ret == TypeId::BOOL
                }
                crate::DerivedOperation::Show => f.params.len() == 1 && f.ret == TypeId::STRING,
            };
        if !valid {
            return Err(invalid(
                &f.name,
                "derived operation has an invalid signature or body",
            ));
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
                let result = value_type(program, f, value, &available, dst_type)?;
                if let Value::Closure(closure) = value {
                    let Type::Fn(ft) = program.core.ty(result) else {
                        unreachable!("closure type was validated")
                    };
                    let mut nested = f.clone();
                    nested.params = closure.params.clone();
                    nested.ret = ft.ret;
                    nested.fails = ft.fails;
                    nested.suspends = ft.suspends;
                    let parameters = closure
                        .params
                        .iter()
                        .chain(&closure.captures)
                        .copied()
                        .collect();
                    validate_sequence(
                        program,
                        &nested,
                        closure.body,
                        &parameters,
                        visiting,
                        None,
                        &closure.params,
                        &HashMap::new(),
                    )?;
                }
                if let Value::Call {
                    on_fail: Some(edge),
                    ..
                } = value
                {
                    match edge {
                        OnFail::Propagate if !f.fails => {
                            return Err(invalid(
                                &f.name,
                                "failure propagation has no function failure channel",
                            ));
                        }
                        OnFail::Propagate => {}
                        OnFail::Break(label) => {
                            if blocks.get(label) != Some(&TypeId::ANY) {
                                return Err(invalid(
                                    &f.name,
                                    "failure Break requires an active Any block destination",
                                ));
                            }
                        }
                        OnFail::Handle { error, body } => {
                            expect_type(f, variable(f, *error)?, TypeId::ANY)?;
                            let mut handler_available = available.clone();
                            handler_available.insert(*error);
                            validate_sequence(
                                program,
                                f,
                                *body,
                                &handler_available,
                                visiting,
                                Some(dst_type.unwrap_or(TypeId::UNIT)),
                                recur_vars,
                                blocks,
                            )?;
                        }
                    }
                }
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
            Stmt::Switch {
                on, cases, default, ..
            } => {
                let base = match on {
                    SwitchOn::Tag(base) | SwitchOn::Lit(base) => *base,
                };
                let ty = operand(f, &Operand::Var(base), &available)?;
                let mut seen = Vec::new();
                for case in cases {
                    if seen.contains(&case.key) {
                        return Err(invalid(&f.name, "duplicate switch case"));
                    }
                    seen.push(case.key.clone());
                    match (&case.key, on, program.core.ty(ty)) {
                        (CaseKey::Variant(variant), SwitchOn::Tag(_), Type::Enum(id, _))
                            if *variant < program.core.enums[*id].variants.len() => {}
                        (CaseKey::Lit(Const::Int(_)), SwitchOn::Lit(_), Type::Int)
                        | (CaseKey::Lit(Const::Int64(_)), SwitchOn::Lit(_), Type::Int64) => {}
                        _ => {
                            return Err(invalid(
                                &f.name,
                                "switch case key differs from its scrutinee type",
                            ));
                        }
                    }
                    validate_sequence(
                        program,
                        f,
                        case.body,
                        &available,
                        visiting,
                        Some(dst_type.unwrap_or(TypeId::UNIT)),
                        recur_vars,
                        blocks,
                    )?;
                }
                if let Some(body) = default {
                    validate_sequence(
                        program,
                        f,
                        *body,
                        &available,
                        visiting,
                        Some(dst_type.unwrap_or(TypeId::UNIT)),
                        recur_vars,
                        blocks,
                    )?;
                } else if matches!(on, SwitchOn::Lit(_)) {
                    return Err(invalid(&f.name, "literal switch has no default sequence"));
                }
            }
            Stmt::Protect { body, cleanup, .. } => {
                validate_sequence(
                    program,
                    f,
                    *body,
                    &available,
                    visiting,
                    Some(dst_type.unwrap_or(TypeId::UNIT)),
                    recur_vars,
                    blocks,
                )?;
                validate_sequence(
                    program,
                    f,
                    *cleanup,
                    &available,
                    visiting,
                    Some(TypeId::UNIT),
                    recur_vars,
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
        Term::Fail(error) => {
            if !f.fails {
                return Err(invalid(&f.name, "Fail has no function failure channel"));
            }
            let ty = operand(f, error, &available)?;
            if ty != TypeId::ANY
                && !matches!(program.core.ty(ty),Type::Enum(id,_) if program.core.enums[*id].error)
            {
                return Err(invalid(&f.name, "Fail value is not an error enum or Any"));
            }
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

fn has_function_recur(f: &Function) -> bool {
    let mut pending: Vec<_> = f.body.into_iter().map(|body| (body, false)).collect();
    while let Some((id, in_loop)) = pending.pop() {
        let sequence = &f.seqs[id];
        if !in_loop && matches!(sequence.term, Term::Recur(_)) {
            return true;
        }
        for &id in &sequence.stmts {
            match &f.stmts[id] {
                Stmt::If { then, else_, .. } => {
                    pending.extend([(*then, in_loop), (*else_, in_loop)]);
                }
                Stmt::Block { body, .. } => pending.push((*body, in_loop)),
                Stmt::Loop { body, .. } => pending.push((*body, true)),
                Stmt::Switch { cases, default, .. } => {
                    pending.extend(cases.iter().map(|case| (case.body, in_loop)));
                    pending.extend(default.iter().map(|&body| (body, in_loop)));
                }
                Stmt::Protect { body, cleanup, .. } => {
                    pending.extend([(*body, in_loop), (*cleanup, in_loop)])
                }
                Stmt::Let {
                    value:
                        Value::Call {
                            on_fail: Some(OnFail::Handle { body, .. }),
                            ..
                        },
                    ..
                } => pending.push((*body, in_loop)),
                _ => {}
            }
        }
    }
    false
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
                Stmt::Switch { cases, default, .. } => {
                    for case in cases {
                        visit(f, case.body, next, stmts, seqs);
                    }
                    if let Some(body) = default {
                        visit(f, *body, next, stmts, seqs);
                    }
                }
                Stmt::Protect { body, cleanup, .. } => {
                    visit(f, *body, next, stmts, seqs);
                    visit(f, *cleanup, next, stmts, seqs);
                }
                Stmt::Let {
                    value:
                        Value::Call {
                            on_fail: Some(OnFail::Handle { body, .. }),
                            ..
                        },
                    ..
                } => visit(f, *body, next, stmts, seqs),
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
