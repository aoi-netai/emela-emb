use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = include_str!("../../../examples/minimal/main.emel");
const EXPECTED_C: &str =
    "#include \"emela_embedded.h\"\n\nint32_t answer(void) {\n    return 42;\n}\n";
const EXPECTED_HEADER: &str = "#ifndef EMELA_EMBEDDED_H\n#define EMELA_EMBEDDED_H\n\n#include <stdint.h>\n\nint32_t answer(void);\n\n#endif\n";

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

fn assert_generated(out_dir: &Path, c: &str, header: &str) {
    assert_eq!(
        fs::read_to_string(out_dir.join("emela_embedded.c")).unwrap(),
        c
    );
    assert_eq!(
        fs::read_to_string(out_dir.join("emela_embedded.h")).unwrap(),
        header
    );
    assert_eq!(fs::read_dir(out_dir).unwrap().count(), 2);
}

#[test]
fn generates_the_accepted_example_through_the_real_cli() {
    let temp = TestDirectory::new();
    let input = temp.input(SOURCE);
    let out_dir = temp.path("generated");
    let result = build(&input, &out_dir);
    assert_success(&result);
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);
    temp.assert_no_staging_directories();
}

#[test]
fn exports_the_requested_name_and_value_instead_of_fixed_output() {
    let temp = TestDirectory::new();
    for (number, expected) in [("0", "0"), ("0007", "7"), ("2147483647", "2147483647")] {
        let input = temp.input(&format!(
            "@export(c, \"read_value\")\npub fn calculate() -> Int {{ {number} }}\n"
        ));
        let out_dir = temp.path(format!("generated-{number}"));
        let result = build(&input, &out_dir);
        assert_success(&result);
        assert_generated(
            &out_dir,
            &EXPECTED_C
                .replace("answer", "read_value")
                .replace("42", expected),
            &EXPECTED_HEADER.replace("answer", "read_value"),
        );
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
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);
}

#[test]
fn rejects_bad_inputs_with_a_source_location_and_no_artifacts() {
    let temp = TestDirectory::new();
    let cases = [
        "@export(c, \"answer\")\npub fn answer() -> Int { $ }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Int { 42\n".to_string(),
        format!("{SOURCE}pub fn extra() -> Int {{ 1 }}\n"),
        "@export(c, \"answer\")\npub fn answer() -> Bool { 42 }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Int { 2147483648 }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer() -> Int { 1 + 2 }\n".to_string(),
        "@export(c, \"answer\")\npub fn answer(value: Int) -> Int { 42 }\n".to_string(),
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
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);
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
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);

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
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);
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
        vec!["build", "main.emel", "extra.emel", "--out-dir", "generated"],
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
    assert_generated(&temp.path("generated"), EXPECTED_C, EXPECTED_HEADER);
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
    assert_generated(&out_dir, EXPECTED_C, EXPECTED_HEADER);
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
