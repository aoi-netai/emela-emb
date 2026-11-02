use emela_emb_compiler::{DiagnosticKind, compile};

const INPUT: &str = include_str!("../../../examples/minimal/main.emel");
const C: &str = include_str!("snapshots/emela_embedded.c");
const HEADER: &str = include_str!("snapshots/emela_embedded.h");

#[test]
fn accepted_input_matches_complete_c_and_header_snapshots() {
    let output = compile(INPUT).unwrap();
    assert_eq!(output.c_source, C);
    assert_eq!(output.header, HEADER);
    assert_eq!(compile(INPUT).unwrap(), output);
}

#[test]
fn export_name_and_value_come_from_source_and_source_name_is_independent() {
    for (source_name, export_name, digits, normalized) in [
        ("value", "read_value", "7", "7"),
        ("userId", "TIM2_IRQHandler", "0", "0"),
        ("_value2", "get_max", "2147483647", "2147483647"),
        ("fails", "read_value2", "00042", "42"),
    ] {
        let source =
            format!("@export(c, \"{export_name}\")\npub fn {source_name}() -> Int {{ {digits} }}");
        let output = compile(&source).unwrap();
        assert_eq!(
            output.c_source,
            C.replace("answer", export_name).replace("42", normalized)
        );
        assert_eq!(output.header, HEADER.replace("answer", export_name));
    }
    assert_eq!(
        compile(&INPUT.replace("fn answer", "fn renamed")).unwrap(),
        compile(INPUT).unwrap()
    );
}

#[test]
fn accepted_layout_has_identical_generated_content() {
    let source =
        "\r\n@export(\r\n c,\r\n\t\"answer\"\r\n)\r\n\r\npub fn answer(\r\n)\r\n -> Int { 42 }\r\n";
    assert_eq!(compile(source).unwrap(), compile(INPUT).unwrap());
}

#[test]
fn invalid_and_unsupported_inputs_return_positioned_errors() {
    let cases = [
        ("", DiagnosticKind::Syntax),
        ("# not supported", DiagnosticKind::Lexical),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { 42",
            DiagnosticKind::Syntax,
        ),
        (
            "@export(c, \"answer\")\npub fn answer() -> Float { 42 }",
            DiagnosticKind::Type,
        ),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { 2147483648 }",
            DiagnosticKind::Range,
        ),
        (
            "@export(c, \"int32_t\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Export,
        ),
        (
            "@export(c, \"é\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Export,
        ),
        (
            "@export(js, \"answer\")\npub fn answer() -> Int { 42 }",
            DiagnosticKind::Unsupported,
        ),
        (
            "@export(c, \"answer\")\npub fn answer(a) -> Int { 42 }",
            DiagnosticKind::Unsupported,
        ),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { 42 43 }",
            DiagnosticKind::Syntax,
        ),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { 42 } 43",
            DiagnosticKind::Syntax,
        ),
    ];
    for (source, kind) in cases {
        let diagnostics = compile(source).expect_err(source);
        assert_eq!(diagnostics[0].kind, kind, "{source:?}");
        for diagnostic in diagnostics {
            assert!(diagnostic.span.start <= diagnostic.span.end);
            assert!(diagnostic.span.end <= source.len());
            assert!(source.is_char_boundary(diagnostic.span.start));
            assert!(source.is_char_boundary(diagnostic.span.end));
            assert!(!diagnostic.message.is_empty());
        }
    }
}
