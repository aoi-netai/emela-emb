//! Explicit fixed-old test adaptations after the normal source pipeline.
use crate::contract::ir::*;
use std::collections::HashSet;

/// Fixed-old tests/fiber/interrupt.rs::protect_cleanup_call. Move its final
/// ordinary `cleanup` call into Protect while preserving captured evidence.
pub(super) fn protect_cleanup_call(program: &mut Program) {
    let cleanup = program
        .functions
        .iter()
        .find(|(_, function)| function.name == "cleanup")
        .unwrap()
        .0;
    let mut count = 0;
    for (_, function) in program.functions.iter_mut() {
        let sequences = function
            .seqs
            .iter()
            .map(|(id, sequence)| (id, sequence.clone()))
            .collect::<Vec<_>>();
        for (id, sequence) in sequences {
            let Some(position) = sequence.stmts.iter().position(|statement| {
                matches!(&function.stmts[*statement], Stmt::Let {
                    value: Value::Call { callee: Callee::Direct(target), .. }, ..
                } if *target == cleanup)
            }) else {
                continue;
            };
            assert_eq!(position + 1, sequence.stmts.len());
            let Stmt::Let { value, .. } = &function.stmts[sequence.stmts[position]] else {
                unreachable!()
            };
            let mut required = HashSet::new();
            value.for_each_var(|variable| {
                required.insert(variable);
            });
            let mut prefix = HashSet::new();
            for statement in sequence.stmts[..position].iter().rev() {
                if let Stmt::Let {
                    dst: Some(dst),
                    value,
                    ..
                } = &function.stmts[*statement]
                {
                    if required.contains(dst) {
                        assert!(matches!(
                            value,
                            Value::EvExtend { .. } | Value::EvLookup { .. }
                        ));
                        prefix.insert(*statement);
                        value.for_each_var(|variable| {
                            required.insert(variable);
                        });
                    }
                }
            }
            let body = function.seqs.alloc(Seq {
                stmts: sequence.stmts[..position]
                    .iter()
                    .copied()
                    .filter(|statement| !prefix.contains(statement))
                    .collect(),
                term: sequence.term,
            });
            let cleanup = function.seqs.alloc(Seq {
                stmts: vec![sequence.stmts[position]],
                term: Term::Yield(None),
            });
            let (dst, term) = match &function.seqs[body].term {
                Term::Yield(Some(value)) => {
                    let (ty, repr) = match value {
                        Operand::Var(variable) => {
                            let variable = &function.vars[*variable];
                            (variable.ty, variable.repr)
                        }
                        Operand::Const(Const::Int(_)) => (TypeId::INT, Repr::I32),
                        _ => panic!("unsupported cleanup fixture result"),
                    };
                    let dst = function.vars.alloc(VarData {
                        hint: "protected_result".into(),
                        ty,
                        repr,
                    });
                    (Some(dst), Term::Yield(Some(dst.into())))
                }
                Term::Yield(None) => (None, Term::Yield(None)),
                _ => (None, Term::Unreachable),
            };
            let protect = function.stmts.alloc(Stmt::Protect { dst, body, cleanup });
            function.seqs[id] = Seq {
                stmts: sequence.stmts[..position]
                    .iter()
                    .copied()
                    .filter(|statement| prefix.contains(statement))
                    .chain(std::iter::once(protect))
                    .collect(),
                term,
            };
            count += 1;
        }
    }
    assert_eq!(count, 1);
}
