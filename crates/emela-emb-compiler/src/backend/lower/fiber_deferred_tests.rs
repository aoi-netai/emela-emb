use super::*;

#[test]
fn deferred_wakes_fifo_waiters_with_owned_strings_and_releases_the_scope_value() {
    let mut p = program(vec![]);
    let wait = wait_task(&mut p);
    let mut mark = Function::new("mark");
    mark.ret = TypeId::UNIT;
    let number = variable(&mut mark, TypeId::INT);
    mark.params.push(number);
    mark.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("mark".into()),
        }],
    });
    let mark = p.functions.alloc(mark);
    let deferred_type = handle_type(&mut p, "Std.Deferred.Deferred", TypeId::STRING);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::STRING);
    let child_type = function_type(&mut p, vec![], TypeId::ANY, false);
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let deferred = aggregate_variable(&mut f, deferred_type);
    let make = binding(
        &mut f,
        Some(deferred),
        Value::Fiber {
            op: FiberOp::Deferred,
            args: vec![scope.into()],
            on_fail: None,
        },
    );
    let mut statements = vec![make];
    let mut handles = vec![];
    for marker in [1, 2] {
        let received = aggregate_variable(&mut f, TypeId::STRING);
        let await_ = binding(
            &mut f,
            Some(received),
            Value::Fiber {
                op: FiberOp::DeferredAwait,
                args: vec![deferred.into()],
                on_fail: None,
            },
        );
        let observe = binding(
            &mut f,
            None,
            Value::Call {
                callee: Callee::Direct(mark),
                args: vec![Const::Int(marker).into()],
                suspends: false,
                on_fail: None,
            },
        );
        let boxed = aggregate_variable(&mut f, TypeId::ANY);
        let box_value = binding(&mut f, Some(boxed), Value::Coerce(received));
        let child_body = f.seqs.alloc(Seq {
            stmts: vec![await_, observe, box_value],
            term: Term::Return(Some(boxed.into())),
        });
        let child = aggregate_variable(&mut f, child_type);
        statements.push(binding(
            &mut f,
            Some(child),
            Value::Closure(Box::new(Closure {
                params: vec![],
                captures: vec![deferred],
                body: child_body,
                suspends: true,
            })),
        ));
        let handle = aggregate_variable(&mut f, handle_type);
        statements.push(binding(
            &mut f,
            Some(handle),
            Value::Fiber {
                op: FiberOp::Fork,
                args: vec![scope.into(), child.into()],
                on_fail: None,
            },
        ));
        handles.push(handle);
    }
    statements.push(wait_at(&mut f, wait, 2));
    let value = aggregate_variable(&mut f, TypeId::STRING);
    statements.push(binding(
        &mut f,
        Some(value),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![Const::Int(42).into()],
        },
    ));
    let completed = variable(&mut f, TypeId::BOOL);
    statements.push(binding(
        &mut f,
        Some(completed),
        Value::Fiber {
            op: FiberOp::Complete,
            args: vec![deferred.into(), value.into()],
            on_fail: None,
        },
    ));
    statements.push(check_bool(&mut f, completed, true));
    for handle in handles {
        let received = aggregate_variable(&mut f, TypeId::STRING);
        statements.push(binding(
            &mut f,
            Some(received),
            Value::Fiber {
                op: FiberOp::Join,
                args: vec![handle.into()],
                on_fail: None,
            },
        ));
        let equal = variable(&mut f, TypeId::BOOL);
        statements.push(binding(
            &mut f,
            Some(equal),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::String,
                args: vec![received.into(), Const::String("42".into()).into()],
            },
        ));
        statements.push(check_bool(&mut f, equal, true));
    }
    let body = f.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Return(Some(Const::Int(42).into())),
    });
    scope_function(&mut p, &mut f, body, scope);
    p.functions.alloc(f);
    let mut output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(16384)
            .with_frame_bytes(2048)
            .with_deferred_capacity(1)
            .with_task_adapter("wait", "board_wait_start"),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask pending;static int trace;\nvoid mark(int32_t marker){trace=trace*10+marker;}\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t marker){if(marker!=2)abort();pending=t;}\nint main(void){for(int i=0;i<1000;i++){trace=0;int32_t result=0;EmelaFiber root=answer_start(&result);while(emela_scheduler_run(1)){}if(trace||result||emela_fiber_done(root))return 1;if(!emela_task_complete_unit(pending))return 2;while(!emela_fiber_done(root))if(!emela_scheduler_run(1))return 3;emela_fiber_release(root);if(trace!=12||result!=42||!test_heap_empty())return 4;}return 0;}\n",
        0,
    );
}
