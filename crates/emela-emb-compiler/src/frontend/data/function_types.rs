//! Parse the canonical function type text retained by the source parser.
use super::*;

impl Catalog {
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
        for argument in split(&text[3..close]) {
            signature_params.push(backend_type(
                self.type_text(modules, module, argument, span, parameters, self_type)?,
            ));
        }
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
        }
        if let Some(effects) = clauses.strip_prefix("use") {
            if effects.trim() != "{}" {
                return Err(Diagnostic::new(
                    DK::Unsupported,
                    span,
                    "effect function types await effect declaration resolution",
                ));
            }
        } else if !clauses.is_empty() {
            return Err(malformed());
        }
        let fails = !errors.is_empty();
        let errors = self.core.types.intern(b::Type::Set(b::SetSummary {
            fails,
            params: errors,
            ..Default::default()
        }));
        let effects = self.core.types.intern(b::Type::Set(Default::default()));
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
