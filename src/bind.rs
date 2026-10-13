use crate::calendar::CivilDate;
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
    Profiles,
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
            state.selected = match ui.global::<Chrome>().get_page() {
                AppPage::Week => state.selected.saturating_add_days(-7),
                AppPage::Day => state.selected.saturating_add_days(-1),
                _ => state.selected.saturating_add_months(-1),
            };
            refresh(&ui, &mut state);
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Calendar>().on_next_period(move || {
            let ui = ui_weak.unwrap();
            let mut state = state.borrow_mut();
            state.selected = match ui.global::<Chrome>().get_page() {
                AppPage::Week => state.selected.saturating_add_days(7),
                AppPage::Day => state.selected.saturating_add_days(1),
                _ => state.selected.saturating_add_months(1),
            };
            refresh(&ui, &mut state);
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
        let sync_tx = sync_tx.clone();
        ui.global::<I18n>().on_language_changed(move |index| {
            let ui = ui_weak.unwrap();
            let pref = LocalePref::from_index(index);
            let _ = sync_tx.send(SyncCmd::SetLocale(pref.as_str().into()));
            let mut state = state.borrow_mut();
            state.locale_pref = pref;
            i18n::apply(&ui, pref, &Bundle::load(pref.resolve()));
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
                Err(msg) => ui.global::<Calendar>().set_day_list_status(SharedString::from(msg)),
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
            if let Some(profile) = state.profiles.iter_mut().find(|profile| profile.id == pid) {
                profile.enabled = !profile.enabled;
                let snapshot = profile.clone();
                let _ = sync_tx.send(SyncCmd::UpsertProfile(snapshot));
            }
            if !state.profiles.iter().any(|profile| profile.enabled && profile.id == state.active_profile) {
                if let Some(next_id) = state.profiles.iter().find(|profile| profile.enabled).map(|profile| profile.id) {
                    state.active_profile = next_id;
                    let _ = sync_tx.send(SyncCmd::SetActiveProfile(next_id));
                }
            }
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
            let snapshot = state.profiles.iter_mut().find(|profile| profile.id == pid).map(|profile| {
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
            if state.profiles.len() < 2 {
                ui.global::<Profiles>()
                    .set_sheet_error(ui.global::<I18n>().get_profile_one_required());
                return;
            }
            let Some(pid) = crate::profile::ProfileId::parse(id.as_str()) else {
                return;
            };
            state.profiles.retain(|profile| profile.id != pid);
            state.events.remove_profile(pid);
            if state.active_profile == pid {
                if let Some(next_id) = state.profiles.first().map(|profile| profile.id) {
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
