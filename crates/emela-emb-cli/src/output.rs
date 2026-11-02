use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use emela_emb_compiler::Output;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

/// Prepare every generated file before making the directory visible at its final path.
pub fn publish(output: &Output, destination: &Path) -> io::Result<()> {
    let destination_name = destination.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path must name a directory, not a filesystem root or '.'/'..'",
        )
    })?;
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "output path already exists and is not a directory",
                ));
            }
            if fs::read_dir(destination)?.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "output directory is not empty; choose a new or empty directory",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staging = StagingDirectory::create(parent, destination_name)?;
    let mut paths = std::collections::HashSet::new();
    for file in &output.files {
        let path = Path::new(&file.path);
        if path.as_os_str().is_empty()
            || !path
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            || !paths.insert(path)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid or duplicate generated file path",
            ));
        }
        let destination = staging.path.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, &file.contents)?;
    }
    // A directory rename cannot replace a nonempty directory, file, or symlink.
    // This also preserves a destination that became nonempty after the check.
    fs::rename(&staging.path, destination)?;
    staging.published = true;
    Ok(())
}

struct StagingDirectory {
    path: PathBuf,
    published: bool,
}

impl StagingDirectory {
    fn create(parent: &Path, destination_name: &OsStr) -> io::Result<Self> {
        for _ in 0..128 {
            let id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
            let name = format!(".emela-emb-{}-{id}", std::process::id());
            if OsStr::new(&name) == destination_name {
                continue;
            }
            let path = parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        published: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot create a unique staging directory",
        ))
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_never_uses_the_final_name_or_leaks_unpublished_files() {
        let temp = StagingDirectory::create(&std::env::temp_dir(), OsStr::new("unused")).unwrap();
        let next_id = NEXT_STAGING_ID.load(Ordering::Relaxed);
        let final_name = format!(".emela-emb-{}-{next_id}", std::process::id());
        let destination = temp.path.join(&final_name);
        let staging = StagingDirectory::create(&temp.path, OsStr::new(&final_name)).unwrap();
        fs::write(staging.path.join("emela_embedded.c"), "incomplete").unwrap();
        assert!(!destination.exists(), "partial output became visible");
        drop(staging);
        assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
    }

    #[test]
    fn generated_paths_cannot_escape_or_publish_partial_output() {
        for invalid in [
            "../escape.c",
            "/escape.c",
            "nested/../escape.c",
            ".",
            "valid.c",
        ] {
            let temp =
                StagingDirectory::create(&std::env::temp_dir(), OsStr::new("unused")).unwrap();
            let destination = temp.path.join("generated");
            let output = Output {
                files: vec![
                    emela_emb_compiler::GeneratedFile {
                        path: "valid.c".into(),
                        contents: "complete".into(),
                    },
                    emela_emb_compiler::GeneratedFile {
                        path: invalid.into(),
                        contents: "invalid".into(),
                    },
                ],
                ..Output::default()
            };
            let error = publish(&output, &destination).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(!destination.exists());
            assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
        }
    }
}
