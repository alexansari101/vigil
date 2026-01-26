use anyhow::Result;
use backutil_lib::config::{BackupSet, Config};
use backutil_lib::types::{JobState, SetStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tracing::{debug, info};

pub struct JobManager {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
}

struct Job {
    set: BackupSet,
    state: JobState,
    last_change: Option<Instant>,
}

impl JobManager {
    pub fn new(config: &Config) -> Self {
        let mut jobs = HashMap::new();
        for set in &config.backup_sets {
            jobs.insert(
                set.name.clone(),
                Job {
                    set: set.clone(),
                    state: JobState::Idle,
                    last_change: None,
                },
            );
        }
        Self {
            jobs: Arc::new(Mutex::new(jobs)),
        }
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
                    let set_name_owned = set_name.to_string();

                    tokio::spawn(async move {
                        Self::job_worker(jobs_clone, set_name_owned).await;
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
        }
        Ok(())
    }

    async fn job_worker(jobs: Arc<Mutex<HashMap<String, Job>>>, set_name: String) {
        loop {
            // Debouncing phase: wait for timer to stabilize
            let debounce_duration;
            let start_time;
            {
                let jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get(&set_name) {
                    debounce_duration =
                        Duration::from_secs(job.set.debounce_seconds.unwrap_or(60));
                    start_time = job.last_change.unwrap();
                } else {
                    return; // Job removed
                }
            }

            // Poll every 500ms to update remaining time and check for expiration
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if let Some(last_change) = job.last_change {
                        // Check if the timer was reset (new file change)
                        if last_change > start_time {
                            info!("Timer reset for set {}, restarting worker", set_name);
                            drop(jobs_lock);
                            // Exit this worker - a new one was spawned
                            return;
                        }

                        let elapsed = last_change.elapsed();
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
                    }
                } else {
                    return; // Job removed
                }
            }

            // Running phase (Placeholder)
            let backup_start_time = Instant::now();
            debug!("Starting backup execution for set {}", set_name);
            tokio::time::sleep(Duration::from_secs(2)).await;
            info!(
                "Backup completed for set {} in {:.2}s",
                set_name,
                backup_start_time.elapsed().as_secs_f64()
            );

            // Check if new changes occurred during backup
            {
                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if let Some(last_change) = job.last_change {
                        // If changes occurred during backup, re-enter debouncing
                        if last_change > backup_start_time {
                            info!(
                                "New changes detected for set {} during backup, re-debouncing",
                                set_name
                            );
                            let debounce_secs = job.set.debounce_seconds.unwrap_or(60);
                            job.state = JobState::Debouncing {
                                remaining_secs: debounce_secs,
                            };
                            continue; // Loop back to debouncing
                        }
                    }
                    // No new changes, return to Idle
                    job.state = JobState::Idle;
                    break;
                }
            }
        }
    }

    pub async fn get_status(&self) -> Vec<SetStatus> {
        let jobs = self.jobs.lock().await;
        jobs.values()
            .map(|job| SetStatus {
                name: job.set.name.clone(),
                state: job.state.clone(),
                last_backup: None, // Placeholder
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
                is_mounted: false, // Placeholder
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backutil_lib::config::GlobalConfig;

    #[tokio::test]
    async fn test_debounce_logic() -> Result<()> {
        let _ = tracing_subscriber::fmt::try_init();
        let config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![BackupSet {
                name: "test".to_string(),
                source: Some("/tmp/source".to_string()),
                sources: None,
                target: "/tmp/target".to_string(),
                exclude: None,
                debounce_seconds: Some(1), // 1 second for faster test
                retention: None,
            }],
        };

        let manager = JobManager::new(&config);

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

        // Wait for debounce to complete (1s debounce + margin)
        tokio::time::sleep(Duration::from_millis(1400)).await;
        let state = get_test_state().await.unwrap();
        assert_eq!(state, JobState::Running, "Expected Running after debounce");

        // Wait for simulated backup to complete (2s + margin)
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let state = get_test_state().await.unwrap();
        assert_eq!(state, JobState::Idle, "Expected Idle after backup completes");

        Ok(())
    }
}
