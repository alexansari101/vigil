# Technical Specification: backutil

This document defines the implementation contracts for `backutil`. All components (daemon, CLI, TUI) must conform to these specifications.

## 1. Components

The system consists of three logical components. Project structure is left to the implementer.

| Component | Responsibility |
|-----------|----------------|
| **Shared library** | Config parsing, type definitions, IPC message types, path helpers |
| **Daemon** | File watching, debouncing, restic execution, IPC server |
| **CLI/TUI** | Command parsing, TUI rendering, IPC client |

## 2. Suggested Crates

| Purpose | Crate |
|---------|-------|
| Async runtime | tokio |
| Serialization | serde, serde_json, toml |
| CLI parsing | clap |
| TUI | ratatui, crossterm |
| File watching | notify |
| Desktop notifications | notify-rust |
| Logging | tracing |
| Time handling | chrono |
| Error handling | thiserror, anyhow |
| XDG paths | directories |

## 3. File Paths

| Purpose | Path |
|---------|------|
| Config file | `~/.config/backutil/config.toml` |
| Password file | `~/.config/backutil/.repo_password` |
| Log file | `~/.local/share/backutil/backutil.log` |
| Unix socket | `$XDG_RUNTIME_DIR/backutil.sock` (fallback: `/tmp/backutil-$UID.sock`) |
| PID file | `$XDG_RUNTIME_DIR/backutil.pid` |
| FUSE mounts | `~/.local/share/backutil/mnt/<set-name>/` |
| Systemd units | `~/.config/systemd/user/backutil-daemon.service` |

## 4. Config Schema (TOML)

```toml
# ~/.config/backutil/config.toml

[global]
debounce_seconds = 60           # Wait time after last change before backup
retention = { keep_last = 10 }  # Default retention policy

# Optional overrides
# retention = { keep_daily = 7, keep_weekly = 4, keep_monthly = 6 }

[[backup_set]]
name = "personal"
source = "~/personal_records"
target = "/mnt/backup/personal"         # Restic repository path
exclude = ["*.tmp", ".cache/**"]         # Optional glob patterns

[[backup_set]]
name = "financial"
source = "~/financial_docs"
target = "/mnt/backup/financial"
debounce_seconds = 30                    # Override global debounce
retention = { keep_last = 20 }           # Override global retention

# Common Target mode: multiple sources → subfolders of one repo
[[backup_set]]
name = "combined"
sources = ["~/documents", "~/projects"]  # Note: 'sources' plural
target = "/mnt/backup/combined"
# Creates tags: documents, projects (derived from source dir names)
```

### Config Structure

**Config** (root):

- `global` — GlobalConfig
- `backup_set` — list of BackupSet

**GlobalConfig**:

- `debounce_seconds` — integer, default 60
- `retention` — RetentionPolicy, optional

**BackupSet**:

- `name` — string, required, unique identifier
- `source` — path, optional (single source mode)
- `sources` — list of paths, optional (multi-source mode; mutually exclusive with `source`)
- `target` — path, required, restic repository location
- `exclude` — list of glob patterns, optional
- `debounce_seconds` — integer, optional, overrides global
- `retention` — RetentionPolicy, optional, overrides global

**RetentionPolicy**:

- `keep_last` — integer, optional
- `keep_daily` — integer, optional
- `keep_weekly` — integer, optional
- `keep_monthly` — integer, optional

## 5. IPC Protocol

Communication between CLI/TUI and daemon uses JSON over Unix socket. Each message is a newline-delimited JSON object.

### Request Types

| Type | Payload | Description |
|------|---------|-------------|
| `Status` | none | Get status of all backup sets |
| `Backup` | `set_name`: string or null | Trigger backup (null = all sets) |
| `Prune` | `set_name`: string or null | Run retention cleanup |
| `Snapshots` | `set_name`: string, `limit`: int or null | List snapshots |
| `Mount` | `set_name`: string, `snapshot_id`: string or null | Mount snapshot (null = latest) |
| `Unmount` | `set_name`: string or null | Unmount (null = all) |
| `Purge` | `set_name`: string | Remove repo and data for a set |
| `Shutdown` | none | Graceful daemon shutdown |
| `Ping` | none | Health check |

### Response Types

| Type | Payload | Description |
|------|---------|-------------|
| `Ok` | `data`: ResponseData or null | Success |
| `Error` | `code`: string, `message`: string | Failure |
| `Pong` | none | Response to Ping |

### ResponseData Variants

| Kind | Fields |
|------|--------|
| `Status` | `sets`: list of SetStatus |
| `Snapshots` | `snapshots`: list of SnapshotInfo |
| `BackupStarted` | `set_name`: string |
| `BackupsTriggered` | `started`: list of string, `failed`: list of (string, string) |
| `BackupComplete` | `set_name`, `snapshot_id`, `added_bytes`, `duration_secs` |
| `BackupFailed` | `set_name`: string, `error`: string |
| `MountPath` | `path`: string |
| `PruneResult` | `set_name`: string, `reclaimed_bytes`: integer |
| `PrunesTriggered` | `succeeded`: list of (string, integer), `failed`: list of (string, string) |

### Error Codes

`UnknownSet`, `BackupFailed`, `ResticError`, `MountFailed`, `NotMounted`, `DaemonBusy`, `InvalidRequest`

### Example Exchange

```json
// Client → Daemon
{"type":"Backup","payload":{"set_name":"personal"}}

// Daemon → Client
{"type":"Ok","payload":{"kind":"BackupStarted","set_name":"personal"}}

// Daemon → Client (async update)
{"type":"Ok","payload":{"kind":"BackupComplete","set_name":"personal","snapshot_id":"a1b2c3d4","added_bytes":1048576,"duration_secs":4.2}}
```

## 6. Shared Types

**JobState** (enum):

- `Idle` — no activity
- `Debouncing` — waiting after file change; includes `remaining_secs`
- `Running` — backup in progress
- `Error` — last backup failed

**SetStatus**:

- `name` — string
- `state` — JobState
- `last_backup` — BackupResult or null (populated from most recent restic snapshot on daemon startup)
- `source_paths` — list of paths
- `target` — path
- `is_mounted` — boolean

**BackupResult**:

- `snapshot_id` — string
- `timestamp` — ISO 8601 datetime (UTC)
- `added_bytes` — integer
- `duration_secs` — float
- `success` — boolean
- `error_message` — string or null

**SnapshotInfo**:

- `id` — string (full restic snapshot ID)
- `short_id` — string (8-char prefix)
- `timestamp` — ISO 8601 datetime (UTC)
- `paths` — list of paths
- `tags` — list of strings

## 7. State Machine

```
                    ┌─────────────────────────────────────┐
                    │                                     │
                    ▼                                     │
    ┌──────┐   file change   ┌────────────┐   timer    ┌─────────┐
    │ Idle │ ──────────────► │ Debouncing │ ─────────► │ Running │
    └──────┘                 └────────────┘            └─────────┘
        ▲                         │                      │    │
        │                         │ file change          │    │
        │                         │ (reset timer)        │    │
        │                         ▼                      │    │
        │                    ┌────────────┐              │    │
        │                    │ Debouncing │◄─────────────┘    │
        │                    └────────────┘   new change      │
        │                                     during run      │
        │                                                     │
        │                      success                        │
        └─────────────────────────────────────────────────────┘
                    │
                    │ failure
                    ▼
               ┌─────────┐
               │  Error  │ (stays until manual retry or next file change)
               └─────────┘
```

**Note on graceful shutdown:** When the daemon receives a shutdown signal (SIGTERM/SIGINT) while a backup is in the `Running` state, the current implementation allows the restic process to complete before shutting down. Future implementations should consider adding cancellation support to abort in-progress backups gracefully.

## 8. Systemd Unit (Example)

Generated by `backutil bootstrap`. This is a reference — adapt as needed.

**Requirements:**

- User-level service (`~/.config/systemd/user/`)
- Restart on failure
- Appropriate read/write permissions for config and data directories

```ini
# Example: ~/.config/systemd/user/backutil-daemon.service

[Unit]
Description=Backutil Daemon - Automated Backup Service
After=default.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/backutil-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

## 9. Restic Command Mapping

| backutil action | Restic command |
|-----------------|----------------|
| `init` | `restic init --repo <target>` |
| `backup` | `restic backup --repo <target> --password-file <pw> --exclude <patterns> <source>` |
| `prune` | `restic forget --repo <target> --password-file <pw> --prune --keep-last N` |
| `snapshots` | `restic snapshots --repo <target> --password-file <pw> --json` |
| `mount` | `restic mount --repo <target> --password-file <pw> <mountpoint>` |

Password is always passed via `--password-file ~/.config/backutil/.repo_password`.

## 10. Error Handling

### User-Facing Errors

All errors displayed to users must include:

1. What failed (e.g., "Backup of 'personal' failed")
2. Why it failed (e.g., "Repository not found at /mnt/backup/personal")
3. How to fix it (e.g., "Run `backutil init personal` to initialize the repository")

### Log Format

```
2024-01-15T10:30:45Z INFO  [personal] Backup started
2024-01-15T10:30:49Z INFO  [personal] Backup complete: snapshot a1b2c3d4, 1.2MB added
2024-01-15T10:30:49Z ERROR [financial] Backup failed: repository locked by another process
```

## 11. TUI Layout (ASCII Reference)

```
┌─────────────────────────────────────────────────────────────────────┐
│ backutil v0.1.0                                    ● All Systems OK │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  personal        ~/personal_records → /mnt/backup/personal          │
│  ● Idle          Last: 5 min ago  ▁▂▄▂▁                             │
│                                                                     │
│  financial       ~/financial_docs → /mnt/backup/financial           │
│  ◐ Debouncing    Waiting: 45s     ▁▁▂▄▂                             │
│                                                                     │
│  combined        2 sources → /mnt/backup/combined                   │
│  ◌ Running       Progress: 64%    ████████░░░░                      │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│ [b]ackup all  [p]rune  [m]ount  [s]napshots  [?]help  [q]uit        │
└─────────────────────────────────────────────────────────────────────┘
```

## 12. CLI Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Config error (missing, invalid) |
| 3 | Daemon not running |
| 4 | Restic error |
| 5 | Mount/unmount error |

## 13. CLI Output Requirements

### Global Flags

All CLI commands should support these global flags:

| Flag | Description |
|------|-------------|
| `--quiet`, `-q` | Suppress non-essential output; only show errors |
| `--json` | Output machine-readable JSON instead of human-readable text |

### Output Standards

**Short IDs:** When displaying snapshot IDs, use the 8-character `short_id` format rather than full 64-character hashes.

**Human-Readable Sizes:** Use `format_size()` to display bytes as human-readable (e.g., "1.2 MiB" instead of "1258291").

**Plain English Help:** Help text should use plain English:

- Good: `[SET]  Backup set name. If omitted, backs up all sets.`
- Bad: `[SET]  Name of the backup set (null = all sets)`

**Clean CLI Output:** Daemon log messages (e.g., `INFO backutil_daemon::manager: ...`) must not appear in CLI command output. CLI should only display user-facing messages.

### New CLI Commands

**`backutil list`**

Lists all configured backup sets. Does not require daemon to be running.

```
$ backutil list
NAME            SOURCE                          TARGET
personal        ~/personal_records              /mnt/backup/personal
financial       ~/financial_docs                /mnt/backup/financial
```

**`backutil snapshots <SET> [--limit N]`**

Lists available snapshots for a backup set. Requires daemon.

```
$ backutil snapshots personal --limit 5
ID        DATE                 SIZE      PATHS
5a08c7d4  2026-01-28 19:43     3.3 KiB   /tmp/backutil_test/source1
e6ad2ad9  2026-01-28 19:40     1.9 KiB   /tmp/backutil_test/source1
```

**`backutil check [SET]`**

Validates configuration and optionally tests repository access. Does not require daemon for config validation.

```
$ backutil check
✓ Configuration valid: 2 backup sets defined
✓ Password file exists and is readable
✓ personal: Repository accessible
✓ financial: Repository accessible

$ backutil check --config-only
✓ Configuration valid: 2 backup sets defined
✓ Password file exists and is readable
```
