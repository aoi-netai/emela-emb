use super::*;

#[test]
fn external_task_resumes_an_independent_frame_and_rejects_repeated_completion() {
    let mut p = program(vec![]);
    let mut wait = Function::new("wait");
    wait.suspends = true;
    wait.ret = TypeId::INT;
    wait.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("wait".into()),
        }],
    });
    let n = variable(&mut wait, TypeId::INT);
    wait.params.push(n);
    let wait = p.functions.alloc(wait);
    let mut answer = simple();
    answer.suspends = true;
    let result = variable(&mut answer, TypeId::INT);
    let call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(42).into()],
            suspends: true,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    let answer = p.functions.alloc(answer);
    let lowered = lower_program(p.clone()).unwrap();
    assert!(lowered.suspensions.contains_key(&answer));
    let options = Options::default().with_task_adapter("wait", "board_wait_start");
    let output = crate::backend::compile_program(p.clone(), &options).unwrap();
    assert!(output.exports[0].suspends);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic EmelaTask pending;\nstatic int32_t value;\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t n){pending=t;value=n;}\nint main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber f=answer_start(&result);if(emela_fiber_done(f))return 1;while(emela_scheduler_run(1)){}if(emela_fiber_done(f)||result)return 2;if(!emela_task_complete_int(pending,value)||emela_task_complete_int(pending,7))return 3;if(emela_fiber_done(f)||result)return 4;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 5;if(result!=42)return 6;emela_fiber_release(f);if(emela_task_complete_int(pending,8))return 7;}return 0;}\n",
        0,
    );
    let mut invalid = p;
    invalid.functions[answer].suspends = false;
    assert!(
        lower_program(invalid).unwrap_err()[0]
            .message
            .contains("requires a suspending caller")
    );
}

fn external_task(program: &mut Program, name: &str, params: &[TypeId], ret: TypeId) -> FnId {
    let mut function = Function::new(name);
    function.suspends = true;
    function.ret = ret;
    function.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global(name.into()),
        }],
    });
    for &ty in params {
        let param = variable(&mut function, ty);
        function.params.push(param);
    }
    program.functions.alloc(function)
}
fn audit_heap(output: &mut Output) {
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
}

#[test]
fn typed_task_producers_run_on_main_and_their_string_survives_another_suspension() {
    let mut p = program(vec![]);
    let text = external_task(&mut p, "text", &[], TypeId::STRING);
    let wait = external_task(&mut p, "wait", &[TypeId::INT], TypeId::INT);
    let mut answer = simple();
    answer.suspends = true;
    answer.ret = TypeId::STRING;
    let value = aggregate_variable(&mut answer, TypeId::STRING);
    let result = aggregate_variable(&mut answer, TypeId::STRING);
    let get = binding(
        &mut answer,
        Some(value),
        Value::Call {
            callee: Callee::Direct(text),
            args: vec![],
            suspends: true,
            on_fail: None,
        },
    );
    let wait = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(42).into()],
            suspends: true,
            on_fail: None,
        },
    );
    let append = binding(
        &mut answer,
        Some(result),
        Value::Builtin {
            op: Builtin::StringAppend,
            args: vec![value.into(), Const::String("!".into()).into()],
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![get, wait, append],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_frame_bytes(1024)
        .with_task_adapter("text", "board_text_start")
        .with_task_adapter("wait", "board_wait_start");
    let mut output = crate::backend::compile_program(p, &options).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask pending_text,pending_wait;\nstatic int text_started,wait_started,produced,released;\nstatic char buffer[6];\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_text_start(EmelaTask t){pending_text=t;text_started++;}\nvoid board_wait_start(EmelaTask t,int32_t n){if(n!=42)abort();pending_wait=t;wait_started++;}\nstatic void produce(void*ctx,EmelaString*out){produced++;*out=(EmelaString){.data=ctx,.length=5};}\nstatic void release(void*ctx){released++;memset(ctx,'x',5);}\nint main(void){for(int i=0;i<1000;i++){memcpy(buffer,\"forty\",6);EmelaString result={0};EmelaFiber f=answer_start(&result);while(emela_scheduler_run(1)){}if(text_started!=i+1||wait_started!=i||produced!=i||released!=i)return 1;if(!emela_task_complete_value_4(pending_text,produce,buffer,release)||emela_task_complete_value_4(pending_text,produce,buffer,release))return 2;if(produced!=i||released!=i||emela_fiber_done(f))return 3;while(emela_scheduler_run(1)){}if(wait_started!=i+1||produced!=i+1||released!=i+1||emela_fiber_done(f)||result.length)return 4;if(!emela_task_complete_int(pending_wait,0))return 5;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 6;emela_fiber_release(f);if(!result.owner||result.length!=6||memcmp(result.data,\"forty!\",6))return 7;emela_embedded_release_string(&result);if(!test_heap_empty())return 8;}return 0;}\n",
        0,
    );
}

#[test]
fn suspending_closures_keep_managed_captures_and_return_owned_strings() {
    let mut p = program(vec![]);
    let wait = external_task(&mut p, "wait", &[], TypeId::UNIT);
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::STRING,
        fails: false,
        suspends: true,
        ev: false,
        sets: None,
    }));
    let mut answer = simple();
    answer.suspends = true;
    answer.ret = TypeId::STRING;
    let text = aggregate_variable(&mut answer, TypeId::STRING);
    let closure = aggregate_variable(&mut answer, ft);
    let result = aggregate_variable(&mut answer, TypeId::STRING);
    let capture = binding(
        &mut answer,
        Some(text),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![Const::Int(42).into()],
        },
    );
    let wait = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![],
            suspends: true,
            on_fail: None,
        },
    );
    let body = answer.seqs.alloc(Seq {
        stmts: vec![wait],
        term: Term::Return(Some(text.into())),
    });
    let make = binding(
        &mut answer,
        Some(closure),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![text],
            body,
            suspends: true,
        })),
    );
    let call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(closure),
            args: vec![],
            suspends: true,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![capture, make, call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_frame_bytes(1024)
        .with_task_adapter("wait", "board_wait_start");
    let mut output = crate::backend::compile_program(p, &options).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask pending;\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t){pending=t;}\nint main(void){for(int i=0;i<1000;i++){EmelaString result={0};EmelaFiber f=answer_start(&result);while(emela_scheduler_run(1)){}if(emela_fiber_done(f)||result.length)return 1;if(!emela_task_complete_unit(pending))return 2;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 3;emela_fiber_release(f);if(!result.owner||result.length!=2||memcmp(result.data,\"42\",2))return 4;emela_embedded_release_string(&result);if(!test_heap_empty())return 5;}return 0;}\n",
        0,
    );
}

fn operation_program(delayed: bool, duplicate: bool) -> Program {
    let mut p = program(vec![]);
    let effect = p.core.effects.alloc(EffectDef {
        name: "Wait".into(),
        decl: "Wait".into(),
        args: vec![],
        label_of: None,
        ops: vec![OpDef {
            name: "get".into(),
            suspend: true,
            type_params: 0,
            params: vec![TypeId::INT],
            ret: TypeId::INT,
            fails: false,
        }],
    });
    let resume_type = p.core.types.intern(Type::Fn(FnType {
        params: vec![TypeId::INT],
        ret: TypeId::UNIT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let operation_type = p.core.types.intern(Type::Fn(FnType {
        params: vec![TypeId::INT, resume_type],
        ret: TypeId::UNIT,
        fails: false,
        suspends: delayed,
        ev: false,
        sets: None,
    }));
    let handler_type = p.core.types.intern(Type::Handler(effect));
    let wait = delayed.then(|| external_task(&mut p, "wait", &[TypeId::INT], TypeId::INT));
    let mut mark = Function::new("mark");
    mark.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("mark".into()),
        }],
    });
    let number = variable(&mut mark, TypeId::INT);
    mark.params.push(number);
    let mark = p.functions.alloc(mark);
    let mut answer = simple();
    answer.suspends = delayed;
    let operation = aggregate_variable(&mut answer, operation_type);
    let input = variable(&mut answer, TypeId::INT);
    let resume = aggregate_variable(&mut answer, resume_type);
    let handler = aggregate_variable(&mut answer, handler_type);
    let resumed = variable(&mut answer, TypeId::INT);
    let result = variable(&mut answer, TypeId::INT);
    let mark1 = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(mark),
            args: vec![Const::Int(1).into()],
            suspends: false,
            on_fail: None,
        },
    );
    let mut statements = vec![mark1];
    let value = if let Some(wait) = wait {
        let waited = variable(&mut answer, TypeId::INT);
        statements.push(binding(
            &mut answer,
            Some(waited),
            Value::Call {
                callee: Callee::Direct(wait),
                args: vec![input.into()],
                suspends: true,
                on_fail: None,
            },
        ));
        waited
    } else {
        input
    };
    statements.push(binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Indirect(resume),
            args: vec![value.into()],
            suspends: false,
            on_fail: None,
        },
    ));
    if duplicate {
        statements.push(binding(
            &mut answer,
            None,
            Value::Call {
                callee: Callee::Indirect(resume),
                args: vec![value.into()],
                suspends: false,
                on_fail: None,
            },
        ));
    }
    statements.push(binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(mark),
            args: vec![Const::Int(2).into()],
            suspends: false,
            on_fail: None,
        },
    ));
    let body = answer.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Return(None),
    });
    let closure = binding(
        &mut answer,
        Some(operation),
        Value::Closure(Box::new(Closure {
            params: vec![input, resume],
            captures: vec![],
            body,
            suspends: delayed,
        })),
    );
    let make_handler = binding(
        &mut answer,
        Some(handler),
        Value::Handler {
            name: "Waiting".into(),
            effect,
            ops: vec![operation],
        },
    );
    let call = binding(
        &mut answer,
        Some(resumed),
        Value::Call {
            callee: Callee::Op {
                handler,
                effect,
                op: 0,
            },
            args: vec![Const::Int(41).into()],
            suspends: delayed,
            on_fail: None,
        },
    );
    let mark3 = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(mark),
            args: vec![Const::Int(3).into()],
            suspends: false,
            on_fail: None,
        },
    );
    let add = binding(
        &mut answer,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![resumed.into(), Const::Int(1).into()],
        },
    );
    let mut statements = vec![closure, make_handler, call, mark3];
    if let Some(wait) = wait {
        statements.push(binding(
            &mut answer,
            None,
            Value::Call {
                callee: Callee::Direct(wait),
                args: vec![Const::Int(99).into()],
                suspends: true,
                on_fail: None,
            },
        ));
    }
    statements.push(add);
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    p
}

#[test]
fn immediate_suspend_operations_finish_before_the_caller_and_resume_is_one_shot() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_frame_bytes(1024);
    let mut output =
        crate::backend::compile_program(operation_program(false, false), &options).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic int order;\nvoid mark(int32_t n){if(n!=++order)abort();}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){order=0;if(answer()!=42||order!=3||!test_heap_empty())return 1;}return 0;}\n",
        0,
    );
    let output = crate::backend::compile_program(operation_program(false, true), &options).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nvoid mark(int32_t n){(void)n;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char*expected=\"resume called twice in operation `get` of handler `Waiting`\";exit(k==EMELA_DEFECT_PANIC&&n==strlen(expected)&&!memcmp(m,expected,n)?77:78);}\nint main(void){(void)answer();return 1;}\n",
        77,
    );
}

#[test]
fn delayed_suspend_operations_resume_the_waiting_caller_after_external_completion() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_frame_bytes(1024)
        .with_task_adapter("wait", "board_wait_start");
    let mut output =
        crate::backend::compile_program(operation_program(true, false), &options).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask pending;\nstatic int32_t value;\nstatic int order;\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\nvoid mark(int32_t n){order=order*10+n;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t n){pending=t;value=n;}\nint main(void){for(int i=0;i<1000;i++){order=0;int32_t result=0;EmelaFiber f=answer_start(&result);while(emela_scheduler_run(1)){}if(order!=1||emela_fiber_done(f)||result)return 1;if(!emela_task_complete_int(pending,value))return 2;while(emela_scheduler_run(1)){}if(value!=99||order!=132||result||emela_fiber_done(f))return 3;if(!emela_task_complete_int(pending,0))return 4;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 5;emela_fiber_release(f);while(emela_scheduler_run(1)){}if(result!=42||order!=132||!test_heap_empty())return 6;}return 0;}\n",
        0,
    );
}
