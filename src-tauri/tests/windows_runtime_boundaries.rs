#![cfg(target_os = "windows")]
//! Pure startup checks: no registry writes, runtime installation or native dialogs.
use clip_studio_lib::{windows_runtime, windows_webview2};
use windows_runtime::{RuntimeStatus, Version, MINIMUM_VERSION};

#[test]
fn vc_runtime_accepts_the_minimum_and_newer_versions_but_rejects_older_builds() {
    assert_eq!(
        windows_runtime::assess_version(None),
        RuntimeStatus::Missing
    );
    for version in [
        Version::new(14, 44, 35210, 65535),
        Version::new(14, 43, 65535, 65535),
    ] {
        assert_eq!(
            windows_runtime::assess_version(Some(version)),
            RuntimeStatus::TooOld(version)
        );
    }
    for version in [
        MINIMUM_VERSION,
        Version::new(14, 44, 35211, 1),
        Version::new(14, 45, 0, 0),
        Version::new(15, 0, 0, 0),
    ] {
        assert_eq!(
            windows_runtime::assess_version(Some(version)),
            RuntimeStatus::Ready
        );
    }
}

#[test]
fn vc_runtime_versions_compare_numerically_and_keep_all_components_in_diagnostics() {
    assert!(Version::new(14, 100, 0, 0) > Version::new(14, 99, 65535, 65535));
    assert!(Version::new(15, 0, 0, 0) > Version::new(14, 65535, 65535, 65535));
    assert_eq!(
        Version::new(14, 44, 35211, 12).to_string(),
        "14.44.35211.12"
    );
}

#[test]
fn webview_runtime_requires_four_numeric_components_and_a_nonzero_version() {
    for version in ["124.0.2478.80", "0.0.0.1", "1.0.0.0"] {
        assert!(
            windows_webview2::valid_runtime_version(version),
            "{version}"
        );
    }
    for version in [
        "",
        "0.0.0.0",
        "124.0.2478",
        "1.2.3.4.5",
        "1..2.3",
        "1.2.3.-4",
        "1.2.3.4294967296",
        "1.2.3.4 ",
        "not installed",
    ] {
        assert!(
            !windows_webview2::valid_runtime_version(version),
            "{version}"
        );
    }
}
