use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::codec::EventBody;
use crate::config::Config;
use crate::crypto::{
    self,
    Blob,
    DataKey,
};
use crate::event::{
    Event,
    EventId,
    EventStore,
};
use crate::profile::ProfileId;
use crate::store;

#[derive(Debug)]
pub enum SyncCmd {
    Connect {
        server: String,
        username: String,
        token: String,
    },
    SetEncryptKey {
        key: String,
    },
    Save(Event),
    Delete(Event),
    PushPull,
    SetLocale(String),
    // Local appearance only. Not pushed to the server.
    SetTheme { scheme: String, mode: String },
    UpsertProfile(crate::profile::Profile),
    DeleteProfile(crate::profile::ProfileId),
    SetActiveProfile(crate::profile::ProfileId),
    AddIcal { url: String, name: String },
    SetIcalEnabled { id: i64, enabled: bool },
    DeleteIcal(i64),
}

#[derive(Debug)]
pub enum SyncEvent {
    Status(String),
    Error(String),
    Ready {
        server: String,
        username: String,
        token: String,
        encrypt_key: String,
        connected: bool,
    },
    ProfileSaved(crate::profile::Profile),
    Events(EventStore),
    IcalFeeds { feeds: Vec<crate::ical::IcalFeed>, close_sheet: bool },
    IcalError(String),
    Conflict { title: String, item_id: String },
}

#[derive(Clone)]
struct Session {
    server: String,
    username: String,
    token: String,
    key: Option<DataKey>,
    old_key: Option<DataKey>,
    key_revision: i32,
}

pub fn start(
    pool: SqlitePool,
    config: Config,
    config_path: PathBuf,
) -> (mpsc::UnboundedSender<SyncCmd>, mpsc::UnboundedReceiver<SyncEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio");
        rt.block_on(worker(pool, config, config_path, cmd_rx, ev_tx));
    });
    (cmd_tx, ev_rx)
}

async fn worker(
    pool: SqlitePool,
    mut config: Config,
    config_path: PathBuf,
    mut cmds: mpsc::UnboundedReceiver<SyncCmd>,
    events: mpsc::UnboundedSender<SyncEvent>,
) {
    if let Ok(store) = store::load_events(&pool).await {
        let _ = events.send(SyncEvent::Events(store));
    }
    match refresh_stale_ical(&pool).await {
        Ok(feeds) => {
            let _ = events.send(SyncEvent::IcalFeeds { feeds, close_sheet: false });
        }
        Err(err) => tracing::warn!("ical refresh: {err}"),
    }
    let mut session = restore_session(&mut config, &config_path);
    let _ = events.send(ready_event(&config, session.is_some()));
    if session.is_some() {
        let _ = events.send(SyncEvent::Status("Đã có token. Có thể đồng bộ.".into()));
        if let Some(sess) = session.as_mut() {
            if let Err(e) = resume_rotation(&pool, &mut config, &config_path, sess).await {
                let _ = events.send(SyncEvent::Error(e.to_string()));
            } else if let Err(e) = sync_round(&pool, &mut config, &config_path, sess).await {
                let _ = events.send(SyncEvent::Error(e.to_string()));
            } else if let Ok(store) = store::load_events(&pool).await {
                let _ = events.send(SyncEvent::Events(store));
                let _ = events.send(SyncEvent::Status("Đồng bộ xong.".into()));
            }
        }
    }
    while let Some(cmd) = cmds.recv().await {
        match cmd {
            SyncCmd::Connect {
                server,
                username,
                token,
            } => match connect(&mut config, &config_path, &server, &username, &token) {
                Ok(mut sess) => {
                    if let Err(e) = resume_rotation(&pool, &mut config, &config_path, &mut sess).await {
                        let _ = events.send(SyncEvent::Error(e.to_string()));
                    }
                    match sync_round(&pool, &mut config, &config_path, &sess).await {
                        Ok(()) => {
                            session = Some(sess);
                            let _ = events.send(ready_event(&config, true));
                            if let Ok(store) = store::load_events(&pool).await {
                                let _ = events.send(SyncEvent::Events(store));
                            }
                            let _ = events.send(SyncEvent::Status("Đồng bộ xong.".into()));
                        }
                        Err(e) => {
                            session = Some(sess);
                            let _ = events.send(ready_event(&config, true));
                            let _ = events.send(SyncEvent::Error(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                }
            },
            SyncCmd::SetEncryptKey { key } => {
                match set_encrypt_key(&mut config, &config_path, session.as_ref(), &key) {
                    Ok(sess) => {
                        session = sess;
                        let _ = events.send(ready_event(&config, session.is_some()));
                        if let Some(sess) = session.as_mut() {
                            match resume_rotation(&pool, &mut config, &config_path, sess).await {
                                Ok(()) => {
                                    if let Ok(store) = store::load_events(&pool).await {
                                        let _ = events.send(SyncEvent::Events(store));
                                    }
                                    let _ = events.send(SyncEvent::Status("Đã cập nhật khoá mã hoá.".into()));
                                }
                                Err(e) => {
                                    let _ = events.send(SyncEvent::Error(e.to_string()));
                                }
                            }
                        } else {
                            let _ = events.send(SyncEvent::Status(
                                "Đã lưu khoá. Điền username và token rồi đồng bộ để áp dụng lên server.".into(),
                            ));
                        }
                    }
                    Err(e) => {
                        let _ = events.send(SyncEvent::Error(e.to_string()));
                    }
                }
            }
            SyncCmd::SetLocale(value) => {
                config.ui_locale = value;
                if let Err(e) = config.save(&config_path) {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                }
            }
            SyncCmd::UpsertProfile(profile) => {
                let is_new = profile.id.is_nil();
                match store::upsert_profile(&pool, &profile).await {
                    Ok(id) => {
                        if is_new {
                            config.active_profile = id.0;
                            if let Err(e) = config.save(&config_path) {
                                let _ = events.send(SyncEvent::Error(e.to_string()));
                            }
                        }
                        let mut profile = profile;
                        profile.id = id;
                        let _ = events.send(SyncEvent::ProfileSaved(profile));
                    }
                    Err(e) => { let _ = events.send(SyncEvent::Error(e.to_string())); }
                }
            }
            SyncCmd::DeleteProfile(id) => {
                match store::is_embedded_profile(&pool, id.0).await {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(e) => {
                        let _ = events.send(SyncEvent::Error(e.to_string()));
                        continue;
                    }
                }
                if let Err(e) = store::tombstone_profile_events(&pool, id.0).await {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                    continue;
                }
                if let Err(e) = store::delete_profile(&pool, id.0).await {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                }
                if let Some(sess) = &session
                    && let Err(e) = push_outbox(&pool, sess).await
                {
                    let _ = events.send(SyncEvent::Error(format!("Đã xóa cục bộ. Đẩy lên server lỗi: {e}")));
                }
            }
            SyncCmd::SetActiveProfile(id) => {
                config.active_profile = id.0;
                if let Err(e) = config.save(&config_path) {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                }
            }
            SyncCmd::SetTheme { scheme, mode } => {
                config.color_scheme = scheme;
                config.color_mode = mode;
                if let Err(e) = config.save(&config_path) {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                }
            }
            SyncCmd::Save(event) => {
                if let Err(e) = store::upsert_event(&pool, &event).await {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                    continue;
                }
                match store::load_events(&pool).await {
                    Ok(store) => { let _ = events.send(SyncEvent::Events(store)); }
                    Err(e) => { let _ = events.send(SyncEvent::Error(e.to_string())); }
                }
                if let Some(sess) = &session
                    && let Err(e) = push_outbox(&pool, sess).await
                {
                    let _ = events.send(SyncEvent::Error(format!("Lưu cục bộ. Đẩy lên server lỗi: {e}")));
                }
            }
            SyncCmd::Delete(event) => {
                if let Err(e) = store::tombstone_event(&pool, event.id.0, event.rev).await {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                    continue;
                }
                if let Some(sess) = &session
                    && let Err(e) = push_outbox(&pool, sess).await
                {
                    let _ = events.send(SyncEvent::Error(format!("Đã xóa cục bộ. Đẩy lên server lỗi: {e}")));
                }
            }
            SyncCmd::AddIcal { url, name } => match add_ical(&pool, &url, &name).await {
                Ok(()) => match store::load_ical_feeds(&pool).await {
                    Ok(feeds) => {
                        let _ = events.send(SyncEvent::IcalFeeds { feeds, close_sheet: true });
                    }
                    Err(err) => {
                        let _ = events.send(SyncEvent::IcalError(err.to_string()));
                    }
                },
                Err(err) => {
                    let _ = events.send(SyncEvent::IcalError(err));
                }
            },
            SyncCmd::SetIcalEnabled { id, enabled } => {
                if let Err(err) = store::set_ical_enabled(&pool, id, enabled).await {
                    let _ = events.send(SyncEvent::IcalError(err.to_string()));
                    continue;
                }
                if let Ok(feeds) = store::load_ical_feeds(&pool).await {
                    let _ = events.send(SyncEvent::IcalFeeds { feeds, close_sheet: false });
                }
            }
            SyncCmd::DeleteIcal(id) => {
                if let Err(err) = store::delete_ical_feed(&pool, id).await {
                    let _ = events.send(SyncEvent::IcalError(err.to_string()));
                    continue;
                }
                if let Ok(feeds) = store::load_ical_feeds(&pool).await {
                    let _ = events.send(SyncEvent::IcalFeeds { feeds, close_sheet: false });
                }
            }
            SyncCmd::PushPull => {
                let Some(sess) = session.as_mut() else {
                    let _ = events.send(SyncEvent::Error("Chưa có username/token.".into()));
                    continue;
                };
                if let Err(e) = resume_rotation(&pool, &mut config, &config_path, sess).await {
                    let _ = events.send(SyncEvent::Error(e.to_string()));
                    continue;
                }
                match sync_round(&pool, &mut config, &config_path, sess).await {
                    Ok(()) => {
                        if let Ok(store) = store::load_events(&pool).await {
                            let _ = events.send(SyncEvent::Events(store));
                        }
                        let _ = events.send(SyncEvent::Status("Đồng bộ xong.".into()));
                    }
                    Err(e) => {
                        let _ = events.send(SyncEvent::Error(e.to_string()));
                    }
                }
            }
        }
    }
}

async fn add_ical(pool: &SqlitePool, url: &str, name: &str) -> Result<(), String> {
    let url = crate::ical::normalize_url(url)?;
    let body = crate::ical::fetch_ics(&url).await?;
    let parsed = crate::ical::parse_ics(&body)?;
    let name = name.trim();
    let name = if name.is_empty() { parsed.name } else { name.to_string() };
    let name = if name.is_empty() { url.clone() } else { name };
    let fetched_at = chrono::Utc::now().timestamp();
    store::insert_ical_feed(pool, &url, &name, &body, fetched_at).await.map_err(|err| {
        if err.to_string().contains("UNIQUE") {
            "Lịch này đã được thêm.".to_string()
        } else {
            err.to_string()
        }
    })?;
    Ok(())
}

async fn refresh_stale_ical(pool: &SqlitePool) -> Result<Vec<crate::ical::IcalFeed>, String> {
    let feeds = store::load_ical_feeds(pool).await.map_err(|err| err.to_string())?;
    let now = chrono::Utc::now().timestamp();
    for feed in &feeds {
        if !feed.enabled || now.saturating_sub(feed.fetched_at) < store::ICAL_REFRESH_SECS {
            continue;
        }
        match crate::ical::fetch_ics(&feed.url).await {
            Ok(body) if body.to_ascii_uppercase().contains("BEGIN:VCALENDAR") => {
                if let Err(err) = store::update_ical_body(pool, feed.id, &body, now).await {
                    tracing::warn!("ical update {}: {err}", feed.url);
                }
            }
            Ok(_) => tracing::warn!("ical refresh ignored a non-calendar body for {}", feed.url),
            Err(err) => tracing::warn!("ical refresh {}: {err}", feed.url),
        }
    }
    store::load_ical_feeds(pool).await.map_err(|err| err.to_string())
}

fn ready_event(config: &Config, connected: bool) -> SyncEvent {
    SyncEvent::Ready {
        server: if config.server_url.is_empty() {
            "http://127.0.0.1:8080".into()
        } else {
            config.server_url.clone()
        },
        username: config.username.clone(),
        token: config.token.clone(),
        encrypt_key: config.encrypt_key.clone(),
        connected,
    }
}

fn restore_session(config: &mut Config, config_path: &Path) -> Option<Session> {
    if config.server_url.is_empty() || config.username.is_empty() || config.token.is_empty() {
        return None;
    }
    session_from_store(
        config,
        config_path,
        config.server_url.clone(),
        config.username.clone(),
        config.token.clone(),
    )
    .ok()
}

fn connect(
    config: &mut Config,
    config_path: &Path,
    server: &str,
    username: &str,
    token: &str,
) -> anyhow::Result<Session> {
    let server = server.trim_end_matches('/').to_string();
    if username.is_empty() || token.is_empty() {
        anyhow::bail!("Cần username và token.");
    }
    config.server_url = server.clone();
    config.username = username.to_string();
    config.token = token.to_string();
    config.save(config_path)?;
    session_from_store(config, config_path, server, username.to_string(), token.to_string())
}

fn session_from_store(
    config: &mut Config,
    config_path: &Path,
    server: String,
    username: String,
    token: String,
) -> anyhow::Result<Session> {
    let pass = config.encrypt_key.clone();
    let old_pass = config.old_encrypt_key.clone().unwrap_or_default();
    let key_revision = if pass.is_empty() { 0 } else { config.key_revision.max(1) };
    let key = if pass.is_empty() {
        None
    } else {
        Some(local_key(config, config_path, &pass)?)
    };
    let old_key = if old_pass.is_empty() {
        None
    } else {
        Some(local_key(config, config_path, &old_pass)?)
    };
    Ok(Session {
        server,
        username,
        token,
        key,
        old_key,
        key_revision,
    })
}

fn local_key(config: &mut Config, config_path: &Path, passphrase: &str) -> anyhow::Result<DataKey> {
    if config.kdf_salt.is_empty() {
        let salt = crypto::random_salt();
        config.kdf_salt = crypto::default_kdf(&salt).salt;
        config.save(config_path)?;
    }
    let kdf = crypto::KdfParams {
        alg: "argon2id".into(),
        salt: config.kdf_salt.clone(),
        m_kib: 19456,
        t: 2,
        p: 1,
        key_len: 32,
    };
    crypto::derive(passphrase, &kdf)
}

fn set_encrypt_key(
    config: &mut Config,
    config_path: &Path,
    session: Option<&Session>,
    new_pass: &str,
) -> anyhow::Result<Option<Session>> {
    let old_pass = config.encrypt_key.clone();
    if old_pass == new_pass {
        return Ok(match session {
            Some(session) => Some(session.clone()),
            None => restore_session(config, config_path),
        });
    }
    if let Some(pending_old) = &config.old_encrypt_key
        && !pending_old.is_empty()
        && pending_old != &old_pass
        && old_pass != new_pass
    {
        anyhow::bail!("Còn dữ liệu chưa mã hoá lại bằng khoá trước. Đồng bộ xong rồi hãy đổi khoá.");
    }
    if old_pass != new_pass {
        config.old_encrypt_key = Some(old_pass);
    }
    config.encrypt_key = new_pass.to_string();
    config.key_revision = if new_pass.is_empty() {
        0
    } else {
        (config.key_revision + 1).max(1)
    };
    config.save(config_path)?;
    if let Some(session) = session {
        return session_from_store(
            config,
            config_path,
            session.server.clone(),
            session.username.clone(),
            session.token.clone(),
        )
        .map(Some);
    }
    Ok(restore_session(config, config_path))
}

fn blob_needs_reencrypt(blob: &Blob, sess: &Session) -> bool {
    match sess.key.as_ref() {
        None => !crypto::is_plain(blob),
        Some(_) => crypto::is_plain(blob) || blob.key_revision != sess.key_revision,
    }
}

async fn resume_rotation(
    pool: &SqlitePool,
    config: &mut Config,
    config_path: &Path,
    sess: &mut Session,
) -> anyhow::Result<()> {
    let Some(old_pass) = config.old_encrypt_key.clone() else {
        return Ok(());
    };
    if old_pass == config.encrypt_key {
        config.old_encrypt_key = None;
        config.save(config_path)?;
        sess.old_key = None;
        return Ok(());
    }
    rotate_server_items(pool, config, config_path, sess).await?;
    if config.old_encrypt_key.is_none() {
        sess.old_key = None;
    }
    Ok(())
}

async fn rotate_server_items(
    pool: &SqlitePool,
    config: &mut Config,
    config_path: &Path,
    sess: &Session,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let mut cursor = String::new();
    let mut leftover = 0u32;
    loop {
        let url = format!("{}/v1/sync/items", sess.server);
        let body = get_url_query(
            &client,
            &sess.token,
            &sess.username,
            &url,
            &[("cursor", cursor.as_str()), ("limit", "100")],
        )
        .await?;
        if let Some(err) = body.get("err").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            anyhow::bail!(err.to_string());
        }
        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for item in &items {
            apply_item(pool, sess, item, ProfileId(config.active_profile)).await?;
            if item.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            let blob: Blob = serde_json::from_value(item.get("blob").cloned().unwrap_or(json!({})))?;
            if blob_needs_reencrypt(&blob, sess) {
                leftover += 1;
                reencrypt_push(
                    &client,
                    pool,
                    sess,
                    item,
                    &blob,
                    ProfileId(config.active_profile),
                )
                .await?;
                leftover = leftover.saturating_sub(1);
            }
        }
        cursor = body
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        config.pull_cursor = cursor.clone();
        config.save(config_path)?;
        if !body.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false) {
            break;
        }
    }
    if leftover == 0 {
        config.old_encrypt_key = None;
        config.save(config_path)?;
    }
    Ok(())
}

async fn reencrypt_push(
    client: &reqwest::Client,
    pool: &SqlitePool,
    sess: &Session,
    item: &Value,
    blob: &Blob,
    active_profile: ProfileId,
) -> anyhow::Result<()> {
    let item_id = item.get("item_id").and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("Missing server item ID"))?;
    let sync_key = item.get("sync_key").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing item sync key"))?;
    let rev = item.get("rev").and_then(|v| v.as_i64()).unwrap_or(1);
    let item_type = item.get("item_type").and_then(|v| v.as_str()).unwrap_or("event");
    let plain = crypto::open_with_keys(sess.key.as_ref(), sess.old_key.as_ref(), sync_key.as_bytes(), blob)?;
    let new_blob = crypto::seal(sess.key.as_ref(), sync_key.as_bytes(), &plain, sess.key_revision)?;
    let req = json!({
        "items": [{
            "item_id": item_id,
            "sync_key": sync_key,
            "item_type": item_type,
            "rev": rev + 1,
            "base_rev": rev,
            "deleted": false,
            "blob": new_blob,
        }]
    });
    let body = post_json(
        client,
        &sess.server,
        &sess.token,
        &sess.username,
        "/v1/sync/items",
        req,
    )
    .await?;
    if let Some(err) = body.get("err").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        anyhow::bail!(err.to_string());
    }
    if item_type == "event" {
        let event_body: EventBody = serde_json::from_slice(&plain)?;
        let event = event_body.into_event(EventId::default(), rev + 1)?;
        store::apply_pulled(pool, item_id, sync_key, &event, false, active_profile).await?;
    }
    Ok(())
}

async fn sync_round(
    pool: &SqlitePool,
    config: &mut Config,
    config_path: &Path,
    sess: &Session,
) -> anyhow::Result<()> {
    pull(pool, config, config_path, sess).await?;
    push_outbox(pool, sess).await?;
    Ok(())
}

async fn pull(pool: &SqlitePool, config: &mut Config, config_path: &Path, sess: &Session) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let mut cursor = config.pull_cursor.clone();
    loop {
        let url = format!("{}/v1/sync/items", sess.server);
        let body = get_url_query(
            &client,
            &sess.token,
            &sess.username,
            &url,
            &[("cursor", cursor.as_str()), ("limit", "100")],
        )
        .await?;
        if let Some(err) = body.get("err").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            anyhow::bail!(err.to_string());
        }
        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for item in items {
            apply_item(pool, sess, &item, ProfileId(config.active_profile)).await?;
        }
        cursor = body
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        config.pull_cursor = cursor.clone();
        config.save(config_path)?;
        if !body.get("has_more").and_then(|v| v.as_bool()).unwrap_or(false) {
            break;
        }
    }
    Ok(())
}

async fn apply_item(
    pool: &SqlitePool,
    sess: &Session,
    item: &Value,
    active_profile: ProfileId,
) -> anyhow::Result<()> {
    let item_id = item.get("item_id").and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("Missing server item ID"))?;
    let sync_key = item.get("sync_key").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing item sync key"))?;
    let item_type = item.get("item_type").and_then(|v| v.as_str()).unwrap_or("");
    if item_type != "event" {
        return Ok(());
    }
    let rev = item.get("rev").and_then(|v| v.as_i64()).unwrap_or(1);
    let deleted = item.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false);
    let blob_v = item.get("blob").cloned().unwrap_or(json!({}));
    let blob: Blob = serde_json::from_value(blob_v)?;
    if deleted {
        let dummy = Event::tombstone_shell(EventId::default(), rev);
        store::apply_pulled(pool, item_id, sync_key, &dummy, true, active_profile).await?;
        return Ok(());
    }
    let plain = crypto::open_with_keys(sess.key.as_ref(), sess.old_key.as_ref(), sync_key.as_bytes(), &blob)?;
    let body: EventBody = serde_json::from_slice(&plain)?;
    let event = body.into_event(EventId::default(), rev)?;
    store::apply_pulled(pool, item_id, sync_key, &event, false, active_profile).await
}

async fn push_outbox(pool: &SqlitePool, sess: &Session) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let rows = store::outbox_rows(pool).await?;
    for row in rows {
        let Some(event) = store::event_by_id(pool, row.item_id).await? else {
            store::drop_outbox(pool, row.item_id).await?;
            continue;
        };
        let plain = serde_json::to_vec(&EventBody::from_event(&event))?;
        let blob = crypto::seal(sess.key.as_ref(), row.sync_key.as_bytes(), &plain, sess.key_revision)?;
        let req = json!({
            "items": [{
            "item_id": row.sync_id,
            "sync_key": row.sync_key,
                "item_type": row.item_type,
                "rev": row.rev,
                "base_rev": row.base_rev,
                "deleted": row.deleted,
                "blob": blob,
            }]
        });
        let body = post_json(
            &client,
            &sess.server,
            &sess.token,
            &sess.username,
            "/v1/sync/items",
            req,
        )
        .await?;
        if let Some(err) = body.get("err").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            if body.get("server").is_some() {
                anyhow::bail!(format!("Xung đột: {err}"));
            }
            anyhow::bail!(err.to_string());
        }
        let result = body.get("results").and_then(Value::as_array)
            .and_then(|items| items.first()).ok_or_else(|| anyhow::anyhow!("Missing sync result"))?;
        if result.get("outcome").and_then(Value::as_str) != Some("applied") {
            let message = result.get("error").and_then(|error| error.get("err"))
                .and_then(Value::as_str).unwrap_or("Sync item was not applied.");
            anyhow::bail!(message.to_string());
        }
        let server_id = result.get("item_id").and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("Missing server item ID"))?;
        store::acknowledge_outbox(pool, row.item_id, server_id).await?;
    }
    Ok(())
}

async fn get_url_query(
    client: &reqwest::Client,
    token: &str,
    username: &str,
    url: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<Value> {
    let resp = client
        .get(url)
        .query(query)
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Username", username)
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({ "err": status.to_string() }));
    if let Some(err) = body.get("err").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        if !status.is_success() {
            anyhow::bail!(err.to_string());
        }
    } else if !status.is_success() {
        anyhow::bail!(status.to_string());
    }
    Ok(body)
}

async fn post_json(
    client: &reqwest::Client,
    server: &str,
    token: &str,
    username: &str,
    path: &str,
    body: Value,
) -> anyhow::Result<Value> {
    let resp = client
        .post(format!("{server}{path}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("X-Username", username)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({ "err": status.to_string() }));
    Ok(body)
}
