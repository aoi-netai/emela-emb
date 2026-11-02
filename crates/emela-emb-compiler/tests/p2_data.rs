mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdint.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) { (void)kind; (void)message; (void)length; abort(); }
"#;
#[test]
fn record_fields_named_evaluation_and_concrete_type_arguments_execute() {
    let source = r#"
type Point(x: Int, y: Int)
type Box[A](value: A)
type Marker
@external(c, "mark")
fn mark(value: Int) -> Int
fn ignore(value: ((), Int, (), Int)) -> Int { 42 }
@export(c, "records")
pub fn records() -> Int {
  p = Point(y: mark(2), x: mark(1))
  assert p.x * 10 + p.y == 12
  b: Box[Int64] = Box(value: 2147483648)
  assert Int64.to_int(b.value) == Int.MIN
  literal = Box(value: 7)
  assert literal.value == 7
  marker = Marker
  ignore(((), 1, (), 2))
}
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nstatic int trace; int32_t mark(int32_t value) {{ trace = trace * 10 + value; return value; }}\nint main(void) {{ assert(records() == 42); assert(trace == 21); return 0; }}"
        ),
    );
}
#[test]
fn opaque_construction_and_fields_are_available_only_in_the_defining_module() {
    let email = "pub opaque type Email(value: String)\npub fn make(text: String) -> Email { Email(value: text) }\npub fn size(value: Email) -> Int { String.byte_size(value.value) }";
    let main = "import Email.{make, size}\n@export(c, \"email_size\")\npub fn email_size() -> Int { size(make(\"é\")) }";
    let output = compile_sources(
        &[
            Source::new("main.emel", main),
            Source::new("email.emel", email),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        &format!("{BOARD}\nint main(void) {{ assert(email_size() == 2); return 0; }}"),
    );
    for main in [
        "import Email.{Email}\nfn main() { Email(value: \"secret\") }",
        "import Email.{make}\nfn main() { value = make(\"secret\")\n value.value }",
    ] {
        let errors = compile_sources(
            &[
                Source::new("main.emel", main),
                Source::new("email.emel", email),
            ],
            "main.emel",
            &Options::default(),
        )
        .unwrap_err();
        assert_eq!(errors[0].source.as_deref(), Some("main.emel"));
    }
}

#[test]
fn rejects_invalid_constructor_fields_type_arguments_and_duplicate_definitions() {
    for source in [
        "type Point(x: Int, y: Int)\nfn main() { Point(x: 1) }",
        "type Point(x: Int)\nfn main() { Point(y: 1) }",
        "type Point(x: Int)\nfn main() { Point(x: 1, x: 2) }",
        "type Point(x: Int)\nfn main() { Point(x: True) }",
        "type Point(x: Int)\nfn main() { Point(x: 1).missing }",
        "type Box[A](value: A)\nfn main() -> Box { Box(1) }",
        "type Point(x: Int, x: Int)\nfn main() { 0 }",
        "enum Choice { First, First }\nfn main() { 0 }",
        "type Point(x: Unknown)\nfn main() { 0 }",
        "type Point(x: Int) derive Unknown\nfn main() { 0 }",
    ] {
        let errors = compile(source).unwrap_err();
        assert_eq!(
            errors[0].source.as_deref(),
            Some("main.emel"),
            "{source}: {errors:?}"
        );
        assert!(errors[0].span.end <= source.len());
    }
}

#[test]
fn tuple_constructor_patterns_guards_nested_coverage_and_tail_match_execute() {
    let source = r#"
enum Choice { Empty, Pair(left: Int, right: Int64) }
type Point(x: Int, y: Int)
fn read(value: Choice) -> Int {
  value match {
    Empty -> 0
    Pair(right: r, left: l) if l > 0 -> l + Int64.to_int(r)
    Pair(..) -> -1
  }
}
fn count(n: Int) -> Int { n match { 0 -> 0
_ -> count(n - 1) } }
fn minimum(n: Int) -> Int { n match { -2147483648 -> 42
_ -> 0 } }
fn boolean_pair(value: (Bool, Bool)) -> Int {
  value match { (True, _) -> 1
(False, True) -> 2
(False, False) -> 3 }
}
@export(c, "patterns")
pub fn patterns() -> Int {
  Point(x:, ..) = Point(x: 3, y: 99)
  ((), a, (), b) = ((), 10, (), 20)
  assert x + a + b == 33
  assert read(Empty) == 0
  assert read(Pair(right: 32, left: 10)) == 42
  assert read(Pair(left: -1, right: 99)) == -1
  assert boolean_pair((True, False)) == 1
  assert boolean_pair((False, True)) == 2
  assert boolean_pair((False, False)) == 3
  assert minimum(Int.MIN) == 42
  count(1000000)
  42
}
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        &format!("{BOARD}\nint main(void) {{ assert(patterns() == 42); return 0; }}"),
    );
}

#[test]
fn rejects_refutable_bindings_non_exhaustive_matches_and_wrong_patterns() {
    for source in [
        "fn main() { True = False\n () }",
        "fn main() { (a, a) = (1, 2)\n a }",
        "fn main() { (a, b) = (1, 2, 3)\n a }",
        "fn main(value: Bool) -> Int { value match { True -> 1 } }",
        "fn main(value: Bool) -> Int { value match { True if value -> 1\n False -> 2 } }",
        "fn main(value: Int) -> Int { value match { 0 -> 1 } }",
        "fn main(value: (Bool, Bool)) -> Int { value match { (True, _) -> 1\n (False, True) -> 2 } }",
        "type Point(x: Int, y: Int)\nfn main(value: Point) { Point(x:) = value\n x }",
        "type Point(x: Int)\nfn main(value: Point) { Point(0) = value\n () }",
        "enum Choice { First, Second(Int) }\nfn main(value: Choice) { value match { First -> 1 } }",
        "fn main(value: Bool) -> Int { value match {} }",
        "fn main(value: Int) { value match { -2147483649 -> 1\n _ -> 0 } }",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
