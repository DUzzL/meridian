//! A small pool of background threads.
//!
//! Jobs are closures; their results come back through [`WorkerPool::poll`]
//! on the thread that owns the pool (the game's main thread). Dropping the
//! pool finishes the queued jobs (so pending saves reach the disk) and joins
//! the threads.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use parking_lot::Mutex;

type Job<T> = Box<dyn FnOnce() -> T + Send + 'static>;

/// Run by each pool thread as it starts: the host lowers the thread's
/// priority there (tiles, loads and saves are nothing a frame waits for).
pub static THREAD_START: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

pub struct WorkerPool<T: Send + 'static> {
    jobs: Option<Sender<Job<T>>>,
    results: Receiver<T>,
    threads: Vec<JoinHandle<()>>,
    /// Jobs submitted whose results were not polled yet.
    outstanding: Arc<AtomicUsize>,
}

impl<T: Send + 'static> std::fmt::Debug for WorkerPool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerPool")
            .field("threads", &self.threads.len())
            .field("outstanding", &self.outstanding())
            .finish()
    }
}

impl<T: Send + 'static> WorkerPool<T> {
    pub fn new(threads: usize, name: &str) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job<T>>();
        let (result_tx, results) = mpsc::channel::<T>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let threads = (0..threads.max(1))
            .map(|i| {
                let job_rx = Arc::clone(&job_rx);
                let result_tx = result_tx.clone();
                std::thread::Builder::new()
                    .name(format!("{name}-{i}"))
                    .spawn(move || {
                        if let Some(start) = THREAD_START.get() {
                            start();
                        }
                        loop {
                            // Hold the lock only while taking a job.
                            let job = job_rx.lock().recv();
                            let Ok(job) = job else { break };
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                            match result {
                                Ok(r) => {
                                    if result_tx.send(r).is_err() {
                                        break;
                                    }
                                }
                                Err(_) => log::error!("meridian: a background job panicked"),
                            }
                        }
                    })
                    .expect("cannot start a worker thread")
            })
            .collect();
        Self {
            jobs: Some(job_tx),
            results,
            threads,
            outstanding: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn submit(&self, job: impl FnOnce() -> T + Send + 'static) {
        let outstanding = Arc::clone(&self.outstanding);
        outstanding.fetch_add(1, Ordering::SeqCst);
        let job: Job<T> = Box::new(job);
        // A panicking job never answers; count it as done when it unwinds.
        let guarded: Job<T> = Box::new(move || {
            struct Done(Arc<AtomicUsize>, bool);
            impl Drop for Done {
                fn drop(&mut self) {
                    if !self.1 {
                        self.0.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
            let mut done = Done(outstanding, false);
            let r = job();
            done.1 = true;
            r
        });
        if let Some(jobs) = &self.jobs {
            jobs.send(guarded)
                .expect("workers outlive the pool's sender");
        }
    }

    /// The next finished result, if any; never blocks.
    pub fn poll(&self) -> Option<T> {
        let r = self.results.try_recv().ok()?;
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
        Some(r)
    }

    /// Waits for the next result (tests and shutdown).
    pub fn wait(&self) -> Option<T> {
        // Re-check the count while waiting: a job that panics never answers.
        while self.outstanding() > 0 {
            match self
                .results
                .recv_timeout(std::time::Duration::from_millis(5))
            {
                Ok(r) => {
                    self.outstanding.fetch_sub(1, Ordering::SeqCst);
                    return Some(r);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
        None
    }

    /// The next result, waiting at most `timeout` for one.
    pub fn wait_timeout(&self, timeout: std::time::Duration) -> Option<T> {
        let r = self.results.recv_timeout(timeout).ok()?;
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
        Some(r)
    }

    /// Jobs not yet polled.
    pub fn outstanding(&self) -> usize {
        self.outstanding.load(Ordering::SeqCst)
    }
}

impl<T: Send + 'static> Drop for WorkerPool<T> {
    fn drop(&mut self) {
        // Closing the job channel lets each thread finish the queue and exit.
        self.jobs.take();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn runs_jobs_on_workers() {
        let pool = WorkerPool::new(3, "test");
        for i in 0..50u32 {
            pool.submit(move || i * 2);
        }
        let mut got = Vec::new();
        while let Some(r) = pool.wait() {
            got.push(r);
        }
        got.sort();
        assert_eq!(got, (0..50).map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(pool.outstanding(), 0);
        assert!(pool.poll().is_none());
    }

    #[test]
    fn drop_finishes_queued_jobs() {
        let ran = Arc::new(AtomicU32::new(0));
        {
            let pool = WorkerPool::new(1, "test");
            for _ in 0..20 {
                let ran = Arc::clone(&ran);
                pool.submit(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    ran.fetch_add(1, Ordering::SeqCst);
                });
            }
        }
        assert_eq!(ran.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn a_panicking_job_does_not_stop_the_pool() {
        let pool = WorkerPool::new(1, "test");
        pool.submit(|| -> u32 { panic!("boom") });
        pool.submit(|| 7);
        assert_eq!(pool.wait(), Some(7));
        assert_eq!(pool.outstanding(), 0);
    }
}
