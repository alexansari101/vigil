use anyhow::Result;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

struct MultiTestEnv {
    #[allow(dead_code)]
    temp_dir: TempDir,
    daemon: Child,
}

impl MultiTestEnv {
    async fn setup(set_count: usize) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let runtime_dir = temp_dir.path().join("runtime");

        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&runtime_dir)?;

        let config_path = config_dir.join("backutil/config.toml");
        fs::create_dir_all(config_path.parent().unwrap())?;

        let mut config_content = String::from("[global]\ndebounce_seconds = 60\n");

        let pw_file = config_dir.join("backutil/.repo_password");
        fs::create_dir_all(pw_file.parent().unwrap())?;
        fs::write(&pw_file, "testpassword")?;

        for i in 1..=set_count {
            let source_dir = temp_dir.path().join(format!("source{}", i));
            let target_dir = temp_dir.path().join(format!("target{}", i));
            fs::create_dir_all(&source_dir)?;
            fs::create_dir_all(&target_dir)?;

            config_content.push_str(&format!(
                r#"
[[backup_set]]
name = "set{}"
source = "{}"
target = "{}"
"#,
                i,
                source_dir.display(),
                target_dir.display()
            ));

            // Initialize restic repo
            let status = Command::new("restic")
                .args([
                    "init",
                    "--repo",
                    target_dir.to_str().unwrap(),
                    "--password-file",
                    pw_file.to_str().unwrap(),
                ])
                .status()?;
            assert!(status.success());

            // Create a test file
            fs::write(source_dir.join("test.txt"), format!("hello world {}", i))?;
        }

        fs::write(&config_path, config_content)?;

        // Start daemon
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let daemon_path = workspace_root.join("target/debug/backutil-daemon");

        let daemon_path = if daemon_path.exists() {
            daemon_path
        } else {
            std::path::PathBuf::from("backutil-daemon")
        };

        let daemon = Command::new(daemon_path)
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("BACKUTIL_CONFIG", &config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let socket_path = runtime_dir.join("backutil.sock");

        // Wait for socket
        let mut attempts = 0;
        while !socket_path.exists() && attempts < 50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }
        assert!(
            socket_path.exists(),
            "Daemon failed to start or create socket"
        );

        Ok(Self { temp_dir, daemon })
    }

    fn run_cli(&self, args: &[&str]) -> Result<(bool, String, String)> {
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let cli_path = workspace_root.join("target/debug/backutil");

        let cli_path = if cli_path.exists() {
            cli_path
        } else {
            std::path::PathBuf::from("backutil")
        };

        let output = Command::new(cli_path)
            .args(args)
            .env("XDG_CONFIG_HOME", self.temp_dir.path().join("config"))
            .env("XDG_DATA_HOME", self.temp_dir.path().join("data"))
            .env("XDG_RUNTIME_DIR", self.temp_dir.path().join("runtime"))
            .env(
                "BACKUTIL_CONFIG",
                self.temp_dir.path().join("config/backutil/config.toml"),
            )
            .output()?;

        Ok((
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

impl Drop for MultiTestEnv {
    fn drop(&mut self) {
        // Try graceful shutdown with SIGTERM first
        #[cfg(unix)]
        {
            let pid = self.daemon.id();
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        // Give it a moment to cleanup
        let mut attempts = 0;
        while attempts < 20 {
            if let Ok(Some(_)) = self.daemon.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        }

        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[tokio::test]
#[ignore] // Requires restic
async fn test_cli_backup_all_multi_set() -> Result<()> {
    // 3 sets
    let env = MultiTestEnv::setup(3).await?;

    let (success, stdout, stderr) = env.run_cli(&["backup"])?;

    assert!(success, "CLI failed: {}", stderr);
    assert!(stdout.contains("Backup triggered for set 'set1'"));
    assert!(stdout.contains("Backup triggered for set 'set2'"));
    assert!(stdout.contains("Backup triggered for set 'set3'"));
    assert!(stdout.contains("Backup complete for set 'set1'"));
    assert!(stdout.contains("Backup complete for set 'set2'"));
    assert!(stdout.contains("Backup complete for set 'set3'"));

    Ok(())
}

#[tokio::test]
#[ignore] // Requires restic
async fn test_cli_backup_no_wait() -> Result<()> {
    let env = MultiTestEnv::setup(1).await?;

    let (success, stdout, stderr) = env.run_cli(&["backup", "--no-wait"])?;

    assert!(success, "CLI failed: {}", stderr);
    assert!(stdout.contains("Backup triggered for set 'set1'"));
    // Should NOT contain complete message because we didn't wait
    assert!(!stdout.contains("Backup complete for set 'set1'"));

    Ok(())
}

#[tokio::test]
#[ignore] // Requires restic
async fn test_cli_backup_timeout() -> Result<()> {
    let env = MultiTestEnv::setup(1).await?;

    // Use a very short timeout that is likely to expire (0 seconds)
    // Actually 1 second might be enough for it to fail if it takes longer.
    let (success, _stdout, stderr) = env.run_cli(&["backup", "--timeout", "0"])?;

    assert!(!success, "CLI should have timed out");
    assert!(stderr.contains("Timeout waiting for backup completion"));

    Ok(())
}
