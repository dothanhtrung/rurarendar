use crate::calendar::{
    CivilDate,
    DateError,
    LunarYmd,
    UiLang,
    days_in_month,
    lookup,
};
use crate::i18n::Bundle;
use crate::event::{
    ActionEffect,
    CalendarBasis,
    ClockTime,
    Event,
    EventId,
    ItemAction,
    ItemKind,
    ItemScope,
    NewGregorianEvent,
    NewLunarEvent,
    Recurrence,
    apply_item_action,
    create_gregorian_event,
    create_lunar_event,
};
use crate::state::{
    AppState,
    local_timezone,
};
use crate::ui::{
    AppPage,
    AppWindow,
    Calendar,
    Chrome,
    I18n,
};
use slint::{
    ComponentHandle,
    ModelRc,
    SharedString,
    VecModel,
};

pub fn parse_jump(year: &str, month: &str, day: &str) -> Result<CivilDate, String> {
    let year: i32 = year.trim().parse().map_err(|_| "Năm không hợp lệ.".to_string())?;
    let month: u8 = month.trim().parse().map_err(|_| "Tháng không hợp lệ.".to_string())?;
    let day: u8 = day.trim().parse().map_err(|_| "Ngày không hợp lệ.".to_string())?;
    CivilDate::from_ymd_in_range(year, month, day).map_err(|err| match err {
        DateError::OutOfRange => "Ngoài 1900–2100 sẽ bị từ chối.".to_string(),
        DateError::Invalid => "Ngày không hợp lệ.".to_string(),
    })
}

fn parse_iso_date(text: &str) -> Result<CivilDate, String> {
    let parts: Vec<&str> = text.trim().split('-').collect();
    if parts.len() != 3 {
        return Err("Ngày dương phải là YYYY-MM-DD.".into());
    }
    let year: i32 = parts[0].parse().map_err(|_| "Năm không hợp lệ.".to_string())?;
    let month: u8 = parts[1].parse().map_err(|_| "Tháng không hợp lệ.".to_string())?;
    let day: u8 = parts[2].parse().map_err(|_| "Ngày không hợp lệ.".to_string())?;
    CivilDate::from_ymd_in_range(year, month, day).map_err(|err| match err {
        DateError::OutOfRange => "Ngoài 1900–2100 sẽ bị từ chối.".to_string(),
        DateError::Invalid => "Ngày không hợp lệ.".to_string(),
    })
}

fn parse_optional_clock(text: &str) -> Result<Option<ClockTime>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    ClockTime::parse(text).map(Some).map_err(|err| err.to_string())
}

fn recurrence_from_index(index: i32) -> Recurrence {
    match index {
        1 => Recurrence::Daily,
        2 => Recurrence::Weekly,
        3 => Recurrence::Monthly,
        4 => Recurrence::Yearly,
        _ => Recurrence::None,
    }
}

fn recurrence_index(recurrence: Recurrence) -> i32 {
    match recurrence {
        Recurrence::None => 0,
        Recurrence::Daily => 1,
        Recurrence::Weekly => 2,
        Recurrence::Monthly => 3,
        Recurrence::Yearly => 4,
    }
}

fn clock_or(clock: Option<ClockTime>, fallback: &str) -> SharedString {
    SharedString::from(clock.map(|clock| clock.to_string()).unwrap_or_else(|| fallback.to_string()))
}

/// Keep identity, notes, timezone, and enable/delete exceptions. The form does not edit those.
fn edited_event(existing: Event, mut draft: Event) -> Event {
    draft.id = existing.id;
    draft.kind = existing.kind;
    draft.description = existing.description;
    draft.timezone = existing.timezone;
    draft.rev = existing.rev.saturating_add(1);
    draft.enabled = existing.enabled;
    draft.skip_gregorian = existing.skip_gregorian;
    draft.disabled_gregorian = existing.disabled_gregorian;
    draft.enabled_gregorian = existing.enabled_gregorian;
    draft.profile_id = existing.profile_id;
    if draft.reminder.is_some() {
        draft.reminder_enabled = existing.reminder_enabled;
        draft.reminder_skip_gregorian = existing.reminder_skip_gregorian;
        draft.reminder_disabled_gregorian = existing.reminder_disabled_gregorian;
        draft.reminder_enabled_gregorian = existing.reminder_enabled_gregorian;
    }
    draft
}

fn fill_event_form(ui: &AppWindow, state: &mut AppState, existing: Option<&Event>) {
    let selected = state.selected;
    let cal = ui.global::<Calendar>();
    let event_date = existing.and_then(|event| event.gregorian).unwrap_or(selected);
    cal.set_editing_id(SharedString::from(
        existing.map(|event| event.id.0.to_string()).unwrap_or_default(),
    ));
    cal.set_event_title(SharedString::from(
        existing.map(|event| event.title.as_str()).unwrap_or(""),
    ));
    cal.set_event_basis(SharedString::from(
        existing
            .map(|event| match event.basis {
                CalendarBasis::Lunar => "lunar",
                CalendarBasis::Gregorian => "gregorian",
            })
            .unwrap_or("gregorian"),
    ));
    cal.set_event_date(SharedString::from(format!(
        "{:04}-{:02}-{:02}",
        event_date.year(),
        event_date.month(),
        event_date.day()
    )));
    if let Some(lunar) = existing.and_then(|event| event.lunar) {
        cal.set_event_lunar_year(SharedString::from(lunar.year_fn.to_string()));
        cal.set_event_lunar_month(SharedString::from(lunar.month.to_string()));
        cal.set_event_lunar_day(SharedString::from(lunar.day.to_string()));
        cal.set_event_lunar_leap(lunar.leap);
    } else if let Ok(lunar) = lookup(&mut state.tables, event_date) {
        cal.set_event_lunar_year(SharedString::from(lunar.year_fn.to_string()));
        cal.set_event_lunar_month(SharedString::from(lunar.month.to_string()));
        cal.set_event_lunar_day(SharedString::from(lunar.day.to_string()));
        cal.set_event_lunar_leap(lunar.leap);
    } else {
        cal.set_event_lunar_year(SharedString::from(event_date.year().to_string()));
        cal.set_event_lunar_month(SharedString::from("1"));
        cal.set_event_lunar_day(SharedString::from("1"));
        cal.set_event_lunar_leap(false);
    }
    cal.set_event_all_day(existing.is_some_and(|event| event.all_day));
    cal.set_event_start(clock_or(existing.and_then(|event| event.start_time), "09:00"));
    cal.set_event_end(clock_or(existing.and_then(|event| event.end_time), "10:30"));
    cal.set_event_reminder_on(existing.is_some_and(|event| event.reminder.is_some()));
    cal.set_event_reminder(clock_or(
        existing.and_then(|event| event.reminder.as_ref().map(|reminder| reminder.clock)),
        "08:00",
    ));
    let recurrence = existing
        .map(|event| {
            if event.birthday { Recurrence::Yearly } else { event.recurrence }
        })
        .unwrap_or(Recurrence::None);
    cal.set_event_recurrence_index(recurrence_index(recurrence));
    cal.set_event_error(SharedString::new());
    cal.set_event_status(SharedString::new());
    set_scope_options(ui);
}

pub fn open_event_sheet(ui: &AppWindow, state: &mut AppState) {
    fill_event_form(ui, state, None);
    ui.global::<Chrome>().set_event_sheet_open(true);
}

pub fn open_existing_event(ui: &AppWindow, state: &mut AppState, id: &str) -> Result<(), String> {
    let id = id.trim().parse::<i64>().map_err(|_| "Sự kiện không còn.".to_string())?;
    let event = state
        .events
        .get(EventId(id))
        .cloned()
        .ok_or_else(|| "Sự kiện không còn.".to_string())?;
    fill_event_form(ui, state, Some(&event));
    ui.global::<Chrome>().set_event_sheet_open(true);
    Ok(())
}

fn event_from_form(ui: &AppWindow, state: &mut AppState) -> Result<Event, String> {
    let cal = ui.global::<Calendar>();
    let title = cal.get_event_title().to_string();
    let all_day = cal.get_event_all_day();
    let start_time = if all_day { None } else { parse_optional_clock(cal.get_event_start().as_str())? };
    let end_time = if all_day { None } else { parse_optional_clock(cal.get_event_end().as_str())? };
    let reminder = if cal.get_event_reminder_on() {
        parse_optional_clock(cal.get_event_reminder().as_str())?.or(Some(ClockTime::REMINDER_DEFAULT))
    } else {
        None
    };
    let recurrence = recurrence_from_index(cal.get_event_recurrence_index());
    let timezone = local_timezone();
    if cal.get_event_basis().as_str() == "lunar" {
        let year_fn: i32 = cal
            .get_event_lunar_year()
            .trim()
            .parse()
            .map_err(|_| "Năm âm không hợp lệ.")?;
        let month: u8 = cal
            .get_event_lunar_month()
            .trim()
            .parse()
            .map_err(|_| "Tháng âm không hợp lệ.")?;
        let day: u8 = cal
            .get_event_lunar_day()
            .trim()
            .parse()
            .map_err(|_| "Ngày âm không hợp lệ.")?;
        create_lunar_event(
            &mut state.tables,
            NewLunarEvent {
                title,
                description: None,
                date: LunarYmd {
                    year_fn,
                    month,
                    leap: cal.get_event_lunar_leap(),
                    day,
                },
                all_day,
                start_time,
                end_time,
                timezone,
                reminder,
                recurrence,
                birthday: false,
            },
        )
        .map_err(|err| err.to_string())
    } else {
        create_gregorian_event(NewGregorianEvent {
            title,
            description: None,
            date: parse_iso_date(cal.get_event_date().as_str())?,
            all_day,
            start_time,
            end_time,
            timezone,
            reminder,
            recurrence,
            birthday: false,
        })
        .map_err(|err| err.to_string())
    }
}

pub fn save_event(ui: &AppWindow, state: &mut AppState) -> Result<Event, String> {
    let draft = event_from_form(ui, state)?;
    let editing = ui.global::<Calendar>().get_editing_id().to_string();
    let event = if editing.trim().is_empty() {
        let mut event = draft;
        if event.profile_id.is_nil() {
            event.profile_id = state.active_profile;
        }
        event
    } else {
        let id = editing.trim().parse::<i64>().map_err(|_| "Sự kiện không còn.".to_string())?;
        let existing = state
            .events
            .get(EventId(id))
            .cloned()
            .ok_or_else(|| "Sự kiện không còn.".to_string())?;
        edited_event(existing, draft)
    };
    if !editing.trim().is_empty() {
        state.events.replace(event.clone());
    }
    ui.global::<Calendar>().set_editing_id(SharedString::new());
    ui.global::<Chrome>().set_event_sheet_open(false);
    Ok(event)
}

pub(crate) fn set_scope_options(ui: &AppWindow) {
    let i18n = ui.global::<I18n>();
    let page = ui.global::<Chrome>().get_page();
    let mut options = vec![i18n.get_scope_day()];
    let has_period = matches!(page, AppPage::Week | AppPage::Month);
    if page == AppPage::Week {
        options.push(i18n.get_scope_week());
    } else if page == AppPage::Month {
        options.push(i18n.get_scope_month());
    }
    options.push(i18n.get_scope_series());
    let cal = ui.global::<Calendar>();
    cal.set_event_scope_has_period(has_period);
    cal.set_event_scope_options(ModelRc::new(VecModel::from(options)));
    cal.set_event_scope_index(0);
}

fn period_bounds(page: AppPage, selected: CivilDate) -> Option<(CivilDate, CivilDate)> {
    match page {
        AppPage::Week => {
            let days = selected.week_days();
            Some((days[0], days[6]))
        }
        AppPage::Month => {
            let last = days_in_month(selected.year(), selected.month());
            Some((
                CivilDate::new(selected.year(), selected.month(), 1).ok()?,
                CivilDate::new(selected.year(), selected.month(), last).ok()?,
            ))
        }
        _ => None,
    }
}

fn scope_of(index: i32, has_period: bool) -> ItemScope {
    if index <= 0 {
        ItemScope::Day
    } else if has_period && index == 1 {
        ItemScope::Period
    } else {
        ItemScope::Series
    }
}

fn action_note(lang: UiLang, _kind: ItemKind, action: ItemAction, scope: ItemScope) -> String {
    let bundle = Bundle::load(lang);
    match (action, scope) {
        (ItemAction::Enable, ItemScope::Day) => bundle.note_enable_day,
        (ItemAction::Enable, ItemScope::Period) => bundle.note_enable_period,
        (ItemAction::Enable, ItemScope::Series) => bundle.note_enable_series,
        (ItemAction::Disable, ItemScope::Day) => bundle.note_disable_day,
        (ItemAction::Disable, ItemScope::Period) => bundle.note_disable_period,
        (ItemAction::Disable, ItemScope::Series) => bundle.note_disable_series,
        (ItemAction::Delete, ItemScope::Day) => bundle.note_delete_day,
        (ItemAction::Delete, ItemScope::Period) => bundle.note_delete_period,
        (ItemAction::Delete, ItemScope::Series) => bundle.note_delete_series,
    }
}

pub enum StoredChange {
    Save(Event),
    Delete(Event),
}

pub fn apply_stored_action(
    ui: &AppWindow,
    state: &mut AppState,
    action: i32,
) -> Result<StoredChange, String> {
    let cal = ui.global::<Calendar>();
    cal.set_event_error(SharedString::new());
    cal.set_event_status(SharedString::new());
    let id = cal.get_editing_id().to_string();
    if id.trim().is_empty() {
        return Err("Hãy lưu sự kiện trước khi tắt hoặc xóa.".into());
    }
    let id = id.trim().parse::<i64>().map_err(|_| "Sự kiện không còn.".to_string())?;
    let mut event = state
        .events
        .get(EventId(id))
        .cloned()
        .ok_or_else(|| "Sự kiện không còn.".to_string())?;
    let kind = ItemKind::Event;
    let action = match action {
        1 => ItemAction::Enable,
        2 => ItemAction::Delete,
        _ => ItemAction::Disable,
    };
    let has_period = cal.get_event_scope_has_period();
    let scope = scope_of(cal.get_event_scope_index(), has_period);
    let day = state.selected;
    let (from, to) = period_bounds(ui.global::<Chrome>().get_page(), day).unwrap_or((day, day));
    let effect = apply_item_action(&mut state.tables, &mut event, kind, action, scope, day, from, to)
        .map_err(|err| err.to_string())?;
    let note = action_note(state.lang(), kind, action, scope);
    match effect {
        ActionEffect::Deleted => {
            state.events.remove(event.id);
            cal.set_editing_id(SharedString::new());
            cal.set_event_status(SharedString::from(note));
            ui.global::<Chrome>().set_event_sheet_open(false);
            ui.global::<Chrome>().set_day_list_open(false);
            Ok(StoredChange::Delete(event))
        }
        ActionEffect::Updated => {
            state.events.replace(event.clone());
            cal.set_event_reminder_on(event.reminder.is_some());
            if let Some(reminder) = &event.reminder {
                cal.set_event_reminder(SharedString::from(reminder.clock.to_string()));
            }
            cal.set_event_error(SharedString::new());
            cal.set_event_status(SharedString::from(note));
            Ok(StoredChange::Save(event))
        }
    }
}

/// Delete the whole event. Skipping one day or a period stays on the editor.
pub fn delete_listed_event(ui: &AppWindow, state: &mut AppState, id: &str) -> Result<StoredChange, String> {
    let cal = ui.global::<Calendar>();
    cal.set_day_list_status(SharedString::new());
    let event_id = id.trim().parse::<i64>().map_err(|_| "Sự kiện không còn.".to_string())?;
    let mut event = state
        .events
        .get(EventId(event_id))
        .cloned()
        .ok_or_else(|| "Sự kiện không còn.".to_string())?;
    let day = state.selected;
    let effect = apply_item_action(
        &mut state.tables,
        &mut event,
        ItemKind::Event,
        ItemAction::Delete,
        ItemScope::Series,
        day,
        day,
        day,
    )
    .map_err(|err| err.to_string())?;
    match effect {
        ActionEffect::Deleted => {
            state.events.remove(event.id);
            if cal.get_editing_id().as_str() == id.trim() {
                cal.set_editing_id(SharedString::new());
                ui.global::<Chrome>().set_event_sheet_open(false);
            }
            Ok(StoredChange::Delete(event))
        }
        ActionEffect::Updated => {
            state.events.replace(event.clone());
            Ok(StoredChange::Save(event))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_keeps_identity_and_bumps_rev() {
        let existing = create_gregorian_event(NewGregorianEvent {
            title: "Họp".into(),
            description: Some("ghi chú".into()),
            date: CivilDate::new(2026, 10, 5).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: Some(ClockTime { hour: 10, minute: 0 }),
            timezone: "Asia/Ho_Chi_Minh".into(),
            reminder: Some(ClockTime::REMINDER_DEFAULT),
            recurrence: Recurrence::None,
            birthday: false,
        })
        .unwrap();
        let mut draft = existing.clone();
        draft.id = EventId(2);
        draft.title = "Họp lại".into();
        draft.description = None;
        draft.timezone = "UTC".into();
        draft.rev = 1;
        let saved = edited_event(existing.clone(), draft);
        assert_eq!(saved.id, existing.id);
        assert_eq!(saved.rev, existing.rev + 1);
        assert_eq!(saved.title, "Họp lại");
        assert_eq!(saved.description.as_deref(), Some("ghi chú"));
        assert_eq!(saved.timezone, "Asia/Ho_Chi_Minh");
    }
}
