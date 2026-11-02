#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

fn output(original: &str) -> emela_emb_compiler::Output {
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header.split('(').next().unwrap();
            source.push_str(&format!("@export(c,\"{name}\")\n"));
        }
        source.push_str(line);
    }
    heap::with_heap_audit(
        compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(16384)
                .with_string_capacity(256),
        )
        .unwrap_or_else(|errors| panic!("{errors:?}")),
    )
}
const HEADERS: &str = r#"
#include <assert.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
"#;

#[test]
fn full_legacy_helper_source_preserves_lambda_patterns_numerics_and_nul_ascii() {
    let output = output(include_str!(
        "compat/legacy-corpus/sources/aec1355cd027e7f537cecf16a009001890e7075427332b2744204853c841896f.emel"
    ));
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void) {{
  char buffer[256];static const char text[]="A\xce\xbbz\0Q";
  EmelaString input={{text,sizeof text-1,NULL}};
  for(int i=0;i<1000;i++){{
    assert(lambda_patterns(i)==4*i+3);
    assert(list_helpers(i)==3*i+3);
    EmelaString result=ascii(input,buffer,sizeof buffer);
    static const char expected[]="a\xce\xbbz\0q|A\xce\xbbZ\0Q";
    assert(result.length==sizeof expected-1 && !memcmp(result.data,expected,sizeof expected-1));
    assert(numeric(-i)==i && numeric(INT32_MIN)==INT32_MIN);
    assert(numeric64(INT64_MIN)==INT64_MIN && numeric64(-INT64_MAX)==INT64_MAX);
    assert(numeric_float(NAN) && numeric_float(-0.0) && numeric_float(-INFINITY));
    assert(boolean((EmelaString){{"True",4,NULL}}));
    assert(!boolean((EmelaString){{"False",5,NULL}}));
    assert(test_heap_empty());
  }}return 0;
}}
"#
        ),
    );
}

#[test]
fn full_legacy_collection_source_preserves_updates_custom_keys_and_rotations() {
    let output = output(include_str!(
        "compat/legacy-corpus/sources/d8521d9bfb49b8efe8d3039e21e481493720654a2545736f224a9d01da23f0aa.emel"
    ));
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{
  assert(collections(i)==3*i+41 && test_heap_empty());
  assert(rotations(i)==12*i && test_heap_empty());
}}return 0;}}
"#
        ),
    );
}
