//! Tiết khí year CSVs loaded from `assets/24_solar_terms/YYYY.csv` at runtime.
//!
//! Format (no header): `name,d/m,HH:MM` — ICT civil day/month in the file year
//! and the instant to the minute. Missing year → no marks (DESIGN §4).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::calendar::assets::{
    read_year_csv,
    scan_year_csvs,
    solar_terms_dir,
};
use crate::calendar::date::CivilDate;
use crate::calendar::tables::ParseError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolarTerm {
    pub date: CivilDate,
    pub name: String,
    pub ict_time: String,
}

/// Catalog `T-{id}` uses 1 = Lập Xuân … 5 = Thanh Minh (SPEC §5.4).
pub fn tiet_khi_id(name: &str) -> Option<u8> {
    Some(match name.trim() {
        "Spring commences" | "Lập Xuân" | "Lập xuân" => 1,
        "Spring showers" | "Vũ Thủy" | "Vũ thủy" => 2,
        "Insects waken" | "Kinh Trập" | "Kinh trập" => 3,
        "Vernal equinox" | "Xuân Phân" | "Xuân phân" => 4,
        "Bright and clear" | "Thanh Minh" | "Thanh minh" => 5,
        "Corn rain" | "Cốc Vũ" | "Cốc vũ" => 6,
        "Summer commences" | "Lập Hạ" | "Lập hạ" => 7,
        "Corn forms" | "Tiểu Mãn" | "Tiểu mãn" => 8,
        "Corn on ear" | "Mang Chủng" | "Mang chủng" => 9,
        "Summer solstice" | "Hạ Chí" | "Hạ chí" => 10,
        "Moderate heat" | "Tiểu Thử" | "Tiểu thử" => 11,
        "Great heat" | "Đại Thử" | "Đại thử" => 12,
        "Autumn commences" | "Lập Thu" | "Lập thu" => 13,
        "End of heat" | "Xử Thử" | "Xử thử" => 14,
        "White dew" | "Bạch Lộ" | "Bạch lộ" => 15,
        "Autumnal equinox" | "Thu Phân" | "Thu phân" => 16,
        "Cold dew" | "Hàn Lộ" | "Hàn lộ" => 17,
        "Frost" | "Sương Giáng" | "Sương giáng" => 18,
        "Winter commences" | "Lập Đông" | "Lập đông" => 19,
        "Light snow" | "Tiểu Tuyết" | "Tiểu tuyết" => 20,
        "Heavy snow" | "Đại Tuyết" | "Đại tuyết" => 21,
        "Winter solstice" | "Đông Chí" | "Đông chí" => 22,
        "Moderate cold" | "Tiểu Hàn" | "Tiểu hàn" => 23,
        "Severe cold" | "Đại Hàn" | "Đại hàn" => 24,
        _ => return None,
    })
}

#[derive(Debug)]
pub struct SolarTermTables {
    dir: PathBuf,
    files: HashMap<i32, PathBuf>,
    parsed: HashMap<i32, Vec<SolarTerm>>,
    pinned_year: i32,
}

impl SolarTermTables {
    pub fn new(pinned_year: i32) -> Result<Self, ParseError> {
        Self::from_dir(solar_terms_dir(), pinned_year)
    }

    pub fn from_dir(dir: PathBuf, pinned_year: i32) -> Result<Self, ParseError> {
        let files = scan_year_csvs(&dir);
        let mut tables = Self {
            dir,
            files,
            parsed: HashMap::new(),
            pinned_year,
        };
        tables.pin_view(pinned_year)?;
        Ok(tables)
    }

    pub fn has_year(&self, year: i32) -> bool {
        self.files.contains_key(&year)
    }

    pub fn pin_view(&mut self, viewed_year: i32) -> Result<(), ParseError> {
        self.files = scan_year_csvs(&self.dir);
        let keep = [
            self.pinned_year,
            viewed_year.saturating_sub(1),
            viewed_year,
            viewed_year.saturating_add(1),
        ];
        self.parsed.retain(|year, _| keep.contains(year));
        for year in keep {
            self.reload(year)?;
        }
        Ok(())
    }

    pub fn on_date(&mut self, date: CivilDate) -> Option<&SolarTerm> {
        let _ = self.ensure(date.year());
        self.parsed
            .get(&date.year())
            .and_then(|rows| rows.iter().find(|term| term.date == date))
    }

    fn ensure(&mut self, year: i32) -> Result<(), ParseError> {
        if self.parsed.contains_key(&year) {
            return Ok(());
        }
        self.reload(year)
    }

    fn reload(&mut self, year: i32) -> Result<(), ParseError> {
        if !self.files.contains_key(&year) {
            self.files = scan_year_csvs(&self.dir);
        }
        let Some(path) = self.files.get(&year).cloned() else {
            self.parsed.remove(&year);
            return Ok(());
        };
        let csv = read_year_csv(&path)?;
        let rows = parse_solar_term_csv(year, &csv)?;
        self.parsed.insert(year, rows);
        Ok(())
    }
}

pub fn parse_solar_term_csv(year: i32, csv: &str) -> Result<Vec<SolarTerm>, ParseError> {
    let mut rows = Vec::new();
    for (index, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row = parse_row(year, line).map_err(|msg| ParseError(format!("solar term line {}: {msg}", index + 1)))?;
        rows.push(row);
    }
    Ok(rows)
}

fn parse_row(year: i32, line: &str) -> Result<SolarTerm, String> {
    let (name, rest) = line.split_once(',').ok_or("expected name,d/m,HH:MM")?;
    let (day_month, time) = rest.split_once(',').ok_or("expected name,d/m,HH:MM")?;
    if rest.split(',').count() != 2 {
        return Err("expected 3 columns".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("empty name".into());
    }
    Ok(SolarTerm {
        date: parse_day_month(year, day_month.trim())?,
        name: name.to_string(),
        ict_time: parse_hhmm(time.trim())?,
    })
}

fn parse_day_month(year: i32, text: &str) -> Result<CivilDate, String> {
    let (day, month) = text.split_once('/').ok_or_else(|| format!("bad d/m {text}"))?;
    let day: u8 = day.parse().map_err(|_| format!("bad day {text}"))?;
    let month: u8 = month.parse().map_err(|_| format!("bad month {text}"))?;
    CivilDate::new(year, month, day).map_err(|_| format!("invalid date {text} in {year}"))
}

fn parse_hhmm(text: &str) -> Result<String, String> {
    let (hour, minute) = text.split_once(':').ok_or_else(|| format!("bad time {text}"))?;
    let hour: u8 = hour.parse().map_err(|_| format!("bad hour {text}"))?;
    let minute: u8 = minute.parse().map_err(|_| format!("bad minute {text}"))?;
    if hour > 23 || minute > 59 {
        return Err(format!("time out of range {text}"));
    }
    Ok(format!("{hour:02}:{minute:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::assets::{
        read_year_csv,
        solar_terms_dir,
    };
    use std::fs;

    #[test]
    fn folder_2026_has_twenty_four_terms() {
        let path = solar_terms_dir().join("2026.csv");
        let csv = read_year_csv(&path).unwrap();
        let rows = parse_solar_term_csv(2026, &csv).unwrap();
        assert_eq!(rows.len(), 24);
        assert_eq!(rows[0].name, "Moderate cold");
        assert_eq!(rows[0].date, CivilDate::new(2026, 1, 5).unwrap());
        assert_eq!(rows[0].ict_time, "14:23");
        let lap_xuan = rows.iter().find(|t| t.name == "Spring commences").unwrap();
        assert_eq!(lap_xuan.date, CivilDate::new(2026, 2, 4).unwrap());
        assert_eq!(lap_xuan.ict_time, "02:02");
        let thanh_minh = rows.iter().find(|t| t.name == "Bright and clear").unwrap();
        assert_eq!(thanh_minh.date, CivilDate::new(2026, 4, 5).unwrap());
        assert_eq!(thanh_minh.ict_time, "00:40");
    }

    #[test]
    fn lookup_and_missing_year() {
        let mut tables = SolarTermTables::new(2026).unwrap();
        assert!(tables.has_year(2026));
        assert!(!tables.has_year(1900));
        let term = tables.on_date(CivilDate::new(2026, 9, 23).unwrap()).unwrap();
        assert_eq!(term.name, "Autumnal equinox");
        assert_eq!(term.ict_time, "06:05");
        assert!(tables.on_date(CivilDate::new(2026, 9, 17).unwrap()).is_none());
        assert!(tables.on_date(CivilDate::new(2029, 1, 1).unwrap()).is_none());
    }

    #[test]
    fn new_year_file_is_picked_up_without_rebuild() {
        let dir = std::env::temp_dir().join(format!(
            "rurarendar-solar-terms-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut tables = SolarTermTables::from_dir(dir.clone(), 2030).unwrap();
        assert!(!tables.has_year(2030));
        fs::write(dir.join("2030.csv"), "Spring commences,4/2,02:02\n").unwrap();
        tables.pin_view(2030).unwrap();
        assert!(tables.has_year(2030));
        let term = tables.on_date(CivilDate::new(2030, 2, 4).unwrap()).unwrap();
        assert_eq!(term.name, "Spring commences");
        let _ = fs::remove_dir_all(&dir);
    }
}
