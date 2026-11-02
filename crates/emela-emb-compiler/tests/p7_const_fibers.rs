//! Cross-phase const callable families retain invocation evidence in suspend frames.
#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn shared_const_callback_families_dispatch_pure_and_suspending_scoped_evidence() {
    let source = r#"
import Std.Fiber
effect Read{fn read()->Int}
handler Fixed:Read{fn read(){21}}
const APPLY=(fn(callback){callback()},())
const IDENTITY=(fn(value){value},())
@export(c,"pure")
pub fn pure()->Int{(apply,_)=APPLY
 apply(fn(){(identity,_)=IDENTITY
 String.byte_size(identity("42"))+40})}
@export(c,"answer")
pub fn answer()->Int{with Fixed{(apply,_)=APPLY
 apply(fn(){Fiber.scope(fn(){child=Fiber.fork(fn(){(identity,_)=IDENTITY
 identity((use Read).read())})
 Fiber.join(child)+(use Read).read()})})}}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(32768)
            .with_frame_bytes(2048),
    )
    .unwrap();
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){if(pure()!=42||!test_heap_empty())return 1;int32_t result=0;EmelaFiber f=answer_start(&result);while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 2;emela_fiber_release(f);if(result!=42||!test_heap_empty())return 3;}return 0;}
"#,
    );
}
