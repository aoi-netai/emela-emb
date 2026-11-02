use crate::{Diagnostic, DiagnosticKind};

use super::ast::{Export, Function, Module, Text};
use super::lexer::{Token, TokenKind};

pub(crate) fn parse<'a>(tokens: &[Token<'a>]) -> Result<Module<'a>, Diagnostic> {
    Parser { tokens, cursor: 0 }.module()
}

struct Parser<'tokens, 'source> {
    tokens: &'tokens [Token<'source>],
    cursor: usize,
}

impl<'a> Parser<'_, 'a> {
    fn module(mut self) -> Result<Module<'a>, Diagnostic> {
        self.newlines();
        self.expect(
            TokenKind::At,
            "expected @export(c, \"name\") before the function",
        )?;
        let attribute = self.name("expected the export attribute")?;
        if attribute.value != "export" {
            return Err(Diagnostic::new(
                DiagnosticKind::Unsupported,
                attribute.span,
                "only @export is supported",
            ));
        }
        self.expect(
            TokenKind::LParen,
            "expected '(' after export on the same line",
        )?;
        self.newlines();
        let target = self.name("expected export target c")?;
        self.newlines();
        self.expect(TokenKind::Comma, "expected ',' after export target")?;
        self.newlines();
        let token = self.current();
        let TokenKind::String(symbol) = token.kind else {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "expected a string for the C export name",
            ));
        };
        self.cursor += 1;
        self.newlines();
        self.expect(TokenKind::RParen, "expected ')' after the export name")?;
        self.expect(
            TokenKind::Newline,
            "expected a line break between @export and pub fn",
        )?;
        self.newlines();
        self.expect(
            TokenKind::Pub,
            "expected pub fn; only public functions are supported",
        )?;
        self.expect(TokenKind::Fn, "expected fn on the same line as pub")?;
        let name = self.name("expected a function name on the same line as fn")?;
        if name.value == "_" || !matches!(name.value.as_bytes()[0], b'a'..=b'z' | b'_') {
            return Err(Diagnostic::new(
                DiagnosticKind::Syntax,
                name.span,
                "function name must be a lowercase name, not '_'",
            ));
        }
        self.expect(
            TokenKind::LParen,
            "expected '(' on the same line as the function name",
        )?;
        self.newlines();
        if self.current().kind != TokenKind::RParen {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "only functions without parameters are supported",
            ));
        }
        self.cursor += 1;
        // A line starting with -> continues the signature. Other signature
        // tokens cannot be arbitrarily separated by statement-ending newlines.
        self.newlines();
        self.expect(TokenKind::Arrow, "expected '-> Int' after the parameters")?;
        let return_type = self.name("expected return type Int on the same line as '->'")?;
        self.expect(
            TokenKind::LBrace,
            "expected '{' on the same line as the return type; effects are unsupported",
        )?;
        self.newlines();
        let value_token = self.current();
        let TokenKind::Integer(value) = value_token.kind else {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "function body must contain one nonnegative decimal Int literal",
            ));
        };
        self.cursor += 1;
        self.newlines();
        self.expect(
            TokenKind::RBrace,
            "expected '}' after the single integer literal",
        )?;
        self.newlines();
        self.expect(
            TokenKind::Eof,
            "expected end of input; only one function is supported",
        )?;
        Ok(Module {
            function: Function {
                export: Export {
                    target,
                    symbol: Text {
                        value: symbol,
                        span: token.span,
                    },
                },
                name,
                return_type,
                value: Text {
                    value,
                    span: value_token.span,
                },
            },
        })
    }

    fn current(&self) -> Token<'a> {
        self.tokens[self.cursor]
    }

    fn newlines(&mut self) {
        while self.current().kind == TokenKind::Newline {
            self.cursor += 1;
        }
    }

    fn expect(&mut self, expected: TokenKind<'_>, message: &str) -> Result<(), Diagnostic> {
        if self.current().kind != expected {
            return Err(self.error(DiagnosticKind::Syntax, message));
        }
        self.cursor += 1;
        Ok(())
    }

    fn name(&mut self, message: &str) -> Result<Text<'a>, Diagnostic> {
        let token = self.current();
        let TokenKind::Name(value) = token.kind else {
            return Err(self.error(DiagnosticKind::Syntax, message));
        };
        self.cursor += 1;
        Ok(Text {
            value,
            span: token.span,
        })
    }

    fn error(&self, kind: DiagnosticKind, message: &str) -> Diagnostic {
        Diagnostic::new(kind, self.current().span, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::lex;

    #[test]
    fn keeps_function_and_export_names_distinct() {
        let source = "@export(c, \"read_value\")\npub fn value() -> Int { 007 }";
        let tokens = lex(source).unwrap();
        let module = parse(&tokens).unwrap();
        assert_eq!(module.function.name.value, "value");
        assert_eq!(module.function.export.symbol.value, "read_value");
        assert_eq!(module.function.value.value, "007");
        assert_eq!(
            &source[module.function.name.span.start..module.function.name.span.end],
            "value"
        );
    }

    #[test]
    fn accepts_supported_layout_and_crlf() {
        let source = "\r\n@export(\r\n\tc,\r\n \"answer\"\r\n)\r\n\r\npub fn answer(\r\n)\r\n -> Int {\r\n42\r\n}\r\n";
        let tokens = lex(source).unwrap();
        assert_eq!(parse(&tokens).unwrap().function.value.value, "42");
    }

    #[test]
    fn rejects_incomplete_or_extra_input_without_panicking() {
        let source = include_str!("../../../../examples/minimal/main.emel");
        for end in 0..source.len() - 2 {
            let prefix = &source[..end];
            if let Ok(tokens) = lex(prefix) {
                assert!(parse(&tokens).is_err(), "accepted prefix {prefix:?}");
            }
        }
        for suffix in [
            " 9",
            "\npub fn extra() -> Int { 1 }",
            "\n@export(c, \"extra\")\npub fn extra() -> Int { 1 }",
        ] {
            let source = format!("{source}{suffix}");
            assert!(parse(&lex(&source).unwrap()).is_err());
        }
    }

    #[test]
    fn rejects_unsupported_bodies_and_invalid_names_or_newlines() {
        for declaration in [
            "pub fn f(a) -> Int { 1 }",
            "pub fn f() -> Int { one }",
            "pub fn f() -> Int { 1 2 }",
            "pub fn f() -> Int { 1\n2 }",
            "pub fn While() -> Int { 1 }",
            "pub fn _() -> Int { 1 }",
            "pub fn while() -> Int { 1 }",
            "pub\nfn f() -> Int { 1 }",
            "pub fn\nf() -> Int { 1 }",
            "pub fn f\n() -> Int { 1 }",
            "pub fn f() ->\nInt { 1 }",
            "pub fn f() -> Int\n{ 1 }",
        ] {
            let source = format!("@export(c, \"answer\")\n{declaration}");
            assert!(
                parse(&lex(&source).unwrap()).is_err(),
                "accepted {source:?}"
            );
        }
        let source = "@export(c, \"answer\") pub fn f() -> Int { 1 }";
        assert!(parse(&lex(source).unwrap()).is_err());
    }
}
