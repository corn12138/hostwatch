//! Short background jobs (process scan, optimization analysis). The main thread polls for a
//! result and joins the thread, so no worker outlives its job or the app.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub(super) struct Job<T> {
    handle: JoinHandle<()>,
    slot: Arc<Mutex<Option<T>>>,
}

impl<T: Send + 'static> Job<T> {
    pub(super) fn spawn(
        name: &str,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> std::io::Result<Self> {
        let slot = Arc::new(Mutex::new(None));
        let output = Arc::clone(&slot);
        let handle = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let result = work();
                if let Ok(mut out) = output.lock() {
                    *out = Some(result);
                }
            })?;
        Ok(Self { handle, slot })
    }

    pub(super) fn done(&self) -> bool {
        self.handle.is_finished()
    }

    /// Waits for the thread; `None` if it ended without a result (it panicked).
    pub(super) fn finish(self) -> Option<T> {
        let _ = self.handle.join();
        self.slot.lock().ok().and_then(|mut slot| slot.take())
    }
}
