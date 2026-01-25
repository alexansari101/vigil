use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Current state of a backup set job.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum JobState {
    /// No activity.
    Idle,
    /// Waiting after a file change before triggering a backup.
    Debouncing { remaining_secs: u64 },
    /// Backup operation is currently in progress.
    Running,
    /// The last backup operation failed.
    Error,
}

/// Summary status of a backup set.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SetStatus {
    /// Unique identifier for the backup set.
    pub name: String,
    /// Current job state.
    pub state: JobState,
    /// Details of the most recent backup attempt, if any.
    pub last_backup: Option<BackupResult>,
    /// List of source directory paths being watched.
    pub source_paths: Vec<PathBuf>,
    /// Restic repository target path.
    pub target: PathBuf,
    /// Whether the backup set is currently mounted via FUSE.
    pub is_mounted: bool,
}

/// Results of a single backup operation.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BackupResult {
    /// Restic snapshot ID.
    pub snapshot_id: String,
    /// UTC timestamp when the backup completed.
    pub timestamp: DateTime<Utc>,
    /// Number of new bytes added to the repository.
    pub added_bytes: u64,
    /// Total time taken for the backup operation.
    pub duration_secs: f64,
    /// Whether the backup was successful.
    pub success: bool,
    /// Error message if the backup failed.
    pub error_message: Option<String>,
}

/// Information about a restic snapshot.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SnapshotInfo {
    /// Full 64-character hex restic snapshot ID.
    pub id: String,
    /// 8-character prefix of the ID.
    pub short_id: String,
    /// UTC timestamp of the snapshot.
    pub timestamp: DateTime<Utc>,
    /// List of paths included in the snapshot.
    pub paths: Vec<PathBuf>,
    /// List of tags associated with the snapshot.
    pub tags: Vec<String>,
}
