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
    /// Show results in JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Restic repository
    Init {
        /// Name of the backup set to initialize. If omitted, initializes all sets.
        set: Option<String>,
    },
    /// Start a backup now
    Backup {
        /// Name of the backup set to back up. If omitted, backs up all sets.
        set: Option<String>,
        /// Do not wait for the backup to complete
        #[arg(long, conflicts_with = "timeout")]
        no_wait: bool,
        /// Maximum time to wait for completion (in seconds)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Show health summary and recent snapshots
    Status,
    /// Mount a backup as a folder
    Mount {
        /// Name of the backup set to mount
        set: String,
        /// Specific snapshot ID to mount. If omitted, mounts the latest one.
        snapshot_id: Option<String>,
    },
    /// Unmount previously mounted folders
    Unmount {
        /// Name of the backup set to unmount. If omitted, unmounts all.
        set: Option<String>,
    },
    /// Clean up old backups according to retention policy
    Prune {
        /// Name of the backup set to prune. If omitted, prunes all.
        set: Option<String>,
    },
    /// Launch interactive dashboard
    Tui,
    /// Generate and enable the background service
    Bootstrap,
    /// Stop and disable the background service
    Disable,
    /// Remove the background service
    Uninstall {
        /// Also remove configuration, logs, and password files
        #[arg(long)]
        purge: bool,
    },
    /// Tail the log file
    Logs {
        /// Follow mode
        #[arg(short, long)]
        follow: bool,
    },
    /// List all defined backup sets
    List,
    /// Permanently delete a backup set and its repository
    Purge {
        /// Name of the backup set to delete
        set: String,
        /// Skip confirmation and force deletion even if set is in configuration
        #[arg(long)]
        force: bool,
    },
    /// Show all available backups for a set
    Snapshots {
        /// Name of the backup set
        set: String,
        /// Limit the number of backups shown
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Check if configuration and repositories are healthy
    Check {
        /// Name of the backup set to check. If omitted, checks all.
        set: Option<String>,
        /// Only check configuration, do not try to reach repositories
        #[arg(long)]
        config_only: bool,
    },
    /// Reload the daemon configuration
    Reload,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let json = cli.json;
    let quiet = cli.quiet;

    match cli.command {
        Commands::Init { set } => {
            handle_init(set, json, quiet).await?;
        }
        Commands::Backup {
            set,
            no_wait,
            timeout,
        } => {
            handle_backup(set, no_wait, timeout, json, quiet).await?;
        }
        Commands::Status => {
            handle_status(json, quiet).await?;
        }
        Commands::Mount { set, snapshot_id } => {
            handle_mount(set, snapshot_id, json, quiet).await?;
        }
        Commands::Unmount { set } => {
            handle_unmount(set, json, quiet).await?;
        }
        Commands::Prune { set } => {
            handle_prune(set, json, quiet).await?;
        }
        Commands::Logs { follow } => {
            handle_logs(follow, json, quiet).await?;
        }
        Commands::Bootstrap => {
            handle_bootstrap(json, quiet).await?;
        }
        Commands::Disable => {
            handle_disable(json, quiet).await?;
        }
        Commands::Uninstall { purge } => {
            handle_uninstall(purge, json, quiet).await?;
        }
        Commands::Purge { set, force } => {
            handle_purge(set, force, json, quiet).await?;
        }
        Commands::List => {
            handle_list(json, quiet).await?;
        }
        Commands::Snapshots { set, limit } => {
            handle_snapshots(set, limit, json, quiet).await?;
        }
        Commands::Check { set, config_only } => {
            handle_check(set, config_only, json, quiet).await?;
        }
        Commands::Reload => {
            handle_reload(json, quiet).await?;
        }
        Commands::Tui => {
            println!("Command not yet implemented.");
        }
    }

    Ok(())
}

async fn handle_init(set_name: Option<String>, json: bool, quiet: bool) -> anyhow::Result<()> {
    let config = backutil_lib::config::load_config().context("Failed to load configuration")?;
    let password_path = paths::password_path();

    if !password_path.exists() {
        if !quiet && !json {
            println!("Repository password file not found.");
        }
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
        if !quiet && !json {
            println!("Password saved to {:?}", password_path);
        }
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
        if json {
            println!("[]");
        } else if !quiet {
            println!("No backup sets found to initialize.");
        }
        return Ok(());
    }

    let mut results = Vec::new();
    let mut failed = false;

    for set in sets_to_init {
        if !quiet && !json {
            println!(
                "Initializing repository for set '{}' at '{}'...",
                set.name, set.target
            );
        }

        let output = tokio::process::Command::new("restic")
            .arg("init")
            .arg("--repo")
            .arg(&set.target)
            .arg("--password-file")
            .arg(&password_path)
            .output()
            .await?;

        if output.status.success() {
            if !quiet && !json {
                println!("Successfully initialized set '{}'.", set.name);
            }
            results.push(serde_json::json!({
                "set": set.name,
                "status": "initialized"
            }));
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("repository master key and config already initialized")
                || stderr.contains("config already initialized")
                || stderr.contains("config file already exists")
            {
                if !quiet && !json {
                    println!("Set '{}' is already initialized.", set.name);
                }
                results.push(serde_json::json!({
                    "set": set.name,
                    "status": "already_initialized"
                }));
            } else {
                eprintln!("Failed to initialize set '{}': {}", set.name, stderr.trim());
                failed = true;
                results.push(serde_json::json!({
                    "set": set.name,
                    "status": "failed",
                    "error": stderr.trim()
                }));
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    if failed {
        anyhow::bail!("One or more backup sets failed to initialize.");
    }

    Ok(())
}

async fn handle_backup(
    set_name: Option<String>,
    no_wait: bool,
    timeout: Option<u64>,
    json: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(
        reader.get_mut(),
        Request::Backup {
            set_name: set_name.clone(),
        },
    )
    .await?;
    let mut expected_sets = std::collections::HashSet::new();
    let mut completed_count = 0;
    let mut had_failures = false;
    let mut initial_response_received = false;

    let timeout_duration = timeout.map(std::time::Duration::from_secs);
    let start_instant = std::time::Instant::now();

    loop {
        if let Some(d) = timeout_duration {
            if start_instant.elapsed() > d {
                anyhow::bail!("Timeout waiting for backup completion");
            }
        }

        let recv_timeout = std::time::Duration::from_millis(500);
        let res = tokio::time::timeout(recv_timeout, receive_response(&mut reader)).await;

        let response = match res {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(e),
            Err(_) => continue, // Timeout, check global timeout and loop
        };

        match response {
            Response::Ok(Some(ref data)) => match data {
                ResponseData::BackupStarted {
                    set_name: started_set,
                } => {
                    if json {
                        println!("{}", serde_json::to_string(data)?);
                    } else if !quiet {
                        println!("Backup started for set '{}'.", started_set);
                    }
                    expected_sets.insert(started_set.clone());
                    initial_response_received = true;
                }
                ResponseData::BackupsTriggered { started, failed } => {
                    if json {
                        println!("{}", serde_json::to_string(data)?);
                    }
                    for set in started {
                        if !quiet && !json {
                            println!("Backup triggered for set '{}'.", set);
                        }
                        expected_sets.insert(set.clone());
                    }
                    for (set, error) in failed {
                        eprintln!("Failed to trigger backup for set '{}': {}", set, error);
                        had_failures = true;
                    }
                    initial_response_received = true;
                }
                ResponseData::BackupComplete {
                    set_name: completed_set_name,
                    snapshot_id,
                    added_bytes,
                    duration_secs,
                } => {
                    if expected_sets.contains(completed_set_name) {
                        if json {
                            println!("{}", serde_json::to_string(data)?);
                        } else if !quiet {
                            println!(
                                "Backup complete for set '{}': snapshot {}, {} added in {:.1}s",
                                completed_set_name,
                                snapshot_id,
                                format_size(*added_bytes),
                                duration_secs
                            );
                        }
                        completed_count += 1;
                    }

                    if initial_response_received && completed_count >= expected_sets.len() {
                        break;
                    }
                }
                ResponseData::BackupFailed {
                    set_name: failed_set,
                    error,
                } => {
                    if expected_sets.contains(failed_set) {
                        if json {
                            println!("{}", serde_json::to_string(data)?);
                        }
                        eprintln!("Backup failed for set '{}': {}", failed_set, error);
                        had_failures = true;
                        completed_count += 1;
                    }
                    if initial_response_received && completed_count >= expected_sets.len() {
                        break;
                    }
                }
                _ => {}
            },
            Response::Ok(None) => {
                // Some Ok(None) might be returned for other requests, but here we expect data
            }
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

        if no_wait && initial_response_received {
            break;
        }

        if initial_response_received && expected_sets.is_empty() {
            break;
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

async fn handle_status(json: bool, quiet: bool) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(reader.get_mut(), Request::Status).await?;
    let response = receive_response(&mut reader).await?;

    match response {
        Response::Ok(Some(ResponseData::Status { sets })) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&sets)?);
            } else if !quiet {
                display_status(sets);
            }
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

async fn handle_mount(
    set_name: String,
    snapshot_id: Option<String>,
    json: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(
        reader.get_mut(),
        Request::Mount {
            set_name,
            snapshot_id,
        },
    )
    .await?;

    let response = receive_response(&mut reader).await?;
    match response {
        Response::Ok(Some(ref data)) => {
            if let ResponseData::MountPath { ref path } = data {
                if json {
                    println!("{}", serde_json::to_string(data)?);
                } else if !quiet {
                    println!("Repository mounted successfully.");
                    println!();
                    println!("Browse your snapshots at: {}/", path);
                    println!("  by ID:        {}/ids/<snapshot-id>/", path);
                    println!("  by timestamp: {}/snapshots/<timestamp>/", path);
                    println!("  by host:      {}/hosts/<hostname>/", path);
                    println!("  by tags:      {}/tags/<tag>/", path);
                    println!();
                    println!("Use `cp` to recover files, then `backutil unmount` when done.");
                }
            } else {
                println!("Unexpected response from daemon.");
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error mounting snapshot ({}): {}", code, message);
            std::process::exit(5); // Exit code 5 per spec.md Section 12: Mount/unmount error
        }
        _ => {
            println!("Unexpected response from daemon.");
        }
    }

    Ok(())
}

async fn handle_unmount(set_name: Option<String>, json: bool, quiet: bool) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(
        reader.get_mut(),
        Request::Unmount {
            set_name: set_name.clone(),
        },
    )
    .await?;

    let response = receive_response(&mut reader).await?;
    match response {
        Response::Ok(_) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "success",
                        "unmounted": set_name.as_deref().unwrap_or("all")
                    })
                );
            } else if !quiet {
                if let Some(name) = set_name {
                    println!("Successfully unmounted set '{}'.", name);
                } else {
                    println!("Successfully unmounted all sets.");
                }
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error unmounting ({}): {}", code, message);
            std::process::exit(5); // Exit code 5 per spec.md Section 12: Mount/unmount error
        }
        _ => {
            println!("Unexpected response from daemon.");
        }
    }

    Ok(())
}

async fn handle_logs(follow: bool, _json: bool, quiet: bool) -> anyhow::Result<()> {
    use std::io::Write;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let log_dir = paths::log_path().parent().unwrap().to_path_buf();

    let find_latest_log = || {
        if !log_dir.exists() {
            return None;
        }
        let active_log = log_dir.join("backutil.log");
        if active_log.exists() {
            return Some(active_log);
        }

        let entries = std::fs::read_dir(&log_dir).ok()?;
        let mut logs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("backutil.log"))
            .collect();
        logs.sort_by_key(|e| e.file_name());
        logs.last().map(|e| e.path())
    };

    let mut log_path = find_latest_log();

    if log_path.is_none() {
        if !follow {
            if !quiet {
                println!("No log files found in {:?}", log_dir);
            }
            return Ok(());
        }
        if !quiet {
            println!("Waiting for log file in {:?} to be created...", log_dir);
        }
        while log_path.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            log_path = find_latest_log();
        }
    }

    let log_path = log_path.unwrap();

    let mut file = tokio::fs::File::open(&log_path).await?;
    let mut pos;

    // Initial tail: show last ~4KB
    let metadata = file.metadata().await?;
    let size = metadata.len();
    if size > 4096 {
        pos = size - 4096;
    } else {
        pos = 0;
    }

    file.seek(std::io::SeekFrom::Start(pos)).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;

    let content = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<&str> = content.lines().collect();

    // If we didn't start at the beginning, the first line is likely partial
    if pos > 0 && !lines.is_empty() {
        lines.remove(0);
    }

    // Show last 20 lines
    let start_idx = if lines.len() > 20 {
        lines.len() - 20
    } else {
        0
    };
    for line in &lines[start_idx..] {
        println!("{}", line);
    }

    if !follow {
        return Ok(());
    }

    // Follow mode
    pos = size;
    let mut current_log_path = log_path;
    loop {
        let metadata = match tokio::fs::metadata(&current_log_path).await {
            Ok(m) => m,
            Err(_) => {
                // File might have been rotated/deleted, try to find latest again
                if let Some(latest) = find_latest_log() {
                    if latest != current_log_path {
                        if !quiet {
                            println!("--- Log shifted/rotated to {} ---", latest.display());
                            std::io::stdout().flush()?;
                        }
                        current_log_path = latest;
                        file = tokio::fs::File::open(&current_log_path).await?;
                        pos = 0;
                        continue;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                continue;
            }
        };

        let current_size = metadata.len();

        if current_size < pos {
            // Log file was truncated or rotated - re-open the file
            if !quiet {
                println!("--- Log file truncated ---");
                std::io::stdout().flush()?;
            }
            file = tokio::fs::File::open(&current_log_path).await?;
            pos = 0;
        }

        if current_size > pos {
            file.seek(std::io::SeekFrom::Start(pos)).await?;
            let mut new_content = Vec::new();
            match file.read_to_end(&mut new_content).await {
                Ok(n) if n > 0 => {
                    print!("{}", String::from_utf8_lossy(&new_content));
                    std::io::stdout().flush()?;
                    pos += n as u64;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error reading log: {}", e);
                    break;
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Check for log rotation
        if let Some(latest) = find_latest_log() {
            if latest != current_log_path {
                if !quiet {
                    println!("--- Log rotated to {} ---", latest.display());
                    std::io::stdout().flush()?;
                }
                current_log_path = latest;
                file = tokio::fs::File::open(&current_log_path).await?;
                pos = 0;
            }
        }
    }

    Ok(())
}

async fn handle_bootstrap(json: bool, quiet: bool) -> anyhow::Result<()> {
    if !quiet && !json {
        println!("Bootstrapping backutil...");
    }

    // 1. Dependency check
    let deps = ["restic", "fusermount3", "notify-send"];
    let mut missing = Vec::new();
    for dep in deps {
        // Use `which` for dependency check since some tools (e.g., notify-send)
        // don't reliably support --version flag
        let status = tokio::process::Command::new("which")
            .arg(dep)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;

        if status.is_err() || !status.unwrap().success() {
            missing.push(dep);
        }
    }

    if !missing.is_empty() && !quiet && !json {
        println!("Warning: Missing dependencies: {}", missing.join(", "));
        println!("Please install them to use all features.");
    }

    // 2. Generate systemd unit file
    let unit_path = paths::systemd_unit_path();
    if let Some(parent) = unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let unit_content = r#"[Unit]
Description=Backutil Daemon - Automated Backup Service
After=default.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/backutil-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#;

    std::fs::write(&unit_path, unit_content)?;
    if !quiet && !json {
        println!("Generated systemd unit at {:?}", unit_path);
    }

    // 3. systemctl --user daemon-reload
    if !quiet && !json {
        println!("Reloading systemd daemon...");
    }
    let status = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("daemon-reload")
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("Failed to reload systemd daemon.");
    }

    // 4. systemctl --user enable --now backutil-daemon.service
    if !quiet && !json {
        println!("Enabling and starting backutil-daemon service...");
    }
    let status = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("enable")
        .arg("--now")
        .arg("backutil-daemon.service")
        .status()
        .await?;

    if status.success() {
        if json {
            println!("{}", serde_json::json!({ "status": "bootstrapped" }));
        } else if !quiet {
            println!("Successfully bootstrapped backutil-daemon.");
        }
    } else {
        anyhow::bail!("Failed to enable/start backutil-daemon service.");
    }

    Ok(())
}

/// Check if any mounts are active and warn the user
fn warn_if_mounts_active() {
    let mount_base = paths::mount_base_dir();
    if mount_base.exists() {
        if let Ok(entries) = std::fs::read_dir(&mount_base) {
            let active_mounts: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().is_dir()
                        && std::fs::read_dir(e.path())
                            .map(|mut r| r.next().is_some())
                            .unwrap_or(false)
                })
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            if !active_mounts.is_empty() {
                println!(
                    "Warning: Active mounts detected: {}. Consider unmounting first with `backutil unmount`.",
                    active_mounts.join(", ")
                );
            }
        }
    }
}

async fn handle_disable(json: bool, quiet: bool) -> anyhow::Result<()> {
    if !quiet && !json {
        warn_if_mounts_active();
        println!("Stopping and disabling backutil-daemon service...");
    }
    let status = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("disable")
        .arg("--now")
        .arg("backutil-daemon.service")
        .status()
        .await?;

    if status.success() {
        if json {
            println!("{}", serde_json::json!({ "status": "disabled" }));
        } else if !quiet {
            println!("Successfully disabled backutil-daemon.");
        }
    } else {
        anyhow::bail!("Failed to disable backutil-daemon service.");
    }

    Ok(())
}

async fn handle_uninstall(purge: bool, json: bool, quiet: bool) -> anyhow::Result<()> {
    if !quiet && !json {
        warn_if_mounts_active();
        println!("Uninstalling backutil...");
    }

    // 1. Stop and disable service
    let _ = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("stop")
        .arg("backutil-daemon.service")
        .status()
        .await;

    let _ = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("disable")
        .arg("backutil-daemon.service")
        .status()
        .await;

    // 2. Remove unit file
    let unit_path = paths::systemd_unit_path();
    if unit_path.exists() {
        std::fs::remove_file(&unit_path)?;
        if !quiet && !json {
            println!("Removed systemd unit {:?}", unit_path);
        }
    }

    // 3. daemon-reload
    let _ = tokio::process::Command::new("systemctl")
        .arg("--user")
        .arg("daemon-reload")
        .status()
        .await;

    // 4. Purge if requested
    if purge {
        println!("Purging configuration and data...");
        let config_dir = paths::config_dir();
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir)?;
            if !quiet && !json {
                println!("Removed configuration directory {:?}", config_dir);
            }
        }

        let data_dir = paths::log_path()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                let mut p = std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
                p.push(".local");
                p.push("share");
                p.push("backutil");
                p
            });

        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir)?;
            if !quiet && !json {
                println!("Removed data directory {:?}", data_dir);
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({ "status": "uninstalled", "purged": purge })
        );
    } else if !quiet {
        println!("Uninstall complete.");
    }
    Ok(())
}

async fn handle_prune(set_name: Option<String>, json: bool, quiet: bool) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(
        reader.get_mut(),
        Request::Prune {
            set_name: set_name.clone(),
        },
    )
    .await?;

    let response = receive_response(&mut reader).await?;
    match response {
        Response::Ok(Some(ref data)) => match data {
            ResponseData::PruneResult {
                set_name,
                reclaimed_bytes,
            } => {
                if json {
                    println!("{}", serde_json::to_string(data)?);
                } else if !quiet {
                    println!(
                        "Pruned set '{}': {} reclaimed",
                        set_name,
                        format_size(*reclaimed_bytes)
                    );
                }
            }
            ResponseData::PrunesTriggered { succeeded, failed } => {
                if json {
                    println!("{}", serde_json::to_string(data)?);
                } else if !quiet {
                    if succeeded.is_empty() && failed.is_empty() {
                        println!("No backup sets found to prune.");
                        return Ok(());
                    }

                    println!("{:<15} {:<15}", "NAME", "RECLAIMED");
                    println!("{}", "-".repeat(31));

                    let mut total_reclaimed = 0;
                    for (name, reclaimed) in succeeded {
                        println!("{:<15} {:<15}", name, format_size(*reclaimed));
                        total_reclaimed += reclaimed;
                    }

                    for (name, error) in failed {
                        println!("{:<15} Error: {:<15}", name, error);
                    }

                    println!("{}", "-".repeat(31));
                    println!("{:<15} {:<15}", "TOTAL", format_size(total_reclaimed));
                }

                if !failed.is_empty() {
                    anyhow::bail!("One or more prune operations failed.");
                }
            }
            _ => {
                println!("Unexpected response from daemon.");
            }
        },
        Response::Ok(None) => {
            println!("Prune operation completed.");
        }
        Response::Error { code, message } => {
            eprintln!("Error from daemon ({}): {}", code, message);
            // Exit code 4 for restic errors per spec.md Section 12
            std::process::exit(4);
        }
        _ => {
            println!("Unexpected response from daemon.");
        }
    }

    Ok(())
}

async fn handle_check(
    set_name: Option<String>,
    config_only: bool,
    json: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    // 1. Config Validation
    let config = match backutil_lib::config::load_config() {
        Ok(c) => c,
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "error", "error": e.to_string(), "code": 2 })
                );
            } else {
                eprintln!("✗ Configuration invalid: {}", e);
            }
            std::process::exit(2);
        }
    };

    if !json && !quiet {
        println!(
            "✓ Configuration valid: {} backup sets defined",
            config.backup_sets.len()
        );
    }

    let password_path = paths::password_path();
    let password_exists = password_path.exists();

    if config_only {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "config_valid": true,
                    "backup_sets_count": config.backup_sets.len(),
                    "password_file_exists": password_exists
                })
            );
        } else if !quiet {
            if password_exists {
                println!("✓ Password file exists");
            } else {
                println!("✗ Password file missing at {:?}", password_path);
            }
        }

        if !password_exists {
            std::process::exit(2);
        }
        return Ok(());
    }

    // 2. Repo Validation
    if !password_exists {
        if json {
            println!(
                "{}",
                serde_json::json!({ "status": "error", "error": "Password file missing", "code": 2 })
            );
        } else {
            eprintln!("✗ Password file missing at {:?}", password_path);
            eprintln!("  Run `backutil init` to create it.");
        }
        std::process::exit(2);
    } else if !json && !quiet {
        println!("✓ Password file exists");
    }

    let sets_to_check: Vec<_> = if let Some(name) = set_name {
        let set = config
            .backup_sets
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow!("Backup set '{}' not found in config", name))?;
        vec![set]
    } else {
        config.backup_sets.iter().collect()
    };

    if sets_to_check.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({ "status": "ok", "sets_checked": 0 })
            );
        } else if !quiet {
            println!("No backup sets found to check.");
        }
        return Ok(());
    }

    let mut failed = false;
    let mut results = Vec::new();

    for set in sets_to_check {
        if !json && !quiet {
            print!("Checking '{}'... ", set.name);
            use std::io::Write;
            std::io::stdout().flush()?;
        }

        // Use `restic snapshots --latest 1` as a quick check for repo accessibility
        let output = tokio::process::Command::new("restic")
            .arg("snapshots")
            .arg("--repo")
            .arg(&set.target)
            .arg("--password-file")
            .arg(&password_path)
            .arg("--latest")
            .arg("1")
            .arg("--json")
            .output()
            .await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    if !json && !quiet {
                        println!("\r✓ {}: Repository accessible", set.name);
                    }
                    results.push(serde_json::json!({ "set": set.name, "accessible": true }));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !json {
                        println!("\r✗ {}: Repository check failed", set.name);
                        eprintln!("  Error: {}", stderr.trim());
                        if stderr.contains("repository does not exist") {
                            eprintln!("  Hint: You might need to initialize the repository first.");
                            eprintln!("        Run `backutil init {}` to initialize it.", set.name);
                        }
                    }
                    results.push(serde_json::json!({ "set": set.name, "accessible": false, "error": stderr.trim() }));
                    failed = true;
                }
            }
            Err(e) => {
                if !json {
                    println!("\r✗ {}: Failed to execute restic", set.name);
                    eprintln!("  Error: {}", e);
                }
                results.push(serde_json::json!({ "set": set.name, "accessible": false, "error": e.to_string() }));
                failed = true;
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": if failed { "error" } else { "ok" },
                "results": results
            })
        );
    }

    if failed {
        std::process::exit(4);
    }

    Ok(())
}

async fn handle_purge(
    set_name: String,
    force: bool,
    json: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let config_res = backutil_lib::config::load_config();
    let mut target_path = None;

    if let Ok(config) = config_res {
        if let Some(set) = config.backup_sets.iter().find(|s| s.name == set_name) {
            if !force {
                if json || quiet {
                    anyhow::bail!("Purge requires --force when running in --json or --quiet mode");
                }
                println!("Backup set '{}' is still present in config.toml. Remove it first or use --force.", set_name);
            }
            target_path = Some(set.target.clone());
        }
    }

    // Try to get target path from daemon if not found in config
    if target_path.is_none() {
        if let Ok(mut stream) = UnixStream::connect(paths::socket_path()).await {
            let _ = send_request(&mut stream, Request::Status).await;
            let mut reader = BufReader::new(&mut stream);
            if let Ok(Response::Ok(Some(ResponseData::Status { sets }))) =
                receive_response(&mut reader).await
            {
                if let Some(set) = sets.iter().find(|s| s.name == set_name) {
                    target_path = Some(set.target.to_string_lossy().to_string());
                }
            }
        }
    }

    let target_path = target_path.ok_or_else(|| {
        anyhow!(
            "Could not determine target path for backup set '{}'. Is it in config.toml?",
            set_name
        )
    })?;

    if !force {
        if json || quiet {
            anyhow::bail!("Purge requires --force when running in --json or --quiet mode");
        }
        println!(
            "WARNING: This will permanently delete ALL backup data for '{}' at '{}' and can NOT be undone!",
            set_name, target_path
        );
        println!("Source files will NOT be affected.");
        print!("Are you sure you want to proceed? [y/N]: ");
        use std::io::Write;
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            println!("Purge cancelled.");
            return Ok(());
        }
    }

    if !quiet && !json {
        println!("Unmounting set '{}' if active...", set_name);
    }

    // 1. Unmount if mounted
    if let Ok(mut stream) = UnixStream::connect(paths::socket_path()).await {
        let mut reader = BufReader::new(&mut stream);
        let _ = send_request(
            reader.get_mut(),
            Request::Unmount {
                set_name: Some(set_name.clone()),
            },
        )
        .await;
        let _ = receive_response(&mut reader).await; // Ignore response details

        // 2. Reload daemon config to stop tracking it (in case it's still there)
        if !quiet && !json {
            println!("Refreshing daemon configuration...");
        }
        let _ = send_request(reader.get_mut(), Request::ReloadConfig).await;
        let _ = receive_response(&mut reader).await;
    }

    // 3. Delete repository
    if !quiet && !json {
        println!("Deleting Restic repository at '{}'...", target_path);
    }
    let path = std::path::Path::new(&target_path);
    if path.exists() {
        if path.is_dir() {
            std::fs::remove_dir_all(path).context("Failed to remove repository directory")?;
        } else {
            anyhow::bail!(
                "Target path '{}' exists but is not a directory. Refusing to delete.",
                target_path
            );
        }
    } else if !quiet && !json {
        println!("Repository directory does not exist, skipping.");
    }

    // 4. Delete mount point
    let mount_path = paths::mount_path(&set_name);
    if mount_path.exists() {
        if !quiet && !json {
            println!("Deleting mount point at {:?}...", mount_path);
        }
        // We try a few times because unmount might take a moment to propagate in the kernel
        let mut success = false;
        for _ in 0..5 {
            if std::fs::remove_dir_all(&mount_path).is_ok() {
                success = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        if !success && !quiet && !json {
            println!(
                "Warning: Could not remove mount point directory {:?}. It might still be busy.",
                mount_path
            );
        }
    }

    if json {
        println!(
            "{}",
            serde_json::json!({ "status": "purged", "set": set_name, "target": target_path })
        );
    } else if !quiet {
        println!("Successfully purged backup set '{}'.", set_name);
    }

    Ok(())
}

async fn handle_snapshots(
    set_name: String,
    limit: usize,
    json: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(
        reader.get_mut(),
        Request::Snapshots {
            set_name: set_name.clone(),
            limit: Some(limit),
        },
    )
    .await?;

    let response = receive_response(&mut reader).await?;
    match response {
        Response::Ok(Some(ResponseData::Snapshots { snapshots })) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else if !quiet {
                if snapshots.is_empty() {
                    println!("No snapshots found for set '{}'.", set_name);
                    return Ok(());
                }

                println!("{:<10} {:<20} {:<10} {:<30}", "ID", "DATE", "SIZE", "PATHS");
                println!("{}", "-".repeat(70));

                for s in snapshots {
                    let date = s.timestamp.format("%Y-%m-%d %H:%M").to_string();
                    let size = s
                        .total_bytes
                        .map(format_size)
                        .unwrap_or_else(|| "N/A".to_string());
                    let paths = s
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ");

                    println!("{:<10} {:<20} {:<10} {:<30}", s.short_id, date, size, paths);
                }
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error from daemon ({}): {}", code, message);
            if code == backutil_lib::ipc::error_codes::RESTIC_ERROR {
                std::process::exit(4);
            } else {
                std::process::exit(1);
            }
        }
        _ => {
            println!("Unexpected response from daemon.");
        }
    }

    Ok(())
}

async fn handle_reload(json: bool, quiet: bool) -> anyhow::Result<()> {
    let mut stream = connect_to_daemon().await?;
    let mut reader = BufReader::new(&mut stream);
    send_request(reader.get_mut(), Request::ReloadConfig).await?;

    let response = receive_response(&mut reader).await?;
    match response {
        Response::Ok(_) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "success", "message": "Configuration reload triggered" })
                );
            } else if !quiet {
                println!("Successfully triggered configuration reload.");
            }
        }
        Response::Error { code, message } => {
            eprintln!("Error triggering reload ({}): {}", code, message);
            std::process::exit(1);
        }
        _ => {
            println!("Unexpected response from daemon.");
        }
    }

    Ok(())
}

async fn handle_list(json: bool, quiet: bool) -> anyhow::Result<()> {
    let config = match backutil_lib::config::load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(2);
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else if !quiet {
        if config.backup_sets.is_empty() {
            println!("No backup sets configured.");
            return Ok(());
        }

        println!("{:<15} {:<30} {:<30}", "NAME", "SOURCE", "TARGET");
        println!("{}", "-".repeat(75));

        for set in &config.backup_sets {
            let source_str = if let Some(ref s) = set.source {
                s.clone()
            } else if let Some(ref ss) = set.sources {
                if ss.is_empty() {
                    "None".to_string()
                } else {
                    let first = &ss[0];
                    if ss.len() > 1 {
                        format!("{} (+{} more)", first, ss.len() - 1)
                    } else {
                        first.clone()
                    }
                }
            } else {
                "None".to_string()
            };

            println!("{:<15} {:<30} {:<30}", set.name, source_str, set.target);
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

async fn receive_response<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> anyhow::Result<Response> {
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
        "{:<15} {:<15} {:<10} {:<10} {:<20} {:<10}",
        "NAME", "STATE", "SNAPSHOTS", "SIZE", "LAST BACKUP", "MOUNTED"
    );
    println!("{}", "-".repeat(95));

    for set in sets {
        let state_str = match set.state {
            JobState::Idle => "Idle".to_string(),
            JobState::Debouncing { remaining_secs } => {
                format!("Debounce({}s)", remaining_secs)
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
                    format!("{} (fail)", time_str)
                }
            }
            None => "Never".to_string(),
        };

        let mounted_str = if set.is_mounted { "Yes" } else { "No" };

        let snapshots_str = set
            .snapshot_count
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());

        let size_str = set
            .total_bytes
            .map(format_size)
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<15} {:<15} {:<10} {:<10} {:<20} {:<10}",
            set.name, state_str, snapshots_str, size_str, last_backup_str, mounted_str
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
