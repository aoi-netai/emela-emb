use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use emela_emb_compiler::Output;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

/// Prepare both files before making the directory visible at its final path.
pub fn publish(output: &Output, destination: &Path) -> io::Result<()> {
    if destination.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path must name a directory, not a filesystem root or '.'/'..'",
        ));
    }
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
    let mut staging = StagingDirectory::create(parent)?;
    fs::write(staging.path.join("emela_embedded.c"), &output.c_source)?;
    fs::write(staging.path.join("emela_embedded.h"), &output.header)?;
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
    fn create(parent: &Path) -> io::Result<Self> {
        for _ in 0..128 {
            let id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".emela-emb-{}-{id}", std::process::id()));
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
