//! Fiber failure edges belong to the closure's lifted arena, including handler-only locals.
use super::*;

fn closure_with_fiber_failure_edge(local_handler: bool) -> Program {
    let mut p = program(vec![]);
    let (cause_type, exit, exit_type, _, _) = exit_types(&mut p, TypeId::INT);
    let error = p.core.enums.alloc(EnumDef {
        name: "CapturedFailure".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "CapturedFailure".into(),
            fields: VariantFields::Positional(1),
            tys: vec![TypeId::INT],
        }],
        error: true,
        external: false,
    });
    let error_type = p.core.types.enum_type(error, vec![]);
    let mut show = Function::new("show_captured_failure");
    show.ret = TypeId::STRING;
    let shown = aggregate_variable(&mut show, error_type);
    show.params.push(shown);
    show.body = Some(show.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::String("CapturedFailure".into()).into())),
    }));
    let show = p.functions.alloc(show);
    p.error_shows.push((error, show));
    let failing_type = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: true,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let pure_type = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut f = simple();
    // Parent IDs must differ from the compact lifted arena. An identity mapping
    // could otherwise conceal an unremapped SeqId, LabelId or handler local.
    for _ in 0..11 {
        variable(&mut f, TypeId::INT);
        f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(Const::Int(-1).into())),
        });
        f.labels.alloc(LabelData {
            hint: "padding".into(),
        });
    }
    let input = variable(&mut f, TypeId::INT);
    f.params.push(input);
    let payload = aggregate_variable(&mut f, error_type);
    let make_error = binding(
        &mut f,
        Some(payload),
        Value::Make {
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            args: vec![input.into()],
        },
    );
    let failing_body = f.seqs.alloc(Seq {
        stmts: vec![make_error],
        term: Term::Fail(payload.into()),
    });
    let failing = aggregate_variable(&mut f, failing_type);
    let make_failing = binding(
        &mut f,
        Some(failing),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![input],
            body: failing_body,
            suspends: false,
        })),
    );
    let outcome = aggregate_variable(&mut f, exit_type);
    let attempt = binding(
        &mut f,
        Some(outcome),
        Value::Fiber {
            op: FiberOp::Attempt {
                exit,
                suspends: false,
            },
            args: vec![failing.into()],
            on_fail: None,
        },
    );
    let cause = aggregate_variable(&mut f, cause_type);
    let extract = binding(
        &mut f,
        Some(cause),
        Value::Field {
            base: outcome,
            ctor: CtorRef {
                enum_id: exit,
                variant: EXIT_FAILURE,
            },
            index: 0,
        },
    );
    let caught = aggregate_variable(&mut f, TypeId::ANY);
    let recovered = aggregate_variable(&mut f, error_type);
    let result = variable(&mut f, TypeId::INT);
    let unpack = binding(
        &mut f,
        Some(recovered),
        Value::AsError {
            error: caught,
            enum_id: error,
            error_type,
        },
    );
    let read = binding(
        &mut f,
        Some(result),
        Value::Field {
            base: recovered,
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            index: 0,
        },
    );
    let closure_body = if local_handler {
        let handler = f.seqs.alloc(Seq {
            stmts: vec![unpack, read],
            term: Term::Return(Some(result.into())),
        });
        let raise = binding(
            &mut f,
            None,
            Value::Fiber {
                op: FiberOp::CauseRaise,
                args: vec![cause.into()],
                on_fail: Some(OnFail::Handle {
                    error: caught,
                    body: handler,
                }),
            },
        );
        f.seqs.alloc(Seq {
            stmts: vec![raise],
            term: Term::Unreachable,
        })
    } else {
        let label = f.labels.alloc(LabelData {
            hint: "recover".into(),
        });
        let raise = binding(
            &mut f,
            None,
            Value::Fiber {
                op: FiberOp::CauseRaise,
                args: vec![cause.into()],
                on_fail: Some(OnFail::Break(label)),
            },
        );
        let body = f.seqs.alloc(Seq {
            stmts: vec![raise],
            term: Term::Unreachable,
        });
        let block = f.stmts.alloc(Stmt::Block {
            dst: Some(caught),
            label,
            body,
        });
        f.seqs.alloc(Seq {
            stmts: vec![block, unpack, read],
            term: Term::Return(Some(result.into())),
        })
    };
    let callback = aggregate_variable(&mut f, pure_type);
    let make = binding(
        &mut f,
        Some(callback),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![cause],
            body: closure_body,
            suspends: false,
        })),
    );
    let returned = variable(&mut f, TypeId::INT);
    let invoke = binding(
        &mut f,
        Some(returned),
        Value::Call {
            callee: Callee::Indirect(callback),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![make_failing, attempt, extract, make, invoke],
        term: Term::Return(Some(returned.into())),
    };
    p.functions.alloc(f);
    p
}

fn execute_shifted_edge(local_handler: bool) {
    let mut output = crate::backend::compile_program(
        closure_with_fiber_failure_edge(local_handler),
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    audit_owned_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer(42)!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}

#[test]
fn lifting_a_closure_collects_and_remaps_fiber_local_failure_handlers() {
    execute_shifted_edge(true);
}

#[test]
fn lifting_a_closure_remaps_fiber_failure_break_labels_and_their_result_locals() {
    execute_shifted_edge(false);
}
