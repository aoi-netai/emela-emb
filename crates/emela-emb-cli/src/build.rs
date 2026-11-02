use std::fs;
use std::path::Path;

pub fn run(input: &Path, out_dir: &Path) -> Result<(), String> {
    let source = fs::read_to_string(input)
        .map_err(|error| format!("{}: cannot read input: {error}", input.display()))?;
    let output = emela_emb_compiler::compile(&source).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| {
                let (line, column) = line_column(&source, diagnostic.span.start);
                format!(
                    "{}:{line}:{column}: error[{}]: {}",
                    input.display(),
                    diagnostic.kind,
                    diagnostic.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
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
