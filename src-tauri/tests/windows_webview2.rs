#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use clip_studio_lib::windows_webview2::valid_runtime_version;

#[test]
fn webview2_runtime_version_must_be_a_nonzero_four_part_version() {
    assert!(valid_runtime_version("154.0.0.0"));
    assert!(valid_runtime_version("1.2.3.4"));
    for value in ["", "0.0.0.0", "154.0.0", "154.0.0.0-beta", "foo.0.0.0"] {
        assert!(!valid_runtime_version(value), "{value}");
    }
}
