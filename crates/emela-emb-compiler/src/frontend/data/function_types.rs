//! Parse the canonical function type text retained by the source parser.
use super::*;

impl Catalog {
    fn omitted_function_row(&mut self, kind: super::super::sets::SetKind) -> Vec<b::TypeId> {
        let origin = match self.omitted_rows {
            OmittedRows::Empty => return vec![],
            OmittedRows::Hidden => super::super::sets::Origin::HiddenParameter,
            OmittedRows::Inferred => super::super::sets::Origin::Inferred,
        };
        let mut index = usize::MAX;
        while self.core.types.ids.contains_key(&b::Type::Param(index)) {
            index -= 1;
        }
        let proxy = self.core.types.intern(b::Type::Param(index));
        self.row_requests.push(RowRequest {
            proxy,
            kind,
            origin,
            positive: self.row_positive,
        });
        vec![proxy]
    }
    pub(super) fn function_type(
        &mut self,
        modules: &Modules,
        module: usize,
        text: &str,
        span: Span,
        parameters: &HashMap<String, usize>,
        self_type: Option<Ty>,
    ) -> Result<Ty, Diagnostic> {
        let malformed = || Diagnostic::new(DK::Type, span, "malformed function type");
        let mut depth = 0usize;
        let mut close = None;
        for (index, ch) in text.char_indices().skip(2) {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1).ok_or_else(malformed)?;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.ok_or_else(malformed)?;
        let tail = text[close + 1..].strip_prefix("->").ok_or_else(malformed)?;
        let mut signature_params = Vec::new();
        self.row_positive = !self.row_positive;
        for argument in split(&text[3..close]) {
            signature_params.push(backend_type(
                self.type_text(modules, module, argument, span, parameters, self_type)?,
            ));
        }
        self.row_positive = !self.row_positive;
        let mut clause_start = tail.len();
        let mut nesting = 0usize;
        if !tail.starts_with("fn(") {
            for (index, ch) in tail.char_indices() {
                if nesting == 0
                    && (tail[index..].starts_with(" fails") || tail[index..].starts_with(" use"))
                {
                    clause_start = index;
                    break;
                }
                match ch {
                    '(' | '[' | '{' => nesting += 1,
                    ')' | ']' | '}' => nesting = nesting.saturating_sub(1),
                    _ => {}
                }
            }
        }
        let ret = backend_type(self.type_text(
            modules,
            module,
            tail[..clause_start].trim(),
            span,
            parameters,
            self_type,
        )?);
        let mut clauses = tail[clause_start..].trim();
        let mut errors = Vec::new();
        if let Some(rest) = clauses.strip_prefix("fails") {
            let (written, remaining) = rest
                .find(" use")
                .map_or((rest, ""), |index| (&rest[..index], &rest[index..]));
            let written = written.trim();
            if written != "{}" {
                for error in written.split('|') {
                    let ty = backend_type(self.type_text(
                        modules,
                        module,
                        error.trim(),
                        span,
                        parameters,
                        self_type,
                    )?);
                    if !matches!(self.core.ty(ty), b::Type::Param(_))
                        && !matches!(self.core.ty(ty),b::Type::Enum(id,_) if self.core.enums[*id].error)
                    {
                        return Err(Diagnostic::new(
                            DK::Type,
                            span,
                            "a function failure clause requires error types",
                        ));
                    }
                    errors.push(ty);
                }
            }
            clauses = remaining.trim();
        } else {
            errors = self.omitted_function_row(super::super::sets::SetKind::Error);
        }
        let effects = if let Some(effects) = clauses.strip_prefix("use") {
            if effects.trim() != "{}" {
                return Err(Diagnostic::new(
                    DK::Unsupported,
                    span,
                    "effect function types await effect declaration resolution",
                ));
            }
            Vec::new()
        } else if !clauses.is_empty() {
            return Err(malformed());
        } else {
            self.omitted_function_row(super::super::sets::SetKind::Effect)
        };
        let fails = !errors.is_empty();
        let errors = self.core.types.intern(b::Type::Set(b::SetSummary {
            params: errors,
            ..Default::default()
        }));
        let effects = self.core.types.intern(b::Type::Set(b::SetSummary {
            params: effects,
            ..Default::default()
        }));
        Ok(Ty::Compound(self.core.types.intern(b::Type::Fn(
            b::FnType {
                params: signature_params,
                ret,
                fails,
                suspends: false,
                ev: false,
                sets: Some(b::FnSets { errors, effects }),
            },
        ))))
    }
}
