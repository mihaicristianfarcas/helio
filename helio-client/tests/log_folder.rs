//! The app's real log start-up, in a test process of its own: `logging::init`
//! works once per process and reads `HELIO_DATA_DIR`, so it cannot share one
//! with other tests.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

#[test]
fn logs_go_in_a_folder_only_the_user_can_enter_even_one_that_was_open() {
    let data = tempfile::tempdir().unwrap();
    let logs = data.path().join("logs");
    let earlier = logs.join("helio-2026-11-01-144906.log");
    std::fs::create_dir(&logs).unwrap();
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(&earlier, "an earlier launch").unwrap();

    std::env::set_var("HELIO_DATA_DIR", data.path());
    helio_client::logging::init();

    let mode = std::fs::metadata(&logs).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    assert!(logs.join("helio.log").exists());
    assert!(earlier.exists());
}
