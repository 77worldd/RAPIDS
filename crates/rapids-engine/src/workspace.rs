use std::path::{Path, PathBuf};

use crate::error::EngineError;
use crate::manifest::LOCK_FILE;

/// The sidecar directory that holds everything belonging to one
/// in-progress download: the manifest, the part file, and the lock.
///
/// Layout for destination `video.mkv`:
///
/// ```text
/// video.mkv.rapids/
/// ├── manifest.json
/// ├── data.part
/// └── lock
/// ```
pub struct Workspace {
    /// The sidecar directory itself.
    pub dir: PathBuf,
    /// The part file every segment writes into.
    pub part_path: PathBuf,
    /// The resume manifest.
    pub manifest_path: PathBuf,
    lock: Option<std::fs::File>,
}

impl Workspace {
    pub fn for_dest(dest: &Path) -> Workspace {
        let mut dir_name = dest.as_os_str().to_os_string();
        dir_name.push(".rapids");
        let dir = PathBuf::from(dir_name);
        let part_path = dir.join(crate::manifest::PART_FILE);
        let manifest_path = dir.join(crate::manifest::MANIFEST_FILE);
        Workspace {
            dir,
            part_path,
            manifest_path,
            lock: None,
        }
    }

    /// True when this workspace holds data from an earlier attempt.
    pub async fn has_history(&self) -> bool {
        tokio::fs::metadata(&self.manifest_path).await.is_ok()
            || tokio::fs::metadata(&self.part_path).await.is_ok()
    }

    /// Creates the sidecar directory and takes the cross-process lock.
    /// The lock is an operating-system file lock: it disappears by itself
    /// when the holding process dies, so a killed download never leaves a
    /// stale lock behind.
    pub async fn create_and_lock(&mut self) -> Result<(), EngineError> {
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|error| EngineError::io("create workspace directory", &self.dir, error))?;

        let dir_for_error = self.dir.clone();
        let lock_path = self.dir.join(LOCK_FILE);
        let result = tokio::task::spawn_blocking(move || -> Result<std::fs::File, EngineError> {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|error| EngineError::io("open workspace lock", &lock_path, error))?;
            match file.try_lock() {
                Ok(()) => Ok(file),
                Err(std::fs::TryLockError::WouldBlock) => Err(EngineError::WorkspaceLocked {
                    path: dir_for_error,
                }),
                Err(std::fs::TryLockError::Error(error)) => {
                    Err(EngineError::io("lock workspace", &lock_path, error))
                }
            }
        })
        .await
        .map_err(|error| EngineError::Io {
            operation: "workspace lock task",
            path: self.dir.clone(),
            reason: error.to_string(),
        })?;

        match result {
            Ok(file) => {
                self.lock = Some(file);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock.is_some()
    }

    /// Removes the part file and manifest, returning the workspace to a
    /// fresh state while keeping the directory and lock.
    pub async fn clear_data(&self) -> Result<(), EngineError> {
        for path in [&self.part_path, &self.manifest_path] {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(EngineError::io("clear workspace", path, error)),
            }
        }
        let temp = self
            .dir
            .join(crate::manifest::MANIFEST_TEMP_FILE);
        let _ = tokio::fs::remove_file(temp).await;
        Ok(())
    }

    /// Drops the lock and removes the whole sidecar directory. Missing
    /// files are not an error — discarding twice stays safe.
    pub async fn discard(&mut self) -> Result<(), EngineError> {
        // Dropping the handle releases the operating-system lock.
        drop(self.lock.take());
        match tokio::fs::remove_dir_all(&self.dir).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(EngineError::io(
                "remove workspace directory",
                &self.dir,
                error,
            )),
        }
    }
}
