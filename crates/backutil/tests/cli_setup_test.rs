use anyhow::Result;
use serial_test::serial;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

#[test]
#[serial]
fn test_cli_setup_idempotent() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");

    let backutil_config_dir = config_dir.join("backutil");
    fs::create_dir_all(&backutil_config_dir)?;
    fs::create_dir_all(&data_dir)?;

    let config_file = backutil_config_dir.join("config.toml");
    let pass_path = backutil_config_dir.join(".repo_password");
    fs::write(&pass_path, "existingpass")?;

    let config_content = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "existing"
source = "/tmp/source"
target = "/tmp/target"
"#;
    fs::write(&config_file, config_content)?;

    let backutil_bin = env!("CARGO_BIN_EXE_backutil");

    let output = Command::new(&backutil_bin)
        .arg("setup")
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("BACKUTIL_CONFIG", &config_file)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Password file found"));
    assert!(stdout.contains("Configuration found"));
    assert!(stdout.contains("existing"));

    Ok(())
}

#[test]
#[serial]
#[ignore] // Still issues with rpassword hidden input in tests
fn test_cli_setup_fresh() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");

    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(&data_dir)?;

    let backutil_bin = env!("CARGO_BIN_EXE_backutil");

    let mut child = Command::new(&backutil_bin)
        .arg("setup")
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.as_mut().unwrap();

    // Step 1: Password
    writeln!(stdin, "testpass")?;
    writeln!(stdin, "testpass")?;

    // Step 2: Config
    writeln!(stdin, "testset")?;
    writeln!(stdin, "/tmp/src")?;
    writeln!(stdin, "/tmp/tgt")?;

    // Init offer?
    writeln!(stdin, "n")?;

    // Step 3: Service?
    writeln!(stdin, "n")?;

    let output = child.wait_with_output()?;
    assert!(output.status.success());

    let config_file = config_dir.join("backutil/config.toml");
    let password_file = config_dir.join("backutil/.repo_password");

    assert!(config_file.exists());
    assert!(password_file.exists());

    let config_content = fs::read_to_string(config_file)?;
    assert!(config_content.contains("testset"));

    Ok(())
}

#[test]
#[serial]
fn test_cli_setup_partial() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");

    let backutil_config_dir = config_dir.join("backutil");
    fs::create_dir_all(&backutil_config_dir)?;
    fs::create_dir_all(&data_dir)?;

    let config_file = backutil_config_dir.join("config.toml");
    let pass_path = backutil_config_dir.join(".repo_password");
    fs::write(&pass_path, "existingpass")?;

    let backutil_bin = env!("CARGO_BIN_EXE_backutil");

    let mut child = Command::new(&backutil_bin)
        .arg("setup")
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("BACKUTIL_CONFIG", &config_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.as_mut().unwrap();

    // Should skip password and go straight to config
    writeln!(stdin, "partialset")?;
    writeln!(stdin, "/tmp/src")?;
    writeln!(stdin, "/tmp/tgt")?;

    // Init offer?
    writeln!(stdin, "n")?;

    // Step 3: Service?
    writeln!(stdin, "n")?;

    let output = child.wait_with_output()?;
    assert!(output.status.success());

    assert!(config_file.exists());
    let config_content = fs::read_to_string(config_file)?;
    assert!(config_content.contains("partialset"));

    Ok(())
}
