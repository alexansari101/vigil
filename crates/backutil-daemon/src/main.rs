use anyhow::{Context, Result};
use backutil_lib::ipc::{Request, Response, ResponseData};
use backutil_lib::paths;
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

struct Daemon {
    pid_path: PathBuf,
    socket_path: PathBuf,
    shutdown_tx: broadcast::Sender<()>,
}

impl Daemon {
    fn new() -> Result<Self> {
        let pid_path = paths::pid_path();
        let socket_path = paths::socket_path();
        let (shutdown_tx, _) = broadcast::channel(1);
        Ok(Self {
            pid_path,
            socket_path,
            shutdown_tx,
        })
    }

    fn create_pid_file(&self) -> Result<()> {
        if self.pid_path.exists() {
            let old_pid = fs::read_to_string(&self.pid_path)?;
            if let Ok(pid) = old_pid.trim().parse::<i32>() {
                // Check if process exists
                if unsafe { libc::kill(pid, 0) } == 0 {
                    anyhow::bail!("Daemon is already running with PID {}", pid);
                } else {
                    warn!("Stale PID file found (PID {}), removing...", pid);
                    let _ = fs::remove_file(&self.pid_path);
                }
            }
        }

        if let Some(parent) = self.pid_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.pid_path, std::process::id().to_string())
            .context("Failed to write PID file")?;
        Ok(())
    }

    fn cleanup(&self) {
        // Only cleanup if the PID file contains our PID
        if let Ok(content) = fs::read_to_string(&self.pid_path) {
            if content.trim() == std::process::id().to_string() {
                info!("Cleaning up PID and socket files...");
                let _ = fs::remove_file(&self.pid_path);
                let _ = fs::remove_file(&self.socket_path);
            }
        }
    }

    async fn run(&self) -> Result<()> {
        self.create_pid_file()?;

        // Ensure socket directory exists
        if let Some(parent) = self.socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Remove old socket if it exists
        if self.socket_path.exists() {
            fs::remove_file(&self.socket_path)?;
        }

        let listener =
            UnixListener::bind(&self.socket_path).context("Failed to bind Unix socket")?;

        info!("Daemon listening on {:?}", self.socket_path);

        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let shutdown_tx = self.shutdown_tx.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, shutdown_tx).await {
                                    error!("Error handling client: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down...");
                    break;
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down...");
                    break;
                }
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown request via IPC, shutting down...");
                    break;
                }
            }
        }

        Ok(())
    }
}

async fn handle_client(mut stream: UnixStream, shutdown_tx: broadcast::Sender<()>) -> Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let err_resp = Response::Error {
                    code: "InvalidRequest".into(),
                    message: format!("Failed to parse JSON: {}", e),
                };
                let json = serde_json::to_string(&err_resp)? + "\n";
                writer.write_all(json.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        line.clear();

        let response = match request {
            Request::Ping => Response::Pong,
            Request::Status => {
                // TODO: Implement actual status
                Response::Ok(Some(ResponseData::Status { sets: vec![] }))
            }
            Request::Shutdown => {
                info!("Shutdown requested via IPC");
                // Send shutdown signal before responding
                let _ = shutdown_tx.send(());
                Response::Ok(None)
            }
            _ => Response::Error {
                code: "NotImplemented".into(),
                message: "Command not implemented yet".into(),
            },
        };

        let json = serde_json::to_string(&response)? + "\n";
        writer.write_all(json.as_bytes()).await?;

        // If shutdown was requested, close connection after responding
        if matches!(request, Request::Shutdown) {
            break;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let daemon = Daemon::new()?;

    let res = daemon.run().await;

    daemon.cleanup();

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_pid_file_management() -> Result<()> {
        let tmp = tempdir()?;
        let pid_path = tmp.path().join("backutil.pid");
        let socket_path = tmp.path().join("backutil.sock");

        let (shutdown_tx, _) = broadcast::channel(1);

        let daemon = Daemon {
            pid_path: pid_path.clone(),
            socket_path: socket_path.clone(),
            shutdown_tx,
        };

        daemon.create_pid_file()?;
        assert!(pid_path.exists());

        let pid_content = fs::read_to_string(&pid_path)?;
        assert_eq!(pid_content, std::process::id().to_string());

        daemon.cleanup();
        assert!(!pid_path.exists());

        Ok(())
    }
}
