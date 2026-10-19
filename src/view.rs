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
use crate::event::{
    expand_event,
    EventStore,
    Occurrence,
};
use crate::ical::{
    IcalEvent,
    IcalFeed,
};
use crate::profile::{Profile, ProfileId};
use crate::state::{
    AppState,
    now_local,
};
use crate::ui::{
    AgendaRow,
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
use std::collections::HashSet;

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
        mark0: -1,
        mark1: -1,
        mark2: -1,
    }
}

/// Profiles wrap in 0..8. iCal feeds wrap in 8..16. Matches ProfileColor.hues.
const PROFILE_COLOR_COUNT: i32 = 8;
const FEED_COLOR_BASE: i32 = 8;
/// Day and popup rows use this so a catalog holiday keeps the holiday mark.
const HOLIDAY_COLOR: i32 = -2;

/// Palette slot for a profile, matching the sidebar list. An event with no profile uses the first slot.
fn profile_color(profiles: &[Profile], id: ProfileId) -> i32 {
    profiles
        .iter()
        .position(|profile| profile.id == id)
        .map(|index| (index as i32) % PROFILE_COLOR_COUNT)
        .unwrap_or(0)
}

/// Palette slot for an iCal feed, matching the sidebar list, in the feed bank (8-15).
fn feed_color(feeds: &[IcalFeed], id: i64) -> i32 {
    feeds
        .iter()
        .position(|feed| feed.id == id)
        .map(|index| FEED_COLOR_BASE + ((index as i32) % PROFILE_COLOR_COUNT))
        .unwrap_or(FEED_COLOR_BASE)
}

/// Color of the catalog that paints holidays: the embedded profile, else any enabled catalog.
fn catalog_color(profiles: &[Profile]) -> i32 {
    let catalog = |profile: &&Profile| profile.enabled && profile.catalog_slug().is_some();
    profiles
        .iter()
        .find(|profile| profile.embedded && catalog(profile))
        .or_else(|| profiles.iter().find(catalog))
        .map(|profile| profile_color(profiles, profile.id))
        .unwrap_or(HOLIDAY_COLOR)
}

fn push_mark(cell: &mut DayCellData, color: i32) {
    if color < 0 {
        return;
    }
    if cell.mark0 == color || cell.mark1 == color || cell.mark2 == color {
        return;
    }
    if cell.mark0 < 0 {
        cell.mark0 = color;
    } else if cell.mark1 < 0 {
        cell.mark1 = color;
    } else if cell.mark2 < 0 {
        cell.mark2 = color;
    }
}

fn overlay_catalog(cells: &mut [DayCellData], color: i32) {
    if color < 0 {
        return;
    }
    for cell in cells {
        if cell.holiday {
            push_mark(cell, color);
        }
    }
}

fn overlay_feeds(cells: &mut [DayCellData], feeds: &[IcalFeed]) {
    for cell in cells {
        let Ok(date) = CivilDate::new(cell.year, cell.month as u8, cell.gregorian as u8) else {
            continue;
        };
        for feed in feeds.iter().filter(|feed| feed.enabled) {
            let Some(event) = feed.events.iter().find(|event| event.covers(date)) else {
                continue;
            };
            cell.has_event = true;
            if cell.title.is_empty() {
                cell.title = SharedString::from(event.summary.as_str());
            }
            push_mark(cell, feed_color(feeds, feed.id));
        }
    }
}

fn overlay_events(cells: &mut [DayCellData], occs: &[Occurrence], profiles: &[Profile]) {
    for cell in cells {
        let Ok(date) = CivilDate::new(cell.year, cell.month as u8, cell.gregorian as u8) else {
            continue;
        };
        for occ in occs.iter().filter(|occ| occ.civil == date) {
            cell.has_event = true;
            if cell.title.is_empty() {
                cell.title = SharedString::from(occ.title.as_str());
            }
            push_mark(cell, profile_color(profiles, occ.profile_id));
        }
    }
}

fn week_timed(week: &[CivilDate; 7], occs: &[Occurrence], profiles: &[Profile]) -> Vec<TimedEvent> {
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
            color: profile_color(profiles, occ.profile_id),
        });
    }
    out
}

fn feed_blocks(week: &[CivilDate; 7], feeds: &[IcalFeed]) -> Vec<TimedEvent> {
    let mut out = Vec::new();
    for (day, date) in week.iter().enumerate() {
        for feed in feeds.iter().filter(|feed| feed.enabled) {
            let color = feed_color(feeds, feed.id);
            for event in feed.events.iter().filter(|event| event.covers(*date)) {
                if event.all_day {
                    continue;
                }
                let Some(start) = event.start_time else {
                    continue;
                };
                let hours = event
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
                    title: SharedString::from(event.summary.as_str()),
                    holiday: false,
                    id: SharedString::new(),
                    enabled: true,
                    color,
                });
            }
        }
    }
    out
}

fn day_event_rows(
    occs: &[Occurrence],
    holidays: &[Holiday],
    date: CivilDate,
    lang: UiLang,
    profiles: &[Profile],
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
            color: catalog_color(profiles),
        })
        .collect();
    rows.extend(occs.iter().filter(|occ| occ.civil == date).map(|occ| {
        let time = marked_time(event_time_line(occ, all_day, during, lang), occ.enabled, lang);
        DayEvent {
            time: SharedString::from(time),
            title: SharedString::from(occ.title.as_str()),
            id: SharedString::from(occ.event_id.0.to_string()),
            enabled: occ.enabled,
            color: profile_color(profiles, occ.profile_id),
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
fn popup_items(occs: &[Occurrence], holidays: &[Holiday], date: CivilDate, lang: UiLang, profiles: &[Profile]) -> Vec<DayEvent> {
    let all_day = all_day_label(lang);
    let during = untimed_label(lang);
    let mut rows: Vec<DayEvent> = holidays
        .iter()
        .map(|holiday| DayEvent {
            time: SharedString::from(all_day),
            title: SharedString::from(holiday.name.as_str()),
            id: SharedString::new(),
            enabled: true,
            color: catalog_color(profiles),
        })
        .collect();
    for occ in occs.iter().filter(|occ| occ.civil == date) {
        rows.push(DayEvent {
            time: SharedString::from(marked_time(event_time_line(occ, all_day, during, lang), occ.enabled, lang)),
            title: SharedString::from(occ.title.as_str()),
            id: SharedString::from(occ.event_id.0.to_string()),
            enabled: occ.enabled,
            color: profile_color(profiles, occ.profile_id),
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

fn profile_visible(profile_id: ProfileId, enabled: &[ProfileId]) -> bool {
    profile_id.is_nil() || enabled.contains(&profile_id)
}

/// Days kept in the scrolling list. An event with nothing in this window still
/// contributes its next later occurrence, so a far one-shot is not dropped.
const AGENDA_HORIZON_DAYS: i64 = 365;

fn upcoming_occurrences(
    events: &EventStore,
    tables: &mut CalendarTables,
    today: CivilDate,
    enabled_profiles: &[ProfileId],
) -> Vec<Occurrence> {
    let until = today.saturating_add_days(AGENDA_HORIZON_DAYS);
    let mut occs: Vec<Occurrence> = events
        .expand(tables, today, until)
        .into_iter()
        .filter(|occ| occ.enabled && profile_visible(occ.profile_id, enabled_profiles))
        .collect();
    let seen: HashSet<_> = occs.iter().map(|occ| occ.event_id).collect();
    let pending: Vec<_> = events
        .events()
        .iter()
        .filter(|event| !seen.contains(&event.id) && profile_visible(event.profile_id, enabled_profiles))
        .cloned()
        .collect();
    for event in &pending {
        let next = expand_event(tables, event, today, CivilDate::MAX)
            .into_iter()
            .find(|occ| occ.enabled && occ.civil >= today);
        if let Some(next) = next {
            occs.push(next);
        }
    }
    occs.sort_by(|a, b| {
        let start = |occ: &Occurrence| occ.start_time.map(|clock| clock.minutes_from_midnight()).unwrap_or(0);
        a.civil.cmp(&b.civil).then(start(a).cmp(&start(b))).then(a.event_id.0.cmp(&b.event_id.0))
    });
    occs
}

/// Next civil day of each enabled catalog holiday and festival, through the agenda window.
/// A yearly rule always has its next day inside that window, so nothing is dropped as "too far".
fn upcoming_observances(
    holidays: &HolidayTables,
    tables: &mut CalendarTables,
    solar_terms: &mut SolarTermTables,
    today: CivilDate,
) -> Vec<Holiday> {
    let until = today.saturating_add_days(AGENDA_HORIZON_DAYS);
    let mut out = Vec::new();
    let mut date = today;
    loop {
        let lunar = lookup(tables, date).ok();
        let term = solar_terms.on_date(date).cloned();
        out.extend(holidays.on_date(tables, date, lunar.as_ref(), term.as_ref()));
        if date >= until {
            break;
        }
        date = match date.add_days(1) {
            Ok(next) => next,
            Err(_) => break,
        };
    }
    out
}

struct ListedObservance {
    date: CivilDate,
    name: String,
    time: Option<String>,
    minutes: i32,
    color: i32,
}

fn listed_holiday(row: &Holiday, color: i32) -> ListedObservance {
    ListedObservance {
        date: row.date,
        name: row.name.clone(),
        time: None,
        minutes: 0,
        color,
    }
}

fn listed_feeds(feeds: &[IcalFeed], today: CivilDate) -> Vec<ListedObservance> {
    let until = today.saturating_add_days(AGENDA_HORIZON_DAYS);
    let mut out = Vec::new();
    for feed in feeds.iter().filter(|feed| feed.enabled) {
        let color = feed_color(feeds, feed.id);
        for event in &feed.events {
            let mut date = if event.start < today { today } else { event.start };
            while date < event.end_exclusive && date <= until {
                let on_start = date == event.start;
                out.push(ListedObservance {
                    date,
                    name: event.summary.clone(),
                    time: if on_start { event.time_label() } else { None },
                    minutes: if on_start { event.minutes() } else { 0 },
                    color,
                });
                date = match date.add_days(1) {
                    Ok(next) => next,
                    Err(_) => break,
                };
            }
        }
    }
    out
}

fn feed_day_rows(feeds: &[IcalFeed], date: CivilDate, lang: UiLang) -> Vec<DayEvent> {
    let all_day = all_day_label(lang);
    let mut rows = Vec::new();
    for feed in feeds.iter().filter(|feed| feed.enabled) {
        let color = feed_color(feeds, feed.id);
        for event in feed.events.iter().filter(|event| event.covers(date)) {
            rows.push(day_row_for_feed(event, all_day, color));
        }
    }
    rows
}

fn day_row_for_feed(event: &IcalEvent, all_day: &str, color: i32) -> DayEvent {
    DayEvent {
        time: SharedString::from(event.time_label().unwrap_or_else(|| all_day.to_string())),
        title: SharedString::from(event.summary.as_str()),
        id: SharedString::new(),
        enabled: true,
        color,
    }
}

fn insert_timed(rows: &mut Vec<DayEvent>, at: usize, timed: Vec<DayEvent>) {
    rows.splice(at..at, timed);
}

fn agenda_rows(
    occs: &[Occurrence],
    observances: &[ListedObservance],
    alerts_only: bool,
    today: CivilDate,
    lang: UiLang,
    profiles: &[Profile],
) -> Vec<AgendaRow> {
    let all_day = all_day_label(lang);
    let during = untimed_label(lang);
    let shown: Vec<&Occurrence> = occs
        .iter()
        .filter(|occ| !alerts_only || occ.reminder_enabled)
        .collect();
    // Built-ins have no alert, so they stay off the reminder list.
    let observances = if alerts_only { &[][..] } else { observances };
    let mut rows = Vec::new();
    let mut last: Option<CivilDate> = None;
    let mut i = 0;
    let mut j = 0;
    while i < shown.len() || j < observances.len() {
        let observance_first = j < observances.len() && (i >= shown.len() || {
            let occ_minutes = shown[i].start_time.map(|clock| clock.minutes_from_midnight()).unwrap_or(0);
            observances[j].date < shown[i].civil
                || (observances[j].date == shown[i].civil && observances[j].minutes <= occ_minutes)
        });
        if observance_first {
            let holiday = &observances[j];
            j += 1;
            note_agenda_day(&mut rows, &mut last, holiday.date, today, lang);
            let time = holiday.time.clone().unwrap_or_else(|| all_day.to_string());
            rows.push(AgendaRow {
                header: false,
                title: SharedString::from(holiday.name.as_str()),
                time: SharedString::from(time),
                id: SharedString::new(),
                enabled: true,
                holiday: holiday.color < 0,
                year: holiday.date.year(),
                month: i32::from(holiday.date.month()),
                day: i32::from(holiday.date.day()),
                today: holiday.date == today,
                color: holiday.color,
            });
        } else {
            let occ = shown[i];
            i += 1;
            note_agenda_day(&mut rows, &mut last, occ.civil, today, lang);
            let time = marked_time(event_time_line(occ, all_day, during, lang), occ.enabled, lang);
            rows.push(AgendaRow {
                header: false,
                title: SharedString::from(occ.title.as_str()),
                time: SharedString::from(time),
                id: SharedString::from(occ.event_id.0.to_string()),
                enabled: occ.enabled,
                holiday: false,
                year: occ.civil.year(),
                month: i32::from(occ.civil.month()),
                day: i32::from(occ.civil.day()),
                today: occ.civil == today,
                color: profile_color(profiles, occ.profile_id),
            });
        }
    }
    rows
}

fn note_agenda_day(rows: &mut Vec<AgendaRow>, last: &mut Option<CivilDate>, date: CivilDate, today: CivilDate, lang: UiLang) {
    if *last == Some(date) {
        return;
    }
    rows.push(AgendaRow {
        header: true,
        title: SharedString::from(day_title_in(date, lang)),
        time: SharedString::new(),
        id: SharedString::new(),
        enabled: true,
        holiday: false,
        year: date.year(),
        month: i32::from(date.month()),
        day: i32::from(date.day()),
        today: date == today,
        color: -1,
    });
    *last = Some(date);
}

pub fn refresh(ui: &AppWindow, state: &mut AppState) {
    let (today, hour) = now_local();
    state.today = today;
    let lang = state.lang();
    let selected = state.selected;
    if !CivilDate::in_month_grid(state.shown_year, state.shown_month, selected) {
        state.shown_year = selected.year();
        state.shown_month = selected.month();
    }
    let view_year = state.shown_year;
    let view_month = state.shown_month;
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
    let catalog = catalog_color(&state.profiles);
    overlay_catalog(&mut cells, catalog);
    overlay_events(&mut cells, &occs, &state.profiles);
    overlay_feeds(&mut cells, &state.feeds);
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
    overlay_catalog(&mut week_days, catalog);
    overlay_events(&mut week_days, &occs, &state.profiles);
    overlay_feeds(&mut week_days, &state.feeds);

    let selected_cell = cells[selected_index as usize].clone();
    let selected_lunar = lookup(&mut state.tables, selected).ok();
    let selected_term = state.solar_terms.on_date(selected).cloned();
    let day_holidays = state.holidays.on_date(
        &mut state.tables,
        selected,
        selected_lunar.as_ref(),
        selected_term.as_ref(),
    );
    let feed_rows = feed_day_rows(&state.feeds, selected, lang);
    let mut day_events = day_event_rows(&occs, &day_holidays, selected, lang, &state.profiles);
    insert_timed(&mut day_events, day_holidays.len(), feed_rows.clone());
    let mut day_popup_items = popup_items(&occs, &day_holidays, selected, lang, &state.profiles);
    insert_timed(&mut day_popup_items, day_holidays.len(), feed_rows);
    let upcoming = upcoming_occurrences(&state.events, &mut state.tables, today, &enabled);
    let mut observances: Vec<ListedObservance> = upcoming_observances(
        &state.holidays,
        &mut state.tables,
        &mut state.solar_terms,
        today,
    )
    .iter()
    .map(|row| listed_holiday(row, catalog))
    .collect();
    observances.extend(listed_feeds(&state.feeds, today));
    observances.sort_by(|a, b| a.date.cmp(&b.date).then(a.minutes.cmp(&b.minutes)).then(a.name.cmp(&b.name)));
    let upcoming_events = agenda_rows(&upcoming, &observances, false, today, lang, &state.profiles);
    let upcoming_alerts = agenda_rows(&upcoming, &observances, true, today, lang, &state.profiles);
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
    let mut week_events = week_timed(&week, &occs, &state.profiles);
    week_events.extend(feed_blocks(&week, &state.feeds));
    cal.set_week_events(ModelRc::new(VecModel::from(week_events)));
    cal.set_day_events(ModelRc::new(VecModel::from(day_events)));
    cal.set_day_popup_items(ModelRc::new(VecModel::from(day_popup_items)));
    cal.set_upcoming_events(ModelRc::new(VecModel::from(upcoming_events)));
    cal.set_upcoming_alerts(ModelRc::new(VecModel::from(upcoming_alerts)));
    cal.set_agenda_status(SharedString::new());
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
    refresh_feeds(ui, state);
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
            embedded: profile.embedded,
            color: profile_color(&state.profiles, profile.id),
        })
        .collect();
    let normal = state.profiles.iter().filter(|profile| !profile.embedded).count() as i32;
    let g = ui.global::<crate::ui::Profiles>();
    g.set_items(ModelRc::new(VecModel::from(rows)));
    g.set_active_id(SharedString::from(state.active_profile.0.to_string()));
    g.set_normal_count(normal);
}

fn refresh_feeds(ui: &AppWindow, state: &AppState) {
    let rows: Vec<crate::ui::ProfileRow> = state
        .feeds
        .iter()
        .map(|feed| crate::ui::ProfileRow {
            id: SharedString::from(feed.id.to_string()),
            name: SharedString::from(feed.name.as_str()),
            enabled: feed.enabled,
            active: false,
            embedded: false,
            color: feed_color(&state.feeds, feed.id),
        })
        .collect();
    ui.global::<crate::ui::Feeds>().set_items(ModelRc::new(VecModel::from(rows)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        create_gregorian_event,
        ClockTime,
        Event,
        EventId,
        NewGregorianEvent,
        Recurrence,
    };
    use crate::profile::ProfileId;

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
        let rows = popup_items(&[with_reminder.clone(), plain.clone(), elsewhere], &[], date, UiLang::Vi, &[]);
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
        let popup = popup_items(&[row.clone()], &[], date, UiLang::En, &[]);
        assert_eq!(popup.len(), 1);
        assert!(!popup[0].enabled);
        assert_eq!(popup[0].title.as_str(), "Họp");
        assert_eq!(popup[0].time.as_str(), "09:00 – 10:30 · Alert 08:00 · off");
        let day = day_event_rows(&[row], &[], date, UiLang::Vi, &[]);
        assert_eq!(day.len(), 1);
        assert!(!day[0].enabled);
        assert_eq!(day[0].time.as_str(), "09:00 – 10:30 · Báo 08:00 · tắt");
    }

    fn greg_event(
        id: i64,
        title: &str,
        date: CivilDate,
        reminder: Option<ClockTime>,
        recurrence: Recurrence,
    ) -> Event {
        let mut event = create_gregorian_event(NewGregorianEvent {
            title: title.into(),
            description: None,
            date,
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: Some(ClockTime { hour: 10, minute: 0 }),
            timezone: "+07:00".into(),
            reminder,
            recurrence,
            birthday: false,
        })
        .unwrap();
        event.id = EventId(id);
        event
    }

    #[test]
    fn agenda_groups_by_date_and_keeps_alerts_separate() {
        let today = CivilDate::new(2026, 10, 15).unwrap();
        let later = CivilDate::new(2026, 10, 16).unwrap();
        let mut alert = occ(today, "Họp", Some(ClockTime { hour: 8, minute: 0 }));
        alert.event_id = EventId(1);
        let mut plain = occ(today, "Ăn trưa", None);
        plain.event_id = EventId(2);
        let mut next = occ(later, "Sinh nhật", Some(ClockTime { hour: 7, minute: 30 }));
        next.event_id = EventId(3);
        let occs = [alert, plain, next];

        let events = agenda_rows(&occs, &[], false, today, UiLang::En, &[]);
        assert!(events[0].header && events[0].today);
        assert_eq!(events[0].title.as_str(), day_title_in(today, UiLang::En));
        assert_eq!(events[1].title.as_str(), "Họp");
        assert_eq!(events[1].time.as_str(), "09:00 – 10:30 · Alert 08:00");
        assert_eq!(events[1].year, 2026);
        assert_eq!(events[1].month, 10);
        assert_eq!(events[1].day, 15);
        assert_eq!(events[2].title.as_str(), "Ăn trưa");
        assert!(events[3].header && !events[3].today);
        assert_eq!(events[4].title.as_str(), "Sinh nhật");
        assert_eq!(events.len(), 5);

        let alerts = agenda_rows(&occs, &[], true, today, UiLang::En, &[]);
        let titles: Vec<_> = alerts.iter().filter(|row| !row.header).map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["Họp", "Sinh nhật"]);
        assert_eq!(alerts.iter().filter(|row| row.header).count(), 2);
    }

    #[test]
    fn upcoming_list_is_from_today_and_includes_a_far_event() {
        let today = CivilDate::new(2026, 10, 15).unwrap();
        let mut store = EventStore::default();
        store.insert(greg_event(
            1,
            "Hôm qua",
            CivilDate::new(2026, 10, 1).unwrap(),
            None,
            Recurrence::None,
        ));
        store.insert(greg_event(
            2,
            "Hôm nay",
            today,
            Some(ClockTime { hour: 8, minute: 0 }),
            Recurrence::None,
        ));
        let mut quiet = greg_event(3, "Ngày mai", CivilDate::new(2026, 10, 16).unwrap(), Some(ClockTime::REMINDER_DEFAULT), Recurrence::None);
        quiet.reminder_enabled = false;
        store.insert(quiet);
        let mut hidden = greg_event(4, "Ẩn", CivilDate::new(2026, 10, 20).unwrap(), None, Recurrence::None);
        hidden.profile_id = ProfileId(9);
        store.insert(hidden);
        let mut off = greg_event(5, "Tắt", CivilDate::new(2026, 10, 21).unwrap(), Some(ClockTime::REMINDER_DEFAULT), Recurrence::None);
        off.enabled = false;
        store.insert(off);
        store.insert(greg_event(
            6,
            "Xa",
            CivilDate::new(2028, 3, 1).unwrap(),
            None,
            Recurrence::None,
        ));
        store.insert(greg_event(
            7,
            "Mỗi ngày",
            today,
            Some(ClockTime { hour: 7, minute: 0 }),
            Recurrence::Daily,
        ));

        let mut tables = CalendarTables::new(2026).unwrap();
        let occs = upcoming_occurrences(&store, &mut tables, today, &[ProfileId(1)]);
        let names: Vec<&str> = occs.iter().map(|occ| occ.title.as_str()).collect();
        assert!(!names.contains(&"Hôm qua"));
        assert!(names.contains(&"Hôm nay"));
        assert!(names.contains(&"Ngày mai"));
        assert!(names.contains(&"Xa"));
        assert!(!names.contains(&"Ẩn"));
        assert!(!names.contains(&"Tắt"));
        let daily = occs.iter().filter(|occ| occ.title == "Mỗi ngày").count();
        assert_eq!(daily, (AGENDA_HORIZON_DAYS + 1) as usize);

        let alerts = agenda_rows(&occs, &[], true, today, UiLang::Vi, &[]);
        let alert_titles: Vec<_> = alerts.iter().filter(|row| !row.header).map(|row| row.title.as_str()).collect();
        assert!(alert_titles.contains(&"Hôm nay"));
        assert!(alert_titles.contains(&"Mỗi ngày"));
        assert!(!alert_titles.contains(&"Ngày mai"));
        assert!(!alert_titles.contains(&"Xa"));
    }

    #[test]
    fn upcoming_list_includes_holidays_and_festivals() {
        let today = CivilDate::new(2026, 10, 15).unwrap();
        let soon = CivilDate::new(2026, 10, 20).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "rurarendar-agenda-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("2026.csv"),
            r#"catalog_id,kind,name_vi,rule,extra
holiday.quoc-khanh,holiday,Quốc khánh,G-09-02,
festival.soon,festival,Sắp tới,G-10-20,
holiday.tet,holiday,Tết Dương lịch,G-01-01,
festival.hung,festival,Giỗ Tổ Hùng Vương,L-3-10,
"#,
        )
        .unwrap();
        let holidays = HolidayTables::from_dir(dir.clone()).unwrap();
        let mut tables = CalendarTables::new(2026).unwrap();
        let mut terms = SolarTermTables::new(2026).unwrap();
        let observances = upcoming_observances(&holidays, &mut tables, &mut terms, today);
        assert!(observances.iter().all(|row| row.date >= today));
        assert!(observances.iter().all(|row| row.date <= today.saturating_add_days(AGENDA_HORIZON_DAYS)));
        let quoc: Vec<_> = observances.iter().filter(|row| row.name == "Quốc khánh").collect();
        assert_eq!(quoc.len(), 1);
        assert_eq!(quoc[0].date, CivilDate::new(2027, 9, 2).unwrap());
        assert!(observances.iter().any(|row| row.name == "Tết Dương lịch" && row.date == CivilDate::new(2027, 1, 1).unwrap()));
        let hung = observances.iter().find(|row| row.name == "Giỗ Tổ Hùng Vương").expect("next lunar festival");
        assert!(hung.date > today);
        assert_eq!(hung.date.year(), 2027);

        let mut meeting = occ(soon, "Họp", Some(ClockTime { hour: 9, minute: 0 }));
        meeting.event_id = EventId(4);
        let listed: Vec<_> = observances.iter().map(|row| listed_holiday(row, HOLIDAY_COLOR)).collect();
        let rows = agenda_rows(std::slice::from_ref(&meeting), &listed, false, today, UiLang::Vi, &[]);
        let soon_rows: Vec<_> = rows.iter().filter(|row| row.year == 2026 && row.month == 10 && row.day == 20).collect();
        assert!(soon_rows[0].header);
        assert!(soon_rows[1].holiday);
        assert_eq!(soon_rows[1].title.as_str(), "Sắp tới");
        assert_eq!(soon_rows[1].time.as_str(), "Cả ngày");
        assert!(soon_rows[1].id.is_empty());
        assert!(!soon_rows[2].holiday);
        assert_eq!(soon_rows[2].title.as_str(), "Họp");

        let alerts = agenda_rows(std::slice::from_ref(&meeting), &listed, true, today, UiLang::Vi, &[]);
        let alert_titles: Vec<_> = alerts.iter().filter(|row| !row.header).map(|row| row.title.as_str()).collect();
        assert_eq!(alert_titles, ["Họp"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn named_profile(id: i64, name: &str) -> crate::profile::Profile {
        crate::profile::Profile {
            id: ProfileId(id),
            name: name.into(),
            enabled: true,
            catalog: String::new(),
            sort: id,
            embedded: false,
        }
    }

    #[test]
    fn each_profile_keeps_its_own_color() {
        let profiles = [named_profile(1, "Nhà"), named_profile(2, "Công việc")];
        assert_eq!(profile_color(&profiles, ProfileId(1)), 0);
        assert_eq!(profile_color(&profiles, ProfileId(2)), 1);
        let many: Vec<_> = (1..=9).map(|id| named_profile(id, "p")).collect();
        assert_eq!(profile_color(&many, ProfileId(9)), 0);

        let date = CivilDate::new(2026, 10, 5).unwrap();
        let mut home = occ(date, "Ở nhà", None);
        home.profile_id = ProfileId(1);
        home.event_id = EventId(1);
        let mut work = occ(date, "Họp", None);
        work.profile_id = ProfileId(2);
        work.event_id = EventId(2);
        let mut again = occ(date, "Cơm", None);
        again.profile_id = ProfileId(1);
        again.event_id = EventId(3);

        let mut tables = CalendarTables::new(2026).unwrap();
        let mut terms = SolarTermTables::new(2026).unwrap();
        let mut holidays = HolidayTables::empty();
        let mut cell = cell_from_date(&mut tables, &mut terms, &mut holidays, date, 2026, 10, date, 9, UiLang::Vi);
        overlay_events(std::slice::from_mut(&mut cell), &[home.clone(), work.clone(), again], &profiles);
        assert!(cell.has_event);
        assert_eq!(cell.mark0, 0);
        assert_eq!(cell.mark1, 1);
        assert_eq!(cell.mark2, -1);

        let blocks = week_timed(&date.week_days(), &[home.clone(), work.clone()], &profiles);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].color, 0);
        assert!(!blocks[0].holiday);
        assert_eq!(blocks[1].color, 1);

        let holiday = Holiday {
            date,
            name: "Lễ".into(),
            kind: crate::calendar::ObservanceKind::Holiday,
        };
        let rows = day_event_rows(&[home], &[holiday], date, UiLang::Vi, &profiles);
        assert_eq!(rows[0].color, HOLIDAY_COLOR);
        assert_eq!(rows[1].color, 0);

        let agenda = agenda_rows(&[work], &[], false, date, UiLang::Vi, &profiles);
        let event = agenda.iter().find(|row| !row.header).expect("event row");
        assert_eq!(event.color, 1);
        assert!(!event.holiday);
    }

    #[test]
    fn embedded_profile_marks_match_its_swatch() {
        let date = CivilDate::new(2026, 10, 5).unwrap();
        let embedded = crate::profile::Profile {
            id: ProfileId(1),
            name: "Lễ".into(),
            enabled: true,
            catalog: crate::profile::EMBEDDED_CATALOG.into(),
            sort: -1,
            embedded: true,
        };
        let personal = named_profile(2, "Nhà");
        let profiles = [embedded, personal];
        let color = catalog_color(&profiles);
        assert_eq!(color, profile_color(&profiles, ProfileId(1)));
        assert_eq!(color, 0);
        assert_ne!(color, HOLIDAY_COLOR);
        assert!(color < FEED_COLOR_BASE);

        let mut tables = CalendarTables::new(2026).unwrap();
        let mut terms = SolarTermTables::new(2026).unwrap();
        let mut holidays = HolidayTables::empty();
        let mut cell = cell_from_date(&mut tables, &mut terms, &mut holidays, date, 2026, 10, date, 9, UiLang::Vi);
        cell.holiday = true;
        overlay_catalog(std::slice::from_mut(&mut cell), color);
        assert_eq!(cell.mark0, color);
        assert_eq!(cell.mark1, -1);

        let holiday = Holiday {
            date,
            name: "Quốc khánh".into(),
            kind: crate::calendar::ObservanceKind::Holiday,
        };
        let rows = day_event_rows(&[], &[holiday.clone()], date, UiLang::Vi, &profiles);
        assert_eq!(rows[0].color, color);
        let listed = listed_holiday(&holiday, color);
        let agenda = agenda_rows(&[], &[listed], false, date, UiLang::Vi, &profiles);
        let event = agenda.iter().find(|row| !row.header).expect("catalog row");
        assert_eq!(event.color, color);
    }

    fn named_feed(id: i64, name: &str, date: CivilDate, all_day: bool) -> IcalFeed {
        IcalFeed {
            id,
            url: format!("https://example.invalid/{id}"),
            name: name.into(),
            enabled: true,
            fetched_at: 0,
            events: vec![IcalEvent {
                summary: name.into(),
                start: date,
                end_exclusive: date.add_days(1).expect("next day"),
                all_day,
                start_time: if all_day { None } else { Some(ClockTime { hour: 11, minute: 0 }) },
                end_time: if all_day { None } else { Some(ClockTime { hour: 12, minute: 0 }) },
            }],
        }
    }

    #[test]
    fn ical_feeds_have_their_own_marks() {
        let date = CivilDate::new(2026, 10, 5).unwrap();
        let feeds = [named_feed(1, "Work", date, true), named_feed(2, "School", date, true)];
        assert_eq!(feed_color(&feeds, 1), FEED_COLOR_BASE);
        assert_eq!(feed_color(&feeds, 2), FEED_COLOR_BASE + 1);
        assert_ne!(feed_color(&feeds, 1), profile_color(&[named_profile(1, "Nhà")], ProfileId(1)));

        let mut tables = CalendarTables::new(2026).unwrap();
        let mut terms = SolarTermTables::new(2026).unwrap();
        let mut holidays = HolidayTables::empty();
        let mut cell = cell_from_date(&mut tables, &mut terms, &mut holidays, date, 2026, 10, date, 9, UiLang::Vi);
        overlay_feeds(std::slice::from_mut(&mut cell), &feeds);
        assert!(!cell.holiday);
        assert!(cell.has_event);
        assert_eq!(cell.mark0, FEED_COLOR_BASE);
        assert_eq!(cell.mark1, FEED_COLOR_BASE + 1);

        let profiles = [named_profile(1, "Nhà")];
        let mut home = occ(date, "Ở nhà", None);
        home.profile_id = ProfileId(1);
        let mut mixed = cell_from_date(&mut tables, &mut terms, &mut holidays, date, 2026, 10, date, 9, UiLang::Vi);
        overlay_events(std::slice::from_mut(&mut mixed), std::slice::from_ref(&home), &profiles);
        overlay_feeds(std::slice::from_mut(&mut mixed), &feeds);
        assert_eq!(mixed.mark0, 0);
        assert_eq!(mixed.mark1, FEED_COLOR_BASE);
        assert_eq!(mixed.mark2, FEED_COLOR_BASE + 1);

        let timed = [named_feed(1, "Call", date, false)];
        let blocks = feed_blocks(&date.week_days(), &timed);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].color, FEED_COLOR_BASE);
        assert!(!blocks[0].holiday);

        let rows = feed_day_rows(&feeds, date, UiLang::Vi);
        assert_eq!(rows[0].color, FEED_COLOR_BASE);
        assert_ne!(rows[0].color, HOLIDAY_COLOR);

        let listed = listed_feeds(&feeds, date);
        let agenda = agenda_rows(&[], &listed, false, date, UiLang::Vi, &[]);
        let event = agenda.iter().find(|row| !row.header).expect("feed row");
        assert_eq!(event.color, FEED_COLOR_BASE);
        assert!(!event.holiday);
    }
}
