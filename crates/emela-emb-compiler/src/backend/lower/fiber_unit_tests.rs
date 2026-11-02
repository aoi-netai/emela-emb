use super::*;

#[test]
fn unit_any_boxes_round_trip_without_c_storage_and_release_their_heap() {
    let mut f = simple();
    let unit = variable(&mut f, TypeId::UNIT);
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let restored = variable(&mut f, TypeId::UNIT);
    let make = binding(&mut f, Some(unit), Value::Const(Const::Unit));
    let box_ = binding(&mut f, Some(boxed), Value::Coerce(unit));
    let unbox = binding(&mut f, Some(restored), Value::Coerce(boxed));
    f.seqs[f.body.unwrap()].stmts = vec![make, box_, unbox];
    let mut output = crate::backend::compile_program(
        program(vec![f]),
        &Options::default().with_heap_bytes(1024),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}

#[test]
fn unit_children_keep_side_effects_and_join_without_a_result_slot() {
    let mut p = program(vec![]);
    let mut mark = Function::new("mark");
    mark.ret = TypeId::UNIT;
    mark.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("mark".into()),
        }],
    });
    let mark = p.functions.alloc(mark);
    let child_type = function_type(&mut p, vec![], TypeId::ANY, false);
    let handle_type = handle_type(&mut p, "Std.Fiber.Fiber", TypeId::UNIT);
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let unit = variable(&mut f, TypeId::UNIT);
    let boxed = aggregate_variable(&mut f, TypeId::ANY);
    let observe = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(mark),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let make_unit = binding(&mut f, Some(unit), Value::Const(Const::Unit));
    let box_value = binding(&mut f, Some(boxed), Value::Coerce(unit));
    let child_body = f.seqs.alloc(Seq {
        stmts: vec![observe, make_unit, box_value],
        term: Term::Return(Some(boxed.into())),
    });
    let child = aggregate_variable(&mut f, child_type);
    let handle = aggregate_variable(&mut f, handle_type);
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
        None,
        Value::Fiber {
            op: FiberOp::Join,
            args: vec![handle.into()],
            on_fail: None,
        },
    );
    let scope_body = f.seqs.alloc(Seq {
        stmts: vec![make_child, fork, join],
        term: Term::Return(Some(Const::Int(42).into())),
    });
    scope_function(&mut p, &mut f, scope_body, scope);
    p.functions.alloc(f);
    let mut output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(1024),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);static int marks;void mark(void){marks++;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){marks=0;int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);if(result!=42||marks!=1||!test_heap_empty())return 2;}return 0;}\n",
        0,
    );
}

#[test]
fn unit_deferred_distinguishes_empty_and_full_without_payload_storage() {
    let mut p = program(vec![]);
    let deferred_type = handle_type(&mut p, "Std.Deferred.Deferred", TypeId::UNIT);
    let param = p.core.types.intern(Type::Param(0));
    let option = p.core.enums.alloc(EnumDef {
        name: "Option".into(),
        params: 1,
        variants: vec![
            VariantDef {
                name: "Some".into(),
                fields: VariantFields::Positional(1),
                tys: vec![param],
            },
            VariantDef {
                name: "None".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
        error: false,
        external: false,
    });
    let option_type = p.core.types.intern(Type::Enum(option, vec![TypeId::UNIT]));
    let mut f = simple();
    f.suspends = true;
    let scope = aggregate_variable(&mut f, TypeId::FIBER_SCOPE);
    let deferred = aggregate_variable(&mut f, deferred_type);
    let mut statements = vec![binding(
        &mut f,
        Some(deferred),
        Value::Fiber {
            op: FiberOp::Deferred,
            args: vec![scope.into()],
            on_fail: None,
        },
    )];
    for full in [false, true] {
        let poll = aggregate_variable(&mut f, option_type);
        statements.push(binding(
            &mut f,
            Some(poll),
            Value::Fiber {
                op: FiberOp::Poll,
                args: vec![deferred.into()],
                on_fail: None,
            },
        ));
        let good = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Yield(None),
        });
        let bad = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Defect(Const::String("unexpected Unit Deferred poll".into()).into()),
        });
        statements.push(f.stmts.alloc(Stmt::Switch {
            dst: None,
            on: SwitchOn::Tag(poll),
            cases: vec![Case {
                key: CaseKey::Variant(if full { 0 } else { 1 }),
                body: good,
            }],
            default: Some(bad),
        }));
        let completed = variable(&mut f, TypeId::BOOL);
        statements.push(binding(
            &mut f,
            Some(completed),
            Value::Fiber {
                op: FiberOp::Complete,
                args: vec![deferred.into(), Const::Unit.into()],
                on_fail: None,
            },
        ));
        statements.push(check_bool(&mut f, completed, !full));
    }
    statements.push(binding(
        &mut f,
        None,
        Value::Fiber {
            op: FiberOp::DeferredAwait,
            args: vec![deferred.into()],
            on_fail: None,
        },
    ));
    let body = f.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Return(Some(Const::Int(42).into())),
    });
    scope_function(&mut p, &mut f, body, scope);
    p.functions.alloc(f);
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(1024)
            .with_deferred_capacity(1),
    )
    .unwrap();
    execute_answer(output);
}
