//! The old Flow input tests still have representable residuals in our private contract.
use super::*;

#[test]
fn residual_heap_runtime_operations_are_rejected_in_native_and_suspending_input() {
    for suspends in [false, true] {
        for op in [
            RuntimeOp::Defect,
            RuntimeOp::DefectAt,
            RuntimeOp::DefectTake,
            RuntimeOp::DefectResume,
            RuntimeOp::Yield,
            RuntimeOp::YieldPerform,
        ] {
            let mut function = simple();
            function.ret = TypeId::UNIT;
            function.suspends = suspends;
            function.name = format!("residual_{op:?}_{suspends}");
            let args = match op {
                RuntimeOp::Defect => vec![Const::String("defect".into()).into()],
                RuntimeOp::DefectAt => vec![
                    Const::String("defect".into()).into(),
                    Const::String("main.emel:1:1".into()).into(),
                ],
                RuntimeOp::DefectResume => {
                    let pending = aggregate_variable(&mut function, TypeId::ANY);
                    function.params.push(pending);
                    vec![pending.into()]
                }
                _ => vec![],
            };
            let statement = binding(&mut function, None, Value::Runtime { op, args });
            function.seqs[function.body.unwrap()] = Seq {
                stmts: vec![statement],
                term: Term::Return(None),
            };
            let errors =
                crate::backend::compile_program(program(vec![function]), &Options::default())
                    .unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.kind == ErrorKind::Unsupported
                        && error.message.contains("value variant")),
                "{op:?}/{suspends}: {errors:?}"
            );
        }
    }
}

#[test]
fn explicit_defect_and_suspension_landing_edges_are_rejected_before_planning() {
    for suspend in [false, true] {
        let mut function = simple();
        function.ret = TypeId::UNIT;
        function.suspends = true;
        let landing = function.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(None),
        });
        let statement = function.stmts.alloc(Stmt::Let {
            dst: None,
            value: Value::Const(Const::Unit),
            on_defect: (!suspend).then_some(landing),
            on_suspend: suspend.then_some(Suspend {
                state: 0,
                saved: vec![],
            }),
        });
        function.seqs[function.body.unwrap()] = Seq {
            stmts: vec![statement],
            term: Term::Return(None),
        };
        let errors = crate::backend::compile_program(program(vec![function]), &Options::default())
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == ErrorKind::Unsupported
                    && error.message.contains("statement variant")),
            "{suspend}: {errors:?}"
        );
    }
}
