//! CSV lookup + day offset. Algorithm fallback only when no month-start row covers the date.

use crate::calendar::algorithm;
use crate::calendar::date::CivilDate;
use crate::calendar::tables::{
    CalendarTables,
    MonthStart,
    ParseError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LunarSource {
    Table,
    Computed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LunarDate {
    pub year_fn: i32,
    pub month: u8,
    pub leap: bool,
    pub day: u8,
    pub year_stem: u8,
    pub year_branch: u8,
    pub source: LunarSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LunarYmd {
    pub year_fn: i32,
    pub month: u8,
    pub leap: bool,
    pub day: u8,
}

impl LunarDate {
    pub fn ymd(self) -> LunarYmd {
        LunarYmd {
            year_fn: self.year_fn,
            month: self.month,
            leap: self.leap,
            day: self.day,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertError {
    Parse(ParseError),
    /// No month-start CSV covers this civil date (missing year or before first known start).
    MissingTable {
        date: CivilDate,
    },
    MissingLunarMonth {
        year_fn: i32,
        month: u8,
        leap: bool,
    },
    InvalidLunarDate {
        ymd: LunarYmd,
    },
}

impl From<ParseError> for ConvertError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

pub fn lookup(tables: &mut CalendarTables, date: CivilDate) -> Result<LunarDate, ConvertError> {
    tables.month_starts_covering(date.year())?;
    if let Some(lunar) = lookup_from_table(tables, date) {
        return Ok(lunar);
    }
    algorithm::solar_to_lunar(date)
}

fn lookup_from_table(tables: &CalendarTables, date: CivilDate) -> Option<LunarDate> {
    let starts = tables.starts_for_lookup();
    let start = last_start_on_or_before(&starts, date)?;
    let offset = date.julian_day_number() - start.gregorian.julian_day_number();
    if !(0..=30).contains(&offset) {
        return None;
    }
    if let Some(next) = next_start_after(&starts, start.gregorian) {
        if date >= next.gregorian {
            return None;
        }
    }
    Some(LunarDate {
        year_fn: start.lunar_year_fn,
        month: start.lunar_month,
        leap: start.leap,
        day: (offset + 1) as u8,
        year_stem: start.stem_id,
        year_branch: start.branch_id,
        source: LunarSource::Table,
    })
}

/// Civil date of a lunar day. Day 30 in a 29-day month moves to the last day (SPEC §7.3).
pub fn civil_from_lunar(tables: &mut CalendarTables, ymd: LunarYmd) -> Result<CivilDate, ConvertError> {
    civil_from_lunar_with(tables, ymd, true)
}

/// Birthday / exact-date: fail if that lunar month has no such day.
pub fn civil_from_lunar_exact(tables: &mut CalendarTables, ymd: LunarYmd) -> Result<CivilDate, ConvertError> {
    civil_from_lunar_with(tables, ymd, false)
}

fn civil_from_lunar_with(
    tables: &mut CalendarTables,
    ymd: LunarYmd,
    clamp_to_month: bool,
) -> Result<CivilDate, ConvertError> {
    if ymd.month == 0 || ymd.month > 12 || ymd.day == 0 || ymd.day > 30 {
        return Err(ConvertError::InvalidLunarDate { ymd });
    }
    tables.month_starts_covering(ymd.year_fn)?;
    tables.month_starts_covering(ymd.year_fn.saturating_add(1))?;
    tables.month_starts_covering(ymd.year_fn.saturating_sub(1))?;
    let starts = tables.starts_for_lookup();
    if starts.iter().any(|row| row.lunar_year_fn == ymd.year_fn) {
        return civil_from_table_year(&starts, ymd, clamp_to_month);
    }
    algorithm::lunar_to_solar(ymd, clamp_to_month)
}

fn civil_from_table_year(
    starts: &[MonthStart],
    ymd: LunarYmd,
    clamp_to_month: bool,
) -> Result<CivilDate, ConvertError> {
    let Some(start) = starts
        .iter()
        .copied()
        .find(|row| row.lunar_year_fn == ymd.year_fn && row.lunar_month == ymd.month && row.leap == ymd.leap)
    else {
        return Err(ConvertError::MissingLunarMonth {
            year_fn: ymd.year_fn,
            month: ymd.month,
            leap: ymd.leap,
        });
    };
    let length = month_length_days(starts, start);
    let day = if ymd.day <= length {
        ymd.day
    } else if clamp_to_month {
        length
    } else {
        return Err(ConvertError::InvalidLunarDate { ymd });
    };
    start
        .gregorian
        .add_days(i64::from(day.saturating_sub(1)))
        .map_err(|_| ConvertError::InvalidLunarDate { ymd })
}

fn month_length_days(starts: &[MonthStart], start: MonthStart) -> u8 {
    match next_start_after(starts, start.gregorian) {
        Some(next) => {
            let days = next.gregorian.julian_day_number() - start.gregorian.julian_day_number();
            days.clamp(29, 30) as u8
        }
        None => 30,
    }
}

fn last_start_on_or_before(starts: &[MonthStart], date: CivilDate) -> Option<MonthStart> {
    starts.iter().copied().filter(|row| row.gregorian <= date).next_back()
}

fn next_start_after(starts: &[MonthStart], date: CivilDate) -> Option<MonthStart> {
    starts.iter().copied().find(|row| row.gregorian > date)
}

/// Month Can Chi (五虎遁). Leap months reuse the numbered month's pair.
pub fn month_can_chi(year_stem: u8, lunar_month: u8) -> (u8, u8) {
    let first_stem = match year_stem % 5 {
        0 => 2, // 甲/己 → 丙
        1 => 4, // 乙/庚 → 戊
        2 => 6, // 丙/辛 → 庚
        3 => 8, // 丁/壬 → 壬
        _ => 0, // 戊/癸 → 甲
    };
    let month_index = lunar_month.saturating_sub(1);
    let stem = (first_stem + month_index) % 10;
    let branch = (2 + month_index) % 12; // month 1 = 寅
    (stem, branch)
}

/// Day Can Chi from JDN (Hồ Ngọc Đức: `(jd+9)%10`, `(jd+1)%12`).
pub fn day_can_chi(date: CivilDate) -> (u8, u8) {
    let jd = date.julian_day_number();
    let stem = ((jd + 9).rem_euclid(10)) as u8;
    let branch = ((jd + 1).rem_euclid(12)) as u8;
    (stem, branch)
}

/// 12 double-hours. 23:00–01:00 = Tý (0).
pub fn hour_branch(hour_ict: u8) -> u8 {
    ((u16::from(hour_ict) + 1) / 2 % 12) as u8
}

pub fn hour_range_label(branch: u8) -> &'static str {
    match branch {
        0 => "23–01",
        1 => "01–03",
        2 => "03–05",
        3 => "05–07",
        4 => "07–09",
        5 => "09–11",
        6 => "11–13",
        7 => "13–15",
        8 => "15–17",
        9 => "17–19",
        10 => "19–21",
        11 => "21–23",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::tables::CalendarTables;

    fn tables() -> CalendarTables {
        CalendarTables::new(2026).unwrap()
    }

    #[test]
    fn tet_2026() {
        let mut tables = tables();
        let lunar = lookup(&mut tables, CivilDate::new(2026, 2, 17).unwrap()).unwrap();
        assert_eq!(
            lunar,
            LunarDate {
                year_fn: 2026,
                month: 1,
                leap: false,
                day: 1,
                year_stem: 2,
                year_branch: 6,
                source: LunarSource::Table,
            }
        );
    }

    #[test]
    fn september_2026_grid_matches_hk_demo() {
        use crate::calendar::format::lunar_cell_label;
        let mut tables = tables();
        let grid = CivilDate::month_grid(2026, 9).unwrap();
        let labels: Vec<String> = grid
            .iter()
            .map(|date| {
                lookup(&mut tables, *date)
                    .map(|lunar| lunar_cell_label(&lunar))
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(labels[0], "19"); // 31 Aug
        assert_eq!(labels[1], "20"); // 1 Sep
        assert_eq!(labels[11], "1/8"); // 11 Sep
        assert_eq!(labels[17], "7"); // 17 Sep
        assert_eq!(labels[40], "1/9"); // 10 Oct
    }

    #[test]
    fn demo_september_2026() {
        let mut tables = tables();
        let d = lookup(&mut tables, CivilDate::new(2026, 9, 17).unwrap()).unwrap();
        assert_eq!(d.month, 8);
        assert_eq!(d.day, 7);
        assert!(!d.leap);
        let start = lookup(&mut tables, CivilDate::new(2026, 9, 11).unwrap()).unwrap();
        assert_eq!(start.day, 1);
        assert_eq!(start.month, 8);
    }

    #[test]
    fn uses_previous_year_csv_in_january() {
        let mut tables = tables();
        let d = lookup(&mut tables, CivilDate::new(2026, 1, 1).unwrap()).unwrap();
        assert_eq!(d.year_fn, 2025);
        assert_eq!(d.month, 11);
        assert_eq!(d.day, 13);
        assert_eq!(d.year_stem, 1);
        assert_eq!(d.year_branch, 5);
    }

    #[test]
    fn leap_month_2025() {
        let mut tables = tables();
        let d = lookup(&mut tables, CivilDate::new(2025, 7, 25).unwrap()).unwrap();
        assert!(d.leap);
        assert_eq!(d.month, 6);
        assert_eq!(d.day, 1);
        let later = lookup(&mut tables, CivilDate::new(2025, 8, 1).unwrap()).unwrap();
        assert!(later.leap);
        assert_eq!(later.day, 8);
    }

    #[test]
    fn table_covers_shipped_year() {
        let mut tables = tables();
        let lunar = lookup(&mut tables, CivilDate::new(2026, 9, 17).unwrap()).unwrap();
        assert_eq!(lunar.source, LunarSource::Table);
        assert_eq!(lunar.month, 8);
        assert_eq!(lunar.day, 7);
    }

    #[test]
    fn missing_year_uses_algorithm_not_table() {
        let mut tables = tables();
        let lunar = lookup(&mut tables, CivilDate::new(2030, 1, 15).unwrap()).unwrap();
        assert_eq!(lunar.source, LunarSource::Computed);
        assert!(lunar.month >= 1 && lunar.month <= 12);
    }

    #[test]
    fn days_before_first_csv_row_use_algorithm() {
        let mut tables = tables();
        let lunar = lookup(&mut tables, CivilDate::new(2023, 1, 1).unwrap()).unwrap();
        assert_eq!(lunar.source, LunarSource::Computed);
        let tet = lookup(&mut tables, CivilDate::new(2023, 1, 22).unwrap()).unwrap();
        assert_eq!(tet.source, LunarSource::Table);
        assert_eq!((tet.month, tet.day), (1, 1));
    }

    #[test]
    fn lunar_year_with_csv_does_not_invent_a_leap_month() {
        let mut tables = tables();
        let err = civil_from_lunar_exact(
            &mut tables,
            LunarYmd {
                year_fn: 2026,
                month: 6,
                leap: true,
                day: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ConvertError::MissingLunarMonth { year_fn: 2026, .. }));
    }

    #[test]
    fn month_can_chi_binh_ngo_month_8_is_dinh_dau() {
        assert_eq!(month_can_chi(2, 8), (3, 9));
    }

    #[test]
    fn hour_branch_than_is_15() {
        assert_eq!(hour_branch(15), 8);
        assert_eq!(hour_branch(23), 0);
        assert_eq!(hour_branch(0), 0);
    }

    #[test]
    fn civil_from_lunar_tet_and_trung_thu_2026() {
        let mut tables = tables();
        assert_eq!(
            civil_from_lunar(
                &mut tables,
                LunarYmd {
                    year_fn: 2026,
                    month: 1,
                    leap: false,
                    day: 1
                }
            )
            .unwrap(),
            CivilDate::new(2026, 2, 17).unwrap()
        );
        assert_eq!(
            civil_from_lunar(
                &mut tables,
                LunarYmd {
                    year_fn: 2026,
                    month: 8,
                    leap: false,
                    day: 15
                }
            )
            .unwrap(),
            CivilDate::new(2026, 9, 25).unwrap()
        );
    }

    #[test]
    fn civil_from_lunar_leap_and_short_month() {
        let mut tables = tables();
        assert_eq!(
            civil_from_lunar(
                &mut tables,
                LunarYmd {
                    year_fn: 2025,
                    month: 6,
                    leap: true,
                    day: 1
                }
            )
            .unwrap(),
            CivilDate::new(2025, 7, 25).unwrap()
        );
        // 2026 tháng 2 is 29 days (19 Mar – 16 Apr); day 30 → 29.
        let day29 = civil_from_lunar(
            &mut tables,
            LunarYmd {
                year_fn: 2026,
                month: 2,
                leap: false,
                day: 29,
            },
        )
        .unwrap();
        let day30 = civil_from_lunar(
            &mut tables,
            LunarYmd {
                year_fn: 2026,
                month: 2,
                leap: false,
                day: 30,
            },
        )
        .unwrap();
        assert_eq!(day29, day30);
        assert_eq!(day30, CivilDate::new(2026, 4, 16).unwrap());
    }
}
