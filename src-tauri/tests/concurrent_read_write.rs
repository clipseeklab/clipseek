//! Storage concurrency regressions keep asset reads and vector search available during
//! imports.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::json;

use clip_studio_lib::{library, vector};

fn test_library(name: &str) -> (std::path::PathBuf, Arc<library::LibrarySession>) {
    let root = std::env::temp_dir().join(format!(
        "clip-studio-{name}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let meta = library::init_library(&root, name).expect("Create the test library");
    let session = Arc::new(library::open_session(meta).expect("Open the test library"));
    (root, session)
}

#[test]
fn sqlite_wal_reader_keeps_working_during_an_uncommitted_write() {
    let (root, session) = test_library("sqlite-concurrency");
    let db_path = root.join(".clipstudio-open/library.db");
    {
        let conn = session.db.lock().unwrap();
        conn.execute("CREATE TABLE concurrency_probe(value INTEGER NOT NULL)", [])
            .unwrap();
    }
    drop(session);

    let (started_tx, started_rx) = mpsc::channel();
    let (commit_tx, wait_for_commit) = mpsc::channel();
    let writer_path = db_path.clone();
    let writer = std::thread::spawn(move || {
        let mut conn = Connection::open(writer_path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute("INSERT INTO concurrency_probe(value) VALUES (1)", [])
            .unwrap();
        started_tx.send(()).unwrap();
        wait_for_commit.recv().unwrap();
        tx.commit().unwrap();
    });

    started_rx.recv().unwrap();
    let reader = Connection::open(&db_path).unwrap();
    reader.pragma_update(None, "journal_mode", "WAL").unwrap();
    let started = Instant::now();
    let count_during_write: i64 = reader
        .query_row("SELECT COUNT(*) FROM concurrency_probe", [], |row| {
            row.get(0)
        })
        .unwrap();
    let read_elapsed = started.elapsed();

    // WAL readers must immediately see the committed snapshot without waiting for the writer.
    assert_eq!(count_during_write, 0);
    assert!(
        read_elapsed < Duration::from_millis(100),
        "WAL read blocked behind an uncommitted writer for {read_elapsed:?}"
    );
    eprintln!("SQLite WAL read during an uncommitted write: {read_elapsed:?}");
    commit_tx.send(()).unwrap();
    writer.join().unwrap();
    let count_after_commit: i64 = reader
        .query_row("SELECT COUNT(*) FROM concurrency_probe", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count_after_commit, 1);

    drop(reader);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn app_sqlite_connection_mutex_serializes_access() {
    let (root, session) = test_library("sqlite-app-lock");
    let (locked_tx, locked_rx) = mpsc::channel();
    let holder_session = session.clone();
    let holder = std::thread::spawn(move || {
        let _conn = holder_session.db.lock().unwrap();
        locked_tx.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(150));
    });

    locked_rx.recv().unwrap();
    let started = Instant::now();
    let conn = session.db.lock().unwrap();
    let lock_wait = started.elapsed();
    let _: i64 = conn.query_row("SELECT 1", [], |row| row.get(0)).unwrap();
    drop(conn);
    holder.join().unwrap();

    assert!(
        lock_wait >= Duration::from_millis(100),
        "SQLite access did not use the expected shared mutex: {lock_wait:?}"
    );
    eprintln!("Application connection mutex wait: {lock_wait:?}");

    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn qdrant_search_succeeds_while_batches_are_being_upserted() {
    let (root, session) = test_library("qdrant-concurrency");
    let query = vec![1.0_f32; 768];
    let writing = Arc::new(AtomicBool::new(true));

    let writer_session = session.clone();
    let writer_flag = writing.clone();
    let writer = std::thread::spawn(move || {
        for batch in 0..6_i64 {
            let points = (0..8_i64)
                .map(|offset| {
                    let id = batch * 8 + offset + 1;
                    (
                        id,
                        vec![1.0_f32; 768],
                        json!({
                            "asset_id": id,
                            "seq": 0,
                            "deleted": false,
                            "kind": "image"
                        }),
                    )
                })
                .collect();
            vector::upsert_points(&writer_session, points).expect("Concurrent vector write");
            std::thread::sleep(Duration::from_millis(10));
        }
        writer_flag.store(false, Ordering::Release);
    });

    let mut searches_while_writing = 0_usize;
    let mut observed_partial_index = false;
    let mut max_search_elapsed = Duration::ZERO;
    while writing.load(Ordering::Acquire) {
        let started = Instant::now();
        let hits =
            vector::search(&session, query.clone(), None, 100).expect("Search during writes");
        max_search_elapsed = max_search_elapsed.max(started.elapsed());
        searches_while_writing += 1;
        observed_partial_index |= !hits.is_empty() && hits.len() < 48;
        std::thread::sleep(Duration::from_millis(1));
    }
    writer.join().unwrap();

    let final_hits = vector::search(&session, query, None, 100).expect("Search after writes");
    assert_eq!(final_hits.len(), 48);
    assert!(
        searches_while_writing > 0,
        "No searches completed during writing"
    );
    assert!(
        observed_partial_index,
        "No intermediate incremental-index state was observed"
    );
    eprintln!(
        "Completed {searches_while_writing} searches during Qdrant batch writes; maximum duration {max_search_elapsed:?}"
    );

    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}
