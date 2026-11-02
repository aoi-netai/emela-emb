use emela_emb_compiler::{Options, Output, Source, compile_sources};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn execute(output: Output, harness: &str, expected: i32) {
    let directory = Directory(std::env::temp_dir().join(format!(
        "emela-p2-string-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir_all(&directory.0).unwrap();
    for file in output.files {
        let path = directory.0.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, file.contents).unwrap();
    }
    fs::write(directory.0.join("harness.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = Command::new("cc")
            .current_dir(&directory.0)
            .args([
                "-std=c11",
                optimization,
                "emela_embedded.c",
                "harness.c",
                "-lm",
                "-o",
                "run",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{optimization}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(directory.0.join("run")).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(expected),
            "{optimization}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
fn compile(source: &str, options: Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", &options).unwrap()
}
const BOARD: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) { (void)kind; (void)message; (void)length; abort(); }
"#;
#[test]
fn scalar_interpolation_unicode_nul_string_operators_and_numeric_formats_execute() {
    let source = r##"
@export(c, "exercise")
pub fn exercise() -> Int {
  assert String.length("a\u{301}") == 1
  assert String.byte_size("a\u{301}") == 3
  assert String.trim(" \témela\n") == "émela"
  assert String.contains("a\u{0}b", "\u{0}")
  assert String.starts_with("émela", "é")
  assert String.ends_with("émela", "la")
  assert "a\u{0}x" < "a\u{0}y"
  assert "é" >= "a"
  assert "#{42},#{Int64.MAX},#{1.5},#{True},#{()}" == "42,9223372036854775807,1.5,True,()"
  assert "nested #{"value #{1 + 1}"}" == "nested value 2"
  assert Float.floor(2.9) == 2
  assert Float.ceil(-2.9) == -2
  assert Float.round(-2.5) == -3
  assert Float.truncate(-2.9) == -2
  assert Int.to_string(Int.MIN) == "-2147483648"
  assert Int64.to_string(Int64.MIN) == "-9223372036854775808"
  42
}
"##;
    execute(
        compile(source, Options::default()),
        &format!("{BOARD}\nint main(void) {{ assert(exercise() == 42); return 0; }}"),
        0,
    );
}
#[test]
fn multiline_strings_and_return_buffer_preserve_embedded_bytes() {
    let source = r##"
@export(c, "message")
pub fn message(text: String) -> String {
  prefix = """
    first
    second
    """
  assert prefix == "first\nsecond"
  "#{prefix}:#{text}"
}
"##;
    execute(
        compile(source, Options::default().with_string_capacity(64)),
        &format!(
            "{BOARD}\nint main(void) {{ char out[64]; const char bytes[] = {{'a', 0, 'b'}}; EmelaString value = message((EmelaString){{bytes, 3, NULL}}, out, 64); const char expected[] = \"first\\nsecond:a\\0b\"; assert(value.length == sizeof(expected) - 1); assert(memcmp(value.data, expected, value.length) == 0); return 0; }}"
        ),
        0,
    );
}
#[test]
fn discarded_string_append_keeps_capacity_defects() {
    let source = "@export(c, \"discard\")\npub fn discard(text: String) { String.append(text, \"bb\")\n () }";
    let harness = "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) { (void)message; (void)length; exit(kind == EMELA_DEFECT_STRING_CAPACITY ? 7 : 8); }\nint main(void) { discard((EmelaString){\"aa\", 2, NULL}); return 1; }\n";
    execute(
        compile(source, Options::default().with_string_capacity(3)),
        harness,
        7,
    );
}
#[test]
fn string_rejections_keep_original_interpolation_positions() {
    for source in [
        r#"fn main() { "a" + "b" }"#,
        r#"fn main() -> Int { "42" }"#,
        r#"fn main() { "\u{D800}" }"#,
        r#"fn main() { "\x" }"#,
    ] {
        assert!(emela_emb_compiler::compile(source).is_err(), "{source}");
    }
    let source = r##"fn main() { "é #{missing}" }"##;
    let errors = emela_emb_compiler::compile(source).unwrap_err();
    assert_eq!(&source[errors[0].span.start..errors[0].span.end], "missing");
}

#[test]
fn named_arguments_shorthand_and_tail_calls_preserve_written_evaluation_order() {
    let source = r#"
@external(c, "record")
fn record(value: Int) -> Int
fn compose(a: Int, b: Int, c: Int) -> Int { a * 100 + b * 10 + c }
fn swap(a: Int, b: Int, count: Int) -> Int {
  if count == 0 { a } else { swap(count: count - 1, b: a, a: b) }
}
@export(c, "named")
pub fn named() -> Int {
  c = 3
  assert compose(c: record(3), a: record(1), b: record(2)) == 123
  assert compose(1, b: 2, c:) == 123
  assert (1 |> Int.shift_left(by: 2)) == 4
  swap(1, 2, 1000001)
}
"#;
    execute(
        compile(source, Options::default()),
        &format!(
            "{BOARD}\nstatic int sequence; int32_t record(int32_t value) {{ sequence = sequence * 10 + value; return value; }}\nint main(void) {{ assert(named() == 2); assert(sequence == 312); return 0; }}"
        ),
        0,
    );
    for call in [
        "compose(z: 1, b: 2, c: 3)",
        "compose(1, a: 2, c: 3)",
        "compose(a: 1, 2, 3)",
        "compose(a: 1)",
    ] {
        let source = format!(
            "fn compose(a: Int, b: Int, c: Int) -> Int {{ a + b + c }}\nfn main() -> Int {{ {call} }}"
        );
        assert!(emela_emb_compiler::compile(&source).is_err(), "{source}");
    }
}
