mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn embedded_handlers_preserve_all_services_errors_buffers_and_owned_receive_lists() {
    let mut output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p7/embedded.emel"),
        )],
        "main.emel",
        &Options::default()
            .with_heap_bytes(8192)
            .with_string_capacity(96)
            .with_peripheral_buffer_capacity(4)
            .with_fiber_capacity(4)
            .with_frame_capacity(12)
            .with_frame_bytes(2048)
            .with_task_capacity(2)
            .with_event_capacity(2)
            .with_deferred_capacity(1),
    )
    .unwrap_or_else(|errors| panic!("Embedded source rejected: {errors:#?}"));
    for name in [
        "gpio",
        "uart",
        "i2c",
        "spi",
        "clock",
        "console",
        "peripheral",
    ] {
        assert!(
            output
                .files
                .iter()
                .any(|file| file.path == format!("emela_{name}.h"))
        );
    }
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(
            r#"
size_t fixture_heap_live(void) {
  size_t used = 0;
  if (!emela_heap_initialized) return 0;
  for (unsigned char *p = emela_heap_storage; p < emela_heap_end();) {
    emela_heap_block *block = (emela_heap_block *)(void *)p;
    if (!block->is_free) used += block->size;
    p += block->size;
  }
  return used;
}

"#,
        );
    support::compile_and_run(&output, include_str!("compat/p7/embedded-harness.c"));
}

#[test]
fn user_modules_cannot_impersonate_compiler_owned_board_bindings() {
    for symbol in [
        "__emela_peripheral_clock_now",
        "__emela_peripheral_gpio_read",
        "__emela_peripheral_uart_receive",
    ] {
        let source = format!("@external(c,\"{symbol}\")\nfn clock_now()->Int64");
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
        &[
            Source::new(
                "main.emel",
                "import Embedded.Board\nfn main()->Int64{Board.clock_now()}",
            ),
            Source::new(
                "embedded/board.emel",
                "@external(c,\"__emela_peripheral_clock_now\")\npub fn clock_now()->Int64",
            ),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Module
                && error
                    .message
                    .contains("reserved by the compiler standard library")
        ),
        "{errors:#?}"
    );
}

#[test]
fn board_clock_sleep_clamps_boundaries_and_notifies_before_main_resumes() {
    let source = r#"
import Std.Clock.{Clock}
import Embedded.Board.{BoardClock}
@export(c,"board_wait")
pub fn wait_then_return(ms:Int)->Int use Async {
  with BoardClock {(use Clock).sleep(ms)
    42}
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(2048),
    )
    .unwrap_or_else(|errors| panic!("BoardClock source rejected: {errors:#?}"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_clock.h"
#include "emela_board.h"
#ifndef EMELA_HAS_TASK_RUNTIME
#error "Clock.sleep requires the task runtime"
#endif
static EmelaTask pending_task;
static int pending;
static int32_t requested;
static unsigned starts;
static uint32_t locked;
int64_t emela_board_clock_now(void){return 0;}
uint32_t emela_board_event_lock(void){uint32_t old=locked;locked=1;return old;}
void emela_board_event_unlock(uint32_t old){locked=old;}
void emela_board_clock_sleep(EmelaTask task,int32_t ms){assert(!locked&&!pending);pending_task=task;requested=ms;pending=1;starts++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){const int32_t delays[]={INT32_MIN,-1,0,7,INT32_MAX};for(unsigned i=0;i<1000;i++){
 int32_t ms=delays[i%5],result=-1;EmelaFiber fiber=board_wait_start(ms,&result);
 while(emela_scheduler_run(1)){}
 assert(pending&&starts==i+1&&requested==(ms<0?0:ms));
 assert(result==-1&&!emela_fiber_done(fiber));pending=0;
 assert(emela_task_complete_unit(pending_task)&&!emela_task_complete_unit(pending_task));
 assert(result==-1&&!emela_fiber_done(fiber));
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 assert(result==42&&!locked);emela_fiber_release(fiber);
 assert(!emela_task_complete_unit(pending_task));
}return 0;}
"#,
    );
}
