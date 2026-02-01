use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{Config as NotifyConfig, Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use vigil_lib::config::Config;

#[derive(Debug)]
pub enum WatcherEvent {
    FileChanged { set_name: String, path: PathBuf },
}

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    // Maps watched paths to their backup set name
    // We use Arc to share it with the watcher callback
    inner: Arc<WatcherInner>,
}

struct WatcherInner {
    // Maps watched root paths to their backup set name
    path_to_set: HashMap<PathBuf, String>,
    // Maps backup set name to its exclusion patterns
    exclusion_sets: HashMap<String, GlobSet>,
    event_tx: mpsc::Sender<WatcherEvent>,
}

impl FileWatcher {
    pub fn new(config: &Config, event_tx: mpsc::Sender<WatcherEvent>) -> Result<Self> {
        let mut path_to_set = HashMap::new();
        let mut exclusion_sets = HashMap::new();

        for set in &config.backup_sets {
            // Build exclusion set
            if let Some(ref excludes) = set.exclude {
                let mut builder = GlobSetBuilder::new();
                for pattern in excludes {
                    builder.add(Glob::new(pattern).context("Invalid exclusion pattern")?);
                }
                exclusion_sets.insert(
                    set.name.clone(),
                    builder.build().context("Failed to build GlobSet")?,
                );
            }

            // Register paths
            if let Some(ref source) = set.source {
                path_to_set.insert(PathBuf::from(source), set.name.clone());
            }
            if let Some(ref sources) = set.sources {
                for source in sources {
                    path_to_set.insert(PathBuf::from(source), set.name.clone());
                }
            }
        }

        let inner = Arc::new(WatcherInner {
            path_to_set,
            exclusion_sets,
            event_tx,
        });

        let inner_clone = inner.clone();
        let watcher = RecommendedWatcher::new(
            move |res: std::result::Result<Event, Error>| match res {
                Ok(event) => {
                    if let Err(e) = handle_event(&inner_clone, event) {
                        error!("Error handling watcher event: {}", e);
                    }
                }
                Err(e) => error!("Watch error: {}", e),
            },
            NotifyConfig::default(),
        )?;

        let mut file_watcher = Self { watcher, inner };

        file_watcher.start_watching()?;

        Ok(file_watcher)
    }

    fn start_watching(&mut self) -> Result<()> {
        for path in self.inner.path_to_set.keys() {
            if path.exists() {
                info!("Watching path: {:?}", path);
                self.watcher
                    .watch(path, RecursiveMode::Recursive)
                    .context(format!("Failed to watch path: {:?}", path))?;
            } else {
                warn!("Source path does not exist, skipping: {:?}", path);
            }
        }
        Ok(())
    }
}

fn handle_event(inner: &WatcherInner, event: Event) -> Result<()> {
    // Only interested in data changes (creates, modifies, deletes)
    debug!("Event kind: {:?}, paths: {:?}", event.kind, event.paths);

    for path in event.paths {
        // Use metadata to check if it's a directory, but don't fail if file is already gone (e.g. rapid delete/move)
        if path.is_dir() {
            debug!("Skipping directory: {:?}", path);
            continue;
        }

        debug!("Processing path: {:?}", path);
        let mut found_set = None;

        // Try to match the path against our watched roots
        for (root, set_name) in &inner.path_to_set {
            if path.starts_with(root) {
                found_set = Some((root, set_name));
                break;
            }

            // Try absolute path if it's not already
            if let Ok(abs_path) = std::fs::canonicalize(&path) {
                if abs_path.starts_with(root) {
                    found_set = Some((root, set_name));
                    break;
                }
            }
        }

        if let Some((root, set_name)) = found_set {
            // Check exclusions
            if let Some(exclusion_set) = inner.exclusion_sets.get(set_name) {
                let is_excluded = exclusion_set.is_match(&path)
                    || path
                        .file_name()
                        .map(|n| exclusion_set.is_match(n))
                        .unwrap_or(false)
                    || path
                        .strip_prefix(root)
                        .ok()
                        .map(|p| exclusion_set.is_match(p))
                        .unwrap_or(false);

                if is_excluded {
                    debug!("Excluding path: {:?}", path);
                    continue;
                }
            }

            info!(
                "File change detected in set {}: {:?} (event: {:?})",
                set_name, path, event.kind
            );
            let _ = inner.event_tx.try_send(WatcherEvent::FileChanged {
                set_name: set_name.clone(),
                path,
            });
        } else {
            debug!("Path not in any watched set: {:?}", path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use vigil_lib::config::{BackupSet, GlobalConfig};

    #[tokio::test]
    async fn test_watcher_filtering() -> Result<()> {
        let _ = tracing_subscriber::fmt::try_init();
        let tmp = tempdir()?;
        let source_path = tmp.path().join("source");
        fs::create_dir(&source_path)?;

        let config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![BackupSet {
                name: "test".to_string(),
                source: Some(source_path.to_string_lossy().to_string()),
                sources: None,
                target: "/tmp/target".to_string(),
                exclude: Some(vec!["*.tmp".to_string(), "ignore_me/*".to_string()]),
                debounce_seconds: None,
                retention: None,
            }],
        };

        let (tx, mut rx) = mpsc::channel(100);
        let _watcher = FileWatcher::new(&config, tx)?;

        // Test normal file
        let file1 = source_path.join("file1.txt");
        fs::write(&file1, "hello")?;

        // Wait for event (with timeout)
        let event = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await;
        assert!(event.is_ok(), "Timed out waiting for event");
        let event = event.unwrap().expect("No event received");
        let WatcherEvent::FileChanged { set_name, path } = event;
        assert_eq!(set_name, "test");
        assert!(path.ends_with("file1.txt"));

        // Drain any extra events (e.g. Modify after Create)
        while let Ok(Some(_)) =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await
        {}

        // Test excluded file (glob)
        let file2 = source_path.join("file2.tmp");
        fs::write(&file2, "ignore")?;

        let event = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(event.is_err(), "Received event for excluded file");

        // Test excluded directory (glob)
        let ignore_dir = source_path.join("ignore_me");
        fs::create_dir(&ignore_dir)?;
        let file3 = ignore_dir.join("secret.txt");
        fs::write(&file3, "shh")?;

        let event = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await;
        assert!(
            event.is_err(),
            "Received event for excluded directory content"
        );

        Ok(())
    }
}
