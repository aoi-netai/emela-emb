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
