#[path = "support/heap.rs"]
mod heap;
mod support;
use heap::with_heap_audit;

use emela_emb_compiler::{Options, Output, Source, compile_sources};
use support::compile_and_run;

fn compile_source(source: &str, options: &Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("P3 ownership source rejected: {errors:#?}"))
}

#[test]
fn strings_in_nested_lists_and_error_payloads_outlive_their_original_owners() {
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/owned-payload.emel"),
        &Options::default().with_heap_bytes(1024),
    ));
    compile_and_run(&output, include_str!("compat/p3/owned-payload-harness.c"));
}

#[test]
fn owned_string_subviews_compare_without_allocating_or_changing_their_owner() {
    let output = with_heap_audit(compile_source(
        concat!(
            include_str!("compat/p3/owned-payload.emel"),
            include_str!("compat/p2/strings.emel")
        ),
        &Options::default().with_heap_bytes(1024),
    ));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
size_t test_heap_used(void);
void board_smash(void) {}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char *s,uint16_t n) { (void)k;(void)s;(void)n;abort(); }
static void compare(EmelaString left,EmelaString right,int order) {
  assert(string_eq(left,right)==(order==0) && string_ne(left,right)==(order!=0));
  assert(string_lt(left,right)==(order<0) && string_le(left,right)==(order<=0));
  assert(string_gt(left,right)==(order>0) && string_ge(left,right)==(order>=0));
}
int main(void) {
  for(int i=0;i<1000;i++) {
    char original[]={'x','a',0,'b','y'};
    void *list=store_borrowed((EmelaString){original,sizeof original,NULL});
    EmelaString view=read_owned(list);emela_embedded_release(list);memset(original,'z',sizeof original);
    assert(view.owner);view.data++;view.length=3;
    size_t before=test_heap_used();
    compare(view,(EmelaString){"a\0b",3,NULL},0);
    compare(view,(EmelaString){"a\0c",3,NULL},-1);
    compare(view,(EmelaString){"a",1,NULL},1);
    assert(test_heap_used()==before);
    emela_embedded_release(view.owner);assert(test_heap_empty());
  }
  return 0;
}
"#,
    );
}

#[test]
fn finite_changing_recursive_types_and_deep_recursive_release_execute_from_source() {
    let output = with_heap_audit(compile_source(
        include_str!("compat/p3/recursive.emel"),
        &Options::default().with_heap_bytes(4 * 1024 * 1024),
    ));
    compile_and_run(&output, include_str!("compat/p3/recursive-harness.c"));
}
