//! Shared model ownership and conservative idle eviction.
use crate::error::AppResult;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

pub(super) struct ModelCache<T> {
    state: Mutex<State<T>>,
    evicted: AtomicBool,
}

struct State<T> {
    model: Option<Arc<T>>,
    idle_since: Option<Instant>,
}

impl<T> Default for ModelCache<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                model: None,
                idle_since: None,
            }),
            evicted: AtomicBool::new(false),
        }
    }
}

impl<T> ModelCache<T> {
    pub fn get_or_try_init(&self, load: impl FnOnce() -> AppResult<T>) -> AppResult<Arc<T>> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Self::load(&mut state, load)
    }

    fn load(state: &mut State<T>, load: impl FnOnce() -> AppResult<T>) -> AppResult<Arc<T>> {
        if state.model.is_none() {
            state.model = Some(Arc::new(load()?));
        }
        // Acquiring even briefly between cleanup polls starts a fresh idle period.
        state.idle_since = None;
        Ok(state.model.as_ref().unwrap().clone())
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.model = None;
        state.idle_since = None;
        self.evicted.store(false, Ordering::Release);
    }
    pub fn was_evicted(&self) -> bool {
        self.evicted.load(Ordering::Acquire)
    }

    pub fn get_for_warmup(&self, load: impl FnOnce() -> AppResult<T>) -> AppResult<Option<Arc<T>>> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // Recheck under the same lock as eviction, including an already scheduled warmup.
        if self.was_evicted() {
            return Ok(None);
        }
        Self::load(&mut state, load).map(Some)
    }

    pub fn inspect<R>(&self, read: impl FnOnce(&T) -> R) -> Option<R> {
        // Readiness polling neither waits for initialization nor prolongs residency.
        let state = self.state.try_lock().ok()?;
        state.model.as_deref().map(read)
    }

    pub fn release_if_idle(&self, now: Instant, timeout: Duration) -> bool {
        let Ok(mut state) = self.state.try_lock() else {
            return false;
        };
        let Some(model) = state.model.as_ref() else {
            return false;
        };
        if Arc::strong_count(model) > 1 {
            state.idle_since = None;
            return false;
        }
        let idle_since = *state.idle_since.get_or_insert(now);
        if now.saturating_duration_since(idle_since) < timeout {
            return false;
        }
        self.evicted.store(true, Ordering::Release);
        // Finish unloading before a new caller initializes another model. Status reads use try_lock.
        drop(state.model.take());
        state.idle_since = None;
        true
    }
}

#[cfg(test)]
#[path = "../../tests/unit/clip_resident.rs"]
mod tests;
