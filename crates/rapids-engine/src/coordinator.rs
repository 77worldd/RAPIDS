use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::events::{DownloadState, EngineEvent};
use crate::http::fetch::{
    Control, CoordMsg, DownloadContext, WorkQueue, apply_control, worker_loop,
};
use crate::http::{build_client, probe};
use crate::manifest::{Manifest, SegmentState, unix_timestamp};
use crate::rate::RateLimits;
use crate::segmenter::{ScaleDecision, Segment, scaling_decision, tile};
use crate::workspace::Workspace;

/// Everything needed to start one download.
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    /// Explicit destination path. `None` lets the engine pick the name:
    /// server-provided filename first, then the URL's last segment.
    pub dest: Option<PathBuf>,
    pub expected_sha256: Option<String>,
    /// Pins this task's worker plan (scaling off); `None` runs the default
    /// 8 → 32 dynamic behaviour. A shared budget may allow fewer requests.
    pub connections: Option<u8>,
    pub overwrite: bool,
}

struct HandleInner {
    control: Arc<watch::Sender<Control>>,
    state: watch::Receiver<DownloadState>,
    finished: watch::Receiver<bool>,
}

/// Live control surface for a running download.
#[derive(Clone)]
pub struct DownloadHandle {
    inner: Arc<HandleInner>,
}

impl DownloadHandle {
    pub fn state(&self) -> DownloadState {
        *self.inner.state.borrow()
    }

    /// Requests a pause and waits until the engine confirms it.
    pub async fn pause(&self) {
        apply_control(&self.inner.control, |flags| flags.paused = true);
        self.wait_for(DownloadState::Paused).await;
    }

    /// Lifts a pause and waits until transferring resumes.
    pub async fn resume(&self) {
        apply_control(&self.inner.control, |flags| flags.paused = false);
        self.wait_for(DownloadState::Downloading).await;
    }

    /// Aborts the download and waits for the terminal event.
    pub async fn cancel(&self) {
        apply_control(&self.inner.control, |flags| {
            flags.cancelled = true;
            flags.paused = false;
        });
        self.wait_for(DownloadState::Cancelled).await;
    }

    /// Ends this run without discarding its part file. Once transfer has
    /// begun, workers finish their writes and the manifest is checkpointed.
    /// The workspace lock is released before this returns. A later run
    /// rechecks the sidecar's size, range support, and validator before
    /// trusting any saved bytes.
    pub async fn stop(&self) {
        if !self.state().is_terminal() {
            apply_control(&self.inner.control, |flags| {
                flags.stopping = true;
                flags.paused = false;
            });
        }
        // A natural completion or failure can race this request. Waiting for
        // the task itself, rather than merely a terminal state, also covers
        // the time between a terminal event and the lock being dropped.
        let mut finished = self.inner.finished.clone();
        loop {
            if *finished.borrow() {
                return;
            }
            if finished.changed().await.is_err() {
                return;
            }
        }
    }

    async fn wait_for(&self, target: DownloadState) {
        let mut state = self.inner.state.clone();
        loop {
            let current = *state.borrow();
            if current == target || current.is_terminal() {
                return;
            }
            if state.changed().await.is_err() {
                return;
            }
        }
    }
}

/// Validates the request synchronously (no side effects), then spawns the
/// coordinator task. The returned receiver streams the download's events;
/// drop it (or call `cancel`) to let a download run unobserved.
pub fn spawn_download(
    spec: DownloadSpec,
    config: EngineConfig,
) -> Result<(DownloadHandle, mpsc::Receiver<EngineEvent>), EngineError> {
    let mut config = config.validated()?;
    if let Some(connections) = spec.connections {
        config.start_connections = connections;
        config.max_connections = connections;
    }
    reqwest::Url::parse(&spec.url).map_err(|error| EngineError::InvalidUrl {
        url: spec.url.clone(),
        reason: error.to_string(),
    })?;
    if let Some(expected) = &spec.expected_sha256 {
        let expected = expected.trim();
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(EngineError::invalid_spec(
                "SHA-256 must be exactly 64 hex characters",
            ));
        }
    }
    if let Some(connections) = spec.connections
        && !(1..=32).contains(&connections)
    {
        return Err(EngineError::invalid_spec(
            "connections must be between 1 and 32",
        ));
    }
    if let Some(dest) = &spec.dest {
        if dest.is_dir() {
            return Err(EngineError::invalid_spec(format!(
                "destination {} is a directory",
                dest.display()
            )));
        }
        if dest.exists() && !spec.overwrite {
            return Err(EngineError::DestinationExists { path: dest.clone() });
        }
    }

    let (event_tx, event_rx) = mpsc::channel(64);
    let (control_tx, control_rx) = watch::channel(Control::default());
    let control_tx = Arc::new(control_tx);
    let (state_tx, state_rx) = watch::channel(DownloadState::Probing);
    let (finished_tx, finished_rx) = watch::channel(false);
    let handle = DownloadHandle {
        inner: Arc::new(HandleInner {
            control: control_tx.clone(),
            state: state_rx,
            finished: finished_rx,
        }),
    };
    let coordinator = Coordinator {
        spec,
        config,
        control: control_tx,
        control_rx,
        state_tx,
        event_tx,
    };
    tokio::spawn(async move {
        coordinator.run().await;
        let _ = finished_tx.send_replace(true);
    });
    Ok((handle, event_rx))
}

enum Settle {
    Finished,
    StartFresh,
}

/// Everything the transfer loop needs once preparation finished. The HTTP
/// client built for the probe is handed over instead of built twice: one
/// pool, one set of timeouts.
struct Prepared {
    probe: probe::ProbeResult,
    workspace: Workspace,
    stored: Option<Manifest>,
    client: reqwest::Client,
}

/// One tick of the coordinator's select loop.
enum IoTurn {
    Message(CoordMsg),
    ChannelClosed,
    Progress,
    Scale,
    Checkpoint,
    ControlWoken,
}

struct Coordinator {
    spec: DownloadSpec,
    config: EngineConfig,
    control: Arc<watch::Sender<Control>>,
    control_rx: watch::Receiver<Control>,
    state_tx: watch::Sender<DownloadState>,
    event_tx: mpsc::Sender<EngineEvent>,
}

impl Coordinator {
    async fn transition(&self, state: DownloadState) {
        self.state_tx.send_replace(state);
        let _ = self
            .event_tx
            .send(EngineEvent::StateChanged { state })
            .await;
    }

    async fn emit(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event).await;
    }

    async fn fail(&self, error: EngineError) {
        // The classification happens here, where the concrete error is still
        // in hand, so no caller ever has to infer it from the message.
        let retryable = error.is_retryable();
        apply_control(&self.control, |flags| flags.aborted = true);
        self.transition(DownloadState::Failed).await;
        self.emit(EngineEvent::Failed {
            message: error.to_string(),
            retryable,
        })
        .await;
    }

    async fn complete(&self, path: PathBuf, sha256: Option<String>) {
        self.transition(DownloadState::Completed).await;
        self.emit(EngineEvent::Completed { path, sha256 }).await;
    }

    async fn cancel_now(&self, workspace: Option<&mut Workspace>) {
        let result = match workspace {
            Some(workspace) => workspace.discard().await,
            None => Ok(()),
        };
        match result {
            Ok(()) => {
                self.transition(DownloadState::Cancelled).await;
                self.emit(EngineEvent::Cancelled).await;
            }
            Err(error) => self.fail(error).await,
        }
    }

    async fn stop_now(&self, workspace: Option<Workspace>) {
        // Pausing leaves this file handle locked. Stop must drop it before
        // telling the caller the run is over, without removing the sidecar.
        drop(workspace);
        self.transition(DownloadState::Stopped).await;
        self.emit(EngineEvent::Stopped).await;
    }

    async fn checkpoint(
        &self,
        dir: &Path,
        manifest: &mut Manifest,
        progress: &[AtomicU64],
    ) -> Result<(), EngineError> {
        let written: Vec<u64> = progress
            .iter()
            .map(|atomic| atomic.load(Ordering::Acquire))
            .collect();
        manifest.record_progress(&written);
        manifest.save_atomic(dir).await
    }

    async fn create_part(
        &self,
        workspace: &Workspace,
        manifest: &Manifest,
    ) -> Result<(), EngineError> {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&workspace.part_path)
            .await
            .map_err(|error| EngineError::io("create part file", &workspace.part_path, error))?;
        if let Some(total) = manifest.total_bytes {
            file.set_len(total)
                .await
                .map_err(|error| EngineError::io("size part file", &workspace.part_path, error))?;
        }
        file.flush()
            .await
            .map_err(|error| EngineError::io("flush part file", &workspace.part_path, error))?;
        Ok(())
    }

    /// Settles a manifest that was marked complete by an interrupted
    /// previous run.
    async fn settle_completed(
        &mut self,
        workspace: &mut Workspace,
        mut existing: Manifest,
    ) -> Result<Settle, EngineError> {
        let part_exists = tokio::fs::metadata(&workspace.part_path).await.is_ok();
        if part_exists {
            let (path, sha256) = self.finalize(workspace, &mut existing).await?;
            self.complete(path, sha256).await;
            return Ok(Settle::Finished);
        }
        let dest_exists = tokio::fs::metadata(&existing.dest).await.is_ok();
        if dest_exists {
            let path = existing.dest.clone();
            let sha256 = existing.observed_sha256.clone();
            workspace.discard().await?;
            self.complete(path, sha256).await;
            return Ok(Settle::Finished);
        }
        // Neither the part file nor the destination survived: start over.
        workspace.clear_data().await?;
        Ok(Settle::StartFresh)
    }

    /// If the stored manifest belongs to a finished-but-unpublished
    /// download, settle it now. Returns whether the task is complete.
    async fn settle_if_completed(
        &mut self,
        workspace: &mut Workspace,
        stored: &mut Option<Manifest>,
    ) -> Result<Settle, EngineError> {
        let Some(existing) = stored.clone() else {
            return Ok(Settle::StartFresh);
        };
        if !existing.completed {
            return Ok(Settle::StartFresh);
        }
        match self.settle_completed(workspace, existing).await {
            Ok(Settle::Finished) => Ok(Settle::Finished),
            Ok(Settle::StartFresh) => {
                *stored = None;
                Ok(Settle::StartFresh)
            }
            Err(error) => Err(error),
        }
    }

    /// Sync, hash, verify, publish. The part file is only renamed after the
    /// manifest is durably marked complete, so any interruption lands on a
    /// recoverable state.
    async fn finalize(
        &mut self,
        workspace: &mut Workspace,
        manifest: &mut Manifest,
    ) -> Result<(PathBuf, Option<String>), EngineError> {
        self.transition(DownloadState::Verifying).await;

        let sync_path = workspace.part_path.clone();
        match tokio::task::spawn_blocking(move || -> Result<(), EngineError> {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&sync_path)
                .map_err(|error| EngineError::io("open part file for sync", &sync_path, error))?;
            file.sync_all()
                .map_err(|error| EngineError::io("sync part file", &sync_path, error))
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error),
            Err(error) => {
                return Err(EngineError::Io {
                    operation: "sync part file",
                    path: workspace.part_path.clone(),
                    reason: error.to_string(),
                });
            }
        }

        let hash_path = workspace.part_path.clone();
        let hash_task = tokio::task::spawn_blocking(move || crate::hash::sha256_file(&hash_path));
        let observed = match hash_task.await {
            Ok(Ok(hash)) => hash,
            Ok(Err(error)) => return Err(error),
            Err(error) => {
                return Err(EngineError::Io {
                    operation: "hash part file",
                    path: workspace.part_path.clone(),
                    reason: error.to_string(),
                });
            }
        };
        manifest.observed_sha256 = Some(observed.clone());

        if let Some(expected) = manifest.expected_sha256.as_deref()
            && !observed.eq_ignore_ascii_case(expected)
        {
            manifest.completed = false;
            manifest.save_atomic(&workspace.dir).await?;
            return Err(EngineError::VerificationFailed {
                expected: expected.to_string(),
                actual: observed,
            });
        }

        if !self.spec.overwrite && tokio::fs::metadata(&manifest.dest).await.is_ok() {
            return Err(EngineError::DestinationExists {
                path: manifest.dest.clone(),
            });
        }

        manifest.completed = true;
        manifest.save_atomic(&workspace.dir).await?;

        let dest = manifest.dest.clone();
        tokio::fs::rename(&workspace.part_path, &dest)
            .await
            .map_err(|error| EngineError::io("publish download", &dest, error))?;

        // The bytes are final at this point; a leftover sidecar directory
        // only needs `rapids cancel <dest>` if the filesystem refuses the
        // cleanup.
        if let Err(error) = workspace.discard().await {
            eprintln!("rapids: cleanup after completion: {error}");
        }
        Ok((dest, Some(observed)))
    }

    /// Everything that must happen before transfers: destination naming,
    /// workspace lock, settlement of an interrupted finalization, and the
    /// server probe. Returning `None` means a terminal path already
    /// emitted its events (success, failure, or cancellation).
    async fn prepare(&mut self) -> Option<Prepared> {
        let mut workspace: Option<Workspace> = None;
        let mut stored: Option<Manifest> = None;

        // Explicit destination: settle the previous run first (works even
        // when the server is unreachable), then probe.
        if let Some(dest) = self.spec.dest.clone() {
            if let Err(error) = ensure_destination_parent(&dest).await {
                self.fail(error).await;
                return None;
            }
            let mut created = Workspace::for_dest(&dest);
            if let Err(error) = created.create_and_lock().await {
                self.fail(error).await;
                return None;
            }
            stored = Manifest::load(&created.dir).await;
            match self.settle_if_completed(&mut created, &mut stored).await {
                Ok(Settle::Finished) => return None,
                Ok(Settle::StartFresh) => {}
                Err(error) => {
                    self.fail(error).await;
                    return None;
                }
            }
            workspace = Some(created);
        }

        // A stop that arrives during workspace setup must not begin a probe.
        let flags = *self.control_rx.borrow();
        if flags.cancelled {
            self.cancel_now(workspace.as_mut()).await;
            return None;
        }
        if flags.stopping {
            self.stop_now(workspace.take()).await;
            return None;
        }

        // --- probe the server (cancel- and stop-aware) ------------------
        let client = match build_client(&self.config) {
            Ok(client) => client,
            Err(error) => {
                self.fail(error).await;
                return None;
            }
        };
        // The probe future borrows `self.spec` and `self.config`, so it lives
        // in its own scope: those borrows end here, before the destination
        // branch below writes to `self.spec`. A stop also drops a probe that
        // is waiting for a connection budget lease.
        let probe_result = {
            let mut wake = self.control_rx.clone();
            let mut probe_fut = Box::pin(probe::probe(&self.spec.url, &self.config, &client));
            loop {
                tokio::select! {
                    result = &mut probe_fut => break result,
                    changed = wake.changed() => {
                        let flags = *self.control_rx.borrow();
                        if changed.is_err() || flags.cancelled {
                            // Returning here drops `probe_fut`, which cancels
                            // the request.
                            self.cancel_now(workspace.as_mut()).await;
                            return None;
                        }
                        if flags.stopping {
                            self.stop_now(workspace.take()).await;
                            return None;
                        }
                    }
                }
            }
        };
        let flags = *self.control_rx.borrow();
        if flags.cancelled {
            self.cancel_now(workspace.as_mut()).await;
            return None;
        }
        if flags.stopping {
            self.stop_now(workspace.take()).await;
            return None;
        }
        let probe = match probe_result {
            Ok(result) => result,
            Err(error) => {
                self.fail(error).await;
                return None;
            }
        };

        // Default destination: server-provided filename, then the URL's
        // last meaningful segment.
        if self.spec.dest.is_none() {
            let filename = match probe.filename.clone() {
                Some(filename) => filename,
                None => {
                    self.fail(EngineError::invalid_spec(
                        "no destination given and the URL does not provide a filename",
                    ))
                    .await;
                    return None;
                }
            };
            let current_dir = match std::env::current_dir() {
                Ok(dir) => dir,
                Err(error) => {
                    self.fail(EngineError::io(
                        "read current directory",
                        Path::new("."),
                        error,
                    ))
                    .await;
                    return None;
                }
            };
            let dest = current_dir.join(filename);
            if dest.is_dir() {
                self.fail(EngineError::invalid_spec(format!(
                    "destination {} is a directory",
                    dest.display()
                )))
                .await;
                return None;
            }
            if dest.exists() && !self.spec.overwrite {
                self.fail(EngineError::DestinationExists { path: dest.clone() })
                    .await;
                return None;
            }
            if let Err(error) = ensure_destination_parent(&dest).await {
                self.fail(error).await;
                return None;
            }
            let mut created = Workspace::for_dest(&dest);
            if let Err(error) = created.create_and_lock().await {
                self.fail(error).await;
                return None;
            }
            stored = Manifest::load(&created.dir).await;
            match self.settle_if_completed(&mut created, &mut stored).await {
                Ok(Settle::Finished) => return None,
                Ok(Settle::StartFresh) => {}
                Err(error) => {
                    self.fail(error).await;
                    return None;
                }
            }
            self.spec.dest = Some(dest);
            workspace = Some(created);
        }

        let workspace = workspace?;
        Some(Prepared {
            probe,
            workspace,
            stored,
            client,
        })
    }

    async fn run(mut self) {
        self.transition(DownloadState::Probing).await;

        let Some(prepared) = self.prepare().await else {
            return;
        };
        let Prepared {
            probe,
            mut workspace,
            stored,
            client,
        } = prepared;
        let flags = *self.control_rx.borrow();
        if flags.cancelled {
            self.cancel_now(Some(&mut workspace)).await;
            return;
        }
        if flags.stopping {
            self.stop_now(Some(workspace)).await;
            return;
        }
        let Some(dest) = self.spec.dest.clone() else {
            self.fail(EngineError::protocol("destination was not resolved"))
                .await;
            return;
        };

        // --- plan: resume what is verifiably resumable, else restart ----
        // The part file must exist and match the fresh size exactly;
        // anything less risks sparse zeros where a segment assumed data.
        let part_matches_size = match (tokio::fs::metadata(&workspace.part_path).await, probe.total)
        {
            (Ok(metadata), Some(total)) => metadata.len() == total,
            _ => false,
        };
        let resumable = stored.as_ref().is_some_and(|existing| {
            !existing.completed
                && existing.can_resume(
                    probe.total,
                    probe.etag.as_deref(),
                    probe.last_modified.as_deref(),
                )
                && part_matches_size
        });
        let mut manifest = match stored {
            Some(existing) if resumable => {
                let mut resumed = existing;
                resumed.final_url = probe.final_url.clone();
                if resumed.content_type.is_none() {
                    resumed.content_type = probe.content_type.clone();
                }
                if resumed.filename.is_none() {
                    resumed.filename = probe.filename.clone();
                }
                resumed
            }
            _ => {
                if let Err(error) = workspace.clear_data().await {
                    self.fail(error).await;
                    return;
                }
                let fresh = build_manifest(&self.spec, probe, &self.config, &dest);
                if let Err(error) = self.create_part(&workspace, &fresh).await {
                    self.fail(error).await;
                    return;
                }
                if let Err(error) = fresh.save_atomic(&workspace.dir).await {
                    self.fail(error).await;
                    return;
                }
                fresh
            }
        };

        let limits = RateLimits::compose(
            self.config.shared_limiter.clone(),
            self.config.rate_limit_bps,
        );
        let context = Arc::new(DownloadContext {
            client,
            config: self.config.clone(),
            part_path: workspace.part_path.clone(),
            final_url: manifest.final_url.clone(),
            etag: manifest.etag.clone(),
            last_modified: manifest.last_modified.clone(),
            accepts_ranges: manifest.accepts_ranges,
            total: manifest.total_bytes,
            limits,
        });
        let progress: Arc<Vec<AtomicU64>> = Arc::new(
            manifest
                .segments
                .iter()
                .map(|segment| AtomicU64::new(segment.written))
                .collect(),
        );
        let (coord_tx, mut coord_rx) = mpsc::channel::<CoordMsg>(256);
        let active = Arc::new(AtomicUsize::new(0));

        let mut progress_tick = tokio::time::interval(self.config.progress_interval);
        progress_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut scale_tick = tokio::time::interval(self.config.scale_window);
        scale_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut checkpoint_tick = tokio::time::interval(self.config.checkpoint_interval);
        checkpoint_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let mut last_progress_bytes = 0u64;
        let mut speed_ema = 0f64;
        let mut have_speed_sample = false;
        let scale_enabled = manifest.accepts_ranges
            && manifest.total_bytes.is_some()
            && self.config.max_connections > self.config.start_connections;

        self.transition(DownloadState::Downloading).await;

        // One control receiver for the whole transfer phase: the session
        // select sleeps on it, and a paused run keeps its own clone so the
        // pause wait cannot swallow a wake meant for the session loop.
        let mut wake = self.control_rx.clone();

        'run: loop {
            // --- stop flags before any work ----------------------------
            let flags = *self.control_rx.borrow();
            if flags.cancelled {
                self.cancel_now(Some(&mut workspace)).await;
                return;
            }
            if flags.stopping {
                let checkpoint = self
                    .checkpoint(&workspace.dir, &mut manifest, &progress)
                    .await;
                let cancelled = self.control_rx.borrow().cancelled;
                if cancelled {
                    self.cancel_now(Some(&mut workspace)).await;
                    return;
                }
                match checkpoint {
                    Ok(()) => self.stop_now(Some(workspace)).await,
                    Err(error) => {
                        drop(workspace);
                        self.fail(error).await;
                    }
                }
                return;
            }
            if flags.paused {
                if let Err(error) = self
                    .checkpoint(&workspace.dir, &mut manifest, &progress)
                    .await
                {
                    self.fail(error).await;
                    return;
                }
                self.transition(DownloadState::Paused).await;
                let mut pause_wake = self.control_rx.clone();
                loop {
                    let inner = *self.control_rx.borrow();
                    if !inner.paused || inner.cancelled || inner.stopping {
                        break;
                    }
                    if pause_wake.changed().await.is_err() {
                        self.fail(EngineError::protocol("control channel closed"))
                            .await;
                        return;
                    }
                }
                // A resume lifts the pause before a single new byte moves, so
                // announce the state here; the handle's resume call waits for
                // exactly this transition. Stop goes straight to its checkpoint.
                let flags = *self.control_rx.borrow();
                if !flags.cancelled && !flags.stopping {
                    self.transition(DownloadState::Downloading).await;
                }
                continue 'run;
            }

            let incomplete = incomplete_segments(&manifest, &progress);
            if incomplete.is_empty() {
                break 'run;
            }

            // --- start a transfer session ------------------------------
            // Fresh session: stale flags and buffered messages from the
            // previous session must not leak into this one.
            apply_control(&self.control, |flags| flags.shutdown = false);
            while coord_rx.try_recv().is_ok() {}
            let queue = WorkQueue::new(incomplete.clone());
            let worker_control = self.control_rx.clone();
            let started = Arc::new(AtomicUsize::new(0));
            let initial = (self.config.start_connections as usize).min(incomplete.len());
            let mut workers: Vec<JoinHandle<()>> = (0..initial)
                .map(|_| {
                    spawn_worker(
                        &context,
                        &queue,
                        &worker_control,
                        &progress,
                        &coord_tx,
                        &active,
                        &started,
                    )
                })
                .collect();

            let mut session_done = 0usize;
            let mut failure: Option<EngineError> = None;
            let mut last_window_sum: Option<u64> = None;
            // `None` until the first window is measured, so the first
            // comparison always has a real previous value.
            let mut last_window_bps: Option<f64> = None;
            let mut settled = !scale_enabled;
            let mut last_checkpoint_bytes: u64 = 0;

            'io: loop {
                let flags = *self.control_rx.borrow();
                if flags.cancelled {
                    join_workers(workers).await;
                    self.cancel_now(Some(&mut workspace)).await;
                    return;
                }
                if flags.stopping || flags.paused {
                    join_workers(workers).await;
                    continue 'run;
                }

                let turn = tokio::select! {
                    message = coord_rx.recv() => match message {
                        Some(message) => IoTurn::Message(message),
                        None => IoTurn::ChannelClosed,
                    },
                    _ = progress_tick.tick() => IoTurn::Progress,
                    _ = scale_tick.tick() => IoTurn::Scale,
                    _ = checkpoint_tick.tick() => IoTurn::Checkpoint,
                    changed = wake.changed() => {
                        if changed.is_err() {
                            IoTurn::ChannelClosed
                        } else {
                            IoTurn::ControlWoken
                        }
                    }
                };

                match turn {
                    IoTurn::ControlWoken => continue,
                    IoTurn::ChannelClosed => {
                        failure = Some(EngineError::protocol("worker channel closed"));
                        apply_control(&self.control, |flags| flags.aborted = true);
                        join_workers(workers).await;
                        break 'io;
                    }
                    IoTurn::Progress => {
                        let downloaded: u64 = progress
                            .iter()
                            .map(|atomic| atomic.load(Ordering::Acquire))
                            .sum();
                        let interval_secs = self
                            .config
                            .progress_interval
                            .as_secs_f64()
                            .max(f64::EPSILON);
                        let instantaneous =
                            downloaded.saturating_sub(last_progress_bytes) as f64 / interval_secs;
                        let speed = if have_speed_sample {
                            0.7 * speed_ema + 0.3 * instantaneous
                        } else {
                            instantaneous
                        };
                        have_speed_sample = true;
                        speed_ema = speed;
                        last_progress_bytes = downloaded;
                        let speed_bps = speed as u64;
                        let eta_secs = match (manifest.total_bytes, speed_bps) {
                            (Some(total), _) if downloaded >= total => Some(0u64),
                            (Some(total), speed) if speed > 0 => Some((total - downloaded) / speed),
                            _ => None,
                        };
                        let _ = self.event_tx.try_send(EngineEvent::Progress {
                            downloaded,
                            total: manifest.total_bytes,
                            speed_bps,
                            eta_secs,
                            active_connections: active.load(Ordering::Acquire),
                            segments_done: completed_count(&manifest, &progress),
                            segments_total: manifest.segments.len(),
                        });
                        if downloaded.saturating_sub(last_checkpoint_bytes)
                            >= self.config.checkpoint_bytes
                        {
                            if let Err(error) = self
                                .checkpoint(&workspace.dir, &mut manifest, &progress)
                                .await
                            {
                                failure = Some(error);
                                apply_control(&self.control, |flags| flags.aborted = true);
                                join_workers(workers).await;
                                break 'io;
                            }
                            last_checkpoint_bytes = downloaded;
                        }
                    }
                    IoTurn::Scale => {
                        if settled {
                            continue;
                        }
                        let sum: u64 = progress
                            .iter()
                            .map(|atomic| atomic.load(Ordering::Acquire))
                            .sum();
                        let window = self.config.scale_window.as_secs_f64().max(f64::EPSILON);
                        let current_bps = match last_window_sum {
                            Some(previous) => (sum.saturating_sub(previous)) as f64 / window,
                            None => 0.0,
                        };
                        last_window_sum = Some(sum);
                        if let Some(previous_bps) = last_window_bps {
                            match scaling_decision(
                                previous_bps,
                                current_bps,
                                self.config.gain_threshold,
                            ) {
                                ScaleDecision::Add => {
                                    let cap = (self.config.max_connections as usize)
                                        .min(incomplete.len());
                                    if workers.len() < cap
                                        && started.load(Ordering::Acquire) < incomplete.len()
                                    {
                                        workers.push(spawn_worker(
                                            &context,
                                            &queue,
                                            &worker_control,
                                            &progress,
                                            &coord_tx,
                                            &active,
                                            &started,
                                        ));
                                    } else {
                                        settled = true;
                                    }
                                }
                                ScaleDecision::Settle => settled = true,
                            }
                        }
                        last_window_bps = Some(current_bps);
                    }
                    IoTurn::Checkpoint => {
                        if let Err(error) = self
                            .checkpoint(&workspace.dir, &mut manifest, &progress)
                            .await
                        {
                            failure = Some(error);
                            apply_control(&self.control, |flags| flags.aborted = true);
                            join_workers(workers).await;
                            break 'io;
                        }
                        last_checkpoint_bytes = progress
                            .iter()
                            .map(|atomic| atomic.load(Ordering::Acquire))
                            .sum();
                    }
                    IoTurn::Message(message) => match message {
                        CoordMsg::Started => {}
                        CoordMsg::Done => {
                            session_done += 1;
                            if session_done >= incomplete.len() {
                                apply_control(&self.control, |flags| flags.shutdown = true);
                                join_workers(workers).await;
                                break 'io;
                            }
                        }
                        CoordMsg::Failed { error } => {
                            if failure.is_none() {
                                failure = Some(error);
                            }
                            apply_control(&self.control, |flags| flags.aborted = true);
                            join_workers(workers).await;
                            break 'io;
                        }
                        CoordMsg::Retrying {
                            segment,
                            attempt,
                            delay_ms,
                            reason,
                        } => {
                            let _ = self
                                .event_tx
                                .send(EngineEvent::Retrying {
                                    segment,
                                    attempt,
                                    delay_ms,
                                    reason,
                                })
                                .await;
                        }
                    },
                }
            }

            // A session can end three ways: all segments finished (breaks
            // the outer loop below), a failure landed in `failure`, or a
            // control flag made the session `continue`/`return` directly.
            if let Some(error) = failure {
                let _ = self
                    .checkpoint(&workspace.dir, &mut manifest, &progress)
                    .await;
                self.fail(error).await;
                return;
            }
            break 'run;
        }

        match self.finalize(&mut workspace, &mut manifest).await {
            Ok((path, sha256)) => self.complete(path, sha256).await,
            Err(error) => {
                self.fail(error).await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_worker(
    context: &Arc<DownloadContext>,
    queue: &Arc<WorkQueue>,
    control: &watch::Receiver<Control>,
    progress: &Arc<Vec<AtomicU64>>,
    coord: &mpsc::Sender<CoordMsg>,
    active: &Arc<AtomicUsize>,
    started: &Arc<AtomicUsize>,
) -> JoinHandle<()> {
    tokio::spawn(worker_loop(
        context.clone(),
        queue.clone(),
        control.clone(),
        progress.clone(),
        coord.clone(),
        active.clone(),
        started.clone(),
    ))
}

async fn join_workers(workers: Vec<JoinHandle<()>>) {
    for worker in workers {
        let _ = worker.await;
    }
}

async fn ensure_destination_parent(dest: &Path) -> Result<(), EngineError> {
    let Some(parent) = dest.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| EngineError::io("create destination directory", parent, error))
}

fn runtime_segments(manifest: &Manifest) -> Vec<Segment> {
    manifest
        .segments
        .iter()
        .map(|segment| Segment {
            index: segment.index,
            start: segment.start,
            end: segment.end,
        })
        .collect()
}

fn segment_done(manifest: &Manifest, progress: &[AtomicU64], segment: &Segment) -> bool {
    let written = progress
        .get(segment.index as usize)
        .map(|atomic| atomic.load(Ordering::Acquire))
        .unwrap_or(0);
    if manifest.accepts_ranges {
        written >= segment.size()
    } else {
        match manifest.total_bytes {
            Some(total) => written >= total,
            None => false,
        }
    }
}

fn completed_count(manifest: &Manifest, progress: &[AtomicU64]) -> usize {
    runtime_segments(manifest)
        .iter()
        .filter(|segment| segment_done(manifest, progress, segment))
        .count()
}

fn incomplete_segments(manifest: &Manifest, progress: &[AtomicU64]) -> Vec<Segment> {
    runtime_segments(manifest)
        .into_iter()
        .filter(|segment| !segment_done(manifest, progress, segment))
        .collect()
}

fn build_manifest(
    spec: &DownloadSpec,
    probe: probe::ProbeResult,
    config: &EngineConfig,
    dest: &Path,
) -> Manifest {
    let segments: Vec<SegmentState> = match probe.total {
        None => vec![SegmentState {
            index: 0,
            start: 0,
            end: 0,
            written: 0,
        }],
        Some(0) => Vec::new(),
        Some(total) if probe.accepts_ranges => tile(
            total,
            config.min_segment_bytes,
            config.max_connections as u32,
        )
        .into_iter()
        .map(|segment| SegmentState {
            index: segment.index,
            start: segment.start,
            end: segment.end,
            written: 0,
        })
        .collect(),
        Some(total) => vec![SegmentState {
            index: 0,
            start: 0,
            end: total - 1,
            written: 0,
        }],
    };
    Manifest {
        v: crate::manifest::MANIFEST_VERSION,
        url: spec.url.clone(),
        final_url: probe.final_url,
        dest: dest.to_path_buf(),
        total_bytes: probe.total,
        accepts_ranges: probe.accepts_ranges,
        etag: probe.etag,
        last_modified: probe.last_modified,
        content_type: probe.content_type,
        filename: probe.filename,
        expected_sha256: spec
            .expected_sha256
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase()),
        observed_sha256: None,
        created_at: unix_timestamp(),
        segments,
        completed: false,
    }
}
