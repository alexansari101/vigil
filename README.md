# 🛡️ backutil

**Automated, event-driven, encrypted backups for the modern Linux terminal.**

`backutil` is a high-level orchestration layer for [restic](https://restic.net). It transforms the world's most robust backup engine into a "set-it-and-forget-it" system that watches your files/folders and backs them up the moment they change.

---

## ✨ Why backutil?

The backup landscape is often a choice between **simple but limited** desktop apps and **powerful but manual** CLI tools. `backutil` lives in the sweet spot. While `restic` provides the engine, `backutil` aggregates multiple repositories into a single pane of glass, watching them for changes and tracking health in real-time.

| Feature | Raw `restic` | `backutil` |
| :--- | :---: | :---: |
| **Trigger** | Manual / Cron | ⚡ **Event-driven (inotify)** |
| **Config** | Scripting/Flags | 🛠️ **Unified TOML** |
| **Monitoring** | Per-repo snapshots | 📊 **Global Backup Status Monitoring** |
| **Security** | Manual PW entry | 🔐 **Secure Headless PW** |
| **UX** | Technical / Low-level | 🚀 **Guided Setup Wizard** |

---

## 🚀 Quickstart

### 1. Install

### Prerequisites

`backutil` is designed for Linux systems.

- **Tested on:** Fedora 43 (Workstation Edition)
- **Minimum Dependencies:** `restic` (tested with v0.18.1), `fusermount3`, and (optional) `notify-send` for system notifications.

Ensure these are installed on your Linux system, then install both `backutil` and `backutil-daemon` directly from source:

```bash
cargo install --git https://github.com/alexansari101/backutil
```

### 2. Guided Setup

Run the wizard to configure your first backup set, set your repository password, and install the background service.

```bash
backutil setup
```

From there, you can add additional backup sets effortlessly:

```bash
backutil track "work-docs" ~/documents /mnt/backups/work
```

Folders are now being watched for changes and backed up automatically. However, you can kick-off a first backup manually:

```bash
backutil backup
```

And check the status of your backup sets:

```bash
backutil status
```

> 💡 **Note:** The service daemon watches files/folders for changes and includes a "debounce" period to prevent multiple backups from being triggered in quick succession.

---

## 🔍 Key Concepts

### Zero-Touch Automation

`backutil` uses a lightweight background daemon (`backutil-daemon`) managed by `systemd`. It watches your source directories and triggers an encrypted backup 60 seconds (configurable) after the last change is detected.

### Artifacts & Governance

Everything is stored exactly where you'd expect on a Linux system:

- **Configuration**: `~/.config/backutil/config.toml`
- **Encryption Key**: `~/.config/backutil/.repo_password` (chmod 600)
- **Service Logs**: `~/.local/share/backutil/backutil.log`
- **Systemd Unit**: `~/.config/systemd/user/backutil-daemon.service`

### "Time Machine" Restores

Browse your history using any standard file manager or terminal tool.

```bash
# Mount the "work-docs" set
backutil mount work-docs

# Your snapshots are now available at:
# ~/.local/share/backutil/mnt/work-docs/snapshots/latest/
ls ~/.local/share/backutil/mnt/work-docs/snapshots/

# When finished
backutil unmount work-docs
```

---

## 🛠 Subcommands at a Glance

- `setup` - Guided first-time setup wizard.
- `track` / `untrack` - Add or remove backup sets from configuration.
- `backup` - Manually trigger a backup immediately.
- `mount` / `unmount` - Browse backups as standard folders.
- `status` - Show health summary and status of all tracked backup sets.

For a full list of subcommands, run `backutil --help`.

---

## 🗑️ Cleanup & Uninstall

### Remove a Backup Set

To stop tracking a directory and permanently delete its Restic repository:

```bash
backutil untrack <name> --purge
```

### Uninstall the Service

To stop the background daemon and remove the systemd user service:

```bash
backutil service uninstall
```

To remove the service and delete all configuration, logs, and encryption keys:
> ⚠️ **Warning:** This deletes your local encryption keys. You will lose access to your Restic repositories unless you have the password stored elsewhere.

```bash
backutil service uninstall --purge
```

### Remove the Tool

To remove the `backutil` and `backutil-daemon` binaries:

```bash
cargo uninstall backutil backutil-daemon
```

---

## 🤝 Contributing

`backutil` is built in Rust with ❤️. Issues and PRs are welcome!

Check out the [Product Requirements](./prd.md) and [Project Specs](./spec.md) and [Developer Guidelines](./developer_guidelines.md) for more details.

## 📄 License

MIT / Apache 2.0
