use anyhow::Result;
use serial_test::serial;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn backutil() -> String {
    env!("CARGO_BIN_EXE_backutil").to_string()
}

#[test]
#[serial]
fn test_track_creates_config() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file = temp_dir.path().join("config.toml");

    // No config file exists yet — track should create one
    let output = Command::new(backutil())
        .args(["track", "myset", "/tmp/src", "/tmp/tgt"])
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    // track calls init which will fail without restic, but config should be written
    // before that step. Check that the config file was created with the set.
    let content = fs::read_to_string(&config_file)?;
    assert!(content.contains("myset"), "config should contain set name");
    assert!(
        content.contains("/tmp/src"),
        "config should contain source path"
    );
    assert!(
        content.contains("/tmp/tgt"),
        "config should contain target path"
    );

    // Verify it's valid TOML with the expected structure
    let config: toml::Value = toml::from_str(&content)?;
    let sets = config["backup_set"].as_array().unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0]["name"].as_str().unwrap(), "myset");

    // Suppress unused warning — we check the config file, not exit status,
    // because init/reload may fail in test environment without restic/daemon.
    let _ = output;

    Ok(())
}

#[test]
#[serial]
fn test_track_duplicate_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file = temp_dir.path().join("config.toml");

    let config_content = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "existing"
source = "/tmp/src"
target = "/tmp/tgt"
"#;
    fs::write(&config_file, config_content)?;

    let output = Command::new(backutil())
        .args(["track", "existing", "/tmp/src2", "/tmp/tgt2"])
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    assert!(!output.status.success(), "should fail for duplicate name");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists"),
        "error should mention duplicate: {}",
        stderr
    );

    Ok(())
}

#[test]
#[serial]
fn test_track_invalid_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file = temp_dir.path().join("config.toml");

    let output = Command::new(backutil())
        .args(["track", "../../etc", "/tmp/src", "/tmp/tgt"])
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    assert!(!output.status.success(), "should reject invalid name");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid set name"),
        "error should mention invalid name: {}",
        stderr
    );

    Ok(())
}

#[test]
#[serial]
fn test_untrack_removes_set() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file = temp_dir.path().join("config.toml");

    let config_content = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "removeme"
source = "/tmp/src"
target = "/tmp/tgt"

[[backup_set]]
name = "keepme"
source = "/tmp/src2"
target = "/tmp/tgt2"
"#;
    fs::write(&config_file, config_content)?;

    let output = Command::new(backutil())
        .args(["untrack", "removeme"])
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    // Reload may fail without daemon, but config should be updated
    let content = fs::read_to_string(&config_file)?;
    assert!(
        !content.contains("removeme"),
        "removed set should not be in config"
    );
    assert!(
        content.contains("keepme"),
        "other set should remain in config"
    );

    let _ = output;

    Ok(())
}

#[test]
#[serial]
fn test_untrack_unknown_set() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file = temp_dir.path().join("config.toml");

    let config_content = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "existing"
source = "/tmp/src"
target = "/tmp/tgt"
"#;
    fs::write(&config_file, config_content)?;

    let output = Command::new(backutil())
        .args(["untrack", "nonexistent"])
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    assert!(!output.status.success(), "should fail for unknown set");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found"),
        "error should mention not found: {}",
        stderr
    );

    Ok(())
}
