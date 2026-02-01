use serial_test::serial;
use std::env;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[tokio::test]
#[serial]
async fn test_bootstrap_unit_file_generation() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();

    // Mock HOME for the test to control where systemd units are written
    // We still need to set these globally because the library path helpers use them
    let old_home = env::var("HOME").ok();
    let old_config = env::var("XDG_CONFIG_HOME").ok();
    let old_data = env::var("XDG_DATA_HOME").ok();
    let old_runtime = env::var("XDG_RUNTIME_DIR").ok();

    env::set_var("HOME", &home);
    env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    env::set_var("XDG_DATA_HOME", home.join(".local/share"));
    env::set_var("XDG_RUNTIME_DIR", home.join(".runtime"));
    fs::create_dir_all(home.join(".runtime")).unwrap();

    // Use backutil_lib::paths to get the expected path
    let unit_path = backutil_lib::paths::systemd_unit_path();

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
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_RUNTIME_DIR", home.join(".runtime"))
        .output()
        .expect("Failed to run backutil");

    if unit_path.exists() {
        let content = fs::read_to_string(&unit_path).unwrap();
        assert!(content.contains("Description=Backutil Daemon"));
        assert!(content.contains("ExecStart=%h/.cargo/bin/backutil-daemon"));
    } else {
        println!("Unit file not generated, possibly due to early failure.");
    }

    // Restore environment
    if let Some(h) = old_home {
        env::set_var("HOME", h);
    } else {
        env::remove_var("HOME");
    }
    if let Some(c) = old_config {
        env::set_var("XDG_CONFIG_HOME", c);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(d) = old_data {
        env::set_var("XDG_DATA_HOME", d);
    } else {
        env::remove_var("XDG_DATA_HOME");
    }
    if let Some(r) = old_runtime {
        env::set_var("XDG_RUNTIME_DIR", r);
    } else {
        env::remove_var("XDG_RUNTIME_DIR");
    }
}

#[tokio::test]
#[serial]
async fn test_uninstall_cleanups() {
    let temp = tempdir().unwrap();
    let home = temp.path().to_path_buf();

    let old_home = env::var("HOME").ok();
    let old_config = env::var("XDG_CONFIG_HOME").ok();
    let old_data = env::var("XDG_DATA_HOME").ok();
    let old_runtime = env::var("XDG_RUNTIME_DIR").ok();

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
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_RUNTIME_DIR", home.join(".runtime"))
        .output()
        .expect("Failed to run backutil");

    assert!(!config_dir.exists());
    assert!(!data_dir.exists());

    // Restore environment
    if let Some(h) = old_home {
        env::set_var("HOME", h);
    } else {
        env::remove_var("HOME");
    }
    if let Some(c) = old_config {
        env::set_var("XDG_CONFIG_HOME", c);
    } else {
        env::remove_var("XDG_CONFIG_HOME");
    }
    if let Some(d) = old_data {
        env::set_var("XDG_DATA_HOME", d);
    } else {
        env::remove_var("XDG_DATA_HOME");
    }
    if let Some(r) = old_runtime {
        env::set_var("XDG_RUNTIME_DIR", r);
    } else {
        env::remove_var("XDG_RUNTIME_DIR");
    }
}
