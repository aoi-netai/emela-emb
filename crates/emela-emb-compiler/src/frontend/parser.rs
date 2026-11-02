use super::{
    ast::*,
    lexer::{Token, TokenKind as K},
};
use crate::{Diagnostic, DiagnosticKind, Span};

pub(crate) fn parse(tokens: &[Token<'_>]) -> Result<Module, Diagnostic> {
    Parser {
        tokens,
        cursor: 0,
        depth: 0,
    }
    .module()
}
struct Parser<'t, 's> {
    tokens: &'t [Token<'s>],
    cursor: usize,
    depth: usize,
}
impl<'t, 's> Parser<'t, 's> {
    fn module(mut self) -> Result<Module, Diagnostic> {
        let mut imports = Vec::new();
        let mut functions = Vec::new();
        self.newlines();
        while self.current().kind != K::Eof {
            if self.word("import") {
                if !functions.is_empty() {
                    return Err(
                        self.error(DiagnosticKind::Syntax, "imports must precede declarations")
                    );
                }
                imports.push(self.import()?);
            } else {
                functions.push(self.function()?);
            }
            if !matches!(self.current().kind, K::Newline | K::Eof) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "expected a line break after declaration",
                ));
            }
            self.newlines();
        }
        Ok(Module { imports, functions })
    }
    fn import(&mut self) -> Result<Import, Diagnostic> {
        self.advance();
        let first = self.name("expected module path after import")?;
        let mut path = first.clone();
        let mut members = None;
        while self.take(K::Dot) {
            if self.take(K::LBrace) {
                self.newlines();
                let mut names = Vec::new();
                while self.current().kind != K::RBrace {
                    names.push(self.name("expected imported declaration name")?);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(K::RBrace, "expected '}' after import members")?;
                members = Some(names);
                break;
            }
            let segment = self.name("expected module name after '.'")?;
            path.value.push('.');
            path.value.push_str(&segment.value);
            path.span.end = segment.span.end;
        }
        Ok(Import { path, members })
    }
    fn function(&mut self) -> Result<Function, Diagnostic> {
        let mut attributes = Vec::new();
        while self.take(K::At) {
            let attribute = self.name("expected attribute name")?;
            match attribute.value.as_str() {
                "export" | "external" => {
                    self.expect(K::LParen, "expected '(' after attribute")?;
                    self.newlines();
                    let target = self.name("expected external target c")?;
                    self.newlines();
                    self.expect(K::Comma, "expected ',' after target")?;
                    self.newlines();
                    let token = self.current();
                    let K::String(value) = token.kind else {
                        return Err(self.error(DiagnosticKind::Syntax, "expected C symbol string"));
                    };
                    let symbol = Text::new(value, token.span);
                    self.advance();
                    self.newlines();
                    self.expect(K::RParen, "expected ')' after symbol")?;
                    attributes.push(if attribute.value == "export" {
                        Attribute::Export { target, symbol }
                    } else {
                        Attribute::External { target, symbol }
                    });
                }
                "intrinsic" => attributes.push(Attribute::Intrinsic(attribute.span)),
                _ => {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Unsupported,
                        attribute.span,
                        format!("attribute @{} is not implemented", attribute.value),
                    ));
                }
            }
            self.expect(K::Newline, "expected a line break after attribute")?;
            self.newlines();
        }
        let public = self.take_word("pub");
        if !self.take_word("fn") {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "P1 supports function declarations; this declaration requires a later phase",
            ));
        }
        let name = self.lower_name("expected a lowercase function name")?;
        if self.current().kind == K::LBracket {
            return Err(self.error(DiagnosticKind::Unsupported, "type parameters require P2"));
        }
        self.expect(
            K::LParen,
            "expected '(' after function name on the same line",
        )?;
        self.newlines();
        let mut params = Vec::new();
        while self.current().kind != K::RParen {
            let name = self.lower_name("expected parameter name")?;
            self.newlines();
            if !self.take(K::Colon) {
                return Err(self.error(
                    DiagnosticKind::Type,
                    "top-level function parameters require a type annotation",
                ));
            }
            self.newlines();
            let ty = self.type_name()?;
            params.push(Parameter { name, ty });
            self.newlines();
            if !self.take(K::Comma) {
                break;
            }
            self.newlines();
        }
        self.expect(K::RParen, "expected ')' after parameters")?;
        self.continue_before(K::Arrow);
        let return_type = if self.take(K::Arrow) {
            Some(self.type_name()?)
        } else {
            None
        };
        self.continue_word("fails");
        self.continue_word("use");
        if self.word("fails") || self.word("use") || self.word("suspend") {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "error and effect signatures require a later phase",
            ));
        }
        let body = if self.current().kind == K::LBrace {
            Some(self.block()?)
        } else {
            None
        };
        if body.is_none()
            && !attributes
                .iter()
                .any(|a| matches!(a, Attribute::External { .. } | Attribute::Intrinsic(_)))
        {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "expected function body on the last signature line",
            ));
        }
        Ok(Function {
            name,
            public,
            params,
            return_type,
            attributes,
            body,
        })
    }
    fn type_name(&mut self) -> Result<Text, Diagnostic> {
        if self.current().kind == K::LParen {
            let start = self.current().span.start;
            self.advance();
            self.newlines();
            self.expect(K::RParen, "only scalar and Unit types are supported in P1")?;
            return Ok(Text::new("Unit", Span::new(start, self.previous_end())));
        }
        let ty = self.name("expected type name on the same line")?;
        if matches!(self.current().kind, K::LBracket | K::Dot) {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "compound and qualified types require P2",
            ));
        }
        Ok(ty)
    }
    fn block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.current().span.start;
        self.expect(K::LBrace, "expected '{'")?;
        self.newlines();
        let mut statements = Vec::new();
        while !matches!(self.current().kind, K::RBrace | K::Eof) {
            let binding = matches!(self.current().kind, K::Name(_))
                && matches!(self.peek(1).kind, K::Eq | K::Colon);
            if binding {
                let name = self.lower_name("bindings require a lowercase name or '_'")?;
                let annotation = if self.take(K::Colon) {
                    Some(self.type_name()?)
                } else {
                    None
                };
                self.expect(K::Eq, "expected '=' in binding")?;
                let value = self.expr(0, false)?;
                statements.push(Statement::Bind {
                    name,
                    annotation,
                    value,
                });
            } else {
                statements.push(Statement::Expr(self.expr(0, false)?));
            }
            if !matches!(self.current().kind, K::Newline | K::RBrace) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "expected a line break between expressions",
                ));
            }
            self.newlines();
        }
        self.expect(K::RBrace, "expected '}' to close block")?;
        Ok(Block {
            statements,
            span: Span::new(start, self.previous_end()),
        })
    }
    fn expr(&mut self, min: u8, multiline: bool) -> Result<Expr, Diagnostic> {
        if self.depth >= 256 {
            return Err(self.error(DiagnosticKind::Syntax, "expression nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.expr_inner(min, multiline);
        self.depth -= 1;
        result
    }
    fn expr_inner(&mut self, min: u8, multiline: bool) -> Result<Expr, Diagnostic> {
        if multiline {
            self.newlines();
        }
        let token = self.current();
        let mut left = match token.kind {
            K::Integer(raw) => {
                self.advance();
                Expr {
                    kind: ExprKind::Integer(raw.into()),
                    span: token.span,
                }
            }
            K::Float(raw) => {
                self.advance();
                Expr {
                    kind: ExprKind::Float(raw.into()),
                    span: token.span,
                }
            }
            K::String(_) => {
                self.advance();
                Expr {
                    kind: ExprKind::String,
                    span: token.span,
                }
            }
            K::Minus | K::Bang => {
                self.advance();
                let value = self.expr(8, multiline)?;
                Expr {
                    span: Span::new(token.span.start, value.span.end),
                    kind: ExprKind::Unary {
                        op: if token.kind == K::Minus {
                            Unary::Neg
                        } else {
                            Unary::Not
                        },
                        value: Box::new(value),
                    },
                }
            }
            K::Name("True" | "False") => {
                self.advance();
                Expr {
                    kind: ExprKind::Bool(token.kind == K::Name("True")),
                    span: token.span,
                }
            }
            K::Name("if") => self.if_expr(multiline)?,
            K::Name("assert") => {
                self.advance();
                let value = self.expr(0, multiline)?;
                Expr {
                    span: Span::new(token.span.start, value.span.end),
                    kind: ExprKind::Assert(Box::new(value)),
                }
            }
            K::Name(name) if !reserved(name) => {
                self.advance();
                Expr {
                    kind: ExprKind::Name(vec![Text::new(name, token.span)]),
                    span: token.span,
                }
            }
            K::LParen => {
                self.advance();
                self.newlines();
                if self.take(K::RParen) {
                    Expr {
                        kind: ExprKind::Unit,
                        span: Span::new(token.span.start, self.previous_end()),
                    }
                } else {
                    let mut expression = self.expr(0, true)?;
                    self.newlines();
                    if self.current().kind == K::Comma {
                        return Err(self.error(DiagnosticKind::Unsupported, "tuples require P2"));
                    }
                    self.expect(K::RParen, "expected ')' after expression")?;
                    expression.span = Span::new(token.span.start, self.previous_end());
                    expression
                }
            }
            K::LBrace => {
                let block = self.block()?;
                Expr {
                    span: block.span,
                    kind: ExprKind::Block(block),
                }
            }
            K::Name(_) | K::LBracket => {
                return Err(self.error(
                    DiagnosticKind::Unsupported,
                    "this expression requires a later phase",
                ));
            }
            _ => return Err(self.error(DiagnosticKind::Syntax, "expected expression")),
        };
        let mut comparison_seen = false;
        loop {
            if multiline {
                self.newlines();
            } else if self.current().kind == K::Newline {
                let mut next = self.cursor;
                while self.tokens[next].kind == K::Newline {
                    next += 1;
                }
                if matches!(
                    self.tokens[next].kind,
                    K::Dot
                        | K::Plus
                        | K::Star
                        | K::Slash
                        | K::Percent
                        | K::EqEq
                        | K::Ne
                        | K::Lt
                        | K::Le
                        | K::Gt
                        | K::Ge
                        | K::AndAnd
                        | K::OrOr
                        | K::Pipe
                ) {
                    self.cursor = next;
                }
            }
            if self.current().kind == K::LParen && min <= 9 {
                self.advance();
                self.newlines();
                let mut args = Vec::new();
                while self.current().kind != K::RParen {
                    if matches!(self.current().kind, K::Name(_)) && self.peek(1).kind == K::Colon {
                        return Err(
                            self.error(DiagnosticKind::Unsupported, "named arguments require P2")
                        );
                    }
                    args.push(self.expr(0, true)?);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(K::RParen, "expected ')' after arguments")?;
                let start = left.span.start;
                left = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(left),
                        args,
                    },
                    span: Span::new(start, self.previous_end()),
                };
                continue;
            }
            if self.current().kind == K::Dot && min <= 9 {
                self.advance();
                let segment = self.name("expected name after '.'")?;
                let ExprKind::Name(path) = &mut left.kind else {
                    return Err(self.error(DiagnosticKind::Unsupported, "field access requires P2"));
                };
                left.span.end = segment.span.end;
                path.push(segment);
                continue;
            }
            let Some((op, precedence)) = binary(self.current().kind) else {
                break;
            };
            if precedence < min {
                break;
            }
            if op.comparison() && comparison_seen {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "comparison operators cannot be chained",
                ));
            }
            comparison_seen |= op.comparison();
            self.advance();
            let right = self.expr(precedence + 1, multiline)?;
            let span = Span::new(left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }
    fn if_expr(&mut self, multiline: bool) -> Result<Expr, Diagnostic> {
        let start = self.current().span.start;
        self.advance();
        let condition = self.expr(0, multiline)?;
        let then = self.block()?;
        self.continue_word("else");
        let else_ = if self.take_word("else") {
            if self.word("if") {
                let nested = self.if_expr(multiline)?;
                let span = nested.span;
                Some(Block {
                    statements: vec![Statement::Expr(nested)],
                    span,
                })
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        let end = else_.as_ref().map_or(then.span.end, |b| b.span.end);
        Ok(Expr {
            kind: ExprKind::If {
                condition: Box::new(condition),
                then,
                else_,
            },
            span: Span::new(start, end),
        })
    }
    fn current(&self) -> Token<'s> {
        self.tokens[self.cursor]
    }
    fn peek(&self, distance: usize) -> Token<'s> {
        self.tokens[(self.cursor + distance).min(self.tokens.len() - 1)]
    }
    fn advance(&mut self) {
        if self.current().kind != K::Eof {
            self.cursor += 1;
        }
    }
    fn previous_end(&self) -> usize {
        self.tokens[self.cursor.saturating_sub(1)].span.end
    }
    fn newlines(&mut self) {
        while self.take(K::Newline) {}
    }
    fn continue_before(&mut self, expected: K<'_>) {
        let mut next = self.cursor;
        while self.tokens[next].kind == K::Newline {
            next += 1;
        }
        if self.tokens[next].kind == expected {
            self.cursor = next;
        }
    }
    fn continue_word(&mut self, word: &str) {
        self.continue_before(K::Name(word));
    }
    fn take(&mut self, expected: K<'_>) -> bool {
        if self.current().kind == expected {
            self.advance();
            true
        } else {
            false
        }
    }
    fn word(&self, word: &str) -> bool {
        self.current().kind == K::Name(word)
    }
    fn take_word(&mut self, word: &str) -> bool {
        self.take(K::Name(word))
    }
    fn expect(&mut self, expected: K<'_>, message: &str) -> Result<(), Diagnostic> {
        if self.take(expected) {
            Ok(())
        } else {
            Err(self.error(DiagnosticKind::Syntax, message))
        }
    }
    fn name(&mut self, message: &str) -> Result<Text, Diagnostic> {
        let token = self.current();
        let K::Name(name) = token.kind else {
            return Err(self.error(DiagnosticKind::Syntax, message));
        };
        self.advance();
        Ok(Text::new(name, token.span))
    }
    fn lower_name(&mut self, message: &str) -> Result<Text, Diagnostic> {
        let name = self.name(message)?;
        if reserved(&name.value)
            || !name
                .value
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_lowercase() || *b == b'_')
        {
            return Err(Diagnostic::new(DiagnosticKind::Syntax, name.span, message));
        }
        Ok(name)
    }
    fn error(&self, kind: DiagnosticKind, message: &str) -> Diagnostic {
        Diagnostic::new(kind, self.current().span, message)
    }
}
fn reserved(name: &str) -> bool {
    matches!(
        name,
        "pub"
            | "fn"
            | "type"
            | "enum"
            | "error"
            | "opaque"
            | "const"
            | "trait"
            | "impl"
            | "effect"
            | "handler"
            | "layer"
            | "import"
            | "suspend"
            | "if"
            | "else"
            | "match"
            | "fail"
            | "escape"
            | "use"
            | "with"
            | "assert"
            | "self"
            | "Self"
            | "as"
            | "var"
    )
}
fn binary(kind: K<'_>) -> Option<(Binary, u8)> {
    Some(match kind {
        K::Pipe => (Binary::Pipe, 1),
        K::OrOr => (Binary::Or, 2),
        K::AndAnd => (Binary::And, 3),
        K::EqEq => (Binary::Eq, 4),
        K::Ne => (Binary::Ne, 4),
        K::Lt => (Binary::Lt, 4),
        K::Le => (Binary::Le, 4),
        K::Gt => (Binary::Gt, 4),
        K::Ge => (Binary::Ge, 4),
        K::Plus => (Binary::Add, 5),
        K::Minus => (Binary::Sub, 5),
        K::Star => (Binary::Mul, 6),
        K::Slash => (Binary::Div, 6),
        K::Percent => (Binary::Rem, 6),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::lex;
    #[test]
    fn parses_real_bindings_multiple_functions_and_continuation() {
        let source = "import Math.{add}\nfn twice(n: Int) -> Int { n\n + n }\n@export(c, \"answer\")\npub fn answer() -> Int { x = add(20, 22)\n assert x == 42\n if True { twice(x) } else { 0 } }";
        let module = parse(&lex(source).unwrap()).unwrap();
        assert_eq!(module.functions.len(), 2);
        assert_eq!(module.imports[0].path.value, "Math");
    }
    #[test]
    fn statement_break_and_operator_placement_match_language() {
        for source in [
            "fn f() { 1 +\n2 }",
            "fn f() { 1 < 2 < 3 }",
            "fn f() { 1 2 }",
            "fn f() { let x = 1 }",
            "fn f() { loop {} }",
        ] {
            assert!(parse(&lex(source).unwrap()).is_err(), "{source}");
        }
        assert!(parse(&lex("fn f() { (1 +\n2) }").unwrap()).is_ok());
        assert!(parse(&lex("fn f() { 1\n-2 }").unwrap()).is_ok());
    }
}
