//! Concurrent transfer pool used by the parallel executor path.
//!
//! Design:
//! - bounded upstream queue (`sync_channel`) for backpressure
//! - single dispatcher thread with round-robin worker routing
//! - bounded per-worker inboxes
//! - bounded result queue drained by coordinator

use crate::types::{KopyError, SyncAction};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{
    self, Receiver, RecvError, RecvTimeoutError, SyncSender, TryRecvError, TrySendError,
};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Work item accepted by the transfer pool.
#[derive(Debug, Clone)]
pub struct TransferJob {
    pub index: usize,
    pub total: usize,
    pub action_name: &'static str,
    pub path: Option<PathBuf>,
    pub action: SyncAction,
}

/// Result emitted by workers.
#[derive(Debug)]
pub struct TransferResult {
    pub index: usize,
    pub total: usize,
    pub action_name: &'static str,
    pub path: Option<PathBuf>,
    pub result: Result<u64, KopyError>,
}

/// Runtime stats for transfer pool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolStats {
    pub workers: usize,
    pub enqueued: usize,
    pub dispatched: usize,
    pub completed: usize,
    pub per_worker_completed: Vec<usize>,
}

impl PoolStats {
    fn new(workers: usize) -> Self {
        Self {
            workers,
            enqueued: 0,
            dispatched: 0,
            completed: 0,
            per_worker_completed: vec![0; workers],
        }
    }
}

type JobHandler = dyn Fn(TransferJob) -> TransferResult + Send + Sync + 'static;

/// Reusable transfer executor with bounded queues and explicit shutdown.
pub struct ParallelExecutor {
    enqueue_tx: Option<SyncSender<TransferJob>>,
    result_rx: Receiver<TransferResult>,
    dispatcher_handle: Option<JoinHandle<()>>,
    worker_handles: Vec<JoinHandle<()>>,
    stats: Arc<Mutex<PoolStats>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueError {
    Full,
    Closed,
}

impl ParallelExecutor {
    /// Create a dispatcher + worker pool.
    pub fn new(
        worker_count: usize,
        queue_capacity: usize,
        handler: Arc<JobHandler>,
    ) -> Result<Self, KopyError> {
        let workers = worker_count.max(1);
        let capacity = queue_capacity.max(1);

        let stats = Arc::new(Mutex::new(PoolStats::new(workers)));
        let (enqueue_tx, enqueue_rx) = mpsc::sync_channel::<TransferJob>(capacity);
        let (result_tx, result_rx) = mpsc::sync_channel::<TransferResult>(capacity);

        let mut worker_txs = Vec::with_capacity(workers);
        let mut worker_handles = Vec::with_capacity(workers);

        for worker_id in 0..workers {
            let (worker_tx, worker_rx) = mpsc::sync_channel::<TransferJob>(capacity);
            worker_txs.push(worker_tx);

            worker_handles.push(spawn_worker(
                worker_id,
                worker_rx,
                result_tx.clone(),
                Arc::clone(&stats),
                Arc::clone(&handler),
            ));
        }

        let dispatcher_handle = Some(spawn_dispatcher(enqueue_rx, worker_txs, Arc::clone(&stats)));

        Ok(Self {
            enqueue_tx: Some(enqueue_tx),
            result_rx,
            dispatcher_handle,
            worker_handles,
            stats,
        })
    }

    /// Try to enqueue a job into the upstream dispatcher queue.
    pub fn try_enqueue(&self, job: TransferJob) -> Result<(), EnqueueError> {
        let sender = self.enqueue_tx.as_ref().ok_or(EnqueueError::Closed)?;

        match sender.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(EnqueueError::Full),
            Err(TrySendError::Disconnected(_)) => return Err(EnqueueError::Closed),
        }

        let mut guard = self.stats.lock().map_err(|_| EnqueueError::Closed)?;
        guard.enqueued += 1;
        Ok(())
    }

    /// Try receiving one completed result without blocking.
    pub fn try_recv_result(&self) -> Result<Option<TransferResult>, KopyError> {
        match self.result_rx.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(KopyError::Validation(
                "parallel executor result channel closed unexpectedly".to_string(),
            )),
        }
    }

    /// Receive one completed result, blocking until available.
    pub fn recv_result(&self) -> Result<TransferResult, KopyError> {
        self.result_rx.recv().map_err(|_: RecvError| {
            KopyError::Validation(
                "parallel executor result channel closed unexpectedly".to_string(),
            )
        })
    }

    /// Close queue input. Already in-flight jobs continue processing.
    pub fn close_input(&mut self) {
        self.enqueue_tx.take();
    }

    /// Wait for dispatcher/workers to exit and return pool stats.
    pub fn close_and_wait(mut self) -> Result<PoolStats, KopyError> {
        self.close_input();

        while self.dispatcher_handle.is_some() || !self.worker_handles.is_empty() {
            drain_results_nonblocking(&self.result_rx);

            if let Some(dispatcher) = self.dispatcher_handle.as_ref() {
                if dispatcher.is_finished() {
                    let dispatcher = self.dispatcher_handle.take().expect("dispatcher present");
                    dispatcher.join().map_err(|_| {
                        KopyError::Validation("parallel dispatcher thread panicked".to_string())
                    })?;
                }
            }

            let mut joined_any = false;
            let mut idx = 0usize;
            while idx < self.worker_handles.len() {
                if self.worker_handles[idx].is_finished() {
                    let handle = self.worker_handles.swap_remove(idx);
                    handle.join().map_err(|_| {
                        KopyError::Validation("parallel worker thread panicked".to_string())
                    })?;
                    joined_any = true;
                } else {
                    idx += 1;
                }
            }

            if joined_any {
                continue;
            }

            // If workers are blocked trying to emit results, this receive frees queue space.
            match self.result_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        drain_results_nonblocking(&self.result_rx);

        self.stats
            .lock()
            .map(|stats| stats.clone())
            .map_err(|_| KopyError::Validation("parallel executor stats lock poisoned".to_string()))
    }
}

fn spawn_dispatcher(
    enqueue_rx: Receiver<TransferJob>,
    worker_txs: Vec<SyncSender<TransferJob>>,
    stats: Arc<Mutex<PoolStats>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let worker_len = worker_txs.len();
        if worker_len == 0 {
            return;
        }

        let mut next_worker = 0usize;

        while let Ok(job) = enqueue_rx.recv() {
            let mut pending = Some(job);
            while let Some(current_job) = pending.take() {
                let mut disconnected_count = 0usize;
                let mut sent = false;
                let mut observed_full = false;
                for attempt in 0..worker_len {
                    let idx = (next_worker + attempt) % worker_len;
                    match worker_txs[idx].try_send(current_job.clone()) {
                        Ok(()) => {
                            if let Ok(mut guard) = stats.lock() {
                                guard.dispatched += 1;
                            }
                            next_worker = (idx + 1) % worker_len;
                            sent = true;
                            break;
                        }
                        Err(TrySendError::Full(_)) => {
                            observed_full = true;
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            disconnected_count += 1;
                        }
                    }
                }

                if disconnected_count == worker_len {
                    return;
                }
                if sent {
                    continue;
                }
                if observed_full {
                    pending = Some(current_job);
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                // No sends succeeded and no workers reported `Full`: remaining workers disconnected.
                return;
            }
        }
        // worker_txs are dropped here, which closes worker inboxes.
    })
}

fn drain_results_nonblocking(result_rx: &Receiver<TransferResult>) {
    while result_rx.try_recv().is_ok() {}
}

fn spawn_worker(
    worker_id: usize,
    worker_rx: Receiver<TransferJob>,
    result_tx: SyncSender<TransferResult>,
    stats: Arc<Mutex<PoolStats>>,
    handler: Arc<JobHandler>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(job) = worker_rx.recv() {
            let output = panic::catch_unwind(AssertUnwindSafe(|| (handler)(job.clone())));
            let result = match output {
                Ok(result) => result,
                Err(_) => TransferResult {
                    index: job.index,
                    total: job.total,
                    action_name: job.action_name,
                    path: job.path,
                    result: Err(KopyError::Validation(
                        "parallel worker panicked while processing transfer job".to_string(),
                    )),
                },
            };

            if result_tx.send(result).is_err() {
                break;
            }

            if let Ok(mut guard) = stats.lock() {
                guard.completed += 1;
                if let Some(slot) = guard.per_worker_completed.get_mut(worker_id) {
                    *slot += 1;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn passthrough_handler() -> Arc<JobHandler> {
        Arc::new(|job: TransferJob| TransferResult {
            index: job.index,
            total: job.total,
            action_name: job.action_name,
            path: job.path,
            result: Ok(0),
        })
    }

    #[test]
    fn test_parallel_executor_dispatches_jobs_across_workers() {
        let pool = ParallelExecutor::new(4, 256, passthrough_handler()).expect("create pool");
        for i in 0..64 {
            pool.try_enqueue(TransferJob {
                index: i,
                total: 64,
                action_name: "skip",
                path: None,
                action: SyncAction::Skip,
            })
            .expect("enqueue");
        }

        let mut seen = 0usize;
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen < 64 {
            if Instant::now() > deadline {
                panic!(
                    "timed out waiting for worker results: received {seen}/64 results before deadline"
                );
            }

            match pool.try_recv_result().expect("try recv result") {
                Some(_) => seen += 1,
                None => thread::sleep(Duration::from_millis(1)),
            }
        }

        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.workers, 4);
        assert_eq!(stats.enqueued, 64);
        assert_eq!(stats.dispatched, 64);
        assert_eq!(stats.completed, 64);
        assert!(
            stats
                .per_worker_completed
                .iter()
                .filter(|&&c| c > 0)
                .count()
                > 1,
            "expected jobs distributed across multiple workers"
        );
    }

    #[test]
    fn test_try_enqueue_reports_full_without_blocking() {
        let pool = ParallelExecutor::new(1, 1, passthrough_handler()).expect("create pool");
        pool.try_enqueue(TransferJob {
            index: 0,
            total: 2,
            action_name: "skip",
            path: None,
            action: SyncAction::Skip,
        })
        .expect("first enqueue");

        let second = pool.try_enqueue(TransferJob {
            index: 1,
            total: 2,
            action_name: "skip",
            path: None,
            action: SyncAction::Skip,
        });
        assert_eq!(second, Err(EnqueueError::Full));

        let _ = pool.recv_result().expect("drain one result");
        let _ = pool.close_and_wait().expect("close and wait");
    }

    #[test]
    fn test_parallel_executor_shutdowns_cleanly_without_jobs() {
        let pool = ParallelExecutor::new(2, 8, passthrough_handler()).expect("create pool");
        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.enqueued, 0);
        assert_eq!(stats.dispatched, 0);
        assert_eq!(stats.completed, 0);
    }

    #[test]
    fn test_parallel_executor_enforces_minimum_one_worker() {
        let pool = ParallelExecutor::new(0, 4, passthrough_handler()).expect("create pool");
        while pool
            .try_enqueue(TransferJob {
                index: 0,
                total: 1,
                action_name: "skip",
                path: None,
                action: SyncAction::Skip,
            })
            .is_err()
        {
            let _ = pool.recv_result().expect("recv result while enqueueing");
        }
        let _ = pool.recv_result().expect("recv result");

        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.workers, 1);
        assert_eq!(stats.completed, 1);
    }

    #[test]
    fn test_close_and_wait_drains_results_without_explicit_receives() {
        let pool = ParallelExecutor::new(2, 128, passthrough_handler()).expect("create pool");
        for i in 0..64 {
            while pool
                .try_enqueue(TransferJob {
                    index: i,
                    total: 64,
                    action_name: "skip",
                    path: None,
                    action: SyncAction::Skip,
                })
                .is_err()
            {
                let _ = pool.recv_result().expect("recv result while enqueueing");
            }
        }

        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.enqueued, 64);
        assert_eq!(stats.completed, 64);
    }

    #[test]
    fn test_dispatcher_routes_around_full_worker_inbox() {
        let stats = Arc::new(Mutex::new(PoolStats::new(2)));
        let (enqueue_tx, enqueue_rx) = mpsc::sync_channel::<TransferJob>(8);
        let (w0_tx, _w0_rx) = mpsc::sync_channel::<TransferJob>(1);
        let (w1_tx, w1_rx) = mpsc::sync_channel::<TransferJob>(1);
        let delivered_to_w1 = Arc::new(Mutex::new(0usize));
        let delivered_ref = Arc::clone(&delivered_to_w1);

        let consumer = thread::spawn(move || {
            while w1_rx.recv().is_ok() {
                if let Ok(mut count) = delivered_ref.lock() {
                    *count += 1;
                }
            }
        });

        let dispatcher = spawn_dispatcher(enqueue_rx, vec![w0_tx, w1_tx], Arc::clone(&stats));

        for i in 0..3 {
            enqueue_tx
                .send(TransferJob {
                    index: i,
                    total: 3,
                    action_name: "skip",
                    path: None,
                    action: SyncAction::Skip,
                })
                .expect("enqueue job");
        }
        drop(enqueue_tx);

        dispatcher.join().expect("dispatcher join");
        consumer.join().expect("consumer join");

        // One worker inbox is intentionally unread/full; dispatcher should still route work to worker 1.
        let worker1_count = *delivered_to_w1.lock().expect("delivered count lock");
        assert!(
            worker1_count >= 1,
            "expected dispatcher to route around full inbox"
        );
    }
}
