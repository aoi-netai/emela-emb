mod support;
use emela_emb_compiler::{
    ExternalAdapter, ExternalResult, Options, Source, compile, compile_sources,
};

#[test]
fn polymorphic_empty_list_const_keeps_one_heapless_representation() {
    let output = compile(
        r#"
const EMPTY=[]
fn ints(xs:List[Int])->Int{if List.is_empty(xs){0}else{1}}
fn texts(xs:List[String])->Int{if List.is_empty(xs){0}else{1}}
@export(c,"answer")
pub fn answer()->Int{ints(EMPTY)+texts(EMPTY)+42}
"#,
    )
    .unwrap();
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42)return 1;return 0;}\n",
    );
}

#[test]
fn polymorphic_const_closures_share_initialization_across_int_and_string_getters() {
    let source = r#"
@external(c,"tick")
fn tick()->Int use {}
const IDENTITY={tick()
 (fn(value){value},())}
@export(c,"integer")
pub fn integer()->Int{(identity,_)=IDENTITY
 identity(42)}
@export(c,"text")
pub fn text()->String{(identity,_)=IDENTITY
 identity("42")}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(8192),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int calls;\nint32_t tick(void){return ++calls;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){EmelaString value=text();if(integer()!=42||calls!=1||value.length!=2||memcmp(value.data,\"42\",2))return 1;}return 0;}\n",
    );
}

#[test]
fn const_function_references_pair_generic_callable_families() {
    let source = r#"
fn identity[A](value:A,unit:())->A{value}
const IDENTITY=(identity,())
@export(c,"integer")
pub fn integer()->Int{(apply,_)=IDENTITY
 apply(42,())}
@export(c,"text")
pub fn text()->String{(apply,_)=IDENTITY
 apply("42",())}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(8192),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){EmelaString value=text();if(integer()!=42||value.length!=2||memcmp(value.data,\"42\",2))return 1;}return 0;}\n",
    );
}

#[test]
fn canonical_const_dependencies_reach_a_fixed_point_and_initialize_once() {
    let source = r#"
const BASE={dbg("BASE")
 []}
const PAIR={dbg("PAIR")
 (BASE,fn(value){value})}
fn integers(value:(List[Int],fn(Int)->Int))->Int{(items,apply)=value
 apply(42)}
fn texts(value:(List[Int],fn(String)->String))->Int{(items,apply)=value
 String.byte_size(apply("42"))+40}
@export(c,"integer")
pub fn integer()->Int{integers(PAIR)}
@export(c,"text")
pub fn text()->Int{texts(PAIR)}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(16384),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int base_logs,pair_logs;void emela_board_debug(const char*s,uint16_t n){if(n==6&&!memcmp(s,\"\\\"BASE\\\"\",6))base_logs++;else if(n==6&&!memcmp(s,\"\\\"PAIR\\\"\",6))pair_logs++;else abort();}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(text()!=42||integer()!=42||base_logs!=1||pair_logs!=1)return 1;return 0;}\n",
    );
}

#[test]
fn polymorphic_const_defects_are_cached_once_and_self_reference_can_retry() {
    for self_reference in [false, true] {
        let source = format!(
            r#"
@external(c,"trigger")
fn trigger()->Int use {{}}
const BROKEN={{dbg("once")
 if trigger()==1{{{failure}}}else{{[]}}}}
fn ints(items:List[Int])->Int{{if List.is_empty(items){{42}}else{{0}}}}
fn texts(items:List[String])->Int{{if List.is_empty(items){{42}}else{{0}}}}
@export(c,"integer")
pub fn integer()->Int{{ints(BROKEN)}}
@export(c,"text")
pub fn text()->Int{{texts(BROKEN)}}
"#,
            failure = if self_reference {
                "BROKEN"
            } else {
                "panic(\"broken\")"
            }
        );
        let output = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default().with_heap_bytes(8192),
        )
        .unwrap();
        let harness = format!(
            r#"
#include <stdlib.h>
#include <setjmp.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;
static int calls,logs,defects;
static char last[1024];
int32_t trigger(void){{return ++calls;}}
void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;logs++;}}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;if(n>=sizeof(last))abort();memcpy(last,m,n);last[n]=0;defects++;longjmp(jump,1);}}
int main(void){{
 if(setjmp(jump)==0){{integer();abort();}}
 if(calls!=1||logs!=1||defects!=1)return 1;
 {checks}
 return 0;
}}
"#,
            checks = if self_reference {
                "if(strstr(last,\"while evaluating\")){return 2;}for(int i=0;i<1000;i++){if(text()!=42||integer()!=42)return 3;}if(calls!=2||logs!=2||defects!=1){return 4;}"
            } else {
                "const char*prefix=strstr(last,\"while evaluating const BROKEN\");if(!prefix||strstr(prefix+1,\"while evaluating const BROKEN\"))return 2;for(volatile int i=0;i<1000;i++){if(setjmp(jump)==0){text();abort();}}if(calls!=1||logs!=1||defects!=1001)return 3;"
            }
        );
        support::compile_and_run(&output, &harness);
    }
}

#[test]
fn polymorphic_consts_preserve_nested_captures_and_higher_order_invocation_rows() {
    let source = include_str!("compat/p7/polymorphic-consts.emel");
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(32768),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int ticks,logs,marks;
int32_t tick(void){return ++ticks;}
void mark(int32_t n){if(n!=2)abort();marks++;}
void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;logs++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++)if(integer()!=42||text()!=42||ticks!=2||logs!=6||marks!=2*(i+1))return 1;return 0;}
"#,
    );
}

#[test]
fn const_bridge_heap_defects_use_one_source_context_and_are_cached() {
    let source = r#"
const ITEMS={dbg("once")
 [[]]}
fn ints(items:List[List[Int]])->Int{42}
fn texts(items:List[List[String]])->Int{42}
@export(c,"integer")
pub fn integer()->Int{ints(ITEMS)}
@export(c,"text")
pub fn text()->Int{texts(ITEMS)}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(96),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <setjmp.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;static int logs,defects;
void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;logs++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char expected[]="while evaluating const ITEMS: fixed heap exhausted";if(k!=EMELA_DEFECT_HEAP_EXHAUSTED||n!=sizeof(expected)-1||memcmp(m,expected,n))abort();defects++;longjmp(jump,1);}
int main(void){for(volatile int i=0;i<1000;i++){if(setjmp(jump)==0){integer();abort();}if(setjmp(jump)==0){text();abort();}}return logs!=1||defects!=2000;}
"#,
    );
}

#[test]
fn generic_external_const_handles_share_one_nullable_host_pointer_without_heap() {
    let source = r#"
@external
type Handle[A]
@external(c,"host_handle")
fn handle[A]()->Handle[A] use {}
const HANDLE=handle()
@export(c,"integer")
pub fn integer()->Handle[Int]{HANDLE}
@export(c,"text")
pub fn text()->Handle[String]{HANDLE}
"#;
    for policy in [ExternalResult::Borrowed, ExternalResult::Owned] {
        let output = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default()
                .with_external_adapter("host_handle", ExternalAdapter::new("board_handle", policy)),
        )
        .unwrap();
        assert!(!output.c_source.contains("emela_heap_storage"));
        for pointer in ["NULL", "&object"] {
            let harness = format!(
                r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int object,calls;
void *board_handle(void){{(void)&object;calls++;return {pointer};}}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;(void)m;(void)n;abort();}}
int main(void){{for(int i=0;i<1000;i++)if(integer()!={pointer}||text()!={pointer}||calls!=1)return 1;return 0;}}
"#
            );
            support::compile_and_run(&output, &harness);
        }
    }
}

#[test]
fn unreachable_const_initializers_do_not_emit_unused_runtime_context_helpers() {
    let source = r#"
@external(c,"host_packet")
fn packet()->(String,List[String]) use {}
const VALUE=packet()
pub fn read()->(String,List[String]){VALUE}
@export(c,"make")
pub fn make(value:String)->(String,List[String]){(value,[value])}
pub fn first()->String{(_,values)=VALUE
 values match{[head,..tail]->head
 _->""}}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(8192),
    )
    .unwrap();
    assert!(!output.c_source.contains("emela_const_self_reference"));
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){return 0;}\n",
    );
}

#[test]
fn const_callback_families_keep_concrete_error_and_effect_rows() {
    let source = r#"
error Boom
effect Read{fn read()->Int}
handler Fixed:Read{fn read(){21}}
const APPLY=(fn(callback){callback()},())
fn may_fail()->Int fails Boom{fail Boom}
@export(c,"answer")
pub fn answer()->Int{(apply_read,_)=APPLY
 left=with Fixed{apply_read(fn(){(use Read).read()})}
 (apply_error,_)=APPLY
 right=apply_error(fn(){may_fail()}) escape{Boom->21}
 left+right}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(32768),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42)return 1;return 0;}\n",
    );
}
