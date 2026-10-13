//! Hồ Ngọc Đức lunisolar conversion (ICT, UTC+7). Used only when a month-start
//! CSV does not cover the date (DESIGN §4, SPEC §5.2).

use crate::calendar::convert::{
    ConvertError,
    LunarDate,
    LunarSource,
    LunarYmd,
};
use crate::calendar::date::{
    CivilDate,
    MAX_YEAR,
    MIN_YEAR,
};

const TIMEZONE: f64 = 7.0;
const PI: f64 = std::f64::consts::PI;
const SYNODIC: f64 = 29.530_588_853;

pub fn solar_to_lunar(date: CivilDate) -> Result<LunarDate, ConvertError> {
    if date < CivilDate::MIN || date > CivilDate::MAX {
        return Err(ConvertError::MissingTable { date });
    }
    let (day, month, year_fn, leap) = convert_solar_to_lunar(date);
    if !(1..=12).contains(&month) || !(1..=30).contains(&day) {
        return Err(ConvertError::MissingTable { date });
    }
    if year_fn < MIN_YEAR || year_fn > MAX_YEAR {
        return Err(ConvertError::MissingTable { date });
    }
    let (year_stem, year_branch) = year_stem_branch(year_fn);
    Ok(LunarDate {
        year_fn,
        month,
        leap,
        day,
        year_stem,
        year_branch,
        source: LunarSource::Computed,
    })
}

pub fn lunar_to_solar(ymd: LunarYmd, clamp_to_month: bool) -> Result<CivilDate, ConvertError> {
    if ymd.month == 0 || ymd.month > 12 || ymd.day == 0 || ymd.day > 30 {
        return Err(ConvertError::InvalidLunarDate { ymd });
    }
    if ymd.year_fn < MIN_YEAR || ymd.year_fn > MAX_YEAR {
        return Err(ConvertError::MissingLunarMonth {
            year_fn: ymd.year_fn,
            month: ymd.month,
            leap: ymd.leap,
        });
    }
    let Some(start_jdn) = lunar_month_start_jdn(ymd) else {
        return Err(ConvertError::MissingLunarMonth {
            year_fn: ymd.year_fn,
            month: ymd.month,
            leap: ymd.leap,
        });
    };
    let k = int_floor((start_jdn as f64 - 2_415_021.076_998_695) / SYNODIC + 0.5);
    let next_jdn = new_moon_day(k + 1);
    let length = (next_jdn - start_jdn).clamp(29, 30) as u8;
    let day = if ymd.day <= length {
        ymd.day
    } else if clamp_to_month {
        length
    } else {
        return Err(ConvertError::InvalidLunarDate { ymd });
    };
    CivilDate::from_julian_day_number(start_jdn + i64::from(day) - 1)
        .map_err(|_| ConvertError::InvalidLunarDate { ymd })
}

pub fn year_stem_branch(year_fn: i32) -> (u8, u8) {
    let n = year_fn - 4;
    ((n.rem_euclid(10)) as u8, (n.rem_euclid(12)) as u8)
}

fn convert_solar_to_lunar(date: CivilDate) -> (u8, u8, i32, bool) {
    let day_number = date.julian_day_number();
    let k = int_floor((day_number as f64 - 2_415_021.076_998_695) / SYNODIC);
    let mut month_start = new_moon_day(k + 1);
    if month_start > day_number {
        month_start = new_moon_day(k);
    }
    let mut a11 = lunar_month_11(date.year());
    let mut b11 = a11;
    let mut lunar_year = if a11 >= month_start {
        a11 = lunar_month_11(date.year() - 1);
        date.year()
    } else {
        b11 = lunar_month_11(date.year() + 1);
        date.year() + 1
    };
    let lunar_day = (day_number - month_start + 1) as u8;
    let diff = int_floor((month_start - a11) as f64 / 29.0);
    let mut lunar_leap = false;
    let mut lunar_month = diff + 11;
    if b11 - a11 > 365 {
        let leap_month_diff = leap_month_offset(a11);
        if diff >= leap_month_diff {
            lunar_month = diff + 10;
            if diff == leap_month_diff {
                lunar_leap = true;
            }
        }
    }
    if lunar_month > 12 {
        lunar_month -= 12;
    }
    if lunar_month >= 11 && diff < 4 {
        lunar_year -= 1;
    }
    (lunar_day, lunar_month as u8, lunar_year, lunar_leap)
}

fn lunar_month_start_jdn(ymd: LunarYmd) -> Option<i64> {
    let (a11, b11) = if ymd.month < 11 {
        (lunar_month_11(ymd.year_fn - 1), lunar_month_11(ymd.year_fn))
    } else {
        (lunar_month_11(ymd.year_fn), lunar_month_11(ymd.year_fn + 1))
    };
    let mut off = i64::from(ymd.month) - 11;
    if off < 0 {
        off += 12;
    }
    if b11 - a11 > 365 {
        let leap_off = leap_month_offset(a11);
        let mut leap_month = leap_off - 2;
        if leap_month < 0 {
            leap_month += 12;
        }
        if ymd.leap && i64::from(ymd.month) != leap_month {
            return None;
        }
        if ymd.leap || off >= leap_off {
            off += 1;
        }
    } else if ymd.leap {
        return None;
    }
    let k = int_floor(0.5 + (a11 as f64 - 2_415_021.076_998_695) / SYNODIC);
    Some(new_moon_day(k + off))
}

fn new_moon(k: i64) -> f64 {
    let k = k as f64;
    let t = k / 1236.85;
    let t2 = t * t;
    let t3 = t2 * t;
    let dr = PI / 180.0;
    let mut jd1 = 2_415_020.759_33 + 29.530_588_68 * k + 0.000_117_8 * t2 - 0.000_000_155 * t3;
    jd1 += 0.000_33 * (166.56 + 132.87 * t - 0.009_173 * t2).to_radians().sin();
    let m = 359.2242 + 29.105_356_08 * k - 0.000_033_3 * t2 - 0.000_003_47 * t3;
    let mpr = 306.0253 + 385.816_918_06 * k + 0.010_730_6 * t2 + 0.000_012_36 * t3;
    let f = 21.2964 + 390.670_506_46 * k - 0.001_652_8 * t2 - 0.000_002_39 * t3;
    let mut c1 = (0.1734 - 0.000_393 * t) * (m * dr).sin() + 0.0021 * (2.0 * dr * m).sin();
    c1 = c1 - 0.4068 * (mpr * dr).sin() + 0.0161 * (dr * 2.0 * mpr).sin();
    c1 -= 0.0004 * (dr * 3.0 * mpr).sin();
    c1 = c1 + 0.0104 * (dr * 2.0 * f).sin() - 0.0051 * (dr * (m + mpr)).sin();
    c1 = c1 - 0.0074 * (dr * (m - mpr)).sin() + 0.0004 * (dr * (2.0 * f + m)).sin();
    c1 = c1 - 0.0004 * (dr * (2.0 * f - m)).sin() - 0.0006 * (dr * (2.0 * f + mpr)).sin();
    c1 = c1 + 0.0010 * (dr * (2.0 * f - mpr)).sin() + 0.0005 * (dr * (2.0 * mpr + m)).sin();
    let deltat = if t < -11.0 {
        0.001 + 0.000_839 * t + 0.000_226_1 * t2 - 0.000_008_45 * t3 - 0.000_000_081 * t * t3
    } else {
        -0.000_278 + 0.000_265 * t + 0.000_262 * t2
    };
    jd1 + c1 - deltat
}

fn new_moon_day(k: i64) -> i64 {
    int_floor(new_moon(k) + 0.5 + TIMEZONE / 24.0)
}

fn sun_longitude(jdn: f64) -> f64 {
    let t = (jdn - 2_451_545.0) / 36_525.0;
    let t2 = t * t;
    let dr = PI / 180.0;
    let m = 357.52910 + 35_999.05030 * t - 0.000_155_9 * t2 - 0.000_000_48 * t * t2;
    let l0 = 280.46645 + 36_000.76983 * t + 0.000_303_2 * t2;
    let mut dl = (1.914_600 - 0.004_817 * t - 0.000_014 * t2) * (dr * m).sin();
    dl += (0.019_993 - 0.000_101 * t) * (dr * 2.0 * m).sin() + 0.000_290 * (dr * 3.0 * m).sin();
    let mut l = (l0 + dl) * dr;
    l -= PI * 2.0 * int_floor(l / (PI * 2.0)) as f64;
    l
}

fn get_sun_longitude(day_number: i64) -> i64 {
    int_floor(sun_longitude(day_number as f64 - 0.5 - TIMEZONE / 24.0) / PI * 6.0)
}

fn lunar_month_11(yy: i32) -> i64 {
    let off = CivilDate::new(yy, 12, 31).expect("31 Dec").julian_day_number() - 2_415_021;
    let k = int_floor(off as f64 / SYNODIC);
    let mut nm = new_moon_day(k);
    if get_sun_longitude(nm) >= 9 {
        nm = new_moon_day(k - 1);
    }
    nm
}

fn leap_month_offset(a11: i64) -> i64 {
    let k = int_floor((a11 as f64 - 2_415_021.076_998_695) / SYNODIC + 0.5);
    let mut last = 0;
    let mut i = 1;
    let mut arc = get_sun_longitude(new_moon_day(k + i));
    while arc != last && i < 14 {
        last = arc;
        i += 1;
        arc = get_sun_longitude(new_moon_day(k + i));
    }
    i - 1
}

fn int_floor(x: f64) -> i64 {
    x.floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tet_dates_from_algorithm() {
        let tet = |y, m, d| solar_to_lunar(CivilDate::new(y, m, d).unwrap()).unwrap();
        let t2023 = tet(2023, 1, 22);
        assert_eq!((t2023.year_fn, t2023.month, t2023.day, t2023.leap), (2023, 1, 1, false));
        assert_eq!(t2023.source, LunarSource::Computed);
        let t2026 = tet(2026, 2, 17);
        assert_eq!((t2026.year_fn, t2026.month, t2026.day, t2026.leap), (2026, 1, 1, false));
        assert_eq!(t2026.year_stem, 2);
        assert_eq!(t2026.year_branch, 6);
    }

    #[test]
    fn roundtrip_and_leap_2025() {
        let civil = CivilDate::new(2025, 7, 25).unwrap();
        let lunar = solar_to_lunar(civil).unwrap();
        assert!(lunar.leap);
        assert_eq!(lunar.month, 6);
        assert_eq!(lunar.day, 1);
        assert_eq!(lunar_to_solar(lunar.ymd(), false).unwrap(), civil);
    }

    #[test]
    fn year_without_csv_converts() {
        let d = CivilDate::new(2030, 1, 15).unwrap();
        let lunar = solar_to_lunar(d).unwrap();
        assert_eq!(lunar.source, LunarSource::Computed);
        assert!(lunar.month >= 1 && lunar.month <= 12);
        assert_eq!(lunar_to_solar(lunar.ymd(), false).unwrap(), d);
    }
}
