use std::fmt;

/// A half-open UTF-8 byte range in the input source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Categories are specific to this compiler, not Emela's diagnostic codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Lexical,
    Syntax,
    Unsupported,
    Type,
    Range,
    Export,
    Name,
    Module,
    Backend,
}

impl fmt::Display for DiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Lexical => "lexical",
            Self::Syntax => "syntax",
            Self::Unsupported => "unsupported",
            Self::Type => "type",
            Self::Range => "range",
            Self::Export => "export",
            Self::Name => "name",
            Self::Module => "module",
            Self::Backend => "backend",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    /// Source name, when this diagnostic belongs to a supplied file.
    pub source: Option<String>,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            source: None,
            span,
            message: message.into(),
        }
    }
}

impl Diagnostic {
    pub fn in_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}
