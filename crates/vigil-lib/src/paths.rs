use directories::ProjectDirs;
use std::path::PathBuf;

/// Get the project directories for vigil.
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "vigil")
}

/// Returns the configuration directory: `~/.config/vigil/`
pub fn config_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback if ProjectDirs fails (unlikely on Linux)
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".config");
            path.push("vigil");
            path
        })
}

/// Returns the path to the config file: `~/.config/vigil/config.toml`
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the path to the repository password file: `~/.config/vigil/.repo_password`
pub fn password_path() -> PathBuf {
    config_dir().join(".repo_password")
}

/// Returns the active configuration path, respecting `VIGIL_CONFIG` environment variable.
pub fn active_config_path() -> PathBuf {
    std::env::var("VIGIL_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| config_path())
}

/// Returns the log file path: `~/.local/share/vigil/vigil.log`
pub fn log_path() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".local");
            path.push("share");
            path.push("vigil");
            path
        })
        .join("vigil.log")
}

/// Returns the Unix socket path.
/// Respects `$XDG_RUNTIME_DIR/vigil.sock` with fallback to `/tmp/vigil-$UID.sock`.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("vigil.sock")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/vigil-{}.sock", uid))
    }
}

/// Returns the PID file path.
/// Respects `$XDG_RUNTIME_DIR/vigil.pid` with fallback to `/tmp/vigil-$UID.pid`.
pub fn pid_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("vigil.pid")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/vigil-{}.pid", uid))
    }
}

/// Returns the base directory for FUSE mounts: `~/.local/share/vigil/mnt/`
pub fn mount_base_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let mut path = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp"));
            path.push(".local");
            path.push("share");
            path.push("vigil");
            path
        })
        .join("mnt")
}

/// Returns the mount path for a specific backup set.
pub fn mount_path(set_name: &str) -> PathBuf {
    mount_base_dir().join(set_name)
}

/// Checks if the given path is a current mount point by reading /proc/mounts.
/// This is used to synchronize daemon state with the filesystem on restart.
pub fn is_mount_point(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }

    let target = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mounts = match std::fs::read_to_string("/proc/mounts") {
        Ok(s) => s,
        Err(_) => return false,
    };

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let mount_point = parts[1];
            // Restic mounts appear as 'restic' or similar in /proc/mounts
            if let Ok(p) = std::path::Path::new(mount_point).canonicalize() {
                if p == target {
                    return true;
                }
            }
        }
    }

    false
}

/// Returns the path to the systemd user unit: `~/.config/systemd/user/vigil-daemon.service`
pub fn systemd_unit_path() -> PathBuf {
    let mut path = project_dirs()
        .map(|d| d.config_dir().to_path_buf()) // This is ~/.config/vigil
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
    path.push("vigil-daemon.service");
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_paths() {
        let dir = config_dir();
        assert!(dir.ends_with("vigil"));
        assert!(config_path().ends_with("config.toml"));
        assert!(password_path().ends_with(".repo_password"));
    }

    #[test]
    fn test_log_path() {
        assert!(log_path().ends_with("vigil/vigil.log"));
    }

    #[test]
    fn test_socket_pid_paths() {
        // Just verify they don't panic and look reasonable
        let s = socket_path();
        let p = pid_path();
        assert!(s.to_string_lossy().contains("vigil.sock"));
        assert!(p.to_string_lossy().contains("vigil.pid"));
    }

    #[test]
    fn test_mount_paths() {
        let base = mount_base_dir();
        assert!(base.ends_with("vigil/mnt"));
        assert!(mount_path("test").ends_with("vigil/mnt/test"));
    }

    #[test]
    fn test_systemd_path() {
        let p = systemd_unit_path();
        assert!(p.ends_with("systemd/user/vigil-daemon.service"));
    }

    #[test]
    fn test_is_mount_point_nonexistent() {
        // A path that does not exist should not be a mount point
        assert!(!is_mount_point(std::path::Path::new(
            "/tmp/vigil_nonexistent_path_for_test"
        )));
    }

    #[test]
    fn test_is_mount_point_regular_dir() {
        // A regular temp directory should not be detected as a mount point
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_mount_point(tmp.path()));
    }
}
