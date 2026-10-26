//! The economy worker: one thread per main server owns the mod realm's
//! [`Economy`] (its SQLite connection to `realm.sqlite`) and runs the
//! jobs given to it one after the other, in the order they came.
//!
//! Every economy call of the main server goes through it: the shared
//! stash's moves ([`crate::realm::stash`]) now, the market's later. A job
//! is a closure on the economy; [`Bank::run`] waits for its result
//! without blocking the network threads, [`Bank::spawn`] doesn't wait.

use std::sync::mpsc;

use d2_economy::Economy;
use tokio::sync::oneshot;

type Job = Box<dyn FnOnce(&mut Economy) + Send>;

/// The economy worker. Cloning shares it; the thread ends when the last
/// clone is dropped.
#[derive(Clone, Debug)]
pub struct Bank {
    jobs: mpsc::Sender<Job>,
}

impl Bank {
    /// Starts the worker thread with `economy`.
    pub fn start(mut economy: Economy) -> Result<Bank, String> {
        let (jobs, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("d2rust-realm-economy".into())
            .spawn(move || {
                for job in rx {
                    job(&mut economy);
                }
            })
            .map_err(|e| format!("starting the economy worker: {e}"))?;
        Ok(Bank { jobs })
    }

    /// Runs `f` on the economy and returns its result. `Err`: the worker
    /// is gone (it stopped on a panic).
    pub async fn run<T, F>(&self, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut Economy) -> T + Send + 'static,
    {
        self.queue(f)?.await.map_err(|_| GONE.to_owned())
    }

    /// [`Bank::run`] from a thread outside the runtime (the main server's
    /// start).
    pub fn run_blocking<T, F>(&self, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut Economy) -> T + Send + 'static,
    {
        self.queue(f)?.blocking_recv().map_err(|_| GONE.to_owned())
    }

    /// Queues `f` without waiting for it.
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&mut Economy) + Send + 'static,
    {
        let _ = self.jobs.send(Box::new(f));
    }

    fn queue<T, F>(&self, f: F) -> Result<oneshot::Receiver<T>, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut Economy) -> T + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.jobs
            .send(Box::new(move |e: &mut Economy| {
                let _ = tx.send(f(e));
            }))
            .map_err(|_| GONE.to_owned())?;
        Ok(rx)
    }
}

const GONE: &str = "the economy worker stopped";

#[cfg(test)]
mod tests {
    use super::*;
    use d2_economy::EconomyConfig;

    #[test]
    fn jobs_run_in_order_on_one_economy() {
        let e = Economy::in_memory("d2rust", EconomyConfig::default()).unwrap();
        let bank = Bank::start(e).unwrap();
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        for i in 0..20 {
            let log = log.clone();
            bank.spawn(move |_| log.lock().unwrap().push(i));
        }
        let mod_id = bank.run_blocking(|e| e.mod_id().to_owned()).unwrap();
        assert_eq!(mod_id, "d2rust");
        assert_eq!(*log.lock().unwrap(), (0..20).collect::<Vec<_>>());
    }
}
