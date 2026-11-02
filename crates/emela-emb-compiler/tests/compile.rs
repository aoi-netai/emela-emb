use emela_emb_compiler::{DiagnosticKind, compile};

const INPUT: &str = include_str!("../../../examples/minimal/main.emel");

#[test]
fn accepted_input_returns_complete_and_deterministic_generated_files() {
    let output = compile(INPUT).unwrap();
    assert_eq!(output.files, compile(INPUT).unwrap().files);
    assert_eq!(
        output
            .files
            .iter()
            .find(|file| file.path == "emela_embedded.c")
            .unwrap()
            .contents,
        output.c_source
    );
    assert_eq!(
        output
            .files
            .iter()
            .find(|file| file.path == "emela_embedded.h")
            .unwrap()
            .contents,
        output.header
    );
    for path in [
        "emela_embedded.c",
        "emela_embedded.h",
        "emela_board.h",
        "emela_artifact.json",
    ] {
        assert!(
            output.files.iter().any(|file| file.path == path),
            "missing generated file: {path}"
        );
    }
    assert_eq!(output.exports.len(), 1);
    assert_eq!(output.exports[0].source_name, "answer");
    assert!(!output.exports[0].suspends);
}

#[test]
fn export_name_and_value_come_from_source_and_source_name_is_independent() {
    for (source_name, export_name, digits) in [
        ("value", "read_value", "7"),
        ("userId", "TIM2_IRQHandler", "0"),
        ("_value2", "get_max", "2147483647"),
        ("fails", "read_value2", "00042"),
    ] {
        let source =
            format!("@export(c, \"{export_name}\")\npub fn {source_name}() -> Int {{ {digits} }}");
        let output = compile(&source).unwrap();
        assert_eq!(output.exports[0].source_name, source_name);
        assert!(
            output
                .files
                .iter()
                .any(|file| file.contents.contains(export_name))
        );
        assert!(!output.exports[0].suspends);
    }
}

#[test]
fn accepted_layout_has_identical_generated_content() {
    let source =
        "\r\n@export(\r\n c,\r\n\t\"answer\"\r\n)\r\n\r\npub fn answer(\r\n)\r\n -> Int { 42 }\r\n";
    assert_eq!(
        compile(source).unwrap().files,
        compile(INPUT).unwrap().files
    );
}

#[test]
fn invalid_and_unsupported_inputs_return_positioned_errors() {
    let cases = [
        ("$ not supported", DiagnosticKind::Lexical),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { 42",
            DiagnosticKind::Syntax,
        ),
        (
            "@export(c, \"answer\")\npub fn answer() -> Int { True }",
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
