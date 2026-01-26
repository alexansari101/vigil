# Task List

## Instructions for Agents

1. **Read first:** `prd.md`, `spec.md`, `developer_guidelines.md`, `changelog.md`
2. **Claim a task:** Take the topmost `[ ]` task that is not blocked
3. **Create a feature branch:** `git checkout -b feature/<task-short-name>`
4. **Implement, test, verify** per `developer_guidelines.md` Section 0
5. **Update changelog.md:** Add entry describing your changes
6. **Update this file:** Mark task `[x]` and note your branch name. Move to the end of the list matching the format of other completed tasks.
7. **Merge to main:** Only after all tests pass and regression testing is complete and the pre-commit checklist has been completed per `developer_guidelines.md` Section 8.

**Parallel work:** Multiple agents may work simultaneously on unblocked tasks. Communicate via changelog.md and commit messages. If you encounter a merge conflict, resolve it carefully and re-run all tests.

**Blocked tasks:** Tasks marked `[BLOCKED BY: #N]` cannot start until task N is complete and merged to main.

---

## Phase 1: Foundation

*All Phase 1 tasks completed.*

---

## Phase 2: Daemon Core

---

---

### 10. [ ] Daemon status and snapshots

Implement `Status` and `Snapshots` IPC handlers. Return current state of all backup sets.

**Acceptance criteria:**

- `Status` returns list of SetStatus for all configured sets
- `Snapshots` returns list of SnapshotInfo for specified set
- Includes last backup result, current state, mount status
- `[BLOCKED BY: #9]`

---

### 11. [ ] Daemon IPC integration test

Create integration test that starts actual daemon, sends IPC commands via Unix socket, verifies responses and shutdown.

**Acceptance criteria:**

- Test spawns daemon as subprocess with temp config
- Sends IPC requests over Unix socket (Ping, Status, Shutdown)
- Verifies correct responses and deserialization
- Tests daemon graceful shutdown
- Cleans up daemon process and temp files on test completion
- `[BLOCKED BY: #10]`

---

### 12. [ ] Daemon mount/unmount

Implement `Mount` and `Unmount` IPC handlers. Spawn restic mount process, track mount state.

**Acceptance criteria:**

- `Mount` starts restic mount in background, returns mount path
- `Unmount` kills restic mount process cleanly
- Mount status tracked and returned by `Status` handler
- `[BLOCKED BY: #8, #10]`

---

### 13. [ ] Daemon prune

Implement `Prune` IPC handler. Run retention policy cleanup.

**Acceptance criteria:**

- Runs `restic forget --prune` with configured retention flags
- Supports per-set retention override
- Logs bytes reclaimed
- `[BLOCKED BY: #8]`

---

## Phase 3: CLI

### 14. [ ] CLI skeleton and status command

Create CLI binary with clap. Implement `backutil status` command that connects to daemon and displays set status.

**Acceptance criteria:**

- `backutil status` shows all sets with state, last backup time, mount status
- Human-readable output (e.g., "5 min ago" not timestamps)
- Exits with code 3 if daemon not running
- `[BLOCKED BY: #10]`

---

### 15. [ ] CLI init command

Implement `backutil init [SET]` to initialize restic repository.

**Acceptance criteria:**

- Prompts for password if password file doesn't exist
- Creates password file with mode 600
- Runs `restic init` for specified set or all sets
- Clear error if repository already initialized
- `[BLOCKED BY: #8, #14]`

---

### 16. [ ] CLI backup command

Implement `backutil backup [SET]` to trigger immediate backup via daemon.

**Acceptance criteria:**

- Sends `Backup` request to daemon
- Shows progress/completion message
- Exits with code 4 on restic error
- `[BLOCKED BY: #9, #14]`

---

### 17. [ ] CLI mount/unmount commands

Implement `backutil mount <SET>` and `backutil unmount [SET]`.

**Acceptance criteria:**

- `mount` shows interactive snapshot picker (or uses latest)
- Prints mount path on success
- `unmount` with no args unmounts all
- Clear error messages for edge cases
- `[BLOCKED BY: #12, #14]`

---

### 18. [ ] CLI prune command

Implement `backutil prune [SET]`.

**Acceptance criteria:**

- Triggers prune via daemon
- Shows summary of space reclaimed
- `[BLOCKED BY: #13, #14]`

---

### 19. [ ] CLI logs command

Implement `backutil logs` to tail the log file.

**Acceptance criteria:**

- Tails `~/.local/share/backutil/backutil.log`
- Supports `-f` for follow mode
- Graceful handling if log doesn't exist
- `[BLOCKED BY: #4, #14]`

---

### 20. [ ] CLI bootstrap command

Implement `backutil bootstrap` to generate and enable systemd user unit.

**Acceptance criteria:**

- Generates unit file per spec.md Section 8
- Runs `systemctl --user daemon-reload`
- Enables and starts the service
- Checks for missing dependencies (restic, fusermount3, notify-send)
- `[BLOCKED BY: #14]`

---

### 21. [ ] CLI disable/uninstall commands

Implement `backutil disable` and `backutil uninstall [--purge]`.

**Acceptance criteria:**

- `disable` stops and disables systemd unit
- `uninstall` removes systemd unit
- `uninstall --purge` also removes config, logs, password file
- Warns if mounts are active
- `[BLOCKED BY: #20]`

---

## Phase 4: TUI

### 22. [ ] TUI basic layout

Implement TUI with ratatui per spec.md Section 11. Header, job list, footer with keybindings.

**Acceptance criteria:**

- Renders header with app name and global status
- Renders list of backup sets with basic info
- Renders footer with keybinding hints
- Keyboard: `q` quits
- `[BLOCKED BY: #14]`

---

### 23. [ ] TUI live status updates

Poll daemon for status, update display. Show job state, debounce countdown, last backup time.

**Acceptance criteria:**

- Polls status every 1 second
- Shows state indicators (Idle/Debouncing/Running/Error)
- Shows debounce countdown when applicable
- Shows "Last: X ago" with human-readable time
- `[BLOCKED BY: #10, #22]`

---

### 24. [ ] TUI sparklines

Add sparkline visualization of recent backup durations.

**Acceptance criteria:**

- Shows last 5 backup durations as sparkline
- Scales appropriately
- Handles sets with <5 backups gracefully
- `[BLOCKED BY: #23]`

---

### 25. [ ] TUI interactive commands

Implement keybindings: `b` backup all, `p` prune, `m` mount, `s` snapshots, `?` help modal.

**Acceptance criteria:**

- Each keybinding triggers appropriate action
- Non-blocking: TUI remains responsive during operations
- `?` shows overlay modal with command list
- Warn on quit if mounts are active
- `[BLOCKED BY: #23]`

---

## Phase 5: Polish

### 26. [ ] Error message improvements

Review all error paths. Ensure user-facing errors include what/why/how-to-fix per spec.md Section 10.

**Acceptance criteria:**

- Audit CLI and TUI error output
- No raw error messages shown to users
- Actionable suggestions for common errors
- `[BLOCKED BY: #25]`

---

### 27. [ ] Logging and observability

Ensure daemon logs all significant events. Implement log rotation or size limits.

**Acceptance criteria:**

- All significant events logged (backup start/complete, state changes)
- Logs rotated or size-limited to prevent unbounded growth
- Graceful shutdown cancels in-flight worker tasks (debounce/backup)
- `[BLOCKED BY: #1-#25]`

---

### 28. [ ] Polish and final testing

Performance audit, documentation review, and final integration checks.

**Acceptance criteria:**

- Complete manual run of all CLI commands
- Verify TUI remains smooth under high log volume
- Audit code for any remaining TODOs or placeholders
- `[BLOCKED BY: #27]`

---

### 29. [ ] End-to-end integration tests

Create integration test suite that exercises full workflow: init → backup → mount → restore → prune.

**Acceptance criteria:**

- Tests run with temporary directories and config
- Tests marked `#[ignore]` (require restic)
- CI can run tests with restic installed
- `[BLOCKED BY: #28]`

---

### 30. [ ] Documentation and README

Write user-facing README with installation, quick start, and configuration examples.

**Acceptance criteria:**

- Installation instructions (cargo install, dependencies)
- Quick start guide
- Configuration reference with examples
- Troubleshooting section
- `[BLOCKED BY: #29]`

---

## Completed Tasks

### 1. [x] Project scaffolding

Branch: `feature/project-scaffold`
Completed: 2026-01-24

### 2. [x] Config parsing

Branch: `feature/config-parsing`
Completed: 2026-01-25

### 3. [x] Shared types and IPC messages

Branch: `feature/shared-types-ipc`
Completed: 2026-01-25

### 4. [x] Path helpers

Branch: `feature/path-helpers`
Completed: 2026-01-25

### 5. [x] Daemon skeleton with IPC server

Branch: `feature/daemon-skeleton`
Completed: 2026-01-25

### 6. [x] File watcher

Branch: `feature/file-watcher`
Completed: 2026-01-26

### 7. [x] Debounce logic

Branch: `feature/debounce-logic`
Completed: 2026-01-26

### 8. [x] Restic executor

Branch: `feature/restic-executor`
Completed: 2026-01-26

### 9. [x] Daemon backup orchestration

Branch: `feature/daemon-orchestration`
Completed: 2026-01-26
