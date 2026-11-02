//! One fork-operation ABI for Scope instances with different error sets.
use std::collections::HashMap;

use crate::contract::ir::{Callee, Function, OnFail, Program as Module, SeqId, Stmt, Term, Value};
use crate::contract::ir::{Type, TypeId, TypeTable};

use crate::{Error, ErrorKind};

pub(super) fn normalize(module: &mut Module) -> Result<(), Vec<Error>> {
    let mut reachable: Vec<_> = module.functions.iter().map(|(id, _)| id).collect();
    reachable.sort_by_key(|id| id.into_raw());
    let mut operations = HashMap::new();
    for &id in &reachable {
        let f = &module.functions[id];
        for (_, stmt) in f.stmts.iter() {
            let Stmt::Let {
                value: Value::Handler { effect, ops, .. },
                ..
            } = stmt
            else {
                continue;
            };
            if !is_scope(&module.core.effects[*effect]) {
                continue;
            }
            let mut types: Vec<_> = ops.iter().map(|v| f.vars[*v].ty).collect();
            if let Some(previous) = operations.get(effect) {
                let previous: &Vec<TypeId> = previous;
                for (ty, &other) in types.iter_mut().zip(previous) {
                    *ty = merge_operation(&mut module.core.types, *ty, other).ok_or_else(|| {
                        unsupported(&f.name, "Scope operations with different callback layouts")
                    })?;
                }
            }
            operations.insert(*effect, types);
        }
    }
    for (&effect, operations) in &operations {
        for (operation, &ty) in module.core.effects[effect].ops.iter_mut().zip(operations) {
            let Type::Fn(ft) = &module.core.types[ty] else {
                unreachable!()
            };
            operation.type_params = 0;
            operation.params = ft.params.clone();
            operation.ret = ft.ret;
            operation.fails = ft.fails;
        }
    }
    for id in reachable {
        let f = &mut module.functions[id];
        let mut pending = Vec::new();
        for (_, stmt) in f.stmts.iter() {
            let Stmt::Let {
                value: Value::Handler { effect, ops, .. },
                ..
            } = stmt
            else {
                continue;
            };
            let Some(expected) = operations.get(effect) else {
                continue;
            };
            pending.extend(
                ops.iter()
                    .zip(expected)
                    .filter_map(|(&v, &ty)| (f.vars[v].ty != ty).then_some((v, ty))),
            );
        }
        for (operation, ty) in pending {
            let closure = f
                .stmts
                .iter()
                .find_map(|(_, stmt)| match stmt {
                    Stmt::Let {
                        dst: Some(dst),
                        value: Value::Closure(c),
                        ..
                    } if *dst == operation => Some(c.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    unsupported(&f.name, "Scope operation without its generated closure")
                })?;
            let Type::Fn(ft) = module.core.types[ty].clone() else {
                unreachable!()
            };
            for (&parameter, &expected) in closure.params.iter().zip(&ft.params) {
                f.vars[parameter].ty = expected;
            }
            f.vars[operation].ty = ty;
            if adapt_body(&mut module.core.types, f, closure.body)? && !ft.fails {
                return Err(unsupported(&f.name, "failing Scope operation body"));
            }
        }
    }
    Ok(())
}

fn merge_operation(types: &mut TypeTable, left: TypeId, right: TypeId) -> Option<TypeId> {
    if left == right {
        return Some(left);
    }
    let (Type::Fn(mut a), Type::Fn(b)) = (types[left].clone(), types[right].clone()) else {
        return None;
    };
    if a.params.len() != b.params.len() {
        return None;
    }
    for (arg, &other) in a.params.iter_mut().zip(&b.params) {
        if *arg == other {
            continue;
        }
        let (Type::Fn(mut x), Type::Fn(mut y)) = (types[*arg].clone(), types[other].clone()) else {
            return None;
        };
        let fails = x.fails || y.fails;
        x.fails = false;
        y.fails = false;
        if x != y {
            return None;
        }
        x.fails = fails;
        *arg = types.intern(Type::Fn(x));
    }
    let mut check = a.clone();
    check.params = b.params.clone();
    if check != b {
        return None;
    }
    Some(types.intern(Type::Fn(a)))
}

fn adapt_body(types: &mut TypeTable, f: &mut Function, seq: SeqId) -> Result<bool, Vec<Error>> {
    let mut fails = matches!(f.seqs[seq].term, Term::Fail(_));
    for id in f.seqs[seq].stmts.clone() {
        match f.stmts[id].clone() {
            Stmt::Let {
                dst: Some(dst),
                value: Value::Closure(c),
                ..
            } => {
                let nested = adapt_body(types, f, c.body)?;
                if nested {
                    let Type::Fn(mut ft) = types[f.vars[dst].ty].clone() else {
                        unreachable!()
                    };
                    ft.fails = true;
                    f.vars[dst].ty = types.intern(Type::Fn(ft));
                }
            }
            Stmt::Let {
                value:
                    Value::Call {
                        callee: Callee::Indirect(callee),
                        on_fail,
                        ..
                    },
                ..
            } => {
                if matches!(&types[f.vars[callee].ty], Type::Fn(ft) if ft.fails) {
                    if on_fail.is_none() {
                        let Stmt::Let {
                            value: Value::Call { on_fail, .. },
                            ..
                        } = &mut f.stmts[id]
                        else {
                            unreachable!()
                        };
                        *on_fail = Some(OnFail::Propagate);
                        fails = true;
                    } else {
                        fails |= on_fail == Some(OnFail::Propagate);
                    }
                }
            }
            Stmt::Let {
                value: Value::Coerce(source),
                dst: Some(dst),
                ..
            } => {
                // Scope's generated aliases keep the widened callback ABI inside the operation.
                if let (Type::Fn(from), Type::Fn(to)) =
                    (&types[f.vars[source].ty], &types[f.vars[dst].ty])
                {
                    let mut expected = to.clone();
                    expected.fails = from.fails;
                    if &expected == from {
                        f.vars[dst].ty = f.vars[source].ty;
                    }
                }
            }
            Stmt::If { then, else_, .. } => {
                fails |= adapt_body(types, f, then)?;
                fails |= adapt_body(types, f, else_)?;
            }
            Stmt::Switch { cases, default, .. } => {
                for case in cases {
                    fails |= adapt_body(types, f, case.body)?;
                }
                if let Some(body) = default {
                    fails |= adapt_body(types, f, body)?;
                }
            }
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => {
                fails |= adapt_body(types, f, body)?
            }
            Stmt::Protect { body, cleanup, .. } => {
                fails |= adapt_body(types, f, body)?;
                if adapt_body(types, f, cleanup)? {
                    return Err(unsupported(&f.name, "failing Scope operation cleanup"));
                }
            }
            _ => {}
        }
    }
    Ok(fails)
}

fn unsupported(function: &str, detail: &str) -> Vec<Error> {
    vec![Error {
        kind: ErrorKind::Unsupported,
        message: format!(
            "embedded C codegen does not support {detail} in function `{function}` yet"
        ),
    }]
}

/// Scope's row arguments are erased from its trusted evidence key by frontend synthesis.
pub(super) fn is_scope(effect: &crate::contract::ir::EffectDef) -> bool {
    effect.decl == "Std.Fiber.Scope" || effect.name.starts_with("Std.Fiber.Scope[")
}

pub(super) fn normalize_calls(module: &mut Module) {
    let mut operations = HashMap::new();
    for (_, f) in module.functions.iter() {
        for (_, s) in f.stmts.iter() {
            if let Stmt::Let {
                value: Value::Handler { effect, ops, .. },
                ..
            } = s
            {
                operations.insert(
                    *effect,
                    ops.iter().map(|v| f.vars[*v].ty).collect::<Vec<_>>(),
                );
            }
        }
    }
    for id in {
        let mut ids: Vec<_> = module.functions.iter().map(|(id, _)| id).collect();
        ids.sort_by_key(|id| id.into_raw());
        ids
    } {
        let mut before: HashMap<_, Vec<_>> = HashMap::new();
        let mut after = HashMap::new();
        let f = &mut module.functions[id];
        for (stmt, s) in f.stmts.clone().iter() {
            let Stmt::Let {
                dst,
                value:
                    Value::Call {
                        callee: crate::contract::ir::Callee::Op { effect, op, .. },
                        args,
                        ..
                    },
                ..
            } = s
            else {
                continue;
            };
            if !is_scope(&module.core.effects[*effect])
                || !matches!(
                    module.core.effects[*effect].ops[*op].name.as_str(),
                    "fork" | "deferred"
                )
            {
                continue;
            }
            if let Some(Type::Fn(ft)) = operations
                .get(effect)
                .and_then(|ops| ops.get(*op))
                .map(|t| &module.core.types[*t])
            {
                for (i, (arg, &expected)) in args.iter().zip(&ft.params).enumerate() {
                    let crate::contract::ir::Operand::Var(arg) = arg else {
                        continue;
                    };
                    let actual = f.vars[*arg].ty;
                    if actual == expected {
                        continue;
                    }
                    let (Type::Fn(from), Type::Fn(to)) =
                        (&module.core.types[actual], &module.core.types[expected])
                    else {
                        continue;
                    };
                    if from.params != to.params || from.ev != to.ev {
                        continue;
                    }
                    let adapted = f.vars.alloc(crate::contract::ir::VarData {
                        hint: "scope_argument".into(),
                        ty: expected,
                        repr: crate::contract::ir::Repr::Ref,
                    });
                    let value = if from.ret == to.ret {
                        Value::Coerce(*arg)
                    } else if to.ret == TypeId::ANY
                        && !matches!(from.ret, TypeId::UNIT | TypeId::NEVER)
                    {
                        // Flow can pass reference results directly to a polymorphic operation.
                        // Our tagged Any ABI needs an explicit box, including for String/List.
                        let params: Vec<_> = from
                            .params
                            .iter()
                            .copied()
                            .chain(from.ev.then_some(TypeId::EVIDENCE))
                            .filter(|ty| {
                                repr_of(&module.core, *ty) != crate::contract::ir::Repr::Void
                            })
                            .map(|ty| {
                                f.vars.alloc(crate::contract::ir::VarData {
                                    hint: "child_arg".into(),
                                    ty,
                                    repr: repr_of(&module.core, ty),
                                })
                            })
                            .collect();
                        let result = f.vars.alloc(crate::contract::ir::VarData {
                            hint: "child_value".into(),
                            ty: from.ret,
                            repr: repr_of(&module.core, from.ret),
                        });
                        let boxed = f.vars.alloc(crate::contract::ir::VarData {
                            hint: "child_box".into(),
                            ty: TypeId::ANY,
                            repr: crate::contract::ir::Repr::Ref,
                        });
                        let call = f.stmts.alloc(Stmt::Let {
                            dst: Some(result),
                            value: Value::Call {
                                callee: crate::contract::ir::Callee::Indirect(*arg),
                                args: params.iter().copied().map(Into::into).collect(),
                                suspends: from.suspends,
                                on_fail: from
                                    .fails
                                    .then_some(crate::contract::ir::OnFail::Propagate),
                            },
                            on_defect: None,
                            on_suspend: None,
                        });
                        let box_value = f.stmts.alloc(Stmt::Let {
                            dst: Some(boxed),
                            value: Value::Coerce(result),
                            on_defect: None,
                            on_suspend: None,
                        });
                        let body = f.seqs.alloc(crate::contract::ir::Seq {
                            stmts: vec![call, box_value],
                            term: Term::Return(Some(boxed.into())),
                        });
                        Value::Closure(Box::new(crate::contract::ir::Closure {
                            params,
                            captures: vec![*arg],
                            body,
                            suspends: to.suspends,
                        }))
                    } else {
                        continue;
                    };
                    let convert = f.stmts.alloc(Stmt::Let {
                        dst: Some(adapted),
                        value,
                        on_defect: None,
                        on_suspend: None,
                    });
                    before.entry(stmt).or_default().push(convert);
                    if let Stmt::Let {
                        value: Value::Call { args, .. },
                        ..
                    } = &mut f.stmts[stmt]
                    {
                        args[i] = adapted.into();
                    }
                }
            }
            let Some(dst) = dst.filter(|v| f.vars[*v].ty != TypeId::ANY) else {
                continue;
            };
            let raw = f.vars.alloc(crate::contract::ir::VarData {
                hint: "scope_result".into(),
                ty: TypeId::ANY,
                repr: crate::contract::ir::Repr::Ref,
            });
            if let Stmt::Let { dst, .. } = &mut f.stmts[stmt] {
                *dst = Some(raw);
            }
            let convert = f.stmts.alloc(Stmt::Let {
                dst: Some(dst),
                value: Value::Coerce(raw),
                on_defect: None,
                on_suspend: None,
            });
            after.insert(stmt, convert);
        }
        for (_, seq) in f.seqs.iter_mut() {
            seq.stmts = seq
                .stmts
                .iter()
                .flat_map(|s| {
                    before
                        .get(s)
                        .into_iter()
                        .flatten()
                        .copied()
                        .chain(std::iter::once(*s))
                        .chain(after.get(s).copied())
                })
                .collect();
        }
    }
}

fn repr_of(core: &crate::contract::ir::Definitions, ty: TypeId) -> crate::contract::ir::Repr {
    use crate::contract::ir::Repr;
    match core.ty(ty) {
        Type::Int | Type::Bool => Repr::I32,
        Type::Int64 => Repr::I64,
        Type::Float => Repr::F64,
        Type::Unit | Type::Never => Repr::Void,
        Type::Enum(id, _)
            if !core.enums[*id].external
                && !core.enums[*id].error
                && !core.enums[*id].variants.is_empty()
                && core.enums[*id].variants.iter().all(|v| v.tys.is_empty()) =>
        {
            Repr::I32
        }
        _ => Repr::Ref,
    }
}
