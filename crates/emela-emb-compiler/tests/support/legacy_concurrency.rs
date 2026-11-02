//! Fixed legacy concurrency bodies with explicit exports and their original C observations.
#[path = "heap.rs"]
mod heap;
use emela_emb_compiler::{Diagnostic, Options, Output, Source, compile_sources};

pub fn options() -> Options {
    Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(512)
}

pub fn compile(source: &str, options: &Options) -> Result<Output, Vec<Diagnostic>> {
    let mut exported = String::new();
    for line in source.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("pub fn ") {
            let name = rest.split('(').next().unwrap();
            exported.push_str(&format!("@export(c,\"legacy_{name}\")\n"));
        }
        exported.push_str(line);
    }
    compile_sources(&[Source::new("main.emel", &exported)], "main.emel", options)
}

pub fn run(source: &str, harness: &str, options: &Options) -> Output {
    let output = heap::with_heap_audit(compile(source, options).unwrap());
    let header = output
        .files
        .iter()
        .find(|file| file.path == "emela_embedded.h")
        .unwrap();
    let mut harness = harness.to_owned();
    if harness.contains("{ty}") {
        let (symbol, prefix) = if harness.contains("emela_function_type_{ty}") {
            ("legacy_leak_start(", "emela_function_type_")
        } else {
            ("legacy_terminal_start(", "emela_type_")
        };
        let declaration = header
            .contents
            .lines()
            .find(|line| line.contains(symbol))
            .unwrap();
        let number: String = declaration
            .split(prefix)
            .nth(1)
            .unwrap()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        assert!(!number.is_empty());
        if harness.contains("{release}") {
            let release = header
                .contents
                .lines()
                .find(|line| {
                    line.starts_with("void emela_embedded_release_value_")
                        && line.contains(&format!("(emela_type_{number} *value)"))
                })
                .unwrap()
                .strip_prefix("void ")
                .unwrap()
                .split('(')
                .next()
                .unwrap();
            harness = harness.replace("{release}", release);
        }
        harness = harness.replace("{ty}", &number);
    }
    super::support::compile_and_run(&output, &harness);
    output
}
