use super::*;

fn function_type(p: &mut Program, params: Vec<TypeId>, ret: TypeId, fails: bool) -> TypeId {
    p.core.types.intern(Type::Fn(FnType {
        params,
        ret,
        fails,
        suspends: true,
        ev: false,
        sets: None,
    }))
}
fn handle_type(p: &mut Program, name: &str, value: TypeId) -> TypeId {
    let id = p.core.enums.alloc(EnumDef {
        name: name.into(),
        params: 1,
        variants: vec![],
        error: false,
        external: true,
    });
    p.core.types.intern(Type::Enum(id, vec![value]))
}
fn audit_heap(output: &mut Output) {
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
}
fn execute_answer(mut output: Output) {
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);if(result!=42||!test_heap_empty())return 2;}return 0;}\n",
        0,
    );
}
fn scope_function(p: &mut Program, f: &mut Function, scope_body: SeqId, scope: VarId) {
    let ty = function_type(p, vec![TypeId::FIBER_SCOPE], TypeId::INT, false);
    let body = aggregate_variable(f, ty);
    let result = variable(f, TypeId::INT);
    let make = binding(
        f,
        Some(body),
        Value::Closure(Box::new(Closure {
            params: vec![scope],
            captures: vec![],
            body: scope_body,
            suspends: true,
        })),
    );
    let enter = binding(
        f,
        Some(result),
        Value::Fiber {
            op: FiberOp::Scope { fails: false },
            args: vec![body.into()],
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![make, enter],
        term: Term::Return(Some(result.into())),
    };
}

#[test]
fn scope_fork_and_join_reclaim_children_and_their_erased_values() {
    let mut p = program(vec![]);
    let child_type = function_type(&mut p, vec![], TypeId::ANY, false);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::INT);
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let value = variable(&mut f, TypeId::INT);
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let make_value = binding(&mut f, Some(value), Value::Const(Const::Int(42)));
    let box_value = binding(&mut f, Some(boxed), Value::Coerce(value));
    let child_body = f.seqs.alloc(Seq {
        stmts: vec![make_value, box_value],
        term: Term::Return(Some(boxed.into())),
    });
    let child = aggregate_variable(&mut f, child_type);
    let handle = aggregate_variable(&mut f, handle_type);
    let joined = variable(&mut f, TypeId::INT);
    let make_child = binding(
        &mut f,
        Some(child),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![],
            body: child_body,
            suspends: true,
        })),
    );
    let fork = binding(
        &mut f,
        Some(handle),
        Value::Fiber {
            op: FiberOp::Fork,
            args: vec![scope.into(), child.into()],
            on_fail: None,
        },
    );
    let join = binding(
        &mut f,
        Some(joined),
        Value::Fiber {
            op: FiberOp::Join,
            args: vec![handle.into()],
            on_fail: None,
        },
    );
    let scope_body = f.seqs.alloc(Seq {
        stmts: vec![make_child, fork, join],
        term: Term::Return(Some(joined.into())),
    });
    scope_function(&mut p, &mut f, scope_body, scope);
    p.functions.alloc(f);
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(1024),
    )
    .unwrap();
    execute_answer(output);
}
