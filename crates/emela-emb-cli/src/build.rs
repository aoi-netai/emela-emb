use std::fs;
use std::path::{Path, PathBuf};

use emela_emb_compiler::{Options, Source};

pub fn run(
    inputs: &[PathBuf],
    out_dir: &Path,
    entry: Option<&str>,
    options: &Options,
) -> Result<(), String> {
    let sources = inputs
        .iter()
        .map(|input| {
            fs::read_to_string(input)
                .map(|text| (input.to_string_lossy().into_owned(), text))
                .map_err(|error| format!("{}: cannot read input: {error}", input.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_inputs = sources
        .iter()
        .map(|(name, text)| Source::new(name, text))
        .collect::<Vec<_>>();
    let entry = entry.unwrap_or(&sources[0].0);
    let output = emela_emb_compiler::compile_sources(&source_inputs, entry, options).map_err(
        |diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| {
                    let name = diagnostic.source.as_deref().unwrap_or(entry);
                    let source = sources
                        .iter()
                        .find(|(path, _)| path == name)
                        .map(|(_, text)| text.as_str())
                        .unwrap_or("");
                    let (line, column) = line_column(source, diagnostic.span.start);
                    format!(
                        "{name}:{line}:{column}: error[{}]: {}",
                        diagnostic.kind, diagnostic.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        },
    )?;
    crate::output::publish(&output, out_dir).map_err(|error| {
        format!(
            "{}: cannot save generated files: {error}",
            out_dir.display()
        )
    })
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else if ch != '\r' {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::line_column;

    #[test]
    fn columns_count_characters_and_crlf_is_one_line_break() {
        let source = "éa\r\nλ!";
        assert_eq!(line_column(source, "é".len()), (1, 2));
        assert_eq!(line_column(source, "éa\r\n".len()), (2, 1));
        assert_eq!(line_column(source, "éa\r\nλ".len()), (2, 2));
        assert_eq!(line_column(source, source.len()), (2, 3));
    }
}
