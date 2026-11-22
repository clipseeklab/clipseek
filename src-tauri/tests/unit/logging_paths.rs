use super::*;

#[test]
fn redacts_windows_user_paths_with_case_and_separator_variants() {
    let paths = vec![
        r"C:\Users\Alice".to_string(),
        r"C:\Users\Alice\AppData\Local".to_string(),
    ];
    let output = redact_paths(
        r"source=C:\USERS\ALICE\Pictures\a.heic cache=C:/Users/Alice/AppData/Local/ClipSeek",
        &paths,
    );
    assert!(!output.to_ascii_lowercase().contains("alice"));
    assert!(output.contains("<home>\\Pictures"));
    assert!(output.contains("<home>/ClipSeek"));
}

#[test]
fn long_multibyte_records_preserve_the_byte_budget_and_the_next_record() {
    for padding in 0..4 {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = None;
        let mut written = 0;
        let record = Record {
            level: 'E',
            module: "test".into(),
            event: "unicode".into(),
            operation: "-".into(),
            detail: format!("{}{}", "x".repeat(padding), "界😀".repeat(1000)),
        };
        write_record(&record, &mut writer, temp.path(), &mut written);
        write_record(
            &Record {
                detail: "next-record".into(),
                ..record
            },
            &mut writer,
            temp.path(),
            &mut written,
        );
        drop(writer);
        let file = fs::read_dir(temp.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let text = fs::read_to_string(file).unwrap();
        assert!(text.contains("next-record"));
        assert!(text.lines().all(|line| line.len() < LINE_LIMIT));
    }
}
