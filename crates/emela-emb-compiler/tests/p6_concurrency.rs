#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};
#[test]
fn source_concurrency_preserves_handles_values_causes_and_unit_results() {
    let source = r#"
import Std.Fiber
import Std.Deferred
import Std.Cause
@export(c,"p6_scope")
pub fn answer()->Int{Fiber.scope(fn(){child=Fiber.fork(fn(){42})
Fiber.join(child)})}
@export(c,"p6_attempt")
pub fn attempt()->Int{Fiber.attempt(fn(){42}) match{Fiber.Success(value)->value
Fiber.Failure(cause)->0}}
@export(c,"p6_unit")
pub fn unit()->Int{Fiber.scope(fn(){child=Fiber.fork(fn(){()})
Fiber.join(child)
42})}
@export(c,"p6_deferred")
pub fn deferred()->Int{Fiber.scope(fn(){d=Fiber.deferred()
Fiber.fork(fn(){Deferred.complete(d,42)})
Deferred.await(d)})}
@export(c,"p6_deferred_unit")
pub fn deferred_unit()->Int {Fiber.scope(fn(){d=Fiber.deferred()
first=Deferred.complete(d,())
second=Deferred.complete(d,())
Deferred.await(d)
if first && !second {Deferred.poll(d) match {Some(())->42
None->0}}else{0}
})}
error Oops(message:String) derive Show
fn raise()->Int fails Oops {fail Oops(message:String.append("x","!"))}
@export(c,"p6_cause")
pub fn cause()->Bool {Fiber.attempt(raise) match {
Fiber.Success(value)->False
Fiber.Failure(cause)->Cause.kind(cause)==Cause.Fail && Cause.to_string(cause)=="error: Oops(message: \"x!\")"
}}
@export(c,"p6_raise")
pub fn recover()->Int {{Fiber.attempt(raise) match {
Fiber.Success(value)->value
Fiber.Failure(cause)->Cause.raise(cause)
}} escape {Oops(message:)->42}}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(32768)
            .with_frame_bytes(1024),
    )
    .unwrap();
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",(int)k,(int)n,m);abort();}
static void finish(EmelaFiber fiber,int32_t*result){while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));assert(*result==42);emela_fiber_release(fiber);assert(test_heap_empty());}
int main(void){for(int i=0;i<1000;i++){assert(p6_attempt()==42&&test_heap_empty());assert(p6_cause()&&test_heap_empty());assert(p6_raise()==42&&test_heap_empty());int32_t result=0;EmelaFiber fiber=p6_scope_start(&result);finish(fiber,&result);result=0;fiber=p6_unit_start(&result);finish(fiber,&result);result=0;fiber=p6_deferred_start(&result);finish(fiber,&result);result=0;fiber=p6_deferred_unit_start(&result);finish(fiber,&result);}return 0;}
"#,
    );
}

#[test]
fn scope_result_and_row_kind_constraints_report_the_source_boundary() {
    let cases = [
        (
            "import Std.Fiber\nfn wrong(){Fiber.scope(fn(){Fiber.fork(fn(){42})})}",
            "scope result",
        ),
        (
            "import Std.Fiber\nfn wrong(){Fiber.scope(fn(){Fiber.deferred()})}",
            "scope result",
        ),
        (
            "import Std.Fiber\nfn wrong(){Fiber.scope(fn(){Some(Fiber.fork(fn(){42}))})}",
            "scope result",
        ),
        (
            "import Std.Cause.{Cause}\nfn wrong(c:Cause[Int]){()}",
            "error-set type argument",
        ),
        (
            "import Std.Fiber.{Scope}\nfn wrong()use Scope{()}",
            "expects 2 type arguments",
        ),
        (
            "import Std.Fiber.{Scope}\nfn wrong()use Scope[Int,Int]{()}",
            "error-set type argument",
        ),
        (
            "import Std.Fiber.{Scope}\nhandler Wrong:Scope[{},{}]{}",
            "intrinsic effects",
        ),
    ];
    for (source, message) in cases {
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default().with_heap_bytes(32768),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.source.as_deref() == Some("main.emel")
                    && error.message.contains(message)),
            "{source}\n{errors:?}"
        );
    }
}
