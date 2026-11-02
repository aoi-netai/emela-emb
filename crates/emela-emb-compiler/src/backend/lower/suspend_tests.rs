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

#[test]
fn suspending_cleanup_delays_delivery_of_the_original_result() {
    let mut p = program(vec![]);
    let wait = external_task(&mut p, "wait", &[TypeId::INT], TypeId::INT);
    let mut answer = simple();
    answer.suspends = true;
    let result = variable(&mut answer, TypeId::INT);
    let body_result = variable(&mut answer, TypeId::INT);
    let body_call = binding(
        &mut answer,
        Some(body_result),
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(42).into()],
            suspends: true,
            on_fail: None,
        },
    );
    let cleanup_call = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(99).into()],
            suspends: true,
            on_fail: None,
        },
    );
    let body = answer.seqs.alloc(Seq {
        stmts: vec![body_call],
        term: Term::Yield(Some(body_result.into())),
    });
    let cleanup = answer.seqs.alloc(Seq {
        stmts: vec![cleanup_call],
        term: Term::Yield(None),
    });
    let protect = answer.stmts.alloc(Stmt::Protect {
        dst: Some(result),
        body,
        cleanup,
    });
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![protect],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_frame_bytes(1024)
            .with_task_adapter("wait", "board_wait_start"),
    )
    .unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic EmelaTask pending;\nstatic int32_t value;\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t n){pending=t;value=n;}\nint main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber f=answer_start(&result);while(emela_scheduler_run(1)){}if(value!=42||result||emela_fiber_done(f))return 1;if(!emela_task_complete_int(pending,value))return 2;while(emela_scheduler_run(1)){}if(value!=99||result||emela_fiber_done(f))return 3;if(!emela_task_complete_int(pending,0))return 4;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 5;emela_fiber_release(f);if(result!=42)return 6;}return 0;}\n",
        0,
    );
}

#[test]
fn typed_task_success_and_error_notifications_share_main_context_ownership() {
    for indirect in [false, true] {
        let mut p = program(vec![]);
        let error = p.core.enums.alloc(EnumDef {
            name: "Boom".into(),
            params: 0,
            variants: vec![VariantDef {
                name: "Boom".into(),
                fields: VariantFields::Positional(1),
                tys: vec![TypeId::STRING],
            }],
            error: true,
            external: false,
        });
        let error_type = p.core.types.intern(Type::Enum(error, vec![]));
        let callback = p.core.types.intern(Type::Fn(FnType {
            params: vec![TypeId::STRING],
            ret: TypeId::STRING,
            fails: true,
            suspends: true,
            ev: false,
            sets: None,
        }));
        let mut task = Function::new("value");
        task.ret = TypeId::STRING;
        task.suspends = true;
        task.fails = true;
        let input = aggregate_variable(&mut task, TypeId::STRING);
        task.params.push(input);
        task.external = Some(External {
            bindings: vec![ExternalBinding {
                target: "c".into(),
                host: HostRef::Global("value".into()),
            }],
        });
        let target = p.functions.alloc(task);
        let mut answer = simple();
        answer.suspends = true;
        answer.ret = TypeId::STRING;
        let input = aggregate_variable(&mut answer, TypeId::STRING);
        answer.params.push(input);
        let callback_value = aggregate_variable(&mut answer, callback);
        let make_callback =
            indirect.then(|| binding(&mut answer, Some(callback_value), Value::FnRef(target)));
        let callee = if indirect {
            Callee::Indirect(callback_value)
        } else {
            Callee::Direct(target)
        };
        let caught = aggregate_variable(&mut answer, TypeId::ANY);
        let typed = aggregate_variable(&mut answer, error_type);
        let success = aggregate_variable(&mut answer, TypeId::STRING);
        let result = aggregate_variable(&mut answer, TypeId::STRING);
        let label = answer.labels.alloc(LabelData {
            hint: "caught".into(),
        });
        let call = binding(
            &mut answer,
            Some(success),
            Value::Call {
                callee,
                args: vec![input.into()],
                suspends: true,
                on_fail: Some(OnFail::Break(label)),
            },
        );
        let body = answer.seqs.alloc(Seq {
            stmts: vec![call],
            term: Term::Return(Some(success.into())),
        });
        let block = answer.stmts.alloc(Stmt::Block {
            dst: Some(caught),
            label,
            body,
        });
        let unpack = binding(
            &mut answer,
            Some(typed),
            Value::AsError {
                error: caught,
                enum_id: error,
                error_type,
            },
        );
        let field = binding(
            &mut answer,
            Some(result),
            Value::Field {
                base: typed,
                ctor: CtorRef {
                    enum_id: error,
                    variant: 0,
                },
                index: 0,
            },
        );
        answer.seqs[answer.body.unwrap()] = Seq {
            stmts: [make_callback, Some(block), Some(unpack), Some(field)]
                .into_iter()
                .flatten()
                .collect(),
            term: Term::Return(Some(result.into())),
        };
        p.functions.alloc(answer);
        let options = Options::default()
            .with_heap_bytes(4096)
            .with_frame_bytes(1024)
            .with_task_adapter("value", "board_value_start");
        let mut output = crate::backend::compile_program(p, &options).unwrap();
        audit_heap(&mut output);
        let harness = r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
typedef struct { EmelaString argument; char bytes[3]; } Context;
static Context context;
static EmelaTask task;
static int produced,released,isr,locked;
uint32_t emela_board_event_lock(void){uint32_t old=(uint32_t)locked;locked=1;return old;}
void emela_board_event_unlock(uint32_t p){locked=(int)p;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
void board_value_start(EmelaTask t,EmelaString input){task=t;context.argument=input;emela_embedded_retain(input.owner);memcpy(context.bytes,"a\0b",3);}
static void success(void*p,EmelaString*out){Context*c=p;if(isr||locked)abort();produced++;*out=(EmelaString){.data=c->bytes,.length=3};}
static void failure(void*p,EmelaError*out){Context*c=p;if(isr||locked)abort();produced++;*out=(EmelaError){.tag=1,.payload.e0=(emela_type_ERROR_TYPE){.tag=0,.payload.v0.f0={.data=c->bytes,.length=3}}};}
static void release(void*p){Context*c=p;if(isr||locked)abort();released++;memset(c->bytes,'z',3);emela_embedded_release(c->argument.owner);}
int main(void){for(int i=0;i<1000;i++){char original[]="arg";EmelaString result={0};produced=released=0;EmelaFiber f=answer_start((EmelaString){.data=original,.length=3},i%3?&result:NULL);memset(original,'x',3);while(emela_scheduler_run(1)){}if(!context.argument.owner||memcmp(context.argument.data,"arg",3))return 1;isr=1;if(i%2){if(!emela_task_fail(task,failure,&context,release)||emela_task_complete_value_4(task,success,&context,release))return 2;}else{if(!emela_task_complete_value_4(task,success,&context,release)||emela_task_fail(task,failure,&context,release))return 3;}if(produced||released)return 4;isr=0;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 5;emela_fiber_release(f);if(produced!=1||released!=1)return 6;if(i%3){if(!result.owner||result.length!=3||memcmp(result.data,"a\0b",3))return 7;emela_embedded_release_string(&result);}if(!test_heap_empty())return 8;}return 0;}
"#.replace("ERROR_TYPE", &error_type.index().to_string());
        execute(&output, &harness, 0);
    }
}

fn recursive_program() -> Program {
    let mut p = program(vec![]);
    let id = p.functions.alloc(Function::new("answer"));
    let mut answer = simple();
    answer.suspends = true;
    let n = variable(&mut answer, TypeId::INT);
    answer.params.push(n);
    let done = variable(&mut answer, TypeId::BOOL);
    let less = variable(&mut answer, TypeId::INT);
    let value = variable(&mut answer, TypeId::INT);
    let result = variable(&mut answer, TypeId::INT);
    let compare = binding(
        &mut answer,
        Some(done),
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![n.into(), Const::Int(0).into()],
        },
    );
    let decrement = binding(
        &mut answer,
        Some(less),
        Value::Prim {
            op: PrimOp::Sub,
            ty: PrimTy::I32,
            args: vec![n.into(), Const::Int(1).into()],
        },
    );
    let call = binding(
        &mut answer,
        Some(value),
        Value::Call {
            callee: Callee::Direct(id),
            args: vec![less.into()],
            suspends: true,
            on_fail: None,
        },
    );
    let add = binding(
        &mut answer,
        Some(result),
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![value.into(), Const::Int(1).into()],
        },
    );
    let base = answer.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    });
    let recursive = answer.seqs.alloc(Seq {
        stmts: vec![decrement, call, add],
        term: Term::Return(Some(result.into())),
    });
    let branch = answer.stmts.alloc(Stmt::If {
        dst: None,
        cond: done.into(),
        then: base,
        else_: recursive,
    });
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![compare, branch],
        term: Term::Unreachable,
    };
    p.functions[id] = answer;
    p
}

#[test]
fn recursive_frames_return_into_the_callers_storage_and_exhaust_fixed_capacity() {
    let p = recursive_program();
    let options = Options::default()
        .with_frame_capacity(8)
        .with_frame_bytes(512);
    let output = crate::backend::compile_program(p.clone(), &options).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber f=answer_start(i%6,&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 1;emela_fiber_release(f);if(result!=42+i%6)return 2;}return 0;}\n",
        0,
    );
    let output =
        crate::backend::compile_program(p.clone(), &options.clone().with_frame_capacity(2))
            .unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char*expected=\"fixed scheduler pool exhausted\";exit(k==EMELA_DEFECT_HEAP_EXHAUSTED&&n==strlen(expected)&&!memcmp(m,expected,n)?77:78);}\nint main(void){int32_t result=0;(void)answer_start(3,&result);(void)emela_scheduler_run(10);return 1;}\n",
        77,
    );
    for options in [
        options.clone().with_frame_capacity(0),
        options.clone().with_fiber_capacity(0),
        options.clone().with_frame_bytes(0),
    ] {
        assert!(
            crate::backend::compile_program(p.clone(), &options)
                .unwrap_err()
                .iter()
                .any(|error| error.kind == crate::ErrorKind::InvalidOptions)
        );
    }
    assert!(
        crate::backend::compile_program(p, &options.with_isr_entry("answer"))
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("ISR"))
    );
}

#[test]
fn suspending_cleanup_preserves_the_failure_until_an_outer_handler_runs() {
    let mut p = program(vec![]);
    let wait = external_task(&mut p, "wait", &[TypeId::INT], TypeId::INT);
    let error = p.core.enums.alloc(EnumDef {
        name: "Failed".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Failed".into(),
            fields: VariantFields::Positional(1),
            tys: vec![TypeId::STRING],
        }],
        error: true,
        external: false,
    });
    let error_type = p.core.types.intern(Type::Enum(error, vec![]));
    let mut worker = Function::new("worker");
    worker.suspends = true;
    worker.fails = true;
    worker.ret = TypeId::INT;
    let payload = aggregate_variable(&mut worker, error_type);
    let wait_body = binding(
        &mut worker,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(1).into()],
            suspends: true,
            on_fail: None,
        },
    );
    let text = aggregate_variable(&mut worker, TypeId::STRING);
    let format = binding(
        &mut worker,
        Some(text),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![Const::Int(42).into()],
        },
    );
    let make = binding(
        &mut worker,
        Some(payload),
        Value::Make {
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            args: vec![text.into()],
        },
    );
    let wait_cleanup = binding(
        &mut worker,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![Const::Int(99).into()],
            suspends: true,
            on_fail: None,
        },
    );
    let body = worker.seqs.alloc(Seq {
        stmts: vec![wait_body, format, make],
        term: Term::Fail(payload.into()),
    });
    let cleanup = worker.seqs.alloc(Seq {
        stmts: vec![wait_cleanup],
        term: Term::Yield(None),
    });
    let protect = worker.stmts.alloc(Stmt::Protect {
        dst: None,
        body,
        cleanup,
    });
    worker.body = Some(worker.seqs.alloc(Seq {
        stmts: vec![protect],
        term: Term::Unreachable,
    }));
    let worker = p.functions.alloc(worker);
    let mut answer = simple();
    answer.suspends = true;
    answer.ret = TypeId::STRING;
    let caught = aggregate_variable(&mut answer, TypeId::ANY);
    let unpacked = aggregate_variable(&mut answer, error_type);
    let result = aggregate_variable(&mut answer, TypeId::STRING);
    let unpack = binding(
        &mut answer,
        Some(unpacked),
        Value::AsError {
            error: caught,
            enum_id: error,
            error_type,
        },
    );
    let field = binding(
        &mut answer,
        Some(result),
        Value::Field {
            base: unpacked,
            ctor: CtorRef {
                enum_id: error,
                variant: 0,
            },
            index: 0,
        },
    );
    let handler = answer.seqs.alloc(Seq {
        stmts: vec![unpack, field],
        term: Term::Return(Some(result.into())),
    });
    let call = binding(
        &mut answer,
        None,
        Value::Call {
            callee: Callee::Direct(worker),
            args: vec![],
            suspends: true,
            on_fail: Some(OnFail::Handle {
                error: caught,
                body: handler,
            }),
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(Const::String(String::new()).into())),
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
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic EmelaTask pending;\nstatic int32_t value;\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t p){(void)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nvoid board_wait_start(EmelaTask t,int32_t n){pending=t;value=n;}\nint main(void){for(int i=0;i<1000;i++){EmelaString result={0};EmelaFiber f=answer_start(&result);while(emela_scheduler_run(1)){}if(value!=1||result.length||emela_fiber_done(f))return 1;if(!emela_task_complete_int(pending,0))return 2;while(emela_scheduler_run(1)){}if(value!=99||result.length||emela_fiber_done(f))return 3;if(!emela_task_complete_int(pending,0))return 4;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 5;emela_fiber_release(f);if(!result.owner||result.length!=2||memcmp(result.data,\"42\",2))return 6;emela_embedded_release_string(&result);if(!test_heap_empty())return 7;}return 0;}\n",
        0,
    );
}

#[test]
fn constructing_an_immediate_handler_keeps_scheduler_storage_omitted() {
    let mut p = operation_program(false, false);
    let (_, answer) = p
        .functions
        .iter_mut()
        .find(|(_, function)| function.exported)
        .unwrap();
    let original = answer.seqs[answer.body.unwrap()].stmts.clone();
    let handler = match &answer.stmts[original[1]] {
        Stmt::Let {
            dst: Some(dst),
            value: Value::Handler { .. },
            ..
        } => *dst,
        _ => unreachable!(),
    };
    answer.ret = answer.vars[handler].ty;
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: original[..2].to_vec(),
        term: Term::Return(Some(handler.into())),
    };
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    assert!(
        !output.c_source.contains("emela_frames[")
            && !output.c_source.contains("struct emela_resume {")
    );
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nvoid mark(int32_t n){(void)n;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){emela_embedded_release(answer());if(!test_heap_empty())return 1;}return 0;}\n",
        0,
    );
}

#[test]
fn task_adapter_and_capacities_are_required_and_exhaustion_unlocks_events() {
    let mut p = program(vec![]);
    let wait = external_task(&mut p, "wait", &[TypeId::INT], TypeId::INT);
    let mut answer = simple();
    answer.suspends = true;
    let n = variable(&mut answer, TypeId::INT);
    answer.params.push(n);
    let result = variable(&mut answer, TypeId::INT);
    let call = binding(
        &mut answer,
        Some(result),
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![n.into()],
            suspends: true,
            on_fail: None,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(answer);
    assert!(
        crate::backend::compile_program(p.clone(), &Options::default())
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("with_task_adapter"))
    );
    let options = Options::default()
        .with_frame_bytes(512)
        .with_task_adapter("wait", "board_wait_start");
    for options in [
        options.clone().with_task_capacity(0),
        options.clone().with_event_capacity(0),
    ] {
        assert!(
            crate::backend::compile_program(p.clone(), &options)
                .unwrap_err()
                .iter()
                .any(|error| error.kind == crate::ErrorKind::InvalidOptions)
        );
    }
    for options in [
        options.clone().with_task_capacity(1),
        options.clone().with_event_capacity(1),
    ] {
        let output = crate::backend::compile_program(p.clone(), &options).unwrap();
        execute(
            &output,
            "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int locked;\nstatic unsigned count;\nstatic EmelaTask pending[2];\nuint32_t emela_board_event_lock(void){uint32_t old=(uint32_t)locked;locked=1;return old;}\nvoid emela_board_event_unlock(uint32_t p){if(!locked)abort();locked=(int)p;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)m;(void)n;exit(k==EMELA_DEFECT_HEAP_EXHAUSTED&&!locked?77:78);}\nvoid board_wait_start(EmelaTask t,int32_t n){(void)n;if(count>=2)abort();pending[count++]=t;}\nint main(void){static int32_t a,b;(void)answer_start(1,&a);(void)answer_start(2,&b);(void)emela_scheduler_run(2);if(count!=2)return 1;if(!emela_task_complete_int(pending[0],1))return 2;(void)emela_task_complete_int(pending[1],2);return 3;}\n",
            77,
        );
    }
}
