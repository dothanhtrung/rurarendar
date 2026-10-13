use crate::calendar::{
    CalendarTables,
    CivilDate,
    HolidayTables,
    ParseError,
    SolarTermTables,
    UiLang,
};
use crate::event::EventStore;
use std::path::PathBuf;

use crate::i18n::LocalePref;
use crate::profile::{
    Profile,
    ProfileId,
};

pub struct AppState {
    pub tables: CalendarTables,
    pub solar_terms: SolarTermTables,
    pub holidays: HolidayTables,
    pub selected: CivilDate,
    pub today: CivilDate,
    pub events: EventStore,
    pub locale_pref: LocalePref,
    pub profiles: Vec<Profile>,
    pub active_profile: ProfileId,
    pub catalogs_dir: PathBuf,
}

impl AppState {
    pub fn lang(&self) -> UiLang {
        self.locale_pref.resolve()
    }

    pub fn pin_view(&mut self, year: i32) -> Result<(), ParseError> {
        self.tables.pin_view(year)?;
        self.solar_terms.pin_view(year)?;
        self.holidays.pin_view(year)?;
        Ok(())
    }

    pub fn reload_catalogs(&mut self) -> Result<(), ParseError> {
        let slugs: Vec<String> = self
            .profiles
            .iter()
            .filter(|profile| profile.enabled)
            .filter_map(Profile::catalog_slug)
            .map(str::to_string)
            .collect();
        let dirs = vec![crate::calendar::catalog_dir(), self.catalogs_dir.clone()];
        self.holidays = HolidayTables::from_slugs(slugs, dirs)?;
        Ok(())
    }

    pub fn enabled_profile_ids(&self) -> Vec<ProfileId> {
        self.profiles
            .iter()
            .filter(|profile| profile.enabled)
            .map(|profile| profile.id)
            .collect()
    }
}

pub fn now_local() -> (CivilDate, u8) {
    use chrono::{
        Datelike,
        Local,
        Timelike,
    };
    let now = Local::now();
    let date =
        CivilDate::from_ymd_in_range(now.year(), now.month() as u8, now.day() as u8).unwrap_or_else(|_| CivilDate::MAX);
    (date, now.hour() as u8)
}

pub fn local_timezone() -> String {
    chrono::Local::now().offset().to_string()
}
