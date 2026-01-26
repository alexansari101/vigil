use anyhow::{Context, Result};
use backutil_lib::config::{load_config, Config};
use backutil_lib::ipc::{Request, Response, ResponseData};
use backutil_lib::paths;
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use backutil_daemon::manager::JobManager;
use backutil_daemon::watcher::{FileWatcher, WatcherEvent};
use std::sync::Arc;

struct Daemon {
    pid_path: PathBuf,
    socket_path: PathBuf,
    config: Config,
    shutdown_tx: broadcast::Sender<()>,
    job_manager: Arc<JobManager>,
}

impl Daemon {
    fn new() -> Result<Self> {
        let pid_path = paths::pid_path();
        let socket_path = paths::socket_path();
        let config = load_config().context("Failed to load configuration")?;
        let (shutdown_tx, _) = broadcast::channel(1);
        let job_manager = Arc::new(JobManager::new(&config));
        Ok(Self {
            pid_path,
            socket_path,
            config,
            shutdown_tx,
            job_manager,
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

        let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel(100);
        let _watcher =
            FileWatcher::new(&self.config, watcher_tx).context("Failed to start file watcher")?;

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
                            let job_manager = self.job_manager.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, shutdown_tx, job_manager).await {
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
                res = watcher_rx.recv() => {
                    if let Some(event) = res {
                        match event {
                            WatcherEvent::FileChanged { set_name, path } => {
                                debug!("File change detected for set {}: {:?}", set_name, path);
                                if let Err(e) = self.job_manager.handle_file_change(&set_name).await {
                                    error!("Error handling file change for set {}: {}", set_name, e);
                                }
                            }
                        }
                    }
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

async fn handle_client(
    mut stream: UnixStream,
    shutdown_tx: broadcast::Sender<()>,
    job_manager: Arc<JobManager>,
) -> Result<()> {
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
        let is_shutdown = matches!(request, Request::Shutdown);

        let response = match request {
            Request::Ping => Response::Pong,
            Request::Status => {
                let sets = job_manager.get_status().await;
                Response::Ok(Some(ResponseData::Status { sets }))
            }
            Request::Shutdown => {
                info!("Shutdown requested via IPC");
                // Send shutdown signal before responding
                let _ = shutdown_tx.send(());
                Response::Ok(None)
            }
            Request::Backup { set_name } => {
                match set_name {
                    Some(name) => match job_manager.trigger_backup(&name).await {
                        Ok(_) => Response::Ok(Some(ResponseData::BackupStarted { set_name: name })),
                        Err(e) => Response::Error {
                            code: "BackupFailed".into(),
                            message: e.to_string(),
                        },
                    },
                    None => {
                        // Backup all sets
                        let statuses = job_manager.get_status().await;
                        let mut started = Vec::new();
                        let mut failed = Vec::new();
                        for status in statuses {
                            match job_manager.trigger_backup(&status.name).await {
                                Ok(_) => started.push(status.name),
                                Err(e) => {
                                    warn!(
                                        "Failed to trigger backup for set {}: {}",
                                        status.name, e
                                    );
                                    failed.push((status.name, e.to_string()));
                                }
                            }
                        }
                        Response::Ok(Some(ResponseData::BackupsTriggered { started, failed }))
                    }
                }
            }
            Request::Snapshots { set_name, limit: _ } => {
                match job_manager.get_snapshots(&set_name).await {
                    Ok(snapshots) => Response::Ok(Some(ResponseData::Snapshots { snapshots })),
                    Err(e) => Response::Error {
                        code: "ResticError".into(),
                        message: e.to_string(),
                    },
                }
            }
            _ => Response::Error {
                code: "NotImplemented".into(),
                message: "Command not implemented yet".into(),
            },
        };

        let json = serde_json::to_string(&response)? + "\n";
        writer.write_all(json.as_bytes()).await?;

        // If shutdown was requested, close connection after responding
        if is_shutdown {
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
            config: Config {
                global: Default::default(),
                backup_sets: vec![],
            },
            shutdown_tx,
            job_manager: Arc::new(JobManager::new(&Config {
                global: Default::default(),
                backup_sets: vec![],
            })),
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
