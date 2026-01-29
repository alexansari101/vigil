# Changelog

**Instructions for subagents: Always add new entries at the TOP of this file, directly under the divider below, in reverse chronological order (newest first).**

This file tracks recent changes. For format guidelines, see `developer_guidelines.md` Section 9.

> **Note:** Historical entries for Phase 1-2 (Tasks #1-13) have been archived. Use `git log` to view detailed history.

---

## [2026-01-28] — feature: cli list command

**What changed:**

- Implemented `backutil list` command to display all configured backup sets.
- Added support for `--json` flag to output machine-readable configuration.
- Enhanced `Config` and related structs in `backutil-lib` with `Serialize` derive.
- Improved tabular output to handle multiple sources (e.g., `~/docs (+1 more)`).
- Added unit test for config serialization and a new integration test `cli_list_test.rs`.

**Why:**

- Implements Task #34. Provides users with a quick way to view their backup configuration without requiring the daemon to be running.

**Files affected:**

- crates/backutil-lib/src/config.rs (modified)
- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_list_test.rs (new)

**Testing notes:**

- Verified tabular output with single and multiple sources.
- Verified `--json` output is valid and contains all configuration fields.
- Verified that it correctly identifies missing or invalid configuration files (exit code 2).
- All unit and integration tests passed.

---

## [2026-01-28] — bugfix: fix backup hanging issue and add timeout/no-wait flags

**What changed:**

- Fixed a critical bug in the CLI where `BufReader` was being re-instantiated on every response read, causing data loss and potential hangs.
- Improved `backutil backup` logic to correctly track and wait for all triggered backup sets.
- Fixed a hang that occurred when `backutil backup` was run with no sets configured or when all sets were already busy.
- Added `--no-wait` flag to `backutil backup` for "fire-and-forget" mode.
- Added `--timeout <SECONDS>` flag to `backutil backup` to prevent indefinite waiting.
- Added comprehensive integration tests in `crates/backutil/tests/cli_multi_test.rs` covering multi-set backups, timeout, and no-wait scenarios.

**Why:**

- Implements Task #40. Resolves reports of the CLI hanging indefinitely when multiple sets were involved or when the daemon was busy. Improves CLI UX by providing more control over backup wait times.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_multi_test.rs (new)

**Testing notes:**

- Verified that `backutil backup` (all sets) completes correctly even if some sets are busy or if no sets are configured.
- Verified that `--no-wait` returns immediately after triggering backups.
- Verified that `--timeout` correctly aborts if backups take too long.
- All integration tests passed.

## [2026-01-28] — feature: daemon status persistence on startup

**What changed:**

- Implemented `initialize_status` in the daemon's `JobManager` to query restic repositories for the latest snapshots upon startup.
- Added call to `initialize_status` during daemon initialization in `main.rs`.
- Populated `last_backup` in `SetStatus` for each backup set with existing snapshot information if available.
- Added unit test `test_initialize_status` to verify persistence after daemon restart.

**Why:**

- Implements Task #31. Ensures that the `backutil status` command displays the correct "last backup" information immediately after the daemon is started or restarted, rather than showing "Never" until the first backup occurs.

**Files affected:**

- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil-daemon/src/main.rs (modified)

**Testing notes:**

- Verified with `test_initialize_status` that `last_backup` is correctly restored from an existing restic repository after a simulated daemon restart.
- All integration tests pass.

---

## [2026-01-28] — feature: graceful removal and purge command

**What changed:**

- Implemented `backutil purge <set-name>` command to delete Restic repositories and cleanup artifacts.
- Added automatic configuration reload in the daemon when `config.toml` is modified.
- Implemented automatic unmount of backup sets when they are removed from the configuration.
- Added `ReloadConfig` IPC command for manual or programmatic configuration refresh.
- Updated `Config` struct to allow empty or missing `backup_set` list.
- Improved `JobManager` to handle dynamic addition and removal of backup jobs.

**Why:**

- Implements Task #33. Provides a safe and convenient way for users to remove backup sets and cleanup storage. Ensures the daemon remains in sync with the configuration file without requiring a restart.

**Files affected:**

- crates/backutil-lib/src/ipc.rs (modified)
- crates/backutil-lib/src/config.rs (modified)
- crates/backutil-daemon/src/main.rs (modified)
- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil/src/main.rs (modified)

**Testing notes:**

- Verified that `backutil purge` correctly prompts for confirmation and deletes repository data.
- Verified that the daemon automatically unmounts backup sets when removed from `config.toml`.
- Verified auto-reload functionality when the config file is edited.
- All workspace tests pass.

---

## [2026-01-27] — bugfix: fix false positive mount detection during uninstall

**What changed:**

- Updated `warn_if_mounts_active` to only report directories that are non-empty.

**Why:**

- The previous check reported any directory in the mount base, even if it was just an empty directory left over from a previous unmount, causing confusing warnings during `uninstall` and `disable`.

**Files affected:**

- crates/backutil/src/main.rs (modified)

**Testing notes:**

- Verified that empty directories no longer trigger the warning.
- Verified that active mounts (non-empty directories) still trigger the warning.

---

## [2026-01-27] — bugfix: fix snapshots query using deprecated --last flag

**What changed:**

- Changed `--last` to `--latest` in the restic snapshots query in `executor.rs`.

**Why:**

- Restic has deprecated `--last` flag in favor of `--latest`. The old flag was being misinterpreted as a snapshot ID prefix, causing the snapshots query to return empty results and breaking the `mount` command's interactive snapshot picker.

**Files affected:**

- crates/backutil-daemon/src/executor.rs (modified)

**Testing notes:**

- Verified mount command now correctly shows available snapshots.
- Verified mounting works end-to-end.

---

## [2026-01-27] — bugfix: fix mount command invalid --snapshot flag

**What changed:**

- Removed invalid `--snapshot` flag from restic mount command in `executor.rs`.
- Restic mount mounts the entire repository; snapshots are accessed via directory paths like `/ids/<snapshot_id>/`.

**Why:**

- `restic mount` does not have a `--snapshot` flag. The mount command was failing with "unknown flag: --snapshot".

**Files affected:**

- crates/backutil-daemon/src/executor.rs (modified)

**Testing notes:**

- Verified mount command works correctly.
- Snapshots accessible via `/ids/`, `/snapshots/`, `/hosts/`, `/tags/` directories.

---

## [2026-01-26] — feature: cli bootstrap, disable, and uninstall commands

**What changed:**

- Implemented `backutil bootstrap` to generate and enable systemd user unit.
- Implemented `backutil disable` to stop and disable the service.
- Implemented `backutil uninstall [--purge]` to remove the service and optionally purge configuration and logs.
- Added dependency checks for `restic`, `fusermount3`, and `notify-send`.
- Added integration tests for systemd-related CLI commands.

**Why:**

- Implements Task #20 and #21. Provides users with an easy way to set up and manage the background daemon.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/Cargo.toml (modified)
- crates/backutil/tests/cli_systemd_test.rs (new)

**Testing notes:**

- Verified unit file generation in a temporary directory.
- Verified manual bootstrap and uninstall (purge) functionality.
- All integration tests pass.

---

## [2026-01-26] — review: fix logs command edge cases

**What changed:**

- Fixed partial first line issue: when seeking mid-file, now skips the first (partial) line.
- Added `stdout().flush()` calls in follow mode to ensure output appears immediately.
- Fixed stale file handle issue: re-opens file after log truncation/rotation instead of just seeking.

**Why:**

- Senior code review identified that seeking to `size - 2048` could land mid-line, displaying garbage.
- Without explicit flush, `print!` output may not appear until the buffer fills.
- After log rotation, the old file handle could become stale; re-opening ensures fresh content.

**Files affected:**

- crates/backutil/src/main.rs (updated)

**Testing notes:**

- All workspace tests pass.
- Quality checks pass: cargo test, cargo fmt --check, cargo clippy.

---

## [2026-01-26] — feature: cli logs command

**What changed:**

- Implemented `backutil logs [-f]` command.
- Added logic to tail the log file, showing the last 20 lines by default.
- Implemented follow mode (`-f`) which waits for new log entries.
- Added graceful handling for missing log files (waits in follow mode, exits otherwise).

**Why:**

- Implements Task #19 and provides users with a way to monitor daemon activity and backup progress.

**Files affected:**

- crates/backutil/src/main.rs (modified)

**Testing notes:**

- All workspace tests pass.
- Verified `cargo fmt` and `cargo clippy`.

---

## [2026-01-26] — review: fix prune exit code and update spec

**What changed:**

- Fixed CLI prune command to exit with code 4 on daemon errors (restic errors) per spec.md Section 12.
- Updated spec.md to document the improved `PrunesTriggered` response format with reclaimed bytes per set.

**Why:**

- Spec compliance: prune errors are restic-related and should use exit code 4, not 1.

**Files affected:**

- spec.md (updated)
- crates/backutil/src/main.rs (updated)

---

## [2026-01-26] — feature: cli prune command

**What changed:**

- Implemented `backutil prune [SET]` command in the CLI.
- Updated `ResponseData::PrunesTriggered` in `backutil-lib` to include reclaimed space information.
- Added human-readable size formatting for reclaimed space.

**Why:**

- Implements Task #18 and provides users with a way to manually trigger repository cleanup via the CLI.

**Files affected:**

- crates/backutil-lib/src/ipc.rs (modified)
- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil/src/main.rs (modified)

---

## [2026-01-26] — feature: cli mount/unmount commands

**What changed:**

- Implemented `backutil mount <SET> [SNAPSHOT_ID]` command.
- Added interactive snapshot picker for `mount` when no ID is provided and stdout/stdin are TTYs.
- Implemented `backutil unmount [SET]` command (supports specific set or all).
- Added integration test `cli_mount_test.rs` covering mount and unmount flows.

**Why:**

- Implements Task #17 and completes the CLI's core functionality for browsing backups.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil/tests/cli_mount_test.rs (new)

---

## [2026-01-26] — feature: cli backup command

**What changed:**

- Implemented `backutil backup [SET]` command in the CLI.
- Added a broadcast mechanism to the daemon's `JobManager` for backup completion events.
- Updated the daemon's client handler to forward async broadcast events to clients.
- Added integration tests for both single-set and multi-set backup scenarios.

**Why:**

- Implements Task #16. Provides users with a way to manually trigger and monitor backups from the CLI.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil-daemon/src/main.rs (modified)
- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil/tests/cli_backup_test.rs (new)

---

## [2026-01-26] — cli: implement skeleton and status command

**What changed:**

- Implemented `backutil` CLI using `clap` with subcommands for all Phase 3 and 5 actions.
- Implemented IPC client with Unix socket connection and newline-delimited JSON protocol.
- Implemented `status` command with human-readable formatting and daemon-running check (exit code 3).

**Why:**

- Implements Task #14 and provides the entry point for all CLI interactions.
- Enables monitoring of backup set health as required by FR3.

**Files affected:**

- crates/backutil/src/main.rs (updated)
- crates/backutil/Cargo.toml (updated)

---

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
