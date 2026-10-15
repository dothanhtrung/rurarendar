//! iCal (RFC 5545) feeds subscribed by URL. Client-only: not synced.
//!
//! Expanded here: all-day and timed `VEVENT`s, exclusive `DTEND` for date
//! values, and `RRULE` of `DAILY` / `WEEKLY` / `MONTHLY` / `YEARLY` with
//! `INTERVAL`, `COUNT`, and `UNTIL`. A rule that uses `BY*` parts keeps the
//! listed start only.

use chrono::{
    Datelike,
    NaiveDateTime,
    Timelike,
};
use crate::calendar::CivilDate;
use crate::event::ClockTime;

const MAX_OCCURRENCES: usize = 2000;
const OPEN_ENDED_DAYS: i64 = 366 * 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcalFeed {
    pub id: i64,
    pub url: String,
    pub name: String,
    pub enabled: bool,
    pub fetched_at: i64,
    pub events: Vec<IcalEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcalCalendar {
    pub name: String,
    pub events: Vec<IcalEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcalEvent {
    pub summary: String,
    pub start: CivilDate,
    pub end_exclusive: CivilDate,
    pub all_day: bool,
    pub start_time: Option<ClockTime>,
    pub end_time: Option<ClockTime>,
}

impl IcalEvent {
    pub fn covers(&self, date: CivilDate) -> bool {
        date >= self.start && date < self.end_exclusive
    }

    pub fn time_label(&self) -> Option<String> {
        if self.all_day {
            return None;
        }
        Some(match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => format!("{start} – {end}"),
            (Some(start), None) => start.to_string(),
            _ => return None,
        })
    }

    pub fn minutes(&self) -> i32 {
        if self.all_day {
            0
        } else {
            self.start_time.map(|clock| clock.minutes_from_midnight()).unwrap_or(0)
        }
    }
}

pub fn normalize_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let url = if let Some(rest) = raw.strip_prefix("webcal://") {
        format!("https://{rest}")
    } else if let Some(rest) = raw.strip_prefix("webcals://") {
        format!("https://{rest}")
    } else {
        raw.to_string()
    };
    if !(url.starts_with("https://") || url.starts_with("http://")) || url.contains(char::is_whitespace) {
        return Err("Địa chỉ iCal cần bắt đầu bằng http://, https:// hoặc webcal://.".into());
    }
    Ok(url)
}

pub async fn fetch_ics(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|err| format!("Không tải được lịch: {err}"))?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "rurarendar")
        .header(reqwest::header::ACCEPT, "text/calendar, text/plain, */*")
        .send()
        .await
        .map_err(|err| format!("Không tải được lịch: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("Máy chủ trả về {}.", response.status()));
    }
    let text = response.text().await.map_err(|err| format!("Không đọc được lịch: {err}"))?;
    if !text.to_ascii_uppercase().contains("BEGIN:VCALENDAR") {
        return Err("Liên kết không phải tệp iCal.".into());
    }
    Ok(text)
}

pub fn parse_ics(text: &str) -> Result<IcalCalendar, String> {
    let unfolded = unfold(text);
    let mut name = String::new();
    let mut events = Vec::new();
    let mut in_event = false;
    let mut skip = 0i32;
    let mut summary = String::new();
    let mut start: Option<Stamp> = None;
    let mut end: Option<Stamp> = None;
    let mut rrule = String::new();
    let mut ms_allday = false;

    for raw in unfolded.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if skip > 0 {
            if upper.starts_with("BEGIN:") {
                skip += 1;
            } else if upper.starts_with("END:") {
                skip -= 1;
            }
            continue;
        }
        if upper == "BEGIN:VEVENT" {
            in_event = true;
            summary.clear();
            start = None;
            end = None;
            rrule.clear();
            ms_allday = false;
            continue;
        }
        if upper == "END:VEVENT" {
            if let Some(start) = start.take() {
                events.extend(expand_event(&summary, start, end.take(), ms_allday, &rrule));
            }
            in_event = false;
            continue;
        }
        if in_event && upper.starts_with("BEGIN:") {
            skip = 1;
            continue;
        }
        let Some((prop, params, value)) = split_prop(line) else {
            continue;
        };
        if !in_event {
            if prop == "X-WR-CALNAME" && name.is_empty() {
                name = unescape(value);
            }
            continue;
        }
        match prop.as_str() {
            "SUMMARY" if summary.is_empty() => summary = unescape(value),
            "DTSTART" => start = parse_stamp(params, value),
            "DTEND" => end = parse_stamp(params, value),
            "RRULE" => rrule = value.to_ascii_uppercase(),
            "X-MICROSOFT-CDO-ALLDAYEVENT" | "X-MICROSOFT-MSNCALENDAR-ALLDAYEVENT" => {
                ms_allday = value.eq_ignore_ascii_case("TRUE");
            }
            _ => {}
        }
    }
    if name.is_empty() {
        name = "iCal".into();
    }
    Ok(IcalCalendar { name, events })
}

pub fn events_on(feeds: &[IcalFeed], date: CivilDate) -> Vec<&IcalEvent> {
    feeds
        .iter()
        .filter(|feed| feed.enabled)
        .flat_map(|feed| feed.events.iter().filter(move |event| event.covers(date)))
        .collect()
}

#[derive(Clone, Copy)]
struct Stamp {
    date: CivilDate,
    time: Option<ClockTime>,
    date_only: bool,
}

#[derive(Clone, Copy)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

struct Rule {
    freq: Freq,
    interval: i32,
    count: Option<usize>,
    until: Option<CivilDate>,
}

fn expand_event(summary: &str, start: Stamp, end: Option<Stamp>, ms_allday: bool, rrule: &str) -> Vec<IcalEvent> {
    let summary = if summary.is_empty() { "Sự kiện".to_string() } else { summary.to_string() };
    let (all_day, end_exclusive, end_time) = bounds(start, end, ms_allday);
    let span = day_span(start.date, end_exclusive).max(1);
    let instances = match parse_rule(rrule) {
        Some(rule) => recur(start.date, span, rule),
        None => vec![(start.date, end_exclusive)],
    };
    instances
        .into_iter()
        .map(|(start_date, end_date)| IcalEvent {
            summary: summary.clone(),
            start: start_date,
            end_exclusive: end_date,
            all_day,
            start_time: if all_day { None } else { start.time },
            end_time: if all_day { None } else { end_time },
            })
        .collect()
}

fn bounds(start: Stamp, end: Option<Stamp>, ms_allday: bool) -> (bool, CivilDate, Option<ClockTime>) {
    let Some(end) = end else {
        let exclusive = start.date.saturating_add_days(1);
        return (!start.time.is_some() || ms_allday, exclusive, None);
    };
    let date_valued = start.date_only || end.date_only;
    let midnight = start.time == Some(ClockTime { hour: 0, minute: 0 }) && end.time == Some(ClockTime { hour: 0, minute: 0 });
    if date_valued || ms_allday || (midnight && end.date > start.date) {
        let exclusive = if end.date > start.date { end.date } else { start.date.saturating_add_days(1) };
        return (true, exclusive, None);
    }
    let end_time = if end.date == start.date { end.time } else { None };
    (false, start.date.saturating_add_days(1), end_time)
}

fn recur(start: CivilDate, span: i64, rule: Rule) -> Vec<(CivilDate, CivilDate)> {
    let mut out = Vec::new();
    let mut cursor = start;
    let horizon = start.saturating_add_days(OPEN_ENDED_DAYS);
    while out.len() < MAX_OCCURRENCES {
        if rule.count.is_some_and(|count| out.len() >= count) {
            break;
        }
        if rule.until.is_some_and(|until| cursor > until) {
            break;
        }
        if rule.count.is_none() && rule.until.is_none() && cursor > horizon {
            break;
        }
        let end = cursor.saturating_add_days(span);
        out.push((cursor, if end > cursor { end } else { cursor.saturating_add_days(1) }));
        let next = step(cursor, rule.freq, rule.interval);
        if next <= cursor {
            break;
        }
        cursor = next;
    }
    out
}

fn step(date: CivilDate, freq: Freq, interval: i32) -> CivilDate {
    let interval = interval.max(1);
    match freq {
        Freq::Daily => date.saturating_add_days(i64::from(interval)),
        Freq::Weekly => date.saturating_add_days(7 * i64::from(interval)),
        Freq::Monthly => date.saturating_add_months(interval),
        Freq::Yearly => date.saturating_add_months(interval.saturating_mul(12)),
    }
}

fn parse_rule(rrule: &str) -> Option<Rule> {
    if rrule.is_empty() {
        return None;
    }
    if rrule.split(';').any(|part| {
        let key = part.split('=').next().unwrap_or("");
        key.starts_with("BY")
    }) {
        return None;
    }
    let mut freq = None;
    let mut interval = 1i32;
    let mut count = None;
    let mut until = None;
    for part in rrule.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "FREQ" => {
                freq = Some(match value {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                });
            }
            "INTERVAL" => interval = value.parse().unwrap_or(1).max(1),
            "COUNT" => count = value.parse().ok(),
            "UNTIL" => until = date_prefix(value),
            _ => {}
        }
    }
    Some(Rule { freq: freq?, interval, count, until })
}

fn parse_stamp(params: &str, value: &str) -> Option<Stamp> {
    let value = value.trim();
    let date_only = params.to_ascii_uppercase().contains("VALUE=DATE") && !params.to_ascii_uppercase().contains("VALUE=DATE-TIME")
        || value.len() == 8;
    if date_only {
        return Some(Stamp { date: parse_date(value)?, time: None, date_only: true });
    }
    let utc = value.ends_with('Z');
    let bare = value.trim_end_matches('Z');
    if utc {
        let (date, time) = parse_utc(bare)?;
        return Some(Stamp { date, time: Some(time), date_only: false });
    }
    let (date_part, time_part) = bare.split_once('T')?;
    Some(Stamp {
        date: parse_date(date_part)?,
        time: parse_clock(time_part),
        date_only: false,
    })
}

fn parse_utc(value: &str) -> Option<(CivilDate, ClockTime)> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let local = naive.and_utc().with_timezone(&chrono::Local);
    let date = CivilDate::from_ymd_in_range(local.year(), local.month() as u8, local.day() as u8).ok()?;
    let time = ClockTime::new(local.hour() as u8, local.minute() as u8).ok()?;
    Some((date, time))
}

fn parse_date(value: &str) -> Option<CivilDate> {
    let digits = date_digits(value)?;
    let year: i32 = digits[..4].parse().ok()?;
    let month: u8 = digits[4..6].parse().ok()?;
    let day: u8 = digits[6..8].parse().ok()?;
    CivilDate::from_ymd_in_range(year, month, day).ok()
}

fn date_prefix(value: &str) -> Option<CivilDate> {
    parse_date(value)
}

fn date_digits(value: &str) -> Option<&str> {
    let digits = value.get(..8)?;
    digits.chars().all(|ch| ch.is_ascii_digit()).then_some(digits)
}

fn parse_clock(value: &str) -> Option<ClockTime> {
    if value.len() < 4 {
        return None;
    }
    let hour: u8 = value.get(..2)?.parse().ok()?;
    let minute: u8 = value.get(2..4)?.parse().ok()?;
    ClockTime::new(hour, minute).ok()
}

fn day_span(start: CivilDate, end: CivilDate) -> i64 {
    let mut count = 0i64;
    let mut date = start;
    while date < end && count < 366 {
        count += 1;
        date = match date.add_days(1) {
            Ok(next) => next,
            Err(_) => break,
        };
    }
    count
}

fn split_prop(line: &str) -> Option<(String, &str, &str)> {
    let (head, value) = line.split_once(':')?;
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    let name = name.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    Some((name, params, value))
}

fn unfold(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if out.is_empty() {
            out.push_str(line);
            continue;
        }
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            out.push_str(rest);
        } else {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFICE: &str = "\
BEGIN:VCALENDAR\r\n\
X-WR-CALNAME:Vietnam Holidays\r\n\
BEGIN:VEVENT\r\n\
UID:2026-01-01VN@example\r\n\
SUMMARY;LANGUAGE=en-us:New Year's Day\r\n\
DTSTART;VALUE=DATE:20260101\r\n\
DTEND;VALUE=DATE:20260102\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
SUMMARY:Tet Eve\r\n\
DTSTART;VALUE=DATE:20260216\r\n\
DTEND;VALUE=DATE:20260217\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn office_holidays_are_single_civil_days() {
        let cal = parse_ics(OFFICE).unwrap();
        assert_eq!(cal.name, "Vietnam Holidays");
        assert_eq!(cal.events.len(), 2);
        assert_eq!(cal.events[0].summary, "New Year's Day");
        assert!(cal.events[0].all_day);
        assert!(cal.events[0].covers(CivilDate::new(2026, 1, 1).unwrap()));
        assert!(!cal.events[0].covers(CivilDate::new(2026, 1, 2).unwrap()));
        assert!(cal.events[1].covers(CivilDate::new(2026, 2, 16).unwrap()));
    }

    #[test]
    fn folded_summary_and_yearly_rule() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Hello\n world\nDTSTART;VALUE=DATE:20260101\nRRULE:FREQ=YEARLY;COUNT=2\nEND:VEVENT\nEND:VCALENDAR\n";
        let cal = parse_ics(text).unwrap();
        assert_eq!(cal.events.len(), 2);
        assert_eq!(cal.events[0].summary, "Helloworld");
        assert_eq!(cal.events[0].start, CivilDate::new(2026, 1, 1).unwrap());
        assert_eq!(cal.events[1].start, CivilDate::new(2027, 1, 1).unwrap());
    }

    #[test]
    fn byday_rule_keeps_the_listed_day_only() {
        let text = "\
BEGIN:VCALENDAR\n\
BEGIN:VEVENT\n\
SUMMARY:Monday\n\
DTSTART;VALUE=DATE:20260105\n\
RRULE:FREQ=WEEKLY;BYDAY=MO\n\
END:VEVENT\n\
END:VCALENDAR\n";
        let cal = parse_ics(text).unwrap();
        assert_eq!(cal.events.len(), 1);
        assert_eq!(cal.events[0].start, CivilDate::new(2026, 1, 5).unwrap());
    }

    #[test]
    fn webcal_url_becomes_https() {
        assert_eq!(
            normalize_url("webcal://www.officeholidays.com/ics-all/vietnam").unwrap(),
            "https://www.officeholidays.com/ics-all/vietnam"
        );
        assert!(normalize_url("ftp://example.com/a.ics").is_err());
    }
}
