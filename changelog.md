# Changelog

This file tracks changes made during development. Subagents: **read this before starting work** to understand recent changes and current project state.

For format guidelines, see `developer_guidelines.md` Section 9.

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
- crates/backutil-daemon/Cargo.toml (updated)
- Cargo.toml (updated)

**Testing notes:**

- Unit tests verify filtering of `*.tmp` and directory-based exclusions.
- Manual verification: daemon detects file changes in real-time.
- Verified that directory creation/deletion events are correctly ignored.

**Dependencies/blockers:**

- Added `globset`.
- Unblocks: Task #7 (Debounce logic).

---

## [2026-01-26] 45a5a30 — daemon: implement debounce logic with JobManager

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
