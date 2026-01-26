use anyhow::Result;
use backutil_lib::ipc::{Request, Response, ResponseData};
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

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

        let config_path = config_dir.join("backutil/config.toml");
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

        let daemon_path = env!("CARGO_BIN_EXE_backutil-daemon");

        let mut child = Command::new(daemon_path)
            .env("XDG_CONFIG_HOME", &config_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let socket_path = runtime_dir.join("backutil.sock");
        let pid_path = runtime_dir.join("backutil.pid");

        // Wait for socket to appear
        let mut attempts = 0;
        while !socket_path.exists() && attempts < 50 {
            std::thread::sleep(Duration::from_millis(100));
            attempts += 1;
        }

        if !socket_path.exists() {
            // Check if process is still running
            if let Ok(Some(status)) = child.try_wait() {
                panic!("Daemon exited prematurely with status: {}", status);
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
