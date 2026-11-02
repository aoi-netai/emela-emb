use crate::{Diagnostic, DiagnosticKind};

use super::ast::Module;

/// Constructed only after all subset and boundary checks have succeeded.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CheckedModule<'a> {
    pub export_symbol: &'a str,
    pub value: i32,
}

pub(crate) fn check(module: Module<'_>) -> Result<CheckedModule<'_>, Vec<Diagnostic>> {
    let function = module.function;
    let mut errors = Vec::new();
    if function.export.target.value != "c" {
        errors.push(Diagnostic::new(
            DiagnosticKind::Unsupported,
            function.export.target.span,
            "only the C export target 'c' is supported",
        ));
    }
    if function.return_type.value != "Int" {
        errors.push(Diagnostic::new(
            DiagnosticKind::Type,
            function.return_type.span,
            format!(
                "function '{}' must return Int; return type '{}' is unsupported",
                function.name.value, function.return_type.value
            ),
        ));
    }
    if !valid_export_name(function.export.symbol.value) {
        errors.push(Diagnostic::new(
            DiagnosticKind::Export,
            function.export.symbol.span,
            "C export name must be a non-reserved ASCII C identifier (see docs/language-subset.md)",
        ));
    }
    let value = function.value.value.parse::<i32>();
    if value.is_err() {
        errors.push(Diagnostic::new(
            DiagnosticKind::Range,
            function.value.span,
            "Int literal must be in the range 0..=2147483647",
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(CheckedModule {
        export_symbol: function.export.symbol.value,
        value: value.expect("range was checked above"),
    })
}

fn valid_export_name(name: &str) -> bool {
    let mut chars = name.bytes();
    if !chars.next().is_some_and(|b| b.is_ascii_alphabetic())
        || !chars.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    if matches!(
        name,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "main"
    ) {
        return false;
    }
    // Leading '_' was rejected above, including C11's underscore keywords.
    // Reserve stdint type/macro families and the generated header namespace.
    if (name.starts_with("int") || name.starts_with("uint")) && name.ends_with("_t") {
        return false;
    }
    ![
        "emela_embedded",
        "EMELA_EMBEDDED",
        "INT",
        "UINT",
        "SIZE_",
        "PTRDIFF_",
        "SIG_ATOMIC_",
        "WCHAR_",
        "WINT_",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{lexer::lex, parser::parse};

    fn checked(source: &str) -> Result<CheckedModule<'_>, Vec<Diagnostic>> {
        check(parse(&lex(source).unwrap()).unwrap())
    }

    #[test]
    fn checks_int_boundaries_and_normalizes_decimal_spelling() {
        for (digits, value) in [("0", 0), ("00042", 42), ("2147483647", i32::MAX)] {
            let source =
                format!("@export(c, \"read_value\")\npub fn value() -> Int {{ {digits} }}");
            assert_eq!(
                checked(&source).unwrap(),
                CheckedModule {
                    export_symbol: "read_value",
                    value
                }
            );
        }
        for digits in ["2147483648".to_owned(), "9".repeat(10_000)] {
            let source = format!("@export(c, \"answer\")\npub fn answer() -> Int {{ {digits} }}");
            let errors = checked(&source).unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].kind, DiagnosticKind::Range);
            assert_eq!(&source[errors[0].span.start..errors[0].span.end], digits);
        }
    }

    #[test]
    fn validates_c_identifiers_and_reserved_names() {
        for name in ["answer", "TIM2_IRQHandler", "a", "read_value2"] {
            assert!(valid_export_name(name), "rejected {name:?}");
        }
        for name in [
            "",
            "42",
            "a-b",
            "a b",
            "é",
            "a\0b",
            "_private",
            "for",
            "_Alignas",
            "main",
            "int32_t",
            "uint_fast16_t",
            "intptr_t",
            "INT32_MAX",
            "UINT64_C",
            "SIZE_MAX",
            "PTRDIFF_MIN",
            "SIG_ATOMIC_MAX",
            "WCHAR_MAX",
            "WINT_MIN",
            "EMELA_EMBEDDED_H",
            "emela_embedded_other",
        ] {
            let source = format!("@export(c, \"{name}\")\npub fn valid() -> Int {{ 1 }}");
            let errors = checked(&source).unwrap_err();
            assert_eq!(errors[0].kind, DiagnosticKind::Export, "accepted {name:?}");
            assert_eq!(
                &source[errors[0].span.start..errors[0].span.end],
                format!("\"{name}\"")
            );
        }
    }

    #[test]
    fn reports_target_type_export_and_range_errors_without_lowering() {
        let source = "@export(js, \"int32_t\")\npub fn value() -> Float { 2147483648 }";
        let errors = checked(source).unwrap_err();
        assert_eq!(
            errors.iter().map(|d| d.kind).collect::<Vec<_>>(),
            [
                DiagnosticKind::Unsupported,
                DiagnosticKind::Type,
                DiagnosticKind::Export,
                DiagnosticKind::Range
            ]
        );
        for error in errors {
            assert!(error.span.start < error.span.end);
            assert!(error.span.end <= source.len());
        }
    }
}
