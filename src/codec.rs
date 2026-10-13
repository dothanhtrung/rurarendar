use serde::{
    Deserialize,
    Serialize,
};

use crate::calendar::{
    CivilDate,
    LunarYmd,
};
use crate::event::{
    CalendarBasis,
    ClockTime,
    Event,
    EventId,
    EventKind,
    LeapPolicy,
    Recurrence,
    Reminder,
};

#[derive(Serialize, Deserialize)]
pub struct EventBody {
    pub v: u32,
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub basis: String,
    pub gregorian: Option<Ymd>,
    pub lunar: Option<LunarBody>,
    pub leap_policy: String,
    pub all_day: bool,
    pub start_time: Option<Hm>,
    pub end_time: Option<Hm>,
    pub timezone: String,
    pub reminder: Option<Hm>,
    #[serde(default = "default_enabled")]
    pub reminder_enabled: bool,
    pub recurrence: String,
    pub birthday: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub skip_gregorian: Vec<String>,
    #[serde(default)]
    pub disabled_gregorian: Vec<String>,
    #[serde(default)]
    pub enabled_gregorian: Vec<String>,
    #[serde(default)]
    pub reminder_skip_gregorian: Vec<String>,
    #[serde(default)]
    pub reminder_disabled_gregorian: Vec<String>,
    #[serde(default)]
    pub reminder_enabled_gregorian: Vec<String>,
    pub overlay_catalog_id: Option<String>,
    #[serde(default)]
    pub profile_id: String,
}

fn default_enabled() -> bool {
    true
}

fn iso_date(date: CivilDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn parse_dates(raw: &[String]) -> Vec<CivilDate> {
    let mut dates = Vec::new();
    for text in raw {
        let parts: Vec<&str> = text.split('-').collect();
        if parts.len() != 3 {
            continue;
        }
        let Ok(year) = parts[0].parse::<i32>() else { continue };
        let Ok(month) = parts[1].parse::<u8>() else { continue };
        let Ok(day) = parts[2].parse::<u8>() else { continue };
        if let Ok(date) = CivilDate::from_ymd_in_range(year, month, day) {
            dates.push(date);
        }
    }
    dates.sort();
    dates.dedup();
    dates
}

fn write_dates(dates: &[CivilDate]) -> Vec<String> {
    dates.iter().copied().map(iso_date).collect()
}

#[derive(Serialize, Deserialize)]
pub struct Ymd {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Serialize, Deserialize)]
pub struct LunarBody {
    pub year_fn: i32,
    pub month: u8,
    pub leap: bool,
    pub day: u8,
}

#[derive(Serialize, Deserialize)]
pub struct Hm {
    pub hour: u8,
    pub minute: u8,
}

impl EventBody {
    pub fn from_event(event: &Event) -> Self {
        Self {
            v: 1,
            kind: match event.kind {
                EventKind::User => "user".into(),
            },
            title: event.title.clone(),
            description: event.description.clone(),
            basis: match event.basis {
                CalendarBasis::Gregorian => "gregorian".into(),
                CalendarBasis::Lunar => "lunar".into(),
            },
            gregorian: event.gregorian.map(|d| Ymd {
                year: d.year(),
                month: d.month(),
                day: d.day(),
            }),
            lunar: event.lunar.map(|d| LunarBody {
                year_fn: d.year_fn,
                month: d.month,
                leap: d.leap,
                day: d.day,
            }),
            leap_policy: match event.leap_policy {
                LeapPolicy::NonLeapOnly => "non_leap_only".into(),
                LeapPolicy::LeapOnly => "leap_only".into(),
            },
            all_day: event.all_day,
            start_time: event.start_time.map(|t| Hm {
                hour: t.hour,
                minute: t.minute,
            }),
            end_time: event.end_time.map(|t| Hm {
                hour: t.hour,
                minute: t.minute,
            }),
            timezone: event.timezone.clone(),
            reminder: event.reminder.as_ref().map(|r| Hm {
                hour: r.clock.hour,
                minute: r.clock.minute,
            }),
            reminder_enabled: event.reminder_enabled,
            recurrence: match event.recurrence {
                Recurrence::None => "none".into(),
                Recurrence::Daily => "daily".into(),
                Recurrence::Weekly => "weekly".into(),
                Recurrence::Monthly => "monthly".into(),
                Recurrence::Yearly => "yearly".into(),
            },
            birthday: event.birthday,
            enabled: event.enabled,
            skip_gregorian: write_dates(&event.skip_gregorian),
            disabled_gregorian: write_dates(&event.disabled_gregorian),
            enabled_gregorian: write_dates(&event.enabled_gregorian),
            reminder_skip_gregorian: write_dates(&event.reminder_skip_gregorian),
            reminder_disabled_gregorian: write_dates(&event.reminder_disabled_gregorian),
            reminder_enabled_gregorian: write_dates(&event.reminder_enabled_gregorian),
            overlay_catalog_id: None,
            profile_id: String::new(),
        }
    }

    pub fn into_event(self, id: EventId, rev: i64) -> anyhow::Result<Event> {
        Ok(Event {
            id,
            profile_id: crate::profile::ProfileId::parse(&self.profile_id)
                .unwrap_or_else(crate::profile::ProfileId::nil),
            kind: EventKind::User,
            title: self.title,
            description: self.description,
            basis: match self.basis.as_str() {
                "lunar" => CalendarBasis::Lunar,
                _ => CalendarBasis::Gregorian,
            },
            gregorian: self
                .gregorian
                .map(|d| CivilDate::from_ymd_in_range(d.year, d.month, d.day))
                .transpose()
                .map_err(|_| anyhow::anyhow!("bad gregorian date"))?,
            lunar: self.lunar.map(|d| LunarYmd {
                year_fn: d.year_fn,
                month: d.month,
                leap: d.leap,
                day: d.day,
            }),
            leap_policy: match self.leap_policy.as_str() {
                "leap_only" => LeapPolicy::LeapOnly,
                _ => LeapPolicy::NonLeapOnly,
            },
            all_day: self.all_day,
            start_time: self.start_time.map(|t| ClockTime {
                hour: t.hour,
                minute: t.minute,
            }),
            end_time: self.end_time.map(|t| ClockTime {
                hour: t.hour,
                minute: t.minute,
            }),
            timezone: self.timezone,
            reminder: self.reminder.map(|t| Reminder {
                clock: ClockTime {
                    hour: t.hour,
                    minute: t.minute,
                },
            }),
            reminder_enabled: self.reminder_enabled,
            recurrence: match self.recurrence.as_str() {
                "daily" => Recurrence::Daily,
                "weekly" => Recurrence::Weekly,
                "monthly" => Recurrence::Monthly,
                "yearly" => Recurrence::Yearly,
                _ => Recurrence::None,
            },
            birthday: self.birthday,
            enabled: self.enabled,
            skip_gregorian: parse_dates(&self.skip_gregorian),
            disabled_gregorian: parse_dates(&self.disabled_gregorian),
            enabled_gregorian: parse_dates(&self.enabled_gregorian),
            reminder_skip_gregorian: parse_dates(&self.reminder_skip_gregorian),
            reminder_disabled_gregorian: parse_dates(&self.reminder_disabled_gregorian),
            reminder_enabled_gregorian: parse_dates(&self.reminder_enabled_gregorian),
            rev,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::CivilDate;
    use crate::event::{
        ClockTime,
        EventId,
        NewGregorianEvent,
        Recurrence,
        create_gregorian_event,
    };

    #[test]
    fn old_payload_defaults_to_enabled() {
        let raw = r#"{
            "v": 1,
            "kind": "user",
            "title": "Hop",
            "description": null,
            "basis": "gregorian",
            "gregorian": {"year": 2026, "month": 10, "day": 5},
            "lunar": null,
            "leap_policy": "non_leap_only",
            "all_day": true,
            "start_time": null,
            "end_time": null,
            "timezone": "+07:00",
            "reminder": {"hour": 8, "minute": 0},
            "recurrence": "daily",
            "birthday": false,
            "overlay_catalog_id": null
        }"#;
        let body: EventBody = serde_json::from_str(raw).unwrap();
        let event = body.into_event(EventId(1), 3).unwrap();
        assert!(event.enabled);
        assert!(event.reminder_enabled);
        assert!(event.reminder.is_some());
        assert!(event.skip_gregorian.is_empty());
        assert!(event.disabled_gregorian.is_empty());
        assert!(event.enabled_gregorian.is_empty());
        assert!(event.reminder_skip_gregorian.is_empty());
        assert!(event.reminder_disabled_gregorian.is_empty());
        assert!(event.reminder_enabled_gregorian.is_empty());
    }

    #[test]
    fn exception_lists_roundtrip() {
        let mut event = create_gregorian_event(NewGregorianEvent {
            title: "Họp".into(),
            description: None,
            date: CivilDate::new(2026, 10, 5).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime { hour: 8, minute: 0 }),
            recurrence: Recurrence::Daily,
            birthday: false,
        })
        .unwrap();
        event.enabled = false;
        event.reminder_enabled = false;
        event.skip_gregorian = vec![CivilDate::new(2026, 10, 8).unwrap()];
        event.enabled_gregorian = vec![CivilDate::new(2026, 10, 6).unwrap()];
        event.reminder_enabled_gregorian = vec![CivilDate::new(2026, 10, 6).unwrap()];
        let again = EventBody::from_event(&event)
            .into_event(event.id, event.rev)
            .unwrap();
        assert_eq!(again, event);
    }
}
