use emela_emb_compiler::{DiagnosticKind, Options, Source, compile, compile_sources};

#[test]
fn source_names_functions_visibility_and_shadowing_are_resolved() {
    let sources = [
        Source::new(
            "/app/main.emel",
            "import Math.{add}\n@export(c, \"answer\")\npub fn value() -> Int { x = add(20, 22)\n x = x + 1\n assert x == 43\n { x = 99\n assert x == 99 }\n x }",
        ),
        Source::new(
            "/app/math.emel",
            "pub fn add(a: Int, b: Int) -> Int { a + b }\nfn hidden() -> Int { 0 }",
        ),
        Source::new("/app/unreachable.emel", "@ malformed unparsed source"),
    ];
    let output = compile_sources(&sources, sources[0].name, &Options::default()).unwrap();
    assert_eq!(output.exports.len(), 1);
    assert_eq!(output.exports[0].c_symbol, "answer");
    assert!(output.header.contains("int32_t answer(void)"));
}

#[test]
fn checks_scalar_signatures_numeric_context_and_tail_recursion() {
    for source in [
        "pub fn count(n: Int, total: Int) -> Int { if n == 0 { total } else { count(n - 1, total + n) } }",
        "pub fn count(n: Int64, total: Int64) -> Int64 { if n == 0 { total } else { count(n - 1, total + n) } }",
        "pub fn number() -> Int64 { 0x7FFF_FFFF_FFFF_FFFF }",
        "pub fn value() -> Int { Int.bit_and(0b1010, 0xF) }",
        "pub fn equal(a: Bool, b: Bool) -> Bool { !a || a && b }",
        "pub fn unit() -> () { assert True\n () }",
        "pub fn unit_equal() -> Bool { () == () }",
        "pub fn real(a: Float) -> Float { a / 2.0 + 1.0e-2 }",
        "fn add(a: Int, b: Int) { a + b }\npub fn answer() { add(20, 22) }",
        "pub fn value() -> Int { x: Int64 = 2147483648\n Int64.to_int(x) }",
        "pub fn value() -> Int { 42 |> Int.bit_and(63) }",
    ] {
        compile(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    }
}

#[test]
fn rejects_invalid_names_types_arity_and_boundaries_with_source_positions() {
    let cases = [
        ("pub fn f() -> Int { missing }", DiagnosticKind::Name),
        (
            "fn g(a: Int) -> Int { a }\npub fn f() -> Int { g() }",
            DiagnosticKind::Type,
        ),
        ("pub fn f() -> Int { True }", DiagnosticKind::Type),
        ("pub fn f() -> Int { 1.0 }", DiagnosticKind::Type),
        ("pub fn f() -> Int { 2147483648 }", DiagnosticKind::Range),
        ("pub fn f() -> Int { -2147483648 }", DiagnosticKind::Range),
        (
            "pub fn f() -> Int64 { 9223372036854775808 }",
            DiagnosticKind::Range,
        ),
        ("pub fn f() -> Int { if True { 1 } }", DiagnosticKind::Type),
        (
            "pub fn f() -> Bool { assert 1\n True }",
            DiagnosticKind::Type,
        ),
        ("fn f(a: Int, a: Int) { a }", DiagnosticKind::Name),
        (
            "@intrinsic\npub fn evil() -> Int",
            DiagnosticKind::Unsupported,
        ),
        ("fn f() { f() }", DiagnosticKind::Type),
        ("pub fn f() -> Int { x = 1\n x() }", DiagnosticKind::Type),
        (
            "@export(c, \"same\")\npub fn a() { 1 }\n@export(c, \"same\")\npub fn b() { 2 }",
            DiagnosticKind::Export,
        ),
        (
            "pub fn f() -> Int { Int.shift_left(1, 1.0) }",
            DiagnosticKind::Type,
        ),
    ];
    for (source, expected) in cases {
        let errors = compile(source).unwrap_err();
        assert_eq!(errors[0].kind, expected, "{source}: {errors:?}");
        for error in errors {
            assert_eq!(error.source.as_deref(), Some("main.emel"));
            assert!(error.span.end <= source.len());
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
        }
    }
}

#[test]
fn rejects_private_missing_cyclic_and_conflicting_imports() {
    for (main, math) in [
        (
            "import Math.{hidden}\npub fn value() { hidden() }",
            "fn hidden() { 1 }",
        ),
        (
            "import Missing\npub fn value() { 1 }",
            "pub fn value() { 1 }",
        ),
        (
            "import Math\npub fn value() { Math.hidden() }",
            "fn hidden() { 1 }",
        ),
        (
            "import Math\npub fn value() { 1 }",
            "import Main\npub fn value() { 1 }",
        ),
        (
            "import Math.{value}\nfn value() { 1 }",
            "pub fn value() { 1 }",
        ),
    ] {
        assert!(
            compile_sources(
                &[
                    Source::new("main.emel", main),
                    Source::new("math.emel", math)
                ],
                "main.emel",
                &Options::default()
            )
            .is_err()
        );
    }
}

#[test]
fn emela_public_visibility_is_separate_from_explicit_c_exports() {
    let output = compile("pub fn helper() -> Int { 42 }\nfn main() -> Int { helper() }").unwrap();
    assert!(output.exports.is_empty());
    assert!(!output.header.contains("helper("));
    assert!(!output.header.contains("emela_export_helper"));
    let sources = [
        Source::new(
            "main.emel",
            "import Math\nfn main() -> Int { Math.add(20, 22) }",
        ),
        Source::new(
            "math.emel",
            "@export(c, \"sum\")\npub fn add(a: Int, b: Int) -> Int { a + b }",
        ),
    ];
    let output = compile_sources(&sources, "main.emel", &Options::default()).unwrap();
    assert_eq!(output.exports.len(), 1);
    assert_eq!(output.exports[0].c_symbol, "sum");
    assert!(output.header.contains("int32_t sum("));
}

#[test]
fn rejects_cross_module_exports_and_conflicting_c_boundaries() {
    let sources = [
        Source::new(
            "main.emel",
            "import Math\n@export(c, \"answer\")\npub fn value() -> Int { 42 }",
        ),
        Source::new(
            "math.emel",
            "@export(c, \"answer\")\npub fn value() -> Int { 42 }",
        ),
    ];
    let errors = compile_sources(&sources, "main.emel", &Options::default()).unwrap_err();
    assert_eq!(errors[0].kind, DiagnosticKind::Export);
    assert_eq!(errors[0].source.as_deref(), Some("math.emel"));
    for source in [
        "@external(c, \"answer\")\nfn read() -> Int\n@export(c, \"answer\")\npub fn value() -> Int { read() }",
        "@external(c, \"read\")\nfn read_int() -> Int\n@external(c, \"read\")\nfn read_float() -> Float\nfn main() -> Int { read_int() }",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn retains_entry_filename_compatibility_and_parenthesized_comparisons() {
    for name in ["-input.emel", "input-�.emel", "arbitrary.input"] {
        compile_sources(
            &[Source::new(
                name,
                "@export(c, \"answer\")\npub fn answer() -> Int { 42 }",
            )],
            name,
            &Options::default(),
        )
        .unwrap();
    }
    compile("@export(c, \"compare\")\npub fn compare() -> Bool { (1 < 2) == True }").unwrap();
    compile("fn value(a\n:\nInt) -> Int { a }\nfn main() -> Int { value(42) }").unwrap();
    assert_eq!(
        compile("# empty entry").unwrap_err()[0].kind,
        DiagnosticKind::Syntax
    );
}
