//! Concurrent transfer pool scaffolding for Phase 2.
//!
//! This module provides a dispatcher + worker inbox design:
//! - single-consumer upstream `mpsc::Receiver` (dispatcher)
//! - per-worker `mpsc` inbox channels
//! - explicit sender drop on shutdown before awaiting workers

use crate::types::{KopyError, SyncAction};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use std::sync::Arc;

/// Work item accepted by the transfer pool.
#[derive(Debug, Clone)]
pub struct TransferJob {
    pub index: usize,
    pub action: SyncAction,
}

/// Runtime stats for transfer pool scaffolding.
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

/// Thread-pool executor scaffold for concurrent transfer infrastructure.
pub struct ParallelExecutor {
    runtime: Runtime,
    enqueue_tx: Option<mpsc::Sender<TransferJob>>,
    dispatcher_handle: Option<JoinHandle<()>>,
    worker_handles: Vec<JoinHandle<()>>,
    stats: Arc<Mutex<PoolStats>>,
}

impl ParallelExecutor {
    /// Create a dispatcher + worker pool with bounded channels.
    pub fn new(worker_count: usize, queue_capacity: usize) -> Result<Self, KopyError> {
        let workers = worker_count.max(1);
        let capacity = queue_capacity.max(1);
        let runtime = Builder::new_multi_thread()
            .worker_threads(workers)
            .enable_all()
            .build()
            .map_err(KopyError::Io)?;

        let stats = Arc::new(Mutex::new(PoolStats::new(workers)));
        let handle = runtime.handle().clone();

        let (enqueue_tx, enqueue_rx) = mpsc::channel::<TransferJob>(capacity);

        let mut worker_txs = Vec::with_capacity(workers);
        let mut worker_handles = Vec::with_capacity(workers);
        for worker_id in 0..workers {
            let (worker_tx, worker_rx) = mpsc::channel::<TransferJob>(capacity);
            worker_txs.push(worker_tx);
            worker_handles.push(handle.spawn(worker_loop(
                worker_id,
                worker_rx,
                Arc::clone(&stats),
            )));
        }

        let dispatcher_handle =
            handle.spawn(dispatcher_loop(enqueue_rx, worker_txs, Arc::clone(&stats)));

        Ok(Self {
            runtime,
            enqueue_tx: Some(enqueue_tx),
            dispatcher_handle: Some(dispatcher_handle),
            worker_handles,
            stats,
        })
    }

    /// Enqueue a job into upstream dispatcher queue.
    pub fn enqueue(&self, job: TransferJob) -> Result<(), KopyError> {
        let sender = self.enqueue_tx.as_ref().ok_or_else(|| {
            KopyError::Validation("parallel executor queue is already closed".to_string())
        })?;
        let stats = Arc::clone(&self.stats);

        self.runtime.block_on(async {
            sender.send(job).await.map_err(|_| {
                KopyError::Validation("parallel executor queue receiver is closed".to_string())
            })?;

            let mut guard = stats.lock().await;
            guard.enqueued += 1;
            Ok(())
        })
    }

    /// Close queue input and wait for dispatcher/workers to exit cleanly.
    pub fn close_and_wait(mut self) -> Result<PoolStats, KopyError> {
        self.enqueue_tx.take();

        let dispatcher = self.dispatcher_handle.take();
        let workers = std::mem::take(&mut self.worker_handles);
        let stats = Arc::clone(&self.stats);

        self.runtime.block_on(async move {
            if let Some(handle) = dispatcher {
                handle.await.map_err(map_join_error)?;
            }
            for handle in workers {
                handle.await.map_err(map_join_error)?;
            }
            Ok(stats.lock().await.clone())
        })
    }
}

async fn dispatcher_loop(
    mut enqueue_rx: mpsc::Receiver<TransferJob>,
    worker_txs: Vec<mpsc::Sender<TransferJob>>,
    stats: Arc<Mutex<PoolStats>>,
) {
    let mut next_worker = 0usize;
    let worker_len = worker_txs.len();

    while let Some(job) = enqueue_rx.recv().await {
        if worker_len == 0 {
            break;
        }

        let target = next_worker % worker_len;
        if worker_txs[target].send(job).await.is_ok() {
            let mut guard = stats.lock().await;
            guard.dispatched += 1;
            next_worker = (next_worker + 1) % worker_len;
        }
    }
    // worker_txs are dropped here, which closes worker inboxes.
}

async fn worker_loop(
    worker_id: usize,
    mut worker_rx: mpsc::Receiver<TransferJob>,
    stats: Arc<Mutex<PoolStats>>,
) {
    while let Some(_job) = worker_rx.recv().await {
        let mut guard = stats.lock().await;
        guard.completed += 1;
        if let Some(slot) = guard.per_worker_completed.get_mut(worker_id) {
            *slot += 1;
        }
    }
}

fn map_join_error(error: tokio::task::JoinError) -> KopyError {
    KopyError::Validation(format!("parallel executor task failed: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_executor_dispatches_jobs_across_workers() {
        let pool = ParallelExecutor::new(4, 32).expect("create pool");
        for i in 0..64 {
            pool.enqueue(TransferJob {
                index: i,
                action: SyncAction::Skip,
            })
            .expect("enqueue");
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
    fn test_parallel_executor_shutdowns_cleanly_without_jobs() {
        let pool = ParallelExecutor::new(2, 8).expect("create pool");
        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.enqueued, 0);
        assert_eq!(stats.dispatched, 0);
        assert_eq!(stats.completed, 0);
    }

    #[test]
    fn test_parallel_executor_enforces_minimum_one_worker() {
        let pool = ParallelExecutor::new(0, 4).expect("create pool");
        pool.enqueue(TransferJob {
            index: 0,
            action: SyncAction::Skip,
        })
        .expect("enqueue");
        let stats = pool.close_and_wait().expect("close and wait");
        assert_eq!(stats.workers, 1);
        assert_eq!(stats.completed, 1);
    }
}
