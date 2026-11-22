use crate::error::{AppError, AppResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Own the shared media-processing slot until retry processing and embedding finish.
pub(crate) struct ProcessingPermit {
    running: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl ProcessingPermit {
    pub(crate) fn claim(running: &Arc<AtomicBool>, cancel: &Arc<AtomicBool>) -> AppResult<Self> {
        running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| AppError::OperationBusy)?;
        cancel.store(false, Ordering::SeqCst);
        Ok(Self {
            running: running.clone(),
            cancel: cancel.clone(),
        })
    }
}

impl Drop for ProcessingPermit {
    fn drop(&mut self) {
        self.cancel.store(false, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/processing_permit.rs"]
mod tests;
