use super::*;
use std::io::Write;

#[test]
fn sips_exists_on_macos() {
    // Report unavailable optional macOS resources without claiming their behavior was tested.
    if !available() {
        eprintln!("sips unavailable; HEIC-specific tests cannot run");
    }
}

#[test]
fn rejects_invalid_heic() {
    if !available() {
        return;
    }
    let mut src = tempfile::Builder::new().suffix(".heic").tempfile().unwrap();
    src.write_all(b"not a heic file").unwrap();
    let dest = tempfile::Builder::new().suffix(".jpg").tempfile().unwrap();
    assert!(heic_to_jpeg(src.path(), dest.path()).is_err());
}
