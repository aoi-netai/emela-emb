//! Give frame planning an owned function with a private call-failure landing slot.
use super::*;
use crate::contract::lowered::SuspensionInput;

pub(super) fn prepare(program: &Program) -> HashMap<FnId, SuspensionInput> {
    program
        .functions
        .iter()
        .filter_map(|(id, function)| {
            if !function.suspends || function.external.is_some() {
                return None;
            }
            let mut function = function.clone();
            let error_variable = function.vars.alloc(VarData {
                hint: "call_error".into(),
                ty: TypeId::ANY,
                repr: Repr::Ref,
            });
            let error_label = function.labels.alloc(LabelData {
                hint: "call_error".into(),
            });
            Some((
                id,
                SuspensionInput {
                    function,
                    error_variable,
                    error_label,
                },
            ))
        })
        .collect()
}

/// Suspend declarations have one runtime slot ABI even when an implementation
/// resumes immediately. A sync closure can implement that slot through Coerce.
pub(super) fn normalize_handlers(program: &mut Program) {
    for (_, function) in program.functions.iter_mut() {
        let mut inserted = HashMap::new();
        let handlers: Vec<_> = function
            .stmts
            .iter()
            .filter_map(|(id, statement)| {
                let Stmt::Let {
                    value: Value::Handler { effect, ops, .. },
                    ..
                } = statement
                else {
                    return None;
                };
                Some((id, *effect, ops.clone()))
            })
            .collect();
        for (statement, effect, mut operations) in handlers {
            let mut coercions = Vec::new();
            for (index, operation) in operations.iter_mut().enumerate() {
                if !program.core.effects[effect].ops[index].suspend {
                    continue;
                }
                let Type::Fn(mut ft) = program.core.types[function.vars[*operation].ty].clone()
                else {
                    continue;
                };
                if ft.suspends {
                    continue;
                }
                ft.suspends = true;
                let ty = program.core.types.intern(Type::Fn(ft));
                let target = function.vars.alloc(VarData {
                    hint: "suspend_operation".into(),
                    repr: Repr::Ref,
                    ty,
                });
                coercions.push(function.stmts.alloc(Stmt::Let {
                    dst: Some(target),
                    value: Value::Coerce(*operation),
                    on_defect: None,
                    on_suspend: None,
                }));
                *operation = target;
            }
            if let Stmt::Let {
                value: Value::Handler { ops, .. },
                ..
            } = &mut function.stmts[statement]
            {
                *ops = operations;
            }
            inserted.insert(statement, coercions);
        }
        for (_, sequence) in function.seqs.iter_mut() {
            sequence.stmts = sequence
                .stmts
                .iter()
                .flat_map(|id| {
                    inserted
                        .get(id)
                        .into_iter()
                        .flatten()
                        .copied()
                        .chain(std::iter::once(*id))
                })
                .collect();
        }
    }
}
