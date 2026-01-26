use anyhow::Result;
use backutil_daemon::executor::ResticExecutor;
use backutil_daemon::manager::JobManager;
use backutil_daemon::watcher::{FileWatcher, WatcherEvent};
use backutil_lib::config::{BackupSet, Config, GlobalConfig};
use backutil_lib::paths;
use backutil_lib::types::JobState;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;

/// End-to-end integration test for file watcher + debounce logic.
/// This test validates the complete pipeline: file change → watcher → JobManager → state transitions.
/// Eliminates the need for manual verification of real-time file detection.
///
/// **NOTE:** This test modifies XDG environment variables and must be run single-threaded:
/// ```bash
/// cargo test -p backutil-daemon --test integration_test -- --ignored --test-threads=1
/// ```
#[tokio::test]
#[ignore]
async fn test_file_watcher_to_debounce_integration() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    // Setup: Create temporary directories
    let tmp = tempdir()?;
    let source_path = tmp.path().join("source");
    let repo_path = tmp.path().join("repo");
    fs::create_dir(&source_path)?;

    // Setup: Isolated config/data dirs via env vars to avoid polluting user config
    let config_home = tmp.path().join("config");
    let data_home = tmp.path().join("data");
    fs::create_dir_all(&config_home)?;
    fs::create_dir_all(&data_home)?;
    std::env::set_var("XDG_CONFIG_HOME", &config_home);
    std::env::set_var("XDG_DATA_HOME", &data_home);

    // Setup: Create password file
    let pw_file = paths::password_path();
    fs::create_dir_all(pw_file.parent().unwrap())?;
    fs::write(&pw_file, "testpassword")?;
    fs::set_permissions(&pw_file, fs::Permissions::from_mode(0o600))?;

    // Setup: Initialize restic repository
    let executor = ResticExecutor::new();
    executor.init(repo_path.to_str().unwrap()).await?;

    let config = Config {
        global: GlobalConfig::default(),
        backup_sets: vec![BackupSet {
            name: "test".to_string(),
            source: Some(source_path.to_string_lossy().to_string()),
            sources: None,
            target: repo_path.to_string_lossy().to_string(),
            exclude: Some(vec!["*.tmp".to_string()]),
            debounce_seconds: Some(1), // 1 second for faster test
            retention: None,
        }],
    };

    // Create JobManager and FileWatcher (mimicking daemon setup)
    let job_manager = JobManager::new(&config);
    let (watcher_tx, mut watcher_rx) = mpsc::channel(100);
    let _watcher = FileWatcher::new(&config, watcher_tx)?;

    // Helper to get job state
    let get_state = || async {
        job_manager
            .get_status()
            .await
            .into_iter()
            .find(|s| s.name == "test")
            .map(|s| s.state)
            .unwrap()
    };

    // Initial state should be Idle
    assert_eq!(get_state().await, JobState::Idle);

    // Test 1: File creation triggers debounce
    let test_file = source_path.join("test.txt");
    fs::write(&test_file, "hello world")?;

    // Wait for watcher event
    let event = tokio::time::timeout(Duration::from_secs(2), watcher_rx.recv())
        .await
        .expect("Timeout waiting for file change event")
        .expect("No event received");

    let WatcherEvent::FileChanged { set_name, path } = event;
    assert_eq!(set_name, "test");
    assert!(path.ends_with("test.txt"));

    // Trigger debounce
    job_manager.handle_file_change(&set_name).await?;

    // Should enter Debouncing state
    tokio::time::sleep(Duration::from_millis(100)).await;
    let state = get_state().await;
    assert!(
        matches!(state, JobState::Debouncing { .. }),
        "Expected Debouncing, got {:?}",
        state
    );

    // Wait for debounce to complete and backup to finish
    // (1s debounce + real backup which is fast for small files)
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let state = get_state().await;
    assert_eq!(
        state,
        JobState::Idle,
        "Expected Idle after backup completes"
    );

    // Drain any remaining events from the first test
    while tokio::time::timeout(Duration::from_millis(50), watcher_rx.recv())
        .await
        .is_ok()
    {}

    // Test 2: Excluded files don't trigger events
    let excluded_file = source_path.join("ignore.tmp");
    fs::write(&excluded_file, "should be ignored")?;

    // Should NOT receive an event (wait a bit to be sure)
    let event = tokio::time::timeout(Duration::from_millis(500), watcher_rx.recv()).await;
    assert!(event.is_err(), "Should not receive event for excluded file");

    // State should remain Idle
    assert_eq!(get_state().await, JobState::Idle);

    // Test 3: Verify the basic integration works end-to-end
    // (Timer reset behavior is already well-tested in unit tests)
    let file3 = source_path.join("final.txt");
    fs::write(&file3, "final test")?;

    let event3 = tokio::time::timeout(Duration::from_millis(500), watcher_rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("No event");

    let WatcherEvent::FileChanged { set_name, .. } = event3;
    job_manager.handle_file_change(&set_name).await?;

    // Should enter Debouncing
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        matches!(get_state().await, JobState::Debouncing { .. }),
        "Expected Debouncing"
    );

    // Wait for full cycle: debounce (1s) + backup + margin
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(
        get_state().await,
        JobState::Idle,
        "Expected Idle after backup"
    );

    // Cleanup is automatic via tempdir drop
    Ok(())
}
