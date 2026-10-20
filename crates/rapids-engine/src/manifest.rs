use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

pub const MANIFEST_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const MANIFEST_TEMP_FILE: &str = "manifest.json.tmp";
pub const PART_FILE: &str = "data.part";
pub const LOCK_FILE: &str = "lock";

/// Resume bookkeeping for one segment. `written` is the number of bytes
/// confirmed flushed for this segment — always a conservative low-water mark,
/// so a stale manifest can only cause a re-fetch, never corruption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentState {
    #[serde(rename = "i")]
    pub index: u32,
    pub start: u64,
    /// Inclusive end offset.
    pub end: u64,
    pub written: u64,
}

impl SegmentState {
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// The sidecar JSON file kept next to every in-flight download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub url: String,
    pub final_url: String,
    pub dest: PathBuf,
    /// `None` when the server never reported a length.
    pub total_bytes: Option<u64>,
    pub accepts_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub filename: Option<String>,
    pub expected_sha256: Option<String>,
    pub observed_sha256: Option<String>,
    pub created_at: u64,
    pub segments: Vec<SegmentState>,
    /// Set immediately before the part file is renamed into place, so an
    /// interrupted finalization can be finished by the next run.
    pub completed: bool,
}

impl Manifest {
    /// Loads the sidecar manifest. A missing or unreadable file yields
    /// `None`, which sends the caller down the fresh-download path.
    pub async fn load(dir: &Path) -> Option<Manifest> {
        let path = dir.join(MANIFEST_FILE);
        let bytes = tokio::fs::read(path).await.ok()?;
        let manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
        (manifest.v == MANIFEST_VERSION).then_some(manifest)
    }

    /// Writes the manifest through a temporary file plus atomic rename.
    pub async fn save_atomic(&self, dir: &Path) -> Result<(), EngineError> {
        let temp = dir.join(MANIFEST_TEMP_FILE);
        let final_path = dir.join(MANIFEST_FILE);
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| EngineError::Io {
            operation: "encode manifest",
            path: final_path.clone(),
            reason: error.to_string(),
        })?;
        tokio::fs::write(&temp, &bytes)
            .await
            .map_err(|error| EngineError::io("write manifest", &temp, error))?;
        tokio::fs::rename(&temp, &final_path)
            .await
            .map_err(|error| EngineError::io("commit manifest", &final_path, error))?;
        Ok(())
    }

    /// Resume gate: a stored download may only continue when it recorded
    /// range support, a size, and a validator that the server still agrees
    /// with. Anything else restarts from zero.
    pub fn can_resume(
        &self,
        fresh_total: Option<u64>,
        fresh_etag: Option<&str>,
        fresh_last_modified: Option<&str>,
    ) -> bool {
        if !self.accepts_ranges {
            return false;
        }
        let stored_total = match self.total_bytes {
            Some(value) => value,
            None => return false,
        };
        match fresh_total {
            Some(value) if value == stored_total => {}
            _ => return false,
        }
        if let Some(stored) = self.etag.as_deref() {
            return fresh_etag == Some(stored);
        }
        if let Some(stored) = self.last_modified.as_deref() {
            return fresh_last_modified == Some(stored);
        }
        false
    }

    /// Progress checkpoint: copies the in-memory flushed offsets into the
    /// manifest without touching anything else.
    pub fn record_progress(&mut self, written: &[u64]) {
        for (position, segment) in self.segments.iter_mut().enumerate() {
            if let Some(value) = written.get(position) {
                segment.written = *value;
            }
        }
    }

    pub fn is_fully_written(&self) -> bool {
        match self.total_bytes {
            None => false,
            Some(total) => {
                if total == 0 {
                    return true;
                }
                if self.accepts_ranges {
                    self.segments
                        .iter()
                        .all(|segment| segment.written >= segment.size())
                } else {
                    self.segments
                        .first()
                        .is_some_and(|segment| segment.written >= total)
                }
            }
        }
    }
}

pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
