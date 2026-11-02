#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};

fn suspended(original: &str, name: &str, options: &Options) -> Output {
    let source = original.replace(
        &format!("pub fn {name}("),
        &format!("@export(c,\"legacy_{name}\")\npub fn {name}("),
    );
    let mut output = heap::with_heap_audit(
        compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
            .unwrap_or_else(|errors| panic!("{errors:?}")),
    );
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    // The old test directly requests root cancellation through the private runtime.
    source.push_str("\nvoid test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}\n");
    output.c_source = source.clone();
    output
}

const HEADERS: &str = r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void test_cancel(EmelaFiber f);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
"#;

#[test]
fn full_legacy_bytes_corpus_preserves_const_traits_and_every_debug_observation() {
    // The saved source already contains the old embedded test's two adaptations:
    // public main, and its bounded 128-element example. Add only an explicit C root.
    let original = include_str!(
        "compat/legacy-corpus/sources/1ad66c9f12980347b7e4a5d94fa35ad1c5c0f96b6e13e7f48e96e431d5119936.emel"
    );
    let source = original.replace(
        "pub fn main()",
        "@export(c,\"legacy_bytes\")\npub fn main()",
    );
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(131072)
            .with_string_capacity(512),
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    let output = heap::with_heap_audit(output);
    let expected = include_bytes!("compat/p7/bytes-std.expected")
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    support::compile_and_run(
        &output,
        &format!(
            r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
size_t test_heap_used(void);
void emela_platform_start(void (*program)(void)){{program();}}
void emela_board_init(void){{}}
void emela_board_idle(void){{}}
static const unsigned char expected[]={{ {expected} }};
static size_t position;
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}}
void emela_board_debug(const char*s,uint16_t n){{
  assert(position+n<sizeof expected);
  assert(!memcmp(s,expected+position,n));position+=n;
  assert(expected[position++]=='\n');
}}
int main(void){{
  size_t retained=0;
  for(int i=0;i<1000;i++){{
    position=0;(void)legacy_bytes();assert(position==sizeof expected);
    size_t used=test_heap_used();
    if(i==0)retained=used;else assert(used==retained);
  }}return 0;
}}
"#
        ),
    );
}

#[test]
fn legacy_bytes_keep_owned_snapshots_across_suspend_null_output_and_cancellation() {
    let output = suspended(
        include_str!(
            "compat/legacy-corpus/sources/94571daa8fcda4685ef21d04748964f732075d256ab472559d3595abcec69f41.emel"
        ),
        "across",
        &Options::default()
            .with_heap_bytes(4096)
            .with_fiber_capacity(2)
            .with_frame_capacity(8)
            .with_frame_bytes(512)
            .with_task_capacity(1)
            .with_event_capacity(1)
            .with_deferred_capacity(0)
            .with_task_adapter("host_wait", "board_wait_start"),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
static EmelaTask task;
uint32_t emela_board_event_lock(void){{return 0;}}
void emela_board_event_unlock(uint32_t n){{(void)n;}}
void board_wait_start(EmelaTask t,int32_t n){{assert(n==1);task=t;}}
static void drain(void){{while(emela_scheduler_run(1)){{}}}}
int main(void){{for(unsigned i=0;i<1000;i++){{for(unsigned route=0;route<3;route++){{
 uint8_t b[]={{1,0,255}};int32_t result=-1;
 EmelaFiber f=legacy_across_start((EmelaBytes){{b,3,NULL}},route==1?NULL:&result);
 memset(b,9,3);drain();
 if(route==2){{test_cancel(f);drain();}}
 else{{assert(emela_task_complete_int(task,1));drain();if(route==0)assert(result==256);}}
 emela_fiber_release(f);assert(test_heap_empty());
}}}}return 0;}}
"#
        ),
    );
}

#[test]
fn legacy_bytes_tasks_release_contexts_after_queued_completion_or_cancellation() {
    let output = suspended(
        include_str!(
            "compat/legacy-corpus/sources/fcd6d29ec6ec8b5b668b3e8966a27dc8f2bf9884648a9e35390c8ea75f814a04.emel"
        ),
        "echoed",
        &Options::default()
            .with_heap_bytes(4096)
            .with_frame_bytes(1024)
            .with_task_adapter("host_bytes", "board_bytes_start"),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
static EmelaTask pending;
static uint8_t board_bytes[3];
static int produced,released,locked,isr;
uint32_t emela_board_event_lock(void){{int previous=locked;locked=1;return (uint32_t)previous;}}
void emela_board_event_unlock(uint32_t p){{assert(locked);locked=(int)p;}}
void board_bytes_start(EmelaTask task,EmelaBytes input){{
 assert(!locked&&!isr&&input.owner&&input.length==3);
 pending=task;memcpy(board_bytes,input.data,3);
}}
static void produce(void*context,EmelaBytes*out){{assert(!locked&&!isr);produced++;*out=(EmelaBytes){{context,3,NULL}};}}
static void release(void*context){{assert(!locked&&!isr);released++;memset(context,1,3);}}
static void drain(void){{while(emela_scheduler_run(1)){{}}}}
int main(void){{for(int i=0;i<1000;i++){{
 uint8_t input[]={{0,128,255}};EmelaBytes result={{0}};produced=released=0;
 int cancel=i%5==1,discard=i%3==0;
 EmelaFiber f=legacy_echoed_start((EmelaBytes){{input,3,NULL}},discard?NULL:&result);
 memset(input,1,3);drain();assert(board_bytes[0]==0&&board_bytes[1]==128&&board_bytes[2]==255);
 isr=1;assert(!emela_task_complete_int(pending,0));
 assert(emela_task_complete_value_10(pending,produce,board_bytes,release));
 assert(!emela_task_complete_value_10(pending,produce,board_bytes,release));
 assert(!produced&&!released);isr=0;
 if(cancel)test_cancel(f);
 drain();assert(emela_fiber_done(f));emela_fiber_release(f);
 assert(produced==!cancel&&released==1);
 if(!discard&&!cancel){{assert(result.owner&&result.length==3&&result.data[0]==0&&result.data[1]==128&&result.data[2]==255);emela_embedded_release_bytes(&result);}}
 assert(test_heap_empty());
}}return 0;}}
"#
        ),
    );
}
