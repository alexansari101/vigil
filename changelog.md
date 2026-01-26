# Changelog

**Instructions for subagents: Always add new entries at the TOP of this file, directly under the line below, in reverse chronological order (newest first).**

This file tracks changes made during development. Subagents: **read this before starting work** to understand recent changes and current project state.

For format guidelines, see `developer_guidelines.md` Section 9.

---

## [2026-01-26] — review: fix exit code on backup failure and update spec

**What changed:**

- Fixed CLI to exit with code 4 when any backup fails (per spec.md Section 12).
- Updated spec.md to document `BackupFailed` and `BackupsTriggered` response data variants.
- Updated `test_cli_backup_failure` to verify non-zero exit code on failure.

**Why:**

- Code review identified that the CLI exited with code 0 even when backups failed, violating spec and user expectations.
- The spec was missing documentation for two `ResponseData` variants that were added during implementation.

**Files affected:**

- spec.md (updated)
- crates/backutil/src/main.rs (updated)
- crates/backutil/tests/cli_backup_test.rs (updated)

**Testing notes:**

- All workspace tests pass.
- Quality checks pass: cargo test, cargo fmt --check, cargo clippy.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — review: fix cli backup command hanging on failure

**What changed:**

- Added `BackupFailed` variant to IPC protocol.
- Updated daemon to broadcast `BackupFailed` event when a backup job fails.
- Updated CLI to handle `BackupFailed` events and correctly count completions.
- Added `test_cli_backup_failure` regression test.

**Why:**

- Code review identified that `backutil backup` (especially "backup all") would hang indefinitely if a backup job failed, because the daemon only broadcasted completion on success.

**Files affected:**

- crates/backutil-lib/src/ipc.rs (updated)
- crates/backutil-daemon/src/manager.rs (updated)
- crates/backutil/src/main.rs (updated)
- crates/backutil/tests/cli_backup_test.rs (updated)

**Testing notes:**

- Added `test_cli_backup_failure` which verifies that a failed backup (invalid source) does not cause the test to hang.
- Verified existing behavior for successful backups.
- Full workspace tests pass.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — feature: cli backup command

**What changed:**

- Implemented `backutil backup [SET]` command in the CLI.
- Added a broadcast mechanism to the daemon's `JobManager` for backup completion events.
- Updated the daemon's client handler to forward async broadcast events to clients.
- Refined the CLI to wait for all triggered backups to complete before exiting.
- Added integration tests for both single-set and multi-set backup scenarios.

**Why:**

- To provide users with a way to manually trigger and monitor backups from the CLI, fulfilling a core requirement of the utility.
- To ensure the CLI provides accurate progress and completion information by listening to async updates from the daemon.

**Files affected:**

- crates/backutil/src/main.rs (modified)
- crates/backutil-daemon/src/main.rs (modified)
- crates/backutil-daemon/src/manager.rs (modified)
- crates/backutil/tests/cli_backup_test.rs (new)

**Testing notes:**

- Added integration test `cli_backup_test.rs` covering single-set and multi-set scenarios.
- Verified CLI exit codes (code 4 on restic failure).
- Verified human-readable size output.

**Dependencies/blockers:**

- Unblocks: TUI implementation for manual backup triggers.

---

## [2026-01-26] — review: fix cli init error handling and add integration test

**What changed:**

- Refactored `backutil init` to return an error if any backup set fails to initialize.
- Added `tests/cli_init_test.rs` integration test to verify initialization and idempotency.
- Added `tempfile` as a dev-dependency for `backutil`.
- Expanded "already initialized" check to handle `config file already exists` error from restic.

**Why:**

- Code review identified that failures during initialization were not propagated to the exit code.
- Automated testing was missing for the CLI init command.
- Idempotency check was brittle and failed against real restic error messages.

**Files affected:**

- crates/backutil/src/main.rs (updated)
- crates/backutil/Cargo.toml (updated)
- crates/backutil/tests/cli_init_test.rs (new)

**Testing notes:**

- Added `cli_init_test.rs` which verifies:
  - Successful initialization of a new repo.
  - Idempotency (re-running init doesn't fail).
- Verified against real restic instance via `cargo test -p backutil -- --ignored`.
- Full workspace tests pass.

**Dependencies/blockers:**

- None.

## [2026-01-26] — review: fix duration formatting and add unit tests

**What changed:**

- Fixed grammar for duration display (singular/plural).
- Added handling for negative durations ("just now").
- Added "(failed)" indicator for failed backups in status display.
- Added comprehensive unit tests for `format_human_duration`.

**Why:**

- Improve user experience and fix edge cases identified during senior code review of Task #14.

**Files affected:**

- crates/backutil/src/main.rs (updated)

**Testing notes:**

- Verified with new unit tests covering seconds, minutes, hours, days, and negative durations.
- Ran full workspace test suite.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — cli: implement skeleton and status command

**What changed:**

- Implemented `backutil` CLI using `clap` with subcommands for all Phase 3 and 5 actions.
- Implemented IPC client with Unix socket connection and newline-delimited JSON protocol.
- Implemented `status` command with human-readable formatting and daemon-running check (exit code 3).
- Added duration formatting for "Last Backup" (e.g., "5 min ago").

**Why:**

- Implements Task #14 and provides the entry point for all CLI interactions.
- Enables monitoring of backup set health as required by FR3.

**Files affected:**

- crates/backutil/src/main.rs (updated)
- crates/backutil/Cargo.toml (updated)

**Testing notes:**

- Verified `backutil status` exits with code 3 when daemon is not running.
- Verified `backutil status` displays correct set information when daemon is running with mock config.
- Verified human-readable duration formatting.

**Dependencies/blockers:**

- Unblocks: All other CLI commands (Tasks #15-#21) and TUI (Task #22).

---

## [2026-01-26] — review: fix global retention fallback for prune command

**What changed:**

- Fixed spec compliance issue: prune now falls back to global retention when per-set retention is not specified.
- Added `global_retention` field to `JobManager` and helper method `with_effective_retention()`.
- Fixed lint warnings in tests: removed useless `>= 0` comparisons for `u64` values.
- Renamed internal variable from `started` to `succeeded` for clarity in multi-prune responses.

**Why:**

- Spec.md Section 4 defines global retention as the default, with per-set overrides. The original implementation only checked per-set retention, causing prune to fail for sets relying on global config.
- This would have caused confusing errors for users who set retention globally (the default pattern shown in spec).

**Files affected:**

- crates/backutil-daemon/src/manager.rs (updated)
- crates/backutil-daemon/tests/ipc_integration_test.rs (updated)
- crates/backutil-daemon/tests/restic_test.rs (updated)

**Testing notes:**

- All workspace tests pass.
- Quality checks pass: cargo test, cargo fmt --check, cargo clippy.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — daemon: implement prune command with reclaimed space reporting

**What changed:**

- Implemented `Prune` IPC handler in `main.rs`.
- Added `prune` method to `JobManager` with single and multi-set support.
- Updated `ResticExecutor` to execute `forget --prune` and parse text output for reclaimed bytes.
- Added `PruneResult` and `PrunesTriggered` variants to IPC `ResponseData`.
- Added regex-like text parsing in `executor.rs` for restic sizes (KiB, MiB, GiB, etc.).
- Added `test_ipc_prune` integration test and updated `restic_test.rs`.

**Why:**

- Implements Task #13 and completes the daemon's core restic command set.
- Enables the upcoming `backutil prune` CLI command.
- Reclaimed space reporting provides immediate feedback on repository cleanup operations.

**Files affected:**

- crates/backutil-lib/src/ipc.rs (updated)
- crates/backutil-daemon/src/executor.rs (updated)
- crates/backutil-daemon/src/manager.rs (updated)
- crates/backutil-daemon/src/main.rs (updated)
- crates/backutil-daemon/tests/restic_test.rs (updated)
- crates/backutil-daemon/tests/ipc_integration_test.rs (updated)
- spec.md (updated)

**Testing notes:**

- Verified end-to-end flow via `test_ipc_prune`.
- Verified output parsing in `executor.rs`.
- All integration tests pass: `cargo test -p backutil-daemon -- --ignored --test-threads=1`.

---

## [2026-01-26] — review: second pass on mount/unmount edge cases

**What changed:**

- Added named constants `MOUNT_STARTUP_CHECK_MS` and `MOUNT_GRACEFUL_EXIT_TIMEOUT_SECS` to replace magic numbers.
- Set mount directory permissions to 0700 for security (sensitive backup data per PRD).
- Added documentation to `get_status()` explaining its side effects (updates mount state).
- Added warning when unmounting during an active backup to help debug potential backup failures.

**Why:**

- Second senior review identified security issue: mount directories had default permissions (potentially world-readable).
- Magic timeout values (200ms, 2s) were unexplained, reducing maintainability.
- `get_status()` function name suggested read-only behavior but actually modifies state - needs documentation.
- Unmounting during backup could cause failures without any warning to operators.

**Files affected:**

- crates/backutil-daemon/src/executor.rs (updated)
- crates/backutil-daemon/src/manager.rs (updated)

**Testing notes:**

- All workspace tests pass.
- Quality checks pass: cargo test, cargo fmt --check, cargo clippy.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — daemon: implement mount and unmount IPC handlers

**What changed:**

- Implemented `mount` and `unmount` methods in `JobManager`.
- Added support for `Mount` and `Unmount` IPC requests in daemon's `handle_client`.
- Updated `Job` struct to track `mount_process` (restic child process) and `is_mounted` state.
- Integrated `restic mount` execution via `ResticExecutor`.
- Added `test_ipc_mount_unmount` to `ipc_integration_test.rs` for end-to-end verification.
- Fixed a lint warning regarding unused variable in `Prune` handler.

**Why:**

- Implements Task #12 and FR3.
- Allows users to interactively browse backup snapshots via FUSE mounts.
- Provides the backend logic for the upcoming `backutil mount` and `backutil unmount` CLI commands.

**Files affected:**

- [manager.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/manager.rs) (updated)
- [main.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/main.rs) (updated)
- [ipc_integration_test.rs](file:///home/alex/backup_util/crates/backutil-daemon/tests/ipc_integration_test.rs) (updated)

**Testing notes:**

- Verified via `test_ipc_mount_unmount` integration test.
- Regression tested with existing unit and integration tests.
- Successfully ran `cargo test -p backutil-daemon --test ipc_integration_test -- test_ipc_mount_unmount --ignored` confirming successful repository initialization, mounting, and unmounting.

---

## [2026-01-26] — daemon: implement IPC integration tests

**What changed:**

- Created `crates/backutil-daemon/tests/ipc_integration_test.rs`.
- Implemented `TestDaemon` helper to spawn daemon with temporary config and isolated environment (XDG environment variables).
- Added test cases for `Ping`, `Status`, and `Shutdown` IPC requests.
- Verified graceful shutdown and cleanup of PID/socket files.

**Why:**

- Implements Task #11 and verifies end-to-end IPC communication between client and daemon.
- Ensures daemon lifecycle management (startup, IPC handling, shutdown) is robust.

**Files affected:**

- [ipc_integration_test.rs](file:///home/alex/backup_util/crates/backutil-daemon/tests/ipc_integration_test.rs) (new)

**Testing notes:**

- Verified via `cargo test --test ipc_integration_test`.
- Tests run in an isolated environment using temporary directories.
- Verified that `Shutdown` cleanly stops the process and removes files.

**Dependencies/blockers:**

- Unblocks: CLI skeleton implementation.

---

## [2026-01-26] — review: implement limit parameter for snapshots

**What changed:**

- Implemented missing `limit` parameter for `Snapshots` IPC request.
- Updated `ResticExecutor::snapshots()` to accept and use `limit` parameter via `--last N` flag.
- Updated `JobManager::get_snapshots()` to pass limit through to executor.
- Updated IPC handler in `main.rs` to use limit from request instead of ignoring it.
- Fixed test calls to pass `None` for limit parameter.

**Why:**

- Spec compliance: spec.md Section 5 defines `limit: int or null` for Snapshots request.
- Original implementation ignored this parameter, which would cause issues when CLI/TUI clients expect it to work.
- Allows clients to efficiently retrieve only recent snapshots instead of full history.

**Files affected:**

- [executor.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/executor.rs) (updated)
- [manager.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/manager.rs) (updated)
- [main.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/main.rs) (updated)
- [restic_test.rs](file:///home/alex/backup_util/crates/backutil-daemon/tests/restic_test.rs) (updated)

**Testing notes:**

- All existing tests pass with updated signature.
- Quality checks pass: cargo test, cargo fmt --check, cargo clippy.

---

## [2026-01-26] — daemon: implement status and snapshots IPC handlers

**What changed:**

- Implemented `get_snapshots` in `JobManager` to retrieve repository snapshots via `ResticExecutor`.
- Added support for `Status` and `Snapshots` IPC requests in `handle_client`.
- Fully integrated `SetStatus` reporting including job state, last backup, and mount status.
- Handled error paths for unknown backup sets and restic execution failures in IPC responses.

**Why:**

- Implements Task #10 and FR1/FR2.
- Essential for CLI and TUI components to display the current state and history of backup sets.

**Files affected:**

- [manager.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/manager.rs) (updated)
- [main.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/main.rs) (updated)

**Testing notes:**

- Verified that `Ping`, `Status`, and `Snapshots` requests return correct JSON responses via Unix socket.
- Verified error handling for unknown set names.
- Regression tested existing backup orchestration and debounce logic.

**Dependencies/blockers:**

- Unblocks: Task #11 (Daemon IPC integration test), Task #12 (Daemon mount/unmount), Task #14 (CLI skeleton).

---

## [2026-01-26] — daemon: implement backup orchestration and manual trigger

**What changed:**

- Implemented `trigger_backup` in `JobManager` to handle immediate backup requests.
- Updated `Job` state machine to support skipping/shortening debounce on manual trigger.
- Integrated desktop notifications for backup failures using `notify-rust`.
- Handled `Backup` IPC request in daemon's `handle_client`.
- Added `test_manual_trigger` to verify orchestration logic.
- Fixed race condition in `immediate_trigger` flag management.

**Why:**

- Implements Task #9 and connects file watcher, debounce, and restic executor.
- Provides immediate user-triggered backups alongside automated ones.
- Enhances observability through desktop notifications.

**Files affected:**

- [manager.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/manager.rs) (updated)
- [main.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/main.rs) (updated)

**Testing notes:**

- Verified via `test_manual_trigger` and `test_debounce_logic`.
- Regression tested with `test_file_watcher_to_debounce_integration` and `test_restic_workflow_integration`.
- All ignored tests pass with `cargo test -p backutil-daemon -- --ignored --test-threads=1`.

**Dependencies/blockers:**

- Unblocks: Task #10 (Daemon status and snapshots), Task #16 (CLI backup command).

---

## [2026-01-26] — test: fix integration tests for real restic execution

**What changed:**

- Updated `test_debounce_logic` and `test_file_watcher_to_debounce_integration` to properly set up isolated restic environments.
- Tests now use XDG_CONFIG_HOME/XDG_DATA_HOME env vars to avoid polluting user config.
- Added documentation noting that ignored tests must run with `--test-threads=1` due to environment variable usage.

**Why:**

- Tests were failing after ResticExecutor integration because they didn't set up password files or repositories.
- Environment variable isolation prevents tests from interfering with each other or user config.

**Files affected:**

- manager.rs (updated test)
- integration_test.rs (updated test)

**Testing notes:**

- All ignored tests pass with: `cargo test -p backutil-daemon -- --ignored --test-threads=1`
- Standard tests remain unaffected.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — review: fix safety issues in restic executor

**What changed:**

- Added safety guard in `prune()` to prevent deleting all snapshots when no retention policy is specified.
- Fixed potential panic in `job_worker()` by replacing `unwrap()` with safe pattern matching.
- Added `#[derive(Default)]` to `ResticExecutor` to satisfy clippy.

**Why:**

- Code review of Task #8 identified critical safety issue: `prune()` without retention flags would delete all snapshots.
- The `unwrap()` in job_worker could panic if job was removed during backup execution.

**Files affected:**

- executor.rs (updated)
- manager.rs (updated)

**Testing notes:**

- All workspace tests pass.
- Clippy and fmt checks pass.

**Dependencies/blockers:**

- None.

---

## [2026-01-26] — daemon: implement restic executor and integrate with manager

**What changed:**

- Implemented `ResticExecutor` in `executor.rs` for executing `init`, `backup`, `forget/prune`, `snapshots`, and `mount`.
- Added support for `--password-file` and `--json` output parsing.
- Integrated `ResticExecutor` into `JobManager` to replace placeholder backup logic.
- Implemented robust integration tests in `tests/restic_test.rs` covering the full restic workflow.
- Refactored binary/library structure in `backutil-daemon` to fix module visibility issues.

**Why:**

- Implements Task #8 from Phase 2.
- Direct execution of restic commands is the core functionality of the daemon.
- Isolated integration tests ensure reliability of the restic command mapping and output parsing.

**Files affected:**

- [executor.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/executor.rs) (new)
- [lib.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/lib.rs) (updated)
- [main.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/main.rs) (updated)
- [manager.rs](file:///home/alex/backup_util/crates/backutil-daemon/src/manager.rs) (updated)
- [restic_test.rs](file:///home/alex/backup_util/crates/backutil-daemon/tests/restic_test.rs) (new)
- [integration_test.rs](file:///home/alex/backup_util/crates/backutil-daemon/tests/integration_test.rs) (updated)

**Testing notes:**

- Added `restic_test.rs` as a robust integration suite (requires `restic`).
- Verified all restic commands (init, backup, snapshots, prune, mount) work in an isolated environment.
- Marked restic-dependent tests as `#[ignore]` to keep standard test runs clean.
- Successfully ran `cargo test -p backutil-daemon -- --ignored`.

**Dependencies/blockers:**

- Unblocks: Task #9 (Daemon backup orchestration).

---

## [2026-01-26] — daemon: implement debounce logic with JobManager

**What changed:**

- Implemented `JobManager` in `crates/backutil-daemon/src/manager.rs` to handle per-set backup jobs.
- Added debounce timer logic: file changes trigger a delay before a backup is initiated.
- Implemented state machine: `Idle` -> `Debouncing` -> `Running` -> `Idle`.
- Handled concurrent changes: if a file is changed during an active backup, a new debounce cycle starts after the current backup finishes.
- Integrated `JobManager` into `main.rs`, updating IPC `Status` to return real-time job states.
- Added comprehensive unit tests for debounce logic and state transitions.

**Why:**

- Implements Task #7 and FR1.
- Prevents resource thrashing by batching multiple rapid file changes into a single backup run.

**Files affected:**

- crates/backutil-daemon/src/manager.rs (new)
- crates/backutil-daemon/src/main.rs (updated)

**Testing notes:**

- Unit tests verify timer reset, expiration, and state transitions.
- Manual verification using temporary config and source directory confirmed correct debounce behavior and IPC status reporting.
- Verified that changes during "Running" state correctly queue a new "Debouncing" cycle.

**Dependencies/blockers:**

- Unblocks: Task #9 (Daemon backup orchestration).

---

## [2026-01-26] 5a83f42 — daemon: implement file watcher with glob filtering

**What changed:**

- Implemented `FileWatcher` using the `notify` crate.
- Added `globset` dependency for exclusion pattern support.
- Implemented recursive watching of configured source paths.
- Added robust filtering for excluded patterns (filename, absolute, and relative paths).
- Integrated watcher into the daemon's main loop via an `mpsc` channel.
- Added unit tests for filtering logic in `watcher.rs`.

**Why:**

- Implements Task #6 and Section 4 of `spec.md`.
- Foundation for automated backups triggered by file changes.

**Files affected:**

- crates/backutil-daemon/src/watcher.rs (new)
- crates/backutil-daemon/src/main.rs (updated)
- crates/backutil-daemon/src/Cargo.toml (updated)
- Cargo.toml (updated)

**Testing notes:**

- Unit tests verify filtering of `*.tmp` and directory-based exclusions.
- Manual verification: daemon detects file changes in real-time.
- Verified that directory creation/deletion events are correctly ignored.

**Dependencies/blockers:**

- Added `globset`.
- Unblocks: Task #7 (Debounce logic).

---

## [2026-01-25] c8e7f76 — daemon: implement skeleton with IPC server and graceful shutdown

**What changed:**

- Implemented `Daemon` struct with PID file management and Unix socket lifecycle.
- Added line-based JSON IPC server handling `Ping` and `Status` (placeholder).
- Implemented graceful shutdown on `SIGTERM` and `SIGINT` with proper cleanup.
- Added unit tests for PID file management and manual verification for IPC.
- Updated workspace dependencies to include `tracing-subscriber`.

**Why:**

- Implements Task #5 and core infrastructure for the daemon.
- Enables basic health checking and IPC foundation for CLI/TUI.

**Files affected:**

- crates/backutil-daemon/src/main.rs (updated)
- crates/backutil-daemon/Cargo.toml (updated)
- Cargo.toml (updated)

**Testing notes:**

- Unit tests for PID file creation and cleanup.
- Manual verification: daemon starts, creates PID/socket, responds to `Ping` via `nc -U`, and shuts down cleanly on `SIGTERM`.
- Verified PID file prevents multiple instances.

**Dependencies/blockers:**

- Unblocks: Task #6 (File watcher), Task #8 (Restic executor), Task #13 (CLI status).

---

## [2026-01-25] f037857 — lib: implement path helpers

**What changed:**

- Implemented standard path helper functions in `crates/backutil-lib/src/paths.rs`.
- Added functions for: `config_dir`, `config_path`, `password_path`, `log_path`, `socket_path`, `pid_path`, `mount_base_dir`, `mount_path`, and `systemd_unit_path`.
- Added `libc` dependency to correctly handle UID-based fallbacks for Unix socket and PID files.
- Exported `paths` module in `backutil-lib`.
- Added comprehensive unit tests for all path functions.

**Why:**

- Implements Task #4 and Section 3 of `spec.md`.
- Provides a centralized, consistent way to handle project paths across all components.

**Files affected:**

- Cargo.toml (updated)
- crates/backutil-lib/Cargo.toml (updated)
- crates/backutil-lib/src/paths.rs (new)
- crates/backutil-lib/src/lib.rs (updated)

**Testing notes:**

- Added unit tests in `paths.rs` verifying each path's structure and suffix.
- Verified that all tests pass with `cargo test -p backutil-lib`.

**Dependencies/blockers:**

- Unblocks: Task #5 (Daemon skeleton), Task #8 (Restic executor), Task #18 (CLI logs).

---

## [2026-01-25] c98161a — lib: implement shared types and IPC messages

**What changed:**

- Defined shared job and status types: `JobState`, `SetStatus`, `BackupResult`, `SnapshotInfo`.
- Defined IPC communication protocol: `Request`, `Response`, `ResponseData`.
- Enabled `serde` feature for `chrono` in workspace.
- Implemented round-trip JSON serialization tests for all IPC messages.

**Why:**

- Implements Task #3 and Section 5-6 of `spec.md`.
- Provides the communication contract between daemon and CLI/TUI.

**Files affected:**

- Cargo.toml (updated)
- crates/backutil-lib/src/types.rs (new)
- crates/backutil-lib/src/ipc.rs (new)
- crates/backutil-lib/src/lib.rs (updated)

**Testing notes:**

- Added comprehensive unit tests for serialization/deserialization of each request and response variant.
- Verified that all tests pass with `cargo test -p backutil-lib`.

**Dependencies/blockers:**

- Unblocks: Task #5 (Daemon skeleton), Task #10 (Daemon status/snapshots).

---

## [2026-01-25] 1783da7 — lib: add config parsing

**What changed:**

- Added `Config`, `BackupSet`, `RetentionPolicy` structs.
- Implemented TOML parsing with validation.
- Added `load_config()` with `BACKUTIL_CONFIG` env var support.
- Implemented path expansion for `~`.
- Added `tempfile` for testing.

**Why:**

- Implements Task #2 and FR4.
- Essential foundation for daemon and CLI.

**Files affected:**

- crates/backutil-lib/src/config.rs (new)
- crates/backutil-lib/src/lib.rs (updated)
- crates/backutil-lib/Cargo.toml (updated)

**Testing notes:**

- Unit tests for validation (duplicates, mutually exclusive sources).
- Unit tests for path expansion.
- Integrated test for `load_config` with temporary files.

**Dependencies/blockers:**

- Unblocks: Task #6 (File watcher), Task #14 (CLI init).

---

## [2026-01-25] 026eec9 — project: initial scaffolding

**What changed:**

- Initialized Cargo workspace with three crates: `backutil-lib`, `backutil-daemon`, and `backutil`.
- Configured workspace dependencies in root `Cargo.toml`.
- Created placeholder `lib.rs` and `main.rs` for each crate.
- Added preliminary configuration and IPC modules to `backutil-lib`.

**Why:**

- Implements FR4 (Unified Configuration) foundation and Task #1 from `task_list.md`.
- Required to start development on daemon and CLI components.

**Files affected:**

- Cargo.toml (new)
- crates/backutil-lib/Cargo.toml (new)
- crates/backutil-lib/src/lib.rs (new)
- crates/backutil-daemon/Cargo.toml (new)
- crates/backutil-daemon/src/main.rs (new)
- crates/backutil/Cargo.toml (new)
- crates/backutil/src/main.rs (new)

**Testing notes:**

- Verified workspace builds with `cargo build`.
- Ran `cargo test` and ensured all tests (including new placeholder tests) pass.
- Verified `cargo clippy` and `cargo fmt` pass without warnings.

**Dependencies/blockers:**

- Unblocks: Task #2 (Config parsing), Task #3 (Shared types), Task #5 (Daemon skeleton).

---

## [2024-01-24] — Task list creation

**What changed:**

- Created task_list.md with 28 implementation tasks across 5 phases
- Phase 1 (Foundation): scaffolding, config, types, paths
- Phase 2 (Daemon Core): IPC server, watcher, debounce, restic execution
- Phase 3 (CLI): all 8 CLI commands
- Phase 4 (TUI): layout, live updates, sparklines, interactivity
- Phase 5 (Polish): error messages, logging, e2e tests, docs
- Each task includes acceptance criteria and dependency markers

**Why:**

- Enable parallel work by multiple agents
- Clear dependencies prevent agents from getting blocked
- Acceptance criteria make completion verifiable

**Files affected:**

- task_list.md (new)

**Testing notes:**

- N/A (documentation only)

**Dependencies/blockers:**

- Unblocks: Task #1 (Project scaffolding) can begin immediately

---

## [2024-01-24] — Developer guidelines enhancements

**What changed:**

- Added Section 0: Subagent Workflow with explicit 7-step process
- Strengthened testing requirements: must write tests for new features, must manually verify, must leave system in working state after each commit
- Added Section 9: Changelog format with template and example entry
- Updated pre-commit checklist with new verification steps

**Why:**

- Ensure subagents follow consistent workflow
- Prevent broken builds and partial features from being committed
- Changelog provides context for agents starting new tasks

**Files affected:**

- developer_guidelines.md (added Sections 0 and 9, updated Section 8)
- changelog.md (new)

**Testing notes:**

- N/A (documentation only)

**Dependencies/blockers:**

- None

---

## [2024-01-24] — Spec revision: reduce over-specification

**What changed:**

- Removed exact project directory structure from spec.md Section 1; replaced with component responsibility table
- Simplified dependencies from pinned Cargo.toml to suggested crate names table
- Converted Rust struct definitions to language-agnostic field descriptions (Sections 4, 5, 6)
- Marked systemd unit template as example rather than exact specification

**Why:**

- Allow implementation flexibility while preserving essential contracts
- Spec should define "what" (interfaces, protocols), not "how" (code structure)
- IPC protocol and config schema remain precise for component compatibility

**Files affected:**

- spec.md (revised Sections 1, 2, 4, 5, 6, 8)

**Testing notes:**

- N/A (documentation only)

**Dependencies/blockers:**

- None

---

## [2024-01-24] — Project initialization

**What changed:**

- Created `prd.md` with product requirements
- Created `spec.md` with technical specifications
- Created `developer_guidelines.md` with coding standards and workflow

**Why:**

- Establish foundation documents for subagent implementation

**Files affected:**

- prd.md (new)
- spec.md (new)
- developer_guidelines.md (new)
- changelog.md (new)

**Testing notes:**

- N/A (documentation only)

**Dependencies/blockers:**

- Unblocks: All implementation work
