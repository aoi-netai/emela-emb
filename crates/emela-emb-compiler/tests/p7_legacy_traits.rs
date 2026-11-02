#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};

fn fixture() -> Output {
    let original = include_str!(
        "compat/legacy-corpus/sources/c2cacb8eb0a458543a2c75f44e74015be514f14c24db2ca938af34204d514b74.emel"
    );
    let mut source = original.to_owned();
    for name in [
        "traits",
        "quoted",
        "debugging",
        "written_cause",
        "derived_cause",
        "plain_cause",
        "entry_failure",
    ] {
        source = source.replace(
            &format!("pub fn {name}("),
            &format!("@export(c,\"{name}\")\npub fn {name}("),
        );
    }
    heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(16384)
                .with_string_capacity(256),
        )
        .unwrap_or_else(|errors| panic!("legacy traits: {errors:#?}")),
    )
}

const BASE: &str = r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static void equal_text(EmelaString text,const char*expected){size_t n=strlen(expected);if(text.length!=n||memcmp(text.data,expected,n)){fprintf(stderr,"got %.*s (%u), expected %s (%zu)\n",text.length,text.data,text.length,expected,n);abort();}}
"#;

#[test]
fn full_legacy_traits_preserve_structural_display_debug_and_typed_causes() {
    let harness = r#"
static unsigned debug_count;
void emela_board_debug(const char*text,uint16_t length){static const char*expected[]={"<fn>","<fn>","K7"};assert(debug_count<3);equal_text((EmelaString){text,length,NULL},expected[debug_count++]);}
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char*text,uint16_t length){fprintf(stderr,"defect %d: %.*s\n",kind,length,text);abort();}
int main(void){
 char buffer[256];
 static const char bytes[]={'a',0,'"','\\','\n','#','{',(char)0xce,(char)0xbb};
 EmelaString quoted_input={bytes,sizeof bytes,NULL},text={"x",1,NULL};
 for(unsigned i=0;i<1000;i++){
  equal_text(traits(7,text,buffer,sizeof buffer),"(Envelope(key: K7, text: \"x\"), [Some(K7)], Present(K7, \"x\"))");
  equal_text(quoted(quoted_input,buffer,sizeof buffer),"\"a\\u{0}\\\"\\\\\\n\\#{\xce\xbb\"");
  debug_count=0;assert(debugging(7)==7&&debug_count==3);
  equal_text(written_cause(text,buffer,sizeof buffer),"error: written<x>");
  equal_text(derived_cause(text,buffer,sizeof buffer),"error: Derived(message: \"x\")");
  equal_text(plain_cause(text,buffer,sizeof buffer),"error: Plain");
  assert(test_heap_empty());
 }
 return 0;
}
"#;
    support::compile_and_run(&fixture(), &format!("{BASE}{harness}"));
}

#[test]
fn full_legacy_traits_keep_public_failure_as_an_immediate_defect() {
    let harness = r#"
void emela_board_debug(const char*text,uint16_t length){(void)text;(void)length;}
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char*text,uint16_t length){assert(kind==EMELA_DEFECT_UNHANDLED_ERROR);equal_text((EmelaString){text,length,NULL},"unhandled error");exit(0);}
int main(void){(void)entry_failure((EmelaString){"x",1,NULL});return 1;}
"#;
    support::compile_and_run(&fixture(), &format!("{BASE}{harness}"));
}
