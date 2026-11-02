//! An independent compiler for the minimal Emela subset in `docs/language-subset.md`.

mod backend;
mod contract;
mod diagnostic;
mod frontend;

pub use contract::Output;
pub use diagnostic::{Diagnostic, DiagnosticKind, Span};

/// Compile the supported single-file Emela subset into C11 and its header.
///
/// All generated content is returned in memory. No files, environment variables,
/// external compilers or Emela installations are accessed.
pub fn compile(source: &str) -> Result<Output, Vec<Diagnostic>> {
    let tokens = frontend::lexer::lex(source).map_err(|error| vec![error])?;
    let ast = frontend::parser::parse(&tokens).map_err(|error| vec![error])?;
    let checked = frontend::check::check(ast)?;
    let lowered = backend::lower::lower(checked);
    let planned = backend::plan::plan(lowered);
    Ok(backend::emit::emit(planned))
}
