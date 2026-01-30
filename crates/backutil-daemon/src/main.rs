use anyhow::{Context, Result};
use backutil_lib::config::{load_config, Config};
use backutil_lib::ipc::{Request, Response, ResponseData};
use backutil_lib::paths;
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use backutil_daemon::manager::JobManager;
use backutil_daemon::watcher::{FileWatcher, WatcherEvent};
use std::sync::Arc;

struct Daemon {
    pid_path: PathBuf,
    socket_path: PathBuf,
    config: Config,
    shutdown_token: CancellationToken,
    job_manager: Arc<JobManager>,
}

impl Daemon {
    fn new(shutdown_token: CancellationToken) -> Result<Self> {
        let pid_path = paths::pid_path();
        let socket_path = paths::socket_path();
        let config = load_config().context("Failed to load configuration")?;
        let job_manager = Arc::new(JobManager::new(&config, shutdown_token.clone()));
        Ok(Self {
            pid_path,
            socket_path,
            config,
            shutdown_token,
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

        // Query existing snapshots to populate status
        self.job_manager.initialize_status().await;

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
        let mut _watcher = FileWatcher::new(&self.config, watcher_tx.clone())
            .context("Failed to start file watcher")?;

        let (reload_tx, mut reload_rx) = tokio::sync::mpsc::channel(1);

        // Watch config file for changes
        let config_path = std::env::var("BACKUTIL_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| paths::config_path());
        let config_reload_tx = reload_tx.clone();
        let mut _config_watcher = RecommendedWatcher::new(
            move |res: std::result::Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if !event.kind.is_access() {
                        let _ = config_reload_tx.try_send(());
                    }
                }
            },
            NotifyConfig::default(),
        )?;
        if config_path.exists() {
            _config_watcher.watch(&config_path, RecursiveMode::NonRecursive)?;
        }

        info!("Daemon listening on {:?}", self.socket_path);

        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let shutdown_token = self.shutdown_token.clone();
                            let reload_tx = reload_tx.clone();
                            let job_manager = self.job_manager.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, shutdown_token, reload_tx, job_manager).await {
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
                    self.shutdown_token.cancel();
                    break;
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down...");
                    self.shutdown_token.cancel();
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
                _ = reload_rx.recv() => {
                    info!("Reloading configuration...");
                    match load_config() {
                        Ok(new_config) => {
                            if let Err(e) = self.job_manager.sync_config(&new_config).await {
                                error!("Failed to sync job manager with new config: {}", e);
                            } else {
                                // Re-create watcher with new config
                                match FileWatcher::new(&new_config, watcher_tx.clone()) {
                                    Ok(new_watcher) => {
                                        _watcher = new_watcher;
                                        info!("Configuration reloaded and file watcher updated");
                                    }
                                    Err(e) => {
                                        error!("Failed to restart file watcher after config reload: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to load configuration for reload: {}", e);
                        }
                    }
                }
                _ = self.shutdown_token.cancelled() => {
                    info!("Shutdown requested via IPC, shutting down...");
                    break;
                }
            }
        }

        // Cleanup any active mounts on shutdown
        if let Err(e) = self.job_manager.unmount(None).await {
            error!("Error unmounting sets on shutdown: {}", e);
        }

        Ok(())
    }
}

async fn handle_client(
    mut stream: UnixStream,
    shutdown_token: CancellationToken,
    reload_tx: tokio::sync::mpsc::Sender<()>,
    job_manager: Arc<JobManager>,
) -> Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut event_rx = job_manager.subscribe();

    loop {
        tokio::select! {
            read_res = reader.read_line(&mut line) => {
                let bytes_read = read_res?;
                if bytes_read == 0 {
                    break;
                }

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
                        // Trigger shutdown
                        shutdown_token.cancel();
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
                    Request::Snapshots { set_name, limit } => {
                        match job_manager.get_snapshots(&set_name, limit).await {
                            Ok(snapshots) => Response::Ok(Some(ResponseData::Snapshots { snapshots })),
                            Err(e) => Response::Error {
                                code: "ResticError".into(),
                                message: e.to_string(),
                            },
                        }
                    }
                    Request::Mount {
                        set_name,
                        snapshot_id,
                    } => match job_manager.mount(&set_name, snapshot_id).await {
                        Ok(path) => Response::Ok(Some(ResponseData::MountPath {
                            path: path.to_string_lossy().to_string(),
                        })),
                        Err(e) => Response::Error {
                            code: "MountFailed".into(),
                            message: e.to_string(),
                        },
                    },
                    Request::Unmount { set_name } => match job_manager.unmount(set_name).await {
                        Ok(_) => Response::Ok(None),
                        Err(e) => Response::Error {
                            code: "ResticError".into(),
                            message: e.to_string(),
                        },
                    },
                    Request::Prune { set_name } => match job_manager.prune(set_name).await {
                        Ok(data) => Response::Ok(Some(data)),
                        Err(e) => Response::Error {
                            code: "ResticError".into(),
                            message: e.to_string(),
                        },
                    },
                    Request::ReloadConfig => {
                        let _ = reload_tx.send(()).await;
                        Response::Ok(None)
                    }
                };

                let json = serde_json::to_string(&response)? + "\n";
                writer.write_all(json.as_bytes()).await?;

                // If shutdown was requested, close connection after responding
                if is_shutdown {
                    break;
                }
            }
            event_res = event_rx.recv() => {
                match event_res {
                    Ok(response) => {
                        let json = serde_json::to_string(&response)? + "\n";
                        writer.write_all(json.as_bytes()).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client lagged behind on broadcast events by {}", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Job manager was dropped, should only happen on shutdown
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn init_logging() -> WorkerGuard {
    let log_path_full = paths::log_path();
    let log_dir = log_path_full
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    // tracing_appender::rolling::daily will create files like "backutil.log.YYYY-MM-DD" inside log_dir
    let file_appender = tracing_appender::rolling::daily(log_dir, "backutil.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer().with_writer(non_blocking).with_ansi(false);

    let stdout_layer = if std::env::var("BACKUTIL_LOG_STDOUT").is_ok() {
        Some(fmt::layer().with_writer(std::io::stdout))
    } else {
        None
    };

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with rotation
    let _guard = init_logging();

    let shutdown_token = CancellationToken::new();
    let daemon = Daemon::new(shutdown_token)?;

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
        let shutdown_token = CancellationToken::new();

        let daemon = Daemon {
            pid_path: pid_path.clone(),
            socket_path: socket_path.clone(),
            config: Config {
                global: Default::default(),
                backup_sets: vec![],
            },
            shutdown_token: shutdown_token.clone(),
            job_manager: Arc::new(JobManager::new(
                &Config {
                    global: Default::default(),
                    backup_sets: vec![],
                },
                shutdown_token,
            )),
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
