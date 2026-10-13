use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ron::ser::{PrettyConfig, to_string_pretty};
use serde::{Deserialize, Serialize};

fn default_db() -> String {
    match std::env::var("RURARENDAR_DB") {
        Ok(path) if path.starts_with("sqlite:") => path,
        Ok(path) => format!("sqlite://{path}"),
        Err(_) => "sqlite://rurarendar.sqlite".into(),
    }
}

fn default_ui_locale() -> String {
    "auto".into()
}

fn default_color_scheme() -> String {
    "modern".into()
}

fn default_color_mode() -> String {
    "auto".into()
}

fn default_server_url() -> String {
    "http://127.0.0.1:8080".into()
}

/// Device settings. Calendar data stays in SQLite; nothing in this file is synced.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_db")]
    pub db: String,
    #[serde(default = "default_ui_locale")]
    pub ui_locale: String,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default = "default_color_mode")]
    pub color_mode: String,
    pub active_profile: i64,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    pub username: String,
    pub token: String,
    pub encrypt_key: String,
    /// `None` means no key rotation is in progress. `Some("")` means the previous data was plaintext.
    pub old_encrypt_key: Option<String>,
    pub key_revision: i32,
    pub kdf_salt: String,
    pub pull_cursor: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: default_db(),
            ui_locale: default_ui_locale(),
            color_scheme: default_color_scheme(),
            color_mode: default_color_mode(),
            active_profile: 0,
            server_url: default_server_url(),
            username: String::new(),
            token: String::new(),
            encrypt_key: String::new(),
            old_encrypt_key: None,
            key_revision: 0,
            kdf_salt: String::new(),
            pull_cursor: String::new(),
        }
    }
}

impl Config {
    pub fn load(config_path: &Path) -> anyhow::Result<Self> {
        if !config_path.exists() {
            let config = Self::default();
            config.save(config_path)?;
            return Ok(config);
        }
        let text = std::fs::read_to_string(config_path)
            .with_context(|| format!("Could not read {}", config_path.display()))?;
        ron::from_str(&text).with_context(|| format!("Invalid config in {}", config_path.display()))
    }

    pub fn save(&self, config_path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = config_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Could not create {}", parent.display()))?;
        }
        let text = to_string_pretty(self, PrettyConfig::default())?;
        let tmp = config_path.with_extension("ron.tmp");
        let mut file = File::create(&tmp).with_context(|| format!("Could not write {}", tmp.display()))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, config_path)
            .with_context(|| format!("Could not replace {}", config_path.display()))?;
        Ok(())
    }

    pub fn sqlite_path(&self) -> PathBuf {
        let url = self
            .db
            .trim_start_matches("sqlite://")
            .trim_start_matches("sqlite:");
        let database = url.split('?').next().unwrap_or_default();
        if database.is_empty() || database == ":memory:" {
            PathBuf::from(".")
        } else {
            PathBuf::from(database)
        }
    }

    pub fn catalogs_dir(&self) -> PathBuf {
        let file = self.sqlite_path();
        match file.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join("catalogs"),
            _ => PathBuf::from("catalogs"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_created_and_roundtrips() {
        let path = std::env::temp_dir().join(format!("rurarendar-config-{}.ron", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.ui_locale, "auto");
        assert_eq!(loaded.color_scheme, "modern");
        assert_eq!(loaded.server_url, "http://127.0.0.1:8080");
        assert!(loaded.old_encrypt_key.is_none());

        let mut config = loaded;
        config.ui_locale = "vi".into();
        config.encrypt_key = "secret".into();
        config.old_encrypt_key = Some(String::new());
        config.active_profile = 3;
        config.save(&path).unwrap();

        let again = Config::load(&path).unwrap();
        assert_eq!(again.ui_locale, "vi");
        assert_eq!(again.encrypt_key, "secret");
        assert_eq!(again.old_encrypt_key.as_deref(), Some(""));
        assert_eq!(again.active_profile, 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let path = std::env::temp_dir().join(format!("rurarendar-config-partial-{}.ron", std::process::id()));
        std::fs::write(&path, "(db: \"sqlite::memory:\",)\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.db, "sqlite::memory:");
        assert_eq!(config.color_mode, "auto");
        assert_eq!(config.server_url, "http://127.0.0.1:8080");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sqlite_path_and_catalogs_dir() {
        let mut config = Config::default();
        config.db = "sqlite://rurarendar.sqlite".into();
        assert_eq!(config.sqlite_path(), PathBuf::from("rurarendar.sqlite"));
        assert_eq!(config.catalogs_dir(), PathBuf::from("catalogs"));
        config.db = "sqlite:///tmp/client.sqlite?mode=rwc".into();
        assert_eq!(config.sqlite_path(), PathBuf::from("/tmp/client.sqlite"));
        assert_eq!(config.catalogs_dir(), PathBuf::from("/tmp/catalogs"));
    }
}
