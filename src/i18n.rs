//! App chrome strings. `ui_locale` is per device (`auto` | `vi` | `en`) in `rurarendar.ron`.

use crate::calendar::UiLang;
use crate::ui::{
    AppWindow,
    Calendar,
    I18n,
};
use serde::Deserialize;
use slint::{
    ComponentHandle,
    ModelRc,
    SharedString,
    VecModel,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalePref {
    #[default]
    Auto,
    Vi,
    En,
}

impl LocalePref {
    pub fn as_str(self) -> &'static str {
        match self {
            LocalePref::Auto => "auto",
            LocalePref::Vi => "vi",
            LocalePref::En => "en",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "vi" | "vi-vn" | "vietnamese" => LocalePref::Vi,
            "en" | "en-us" | "en-gb" | "english" => LocalePref::En,
            _ => LocalePref::Auto,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => LocalePref::Vi,
            2 => LocalePref::En,
            _ => LocalePref::Auto,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            LocalePref::Auto => 0,
            LocalePref::Vi => 1,
            LocalePref::En => 2,
        }
    }

    pub fn resolve(self) -> UiLang {
        match self {
            LocalePref::Vi => UiLang::Vi,
            LocalePref::En => UiLang::En,
            LocalePref::Auto => system_lang(),
        }
    }
}

pub fn system_lang() -> UiLang {
    let candidates = [
        std::env::var("LC_ALL").ok(),
        std::env::var("LC_MESSAGES").ok(),
        std::env::var("LANG").ok(),
        std::env::var("LANGUAGE").ok(),
    ];
    for raw in candidates.into_iter().flatten() {
        let lower = raw.to_ascii_lowercase();
        let tag = lower.split(['.', ':', '@', ',']).next().unwrap_or(&lower);
        if tag == "c" || tag == "posix" {
            continue;
        }
        if tag.starts_with("en") {
            return UiLang::En;
        }
        if tag.starts_with("vi") {
            return UiLang::Vi;
        }
    }
    UiLang::Vi
}

#[derive(Debug, Deserialize)]
pub struct Bundle {
    pub app_name: String,
    pub tagline: String,
    pub nav_section: String,
    pub nav_day: String,
    pub nav_week: String,
    pub nav_month: String,
    pub nav_event_list: String,
    pub nav_reminder_list: String,
    pub nav_settings: String,
    pub nav_about: String,
    pub hide_sidebar: String,
    pub show_sidebar: String,
    pub prev: String,
    pub next: String,
    pub back: String,
    pub today: String,
    pub event: String,
    pub jump_title: String,
    pub gregorian: String,
    pub lunar: String,
    pub year: String,
    pub month: String,
    pub day: String,
    pub hour: String,
    pub cancel: String,
    pub go_to_date: String,
    pub out_of_range: String,
    pub new_event: String,
    pub edit_event: String,
    pub title_label: String,
    pub title_placeholder: String,
    pub gregorian_date: String,
    pub lunar_year: String,
    pub lunar_month: String,
    pub lunar_day: String,
    pub leap_month: String,
    pub all_day: String,
    pub start: String,
    pub end: String,
    pub reminder: String,
    pub remind_at: String,
    pub reminder_hint: String,
    pub repeat: String,
    pub save: String,
    pub about_body: String,
    pub about_meta: String,
    pub sync_title: String,
    pub sync_body: String,
    pub server: String,
    pub username: String,
    pub token: String,
    pub save_and_sync: String,
    pub sync_now: String,
    pub encrypt_title: String,
    pub encrypt_body: String,
    pub encrypt_key: String,
    pub save_key: String,
    pub reminders_title: String,
    pub reminders_body: String,
    pub display_title: String,
    pub display_body: String,
    pub timezone_title: String,
    pub timezone_body: String,
    pub language_title: String,
    pub language_body: String,
    pub language_label: String,
    pub appearance_title: String,
    pub appearance_body: String,
    pub color_scheme_label: String,
    pub color_mode_label: String,
    pub mode_auto: String,
    pub mode_light: String,
    pub mode_dark: String,
    pub switch_to_light: String,
    pub switch_to_dark: String,
    pub legend_holiday: String,
    pub legend_festival: String,
    pub legend_tiet_khi: String,
    pub legend_event: String,
    pub add_event: String,
    pub empty_day: String,
    pub empty_event_list: String,
    pub empty_reminder_list: String,
    pub day_popup_empty: String,
    pub row_delete: String,
    pub apply_to: String,
    pub scope_day: String,
    pub scope_week: String,
    pub scope_month: String,
    pub scope_series: String,
    pub disable_event: String,
    pub enable_event: String,
    pub delete_event: String,
    pub during_day: String,
    pub solar_term: String,
    pub year_word: String,
    pub lunar_short: String,
    pub lunar_long: String,
    pub syncing: String,
    pub updating_key: String,
    pub recurrence_none: String,
    pub recurrence_daily: String,
    pub recurrence_weekly: String,
    pub recurrence_monthly: String,
    pub recurrence_yearly: String,
    pub language_auto: String,
    pub language_vi: String,
    pub language_en: String,
    pub weekdays_short: Vec<String>,
    pub weekdays_long: Vec<String>,
    pub greg_months: Vec<String>,
    pub lunar_months: Vec<String>,
    pub tiet_khi: Vec<String>,
    pub note_enable_day: String,
    pub note_enable_period: String,
    pub note_enable_series: String,
    pub note_disable_day: String,
    pub note_disable_period: String,
    pub note_disable_series: String,
    pub note_delete_day: String,
    pub note_delete_period: String,
    pub note_delete_series: String,
    pub profiles: String,
    pub profile_default: String,
    pub profile_new: String,
    pub profile_import: String,
    pub profile_name: String,
    pub profile_path: String,
    pub profile_path_hint: String,
    pub profile_one_required: String,
    pub profile_name_required: String,
    pub profile_embedded: String,
    pub profile_embedded_kept: String,
    pub profile_embedded_catalog: String,
    pub ical_section: String,
    pub ical_add: String,
    pub ical_url: String,
    pub ical_hint: String,
    pub ical_url_required: String,
    pub ical_fetching: String,
}

impl Bundle {
    pub fn load(lang: UiLang) -> Self {
        let raw = match lang {
            UiLang::Vi => include_str!("../assets/i18n/vi.json"),
            UiLang::En => include_str!("../assets/i18n/en.json"),
        };
        serde_json::from_str(raw).expect("bundled i18n JSON")
    }
}

fn s(text: &str) -> SharedString {
    SharedString::from(text)
}

fn list(items: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(items.iter().map(|item| s(item)).collect::<Vec<_>>()))
}

pub fn apply(ui: &AppWindow, pref: LocalePref, bundle: &Bundle) {
    let g = ui.global::<I18n>();
    g.set_language_index(pref.index());
    g.set_language_options(ModelRc::new(VecModel::from(vec![
        s(&bundle.language_auto),
        s(&bundle.language_vi),
        s(&bundle.language_en),
    ])));
    g.set_app_name(s(&bundle.app_name));
    g.set_tagline(s(&bundle.tagline));
    g.set_nav_section(s(&bundle.nav_section));
    g.set_nav_day(s(&bundle.nav_day));
    g.set_nav_week(s(&bundle.nav_week));
    g.set_nav_month(s(&bundle.nav_month));
    g.set_nav_event_list(s(&bundle.nav_event_list));
    g.set_nav_reminder_list(s(&bundle.nav_reminder_list));
    g.set_nav_settings(s(&bundle.nav_settings));
    g.set_nav_about(s(&bundle.nav_about));
    g.set_hide_sidebar(s(&bundle.hide_sidebar));
    g.set_show_sidebar(s(&bundle.show_sidebar));
    g.set_prev(s(&bundle.prev));
    g.set_next(s(&bundle.next));
    g.set_back(s(&bundle.back));
    g.set_today(s(&bundle.today));
    g.set_event(s(&bundle.event));
    g.set_jump_title(s(&bundle.jump_title));
    g.set_gregorian(s(&bundle.gregorian));
    g.set_lunar(s(&bundle.lunar));
    g.set_year(s(&bundle.year));
    g.set_month(s(&bundle.month));
    g.set_day(s(&bundle.day));
    g.set_hour(s(&bundle.hour));
    g.set_cancel(s(&bundle.cancel));
    g.set_go_to_date(s(&bundle.go_to_date));
    g.set_out_of_range(s(&bundle.out_of_range));
    g.set_new_event(s(&bundle.new_event));
    g.set_edit_event(s(&bundle.edit_event));
    g.set_title_label(s(&bundle.title_label));
    g.set_title_placeholder(s(&bundle.title_placeholder));
    g.set_gregorian_date(s(&bundle.gregorian_date));
    g.set_lunar_year(s(&bundle.lunar_year));
    g.set_lunar_month(s(&bundle.lunar_month));
    g.set_lunar_day(s(&bundle.lunar_day));
    g.set_leap_month(s(&bundle.leap_month));
    g.set_all_day(s(&bundle.all_day));
    g.set_start(s(&bundle.start));
    g.set_end(s(&bundle.end));
    g.set_reminder(s(&bundle.reminder));
    g.set_remind_at(s(&bundle.remind_at));
    g.set_reminder_hint(s(&bundle.reminder_hint));
    g.set_repeat(s(&bundle.repeat));
    g.set_save(s(&bundle.save));
    g.set_about_body(s(&bundle.about_body));
    g.set_about_meta(s(&bundle.about_meta));
    g.set_sync_title(s(&bundle.sync_title));
    g.set_sync_body(s(&bundle.sync_body));
    g.set_server(s(&bundle.server));
    g.set_username(s(&bundle.username));
    g.set_token(s(&bundle.token));
    g.set_save_and_sync(s(&bundle.save_and_sync));
    g.set_sync_now(s(&bundle.sync_now));
    g.set_encrypt_title(s(&bundle.encrypt_title));
    g.set_encrypt_body(s(&bundle.encrypt_body));
    g.set_encrypt_key(s(&bundle.encrypt_key));
    g.set_save_key(s(&bundle.save_key));
    g.set_reminders_title(s(&bundle.reminders_title));
    g.set_reminders_body(s(&bundle.reminders_body));
    g.set_display_title(s(&bundle.display_title));
    g.set_display_body(s(&bundle.display_body));
    g.set_timezone_title(s(&bundle.timezone_title));
    g.set_timezone_body(s(&bundle.timezone_body));
    g.set_language_title(s(&bundle.language_title));
    g.set_language_body(s(&bundle.language_body));
    g.set_language_label(s(&bundle.language_label));
    g.set_appearance_title(s(&bundle.appearance_title));
    g.set_appearance_body(s(&bundle.appearance_body));
    g.set_color_scheme_label(s(&bundle.color_scheme_label));
    g.set_color_mode_label(s(&bundle.color_mode_label));
    g.set_mode_auto(s(&bundle.mode_auto));
    g.set_mode_light(s(&bundle.mode_light));
    g.set_mode_dark(s(&bundle.mode_dark));
    g.set_switch_to_light(s(&bundle.switch_to_light));
    g.set_switch_to_dark(s(&bundle.switch_to_dark));
    g.set_legend_holiday(s(&bundle.legend_holiday));
    g.set_legend_festival(s(&bundle.legend_festival));
    g.set_legend_tiet_khi(s(&bundle.legend_tiet_khi));
    g.set_legend_event(s(&bundle.legend_event));
    g.set_add_event(s(&bundle.add_event));
    g.set_empty_day(s(&bundle.empty_day));
    g.set_empty_event_list(s(&bundle.empty_event_list));
    g.set_empty_reminder_list(s(&bundle.empty_reminder_list));
    g.set_day_popup_empty(s(&bundle.day_popup_empty));
    g.set_row_delete(s(&bundle.row_delete));
    g.set_apply_to(s(&bundle.apply_to));
    g.set_scope_day(s(&bundle.scope_day));
    g.set_scope_week(s(&bundle.scope_week));
    g.set_scope_month(s(&bundle.scope_month));
    g.set_scope_series(s(&bundle.scope_series));
    g.set_disable_event(s(&bundle.disable_event));
    g.set_enable_event(s(&bundle.enable_event));
    g.set_delete_event(s(&bundle.delete_event));
    g.set_during_day(s(&bundle.during_day));
    g.set_solar_term(s(&bundle.solar_term));
    g.set_year_word(s(&bundle.year_word));
    g.set_lunar_short(s(&bundle.lunar_short));
    g.set_lunar_long(s(&bundle.lunar_long));
    g.set_syncing(s(&bundle.syncing));
    g.set_updating_key(s(&bundle.updating_key));
    g.set_greg_months(list(&bundle.greg_months));
    g.set_weekdays_short(list(&bundle.weekdays_short));
    g.set_weekdays_long(list(&bundle.weekdays_long));
    g.set_lunar_months(list(&bundle.lunar_months));
    g.set_tiet_khi(list(&bundle.tiet_khi));
    g.set_profiles(s(&bundle.profiles));
    g.set_profile_default(s(&bundle.profile_default));
    g.set_profile_new(s(&bundle.profile_new));
    g.set_profile_import(s(&bundle.profile_import));
    g.set_profile_name(s(&bundle.profile_name));
    g.set_profile_path(s(&bundle.profile_path));
    g.set_profile_path_hint(s(&bundle.profile_path_hint));
    g.set_profile_one_required(s(&bundle.profile_one_required));
    g.set_profile_name_required(s(&bundle.profile_name_required));
    g.set_profile_embedded(s(&bundle.profile_embedded));
    g.set_profile_embedded_kept(s(&bundle.profile_embedded_kept));
    g.set_profile_embedded_catalog(s(&bundle.profile_embedded_catalog));
    g.set_ical_section(s(&bundle.ical_section));
    g.set_ical_add(s(&bundle.ical_add));
    g.set_ical_url(s(&bundle.ical_url));
    g.set_ical_hint(s(&bundle.ical_hint));
    g.set_ical_url_required(s(&bundle.ical_url_required));
    g.set_ical_fetching(s(&bundle.ical_fetching));

    let cal = ui.global::<Calendar>();
    cal.set_event_recurrence_options(ModelRc::new(VecModel::from(vec![
        s(&bundle.recurrence_none),
        s(&bundle.recurrence_daily),
        s(&bundle.recurrence_weekly),
        s(&bundle.recurrence_monthly),
        s(&bundle.recurrence_yearly),
    ])));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_parse() {
        let vi = Bundle::load(UiLang::Vi);
        let en = Bundle::load(UiLang::En);
        assert_eq!(vi.nav_settings, "Cài đặt");
        assert_eq!(en.nav_settings, "Settings");
        assert_eq!(vi.today, "Hôm nay");
        assert_eq!(en.today, "Today");
        assert_eq!(vi.weekdays_short.len(), 7);
        assert_eq!(en.greg_months.len(), 12);
        assert_eq!(vi.tiet_khi.len(), 24);
        assert_eq!(en.lunar_months.len(), 12);
    }

    #[test]
    fn auto_english_env() {
        assert_eq!(LocalePref::from_setting("en"), LocalePref::En);
        assert_eq!(LocalePref::from_setting("auto").as_str(), "auto");
        assert_eq!(LocalePref::Vi.resolve(), UiLang::Vi);
        assert_eq!(LocalePref::En.resolve(), UiLang::En);
    }
}
