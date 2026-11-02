use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "Usage: emela-emb build <file> --out-dir <dir>\n\
                        Generate emela_embedded.c and emela_embedded.h.\n\
                        The output directory must be new or empty.\n\
                        Use -- before an input path beginning with '-'.";

#[derive(Debug)]
pub enum Command {
    Build { input: PathBuf, out_dir: PathBuf },
    Help,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter().peekable();
    let Some(command) = args.next() else {
        return Err("expected the 'build' command".into());
    };
    if command == "--help" || command == "-h" {
        return if args.peek().is_none() {
            Ok(Command::Help)
        } else {
            Err("unexpected argument after --help".into())
        };
    }
    if command != "build" {
        return Err(format!("unknown command '{}'", command.to_string_lossy()));
    }
    let mut input = None;
    let mut out_dir = None;
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only && arg == "--" {
            positional_only = true;
        } else if !positional_only && arg == "--out-dir" {
            if out_dir.is_some() {
                return Err("--out-dir was specified more than once".into());
            }
            let value = args.next().ok_or("expected a directory after --out-dir")?;
            if value.is_empty() {
                return Err("--out-dir cannot be empty".into());
            }
            out_dir = Some(PathBuf::from(value));
        } else if !positional_only && (arg == "--help" || arg == "-h") {
            if input.is_none() && out_dir.is_none() && args.peek().is_none() {
                return Ok(Command::Help);
            }
            return Err("--help must be used on its own after 'build'".into());
        } else if !positional_only && arg.to_str().is_some_and(|s| s.starts_with('-')) {
            return Err(format!("unknown option '{}'", arg.to_string_lossy()));
        } else if input.is_some() {
            return Err(format!("unexpected input '{}'", arg.to_string_lossy()));
        } else if arg.is_empty() {
            return Err("input path cannot be empty".into());
        } else {
            input = Some(PathBuf::from(arg));
        }
    }
    Ok(Command::Build {
        input: input.ok_or("expected an input file")?,
        out_dir: out_dir.ok_or("expected --out-dir <dir>")?,
    })
}
