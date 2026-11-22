use std::sync::{Arc, Condvar, Mutex, OnceLock};

static INITIAL_SESSION: OnceLock<Arc<Readiness>> = OnceLock::new();

#[derive(Default)]
struct Readiness {
    finished: Mutex<bool>,
    changed: Condvar,
}

impl Readiness {
    fn wait(&self) {
        let mut finished = self.finished.lock().unwrap();
        while !*finished {
            finished = self.changed.wait(finished).unwrap();
        }
    }
}

pub(crate) struct Completion(Arc<Readiness>);

impl Drop for Completion {
    fn drop(&mut self) {
        *self.0.finished.lock().unwrap() = true;
        self.0.changed.notify_all();
    }
}

pub(crate) fn begin() -> Completion {
    Completion(
        INITIAL_SESSION
            .get_or_init(|| Arc::new(Readiness::default()))
            .clone(),
    )
}

/// Called only inside the blocking command worker, never by the window thread.
pub(crate) fn wait_for_initial_session() {
    if let Some(readiness) = INITIAL_SESSION.get() {
        readiness.wait();
    }
}

#[cfg(test)]
#[path = "../tests/unit/library_startup.rs"]
mod tests;
