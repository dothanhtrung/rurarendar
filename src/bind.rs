use crate::calendar::{
    days_in_month,
    CivilDate,
};
use crate::editor::{
    apply_stored_action,
    delete_listed_event,
    open_event_sheet,
    open_existing_event,
    parse_jump,
    save_event,
    set_scope_options,
    StoredChange,
};
use crate::state::{
    AppState,
    now_local,
};
use crate::sync::{
    SyncCmd,
    SyncEvent,
};
use crate::i18n::{
    self,
    Bundle,
    LocalePref,
};
use crate::notifications::NotificationScheduler;
use crate::theme::{
    ColorMode,
    ColorSchemeId,
};
use crate::ui::{
    AppPage,
    AppWindow,
    Calendar,
    Chrome,
    I18n,
    Feeds,
    Profiles,
    Motion,
    Sync,
    Theme,
};
use crate::view::refresh;
use slint::{
    ComponentHandle,
    SharedString,
    Timer,
    TimerMode,
};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use tokio::sync::mpsc::{
    UnboundedReceiver,
    UnboundedSender,
};

pub fn wire(
    ui: &AppWindow,
    state: Rc<RefCell<AppState>>,
    sync_tx: UnboundedSender<SyncCmd>,
    mut sync_rx: UnboundedReceiver<SyncEvent>,
) -> (Timer, Timer, Timer) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_prev_period(move || {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            shift_period(&mut state, ui.global::<Chrome>().get_page(), -1);
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_next_period(move || {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            shift_period(&mut state, ui.global::<Chrome>().get_page(), 1);
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_step_day(move |delta| {
            if delta == 0 {
                return false;
            }
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            let from = state.selected;
            let to = from.saturating_add_days(i64::from(delta));
            if to == from {
                return false;
            }
            let page = ui.global::<Chrome>().get_page();
            let slide = if page == AppPage::Month {
                let stays = CivilDate::in_month_grid(state.shown_year, state.shown_month, to);
                if !stays {
                    state.shown_year = to.year();
                    state.shown_month = to.month();
                }
                !stays
            } else {
                focus_page_changed(page, from, to)
            };
            state.selected = to;
            refresh(&ui, &mut state);
            slide
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_go_today(move || {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            state.selected = now_local().0;
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_select_cell(move |index| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            let Ok(grid) = CivilDate::month_grid(state.selected.year(), state.selected.month()) else {
                return;
            };
            if let Some(date) = grid.get(index as usize) {
                state.selected = *date;
                refresh(&ui, &mut state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_select_week_day(move |index| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            if let Some(date) = state.selected.week_days().get(index as usize) {
                state.selected = *date;
                refresh(&ui, &mut state);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_jump_to(move || {
            let ui = ui_weak.unwrap();
            let cal = ui.global::<Calendar>();
            match parse_jump(
                cal.get_jump_year().as_str(),
                cal.get_jump_month().as_str(),
                cal.get_jump_day().as_str(),
            ) {
                Ok(date) => {
                    let mut state = state.borrow_mut();
                    state.selected = date;
                    ui.global::<Chrome>().set_jump_open(false);
                    refresh(&ui, &mut state);
                }
                Err(msg) => cal.set_jump_error(SharedString::from(msg)),
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_open_event_sheet(move || {
            let ui = ui_weak.unwrap();
            open_event_sheet(&ui, &mut state.borrow_mut());
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_open_event(move |id| {
            let ui = ui_weak.unwrap();
            let _ = open_existing_event(&ui, &mut state.borrow_mut(), id.as_str());
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_open_listed_event(move |id, year, month, day| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            if let Ok(date) = CivilDate::from_ymd_in_range(year, month as u8, day as u8) {
                state.selected = date;
            }
            // A built-in holiday or festival has no stored event. Opening the day is enough.
            if !id.is_empty() {
                let _ = open_existing_event(&ui, &mut state, id.as_str());
            }
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<I18n>().on_language_changed(move |index| {
            let ui = ui_weak.unwrap();
            let pref = LocalePref::from_index(index);
            let _ = sync_tx.send(SyncCmd::SetLocale(pref.as_str().into()));
            let mut state = state.borrow_mut();
            state.locale_pref = pref;
            i18n::apply(&ui, pref, &Bundle::load(pref.resolve()));
            let embedded_name = ui.global::<I18n>().get_profile_embedded().to_string();
            if let Some(profile) = state.profiles.iter_mut().find(|profile| profile.embedded) {
                if profile.name != embedded_name {
                    profile.name = embedded_name;
                    let _ = sync_tx.send(SyncCmd::UpsertProfile(profile.clone()));
                }
            }
            set_scope_options(&ui);
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Theme>().on_scheme_changed(move |index| {
            let ui = ui_weak.unwrap();
            let scheme = ColorSchemeId::from_index(index);
            let mode = ColorMode::from_index(ui.global::<Theme>().get_mode_index());
            let _ = sync_tx.send(SyncCmd::SetTheme {
                scheme: scheme.as_str().into(),
                mode: mode.as_str().into(),
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Theme>().on_mode_changed(move |index| {
            let ui = ui_weak.unwrap();
            let scheme = ColorSchemeId::from_index(ui.global::<Theme>().get_scheme_index());
            let mode = ColorMode::from_index(index);
            let _ = sync_tx.send(SyncCmd::SetTheme {
                scheme: scheme.as_str().into(),
                mode: mode.as_str().into(),
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Motion>().on_save_enabled(move || {
            let ui = ui_weak.unwrap();
            let _ = sync_tx.send(SyncCmd::SetAnimations(ui.global::<Motion>().get_enabled()));
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Calendar>().on_save_event(move || {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            match save_event(&ui, &mut state) {
                Ok(event) => {
                    let _ = sync_tx.send(SyncCmd::Save(event));
                    refresh(&ui, &mut state);
                }
                Err(msg) => ui.global::<Calendar>().set_event_error(SharedString::from(msg)),
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Calendar>().on_apply_item_action(move |action| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            match apply_stored_action(&ui, &mut state, action) {
                Ok(StoredChange::Save(event)) => {
                    let _ = sync_tx.send(SyncCmd::Save(event));
                    refresh(&ui, &mut state);
                }
                Ok(StoredChange::Delete(event)) => {
                    let _ = sync_tx.send(SyncCmd::Delete(event));
                    refresh(&ui, &mut state);
                }
                Err(msg) => ui.global::<Calendar>().set_event_error(SharedString::from(msg)),
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Calendar>().on_delete_event(move |id| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            match delete_listed_event(&ui, &mut state, id.as_str()) {
                Ok(StoredChange::Save(event)) => {
                    let _ = sync_tx.send(SyncCmd::Save(event));
                    refresh(&ui, &mut state);
                }
                Ok(StoredChange::Delete(event)) => {
                    let _ = sync_tx.send(SyncCmd::Delete(event));
                    refresh(&ui, &mut state);
                }
                Err(msg) => {
                    let text = SharedString::from(msg);
                    let cal = ui.global::<Calendar>();
                    cal.set_day_list_status(text.clone());
                    cal.set_agenda_status(text);
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Profiles>().on_toggle(move |id| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            let Some(pid) = crate::profile::ProfileId::parse(id.as_str()) else {
                return;
            };
            if let Some(profile) = state.profiles.iter_mut().find(|profile| profile.id == pid && !profile.embedded) {
                profile.enabled = !profile.enabled;
                let snapshot = profile.clone();
                let _ = sync_tx.send(SyncCmd::UpsertProfile(snapshot));
            }
            activate_ordinary_if_needed(&mut state, &sync_tx);
            let _ = state.reload_catalogs();
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Profiles>().on_toggle_embedded(move || {
            let ui = ui_weak.unwrap();
            let enabled = ui.global::<Profiles>().get_embedded_enabled();
            let mut state = state.borrow_mut();
            if let Some(profile) = state.profiles.iter_mut().find(|profile| profile.embedded) {
                profile.enabled = enabled;
                let snapshot = profile.clone();
                let _ = sync_tx.send(SyncCmd::UpsertProfile(snapshot));
            }
            activate_ordinary_if_needed(&mut state, &sync_tx);
            let _ = state.reload_catalogs();
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Profiles>().on_select(move |id| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            let Some(pid) = crate::profile::ProfileId::parse(id.as_str()) else {
                return;
            };
            let snapshot = state.profiles.iter_mut().find(|profile| profile.id == pid && !profile.embedded).map(|profile| {
                profile.enabled = true;
                profile.clone()
            });
            if let Some(snapshot) = snapshot {
                state.active_profile = pid;
                let _ = sync_tx.send(SyncCmd::UpsertProfile(snapshot));
                let _ = sync_tx.send(SyncCmd::SetActiveProfile(pid));
            }
            let _ = state.reload_catalogs();
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Profiles>().on_delete(move |id| {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            let Some(pid) = crate::profile::ProfileId::parse(id.as_str()) else {
                return;
            };
            if !crate::profile::can_delete_profile(&state.profiles, pid) {
                let message = if state.profiles.iter().any(|profile| profile.id == pid && profile.embedded) {
                    ui.global::<I18n>().get_profile_embedded_kept()
                } else {
                    ui.global::<I18n>().get_profile_one_required()
                };
                ui.global::<Profiles>().set_sheet_error(message);
                return;
            }
            state.profiles.retain(|profile| profile.id != pid);
            state.events.remove_profile(pid);
            if state.active_profile == pid {
                if let Some(next_id) = state
                    .profiles
                    .iter()
                    .find(|profile| !profile.embedded)
                    .or_else(|| state.profiles.first())
                    .map(|profile| profile.id)
                {
                    state.active_profile = next_id;
                    let _ = sync_tx.send(SyncCmd::SetActiveProfile(next_id));
                }
            }
            let _ = sync_tx.send(SyncCmd::DeleteProfile(pid));
            let _ = state.reload_catalogs();
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Profiles>().on_open_create(move || {
            let ui = ui_weak.unwrap();
            let p = ui.global::<Profiles>();
            p.set_sheet_import(false);
            p.set_sheet_title(ui.global::<I18n>().get_profile_new());
            p.set_sheet_name(SharedString::new());
            p.set_sheet_path(SharedString::new());
            p.set_sheet_error(SharedString::new());
            ui.global::<Chrome>().set_profile_sheet_open(true);
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Profiles>().on_open_import(move || {
            let ui = ui_weak.unwrap();
            let p = ui.global::<Profiles>();
            p.set_sheet_import(true);
            p.set_sheet_title(ui.global::<I18n>().get_profile_import());
            p.set_sheet_name(SharedString::new());
            p.set_sheet_path(SharedString::new());
            p.set_sheet_error(SharedString::new());
            ui.global::<Chrome>().set_profile_sheet_open(true);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Profiles>().on_save_sheet(move || {
            let ui = ui_weak.unwrap();
            let p = ui.global::<Profiles>();
            let name = p.get_sheet_name().trim().to_string();
            if name.is_empty() {
                p.set_sheet_error(ui.global::<I18n>().get_profile_name_required());
                return;
            }
            let import = p.get_sheet_import();
            let path = p.get_sheet_path().to_string();
            if import && path.trim().is_empty() {
                p.set_sheet_error(ui.global::<I18n>().get_profile_path_hint());
                return;
            }
            let mut state = state.borrow_mut();
            let id = crate::profile::ProfileId::default();
            let src = std::path::Path::new(path.trim());
            let catalog = if import || !path.trim().is_empty() {
                let slug = crate::profile::detect_catalog_slug(src, &name);
                if slug == crate::profile::EMBEDDED_CATALOG {
                    p.set_sheet_error(ui.global::<I18n>().get_profile_embedded_catalog());
                    return;
                }
                let dest = state.catalogs_dir.clone();
                if let Err(err) = crate::profile::import_catalog_dir(src, &dest, &slug) {
                    p.set_sheet_error(SharedString::from(err));
                    return;
                }
                slug
            } else {
                String::new()
            };
            let sort = state.profiles.iter().map(|profile| profile.sort).max().unwrap_or(0) + 1;
            let profile = crate::profile::Profile {
                id,
                name,
                enabled: true,
                catalog,
                sort,
                embedded: false,
            };
            state.profiles.push(profile.clone());
            state.active_profile = id;
            let _ = state.reload_catalogs();
            let _ = sync_tx.send(SyncCmd::UpsertProfile(profile));
            ui.global::<Chrome>().set_profile_sheet_open(false);
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Feeds>().on_open_add(move || {
            let ui = ui_weak.unwrap();
            let feeds = ui.global::<Feeds>();
            feeds.set_sheet_url(SharedString::new());
            feeds.set_sheet_name(SharedString::new());
            feeds.set_sheet_error(SharedString::new());
            feeds.set_sheet_open(true);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Feeds>().on_save(move || {
            let ui = ui_weak.unwrap();
            let feeds = ui.global::<Feeds>();
            let url = feeds.get_sheet_url().trim().to_string();
            if url.is_empty() {
                feeds.set_sheet_error(ui.global::<I18n>().get_ical_url_required());
                return;
            }
            feeds.set_sheet_error(ui.global::<I18n>().get_ical_fetching());
            let _ = sync_tx.send(SyncCmd::AddIcal {
                url,
                name: feeds.get_sheet_name().to_string(),
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Feeds>().on_toggle(move |id| {
            let ui = ui_weak.unwrap();
            let Ok(id) = id.as_str().parse::<i64>() else {
                return;
            };
            let mut state = state.borrow_mut();
            let Some(feed) = state.feeds.iter_mut().find(|feed| feed.id == id) else {
                return;
            };
            feed.enabled = !feed.enabled;
            let enabled = feed.enabled;
            let _ = sync_tx.send(SyncCmd::SetIcalEnabled { id, enabled });
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        let sync_tx = sync_tx.clone();
        ui.global::<Feeds>().on_delete(move |id| {
            let ui = ui_weak.unwrap();
            let Ok(id) = id.as_str().parse::<i64>() else {
                return;
            };
            let mut state = state.borrow_mut();
            state.feeds.retain(|feed| feed.id != id);
            let _ = sync_tx.send(SyncCmd::DeleteIcal(id));
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Sync>().on_save_account(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let s = ui.global::<Sync>();
            s.set_error(SharedString::new());
            s.set_status(ui.global::<I18n>().get_syncing());
            let _ = sync_tx.send(SyncCmd::Connect {
                server: s.get_server_url().to_string(),
                username: s.get_username().to_string(),
                token: s.get_token().to_string(),
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Sync>().on_save_encrypt_key(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let s = ui.global::<Sync>();
            s.set_error(SharedString::new());
            s.set_status(ui.global::<I18n>().get_updating_key());
            let _ = sync_tx.send(SyncCmd::SetEncryptKey {
                key: s.get_encrypt_key().to_string(),
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Sync>().on_sync_now(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<Sync>().set_status(ui.global::<I18n>().get_syncing());
                ui.global::<Sync>().set_error(SharedString::new());
            }
            let _ = sync_tx.send(SyncCmd::PushPull);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Sync>().on_disconnect(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<Sync>().set_error(SharedString::new());
            }
            let _ = sync_tx.send(SyncCmd::Disconnect);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        ui.global::<Sync>().on_clear_server(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<Sync>().set_error(SharedString::new());
            }
            let _ = sync_tx.send(SyncCmd::ClearServer);
        });
    }

    let sync_timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        sync_timer.start(TimerMode::Repeated, std::time::Duration::from_millis(250), move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            while let Ok(ev) = sync_rx.try_recv() {
                apply_sync_event(&ui, &state, ev);
            }
        });
    }

    let notification_timer = Timer::default();
    {
        let state = state.clone();
        let mut scheduler = NotificationScheduler::default();
        notification_timer.start(TimerMode::Repeated, std::time::Duration::from_secs(30), move || {
            scheduler.fire_due(&mut state.borrow_mut());
        });
    }

    if let Ok(path) = std::env::var("RURARENDAR_SNAPSHOT") {
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(200), move || {
            if let Some(ui) = ui_weak.upgrade()
                && let Ok(buffer) = ui.window().take_snapshot()
                && let Err(err) = write_ppm(&path, buffer.width(), buffer.height(), buffer.as_bytes())
            {
                tracing::error!("snapshot failed: {err}");
            }
            let _ = slint::quit_event_loop();
        });
    }

    let pull_timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let sync_tx = sync_tx.clone();
        pull_timer.start(
            TimerMode::Repeated,
            std::time::Duration::from_secs(15 * 60),
            move || {
                if let Some(ui) = ui_weak.upgrade()
                    && ui.global::<Sync>().get_logged_in()
                {
                    let _ = sync_tx.send(SyncCmd::PushPull);
                }
            },
        );
    }

    (sync_timer, pull_timer, notification_timer)
}

fn activate_ordinary_if_needed(state: &mut AppState, sync_tx: &UnboundedSender<SyncCmd>) {
    let active_ok = state.profiles.iter().any(|profile| {
        profile.id == state.active_profile && !profile.embedded && profile.enabled
    });
    if active_ok {
        return;
    }
    let Some(next_id) = state
        .profiles
        .iter()
        .find(|profile| !profile.embedded && profile.enabled)
        .or_else(|| state.profiles.iter().find(|profile| !profile.embedded))
        .map(|profile| profile.id)
    else {
        return;
    };
    if next_id == state.active_profile {
        return;
    }
    state.active_profile = next_id;
    let _ = sync_tx.send(SyncCmd::SetActiveProfile(next_id));
}

fn apply_sync_event(ui: &AppWindow, state: &Rc<RefCell<AppState>>, ev: SyncEvent) {
    match ev {
        SyncEvent::Status(msg) => ui.global::<Sync>().set_status(SharedString::from(msg)),
        SyncEvent::Error(msg) => ui.global::<Sync>().set_error(SharedString::from(msg)),
        SyncEvent::Ready {
            server,
            username,
            token,
            encrypt_key,
            connected,
        } => {
            let s = ui.global::<Sync>();
            s.set_server_url(SharedString::from(server));
            s.set_username(SharedString::from(username));
            s.set_token(SharedString::from(token));
            s.set_encrypt_key(SharedString::from(encrypt_key));
            s.set_logged_in(connected);
            s.set_error(SharedString::new());
            if connected {
                s.set_status(SharedString::from("Sẵn sàng đồng bộ"));
            }
        }
        SyncEvent::Events(events) => {
            let mut state = state.borrow_mut();
            state.events = events;
            let active = state.active_profile;
            state.events.assign_nil_profile(active);
            refresh(ui, &mut state);
        }
        SyncEvent::ProfileSaved(profile) => {
            let mut state = state.borrow_mut();
            if let Some(existing) = state.profiles.iter_mut().find(|existing| {
                existing.id == profile.id || (existing.id.is_nil() && existing.name == profile.name)
            }) {
                let old_id = existing.id;
                *existing = profile.clone();
                if state.active_profile == old_id {
                    state.active_profile = profile.id;
                }
            } else {
                state.profiles.push(profile);
            }
            let _ = state.reload_catalogs();
            refresh(ui, &mut state);
        }
        SyncEvent::Conflict { title, .. } => {
            ui.global::<Sync>()
                .set_error(SharedString::from(format!("Xung đột: {title}")));
        }
        SyncEvent::IcalFeeds { feeds, close_sheet } => {
            let mut state = state.borrow_mut();
            state.feeds = feeds;
            refresh(ui, &mut state);
            if close_sheet {
                let feeds = ui.global::<Feeds>();
                feeds.set_sheet_error(SharedString::new());
                feeds.set_sheet_open(false);
            }
        }
        SyncEvent::IcalError(msg) => {
            ui.global::<Feeds>().set_sheet_error(SharedString::from(msg));
        }
    }
}

fn shift_period(state: &mut AppState, page: AppPage, dir: i32) {
    match page {
        AppPage::Week => state.selected = state.selected.saturating_add_days(i64::from(dir) * 7),
        AppPage::Day => state.selected = state.selected.saturating_add_days(i64::from(dir)),
        _ => {
            let day = state.selected.day().min(days_in_month(state.shown_year, state.shown_month).max(1));
            let base = CivilDate::from_ymd_in_range(state.shown_year, state.shown_month, day).unwrap_or(state.selected);
            let next = base.saturating_add_months(dir);
            state.shown_year = next.year();
            state.shown_month = next.month();
            state.selected = next;
        }
    }
}

/// True when `to` is no longer on the day or week page `from` is showing.
fn focus_page_changed(page: AppPage, from: CivilDate, to: CivilDate) -> bool {
    match page {
        AppPage::Day => from != to,
        AppPage::Week => from.monday_of_week() != to.monday_of_week(),
        _ => from.year() != to.year() || from.month() != to.month(),
    }
}

fn write_ppm(path: &str, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;
    for pixel in rgba.chunks(4) {
        file.write_all(&pixel[..3])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> CivilDate {
        CivilDate::new(year, month, day).unwrap()
    }

    #[test]
    fn day_step_slides_only_when_the_visible_page_changes() {
        let monday = date(2026, 10, 15).monday_of_week();
        let sunday = monday.saturating_add_days(6);
        let next_monday = monday.saturating_add_days(7);

        assert!(!focus_page_changed(AppPage::Week, monday, sunday));
        assert!(focus_page_changed(AppPage::Week, sunday, next_monday));
        assert!(focus_page_changed(AppPage::Day, monday, monday.saturating_add_days(1)));
        assert!(!focus_page_changed(AppPage::Day, monday, monday));
    }

    #[test]
    fn month_row_stays_until_focus_leaves_the_grid() {
        let grid = CivilDate::month_grid(2026, 10).unwrap();
        let first = grid[0];
        let below = grid[7];
        assert_eq!(below, first.saturating_add_days(7));
        assert!(CivilDate::in_month_grid(2026, 10, first));
        assert!(CivilDate::in_month_grid(2026, 10, below));
        assert!(CivilDate::in_month_grid(2026, 10, date(2026, 11, 1)));
        assert!(!CivilDate::in_month_grid(2026, 10, first.saturating_add_days(-7)));
        assert!(!CivilDate::in_month_grid(2026, 10, grid[41].saturating_add_days(7)));
    }
}
