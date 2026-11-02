#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_scope_evidence_nested_scopes_and_standard_combinators_preserve_ownership() {
    let source = r#"
import Std.Fiber
effect Read {fn read()->Int}
handler One:Read {fn read(){1}}
handler Two:Read {fn read(){2}}
fn read()->Int use Read {(use Read).read()}
@export(c,"p6_providers")
pub fn providers()->Int {
  with One {Fiber.scope(fn(){
    with Two {
      inherited=Fiber.fork(read)
      lexical=Fiber.fork(fn(){read()})
      Fiber.join(inherited)*10+Fiber.join(lexical)
    }
  })}
}
@export(c,"p6_nested")
pub fn nested()->Int {
  Fiber.scope(fn(){
    outer=use Fiber.Scope
    Fiber.scope(fn(){
      sibling=outer.fork(fn(){40})
      inner=Fiber.fork(fn(){2})
      Fiber.join(sibling)+Fiber.join(inner)
    })
  })
}
@export(c,"p6_combinators")
pub fn combinators()->Int {
  (a,b)=Fiber.both(fn(){20},fn(){22})
  first=Fiber.race([fn(){41},fn(){42}])
  total=List.fold(Fiber.all([fn(){a},fn(){b}]),0,fn(sum,n){sum+n})
  total+first-41
}
@export(c,"p6_managed")
pub fn managed()->Bool {
  mapped=Fiber.map(["a","b"],fn(value){String.append(value,"!")})
  all=Fiber.all([fn(){String.append("x","!")},fn(){String.append("y","!")}])
  mapped==["a!","b!"] && all==["x!","y!"]
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(131072)
            .with_fiber_capacity(16)
            .with_frame_capacity(128)
            .with_frame_bytes(1024),
    )
    .unwrap();
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
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",(int)k,(int)n,m);abort();}
static void finish(EmelaFiber fiber,int32_t*result,int32_t expected){while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));assert(*result==expected);emela_fiber_release(fiber);assert(test_heap_empty());}
int main(void){for(int i=0;i<1000;i++){
 int32_t result=0;EmelaFiber fiber=p6_providers_start(&result);finish(fiber,&result,12);
 result=0;fiber=p6_nested_start(&result);finish(fiber,&result,42);
 result=0;fiber=p6_combinators_start(&result);finish(fiber,&result,42);
 result=0;fiber=p6_managed_start(&result);finish(fiber,&result,1);
}return 0;}
"#,
    );
}
