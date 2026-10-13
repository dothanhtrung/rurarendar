use sqlx::SqlitePool;

use crate::codec::EventBody;
use crate::event::{
    Event,
    EventId,
    EventStore,
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
    let base_rev = (event.rev - 1).max(0);
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
    let base_rev = (rev - 1).max(0);
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
        r#"SELECT id as "id!", name as "name!", enabled as "enabled!", catalog as "catalog!", sort as "sort!" FROM profiles ORDER BY sort, name"#
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
        });
    }
    Ok(out)
}

pub async fn upsert_profile(pool: &SqlitePool, profile: &Profile) -> anyhow::Result<ProfileId> {
    let enabled = if profile.enabled { 1i64 } else { 0 };
    let id = if profile.id.is_nil() {
        sqlx::query_scalar!(
            r#"INSERT INTO profiles (name, enabled, catalog, sort) VALUES (?, ?, ?, ?) RETURNING id as "id!: i64""#,
            profile.name,
            enabled,
            profile.catalog,
            profile.sort
        ).fetch_one(pool).await?
    } else {
        let id = profile.id.0;
        sqlx::query!(
            "INSERT INTO profiles (id, name, enabled, catalog, sort) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, enabled = excluded.enabled, catalog = excluded.catalog, sort = excluded.sort",
            id,
            profile.name,
            enabled,
            profile.catalog,
            profile.sort
        ).execute(pool).await?;
        id
    };
    Ok(ProfileId(id))
}

pub async fn delete_profile(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query!("DELETE FROM profiles WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(())
}

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
    if let Some(first) = existing.into_iter().next() {
        return Ok(first);
    }
    let mut profile = Profile {
        id: ProfileId::default(),
        name: name.to_string(),
        enabled: true,
        catalog: "vn".into(),
        sort: 0,
    };
    profile.id = upsert_profile(pool, &profile).await?;
    let id = profile.id.0;
    sqlx::query!("UPDATE events SET profile_id = ? WHERE profile_id = 0", id)
        .execute(pool)
        .await?;
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
        tombstone_event(&pool, event.id.0, event.rev + 1).await.unwrap();
        assert!(load_events(&pool).await.unwrap().get(event.id).is_none());
        let deleted = outbox_rows(&pool).await.unwrap().into_iter()
            .find(|row| row.item_id == event.id.0).unwrap();
        assert!(deleted.deleted);
        assert_eq!(deleted.sync_key, original_sync_key);
        assert_eq!(deleted.sync_id, Some(31));
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
