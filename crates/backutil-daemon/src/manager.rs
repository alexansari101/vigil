use crate::executor::ResticExecutor;
use anyhow::Result;
use backutil_lib::config::{BackupSet, Config, RetentionPolicy};
use backutil_lib::ipc::{Response, ResponseData};
use backutil_lib::types::{BackupResult, JobState, SetStatus, SnapshotInfo};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// How long to wait for restic mount process to exit gracefully after fusermount3 -u
const MOUNT_GRACEFUL_EXIT_TIMEOUT_SECS: u64 = 2;

pub struct JobManager {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
    executor: Arc<ResticExecutor>,
    /// Global retention policy for fallback when per-set retention is not specified.
    global_retention: Mutex<Option<RetentionPolicy>>,
    /// Broadcast sender for async events (e.g. backup completion)
    event_tx: broadcast::Sender<Response>,
    /// Token to signal shutdown
    shutdown_token: CancellationToken,
}

struct Job {
    set: BackupSet,
    state: JobState,
    last_change: Option<Instant>,
    last_backup: Option<BackupResult>,
    is_mounted: bool,
    immediate_trigger: bool,
    mount_process: Option<tokio::process::Child>,
    snapshot_count: Option<usize>,
    total_bytes: Option<u64>,
}

impl JobManager {
    pub fn new(config: &Config, shutdown_token: CancellationToken) -> Self {
        let mut jobs = HashMap::new();
        for set in &config.backup_sets {
            jobs.insert(
                set.name.clone(),
                Job {
                    set: set.clone(),
                    state: JobState::Idle,
                    last_change: None,
                    last_backup: None,
                    is_mounted: false,
                    immediate_trigger: false,
                    mount_process: None,
                    snapshot_count: None,
                    total_bytes: None,
                },
            );
        }
        let (event_tx, _) = broadcast::channel(100);
        Self {
            jobs: Arc::new(Mutex::new(jobs)),
            executor: Arc::new(ResticExecutor::new()),
            global_retention: Mutex::new(config.global.retention.clone()),
            event_tx,
            shutdown_token,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Response> {
        self.event_tx.subscribe()
    }

    /// Queries restic for the latest snapshot of each backup set and populates `last_backup`.
    /// This should be called on daemon startup.
    pub async fn initialize_status(&self) {
        let mut jobs = self.jobs.lock().await;
        for (name, job) in jobs.iter_mut() {
            debug!("Initializing status for backup set '{}'", name);
            // We pass None for token here as this is startup and we generally want it to complete
            // unless shutdown happens *during* startup (less common, but could pass token if strict)
            match self.executor.snapshots(&job.set.target, None, None).await {
                Ok(snapshots) => {
                    job.snapshot_count = Some(snapshots.len());
                    if let Some(latest) = snapshots.first() {
                        debug!("Found latest snapshot for '{}': {}", name, latest.id);
                        job.last_backup = Some(BackupResult {
                            snapshot_id: latest.short_id.clone(),
                            timestamp: latest.timestamp,
                            added_bytes: 0,     // Not available from snapshots command
                            duration_secs: 0.0, // Not available from snapshots command
                            success: true,
                            error_message: None,
                        });
                    } else {
                        debug!("No snapshots found for '{}'", name);
                    }
                }
                Err(e) => {
                    warn!("Failed to query snapshots for '{}': {}", name, e);
                    // We don't fail initialization if one repository is unreachable
                }
            }

            // Calculate repo size
            match Self::calculate_dir_size(std::path::Path::new(&job.set.target)).await {
                Ok(size_opt) => job.total_bytes = size_opt,
                Err(e) => warn!("Failed to calculate repo size for '{}': {}", name, e),
            }
        }
    }

    pub async fn sync_config(&self, config: &Config) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let new_set_names: std::collections::HashSet<String> =
            config.backup_sets.iter().map(|s| s.name.clone()).collect();

        // 1. Identify and handle removed sets
        let removed_set_names: Vec<String> = jobs
            .keys()
            .filter(|name| !new_set_names.contains(*name))
            .cloned()
            .collect();

        for name in removed_set_names {
            info!("Backup set '{}' removed from config, cleaning up...", name);
            if let Some(mut job) = jobs.remove(&name) {
                // Unmount if mounted
                if let Err(e) = Self::perform_unmount(&name, &mut job).await {
                    error!("Failed to unmount removed set '{}': {}", name, e);
                }
            }
        }

        // 2. Add or update remaining sets
        for set in &config.backup_sets {
            if let Some(job) = jobs.get_mut(&set.name) {
                // Update existing job config
                debug!("Updating config for backup set '{}'", set.name);
                job.set = set.clone();
            } else {
                // Add new job
                info!("New backup set '{}' added to config", set.name);
                jobs.insert(
                    set.name.clone(),
                    Job {
                        set: set.clone(),
                        state: JobState::Idle,
                        last_change: None,
                        last_backup: None,
                        is_mounted: false,
                        immediate_trigger: false,
                        mount_process: None,
                        snapshot_count: None,
                        total_bytes: None,
                    },
                );
            }
        }

        // 3. Update global retention
        let mut global_retention = self.global_retention.lock().await;
        *global_retention = config.global.retention.clone();

        Ok(())
    }

    pub async fn handle_file_change(&self, set_name: &str) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get_mut(set_name) {
            let now = Instant::now();
            job.last_change = Some(now);

            match job.state {
                JobState::Idle | JobState::Error => {
                    let debounce_secs = job.set.debounce_seconds.unwrap_or(60);
                    job.state = JobState::Debouncing {
                        remaining_secs: debounce_secs,
                    };
                    info!(
                        "Set {} entered Debouncing state ({}s)",
                        set_name, debounce_secs
                    );

                    let jobs_clone = self.jobs.clone();
                    let executor_clone = self.executor.clone();
                    let set_name_owned = set_name.to_string();
                    let event_tx = self.event_tx.clone();
                    let shutdown_token = self.shutdown_token.clone();

                    tokio::spawn(async move {
                        Self::job_worker(
                            jobs_clone,
                            executor_clone,
                            set_name_owned,
                            event_tx,
                            shutdown_token,
                        )
                        .await;
                    });
                }
                JobState::Debouncing { .. } => {
                    debug!("Set {} is already debouncing, timer reset", set_name);
                    // Timer will be automatically reset because we updated last_change
                }
                JobState::Running => {
                    debug!(
                        "Set {} is currently running, will re-debounce after completion",
                        set_name
                    );
                    // When the current backup finishes, it will check last_change
                }
            }
            Ok(())
        } else {
            anyhow::bail!("Unknown backup set: {}", set_name)
        }
    }

    pub async fn trigger_backup(&self, set_name: &str) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get_mut(set_name) {
            match job.state {
                JobState::Running => {
                    anyhow::bail!("Backup for set {} is already running", set_name);
                }
                JobState::Debouncing { .. } => {
                    job.immediate_trigger = true;
                    info!(
                        "Immediate backup triggered for set {} (was debouncing)",
                        set_name
                    );
                }
                JobState::Idle | JobState::Error => {
                    job.state = JobState::Running; // Set to running immediately to prevent debounce start
                    info!("Immediate backup triggered for set {}", set_name);

                    let jobs_clone = self.jobs.clone();
                    let executor_clone = self.executor.clone();
                    let set_name_owned = set_name.to_string();
                    let event_tx = self.event_tx.clone();
                    let shutdown_token = self.shutdown_token.clone();

                    tokio::spawn(async move {
                        Self::job_worker(
                            jobs_clone,
                            executor_clone,
                            set_name_owned,
                            event_tx,
                            shutdown_token,
                        )
                        .await;
                    });
                }
            }
            Ok(())
        } else {
            anyhow::bail!("Unknown backup set: {}", set_name)
        }
    }

    async fn job_worker(
        jobs: Arc<Mutex<HashMap<String, Job>>>,
        executor: Arc<ResticExecutor>,
        set_name: String,
        event_tx: broadcast::Sender<Response>,
        shutdown_token: CancellationToken,
    ) {
        loop {
            // Check for shutdown at start of loop
            if shutdown_token.is_cancelled() {
                info!("Detailed shutdown check: stopping worker for {}", set_name);
                break;
            }

            // Debouncing phase: wait for timer to stabilize
            let debounce_duration;
            let mut start_time;
            {
                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if matches!(job.state, JobState::Running) {
                        // Already in running state (immediate trigger)
                        job.immediate_trigger = false;
                        debounce_duration = Duration::ZERO; // Skip loop effectively
                        start_time = Instant::now();
                    } else {
                        debounce_duration =
                            Duration::from_secs(job.set.debounce_seconds.unwrap_or(60));
                        start_time = job.last_change.unwrap_or_else(Instant::now);
                    }
                } else {
                    return; // Job removed
                }
            }

            // Poll every 500ms to update remaining time and check for expiration
            loop {
                // Check shutdown
                if shutdown_token.is_cancelled() {
                    return;
                }

                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if matches!(job.state, JobState::Running) {
                        break;
                    }

                    if job.immediate_trigger {
                        job.immediate_trigger = false;
                        job.state = JobState::Running;
                        info!(
                            "Immediate trigger detected for set {}, skipping debounce",
                            set_name
                        );
                        break;
                    }

                    if let Some(last_change) = job.last_change {
                        // Check if the timer was reset (new file change)
                        if last_change > start_time {
                            info!("Timer reset for set {}, restarting debounce", set_name);
                            start_time = last_change;
                        }

                        let elapsed = start_time.elapsed();
                        if elapsed >= debounce_duration {
                            info!(
                                "Debounce timer expired for set {}, transitioning to Running",
                                set_name
                            );
                            job.state = JobState::Running;
                            break;
                        } else {
                            let remaining = debounce_duration.saturating_sub(elapsed).as_secs();
                            job.state = JobState::Debouncing {
                                remaining_secs: remaining,
                            };
                        }
                    } else {
                        // This shouldn't really happen if we are debouncing
                        job.state = JobState::Running;
                        break;
                    }
                } else {
                    return; // Job removed
                }
                drop(jobs_lock);

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                    _ = shutdown_token.cancelled() => {
                        return;
                    }
                }
            }

            // Running phase
            let backup_start_time = Instant::now();
            debug!("Starting backup execution for set {}", set_name);

            let result = {
                let jobs_lock = jobs.lock().await;
                let Some(job) = jobs_lock.get(&set_name) else {
                    // Job was removed during execution
                    return;
                };
                // Pass shutdown token to executor so it can kill the process if shutdown occurs
                executor
                    .backup(&job.set, Some(shutdown_token.clone()))
                    .await
            };

            match result {
                Ok(backup_result) => {
                    info!(
                        "Backup completed for set {} in {:.2}s. Success: {}",
                        set_name,
                        backup_start_time.elapsed().as_secs_f64(),
                        backup_result.success
                    );

                    let mut metrics_target = None;
                    {
                        let mut jobs_lock = jobs.lock().await;
                        if let Some(job) = jobs_lock.get_mut(&set_name) {
                            job.last_backup = Some(backup_result.clone());
                            if !backup_result.success {
                                job.state = JobState::Error;
                                let err_msg = backup_result
                                    .error_message
                                    .clone()
                                    .unwrap_or_else(|| "Unknown error".to_string());
                                error!("Backup failed for set {}: {}", set_name, err_msg);

                                // Only notify if not cancelled due to shutdown
                                if !shutdown_token.is_cancelled() {
                                    let _ = notify_rust::Notification::new()
                                        .summary("Backup Failed")
                                        .body(&format!(
                                            "Backup for set '{}' failed: {}",
                                            set_name, err_msg
                                        ))
                                        .icon("dialog-error")
                                        .show();
                                }

                                // Broadcast failure event
                                let _ =
                                    event_tx.send(Response::Ok(Some(ResponseData::BackupFailed {
                                        set_name: set_name.clone(),
                                        error: err_msg,
                                    })));
                                break;
                            }

                            // Check if new changes occurred during backup
                            if let Some(last_change) = job.last_change {
                                if last_change > backup_start_time {
                                    info!(
                                    "New changes detected for set {} during backup, re-debouncing",
                                    set_name
                                );
                                    let debounce_secs = job.set.debounce_seconds.unwrap_or(60);
                                    job.state = JobState::Debouncing {
                                        remaining_secs: debounce_secs,
                                    };
                                    continue;
                                }
                            }
                            job.state = JobState::Idle;

                            // Broadcast completion event
                            let _ =
                                event_tx.send(Response::Ok(Some(ResponseData::BackupComplete {
                                    set_name: set_name.clone(),
                                    snapshot_id: backup_result.snapshot_id.clone(),
                                    added_bytes: backup_result.added_bytes,
                                    duration_secs: backup_result.duration_secs,
                                })));

                            metrics_target = Some(job.set.target.clone());
                        }
                    }

                    if let Some(target) = metrics_target {
                        let jobs = jobs.clone();
                        let executor = executor.clone();
                        let set_name = set_name.clone();
                        let token = shutdown_token.clone();

                        tokio::spawn(async move {
                            // Metrics calculation is low priority, can just stop if shutdown
                            if token.is_cancelled() {
                                return;
                            }

                            let snapshots_count_res =
                                executor.snapshots(&target, None, Some(token.clone())).await;
                            let size_res =
                                Self::calculate_dir_size(std::path::Path::new(&target)).await;

                            if let Some(job) = jobs.lock().await.get_mut(&set_name) {
                                if let Ok(snapshots) = snapshots_count_res {
                                    job.snapshot_count = Some(snapshots.len());
                                }
                                if let Ok(size_opt) = size_res {
                                    job.total_bytes = size_opt;
                                }
                            }
                        });
                        break;
                    }
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    error!("Backup job error for set {}: {}", set_name, err_msg);

                    {
                        let mut jobs_lock = jobs.lock().await;
                        if let Some(job) = jobs_lock.get_mut(&set_name) {
                            job.state = JobState::Error;
                        }
                    }

                    if !shutdown_token.is_cancelled() {
                        let _ = notify_rust::Notification::new()
                            .summary("Backup Failed")
                            .body(&format!(
                                "Internal error backing up set '{}': {}",
                                set_name, err_msg
                            ))
                            .icon("dialog-error")
                            .show();
                    }

                    // Broadcast failure event
                    let _ = event_tx.send(Response::Ok(Some(ResponseData::BackupFailed {
                        set_name: set_name.clone(),
                        error: err_msg,
                    })));

                    break;
                }
            }
        }
    }

    /// Get status for all backup sets.
    ///
    /// **Note**: This function has side effects - it monitors mount processes and updates
    /// `is_mounted` state if a mount process has died unexpectedly.
    pub async fn get_status(&self) -> Vec<SetStatus> {
        let mut jobs = self.jobs.lock().await;

        let mut statuses = Vec::new();
        for job in jobs.values_mut() {
            // Monitor mount process
            if job.is_mounted {
                if let Some(ref mut child) = job.mount_process {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            warn!(
                                "Mount process for set {} exited unexpectedly with status: {}",
                                job.set.name, status
                            );
                            job.is_mounted = false;
                            job.mount_process = None;
                        }
                        Ok(None) => {
                            // Still running
                        }
                        Err(e) => {
                            error!(
                                "Error checking mount process for set {}: {}",
                                job.set.name, e
                            );
                        }
                    }
                } else {
                    // Should not happen if is_mounted is true
                    job.is_mounted = false;
                }
            }

            statuses.push(SetStatus {
                name: job.set.name.clone(),
                state: job.state.clone(),
                last_backup: job.last_backup.clone(),
                source_paths: {
                    let mut paths = Vec::new();
                    if let Some(ref s) = job.set.source {
                        paths.push(s.into());
                    }
                    if let Some(ref ss) = job.set.sources {
                        for s in ss {
                            paths.push(s.into());
                        }
                    }
                    paths
                },
                target: job.set.target.clone().into(),
                is_mounted: job.is_mounted,
                snapshot_count: job.snapshot_count,
                total_bytes: job.total_bytes,
            });
        }
        statuses
    }

    pub async fn get_snapshots(
        &self,
        set_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<SnapshotInfo>> {
        let jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get(set_name) {
            // Snapshots query typically redundant to be cancelled by shutdown?
            // We can pass token if we want strict shutdown, but for now user-initiated reads are probably fine to finish or fail on pipe close.
            // Let's pass the token to be consistent.
            self.executor
                .snapshots(&job.set.target, limit, Some(self.shutdown_token.clone()))
                .await
        } else {
            anyhow::bail!("Unknown backup set: {}", set_name)
        }
    }

    pub async fn mount(&self, set_name: &str, snapshot_id: Option<String>) -> Result<PathBuf> {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get_mut(set_name) {
            if job.is_mounted {
                return Ok(backutil_lib::paths::mount_path(set_name));
            }

            let mount_path = backutil_lib::paths::mount_path(set_name);
            if !mount_path.exists() {
                std::fs::create_dir_all(&mount_path)?;
                // Set restrictive permissions for sensitive backup data
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&mount_path, std::fs::Permissions::from_mode(0o700))?;
                }
            }

            info!("Mounting set {} at {:?}", set_name, mount_path);
            let child = self
                .executor
                .mount(&job.set.target, snapshot_id.as_deref(), &mount_path)
                .await?;

            job.mount_process = Some(child);
            job.is_mounted = true;

            Ok(mount_path)
        } else {
            anyhow::bail!("Unknown backup set: {}", set_name)
        }
    }

    pub async fn unmount(&self, set_name: Option<String>) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(name) = set_name {
            if let Some(job) = jobs.get_mut(&name) {
                Self::perform_unmount(&name, job).await?;
                Ok(())
            } else {
                anyhow::bail!("Unknown backup set: {}", name)
            }
        } else {
            info!("Unmounting all sets");
            for (name, job) in jobs.iter_mut() {
                if let Err(e) = Self::perform_unmount(name, job).await {
                    error!("Failed to unmount set {}: {}", name, e);
                }
            }
            Ok(())
        }
    }

    pub async fn prune(&self, set_name: Option<String>) -> Result<backutil_lib::ipc::ResponseData> {
        if let Some(name) = set_name {
            let effective_set = {
                let jobs = self.jobs.lock().await;
                if let Some(job) = jobs.get(&name) {
                    self.with_effective_retention(&job.set).await
                } else {
                    anyhow::bail!("Unknown backup set: {}", name)
                }
            };

            info!("Pruning set {}", name);
            // Can pass shutdown token here to allow cancellation
            let reclaimed = self
                .executor
                .prune(&effective_set, Some(self.shutdown_token.clone()))
                .await?;
            info!("Pruned set {}: {} bytes reclaimed", name, reclaimed);

            // Refresh metrics after prune
            let target = effective_set.target.clone();
            let jobs = self.jobs.clone();
            let executor = self.executor.clone();
            let refresh_name = name.clone();
            let token = self.shutdown_token.clone();

            tokio::spawn(async move {
                if token.is_cancelled() {
                    return;
                }
                let snapshots_count_res =
                    executor.snapshots(&target, None, Some(token.clone())).await;
                let size_res = JobManager::calculate_dir_size(std::path::Path::new(&target)).await;

                if let Some(job) = jobs.lock().await.get_mut(&refresh_name) {
                    if let Ok(snapshots) = snapshots_count_res {
                        job.snapshot_count = Some(snapshots.len());
                    }
                    if let Ok(size_opt) = size_res {
                        job.total_bytes = size_opt;
                    }
                }
            });

            Ok(backutil_lib::ipc::ResponseData::PruneResult {
                set_name: name,
                reclaimed_bytes: reclaimed,
            })
        } else {
            // Collect effective sets under the lock, then drop it
            let sets_to_prune: Vec<(String, BackupSet)> = {
                let jobs = self.jobs.lock().await;
                let mut sets = Vec::new();
                for (name, job) in jobs.iter() {
                    let effective_set = self.with_effective_retention(&job.set).await;
                    sets.push((name.clone(), effective_set));
                }
                sets
            };

            info!("Pruning all sets");
            let mut succeeded = Vec::new();
            let mut failed = Vec::new();
            let mut targets_to_refresh = Vec::new();

            for (name, effective_set) in &sets_to_prune {
                // Check shutdown before starting next prune
                if self.shutdown_token.is_cancelled() {
                    break;
                }
                match self
                    .executor
                    .prune(effective_set, Some(self.shutdown_token.clone()))
                    .await
                {
                    Ok(reclaimed) => {
                        info!("Pruned set {}: {} bytes reclaimed", name, reclaimed);
                        succeeded.push((name.clone(), reclaimed));
                        targets_to_refresh.push((name.clone(), effective_set.target.clone()));
                    }
                    Err(e) => {
                        error!("Failed to prune set {}: {}", name, e);
                        failed.push((name.clone(), e.to_string()));
                    }
                }
            }

            // Refresh metrics for successfully pruned sets
            for (name, target) in targets_to_refresh {
                let jobs = self.jobs.clone();
                let executor = self.executor.clone();
                let token = self.shutdown_token.clone();
                tokio::spawn(async move {
                    if token.is_cancelled() {
                        return;
                    }
                    let snapshots_count_res =
                        executor.snapshots(&target, None, Some(token.clone())).await;
                    let size_res =
                        JobManager::calculate_dir_size(std::path::Path::new(&target)).await;

                    if let Some(job) = jobs.lock().await.get_mut(&name) {
                        if let Ok(snapshots) = snapshots_count_res {
                            job.snapshot_count = Some(snapshots.len());
                        }
                        if let Ok(size_opt) = size_res {
                            job.total_bytes = size_opt;
                        }
                    }
                });
            }

            Ok(backutil_lib::ipc::ResponseData::PrunesTriggered { succeeded, failed })
        }
    }

    /// Creates a copy of the BackupSet with effective retention policy.
    /// Falls back to global retention if per-set retention is not specified.
    async fn with_effective_retention(&self, set: &BackupSet) -> BackupSet {
        let mut effective = set.clone();
        if effective.retention.is_none() {
            effective.retention = self.global_retention.lock().await.clone();
        }
        effective
    }

    async fn perform_unmount(name: &str, job: &mut Job) -> Result<()> {
        if !job.is_mounted {
            return Ok(());
        }

        // Warn if unmounting during an active backup
        if matches!(job.state, JobState::Running) {
            warn!(
                "Unmounting set {} while backup is running - this may cause the backup to fail",
                name
            );
        }

        info!("Unmounting set {}", name);
        let mount_path = backutil_lib::paths::mount_path(name);

        // 1. Try fusermount3 -u
        let child = tokio::process::Command::new("fusermount3")
            .arg("-u")
            .arg(&mount_path)
            .spawn();

        let success = match child {
            Ok(mut c) => {
                let status = c.wait().await?;
                status.success()
            }
            Err(_) => false, // fusermount3 not found or failed to spawn
        };

        if !success {
            debug!(
                "fusermount3 failed or not found, killing restic process for {}",
                name
            );
            if let Some(mut child) = job.mount_process.take() {
                let _ = child.kill().await;
            }
        } else {
            // Even if fusermount3 succeeded, we should clean up the restic process
            if let Some(mut child) = job.mount_process.take() {
                // Restic should exit on its own when unmounted, but we'll wait a bit then kill if needed
                match tokio::time::timeout(
                    Duration::from_secs(MOUNT_GRACEFUL_EXIT_TIMEOUT_SECS),
                    child.wait(),
                )
                .await
                {
                    Ok(_) => debug!("Restic mount process for {} exited cleanly", name),
                    Err(_) => {
                        debug!("Restic mount process for {} did not exit, killing", name);
                        let _ = child.kill().await;
                    }
                }
            }
        }

        job.is_mounted = false;
        job.mount_process = None;

        Ok(())
    }

    async fn calculate_dir_size(path: &std::path::Path) -> Result<Option<u64>> {
        if !path.exists() {
            return Ok(None);
        }
        let mut total_size = 0;
        let mut entries = match tokio::fs::read_dir(path).await {
            Ok(e) => e,
            Err(_) => return Ok(None), // treat inaccessible dirs as unknown size
        };

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                if let Some(size) = Box::pin(Self::calculate_dir_size(&entry.path())).await? {
                    total_size += size;
                }
            } else {
                total_size += metadata.len();
            }
        }
        Ok(Some(total_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backutil_lib::config::GlobalConfig;
    use backutil_lib::paths;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;

    /// Tests the debounce state machine with real restic integration.
    ///
    /// **NOTE:** This test modifies XDG environment variables and must be run single-threaded:
    /// ```bash
    /// cargo test -p backutil-daemon --lib -- --ignored --test-threads=1
    /// ```
    #[tokio::test]
    #[ignore]
    async fn test_debounce_logic() -> Result<()> {
        let _ = tracing_subscriber::fmt::try_init();

        // Setup: Isolated temp environment
        let tmp = tempdir()?;
        let source_path = tmp.path().join("source");
        let repo_path = tmp.path().join("repo");
        fs::create_dir(&source_path)?;
        fs::write(source_path.join("test.txt"), "test data")?;

        // Setup: Isolated config/data dirs via env vars
        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        fs::create_dir_all(&config_home)?;
        fs::create_dir_all(&data_home)?;
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        std::env::set_var("XDG_DATA_HOME", &data_home);

        // Setup: Create password file
        let pw_file = paths::password_path();
        fs::create_dir_all(pw_file.parent().unwrap())?;
        fs::write(&pw_file, "testpassword")?;
        fs::set_permissions(&pw_file, fs::Permissions::from_mode(0o600))?;

        // Setup: Initialize restic repository
        let executor = crate::executor::ResticExecutor::new();
        executor.init(repo_path.to_str().unwrap()).await?;

        let config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![BackupSet {
                name: "test".to_string(),
                source: Some(source_path.to_string_lossy().to_string()),
                sources: None,
                target: repo_path.to_string_lossy().to_string(),
                exclude: None,
                debounce_seconds: Some(1), // 1 second for faster test
                retention: None,
            }],
        };

        let manager = JobManager::new(&config, CancellationToken::new());

        // Helper to get state for "test" set
        let get_test_state = || async {
            manager
                .get_status()
                .await
                .into_iter()
                .find(|s| s.name == "test")
                .map(|s| s.state)
        };

        // Initial state should be Idle
        assert_eq!(get_test_state().await.unwrap(), JobState::Idle);

        // Trigger a file change
        manager.handle_file_change("test").await?;

        // Should enter Debouncing state
        tokio::time::sleep(Duration::from_millis(100)).await;
        let state = get_test_state().await.unwrap();
        assert!(
            matches!(state, JobState::Debouncing { .. }),
            "Expected Debouncing, got {:?}",
            state
        );

        // Wait for debounce to complete and backup to finish
        // (1s debounce + real backup which is fast for small files)
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let state = get_test_state().await.unwrap();
        assert_eq!(
            state,
            JobState::Idle,
            "Expected Idle after backup completes"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_manual_trigger() -> Result<()> {
        let _ = tracing_subscriber::fmt::try_init();

        let tmp = tempdir()?;
        let source_path = tmp.path().join("source");
        let repo_path = tmp.path().join("repo");
        fs::create_dir(&source_path)?;
        fs::write(source_path.join("test.txt"), "test data")?;

        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        fs::create_dir_all(&config_home)?;
        fs::create_dir_all(&data_home)?;
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        std::env::set_var("XDG_DATA_HOME", &data_home);

        let pw_file = paths::password_path();
        fs::create_dir_all(pw_file.parent().unwrap())?;
        fs::write(&pw_file, "testpassword")?;
        fs::set_permissions(&pw_file, fs::Permissions::from_mode(0o600))?;

        let executor = crate::executor::ResticExecutor::new();
        executor.init(repo_path.to_str().unwrap()).await?;

        let config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![BackupSet {
                name: "test".to_string(),
                source: Some(source_path.to_string_lossy().to_string()),
                sources: None,
                target: repo_path.to_string_lossy().to_string(),
                exclude: None,
                debounce_seconds: Some(60), // Long debounce to verify skip
                retention: None,
            }],
        };

        let manager = JobManager::new(&config, CancellationToken::new());

        let get_test_state = || async {
            manager
                .get_status()
                .await
                .into_iter()
                .find(|s| s.name == "test")
                .map(|s| s.state)
        };

        // 1. Test trigger from Idle
        manager.trigger_backup("test").await?;

        // Should enter Running immediately
        tokio::time::sleep(Duration::from_millis(100)).await;
        let state = get_test_state().await.unwrap();
        assert_eq!(state, JobState::Running);

        // Wait for completion
        tokio::time::sleep(Duration::from_millis(2000)).await;
        assert_eq!(get_test_state().await.unwrap(), JobState::Idle);

        // 2. Test trigger from Debouncing
        manager.handle_file_change("test").await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let state = get_test_state().await.unwrap();
        assert!(matches!(state, JobState::Debouncing { .. }));

        manager.trigger_backup("test").await?;

        // Should transition to Running soon (after poll)
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let state = get_test_state().await.unwrap();
        // It might be Running or already Idle if the backup was fast
        assert!(matches!(state, JobState::Running | JobState::Idle));

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_initialize_status() -> Result<()> {
        let _ = tracing_subscriber::fmt::try_init();

        let tmp = tempdir()?;
        let source_path = tmp.path().join("source");
        let repo_path = tmp.path().join("repo");
        fs::create_dir(&source_path)?;
        fs::write(source_path.join("test.txt"), "test data")?;

        let config_home = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        fs::create_dir_all(&config_home)?;
        fs::create_dir_all(&data_home)?;
        std::env::set_var("XDG_CONFIG_HOME", &config_home);
        std::env::set_var("XDG_DATA_HOME", &data_home);

        let pw_file = paths::password_path();
        fs::create_dir_all(pw_file.parent().unwrap())?;
        fs::write(&pw_file, "testpassword")?;
        fs::set_permissions(&pw_file, fs::Permissions::from_mode(0o600))?;

        let executor = crate::executor::ResticExecutor::new();
        executor.init(repo_path.to_str().unwrap()).await?;

        let config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![BackupSet {
                name: "test".to_string(),
                source: Some(source_path.to_string_lossy().to_string()),
                sources: None,
                target: repo_path.to_string_lossy().to_string(),
                exclude: None,
                debounce_seconds: Some(1),
                retention: None,
            }],
        };

        // 1. Create a backup first
        let manager = JobManager::new(&config, CancellationToken::new());
        manager.trigger_backup("test").await?;
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let status = manager.get_status().await;
        let original_snapshot_id = status[0].last_backup.as_ref().unwrap().snapshot_id.clone();
        assert!(!original_snapshot_id.is_empty());

        // 2. Create a new manager (simulating daemon restart)
        let manager2 = JobManager::new(&config, CancellationToken::new());
        // Initially last_backup should be None
        assert!(manager2.get_status().await[0].last_backup.is_none());

        // 3. Initialize status
        manager2.initialize_status().await;

        // 4. Verify last_backup is now populated with the same snapshot ID
        let status2 = manager2.get_status().await;
        assert_eq!(
            status2[0].last_backup.as_ref().unwrap().snapshot_id,
            original_snapshot_id
        );
        assert!(status2[0].last_backup.as_ref().unwrap().success);

        Ok(())
    }

    #[tokio::test]
    async fn test_calculate_dir_size() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path();

        // Create some files
        fs::write(path.join("file1.txt"), "hello")?; // 5 bytes
        fs::write(path.join("file2.txt"), "world")?; // 5 bytes
        fs::create_dir(path.join("subdir"))?;
        fs::write(path.join("subdir/file3.txt"), "test")?; // 4 bytes

        let size = JobManager::calculate_dir_size(path).await?;
        assert_eq!(size, Some(14));

        // Test non-existent path
        let non_existent = path.join("does_not_exist");
        let size = JobManager::calculate_dir_size(&non_existent).await?;
        assert_eq!(size, None);

        Ok(())
    }
}
