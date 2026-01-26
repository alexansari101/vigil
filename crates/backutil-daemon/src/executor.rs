use anyhow::{anyhow, Context, Result};
use backutil_lib::config::BackupSet;
use backutil_lib::paths;
use backutil_lib::types::{BackupResult, SnapshotInfo};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};
use tracing::{debug, error, info};

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

    pub async fn snapshots(&self, target: &str) -> Result<Vec<SnapshotInfo>> {
        let password_file = paths::password_path();
        let (stdout, _) = self
            .run_restic(vec![
                "snapshots".to_string(),
                "--repo".to_string(),
                target.to_string(),
                "--password-file".to_string(),
                password_file.to_string_lossy().to_string(),
                "--json".to_string(),
            ])
            .await?;

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

    pub async fn prune(&self, set: &BackupSet) -> Result<()> {
        info!("Pruning repository for set: {}", set.name);
        let password_file = paths::password_path();

        let mut args = vec![
            "forget".to_string(),
            "--repo".to_string(),
            set.target.clone(),
            "--password-file".to_string(),
            password_file.to_string_lossy().to_string(),
            "--prune".to_string(),
        ];

        if let Some(ref r) = set.retention {
            if let Some(last) = r.keep_last {
                args.push("--keep-last".to_string());
                args.push(last.to_string());
            }
            if let Some(daily) = r.keep_daily {
                args.push("--keep-daily".to_string());
                args.push(daily.to_string());
            }
            if let Some(weekly) = r.keep_weekly {
                args.push("--keep-weekly".to_string());
                args.push(weekly.to_string());
            }
            if let Some(monthly) = r.keep_monthly {
                args.push("--keep-monthly".to_string());
                args.push(monthly.to_string());
            }
        }

        self.run_restic(args).await?;
        Ok(())
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

        let child = cmd.spawn().context("Failed to spawn restic mount")?;
        Ok(child)
    }
}
