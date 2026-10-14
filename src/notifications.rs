use crate::calendar::CivilDate;
use crate::calendar::lookup;
use crate::state::AppState;
use chrono::{Datelike, Duration, Local, LocalResult, TimeZone};
#[cfg(target_os = "linux")]
use std::collections::HashSet;

#[derive(Default)]
pub struct NotificationScheduler {
    #[cfg(target_os = "linux")]
    fired: HashSet<String>,
    #[cfg(target_os = "android")]
    android_schedule: Option<String>,
}

impl NotificationScheduler {
    pub fn fire_due(&mut self, state: &mut AppState) {
        let now = Local::now();
        let today = CivilDate::from_ymd_in_range(now.year(), now.month() as u8, now.day() as u8)
            .unwrap_or(CivilDate::MAX);
        #[cfg(target_os = "android")]
        self.schedule_android(state, today, now);
        #[cfg(target_os = "linux")]
        self.notify_due(state, today, now);
    }

    #[cfg(target_os = "linux")]
    fn notify_due(&mut self, state: &mut AppState, today: CivilDate, now: chrono::DateTime<Local>) {
        let yesterday = today.saturating_add_days(-1);
        let enabled_profiles = state.enabled_profile_ids();
        let occurrences = state.events.expand(&mut state.tables, yesterday, today);

        for occurrence in occurrences {
            if !occurrence.profile_id.is_nil() && !enabled_profiles.contains(&occurrence.profile_id) {
                continue;
            }
            let Some(clock) = occurrence.reminder else {
                continue;
            };
            if !occurrence.enabled || !occurrence.reminder_enabled {
                continue;
            }
            let Some(target) = occurrence_time(occurrence.civil, clock.hour, clock.minute) else {
                continue;
            };
            let age = now.signed_duration_since(target);
            if age < Duration::zero() || age > Duration::hours(24) {
                continue;
            }

            let key = format!("{}:{}", occurrence.event_id.0, occurrence.civil);
            if self.fired.contains(&key) {
                continue;
            }

            let body = notification_body(state, &occurrence);
            match send(&occurrence.title, &body) {
                Ok(()) => {
                    self.fired.insert(key);
                }
                Err(error) => tracing::warn!(%error, "failed to send event notification"),
            }
        }
    }

    #[cfg(target_os = "android")]
    fn schedule_android(&mut self, state: &mut AppState, today: CivilDate, now: chrono::DateTime<Local>) {
        use serde_json::json;

        let until = today.saturating_add_days(365);
        let enabled_profiles = state.enabled_profile_ids();
        let occurrences = state.events.expand(&mut state.tables, today, until);
        let mut schedules = Vec::new();
        for occurrence in occurrences {
            if !occurrence.profile_id.is_nil() && !enabled_profiles.contains(&occurrence.profile_id) {
                continue;
            }
            let Some(clock) = occurrence.reminder else {
                continue;
            };
            if !occurrence.enabled || !occurrence.reminder_enabled {
                continue;
            }
            let Some(target) = occurrence_time(occurrence.civil, clock.hour, clock.minute) else {
                continue;
            };
            if target < now - Duration::hours(24) {
                continue;
            }
            let id = format!("{}:{}", occurrence.event_id.0, occurrence.civil);
            schedules.push(json!({
                "id": id,
                "title": occurrence.title,
                "body": notification_body(state, &occurrence),
                "at": target.timestamp_millis(),
            }));
        }
        schedules.sort_by_key(|item| item["at"].as_i64().unwrap_or_default());
        let Ok(serialized) = serde_json::to_string(&schedules) else {
            return;
        };
        if self.android_schedule.as_deref() == Some(&serialized) {
            return;
        }
        match schedule_android(&serialized) {
            Ok(()) => self.android_schedule = Some(serialized),
            Err(error) => tracing::warn!(%error, "failed to schedule Android notifications"),
        }
    }
}

fn occurrence_time(date: CivilDate, hour: u8, minute: u8) -> Option<chrono::DateTime<Local>> {
    match Local.with_ymd_and_hms(
        date.year(),
        date.month() as u32,
        date.day() as u32,
        hour as u32,
        minute as u32,
        0,
    ) {
        LocalResult::Single(target) => Some(target),
        LocalResult::Ambiguous(first, second) => Some(first.min(second)),
        LocalResult::None => None,
    }
}

fn notification_body(state: &mut AppState, occurrence: &crate::event::Occurrence) -> String {
    let gregorian = format!(
        "{:04}-{:02}-{:02}",
        occurrence.civil.year(),
        occurrence.civil.month(),
        occurrence.civil.day()
    );
    let lunar = lookup(&mut state.tables, occurrence.civil)
        .map(|date| format!("Lunar {}-{:02}-{:02}", date.year_fn, date.month, date.day))
        .unwrap_or_default();
    let mut body = format!("Gregorian {gregorian} - {lunar}");
    if let Some(description) = state
        .events
        .events()
        .iter()
        .find(|event| event.id == occurrence.event_id)
        .and_then(|event| event.description.as_deref())
        .filter(|description| !description.trim().is_empty())
    {
        body.push('\n');
        body.extend(description.chars().take(160));
    }
    body
}

#[cfg(target_os = "linux")]
fn send(title: &str, body: &str) -> anyhow::Result<()> {
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()?;
    Ok(())
}

#[cfg(target_os = "android")]
fn schedule_android(schedules: &str) -> anyhow::Result<()> {
    use jni::{
        JavaVM,
        objects::{JObject, JValue},
    };

    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let context_object = unsafe { JObject::from_raw(env, context.context().cast()) };
        let schedules = JObject::from(env.new_string(schedules)?);
        env.call_static_method(
            jni::jni_str!("net/onlyesterday/rurarendar/NotificationBridge"),
            jni::jni_str!("scheduleAll"),
            jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;)V"),
            &[
                JValue::Object(&context_object),
                JValue::Object(&schedules),
            ],
        )?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn send(_title: &str, _body: &str) -> anyhow::Result<()> {
    anyhow::bail!("system notifications are not supported on this platform")
}