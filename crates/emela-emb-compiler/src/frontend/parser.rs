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
        method_context: false,
    }
    .module()
}
struct Parser<'t, 's> {
    tokens: &'t [Token<'s>],
    cursor: usize,
    depth: usize,
    method_context: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FunctionMode {
    Top,
    Trait,
    Impl,
}
impl<'t, 's> Parser<'t, 's> {
    fn module(mut self) -> Result<Module, Diagnostic> {
        let mut imports = Vec::new();
        let mut functions = Vec::new();
        let mut data = Vec::new();
        let mut consts = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        self.newlines();
        while self.current().kind != K::Eof {
            if self.word("import") {
                if !functions.is_empty()
                    || !data.is_empty()
                    || !consts.is_empty()
                    || !traits.is_empty()
                    || !impls.is_empty()
                {
                    return Err(
                        self.error(DiagnosticKind::Syntax, "imports must precede declarations")
                    );
                }
                imports.push(self.import()?);
            } else {
                let intrinsic = self.intrinsic_data_attribute()?;
                let declaration = if self.word("pub") {
                    self.peek(1).kind
                } else {
                    self.current().kind
                };
                if declaration == K::Name("const") {
                    consts.push(self.constant()?);
                } else if declaration == K::Name("trait") {
                    traits.push(self.trait_declaration()?);
                } else if declaration == K::Name("impl") {
                    impls.push(self.implementation()?);
                } else if matches!(declaration, K::Name("type" | "enum" | "error" | "opaque")) {
                    let mut declaration = self.data()?;
                    declaration.intrinsic = intrinsic;
                    data.push(declaration);
                } else {
                    functions.push(self.function()?);
                }
            }
            if !matches!(self.current().kind, K::Newline | K::Eof) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "expected a line break after declaration",
                ));
            }
            self.newlines();
        }
        Ok(Module {
            imports,
            functions,
            data,
            consts,
            traits,
            impls,
        })
    }
    fn intrinsic_data_attribute(&mut self) -> Result<Option<Span>, Diagnostic> {
        let checkpoint = self.cursor;
        if !self.take(K::At) || !self.word("intrinsic") {
            self.cursor = checkpoint;
            return Ok(None);
        }
        let span = self.current().span;
        self.advance();
        self.expect(K::Newline, "expected a line break after attribute")?;
        self.newlines();
        let declaration = if self.word("pub") {
            self.peek(1).kind
        } else {
            self.current().kind
        };
        if matches!(declaration, K::Name("type" | "enum" | "error" | "opaque")) {
            Ok(Some(span))
        } else {
            self.cursor = checkpoint;
            Ok(None)
        }
    }
    fn trait_declaration(&mut self) -> Result<TraitDeclaration, Diagnostic> {
        let public = self.take_word("pub");
        self.expect(K::Name("trait"), "expected trait declaration")?;
        let name = self.name("expected trait name")?;
        let mut supers = Vec::new();
        if self.take(K::Colon) {
            loop {
                self.newlines();
                supers.push(self.type_name()?);
                if !self.take(K::Plus) {
                    break;
                }
            }
        }
        self.expect(K::LBrace, "expected '{' after trait signature")?;
        self.newlines();
        let mut methods = Vec::new();
        while !matches!(self.current().kind, K::RBrace | K::Eof) {
            methods.push(self.function_mode(FunctionMode::Trait)?);
            if !matches!(self.current().kind, K::Newline | K::RBrace) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "expected a line break after trait method",
                ));
            }
            self.newlines();
        }
        self.expect(K::RBrace, "expected '}' after trait methods")?;
        Ok(TraitDeclaration {
            name,
            public,
            supers,
            methods,
        })
    }
    fn implementation(&mut self) -> Result<Implementation, Diagnostic> {
        let start = self.current().span.start;
        self.expect(K::Name("impl"), "expected impl declaration")?;
        let mut target = self.name("expected implementation target type")?;
        while self.take(K::Dot) {
            let segment = self.name("expected qualified implementation type")?;
            target.value.push('.');
            target.value.push_str(&segment.value);
        }
        let params = self.type_parameters()?;
        if !params.is_empty() {
            target.value.push('[');
            target.value.push_str(
                &params
                    .iter()
                    .map(|param| param.name.value.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            target.value.push(']');
        }
        target.span.end = self.previous_end();
        self.expect(K::Colon, "expected ':' before implemented trait")?;
        let trait_name = self.type_name()?;
        self.expect(K::LBrace, "expected '{' after implemented trait")?;
        self.newlines();
        let mut methods = Vec::new();
        while !matches!(self.current().kind, K::RBrace | K::Eof) {
            methods.push(self.function_mode(FunctionMode::Impl)?);
            if !matches!(self.current().kind, K::Newline | K::RBrace) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "expected a line break after impl method",
                ));
            }
            self.newlines();
        }
        self.expect(K::RBrace, "expected '}' after impl methods")?;
        Ok(Implementation {
            target,
            params,
            trait_name,
            methods,
            span: Span::new(start, self.previous_end()),
        })
    }
    fn constant(&mut self) -> Result<ConstDeclaration, Diagnostic> {
        let public = self.take_word("pub");
        self.expect(K::Name("const"), "expected const declaration")?;
        let name = self.name("expected constant name")?;
        if !name.value.as_bytes()[0].is_ascii_uppercase()
            || !name
                .value
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(Diagnostic::new(
                DiagnosticKind::Syntax,
                name.span,
                "constant names require UPPER_SNAKE_CASE",
            ));
        }
        let annotation = if self.take(K::Colon) {
            self.newlines();
            Some(self.type_name()?)
        } else {
            None
        };
        self.expect(K::Eq, "expected '=' after constant name or annotation")?;
        let value = self.expr(0, false)?;
        Ok(ConstDeclaration {
            name,
            public,
            annotation,
            value,
        })
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
        self.function_mode(FunctionMode::Top)
    }
    fn function_mode(&mut self, mode: FunctionMode) -> Result<Function, Diagnostic> {
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
                    let decoded = parse_string(value, token.span, false, false)?;
                    let ExprKind::String(value) = decoded.kind else {
                        return Err(Diagnostic::new(
                            DiagnosticKind::Syntax,
                            token.span,
                            "C boundary symbols cannot contain interpolation",
                        ));
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
        if mode != FunctionMode::Top && self.word("pub") {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "trait and impl methods do not use pub",
            ));
        }
        let public = self.take_word("pub");
        let suspends = self.take_word("suspend");
        if suspends
            && (mode != FunctionMode::Top
                || !attributes
                    .iter()
                    .any(|a| matches!(a, Attribute::External { .. })))
        {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "suspend function declarations require @external",
            ));
        }
        if !self.take_word("fn") {
            return Err(self.error(
                DiagnosticKind::Unsupported,
                "P1 supports function declarations; this declaration requires a later phase",
            ));
        }
        let name = self.lower_name("expected a lowercase function name")?;
        let type_params = self.type_parameters()?;
        self.expect(
            K::LParen,
            "expected '(' after function name on the same line",
        )?;
        self.newlines();
        let mut params = Vec::new();
        while self.current().kind != K::RParen {
            let name = if mode != FunctionMode::Top && self.word("self") {
                self.name("expected receiver")?
            } else {
                self.lower_name("expected parameter name")?
            };
            self.newlines();
            let ty = if self.take(K::Colon) {
                self.newlines();
                self.type_name()?
            } else if mode != FunctionMode::Top && name.value == "self" {
                Text::new("Self", name.span)
            } else if mode == FunctionMode::Impl {
                Text::new("", name.span)
            } else {
                return Err(self.error(
                    DiagnosticKind::Type,
                    "top-level and trait method parameters require a type annotation",
                ));
            };
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
        let fails = self.failure_clause()?;
        let effects = self.effect_clause()?;
        if self.word("fails") || self.word("use") {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "signature clauses must appear once in the order 'fails', then 'use'",
            ));
        }
        let previous_context = self.method_context;
        self.method_context = mode != FunctionMode::Top;
        let body = if self.current().kind == K::LBrace {
            self.block().map(Some)
        } else {
            Ok(None)
        };
        self.method_context = previous_context;
        let body = body?;
        if body.is_none()
            && mode != FunctionMode::Trait
            && (mode == FunctionMode::Impl
                || !attributes
                    .iter()
                    .any(|a| matches!(a, Attribute::External { .. } | Attribute::Intrinsic(_))))
        {
            return Err(self.error(
                DiagnosticKind::Syntax,
                "expected function body on the last signature line",
            ));
        }
        Ok(Function {
            name,
            public,
            constant: false,
            type_params,
            params,
            return_type,
            fails,
            effects,
            suspends,
            attributes,
            body,
        })
    }
    fn failure_clause(&mut self) -> Result<Option<Vec<Text>>, Diagnostic> {
        self.continue_word("fails");
        if !self.take_word("fails") {
            return Ok(None);
        }
        self.newlines();
        if self.take(K::LBrace) {
            self.newlines();
            self.expect(K::RBrace, "an empty failure set is written 'fails {}'")?;
            return Ok(Some(Vec::new()));
        }
        let mut errors = vec![self.type_name()?];
        while self.take(K::Bar) {
            self.newlines();
            errors.push(self.type_name()?);
        }
        Ok(Some(errors))
    }
    fn effect_clause(&mut self) -> Result<Option<Vec<Text>>, Diagnostic> {
        self.continue_word("use");
        if !self.take_word("use") {
            return Ok(None);
        }
        self.newlines();
        if !self.take(K::LBrace) {
            return Ok(Some(vec![self.type_name()?]));
        }
        self.newlines();
        let mut effects = Vec::new();
        while self.current().kind != K::RBrace {
            effects.push(self.type_name()?);
            self.newlines();
            if !self.take(K::Comma) {
                break;
            }
            self.newlines();
        }
        self.expect(K::RBrace, "expected '}' after effect set")?;
        Ok(Some(effects))
    }
    fn lambda(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.current().span.start;
        self.expect(K::Name("fn"), "expected anonymous function")?;
        self.expect(
            K::LParen,
            "anonymous functions are written 'fn(parameters) { body }'",
        )?;
        self.newlines();
        let mut params = Vec::new();
        while self.current().kind != K::RParen {
            let pattern = self.pattern()?;
            self.newlines();
            let ty = if self.take(K::Colon) {
                self.newlines();
                Some(self.type_name()?)
            } else {
                None
            };
            params.push(LambdaParameter { pattern, ty });
            self.newlines();
            if !self.take(K::Comma) {
                break;
            }
            self.newlines();
        }
        self.expect(K::RParen, "expected ')' after anonymous parameters")?;
        let body = self.block()?;
        Ok(Expr {
            kind: ExprKind::Lambda {
                params,
                return_type: None,
                fails: None,
                effects: None,
                body,
            },
            span: Span::new(start, self.previous_end()),
        })
    }
    fn type_parameters(&mut self) -> Result<Vec<TypeParameter>, Diagnostic> {
        let mut params = Vec::new();
        if !self.take(K::LBracket) {
            return Ok(params);
        }
        self.newlines();
        while self.current().kind != K::RBracket {
            let name = self.name("expected type parameter name")?;
            if reserved(&name.value) {
                return Err(Diagnostic::new(
                    DiagnosticKind::Syntax,
                    name.span,
                    "reserved word cannot name a type parameter",
                ));
            }
            self.newlines();
            let mut bounds = Vec::new();
            if self.take(K::Colon) {
                loop {
                    self.newlines();
                    bounds.push(self.type_name()?);
                    self.newlines();
                    if !self.take(K::Plus) {
                        break;
                    }
                }
            }
            params.push(TypeParameter { name, bounds });
            self.newlines();
            if !self.take(K::Comma) {
                break;
            }
            self.newlines();
        }
        self.expect(K::RBracket, "expected ']' after type parameters")?;
        Ok(params)
    }
    fn type_name(&mut self) -> Result<Text, Diagnostic> {
        if self.depth >= 64 {
            return Err(self.error(DiagnosticKind::Syntax, "type nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.type_name_inner();
        self.depth -= 1;
        result
    }
    fn type_name_inner(&mut self) -> Result<Text, Diagnostic> {
        self.newlines();
        let start = self.current().span.start;
        let mut value;
        if self.take_word("fn") {
            self.expect(K::LParen, "expected '(' after fn in function type")?;
            self.newlines();
            let mut params = Vec::new();
            while self.current().kind != K::RParen {
                params.push(self.type_name()?.value);
                self.newlines();
                if !self.take(K::Comma) {
                    break;
                }
                self.newlines();
            }
            self.expect(K::RParen, "expected ')' after function parameter types")?;
            self.expect(K::Arrow, "function types require '->' and a return type")?;
            let ret = self.type_name()?.value;
            value = format!("fn({})->{ret}", params.join(","));
            if let Some(errors) = self.failure_clause()? {
                value.push_str(" fails ");
                value.push_str(&if errors.is_empty() {
                    "{}".into()
                } else {
                    errors
                        .iter()
                        .map(|e| e.value.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                });
            }
            if let Some(effects) = self.effect_clause()? {
                value.push_str(" use{");
                value.push_str(
                    &effects
                        .iter()
                        .map(|e| e.value.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                value.push('}');
            }
        } else if self.take(K::LParen) {
            self.newlines();
            let mut elements = Vec::new();
            while self.current().kind != K::RParen {
                elements.push(self.type_name()?.value);
                self.newlines();
                if !self.take(K::Comma) {
                    break;
                }
                self.newlines();
            }
            self.expect(K::RParen, "expected ')' after tuple type")?;
            value = if elements.is_empty() {
                "Unit".into()
            } else if elements.len() == 1 {
                let element = elements.remove(0);
                if element.starts_with("fn(") {
                    format!("({element})")
                } else {
                    element
                }
            } else {
                format!("({})", elements.join(","))
            };
        } else {
            value = self.name("expected type name")?.value;
            while self.take(K::Dot) {
                value.push('.');
                value.push_str(&self.name("expected qualified type name")?.value);
            }
            if self.take(K::LBracket) {
                self.newlines();
                let mut args = Vec::new();
                while self.current().kind != K::RBracket {
                    args.push(self.type_argument()?.value);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(K::RBracket, "expected ']' after type arguments")?;
                value.push('[');
                value.push_str(&args.join(","));
                value.push(']');
            }
        }
        Ok(Text::new(value, Span::new(start, self.previous_end())))
    }
    fn type_argument(&mut self) -> Result<Text, Diagnostic> {
        let start = self.current().span.start;
        if self.take(K::LBrace) {
            self.newlines();
            let mut items = Vec::new();
            while self.current().kind != K::RBrace {
                items.push(self.type_name()?.value);
                self.newlines();
                if !self.take(K::Comma) {
                    break;
                }
                self.newlines();
            }
            self.expect(K::RBrace, "expected '}' after set type argument")?;
            return Ok(Text::new(
                format!("{{{}}}", items.join(",")),
                Span::new(start, self.previous_end()),
            ));
        }
        let first = self.type_name()?;
        let mut items = vec![first.value];
        while self.take(K::Bar) {
            self.newlines();
            items.push(self.type_name()?.value);
        }
        Ok(Text::new(
            items.join("|"),
            Span::new(start, self.previous_end()),
        ))
    }
    fn fields(&mut self) -> Result<Vec<Field>, Diagnostic> {
        let mut fields = Vec::new();
        if !self.take(K::LParen) {
            return Ok(fields);
        }
        self.newlines();
        let mut named = None;
        while self.current().kind != K::RParen {
            let label =
                if matches!(self.current().kind, K::Name(_)) && self.peek(1).kind == K::Colon {
                    let label = self.lower_name("expected field name")?;
                    self.advance();
                    self.newlines();
                    Some(label)
                } else {
                    None
                };
            if named.is_some_and(|n| n != label.is_some()) {
                return Err(self.error(
                    DiagnosticKind::Syntax,
                    "fields must be all named or all positional",
                ));
            }
            named = Some(label.is_some());
            let ty = self.type_name()?;
            fields.push(Field { name: label, ty });
            self.newlines();
            if !self.take(K::Comma) {
                break;
            }
            self.newlines();
        }
        self.expect(K::RParen, "expected ')' after fields")?;
        Ok(fields)
    }
    fn data(&mut self) -> Result<Data, Diagnostic> {
        let public = self.take_word("pub");
        let opaque = self.take_word("opaque");
        let error = self.word("error");
        let enumeration = self.word("enum");
        if !self.take_word("type") && !self.take_word("error") && !self.take_word("enum") {
            return Err(self.error(DiagnosticKind::Syntax, "expected type or enum after opaque"));
        }
        let name = self.name("expected data type name")?;
        let params = self.type_parameters()?;
        let variants = if enumeration {
            self.expect(K::LBrace, "expected '{' in enum declaration")?;
            self.newlines();
            let mut variants = Vec::new();
            while self.current().kind != K::RBrace {
                let name = self.name("expected variant name")?;
                let fields = self.fields()?;
                variants.push(Variant { name, fields });
                if self.take(K::Comma) || self.current().kind == K::Newline {
                    self.newlines();
                } else {
                    break;
                }
            }
            self.expect(K::RBrace, "expected '}' after enum variants")?;
            variants
        } else {
            vec![Variant {
                name: name.clone(),
                fields: self.fields()?,
            }]
        };
        self.continue_word("derive");
        let mut derives = Vec::new();
        if self.take_word("derive") {
            loop {
                derives.push(self.name("expected derived trait")?);
                if !self.take(K::Comma) {
                    break;
                }
            }
        }
        Ok(Data {
            name,
            intrinsic: None,
            public,
            opaque,
            error,
            params,
            variants,
            derives,
        })
    }
    fn block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.current().span.start;
        self.expect(K::LBrace, "expected '{'")?;
        self.newlines();
        let mut statements = Vec::new();
        while !matches!(self.current().kind, K::RBrace | K::Eof) {
            let checkpoint = self.cursor;
            let candidate = self.pattern();
            let annotation = if candidate.is_ok() && self.take(K::Colon) {
                Some(self.type_name()?)
            } else {
                None
            };
            if let Some(pattern) = candidate.ok().filter(|_| self.take(K::Eq)) {
                let value = self.expr(0, false)?;
                statements.push(Statement::Bind {
                    pattern,
                    annotation,
                    value,
                });
            } else {
                self.cursor = checkpoint;
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
            K::String(raw) | K::MultilineString(raw) => {
                self.advance();
                parse_string(
                    raw,
                    token.span,
                    matches!(token.kind, K::MultilineString(_)),
                    self.method_context,
                )?
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
            K::Name("fn") => self.lambda()?,
            K::Name("assert") => {
                self.advance();
                let value = self.expr(0, multiline)?;
                Expr {
                    span: Span::new(token.span.start, value.span.end),
                    kind: ExprKind::Assert(Box::new(value)),
                }
            }
            K::Name("fail") if min == 0 => {
                self.advance();
                let value = self.expr(0, multiline)?;
                Expr {
                    span: Span::new(token.span.start, value.span.end),
                    kind: ExprKind::Fail(Box::new(value)),
                }
            }
            K::Name(name) if !reserved(name) || (name == "self" && self.method_context) => {
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
                    if self.take(K::Comma) {
                        let mut elements = vec![expression];
                        self.newlines();
                        while self.current().kind != K::RParen {
                            elements.push(self.expr(0, true)?);
                            self.newlines();
                            if !self.take(K::Comma) {
                                break;
                            }
                            self.newlines();
                        }
                        if elements.len() == 1 {
                            return Err(self.error(
                                DiagnosticKind::Syntax,
                                "a tuple needs at least two elements",
                            ));
                        }
                        self.expect(K::RParen, "expected ')' after tuple")?;
                        Expr {
                            kind: ExprKind::Tuple(elements),
                            span: Span::new(token.span.start, self.previous_end()),
                        }
                    } else {
                        self.expect(K::RParen, "expected ')' after expression")?;
                        expression.span = Span::new(token.span.start, self.previous_end());
                        expression
                    }
                }
            }
            K::LBracket => {
                self.advance();
                self.newlines();
                let mut elements = Vec::new();
                let mut tail = None;
                while self.current().kind != K::RBracket {
                    if self.current().kind == K::Dot && self.peek(1).kind == K::Dot {
                        self.advance();
                        self.advance();
                        self.newlines();
                        tail = Some(Box::new(self.expr(0, true)?));
                        self.newlines();
                        self.take(K::Comma);
                        self.newlines();
                        break;
                    }
                    elements.push(self.expr(0, true)?);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(
                    K::RBracket,
                    "expected ']' after list; a spread must be last",
                )?;
                Expr {
                    kind: ExprKind::List { elements, tail },
                    span: Span::new(token.span.start, self.previous_end()),
                }
            }
            K::LBrace => {
                let block = self.block()?;
                Expr {
                    span: block.span,
                    kind: ExprKind::Block(block),
                }
            }
            K::Name(_) => {
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
                    K::Name("match")
                        | K::Name("escape")
                        | K::Dot
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
            if (self.word("match") || self.word("escape")) && min == 0 {
                let start = left.span.start;
                let escape = self.word("escape");
                self.advance();
                self.expect(K::LBrace, "expected '{' after postfix match or escape")?;
                self.newlines();
                let mut arms = Vec::new();
                while self.current().kind != K::RBrace {
                    let pattern = self.pattern()?;
                    let guard = if self.take_word("if") {
                        Some(self.expr(0, false)?)
                    } else {
                        None
                    };
                    self.expect(K::Arrow, "expected '->' after arm pattern")?;
                    let value = self.expr(0, false)?;
                    arms.push(Arm {
                        pattern,
                        guard,
                        value,
                    });
                    if self.take(K::Comma) || self.current().kind == K::Newline {
                        self.newlines();
                    } else {
                        break;
                    }
                }
                self.expect(K::RBrace, "expected '}' after match or escape arms")?;
                left = Expr {
                    kind: if escape {
                        ExprKind::Escape {
                            value: Box::new(left),
                            arms,
                        }
                    } else {
                        ExprKind::Match {
                            value: Box::new(left),
                            arms,
                        }
                    },
                    span: Span::new(start, self.previous_end()),
                };
                continue;
            }
            if self.current().kind == K::LParen && min <= 9 {
                self.advance();
                self.newlines();
                let mut args = Vec::new();
                while self.current().kind != K::RParen {
                    let label = if matches!(self.current().kind, K::Name(_))
                        && self.peek(1).kind == K::Colon
                    {
                        let name = self.lower_name("expected argument label")?;
                        self.advance();
                        self.newlines();
                        Some(name)
                    } else {
                        None
                    };
                    let value = if matches!(self.current().kind, K::Comma | K::RParen) {
                        let Some(label) = &label else {
                            return Err(self.error(DiagnosticKind::Syntax, "expected argument"));
                        };
                        Expr {
                            kind: ExprKind::Name(vec![label.clone()]),
                            span: label.span,
                        }
                    } else {
                        self.expr(0, true)?
                    };
                    args.push(Argument { label, value });
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
                if let ExprKind::Name(path) = &mut left.kind {
                    if path[0].value.as_bytes()[0].is_ascii_uppercase() {
                        left.span.end = segment.span.end;
                        path.push(segment);
                        continue;
                    }
                }
                let span = Span::new(left.span.start, segment.span.end);
                left = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(left),
                        name: segment,
                    },
                    span,
                };
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

    fn pattern(&mut self) -> Result<Pattern, Diagnostic> {
        if self.depth >= 256 {
            return Err(self.error(DiagnosticKind::Syntax, "pattern nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.pattern_inner();
        self.depth -= 1;
        result
    }
    fn pattern_inner(&mut self) -> Result<Pattern, Diagnostic> {
        let token = self.current();
        let start = token.span.start;
        let kind = match token.kind {
            K::Name("_") => {
                self.advance();
                PatternKind::Any
            }
            K::Name(name) if name.as_bytes()[0].is_ascii_lowercase() && !reserved(name) => {
                self.advance();
                PatternKind::Bind(Text::new(name, token.span))
            }
            K::Name("True" | "False")
            | K::Integer(_)
            | K::Float(_)
            | K::String(_)
            | K::MultilineString(_)
            | K::Minus => {
                let expression = self.expr(8, false)?;
                match expression.kind {
                    ExprKind::Integer(_)
                    | ExprKind::Float(_)
                    | ExprKind::String(_)
                    | ExprKind::Bool(_)
                    | ExprKind::Unary { op: Unary::Neg, .. } => {
                        PatternKind::Literal(Box::new(expression))
                    }
                    _ => {
                        return Err(
                            self.error(DiagnosticKind::Syntax, "pattern requires a literal")
                        );
                    }
                }
            }
            K::LParen => {
                self.advance();
                self.newlines();
                let mut elements = Vec::new();
                while self.current().kind != K::RParen {
                    elements.push(self.pattern()?);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(K::RParen, "expected ')' after tuple pattern")?;
                if elements.is_empty() {
                    PatternKind::Literal(Box::new(Expr {
                        kind: ExprKind::Unit,
                        span: Span::new(start, self.previous_end()),
                    }))
                } else if elements.len() == 1 {
                    return Ok(elements.remove(0));
                } else {
                    PatternKind::Tuple(elements)
                }
            }
            K::LBracket => {
                self.advance();
                self.newlines();
                let mut elements = Vec::new();
                let mut tail = None;
                while self.current().kind != K::RBracket {
                    if self.current().kind == K::Dot && self.peek(1).kind == K::Dot {
                        let start = self.current().span.start;
                        self.advance();
                        self.advance();
                        self.newlines();
                        tail = Some(Box::new(
                            if matches!(self.current().kind, K::RBracket | K::Comma) {
                                Pattern {
                                    kind: PatternKind::Any,
                                    span: Span::new(start, self.previous_end()),
                                }
                            } else {
                                self.pattern()?
                            },
                        ));
                        self.newlines();
                        self.take(K::Comma);
                        self.newlines();
                        break;
                    }
                    elements.push(self.pattern()?);
                    self.newlines();
                    if !self.take(K::Comma) {
                        break;
                    }
                    self.newlines();
                }
                self.expect(
                    K::RBracket,
                    "expected ']' after list pattern; a spread must be last",
                )?;
                PatternKind::List { elements, tail }
            }
            K::Name(name) if name.as_bytes()[0].is_ascii_uppercase() => {
                self.advance();
                let mut path = vec![Text::new(name, token.span)];
                while self.take(K::Dot) {
                    path.push(self.name("expected constructor name")?);
                }
                let mut args = Vec::new();
                let mut rest = false;
                if self.take(K::LParen) {
                    self.newlines();
                    while self.current().kind != K::RParen {
                        if self.current().kind == K::Dot && self.peek(1).kind == K::Dot {
                            self.advance();
                            self.advance();
                            rest = true;
                            self.newlines();
                            break;
                        }
                        let label = if matches!(self.current().kind, K::Name(_))
                            && self.peek(1).kind == K::Colon
                        {
                            let name = self.lower_name("expected field name")?;
                            self.advance();
                            self.newlines();
                            Some(name)
                        } else {
                            None
                        };
                        let value = if matches!(self.current().kind, K::Comma | K::RParen) {
                            let Some(label) = &label else {
                                return Err(self.error(DiagnosticKind::Syntax, "expected pattern"));
                            };
                            Pattern {
                                kind: PatternKind::Bind(label.clone()),
                                span: label.span,
                            }
                        } else {
                            self.pattern()?
                        };
                        args.push(PatternArgument { label, value });
                        self.newlines();
                        if !self.take(K::Comma) {
                            break;
                        }
                        self.newlines();
                    }
                    self.expect(K::RParen, "expected ')' after constructor pattern")?;
                }
                PatternKind::Constructor { path, args, rest }
            }
            _ => return Err(self.error(DiagnosticKind::Syntax, "expected pattern")),
        };
        Ok(Pattern {
            kind,
            span: Span::new(start, self.previous_end()),
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
            | "return"
            | "while"
            | "loop"
            | "let"
            | "mut"
            | "in"
            | "where"
            | "do"
            | "try"
            | "catch"
            | "throw"
            | "async"
            | "yield"
            | "defer"
            | "macro"
            | "mod"
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

fn parse_string(
    raw: &str,
    span: Span,
    multiline: bool,
    method_context: bool,
) -> Result<Expr, Diagnostic> {
    let (text, positions) = super::strings::content(raw, span, multiline)?;
    let mut cursor = 0;
    let mut literal = String::new();
    let mut parts = Vec::new();
    while cursor < text.len() {
        if text[cursor..].starts_with("#{") {
            if !literal.is_empty() {
                parts.push(Expr {
                    kind: ExprKind::String(std::mem::take(&mut literal)),
                    span,
                });
            }
            let start = cursor + 2;
            let end = super::strings::interpolation_end(&text, start).map_err(|mut e| {
                e.span = Span::new(positions[e.span.start], positions[e.span.end]);
                e
            })?;
            let mut tokens = super::lexer::lex(&text[start..end]).map_err(|mut e| {
                e.span = Span::new(
                    positions[start + e.span.start],
                    positions[start + e.span.end],
                );
                e
            })?;
            for token in &mut tokens {
                token.span = Span::new(
                    positions[start + token.span.start],
                    positions[start + token.span.end],
                );
            }
            let mut parser = Parser {
                tokens: &tokens,
                cursor: 0,
                depth: 0,
                method_context,
            };
            let value = parser.expr(0, true)?;
            parser.newlines();
            parser.expect(K::Eof, "expected end of interpolation expression")?;
            parts.push(value);
            cursor = end + 1;
        } else if text[cursor..].starts_with('\\') {
            literal.push(super::strings::escape(&text, &mut cursor, &positions)?);
        } else {
            let ch = text[cursor..].chars().next().unwrap();
            literal.push(ch);
            cursor += ch.len_utf8();
        }
    }
    if parts.is_empty() {
        Ok(Expr {
            kind: ExprKind::String(literal),
            span,
        })
    } else {
        if !literal.is_empty() {
            parts.push(Expr {
                kind: ExprKind::String(literal),
                span,
            });
        }
        Ok(Expr {
            kind: ExprKind::Interpolate(parts),
            span,
        })
    }
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

    #[test]
    fn constants_and_failure_signatures_keep_declared_spans() {
        let source = "const MAX_RETRY = 30_000\npub const TEXT: String = \"é\"\n@external(c, \"seed\")\nfn seed() -> Int use {}\nfn load(n: Int) -> Int\n fails Missing | Invalid\n use {} { fail Missing(value: n) }\nfn pure() -> Int fails {} { 42 }";
        let module = parse(&lex(source).unwrap()).unwrap();
        assert_eq!(module.consts.len(), 2);
        let first = &module.consts[0];
        assert_eq!(first.name.value, "MAX_RETRY");
        assert!(!first.public);
        assert!(first.annotation.is_none());
        assert_eq!(
            &source[first.value.span.start..first.value.span.end],
            "30_000"
        );
        let text = &module.consts[1];
        assert!(text.public);
        assert_eq!(text.annotation.as_ref().unwrap().value, "String");
        assert_eq!(&source[text.name.span.start..text.name.span.end], "TEXT");
        assert_eq!(&source[text.value.span.start..text.value.span.end], "\"é\"");
        assert!(module.functions.iter().all(|function| !function.constant));
        assert!(module.functions[0].fails.is_none());
        let failures = module.functions[1].fails.as_ref().unwrap();
        assert_eq!(
            failures
                .iter()
                .map(|ty| ty.value.as_str())
                .collect::<Vec<_>>(),
            ["Missing", "Invalid"]
        );
        for failure in failures {
            assert_eq!(&source[failure.span.start..failure.span.end], failure.value);
        }
        assert!(module.functions[2].fails.as_ref().unwrap().is_empty());
        let Statement::Expr(value) = &module.functions[1].body.as_ref().unwrap().statements[0]
        else {
            panic!("expected failure expression")
        };
        assert!(matches!(value.kind, ExprKind::Fail(_)));
    }

    #[test]
    fn escape_arms_share_patterns_guards_and_postfix_precedence() {
        let source = "fn recover() -> Int { work()\n escape { Missing(value) if value < 0 -> -value\n Other -> 0 } match { 0 -> 1\n _ -> 2 } }";
        let module = parse(&lex(source).unwrap()).unwrap();
        let Statement::Expr(value) = &module.functions[0].body.as_ref().unwrap().statements[0]
        else {
            panic!("expected result")
        };
        let ExprKind::Match { value, .. } = &value.kind else {
            panic!("match must wrap the escape result")
        };
        let ExprKind::Escape { arms, .. } = &value.kind else {
            panic!("expected escape")
        };
        assert_eq!(arms.len(), 2);
        assert!(arms[0].guard.is_some());
        assert!(arms[1].guard.is_none());
        assert!(
            matches!(&arms[0].pattern.kind, PatternKind::Constructor { path, .. } if path[0].value == "Missing")
        );

        // As in the legacy grammar, fail recursively consumes an expression;
        // catching that failure requires an enclosing block or parentheses.
        let module =
            parse(&lex("fn f() { fail Missing escape { Missing -> 0 } }").unwrap()).unwrap();
        let Statement::Expr(value) = &module.functions[0].body.as_ref().unwrap().statements[0]
        else {
            panic!("expected fail")
        };
        let ExprKind::Fail(value) = &value.kind else {
            panic!("expected outer fail")
        };
        assert!(matches!(value.kind, ExprKind::Escape { .. }));
        assert!(parse(&lex("fn f() { work() escape {} }").unwrap()).is_ok());
    }

    #[test]
    fn malformed_constants_failure_sets_and_escape_arms_are_positioned_errors() {
        for source in [
            "const value = 1",
            "const VALUE: Int",
            "const VALUE =",
            "const VALUE = 1\nimport Other",
            "fn f() fails {Missing} { 0 }",
            "fn f() fails Missing | { 0 }",
            "fn f() use {} use {} { 0 }",
            "fn f() { fail }",
            "fn f() { work() escape Missing -> 0 }",
            "fn f() { work() escape { Missing 0 } }",
            "fn f() { work() escape { Missing -> } }",
            "fn f() { work() escape { Missing if -> 0 } }",
        ] {
            let error = parse(&lex(source).unwrap()).expect_err(source);
            assert!(
                error.span.start <= error.span.end && error.span.end <= source.len(),
                "{source}: {error:?}"
            );
            assert!(
                source.is_char_boundary(error.span.start)
                    && source.is_char_boundary(error.span.end)
            );
        }
    }

    #[test]
    fn trait_and_impl_methods_keep_constraints_defaults_and_implicit_types() {
        let source = "pub trait Score: Eq + Show {\n fn score(self) -> Int\n fn pick(self, other: Self) -> Self\n fn display(self) -> String { \"score=#{Score.score(self)}\" }\n}\nimpl Pair[A: Eq, B: Show]: Score {\n fn score(self) { 42 }\n fn pick(self, other) { other }\n}";
        let module = parse(&lex(source).unwrap()).unwrap();
        let definition = &module.traits[0];
        assert!(definition.public);
        assert_eq!(definition.name.value, "Score");
        assert_eq!(
            definition
                .supers
                .iter()
                .map(|item| item.value.as_str())
                .collect::<Vec<_>>(),
            ["Eq", "Show"]
        );
        assert_eq!(definition.methods.len(), 3);
        assert!(definition.methods[0].body.is_none());
        assert!(definition.methods[2].body.is_some());
        let receiver = &definition.methods[0].params[0];
        assert_eq!(receiver.name.value, "self");
        assert_eq!(receiver.ty.value, "Self");
        assert_eq!(receiver.name.span, receiver.ty.span);
        assert_eq!(definition.methods[1].params[1].ty.value, "Self");

        let implementation = &module.impls[0];
        assert_eq!(implementation.target.value, "Pair[A,B]");
        assert_eq!(implementation.trait_name.value, "Score");
        assert_eq!(implementation.params[0].name.value, "A");
        assert_eq!(implementation.params[0].bounds[0].value, "Eq");
        assert_eq!(implementation.params[1].bounds[0].value, "Show");
        assert_eq!(implementation.methods[0].params[0].ty.value, "Self");
        let omitted = &implementation.methods[1].params[1];
        assert!(omitted.ty.value.is_empty());
        assert_eq!(omitted.name.span, omitted.ty.span);
        assert!(
            source[implementation.span.start..implementation.span.end].starts_with("impl Pair")
        );
        assert_eq!(implementation.span.end, source.len());
    }

    #[test]
    fn method_context_is_restored_and_malformed_traits_are_rejected() {
        let source = "impl Key: Show { fn show(self) { \"K#{self.value}\" } }\nfn ordinary() { \"é#{self}\" }";
        let error = parse(&lex(source).unwrap()).unwrap_err();
        assert_eq!(&source[error.span.start..error.span.end], "self");
        for source in [
            "trait Score { fn score(value) -> Int }",
            "trait Score { pub fn score(self) -> Int }",
            "trait Score { fn score(self) -> Int fn next(self) -> Int }",
            "trait Score: { fn score(self) -> Int }",
            "trait Score { fn score(self) -> Int",
            "impl Key: Score { fn score(self) }",
            "impl Key Score { fn score(self) { 0 } }",
            "pub impl Key: Score { fn score(self) { 0 } }",
            "fn ordinary(value) { value }",
            "fn ordinary(self: Self) { self }",
            "impl Key: Show { fn show(self) { \"#{self}\" } }\nconst LEAK = self",
        ] {
            let error = parse(&lex(source).unwrap()).expect_err(source);
            assert!(
                error.span.start <= error.span.end && error.span.end <= source.len(),
                "{source}: {error:?}"
            );
        }
    }
    #[test]
    fn function_and_lambda_signatures_keep_absent_and_empty_sets_distinct() {
        let source = "fn absent(n: Int) { n }\nfn pure(n: Int) -> Int fails {} use {} { n }\nfn effects(n: Int) -> Int fails Missing | Other use {Logger, State[Int]} { n }\n@external(c, \"board_wait\")\nsuspend fn wait(n: Int) -> Int use Async";
        let module = parse(&lex(source).unwrap()).unwrap();
        assert!(module.functions[0].fails.is_none());
        assert!(module.functions[0].effects.is_none());
        assert!(module.functions[1].fails.as_ref().unwrap().is_empty());
        assert!(module.functions[1].effects.as_ref().unwrap().is_empty());
        assert_eq!(
            module.functions[2].effects.as_ref().unwrap()[1].value,
            "State[Int]"
        );
        assert!(module.functions[3].suspends);
        let module = parse(&lex("fn main() { fn(x: Int) { x } }").unwrap()).unwrap();
        let Statement::Expr(Expr {
            kind:
                ExprKind::Lambda {
                    params,
                    return_type,
                    fails,
                    effects,
                    ..
                },
            ..
        }) = &module.functions[0].body.as_ref().unwrap().statements[0]
        else {
            panic!("expected lambda")
        };
        assert_eq!(params[0].ty.as_ref().unwrap().value, "Int");
        assert!(return_type.is_none());
        assert!(fails.is_none());
        assert!(effects.is_none());
    }
    #[test]
    fn nested_function_types_and_set_arguments_preserve_canonical_boundaries() {
        let source = "fn apply(f: fn(Int)->String, pure: fn(Int)->String fails {} use {}, nested: fn()->(fn(Int)->String fails Missing) use {Logger}, data: Holder[Missing|Other,{State[Int],R}]) { 0 }";
        let module = parse(&lex(source).unwrap()).unwrap();
        let params = &module.functions[0].params;
        assert_eq!(params[0].ty.value, "fn(Int)->String");
        assert_eq!(params[1].ty.value, "fn(Int)->String fails {} use{}");
        assert_eq!(
            params[2].ty.value,
            "fn()->(fn(Int)->String fails Missing) use{Logger}"
        );
        assert_eq!(params[3].ty.value, "Holder[Missing|Other,{State[Int],R}]");
    }
    #[test]
    fn anonymous_parameters_keep_patterns_annotations_and_callee_evaluation_shape() {
        let source =
            "fn main() { fn((a,b):(Int,Int), Request(url:,..), _) { a }((1,2),request,()) }";
        let module = parse(&lex(source).unwrap()).unwrap();
        let Statement::Expr(Expr {
            kind: ExprKind::Call { callee, args },
            ..
        }) = &module.functions[0].body.as_ref().unwrap().statements[0]
        else {
            panic!("expected call")
        };
        assert_eq!(args.len(), 3);
        let ExprKind::Lambda { params, .. } = &callee.kind else {
            panic!("expected lambda callee")
        };
        assert!(matches!(params[0].pattern.kind, PatternKind::Tuple(_)));
        assert_eq!(params[0].ty.as_ref().unwrap().value, "(Int,Int)");
        assert!(matches!(
            params[1].pattern.kind,
            PatternKind::Constructor { rest: true, .. }
        ));
        assert!(matches!(params[2].pattern.kind, PatternKind::Any));
    }
    #[test]
    fn malformed_function_types_lambdas_and_clause_order_are_positioned_errors() {
        for source in [
            "fn main(f:fn(Int)) { 0 }",
            "fn main(f:fn(Int)->Int use{} fails{}) { 0 }",
            "fn main() { fn named(x) { x } }",
            "fn main() { fn[A](x) { x } }",
            "fn main() { fn(x) -> Int { x } }",
            "fn main() { fn(x) fails {} { x } }",
            "fn main() { fn(x) use{} use{} { x } }",
            "fn main() { fn(x) x }",
            "suspend fn main() { () }",
            "trait Read { suspend fn read(self)->Int }",
            "fn top(x) { x }",
        ] {
            let error = parse(&lex(source).unwrap()).unwrap_err();
            assert!(
                error.span.start <= error.span.end && error.span.end <= source.len(),
                "{source}: {error:?}"
            );
        }
        let source = format!(
            "fn f(value:{}Int{}) {{ 0 }}",
            "List[".repeat(300),
            "]".repeat(300)
        );
        assert!(
            parse(&lex(&source).unwrap())
                .unwrap_err()
                .message
                .contains("nesting")
        );
    }
}
