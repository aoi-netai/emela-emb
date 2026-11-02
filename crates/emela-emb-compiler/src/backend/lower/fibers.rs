//! Validate the concrete structured-fiber ABI before entering the scheduler planner.
use super::*;

pub(super) fn runtime_metadata(program: &Program, emitted: &HashSet<FnId>) -> bool {
    let mut scopes = false;
    for (id, function) in program.functions.iter() {
        if !emitted.contains(&id) {
            continue;
        }
        for (id, statement) in function.stmts.iter() {
            if !function.active_statements.contains(&id) {
                continue;
            }
            let Stmt::Let {
                value: Value::Fiber { op, .. },
                ..
            } = statement
            else {
                continue;
            };
            scopes |= matches!(
                op,
                FiberOp::Scope { .. }
                    | FiberOp::Fork
                    | FiberOp::Deferred
                    | FiberOp::Join
                    | FiberOp::Await { .. }
                    | FiberOp::Interrupt
                    | FiberOp::Complete
                    | FiberOp::Poll
                    | FiberOp::DeferredAwait
            );
        }
    }
    scopes
}

/// Merely constructing Scope's capability includes its deferred operation closure.
/// It needs storage only when a reachable operation actually calls that constructor.
pub(super) fn needs_deferred_pool(
    program: &Program,
    closures: &crate::contract::lowered::ClosureMetadata,
    options: &crate::Options,
) -> bool {
    let reachable = super::closure::runtime_reachable(program, options);
    let mut constructors = HashSet::new();
    for (id, function) in program.functions.iter() {
        if !reachable.contains(&id) {
            continue;
        }
        let (positions, _) = super::positions(function);
        for (id, statement) in function.stmts.iter() {
            if !positions.contains_key(&id) {
                continue;
            }
            let Stmt::Let {
                value: Value::Handler { effect, ops, .. },
                ..
            } = statement
            else {
                continue;
            };
            if !super::scope_abi::is_scope(&program.core.effects[*effect]) {
                continue;
            }
            for (index, operation) in program.core.effects[*effect].ops.iter().enumerate() {
                if operation.name != "deferred" {
                    continue;
                }
                for (_, definition) in function.stmts.iter() {
                    if let Stmt::Let {
                        dst: Some(dst),
                        value: Value::FnRef(target),
                        ..
                    } = definition
                    {
                        if *dst == ops[index] && closures.functions.contains_key(target) {
                            constructors.insert(*target);
                        }
                    }
                }
            }
        }
    }
    program.functions.iter().any(|(id, function)| {
        if !reachable.contains(&id) {
            return false;
        }
        let (positions, _) = super::positions(function);
        function.stmts.iter().any(|(statement, value)| {
            positions.contains_key(&statement)
                && match value {
                    Stmt::Let {
                        value:
                            Value::Fiber {
                                op: FiberOp::Deferred,
                                ..
                            },
                        ..
                    } => !constructors.contains(&id),
                    Stmt::Let {
                        value:
                            Value::Fiber {
                                op: FiberOp::Complete | FiberOp::Poll | FiberOp::DeferredAwait,
                                ..
                            },
                        ..
                    } => true,
                    Stmt::Let {
                        value:
                            Value::Call {
                                callee: Callee::Op { effect, op, .. },
                                ..
                            },
                        ..
                    } => {
                        super::scope_abi::is_scope(&program.core.effects[*effect])
                            && program.core.effects[*effect].ops[*op].name == "deferred"
                    }
                    _ => false,
                }
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn value_type(
    program: &Program,
    function: &Function,
    op: FiberOp,
    args: &[Operand],
    on_fail: Option<OnFail>,
    available: &HashSet<VarId>,
    expected: Option<TypeId>,
) -> Result<TypeId> {
    let fail = |message: &str| invalid(&function.name, message);
    let actual = args
        .iter()
        .map(|value| operand(function, value, available))
        .collect::<Result<Vec<_>>>()?;
    if !matches!(op, FiberOp::Scope { fails: true } | FiberOp::CauseRaise) && on_fail.is_some() {
        return Err(fail("nonfailing fiber operation has a failure edge"));
    }
    if matches!(
        op,
        FiberOp::Scope { .. }
            | FiberOp::Join
            | FiberOp::Await { .. }
            | FiberOp::Interrupt
            | FiberOp::DeferredAwait
            | FiberOp::Attempt { suspends: true, .. }
    ) && !function.suspends
    {
        return Err(fail("fiber wait requires a suspending caller"));
    }
    match op {
        FiberOp::Scope { fails } => {
            let [body] = actual.as_slice() else {
                return Err(fail("Scope expects one body"));
            };
            let Type::Fn(ft) = program.core.ty(*body) else {
                return Err(fail("Scope body is not a function"));
            };
            if ft.params != [TypeId::FIBER_SCOPE]
                || ft.ev
                || !ft.suspends
                || (ft.fails && !fails)
                || fails != on_fail.is_some()
            {
                return Err(fail("Scope body or failure ABI is invalid"));
            }
            Ok(ft.ret)
        }
        FiberOp::Fork => {
            let [scope, body] = actual.as_slice() else {
                return Err(fail("Fork expects a scope and body"));
            };
            expect_type(function, *scope, TypeId::FIBER_SCOPE)?;
            let Type::Fn(ft) = program.core.ty(*body) else {
                return Err(fail("Fork body is not a function"));
            };
            if !ft.params.is_empty() || ft.ret != TypeId::ANY || ft.ev || !ft.suspends {
                return Err(fail(
                    "Fork body must be a suspending fn() -> Any without evidence",
                ));
            }
            if let Some(ty) = expected.filter(|ty| *ty != TypeId::ANY) {
                handle(program, function, ty, "Std.Fiber.Fiber")?;
            }
            Ok(expected.unwrap_or(TypeId::ANY))
        }
        FiberOp::Deferred => {
            let [scope] = actual.as_slice() else {
                return Err(fail("Deferred expects one scope"));
            };
            expect_type(function, *scope, TypeId::FIBER_SCOPE)?;
            if let Some(ty) = expected.filter(|ty| *ty != TypeId::ANY) {
                handle(program, function, ty, "Std.Deferred.Deferred")?;
            }
            Ok(expected.unwrap_or(TypeId::ANY))
        }
        FiberOp::Join | FiberOp::Await { .. } | FiberOp::Interrupt => {
            let [child] = actual.as_slice() else {
                return Err(fail("fiber wait expects one handle"));
            };
            let value = handle(program, function, *child, "Std.Fiber.Fiber")?;
            match op {
                FiberOp::Join => Ok(value),
                FiberOp::Interrupt => Ok(TypeId::UNIT),
                FiberOp::Await { exit } => exit_type(program, function, exit, value, expected),
                _ => unreachable!(),
            }
        }
        FiberOp::Complete => {
            let [deferred, value] = actual.as_slice() else {
                return Err(fail("Complete expects a deferred and value"));
            };
            let wanted = handle(program, function, *deferred, "Std.Deferred.Deferred")?;
            expect_type(function, *value, wanted)?;
            Ok(TypeId::BOOL)
        }
        FiberOp::DeferredAwait | FiberOp::Poll => {
            let [deferred] = actual.as_slice() else {
                return Err(fail("Deferred read expects one handle"));
            };
            let value = handle(program, function, *deferred, "Std.Deferred.Deferred")?;
            if op == FiberOp::DeferredAwait {
                return Ok(value);
            }
            if let Some(option) = expected {
                let Type::Enum(id, _) = program.core.ty(option) else {
                    return Err(fail("Poll result is not Option"));
                };
                let definition = &program.core.enums[*id];
                if definition.name != "Option" || definition.variants.len() != 2 {
                    return Err(fail("Poll result is not the runtime Option type"));
                }
                let none = definition
                    .variants
                    .iter()
                    .position(|variant| variant.name == "None");
                let some = definition
                    .variants
                    .iter()
                    .position(|variant| variant.name == "Some");
                let valid = match (none, some) {
                    (Some(none), Some(some)) => {
                        types::enum_fields(program, &function.name, option, none)?.is_empty()
                            && types::enum_fields(program, &function.name, option, some)? == [value]
                    }
                    _ => false,
                };
                if !valid {
                    return Err(fail("Poll result has incompatible Option variants"));
                }
            }
            Ok(expected.unwrap_or(TypeId::UNIT))
        }
        FiberOp::Attempt { exit, suspends } => {
            let [body] = actual.as_slice() else {
                return Err(fail("Attempt expects one body"));
            };
            let Type::Fn(ft) = program.core.ty(*body) else {
                return Err(fail("Attempt body is not a function"));
            };
            if !ft.params.is_empty() || ft.ev || ft.suspends != suspends {
                return Err(fail("Attempt body ABI is invalid"));
            }
            exit_type(program, function, exit, ft.ret, expected)
        }
        FiberOp::CauseKind { .. } | FiberOp::CauseToString | FiberOp::CauseRaise => {
            let [cause] = actual.as_slice() else {
                return Err(fail("Cause operation expects one value"));
            };
            if !is_cause(program, *cause) {
                return Err(fail("Cause operation received another type"));
            }
            match op {
                FiberOp::CauseKind { kind } => {
                    let Some(definition) = program
                        .core
                        .enums
                        .iter()
                        .find_map(|(id, definition)| (id == kind).then_some(definition))
                    else {
                        return Err(fail("Cause kind enum is invalid"));
                    };
                    if definition.params != 0
                        || definition.variants.len() != 3
                        || definition
                            .variants
                            .iter()
                            .any(|variant| !variant.tys.is_empty())
                    {
                        return Err(fail("Cause kind requires three nullary variants"));
                    }
                    program
                        .core
                        .types
                        .ids
                        .get(&Type::Enum(kind, vec![]))
                        .copied()
                        .ok_or_else(|| fail("Cause kind type is missing"))
                }
                FiberOp::CauseToString => Ok(TypeId::STRING),
                FiberOp::CauseRaise => {
                    if on_fail.is_none() {
                        return Err(fail("CauseRaise requires an explicit failure edge"));
                    }
                    Ok(TypeId::NEVER)
                }
                _ => unreachable!(),
            }
        }
    }
}
fn handle(program: &Program, function: &Function, ty: TypeId, name: &str) -> Result<TypeId> {
    let Type::Enum(id, args) = program.core.ty(ty) else {
        return Err(invalid(
            &function.name,
            "fiber handle is not a nominal external value",
        ));
    };
    if !program.core.enums[*id].external || program.core.enums[*id].name != name || args.len() != 1
    {
        return Err(invalid(
            &function.name,
            "fiber handle has a different runtime identity",
        ));
    }
    Ok(args[0])
}
fn is_cause(program: &Program, ty: TypeId) -> bool {
    matches!(program.core.ty(ty), Type::Enum(id, _) if program.core.enums[*id].name == "Std.Cause.Cause")
}
fn exit_type(
    program: &Program,
    function: &Function,
    exit: EnumId,
    value: TypeId,
    expected: Option<TypeId>,
) -> Result<TypeId> {
    let Some(definition) = program
        .core
        .enums
        .iter()
        .find_map(|(id, definition)| (id == exit).then_some(definition))
    else {
        return Err(invalid(&function.name, "Exit enum ID is invalid"));
    };
    if definition.params == 0 || definition.variants.len() != 2 {
        return Err(invalid(
            &function.name,
            "Exit requires generic Success and Failure variants",
        ));
    }
    if let Some(ty) = expected {
        if !matches!(program.core.ty(ty), Type::Enum(id, _) if *id == exit)
            || types::enum_fields(program, &function.name, ty, EXIT_SUCCESS)? != [value]
        {
            return Err(invalid(
                &function.name,
                "Exit Success type differs from the body result",
            ));
        }
        let failure = types::enum_fields(program, &function.name, ty, EXIT_FAILURE)?;
        if failure.len() != 1 || !is_cause(program, failure[0]) {
            return Err(invalid(&function.name, "Exit Failure payload is not Cause"));
        }
    }
    Ok(expected.unwrap_or(TypeId::UNIT))
}

/// Runtime constructors return the common owning Any handle; typed destinations
/// retain their nominal identity through an explicit checked extraction.
pub(super) fn normalize_values(program: &mut Program) {
    for (_, function) in program.functions.iter_mut() {
        let pending: Vec<_> = function
            .stmts
            .iter()
            .filter_map(|(id, statement)| {
                let Stmt::Let {
                    dst: Some(dst),
                    value:
                        Value::Fiber {
                            op: FiberOp::Fork | FiberOp::Deferred,
                            ..
                        },
                    ..
                } = statement
                else {
                    return None;
                };
                (function.vars[*dst].ty != TypeId::ANY).then_some((id, *dst))
            })
            .collect();
        let mut after = HashMap::new();
        for (statement, destination) in pending {
            let raw = function.vars.alloc(VarData {
                hint: "fiber_handle".into(),
                ty: TypeId::ANY,
                repr: Repr::Ref,
            });
            if let Stmt::Let { dst, .. } = &mut function.stmts[statement] {
                *dst = Some(raw);
            }
            let cast = function.stmts.alloc(Stmt::Let {
                dst: Some(destination),
                value: Value::Coerce(raw),
                on_defect: None,
                on_suspend: None,
            });
            after.insert(statement, cast);
        }
        for (_, sequence) in function.seqs.iter_mut() {
            sequence.stmts = sequence
                .stmts
                .iter()
                .flat_map(|id| std::iter::once(*id).chain(after.get(id).copied()))
                .collect();
        }
    }
}
