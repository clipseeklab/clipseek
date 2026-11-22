use super::*;

#[test]
fn automation_refuses_missing_unmarked_and_symlinked_profiles() {
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("CLIPSEEK_E2E_ROOT", value),
                None => std::env::remove_var("CLIPSEEK_E2E_ROOT"),
            }
        }
    }
    let _restore = Restore(std::env::var_os("CLIPSEEK_E2E_ROOT"));
    std::env::remove_var("CLIPSEEK_E2E_ROOT");
    assert!(std::panic::catch_unwind(e2e_root).is_err());
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    std::env::set_var("CLIPSEEK_E2E_ROOT", &root);
    assert!(std::panic::catch_unwind(e2e_root).is_err());
    fs::write(root.join(".clipseek-e2e-sandbox"), "incorrect marker").unwrap();
    assert!(std::panic::catch_unwind(e2e_root).is_err());
    fs::write(root.join(".clipseek-e2e-sandbox"), "clipseek-e2e-v1\n").unwrap();
    assert_eq!(
        GlobalConfig::path().unwrap(),
        root.join("config/config.json")
    );
    assert_eq!(crate::logging::log_dir(), root.join("logs"));
    #[cfg(unix)]
    {
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        std::env::set_var("CLIPSEEK_E2E_ROOT", &alias);
        assert!(std::panic::catch_unwind(e2e_root).is_err());
    }
}
