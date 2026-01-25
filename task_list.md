# Task List

## Instructions for Agents

1. **Read first:** `prd.md`, `spec.md`, `developer_guidelines.md`, `changelog.md`
2. **Claim a task:** Take the topmost `[ ]` task that is not blocked
3. **Create a feature branch:** `git checkout -b feature/<task-short-name>`
4. **Implement, test, verify** per `developer_guidelines.md` Section 0
5. **Merge to main:** Only after all tests pass and regression testing is complete
6. **Update this file:** Mark task `[x]` and note your branch name
7. **Update changelog.md:** Add entry describing your changes

**Parallel work:** Multiple agents may work simultaneously on unblocked tasks. Communicate via changelog.md and commit messages. If you encounter a merge conflict, resolve it carefully and re-run all tests.

**Blocked tasks:** Tasks marked `[BLOCKED BY: #N]` cannot start until task N is complete and merged to main.

---

## Phase 1: Foundation

### 1. [ ] Project scaffolding
Create Cargo workspace with crate structure. Set up shared dependencies. Ensure `cargo build` and `cargo test` pass (with no-op tests).

**Acceptance criteria:**
- Workspace compiles with no errors
- Each crate has a placeholder `lib.rs` or `main.rs`
- CI-ready: `cargo fmt --check` and `cargo clippy` pass

---

### 2. [ ] Config parsing
Implement config loading in shared library. Parse TOML per spec.md Section 4. Handle path expansion (`~`), validation, and defaults.

**Acceptance criteria:**
- Loads config from default path and `BACKUTIL_CONFIG` env var
- Validates required fields, returns clear errors for invalid configs
- Unit tests for: valid config, missing fields, path expansion, multi-source mode
- `[BLOCKED BY: #1]`

---

### 3. [ ] Shared types and IPC messages
Define all types from spec.md Sections 5-6 in shared library. Implement JSON serialization/deserialization.

**Acceptance criteria:**
- All IPC request/response types defined
- All shared types (JobState, SetStatus, BackupResult, SnapshotInfo) defined
- Round-trip serialization tests for each message type
- `[BLOCKED BY: #1]`

---

### 4. [ ] Path helpers
Implement canonical path functions in shared library: config path, password path, log path, socket path, mount paths.

**Acceptance criteria:**
- Functions for all paths in spec.md Section 3
- Respects XDG directories with fallbacks
- Unit tests for each path function
- `[BLOCKED BY: #1]`

---

## Phase 2: Daemon Core

### 5. [ ] Daemon skeleton with IPC server
Create daemon binary. Implement Unix socket server that accepts connections and parses IPC messages. Respond to `Ping` with `Pong`.

**Acceptance criteria:**
- Daemon starts and listens on socket path
- Creates PID file, removes on shutdown
- Handles `Ping` request, returns `Pong`
- Graceful shutdown on SIGTERM
- `[BLOCKED BY: #3, #4]`

---

### 6. [ ] File watcher
Implement inotify-based directory watching. Detect file creates, modifies, deletes in configured source directories.

**Acceptance criteria:**
- Watches all source paths from config
- Emits internal events on file changes
- Ignores excluded patterns
- Handles watch errors gracefully (e.g., directory doesn't exist)
- `[BLOCKED BY: #2, #5]`

---

### 7. [ ] Debounce logic
Implement debounce timer per backup set. Reset timer on each file change. Trigger backup when timer expires.

**Acceptance criteria:**
- Per-set debounce with configurable delay
- Timer resets on new changes during debounce
- State transitions: Idle → Debouncing → Running (per spec.md Section 7)
- Unit tests for timer reset behavior
- `[BLOCKED BY: #6]`

---

### 8. [ ] Restic executor
Implement restic command execution. Support: init, backup, forget/prune, snapshots (JSON), mount.

**Acceptance criteria:**
- Executes restic with correct arguments per spec.md Section 9
- Passes password via `--password-file`
- Parses JSON output from `restic snapshots`
- Captures and logs stderr on failure
- Integration tests (marked `#[ignore]`) that require restic installed
- `[BLOCKED BY: #4]`

---

### 9. [ ] Daemon backup orchestration
Connect watcher → debounce → executor. Handle `Backup` IPC request. Track job state per set. Send desktop notification on failure.

**Acceptance criteria:**
- File change triggers debounced backup
- `Backup` IPC request triggers immediate backup
- Concurrent backup requests for same set are queued/rejected
- Sends `notify-send` on backup failure
- `[BLOCKED BY: #7, #8]`

---

### 10. [ ] Daemon status and snapshots
Implement `Status` and `Snapshots` IPC handlers. Return current state of all backup sets.

**Acceptance criteria:**
- `Status` returns list of SetStatus for all configured sets
- `Snapshots` returns list of SnapshotInfo for specified set
- Includes last backup result, current state, mount status
- `[BLOCKED BY: #9]`

---

### 11. [ ] Daemon mount/unmount
Implement `Mount` and `Unmount` IPC handlers. Spawn restic mount process, track mount state.

**Acceptance criteria:**
- `Mount` starts restic mount in background, returns mount path
- `Unmount` kills mount process, cleans up
- Tracks which sets are currently mounted
- Handles "already mounted" and "not mounted" cases
- `[BLOCKED BY: #8, #10]`

---

### 12. [ ] Daemon prune
Implement `Prune` IPC handler. Run retention policy cleanup.

**Acceptance criteria:**
- Runs `restic forget --prune` with configured retention flags
- Supports per-set retention override
- Logs bytes reclaimed
- `[BLOCKED BY: #8]`

---

## Phase 3: CLI

### 13. [ ] CLI skeleton and status command
Create CLI binary with clap. Implement `backutil status` command that connects to daemon and displays set status.

**Acceptance criteria:**
- `backutil status` shows all sets with state, last backup time, mount status
- Human-readable output (e.g., "5 min ago" not timestamps)
- Exits with code 3 if daemon not running
- `[BLOCKED BY: #10]`

---

### 14. [ ] CLI init command
Implement `backutil init [SET]` to initialize restic repository.

**Acceptance criteria:**
- Prompts for password if password file doesn't exist
- Creates password file with mode 600
- Runs `restic init` for specified set or all sets
- Clear error if repository already initialized
- `[BLOCKED BY: #8, #13]`

---

### 15. [ ] CLI backup command
Implement `backutil backup [SET]` to trigger immediate backup via daemon.

**Acceptance criteria:**
- Sends `Backup` request to daemon
- Shows progress/completion message
- Exits with code 4 on restic error
- `[BLOCKED BY: #9, #13]`

---

### 16. [ ] CLI mount/unmount commands
Implement `backutil mount <SET>` and `backutil unmount [SET]`.

**Acceptance criteria:**
- `mount` shows interactive snapshot picker (or uses latest)
- Prints mount path on success
- `unmount` with no args unmounts all
- Clear error messages for edge cases
- `[BLOCKED BY: #11, #13]`

---

### 17. [ ] CLI prune command
Implement `backutil prune [SET]`.

**Acceptance criteria:**
- Triggers prune via daemon
- Shows summary of space reclaimed
- `[BLOCKED BY: #12, #13]`

---

### 18. [ ] CLI logs command
Implement `backutil logs` to tail the log file.

**Acceptance criteria:**
- Tails `~/.local/share/backutil/backutil.log`
- Supports `-f` for follow mode
- Graceful handling if log doesn't exist
- `[BLOCKED BY: #4, #13]`

---

### 19. [ ] CLI bootstrap command
Implement `backutil bootstrap` to generate and enable systemd user unit.

**Acceptance criteria:**
- Generates unit file per spec.md Section 8
- Runs `systemctl --user daemon-reload`
- Enables and starts the service
- Checks for missing dependencies (restic, fusermount3, notify-send)
- `[BLOCKED BY: #13]`

---

### 20. [ ] CLI disable/uninstall commands
Implement `backutil disable` and `backutil uninstall [--purge]`.

**Acceptance criteria:**
- `disable` stops and disables systemd unit
- `uninstall` removes systemd unit
- `uninstall --purge` also removes config, logs, password file
- Warns if mounts are active
- `[BLOCKED BY: #19]`

---

## Phase 4: TUI

### 21. [ ] TUI basic layout
Implement TUI with ratatui per spec.md Section 11. Header, job list, footer with keybindings.

**Acceptance criteria:**
- Renders header with app name and global status
- Renders list of backup sets with basic info
- Renders footer with keybinding hints
- Keyboard: `q` quits
- `[BLOCKED BY: #13]`

---

### 22. [ ] TUI live status updates
Poll daemon for status, update display. Show job state, debounce countdown, last backup time.

**Acceptance criteria:**
- Polls status every 1 second
- Shows state indicators (Idle/Debouncing/Running/Error)
- Shows debounce countdown when applicable
- Shows "Last: X ago" with human-readable time
- `[BLOCKED BY: #10, #21]`

---

### 23. [ ] TUI sparklines
Add sparkline visualization of recent backup durations.

**Acceptance criteria:**
- Shows last 5 backup durations as sparkline
- Scales appropriately
- Handles sets with <5 backups gracefully
- `[BLOCKED BY: #22]`

---

### 24. [ ] TUI interactive commands
Implement keybindings: `b` backup all, `p` prune, `m` mount, `s` snapshots, `?` help modal.

**Acceptance criteria:**
- Each keybinding triggers appropriate action
- Non-blocking: TUI remains responsive during operations
- `?` shows overlay modal with command list
- Warn on quit if mounts are active
- `[BLOCKED BY: #22]`

---

## Phase 5: Polish

### 25. [ ] Error message improvements
Review all error paths. Ensure user-facing errors include what/why/how-to-fix per spec.md Section 10.

**Acceptance criteria:**
- Audit CLI and TUI error output
- No raw error messages shown to users
- Actionable suggestions for common errors
- `[BLOCKED BY: #24]`

---

### 26. [ ] Logging and observability
Ensure daemon logs all significant events. Implement log rotation or size limits.

**Acceptance criteria:**
- Logs backup start/complete/fail with set name
- Logs file watch events at debug level
- Log file doesn't grow unbounded
- `[BLOCKED BY: #9]`

---

### 27. [ ] End-to-end integration tests
Create integration test suite that exercises full workflow: init → backup → mount → restore → prune.

**Acceptance criteria:**
- Tests run with temporary directories and config
- Tests marked `#[ignore]` (require restic)
- CI can run tests with restic installed
- `[BLOCKED BY: #24]`

---

### 28. [ ] Documentation and README
Write user-facing README with installation, quick start, and configuration examples.

**Acceptance criteria:**
- Installation instructions (cargo install, dependencies)
- Quick start guide
- Configuration reference with examples
- Troubleshooting section
- `[BLOCKED BY: #27]`

---

## Completed Tasks

<!-- Move completed tasks here with branch name and completion date -->
<!-- Example:
### 1. [x] Project scaffolding
Branch: `feature/project-scaffold`
Completed: 2024-01-15
-->
