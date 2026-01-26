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
    // We don't strictly need a JoinHandle if we use tokio::spawn with a loop
    // or just let the timer task handle the transition.
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
                    info!("Set {} entered Debouncing state ({}s)", set_name, debounce_secs);

                    let jobs_clone = self.jobs.clone();
                    let set_name_owned = set_name.to_string();

                    tokio::spawn(async move {
                        Self::job_worker(jobs_clone, set_name_owned).await;
                    });
                }
                JobState::Debouncing { .. } => {
                    debug!("Set {} is already debouncing, timer reset", set_name);
                }
                JobState::Running => {
                    debug!("Set {} is currently running, will re-debounce after completion", set_name);
                }
            }
        }
        Ok(())
    }

    async fn job_worker(jobs: Arc<Mutex<HashMap<String, Job>>>, set_name: String) {
        loop {
            // Debouncing phase
            let mut debounce_duration = Duration::ZERO;
            {
                let jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get(&set_name) {
                    debounce_duration = Duration::from_secs(job.set.debounce_seconds.unwrap_or(60));
                }
            }

            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;

                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if let Some(last_change) = job.last_change {
                        let elapsed = last_change.elapsed();
                        if elapsed >= debounce_duration {
                            info!("Debounce timer expired for set {}, transitioning to Running", set_name);
                            job.state = JobState::Running;
                            break;
                        } else {
                            let remaining = debounce_duration.saturating_sub(elapsed).as_secs();
                            job.state = JobState::Debouncing { remaining_secs: remaining };
                        }
                    }
                } else {
                    return; // Job removed?
                }
            }

            // Running phase (Placeholder)
            debug!("Starting backup execution for set {}", set_name);
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Completion check
            {
                let mut jobs_lock = jobs.lock().await;
                if let Some(job) = jobs_lock.get_mut(&set_name) {
                    if let Some(last_change) = job.last_change {
                        if last_change.elapsed() < Duration::from_millis(100) {
                            // New change occurred very recently, loop back to debouncing
                            info!("New change detected for set {} during/after run, re-debouncing", set_name);
                            continue;
                        }
                    }
                    info!("Backup completed for set {}, returning to Idle", set_name);
                    job.state = JobState::Idle;
                    break;
                } else {
                    return;
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

    pub async fn get_job_state(&self, set_name: &str) -> Option<JobState> {
        let jobs = self.jobs.lock().await;
        jobs.get(set_name).map(|j| j.state.clone())
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
                debounce_seconds: Some(2), // 2 seconds for test
                retention: None,
            }],
        };

        let manager = JobManager::new(&config);

        // Initial state
        assert_eq!(manager.get_job_state("test").await.unwrap(), JobState::Idle);

        // Trigger change
        manager.handle_file_change("test").await?;
        let state = manager.get_job_state("test").await.unwrap();
        match state {
            JobState::Debouncing { remaining_secs } => assert_eq!(remaining_secs, 2),
            _ => panic!("Expected Debouncing state"),
        }

        // Wait 1s, trigger again
        tokio::time::sleep(Duration::from_secs(1)).await;
        manager.handle_file_change("test").await?;
        let state = manager.get_job_state("test").await.unwrap();
        match state {
            JobState::Debouncing { remaining_secs } => {
                // Should still be around 2 because it was reset
                assert!(remaining_secs >= 1);
            }
            _ => panic!("Expected Debouncing state"),
        }

        // Wait 3s (more than 2s debounce)
        tokio::time::sleep(Duration::from_secs(3)).await;
        let state = manager.get_job_state("test").await.unwrap();
        assert_eq!(state, JobState::Running);

        // Wait 3s (more than 2s "backup" duration)
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(manager.get_job_state("test").await.unwrap(), JobState::Idle);

        Ok(())
    }
}
