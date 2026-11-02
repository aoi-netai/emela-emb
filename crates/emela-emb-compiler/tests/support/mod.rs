//! Source-to-C execution support shared by compatibility tests.
//!
//! A host C11 compiler is a required test dependency. Both optimization modes
//! always compile, link and execute; missing tools and nonzero exits fail tests.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use emela_emb_compiler::Output;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct TestDirectory(PathBuf);

impl TestDirectory {
    pub fn new() -> Self {
        loop {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("emela-emb-c-test-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("cannot create C test directory: {error}"),
            }
        }
    }

    pub fn path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Compiles all generated C translation units with a caller-supplied C mock.
/// Harnesses own their lifecycle and defect hooks so tests observe the exact ABI.
pub fn compile_and_run(output: &Output, harness: &str) {
    let temp = TestDirectory::new();
    let mut translation_units = Vec::new();
    for file in &output.files {
        let path = temp.path(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create generated file parent");
        }
        fs::write(&path, &file.contents).expect("write generated file");
        if path.extension().is_some_and(|extension| extension == "c") {
            translation_units.push(path);
        }
    }
    assert!(
        !translation_units.is_empty(),
        "compiler generated no C translation units"
    );
    let harness_path = temp.path("harness.c");
    fs::write(&harness_path, harness).expect("write C harness");
    for optimization in ["-O0", "-O2"] {
        let executable = temp.path(format!("probe{}", &optimization[1..]));
        let compilation = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Werror", optimization, "-I"])
            .arg(&temp.0)
            .args(&translation_units)
            .arg(&harness_path)
            .args(["-lm", "-o"])
            .arg(&executable)
            .output()
            .expect("host C compiler 'cc' is required for compatibility tests");
        assert!(
            compilation.status.success(),
            "C compilation failed at {optimization}:\n{}\n{}",
            String::from_utf8_lossy(&compilation.stdout),
            String::from_utf8_lossy(&compilation.stderr)
        );
        let result = Command::new(&executable)
            .output()
            .expect("run generated C executable");
        assert!(
            result.status.success(),
            "generated C failed at {optimization} with {:?}:\n{}\n{}",
            result.status.code(),
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
