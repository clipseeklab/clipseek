use super::*;
#[test]
fn deployment_gate_preserves_the_mac14_fallback() {
    for version in ["15.0", "27.0.1", "100.0"] {
        assert!(supported_version(version));
    }
    for version in ["", "invalid", "14.7.1", "10.16"] {
        assert!(!supported_version(version));
    }
}
