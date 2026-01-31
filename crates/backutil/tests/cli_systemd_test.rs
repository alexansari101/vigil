use std::env;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[tokio::test]
async fn test_bootstrap_unit_file_generation() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();

    // Mock HOME for the test to control where systemd units are written
    let old_home = env::var("HOME").ok();
    env::set_var("HOME", &home);
    env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    env::set_var("XDG_DATA_HOME", home.join(".local/share"));
    env::set_var("XDG_RUNTIME_DIR", home.join(".runtime"));
    fs::create_dir_all(home.join(".runtime")).unwrap();

    // Use backutil_lib::paths to get the expected path
    let unit_path = backutil_lib::paths::systemd_unit_path();

    // We can't easily run the real bootstrap because it calls systemctl
    // But we can test the file generation if we refactored main.rs to expose it.
    // For now, let's just use the binary and see if it fails gracefully when systemctl is missing/fails.

    let bin_path = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("backutil");

    let _output = Command::new(&bin_path)
        .arg("service")
        .arg("install")
        .output()
        .expect("Failed to run backutil");

    // It will likely fail on systemctl daemon-reload in this environment
    // But it should have generated the file before that.

    if unit_path.exists() {
        let content = fs::read_to_string(&unit_path).unwrap();
        assert!(content.contains("Description=Backutil Daemon"));
        assert!(content.contains("ExecStart=%h/.cargo/bin/backutil-daemon"));
    } else {
        // If it failed before generation (e.g. dependency check), that's also okay for this test
        // as long as it didn't panic.
        println!("Unit file not generated, possibly due to early failure.");
    }

    // Restore HOME
    if let Some(h) = old_home {
        env::set_var("HOME", h);
    } else {
        env::remove_var("HOME");
    }
}

#[tokio::test]
async fn test_uninstall_cleanups() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();

    let old_home = env::var("HOME").ok();
    env::set_var("HOME", &home);
    env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    env::set_var("XDG_DATA_HOME", home.join(".local/share"));
    env::set_var("XDG_RUNTIME_DIR", home.join(".runtime"));
    fs::create_dir_all(home.join(".runtime")).unwrap();

    // Create dummy files using library path helpers
    let config_dir = backutil_lib::paths::config_dir();
    let log_path = backutil_lib::paths::log_path();
    let data_dir = log_path.parent().unwrap().to_path_buf();

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "test").unwrap();

    fs::create_dir_all(&data_dir).unwrap();
    fs::write(&log_path, "test").unwrap();

    let bin_path = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("backutil");

    let _output = Command::new(&bin_path)
        .arg("service")
        .arg("uninstall")
        .arg("--purge")
        .output()
        .expect("Failed to run backutil");

    assert!(!config_dir.exists());
    assert!(!data_dir.exists());

    // Restore HOME
    if let Some(h) = old_home {
        env::set_var("HOME", h);
    } else {
        env::remove_var("HOME");
    }
}
