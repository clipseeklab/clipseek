#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use clip_studio_lib::windows_runtime::{assess_version, RuntimeStatus, Version, MINIMUM_VERSION};

#[test]
fn rejects_missing_and_older_x64_runtime() {
    assert_eq!(assess_version(None), RuntimeStatus::Missing);
    assert_eq!(
        assess_version(Some(Version::new(14, 34, 31938, 0))),
        RuntimeStatus::TooOld(Version::new(14, 34, 31938, 0))
    );
    assert_eq!(
        assess_version(Some(Version::new(14, 44, 35210, 0))),
        RuntimeStatus::TooOld(Version::new(14, 44, 35210, 0))
    );
}

#[test]
fn accepts_minimum_and_newer_x64_runtime() {
    assert_eq!(assess_version(Some(MINIMUM_VERSION)), RuntimeStatus::Ready);
    assert_eq!(
        assess_version(Some(Version::new(14, 51, 36247, 0))),
        RuntimeStatus::Ready
    );
}
