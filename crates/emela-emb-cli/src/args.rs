use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use emela_emb_compiler::Options;

pub const USAGE: &str = "Usage: emela-emb build <file>... --out-dir <dir> [options]\n\
                        Generate C, headers, and the artifact manifest.\n\
                        The output directory must be new or empty.\n\
                        --entry <file>           Select the entry source (default: first input).\n\
                        --entry-function <name>  Select the program entry function (default: main).\n\
                        --heap-bytes <bytes>     Set the fixed heap capacity (default: 0).\n\
                        --string-capacity <bytes> Set the generated String capacity (default: 256).\n\
                        --fiber-capacity <count> Set the fiber pool capacity (default: 8).\n\
                        --frame-capacity <count> Set the frame pool capacity (default: 32).\n\
                        --frame-bytes <bytes>    Set each frame capacity (default: 256).\n\
                        --task-capacity <count>  Set the external task capacity (default: 8).\n\
                        --event-capacity <count> Set the completion queue capacity (default: 8).\n\
                        --task-adapter <C=adapter> Map an external task binding (repeatable).\n\
                        Use -- before input paths beginning with '-'.";

#[derive(Debug)]
pub enum Command {
    Build {
        inputs: Vec<PathBuf>,
        out_dir: PathBuf,
        entry: Option<String>,
        options: Box<Options>,
    },
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
    let mut inputs = Vec::new();
    let mut out_dir = None;
    let mut entry = None;
    let mut entry_function = None;
    let mut heap_bytes = None;
    let mut string_capacity = None;
    let mut capacities = BTreeMap::new();
    let mut task_adapters = BTreeMap::new();
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
        } else if !positional_only
            && matches!(
                arg.to_str(),
                Some(
                    "--entry"
                        | "--entry-function"
                        | "--heap-bytes"
                        | "--string-capacity"
                        | "--fiber-capacity"
                        | "--frame-capacity"
                        | "--frame-bytes"
                        | "--task-capacity"
                        | "--event-capacity"
                        | "--task-adapter"
                )
            )
        {
            let option = arg.to_str().unwrap();
            let value = args
                .next()
                .ok_or_else(|| format!("expected a value after {option}"))?;
            let value = value
                .to_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{option} requires nonempty UTF-8 text"))?;
            match option {
                "--entry" => set_once(&mut entry, value.to_owned(), option)?,
                "--entry-function" => set_once(&mut entry_function, value.to_owned(), option)?,
                "--heap-bytes" => {
                    set_once(&mut heap_bytes, parse_number::<u32>(value, option)?, option)?
                }
                "--string-capacity" => set_once(
                    &mut string_capacity,
                    parse_number::<u16>(value, option)?,
                    option,
                )?,
                "--task-adapter" => {
                    let (symbol, adapter) = value
                        .split_once('=')
                        .filter(|(symbol, adapter)| c_identifier(symbol) && c_identifier(adapter))
                        .ok_or("--task-adapter requires C_symbol=adapter_symbol")?;
                    if task_adapters
                        .insert(symbol.to_owned(), adapter.to_owned())
                        .is_some()
                    {
                        return Err(format!("--task-adapter repeats the binding '{symbol}'"));
                    }
                }
                _ => {
                    let capacity = parse_number::<u16>(value, option)?;
                    if capacities.insert(option.to_owned(), capacity).is_some() {
                        return Err(format!("{option} was specified more than once"));
                    }
                }
            }
        } else if !positional_only && (arg == "--help" || arg == "-h") {
            if inputs.is_empty()
                && out_dir.is_none()
                && entry.is_none()
                && entry_function.is_none()
                && heap_bytes.is_none()
                && string_capacity.is_none()
                && capacities.is_empty()
                && task_adapters.is_empty()
                && args.peek().is_none()
            {
                return Ok(Command::Help);
            }
            return Err("--help must be used on its own after 'build'".into());
        } else if !positional_only && arg.to_str().is_some_and(|s| s.starts_with('-')) {
            return Err(format!("unknown option '{}'", arg.to_string_lossy()));
        } else if arg.is_empty() {
            return Err("input path cannot be empty".into());
        } else {
            inputs.push(PathBuf::from(arg));
        }
    }
    if inputs.is_empty() {
        return Err("expected at least one input file".into());
    }
    let mut options = Options::default();
    if let Some(name) = entry_function {
        options = options.with_entry(name);
    }
    if let Some(bytes) = heap_bytes {
        options = options.with_heap_bytes(bytes);
    }
    if let Some(bytes) = string_capacity {
        options = options.with_string_capacity(bytes);
    }
    for (option, capacity) in capacities {
        options = match option.as_str() {
            "--fiber-capacity" => options.with_fiber_capacity(capacity),
            "--frame-capacity" => options.with_frame_capacity(capacity),
            "--frame-bytes" => options.with_frame_bytes(capacity),
            "--task-capacity" => options.with_task_capacity(capacity),
            "--event-capacity" => options.with_event_capacity(capacity),
            _ => unreachable!(),
        };
    }
    for (symbol, adapter) in task_adapters {
        options = options.with_task_adapter(symbol, adapter);
    }
    Ok(Command::Build {
        inputs,
        out_dir: out_dir.ok_or("expected --out-dir <dir>")?,
        entry,
        options: Box::new(options),
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T, option: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{option} was specified more than once"));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_number<T: std::str::FromStr>(value: &str, option: &str) -> Result<T, String> {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{option} requires an unsigned decimal integer"));
    }
    value
        .parse()
        .map_err(|_| format!("{option} is out of range"))
}

fn c_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
