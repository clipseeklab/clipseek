use super::*;
use std::{sync::mpsc, time::Duration};

#[test]
fn initial_data_waits_until_session_worker_completes() {
    let readiness = Arc::new(Readiness::default());
    let completion = Completion(readiness.clone());
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        readiness.wait();
        finished_tx.send(()).unwrap();
    });
    started_rx.recv().unwrap();
    assert!(finished_rx.try_recv().is_err());
    drop(completion);
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    waiter.join().unwrap();
}

#[test]
fn failed_initialization_also_releases_readiness_waiters() {
    let readiness = Arc::new(Readiness::default());
    let result = std::panic::catch_unwind({
        let readiness = readiness.clone();
        move || {
            let _completion = Completion(readiness);
            panic!("opening library failed");
        }
    });
    assert!(result.is_err());
    assert!(*readiness.finished.lock().unwrap());
    readiness.wait();
}
