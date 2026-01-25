use directories::ProjectDirs;
use std::path::PathBuf;

/// Get the project directories for backutil.
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "backutil")
}

/// Returns the configuration directory: `~/.config/backutil/`
pub fn config_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback if ProjectDirs fails (unlikely on Linux)
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".config");
            path.push("backutil");
            path
        })
}

/// Returns the path to the config file: `~/.config/backutil/config.toml`
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the path to the repository password file: `~/.config/backutil/.repo_password`
pub fn password_path() -> PathBuf {
    config_dir().join(".repo_password")
}

/// Returns the log file path: `~/.local/share/backutil/backutil.log`
pub fn log_path() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".local");
            path.push("share");
            path.push("backutil");
            path
        })
        .join("backutil.log")
}

/// Returns the Unix socket path.
/// Respects `$XDG_RUNTIME_DIR/backutil.sock` with fallback to `/tmp/backutil-$UID.sock`.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("backutil.sock")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/backutil-{}.sock", uid))
    }
}

/// Returns the PID file path.
/// Respects `$XDG_RUNTIME_DIR/backutil.pid` with fallback to `/tmp/backutil-$UID.pid`.
pub fn pid_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("backutil.pid")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/backutil-{}.pid", uid))
    }
}

/// Returns the base directory for FUSE mounts: `~/.local/share/backutil/mnt/`
pub fn mount_base_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".local");
            path.push("share");
            path.push("backutil");
            path
        })
        .join("mnt")
}

/// Returns the mount path for a specific backup set.
pub fn mount_path(set_name: &str) -> PathBuf {
    mount_base_dir().join(set_name)
}

/// Returns the path to the systemd user unit: `~/.config/systemd/user/backutil-daemon.service`
pub fn systemd_unit_path() -> PathBuf {
    let mut path = project_dirs()
        .map(|d| d.config_dir().to_path_buf()) // This is ~/.config/backutil
        .and_then(|p| p.parent().map(|p| p.to_path_buf())) // This is ~/.config
        .unwrap_or_else(|| {
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".config");
            path
        });
    path.push("systemd");
    path.push("user");
    path.push("backutil-daemon.service");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        let dir = config_dir();
        assert!(dir.ends_with("backutil"));
        assert!(config_path().ends_with("config.toml"));
        assert!(password_path().ends_with(".repo_password"));
    }

    #[test]
    fn test_log_path() {
        assert!(log_path().ends_with("backutil/backutil.log"));
    }

    #[test]
    fn test_socket_pid_paths() {
        // Just verify they don't panic and look reasonable
        let s = socket_path();
        let p = pid_path();
        assert!(s.to_string_lossy().contains("backutil.sock"));
        assert!(p.to_string_lossy().contains("backutil.pid"));
    }

    #[test]
    fn test_mount_paths() {
        let base = mount_base_dir();
        assert!(base.ends_with("backutil/mnt"));
        assert!(mount_path("test").ends_with("backutil/mnt/test"));
    }

    #[test]
    fn test_systemd_path() {
        let p = systemd_unit_path();
        assert!(p.ends_with("systemd/user/backutil-daemon.service"));
    }
}
