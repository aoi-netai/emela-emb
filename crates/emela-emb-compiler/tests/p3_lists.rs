mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdint.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  (void)kind; (void)message; (void)length; abort();
}
"#;

#[test]
fn list_construction_spreads_patterns_generics_and_tail_recursion_execute() {
    let source = r#"
fn build(n: Int, tail: List[Int]) -> List[Int] {
  if n == 0 { tail } else { build(n - 1, [n, ..tail]) }
}
fn sum(xs: List[Int], acc: Int) -> Int {
  xs match {
    [] -> acc
    [x, ..rest] -> sum(rest, acc + x)
  }
}
fn length[A](xs: List[A], n: Int) -> Int {
  xs match { [] -> n
    [_, ..rest] -> length(rest, n + 1) }
}
fn shape(xs: List[Int]) -> Int {
  xs match { [] -> 0
    [x] -> x
    [x, y] -> x + y
    [x, _, ..] -> x + 100 }
}
@external(c, "mark")
fn mark(n: Int) -> Int
@export(c, "lists")
pub fn lists() -> Int {
  xs = [mark(1), mark(2), ..[mark(3)]]
  assert sum(xs, 0) == 6
  assert shape([]) == 0 && shape([7]) == 7
  assert shape([1, 2]) == 3 && shape([1, 2, 3]) == 101
  assert length([(), (), ()], 0) == 3
  assert length(["a", "b"], 0) == 2
  wide: List[Int64] = [2147483648, 42]
  assert length(wide, 0) == 2
  assert sum(build(1000, []), 0) == 500500
  [..rest] = xs
  sum(rest, 0)
}
"#;
    let mut output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(65536),
    )
    .unwrap();
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(
            r#"
int test_heap_empty(void) {
  for (unsigned char *p=emela_heap_storage; emela_heap_initialized && p<emela_heap_end();) {
    emela_heap_block *block=(emela_heap_block *)(void *)p;
    if (!block->is_free) return 0;
    p+=block->size;
  }
  return 1;
}
"#,
        );
    support::compile_and_run(
        &output,
        &format!(
            r#"{BOARD}
static int trace;
int test_heap_empty(void);
int32_t mark(int32_t n) {{ trace = trace * 10 + n; return n; }}
int main(void) {{ for (int i=0; i<100; i++) {{ trace=0; assert(lists()==6); assert(trace==123); assert(test_heap_empty()); }} return 0; }}
"#
        ),
    );
}

#[test]
fn list_element_types_spreads_and_exhaustiveness_are_checked() {
    for source in [
        "fn main() { [1, True] }",
        "fn main() { [1, ..2] }",
        "fn main() { [1, ..[True]] }",
        "fn main(xs: List[Int]) -> Int { xs match { [] -> 0 } }",
        "fn main(xs: List[Bool]) -> Int { xs match { [] -> 0\n[True, ..] -> 1 } }",
        "fn main(xs: List[Int]) { [x] = xs\n x }",
        "fn main(xs: Int) { xs match { [] -> 0\n _ -> 1 } }",
        "fn main(xs: List[Int]) { xs match { [x, ..x] -> x\n _ -> [] } }",
        "fn main(xs: List) { 0 }",
        "fn main(xs: List[Int, Int]) { 0 }",
        "fn main() { [1, ..[], 2] }",
        "fn main(xs: List[Int]) { xs match { [..tail, x] -> 0\n _ -> 1 } }",
    ] {
        let errors = compile(source).unwrap_err();
        assert_eq!(
            errors[0].source.as_deref(),
            Some("main.emel"),
            "{source}: {errors:?}"
        );
        assert!(errors[0].span.end <= source.len(), "{source}: {errors:?}");
    }
}

#[test]
fn bool_list_pattern_matrix_tracks_empty_tail_and_literal_elements() {
    let source = r#"
fn classify(xs: List[Bool]) -> Int {
  xs match { [] -> 0
    [True] -> 1
    [False] -> 2
    [True, _, ..] -> 3
    [False, _, ..] -> 4 }
}
@export(c, "matrix")
pub fn matrix() -> Int {
  assert classify([]) == 0
  assert classify([True]) == 1 && classify([False]) == 2
  assert classify([True, False]) == 3 && classify([False, True, False]) == 4
  42
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        &format!("{BOARD}\nint main(void) {{ assert(matrix()==42); return 0; }}"),
    );
}
