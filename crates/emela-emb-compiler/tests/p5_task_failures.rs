#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_suspend_failures_escape_and_typed_string_completions_preserve_ownership() {
    let source = r#"
error Broken(message:String) derive Show
effect Exchange {suspend fn exchange(value:String)->String fails Broken}
@external(c,"exchange")
suspend fn exchange(value:String)->String fails Broken
handler Host:Exchange {
  suspend fn exchange(value,resume){resume(exchange(value))}
}
@export(c,"source_task_failure")
pub fn failure(message:String)->String fails Broken {fail Broken(message:)}
@export(c,"source_task_owned")
pub fn run(value:String)->String {
  {with Host {(use Exchange).exchange(String.append(value,"!"))}}
  escape {Broken(message:)->String.append("caught:",message)}
}
"#;
    let mut output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(32768)
            .with_frame_bytes(1024)
            .with_task_adapter("exchange", "board_exchange_start"),
    )
    .unwrap();
    // Let a source-constructed error supply the typed producer without binding
    // this ownership test to generated enum/type numbering.
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str("\nvoid test_make_error(EmelaString input,EmelaError*out){(void)source_task_failure_impl(input,out);}\nvoid test_release_error(EmelaError*error){emela_error_clear(error);}\n");
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_make_error(EmelaString,EmelaError*);
void test_release_error(EmelaError*);
typedef struct {EmelaString argument;EmelaError error;char bytes[3];} Context;
static Context context;
static EmelaTask task;
static int produced,released,isr,locked;
uint32_t emela_board_event_lock(void){uint32_t previous=(uint32_t)locked;locked=1;return previous;}
void emela_board_event_unlock(uint32_t previous){locked=(int)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
void board_exchange_start(EmelaTask pending,EmelaString value){task=pending;context.argument=value;emela_embedded_retain(value.owner);memcpy(context.bytes,"a\0b",3);}
static void success(void*p,EmelaString*out){Context*c=p;assert(!isr&&!locked);produced++;*out=(EmelaString){.data=c->bytes,.length=3};}
static void failure(void*p,EmelaError*out){Context*c=p;assert(!isr&&!locked);produced++;test_make_error((EmelaString){.data=c->bytes,.length=3},&c->error);*out=c->error;}
static void release(void*p){Context*c=p;assert(!isr&&!locked);released++;memset(c->bytes,'z',3);emela_embedded_release(c->argument.owner);test_release_error(&c->error);}
int main(void){for(int i=0;i<1000;i++){
 char original[]="arg";EmelaString result={0};produced=released=0;
 EmelaFiber fiber=source_task_owned_start((EmelaString){.data=original,.length=3},&result);
 memset(original,'x',3);while(emela_scheduler_run(1)){}
 assert(!emela_fiber_done(fiber)&&context.argument.owner&&context.argument.length==4&&!memcmp(context.argument.data,"arg!",4));
 isr=1;
 if(i%2){assert(emela_task_fail(task,failure,&context,release));assert(!emela_task_complete_value_4(task,success,&context,release));}
 else{assert(emela_task_complete_value_4(task,success,&context,release));assert(!emela_task_fail(task,failure,&context,release));}
 assert(!produced&&!released);isr=0;
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 emela_fiber_release(fiber);assert(produced==1&&released==1&&result.owner);
 if(i%2)assert(result.length==10&&!memcmp(result.data,"caught:a\0b",10));
 else assert(result.length==3&&!memcmp(result.data,"a\0b",3));
 emela_embedded_release_string(&result);assert(test_heap_empty());
 }return 0;}
"#,
    );
}
