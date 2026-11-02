//! Replace local failure landings with structured block exits before planning.
use super::*;

pub(super) fn normalize(program: &mut Program) {
    for (_, function) in program.functions.iter_mut() {
        let statements: Vec<_> = function.stmts.iter().map(|(id, _)| id).collect();
        for id in statements {
            let Stmt::Let {
                dst,
                mut value,
                on_defect,
                on_suspend,
            } = function.stmts[id].clone()
            else {
                continue;
            };
            let (Value::Call {
                on_fail:
                    Some(OnFail::Handle {
                        error,
                        body: handler,
                    }),
                ..
            }
            | Value::Fiber {
                on_fail:
                    Some(OnFail::Handle {
                        error,
                        body: handler,
                    }),
                ..
            }) = value
            else {
                continue;
            };
            let success_label = function.labels.alloc(LabelData {
                hint: "call_result".into(),
            });
            let failure_label = function.labels.alloc(LabelData {
                hint: "call_failure".into(),
            });
            let (Value::Call { on_fail, .. } | Value::Fiber { on_fail, .. }) = &mut value else {
                unreachable!()
            };
            *on_fail = Some(OnFail::Break(failure_label));
            let call = function.stmts.alloc(Stmt::Let {
                dst,
                value,
                on_defect,
                on_suspend,
            });
            let call_body = function.seqs.alloc(Seq {
                stmts: vec![call],
                term: Term::Break {
                    label: success_label,
                    value: dst.map(Operand::Var),
                },
            });
            let failure_block = function.stmts.alloc(Stmt::Block {
                dst: Some(error),
                label: failure_label,
                body: call_body,
            });
            let mut outer_body = function.seqs[handler].clone();
            outer_body.stmts.insert(0, failure_block);
            let outer_body = function.seqs.alloc(outer_body);
            function.stmts[id] = Stmt::Block {
                dst,
                label: success_label,
                body: outer_body,
            };
        }
    }
}
