//! The single owner of the queue and of every running download.
//!
//! All work arrives on one command channel and the engine reports on one
//! report channel, both consumed by this task. That is what makes the
//! database single-writer and keeps a pause from interleaving with a delete.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rapids_engine::{DownloadHandle, DownloadSpec, EngineConfig, filename_from_url};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::error::HostError;
use crate::events::{EventSink, HostEvent, TaskView};
use crate::now_ms;
use crate::runner::{self, Report};
use crate::scheduler::{self, Clock, Window};
use crate::store::settings;
use crate::store::{
    history::{self, NewHistory},
    schedules::{self, ScheduleRow},
    tasks::{self, NewTask, TaskCategory, TaskRow, TaskStatus},
    Outcome, Store,
};

/// How often the manager looks at the clock. A second is fine-grained enough
/// for a start window and coarse enough that an idle app wakes four times a
/// minute at most.
const TICK: Duration = Duration::from_secs(1);

/// Everything a caller must supply to enqueue work.
#[derive(Debug, Clone)]
pub struct AddRequest {
    pub url: String,
    pub dest_dir: PathBuf,
    pub category: TaskCategory,
    pub priority: i64,
    pub connections: Option<u8>,
    pub speed_limit_bps: Option<u64>,
    pub expected_sha256: Option<String>,
    pub schedule_id: Option<i64>,
}

enum Command {
    Add {
        request: Box<AddRequest>,
        reply: oneshot::Sender<Result<TaskView, HostError>>,
    },
    List {
        reply: oneshot::Sender<Result<Vec<TaskView>, HostError>>,
    },
    Pause {
        id: i64,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Resume {
        id: i64,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Retry {
        id: i64,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Cancel {
        id: i64,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Delete {
        id: i64,
        remove_file: bool,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Reorder {
        id: i64,
        priority: i64,
        reply: oneshot::Sender<Result<(), HostError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// A handle to the manager. Cloning is cheap and every clone talks to the
/// same task.
#[derive(Clone)]
pub struct ManagerHandle {
    commands: mpsc::Sender<Command>,
}

impl ManagerHandle {
    pub async fn add(&self, request: AddRequest) -> Result<TaskView, HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Add {
            request: Box::new(request),
            reply,
        })
        .await?;
        receive(answer).await
    }

    pub async fn list(&self) -> Result<Vec<TaskView>, HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::List { reply }).await?;
        receive(answer).await
    }

    pub async fn pause(&self, id: i64) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Pause { id, reply }).await?;
        receive(answer).await
    }

    pub async fn resume(&self, id: i64) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Resume { id, reply }).await?;
        receive(answer).await
    }

    pub async fn retry(&self, id: i64) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Retry { id, reply }).await?;
        receive(answer).await
    }

    pub async fn cancel(&self, id: i64) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Cancel { id, reply }).await?;
        receive(answer).await
    }

    pub async fn delete(&self, id: i64, remove_file: bool) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Delete {
            id,
            remove_file,
            reply,
        })
        .await?;
        receive(answer).await
    }

    pub async fn reorder(&self, id: i64, priority: i64) -> Result<(), HostError> {
        let (reply, answer) = oneshot::channel();
        self.send(Command::Reorder { id, priority, reply }).await?;
        receive(answer).await
    }

    /// Parks every running download (the engine checkpoints on the way down),
    /// then stops the manager task.
    pub async fn shutdown(&self) {
        let (reply, answer) = oneshot::channel();
        if self.send(Command::Shutdown { reply }).await.is_ok() {
            let _ = answer.await;
        }
    }

    async fn send(&self, command: Command) -> Result<(), HostError> {
        self.commands.send(command).await.map_err(|_| {
            HostError::invalid("the download manager is no longer running")
        })
    }
}

/// The schedule a row points at, when the view loaded this beat has it.
fn schedule_for(schedules: &[ScheduleRow], id: Option<i64>) -> Option<&ScheduleRow> {
    let id = id?;
    schedules.iter().find(|schedule| schedule.id == id)
}

async fn receive<T>(answer: oneshot::Receiver<Result<T, HostError>>) -> Result<T, HostError> {
    match answer.await {
        Ok(result) => result,
        Err(_) => Err(HostError::invalid(
            "the download manager stopped before answering",
        )),
    }
}

/// One turn of the manager's loop.
enum Incoming {
    Command(Option<Command>),
    Report(Option<Report>),
    /// The scheduler's beat: windows are re-read and due work may start.
    Tick,
}

struct Manager {
    store: Arc<Store>,
    sink: Arc<dyn EventSink>,
    clock: Clock,
    ticker: tokio::time::Interval,
    running: HashMap<i64, DownloadHandle>,
    reports: mpsc::Sender<Report>,
    report_rx: mpsc::Receiver<Report>,
    commands: mpsc::Receiver<Command>,
}

/// Starts the manager task. The returned handle is how callers talk to it;
/// the join handle completes when the manager stops.
///
/// `clock` is the wall clock start windows are read against. The caller owns
/// it because only the platform layer knows the machine's offset from UTC.
pub fn spawn(
    store: Arc<Store>,
    sink: Arc<dyn EventSink>,
    clock: Clock,
) -> (ManagerHandle, JoinHandle<()>) {
    let (command_tx, command_rx) = mpsc::channel(64);
    let (report_tx, report_rx) = mpsc::channel(256);
    let mut ticker = tokio::time::interval(TICK);
    // A tick that arrives while a handler is busy is not worth a burst of
    // catch-up work afterwards; the next beat is due one interval later.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let manager = Manager {
        store,
        sink,
        clock,
        ticker,
        running: HashMap::new(),
        reports: report_tx,
        report_rx,
        commands: command_rx,
    };
    let join = tokio::spawn(manager.run());
    (
        ManagerHandle {
            commands: command_tx,
        },
        join,
    )
}

impl Manager {
    async fn run(mut self) {
        // Recovery failures leave the manager idle rather than guessing;
        // commands that need the database answer with their own error.
        let _ = self.recover().await;
        loop {
            // The select result is taken as a value so neither channel stays
            // borrowed while the handler runs.
            let incoming = tokio::select! {
                command = self.commands.recv() => Incoming::Command(command),
                report = self.report_rx.recv() => Incoming::Report(report),
                _ = self.ticker.tick() => Incoming::Tick,
            };
            match incoming {
                Incoming::Command(Some(command)) => {
                    if self.handle(command).await {
                        break;
                    }
                }
                Incoming::Command(None) => break,
                Incoming::Report(Some(report)) => self.handle_report(report).await,
                // The manager holds a sender of its own, so this cannot
                // happen; stopping is safer than spinning if it ever did.
                Incoming::Report(None) => break,
                Incoming::Tick => self.tick().await,
            }
        }
    }

    /// Returns downloads a previous process left running to the queue.
    /// How many bytes each one keeps is the sidecar manifest's decision, made
    /// when the engine opens the workspace.
    async fn recover(&mut self) -> Result<(), HostError> {
        let resume = settings::get_bool(self.store.pool(), settings::RESUME_ON_START, true).await?;
        if resume {
            let requeued = tasks::requeue_interrupted(self.store.pool(), now_ms()).await?;
            if requeued > 0 {
                self.sink.emit(HostEvent::QueueChanged);
            }
        }
        self.pump().await
    }

    /// Starts due work while there is room.
    ///
    /// A queued row can still be waiting for two reasons, and both are
    /// honoured here rather than in the query: its retry may not be due yet,
    /// and its window may not be open. Scheduled rows are never picked up —
    /// the tick moves them into the queue when their window opens.
    async fn pump(&mut self) -> Result<(), HostError> {
        let max_parallel = settings::get_i64(self.store.pool(), settings::MAX_PARALLEL, 3)
            .await
            .unwrap_or(3)
            .clamp(1, 32);
        let max_parallel = usize::try_from(max_parallel).unwrap_or(3);
        let schedules = schedules::list(self.store.pool())
            .await
            .unwrap_or_default();
        while self.running.len() < max_parallel {
            let due = tasks::due(self.store.pool(), now_ms()).await?;
            let mut next = None;
            for row in due {
                if self.running.contains_key(&row.id) {
                    continue;
                }
                if !self.window_open(&row, now_ms(), &schedules) {
                    continue;
                }
                next = Some(row);
                break;
            }
            let Some(row) = next else {
                return Ok(());
            };
            if let Err(error) = self.start(&row).await {
                // The engine refused the work itself, so nothing about the
                // network applies: this attempt is not worth repeating.
                self.record_failure(row.id, &error.to_string(), false).await;
            }
        }
        Ok(())
    }

    /// One scheduler beat: open windows release their work, then the queue
    /// fills whatever room is left.
    ///
    /// Windows gate starts only. A transfer that is already running when its
    /// window closes carries on: the engine's only way out of a workspace is
    /// to discard it, and throwing away bytes the person already paid for is
    /// not something a clock should decide.
    async fn tick(&mut self) {
        let now = now_ms();
        let schedules = schedules::list(self.store.pool())
            .await
            .unwrap_or_default();
        if !schedules.is_empty() {
            self.reconcile_windows(now, &schedules).await;
        }
        let _ = self.pump().await;
    }

    /// Keeps the queue and the windows in agreement.
    ///
    /// A scheduled row whose window is open joins the queue, and a queued row
    /// whose window has closed goes back to waiting. Running work is not in
    /// either set, so a transfer that is under way is never disturbed.
    async fn reconcile_windows(&mut self, now: i64, schedules: &[ScheduleRow]) {
        let Ok(waiting) = tasks::waiting(self.store.pool()).await else {
            return;
        };
        for row in waiting {
            let Some(schedule) = schedule_for(schedules, row.schedule_id) else {
                continue;
            };
            let open = schedule.is_enabled()
                && scheduler::window_state(schedule, now, self.clock) == Window::Open;
            let wanted = match (row.status_enum(), open) {
                (Some(TaskStatus::Scheduled), true) => TaskStatus::Queued,
                (Some(TaskStatus::Queued), false) => TaskStatus::Scheduled,
                _ => continue,
            };
            if tasks::set_status(self.store.pool(), row.id, wanted, now)
                .await
                .is_ok()
            {
                self.emit_changed(row.id).await;
            }
        }
    }

    /// Whether a row tied to a schedule may start. A row whose schedule has
    /// gone missing stays parked: the foreign key clears the link when a
    /// schedule is deleted, so anything left pointing at nothing is a state
    /// worth refusing rather than guessing about.
    fn window_open(&self, row: &TaskRow, now: i64, schedules: &[ScheduleRow]) -> bool {
        match schedule_for(schedules, row.schedule_id) {
            None => row.schedule_id.is_none(),
            Some(schedule) => {
                schedule.is_enabled()
                    && scheduler::window_state(schedule, now, self.clock) == Window::Open
            }
        }
    }

    async fn start(&mut self, row: &TaskRow) -> Result<(), HostError> {
        let now = now_ms();
        tasks::mark_started(self.store.pool(), row.id, now).await?;

        let spec = DownloadSpec {
            url: row.url.clone(),
            dest: Some(PathBuf::from(&row.dest_path)),
            expected_sha256: row.expected_sha256.clone(),
            connections: row.connections.and_then(|value| u8::try_from(value).ok()),
            overwrite: false,
        };
        let config = EngineConfig {
            rate_limit_bps: row
                .speed_limit_bps
                .and_then(|value| u64::try_from(value).ok()),
            ..EngineConfig::default()
        };

        match runner::start(row.id, spec, config, self.reports.clone()) {
            Ok(handle) => {
                self.running.insert(row.id, handle);
                self.emit_changed(row.id).await;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    async fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Add { request, reply } => {
                let result = self.add(*request).await;
                let _ = reply.send(result);
                false
            }
            Command::List { reply } => {
                let result = self.list().await;
                let _ = reply.send(result);
                false
            }
            Command::Pause { id, reply } => {
                let result = self.pause(id).await;
                let _ = reply.send(result);
                false
            }
            Command::Resume { id, reply } => {
                let result = self.resume(id).await;
                let _ = reply.send(result);
                false
            }
            Command::Retry { id, reply } => {
                let result = self.retry(id).await;
                let _ = reply.send(result);
                false
            }
            Command::Cancel { id, reply } => {
                let result = self.remove(id, false).await;
                let _ = reply.send(result);
                false
            }
            Command::Delete {
                id,
                remove_file,
                reply,
            } => {
                let result = self.remove(id, remove_file).await;
                let _ = reply.send(result);
                false
            }
            Command::Reorder { id, priority, reply } => {
                let result = self.reorder(id, priority).await;
                let _ = reply.send(result);
                false
            }
            Command::Shutdown { reply } => {
                self.stop_all().await;
                let _ = reply.send(());
                true
            }
        }
    }

    async fn add(&mut self, request: AddRequest) -> Result<TaskView, HostError> {
        let file_name = filename_from_url(&request.url).ok_or_else(|| {
            HostError::invalid("the URL does not name a file; give the download a name")
        })?;
        let dest_path = request.dest_dir.join(&file_name);
        let dest_text = dest_path.to_string_lossy().to_string();
        let now = now_ms();
        let status = if request.schedule_id.is_some() {
            TaskStatus::Scheduled
        } else {
            TaskStatus::Queued
        };

        let new = NewTask {
            url: request.url,
            dest_path: dest_text,
            file_name,
            category: request.category,
            status,
            priority: request.priority,
            connections: request.connections.map(i64::from),
            speed_limit_bps: request
                .speed_limit_bps
                .and_then(|value| i64::try_from(value).ok()),
            expected_sha256: request.expected_sha256,
            schedule_id: request.schedule_id,
            created_at: now,
        };
        let id = tasks::insert(self.store.pool(), &new).await?;
        self.emit_changed(id).await;
        self.pump().await?;
        self.view(id).await
    }

    async fn list(&self) -> Result<Vec<TaskView>, HostError> {
        Ok(tasks::list(self.store.pool())
            .await?
            .iter()
            .map(TaskView::from_row)
            .collect())
    }

    async fn pause(&mut self, id: i64) -> Result<(), HostError> {
        let handle = self
            .running
            .get(&id)
            .cloned()
            .ok_or(HostError::NotFound {
                entity: "running task",
                id,
            })?;
        handle.pause().await;
        tasks::set_status(self.store.pool(), id, TaskStatus::Paused, now_ms()).await?;
        self.emit_changed(id).await;
        Ok(())
    }

    async fn resume(&mut self, id: i64) -> Result<(), HostError> {
        if let Some(handle) = self.running.get(&id).cloned() {
            handle.resume().await;
            tasks::set_status(self.store.pool(), id, TaskStatus::Downloading, now_ms()).await?;
            self.emit_changed(id).await;
            return Ok(());
        }
        let row = self.row(id).await?;
        let status = row.status_enum().ok_or_else(|| HostError::Invalid {
            reason: format!("task {id} has an unknown status"),
        })?;
        match status {
            TaskStatus::Paused => {
                tasks::set_status(self.store.pool(), id, TaskStatus::Queued, now_ms()).await?;
                self.emit_changed(id).await;
                self.pump().await
            }
            TaskStatus::Error => {
                tasks::clear_failure(self.store.pool(), id, TaskStatus::Queued, now_ms()).await?;
                self.emit_changed(id).await;
                self.pump().await
            }
            TaskStatus::Queued | TaskStatus::Scheduled | TaskStatus::Downloading => Ok(()),
            TaskStatus::Completed => Err(HostError::invalid("this download already finished")),
        }
    }

    async fn retry(&mut self, id: i64) -> Result<(), HostError> {
        let row = self.row(id).await?;
        if row.status_enum() != Some(TaskStatus::Error) {
            return Err(HostError::invalid("only a failed download can be retried"));
        }
        // Asking for a retry by hand is a fresh decision, so the automatic
        // attempts that were spent do not shorten the new run's budget.
        tasks::clear_failure(self.store.pool(), id, TaskStatus::Queued, now_ms()).await?;
        self.emit_changed(id).await;
        self.pump().await
    }

    async fn reorder(&mut self, id: i64, priority: i64) -> Result<(), HostError> {
        tasks::set_priority(self.store.pool(), id, priority, now_ms()).await?;
        self.emit_changed(id).await;
        Ok(())
    }

    /// Cancels if running, records the outcome, removes the row, and
    /// optionally deletes the destination file.
    async fn remove(&mut self, id: i64, remove_file: bool) -> Result<(), HostError> {
        let row = self.row(id).await?;

        if let Some(handle) = self.running.remove(&id) {
            handle.cancel().await;
        }

        let outcome = match row.status_enum() {
            Some(TaskStatus::Completed) => Outcome::Completed,
            Some(TaskStatus::Error) => Outcome::Failed,
            _ => Outcome::Cancelled,
        };
        history::insert(
            self.store.pool(),
            &NewHistory {
                task_id: id,
                url: row.url.clone(),
                file_name: row.file_name.clone(),
                dest_path: row.dest_path.clone(),
                outcome,
                total_bytes: row.total_bytes,
                observed_sha256: row.observed_sha256.clone(),
                error_message: row.error_message.clone(),
                started_at: row.started_at,
                ended_at: now_ms(),
            },
        )
        .await?;

        if remove_file {
            match tokio::fs::remove_file(&row.dest_path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(HostError::invalid(format!(
                        "the task was removed but its file could not be deleted: {error}"
                    )));
                }
            }
        }

        tasks::delete(self.store.pool(), id).await?;
        self.sink.emit(HostEvent::TaskRemoved { id });
        self.pump().await
    }

    async fn handle_report(&mut self, report: Report) {
        match report {
            Report::Progress { id, progress, update } => {
                let _ = tasks::set_progress(self.store.pool(), id, progress, now_ms()).await;
                self.sink.emit(HostEvent::Progress {
                    updates: vec![update],
                });
            }
            Report::Completed { id, sha256 } => {
                self.running.remove(&id);
                let row = self.row(id).await.ok();
                let total = row.as_ref().and_then(|row| row.total_bytes);
                if tasks::mark_completed(
                    self.store.pool(),
                    id,
                    total,
                    sha256.as_deref(),
                    now_ms(),
                )
                .await
                .is_ok()
                {
                    self.record_history(id, Outcome::Completed).await;
                    self.emit_changed(id).await;
                }
                let _ = self.pump().await;
            }
            Report::Failed {
                id,
                message,
                retryable,
            } => {
                self.running.remove(&id);
                self.record_failure(id, &message, retryable).await;
                let _ = self.pump().await;
            }
            Report::Cancelled { id } => {
                self.running.remove(&id);
            }
        }
    }

    /// Records a failed attempt, as either a waiting retry or a final error.
    ///
    /// The engine's classification decides, and the attempt counter bounds it:
    /// a retryable failure with budget left goes back to the queue with a due
    /// time, and everything else stops here.
    async fn record_failure(&mut self, id: i64, message: &str, retryable: bool) {
        let Ok(row) = self.row(id).await else {
            return;
        };
        let now = now_ms();
        let attempts = row.attempts + 1;
        let delay = if retryable {
            self.retry_delay(attempts).await
        } else {
            None
        };
        let (status, next_attempt_at) = match delay {
            Some(delay) => (TaskStatus::Queued, Some(now.saturating_add(delay))),
            None => (TaskStatus::Error, None),
        };

        if tasks::set_failure(
            self.store.pool(),
            id,
            status,
            message,
            attempts,
            next_attempt_at,
            now,
        )
        .await
        .is_err()
        {
            return;
        }

        // An attempt that will be repeated is not an outcome. History records
        // what finally happened, so a server that is briefly down does not
        // fill it with rows about one download.
        if status == TaskStatus::Error {
            self.record_history(id, Outcome::Failed).await;
        }
        self.emit_changed(id).await;
    }

    /// The wait before the next attempt, or `None` when the budget is spent.
    async fn retry_delay(&self, attempts: i64) -> Option<i64> {
        let pool = self.store.pool();
        let max = settings::get_i64(pool, settings::RETRY_MAX_ATTEMPTS, 3)
            .await
            .unwrap_or(3);
        if max <= 0 || attempts >= max {
            return None;
        }
        let base = settings::get_i64(pool, settings::RETRY_BASE_MS, 5_000)
            .await
            .unwrap_or(5_000);
        let cap = settings::get_i64(pool, settings::RETRY_CAP_MS, 300_000)
            .await
            .unwrap_or(300_000);
        Some(scheduler::retry_delay_ms(base, cap, attempts))
    }

    async fn record_history(&self, id: i64, outcome: Outcome) {
        let Ok(row) = self.row(id).await else {
            return;
        };
        let entry = NewHistory {
            task_id: id,
            url: row.url.clone(),
            file_name: row.file_name.clone(),
            dest_path: row.dest_path.clone(),
            outcome,
            total_bytes: row.total_bytes,
            observed_sha256: row.observed_sha256.clone(),
            error_message: row.error_message.clone(),
            started_at: row.started_at,
            ended_at: now_ms(),
        };
        let _ = history::insert(self.store.pool(), &entry).await;
    }

    async fn stop_all(&mut self) {
        let handles: Vec<DownloadHandle> = self.running.drain().map(|(_, handle)| handle).collect();
        for handle in handles {
            handle.pause().await;
        }
    }

    async fn row(&self, id: i64) -> Result<TaskRow, HostError> {
        tasks::get(self.store.pool(), id)
            .await?
            .ok_or(HostError::NotFound {
                entity: "task",
                id,
            })
    }

    async fn view(&self, id: i64) -> Result<TaskView, HostError> {
        Ok(TaskView::from_row(&self.row(id).await?))
    }

    async fn emit_changed(&self, id: i64) {
        if let Ok(row) = self.row(id).await {
            self.sink.emit(HostEvent::TaskChanged {
                task: TaskView::from_row(&row),
            });
        }
    }
}
