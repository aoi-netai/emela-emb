//! Fixed legacy tests/flow.rs ISR boundaries which require raw IR inputs.
use super::*;

fn interrupt_action(value: Value, nested: bool) -> Program {
    let mut p = program(vec![]);
    p.core.types.intern(Type::Cell(TypeId::INT));
    let mut action = simple();
    action.name = "action".into();
    action.export_symbol = Some("action".into());
    action.ret = TypeId::UNIT;
    let operation = binding(&mut action, None, value);
    action.seqs[action.body.unwrap()] = Seq {
        stmts: vec![operation],
        term: Term::Return(None),
    };
    assert!(action.vars.is_empty());
    let target = p.functions.alloc(action);
    if nested {
        let mut entry = simple();
        entry.ret = TypeId::UNIT;
        let call = binding(
            &mut entry,
            None,
            Value::Call {
                callee: Callee::Direct(target),
                args: vec![],
                suspends: false,
                on_fail: None,
            },
        );
        entry.seqs[entry.body.unwrap()] = Seq {
            stmts: vec![call],
            term: Term::Return(None),
        };
        p.functions.alloc(entry);
    }
    p
}

#[test]
fn isr_entries_reject_discarded_cells_in_direct_and_nested_calls() {
    for nested in [false, true] {
        let p = interrupt_action(
            Value::Builtin {
                op: Builtin::CellNew,
                args: vec![Const::Int(42).into()],
            },
            nested,
        );
        let options = Options::default().with_heap_bytes(1024);
        crate::backend::compile_program(p.clone(), &options).unwrap();
        let errors = crate::backend::compile_program(
            p,
            &options.with_isr_entry(if nested { "answer" } else { "action" }),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("heap-backed")
                    && error.message.contains("ISR entry")),
            "{errors:?}"
        );
    }
}
