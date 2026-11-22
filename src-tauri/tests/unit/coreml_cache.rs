use super::*;
#[test]
fn templates_cover_every_precision_and_keep_all_payloads_within_the_file() {
    for text in [VISION_LAYOUT, BODY_LAYOUT] {
        let layout: Layout = serde_json::from_str(text).unwrap();
        let mut ranges = Vec::new();
        for s in layout.segments {
            let size = if ["float16", "uint16"].contains(&s.dtype.as_str()) {
                2
            } else {
                4
            };
            assert!(s.offset + (s.count * size) as u64 <= layout.file_size);
            assert_eq!(s.offset % 64, 0);
            assert_ne!(s.source.transpose, Some(true));
            ranges.push((s.offset, s.offset + (s.count * size) as u64));
            if let Some(recipes) = s.source.recipes {
                for p in ["q8", "q4", "fp32"] {
                    assert!(recipes.contains_key(p));
                }
            }
        }
        ranges.sort_unstable();
        assert!(ranges.windows(2).all(|x| x[0].1 <= x[1].0));
    }
    assert_eq!(BODIES.len(), BUCKETS.len());
    assert_eq!(BUCKETS.last(), Some(&8192));
}
#[test]
fn cache_integrity_detects_changed_added_and_linked_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one"), [1, 2, 3]).unwrap();
    fs::hard_link(root.path().join("one"), root.path().join("two")).unwrap();
    let before = digest(root.path()).unwrap();
    assert_eq!(before["one"], before["two"]);
    fs::write(root.path().join("integrity.json"), b"metadata").unwrap();
    assert_eq!(before, digest(root.path()).unwrap());
    fs::write(root.path().join("one"), [1, 2, 4]).unwrap();
    assert_ne!(before, digest(root.path()).unwrap());
    fs::write(root.path().join("extra"), [1]).unwrap();
    assert_eq!(digest(root.path()).unwrap().len(), 3);
}
