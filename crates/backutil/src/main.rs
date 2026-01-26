use anyhow::{anyhow, Context};
use backutil_lib::ipc::{Request, Response, ResponseData};
use backutil_lib::paths;
use backutil_lib::types::{JobState, SetStatus};
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Restic repository
    Init {
        /// Name of the backup set to initialize (null = all sets)
        set: Option<String>,
    },
    /// Run backup now
    Backup {
        /// Name of the backup set to backup (null = all sets)
        set: Option<String>,
    },
    /// Show health summary and recent snapshots
    Status,
    /// Mount a snapshot via FUSE
    Mount {
        /// Name of the backup set to mount
        set: String,
        /// Snapshot ID to mount (null = latest)
        snapshot_id: Option<String>,
    },
    /// Unmount FUSE mounts
    Unmount {
        /// Name of the backup set to unmount (null = all)
        set: Option<String>,
    },
    /// Trigger retention policy cleanup
    Prune {
        /// Name of the backup set to prune (null = all)
        set: Option<String>,
    },
    /// Launch interactive dashboard
    Tui,
    /// Generate and enable systemd user units
    Bootstrap,
    /// Stop and disable systemd units
    Disable,
    /// Remove systemd units
    Uninstall {
        /// Also remove config, logs, and password file
        #[arg(long)]
        purge: bool,
    },
    /// Tail the log file
    Logs {
        /// Follow mode
        #[arg(short, long)]
        follow: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { set } => {
            handle_init(set).await?;
        }
        Commands::Backup { set } => {
            handle_backup(set).await?;
        }
        Commands::Status => {
            handle_status().await?;
        }
        _ => {
            println!("Command not yet implemented.");
        }
    }

    Ok(())
}

async fn handle_init(set_name: Option<String>) -> anyhow::Result<()> {
    let config = backutil_lib::config::load_config().context("Failed to load configuration")?;
    let password_path = paths::password_path();

    if !password_path.exists() {
        println!("Repository password file not found.");
        let password = rpassword::prompt_password("Enter password for new repositories: ")?;
        let confirm = rpassword::prompt_password("Confirm password: ")?;

        if password != confirm {
            anyhow::bail!("Passwords do not match.");
        }

        if let Some(parent) = password_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&password_path, password)?;
        std::fs::set_permissions(&password_path, std::fs::Permissions::from_mode(0o600))?;
        println!("Password saved to {:?}", password_path);
    }

    let sets_to_init: Vec<_> = if let Some(name) = set_name {
        let set = config
            .backup_sets
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("Backup set '{}' not found in config", name))?;
        vec![set]
    } else {
        config.backup_sets.iter().collect()
    };

    if sets_to_init.is_empty() {
        println!("No backup sets found to initialize.");
        return Ok(());
    }

    let mut failed = false;

    for set in sets_to_init {
        println!(
            "Initializing repository for set '{}' at '{}'...",
            set.name, set.target
        );

        let output = tokio::process::Command::new("restic")
            .arg("init")
            .arg("--repo")
            .arg(&set.target)
            .arg("--password-file")
            .arg(&password_path)
            .output()
            .await?;

        if output.status.success() {
            println!("Successfully initialized set '{}'.", set.name);
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("repository master key and config already initialized")
                || stderr.contains("config already initialized")
                || stderr.contains("config file already exists")
            {
                println!("Set '{}' is already initialized.", set.name);
            } else {
                eprintln!("Failed to initialize set '{}': {}", set.name, stderr.trim());
                failed = true;
            }
        }
    }

    if failed {
        anyhow::bail!("One or more backup sets failed to initialize.");
    }

    Ok(())
}

async fn handle_backup(set_name: Option<String>) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    send_request(
        &mut stream,
        Request::Backup {
            set_name: set_name.clone(),
        },
    )
    .await?;

    let mut expected_completions = None;
    let mut completed_count = 0;
    let mut had_failures = false;

    while let Ok(response) = receive_response(&mut stream).await {
        match response {
            Response::Ok(Some(data)) => match data {
                ResponseData::BackupStarted {
                    set_name: started_set,
                } => {
                    println!("Backup started for set '{}'.", started_set);
                    if set_name.is_some() {
                        expected_completions = Some(1);
                    }
                }
                ResponseData::BackupsTriggered { started, failed } => {
                    for set in &started {
                        println!("Backup triggered for set '{}'.", set);
                    }
                    for (set, error) in &failed {
                        eprintln!("Failed to trigger backup for set '{}': {}", set, error);
                    }
                    if !failed.is_empty() {
                        had_failures = true;
                    }
                    if set_name.is_none() {
                        expected_completions = Some(started.len());
                    }
                }
                ResponseData::BackupComplete {
                    set_name: completed_set_name,
                    snapshot_id,
                    added_bytes,
                    duration_secs,
                } => {
                    println!(
                        "Backup complete for set '{}': snapshot {}, {} added in {:.1}s",
                        completed_set_name,
                        snapshot_id,
                        format_size(added_bytes),
                        duration_secs
                    );

                    completed_count += 1;
                    if let Some(expected) = expected_completions {
                        if completed_count >= expected {
                            break;
                        }
                    } else if let Some(target) = &set_name {
                        // Fallback if we somehow missed BackupStarted but got BackupComplete for the requested set
                        if target == &completed_set_name {
                            break;
                        }
                    }
                }
                ResponseData::BackupFailed {
                    set_name: failed_set,
                    error,
                } => {
                    eprintln!("Backup failed for set '{}': {}", failed_set, error);
                    had_failures = true;
                    completed_count += 1;
                    if let Some(expected) = expected_completions {
                        if completed_count >= expected {
                            break;
                        }
                    } else if let Some(target) = &set_name {
                        if target == &failed_set {
                            break;
                        }
                    }
                }
                _ => {}
            },
            Response::Ok(None) => {}
            Response::Error { code, message } => {
                eprintln!("Error from daemon ({}): {}", code, message);
                if code == backutil_lib::ipc::error_codes::RESTIC_ERROR
                    || code == backutil_lib::ipc::error_codes::BACKUP_FAILED
                {
                    std::process::exit(4);
                } else {
                    std::process::exit(1);
                }
            }
            Response::Pong => {}
        }
    }

    // Exit with code 4 (restic error) if any backups failed
    if had_failures {
        std::process::exit(4);
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

async fn handle_status() -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    send_request(&mut stream, Request::Status).await?;
    let response = receive_response(&mut stream).await?;

    match response {
        Response::Ok(Some(ResponseData::Status { sets })) => {
            display_status(sets);
        }
        Response::Ok(_) => {
            println!("Unexpected response from daemon.");
        }
        Response::Error { code, message } => {
            eprintln!("Error from daemon ({}): {}", code, message);
            std::process::exit(1);
        }
        Response::Pong => {
            println!("Unexpected Pong response.");
        }
    }

    Ok(())
}

async fn connect_to_daemon() -> anyhow::Result<UnixStream> {
    let socket_path = paths::socket_path();
    UnixStream::connect(&socket_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound
            || e.kind() == std::io::ErrorKind::ConnectionRefused
        {
            // Exit code 3 per spec.md
            eprintln!("Error: Daemon is not running.");
            std::process::exit(3);
        }
        anyhow!("Failed to connect to daemon: {}", e)
    })
}

async fn send_request(stream: &mut UnixStream, request: Request) -> anyhow::Result<()> {
    let json = serde_json::to_string(&request)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

async fn receive_response(stream: &mut UnixStream) -> anyhow::Result<Response> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    if line.is_empty() {
        return Err(anyhow!("Connection closed by daemon"));
    }
    let response: Response = serde_json::from_str(&line)?;
    Ok(response)
}

fn display_status(sets: Vec<SetStatus>) {
    if sets.is_empty() {
        println!("No backup sets configured.");
        return;
    }

    println!(
        "{:<15} {:<15} {:<20} {:<10}",
        "NAME", "STATE", "LAST BACKUP", "MOUNTED"
    );
    println!("{}", "-".repeat(65));

    for set in sets {
        let state_str = match set.state {
            JobState::Idle => "Idle".to_string(),
            JobState::Debouncing { remaining_secs } => {
                format!("Debouncing ({:?}s)", remaining_secs)
            }
            JobState::Running => "Running".to_string(),
            JobState::Error => "Error".to_string(),
        };

        let last_backup_str = match set.last_backup {
            Some(ref result) => {
                let now = Utc::now();
                let duration = now.signed_duration_since(result.timestamp);
                let time_str = format_human_duration(duration);
                if result.success {
                    time_str
                } else {
                    format!("{} (failed)", time_str)
                }
            }
            None => "Never".to_string(),
        };

        let mounted_str = if set.is_mounted { "Yes" } else { "No" };

        println!(
            "{:<15} {:<15} {:<20} {:<10}",
            set.name, state_str, last_backup_str, mounted_str
        );
    }
}

/// Formats a chrono Duration into a human-readable relative time string.
/// Handles negative durations gracefully by showing "just now".
fn format_human_duration(duration: Duration) -> String {
    let secs = duration.num_seconds();
    if secs < 0 {
        return "just now".to_string();
    }
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        if mins == 1 {
            "1 min ago".to_string()
        } else {
            format!("{} mins ago", mins)
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else {
        let days = secs / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_human_duration_seconds() {
        assert_eq!(format_human_duration(Duration::seconds(0)), "0s ago");
        assert_eq!(format_human_duration(Duration::seconds(30)), "30s ago");
        assert_eq!(format_human_duration(Duration::seconds(59)), "59s ago");
    }

    #[test]
    fn test_format_human_duration_minutes() {
        assert_eq!(format_human_duration(Duration::seconds(60)), "1 min ago");
        assert_eq!(format_human_duration(Duration::seconds(61)), "1 min ago");
        assert_eq!(format_human_duration(Duration::seconds(120)), "2 mins ago");
        assert_eq!(
            format_human_duration(Duration::seconds(3599)),
            "59 mins ago"
        );
    }

    #[test]
    fn test_format_human_duration_hours() {
        assert_eq!(format_human_duration(Duration::seconds(3600)), "1 hour ago");
        assert_eq!(
            format_human_duration(Duration::seconds(7200)),
            "2 hours ago"
        );
        assert_eq!(
            format_human_duration(Duration::seconds(86399)),
            "23 hours ago"
        );
    }

    #[test]
    fn test_format_human_duration_days() {
        assert_eq!(format_human_duration(Duration::seconds(86400)), "1 day ago");
        assert_eq!(
            format_human_duration(Duration::seconds(172800)),
            "2 days ago"
        );
        assert_eq!(
            format_human_duration(Duration::seconds(604800)),
            "7 days ago"
        );
    }

    #[test]
    fn test_format_human_duration_negative() {
        // Edge case: negative durations (clock skew) should show "just now"
        assert_eq!(format_human_duration(Duration::seconds(-1)), "just now");
        assert_eq!(format_human_duration(Duration::seconds(-3600)), "just now");
    }
}
