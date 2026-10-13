//! Client calendar math. No sqlx, Slint, or actix (DESIGN §2).

mod algorithm;
mod assets;
mod convert;
mod date;
mod format;
mod holidays;
mod solar_terms;
mod tables;

pub use assets::{
    assets_dir,
    catalog_dir,
    parse_catalog_stem,
    scan_catalog_csvs,
    scan_named_catalog_csvs,
    scan_year_csvs,
};
pub use convert::{
    ConvertError,
    LunarDate,
    LunarSource,
    LunarYmd,
    civil_from_lunar,
    civil_from_lunar_exact,
    day_can_chi,
    hour_branch,
    lookup,
    month_can_chi,
};
pub use date::{
    CivilDate,
    DateError,
    MAX_YEAR,
    MIN_YEAR,
    days_in_month,
};
pub use format::{
    UiLang,
    alert_mark,
    all_day_label,
    can_chi,
    day_title,
    day_title_in,
    disabled_mark,
    greg_month_name,
    gregorian_month_title,
    gregorian_month_title_in,
    hour_label,
    hour_label_in,
    lunar_cell_label,
    lunar_month_name,
    lunar_month_name_in,
    lunar_period_subtitle,
    lunar_period_subtitle_in,
    tiet_khi_label,
    untimed_label,
    week_title,
    week_title_in,
    weekday_long,
    weekday_short,
};
pub use holidays::{
    Holiday,
    HolidayTables,
    ObservanceKind,
    primary_title,
};
pub use solar_terms::{
    SolarTerm,
    SolarTermTables,
    tiet_khi_id,
};
pub use tables::{
    CalendarTables,
    MonthStart,
    ParseError,
};

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::Parse(err) => write!(f, "{err}"),
            ConvertError::MissingTable { date } => {
                write!(f, "no month-start table covers {date}")
            }
            ConvertError::MissingLunarMonth { year_fn, month, leap } => {
                write!(
                    f,
                    "no lunar month {year_fn}-{month}{}",
                    if *leap { " nhuận" } else { "" }
                )
            }
            ConvertError::InvalidLunarDate { ymd } => {
                write!(
                    f,
                    "invalid lunar date {}/{}/{}{}",
                    ymd.year_fn,
                    ymd.month,
                    ymd.day,
                    if ymd.leap { " nhuận" } else { "" }
                )
            }
        }
    }
}

impl std::error::Error for ConvertError {}
