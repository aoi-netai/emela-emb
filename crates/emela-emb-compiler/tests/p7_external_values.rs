#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{ExternalAdapter, ExternalResult, Options, Source, compile_sources};

#[test]
fn directly_exported_externals_keep_aliases_unit_arguments_and_never_returns() {
    let source = r#"
@export(c,"external_flag")
@external(c,"board_flag")
pub fn flag(value:Bool)->Bool use {}
@export(c,"external_unit")
@external(c,"board_unit")
pub fn unit(value:Unit)->Unit use {}
@export(c,"external_stop")
@external(c,"board_stop")
pub fn stop()->Never use {}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int calls,stops;static jmp_buf jump;
int32_t board_flag(int32_t value){return value;}
void board_unit(void){calls++;}
_Noreturn void board_stop(void){stops++;longjmp(jump,1);}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){assert(!external_flag(0)&&external_flag(1));external_unit();}assert(calls==1000);if(!setjmp(jump)){external_stop();abort();}assert(stops==1);return 0;}
"#,
    );
}

#[test]
fn string_buffer_adapters_preserve_caller_storage_and_own_indirect_results() {
    let source = r#"
@export(c,"external_buffer")
@external(c,"host_buffer")
pub fn raw(value:String)->String use {}
@export(c,"direct_buffer")
pub fn direct(value:String)->String {raw(value)}
@export(c,"indirect_buffer")
pub fn indirect(value:String)->String {
 callback:fn(String)->String fails {} use {}=raw
 callback(value)
}
@export(c,"discard_buffer")
pub fn discard(value:String)->Unit {raw(value)
()}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(4096)
            .with_external_adapter(
                "host_buffer",
                ExternalAdapter::new("board_buffer", ExternalResult::StringBuffer),
            ),
    )
    .unwrap();
    support::compile_and_run(
        &heap::with_heap_audit(output),
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
EmelaString board_buffer(EmelaString input,char*output,uint16_t capacity){assert(input.length<=capacity&&(!input.length||output));if(input.length)memmove(output,input.data,input.length);return(EmelaString){.data=output,.length=input.length};}
static void check(EmelaString value){assert(value.length==3&&!memcmp(value.data,"a\0b",3));emela_embedded_release_string(&value);}
int main(void){for(int i=0;i<1000;i++){
 EmelaString input={.data="a\0b",.length=3};char bytes[3];
 EmelaString value=external_buffer(input,bytes,sizeof(bytes));assert(value.data==bytes&&!value.owner);check(value);
 value=direct_buffer(input,bytes,sizeof(bytes));assert(value.data==bytes&&!value.owner);check(value);
 value=indirect_buffer(input);assert(value.owner);check(value);
 discard_buffer(input);
 value=external_buffer((EmelaString){0},NULL,0);assert(!value.length&&!value.owner&&!value.data);
 assert(test_heap_empty());
}return 0;}
"#,
    );
}

#[test]
fn string_buffer_adapters_reject_capacity_alias_owner_and_invalid_lengths_without_heap() {
    let source = "@export(c,\"external_buffer\")\n@external(c,\"host_buffer\")\npub fn raw(value:String)->String use {}";
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(0).with_external_adapter(
            "host_buffer",
            ExternalAdapter::new("board_buffer", ExternalResult::StringBuffer),
        ),
    )
    .unwrap();
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int mode,defects;static jmp_buf jump;
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)m;(void)n;assert(k==EMELA_DEFECT_STRING_CAPACITY);defects++;longjmp(jump,1);}
EmelaString board_buffer(EmelaString input,char*output,uint16_t capacity){
 if(mode==0){if(input.length>capacity)emela_board_defect(EMELA_DEFECT_STRING_CAPACITY,"capacity",8);return(EmelaString){.data=output,.length=input.length};}
 if(mode==1)return(EmelaString){.data=input.data,.length=input.length};
 if(mode==2)return(EmelaString){.data=output,.length=input.length,.owner=(void*)(uintptr_t)1};
 return(EmelaString){.data=output,.length=(uint16_t)(capacity+1)};
}
int main(void){for(mode=0;mode<4;mode++){char output[3];if(!setjmp(jump)){external_buffer((EmelaString){.data="abc",.length=3},output,mode?3:2);abort();}}assert(defects==4);return 0;}
"#,
    );
}

#[test]
fn synchronous_external_failures_preserve_owned_error_payloads_and_ignore_invalid_success_values() {
    let source = r#"
error Boom(message:String)
@external(c,"host_failure")
fn raw(value:String)->String fails Boom use {}
@export(c,"catch_direct")
pub fn direct(value:String)->String {raw(value) escape{Boom(message:)->message}}
@export(c,"catch_indirect")
pub fn indirect(value:String)->String {
 callback:fn(String)->String fails Boom use {}=raw
 callback(value) escape{Boom(message:)->message}
}
@export(c,"catch_discard")
pub fn discard(value:String)->String {{raw(value)
panic("missing failure")}escape{Boom(message:)->message}}
"#;
    for policy in [ExternalResult::Borrowed, ExternalResult::Owned] {
        let output = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(4096)
                .with_external_adapter(
                    "host_failure",
                    ExternalAdapter::new("board_failure", policy),
                ),
        )
        .unwrap();
        let harness = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
EmelaString board_failure(EmelaString input,EmelaError*error){
 *error=(EmelaError){.tag=1,.payload.e0={.tag=0,.payload.v0.f0=input}};
 __OWN_ERROR__
 return(EmelaString){.data=(const char*)(uintptr_t)1,.length=UINT16_MAX,.owner=(void*)(uintptr_t)1};
}
static void check(EmelaString value){assert(value.owner&&value.length==3&&!memcmp(value.data,"a\0b",3));emela_embedded_release_string(&value);}
int main(void){for(int i=0;i<1000;i++){
 char bytes[]={'a','\0','b'};EmelaString input={.data=bytes,.length=3};
 EmelaString value=catch_direct(input);memset(bytes,'x',3);check(value);assert(test_heap_empty());
 input=(EmelaString){.data="a\0b",.length=3};check(catch_indirect(input));assert(test_heap_empty());
 check(catch_discard(input));assert(test_heap_empty());
}return 0;}
"#;
        support::compile_and_run(
            &heap::with_heap_audit(output),
            &harness.replace(
                "__OWN_ERROR__",
                if policy == ExternalResult::Owned {
                    "emela_external_own_error(error);"
                } else {
                    ""
                },
            ),
        );
    }
}
