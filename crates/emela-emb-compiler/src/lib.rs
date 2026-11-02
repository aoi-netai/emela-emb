//! An independent compiler for the minimal Emela subset in `docs/language-subset.md`.

mod diagnostic;
#[cfg(test)]
mod frontend;

pub use diagnostic::{Diagnostic, DiagnosticKind, Span};
