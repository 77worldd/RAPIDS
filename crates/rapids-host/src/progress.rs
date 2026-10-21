//! Coalescing progress samples into batches the shell can absorb.
//!
//! A download reports often — several times a second per task — and a
//! frontend that re-rendered on every sample would spend its frame budget on
//! updates nobody can read. The buffer keeps the newest sample per task and
//! hands them over in one message.
//!
//! The window is measured against a caller-supplied clock, not the wall
//! clock, so batches cannot stretch or collapse when the machine's time is
//! adjusted, and tests decide when a window closes instead of sleeping.

use crate::events::{HostEvent, ProgressUpdate};

/// How long a sample may wait before it is sent.
pub const BATCH_WINDOW_MS: i64 = 500;

/// How many samples one batch may carry. A large queue would otherwise hold
/// every update back until the window closed.
pub const MAX_BATCH: usize = 64;

/// Samples waiting for the next batch.
#[derive(Debug, Default)]
pub struct ProgressBuffer {
    /// At most one sample per task: a newer one replaces the older.
    pending: Vec<ProgressUpdate>,
    /// When the oldest sample arrived, on the caller's clock.
    opened_at: Option<i64>,
}

impl ProgressBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Adds a sample, replacing any earlier sample for the same task.
    ///
    /// Returns a batch when one is due: the window has closed, or the buffer
    /// is full.
    pub fn push(&mut self, update: ProgressUpdate, now: i64) -> Option<HostEvent> {
        match self
            .pending
            .iter_mut()
            .find(|sample| sample.id == update.id)
        {
            Some(sample) => *sample = update,
            None => self.pending.push(update),
        }
        if self.opened_at.is_none() {
            self.opened_at = Some(now);
        }

        if self.pending.len() >= MAX_BATCH {
            return self.flush();
        }
        self.take_due(now)
    }

    /// Returns a batch if the window has closed, adding nothing.
    pub fn take_due(&mut self, now: i64) -> Option<HostEvent> {
        let opened_at = self.opened_at?;
        if now.saturating_sub(opened_at) < BATCH_WINDOW_MS {
            return None;
        }
        self.flush()
    }

    /// Hands over everything pending, whether the window has closed or not.
    ///
    /// Called when a task reaches a terminal state, so the last sample is not
    /// stranded behind a window that will not be checked again.
    pub fn flush(&mut self) -> Option<HostEvent> {
        if self.pending.is_empty() {
            self.opened_at = None;
            return None;
        }
        self.opened_at = None;
        let updates = std::mem::take(&mut self.pending);
        Some(HostEvent::Progress { updates })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: i64, downloaded: i64) -> ProgressUpdate {
        ProgressUpdate {
            id,
            downloaded,
            speed_bps: 1_000,
            eta_seconds: Some(10),
            total_bytes: Some(10_000),
        }
    }

    fn ids(event: &HostEvent) -> Vec<i64> {
        match event {
            HostEvent::Progress { updates } => updates.iter().map(|update| update.id).collect(),
            other => panic!("expected a progress batch, got {other:?}"),
        }
    }

    fn downloaded(event: &HostEvent, id: i64) -> i64 {
        match event {
            HostEvent::Progress { updates } => updates
                .iter()
                .find(|update| update.id == id)
                .map(|update| update.downloaded)
                .expect("the task is in the batch"),
            other => panic!("expected a progress batch, got {other:?}"),
        }
    }

    #[test]
    fn a_sample_waits_for_the_window() {
        let mut buffer = ProgressBuffer::new();
        assert!(buffer.push(sample(1, 100), 0).is_none());
        assert!(buffer.take_due(499).is_none(), "the window is still open");
        let batch = buffer.take_due(500).expect("the window closed");
        assert_eq!(ids(&batch), vec![1]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn repeated_samples_for_one_task_keep_only_the_newest() {
        let mut buffer = ProgressBuffer::new();
        buffer.push(sample(1, 100), 0);
        buffer.push(sample(1, 400), 100);
        buffer.push(sample(1, 900), 200);
        let batch = buffer.take_due(500).expect("due");
        assert_eq!(ids(&batch), vec![1]);
        assert_eq!(downloaded(&batch, 1), 900);
    }

    #[test]
    fn the_window_is_measured_from_the_oldest_sample() {
        let mut buffer = ProgressBuffer::new();
        buffer.push(sample(1, 10), 0);
        // A later sample must not push the deadline out.
        buffer.push(sample(2, 10), 400);
        let batch = buffer.take_due(500).expect("due from the first sample");
        assert_eq!(ids(&batch), vec![1, 2]);
    }

    #[test]
    fn many_tasks_are_sent_together_in_arrival_order() {
        let mut buffer = ProgressBuffer::new();
        // Arrivals start at zero so the first sample opens the window
        // immediately; the deadline is then exactly one window later instead
        // of one window after an arbitrary arrival offset.
        for id in 1..=5 {
            buffer.push(sample(id, id * 10), id - 1);
        }
        let batch = buffer.take_due(500).expect("due");
        assert_eq!(ids(&batch), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_full_buffer_does_not_wait_for_the_window() {
        let mut buffer = ProgressBuffer::new();
        let mut last = None;
        for id in 1..=MAX_BATCH as i64 {
            last = buffer.push(sample(id, 1), 0);
        }
        let batch = last.expect("the batch was handed over when full");
        assert_eq!(ids(&batch).len(), MAX_BATCH);
        assert!(buffer.is_empty());
    }

    #[test]
    fn flushing_sends_everything_pending_and_resets_the_window() {
        let mut buffer = ProgressBuffer::new();
        buffer.push(sample(7, 5), 0);
        let batch = buffer.flush().expect("pending work");
        assert_eq!(ids(&batch), vec![7]);
        assert!(buffer.flush().is_none(), "nothing left to send");

        // A new sample starts a new window.
        assert!(buffer.push(sample(8, 5), 10).is_none());
        assert!(buffer.take_due(509).is_none());
        assert!(buffer.take_due(510).is_some());
    }

    #[test]
    fn nothing_is_emitted_when_nothing_arrived() {
        let mut buffer = ProgressBuffer::new();
        assert!(buffer.take_due(10_000).is_none());
        assert!(buffer.flush().is_none());
    }
}
