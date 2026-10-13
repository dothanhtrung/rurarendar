use crate::calendar::{
    CalendarTables,
    CivilDate,
    Holiday,
    HolidayTables,
    SolarTermTables,
    UiLang,
    alert_mark,
    all_day_label,
    can_chi,
    day_can_chi,
    day_title_in,
    disabled_mark,
    gregorian_month_title_in,
    hour_label_in,
    lookup,
    lunar_cell_label,
    lunar_month_name_in,
    lunar_period_subtitle_in,
    month_can_chi,
    primary_title,
    tiet_khi_id,
    tiet_khi_label,
    untimed_label,
    week_title_in,
    weekday_long,
    weekday_short,
};
use crate::event::Occurrence;
use crate::state::{
    AppState,
    now_local,
};
use crate::ui::{
    AppWindow,
    Calendar,
    DayCellData,
    DayEvent,
    TimedEvent,
};
use slint::{
    ComponentHandle,
    ModelRc,
    SharedString,
    VecModel,
};

pub fn cell_from_date(
    tables: &mut CalendarTables,
    solar_terms: &mut SolarTermTables,
    holidays: &mut HolidayTables,
    date: CivilDate,
    view_year: i32,
    view_month: u8,
    today: CivilDate,
    hour: u8,
    lang: UiLang,
) -> DayCellData {
    let weekend = date.weekday_monday0() >= 5;
    let lunar = lookup(tables, date).ok();
    let (year_cc, month_cc, day_cc, month_name, label, lday, lmonth, leap) = if let Some(lunar) = &lunar {
        let (ms, mb) = month_can_chi(lunar.year_stem, lunar.month);
        let (ds, db) = day_can_chi(date);
        (
            can_chi(lunar.year_stem, lunar.year_branch),
            can_chi(ms, mb),
            can_chi(ds, db),
            lunar_month_name_in(lunar.month, lunar.leap, lang),
            lunar_cell_label(lunar),
            i32::from(lunar.day),
            i32::from(lunar.month),
            lunar.leap,
        )
    } else {
        (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            0,
            0,
            false,
        )
    };

    let term = solar_terms.on_date(date).cloned();
    let on_holiday = holidays.on_date(tables, date, lunar.as_ref(), term.as_ref());
    let holiday = !on_holiday.is_empty();
    let title = primary_title(&on_holiday)
        .map(|row| SharedString::from(row.name.as_str()))
        .unwrap_or_default();
    let (tiet_khi, tiet_khi_name, tiet_khi_time) = match &term {
        Some(term) => {
            let name = match tiet_khi_id(&term.name) {
                Some(id) => SharedString::from(tiet_khi_label(id, lang)),
                None => SharedString::from(term.name.as_str()),
            };
            (
                true,
                name,
                SharedString::from(term.ict_time.as_str()),
            )
        }
        None => (false, SharedString::new(), SharedString::new()),
    };

    DayCellData {
        gregorian: i32::from(date.day()),
        month: i32::from(date.month()),
        year: date.year(),
        lunar: SharedString::from(label),
        lunar_day: lday,
        lunar_month: lmonth,
        lunar_leap: leap,
        lunar_month_name: SharedString::from(month_name),
        year_can_chi: SharedString::from(year_cc),
        month_can_chi: SharedString::from(month_cc),
        day_can_chi: SharedString::from(day_cc),
        hour_can_chi: SharedString::from(hour_label_in(hour, lang)),
        weekday_name: SharedString::from(weekday_long(lang, date.weekday_monday0())),
        in_month: date.year() == view_year && date.month() == view_month,
        today: date == today,
        weekend,
        holiday,
        tiet_khi,
        tiet_khi_name,
        tiet_khi_time,
        has_event: holiday,
        title,
    }
}

fn overlay_events(cells: &mut [DayCellData], occs: &[Occurrence]) {
    for cell in cells {
        let Ok(date) = CivilDate::new(cell.year, cell.month as u8, cell.gregorian as u8) else {
            continue;
        };
        if let Some(first) = occs.iter().find(|occ| occ.civil == date) {
            cell.has_event = true;
            if cell.title.is_empty() {
                cell.title = SharedString::from(first.title.as_str());
            }
        }
    }
}

fn week_timed(week: &[CivilDate; 7], occs: &[Occurrence]) -> Vec<TimedEvent> {
    let mut out = Vec::new();
    for occ in occs {
        if occ.all_day {
            continue;
        }
        let Some(start) = occ.start_time else {
            continue;
        };
        let Some(day) = week.iter().position(|date| *date == occ.civil) else {
            continue;
        };
        let hours = occ
            .end_time
            .map(|end| {
                let delta = (end.minutes_from_midnight() - start.minutes_from_midnight()) as f32 / 60.0;
                if delta > 0.0 { delta } else { 1.0 }
            })
            .unwrap_or(1.0);
        out.push(TimedEvent {
            day: day as i32,
            start_hour: f32::from(start.hour) + f32::from(start.minute) / 60.0,
            hours,
            title: SharedString::from(occ.title.as_str()),
            holiday: false,
            id: SharedString::from(occ.event_id.0.to_string()),
            enabled: occ.enabled,
        });
    }
    out
}

fn day_event_rows(
    occs: &[Occurrence],
    holidays: &[Holiday],
    date: CivilDate,
    lang: UiLang,
) -> Vec<DayEvent> {
    let all_day = all_day_label(lang);
    let during = untimed_label(lang);
    let mut rows: Vec<DayEvent> = holidays
        .iter()
        .map(|holiday| DayEvent {
            time: SharedString::from(all_day),
            title: SharedString::from(holiday.name.as_str()),
            id: SharedString::new(),
            enabled: true,
        })
        .collect();
    rows.extend(occs.iter().filter(|occ| occ.civil == date).map(|occ| {
        let time = marked_time(event_time_line(occ, all_day, during, lang), occ.enabled, lang);
        DayEvent {
            time: SharedString::from(time),
            title: SharedString::from(occ.title.as_str()),
            id: SharedString::from(occ.event_id.0.to_string()),
            enabled: occ.enabled,
        }
    }));
    rows
}

fn occurrence_time(occ: &Occurrence, all_day: &str, during: &str) -> String {
    if occ.all_day {
        all_day.to_string()
    } else if let Some(start) = occ.start_time {
        match occ.end_time {
            Some(end) => format!("{start} – {end}"),
            None => start.to_string(),
        }
    } else {
        during.to_string()
    }
}

/// One row per event. An alert clock, when set, is part of that row — not a second item.
fn popup_items(occs: &[Occurrence], date: CivilDate, lang: UiLang) -> Vec<DayEvent> {
    let all_day = all_day_label(lang);
    let during = untimed_label(lang);
    let mut rows = Vec::new();
    for occ in occs.iter().filter(|occ| occ.civil == date) {
        rows.push(DayEvent {
            time: SharedString::from(marked_time(event_time_line(occ, all_day, during, lang), occ.enabled, lang)),
            title: SharedString::from(occ.title.as_str()),
            id: SharedString::from(occ.event_id.0.to_string()),
            enabled: occ.enabled,
        });
    }
    rows
}

fn event_time_line(occ: &Occurrence, all_day: &str, during: &str, lang: UiLang) -> String {
    let base = occurrence_time(occ, all_day, during);
    let Some(clock) = occ.reminder else {
        return base;
    };
    format!("{base} · {} {clock}", alert_mark(lang))
}

fn marked_time(time: String, enabled: bool, lang: UiLang) -> String {
    if enabled {
        return time;
    }
    format!("{time} · {}", disabled_mark(lang))
}

pub fn refresh(ui: &AppWindow, state: &mut AppState) {
    let (today, hour) = now_local();
    state.today = today;
    let lang = state.lang();
    let selected = state.selected;
    let view_year = selected.year();
    let view_month = selected.month();
    let _ = state.pin_view(view_year);

    let grid = CivilDate::month_grid(view_year, view_month).expect("valid view month");
    let enabled = state.enabled_profile_ids();
    let occs: Vec<_> = state
        .events
        .expand(&mut state.tables, grid[0], grid[41])
        .into_iter()
        .filter(|occ| occ.profile_id.is_nil() || enabled.contains(&occ.profile_id))
        .collect();
    let mut cells: Vec<DayCellData> = grid
        .iter()
        .map(|date| {
            cell_from_date(
                &mut state.tables,
                &mut state.solar_terms,
                &mut state.holidays,
                *date,
                view_year,
                view_month,
                today,
                hour,
                lang,
            )
        })
        .collect();
    overlay_events(&mut cells, &occs);
    let selected_index = cells
        .iter()
        .position(|cell| {
            cell.year == selected.year()
                && cell.month == i32::from(selected.month())
                && cell.gregorian == i32::from(selected.day())
        })
        .unwrap_or(0) as i32;

    let week = selected.week_days();
    let mut week_days: Vec<DayCellData> = week
        .iter()
        .map(|date| {
            cell_from_date(
                &mut state.tables,
                &mut state.solar_terms,
                &mut state.holidays,
                *date,
                view_year,
                view_month,
                today,
                hour,
                lang,
            )
        })
        .collect();
    overlay_events(&mut week_days, &occs);

    let selected_cell = cells[selected_index as usize].clone();
    let selected_lunar = lookup(&mut state.tables, selected).ok();
    let selected_term = state.solar_terms.on_date(selected).cloned();
    let day_holidays = state.holidays.on_date(
        &mut state.tables,
        selected,
        selected_lunar.as_ref(),
        selected_term.as_ref(),
    );
    let day_events = day_event_rows(&occs, &day_holidays, selected, lang);
    let day_popup_items = popup_items(&occs, selected, lang);
    let subtitle = if selected_cell.lunar_month_name.is_empty() {
        SharedString::new()
    } else if let Ok(lunar) = lookup(&mut state.tables, selected) {
        SharedString::from(lunar_period_subtitle_in(&lunar, lang))
    } else {
        SharedString::new()
    };

    let cal = ui.global::<Calendar>();
    cal.set_cells(ModelRc::new(VecModel::from(cells)));
    cal.set_week_days(ModelRc::new(VecModel::from(week_days)));
    cal.set_week_events(ModelRc::new(VecModel::from(week_timed(&week, &occs))));
    cal.set_day_events(ModelRc::new(VecModel::from(day_events)));
    cal.set_day_popup_items(ModelRc::new(VecModel::from(day_popup_items)));
    cal.set_selected_index(selected_index);
    cal.set_selected(selected_cell);
    cal.set_weekdays(ModelRc::new(VecModel::from(
        (0..7)
            .map(|i| SharedString::from(weekday_short(lang, i)))
            .collect::<Vec<_>>(),
    )));
    cal.set_period_month_title(SharedString::from(gregorian_month_title_in(view_year, view_month, lang)));
    cal.set_period_week_title(SharedString::from(week_title_in(&week, lang)));
    cal.set_period_day_title(SharedString::from(day_title_in(selected, lang)));
    cal.set_period_subtitle(subtitle);
    cal.set_jump_year(SharedString::from(selected.year().to_string()));
    cal.set_jump_month(SharedString::from(selected.month().to_string()));
    cal.set_jump_day(SharedString::from(selected.day().to_string()));
    cal.set_jump_error(SharedString::new());
    cal.set_day_list_status(SharedString::new());
    refresh_profiles(ui, state);
}

fn refresh_profiles(ui: &AppWindow, state: &AppState) {
    let rows: Vec<crate::ui::ProfileRow> = state
        .profiles
        .iter()
        .map(|profile| crate::ui::ProfileRow {
            id: SharedString::from(profile.id.0.to_string()),
            name: SharedString::from(profile.name.as_str()),
            enabled: profile.enabled,
            active: profile.id == state.active_profile,
        })
        .collect();
    let g = ui.global::<crate::ui::Profiles>();
    g.set_items(ModelRc::new(VecModel::from(rows)));
    g.set_active_id(SharedString::from(state.active_profile.0.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        ClockTime,
        EventId,
    };

    fn occ(date: CivilDate, title: &str, reminder: Option<ClockTime>) -> Occurrence {
        Occurrence {
            event_id: EventId(1),
            civil: date,
            title: title.into(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: Some(ClockTime { hour: 10, minute: 30 }),
            reminder,
            enabled: true,
            reminder_enabled: reminder.is_some(),
            profile_id: crate::profile::ProfileId::nil(),
        }
    }

    #[test]
    fn popup_lists_event_then_reminder_for_that_day() {
        let date = CivilDate::new(2026, 10, 5).unwrap();
        let other = CivilDate::new(2026, 10, 6).unwrap();
        let with_reminder = occ(date, "Họp", Some(ClockTime { hour: 8, minute: 0 }));
        let plain = occ(date, "Ăn trưa", None);
        let elsewhere = occ(other, "Khác", Some(ClockTime::REMINDER_DEFAULT));
        let rows = popup_items(&[with_reminder.clone(), plain.clone(), elsewhere], date, UiLang::Vi);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title.as_str(), "Họp");
        assert_eq!(rows[0].time.as_str(), "09:00 – 10:30 · Báo 08:00");
        assert_eq!(rows[0].id.as_str(), with_reminder.event_id.0.to_string());
        assert_eq!(rows[1].title.as_str(), "Ăn trưa");
        assert_eq!(rows[1].time.as_str(), "09:00 – 10:30");
        assert_eq!(rows[1].id.as_str(), plain.event_id.0.to_string());
    }

    #[test]
    fn disabled_occurrence_stays_visible_and_marked_off() {
        let date = CivilDate::new(2026, 10, 5).unwrap();
        let mut row = occ(date, "Họp", Some(ClockTime { hour: 8, minute: 0 }));
        row.enabled = false;
        row.reminder_enabled = false;
        let popup = popup_items(&[row.clone()], date, UiLang::En);
        assert_eq!(popup.len(), 1);
        assert!(!popup[0].enabled);
        assert_eq!(popup[0].title.as_str(), "Họp");
        assert_eq!(popup[0].time.as_str(), "09:00 – 10:30 · Alert 08:00 · off");
        let day = day_event_rows(&[row], &[], date, UiLang::Vi);
        assert_eq!(day.len(), 1);
        assert!(!day[0].enabled);
        assert_eq!(day[0].time.as_str(), "09:00 – 10:30 · Báo 08:00 · tắt");
    }
}
