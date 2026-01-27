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

*All Phase 2 tasks completed.*

---

## Phase 3: CLI

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

| # | Task | Branch | Completed |
|---|------|--------|-----------|
| 1 | Project scaffolding | `feature/project-scaffold` | 2026-01-24 |
| 2 | Config parsing | `feature/config-parsing` | 2026-01-25 |
| 3 | Shared types and IPC messages | `feature/shared-types-ipc` | 2026-01-25 |
| 4 | Path helpers | `feature/path-helpers` | 2026-01-25 |
| 5 | Daemon skeleton with IPC server | `feature/daemon-skeleton` | 2026-01-25 |
| 6 | File watcher | `feature/file-watcher` | 2026-01-26 |
| 7 | Debounce logic | `feature/debounce-logic` | 2026-01-26 |
| 8 | Restic executor | `feature/restic-executor` | 2026-01-26 |
| 9 | Daemon backup orchestration | `feature/daemon-orchestration` | 2026-01-26 |
| 10 | Daemon status and snapshots | `feature/daemon-status-snapshots` | 2026-01-26 |
| 11 | Daemon IPC integration test | `feature/daemon-ipc-integration-test` | 2026-01-26 |
| 12 | Daemon mount/unmount | `feature/daemon-mount-unmount` | 2026-01-26 |
| 13 | Daemon prune | `feature/daemon-prune` | 2026-01-26 |
| 14 | CLI skeleton and status command | `feature/cli-skeleton-status` | 2026-01-26 |
| 15 | CLI init command | `feature/cli-init` | 2026-01-26 |
| 16 | CLI backup command | `feature/cli-backup` | 2026-01-26 |
| 17 | CLI mount/unmount commands | `feature/cli-mount-unmount` | 2026-01-26 |
| 18 | CLI prune command | `feature/cli-prune` | 2026-01-26 |
| 19 | CLI logs command | `feature/cli-logs` | 2026-01-26 |
