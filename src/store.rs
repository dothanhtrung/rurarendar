use sqlx::SqlitePool;

use crate::codec::EventBody;
use crate::event::{
    Event,
    EventId,
    EventStore,
};
use crate::ical::{
    IcalCalendar,
    IcalFeed,
    parse_ics,
};
use crate::profile::{
    Profile,
    ProfileId,
};

pub async fn open_pool(db: &str) -> anyhow::Result<SqlitePool> {
    let opts = db
        .parse::<sqlx::sqlite::SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!("db/migrations/sqlite").run(&pool).await?;
    Ok(pool)
}

pub async fn load_events(pool: &SqlitePool) -> anyhow::Result<EventStore> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", payload as "payload!", rev as "rev!", profile_id as "profile_id!" FROM events WHERE deleted = 0"#
    )
    .fetch_all(pool)
    .await?;
    let mut store = EventStore::default();
    for row in rows {
        let body: EventBody = serde_json::from_str(&row.payload)?;
        let mut event = body.into_event(EventId(row.id), row.rev)?;
        event.profile_id = ProfileId(row.profile_id);
        store.insert(event);
    }
    Ok(store)
}

pub async fn upsert_event(pool: &SqlitePool, event: &Event) -> anyhow::Result<EventId> {
    let payload = serde_json::to_string(&EventBody::from_event(event))?;
    let profile_id = event.profile_id.0;
    // A server the client has never reached still has rev 0, even after local edits.
    let base_rev = if event_synced(pool, event.id).await? {
        (event.rev - 1).max(0)
    } else {
        0
    };
    let mut transaction = pool.begin().await?;
    let id = if event.id == EventId::default() {
        let sync_key = crate::crypto::random_salt()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        sqlx::query!(
            "INSERT INTO events (sync_key, payload, rev, deleted, profile_id) VALUES (?, ?, ?, 0, ?)",
            sync_key,
            payload,
            event.rev,
            profile_id
        )
        .execute(&mut *transaction)
        .await?
        .last_insert_rowid()
    } else {
        let id = event.id.0;
        let result = sqlx::query!(
            "UPDATE events SET payload = ?, rev = ?, deleted = 0, profile_id = ? WHERE id = ?",
            payload,
            event.rev,
            profile_id,
            id
        )
        .execute(&mut *transaction)
        .await?;
        anyhow::ensure!(result.rows_affected() == 1, "Event {id} no longer exists");
        id
    };
    sqlx::query!(
        "INSERT INTO outbox (item_id, item_type, rev, base_rev, deleted)
         VALUES (?, 'event', ?, ?, 0)
         ON CONFLICT(item_id) DO UPDATE SET rev = excluded.rev, base_rev = excluded.base_rev, deleted = 0",
        id,
        event.rev,
        base_rev
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(EventId(id))
}

pub async fn tombstone_event(pool: &SqlitePool, id: i64, rev: i64) -> anyhow::Result<()> {
    let base_rev = if event_synced(pool, EventId(id)).await? {
        (rev - 1).max(0)
    } else {
        0
    };
    sqlx::query!("UPDATE events SET deleted = 1, rev = ? WHERE id = ?", rev, id)
        .execute(pool)
        .await?;
    sqlx::query!(
        "INSERT INTO outbox (item_id, item_type, rev, base_rev, deleted)
         VALUES (?, 'event', ?, ?, 1)
         ON CONFLICT(item_id) DO UPDATE SET rev = excluded.rev, base_rev = excluded.base_rev, deleted = 1",
        id,
        rev,
        base_rev
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn apply_pulled(
    pool: &SqlitePool,
    sync_id: i64,
    sync_key: &str,
    event: &Event,
    deleted: bool,
    active_profile: ProfileId,
) -> anyhow::Result<()> {
    let payload = serde_json::to_string(&EventBody::from_event(event))?;
    let profile_id = if event.profile_id.is_nil() {
        active_profile.0
    } else {
        event.profile_id.0
    };
    let deleted = if deleted { 1i64 } else { 0 };
    let mut transaction = pool.begin().await?;
    let row = sqlx::query!(
        r#"INSERT INTO events (sync_id, sync_key, payload, rev, deleted, profile_id) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(sync_key) DO UPDATE SET sync_id = excluded.sync_id, payload = excluded.payload, rev = excluded.rev, deleted = excluded.deleted, profile_id = excluded.profile_id
         RETURNING id as "id!""#,
        sync_id,
        sync_key,
        payload,
        event.rev,
        deleted,
        profile_id
    )
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query!("DELETE FROM outbox WHERE item_id = ?", row.id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub item_id: i64,
    pub sync_id: Option<i64>,
    pub sync_key: String,
    pub item_type: String,
    pub rev: i64,
    pub base_rev: i64,
    pub deleted: bool,
}

pub async fn outbox_rows(pool: &SqlitePool) -> anyhow::Result<Vec<OutboxRow>> {
    let rows = sqlx::query!(
        r#"SELECT o.item_id as "item_id!", e.sync_id, e.sync_key as "sync_key!", o.item_type as "item_type!", o.rev as "rev!", o.base_rev as "base_rev!", o.deleted as "deleted!" FROM outbox o JOIN events e ON e.id = o.item_id"#
    )
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| OutboxRow {
            item_id: r.item_id,
            sync_id: r.sync_id,
            sync_key: r.sync_key,
            item_type: r.item_type,
            rev: r.rev,
            base_rev: r.base_rev,
            deleted: r.deleted != 0,
        })
        .collect())
}

async fn event_synced(pool: &SqlitePool, id: EventId) -> anyhow::Result<bool> {
    if id == EventId::default() {
        return Ok(false);
    }
    let id = id.0;
    let sync_id = sqlx::query_scalar!(
        r#"SELECT sync_id as "sync_id?" FROM events WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(sync_id.flatten().is_some())
}

pub async fn drop_outbox(pool: &SqlitePool, item_id: i64) -> anyhow::Result<()> {
    sqlx::query!("DELETE FROM outbox WHERE item_id = ?", item_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn acknowledge_outbox(pool: &SqlitePool, item_id: i64, sync_id: i64) -> anyhow::Result<()> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query!("UPDATE events SET sync_id = ? WHERE id = ?", sync_id, item_id)
        .execute(&mut *transaction)
        .await?;
    anyhow::ensure!(result.rows_affected() == 1, "Event {item_id} no longer exists");
    sqlx::query!("DELETE FROM outbox WHERE item_id = ?", item_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn event_by_id(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Event>> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", payload as "payload!", rev as "rev!" FROM events WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    match row {
        None => Ok(None),
        Some(row) => {
            let body: EventBody = serde_json::from_str(&row.payload)?;
            Ok(Some(body.into_event(EventId(row.id), row.rev)?))
        }
    }
}

pub async fn load_profiles(pool: &SqlitePool) -> anyhow::Result<Vec<Profile>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", name as "name!", enabled as "enabled!", catalog as "catalog!", sort as "sort!", embedded as "embedded!" FROM profiles ORDER BY sort, name"#
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        out.push(Profile {
            id: ProfileId(row.id),
            name: row.name,
            enabled: row.enabled != 0,
            catalog: row.catalog,
            sort: row.sort,
            embedded: row.embedded != 0,
        });
    }
    Ok(out)
}

pub async fn upsert_profile(pool: &SqlitePool, profile: &Profile) -> anyhow::Result<ProfileId> {
    let enabled = if profile.enabled { 1i64 } else { 0 };
    let embedded = if profile.embedded { 1i64 } else { 0 };
    let id = if profile.id.is_nil() {
        sqlx::query_scalar!(
            r#"INSERT INTO profiles (name, enabled, catalog, sort, embedded) VALUES (?, ?, ?, ?, ?) RETURNING id as "id!: i64""#,
            profile.name,
            enabled,
            profile.catalog,
            profile.sort,
            embedded
        ).fetch_one(pool).await?
    } else {
        let id = profile.id.0;
        sqlx::query!(
            "INSERT INTO profiles (id, name, enabled, catalog, sort, embedded) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, enabled = excluded.enabled, catalog = excluded.catalog, sort = excluded.sort, embedded = excluded.embedded",
            id,
            profile.name,
            enabled,
            profile.catalog,
            profile.sort,
            embedded
        ).execute(pool).await?;
        id
    };
    Ok(ProfileId(id))
}

pub async fn is_embedded_profile(pool: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let embedded = sqlx::query_scalar!(
        r#"SELECT embedded as "embedded!" FROM profiles WHERE id = ?"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(embedded.is_some_and(|value| value != 0))
}

pub async fn delete_profile(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    if is_embedded_profile(pool, id).await? {
        anyhow::bail!("embedded profile cannot be deleted");
    }
    sqlx::query!("DELETE FROM profiles WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn load_ical_feeds(pool: &SqlitePool) -> anyhow::Result<Vec<IcalFeed>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", url as "url!", name as "name!", enabled as "enabled!", body as "body!", fetched_at as "fetched_at!" FROM ical_feeds ORDER BY name, id"#
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        let parsed = parse_ics(&row.body).unwrap_or(IcalCalendar {
            name: row.name.clone(),
            events: Vec::new(),
        });
        out.push(IcalFeed {
            id: row.id,
            url: row.url,
            name: row.name,
            enabled: row.enabled != 0,
            fetched_at: row.fetched_at,
            events: parsed.events,
        });
    }
    Ok(out)
}

pub async fn insert_ical_feed(pool: &SqlitePool, url: &str, name: &str, body: &str, fetched_at: i64) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        r#"INSERT INTO ical_feeds (url, name, enabled, body, fetched_at) VALUES (?, ?, 1, ?, ?) RETURNING id as "id!: i64""#,
        url,
        name,
        body,
        fetched_at
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn update_ical_body(pool: &SqlitePool, id: i64, body: &str, fetched_at: i64) -> anyhow::Result<()> {
    sqlx::query!("UPDATE ical_feeds SET body = ?, fetched_at = ? WHERE id = ?", body, fetched_at, id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_ical_enabled(pool: &SqlitePool, id: i64, enabled: bool) -> anyhow::Result<()> {
    let enabled = if enabled { 1i64 } else { 0 };
    sqlx::query!("UPDATE ical_feeds SET enabled = ? WHERE id = ?", enabled, id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_ical_feed(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query!("DELETE FROM ical_feeds WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(())
}

pub const ICAL_REFRESH_SECS: i64 = 48 * 60 * 60;

pub async fn tombstone_profile_events(pool: &SqlitePool, profile_id: i64) -> anyhow::Result<Vec<(i64, i64)>> {
    let rows = sqlx::query!(
        r#"SELECT id as "id!", rev as "rev!" FROM events WHERE profile_id = ? AND deleted = 0"#,
        profile_id
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        let next_rev = row.rev + 1;
        tombstone_event(pool, row.id, next_rev).await?;
        out.push((row.id, next_rev));
    }
    Ok(out)
}

pub async fn ensure_default_profile(pool: &SqlitePool, name: &str) -> anyhow::Result<Profile> {
    let existing = load_profiles(pool).await?;
    if let Some(first) = existing.iter().find(|profile| !profile.embedded) {
        return Ok(first.clone());
    }
    let sort = existing.iter().map(|profile| profile.sort).max().unwrap_or(-1) + 1;
    let mut profile = Profile {
        id: ProfileId::default(),
        name: name.to_string(),
        enabled: true,
        catalog: String::new(),
        sort,
        embedded: false,
    };
    profile.id = upsert_profile(pool, &profile).await?;
    let id = profile.id.0;
    sqlx::query!("UPDATE events SET profile_id = ? WHERE profile_id = 0", id)
        .execute(pool)
        .await?;
    Ok(profile)
}

/// The shipped `assets/catalog` set. Created once, then only renamed for the UI language.
/// A profile that already pointed at that slug gives it up so the days are not listed twice.
pub async fn ensure_embedded_profile(pool: &SqlitePool, name: &str) -> anyhow::Result<Profile> {
    let existing = load_profiles(pool).await?;
    if let Some(found) = existing.iter().find(|profile| profile.embedded) {
        if found.name == name {
            return Ok(found.clone());
        }
        let mut renamed = found.clone();
        renamed.name = name.to_string();
        upsert_profile(pool, &renamed).await?;
        return Ok(renamed);
    }
    for profile in &existing {
        if profile.catalog_slug() == Some(crate::profile::EMBEDDED_CATALOG) {
            let mut cleared = profile.clone();
            cleared.catalog.clear();
            upsert_profile(pool, &cleared).await?;
        }
    }
    let sort = existing.iter().map(|profile| profile.sort).min().unwrap_or(0) - 1;
    let mut profile = Profile {
        id: ProfileId::default(),
        name: name.to_string(),
        enabled: true,
        catalog: crate::profile::EMBEDDED_CATALOG.into(),
        sort,
        embedded: true,
    };
    profile.id = upsert_profile(pool, &profile).await?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::CivilDate;
    use crate::event::{NewGregorianEvent, Recurrence, create_gregorian_event};

    async fn memory_pool() -> SqlitePool {
        sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
            .connect("sqlite::memory:").await.unwrap()
    }

    fn draft() -> Event {
        create_gregorian_event(NewGregorianEvent {
            title: "Event".into(),
            description: None,
            date: CivilDate::new(2026, 10, 7).unwrap(),
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "UTC".into(),
            reminder: None,
            recurrence: Recurrence::None,
            birthday: false,
        }).unwrap()
    }

    #[tokio::test]
    async fn sqlite_assigns_ids_and_updates_keep_identity() {
        let pool = memory_pool().await;
        sqlx::migrate!("db/migrations/sqlite").run(&pool).await.unwrap();
        let mut event = draft();
        assert_eq!(event.id, EventId::default());
        event.id = upsert_event(&pool, &event).await.unwrap();
        assert!(event.id.0 > 0);
        let second_id = upsert_event(&pool, &draft()).await.unwrap();
        assert_ne!(second_id, event.id);
        let original = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert!(original.sync_id.is_none());
        let original_sync_key = original.sync_key.clone();
        acknowledge_outbox(&pool, event.id.0, 31).await.unwrap();
        assert_eq!(outbox_rows(&pool).await.unwrap().len(), 1);
        event.title = "Edited".into();
        event.rev += 1;
        assert_eq!(upsert_event(&pool, &event).await.unwrap(), event.id);
        assert_eq!(event_by_id(&pool, event.id.0).await.unwrap(), Some(event.clone()));
        let updated = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert_eq!(updated.sync_key, original_sync_key);
        assert_eq!(updated.sync_id, Some(31));
        assert_eq!(updated.rev, event.rev);
        assert_eq!(updated.base_rev, event.rev - 1);
        tombstone_event(&pool, event.id.0, event.rev + 1).await.unwrap();
        assert!(load_events(&pool).await.unwrap().get(event.id).is_none());
        let deleted = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert!(deleted.deleted);
        assert_eq!(deleted.sync_key, original_sync_key);
        assert_eq!(deleted.sync_id, Some(31));
        assert_eq!(deleted.base_rev, event.rev);
    }

    #[tokio::test]
    async fn unsynced_edits_keep_base_rev_zero() {
        let pool = memory_pool().await;
        sqlx::migrate!("db/migrations/sqlite").run(&pool).await.unwrap();
        let mut event = draft();
        event.id = upsert_event(&pool, &event).await.unwrap();
        event.title = "Edited before sync".into();
        event.rev += 1;
        upsert_event(&pool, &event).await.unwrap();
        event.rev += 1;
        upsert_event(&pool, &event).await.unwrap();
        let row = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert!(row.sync_id.is_none());
        assert_eq!(row.rev, event.rev);
        assert_eq!(row.base_rev, 0);
        tombstone_event(&pool, event.id.0, event.rev + 1).await.unwrap();
        let deleted = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert!(deleted.deleted);
        assert_eq!(deleted.base_rev, 0);
    }

    #[tokio::test]
    async fn assets_catalog_becomes_an_embedded_profile() {
        let pool = memory_pool().await;
        sqlx::migrate!("db/migrations/sqlite").run(&pool).await.unwrap();
        let mut personal = Profile {
            id: ProfileId::default(),
            name: "Cá nhân".into(),
            enabled: true,
            catalog: "vn".into(),
            sort: 0,
            embedded: false,
        };
        personal.id = upsert_profile(&pool, &personal).await.unwrap();
        let embedded = ensure_embedded_profile(&pool, "Ngày lễ tích hợp").await.unwrap();
        assert!(embedded.embedded);
        assert_eq!(embedded.catalog, "vn");
        assert!(embedded.sort < personal.sort);
        let profiles = load_profiles(&pool).await.unwrap();
        let kept = profiles.iter().find(|profile| profile.id == personal.id).unwrap();
        assert!(kept.catalog.is_empty());
        assert!(!kept.embedded);
        assert_eq!(profiles.iter().filter(|profile| profile.embedded).count(), 1);
        let renamed = ensure_embedded_profile(&pool, "Embedded holiday").await.unwrap();
        assert_eq!(renamed.id, embedded.id);
        assert_eq!(renamed.name, "Embedded holiday");
        assert!(delete_profile(&pool, embedded.id.0).await.is_err());
        assert!(is_embedded_profile(&pool, embedded.id.0).await.unwrap());
        let ordinary = ensure_default_profile(&pool, "Personal").await.unwrap();
        assert_eq!(ordinary.id, personal.id);
        assert!(!ordinary.embedded);
    }

    #[tokio::test]
    async fn pulled_events_use_local_ids_and_preserve_sync_identity() {
        let pool = memory_pool().await;
        sqlx::migrate!("db/migrations/sqlite").run(&pool).await.unwrap();
        let mut local = draft();
        local.id = upsert_event(&pool, &local).await.unwrap();
        let sync_key = outbox_rows(&pool).await.unwrap()[0].sync_key.clone();
        let mut remote = draft();
        remote.title = "Remote edit".into();
        remote.rev = 2;
        apply_pulled(&pool, 42, &sync_key, &remote, false, ProfileId::default()).await.unwrap();
        let loaded = load_events(&pool).await.unwrap();
        assert_eq!(loaded.events().len(), 1);
        let local_id = local.id;
        assert_eq!(loaded.events()[0].id, local_id);
        assert_eq!(loaded.events()[0].title, remote.title);
        assert!(outbox_rows(&pool).await.unwrap().is_empty());
        apply_pulled(&pool, 43, "remote-sync-key", &remote, false, ProfileId::default()).await.unwrap();
        let loaded = load_events(&pool).await.unwrap();
        assert_eq!(loaded.events().len(), 2);
        assert_ne!(loaded.events()[0].id, loaded.events()[1].id);
        remote.rev = 3;
        apply_pulled(&pool, 42, &sync_key, &remote, true, ProfileId::default()).await.unwrap();
        assert!(load_events(&pool).await.unwrap().get(local_id).is_none());
    }
}
