use super::*;

#[test]
fn manual_rebuild_accepts_healthy_and_pending_indexes_but_not_an_active_job() {
    assert!(rebuild_start_allowed(&VectorHealth::Ready));
    assert!(rebuild_start_allowed(&VectorHealth::RebuildPending {
        code: "open_failed".into(),
        message: "corrupt".into(),
    }));
    assert!(!rebuild_start_allowed(&VectorHealth::Rebuilding {
        rebuild_id: "active".into(),
    }));
}
