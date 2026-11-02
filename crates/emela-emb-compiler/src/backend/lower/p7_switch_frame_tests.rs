//! Literal switches must preserve their selector through a real suspended task.
use super::*;

fn selector(p: &mut Program, wait: FnId, name: &str, ty: TypeId, keys: [Const; 2]) {
    let mut f = simple();
    f.name = name.into();
    f.export_symbol = Some(name.into());
    f.suspends = true;
    let input = variable(&mut f, ty);
    f.params.push(input);
    let result = variable(&mut f, TypeId::INT);
    let wait = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![],
            suspends: true,
            on_fail: None,
        },
    );
    let cases = keys
        .into_iter()
        .zip([11, 22])
        .map(|(key, answer)| Case {
            key: CaseKey::Lit(key),
            body: f.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Yield(Some(Const::Int(answer).into())),
            }),
        })
        .collect();
    let fallback = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(33).into())),
    });
    let switch = f.stmts.alloc(Stmt::Switch {
        dst: Some(result),
        on: SwitchOn::Lit(input),
        cases,
        default: Some(fallback),
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![wait, switch],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
}

#[test]
fn literal_i32_and_i64_switches_resume_into_cases_and_default() {
    let mut p = program(vec![]);
    let mut wait = Function::new("wait");
    wait.ret = TypeId::INT;
    wait.suspends = true;
    wait.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("switch_wait".into()),
        }],
    });
    let wait = p.functions.alloc(wait);
    selector(
        &mut p,
        wait,
        "select_i32",
        TypeId::INT,
        [Const::Int(i32::MIN), Const::Int(i32::MAX)],
    );
    selector(
        &mut p,
        wait,
        "select_i64",
        TypeId::INT64,
        [Const::Int64(i64::MIN), Const::Int64(i64::MAX)],
    );
    let output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(0)
            .with_frame_bytes(128)
            .with_frame_capacity(4)
            .with_fiber_capacity(1)
            .with_task_capacity(1)
            .with_event_capacity(1)
            .with_task_adapter("switch_wait", "board_wait_start"),
    )
    .unwrap();
    assert_eq!(output.exports.len(), 2);
    assert!(output.exports.iter().all(|export| export.suspends));
    assert!(!output.c_source.contains("emela_heap_storage"));
    execute(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask pending;
static int started;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
void board_wait_start(EmelaTask task){assert(!started++);pending=task;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
static void finish(EmelaFiber fiber,int32_t *result,int expected){
  assert(emela_scheduler_run(0)==0 && !started && *result==-1);
  while(emela_scheduler_run(1)){}
  assert(started==1 && !emela_fiber_done(fiber) && *result==-1);
  assert(emela_task_complete_int(pending,0));
  assert(!emela_task_complete_int(pending,1));
  assert(!emela_fiber_done(fiber) && *result==-1);
  while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
  assert(*result==expected);emela_fiber_release(fiber);
  assert(!emela_task_complete_int(pending,0));
}
int main(void){for(int i=0;i<1000;i++){
 int32_t result=-1;started=0;
 int which=i%3,expected=which==0?11:which==1?22:33;
 EmelaFiber f=select_i32_start(which==0?INT32_MIN:which==1?INT32_MAX:0,&result);finish(f,&result,expected);
 result=-1;started=0;
 f=select_i64_start(which==0?INT64_MIN:which==1?INT64_MAX:0,&result);finish(f,&result,expected);
}return 0;}
"#,
        0,
    );
}
