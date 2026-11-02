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
