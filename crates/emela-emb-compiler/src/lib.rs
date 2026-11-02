//! An independent Emela compiler. All compilation inputs and outputs remain in memory.
pub mod artifact;
mod backend;
mod contract;
mod diagnostic;
mod frontend;
mod semantics;
mod sources;
mod writer;

pub use contract::{
    DerivedOperation, Error, ErrorKind, ExportedFunction, ExternalAdapter, ExternalResult,
    GeneratedFile, Options, Output,
};
pub use diagnostic::{Diagnostic, DiagnosticKind, Span};
pub use sources::Source;

/// Compile a single source using the default resource limits and entry configuration.
pub fn compile(source: &str) -> Result<Output, Vec<Diagnostic>> {
    compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default(),
    )
}

/// Compile explicit source files rooted at `entry` (one of the source names).
///
/// Imports select other supplied modules; unreachable files are not parsed or typechecked.
/// The compiler never reads files, environment variables, installations or external processes.
/// `options.with_entry(...)` independently selects the generated C entry function.
pub fn compile_sources(
    sources: &[Source<'_>],
    entry: &str,
    options: &Options,
) -> Result<Output, Vec<Diagnostic>> {
    let modules = frontend::modules::collect(sources, entry)?;
    let checked = frontend::check::check(modules)?;
    let lowered = frontend::lower::lower(&checked);
    backend::compile_program(lowered, options).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| {
                let function = checked
                    .functions
                    .iter()
                    .find(|f| error.message.contains(&format!("`{}`", f.name)));
                Diagnostic::new(
                    DiagnosticKind::Backend,
                    function.map_or(Span::new(0, 0), |f| f.span),
                    error.message,
                )
                .in_source(function.map_or(entry, |f| f.source_name.as_str()))
            })
            .collect()
    })
}
