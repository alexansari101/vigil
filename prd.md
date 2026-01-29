# PRD: Portable, CLI-Based Automated Backup System

## 1. Problem Statement

The current backup process for sensitive personal records stored on my computer relies on a manual bash script that uses `tar` for compression and `gpg -c` for symmetric encryption. This workflow has three primary points of friction:

1. **Manual Execution:** The user must remember to run the script after modifying files, leading to a risk of data loss if a backup is forgotten.
2. **Interactive Bottlenecks:** Every execution requires manual password entry for encryption, preventing background or "headless" automation.
3. **Storage Inefficiency:** Each backup is a full monolithic archive (`.tar.gz.gpg`), which consumes excessive storage and lacks versioning/incremental update capabilities.

The goal is to transition to an automated, event-driven system that triggers encrypted, deduplicated, and versioned backups in the background whenever changes are detected, while remaining entirely portable across different Linux distributions.

## 2. Goals

* **Zero-Interaction Automation:** Eliminate manual triggers and password prompts during the backup process.
* **Distro-Agnostic Portability:** The setup must be easily restorable on any Linux distribution (specifically tested on Fedora 43) via an automated bootstrapping process.
* **Granular Recovery:** Enable "time-machine" style browsing of file versions through the terminal.
* **Storage Efficiency:** Minimize cloud storage usage through deduplication and proactive space management (pruning).
* **Simple Configuration:** Easily add or modify source folders and backup destinations via a single configuration file.
* **CLI-Only:** The solution must be strictly terminal/TUI based.

## 3. Functional Requirements

### FR1: Event-Driven Automation

* **Change Detection:** Monitor multiple source directories (e.g., `~/personal_records`, `~/financial_docs`) for file system events (writes, moves, deletes).
* **Debouncing:** Implement a delay (e.g., 60s) after the last detected change before triggering the backup to prevent resource thrashing during active work.

### FR2: Storage & Security

* **Deduplication:** Use a chunk-based backup engine to minimize the storage footprint in the target directory.
* **Encryption:** AES-256 encryption at rest. Credentials must be sourced from a local secure store (protected file or keyring) to allow headless execution.
* **Multi-Target Support:** Support mapping different source directories to specific local target directories. Additionally, the tool must support a "Common Target" mode where multiple sources are automatically mapped to distinct subfolders (named after the source directory) within a single root backup folder.

### FR3: Snapshot Management, Space Recovery, & Monitoring

* **Browsing:** Support mounting backup snapshots as a virtual file system (FUSE) to allow standard CLI tools (`ls`, `cp`) to browse and restore specific files. Default mount point: `~/.local/share/backutil/mnt/<set-name>/`.
* **Unmounting:** `backutil unmount [SET]` cleanly unmounts FUSE mounts (one set or all if omitted). The TUI should also warn if mounts are active on exit.
* **Automated Retention:** Automatically prune old snapshots based on a configurable policy (default: keep last 10). Run after each successful backup.
* **Manual Management:** Provide a CLI wrapper to:
  * **Trigger:** Manually start a backup run immediately for one or all backup sets.
  * **Prune:** Manually delete specific snapshots or trigger a "garbage collection" to reclaim space.
  * **Status:** View a summary of the latest backup attempts, success/fail status, and a list of available snapshots for all configured sets.

### FR4: Unified Configuration

* All sources, destinations, exclusion patterns, and retention policies must be defined in a single, human-readable config file (e.g., YAML or TOML). It must support a list of backup "jobs" or "sets."

### FR5: Automated Bootstrapping & Installation

* **Installer Script:** Provide a single-command installation or "bootstrap" script.
* **Service Provisioning:** The script must automatically generate, install, and enable the necessary `systemd` user units based on the unified configuration.
* **Dependency Check:** Verify and alert the user if required binaries (`restic`, `fusermount3`, `notify-send`) are missing from the new system.
* **Uninstall/Disable:**
  * `backutil disable` — Stop and disable systemd units without removing config.
  * `backutil uninstall` — Stop units, remove systemd units, and optionally delete config/logs (`--purge` flag).

### FR5.1: CLI Subcommand Structure

```
backutil init         # Initialize a new Restic repository
backutil backup [SET] # Run backup now (one set or all if omitted)
backutil status       # Show health summary and recent snapshots
backutil snapshots <SET> [--limit N]  # List available snapshots for a set
backutil list         # List all configured backup sets
backutil check [SET]  # Validate config and test repository access (works offline)
backutil mount <SET>  # Mount a snapshot via FUSE (interactive snapshot picker)
backutil unmount      # Unmount all active FUSE mounts
backutil prune        # Trigger retention policy cleanup
backutil tui          # Launch interactive dashboard
backutil bootstrap    # Generate and enable systemd user units
backutil disable      # Stop and disable systemd units
backutil uninstall    # Remove systemd units; --purge to delete configs
backutil logs         # Tail the log file
```

### FR5.2: CLI Output Quality

* **Clean Output:** CLI output must be clean and user-focused. Daemon log messages must not appear in CLI command output.
* **Short IDs:** Snapshot IDs displayed to users should use 8-character short IDs, not full 64-character hashes.
* **Plain English Help:** Help text should use plain English (e.g., "If omitted, backs up all sets") instead of technical jargon (e.g., "null = all sets").
* **Scripting Support:** All commands should support `--quiet` (minimal output) and `--json` (machine-readable) flags for automation.
* **Backup Timeout:** The `backup` command must not hang indefinitely; implement reasonable timeouts or progress indicators.

### FR6: TUI Layout Structure

* **Header:** App name, version, and global status (e.g., "All Systems Normal" or "Syncing...").
* **Active Jobs Panel:** A vertical list of monitored folders. Each entry shows:
  * Source Path ⮕ Target Path.
  * Live Status: (Idle / Debouncing / Backing Up / Error).
  * Sparkline or Progress Bar: Visual representation of the last 5 backup durations.
* **Footer:** Keybindings help bar (e.g., [b] Backup All, [p] Prune, [m] Mount, [q] Quit, etc).

### FR7: TUI Interaction Patterns

* **Non-Blocking Progress:** When a backup is triggered (automatically or manually), it must not freeze the TUI. Use background workers with a spinner or smooth progress bar (mpb style).
* **Human-Readable Feedback:** Bad: Backup completed in 4.342s. 1204 bytes sent. Good: 󰄲 Snapshot created (4s ago). 1.2MB added.
* **Clear Error Messages:** If something goes wrong of a dependency is missing, the system should provide clear error message that allow the user to debug and address the issue.
* **Contextual Help:** Pressing ? should overlay a modal with a list of all CLI commands and shortcuts.

## 4. Technical Constraints and Decisions

* **Platform:** Linux (Fedora 43 primary). Must utilize `systemd` (user-level) for automation.
* **Interface:** CLI/TUI only. Provide a "high-quality," modern TUI that uses a popular theme like Catpuccin Frappe or TokyoNight and NerdFonts if appropriate.
* **Tooling:** Restic for backup engine; Rust for implementation.
* **Dependencies:** `restic`, `fusermount3` (for FUSE mounts), `notify-send` (for desktop alerts).

| Concern | Decision |
|---------|----------|
| Backup Engine | Restic |
| Language | Rust |
| Config Format | TOML |
| Config Location | `~/.config/backutil/config.toml` (overridable via `--config` flag or `BACKUTIL_CONFIG` env var) |
| Password Storage | `~/.config/backutil/.repo_password` with `chmod 600`; one global password per repository |
| File Watching | inotify-based daemon (via `notify` crate) for fine-grained detection and debouncing |
| Architecture | Separate background service (`backutil-daemon`) with TUI/CLI as client; communicates via Unix socket |
| Failure Alerts | Desktop notification (via `notify-send`) + append to `~/.local/share/backutil/backutil.log` |
| Default Retention | `--keep-last 10` |

## 5. Success Criteria

1. **Automated Trigger:** Adding a file to any monitored folder triggers a successful, encrypted, incremental backup without user input.
2. **Interactive Restore:** A user can mount a previous snapshot and copy a file out of it using only the terminal.
3. **Storage Control:** The system successfully deletes old snapshots according to the retention policy, and the user can manually trigger a cleanup to reduce the size of the backup folder.
4. **Observability:** The user can run a single command to confirm the backup system is active and see when the last successful snapshot was taken for each configured backup set.
5. **Portability:** The entire system can be re-provisioned on a new machine using a single bootstrap command that consumes the existing configuration file and connects to the repository.
6. **UI:** The UI is successful if the user can determine the health of all 3+ backup folders in under 2 seconds of glancing at the screen.
