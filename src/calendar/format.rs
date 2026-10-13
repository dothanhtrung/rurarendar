//! Display strings. Calendar math returns ids; copy comes from `app/assets/i18n/`.

use serde::Deserialize;
use std::sync::OnceLock;

use crate::calendar::convert::{
    LunarDate,
    hour_branch,
    hour_range_label,
};
use crate::calendar::date::CivilDate;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiLang {
    #[default]
    Vi,
    En,
}

#[derive(Debug, Deserialize)]
struct Copy {
    stems: Vec<String>,
    branches: Vec<String>,
    weekdays_short: Vec<String>,
    weekdays_long: Vec<String>,
    greg_months: Vec<String>,
    lunar_months: Vec<String>,
    lunar_leap: String,
    tiet_khi: Vec<String>,
    month_title: String,
    week_same_month: String,
    week_same_year: String,
    week_cross_year: String,
    day_title: String,
    lunar_subtitle: String,
    hour_label: String,
    can_chi: String,
    all_day: String,
    untimed: String,
    alert_mark: String,
    disabled_mark: String,
}

fn load(raw: &'static str) -> Copy {
    serde_json::from_str(raw).expect("bundled calendar i18n JSON")
}

fn copy(lang: UiLang) -> &'static Copy {
    static VI: OnceLock<Copy> = OnceLock::new();
    static EN: OnceLock<Copy> = OnceLock::new();
    match lang {
        UiLang::Vi => VI.get_or_init(|| load(include_str!("../../assets/i18n/vi.json"))),
        UiLang::En => EN.get_or_init(|| load(include_str!("../../assets/i18n/en.json"))),
    }
}

fn pick(items: &[String], index: usize) -> &str {
    items.get(index).map(String::as_str).unwrap_or("")
}

fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut keys: Vec<_> = pairs.iter().copied().collect();
    keys.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    let mut out = template.to_string();
    for (k, v) in keys {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

pub fn all_day_label(lang: UiLang) -> &'static str {
    &copy(lang).all_day
}

pub fn untimed_label(lang: UiLang) -> &'static str {
    &copy(lang).untimed
}

pub fn alert_mark(lang: UiLang) -> &'static str {
    &copy(lang).alert_mark
}

pub fn disabled_mark(lang: UiLang) -> &'static str {
    &copy(lang).disabled_mark
}

pub fn can_chi(stem: u8, branch: u8) -> String {
    let c = copy(UiLang::Vi);
    fill(&c.can_chi, &[
        ("stem", pick(&c.stems, stem as usize % 10)),
        ("branch", pick(&c.branches, branch as usize % 12)),
    ])
}

pub fn weekday_short(lang: UiLang, monday0: u8) -> &'static str {
    pick(&copy(lang).weekdays_short, (monday0 as usize).min(6))
}

pub fn weekday_long(lang: UiLang, monday0: u8) -> &'static str {
    pick(&copy(lang).weekdays_long, (monday0 as usize).min(6))
}

pub fn greg_month_name(lang: UiLang, month: u8) -> String {
    pick(&copy(lang).greg_months, (month.clamp(1, 12) - 1) as usize).to_string()
}

pub fn lunar_month_name(month: u8, leap: bool) -> String {
    lunar_month_name_in(month, leap, UiLang::Vi)
}

pub fn lunar_month_name_in(month: u8, leap: bool, lang: UiLang) -> String {
    let c = copy(lang);
    let name = pick(&c.lunar_months, (month.saturating_sub(1) as usize).min(11));
    if leap {
        c.lunar_leap.replace("{name}", name)
    } else {
        name.to_string()
    }
}

pub fn tiet_khi_label(id: u8, lang: UiLang) -> &'static str {
    pick(&copy(lang).tiet_khi, (id.saturating_sub(1) as usize).min(23))
}

/// Month/week cell label: `15`, `1/5`, or `1/5+` on a leap month-start (DESIGN §8.3).
pub fn lunar_cell_label(lunar: &LunarDate) -> String {
    if lunar.day == 1 {
        if lunar.leap {
            format!("1/{}+", lunar.month)
        } else {
            format!("1/{}", lunar.month)
        }
    } else {
        lunar.day.to_string()
    }
}

pub fn gregorian_month_title(year: i32, month: u8) -> String {
    gregorian_month_title_in(year, month, UiLang::Vi)
}

pub fn gregorian_month_title_in(year: i32, month: u8, lang: UiLang) -> String {
    let c = copy(lang);
    fill(&c.month_title, &[
        ("month", pick(&c.greg_months, (month.clamp(1, 12) - 1) as usize)),
        ("year", &year.to_string()),
    ])
}

pub fn week_title(days: &[CivilDate; 7]) -> String {
    week_title_in(days, UiLang::Vi)
}

pub fn week_title_in(days: &[CivilDate; 7], lang: UiLang) -> String {
    let a = days[0];
    let b = days[6];
    let c = copy(lang);
    let d1 = a.day().to_string();
    let d2 = b.day().to_string();
    let month1 = greg_month_name(lang, a.month());
    let month2 = greg_month_name(lang, b.month());
    let y1 = a.year().to_string();
    let y2 = b.year().to_string();
    if a.month() == b.month() {
        fill(&c.week_same_month, &[
            ("d1", &d1),
            ("d2", &d2),
            ("month", &month1),
            ("year", &y1),
        ])
    } else if a.year() == b.year() {
        fill(&c.week_same_year, &[
            ("d1", &d1),
            ("month1", &month1),
            ("d2", &d2),
            ("month2", &month2),
            ("year", &y1),
        ])
    } else {
        fill(&c.week_cross_year, &[
            ("d1", &d1),
            ("month1", &month1),
            ("year1", &y1),
            ("d2", &d2),
            ("month2", &month2),
            ("year2", &y2),
        ])
    }
}

pub fn day_title(date: CivilDate) -> String {
    day_title_in(date, UiLang::Vi)
}

pub fn day_title_in(date: CivilDate, lang: UiLang) -> String {
    let c = copy(lang);
    let month = greg_month_name(lang, date.month());
    fill(&c.day_title, &[
        ("weekday", weekday_long(lang, date.weekday_monday0())),
        ("day", &date.day().to_string()),
        ("month", &month),
        ("year", &date.year().to_string()),
    ])
}

pub fn lunar_period_subtitle(lunar: &LunarDate) -> String {
    lunar_period_subtitle_in(lunar, UiLang::Vi)
}

pub fn lunar_period_subtitle_in(lunar: &LunarDate, lang: UiLang) -> String {
    let c = copy(lang);
    let month = lunar_month_name_in(lunar.month, lunar.leap, lang);
    let year = can_chi(lunar.year_stem, lunar.year_branch);
    fill(&c.lunar_subtitle, &[("month", &month), ("year", &year)])
}

pub fn hour_label(hour_ict: u8) -> String {
    hour_label_in(hour_ict, UiLang::Vi)
}

pub fn hour_label_in(hour_ict: u8, lang: UiLang) -> String {
    let branch = hour_branch(hour_ict);
    let c = copy(lang);
    fill(&c.hour_label, &[
        ("branch", pick(&c.branches, branch as usize)),
        ("range", hour_range_label(branch)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_labels() {
        let mid = LunarDate {
            year_fn: 2026,
            month: 8,
            leap: false,
            day: 7,
            year_stem: 2,
            year_branch: 6,
            source: crate::calendar::LunarSource::Table,
        };
        assert_eq!(lunar_cell_label(&mid), "7");
        let start = LunarDate { day: 1, ..mid };
        assert_eq!(lunar_cell_label(&start), "1/8");
        let leap = LunarDate {
            month: 6,
            leap: true,
            day: 1,
            ..mid
        };
        assert_eq!(lunar_cell_label(&leap), "1/6+");
    }

    #[test]
    fn copy_comes_from_json() {
        assert_eq!(weekday_short(UiLang::Vi, 0), "T2");
        assert_eq!(weekday_short(UiLang::En, 0), "Mon");
        assert_eq!(greg_month_name(UiLang::En, 10), "October");
        assert_eq!(tiet_khi_label(1, UiLang::Vi), "Lập Xuân");
        assert_eq!(all_day_label(UiLang::En), "All day");
    }
}
