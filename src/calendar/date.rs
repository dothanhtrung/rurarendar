//! Proleptic Gregorian civil dates. No time-of-day, no timezone.

use std::fmt;

pub const MIN_YEAR: i32 = 1900;
pub const MAX_YEAR: i32 = 2100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateError {
    Invalid,
    OutOfRange,
}

impl CivilDate {
    pub const MIN: CivilDate = CivilDate {
        year: MIN_YEAR,
        month: 1,
        day: 1,
    };
    pub const MAX: CivilDate = CivilDate {
        year: MAX_YEAR,
        month: 12,
        day: 31,
    };

    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, DateError> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(DateError::Invalid);
        }
        Ok(Self { year, month, day })
    }

    pub fn from_ymd_in_range(year: i32, month: u8, day: u8) -> Result<Self, DateError> {
        let date = Self::new(year, month, day)?;
        if date < Self::MIN || date > Self::MAX {
            return Err(DateError::OutOfRange);
        }
        Ok(date)
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    /// Monday = 0 … Sunday = 6 (SPEC §5.5).
    pub fn weekday_monday0(self) -> u8 {
        let unix = self.to_unix_days();
        ((unix + 3).rem_euclid(7)) as u8
    }

    pub fn add_days(self, days: i64) -> Result<Self, DateError> {
        Self::from_unix_days(self.to_unix_days() + days)
    }

    pub fn saturating_add_days(self, days: i64) -> Self {
        match self.add_days(days) {
            Ok(date) if date < Self::MIN => Self::MIN,
            Ok(date) if date > Self::MAX => Self::MAX,
            Ok(date) => date,
            Err(_) => {
                if days < 0 {
                    Self::MIN
                } else {
                    Self::MAX
                }
            }
        }
    }

    pub fn saturating_add_months(self, delta: i32) -> Self {
        let mut year = self.year;
        let mut month = self.month as i32 + delta;
        while month > 12 {
            month -= 12;
            year += 1;
        }
        while month < 1 {
            month += 12;
            year -= 1;
        }
        let month = month as u8;
        let day = self.day.min(days_in_month(year, month));
        Self::from_ymd_in_range(year, month, day).unwrap_or_else(|_| if delta < 0 { Self::MIN } else { Self::MAX })
    }

    pub fn monday_of_week(self) -> Self {
        self.saturating_add_days(-(self.weekday_monday0() as i64))
    }

    /// Six Monday-start weeks covering `year-month` (42 civil days).
    pub fn month_grid(year: i32, month: u8) -> Result<[CivilDate; 42], DateError> {
        let first = Self::new(year, month, 1)?;
        let start = first.monday_of_week();
        let mut days = [first; 42];
        for (i, slot) in days.iter_mut().enumerate() {
            *slot = start.saturating_add_days(i as i64);
        }
        Ok(days)
    }

    pub fn in_month_grid(year: i32, month: u8, date: Self) -> bool {
        Self::month_grid(year, month).is_ok_and(|grid| grid.contains(&date))
    }

    pub fn week_days(self) -> [CivilDate; 7] {
        let start = self.monday_of_week();
        let mut days = [start; 7];
        for (i, slot) in days.iter_mut().enumerate() {
            *slot = start.saturating_add_days(i as i64);
        }
        days
    }

    /// Julian day number at noon (Hồ Ngọc Đức / Meeus).
    pub fn julian_day_number(self) -> i64 {
        let dd = self.day as i64;
        let mm = self.month as i64;
        let yy = self.year as i64;
        let a = (14 - mm) / 12;
        let y = yy + 4800 - a;
        let m = mm + 12 * a - 3;
        dd + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
    }

    /// Inverse of [`Self::julian_day_number`]. 1970-01-01 is JDN 2440588.
    pub fn from_julian_day_number(jdn: i64) -> Result<Self, DateError> {
        Self::from_unix_days(jdn - 2440588)
    }

    fn to_unix_days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    fn from_unix_days(days: i64) -> Result<Self, DateError> {
        let (year, month, day) = civil_from_days(days);
        Self::new(year, month, day)
    }
}

impl fmt::Display for CivilDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days since 1970-01-01 (Howard Hinnant).
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month as i64 - 3 } else { month as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = (y + if month <= 2 { 1 } else { 0 }) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_epoch_weekday_is_thursday() {
        let d = CivilDate::new(1970, 1, 1).unwrap();
        assert_eq!(d.weekday_monday0(), 3);
    }

    #[test]
    fn september_2026_grid_starts_monday_aug_31() {
        let grid = CivilDate::month_grid(2026, 9).unwrap();
        assert_eq!(grid[0], CivilDate::new(2026, 8, 31).unwrap());
        assert_eq!(grid[1], CivilDate::new(2026, 9, 1).unwrap());
        assert_eq!(grid[1].weekday_monday0(), 1);
        assert_eq!(grid[41], CivilDate::new(2026, 10, 11).unwrap());
    }

    #[test]
    fn add_month_clamps_dom() {
        let jan31 = CivilDate::new(2026, 1, 31).unwrap();
        assert_eq!(jan31.saturating_add_months(1), CivilDate::new(2026, 2, 28).unwrap());
        let jan31_leap = CivilDate::new(2024, 1, 31).unwrap();
        assert_eq!(
            jan31_leap.saturating_add_months(1),
            CivilDate::new(2024, 2, 29).unwrap()
        );
    }

    #[test]
    fn jdn_2000_01_01() {
        assert_eq!(CivilDate::new(2000, 1, 1).unwrap().julian_day_number(), 2451545);
        assert_eq!(
            CivilDate::from_julian_day_number(2451545).unwrap(),
            CivilDate::new(2000, 1, 1).unwrap()
        );
        assert_eq!(
            CivilDate::from_julian_day_number(2440588).unwrap(),
            CivilDate::new(1970, 1, 1).unwrap()
        );
    }

    #[test]
    fn parse_roundtrip_add_days() {
        let d = CivilDate::new(2026, 9, 17).unwrap();
        assert_eq!(d.add_days(0).unwrap(), d);
        assert_eq!(d.add_days(1).unwrap(), CivilDate::new(2026, 9, 18).unwrap());
        assert_eq!(d.add_days(-17).unwrap(), CivilDate::new(2026, 8, 31).unwrap());
    }
}
