#[path = "support/heap.rs"]
mod heap;
mod support;

use emela_emb_compiler::{Options, Output, Source, compile_sources};

fn compile(source: &str) -> Output {
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(131072)
            .with_string_capacity(512),
    )
    .unwrap_or_else(|errors| panic!("P4 standard callback source rejected: {errors:#?}"));
    heap::with_heap_audit(output)
}

fn execute(output: &Output, entry: &str, expected: &[i32]) {
    let expected = expected
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    support::compile_and_run(
        output,
        &format!(
            r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int events[64], count;
int32_t trace(int32_t value) {{ assert(count<64);events[count++]=value;return value; }}
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) {{
  fprintf(stderr,"defect %d: %.*s\n",kind,length,message);abort();
}}
int main(void) {{
  const int expected[]={{{expected}}};
  for(int iteration=0;iteration<1000;iteration++) {{
    count=0;assert({entry}()==42);
    assert(count==(int)(sizeof expected/sizeof *expected));
    for(int i=0;i<count;i++) assert(events[i]==expected[i]);
    assert(test_heap_empty());
  }}
  return 0;
}}
"#
        ),
    );
}

#[test]
fn standard_callbacks_preserve_order_short_circuiting_stability_and_ownership() {
    let output = compile(include_str!("compat/p4/stdlib-callbacks.emel"));
    execute(
        &output,
        "stdlib_callbacks",
        &[
            1, 2, 3, 11, 12, 21, 22, 23, 31, 32, 41, 42, 51, 52, 53, 61, 62, 63,
        ],
    );
}

#[test]
fn hidden_error_sets_propagate_through_standard_callbacks_and_release_partial_results() {
    let output = compile(include_str!("compat/p4/stdlib-failure.emel"));
    execute(&output, "stdlib_failure", &[1, 2]);
}
