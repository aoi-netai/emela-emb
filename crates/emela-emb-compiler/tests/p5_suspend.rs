#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_suspend_operations_and_effectful_closures_resume_through_a_host_task() {
    let source = r#"
effect Wait { suspend fn wait(value:Int)->Int }
@external(c,"wait")
suspend fn wait(value:Int)->Int
handler Delayed:Wait {
  suspend fn wait(value,resume) {resume(wait(value))}
}

@export(c,"source_suspend")
pub fn run()->Int {
  with Delayed {
    callback=fn(value){(use Wait).wait(value)}
    callback(40)+2
  }
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(16384)
            .with_task_adapter("wait", "board_wait_start"),
    )
    .unwrap();
    assert!(output.exports.iter().any(|export| export.suspends));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask pending;
static int32_t value;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
void board_wait_start(EmelaTask task,int32_t n){pending=task;value=n;}
int main(void){for(int i=0;i<1000;i++){
 int32_t result=0;EmelaFiber fiber=source_suspend_start(&result);
 while(emela_scheduler_run(1)){}
 assert(!emela_fiber_done(fiber)&&result==0);
 assert(emela_task_complete_int(pending,value));
 assert(!emela_task_complete_int(pending,7));
 assert(!emela_fiber_done(fiber));
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 assert(result==42);emela_fiber_release(fiber);
 assert(!emela_task_complete_int(pending,8)&&test_heap_empty());
}return 0;}
"#,
    );
}

#[test]
fn suspension_markers_and_runtime_async_capability_have_source_diagnostics() {
    for source in [
        "suspend fn ordinary()->Int{42}",
        "@external(c,\"wait\")\nsuspend fn wait()->Int\nfn wrong()->Int{wait()}",
        "@external(c,\"wait\")\nsuspend fn wait()->Int\nfn wrong(){callback=wait\n()}",
        "fn wrong(){use Async}",
        "handler User:Async{}",
        "effect Wait{suspend fn get()->Int}\nhandler Wrong:Wait{suspend fn get(resume){resume(\"bad\")}}",
    ] {
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default().with_heap_bytes(4096),
        )
        .unwrap_err();
        let expected = if source.starts_with("suspend fn ordinary") {
            emela_emb_compiler::DiagnosticKind::Syntax
        } else {
            emela_emb_compiler::DiagnosticKind::Type
        };
        assert!(
            errors.iter().all(
                |error| error.kind == expected
                    && error.source.as_deref() == Some("main.emel")
            ),
            "{source}\n{errors:?}"
        );
    }
}

#[test]
fn immediately_resuming_operations_keep_synchronous_exports() {
    let source = "effect Wait{suspend fn get()->Int}\nhandler Immediate:Wait{suspend fn get(resume){resume(42)}}\n@export(c,\"source_immediate\")\npub fn run()->Int{with Immediate{(use Wait).get()}}";
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    let output = heap::with_heap_audit(output);
    assert!(!output.exports[0].suspends);
    support::compile_and_run(
        &output,
        "#include <assert.h>\n#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nuint32_t emela_board_event_lock(void){return 0;}\nvoid emela_board_event_unlock(uint32_t previous){(void)previous;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint test_heap_empty(void);\nint main(void){for(int i=0;i<1000;i++)assert(source_immediate()==42&&test_heap_empty());return 0;}",
    );
}

#[test]
fn fixed_embedded_abi_reports_synchronous_calls_through_suspending_wrappers() {
    let source = "effect Wait{suspend fn get()->Int}\nhandler Immediate:Wait{suspend fn get(resume){resume(42)}}\nfn read()->Int use Wait{(use Wait).get()}\n@export(c,\"wrapper\")\npub fn run()->Int{with Immediate{read()}}";
    let errors = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Backend
                && error.source.as_deref() == Some("main.emel")
                && error.span.start > 0
                && error.message.contains("suspending caller")
        ),
        "{errors:?}"
    );
}

#[test]
fn synchronous_operations_can_use_non_suspending_members_of_a_non_immediate_effect() {
    let source = r#"
effect Timer {fn now()->Int
suspend fn wait()->Int}
handler Fixed:Timer {fn now(){42}
suspend fn wait(resume){resume(42)}}
effect Read {fn read()->Int}
handler Reader:Read {fn read(){(use Timer).now()}}
@export(c,"source_sync_operation")
pub fn run()->Int {with Fixed,Reader {(use Read).read()}}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    assert!(!output.exports[0].suspends);
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++)assert(source_sync_operation()==42&&test_heap_empty());return 0;}
"#,
    );

    for body in ["(use Timer).wait()", "wait()", "callback=wait\ncallback()"] {
        let source = format!(
            "effect Timer{{fn now()->Int\nsuspend fn wait()->Int}}\nfn wait()->Int use Timer{{(use Timer).wait()}}\neffect Read{{fn read()->Int}}\nhandler Reader:Read{{fn read(){{{body}}}}}"
        );
        let errors = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default().with_heap_bytes(4096),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(
                |error| error.kind == emela_emb_compiler::DiagnosticKind::Type
                    && error.source.as_deref() == Some("main.emel")
                    && error.message.contains("synchronous effect operation")
            ),
            "{source}\n{errors:?}"
        );
    }
}

#[test]
fn keeping_a_resume_function_adds_async_to_handler_construction_only() {
    let declarations = "effect Wait{suspend fn get()->Int}\nhandler Later:Wait{suspend fn get(resume){callback=resume\ncallback(42)}}";
    let source = format!(
        "{declarations}\n@export(c,\"source_later\")\npub fn run()->Int{{with Later{{(use Wait).get()}}}}"
    );
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    assert!(output.exports[0].suspends);
    let output = heap::with_heap_audit(output);
    let harness = r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){int32_t result=0;EmelaFiber fiber=source_later_start(&result);while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));assert(result==42);emela_fiber_release(fiber);assert(test_heap_empty());}return 0;}
"#;
    support::compile_and_run(&output, harness);
    let source =
        format!("{declarations}\nfn wrong()->Int use{{}}{{with Later{{(use Wait).get()}}}}");
    let errors = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Type
                && error.source.as_deref() == Some("main.emel")
                && error.message.contains("invocation set")
        ),
        "{errors:?}"
    );
    for body in [
        "callback=fn(value){resume(value)}\ncallback(42)",
        "if False{with Later{(use Wait).get()}}else{42}\ncallback=resume\ncallback(42)",
    ] {
        let source = format!(
            "effect Wait{{suspend fn get()->Int}}\nhandler Later:Wait{{suspend fn get(resume){{{body}}}}}\n@export(c,\"source_later\")\npub fn run()->Int{{with Later{{(use Wait).get()}}}}"
        );
        let output = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default().with_heap_bytes(4096),
        )
        .unwrap();
        assert!(output.exports[0].suspends, "{source}");
        let output = heap::with_heap_audit(output);
        support::compile_and_run(&output, harness);
    }
}
