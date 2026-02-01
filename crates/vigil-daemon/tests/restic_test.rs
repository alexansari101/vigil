use anyhow::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;
use vigil_daemon::executor::ResticExecutor;
use vigil_lib::config::{BackupSet, RetentionPolicy};
use vigil_lib::paths;

/// Robust integration test for ResticExecutor.
/// Requires restic installed and fusermount3 for mount test.
#[tokio::test]
#[ignore]
async fn test_restic_workflow_integration() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    // Setup: Create temporary directories
    let tmp = tempdir()?;
    let repo_path = tmp.path().join("repo");
    let source_path = tmp.path().join("source");
    fs::create_dir(&source_path)?;
    fs::write(source_path.join("file1.txt"), "hello world")?;

    // Setup: Isolated config/data dirs via env vars
    let config_home = tmp.path().join("config");
    let data_home = tmp.path().join("data");
    fs::create_dir_all(&config_home)?;
    fs::create_dir_all(&data_home)?;

    // These env vars will influence vigil_lib::paths
    std::env::set_var("XDG_CONFIG_HOME", &config_home);
    std::env::set_var("XDG_DATA_HOME", &data_home);

    // Now paths::password_path() should point into our tmp dir
    let pw_file = paths::password_path();
    fs::create_dir_all(pw_file.parent().unwrap())?;
    fs::write(&pw_file, "testpassword")?;
    fs::set_permissions(&pw_file, fs::Permissions::from_mode(0o600))?;

    let executor = ResticExecutor::new();

    // 1. Init
    executor.init(repo_path.to_str().unwrap()).await?;
    assert!(repo_path.exists());
    assert!(repo_path.join("config").exists());

    // 2. Backup
    let set = BackupSet {
        name: "test".to_string(),
        source: Some(source_path.to_string_lossy().to_string()),
        sources: None,
        target: repo_path.to_string_lossy().to_string(),
        exclude: None,
        debounce_seconds: None,
        retention: None,
    };

    let result = executor.backup(&set, None).await?;
    assert!(result.success, "Backup failed: {:?}", result.error_message);
    assert!(!result.snapshot_id.is_empty());
    assert!(result.added_bytes > 0);

    // 3. Snapshots
    let snapshots = executor
        .snapshots(repo_path.to_str().unwrap(), None, None)
        .await?;
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].short_id, result.snapshot_id);
    assert!(snapshots[0]
        .paths
        .iter()
        .any(|p| p.to_string_lossy().contains("source")));

    // 4. Prune
    let mut set_with_retention = set.clone();
    set_with_retention.retention = Some(RetentionPolicy {
        keep_last: Some(1),
        ..Default::default()
    });
    let reclaimed = executor.prune(&set_with_retention, None).await?;
    // Note: reclaimed is u64, always >= 0. Just verify prune succeeded.
    let _ = reclaimed;

    // Snapshots should still be 1
    let snapshots = executor
        .snapshots(repo_path.to_str().unwrap(), None, None)
        .await?;
    assert_eq!(snapshots.len(), 1);

    // 5. Password Validation: Trigger error with wrong password
    fs::write(&pw_file, "wrongpassword")?;
    let bad_result = executor.backup(&set, None).await?;
    assert!(!bad_result.success);
    assert!(
        bad_result
            .error_message
            .as_ref()
            .unwrap()
            .contains("password")
            || bad_result
                .error_message
                .as_ref()
                .unwrap()
                .contains("Restic error")
    );

    // 6. Mount (Pragmatic start/stop)
    fs::write(&pw_file, "testpassword")?; // Restore correct password
    let mount_point = tmp.path().join("mnt");
    fs::create_dir(&mount_point)?;
    let mut child = executor
        .mount(repo_path.to_str().unwrap(), None, &mount_point)
        .await?;

    // Give it a moment to attempt mount
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Check if process is still running
    let status = child.try_wait()?;
    assert!(status.is_none(), "Restic mount process exited prematurely");

    child.kill().await?;

    Ok(())
}
