use anyhow::anyhow;
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
        Commands::Status => {
            handle_status().await?;
        }
        _ => {
            println!("Command not yet implemented.");
        }
    }

    Ok(())
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
            Some(result) => {
                let now = Utc::now();
                let duration = now.signed_duration_since(result.timestamp);
                format_human_duration(duration)
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

fn format_human_duration(duration: Duration) -> String {
    let secs = duration.num_seconds();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{} hours ago", secs / 3600)
    } else {
        format!("{} days ago", secs / 86400)
    }
}
