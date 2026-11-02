use crate::{Diagnostic, DiagnosticKind, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind<'a> {
    Pub,
    Fn,
    Reserved(&'a str),
    Name(&'a str),
    Integer(&'a str),
    String(&'a str),
    At,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Arrow,
    Newline,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub span: Span,
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token<'_>>, Diagnostic> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        let start = pos;
        let kind = match bytes[pos] {
            b' ' | b'\t' => {
                pos += 1;
                continue;
            }
            b'\n' => TokenKind::Newline,
            b'\r' if bytes.get(pos + 1) == Some(&b'\n') => {
                pos += 1;
                TokenKind::Newline
            }
            b'@' => TokenKind::At,
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b',' => TokenKind::Comma,
            b'-' if bytes.get(pos + 1) == Some(&b'>') => {
                pos += 1;
                TokenKind::Arrow
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                pos += 1;
                while bytes
                    .get(pos)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    pos += 1;
                }
                let word = &source[start..pos];
                let kind = match word {
                    "pub" => TokenKind::Pub,
                    "fn" => TokenKind::Fn,
                    "type" | "enum" | "error" | "const" | "trait" | "impl" | "effect"
                    | "handler" | "layer" | "import" | "opaque" | "suspend" | "if" | "else"
                    | "match" | "fail" | "escape" | "use" | "with" | "assert" | "self" | "Self"
                    | "as" | "var" | "return" | "while" | "loop" | "let" | "mut" | "in"
                    | "where" | "do" | "try" | "catch" | "throw" | "async" | "yield" | "defer"
                    | "macro" | "mod" => TokenKind::Reserved(word),
                    _ => TokenKind::Name(word),
                };
                tokens.push(Token {
                    kind,
                    span: Span::new(start, pos),
                });
                continue;
            }
            b'0'..=b'9' => {
                pos += 1;
                while bytes
                    .get(pos)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    pos += 1;
                }
                let digits = &source[start..pos];
                if !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Lexical,
                        Span::new(start, pos),
                        "expected decimal digits only; number prefixes, separators and suffixes are unsupported",
                    ));
                }
                tokens.push(Token {
                    kind: TokenKind::Integer(digits),
                    span: Span::new(start, pos),
                });
                continue;
            }
            b'"' => {
                if source[pos..].starts_with("\"\"\"") {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Unsupported,
                        Span::new(pos, pos + 3),
                        "multiline strings are unsupported",
                    ));
                }
                pos += 1;
                let content = pos;
                while pos < bytes.len() && bytes[pos] != b'"' {
                    let ch = source[pos..]
                        .chars()
                        .next()
                        .expect("position is a character boundary");
                    if matches!(ch, '\n' | '\r') {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Lexical,
                            Span::new(start, pos),
                            "string cannot contain a line break",
                        ));
                    }
                    if ch == '\\' || source[pos..].starts_with("#{") {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Unsupported,
                            Span::new(pos, pos + ch.len_utf8()),
                            "string escapes and interpolation are unsupported",
                        ));
                    }
                    pos += ch.len_utf8();
                }
                if pos == bytes.len() {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Lexical,
                        Span::new(start, pos),
                        "unterminated string",
                    ));
                }
                tokens.push(Token {
                    kind: TokenKind::String(&source[content..pos]),
                    span: Span::new(start, pos + 1),
                });
                pos += 1;
                continue;
            }
            _ => {
                let ch = source[pos..]
                    .chars()
                    .next()
                    .expect("position is a character boundary");
                return Err(Diagnostic::new(
                    DiagnosticKind::Lexical,
                    Span::new(pos, pos + ch.len_utf8()),
                    format!("unrecognized or unsupported character {ch:?}"),
                ));
            }
        };
        pos += 1;
        tokens.push(Token {
            kind,
            span: Span::new(start, pos),
        });
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(pos, pos),
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_input_with_exact_source_spans() {
        let source = include_str!("../../../../examples/minimal/main.emel");
        let tokens = lex(source).unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [
                TokenKind::At,
                TokenKind::Name("export"),
                TokenKind::LParen,
                TokenKind::Name("c"),
                TokenKind::Comma,
                TokenKind::String("answer"),
                TokenKind::RParen,
                TokenKind::Newline,
                TokenKind::Pub,
                TokenKind::Fn,
                TokenKind::Name("answer"),
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Arrow,
                TokenKind::Name("Int"),
                TokenKind::LBrace,
                TokenKind::Newline,
                TokenKind::Integer("42"),
                TokenKind::Newline,
                TokenKind::RBrace,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
        for token in tokens {
            let slice = &source[token.span.start..token.span.end];
            match token.kind {
                TokenKind::Name(text) | TokenKind::Integer(text) => assert_eq!(slice, text),
                TokenKind::String(text) => assert_eq!(slice, format!("\"{text}\"")),
                _ => {}
            }
        }
    }

    #[test]
    fn preserves_newlines_and_does_not_parse_large_integers() {
        let digits = "9".repeat(10_000);
        let source = format!(" \t\r\n{digits}\n");
        let tokens = lex(&source).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Newline);
        assert_eq!(tokens[0].span, Span::new(2, 4));
        assert_eq!(tokens[1].kind, TokenKind::Integer(&digits));
        assert_eq!(tokens[2].kind, TokenKind::Newline);
    }

    #[test]
    fn rejects_invalid_or_unsupported_lexical_forms() {
        for source in [
            "# comment",
            "\r",
            "\u{3000}",
            "é",
            "1_000",
            "0xFF",
            "1e9",
            "42px",
            "-1",
            "\"no\\n\"",
            "\"#{1}\"",
            "\"no\n\"",
            "\"open",
            "\"\"\"\nno\n\"\"\"",
        ] {
            let error = lex(source).expect_err(source);
            assert!(error.span.end <= source.len());
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
        }
        let error = lex(" \té").unwrap_err();
        assert_eq!(error.span, Span::new(2, 4));
    }
}
