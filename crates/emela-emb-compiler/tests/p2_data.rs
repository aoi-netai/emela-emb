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
