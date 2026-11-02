//! An independent compiler for the minimal Emela subset in `docs/language-subset.md`.

pub mod artifact;
mod backend;
mod contract;
mod diagnostic;
mod frontend;
mod semantics;
mod writer;

pub use contract::{
    DerivedOperation, Error, ErrorKind, ExportedFunction, ExternalAdapter, ExternalResult,
    GeneratedFile, Options, Output,
};
pub use diagnostic::{Diagnostic, DiagnosticKind, Span};

/// Compile the supported single-file Emela subset into C11 and its header.
///
/// All generated content is returned in memory. No files, environment variables,
/// external compilers or Emela installations are accessed.
pub fn compile(source: &str) -> Result<Output, Vec<Diagnostic>> {
    let tokens = frontend::lexer::lex(source).map_err(|error| vec![error])?;
    let ast = frontend::parser::parse(&tokens).map_err(|error| vec![error])?;
    let checked = frontend::check::check(ast)?;
    use contract::ir::*;
    let mut function = Function::new(checked.export_symbol);
    function.exported = true;
    function.export_symbol = Some(checked.export_symbol.to_owned());
    function.ret = TypeId::INT;
    function.body = Some(function.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(checked.value).into())),
    }));
    let mut program = Program {
        core: Definitions::default(),
        functions: Default::default(),
        error_shows: vec![],
        const_function_groups: vec![],
        has_scopes: false,
    };
    program.functions.alloc(function);
    backend::compile_program(program, &Options::default()).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| {
                Diagnostic::new(
                    DiagnosticKind::Unsupported,
                    Span::new(0, source.len()),
                    error.message,
                )
            })
            .collect()
    })
}
