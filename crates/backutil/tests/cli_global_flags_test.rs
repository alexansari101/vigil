use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_json_list() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test"
source = "/tmp/src"
target = "/tmp/repo"
"#,
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo_bin!("backutil"));
    cmd.env("BACKUTIL_CONFIG", &config_path)
        .arg("--json")
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""name": "test""#))
        .stdout(predicate::str::contains(r#""target": "/tmp/repo""#));
}

#[test]
fn test_cli_quiet_list() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test"
source = "/tmp/src"
target = "/tmp/repo"
"#,
    )
    .unwrap();

    let mut cmd = Command::new(assert_cmd::cargo_bin!("backutil"));
    cmd.env("BACKUTIL_CONFIG", &config_path)
        .arg("--quiet")
        .arg("list");

    cmd.assert().success().stdout(predicate::str::is_empty());
}

#[test]
fn test_cli_json_check_config_only() {
    let temp = tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let password_path = temp.path().join(".repo_password");

    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test"
source = "/tmp/src"
target = "/tmp/repo"
"#,
    )
    .unwrap();
    fs::write(&password_path, "password").unwrap();

    let mut cmd = Command::new(assert_cmd::cargo_bin!("backutil"));
    cmd.env("BACKUTIL_CONFIG", &config_path)
        .arg("--json")
        .arg("check")
        .arg("--config-only");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""status":"ok""#))
        .stdout(predicate::str::contains(r#""config_valid":true"#));
}
