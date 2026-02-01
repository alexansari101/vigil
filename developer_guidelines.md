# Developer Guidelines for Subagents

This document defines development practices, cleanup requirements, and coordination patterns for agents working on `vigil`.

## 0. Subagent Workflow

**Every subagent must follow this workflow for each task:**

```
1. READ CONTEXT
   ├── prd.md                 # Understand the product requirements
   ├── spec.md                # Understand technical specifications
   └── changelog.md           # Review recent changes and current state

2. CLAIM TASK
   └── Select an item from the feature list (task list)

3. IMPLEMENT
   ├── Write code following the coding standards in this document
   └── Write tests for all new functionality (unit tests, integration tests as appropriate)

4. TEST & VERIFY
   ├── Run `cargo test` — all tests must pass
   ├── Manually verify new features work end-to-end (CLI commands, daemon behavior, TUI interactions)
   ├── Manually verify ALL existing features still work (regression testing)
   └── Each commit must leave the system in a fully working state — no broken builds, no partial features

5. COMMIT
   └── Commit with message format: `<component>: <short description>`

6. UPDATE CHANGELOG
   └── Append a summary of changes to changelog.md (see Section 9)

7. MARK COMPLETE
   └── Mark the feature as complete and move to the end of the task list matching the format of other completed tasks.
```

**Do not skip steps.** Reading the changelog before starting prevents duplicate work and conflicting implementations.

## 1. Cleanup Requirements

Before completing any development or testing task, ensure the system is left in a clean state. Remove all installed artifacts:

**Systemd units:**

```bash
systemctl --user stop vigil-daemon.service
systemctl --user disable vigil-daemon.service
rm -f ~/.config/systemd/user/vigil-daemon.service
systemctl --user daemon-reload
```

**FUSE mounts (if any are active):**

```bash
fusermount -u ~/.local/share/vigil/mnt/<set-name>
# Or unmount all:
for mnt in ~/.local/share/vigil/mnt/*/; do fusermount -u "$mnt" 2>/dev/null; done
```

**Config and data files:**

```bash
rm -rf ~/.config/vigil          # Config + password file
rm -rf ~/.local/share/vigil     # Logs + mount points
```

**Runtime files:**

```bash
rm -f ${XDG_RUNTIME_DIR:-/tmp}/vigil.sock
rm -f ${XDG_RUNTIME_DIR:-/tmp}/vigil.pid
```

**Installed binaries:**

```bash
rm -f ~/.cargo/bin/vigil
rm -f ~/.cargo/bin/vigil-daemon
```

## 2. Coding Standards

### General

- Follow Rust 2021 edition idioms
- Run `cargo fmt` before committing
- Run `cargo clippy` and address all warnings
- All public functions must have doc comments

### Error Handling

- Use `thiserror` for library error types, `anyhow` for application code
- Never use `.unwrap()` or `.expect()` in library code; use proper error propagation
- Panics are acceptable only for programmer errors (invariant violations), never for runtime conditions

### Testing

- Unit tests go in the same file as the code (`#[cfg(test)]` module)
- Integration tests go in `tests/` directory
- Tests must not leave artifacts on the filesystem; use `tempfile` crate for temporary directories
- Tests must not require `restic` to be installed unless marked `#[ignore]`

### Dependencies

- Do not add new dependencies without justification
- Prefer dependencies already in the workspace (see spec.md Section 2)
- Security-sensitive code (password handling, file permissions) must not use additional crates without review

### Commits

- One logical change per commit
- Commit message format: `<component>: <short description>` (e.g., `daemon: add debounce timer`, `lib: fix config parsing for multi-source`)
- Do not commit generated files, build artifacts, or test fixtures containing real paths

## 3. Component Boundaries

Each crate has a clear responsibility. Do not blur these boundaries:

| Crate | Responsibility | Does NOT do |
|-------|----------------|-------------|
| `vigil-lib` | Config parsing, types, IPC message definitions | Spawning processes, filesystem watching, UI |
| `vigil-daemon` | File watching, restic execution, IPC server | User interaction, config validation beyond parsing |
| `vigil` | CLI parsing, TUI rendering, IPC client | Direct restic calls, file watching |

**Shared code goes in `vigil-lib`.** If both the CLI and daemon need it, it belongs in the library.

## 4. Testing During Development

When testing features that interact with the system:

1. **Use a test config location:** `VIGIL_CONFIG=/tmp/test-vigil/config.toml`
2. **Use a temporary repository:** Create a temp dir for the Restic repo target
3. **Do not install systemd units** unless specifically testing bootstrap functionality
4. **Clean up after tests:** Remove temp dirs, stop any spawned daemon processes

## 5. Coordination Between Components

When implementing features that span multiple crates:

1. **Define the interface in `vigil-lib` first** (IPC messages, types)
2. **Implement the daemon handler** for the new message
3. **Implement the CLI/TUI caller** last
4. **Update spec.md** if adding new IPC messages or types

## 6. File Permissions

Security-sensitive files must have correct permissions:

| File | Permissions | Notes |
|------|-------------|-------|
| `~/.config/vigil/.repo_password` | `600` | Must be set on creation |
| `~/.config/vigil/config.toml` | `644` | Readable, no secrets |
| Unix socket | `700` | Handled by runtime |

When creating the password file programmatically:

```rust
use std::os::unix::fs::OpenOptionsExt;
std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .mode(0o600)
    .open(path)?;
```

## 7. Logging

- Use `tracing` macros (`info!`, `warn!`, `error!`, `debug!`)
- Include the backup set name in spans: `#[instrument(fields(set = %set_name))]`
- Log at appropriate levels:
  - `error!` - Failures requiring user attention
  - `warn!` - Recoverable issues
  - `info!` - Normal operations (backup started, completed)
  - `debug!` - Detailed flow (file change detected, debounce timer reset)

## 8. Pre-Commit Checklist

Before marking a task complete:

- [ ] `cargo fmt` passes
- [ ] `cargo clippy` has no warnings
- [ ] `cargo test` passes
- [ ] If modifying daemon code: `cargo test -p vigil-daemon -- --ignored --test-threads=1` passes (requires restic)
- [ ] New features have corresponding tests
- [ ] Manual verification of new feature completed
- [ ] Manual verification of existing features (regression) completed
- [ ] No temporary files or test artifacts left on disk
- [ ] No systemd units left installed (unless that was the task)
- [ ] spec.md updated if IPC or types changed
- [ ] changelog.md updated with summary of changes

## 9. Changelog Format

The `changelog.md` file provides context for subagents about recent changes. **Read it before starting work. Update it after committing.**

### Format

```markdown
## [YYYY-MM-DD] <commit-short-hash> — <component>: <brief title>

**What changed:**
- Bullet points describing what was added, modified, or removed

**Why:**
- Brief explanation of the motivation or which requirement this addresses

**Files affected:**
- List of files added or significantly modified

**Testing notes:**
- How the feature was verified
- Any manual testing steps performed
- Edge cases covered

**Dependencies/blockers:**
- Any new dependencies added
- Features this unblocks or depends on
```

### Example Entry

```markdown
## [2024-01-15] a1b2c3d — lib: add config parsing

**What changed:**
- Added `Config`, `BackupSet`, `RetentionPolicy` structs
- Implemented TOML parsing with validation
- Added `load_config()` and `default_config_path()` functions

**Why:**
- Implements FR4 (Unified Configuration) from prd.md
- Required foundation for daemon and CLI components

**Files affected:**
- crates/vigil-lib/src/config.rs (new)
- crates/vigil-lib/src/lib.rs (updated exports)

**Testing notes:**
- Unit tests for valid config parsing
- Unit tests for missing required fields
- Unit tests for path expansion (~/...)
- Tested with example config from spec.md

**Dependencies/blockers:**
- Unblocks: daemon watcher implementation, CLI init command
```

### Guidelines

- Write entries as if explaining to another developer joining the project
- Include enough detail that someone can understand the change without reading the diff
- Note any non-obvious design decisions or tradeoffs made
- If a feature is partially complete, explicitly state what remains
