use super::*;

#[test]
fn visual_and_subtitle_state_combinations_exclude_the_expected_generations() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE assets(id INTEGER PRIMARY KEY, kind TEXT, visual_indexed INTEGER, subtitle_indexed INTEGER);
         CREATE TABLE frames(id INTEGER PRIMARY KEY, asset_id INTEGER);
         INSERT INTO assets VALUES(1,'image',1,0),(2,'video',1,0),(3,'video',1,1),(4,'video',0,1);
         INSERT INTO frames VALUES(10,1),(20,2),(30,3),(40,4);",
    ).unwrap();
    for (visual, subtitles, expected) in [
        (None, None, vec![40]),
        (Some(true), None, vec![40]),
        (Some(false), None, vec![10, 20, 30, 40]),
        (None, Some(false), vec![10, 30, 40]),
        (None, Some(true), vec![10, 20, 40]),
        (Some(true), Some(false), vec![10, 30, 40]),
        (Some(true), Some(true), vec![10, 20, 40]),
        (Some(false), Some(false), vec![10, 20, 30, 40]),
        (Some(false), Some(true), vec![10, 20, 30, 40]),
    ] {
        let mut ids = excluded_visual_frame_ids(&conn, visual, subtitles).unwrap();
        ids.sort_unstable();
        assert_eq!(ids, expected, "visual={visual:?}, subtitles={subtitles:?}");
    }
}
