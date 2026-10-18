use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ron::ser::{PrettyConfig, to_string_pretty};
use serde::{Deserialize, Serialize};

pub fn android_data_dir() -> PathBuf {
    PathBuf::from("/data/data/net.onlyesterday.rurarendar")
}

fn default_db() -> String {
    if let Ok(path) = std::env::var("RURARENDAR_DB") {
        if path.starts_with("sqlite:") {
            return path;
        }
        return format!("sqlite://{path}");
    }
    if cfg!(target_os = "android") {
        return sqlite_url(&android_data_dir().join("rurarendar.sqlite"));
    }
    let local = Path::new("rurarendar.sqlite");
    if local.is_file() {
        let path = std::env::current_dir().map(|dir| dir.join(local)).unwrap_or_else(|_| local.to_path_buf());
        return sqlite_url(&path);
    }
    sqlite_url(&data_dir().join("rurarendar.sqlite"))
}

fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

/// Android: `/data/data/net.onlyesterday.rurarendar/rurarendar.ron`.
/// Elsewhere: `./rurarendar.ron` if it exists, otherwise `$XDG_CONFIG_HOME/rurarendar/rurarendar.ron`.
/// `Config::load` creates that file when it is missing.
pub fn default_path() -> PathBuf {
    if cfg!(target_os = "android") {
        return android_data_dir().join("rurarendar.ron");
    }
    prefer_existing(Path::new("rurarendar.ron"), config_dir().join("rurarendar.ron"))
}

/// Create a missing database file. Parent directories are created too.
/// `.` and `:memory:` are left alone.
pub fn ensure_db_file(path: &Path) -> anyhow::Result<()> {
    if path.as_os_str().is_empty() || path == Path::new(".") || path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("Could not create {}", parent.display()))?;
    }
    File::create(path).with_context(|| format!("Could not create {}", path.display()))?;
    Ok(())
}

fn config_dir() -> PathBuf {
    xdg_base_from_env("XDG_CONFIG_HOME", ".config").join("rurarendar")
}

fn data_dir() -> PathBuf {
    xdg_base_from_env("XDG_DATA_HOME", ".local/share").join("rurarendar")
}

fn xdg_base_from_env(var: &str, fallback: &str) -> PathBuf {
    let explicit = std::env::var(var).ok();
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    xdg_base(explicit.as_deref(), &home, fallback)
}

fn xdg_base(explicit: Option<&str>, home: &Path, fallback: &str) -> PathBuf {
    match explicit.map(str::trim).filter(|value| !value.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => home.join(fallback),
    }
}

fn prefer_existing(local: &Path, fallback: PathBuf) -> PathBuf {
    if local.is_file() {
        local.to_path_buf()
    } else {
        fallback
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

fn default_animations() -> bool {
    true
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
    #[serde(default = "default_animations")]
    pub animations: bool,
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
            animations: default_animations(),
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
        assert!(loaded.animations);
        assert!(loaded.old_encrypt_key.is_none());

        let mut config = loaded;
        config.ui_locale = "vi".into();
        config.animations = false;
        config.encrypt_key = "secret".into();
        config.old_encrypt_key = Some(String::new());
        config.active_profile = 3;
        config.save(&path).unwrap();

        let again = Config::load(&path).unwrap();
        assert_eq!(again.ui_locale, "vi");
        assert!(!again.animations);
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
        assert!(config.animations);
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

    #[test]
    fn xdg_base_uses_explicit_dir_or_home_fallback() {
        let home = Path::new("/home/user");
        assert_eq!(xdg_base(Some("/cfg"), home, ".config"), PathBuf::from("/cfg"));
        assert_eq!(xdg_base(Some("  "), home, ".config"), home.join(".config"));
        assert_eq!(xdg_base(None, home, ".config"), home.join(".config"));
        assert_eq!(xdg_base(Some(""), home, ".local/share"), home.join(".local/share"));
    }

    #[test]
    fn existing_local_file_beats_the_fallback() {
        let dir = std::env::temp_dir().join(format!("rurarendar-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let local = dir.join("rurarendar.ron");
        let fallback = dir.join("xdg").join("rurarendar.ron");
        assert_eq!(prefer_existing(&local, fallback.clone()), fallback);
        std::fs::write(&local, "x").unwrap();
        assert_eq!(prefer_existing(&local, fallback), local);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn android_files_live_in_the_app_data_dir() {
        let dir = android_data_dir();
        assert_eq!(dir.join("rurarendar.ron"), PathBuf::from("/data/data/net.onlyesterday.rurarendar/rurarendar.ron"));
        assert_eq!(
            dir.join("rurarendar.sqlite"),
            PathBuf::from("/data/data/net.onlyesterday.rurarendar/rurarendar.sqlite")
        );
    }

    #[test]
    fn absent_files_are_created_in_the_default_dir() {
        let dir = std::env::temp_dir().join(format!("rurarendar-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cwd = dir.join("cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        let local = cwd.join("rurarendar.ron");
        let config_file = dir.join("config").join("rurarendar").join("rurarendar.ron");
        let db_file = dir.join("data").join("rurarendar").join("rurarendar.sqlite");

        let chosen = prefer_existing(&local, config_file.clone());
        assert_eq!(chosen, config_file);
        let mut config = Config::default();
        config.db = "sqlite::memory:".into();
        config.save(&chosen).unwrap();
        ensure_db_file(&db_file).unwrap();

        assert!(config_file.is_file());
        assert!(db_file.is_file());
        assert!(!local.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
