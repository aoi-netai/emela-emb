#[path = "../../emela-emb-compiler/tests/support/mod.rs"]
mod c_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("../../../examples/minimal/main.emel");

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("emela-emb-cli-test-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
    }

    fn path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }

    fn input(&self, source: &str) -> PathBuf {
        let path = self.path("main.emel");
        fs::write(&path, source).unwrap();
        path
    }

    fn assert_no_staging_directories(&self) {
        for entry in fs::read_dir(&self.0).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".emela-emb-"),
                "staging directory leaked: {:?}",
                entry.path()
            );
        }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_emela-emb"))
}

fn build(input: &Path, out_dir: &Path) -> Output {
    cli()
        .arg("build")
        .arg(input)
        .arg("--out-dir")
        .arg(out_dir)
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_generated(out_dir: &Path, source: &str) {
    let expected = emela_emb_compiler::compile(source).unwrap();
    for file in &expected.files {
        assert_eq!(
            fs::read_to_string(out_dir.join(&file.path)).unwrap(),
            file.contents
        );
    }
    assert_eq!(fs::read_dir(out_dir).unwrap().count(), expected.files.len());
}

#[test]
fn generates_the_accepted_example_through_the_real_cli() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let out_dir = temp.path("generated");
    let result = build(&input, &out_dir);
    assert_success(&result);
    assert_generated(&out_dir, SOURCE);
    temp.assert_no_staging_directories();
}

#[test]
fn exports_the_requested_name_and_value_instead_of_fixed_output() {
    let temp = TestDirectory::new();
    for number in ["0", "0007", "2147483647"] {
        let source =
            format!("@export(c, \"read_value\")\npub fn calculate() -> Int {{ {number} }}\n");
        let input = temp.input(&source);
        let out_dir = temp.path(format!("generated-{number}"));
        let result = build(&input, &out_dir);
        assert_success(&result);
        assert_generated(&out_dir, &source);
    }
}

#[test]
fn accepts_spaces_tabs_blank_lines_and_crlf_in_supported_positions() {
    let temp = TestDirectory::new();
    let input = temp.input(
        "\r\n@export(\r\n\tc,\r\n\t\"answer\"\r\n)\r\n\r\npub fn answer(\r\n)\r\n  -> Int {\r\n\t42\r\n}\r\n\r\n",
    );
    let out_dir = temp.path("generated");
    assert_success(&build(&input, &out_dir));
    assert_generated(&out_dir, SOURCE);
}

#[test]
fn rejects_bad_inputs_with_a_source_location_and_no_artifacts() {
    let temp = TestDirectory::new();
    let cases = [
        "@export(c, \"answer\")\npub fn answer() -> Int { $ }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Int { 42\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Bool { 42 }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Int { 2147483648 }\n".to_string(),
        "@export(c, \"int32_t\")\npub fn answer() -> Int { 42 }\n".to_string(),
    ];
    for (index, source) in cases.iter().enumerate() {
        let input = temp.input(source);
        let out_dir = temp.path(format!("bad-{index}"));
        let result = build(&input, &out_dir);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            !result.status.success(),
            "accepted invalid source: {source}"
        );
        assert!(
            stderr.contains(&format!("{}:", input.display())),
            "{stderr}"
        );
        assert!(stderr.contains("error["), "{stderr}");
        let location = stderr
            .strip_prefix(&format!("{}:", input.display()))
            .unwrap();
        let mut parts = location.splitn(3, ':');
        assert!(parts.next().unwrap().parse::<usize>().unwrap() > 0);
        assert!(parts.next().unwrap().parse::<usize>().unwrap() > 0);
        assert!(!out_dir.exists(), "artifacts published for invalid source");
    }
    temp.assert_no_staging_directories();
}

#[test]
fn compile_failure_preserves_previous_generated_files() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let out_dir = temp.path("generated");
    assert_success(&build(&input, &out_dir));
    temp.input("@export(c, \"answer\")\npub fn answer() -> Int { 2147483648 }\n");
    let result = build(&input, &out_dir);
    assert!(!result.status.success());
    assert_generated(&out_dir, SOURCE);
    temp.assert_no_staging_directories();
}

#[test]
fn displays_the_exact_line_and_column_for_an_invalid_character() {
    let temp = TestDirectory::new();
    let input = temp.input("@export(c, \"answer\")\r\npub fn answer() -> Int {\r\n  λ\r\n}\r\n");
    let out_dir = temp.path("generated");
    let result = build(&input, &out_dir);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .starts_with(&format!("{}:3:3: error[", input.display()))
    );
    assert!(!out_dir.exists());
}

#[test]
fn refuses_to_overwrite_existing_output_or_unrelated_content() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let out_dir = temp.path("generated");
    assert_success(&build(&input, &out_dir));
    let result = build(&input, &out_dir);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("not empty"));
    assert_generated(&out_dir, SOURCE);

    let unrelated = temp.path("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep.txt"), "keep").unwrap();
    assert!(!build(&input, &unrelated).status.success());
    assert_eq!(
        fs::read_to_string(unrelated.join("keep.txt")).unwrap(),
        "keep"
    );
    assert_eq!(fs::read_dir(&unrelated).unwrap().count(), 1);
    temp.assert_no_staging_directories();
}

#[test]
fn supports_an_existing_empty_output_directory() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let out_dir = temp.path("generated");
    fs::create_dir(&out_dir).unwrap();
    assert_success(&build(&input, &out_dir));
    assert_generated(&out_dir, SOURCE);
}

#[test]
fn reports_read_failures_without_creating_output() {
    let temp = TestDirectory::new();
    let out_dir = temp.path("generated");
    let result = build(&temp.path("missing.emel"), &out_dir);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot read input"));
    assert!(!out_dir.exists());
    let invalid_utf8 = temp.path("invalid.emel");
    fs::write(&invalid_utf8, [0xff]).unwrap();
    let result = build(&invalid_utf8, &out_dir);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot read input"));
    assert!(!out_dir.exists());
    temp.assert_no_staging_directories();
}

#[test]
fn reports_save_failures_without_partial_artifacts() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let blocker = temp.path("blocker");
    fs::write(&blocker, "keep").unwrap();
    for out_dir in [&blocker, &blocker.join("generated")] {
        let result = build(&input, out_dir);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("cannot save generated files"));
        assert_eq!(fs::read_to_string(&blocker).unwrap(), "keep");
    }
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2);
    temp.assert_no_staging_directories();
}

#[test]
fn requires_explicit_unambiguous_arguments_and_has_help() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["build"],
        vec!["build", "main.emel"],
        vec!["build", "main.emel", "--out-dir"],
        vec!["build", "main.emel", "--out-dir", "one", "--out-dir", "two"],
        vec!["build", "main.emel", "--unknown", "--out-dir", "generated"],
    ] {
        let result = cli().args(&args).output().unwrap();
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("Usage:"));
    }
    for args in [vec!["--help"], vec!["-h"], vec!["build", "--help"]] {
        let result = cli().args(&args).output().unwrap();
        assert_success(&result);
        assert!(String::from_utf8_lossy(&result.stdout).contains("Usage:"));
    }
}

#[test]
fn accepts_an_option_before_the_input_and_a_dash_prefixed_path_after_double_dash() {
    let temp = TestDirectory::new();
    fs::write(temp.path("-input.emel"), SOURCE).unwrap();
    let result = cli()
        .current_dir(&temp.0)
        .args(["build", "--out-dir", "generated", "--", "-input.emel"])
        .output()
        .unwrap();
    assert_success(&result);
    assert_generated(&temp.path("generated"), SOURCE);
}

#[cfg(unix)]
#[test]
fn preserves_non_utf8_input_and_output_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let temp = TestDirectory::new();
    let input = temp.path(OsString::from_vec(b"input-\xff.emel".to_vec()));
    let out_dir = temp.path(OsString::from_vec(b"generated-\xff".to_vec()));
    fs::write(&input, SOURCE).unwrap();
    assert_success(&build(&input, &out_dir));
    assert_generated(&out_dir, SOURCE);
}

#[cfg(unix)]
#[test]
fn refuses_an_output_symlink_without_modifying_its_target() {
    use std::os::unix::fs::symlink;
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let real = temp.path("real");
    fs::create_dir(&real).unwrap();
    let out_dir = temp.path("generated");
    symlink(&real, &out_dir).unwrap();
    assert!(!build(&input, &out_dir).status.success());
    assert!(
        fs::symlink_metadata(&out_dir)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_dir(&real).unwrap().count(), 0);
    temp.assert_no_staging_directories();
}

#[test]
fn multiple_inputs_entry_and_options_generate_executable_c_and_all_artifacts() {
    let temp = TestDirectory::new();
    let main = temp.input(include_str!(
        "../../emela-emb-compiler/tests/compat/p1/modules-main.emel"
    ));
    let math = temp.path("math.emel");
    fs::write(
        &math,
        include_str!("../../emela-emb-compiler/tests/compat/p1/math.emel"),
    )
    .unwrap();
    let out_dir = temp.path("generated");
    let result = cli()
        .arg("build")
        .arg(&math)
        .arg(&main)
        .arg("--entry")
        .arg(&main)
        .args([
            "--entry-function",
            "answer",
            "--heap-bytes",
            "0",
            "--string-capacity",
            "32",
        ])
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert_success(&result);
    for path in [
        "emela_embedded.c",
        "emela_embedded.h",
        "emela_board.h",
        "emela_artifact.json",
    ] {
        assert!(out_dir.join(path).is_file(), "CLI did not save {path}");
    }
    let harness = temp.path("harness.c");
    fs::write(
        &harness,
        include_str!("../../emela-emb-compiler/tests/compat/p1/modules-harness.c"),
    )
    .unwrap();
    let mut translation_units = fs::read_dir(&out_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "c"))
        .collect::<Vec<_>>();
    translation_units.sort();
    for optimization in ["-O0", "-O2"] {
        let executable = temp.path(format!("cli-probe{}", &optimization[1..]));
        let compile = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Werror", optimization, "-I"])
            .arg(&out_dir)
            .args(&translation_units)
            .arg(&harness)
            .args(["-lm", "-o"])
            .arg(&executable)
            .output()
            .expect("cc is required for CLI integration tests");
        assert!(
            compile.status.success(),
            "CLI-generated C failed at {optimization}: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&executable)
            .output()
            .expect("execute CLI-generated C");
        assert!(
            run.status.success(),
            "CLI-generated C failed at {optimization}: {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
    temp.assert_no_staging_directories();
}

#[test]
fn reports_imported_source_locations_and_preserves_atomic_output_on_errors() {
    let temp = TestDirectory::new();
    let main = temp.input(
        "import Math.{invalid}\n@export(c, \"answer\")\npub fn answer() -> Int { invalid() }",
    );
    let math = temp.path("math.emel");
    fs::write(&math, "pub fn invalid() -> Int { missing }\n").unwrap();
    let out_dir = temp.path("generated");
    let result = cli()
        .arg("build")
        .arg(&main)
        .arg(&math)
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).starts_with(&format!("{}:1:", math.display())));
    assert!(!out_dir.exists());
    temp.assert_no_staging_directories();
}

#[test]
fn rejects_malformed_duplicate_and_out_of_range_options() {
    for options in [
        vec!["--entry"],
        vec!["--entry", ""],
        vec!["--entry", "a", "--entry", "b"],
        vec!["--entry-function", "run", "--entry-function", "other"],
        vec!["--heap-bytes", "-1"],
        vec!["--heap-bytes", "4294967296"],
        vec!["--heap-bytes", "1", "--heap-bytes", "2"],
        vec!["--string-capacity", "65536"],
        vec!["--string-capacity", "12.5"],
        vec!["--frame-capacity", "65536"],
        vec!["--frame-bytes", "-1"],
        vec!["--fiber-capacity", "1", "--fiber-capacity", "2"],
        vec!["--task-capacity", "1", "--task-capacity", "2"],
        vec!["--event-capacity", "1.5"],
        vec!["--task-adapter"],
        vec!["--task-adapter", "wait"],
        vec!["--task-adapter", "wait="],
        vec!["--task-adapter", "=board_wait"],
        vec!["--task-adapter", "wait=board_wait=extra"],
        vec!["--task-adapter", "wait=not-a-symbol"],
        vec![
            "--task-adapter",
            "wait=board_wait",
            "--task-adapter",
            "wait=other",
        ],
    ] {
        let result = cli()
            .args(["build", "main.emel", "--out-dir", "generated"])
            .args(&options)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{options:?}");
    }
}

#[test]
fn cli_string_capacity_and_caller_output_buffers_execute() {
    let temp = TestDirectory::new();
    let input = temp.input(include_str!(
        "../../emela-emb-compiler/tests/compat/p2/append.emel"
    ));
    let out_dir = temp.path("generated");
    let result = cli()
        .arg("build")
        .arg(input)
        .args(["--string-capacity", "32", "--out-dir"])
        .arg(&out_dir)
        .output()
        .unwrap();
    assert_success(&result);
    let files = fs::read_dir(&out_dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            emela_emb_compiler::GeneratedFile {
                path: entry.file_name().to_str().unwrap().to_owned(),
                contents: fs::read_to_string(entry.path()).unwrap(),
            }
        })
        .collect();
    let output = emela_emb_compiler::Output {
        files,
        ..emela_emb_compiler::Output::default()
    };
    c_support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)m; exit(k == EMELA_DEFECT_STRING_CAPACITY && n > 0 ? 0 : 5); }
int main(void) {
  const char input[] = {'a', 0, 'b'};
  char output[64];
  EmelaString value = message((EmelaString){input, 3, NULL}, output, 64);
  assert(value.data == output && value.length == 18 && !value.owner);
  assert(memcmp(value.data, "Missing(text: a\0b)", 18) == 0);
  const char too_long[] = "01234567890123456789012345678901";
  (void)message((EmelaString){too_long, sizeof too_long - 1, NULL}, output, 64);
  return 2;
}
"#,
    );
    temp.assert_no_staging_directories();
}

#[test]
fn cli_task_adapter_and_fixed_pools_generate_a_resumable_c_program() {
    let temp = TestDirectory::new();
    let input = temp.input(
        r#"
effect Wait{suspend fn get(value:Int)->Int}
@external(c,"wait")
suspend fn wait(value:Int)->Int
handler Host:Wait{suspend fn get(value,resume){resume(wait(value))}}
@export(c,"answer")
pub fn run()->Int{with Host{(use Wait).get(40)+2}}
"#,
    );
    let out_dir = temp.path("generated");
    let options = [
        "--heap-bytes",
        "16384",
        "--frame-bytes",
        "1024",
        "--fiber-capacity",
        "2",
        "--frame-capacity",
        "16",
        "--task-capacity",
        "4",
        "--event-capacity",
        "4",
    ];
    let missing = cli()
        .arg("build")
        .arg(&input)
        .args(options)
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("with_task_adapter"));
    assert!(!out_dir.exists());
    let result = cli()
        .arg("build")
        .arg(&input)
        .args(options)
        .args(["--task-adapter", "wait=board_wait", "--out-dir"])
        .arg(&out_dir)
        .output()
        .unwrap();
    assert_success(&result);
    let files: Vec<_> = fs::read_dir(&out_dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            emela_emb_compiler::GeneratedFile {
                path: entry.file_name().to_str().unwrap().to_owned(),
                contents: fs::read_to_string(entry.path()).unwrap(),
            }
        })
        .collect();
    let source = files
        .iter()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap();
    for declaration in [
        "#define EMELA_FIBER_CAPACITY 2",
        "#define EMELA_FRAME_CAPACITY 16",
        "#define EMELA_FRAME_BYTES 1024",
        "#define EMELA_TASK_CAPACITY 4",
        "#define EMELA_EVENT_CAPACITY 4",
    ] {
        assert!(source.contents.contains(declaration), "{declaration}");
    }
    let output = emela_emb_compiler::Output {
        files,
        ..emela_emb_compiler::Output::default()
    };
    c_support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaTask pending;
static int32_t value;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
void board_wait(EmelaTask task,int32_t n){pending=task;value=n;}
int main(void){for(int i=0;i<1000;i++){
 int32_t result=0;EmelaFiber fiber=answer_start(&result);
 while(emela_scheduler_run(1)){}
 assert(value==40&&!emela_fiber_done(fiber)&&result==0);
 assert(emela_task_complete_int(pending,value));
 assert(!emela_task_complete_int(pending,7)&&!emela_fiber_done(fiber));
 while(!emela_fiber_done(fiber))assert(emela_scheduler_run(1));
 assert(result==42);emela_fiber_release(fiber);
 assert(!emela_task_complete_int(pending,8));
}return 0;}
"#,
    );
    temp.assert_no_staging_directories();
}
