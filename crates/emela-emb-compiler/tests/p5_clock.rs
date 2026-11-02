#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

const SOURCE: &str = r#"
import Std.Clock.{Clock, SystemClock, TestClock}
import Std.Instant.{Instant}
import Std.State.{State}
fn read_now()->Int64 use Clock {
  Instant.to_unix_milliseconds((use Clock).now())
}
@export(c,"clock_now")
pub fn only_now()->Int64 {with SystemClock {read_now()}}
@export(c,"clock_system")
pub fn system()->Int64 {
  with SystemClock {
    before=Instant.to_unix_milliseconds((use Clock).now())
    (use Clock).sleep(42)
    Instant.to_unix_milliseconds((use Clock).now())-before
  }
}
@export(c,"clock_test")
pub fn virtual_time()->Int64 {
  (_,end)=State.run(Instant.from_unix_milliseconds(100),fn(){
    with TestClock {
      assert Instant.to_unix_milliseconds((use Clock).now())==100
      times=List.map([-7,50],fn(ms){
        (use Clock).sleep(ms)
        Instant.to_unix_milliseconds((use Clock).now())
      })
      assert times==[100,150]
      assert Instant.to_unix_milliseconds((use Clock).now())==150
    }
  })
  Instant.to_unix_milliseconds(end)
}
"#;

#[test]
fn clock_handlers_wait_for_host_tasks_and_test_clock_updates_shared_virtual_time() {
    let output = compile_sources(
        &[Source::new("main.emel", SOURCE)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(65536)
            .with_frame_bytes(1024)
            .with_task_adapter("emela_clock_sleep", "board_clock_sleep")
            .with_task_adapter("emela_clock_when_stalled", "board_clock_when_stalled"),
    )
    .unwrap_or_else(|errors| panic!("Clock source rejected: {errors:#?}"));
    assert!(output.exports.iter().all(|export| export.suspends));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending;
static int starts;
static int64_t now,deadline;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int64_t emela_clock_now_milliseconds(void){return now;}
void board_clock_sleep(EmelaTask task,int32_t ms){assert(ms==42);pending=task;starts++;}
void board_clock_when_stalled(EmelaTask task,int64_t at){pending=task;deadline=at;starts++;}
static void drain(void){while(emela_scheduler_run(1)){};}
static void complete(EmelaFiber fiber){
 assert(!emela_fiber_done(fiber));
 assert(emela_task_complete_unit(pending));
 assert(!emela_task_complete_unit(pending));
 assert(!emela_fiber_done(fiber));
 drain();
}
int main(void){for(int i=0;i<1000;i++){
 int64_t result=0;starts=0;now=1000;
 EmelaFiber fiber=clock_now_start(&result);drain();
 assert(emela_fiber_done(fiber)&&result==1000&&starts==0);
 emela_fiber_release(fiber);assert(test_heap_empty());
 result=0;fiber=clock_system_start(&result);drain();
 assert(starts==1&&!emela_fiber_done(fiber)&&result==0);
 now=1042;complete(fiber);
 assert(emela_fiber_done(fiber)&&result==42);
 emela_fiber_release(fiber);
 assert(!emela_task_complete_unit(pending)&&test_heap_empty());
 starts=0;result=0;deadline=-1;
 fiber=clock_test_start(&result);drain();
 assert(starts==1&&deadline==100&&!emela_fiber_done(fiber)&&result==0);
 complete(fiber);
 assert(starts==2&&deadline==150&&!emela_fiber_done(fiber)&&result==0);
 complete(fiber);
 assert(emela_fiber_done(fiber)&&result==150);
 emela_fiber_release(fiber);
 assert(!emela_task_complete_unit(pending)&&test_heap_empty());
}return 0;}
"#,
    );
}

#[test]
fn clock_task_adapters_are_explicit_and_reserved_symbols_remain_compiler_owned() {
    for symbol in [
        "emela_clock_now_milliseconds",
        "emela_clock_sleep",
        "emela_clock_when_stalled",
    ] {
        let source = format!("@external(c,\"{symbol}\")\nfn now_milliseconds()->Int64");
        let errors = emela_emb_compiler::compile(&source).unwrap_err();
        assert!(
            errors.iter().any(
                |error| error.kind == emela_emb_compiler::DiagnosticKind::Export
                    && error.message.contains("non-reserved")
            ),
            "{errors:#?}"
        );
    }
    let errors = compile_sources(
        &[Source::new("main.emel", SOURCE)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(65536)
            .with_frame_bytes(1024),
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("with_task_adapter")),
        "{errors:#?}"
    );
}

#[test]
fn clock_now_still_requires_a_suspending_effect_in_immediate_constraints() {
    let source = r#"
import Std.Clock.{Clock}
import Std.Instant.{Instant}
fn immediate[R:Immediate](f:fn()->Int64 use R)->Int64 use R {f()}
fn wrong()->Int64 use Clock {
  immediate(fn(){Instant.to_unix_milliseconds((use Clock).now())})
}
"#;
    let errors = emela_emb_compiler::compile(source).unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Type
                && error.message.contains("Immediate")
        ),
        "{errors:#?}"
    );
}
