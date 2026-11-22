use super::*;

#[test]
fn retry_exclusively_owns_processing_and_clears_cancel_on_release() {
    let running = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(true));
    let permit = ProcessingPermit::claim(&running, &cancel).unwrap();
    assert!(running.load(Ordering::SeqCst));
    assert!(!cancel.load(Ordering::SeqCst));
    cancel.store(true, Ordering::SeqCst);
    assert!(ProcessingPermit::claim(&running, &cancel).is_err());
    assert!(
        cancel.load(Ordering::SeqCst),
        "a rejected retry must not erase the active worker's cancellation"
    );
    drop(permit);
    assert!(!running.load(Ordering::SeqCst));
    assert!(!cancel.load(Ordering::SeqCst));
}

#[test]
fn processing_slot_is_released_during_unwind() {
    let running = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let result = std::panic::catch_unwind(|| {
        let _permit = ProcessingPermit::claim(&running, &cancel).unwrap();
        panic!("worker failed");
    });
    assert!(result.is_err());
    assert!(!running.load(Ordering::SeqCst));
}
