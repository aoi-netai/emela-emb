use super::{ast, lexer, parser};
use crate::{Diagnostic, DiagnosticKind, Source, Span, sources::CORE_MODULES};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Component, Path},
};

#[derive(Clone, Debug)]
pub(crate) struct Module {
    pub name: String,
    pub source_name: String,
    pub builtin: bool,
    pub ast: ast::Module,
}
pub(crate) struct Modules {
    pub modules: Vec<Module>,
    pub entry: usize,
}

pub(crate) fn collect(sources: &[Source<'_>], entry: &str) -> Result<Modules, Vec<Diagnostic>> {
    let Some(entry_source) = sources.iter().position(|s| s.name == entry) else {
        return Err(vec![Diagnostic::new(
            DiagnosticKind::Module,
            Span::new(0, 0),
            format!("entry source '{entry}' was not supplied"),
        )]);
    };
    let root = common_parent(sources);
    let mut names = BTreeMap::new();
    let mut source_modules = Vec::new();
    let mut errors = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        match module_name(source.name, &root) {
            Ok(name) => {
                if CORE_MODULES.iter().any(|(core, _)| *core == name)
                    || matches!(name.as_str(), "Prelude" | "Std")
                    || name.starts_with("Std.")
                {
                    errors.push(
                        Diagnostic::new(
                            DiagnosticKind::Module,
                            Span::new(0, 0),
                            format!("module '{name}' is reserved by the compiler standard library"),
                        )
                        .in_source(source.name),
                    );
                }
                if let Some(previous) = names.insert(name.clone(), i) {
                    errors.push(
                        Diagnostic::new(
                            DiagnosticKind::Module,
                            Span::new(0, 0),
                            format!(
                                "duplicate module '{name}' (also '{}')",
                                sources[previous].name
                            ),
                        )
                        .in_source(source.name),
                    );
                }
                source_modules.push(name);
            }
            Err(message) => {
                errors.push(
                    Diagnostic::new(DiagnosticKind::Module, Span::new(0, 0), message)
                        .in_source(source.name),
                );
                source_modules.push(String::new());
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut modules = Vec::new();
    let mut states = vec![0; sources.len()];
    let mut indices = HashMap::new();
    visit(
        entry_source,
        sources,
        &source_modules,
        &names,
        &mut states,
        &mut indices,
        &mut modules,
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors);
    }
    let entry = indices[&entry_source];
    for (name, text) in CORE_MODULES {
        let source_name = format!("<core/{name}>");
        match lexer::lex(text).and_then(|tokens| parser::parse(&tokens)) {
            Ok(ast) => modules.push(Module {
                name: (*name).into(),
                source_name,
                builtin: true,
                ast,
            }),
            Err(error) => errors.push(error.in_source(source_name)),
        }
    }
    if errors.is_empty() {
        Ok(Modules { modules, entry })
    } else {
        Err(errors)
    }
}
#[allow(clippy::too_many_arguments)]
fn visit(
    index: usize,
    sources: &[Source<'_>],
    source_modules: &[String],
    names: &BTreeMap<String, usize>,
    states: &mut [u8],
    indices: &mut HashMap<usize, usize>,
    modules: &mut Vec<Module>,
    errors: &mut Vec<Diagnostic>,
) {
    if states[index] != 0 {
        return;
    }
    states[index] = 1;
    let source = sources[index];
    let ast = match lexer::lex(source.text).and_then(|tokens| parser::parse(&tokens)) {
        Ok(ast) => ast,
        Err(error) => {
            errors.push(error.in_source(source.name));
            states[index] = 2;
            return;
        }
    };
    let imports = ast.imports.clone();
    indices.insert(index, modules.len());
    modules.push(Module {
        name: source_modules[index].clone(),
        source_name: source.name.into(),
        builtin: false,
        ast,
    });
    for import in imports {
        if CORE_MODULES
            .iter()
            .any(|(name, _)| *name == import.path.value)
        {
            continue;
        }
        if let Some(&next) = names.get(&import.path.value) {
            if states[next] == 1 {
                errors.push(
                    Diagnostic::new(
                        DiagnosticKind::Module,
                        import.path.span,
                        format!(
                            "cyclic import from '{}' to '{}'",
                            source_modules[index], import.path.value
                        ),
                    )
                    .in_source(source.name),
                );
            } else {
                visit(
                    next,
                    sources,
                    source_modules,
                    names,
                    states,
                    indices,
                    modules,
                    errors,
                );
            }
        } else {
            errors.push(
                Diagnostic::new(
                    DiagnosticKind::Module,
                    import.path.span,
                    format!("module '{}' was not supplied", import.path.value),
                )
                .in_source(source.name),
            );
        }
    }
    states[index] = 2;
}
fn common_parent(sources: &[Source<'_>]) -> Vec<String> {
    let components = |name: &str| {
        Path::new(name)
            .parent()
            .unwrap_or(Path::new(""))
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                Component::RootDir => Some("/".into()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut root = sources
        .first()
        .map_or_else(Vec::new, |s| components(s.name));
    for source in sources.iter().skip(1) {
        let parts = components(source.name);
        let length = root.iter().zip(&parts).take_while(|(a, b)| a == b).count();
        root.truncate(length);
    }
    root
}
fn module_name(name: &str, root: &[String]) -> Result<String, String> {
    let parts: Vec<_> = Path::new(name)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::RootDir => Some("/".into()),
            _ => None,
        })
        .collect();
    let Some((file, parents)) = parts[root.len()..].split_last() else {
        return Err(format!("invalid source name '{name}'"));
    };
    let Some(stem) = file.strip_suffix(".emel") else {
        return Err(format!("source name '{name}' must end with .emel"));
    };
    let mut result = Vec::new();
    for segment in parents
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(stem))
    {
        if segment.is_empty()
            || !segment.as_bytes()[0].is_ascii_lowercase()
            || !segment
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            || segment.ends_with('_')
            || segment.contains("__")
        {
            return Err(format!(
                "source path segment '{segment}' must be lowercase snake_case"
            ));
        }
        let mut converted = String::new();
        let mut uppercase = true;
        for ch in segment.chars() {
            if ch == '_' {
                uppercase = true;
            } else {
                converted.push(if uppercase {
                    ch.to_ascii_uppercase()
                } else {
                    ch
                });
                uppercase = false;
            }
        }
        if !converted.bytes().any(|b| b.is_ascii_lowercase()) {
            return Err(format!(
                "module name '{converted}' must contain a lowercase letter"
            ));
        }
        result.push(converted);
    }
    Ok(result.join("."))
}
