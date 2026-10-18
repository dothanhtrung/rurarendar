pub mod calendar;
pub mod codec;
pub mod crypto;
pub mod event;
pub mod i18n;
pub mod ical;
pub mod profile;
pub mod store;
pub mod sync;
mod theme;

mod bind;
mod config;
mod editor;
mod notifications;
mod state;
mod ui;
mod view;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

use crate::calendar::{
    CalendarTables,
    HolidayTables,
    SolarTermTables,
};
use crate::config::Config;
use clap::Parser;

use crate::event::EventStore;
use slint::ComponentHandle;
use state::{
    AppState,
    now_local,
};
use std::cell::RefCell;
use std::error::Error;
use std::path::PathBuf;
use std::rc::Rc;
use tracing_subscriber::EnvFilter;
use ui::AppWindow;
use view::refresh;

#[cfg_attr(target_family = "wasm", wasm_bindgen(start))]
pub fn main() {
    if let Err(err) = run() {
        eprintln!("rurarendar: {err}");
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    // NativeActivity calls this with the Rust ABI and expects no return value.
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
    }
    eprintln!("rurarendar: android_main");
    if let Err(err) = prepare_android(&app) {
        eprintln!("rurarendar: prepare failed: {err}");
    }
    eprintln!("rurarendar: slint::android::init");
    slint::android::init(app).unwrap();
    eprintln!("rurarendar: slint init ok");
    if let Err(err) = run() {
        eprintln!("rurarendar: {err}");
    }
}

/// Copy packaged CSVs onto internal storage and point the desktop paths at them.
/// Lunar tables are compiled in; tiết khí and the holiday catalog are read from disk.
#[cfg(target_os = "android")]
fn prepare_android(app: &slint::android::AndroidApp) -> Result<(), Box<dyn Error>> {
    use std::ffi::CString;
    use std::io::Read;

    let data = app
        .internal_data_path()
        .ok_or_else(|| anyhow::anyhow!("android internal data path is missing"))?;
    let assets = data.join("assets");
    let manager = app.asset_manager();
    for dir in ["calendar", "24_solar_terms", "catalog"] {
        let listing = CString::new(dir)?;
        let Some(mut entries) = manager.open_dir(&listing) else {
            continue;
        };
        let names: Vec<CString> = entries.by_ref().collect();
        let out_dir = assets.join(dir);
        std::fs::create_dir_all(&out_dir)?;
        for name in names {
            let Ok(file) = name.to_str() else {
                continue;
            };
            if file.is_empty() {
                continue;
            }
            let rel = CString::new(format!("{dir}/{file}"))?;
            let Some(mut asset) = manager.open(&rel) else {
                continue;
            };
            let mut bytes = Vec::new();
            asset.read_to_end(&mut bytes)?;
            std::fs::write(out_dir.join(file), bytes)?;
        }
    }
    // `set_var` is process-global. This runs on the android_main thread before any other thread starts.
    unsafe {
        std::env::set_var("RURARENDAR_ASSETS", &assets);
        std::env::set_var("RURARENDAR_DB", data.join("client.sqlite"));
        std::env::set_var("RURARENDAR_CONFIG", data.join("rurarendar.ron"));
    }
    Ok(())
}

#[derive(Parser)]
struct Cli {
    #[clap(short, long, default_value = "rurarendar.ron")]
    config: PathBuf,
}

pub fn run() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_thread_ids(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Cli::parse();
    let config_path = std::env::var_os("RURARENDAR_CONFIG")
        .map(PathBuf::from)
        .unwrap_or(args.config);
    let mut config = Config::load(&config_path)?;
    if let Some(parent) = config.sqlite_path().parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let (today, _) = now_local();
    let locale_pref = i18n::LocalePref::from_setting(&config.ui_locale);
    let color_scheme = theme::ColorSchemeId::from_setting(&config.color_scheme);
    let color_mode = theme::ColorMode::from_setting(&config.color_mode);
    let animations = config.animations;
    let catalogs_dir = config.catalogs_dir();

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let pool = runtime.block_on(store::open_pool(&config.db))?;
    let bundle = i18n::Bundle::load(locale_pref.resolve());
    let default_name = bundle.profile_default;
    let embedded_name = bundle.profile_embedded;
    let (profiles, default_profile) = runtime.block_on(async {
        let default_profile = store::ensure_default_profile(&pool, &default_name).await?;
        store::ensure_embedded_profile(&pool, &embedded_name).await?;
        let profiles = store::load_profiles(&pool).await?;
        anyhow::Ok((profiles, default_profile))
    })?;
    let active_profile = {
        let active = crate::profile::ProfileId(config.active_profile);
        if profiles.iter().any(|profile| profile.id == active) {
            active
        } else {
            profiles
                .iter()
                .find(|profile| !profile.embedded)
                .map(|profile| profile.id)
                .unwrap_or(default_profile.id)
        }
    };
    if config.active_profile != active_profile.0 {
        config.active_profile = active_profile.0;
        config.save(&config_path)?;
    }
    let feeds = runtime.block_on(store::load_ical_feeds(&pool)).unwrap_or_else(|err| {
        tracing::warn!("ical load: {err}");
        Vec::new()
    });
    let (sync_tx, sync_rx) = sync::start(pool, config, config_path);

    let mut app_state = AppState {
        tables: CalendarTables::new(today.year())?,
        solar_terms: SolarTermTables::new(today.year())?,
        holidays: HolidayTables::empty(),
        selected: today,
        shown_year: today.year(),
        shown_month: today.month(),
        today,
        events: EventStore::default(),
        feeds,
        locale_pref,
        profiles,
        active_profile,
        catalogs_dir,
    };
    app_state.reload_catalogs()?;
    let state = Rc::new(RefCell::new(app_state));

    let ui = AppWindow::new()?;
    #[cfg(target_os = "android")]
    {
        ui.global::<ui::Chrome>().set_phone(true);
        ui.global::<ui::Chrome>().set_sidebar_open(false);
    }
    let bundle = i18n::Bundle::load(locale_pref.resolve());
    i18n::apply(&ui, locale_pref, &bundle);
    theme::apply(&ui, color_scheme, color_mode);
    ui.global::<ui::Motion>().set_enabled(animations);
    refresh(&ui, &mut state.borrow_mut());
    let (_sync_timer, _pull_timer, _notification_timer) = bind::wire(&ui, state, sync_tx, sync_rx);
    ui.run()?;
    Ok(())
}
