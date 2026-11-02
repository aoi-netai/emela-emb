mod support;

use emela_emb_compiler::{Options, Output, Source, compile_sources};
use support::compile_and_run;

fn source_output(source: &str, options: &Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("P2 regression source rejected: {errors:#?}"))
}

#[test]
fn failures_propagate_payloads_and_escape_guards_without_dangling_strings() {
    let output = source_output(
        include_str!("compat/p2/failures.emel"),
        &Options::default()
            .with_string_capacity(64)
            .with_heap_bytes(4096),
    );
    compile_and_run(&output, include_str!("compat/p2/failures-harness.c"));
}

#[test]
fn monomorphic_runtime_consts_preserve_initialization_order_and_cached_defects() {
    let output = source_output(include_str!("compat/p2/consts.emel"), &Options::default());
    compile_and_run(&output, include_str!("compat/p2/consts-harness.c"));
}

#[test]
fn monomorphic_static_consts_fold_numeric_and_string_values() {
    let output = source_output(
        r#"
const WRAPPED: Int = Int.MAX + 1
const TEXT = Int64.to_string(Int64.MIN)
@export(c, "read_wrapped")
pub fn read_wrapped() -> Int { WRAPPED }
@export(c, "read_static_text")
pub fn read_static_text() -> String { TEXT }
"#,
        &Options::default(),
    );
    assert!(!output.c_source.contains("emela_emb_int_to_string"));
    compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <string.h>
#include "emela_embedded.h"
int main(void) {
  assert(read_wrapped() == INT32_MIN);
  EmelaString text = read_static_text();
  assert(text.length == 20 && !text.owner && memcmp(text.data, "-9223372036854775808", 20) == 0);
  return 0;
}
"#,
    );
}

#[test]
fn legacy_sync_traits_respect_custom_fields_defaults_supers_and_failures() {
    let output = source_output(
        concat!(
            include_str!("compat/p2/traits_sync.emel"),
            include_str!("compat/p2/traits.emel")
        ),
        &Options::default().with_string_capacity(256),
    );
    compile_and_run(&output, include_str!("compat/p2/traits-harness.c"));
}

#[test]
fn standard_values_preserve_option_evaluation_float_bits_and_unicode_scalar_boundaries() {
    let output = source_output(
        include_str!("compat/p2/standard-values.emel"),
        &Options::default()
            .with_string_capacity(128)
            .with_heap_bytes(4096),
    );
    compile_and_run(&output, include_str!("compat/p2/standard-values-harness.c"));
}

#[test]
fn unused_generic_bodies_bounds_and_unresolved_c_exports_are_rejected() {
    use emela_emb_compiler::DiagnosticKind;
    let cases = [
        (
            "fn broken[A](value: A) -> A { missing }\n@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Name,
            "missing",
        ),
        (
            "fn broken[A](value: A) -> Int { True }\n@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Type,
            "True",
        ),
        (
            "fn broken[A](value: A) -> String { Show.show(value) }\n@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Type,
            "value",
        ),
        (
            "@export(c, \"generic\")\npub fn generic[A](value: A) -> A { value }",
            DiagnosticKind::Export,
            "generic",
        ),
        (
            "type FloatBox(value: Float) derive Eq, Ord\n@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Type,
            "Ord",
        ),
        (
            "fn larger[A: Ord](a: A, b: A) -> A { if a < b { b } else { a } }\n@export(c, \"answer\")\npub fn answer() -> Float { larger(1.0, 2.0) }",
            DiagnosticKind::Type,
            "larger",
        ),
        (
            "error Boom\ntype Key(value: Int)\ntrait Pure { fn run(self) -> Int }\nimpl Key: Pure { fn run(self) { fail Boom } }\n@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Type,
            "run",
        ),
    ];
    for (source, kind, relevant) in cases {
        let errors = compile_sources(
            &[Source::new("bad.emel", source)],
            "bad.emel",
            &Options::default(),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|error| error.kind == kind),
            "{source}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .all(|error| error.source.as_deref() == Some("bad.emel")),
            "{source}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .all(|error| error.span.start <= error.span.end && error.span.end <= source.len())
        );
        assert!(
            errors.iter().any(|error| error.span.start < error.span.end
                && (source[error.span.start..error.span.end].contains(relevant)
                    || relevant.contains(&source[error.span.start..error.span.end]))),
            "{source}: {errors:?}"
        );
    }
}

#[test]
fn same_trait_names_in_distinct_modules_dispatch_to_their_own_impls() {
    let sources = [
        Source::new(
            "main.emel",
            "import Alpha\nimport Beta\n@export(c, \"qualified_traits\")\npub fn qualified_traits() -> Int { Alpha.Score.score(Alpha.make()) + Beta.Score.score(Beta.make()) }",
        ),
        Source::new(
            "alpha.emel",
            "pub trait Score { fn score(self) -> Int }\npub type Key(value: Int)\nimpl Key: Score { fn score(self) { self.value + 1 } }\npub fn make() -> Key { Key(value: 10) }",
        ),
        Source::new(
            "beta.emel",
            "pub trait Score { fn score(self) -> Int }\npub type Key(value: Int)\nimpl Key: Score { fn score(self) { self.value + 2 } }\npub fn make() -> Key { Key(value: 20) }",
        ),
    ];
    let output = compile_sources(&sources, "main.emel", &Options::default()).unwrap();
    compile_and_run(
        &output,
        "#include \"emela_embedded.h\"\nint main(void) { return qualified_traits() == 33 ? 0 : 1; }",
    );
}
