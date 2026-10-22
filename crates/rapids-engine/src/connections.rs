//! One app-wide budget for active HTTP attempts.
//!
//! A lease is held from before a probe or transfer request until its response
//! is dropped. Workers waiting for a lease hold no network connection, and a
//! retry returns its lease during backoff. Shrinking the budget never takes a
//! live request's lease away; it withholds new leases until the active count
//! falls below the new limit.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::watch;

const STOP_CHECK: Duration = Duration::from_millis(250);

#[derive(Debug)]
struct Counts {
    limit: usize,
    in_use: usize,
}

#[derive(Debug)]
pub struct ConnectionBudget {
    counts: Mutex<Counts>,
    changed: watch::Sender<u64>,
}

pub(crate) struct ConnectionLease {
    budget: Arc<ConnectionBudget>,
}

impl ConnectionBudget {
    pub fn new(limit: usize) -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            counts: Mutex::new(Counts {
                limit: limit.max(1),
                in_use: 0,
            }),
            changed,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Counts> {
        self.counts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn limit(&self) -> usize {
        self.lock().limit
    }

    /// Current active probes and HTTP attempts, not parked workers.
    pub fn in_use(&self) -> usize {
        self.lock().in_use
    }

    /// New attempts obey a smaller limit as existing leases return. An
    /// increased limit wakes waiting workers without restarting downloads.
    pub fn set_limit(&self, limit: usize) {
        let changed = {
            let mut counts = self.lock();
            let limit = limit.max(1);
            if counts.limit == limit {
                false
            } else {
                counts.limit = limit;
                true
            }
        };
        if changed {
            self.wake();
        }
    }

    fn wake(&self) {
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    /// Waits without holding an HTTP connection. The callback is checked at
    /// most one short slice after a pause, cancellation, or stop request.
    pub(crate) async fn acquire_while<F>(
        self: &Arc<Self>,
        mut keep_going: F,
    ) -> Option<ConnectionLease>
    where
        F: FnMut() -> bool,
    {
        let mut changed = self.changed.subscribe();
        loop {
            if !keep_going() {
                return None;
            }
            let acquired = {
                let mut counts = self.lock();
                if counts.in_use < counts.limit {
                    counts.in_use += 1;
                    true
                } else {
                    false
                }
            };
            if acquired {
                return Some(ConnectionLease {
                    budget: Arc::clone(self),
                });
            }
            tokio::select! {
                _ = changed.changed() => {}
                _ = tokio::time::sleep(STOP_CHECK) => {}
            }
        }
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        {
            let mut counts = self.budget.lock();
            counts.in_use -= 1;
        }
        self.budget.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test(start_paused = true)]
    async fn lowering_the_budget_waits_for_existing_leases_to_return() {
        let budget = Arc::new(ConnectionBudget::new(2));
        let first = budget.acquire_while(|| true).await.expect("first lease");
        let second = budget.acquire_while(|| true).await.expect("second lease");
        assert_eq!(budget.in_use(), 2);

        budget.set_limit(1);
        drop(first);
        assert_eq!(budget.in_use(), 1);
        let waiting = Arc::clone(&budget);
        let waiter =
            tokio::spawn(async move { waiting.acquire_while(|| true).await.expect("new lease") });
        tokio::task::yield_now().await;
        assert!(
            !waiter.is_finished(),
            "the remaining lease uses the new cap"
        );
        drop(second);
        let third = waiter.await.expect("waiter joined");
        assert_eq!(budget.in_use(), 1);
        drop(third);
        assert_eq!(budget.in_use(), 0);
    }

    #[tokio::test]
    async fn a_waiting_worker_can_stop_without_a_free_connection() {
        let budget = Arc::new(ConnectionBudget::new(1));
        let held = budget.acquire_while(|| true).await.expect("held lease");
        let keep_going = Arc::new(AtomicBool::new(true));
        let waiting = Arc::clone(&budget);
        let flag = Arc::clone(&keep_going);
        let waiter = tokio::spawn(async move {
            waiting
                .acquire_while(|| flag.load(Ordering::Acquire))
                .await
                .is_none()
        });
        tokio::task::yield_now().await;
        keep_going.store(false, Ordering::Release);
        assert!(
            tokio::time::timeout(Duration::from_secs(2), waiter)
                .await
                .expect("stop promptly")
                .expect("waiter joined")
        );
        drop(held);
        assert_eq!(budget.in_use(), 0);
    }
}
