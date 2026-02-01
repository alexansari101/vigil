use anyhow::Result;
use std::fs;

use std::process::Command;
use tempfile::TempDir;

#[test]
#[ignore]
fn test_cli_init() -> Result<()> {
    // 1. Setup - Create temp directories
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let repo_dir = temp_dir.path().join("repo");
    let config_file_path = config_dir.join("vigil/config.toml");
    let password_path = config_dir.join("vigil/.repo_password");

    fs::create_dir_all(config_dir.join("vigil"))?;
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&repo_dir)?;

    // 2. Create config file
    let config_content = format!(
        r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test_set"
source = "{}"
target = "{}"
"#,
        data_dir.display(),
        repo_dir.display()
    );
    fs::write(&config_file_path, config_content)?;

    // 3. Create password file to avoid prompt
    fs::write(&password_path, "testpassword")?;
    // Set permissions to 0600 (required by spec implementation logic, though CLI enforces creation)
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&password_path, fs::Permissions::from_mode(0o600))?;

    // 4. Build the binary (ensure it handles the environment variables)
    // We assume the binary is already built or we can run it via cargo run
    // For integration tests, it's often better to run the binary.
    // However, finding the binary might be tricky.
    // Alternatively, we can assume `cargo test` builds the binary if we use `cargo run` inside?
    // A better approach for unit/integration tests within the crate is to test `main` if possible,
    // but `main` is async and consumes args.
    // Given the project structure, let's run `cargo run --bin vigil` with modified env vars.

    let status = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("vigil")
        .arg("--")
        .arg("init")
        .env("VIGIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR")) // Run from crate root
        .status()?;

    assert!(status.success(), "vigil init failed");

    // 5. Verify repository was initialized
    let config_file = repo_dir.join("config");
    assert!(config_file.exists(), "Restic repo config not found");

    // 6. Verify idempotency
    let status = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("vigil")
        .arg("--")
        .arg("init")
        .env("VIGIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()?;

    // Should succeed even if already initialized
    assert!(status.success(), "vigil init idempotency check failed");

    Ok(())
}
