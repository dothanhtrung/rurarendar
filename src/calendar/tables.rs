//! Embedded lunar month-start CSVs and the year RAM window (DESIGN §3, §4).

use std::collections::HashMap;

use crate::calendar::date::CivilDate;

const YEAR_CSVS: &[(i32, &str)] = &[
    (2023, include_str!("../../assets/calendar/2023.csv")),
    (2024, include_str!("../../assets/calendar/2024.csv")),
    (2025, include_str!("../../assets/calendar/2025.csv")),
    (2026, include_str!("../../assets/calendar/2026.csv")),
    (2027, include_str!("../../assets/calendar/2027.csv")),
    (2028, include_str!("../../assets/calendar/2028.csv")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthStart {
    pub gregorian: CivilDate,
    pub lunar_year_fn: i32,
    pub stem_id: u8,
    pub branch_id: u8,
    pub lunar_month: u8,
    pub leap: bool,
}

#[derive(Debug)]
pub struct CalendarTables {
    raw: HashMap<i32, &'static str>,
    parsed: HashMap<i32, Vec<MonthStart>>,
    pinned_year: i32,
}

impl CalendarTables {
    pub fn new(pinned_year: i32) -> Result<Self, ParseError> {
        let mut raw = HashMap::new();
        for (year, csv) in YEAR_CSVS {
            raw.insert(*year, *csv);
        }
        let mut tables = Self {
            raw,
            parsed: HashMap::new(),
            pinned_year,
        };
        tables.pin_view(pinned_year)?;
        Ok(tables)
    }

    pub fn has_year(&self, year: i32) -> bool {
        self.raw.contains_key(&year)
    }

    /// Pin today; keep the viewed year and its neighbours; evict the rest.
    pub fn pin_view(&mut self, viewed_year: i32) -> Result<(), ParseError> {
        let keep = [
            self.pinned_year,
            viewed_year.saturating_sub(1),
            viewed_year,
            viewed_year.saturating_add(1),
        ];
        for year in keep {
            self.ensure(year)?;
        }
        self.parsed.retain(|year, _| keep.contains(year));
        Ok(())
    }

    pub fn month_starts_covering(&mut self, year: i32) -> Result<(), ParseError> {
        self.pin_view(year)
    }

    pub fn starts_for_lookup(&self) -> Vec<MonthStart> {
        let mut all: Vec<MonthStart> = self.parsed.values().flatten().copied().collect();
        all.sort_by_key(|row| row.gregorian);
        all.dedup_by_key(|row| row.gregorian);
        all
    }

    fn ensure(&mut self, year: i32) -> Result<(), ParseError> {
        if self.parsed.contains_key(&year) {
            return Ok(());
        }
        let Some(csv) = self.raw.get(&year).copied() else {
            return Ok(());
        };
        let rows = parse_month_start_csv(csv)?;
        self.parsed.insert(year, rows);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

pub fn parse_month_start_csv(csv: &str) -> Result<Vec<MonthStart>, ParseError> {
    let mut lines = csv.lines();
    let header = lines.next().ok_or_else(|| ParseError("empty CSV".into()))?;
    const EXPECTED: &str = "gregorian_date,lunar_year_fn,stem_id,branch_id,lunar_month,leap,lunar_day";
    if header.trim() != EXPECTED {
        return Err(ParseError(format!("unexpected header: {header}")));
    }
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row = parse_row(line).map_err(|msg| ParseError(format!("line {}: {msg}", index + 2)))?;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(ParseError("no month-start rows".into()));
    }
    Ok(rows)
}

fn parse_row(line: &str) -> Result<MonthStart, String> {
    let cols: Vec<&str> = line.split(',').collect();
    if cols.len() != 7 {
        return Err(format!("expected 7 columns, got {}", cols.len()));
    }
    let gregorian = parse_iso_date(cols[0])?;
    let lunar_year_fn: i32 = cols[1].parse().map_err(|_| "bad lunar_year_fn")?;
    let stem_id: u8 = cols[2].parse().map_err(|_| "bad stem_id")?;
    let branch_id: u8 = cols[3].parse().map_err(|_| "bad branch_id")?;
    let lunar_month: u8 = cols[4].parse().map_err(|_| "bad lunar_month")?;
    let leap = match cols[5] {
        "0" => false,
        "1" => true,
        other => return Err(format!("bad leap {other}")),
    };
    let lunar_day: u8 = cols[6].parse().map_err(|_| "bad lunar_day")?;
    if stem_id > 9 || branch_id > 11 || !(1..=12).contains(&lunar_month) {
        return Err("id or month out of range".into());
    }
    if lunar_day != 1 {
        return Err("lunar_day must be 1".into());
    }
    Ok(MonthStart {
        gregorian,
        lunar_year_fn,
        stem_id,
        branch_id,
        lunar_month,
        leap,
    })
}

fn parse_iso_date(text: &str) -> Result<CivilDate, String> {
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("bad date {text}"));
    }
    let year: i32 = parts[0].parse().map_err(|_| "bad year")?;
    let month: u8 = parts[1].parse().map_err(|_| "bad month")?;
    let day: u8 = parts[2].parse().map_err(|_| "bad day")?;
    CivilDate::new(year, month, day).map_err(|_| format!("invalid date {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_2026_parses() {
        let csv = YEAR_CSVS.iter().find(|(y, _)| *y == 2026).unwrap().1;
        let rows = parse_month_start_csv(csv).unwrap();
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[1].gregorian, CivilDate::new(2026, 2, 17).unwrap());
        assert_eq!(rows[1].lunar_year_fn, 2026);
        assert_eq!(rows[1].stem_id, 2);
        assert_eq!(rows[1].branch_id, 6);
        assert_eq!(rows[1].lunar_month, 1);
        assert!(!rows[1].leap);
    }

    #[test]
    fn ships_known_years() {
        let tables = CalendarTables::new(2026).unwrap();
        assert!(tables.has_year(2026));
        assert!(tables.has_year(2023));
        assert!(!tables.has_year(1900));
    }

    #[test]
    fn leap_row_2025() {
        let csv = YEAR_CSVS.iter().find(|(y, _)| *y == 2025).unwrap().1;
        let rows = parse_month_start_csv(csv).unwrap();
        let leap = rows.iter().find(|r| r.leap).unwrap();
        assert_eq!(leap.gregorian, CivilDate::new(2025, 7, 25).unwrap());
        assert_eq!(leap.lunar_month, 6);
    }
}
