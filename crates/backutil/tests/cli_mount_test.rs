use anyhow::Result;
use std::fs;
use std::process::{Child, Command};
use std::time::Duration;
use tempfile::TempDir;

struct DaemonGuard(Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        // Try graceful shutdown with SIGTERM first
        #[cfg(unix)]
        {
            let pid = self.0.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        // Give it a moment to cleanup
        let mut attempts = 0;
        while attempts < 20 {
            if let Ok(Some(_)) = self.0.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        }

        // Fallback to kill
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct MountGuard {
    set_name: String,
    config_path: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    data_dir: std::path::PathBuf,
    runtime_dir: std::path::PathBuf,
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        let _ = Command::new("cargo")
            .arg("run")
            .arg("--bin")
            .arg("backutil")
            .arg("--")
            .arg("unmount")
            .arg(&self.set_name)
            .env("BACKUTIL_CONFIG", &self.config_path)
            .env("XDG_CONFIG_HOME", &self.config_dir)
            .env("XDG_DATA_HOME", &self.data_dir)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output();
    }
}

#[test]
#[ignore]
fn test_cli_mount_unmount() -> Result<()> {
    // 1. Setup - Create temp directories
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let repo_dir = temp_dir.path().join("repo");
    let source_dir = temp_dir.path().join("source");
    let config_file_path = config_dir.join("backutil/config.toml");
    let password_path = config_dir.join("backutil/.repo_password");

    fs::create_dir_all(config_dir.join("backutil"))?;
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&repo_dir)?;
    fs::create_dir_all(&source_dir)?;

    fs::write(source_dir.join("test.txt"), "hello world")?;

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
        source_dir.display(),
        repo_dir.display()
    );
    fs::write(&config_file_path, config_content)?;
    fs::write(&password_path, "testpassword")?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&password_path, fs::Permissions::from_mode(0o600))?;

    // 3. Initialize repository
    let status = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("backutil")
        .arg("--")
        .arg("init")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()?;
    assert!(status.success(), "Init failed");

    // 4. Start daemon
    let runtime_dir = temp_dir.path().join("runtime");
    fs::create_dir_all(&runtime_dir)?;

    let daemon = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("backutil-daemon")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()?;
    let _daemon_guard = DaemonGuard(daemon);

    // Wait for daemon to be ready
    let mut ready = false;
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_secs(30);

    while start_time.elapsed() < timeout {
        let status = Command::new("cargo")
            .arg("run")
            .arg("--bin")
            .arg("backutil")
            .arg("--")
            .arg("status")
            .env("BACKUTIL_CONFIG", &config_file_path)
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()?;

        if status.status.success() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    assert!(ready, "Daemon did not become ready within 30 seconds");

    // 5. Run a backup to have something to mount
    let status = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("backutil")
        .arg("--")
        .arg("backup")
        .arg("test_set")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()?;
    assert!(status.success(), "Backup failed");

    // 6. Test mount
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("backutil")
        .arg("--")
        .arg("mount")
        .arg("test_set")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;

    assert!(
        output.status.success(),
        "Mount failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _mount_guard = MountGuard {
        set_name: "test_set".to_string(),
        config_path: config_file_path.clone(),
        config_dir: config_dir.clone(),
        data_dir: data_dir.clone(),
        runtime_dir: runtime_dir.clone(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Browse your snapshots at:"),
        "Output should contain mount path hint"
    );

    // Extract mount path
    let mount_path_str = stdout
        .lines()
        .find(|l| l.contains("Browse your snapshots at:"))
        .and_then(|l| l.split(": ").nth(1))
        .map(|p| p.trim_end_matches('/'))
        .expect("Could not find mount path in output");
    let mount_path = std::path::Path::new(mount_path_str);
    assert!(mount_path.exists(), "Mount path does not exist");

    // 7. Test unmount
    let status = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("backutil")
        .arg("--")
        .arg("unmount")
        .arg("test_set")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()?;
    assert!(status.success(), "Unmount failed");

    // 8. Cleanup daemon
    let _ = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("backutil")
        .arg("--")
        .arg("status")
        .env("BACKUTIL_CONFIG", &config_file_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status();

    Ok(())
}
