use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Response;
use reqwest::header::{IF_RANGE, RANGE};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::rate::RateLimits;
use crate::segmenter::Segment;

/// Stop conditions broadcast by the coordinator. Workers re-read them after
/// every chunk and whenever a watch notification arrives, so pause, stop,
/// cancel, and abort land within one chunk of network time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Control {
    pub paused: bool,
    pub stopping: bool,
    pub cancelled: bool,
    pub aborted: bool,
    pub shutdown: bool,
}

impl Control {
    pub fn stop_requested(&self) -> bool {
        self.paused || self.stopping || self.cancelled || self.aborted || self.shutdown
    }
}

/// Applies an update to the shared flags, notifying watchers only when
/// something actually changed (a no-op send would wake every worker for
/// nothing).
pub(crate) fn apply_control(sender: &watch::Sender<Control>, update: impl FnOnce(&mut Control)) {
    let current = *sender.borrow();
    let mut next = current;
    update(&mut next);
    if next != current {
        sender.send_replace(next);
    }
}

/// Messages workers send back to the coordinator.
pub(crate) enum CoordMsg {
    Started,
    Done,
    Failed {
        error: EngineError,
    },
    Retrying {
        segment: u32,
        attempt: u32,
        delay_ms: u64,
        reason: String,
    },
}

/// Immutable per-download context shared by every worker.
pub(crate) struct DownloadContext {
    pub client: reqwest::Client,
    pub config: EngineConfig,
    pub part_path: PathBuf,
    pub final_url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub accepts_ranges: bool,
    pub total: Option<u64>,
    /// Every bandwidth ceiling that applies to this download.
    pub limits: RateLimits,
}

/// Multi-consumer segment queue: workers pull while the coordinator owns
/// the session. Refilled by creating a new queue for every session.
pub(crate) struct WorkQueue {
    inner: Mutex<VecDeque<Segment>>,
}

impl WorkQueue {
    pub fn new(segments: Vec<Segment>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(VecDeque::from(segments)),
        })
    }

    pub fn pop(&self) -> Option<Segment> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.pop_front()
    }
}

/// How many bytes this segment still needs before it counts as done.
fn segment_needed(context: &DownloadContext, segment: Segment) -> Option<u64> {
    if context.accepts_ranges {
        Some(segment.size())
    } else {
        context.total
    }
}

enum RunOutcome {
    Done,
    Stopped,
}

enum AttemptFailure {
    Retry(String),
    Fatal(EngineError),
    Stop,
}

/// Byte cursor over the segment's progress atomic. The atomic is stored
/// only after the corresponding write completed, so its value is always a
/// conservative low-water mark.
struct ProgressCursor<'a> {
    atomic: &'a AtomicU64,
    position: u64,
}

/// Everything the body pump needs besides the response.
struct TransferPolicy {
    idle_read_timeout: Duration,
    target: Option<u64>,
    part_path: PathBuf,
    limits: RateLimits,
}

/// One worker: pull segments until the queue is empty or a stop flag lands.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn worker_loop(
    context: Arc<DownloadContext>,
    queue: Arc<WorkQueue>,
    control: watch::Receiver<Control>,
    progress: Arc<Vec<AtomicU64>>,
    coord: mpsc::Sender<CoordMsg>,
    active: Arc<AtomicUsize>,
    started: Arc<AtomicUsize>,
) {
    let mut wake = control.clone();
    loop {
        if control.borrow().stop_requested() {
            return;
        }
        let Some(segment) = queue.pop() else {
            if wake.changed().await.is_err() {
                return;
            }
            continue;
        };
        let index = segment.index as usize;
        let written = progress
            .get(index)
            .map(|atomic| atomic.load(Ordering::Acquire))
            .unwrap_or(0);
        let already_done = match segment_needed(&context, segment) {
            Some(limit) => written >= limit,
            None => false,
        };
        if already_done {
            let _ = coord.send(CoordMsg::Done).await;
            continue;
        }

        started.fetch_add(1, Ordering::AcqRel);
        let _ = coord.send(CoordMsg::Started).await;

        let atomic = &progress[index];
        let result = run_with_retries(
            &context, segment, &control, &mut wake, atomic, &coord, &active,
        )
        .await;

        match result {
            Ok(RunOutcome::Done) => {
                let _ = coord.send(CoordMsg::Done).await;
            }
            Ok(RunOutcome::Stopped) => return,
            Err(error) => {
                let _ = coord.send(CoordMsg::Failed { error }).await;
                return;
            }
        }
    }
}

async fn run_with_retries(
    context: &DownloadContext,
    segment: Segment,
    control: &watch::Receiver<Control>,
    wake: &mut watch::Receiver<Control>,
    progress: &AtomicU64,
    coord: &mpsc::Sender<CoordMsg>,
    active: &AtomicUsize,
) -> Result<RunOutcome, EngineError> {
    let mut attempt = 0u32;
    loop {
        if control.borrow().stop_requested() {
            return Ok(RunOutcome::Stopped);
        }
        // One lease covers the entire response, including any wait for
        // bandwidth tokens. Return it before reporting a retry or sleeping
        // through backoff; no parked worker holds a network connection.
        let lease = if let Some(budget) = &context.config.connection_budget {
            match budget
                .acquire_while(|| !control.borrow().stop_requested())
                .await
            {
                Some(lease) => Some(lease),
                None => return Ok(RunOutcome::Stopped),
            }
        } else {
            None
        };
        if control.borrow().stop_requested() {
            return Ok(RunOutcome::Stopped);
        }
        attempt += 1;
        active.fetch_add(1, Ordering::AcqRel);
        let result = attempt_segment(context, segment, control, wake, progress).await;
        active.fetch_sub(1, Ordering::AcqRel);
        drop(lease);
        match result {
            Ok(()) => return Ok(RunOutcome::Done),
            Err(AttemptFailure::Stop) => {
                if control.borrow().stop_requested() {
                    return Ok(RunOutcome::Stopped);
                }
                // The wake carried no stop condition (a flag flipped and
                // flipped back); keep going with a fresh attempt.
            }
            Err(AttemptFailure::Fatal(error)) => return Err(error),
            Err(AttemptFailure::Retry(reason)) => {
                if attempt >= context.config.max_attempts {
                    return Err(EngineError::network("download segment", reason, attempt));
                }
                let base = super::base_backoff(&context.config, attempt);
                // The jitter seed is 64-bit; widening the segment index keeps
                // parallel segments from sharing one retry rhythm.
                let entropy = super::now_entropy() ^ u64::from(segment.index).rotate_left(11);
                let delay = super::jittered_backoff(base, entropy);
                let _ = coord
                    .send(CoordMsg::Retrying {
                        segment: segment.index,
                        attempt,
                        delay_ms: delay.as_millis() as u64,
                        reason: reason.clone(),
                    })
                    .await;
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    changed = wake.changed() => {
                        if changed.is_err() {
                            return Ok(RunOutcome::Stopped);
                        }
                    }
                }
            }
        }
    }
}

async fn attempt_segment(
    context: &DownloadContext,
    segment: Segment,
    control: &watch::Receiver<Control>,
    wake: &mut watch::Receiver<Control>,
    progress: &AtomicU64,
) -> Result<(), AttemptFailure> {
    let ranged = context.accepts_ranges;
    let start_offset = if ranged {
        let written = progress.load(Ordering::Acquire);
        if written >= segment.size() {
            return Ok(());
        }
        segment.start + written
    } else {
        // Unranged fetches always restart their single stream from byte 0.
        progress.store(0, Ordering::Release);
        0
    };
    let start_value = progress.load(Ordering::Acquire);

    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&context.part_path)
        .await
        .map_err(|error| {
            AttemptFailure::Fatal(EngineError::io("open part file", &context.part_path, error))
        })?;
    file.seek(std::io::SeekFrom::Start(start_offset))
        .await
        .map_err(|error| {
            AttemptFailure::Fatal(EngineError::io("seek part file", &context.part_path, error))
        })?;

    let mut request = context.client.get(&context.final_url);
    if ranged {
        let range_end = segment.end;
        request = request.header(RANGE, format!("bytes={start_offset}-{range_end}"));
        if let Some(etag) = &context.etag {
            request = request.header(IF_RANGE, etag.clone());
        } else if let Some(last_modified) = &context.last_modified {
            request = request.header(IF_RANGE, last_modified.clone());
        }
    }

    let response = match tokio::time::timeout(context.config.header_timeout, request.send()).await {
        Err(_) => {
            return Err(AttemptFailure::Retry(
                "response headers exceeded the header timeout".to_string(),
            ));
        }
        Ok(Err(error)) => return Err(AttemptFailure::Retry(format!("connection failed: {error}"))),
        Ok(Ok(response)) => response,
    };

    let status = response.status().as_u16();
    match status {
        200..=299 if !ranged => {
            if status != 200 {
                return Err(AttemptFailure::Fatal(EngineError::protocol(format!(
                    "unexpected status {status} for an unranged request"
                ))));
            }
        }
        200..=299 => {
            if status == 206 {
                let value = super::header_str(&response, &reqwest::header::CONTENT_RANGE)
                    .unwrap_or_default();
                match super::probe::parse_content_range_value(&value) {
                    Some((Some(actual_start), _, _)) if actual_start == start_offset => {}
                    Some(_) => {
                        return Err(AttemptFailure::Fatal(EngineError::protocol(format!(
                            "server returned a different range than requested ({value})"
                        ))));
                    }
                    None => {
                        return Err(AttemptFailure::Fatal(EngineError::protocol(
                            "206 response without a parseable Content-Range",
                        )));
                    }
                }
            } else {
                // A 200 answer to a ranged request means the server ignored
                // the range (or the validator expired): writing it at the
                // current offset would splice two versions of the file.
                return Err(AttemptFailure::Fatal(EngineError::ContentChanged));
            }
        }
        408 | 429 | 500..=599 => {
            return Err(AttemptFailure::Retry(format!(
                "server responded with HTTP {status}"
            )));
        }
        416 if ranged => {
            return Err(AttemptFailure::Fatal(EngineError::SizeChanged));
        }
        _ => {
            return Err(AttemptFailure::Fatal(EngineError::HttpStatus {
                status,
                url: context.final_url.clone(),
            }));
        }
    }

    let target = if ranged {
        Some(segment.size())
    } else {
        context.total
    };
    let policy = TransferPolicy {
        idle_read_timeout: context.config.idle_read_timeout,
        target,
        part_path: context.part_path.clone(),
        limits: context.limits.clone(),
    };
    let mut cursor = ProgressCursor {
        atomic: progress,
        position: start_value,
    };
    pump_body(response, file, &mut cursor, policy, control, wake).await
}

async fn pump_body(
    mut response: Response,
    mut file: tokio::fs::File,
    cursor: &mut ProgressCursor<'_>,
    policy: TransferPolicy,
    control: &watch::Receiver<Control>,
    wake: &mut watch::Receiver<Control>,
) -> Result<(), AttemptFailure> {
    loop {
        if let Some(limit) = policy.target
            && cursor.position >= limit
        {
            break;
        }
        let pending_chunk = tokio::time::timeout(policy.idle_read_timeout, response.chunk());
        let step = tokio::select! {
            result = pending_chunk => Some(result),
            changed = wake.changed() => {
                if changed.is_err() {
                    return Err(AttemptFailure::Stop);
                }
                None
            }
        };
        match step {
            None => {
                if control.borrow().stop_requested() {
                    return Err(AttemptFailure::Stop);
                }
                continue;
            }
            Some(Err(_elapsed)) => {
                return Err(AttemptFailure::Retry(
                    "no data received before the read timeout".to_string(),
                ));
            }
            Some(Ok(Err(error))) => {
                return Err(AttemptFailure::Retry(format!(
                    "connection dropped during transfer: {error}"
                )));
            }
            Some(Ok(Ok(None))) => break,
            Some(Ok(Ok(Some(bytes)))) => {
                if bytes.is_empty() {
                    continue;
                }
                let length = bytes.len() as u64;
                if let Some(limit) = policy.target
                    && cursor.position + length > limit
                {
                    return Err(AttemptFailure::Fatal(EngineError::protocol(
                        "server sent more bytes than the range allows",
                    )));
                }
                // Bandwidth is paid for before the bytes land, so the
                // progress atomic never reports data that the limit has
                // not allowed yet. The callback keeps a pause or cancel
                // responsive while a slow bucket is being filled.
                if !policy
                    .limits
                    .acquire_while(length, || !control.borrow().stop_requested())
                    .await
                {
                    return Err(AttemptFailure::Stop);
                }
                file.write_all(&bytes[..]).await.map_err(|error| {
                    AttemptFailure::Fatal(EngineError::io(
                        "write part file",
                        &policy.part_path,
                        error,
                    ))
                })?;
                cursor.position += length;
                cursor.atomic.store(cursor.position, Ordering::Release);
            }
        }
    }

    match policy.target {
        Some(limit) if cursor.position >= limit => Ok(()),
        Some(limit) => {
            let position = cursor.position;
            Err(AttemptFailure::Retry(format!(
                "connection closed after {position} of {limit} bytes"
            )))
        }
        None => Ok(()),
    }
}
