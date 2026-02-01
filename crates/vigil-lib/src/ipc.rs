use crate::types::{SetStatus, SnapshotInfo};
use serde::{Deserialize, Serialize};

/// IPC Request from client (CLI/TUI) to daemon.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum Request {
    /// Get status of all backup sets.
    Status,
    /// Trigger a backup. If set_name is None, all sets are backed up.
    Backup { set_name: Option<String> },
    /// Run retention cleanup. If set_name is None, all sets are pruned.
    Prune { set_name: Option<String> },
    /// List snapshots for a specific set.
    Snapshots {
        set_name: String,
        limit: Option<usize>,
    },
    /// Mount a snapshot. If snapshot_id is None, the latest is mounted.
    Mount {
        set_name: String,
        snapshot_id: Option<String>,
    },
    /// Unmount a set. If set_name is None, all sets are unmounted.
    Unmount { set_name: Option<String> },
    /// Request graceful daemon shutdown.
    Shutdown,
    /// Reload configuration from disk.
    ReloadConfig,
    /// Health check.
    Ping,
}

/// IPC Response from daemon to client.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum Response {
    /// Request succeeded.
    Ok(Option<ResponseData>),
    /// Request failed.
    Error { code: String, message: String },
    /// Health check response.
    Pong,
}

/// Success data payload for an IPC response.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum ResponseData {
    /// Status of all backup sets.
    Status { sets: Vec<SetStatus> },
    /// List of snapshots.
    Snapshots { snapshots: Vec<SnapshotInfo> },
    /// Confirmation that a backup set has started backing up.
    BackupStarted { set_name: String },
    /// Result of triggering backups for multiple sets.
    BackupsTriggered {
        started: Vec<String>,
        failed: Vec<(String, String)>, // (set_name, error_message)
    },
    /// Confirmation that a backup operation has completed.
    BackupComplete {
        set_name: String,
        snapshot_id: String,
        added_bytes: u64,
        duration_secs: f64,
    },
    /// Notification that a backup operation failed.
    BackupFailed { set_name: String, error: String },
    /// The local path where a snapshot was mounted.
    MountPath { path: String },
    /// Result of a prune operation for a single set.
    PruneResult {
        set_name: String,
        reclaimed_bytes: u64,
    },
    /// Result of triggering prunes for multiple sets.
    PrunesTriggered {
        succeeded: Vec<(String, u64)>, // (set_name, reclaimed_bytes)
        failed: Vec<(String, String)>, // (set_name, error_message)
    },
    /// Notification that automatic retention enforcement completed after backup.
    PruneComplete {
        set_name: String,
        reclaimed_bytes: u64,
    },
}

/// Common error codes used in IPC error responses.
pub mod error_codes {
    pub const UNKNOWN_SET: &str = "UnknownSet";
    pub const BACKUP_FAILED: &str = "BackupFailed";
    pub const RESTIC_ERROR: &str = "ResticError";
    pub const MOUNT_FAILED: &str = "MountFailed";
    pub const NOT_MOUNTED: &str = "NotMounted";
    pub const DAEMON_BUSY: &str = "DaemonBusy";
    pub const INVALID_REQUEST: &str = "InvalidRequest";
}
