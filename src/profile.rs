//! Device-local profiles. Events and catalogs belong to one profile.

use std::path::Path;

use crate::calendar::{
    parse_catalog_stem,
    scan_catalog_csvs,
    scan_named_catalog_csvs,
    scan_year_csvs,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProfileId(pub i64);

impl ProfileId {
    pub fn nil() -> Self {
        Self(0)
    }

    pub fn is_nil(self) -> bool {
        self.0 == 0
    }

    pub fn parse(text: &str) -> Option<Self> {
        text.trim().parse::<i64>().ok().filter(|id| *id > 0).map(Self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub enabled: bool,
    /// Catalog slug in `{slug}-{year}.csv` (e.g. `vn`). Empty: no catalog.
    pub catalog: String,
    pub sort: i64,
}

impl Profile {
    pub fn catalog_slug(&self) -> Option<&str> {
        let slug = self.catalog.trim();
        if slug.is_empty() {
            None
        } else if slug == "catalog" {
            Some("vn")
        } else {
            Some(slug)
        }
    }
}

pub fn catalog_slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == ' ' || ch == '_' || ch == '-' {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "catalog".into() } else { out }
}

/// Copy catalog CSVs from `src` as `{slug}-{year}.csv` into `dest`.
/// Accepts `YYYY.csv` or `{slug}-{year}.csv` in the source folder.
pub fn import_catalog_dir(src: &Path, dest: &Path, slug: &str) -> Result<usize, String> {
    let mut years = scan_year_csvs(src);
    for (_file_slug, year, path) in scan_named_catalog_csvs(src) {
        years.insert(year, path);
    }
    if years.is_empty() {
        return Err("Không có file {profile}-{year}.csv hoặc YYYY.csv trong thư mục này.".into());
    }
    std::fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    for (year, path) in years {
        let to = dest.join(format!("{slug}-{year}.csv"));
        std::fs::copy(&path, &to).map_err(|err| err.to_string())?;
    }
    Ok(scan_catalog_csvs(dest, slug).len())
}

pub fn detect_catalog_slug(src: &Path, fallback_name: &str) -> String {
    let mut slugs = Vec::new();
    for (slug, _year, _path) in scan_named_catalog_csvs(src) {
        if !slugs.contains(&slug) {
            slugs.push(slug);
        }
    }
    if slugs.len() == 1 {
        slugs.pop().unwrap()
    } else {
        catalog_slug(fallback_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::catalog_dir;

    #[test]
    fn bundled_vietnam_files_use_profile_and_year() {
        let files = scan_catalog_csvs(&catalog_dir(), "vn");
        assert!(files.contains_key(&2026));
        assert!(parse_catalog_stem("vn-1900") == Some(("vn".into(), 1900)));
    }

    #[test]
    fn slug_from_name() {
        assert_eq!(catalog_slug("Japanese Holiday"), "japanese-holiday");
        assert_eq!(catalog_slug("vn"), "vn");
    }
}
