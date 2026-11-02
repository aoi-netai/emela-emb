//! String scanning and escape decoding, with original UTF-8 byte positions retained.
use crate::{Diagnostic, DiagnosticKind, Span};

pub(crate) fn scan(source: &str, start: usize) -> Result<(usize, usize, bool), Diagnostic> {
    let multiline = source[start..].starts_with("\"\"\"");
    let width = if multiline { 3 } else { 1 };
    let mut cursor = start + width;
    if multiline && !source[cursor..].starts_with('\n') && !source[cursor..].starts_with("\r\n") {
        return Err(error(
            Span::new(start, cursor),
            "multiline string opening must be followed by a line break",
        ));
    }
    while cursor < source.len() {
        if source[cursor..].starts_with(if multiline { "\"\"\"" } else { "\"" }) {
            return Ok((cursor, cursor + width, multiline));
        }
        if source[cursor..].starts_with("#{") {
            cursor = interpolation_end(source, cursor + 2)? + 1;
            continue;
        }
        let ch = source[cursor..].chars().next().unwrap();
        if !multiline && matches!(ch, '\r' | '\n') {
            return Err(error(
                Span::new(start, cursor),
                "string cannot contain a line break",
            ));
        }
        cursor += ch.len_utf8();
        if ch == '\\' {
            let Some(escaped) = source[cursor..].chars().next() else {
                break;
            };
            if matches!(escaped, '\r' | '\n') {
                return Err(error(
                    Span::new(cursor - 1, cursor + escaped.len_utf8()),
                    "a string escape cannot continue a line",
                ));
            }
            cursor += escaped.len_utf8();
        }
    }
    Err(error(Span::new(start, source.len()), "unterminated string"))
}
pub(crate) fn interpolation_end(source: &str, start: usize) -> Result<usize, Diagnostic> {
    let mut cursor = start;
    let mut braces = 0;
    while cursor < source.len() {
        if source[cursor..].starts_with('"') {
            cursor = scan(source, cursor)?.1;
            continue;
        }
        let ch = source[cursor..].chars().next().unwrap();
        match ch {
            '#' => {
                while cursor < source.len() && !source[cursor..].starts_with('\n') {
                    cursor += source[cursor..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            '{' => braces += 1,
            '}' => {
                if braces == 0 {
                    return Ok(cursor);
                } else {
                    braces -= 1;
                }
            }
            _ => {}
        }
        cursor += ch.len_utf8();
    }
    Err(error(
        Span::new(start.saturating_sub(2), source.len()),
        "unterminated string interpolation",
    ))
}

pub(crate) fn content(
    raw: &str,
    span: Span,
    multiline: bool,
) -> Result<(String, Vec<usize>), Diagnostic> {
    let width = if multiline { 3 } else { 1 };
    let base = span.start + width;
    if !multiline {
        return Ok((raw.into(), (base..=base + raw.len()).collect()));
    }
    let initial = if raw.starts_with("\r\n") { 2 } else { 1 };
    let last_line = raw.rfind('\n').map_or(initial, |p| p + 1);
    let indent = &raw[last_line..];
    let standalone = indent.chars().all(|ch| matches!(ch, ' ' | '\t'));
    let end = if standalone {
        last_line.saturating_sub(1).max(initial)
    } else {
        raw.len()
    };
    let mut output = String::new();
    let mut positions = Vec::new();
    let mut cursor = initial;
    while cursor < end {
        let line_end = raw[cursor..end].find('\n').map_or(end, |p| cursor + p);
        let mut start = cursor;
        if standalone && line_end > cursor {
            if !raw[cursor..line_end].starts_with(indent) {
                return Err(error(
                    Span::new(base + cursor, base + line_end),
                    "multiline string line has less indentation than the closing quotes",
                ));
            }
            start += indent.len();
        }
        let mut text_end = line_end;
        if raw.as_bytes().get(text_end.wrapping_sub(1)) == Some(&b'\r') {
            text_end -= 1;
        }
        output.push_str(&raw[start..text_end]);
        positions.extend((start..text_end).map(|p| base + p));
        if line_end < end {
            output.push('\n');
            positions.push(base + line_end);
        }
        cursor = line_end + 1;
    }
    positions.push(base + end);
    Ok((output, positions))
}

pub(crate) fn escape(
    source: &str,
    cursor: &mut usize,
    positions: &[usize],
) -> Result<char, Diagnostic> {
    let start = *cursor;
    *cursor += 1;
    let Some(ch) = source[*cursor..].chars().next() else {
        return Err(error(
            Span::new(positions[start], positions[*cursor]),
            "unterminated escape",
        ));
    };
    *cursor += ch.len_utf8();
    match ch {
        '"' => Ok('"'),
        '\\' => Ok('\\'),
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        '#' => Ok('#'),
        'u' => {
            if !source[*cursor..].starts_with('{') {
                return Err(error(
                    Span::new(positions[start], positions[*cursor]),
                    "Unicode escape requires '{'",
                ));
            }
            *cursor += 1;
            let digits = *cursor;
            while source
                .as_bytes()
                .get(*cursor)
                .is_some_and(u8::is_ascii_hexdigit)
            {
                *cursor += 1;
            }
            let text = &source[digits..*cursor];
            if text.is_empty() || text.len() > 6 || !source[*cursor..].starts_with('}') {
                return Err(error(
                    Span::new(positions[start], positions[*cursor]),
                    "Unicode escape requires 1 to 6 hexadecimal digits and '}'",
                ));
            }
            *cursor += 1;
            u32::from_str_radix(text, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| {
                    error(
                        Span::new(positions[start], positions[*cursor]),
                        "Unicode escape is not a Unicode scalar value",
                    )
                })
        }
        _ => Err(error(
            Span::new(positions[start], positions[*cursor]),
            "unknown string escape",
        )),
    }
}
fn error(span: Span, message: &str) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Lexical, span, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{ast::ExprKind, lexer, parser};
    #[test]
    fn scans_nested_interpolation_strings_and_comments() {
        for text in [
            r##""first #{"nested #{42}"} last""##,
            "\"#{\n# comment\n42\n}\"",
            "\"a\\\"b\"",
        ] {
            let (_, end, _) = scan(text, 0).unwrap();
            assert_eq!(end, text.len());
            assert!(lexer::lex(text).is_ok());
        }
    }
    #[test]
    fn decodes_utf8_escapes_and_multiline_indentation() {
        let source = r#"fn value() -> String { "é\n\t\r\\\"\#\u{0}\u{1F680}" }"#;
        let module = parser::parse(&lexer::lex(source).unwrap()).unwrap();
        let body = module.functions[0].body.as_ref().unwrap();
        let crate::frontend::ast::Statement::Expr(expr) = &body.statements[0] else {
            panic!()
        };
        assert!(matches!(&expr.kind, ExprKind::String(text) if text == "é\n\t\r\\\"#\0🚀"));
        let raw = "\n  first\n  second\n  ";
        let (content, positions) = content(raw, Span::new(0, raw.len() + 6), true).unwrap();
        assert_eq!(content, "first\nsecond");
        assert_eq!(content.len() + 1, positions.len());
    }
    #[test]
    fn rejects_malformed_escapes_and_interpolation_without_panicking() {
        for value in [
            r#""\z""#,
            r#""\u{}""#,
            r#""\u{D800}""#,
            r#""\u{110000}""#,
            r#""\u{0000000}""#,
            r##""#{42""##,
        ] {
            let source = format!("fn value() -> String {{ {value} }}");
            let result = lexer::lex(&source).and_then(|tokens| parser::parse(&tokens));
            let error = result.unwrap_err();
            assert!(error.span.end <= source.len(), "{source}");
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
        }
    }
}
