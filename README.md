# backutil - Portable CLI-Based Automated Backup System

A CLI/TUI-based automated backup utility for Linux that uses Restic for encrypted, deduplicated, versioned backups with file-watching and systemd integration.

## Prerequisites

The following binaries must be installed:

- **restic** - Backup engine
- **fusermount3** - For FUSE mount support
- **notify-send** - For desktop notifications (optional)

## Building

```bash
cargo build --release
```

Binaries will be created in `target/release/`:

- `backutil` - CLI interface
- `backutil-daemon` - Background service

## Quick Start

### 1. Create directories and password file

```bash
# Create config directory
mkdir -p ~/.config/backutil

# Create password file (this password is used to encrypt your backups)
echo "your-secure-password" > ~/.config/backutil/.repo_password
chmod 600 ~/.config/backutil/.repo_password

# Create test directories
mkdir -p /tmp/backutil_test/{source,target}
echo "test file content" > /tmp/backutil_test/source/test.txt
```

### 2. Create configuration file

```bash
cat > ~/.config/backutil/config.toml << 'EOF'
[global]
debounce_seconds = 60
retention = { keep_last = 10 }

[[backup_set]]
name = "test-set"
source = "/tmp/backutil_test/source"
target = "/tmp/backutil_test/target"
EOF
```

### 3. Initialize repository and run backup

```bash
# Initialize the restic repository
./target/release/backutil init test-set
# Expected output:
# Initializing repository for set 'test-set' at '/tmp/backutil_test/target'...
# Successfully initialized set 'test-set'.

# Check status (daemon not running)
./target/release/backutil status
# Expected output:
# Error: Daemon is not running.
# Exit code: 3

# Start the daemon
./target/release/backutil-daemon &

# Wait for daemon to start, then check status
sleep 2
./target/release/backutil status
# Expected output:
# NAME            STATE           LAST BACKUP          MOUNTED   
# -----------------------------------------------------------------
# test-set        Idle            Never                No

# Run a backup
./target/release/backutil backup test-set
# Expected output (example):
# Backup started for set 'test-set'.
# Backup complete for set 'test-set': snapshot e6ad2ad9..., 1.9 KiB added in 0.7s

# Check status again
./target/release/backutil status
# Expected output:
# NAME            STATE           LAST BACKUP          MOUNTED   
# -----------------------------------------------------------------
# test-set        Idle            5s ago               No
```

### 4. Mount and browse snapshots

```bash
# Mount the repository (exposes all snapshots via FUSE)
./target/release/backutil mount test-set
# Expected output:
# Snapshot mounted at: /home/USER/.local/share/backutil/mnt/test-set

# Browse snapshots
ls ~/.local/share/backutil/mnt/test-set/
# Expected output:
# hosts  ids  snapshots  tags

# Browse a specific snapshot by ID
ls ~/.local/share/backutil/mnt/test-set/ids/
# (lists snapshot IDs)

# Unmount
./target/release/backutil unmount test-set
# Expected output:
# Successfully unmounted set 'test-set'.
```

### 5. Prune old snapshots

```bash
./target/release/backutil prune test-set
# Expected output:
# Pruned set 'test-set': 0 B reclaimed
```

### 6. View logs

```bash
# Show logs (if log file exists)
./target/release/backutil logs
# Expected output:
# Log file "~/.local/share/backutil/backutil.log" does not exist.
# (or log contents if file exists)

# Follow logs in real-time
./target/release/backutil logs -f
```

### 7. Systemd integration

**Note:** The `bootstrap` command creates a systemd unit that expects the daemon at `~/.cargo/bin/backutil-daemon`. You must first install the binaries:

```bash
# Install binaries to ~/.cargo/bin/
cargo install --path crates/backutil
cargo install --path crates/backutil-daemon

# Generate and enable systemd user service
backutil bootstrap
# Expected output:
# Bootstrapping backutil...
# Generated systemd unit at "~/.config/systemd/user/backutil-daemon.service"
# ...
# Successfully bootstrapped backutil-daemon.

# Verify the daemon is running
backutil status
# Expected output:
# NAME            STATE           LAST BACKUP          MOUNTED   
# -----------------------------------------------------------------
# test-set        Idle            Never                No

# Disable the service (keeps config)
backutil disable
# Expected output:
# Stopping and disabling backutil-daemon service...
# Successfully disabled backutil-daemon.

# Uninstall completely (removes systemd unit)
backutil uninstall
# Expected output:
# Removed systemd unit "~/.config/systemd/user/backutil-daemon.service"
# Uninstall complete.

# Uninstall with purge (also removes config and data)
backutil uninstall --purge
# Expected output:
# Removed systemd unit ...
# Removed configuration directory "~/.config/backutil"
# Removed data directory "~/.local/share/backutil"
# Uninstall complete.
```

## Complete CLI Reference

| Command | Description |
|---------|-------------|
| `backutil init [SET]` | Initialize a Restic repository for one or all backup sets |
| `backutil backup [SET]` | Run backup now (one set or all if omitted) |
| `backutil status` | Show health summary and backup set status |
| `backutil mount <SET> [SNAPSHOT_ID]` | Mount a snapshot via FUSE |
| `backutil unmount [SET]` | Unmount FUSE mounts (one set or all) |
| `backutil prune [SET]` | Trigger retention policy cleanup |
| `backutil tui` | Launch interactive dashboard (not yet implemented) |
| `backutil bootstrap` | Generate and enable systemd user units |
| `backutil disable` | Stop and disable systemd units |
| `backutil uninstall [--purge]` | Remove systemd units; --purge deletes configs |
| `backutil logs [-f]` | Tail the log file; -f for follow mode |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Config error (missing, invalid) |
| 3 | Daemon not running |
| 4 | Restic error |
| 5 | Mount/unmount error |

## Configuration

Configuration file: `~/.config/backutil/config.toml`

Example with multiple backup sets:

```toml
[global]
debounce_seconds = 60
retention = { keep_last = 10 }

[[backup_set]]
name = "personal"
source = "~/personal_records"
target = "/mnt/backup/personal"
exclude = ["*.tmp", ".cache/**"]

[[backup_set]]
name = "financial"
source = "~/financial_docs"
target = "/mnt/backup/financial"
debounce_seconds = 30
retention = { keep_last = 20 }
```

## Cleanup

To remove all test artifacts created by the above commands:

```bash
# Stop daemon
pkill backutil-daemon

# Remove test directories
rm -rf /tmp/backutil_test

# Uninstall via CLI (removes systemd unit and optionally config/data)
backutil uninstall --purge

# Remove cargo-installed binaries (not removed by uninstall --purge)
cargo uninstall backutil backutil-daemon

# If you didn't use cargo install, just remove the build directory:
# rm -rf target/
```

## Notes

- The daemon must be running for most CLI commands (except `init`, `bootstrap`, `uninstall`)
- Mount exposes the entire repository; browse snapshots via `/ids/<snapshot-id>/` or `/snapshots/<timestamp>/`
- Password is stored in `~/.config/backutil/.repo_password` with mode 600
