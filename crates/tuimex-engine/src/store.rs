//! The store's own thread (ADR-006). The connection threads hand it work
//! and wait for the answer; nothing else holds the store, so the Maildir
//! and the index see one change at a time.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use tuimex_store::Store;

use crate::Error;

enum Job {
    Work(Box<dyn FnOnce(&Store) + Send>),
    Finish,
}

/// A way to reach the store's thread. The thread ends at `finish`, or
/// when the last handle is dropped.
#[derive(Clone)]
pub struct StoreHandle {
    jobs: Sender<Job>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl StoreHandle {
    pub fn start(store: Store) -> StoreHandle {
        let (jobs, queue) = mpsc::channel::<Job>();
        let thread = thread::spawn(move || {
            for job in queue {
                match job {
                    Job::Work(work) => work(&store),
                    Job::Finish => break,
                }
            }
        });
        StoreHandle { jobs, thread: Arc::new(Mutex::new(Some(thread))) }
    }

    /// Carries out everything handed over so far, closes the store and
    /// waits for both. Work is handed over without waiting for it, so a
    /// program that left without this could leave before a draft just
    /// saved or a move just made was written.
    pub fn finish(&self) {
        // A thread that has ended has nothing left to carry out.
        let _ = self.jobs.send(Job::Finish);
        // Whoever panicked holding this was only taking the thread out.
        let thread = self.thread.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(thread) = thread {
            // One that panicked has said so on standard error, and what
            // it had written is on disk.
            let _ = thread.join();
        }
    }

    /// Hands `work` to the store's thread without waiting for it, and
    /// says whether there was a thread to hand it to.
    pub(crate) fn hand(&self, work: impl FnOnce(&Store) + Send + 'static) -> bool {
        self.jobs.send(Job::Work(Box::new(work))).is_ok()
    }

    /// Runs `work` on the store's thread and waits for what it returns.
    pub fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Store) -> tuimex_store::Result<T> + Send + 'static,
    ) -> Result<T, Error> {
        let (answer, answered) = mpsc::channel();
        // The caller waits below until this is sent or dropped, so it is
        // there to receive; were it gone, nobody wants the answer.
        if !self.hand(move |store| _ = answer.send(work(store))) {
            return Err(Error::StoreStopped);
        }
        Ok(answered.recv().map_err(|_| Error::StoreStopped)??)
    }
}
