# Task List

## Instructions for Agents

1. **Read first:** `prd.md`, `spec.md`, `developer_guidelines.md`, `changelog.md`
2. **Claim a task:** Take the topmost `[ ]` task that is not blocked and claim it by marking it `[x]`.
3. **Create a feature branch:** `git checkout -b feature/<task-short-name>`
4. **Implement, test, verify** per `developer_guidelines.md` Section 0
5. **Update changelog.md:** Add entry describing your changes
6. **Update this file:** Remove the task from the list and add it to the completed tasks at the bottom of the list. Match the format of other completed tasks.
7. **Merge to main:** Only after all tests pass and regression testing is complete and the pre-commit checklist has been completed per `developer_guidelines.md` Section 8.
8. **Amend the commit** Update the "completed tasks" section of this file with the short commit id of parent commit.

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

*All Phase 3 tasks completed.*

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
- Proactive check: prune should fail with clear message if set is mounted (not restic lock error)
- `[BLOCKED BY: #25]`

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

## Phase 6: CLI UX Polish

---

---

### 50. [ ] Improved `status` Offline Experience

Make the `status` command useful even when the daemon is not running.

**Acceptance criteria:**

- If daemon is down, print `Service: Offline` and show a list of all configured sets from the local `config.toml`.
- Provide an actionable hint: `Run backutil service install to start the service.`
- `[BLOCKED BY: #47]`

---

## Completed Tasks

| # | Task | Commit | Completed |
| --- | --- | --- | --- |
| 1 | Project scaffolding | `b21674c` | 2026-01-24 |
| 2 | Config parsing | `eab85e0` | 2026-01-25 |
| 3 | Shared types and IPC messages | `62e44a9` | 2026-01-25 |
| 4 | Path helpers | `f037857` | 2026-01-25 |
| 5 | Daemon skeleton with IPC server | `c8e7f76` | 2026-01-25 |
| 6 | File watcher | `5a83f42` | 2026-01-26 |
| 7 | Debounce logic | `8f043eb` | 2026-01-26 |
| 8 | Restic executor | `3ab7da4` | 2026-01-26 |
| 9 | Daemon backup orchestration | `cb1a598` | 2026-01-26 |
| 10 | Daemon status and snapshots | `ba339d4` | 2026-01-26 |
| 11 | Daemon IPC integration test | `7b87efb` | 2026-01-26 |
| 12 | Daemon mount/unmount | `2621a60` | 2026-01-26 |
| 13 | Daemon prune | `77692f4` | 2026-01-26 |
| 14 | CLI skeleton and status command | `f70df4a` | 2026-01-26 |
| 15 | CLI init command | `b35e580` | 2026-01-26 |
| 16 | CLI backup command | `d3cd551` | 2026-01-26 |
| 17 | CLI mount/unmount commands | `4fe32da` | 2026-01-26 |
| 18 | CLI prune command | `143fb2d` | 2026-01-26 |
| 19 | CLI logs command | `762c0fb` | 2026-01-26 |
| 20 | CLI bootstrap command | `060050c` | 2026-01-26 |
| 21 | CLI disable/uninstall commands | `060050c` | 2026-01-26 |
| 33 | Graceful backup set removal / purge command | `16bd0ef` | 2026-01-28 |
| 31 | Daemon status persistence on startup | `3ea6303` | 2026-01-28 |
| 40 | Fix backup all sets timeout/hanging issue | `61c1b94` | 2026-01-28 |
| 34 | CLI list command | `4ee25b1` | 2026-01-28 |
| 37 | Use short_id in CLI output | `review` | 2026-01-28 |
| 35 | CLI snapshots command | `df97124` | 2026-01-28 |
| 36 | CLI `check` command | `1f921a0` | 2026-01-29 |
| 38 | Plain English help text | `c65fccd` | 2026-01-29 |
| 39 | Global `--quiet` and `--json` flags | `abcc197` | 2026-01-29 |
| 32 | Enhanced status output with storage metrics | `231cd49` | 2026-01-30 |
| 27 | Robust Logging and clean output | `5a20702` | 2026-01-30 |
| 41 | Fix test_cli_mount_unmount deterministic failure | `a4f4abc` | 2026-01-30 |
| 42 | Fix flaky daemon manager tests caused by shared env vars | `f84afbc` | 2026-01-30 |
| 43 | Fix status update and file watcher issues | `265ebea` | 2026-01-31 |
| 44 | Address user testing feedback: check error hints and logs output | `8e90c6a` | 2026-01-31 |
| 45 | Fix log selection logic to prefer most recent file by mtime | `0292cf4` | 2026-01-30 |
| 46 | Sync mount status on daemon restart | `2e0f85d` | 2026-01-31 |
| 47 | Command Grouping: `service` subcommand | `744e025` | 2026-01-31 |
| 48 | Guided Onboarding: `backutil setup` | `d819fdd` | 2026-01-31 |
| 49 | Config Management: `track` and `untrack` | `3a60602` | 2026-01-31 |
