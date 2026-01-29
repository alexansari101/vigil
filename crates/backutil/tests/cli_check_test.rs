use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn get_binary_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("failed to get current exe");
    path.pop(); // deps
    if path.file_name().map_or(false, |n| n == "deps") {
        path.pop(); // debug/release
    }
    path.push("backutil");

    if path.exists() {
        path
    } else {
        std::path::PathBuf::from("backutil")
    }
}

#[test]
fn test_check_config_valid() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test"
source = "~/test"
target = "/tmp/backup"
"#
    )
    .unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("backutil");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join(".repo_password"), "password").unwrap();

    let output = Command::new(get_binary_path())
        .env("BACKUTIL_CONFIG", file.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .env("HOME", temp_dir.path())
        .arg("check")
        .arg("--config-only")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("✓ Configuration valid"),
        "Output missing config validation"
    );
    assert!(
        stdout.contains("✓ Password file exists"),
        "Output missing password check"
    );
}

#[test]
fn test_check_config_invalid() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[global]
debounce_seconds = "invalid" # Type mismatch
"#
    )
    .unwrap();

    let output = Command::new(get_binary_path())
        .env("BACKUTIL_CONFIG", file.path())
        .arg("check")
        .arg("--config-only")
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr); // Check stderr for error
    if stderr.is_empty() {
        // Fallback to stdout check if eprintln goes there or captured mixed
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Configuration invalid"));
    } else {
        assert!(stderr.contains("Configuration invalid"));
    }
}

#[test]
fn test_check_repo_failure() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "nonexistent"
source = "/tmp/source"
target = "/tmp/nonexistent_repo"
"#
    )
    .unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("backutil");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join(".repo_password"), "password").unwrap();

    let output = Command::new(get_binary_path())
        .env("BACKUTIL_CONFIG", file.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .env("HOME", temp_dir.path())
        .arg("check")
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(4));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✗ nonexistent: Repository check failed"));
}
