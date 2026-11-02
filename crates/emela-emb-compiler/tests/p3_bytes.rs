#[path = "support/heap.rs"]
mod heap;
mod support;
use heap::with_heap_audit;

use emela_emb_compiler::{
    ExternalAdapter, ExternalResult, Options, Output, Source, compile_sources,
};
use support::compile_and_run;

fn compile_source(source: &str, options: &Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("P3 ownership source rejected: {errors:#?}"))
}

#[test]
fn bytes_views_keep_u32_lengths_and_unsigned_elements_without_a_heap() {
    let output = compile_source(
        include_str!("compat/p3/bytes-views.emel"),
        &Options::default().with_isr_entry("size"),
    );
    assert!(!output.c_source.contains("emela_heap_storage"));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stddef.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) { (void)k;(void)m;(void)n;abort(); }
int main(void) {
  static uint8_t data[70000];data[0]=128;data[69999]=255;
  EmelaBytes value={data,70000,NULL};
  assert(byte_size(value)==70000 && byte_at(value,0)==128 && byte_at(value,69999)==255);
  assert(byte_at(value,-1)==-1 && byte_at(value,70000)==-1);
  assert(byte_size((EmelaBytes){0})==0);
  return 0;
}
"#,
    );
}

#[test]
fn bytes_ownership_snapshots_aliases_and_invalid_list_elements_release_every_allocation() {
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/bytes-owned.emel"),
        &Options::default()
            .with_heap_bytes(262144)
            .with_string_capacity(1),
    ));
    compile_and_run(&output, include_str!("compat/p3/bytes-owned-harness.c"));

    // The shared corpus also contains a 70000-byte case. Reuse its small-value
    // assertions at the old 4096-byte budget instead of inferring that budget
    // from the large-buffer run above.
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/bytes-owned.emel"),
        &Options::default()
            .with_heap_bytes(4096)
            .with_string_capacity(1),
    ));
    let small = include_str!("compat/p3/bytes-owned-harness.c")
        .split_once("  static uint8_t input[70000]")
        .unwrap()
        .0;
    compile_and_run(&output, &format!("{small}\nreturn 0;\n}}\n"));

    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/bytes-owned.emel"),
        &Options::default().with_heap_bytes(256),
    ));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){bytes_discard((EmelaString){"abc",3,NULL});assert(test_heap_empty());}return 0;}
"#,
    );
}

#[test]
fn bytes_unicode_decode_hex_and_unsigned_traits_match_the_fixed_corpus() {
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/bytes-transform.emel"),
        &Options::default()
            .with_heap_bytes(16384)
            .with_string_capacity(512),
    ));
    compile_and_run(&output, include_str!("compat/p3/bytes-transform-harness.c"));
}

#[test]
fn bytes_owned_results_require_a_heap_even_when_the_result_is_discarded() {
    let source = r#"
import Std.Bytes.{Bytes}
@export(c, "discard_bytes")
pub fn discard(s: String) -> Unit { Bytes.encode_utf8(s)
  () }
"#;
    let errors = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|error| error.message.contains("heap")),
        "{errors:?}"
    );
    let output = compile_source(source, &Options::default().with_heap_bytes(128));
    compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) {
  static const char expected[]="fixed heap exhausted";
  exit(kind==EMELA_DEFECT_HEAP_EXHAUSTED && length==sizeof expected-1 && !memcmp(message,expected,length) ? 0:3);
}
int main(void) { static char text[1024];discard_bytes((EmelaString){text,sizeof text,NULL});return 2; }
"#,
    );
}

#[test]
fn bytes_decode_checks_the_full_u32_length_before_narrowing_to_string() {
    let source = r#"
import Std.Bytes.{Bytes}
@export(c, "decode_large")
pub fn decode(value: Bytes) -> Bool { Option.is_some(Bytes.decode_utf8(value)) }
"#;
    let output = compile_source(
        source,
        &Options::default()
            .with_heap_bytes(4096)
            .with_string_capacity(8),
    );
    compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) {
  (void)message;(void)length;exit(kind==EMELA_DEFECT_STRING_CAPACITY ? 0:3);
}

int main(void) { static uint8_t value[70000];(void)decode_large((EmelaBytes){value,sizeof value,NULL});return 2; }
"#,
    );
}

#[test]
fn bytes_external_adapters_snapshot_borrowed_data_and_transfer_owned_data() {
    let source = r#"
import Std.Bytes.{Bytes}
@external(c, "raw_bytes")
fn raw(n: Int) -> Bytes
@export(c, "owned_bytes")
pub fn make(s: String) -> Bytes { Bytes.encode_utf8(s) }
@export(c, "external_bytes")
pub fn work(n: Int) -> Bytes { raw(n) }
"#;
    let errors = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.kind == emela_emb_compiler::DiagnosticKind::Backend
                && error
                    .message
                    .contains("external function with a non-scalar signature")
        }),
        "{errors:?}"
    );
    for ownership in [ExternalResult::Borrowed, ExternalResult::Owned] {
        let output = with_heap_audit(compile_source(
            source,
            &Options::default()
                .with_heap_bytes(4096)
                .with_external_adapter(
                    "raw_bytes",
                    ExternalAdapter::new("native_bytes", ownership),
                ),
        ));
        let result = match ownership {
            ExternalResult::Borrowed => "(EmelaBytes){buffer,3,NULL}",
            ExternalResult::Owned => "owned_bytes((EmelaString){(const char *)buffer,3,NULL})",
            ExternalResult::StringBuffer => unreachable!(),
        };
        compile_and_run(
            &output,
            &format!(
                r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *m,uint16_t n) {{ (void)k;(void)m;(void)n;abort(); }}
EmelaBytes native_bytes(int32_t n) {{
  static uint8_t buffer[3];buffer[0]=(uint8_t)n;buffer[1]=0;buffer[2]=255;
  return {result};
}}
int main(void) {{
  for(int i=0;i<500;i++) {{
    EmelaBytes a=external_bytes(1),b=external_bytes(2);
    assert(a.owner && b.owner && a.length==3 && b.length==3);
    assert(a.data[0]==1 && b.data[0]==2 && a.data[1]==0 && a.data[2]==255);
    emela_embedded_release_bytes(&a);emela_embedded_release_bytes(&b);assert(test_heap_empty());
  }}
  return 0;
}}
"#
            ),
        );
    }
}
