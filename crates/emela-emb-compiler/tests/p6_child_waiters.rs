#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn child_await_and_join_waiters_resume_fifo_after_the_child_finishes() {
    for (original, expected) in [
        (include_str!("compat/p6/fiber-await-join-fifo.emel"), 84),
        (include_str!("compat/p6/fiber-join-fifo.emel"), 126),
    ] {
        let source = original.replace("pub fn work()", "@export(c,\"work\")\npub fn work()");
        let output = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(8192)
                .with_frame_capacity(40)
                .with_frame_bytes(1024)
                .with_task_capacity(2)
                .with_event_capacity(2)
                .with_task_adapter("host_wait", "board_wait_start"),
        )
        .unwrap();
        let output = heap::with_heap_audit(output);
        let harness = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int order,count;static EmelaTask task;
void mark(int32_t n){assert(n==++order);}
void board_wait_start(EmelaTask t,int32_t n){assert(n==40&&count++==0);task=t;}
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){order=count=0;int32_t result=-1;EmelaFiber f=work_start(&result);while(emela_scheduler_run(1)){}assert(order==3&&count==1&&result==-1&&!emela_fiber_done(f));assert(emela_task_complete_int(task,42));while(!emela_fiber_done(f))assert(emela_scheduler_run(1));assert(result==EXPECTED&&order==7);emela_fiber_release(f);assert(!emela_task_complete_int(task,42)&&test_heap_empty());}return 0;}
"#;
        support::compile_and_run(&output, &harness.replace("EXPECTED", &expected.to_string()));
    }
}

#[test]
fn failed_handleless_children_release_records_during_sibling_cleanup() {
    let source = include_str!("compat/p6/fiber-handleless-failure.emel")
        .replace("pub fn work(n:", "@export(c,\"work\")\npub fn work(n:");
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(4096)
            .with_string_capacity(64)
            .with_frame_capacity(40)
            .with_frame_bytes(1024)
            .with_task_capacity(3)
            .with_event_capacity(3)
            .with_task_adapter("host_wait", "board_wait_start"),
    )
    .unwrap();
    let mut output = heap::with_heap_audit(output);
    output.files.iter_mut().find(|file|file.path=="emela_embedded.c").unwrap().contents.push_str(r#"
int test_children(void){int children=0;for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free&&b->kind==EMELA_CHILD_KIND)children++;p+=b->size;}return children;}
void test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}
"#);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_children(void);int test_heap_empty(void);void test_cancel(EmelaFiber);
static EmelaTask tasks[3];static int started;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
void board_wait_start(EmelaTask task,int32_t n){assert(n>=1&&n<=3);tasks[n-1]=task;started++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){EmelaString result={0};started=0;EmelaFiber f=work_start(42,i%3?&result:NULL);while(emela_scheduler_run(1)){}assert(started==3&&!emela_fiber_done(f)&&test_children()==1);if(i%2){test_cancel(f);while(emela_scheduler_run(1)){}}assert(!emela_fiber_done(f)&&test_children()==1);assert(emela_task_complete_int(tasks[2],0));while(!emela_fiber_done(f))assert(emela_scheduler_run(1));emela_fiber_release(f);assert(!emela_task_complete_int(tasks[0],0)&&!emela_task_complete_int(tasks[1],0)&&test_children()==0);if(i%3&&i%2==0){assert(result.owner&&result.length==2&&!memcmp(result.data,"42",2));emela_embedded_release_string(&result);}else{assert(result.data==NULL&&result.owner==NULL&&result.length==0);}assert(test_heap_empty());}return 0;}
"#,
    );
}
