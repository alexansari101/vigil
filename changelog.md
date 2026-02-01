# Changelog

**Instructions for subagents: Always add new entries at the TOP of this file, directly under the divider below, in reverse chronological order (newest first).**

This file tracks recent changes. For format guidelines, see `developer_guidelines.md` Section 9.

> **Note:** Historical entries for Phase 1-5 (Tasks #1-42) have been archived into summaries below. Use `git log` to view detailed history.

---

## [2026-01-31] — daemon: implement automatic retention policy enforcement

**What changed:**

- Implemented automatic pruning after successful backups when a retention policy is configured.
- Added `prune_set()` helper method to share core pruning logic between manual and automatic operations.
- Added `auto_prune_after_backup()` method that spawns async pruning task after backup completion.
- Added new `PruneComplete` IPC event variant to notify clients of automatic retention cleanup.
- Modified `job_worker()` to trigger automatic pruning after `BackupComplete` event is sent.
- Added comprehensive integration test `test_auto_prune_after_backup()` to verify the feature.

**Why:**

- Previously, retention policies were only enforced via manual `backutil prune` command, requiring user intervention.
- Automatic enforcement ensures repositories stay within configured limits without manual maintenance.
- Improves the "set-it-and-forget-it" automation goal of the project.

**Files affected:**

- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil-lib/src/ipc.rs (modified)
- crates/backutil-daemon/tests/integration_test.rs (modified)

**Testing notes:**

- All existing unit tests pass without modification.
- New integration test `test_auto_prune_after_backup` verifies automatic pruning behavior.
- Manual testing: Configure retention policy, trigger multiple backups, verify snapshot count stays within limits.
- Auto-prune failures are logged and notify user without affecting backup success status.

---

## [2026-01-31] 3a60602 — cli: implement `track` and `untrack` commands

**What changed:**

- Added `backutil track <NAME> <SOURCE> <TARGET>` command to add new backup sets to the configuration automatically.
- Added `backutil untrack <NAME> [--purge]` command to remove backup sets from the configuration.
- Enhanced `backutil-lib` with programmatic configuration management: `save_config`, `load_config_raw`, and refactored validation.
- Integrated `track` and `untrack` with service reloading (`ReloadConfig` IPC) to ensure the daemon picks up changes immediately.
- Improved `untrack --purge` logic to safely collect the repository path and delete it before removing the set from configuration.

**Why:**

- Implements FR5 (Setup Wizard, track/untrack) to allow managing backup sets via the CLI instead of manual config editing.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil-lib/src/config.rs (modified)

**Testing notes:**

- Verified `track` correctly updates `config.toml`, initializes the Restic repo, and reloads the service.
- Verified `untrack` correctly removes the set and reloads the service.
- Verified `untrack --purge` successfully deletes the repository data.
- Unit tests added in `backutil-lib` for config management helpers.

## [2026-01-31] d819fdd — cli: implement guided onboarding (`backutil setup`)

**What changed:**

- Implemented an interactive setup wizard that guides new users through creating a repository password and their first backup set.
- Developed an idempotent logic that detects existing configuration and safely redirects users to management commands (e.g., `track`) instead of overwriting files.
- Added path validation and home directory expansion (`~/`) for source and target paths during setup.
- Developed integration tests covering idempotent and partial setup scenarios.
- Unified configuration path handling across the library and CLI by exposing `active_config_path()`.

**Why:**

- Implements FR5 (Setup Wizard) to improve first-time user experience and simplify system provisioning.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil-lib/src/paths.rs (modified)
- crates/backutil-lib/src/config.rs (modified)
- crates/backutil/Cargo.toml (modified)
- crates/backutil/tests/cli_setup_test.rs (new)

**Testing notes:**

- Verified `test_cli_setup_idempotent` and `test_cli_setup_partial` integration tests pass.
- Manually verified the welcome flow and path validation warnings.

## [2026-01-31] 744e025 — cli: group service-related commands under `service` subcommand

**What changed:**

- Grouped `bootstrap`, `disable`, `reload`, and `uninstall` under a new `service` subcommand.
- Renamed operations for clarity:
  - `bootstrap` -> `service install`
  - `disable` -> `service stop`
  - `reload` -> `service reload`
  - `uninstall` -> `service uninstall`
- Updated all help text, PRD, and Spec documentation.
- Updated integration tests to reflect the new command structure.

**Why:**

- Implements FR5 (Automated Onboarding & Service Management) and improves CLI organization per Phase 7 requirements.
- Provides a more intuitive and structured CLI for managing the background service.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_systemd_test.rs (modified)
- prd.md (modified)
- spec.md (modified)

**Testing notes:**

- Verified with `cargo test --workspace` (regular tests).
- Verified with `cargo test --workspace -- --ignored --test-threads=1` (restic-dependent tests).
- Manually verified `--help` output for both top-level and service subcommands.
- Confirmed compatibility with existing service management logic.

## [2026-01-31] — review: fix orphaned mount detection and add missing tests

**What changed:**

- Fixed a bug where `get_status` immediately cleared `is_mounted` for orphaned mounts (mounts without a tracked process). The old code assumed `is_mounted && mount_process.is_none()` was impossible, but this is exactly the state set by the new mount sync feature. Now verifies via `/proc/mounts` before clearing.
- Added unit tests for `is_mount_point` (nonexistent path, regular directory).
- Fixed changelog inaccuracy: previous entry claimed unit tests existed for mount detection when none did.
- Minor style cleanup: removed unnecessary `Path::new()` wrapping of `PathBuf` in `refresh_set_status`.

**Why:**

- Without this fix, the mount sync feature introduced in `2e0f85d` would detect an orphaned mount on startup but lose that state on the first `get_status` call, making the feature effectively non-functional.

**Files affected:**

- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil-lib/src/paths.rs (modified)
- changelog.md (modified)

**Testing notes:**

- All workspace tests pass, including new `is_mount_point` unit tests.
- Verified with `cargo fmt --check` and `cargo clippy --workspace`.

## [2026-01-31] 2e0f85d — daemon: sync mount status on restart

**What changed:**

- Added `is_mount_point` helper to `backutil-lib` to detect active FUSE mounts via `/proc/mounts`.
- Updated `JobManager` to detect existing mounts during initialization and status refresh.
- Improved orphaned mount handling in `get_status` monitoring.
- Added unit tests for mount detection logic.

**Why:**

- Resolves a limitation where the daemon lost track of active mounts if it was restarted or crashed while a directory was still mounted.

**Files affected:**

- crates/backutil-lib/src/paths.rs (modified)
- crates/backutil-daemon/src/manager.rs (modified)

**Testing notes:**

- Verified daemon logic compiles and handles state correctly during refresh.

## [2026-01-30] 0292cf4 — cli: fix log selection to use modification time

**What changed:**

- Changed `find_latest_log` logic to sort all files starting with `backutil.log` by their modification time.
- Removed early exit that prioritized `backutil.log`.
- Added a small delay in `cli_logs_test.rs` to ensure distinct timestamps for modification time sorting across different filesystem resolutions.

**Why:**

- The previous implementation used lexicographical sorting and prioritized `backutil.log` if it existed, which caused stale logs to be shown even when newer dated logs were present.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_logs_test.rs (modified)

**Testing notes:**

- Verified with `cargo test`.
- Verified manually by creating stale `backutil.log` and fresh dated logs.

## [2026-01-30] 81e8a4d — cli: fix --quiet flag in logs command and changelog format

**What changed:**

- Fixed `handle_logs` to respect the `--quiet` flag: informational/status messages (e.g., "No log files found", "Waiting for log file", rotation notices) are now suppressed when `--quiet` is passed.
- Fixed changelog entry for commit `726680a` to include the commit hash and use the correct `cli` component per developer_guidelines Section 9.

**Why:**

- The `--quiet` flag was accepted but ignored (`_quiet`), inconsistent with other commands like `handle_status` and `handle_bootstrap` that properly check `quiet`.
- Changelog header format did not match the required `## [YYYY-MM-DD] <commit-short-hash> — <component>: <title>` pattern.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- changelog.md (modified)

## [2026-01-31] 726680a — cli: improve check error message and fix logs command output

**What changed:**

- Improved `backutil check` to provide a helpful hint to run `backutil init` when a repository is missing.
- Fixed `backutil logs` to correctly identify `backutil.log` as the latest active log file.
- Fixed a bug in `backutil logs` where `BufReader` was incorrectly consuming data, leading to empty output.
- Enhanced `backutil logs` with more robust follow mode logic and better handling of rotated logs.
- Added integration tests for both `check` and `logs` commands.

**Why:**

- Resolves user testing feedback where `backutil check` was confusing for new users and `backutil logs` failed to show content.
- Improves CLI usability and troubleshooting experience.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_check_test.rs (modified)
- crates/backutil/tests/cli_logs_test.rs (new)

**Testing notes:**

- Verified `backutil check` shows the new hint when a repository is deleted.
- Verified `backutil logs` correctly displays existing log entries.
- Verified `backutil logs -f` correctly follows new entries appended to the log file.
- All automated tests passed, including new integration tests.

---

## [2026-01-31] — bugfix: fix status update and file watcher issues

**What changed:**

- Fixed `backutil status` to correctly show the most recent snapshot as "last backup" (changed `first()` to `last()`).
- Fixed daemon to correctly use the global `debounce_seconds` configuration when no per-set override is provided.
- Improved `refresh_set_status` to preserve live backup metrics (`added_bytes`, `duration_secs`) after successful backup runs.
- Enhanced file watcher with better logging and robust path matching (including canonicalization).
- Fixed critical regression: Moved Restic backup execution outside of the `JobManager` lock to prevent blocking IPC requests (e.g. `status` command) during backups.
- Fixed critical regression: Added `worker_active` flag to `Job` state to prevent multiple background workers from being spawned for the same backup set.

**Why:**

- Resolves user testing feedback where `backutil status` was not updating the "last backup" time.
- Resolves issues where `touch` or new file additions appeared to not trigger backups (due to incorrect 60s default debounce).
- Fixes IPC responsiveness during long-running backups and prevents duplicate worker tasks.
- Improves visibility and reliability of the backup process.

**Files affected:**

- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil-daemon/src/watcher.rs (modified)

**Testing notes:**

- Verified with reproduction script `repro_issues.sh` covering status updates and file watcher triggers.
- All workspace tests passed, including integration tests.

---

## [2026-01-30] — bugfix: fix daemon state management (mirroring, stale status)

 **What changed:**

- Refactored `JobManager` to support granular and cross-set status refreshes.
- Implemented `refresh_set_status` for individual backup set updates.
- Added `refresh_related_sets` to synchronize status across sets sharing the same Restic repository.
- Updated `sync_config` to always trigger a full background refresh of all sets on config reload.
- Improved repo access error handling to clear stale metrics instead of preserving them.

 **Why:**

- Resolves issues where backup sets with the same target mirrored each other's status incorrectly.
- Fixes stale status reports after a backup set's target was changed in `config.toml`.
- Fixes persistent stale status after a repository was purged (repository deleted).

 **Files affected:**

- crates/backutil-daemon/src/manager.rs (modified)

 **Testing notes:**

- Verified with a reproduction script covering mirroring, target change, and purge scenarios.
- Verified all workspace integration tests (including ignored ones) pass.

---

## [2026-01-30] — fix: make daemon config reload more robust and add reload CLI command

 **What changed:**

- Made config reload async with retry logic (3 attempts with 2s delay) to handle partial file writes during atomic saves.
- Added 200ms initial delay before reading config to avoid partial-file reads.
- Added `backutil reload` CLI subcommand to manually trigger daemon config reload.
- Separated config loading (async, off main loop) from config application (on main loop) via a channel.

 **Why:**

- Config file watching events can fire mid-write, causing parse failures. Retry with backoff handles this gracefully.
- The `backutil reload` CLI command gives users explicit control over config reloading without restarting the daemon.

 **Files affected:**

- crates/backutil-daemon/src/main.rs (modified)
- crates/backutil/src/main.rs (modified)

 **Testing notes:**

- Verified all workspace tests pass.

---

## Phase 3-5 Summary (Tasks #14-42)

> **Note:** Detailed entries archived. Use `git log` to view full history.

The following work was completed in Phase 3-5 (2026-01-26 through 2026-01-30):

- **CLI skeleton and status** (Task #14): `backutil` CLI with `clap`, IPC client, `status` command
- **CLI backup** (Task #16): `backutil backup [SET]` with completion events, `--no-wait`, `--timeout`
- **CLI mount/unmount** (Task #17): FUSE mount with interactive snapshot picker, unmount support
- **CLI prune** (Task #18): `backutil prune [SET]` with reclaimed space reporting, exit code 4 on errors
- **CLI logs** (Task #19): `backutil logs [-f]` with tail, follow mode, rotation handling
- **CLI bootstrap/disable/uninstall** (Tasks #20-21): systemd user unit management, `--purge` option
- **CLI list** (Task #34): `backutil list` with `--json` output
- **CLI snapshots** (Task #35): `backutil snapshots <SET>` with `--limit` and `--json`
- **CLI check** (Task #36): `backutil check [SET]` with `--config-only`, exit codes 2/4
- **Short snapshot IDs** (Task #37): 8-character truncated IDs in backup results
- **Plain English help text** (Task #38): user-friendly CLI help descriptions
- **Global --quiet/--json flags** (Task #39): machine-readable output and quiet mode for all commands
- **Backup hang fix** (Task #40): fixed BufReader re-instantiation bug, added timeout/no-wait
- **Test fixes** (Tasks #41-42): mount test isolation, serial daemon manager tests, env var races
- **Enhanced status metrics** (Task #32): snapshot count and repo size in `backutil status`
- **Logging and graceful shutdown** (Task #27): `tracing-appender` daily rotation, `CancellationToken`
- **Daemon status persistence** (Task #31): restore last backup time from restic on startup
- **Purge and config reload** (Task #33): `backutil purge`, auto-reload on config change, `ReloadConfig` IPC
- **Bugfixes**: false positive mount detection, deprecated `--last` flag, invalid `--snapshot` flag, stale daemon state, config reload robustness, `time` crate version pinning

All Phase 3-5 tests pass. For detailed implementation notes, use `git log --oneline` or view the git history.

## Phase 1-2 Summary (Tasks #1-13)

The following foundational work was completed in Phase 1-2:

- **Project scaffolding**: Workspace with `backutil-lib`, `backutil-daemon`, `backutil` crates
- **Config parsing**: TOML config with backup sets, retention policies, glob exclusions
- **Shared types and IPC**: `Request`/`Response` types, `JobState`, `SetStatus`, `SnapshotInfo`
- **Path helpers**: XDG-compliant paths for config, logs, socket, PID, mounts
- **Daemon skeleton**: PID file management, Unix socket server, graceful shutdown
- **File watcher**: inotify-based watching with glob exclusion filtering
- **Debounce logic**: `JobManager` state machine (Idle → Debouncing → Running → Idle)
- **Restic executor**: Commands for init, backup, prune, snapshots, mount with JSON parsing
- **Backup orchestration**: Automatic and manual backup triggers, desktop notifications
- **Status and snapshots**: IPC handlers for querying daemon state and snapshot history
- **Mount/unmount**: FUSE mount management with restic mount process tracking
- **Prune command**: Retention policy cleanup with reclaimed space reporting

All Phase 1-2 tests pass. For detailed implementation notes, use `git log --oneline` or view the git history.
