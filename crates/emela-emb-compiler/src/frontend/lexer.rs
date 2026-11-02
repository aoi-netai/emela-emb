use crate::{Diagnostic, DiagnosticKind, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind<'a> {
    Name(&'a str),
    Integer(&'a str),
    Float(&'a str),
    String(&'a str),
    At,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Arrow,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Pipe,
    Newline,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub span: Span,
}

/// Newlines remain tokens: parentheses continue lines, while an inner block resets that rule.
pub(crate) fn lex(source: &str) -> Result<Vec<Token<'_>>, Diagnostic> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let start = pos;
        let single = match bytes[pos] {
            b' ' | b'\t' => {
                pos += 1;
                continue;
            }
            b'#' => {
                while pos < bytes.len() && !matches!(bytes[pos], b'\r' | b'\n') {
                    pos += 1;
                }
                continue;
            }
            b'\n' => Some(TokenKind::Newline),
            b'\r' if bytes.get(pos + 1) == Some(&b'\n') => {
                pos += 1;
                Some(TokenKind::Newline)
            }
            b'@' => Some(TokenKind::At),
            b'(' => Some(TokenKind::LParen),
            b')' => Some(TokenKind::RParen),
            b'{' => Some(TokenKind::LBrace),
            b'}' => Some(TokenKind::RBrace),
            b'[' => Some(TokenKind::LBracket),
            b']' => Some(TokenKind::RBracket),
            b',' => Some(TokenKind::Comma),
            b':' => Some(TokenKind::Colon),
            b'.' => Some(TokenKind::Dot),
            b'+' => Some(TokenKind::Plus),
            b'*' => Some(TokenKind::Star),
            b'/' => Some(TokenKind::Slash),
            b'%' => Some(TokenKind::Percent),
            b'-' if bytes.get(pos + 1) == Some(&b'>') => {
                pos += 1;
                Some(TokenKind::Arrow)
            }
            b'-' => Some(TokenKind::Minus),
            b'=' if bytes.get(pos + 1) == Some(&b'=') => {
                pos += 1;
                Some(TokenKind::EqEq)
            }
            b'=' => Some(TokenKind::Eq),
            b'!' if bytes.get(pos + 1) == Some(&b'=') => {
                pos += 1;
                Some(TokenKind::Ne)
            }
            b'!' => Some(TokenKind::Bang),
            b'<' if bytes.get(pos + 1) == Some(&b'=') => {
                pos += 1;
                Some(TokenKind::Le)
            }
            b'<' => Some(TokenKind::Lt),
            b'>' if bytes.get(pos + 1) == Some(&b'=') => {
                pos += 1;
                Some(TokenKind::Ge)
            }
            b'>' => Some(TokenKind::Gt),
            b'&' if bytes.get(pos + 1) == Some(&b'&') => {
                pos += 1;
                Some(TokenKind::AndAnd)
            }
            b'|' if bytes.get(pos + 1) == Some(&b'|') => {
                pos += 1;
                Some(TokenKind::OrOr)
            }
            b'|' if bytes.get(pos + 1) == Some(&b'>') => {
                pos += 1;
                Some(TokenKind::Pipe)
            }
            _ => None,
        };
        if let Some(kind) = single {
            pos += 1;
            tokens.push(Token {
                kind,
                span: Span::new(start, pos),
            });
            continue;
        }
        match bytes[pos] {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                pos += 1;
                while bytes
                    .get(pos)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    pos += 1;
                }
                let word = &source[start..pos];
                if word.as_bytes()[0].is_ascii_uppercase()
                    && word.contains('_')
                    && word.bytes().any(|b| b.is_ascii_lowercase())
                {
                    return Err(error(
                        Span::new(start, pos),
                        "type names cannot contain underscores",
                    ));
                }
                tokens.push(Token {
                    kind: TokenKind::Name(word),
                    span: Span::new(start, pos),
                });
            }
            b'0'..=b'9' => {
                let prefixed = source[pos..].starts_with("0x") || source[pos..].starts_with("0b");
                if prefixed {
                    let radix = if bytes[pos + 1] == b'x' { 16 } else { 2 };
                    pos += 2;
                    let digits = pos;
                    while bytes
                        .get(pos)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    {
                        pos += 1;
                    }
                    let raw = &source[digits..pos];
                    if raw.is_empty() || !valid_digits(raw, radix) || bytes.get(pos) == Some(&b'.')
                    {
                        return Err(error(
                            Span::new(start, pos),
                            "invalid hexadecimal or binary integer literal",
                        ));
                    }
                    tokens.push(Token {
                        kind: TokenKind::Integer(&source[start..pos]),
                        span: Span::new(start, pos),
                    });
                    continue;
                }
                while bytes
                    .get(pos)
                    .is_some_and(|b| b.is_ascii_digit() || *b == b'_')
                {
                    pos += 1;
                }
                if !valid_digits(&source[start..pos], 10) {
                    return Err(error(Span::new(start, pos), "invalid number separator"));
                }
                let mut float = false;
                if bytes.get(pos) == Some(&b'.')
                    && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit)
                {
                    float = true;
                    pos += 1;
                    let fraction = pos;
                    while bytes
                        .get(pos)
                        .is_some_and(|b| b.is_ascii_digit() || *b == b'_')
                    {
                        pos += 1;
                    }
                    if !valid_digits(&source[fraction..pos], 10) {
                        return Err(error(Span::new(start, pos), "invalid fraction"));
                    }
                    if bytes.get(pos).is_some_and(|b| matches!(b, b'e' | b'E')) {
                        pos += 1;
                        if bytes.get(pos).is_some_and(|b| matches!(b, b'+' | b'-')) {
                            pos += 1;
                        }
                        let exponent = pos;
                        while bytes
                            .get(pos)
                            .is_some_and(|b| b.is_ascii_digit() || *b == b'_')
                        {
                            pos += 1;
                        }
                        if !valid_digits(&source[exponent..pos], 10) {
                            return Err(error(
                                Span::new(start, pos),
                                "expected floating-point exponent digits",
                            ));
                        }
                    }
                }
                if bytes
                    .get(pos)
                    .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                {
                    while bytes
                        .get(pos)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    {
                        pos += 1;
                    }
                    return Err(error(
                        Span::new(start, pos),
                        "invalid numeric suffix; Float exponents require a decimal point",
                    ));
                }
                tokens.push(Token {
                    kind: if float {
                        TokenKind::Float(&source[start..pos])
                    } else {
                        TokenKind::Integer(&source[start..pos])
                    },
                    span: Span::new(start, pos),
                });
            }
            b'"' => {
                if source[pos..].starts_with("\"\"\"") {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Unsupported,
                        Span::new(pos, pos + 3),
                        "multiline strings require P2",
                    ));
                }
                pos += 1;
                let content = pos;
                while pos < bytes.len() && bytes[pos] != b'"' {
                    let ch = source[pos..].chars().next().unwrap();
                    if matches!(ch, '\n' | '\r') {
                        return Err(error(
                            Span::new(start, pos),
                            "string cannot contain a line break",
                        ));
                    }
                    if ch == '\\' || source[pos..].starts_with("#{") {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Unsupported,
                            Span::new(pos, pos + ch.len_utf8()),
                            "string escapes and interpolation require P2",
                        ));
                    }
                    pos += ch.len_utf8();
                }
                if pos == bytes.len() {
                    return Err(error(Span::new(start, pos), "unterminated string"));
                }
                tokens.push(Token {
                    kind: TokenKind::String(&source[content..pos]),
                    span: Span::new(start, pos + 1),
                });
                pos += 1;
            }
            _ => {
                let ch = source[pos..].chars().next().unwrap();
                return Err(error(
                    Span::new(pos, pos + ch.len_utf8()),
                    format!("unrecognized character {ch:?}"),
                ));
            }
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(pos, pos),
    });
    Ok(tokens)
}

fn valid_digits(text: &str, radix: u32) -> bool {
    !text.is_empty()
        && !text.starts_with('_')
        && !text.ends_with('_')
        && !text.contains("__")
        && text.chars().all(|c| c == '_' || c.is_digit(radix))
}
fn error(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Lexical, span, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexes_comments_numbers_and_exact_utf8_spans() {
        let source = "# comment\r\n1_024 0xFF 0b10 1.0e-9 != True";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Newline);
        assert_eq!(tokens[1].kind, TokenKind::Integer("1_024"));
        assert_eq!(tokens[4].kind, TokenKind::Float("1.0e-9"));
        for t in tokens {
            assert!(source.is_char_boundary(t.span.start));
            assert!(source.is_char_boundary(t.span.end));
        }
        assert_eq!(lex(" \té").unwrap_err().span, Span::new(2, 4));
    }
    #[test]
    fn refuses_malformed_numbers_without_panicking() {
        for source in [
            "1e9",
            "3px",
            "0XFF",
            "0b2",
            "0x",
            "1_",
            "1__2",
            "1.0e",
            "Http_client",
            "\r",
            "\u{3000}",
        ] {
            assert!(lex(source).is_err(), "{source}");
        }
    }
}
