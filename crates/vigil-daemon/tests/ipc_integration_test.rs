use anyhow::Result;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use vigil_lib::ipc::{Request, Response, ResponseData};

struct TestDaemon {
    child: Child,
    #[allow(dead_code)]
    temp_dir: TempDir,
    socket_path: std::path::PathBuf,
    pid_path: std::path::PathBuf,
}

impl TestDaemon {
    fn spawn() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let runtime_dir = temp_dir.path().join("runtime");

        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&runtime_dir)?;

        let source_dir = temp_dir.path().join("source");
        let target_dir = temp_dir.path().join("target");
        fs::create_dir_all(&source_dir)?;
        fs::create_dir_all(&target_dir)?;

        let config_path = config_dir.join("vigil/config.toml");
        fs::create_dir_all(config_path.parent().unwrap())?;
        fs::write(
            &config_path,
            format!(
                r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test-set"
source = "{}"
target = "{}"
"#,
                source_dir.display(),
                target_dir.display()
            ),
        )?;

        let daemon_path = env!("CARGO_BIN_EXE_vigil-daemon");

        let mut child = Command::new(daemon_path)
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("VIGIL_CONFIG", &config_path) // Fixed: explicitly set config path to prevent leaking host ENV
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let socket_path = runtime_dir.join("vigil.sock");
        let pid_path = runtime_dir.join("vigil.pid");

        // Wait for socket to appear
        let mut attempts = 0;
        while !socket_path.exists() && attempts < 50 {
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        }

        if !socket_path.exists() {
            // Check if process is still running
            if let Ok(Some(status)) = child.try_wait() {
                let mut stderr = String::new();
                if let Some(mut reader) = child.stderr.take() {
                    use std::io::Read;
                    let _ = reader.read_to_string(&mut stderr);
                }
                panic!(
                    "Daemon exited prematurely with status: {}\nStderr: {}",
                    status, stderr
                );
            }
            panic!("Daemon failed to start or create socket within timeout");
        }

        Ok(Self {
            child,
            temp_dir,
            socket_path,
            pid_path,
        })
    }

    async fn send_request(&self, request: Request) -> Result<Response> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;
        let json = serde_json::to_string(&request)? + "\n";
        stream.write_all(json.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let resp: Response = serde_json::from_str(&line)?;
        Ok(resp)
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn test_ipc_ping() -> Result<()> {
    let daemon = TestDaemon::spawn()?;
    let resp = daemon.send_request(Request::Ping).await?;
    assert!(matches!(resp, Response::Pong));
    Ok(())
}

#[tokio::test]
async fn test_ipc_status() -> Result<()> {
    let daemon = TestDaemon::spawn()?;
    let resp = daemon.send_request(Request::Status).await?;
    if let Response::Ok(Some(ResponseData::Status { sets })) = resp {
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].name, "test-set");
    } else {
        panic!("Unexpected response: {:?}", resp);
    }
    Ok(())
}

#[tokio::test]
async fn test_ipc_shutdown() -> Result<()> {
    let mut daemon = TestDaemon::spawn()?;
    let resp = daemon.send_request(Request::Shutdown).await?;
    assert!(matches!(resp, Response::Ok(None)));

    // Wait for process to exit
    let mut attempts = 0;
    let mut exited = false;
    while attempts < 50 {
        if let Ok(Some(_)) = daemon.child.try_wait() {
            exited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }

    assert!(exited, "Daemon did not exit after shutdown request");

    // Verify cleanup
    assert!(!daemon.socket_path.exists(), "Socket file still exists");
    assert!(!daemon.pid_path.exists(), "PID file still exists");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires restic and fusermount3
async fn test_ipc_mount_unmount() -> Result<()> {
    let daemon = TestDaemon::spawn()?;

    // 1. Initialize repository (needed for mount to work)
    let target_dir = daemon.temp_dir.path().join("target");
    let pw_file = daemon.temp_dir.path().join("config/vigil/.repo_password");
    fs::write(&pw_file, "testpassword")?;

    let status = Command::new("restic")
        .args([
            "init",
            "--repo",
            target_dir.to_str().unwrap(),
            "--password-file",
            pw_file.to_str().unwrap(),
        ])
        .status()?;
    assert!(status.success(), "Failed to init restic repo");

    // 2. Send Mount request
    let resp = daemon
        .send_request(Request::Mount {
            set_name: "test-set".to_string(),
            snapshot_id: None,
        })
        .await?;

    if let Response::Ok(Some(ResponseData::MountPath { path })) = resp {
        let mount_path = std::path::PathBuf::from(path);
        assert!(mount_path.exists());
        // Note: checking if it's actually mounted might be tricky as restic takes a bit to mount
        // and needs fusermount3. We at least verify the response and directory existence.
    } else {
        panic!("Unexpected response to Mount: {:?}", resp);
    }

    // 3. Send Unmount request
    let resp = daemon
        .send_request(Request::Unmount {
            set_name: Some("test-set".to_string()),
        })
        .await?;

    assert!(matches!(resp, Response::Ok(None)));

    Ok(())
}

#[tokio::test]
#[ignore] // Requires restic and fusermount3
async fn test_ipc_mount_cleanup_on_shutdown() -> Result<()> {
    let daemon = TestDaemon::spawn()?;

    // 1. Initialize repository
    let target_dir = daemon.temp_dir.path().join("target");
    let pw_file = daemon.temp_dir.path().join("config/vigil/.repo_password");
    fs::write(&pw_file, "testpassword")?;

    let status = Command::new("restic")
        .args([
            "init",
            "--repo",
            target_dir.to_str().unwrap(),
            "--password-file",
            pw_file.to_str().unwrap(),
        ])
        .status()?;
    assert!(status.success(), "Failed to init restic repo");

    // 2. Send Mount request
    let resp = daemon
        .send_request(Request::Mount {
            set_name: "test-set".to_string(),
            snapshot_id: None,
        })
        .await?;

    let mount_path = if let Response::Ok(Some(ResponseData::MountPath { path })) = resp {
        std::path::PathBuf::from(path)
    } else {
        panic!("Unexpected response to Mount: {:?}", resp);
    };

    assert!(mount_path.exists());

    // 3. Send Shutdown request
    let resp = daemon.send_request(Request::Shutdown).await?;
    assert!(matches!(resp, Response::Ok(None)));

    // Wait for daemon to exit and cleanup
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 4. Verify mount is gone
    // Note: even if it's still "mounted" in the kernel due to some delay,
    // the restic process should be gone.
    // We can check if the directory is empty or gone if unmount worked.
    // On Linux, fusermount3 -u might keep the directory but it won't be a mount point.

    // A better way is to check the process. But we don't have the PID easily here.
    // However, the daemon.cleanup() handles socket/pid file removal.
    assert!(!daemon.socket_path.exists(), "Socket file should be gone");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires restic
async fn test_ipc_prune() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join("config");
    let data_dir = temp_dir.path().join("data");
    let runtime_dir = temp_dir.path().join("runtime");

    fs::create_dir_all(&config_dir)?;
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&runtime_dir)?;

    let source_dir = temp_dir.path().join("source");
    let target_dir = temp_dir.path().join("target");
    fs::create_dir_all(&source_dir)?;
    fs::create_dir_all(&target_dir)?;

    let pw_file = config_dir.join("vigil/.repo_password");
    fs::create_dir_all(pw_file.parent().unwrap())?;
    fs::write(&pw_file, "testpassword")?;

    // 1. Initialize repository
    let status = Command::new("restic")
        .args([
            "init",
            "--repo",
            target_dir.to_str().unwrap(),
            "--password-file",
            pw_file.to_str().unwrap(),
        ])
        .status()?;
    assert!(status.success(), "Failed to init restic repo");

    // 2. Create a backup
    fs::write(source_dir.join("test.txt"), "hello world")?;
    let status = Command::new("restic")
        .args([
            "backup",
            "--repo",
            target_dir.to_str().unwrap(),
            "--password-file",
            pw_file.to_str().unwrap(),
            source_dir.to_str().unwrap(),
        ])
        .status()?;
    assert!(status.success(), "Failed to create backup");

    // 3. Create config WITH retention
    let config_path = config_dir.join("vigil/config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test-set"
source = "{}"
target = "{}"
retention = {{ keep_last = 1 }}
"#,
            source_dir.display(),
            target_dir.display()
        ),
    )?;

    // 4. Start daemon
    let daemon_path = env!("CARGO_BIN_EXE_vigil-daemon");
    let mut child = Command::new(daemon_path)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("VIGIL_CONFIG", &config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let socket_path = runtime_dir.join("vigil.sock");

    // Wait for socket
    let mut attempts = 0;
    while !socket_path.exists() && attempts < 50 {
        if let Ok(Some(status)) = child.try_wait() {
            let mut stderr = String::new();
            if let Some(mut reader) = child.stderr.take() {
                use std::io::Read;
                let _ = reader.read_to_string(&mut stderr);
            }
            panic!(
                "Daemon exited prematurely with status: {}\nStderr: {}",
                status, stderr
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }
    assert!(
        socket_path.exists(),
        "Daemon failed to start or create socket within timeout"
    );

    // 5. Send Prune request
    let mut stream = UnixStream::connect(&socket_path).await?;
    let request = Request::Prune {
        set_name: Some("test-set".to_string()),
    };
    let json = serde_json::to_string(&request)? + "\n";
    stream.write_all(json.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let resp: Response = serde_json::from_str(&line)?;

    if let Response::Ok(Some(ResponseData::PruneResult {
        set_name,
        reclaimed_bytes,
    })) = resp
    {
        // Prune succeeded - since we only have one snapshot and keep_last=1,
        // reclaimed_bytes will be 0, but the command completed successfully.
        assert_eq!(set_name, "test-set");
        // Note: reclaimed_bytes is u64, so no need to check >= 0
        let _ = reclaimed_bytes;
    } else {
        panic!("Unexpected response to Prune: {:?}", resp);
    }

    let _ = child.kill();
    Ok(())
}
