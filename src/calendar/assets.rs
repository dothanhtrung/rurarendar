//! Runtime asset folders. Year CSVs are read from disk so a new `YYYY.csv`
//! is picked up without rebuilding.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use crate::calendar::date::{
    MAX_YEAR,
    MIN_YEAR,
};
use crate::calendar::tables::ParseError;

pub fn assets_dir() -> PathBuf {
    if let Ok(path) = env::var("RURARENDAR_ASSETS") {
        return PathBuf::from(path);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let mut candidates = vec![manifest.clone()];
    if let Ok(exe) = env::current_exe()
        && let Some(parent) = exe.parent()
    {
        candidates.push(parent.join("assets"));
        candidates.push(parent.join("../assets"));
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("app/assets"));
        candidates.push(cwd.join("assets"));
    }
    candidates.into_iter().find(|path| path.is_dir()).unwrap_or(manifest)
}

pub fn solar_terms_dir() -> PathBuf {
    assets_dir().join("24_solar_terms")
}

pub fn catalog_dir() -> PathBuf {
    assets_dir().join("catalog")
}

/// `{slug}-{year}.csv`, e.g. `vn-2026.csv`. Year is the last `-NNNN` part.
pub fn parse_catalog_stem(stem: &str) -> Option<(String, i32)> {
    let (slug, year_s) = stem.rsplit_once('-')?;
    if slug.is_empty() {
        return None;
    }
    let year: i32 = year_s.parse().ok()?;
    (MIN_YEAR..=MAX_YEAR).contains(&year).then(|| (slug.to_string(), year))
}

/// Map Gregorian year → `{slug}-{year}.csv` for one profile slug.
pub fn scan_catalog_csvs(dir: &Path, slug: &str) -> HashMap<i32, PathBuf> {
    let mut years = HashMap::new();
    for (file_slug, year, path) in scan_named_catalog_csvs(dir) {
        if file_slug == slug {
            years.insert(year, path);
        }
    }
    years
}

pub fn scan_named_catalog_csvs(dir: &Path) -> Vec<(String, i32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "csv" {
            continue;
        }
        if let Some((slug, year)) = parse_catalog_stem(stem) {
            out.push((slug, year, path));
        }
    }
    out
}

/// Map Gregorian year → `YYYY.csv` found in `dir`. Missing folder → empty.
pub fn scan_year_csvs(dir: &Path) -> HashMap<i32, PathBuf> {
    let mut years = HashMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return years;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "csv" {
            continue;
        }
        let Ok(year) = stem.parse::<i32>() else {
            continue;
        };
        if (MIN_YEAR..=MAX_YEAR).contains(&year) {
            years.insert(year, path);
        }
    }
    years
}

pub fn read_year_csv(path: &Path) -> Result<String, ParseError> {
    fs::read_to_string(path).map_err(|err| ParseError(format!("read {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_assets_dir_exists() {
        let dir = assets_dir();
        assert!(dir.is_dir(), "assets dir missing: {}", dir.display());
        assert!(solar_terms_dir().is_dir());
        assert!(catalog_dir().is_dir());
    }

    #[test]
    fn scan_finds_2026() {
        let terms = scan_year_csvs(&solar_terms_dir());
        assert!(terms.contains_key(&2026), "expected 2026.csv in 24_solar_terms");
    }

    #[test]
    fn catalog_files_declare_profile_and_year() {
        assert_eq!(parse_catalog_stem("vn-2026"), Some(("vn".into(), 2026)));
        assert_eq!(parse_catalog_stem("jp-national-2026"), Some(("jp-national".into(), 2026)));
        assert!(parse_catalog_stem("2026").is_none());
        let files = scan_catalog_csvs(&catalog_dir(), "vn");
        assert!(files.contains_key(&2026), "expected vn-2026.csv");
        assert!(files.contains_key(&1900), "expected vn-1900.csv");
    }
}
