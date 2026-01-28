use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Config validation error: {0}")]
    Validation(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    #[serde(rename = "backup_set", default)]
    pub backup_sets: Vec<BackupSet>,
}

/// Global configuration settings.
#[derive(Debug, Deserialize, Clone)]
pub struct GlobalConfig {
    /// Wait time in seconds after the last detected change before triggering a backup.
    #[serde(default = "default_debounce")]
    pub debounce_seconds: u64,
    /// Default retention policy for all backup sets.
    pub retention: Option<RetentionPolicy>,
}

fn default_debounce() -> u64 {
    60
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            debounce_seconds: default_debounce(),
            retention: Some(RetentionPolicy {
                keep_last: Some(10),
                ..Default::default()
            }),
        }
    }
}

/// Configuration for a specific backup set.
#[derive(Debug, Deserialize, Clone)]
pub struct BackupSet {
    /// Unique identifier for the backup set.
    pub name: String,
    /// Single source directory path (mutually exclusive with `sources`).
    pub source: Option<String>,
    /// Multiple source directory paths (mutually exclusive with `source`).
    pub sources: Option<Vec<String>>,
    /// Restic repository target path.
    pub target: String,
    /// Optional glob patterns for file exclusion.
    pub exclude: Option<Vec<String>>,
    /// Override for the global debounce delay.
    pub debounce_seconds: Option<u64>,
    /// Override for the global retention policy.
    pub retention: Option<RetentionPolicy>,
}

/// Retention policy defining how many snapshots to keep.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct RetentionPolicy {
    /// Number of most recent snapshots to keep.
    pub keep_last: Option<u32>,
    /// Number of daily snapshots to keep.
    pub keep_daily: Option<u32>,
    /// Number of weekly snapshots to keep.
    pub keep_weekly: Option<u32>,
    /// Number of monthly snapshots to keep.
    pub keep_monthly: Option<u32>,
}

impl Config {
    /// Validates the configuration, ensuring unique names and mutually exclusive source fields.
    /// Also expands `~/` in source and target paths.
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        let mut names = HashSet::new();
        for set in &mut self.backup_sets {
            if !names.insert(set.name.clone()) {
                return Err(ConfigError::Validation(format!(
                    "Duplicate backup set name: {}",
                    set.name
                )));
            }

            if set.source.is_some() && set.sources.is_some() {
                return Err(ConfigError::Validation(format!(
                    "Set '{}' cannot have both 'source' and 'sources'",
                    set.name
                )));
            }

            if set.source.is_none() && set.sources.is_none() {
                return Err(ConfigError::Validation(format!(
                    "Set '{}' must have either 'source' or 'sources'",
                    set.name
                )));
            }

            // Expand paths
            if let Some(ref s) = set.source {
                set.source = Some(expand_home(s));
            }
            if let Some(ref ss) = set.sources {
                set.sources = Some(ss.iter().map(|s| expand_home(s)).collect());
            }
            set.target = expand_home(&set.target);
        }
        Ok(())
    }
}

fn expand_home(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

/// Loads the configuration from the environment variable `BACKUTIL_CONFIG`
/// or the default system location (`~/.config/backutil/config.toml`).
///
/// # Errors
///
/// Returns `ConfigError` if the file cannot be found, read, or parsed,
/// or if validation fails.
pub fn load_config() -> Result<Config, ConfigError> {
    let path = std::env::var("BACKUTIL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate::paths::config_path());

    if !path.exists() {
        return Err(ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Config file not found: {:?}", path),
        )));
    }

    let content = std::fs::read_to_string(path)?;
    let mut config: Config = toml::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_valid_config() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[global]
debounce_seconds = 30

[[backup_set]]
name = "test"
source = "~/test"
target = "/tmp/backup"
"#
        )
        .unwrap();

        std::env::set_var("BACKUTIL_CONFIG", file.path());
        let config = load_config().unwrap();
        assert_eq!(config.global.debounce_seconds, 30);
        assert_eq!(config.backup_sets.len(), 1);
        assert_eq!(config.backup_sets[0].name, "test");
    }

    #[test]
    fn test_path_expansion() {
        let home = directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf();
        let home_str = home.to_string_lossy();

        let set = BackupSet {
            name: "test".to_string(),
            source: Some("~/test".to_string()),
            sources: None,
            target: "~/backup".to_string(),
            exclude: None,
            debounce_seconds: None,
            retention: None,
        };

        let mut config = Config {
            global: GlobalConfig::default(),
            backup_sets: vec![set],
        };

        config.validate().unwrap();

        assert_eq!(
            config.backup_sets[0].source.as_ref().unwrap(),
            &format!("{}/test", home_str)
        );
        assert_eq!(config.backup_sets[0].target, format!("{}/backup", home_str));
    }

    #[test]
    fn test_mutually_exclusive_sources() {
        let config_str = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "test"
source = "~/test"
sources = ["~/test1", "~/test2"]
target = "/tmp/backup"
"#;
        let mut config: Config = toml::from_str(config_str).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot have both 'source' and 'sources'"));
    }

    #[test]
    fn test_duplicate_names() {
        let config_str = r#"
[global]
debounce_seconds = 60

[[backup_set]]
name = "dup"
source = "~/test1"
target = "/tmp/backup1"

[[backup_set]]
name = "dup"
source = "~/test2"
target = "/tmp/backup2"
"#;
        let mut config: Config = toml::from_str(config_str).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate backup set name"));
    }
}
