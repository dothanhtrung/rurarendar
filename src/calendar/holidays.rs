//! Built-in holidays/festivals from `assets/catalog/YYYY.csv`.
//!
//! Same yearly snapshots as the server: greatest file year `<=` the viewed
//! Gregorian year. `G-*` matches the civil date; `L-*` matches the lunar date
//! (CSV conversion, algorithm only if that year has no table); `T-*` matches a
//! tiết khí start when that year’s solar-term file exists.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::calendar::assets::{
    catalog_dir,
    read_year_csv,
    scan_catalog_csvs,
    scan_named_catalog_csvs,
    scan_year_csvs,
};
use crate::calendar::convert::{
    LunarDate,
    lookup,
};
use crate::calendar::date::CivilDate;
use crate::calendar::solar_terms::{
    SolarTerm,
    tiet_khi_id,
};
use crate::calendar::tables::{
    CalendarTables,
    ParseError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservanceKind {
    Holiday,
    Festival,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holiday {
    pub date: CivilDate,
    pub name: String,
    pub kind: ObservanceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CatalogItem {
    kind: ObservanceKind,
    name: String,
    rule: Rule,
    extra: Vec<(u8, u8)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Rule {
    Gregorian {
        month: u8,
        day: u8,
    },
    Lunar {
        month: u8,
        first_day: u8,
        last_day: LunarLast,
    },
    SolarTerm {
        id: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LunarLast {
    Day(u8),
    EndOfMonth,
}

#[derive(Debug)]
enum CatalogLoad {
    /// Test / imported folder: every `YYYY.csv` and `{slug}-{year}.csv`.
    Dirs(Vec<PathBuf>),
    /// Live app: `{slug}-{year}.csv` in the search dirs for these slugs only.
    Slugs {
        slugs: Vec<String>,
        dirs: Vec<PathBuf>,
    },
}

#[derive(Debug)]
pub struct HolidayTables {
    load: CatalogLoad,
    years: BTreeMap<i32, Vec<CatalogItem>>,
}

impl HolidayTables {
    pub fn new() -> Result<Self, ParseError> {
        Self::from_slugs(vec!["vn".into()], vec![catalog_dir()])
    }

    pub fn from_dir(dir: PathBuf) -> Result<Self, ParseError> {
        Self::from_dirs(vec![dir])
    }

    pub fn from_dirs(dirs: Vec<PathBuf>) -> Result<Self, ParseError> {
        let mut tables = Self {
            load: CatalogLoad::Dirs(dirs),
            years: BTreeMap::new(),
        };
        tables.reload()?;
        Ok(tables)
    }

    pub fn from_slugs(slugs: Vec<String>, dirs: Vec<PathBuf>) -> Result<Self, ParseError> {
        let mut tables = Self {
            load: CatalogLoad::Slugs { slugs, dirs },
            years: BTreeMap::new(),
        };
        tables.reload()?;
        Ok(tables)
    }

    pub fn empty() -> Self {
        Self {
            load: CatalogLoad::Dirs(Vec::new()),
            years: BTreeMap::new(),
        }
    }

    pub fn pin_view(&mut self, _viewed_year: i32) -> Result<(), ParseError> {
        self.reload()
    }

    /// Greatest catalog year `<= year`, if any.
    pub fn resolve_year(&self, year: i32) -> Option<i32> {
        self.years.range(..=year).next_back().map(|(y, _)| *y)
    }

    pub fn on_date(
        &self,
        tables: &mut CalendarTables,
        date: CivilDate,
        lunar: Option<&LunarDate>,
        term: Option<&SolarTerm>,
    ) -> Vec<Holiday> {
        let Some((_, items)) = self.years.range(..=date.year()).next_back() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in items {
            if item_matches(tables, date, lunar, term, item) {
                out.push(Holiday {
                    date,
                    name: item.name.clone(),
                    kind: item.kind,
                });
            }
        }
        out
    }

    fn reload(&mut self) -> Result<(), ParseError> {
        let mut years: BTreeMap<i32, Vec<CatalogItem>> = BTreeMap::new();
        match &self.load {
            CatalogLoad::Dirs(dirs) => {
                for dir in dirs {
                    for (year, path) in scan_year_csvs(dir) {
                        let csv = read_year_csv(&path)?;
                        years.entry(year).or_default().extend(parse_catalog_csv(&csv)?);
                    }
                    for (_slug, year, path) in scan_named_catalog_csvs(dir) {
                        let csv = read_year_csv(&path)?;
                        years.entry(year).or_default().extend(parse_catalog_csv(&csv)?);
                    }
                }
            }
            CatalogLoad::Slugs { slugs, dirs } => {
                let mut seen = Vec::new();
                for slug in slugs {
                    if seen.contains(slug) {
                        continue;
                    }
                    seen.push(slug.clone());
                    for dir in dirs {
                        for (year, path) in scan_catalog_csvs(dir, slug) {
                            let csv = read_year_csv(&path)?;
                            years.entry(year).or_default().extend(parse_catalog_csv(&csv)?);
                        }
                    }
                }
            }
        }
        self.years = years;
        Ok(())
    }
}

fn item_matches(
    tables: &mut CalendarTables,
    date: CivilDate,
    lunar: Option<&LunarDate>,
    term: Option<&SolarTerm>,
    item: &CatalogItem,
) -> bool {
    match &item.rule {
        Rule::Gregorian { month, day } => {
            (date.month() == *month && date.day() == *day)
                || item.extra.iter().any(|(m, d)| date.month() == *m && date.day() == *d)
        }
        Rule::Lunar {
            month,
            first_day,
            last_day,
        } => {
            let Some(lunar) = lunar else {
                return false;
            };
            if lunar.month != *month || lunar.leap {
                return false;
            }
            match last_day {
                LunarLast::EndOfMonth => is_last_day_of_lunar_month(tables, date, lunar),
                LunarLast::Day(end) => lunar.day >= *first_day && lunar.day <= *end,
            }
        }
        Rule::SolarTerm { id } => term.and_then(|t| tiet_khi_id(&t.name)).is_some_and(|got| got == *id),
    }
}

fn is_last_day_of_lunar_month(tables: &mut CalendarTables, date: CivilDate, lunar: &LunarDate) -> bool {
    let Ok(next) = date.add_days(1) else {
        return true;
    };
    match lookup(tables, next) {
        Ok(n) => n.year_fn != lunar.year_fn || n.month != lunar.month || n.leap != lunar.leap,
        Err(_) => true,
    }
}

fn parse_catalog_csv(csv: &str) -> Result<Vec<CatalogItem>, ParseError> {
    let mut rows = Vec::new();
    for (index, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("catalog_id,") {
            continue;
        }
        let item = parse_catalog_line(line).map_err(|msg| ParseError(format!("catalog line {}: {msg}", index + 1)))?;
        if let Some(item) = item {
            rows.push(item);
        }
    }
    Ok(rows)
}

fn parse_catalog_line(line: &str) -> Result<Option<CatalogItem>, String> {
    let cols = csv_fields(line);
    if cols.len() < 4 {
        return Err("expected catalog_id,kind,name_vi,rule".into());
    }
    let kind = match cols[1].trim() {
        "holiday" => ObservanceKind::Holiday,
        "festival" => ObservanceKind::Festival,
        _ => return Ok(None),
    };
    let name = cols[2].trim();
    if name.is_empty() {
        return Err("empty name".into());
    }
    let rule = parse_rule(cols[3].trim())?;
    let extra = cols.get(4).map(|s| parse_extra(s.trim())).unwrap_or_default();
    Ok(Some(CatalogItem {
        kind,
        name: name.to_string(),
        rule,
        extra,
    }))
}

fn parse_rule(rule: &str) -> Result<Rule, String> {
    if let Some(rest) = rule.strip_prefix("G-") {
        let (month, day) = parse_md(rest)?;
        return Ok(Rule::Gregorian { month, day });
    }
    if let Some(rest) = rule.strip_prefix("L-") {
        return parse_lunar_rule(rest);
    }
    if let Some(rest) = rule.strip_prefix("T-") {
        let id: u8 = rest.parse().map_err(|_| format!("bad tiết khí id {rule}"))?;
        if !(1..=24).contains(&id) {
            return Err(format!("tiet_khi_id out of range {rule}"));
        }
        return Ok(Rule::SolarTerm { id });
    }
    Err(format!("unknown rule {rule}"))
}

fn parse_lunar_rule(rest: &str) -> Result<Rule, String> {
    let (month_s, day_s) = rest.split_once('-').ok_or_else(|| format!("bad L- rule {rest}"))?;
    let month: u8 = month_s.parse().map_err(|_| format!("bad lunar month {rest}"))?;
    if !(1..=12).contains(&month) {
        return Err(format!("bad lunar month {rest}"));
    }
    if day_s.eq_ignore_ascii_case("last") {
        return Ok(Rule::Lunar {
            month,
            first_day: 1,
            last_day: LunarLast::EndOfMonth,
        });
    }
    let (first_day, last_day) = if let Some((a, b)) = day_s.split_once(':') {
        let first: u8 = a.parse().map_err(|_| format!("bad L- span {rest}"))?;
        let last: u8 = b.parse().map_err(|_| format!("bad L- span {rest}"))?;
        if first == 0 || last == 0 || first > last || last > 30 {
            return Err(format!("bad L- span {rest}"));
        }
        (first, LunarLast::Day(last))
    } else {
        let day: u8 = day_s.parse().map_err(|_| format!("bad lunar day {rest}"))?;
        if day == 0 || day > 30 {
            return Err(format!("bad lunar day {rest}"));
        }
        (day, LunarLast::Day(day))
    };
    Ok(Rule::Lunar {
        month,
        first_day,
        last_day,
    })
}

fn parse_md(text: &str) -> Result<(u8, u8), String> {
    let (m, d) = text.split_once('-').ok_or_else(|| format!("bad G- {text}"))?;
    let month: u8 = m.parse().map_err(|_| format!("bad month {text}"))?;
    let day: u8 = d.parse().map_err(|_| format!("bad day {text}"))?;
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return Err(format!("bad G- {text}"));
    }
    Ok((month, day))
}

fn parse_extra(text: &str) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for part in text.split([';', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some(rest) = part.strip_prefix("G-") else {
            continue;
        };
        if let Ok(md) = parse_md(rest) {
            out.push(md);
        }
    }
    out
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                chars.next();
                cur.push('"');
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Prefer a national holiday title; otherwise the first festival on that day.
pub fn primary_title(rows: &[Holiday]) -> Option<&Holiday> {
    rows.iter()
        .find(|row| row.kind == ObservanceKind::Holiday)
        .or_else(|| rows.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::solar_terms::SolarTermTables;
    use crate::calendar::tables::CalendarTables;
    use std::fs;

    const SAMPLE: &str = r#"catalog_id,kind,name_vi,rule,extra
holiday.quoc-khanh,holiday,Quốc khánh,G-09-02,
holiday.giai-phong,holiday,"Ngày Giải phóng miền Nam, thống nhất đất nước",G-04-30,
holiday.tet-am,holiday,Tết Âm lịch,L-1-1:5,
festival.hung-vuong,festival,Giỗ Tổ Hùng Vương,L-3-10,
festival.trung-thu,festival,Tết Trung Thu,L-8-15,
festival.thanh-minh,festival,Thanh Minh,T-5,
"#;

    fn sample_dir(year: i32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rurarendar-catalog-{}-{}-{}",
            year,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{year}.csv")), SAMPLE).unwrap();
        dir
    }

    #[test]
    fn gregorian_and_quoted_name() {
        let dir = sample_dir(2026);
        let holidays = HolidayTables::from_dir(dir.clone()).unwrap();
        let mut cal = CalendarTables::new(2026).unwrap();
        let d = CivilDate::new(2026, 9, 2).unwrap();
        let rows = holidays.on_date(&mut cal, d, None, None);
        assert_eq!(rows[0].name, "Quốc khánh");
        let d = CivilDate::new(2026, 4, 30).unwrap();
        let rows = holidays.on_date(&mut cal, d, None, None);
        assert_eq!(rows[0].name, "Ngày Giải phóng miền Nam, thống nhất đất nước");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn later_year_uses_nearest_previous_snapshot() {
        let dir = sample_dir(2026);
        let holidays = HolidayTables::from_dir(dir.clone()).unwrap();
        assert_eq!(holidays.resolve_year(2028), Some(2026));
        assert_eq!(holidays.resolve_year(2025), None);
        let mut cal = CalendarTables::new(2026).unwrap();
        let d = CivilDate::new(2028, 9, 2).unwrap();
        let rows = holidays.on_date(&mut cal, d, None, None);
        assert_eq!(rows[0].name, "Quốc khánh");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lunar_rules_use_conversion_not_civil_d_m() {
        let dir = sample_dir(2026);
        let holidays = HolidayTables::from_dir(dir.clone()).unwrap();
        let mut cal = CalendarTables::new(2026).unwrap();
        let hung = CivilDate::new(2026, 4, 26).unwrap();
        let lunar = lookup(&mut cal, hung).unwrap();
        assert_eq!((lunar.month, lunar.day), (3, 10));
        let rows = holidays.on_date(&mut cal, hung, Some(&lunar), None);
        assert!(rows.iter().any(|h| h.name == "Giỗ Tổ Hùng Vương"));
        let civil_march_10 = CivilDate::new(2026, 3, 10).unwrap();
        let lunar = lookup(&mut cal, civil_march_10).unwrap();
        let rows = holidays.on_date(&mut cal, civil_march_10, Some(&lunar), None);
        assert!(rows.iter().all(|h| h.name != "Giỗ Tổ Hùng Vương"));
        let tet = CivilDate::new(2026, 2, 17).unwrap();
        let lunar = lookup(&mut cal, tet).unwrap();
        let rows = holidays.on_date(&mut cal, tet, Some(&lunar), None);
        assert_eq!(primary_title(&rows).unwrap().name, "Tết Âm lịch");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn thanh_minh_is_solar_term_id_5() {
        let dir = sample_dir(2026);
        let holidays = HolidayTables::from_dir(dir.clone()).unwrap();
        let mut cal = CalendarTables::new(2026).unwrap();
        let mut terms = SolarTermTables::new(2026).unwrap();
        let day = CivilDate::new(2026, 4, 5).unwrap();
        let term = terms.on_date(day).cloned();
        assert_eq!(term.as_ref().map(|t| t.name.as_str()), Some("Bright and clear"));
        let lunar = lookup(&mut cal, day).ok();
        let rows = holidays.on_date(&mut cal, day, lunar.as_ref(), term.as_ref());
        assert!(rows.iter().any(|h| h.name == "Thanh Minh"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn shipped_catalog_loads() {
        let holidays = HolidayTables::new().unwrap();
        assert_eq!(holidays.resolve_year(2026), Some(2026));
        assert_eq!(holidays.resolve_year(2028), Some(2026));
        assert_eq!(holidays.resolve_year(2025), Some(1900));
        let mut cal = CalendarTables::new(2026).unwrap();
        let quoc = CivilDate::new(2026, 9, 2).unwrap();
        let rows = holidays.on_date(&mut cal, quoc, None, None);
        assert!(rows.iter().any(|h| h.name == "Quốc khánh"));
        let hung = CivilDate::new(2026, 4, 26).unwrap();
        let lunar = lookup(&mut cal, hung).unwrap();
        let rows = holidays.on_date(&mut cal, hung, Some(&lunar), None);
        assert!(rows.iter().any(|h| h.name == "Giỗ Tổ Hùng Vương"));
    }
}
