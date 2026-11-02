mod support;

use emela_emb_compiler::{DiagnosticKind, Options, Source, compile, compile_sources};
use support::compile_and_run;

fn source_output(source: &str) -> emela_emb_compiler::Output {
    compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default(),
    )
    .unwrap_or_else(|errors| panic!("P1 source rejected: {errors:#?}"))
}

#[test]
fn modules_internal_calls_branches_and_tail_recursion_execute() {
    let sources = [
        Source::new("main.emel", include_str!("compat/p1/modules-main.emel")),
        Source::new("math.emel", include_str!("compat/p1/math.emel")),
    ];
    let output = compile_sources(&sources, "main.emel", &Options::default()).unwrap();
    assert_eq!(output.exports.len(), 1);
    assert_eq!(output.exports[0].source_name, "answer");
    assert!(!output.header.contains("hidden"));
    assert!(!output.header.contains("private_value"));
    compile_and_run(&output, include_str!("compat/p1/modules-harness.c"));
}

#[test]
fn scalar_numeric_boundaries_execute_with_legacy_semantics() {
    let output = source_output(include_str!("compat/p1/numeric.emel"));
    compile_and_run(&output, include_str!("compat/p1/numeric-harness.c"));
}

#[test]
fn native_scalar_externals_preserve_integer_and_float_bits() {
    let output = source_output(include_str!("compat/p1/external.emel"));
    assert!(!output.c_source.contains("emela_heap_storage"));
    compile_and_run(&output, include_str!("compat/p1/external-harness.c"));
}

#[test]
fn short_circuiting_and_argument_evaluation_preserve_side_effect_order() {
    let output = source_output(
        r#"
@external(c, "board_bool")
fn tick() -> Bool
@external(c, "board_value")
fn value(n: Int) -> Int
fn combine(a: Int, b: Int) -> Int { a * 10 + b }
@export(c, "check_order")
pub fn check_order() -> Int {
  assert !(False && tick())
  assert True || tick()
  assert True && tick()
  assert !(False || !tick())
  combine(value(1), value(2))
}
"#,
    );
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdbool.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
static int ticks;
static int ordered;
int32_t board_bool(void) { ticks++; return 1; }
int32_t board_value(int32_t n) { ordered = ordered * 10 + n; return n; }
int main(void) { assert(check_order() == 12); assert(ticks == 2); assert(ordered == 12); return 0; }
"#,
    );
}

#[test]
fn unused_division_remainder_and_negative_shifts_still_raise_defects() {
    for (expression, kind, expected) in [
        ("a / b", "EMELA_DEFECT_DIVISION_BY_ZERO", "division by zero"),
        ("a % b", "EMELA_DEFECT_DIVISION_BY_ZERO", "division by zero"),
        (
            "Int.shift_left(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
        (
            "Int.shift_right(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
        (
            "Int.shift_right_unsigned(a, b)",
            "EMELA_DEFECT_NEGATIVE_SHIFT",
            "negative shift amount: -2147483648",
        ),
    ] {
        for ty in ["Int", "Int64"] {
            let expression = if ty == "Int64" {
                expression.replace("Int.", "Int64.")
            } else {
                expression.into()
            };
            let argument_type = if expression.contains("shift_") {
                "Int"
            } else {
                ty
            };
            let source = format!(
                "@export(c, \"probe\")\npub fn probe(a: {ty}, b: {argument_type}) -> Unit {{ unused = {expression}\n () }}"
            );
            let output = source_output(&source);
            let divisor = if expression.contains("shift_") {
                "INT32_MIN"
            } else {
                "0"
            };
            let harness = format!(
                r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {{
  static const char expected[] = "{expected}";
  exit(k == {kind} && n == sizeof expected - 1 && memcmp(m, expected, n) == 0 ? 0 : 1);
}}
int main(void) {{ probe(7, {divisor}); return 2; }}
"#
            );
            compile_and_run(&output, &harness);
        }
    }
}

#[test]
fn failed_source_assertion_calls_the_board_defect_hook_without_continuing() {
    let output = source_output(
        r#"
@external(c, "board_after")
fn after() -> Unit
@export(c, "probe")
pub fn probe() -> Unit { assert False
 after() }
"#,
    );
    compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  static const char prefix[] = "assertion failed";
  exit(k == EMELA_DEFECT_ASSERT && n >= sizeof prefix - 1 && memcmp(m, prefix, sizeof prefix - 1) == 0 ? 0 : 1);
}
void board_after(void) { exit(3); }
int main(void) { probe(); return 2; }
"#,
    );
}

#[test]
fn entry_source_and_program_entry_are_selected_explicitly() {
    let sources = [
        Source::new("unused.emel", "pub fn other() -> Int { 99 }"),
        Source::new(
            "selected.emel",
            "@external(c, \"board_result\")\nfn record(n: Int) -> Unit\npub fn run() -> Unit { record(42) }",
        ),
    ];
    let options = Options::default()
        .with_entry("run")
        .with_heap_bytes(0)
        .with_string_capacity(32);
    let output = compile_sources(&sources, "selected.emel", &options).unwrap();
    assert!(output.exports.is_empty());
    assert!(!output.header.contains("emela_export_run"));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include "emela_embedded.h"
static int recorded;
void board_result(int32_t n) { recorded = n; }
void emela_platform_start(void (*program)(void)) { program(); }
int main(void) { emela_embedded_start(); assert(recorded == 42); return 0; }
"#,
    );
}

#[test]
fn only_explicit_c_exports_cross_module_public_boundaries() {
    let sources = [
        Source::new(
            "main.emel",
            "import Math.{answer}\npub fn visible() -> Int { answer() }",
        ),
        Source::new(
            "math.emel",
            "pub fn helper() -> Int { 41 }\n@export(c, \"imported_answer\")\npub fn answer() -> Int { helper() + 1 }",
        ),
    ];
    let output = compile_sources(&sources, "main.emel", &Options::default()).unwrap();
    assert_eq!(output.exports.len(), 1);
    assert_eq!(output.exports[0].c_symbol, "imported_answer");
    assert!(!output.header.contains("visible"));
    assert!(!output.header.contains("helper"));
    compile_and_run(
        &output,
        "#include \"emela_embedded.h\"\nint main(void) { return imported_answer() == 42 ? 0 : 1; }",
    );
}

#[test]
fn diagnostics_identify_source_and_precise_invalid_expression() {
    let invalid = [
        ("pub fn answer() -> Int { missing }", "missing"),
        ("pub fn answer() -> Int { True }", "True"),
        (
            "fn f(a: Int) -> Int { a }\npub fn answer() -> Int { f() }",
            "f()",
        ),
        (
            "fn f(a: Int) -> Int { a }\npub fn answer() -> Int { f(True) }",
            "True",
        ),
        (
            "@export(c, \"same\")\npub fn first() -> Int { 1 }\n@export(c, \"same\")\npub fn second() -> Int { 2 }",
            "same",
        ),
    ];
    for (source, relevant) in invalid {
        let errors = compile_sources(
            &[Source::new("bad.emel", source)],
            "bad.emel",
            &Options::default(),
        )
        .unwrap_err();
        assert!(!errors.is_empty());
        for error in &errors {
            assert_eq!(error.source.as_deref(), Some("bad.emel"));
            assert!(error.span.start <= error.span.end && error.span.end <= source.len());
            assert!(
                source.is_char_boundary(error.span.start)
                    && source.is_char_boundary(error.span.end)
            );
            assert!(!error.message.is_empty());
        }
        assert!(
            errors.iter().any(|error| error.span.start < error.span.end
                && (source[error.span.start..error.span.end].contains(relevant)
                    || relevant.contains(&source[error.span.start..error.span.end]))),
            "{source}: {errors:?}"
        );
    }
    let source = "@export(c, \"answer\")\npub fn answer() -> Int { 2147483648 }";
    assert_eq!(compile(source).unwrap_err()[0].kind, DiagnosticKind::Range);
}

#[test]
fn private_imports_unknown_modules_and_unknown_entry_sources_are_rejected() {
    for main in [
        "import Math.{secret}\npub fn answer() -> Int { secret() }",
        "import Missing.{secret}\npub fn answer() -> Int { secret() }",
    ] {
        let sources = [
            Source::new("main.emel", main),
            Source::new("math.emel", "fn secret() -> Int { 42 }"),
        ];
        let errors = compile_sources(&sources, "main.emel", &Options::default()).unwrap_err();
        assert_eq!(errors[0].source.as_deref(), Some("main.emel"));
    }
    assert!(
        compile_sources(
            &[Source::new("main.emel", "pub fn answer() -> Int { 42 }")],
            "missing.emel",
            &Options::default()
        )
        .is_err()
    );
}

#[test]
fn never_returning_external_preserves_the_unreachable_contract() {
    let output = source_output(
        r#"
@external(c, "board_stop")
fn stop() -> Never
@export(c, "never_probe")
pub fn never_probe() -> Never { stop() }
@export(c, "never_as_int")
pub fn never_as_int() -> Int { stop() }
"#,
    );
    for symbol in ["never_probe", "never_as_int"] {
        let harness = format!(
            r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {{ (void)k; (void)m; (void)n; exit(3); }}
_Noreturn void board_stop(void) {{ exit(0); }}
int main(void) {{ {symbol}(); return 2; }}
"#
        );
        compile_and_run(&output, &harness);
    }
}

#[test]
fn unit_arguments_preserve_side_effects_without_c_parameters() {
    let output = source_output(
        r#"
@external(c, "board_mark")
fn mark() -> ()
@external(c, "board_take")
fn take(n: Int) -> Int
fn count(empty: (), n: Int) -> Int {
  if n == 0 { 42 } else { count(empty, n - 1) }
}
fn helper(first: (), n: Int, last: ()) -> Int { count(first, n) }
@export(c, "unit_arguments")
pub fn value(ignored: ()) -> Int {
  assert ignored == ()
  helper(mark(), take(100000), mark())
}
"#,
    );
    assert!(output.header.contains("int32_t unit_arguments(void);"));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
static int order;
void board_mark(void) { order = order * 10 + 1; }
int32_t board_take(int32_t n) { assert(order == 1); order = order * 10 + 2; return n; }
int main(void) { assert(unit_arguments() == 42); assert(order == 121); return 0; }
"#,
    );
}
