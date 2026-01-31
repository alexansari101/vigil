use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn get_binary_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("failed to get current exe");
    path.pop(); // deps
    if path.file_name().is_some_and(|n| n == "deps") {
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
fn test_logs_basic() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("backutil");
    fs::create_dir_all(&data_dir).unwrap();
    let log_path = data_dir.join("backutil.log");
    fs::write(&log_path, "line 1\nline 2\nline 3\n").unwrap();

    let output = Command::new(get_binary_path())
        .env("XDG_DATA_HOME", temp.path())
        .arg("logs")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("line 1"));
    assert!(stdout.contains("line 2"));
    assert!(stdout.contains("line 3"));
}

#[test]
fn test_logs_no_file() {
    let temp = tempdir().unwrap();

    let output = Command::new(get_binary_path())
        .env("XDG_DATA_HOME", temp.path())
        .arg("logs")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No log files found"));
}

#[test]
fn test_logs_rotation_selection() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("backutil");
    fs::create_dir_all(&data_dir).unwrap();

    // Create an old/rotated log
    let rotated_log = data_dir.join("backutil.log.2026-01-29");
    fs::write(&rotated_log, "old log content\n").unwrap();

    // Create the active log
    let active_log = data_dir.join("backutil.log");
    fs::write(&active_log, "active log content\n").unwrap();

    let output = Command::new(get_binary_path())
        .env("XDG_DATA_HOME", temp.path())
        .arg("logs")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("active log content"));
    assert!(!stdout.contains("old log content"));
}

#[test]
fn test_logs_tail_limit() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("backutil");
    fs::create_dir_all(&data_dir).unwrap();
    let log_path = data_dir.join("backutil.log");

    let mut content = String::new();
    for i in 1..=50 {
        content.push_str(&format!("line {}\n", i));
    }
    fs::write(&log_path, content).unwrap();

    let output = Command::new(get_binary_path())
        .env("XDG_DATA_HOME", temp.path())
        .arg("logs")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // Should show last 20 lines
    assert_eq!(lines.len(), 20);
    assert!(stdout.contains("line 50"));
    assert!(stdout.contains("line 31"));
    assert!(!stdout.contains("line 30"));
}
