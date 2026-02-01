use anyhow::Result;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_cli_list() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "set1"
source = "/tmp/src1"
target = "/tmp/tgt1"

[[backup_set]]
name = "set2"
sources = ["/tmp/src2", "/tmp/src3"]
target = "/tmp/tgt2"
"#;
    fs::write(&config_file_path, config_content)?;

    // Test tabular output
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("vigil")
        .arg("--")
        .arg("list")
        .env("VIGIL_CONFIG", &config_file_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("set1"));
    assert!(stdout.contains("/tmp/src1"));
    assert!(stdout.contains("/tmp/tgt1"));
    assert!(stdout.contains("set2"));
    assert!(stdout.contains("/tmp/src2 (+1 more)"));
    assert!(stdout.contains("/tmp/tgt2"));

    // Test JSON output
    let output_json = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("vigil")
        .arg("--")
        .arg("list")
        .arg("--json")
        .env("VIGIL_CONFIG", &config_file_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;

    assert!(output_json.status.success());
    let stdout_json = String::from_utf8_lossy(&output_json.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout_json)?;
    assert_eq!(v["backup_set"][0]["name"], "set1");
    assert_eq!(v["backup_set"][1]["name"], "set2");

    Ok(())
}

#[test]
fn test_cli_list_config_error() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_file_path = temp_dir.path().join("invalid_config.toml");

    // Invalid TOML
    fs::write(&config_file_path, "invalid = [")?;

    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("vigil")
        .arg("--")
        .arg("list")
        .env("VIGIL_CONFIG", &config_file_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;

    // Exit code 2 for config error per spec.md
    assert_eq!(output.status.code(), Some(2));

    Ok(())
}
