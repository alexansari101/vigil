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

* **Browsing:** Support mounting backup snapshots as a virtual file system (FUSE) to allow standard CLI tools (`ls`, `cp`) to browse and restore specific files. Default mount point: `~/.local/share/vigil/mnt/<set-name>/`.
* **Unmounting:** `vigil unmount [SET]` cleanly unmounts FUSE mounts (one set or all if omitted). The TUI should also warn if mounts are active on exit.
* **Automated Retention:** Automatically prune old snapshots based on a configurable policy (default: keep last 10). Run after each successful backup.
* **Manual Management:** Provide a CLI wrapper to:
  * **Trigger:** Manually start a backup run immediately for one or all backup sets.
  * **Prune:** Manually delete specific snapshots or trigger a "garbage collection" to reclaim space.
  * **Status:** View a summary of the latest backup attempts, success/fail status, and a list of available snapshots for all configured sets.

### FR4: Unified Configuration

* All sources, destinations, exclusion patterns, and retention policies must be defined in a single, human-readable config file (e.g., YAML or TOML). It must support a list of backup "jobs" or "sets."

### FR5: Automated Onboarding & Service Management

* **Setup Wizard:** Provide a guided `vigil setup` command that creates the initial configuration and password file interactively.
* **Track/Untrack:** Provide CLI commands to add (`track`) or remove (`untrack`) backup sets without manual configuration editing.
* **Service Provisioning:** Group all service-related tasks under a `service` subcommand.
* **Dependency Check:** Verify and alert the user if required binaries (`restic`, `fusermount3`, `notify-send`) are missing.
* **Service Control:**
  * `vigil service install` — Provision systemd units and start daemon.
  * `vigil service stop` — Stop and disable systemd units without removing config.
  * `vigil service reload` — Trigger daemon to reload configuration.
  * `vigil service uninstall` — Stop and remove systemd units.

### FR5.1: CLI Subcommand Structure

```bash
# Core Operations
vigil setup                  # Guided first-time setup
vigil track <NAME> <SRC> <TGT> # Add new backup set
vigil untrack <NAME> [--purge] # Remove backup set
vigil init [SET]               # Initialize Restic repository
vigil backup [SET]             # Run backup now
vigil status                   # Show health summary (works offline)
vigil snapshots <SET>          # List snapshots
vigil list                     # List configured sets
vigil check [SET]              # Validate config/connectivity
vigil mount <SET> [ID]         # Mount repository (navigates to ID if provided)
vigil unmount [SET]            # Unmount active FUSE mounts
vigil prune [SET]              # Trigger retention cleanup
vigil purge <SET>              # Permanently delete repository data
vigil logs [-f]                # Tail the log file
vigil tui                      # Launch interactive dashboard

# Service Management
vigil service install          # Generate and enable systemd units
vigil service stop             # Stop and disable systemd units
vigil service reload           # Reload daemon configuration
vigil service uninstall [--purge] # Remove systemd units; --purge deletes configs
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
| Config Location | `~/.config/vigil/config.toml` (overridable via `--config` flag or `VIGIL_CONFIG` env var) |
| Password Storage | `~/.config/vigil/.repo_password` with `chmod 600`; one global password per repository |
| File Watching | inotify-based daemon (via `notify` crate) for fine-grained detection and debouncing |
| Architecture | Separate background service (`vigil-daemon`) with TUI/CLI as client; communicates via Unix socket |
| Failure Alerts | Desktop notification (via `notify-send`) + append to `~/.local/share/vigil/vigil.log` |
| Default Retention | `--keep-last 10` |

## 5. Success Criteria

1. **Automated Trigger:** Adding a file to any monitored folder triggers a successful, encrypted, incremental backup without user input.
2. **Interactive Restore:** A user can mount a previous snapshot and copy a file out of it using only the terminal.
3. **Storage Control:** The system successfully deletes old snapshots according to the retention policy, and the user can manually trigger a cleanup to reduce the size of the backup folder.
4. **Observability:** The user can run a single command to confirm the backup system is active and see when the last successful snapshot was taken for each configured backup set.
5. **Portability:** The entire system can be re-provisioned on a new machine using a single bootstrap command that consumes the existing configuration file and connects to the repository.
6. **UI:** The UI is successful if the user can determine the health of all 3+ backup folders in under 2 seconds of glancing at the screen.
