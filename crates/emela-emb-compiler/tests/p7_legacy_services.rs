//! Legacy compiler-generated services with the fixed native C provider as a test fixture.
#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};
fn run(source: &str, name: &str, harness: &str) {
    let source = source.replace(
        &format!("pub fn {name}("),
        &format!("@export(c,\"legacy_{name}\")\npub fn {name}("),
    );
    let output = heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(4096)
                .with_frame_bytes(2048),
        )
        .unwrap(),
    );
    let console = if output.files.iter().any(|f| f.path == "emela_console.h") {
        "#define EMELA_REQUIRE_SERVICE_CONSOLE 1\n"
    } else {
        ""
    };
    let native = include_str!("compat/p7/native-services.c");
    support::compile_and_run(
        &output,
        &format!(
            "#define EMELA_REQUIRE_SERVICE_CLOCK 1\n#define EMELA_PLATFORM_TASK_CAPACITY 8\n{console}{native}\n{harness}"
        ),
    );
}
#[test]
fn original_native_clock_console_and_timer_use_generated_common_services() {
    run(
        include_str!(
            "compat/legacy-corpus/sources/5549983f8aba12d57b0d789684cba2e5255b99a5874cbfa1674dbb16533483e2.emel"
        ),
        "main",
        r#"
#include <assert.h>
int test_heap_empty(void);
int main(void) {
    int32_t result=-1;EmelaFiber f=legacy_main_start(&result);
    while (!emela_fiber_done(f)) {emela_scheduler_run(64);emela_board_idle();}
    assert(result==42);emela_fiber_release(f);assert(test_heap_empty());
    return 0;
}
"#,
    );
}
#[test]
fn original_native_timer_slots_survive_cancellation_and_repeated_reuse() {
    run(
        include_str!(
            "compat/legacy-corpus/sources/d60b4137b5256577f8ab5618caf2ea81c9b88bdcd375988cc79f6ee071a7de12.emel"
        ),
        "wait_ms",
        r#"
#include <assert.h>
int test_heap_empty(void);
int main(void) {
  for (int round=0;round<1000;++round) {
    int32_t result=-1;EmelaFiber fiber=legacy_wait_ms_start(&result);
    while (emela_scheduler_run(64)) {}
    assert(!emela_scheduler_ready());
    emela_board_idle();
    assert(emela_scheduler_ready() && result==-1);
    for (int i=0;i<1000 && !emela_fiber_done(fiber);++i) {emela_scheduler_run(64);emela_board_idle();}
    assert(emela_fiber_done(fiber) && result==42);emela_fiber_release(fiber);
    assert(test_heap_empty());
  }
  return 0;
}
"#,
    );
}
