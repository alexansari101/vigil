use anyhow::{anyhow, Context, Result};
use backutil_lib::config::BackupSet;
use backutil_lib::paths;
use backutil_lib::types::{BackupResult, SnapshotInfo};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};
use tracing::{debug, error, info};

/// How long to wait after spawning restic mount to check for immediate failures
/// (e.g., invalid snapshot ID, mount point busy, missing fusermount3)
const MOUNT_STARTUP_CHECK_MS: u64 = 200;

#[derive(Default)]
pub struct ResticExecutor;

#[derive(Debug, Deserialize)]
struct ResticSummary {
    // message_type is "summary"
    data_added: u64,
    total_duration: f64,
    snapshot_id: String,
}

#[derive(Debug, Deserialize)]
struct ResticSnapshot {
    id: String,
    short_id: String,
    time: chrono::DateTime<chrono::Utc>,
    paths: Vec<PathBuf>,
    tags: Option<Vec<String>>,
}

impl ResticExecutor {
    pub fn new() -> Self {
        Self
    }

    async fn run_restic(&self, args: Vec<String>) -> Result<(String, String)> {
        let mut cmd = Command::new("restic");
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running restic command: restic {}", args.join(" "));
        let output = cmd.output().await.context("Failed to execute restic")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            // Restic backup can return non-zero (3) for some warnings but still produce a snapshot
            if args.contains(&"backup".to_string()) && !stdout.is_empty() {
                debug!("Restic backup returned non-zero ({}) but produced output, checking for summary", output.status);
            } else {
                error!("Restic failed: {}", stderr);
                return Err(anyhow!("Restic error: {}", stderr));
            }
        }

        Ok((stdout, stderr))
    }

    pub async fn init(&self, target: &str) -> Result<()> {
        info!("Initializing restic repository at {}", target);
        let password_file = paths::password_path();
        self.run_restic(vec![
            "init".to_string(),
            "--repo".to_string(),
            target.to_string(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
        ])
        .await?;
        Ok(())
    }

    pub async fn backup(&self, set: &BackupSet) -> Result<BackupResult> {
        info!("Starting backup for set: {}", set.name);
        let password_file = paths::password_path();

        let mut args = vec![
            "backup".to_string(),
            "--repo".to_string(),
            set.target.clone(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
            "--json".to_string(),
        ];

        if let Some(ref excludes) = set.exclude {
            for exclude in excludes {
                args.push("--exclude".to_string());
                args.push(exclude.clone());
            }
        }

        if let Some(ref source) = set.source {
            args.push(source.clone());
        }
        if let Some(ref multi_sources) = set.sources {
            for source in multi_sources {
                args.push(source.clone());
            }
        }

        let (stdout, _) = match self.run_restic(args).await {
            Ok(res) => res,
            Err(e) => {
                return Ok(BackupResult {
                    snapshot_id: String::new(),
                    timestamp: Utc::now(),
                    added_bytes: 0,
                    duration_secs: 0.0,
                    success: false,
                    error_message: Some(e.to_string()),
                });
            }
        };

        // Restic outputs multiple JSON objects. We need to find the "summary" one.
        for line in stdout.lines().rev() {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(line) {
                if map.get("message_type").and_then(|v| v.as_str()) == Some("summary") {
                    let summary: ResticSummary = serde_json::from_value(Value::Object(map.clone()))
                        .context("Failed to parse restic summary JSON")?;

                    return Ok(BackupResult {
                        snapshot_id: summary.snapshot_id,
                        timestamp: Utc::now(),
                        added_bytes: summary.data_added,
                        duration_secs: summary.total_duration,
                        success: true,
                        error_message: None,
                    });
                }
            }
        }

        Ok(BackupResult {
            snapshot_id: String::new(),
            timestamp: Utc::now(),
            added_bytes: 0,
            duration_secs: 0.0,
            success: false,
            error_message: Some("Could not find summary in restic output".to_string()),
        })
    }

    pub async fn snapshots(&self, target: &str, limit: Option<usize>) -> Result<Vec<SnapshotInfo>> {
        let password_file = paths::password_path();
        let mut args = vec![
            "snapshots".to_string(),
            "--repo".to_string(),
            target.to_string(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
            "--json".to_string(),
        ];

        if let Some(n) = limit {
            args.push("--last".to_string());
            args.push(n.to_string());
        }

        let (stdout, _) = self.run_restic(args).await?;

        let snapshots: Vec<ResticSnapshot> =
            serde_json::from_str(&stdout).context("Failed to parse restic snapshots JSON")?;

        Ok(snapshots
            .into_iter()
            .map(|s| SnapshotInfo {
                id: s.id,
                short_id: s.short_id,
                timestamp: s.time,
                paths: s.paths,
                tags: s.tags.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn prune(&self, set: &BackupSet) -> Result<u64> {
        info!("Pruning repository for set: {}", set.name);
        let password_file = paths::password_path();

        // SAFETY: Require at least one retention policy to prevent deleting all snapshots.
        // Running `restic forget --prune` without any --keep-* flags deletes everything.
        let retention = set.retention.as_ref().ok_or_else(|| {
            anyhow!("Cannot prune set '{}': no retention policy specified. This would delete all snapshots.", set.name)
        })?;

        let has_any_policy = retention.keep_last.is_some()
            || retention.keep_daily.is_some()
            || retention.keep_weekly.is_some()
            || retention.keep_monthly.is_some();

        if !has_any_policy {
            return Err(anyhow!(
                "Cannot prune set '{}': retention policy has no keep rules. This would delete all snapshots.",
                set.name
            ));
        }

        let mut args = vec![
            "forget".to_string(),
            "--repo".to_string(),
            set.target.clone(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
            "--prune".to_string(),
        ];

        if let Some(last) = retention.keep_last {
            args.push("--keep-last".to_string());
            args.push(last.to_string());
        }
        if let Some(daily) = retention.keep_daily {
            args.push("--keep-daily".to_string());
            args.push(daily.to_string());
        }
        if let Some(weekly) = retention.keep_weekly {
            args.push("--keep-weekly".to_string());
            args.push(weekly.to_string());
        }
        if let Some(monthly) = retention.keep_monthly {
            args.push("--keep-monthly".to_string());
            args.push(monthly.to_string());
        }

        let (stdout, _) = self.run_restic(args).await?;

        // Parse reclaimed bytes from text output.
        // Example: "total bytes reclaimed: 1.23 MiB" or "reclaimed 123 bytes"
        // Since restic output can vary, we'll look for "reclaimed" and try to parse the number.
        // A more robust way is to look for "total bytes reclaimed: "
        let reclaimed = parse_reclaimed_bytes(&stdout);
        Ok(reclaimed)
    }

    pub async fn mount(
        &self,
        target: &str,
        snapshot_id: Option<&str>,
        mountpoint: &Path,
    ) -> Result<Child> {
        info!("Mounting repository at {:?}", mountpoint);
        let password_file = paths::password_path();

        let mut args = vec![
            "mount".to_string(),
            "--repo".to_string(),
            target.to_string(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
        ];

        if let Some(id) = snapshot_id {
            args.push("--snapshot".to_string());
            args.push(id.to_string());
        }

        args.push(mountpoint.to_string_lossy().to_string());

        let mut cmd = Command::new("restic");
        cmd.args(&args).stdout(Stdio::null()).stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn restic mount")?;

        // Give it a moment to see if it fails immediately (e.g. bad snapshot ID or mount point busy)
        tokio::time::sleep(Duration::from_millis(MOUNT_STARTUP_CHECK_MS)).await;
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                let mut stderr = String::new();
                if let Some(mut reader) = child.stderr.take() {
                    use tokio::io::AsyncReadExt;
                    let _ = reader.read_to_string(&mut stderr).await;
                }
                anyhow::bail!("Restic mount failed: {}", stderr);
            }
            _ => Ok(child),
        }
    }
}

fn parse_reclaimed_bytes(stdout: &str) -> u64 {
    for line in stdout.lines() {
        if line.contains("total bytes reclaimed:") {
            if let Some(val_str) = line.split(':').nth(1) {
                return parse_restic_size(val_str.trim());
            }
        }
    }
    0
}

fn parse_restic_size(s: &str) -> u64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return 0;
    }

    let val: f64 = parts[0].parse().unwrap_or(0.0);
    if parts.len() < 2 {
        return val as u64;
    }

    let unit = parts[1].to_lowercase();
    let multiplier = match unit.as_str() {
        "kib" | "k" => 1024.0,
        "mib" | "m" => 1024.0 * 1024.0,
        "gib" | "g" => 1024.0 * 1024.0 * 1024.0,
        "tib" | "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };

    // Special case for bytes
    if unit == "b" || unit == "bytes" {
        return val as u64;
    }

    (val * multiplier) as u64
}
