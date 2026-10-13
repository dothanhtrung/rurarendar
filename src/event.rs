//! Create user events. An optional alert clock is a field of the event, not a second record.

use crate::calendar::{
    CalendarTables,
    CivilDate,
    ConvertError,
    LunarYmd,
    MAX_YEAR,
    MIN_YEAR,
    civil_from_lunar_exact,
};
use crate::profile::ProfileId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EventId(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarBasis {
    Gregorian,
    Lunar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeapPolicy {
    /// Fire in the non-leap month M (default when created outside a leap month).
    NonLeapOnly,
    /// Fire only in leap month M, in years that have it.
    LeapOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recurrence {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockTime {
    pub hour: u8,
    pub minute: u8,
}

impl ClockTime {
    pub const REMINDER_DEFAULT: ClockTime = ClockTime { hour: 8, minute: 0 };

    pub fn new(hour: u8, minute: u8) -> Result<Self, EventError> {
        if hour > 23 || minute > 59 {
            return Err(EventError::InvalidClock);
        }
        Ok(Self { hour, minute })
    }

    pub fn parse(text: &str) -> Result<Self, EventError> {
        let text = text.trim();
        let (h, m) = text.split_once(':').ok_or(EventError::InvalidClock)?;
        let hour: u8 = h.trim().parse().map_err(|_| EventError::InvalidClock)?;
        let minute: u8 = m.trim().parse().map_err(|_| EventError::InvalidClock)?;
        Self::new(hour, minute)
    }

    pub fn minutes_from_midnight(self) -> i32 {
        i32::from(self.hour) * 60 + i32::from(self.minute)
    }
}

impl std::fmt::Display for ClockTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reminder {
    pub clock: ClockTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub id: EventId,
    pub profile_id: ProfileId,
    pub kind: EventKind,
    pub title: String,
    pub description: Option<String>,
    pub basis: CalendarBasis,
    pub gregorian: Option<CivilDate>,
    pub lunar: Option<LunarYmd>,
    pub leap_policy: LeapPolicy,
    pub all_day: bool,
    pub start_time: Option<ClockTime>,
    pub end_time: Option<ClockTime>,
    pub timezone: String,
    pub reminder: Option<Reminder>,
    /// Series switch. Per-day exceptions live in the gregorian lists below.
    pub reminder_enabled: bool,
    pub recurrence: Recurrence,
    pub birthday: bool,
    /// Series switch. Off hides nothing; the occurrence stays so it can be turned back on.
    pub enabled: bool,
    /// Occurrences removed for these civil days ("this day" / "this period" delete).
    pub skip_gregorian: Vec<CivilDate>,
    /// Occurrences forced off while the series is on.
    pub disabled_gregorian: Vec<CivilDate>,
    /// Occurrences forced on while the series is off.
    pub enabled_gregorian: Vec<CivilDate>,
    pub reminder_skip_gregorian: Vec<CivilDate>,
    pub reminder_disabled_gregorian: Vec<CivilDate>,
    pub reminder_enabled_gregorian: Vec<CivilDate>,
    pub rev: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub event_id: EventId,
    pub profile_id: ProfileId,
    pub civil: CivilDate,
    pub title: String,
    pub all_day: bool,
    pub start_time: Option<ClockTime>,
    pub end_time: Option<ClockTime>,
    /// Clock is set when a reminder row should still be shown, including a disabled one.
    pub reminder: Option<ClockTime>,
    pub enabled: bool,
    pub reminder_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewGregorianEvent {
    pub title: String,
    pub description: Option<String>,
    pub date: CivilDate,
    pub all_day: bool,
    pub start_time: Option<ClockTime>,
    pub end_time: Option<ClockTime>,
    pub timezone: String,
    pub reminder: Option<ClockTime>,
    pub recurrence: Recurrence,
    pub birthday: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewLunarEvent {
    pub title: String,
    pub description: Option<String>,
    pub date: LunarYmd,
    pub all_day: bool,
    pub start_time: Option<ClockTime>,
    pub end_time: Option<ClockTime>,
    pub timezone: String,
    pub reminder: Option<ClockTime>,
    pub recurrence: Recurrence,
    pub birthday: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    EmptyTitle,
    InvalidClock,
    EndBeforeStart,
    OutOfRange,
    Convert(ConvertError),
    NothingInRange,
    NoReminder,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::EmptyTitle => write!(f, "Cần tiêu đề."),
            EventError::InvalidClock => write!(f, "Giờ không hợp lệ."),
            EventError::EndBeforeStart => write!(f, "Giờ kết thúc phải sau giờ bắt đầu."),
            EventError::OutOfRange => write!(f, "Ngoài 1900–2100 sẽ bị từ chối."),
            EventError::Convert(err) => write!(f, "{err}"),
            EventError::NothingInRange => write!(f, "Không có sự kiện trong khoảng này."),
            EventError::NoReminder => write!(f, "Sự kiện này không có nhắc nhở."),
        }
    }
}

impl std::error::Error for EventError {}

impl From<ConvertError> for EventError {
    fn from(value: ConvertError) -> Self {
        Self::Convert(value)
    }
}

fn normalize_title(title: String) -> Result<String, EventError> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(EventError::EmptyTitle);
    }
    Ok(title)
}

fn validate_times(all_day: bool, start_time: Option<ClockTime>, end_time: Option<ClockTime>) -> Result<(), EventError> {
    if all_day {
        return Ok(());
    }
    if let (Some(start), Some(end)) = (start_time, end_time)
        && end.minutes_from_midnight() <= start.minutes_from_midnight()
    {
        return Err(EventError::EndBeforeStart);
    }
    Ok(())
}

fn reminder_from(clock: Option<ClockTime>) -> Option<Reminder> {
    clock.map(|clock| Reminder { clock })
}

/// Create a user event on the Gregorian calendar. Reminder is optional (SPEC §7.5).
pub fn create_gregorian_event(input: NewGregorianEvent) -> Result<Event, EventError> {
    let title = normalize_title(input.title)?;
    validate_times(input.all_day, input.start_time, input.end_time)?;
    let date = CivilDate::from_ymd_in_range(input.date.year(), input.date.month(), input.date.day())
        .map_err(|_| EventError::OutOfRange)?;
    let recurrence = if input.birthday { Recurrence::Yearly } else { input.recurrence };
    Ok(Event {
        id: EventId::default(),
        profile_id: ProfileId::nil(),
        kind: EventKind::User,
        title,
        description: input.description.filter(|s| !s.trim().is_empty()),
        basis: CalendarBasis::Gregorian,
        gregorian: Some(date),
        lunar: None,
        leap_policy: LeapPolicy::NonLeapOnly,
        all_day: input.all_day,
        start_time: if input.all_day { None } else { input.start_time },
        end_time: if input.all_day { None } else { input.end_time },
        timezone: input.timezone,
        reminder: reminder_from(input.reminder),
        reminder_enabled: true,
        recurrence,
        birthday: input.birthday,
        enabled: true,
        skip_gregorian: Vec::new(),
        disabled_gregorian: Vec::new(),
        enabled_gregorian: Vec::new(),
        reminder_skip_gregorian: Vec::new(),
        reminder_disabled_gregorian: Vec::new(),
        reminder_enabled_gregorian: Vec::new(),
        rev: 1,
    })
}

/// Create a user event on the lunar calendar. Civil day is derived from month-start CSVs.
pub fn create_lunar_event(tables: &mut CalendarTables, input: NewLunarEvent) -> Result<Event, EventError> {
    let title = normalize_title(input.title)?;
    validate_times(input.all_day, input.start_time, input.end_time)?;
    let _civil = civil_from_lunar_exact(tables, input.date)?;
    let leap_policy = if input.date.leap { LeapPolicy::LeapOnly } else { LeapPolicy::NonLeapOnly };
    let recurrence = if input.birthday { Recurrence::Yearly } else { input.recurrence };
    Ok(Event {
        id: EventId::default(),
        profile_id: ProfileId::nil(),
        kind: EventKind::User,
        title,
        description: input.description.filter(|s| !s.trim().is_empty()),
        basis: CalendarBasis::Lunar,
        gregorian: None,
        lunar: Some(input.date),
        leap_policy,
        all_day: input.all_day,
        start_time: if input.all_day { None } else { input.start_time },
        end_time: if input.all_day { None } else { input.end_time },
        timezone: input.timezone,
        reminder: reminder_from(input.reminder),
        reminder_enabled: true,
        recurrence,
        birthday: input.birthday,
        enabled: true,
        skip_gregorian: Vec::new(),
        disabled_gregorian: Vec::new(),
        enabled_gregorian: Vec::new(),
        reminder_skip_gregorian: Vec::new(),
        reminder_disabled_gregorian: Vec::new(),
        reminder_enabled_gregorian: Vec::new(),
        rev: 1,
    })
}

/// Attach or replace the single reminder slot (SPEC §7.5).
pub fn set_reminder(event: &mut Event, clock: ClockTime) {
    event.reminder = Some(Reminder { clock });
    event.reminder_enabled = true;
    event.rev += 1;
}

impl Event {
    /// Placeholder used when a sync tombstone has no ciphertext to decode.
    pub fn tombstone_shell(id: EventId, rev: i64) -> Self {
        Self {
            id,
            profile_id: ProfileId::nil(),
            kind: EventKind::User,
            title: String::new(),
            description: None,
            basis: CalendarBasis::Gregorian,
            gregorian: None,
            lunar: None,
            leap_policy: LeapPolicy::NonLeapOnly,
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: String::new(),
            reminder: None,
            reminder_enabled: true,
            recurrence: Recurrence::None,
            birthday: false,
            enabled: true,
            skip_gregorian: Vec::new(),
            disabled_gregorian: Vec::new(),
            enabled_gregorian: Vec::new(),
            reminder_skip_gregorian: Vec::new(),
            reminder_disabled_gregorian: Vec::new(),
            reminder_enabled_gregorian: Vec::new(),
            rev,
        }
    }
}

pub fn clear_reminder(event: &mut Event) {
    event.reminder = None;
    event.reminder_enabled = true;
    event.reminder_skip_gregorian.clear();
    event.reminder_disabled_gregorian.clear();
    event.reminder_enabled_gregorian.clear();
    event.rev += 1;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Event,
    Reminder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemScope {
    Day,
    Period,
    Series,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    Enable,
    Disable,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionEffect {
    Updated,
    Deleted,
}

fn insert_date(dates: &mut Vec<CivilDate>, date: CivilDate) {
    if !dates.contains(&date) {
        dates.push(date);
        dates.sort();
    }
}

fn remove_date(dates: &mut Vec<CivilDate>, date: CivilDate) {
    dates.retain(|existing| *existing != date);
}

fn occurrence_enabled(event: &Event, date: CivilDate) -> bool {
    if event.enabled_gregorian.contains(&date) {
        return true;
    }
    event.enabled && !event.disabled_gregorian.contains(&date)
}

fn reminder_visible(event: &Event, date: CivilDate) -> bool {
    event.reminder.is_some() && !event.reminder_skip_gregorian.contains(&date)
}

fn reminder_is_enabled(event: &Event, date: CivilDate) -> bool {
    if !reminder_visible(event, date) {
        return false;
    }
    if event.reminder_enabled_gregorian.contains(&date) {
        return true;
    }
    event.reminder_enabled && !event.reminder_disabled_gregorian.contains(&date)
}

fn enable_event_on(event: &mut Event, date: CivilDate) {
    remove_date(&mut event.disabled_gregorian, date);
    if event.enabled {
        remove_date(&mut event.enabled_gregorian, date);
    } else {
        insert_date(&mut event.enabled_gregorian, date);
    }
}

fn disable_event_on(event: &mut Event, date: CivilDate) {
    remove_date(&mut event.enabled_gregorian, date);
    if event.enabled {
        insert_date(&mut event.disabled_gregorian, date);
    } else {
        remove_date(&mut event.disabled_gregorian, date);
    }
}

fn delete_event_on(event: &mut Event, date: CivilDate) {
    insert_date(&mut event.skip_gregorian, date);
    remove_date(&mut event.disabled_gregorian, date);
    remove_date(&mut event.enabled_gregorian, date);
    remove_date(&mut event.reminder_skip_gregorian, date);
    remove_date(&mut event.reminder_disabled_gregorian, date);
    remove_date(&mut event.reminder_enabled_gregorian, date);
}

fn enable_reminder_on(event: &mut Event, date: CivilDate) {
    remove_date(&mut event.reminder_skip_gregorian, date);
    remove_date(&mut event.reminder_disabled_gregorian, date);
    if event.reminder_enabled {
        remove_date(&mut event.reminder_enabled_gregorian, date);
    } else {
        insert_date(&mut event.reminder_enabled_gregorian, date);
    }
}

fn disable_reminder_on(event: &mut Event, date: CivilDate) {
    remove_date(&mut event.reminder_enabled_gregorian, date);
    if event.reminder_enabled {
        insert_date(&mut event.reminder_disabled_gregorian, date);
    } else {
        remove_date(&mut event.reminder_disabled_gregorian, date);
    }
}

fn delete_reminder_on(event: &mut Event, date: CivilDate) {
    insert_date(&mut event.reminder_skip_gregorian, date);
    remove_date(&mut event.reminder_disabled_gregorian, date);
    remove_date(&mut event.reminder_enabled_gregorian, date);
}

fn clear_reminder_rules(event: &mut Event) {
    event.reminder = None;
    event.reminder_enabled = true;
    event.reminder_skip_gregorian.clear();
    event.reminder_disabled_gregorian.clear();
    event.reminder_enabled_gregorian.clear();
}

fn apply_event_series(event: &mut Event, action: ItemAction) -> ActionEffect {
    match action {
        ItemAction::Enable => {
            event.enabled = true;
            event.disabled_gregorian.clear();
            event.enabled_gregorian.clear();
            ActionEffect::Updated
        }
        ItemAction::Disable => {
            event.enabled = false;
            event.disabled_gregorian.clear();
            event.enabled_gregorian.clear();
            ActionEffect::Updated
        }
        ItemAction::Delete => ActionEffect::Deleted,
    }
}

fn apply_reminder_series(event: &mut Event, action: ItemAction) -> Result<ActionEffect, EventError> {
    if event.reminder.is_none() {
        return Err(EventError::NoReminder);
    }
    match action {
        ItemAction::Enable => {
            event.reminder_enabled = true;
            event.reminder_disabled_gregorian.clear();
            event.reminder_enabled_gregorian.clear();
        }
        ItemAction::Disable => {
            event.reminder_enabled = false;
            event.reminder_disabled_gregorian.clear();
            event.reminder_enabled_gregorian.clear();
        }
        ItemAction::Delete => clear_reminder_rules(event),
    }
    Ok(ActionEffect::Updated)
}

fn one_shot(event: &Event) -> bool {
    event.recurrence == Recurrence::None && !event.birthday
}

fn apply_on_dates(
    event: &mut Event,
    kind: ItemKind,
    action: ItemAction,
    dates: &[CivilDate],
) -> Result<ActionEffect, EventError> {
    match kind {
        ItemKind::Event => {
            if action == ItemAction::Delete && one_shot(event) {
                return Ok(ActionEffect::Deleted);
            }
            for date in dates {
                match action {
                    ItemAction::Enable => enable_event_on(event, *date),
                    ItemAction::Disable => disable_event_on(event, *date),
                    ItemAction::Delete => delete_event_on(event, *date),
                }
            }
            Ok(ActionEffect::Updated)
        }
        ItemKind::Reminder => {
            if event.reminder.is_none() {
                return Err(EventError::NoReminder);
            }
            if action == ItemAction::Delete && one_shot(event) {
                clear_reminder_rules(event);
                return Ok(ActionEffect::Updated);
            }
            for date in dates {
                match action {
                    ItemAction::Enable => enable_reminder_on(event, *date),
                    ItemAction::Disable => disable_reminder_on(event, *date),
                    ItemAction::Delete => delete_reminder_on(event, *date),
                }
            }
            Ok(ActionEffect::Updated)
        }
    }
}

/// Enable, disable, or delete an event or its reminder.
/// `Day` uses `day`. `Period` uses every occurrence from `period_from` through `period_to`.
/// `Series` ignores the dates. Delete of a one-shot event removes the whole record.
pub fn apply_item_action(
    tables: &mut CalendarTables,
    event: &mut Event,
    kind: ItemKind,
    action: ItemAction,
    scope: ItemScope,
    day: CivilDate,
    period_from: CivilDate,
    period_to: CivilDate,
) -> Result<ActionEffect, EventError> {
    let effect = match scope {
        ItemScope::Series => match kind {
            ItemKind::Event => apply_event_series(event, action),
            ItemKind::Reminder => apply_reminder_series(event, action)?,
        },
        ItemScope::Day | ItemScope::Period => {
            let (from, to) = if scope == ItemScope::Day {
                (day, day)
            } else {
                (period_from, period_to)
            };
            let dates = expand_event(tables, event, from, to)
                .into_iter()
                .map(|occ| occ.civil)
                .collect::<Vec<_>>();
            if dates.is_empty() {
                return Err(EventError::NothingInRange);
            }
            apply_on_dates(event, kind, action, &dates)?
        }
    };
    event.rev = event.rev.saturating_add(1);
    Ok(effect)
}

#[derive(Clone, Debug, Default)]
pub struct EventStore {
    events: Vec<Event>,
}

impl EventStore {
    pub fn insert(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn get(&self, id: EventId) -> Option<&Event> {
        self.events.iter().find(|event| event.id == id)
    }

    pub fn replace(&mut self, event: Event) {
        if let Some(slot) = self.events.iter_mut().find(|existing| existing.id == event.id) {
            *slot = event;
        } else {
            self.events.push(event);
        }
    }

    pub fn remove(&mut self, id: EventId) -> Option<Event> {
        let pos = self.events.iter().position(|event| event.id == id)?;
        Some(self.events.remove(pos))
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn assign_nil_profile(&mut self, id: ProfileId) {
        for event in &mut self.events {
            if event.profile_id.is_nil() {
                event.profile_id = id;
            }
        }
    }

    pub fn remove_profile(&mut self, id: ProfileId) {
        self.events.retain(|event| event.profile_id != id);
    }

    pub fn expand(&self, tables: &mut CalendarTables, from: CivilDate, to: CivilDate) -> Vec<Occurrence> {
        let mut out = Vec::new();
        for event in &self.events {
            out.extend(expand_event(tables, event, from, to));
        }
        out.sort_by_key(|occ| {
            (
                occ.civil,
                occ.start_time.map(|t| t.minutes_from_midnight()).unwrap_or(0),
            )
        });
        out
    }

    pub fn on_date(&self, tables: &mut CalendarTables, date: CivilDate) -> Vec<Occurrence> {
        self.expand(tables, date, date)
    }
}

pub fn expand_event(tables: &mut CalendarTables, event: &Event, from: CivilDate, to: CivilDate) -> Vec<Occurrence> {
    let recurrence = if event.birthday { Recurrence::Yearly } else { event.recurrence };
    let dates = match recurrence {
        Recurrence::None => one_shot_civil(tables, event).into_iter().collect(),
        Recurrence::Daily => expand_civil_step(tables, event, from, to, 1),
        Recurrence::Weekly => expand_civil_step(tables, event, from, to, 7),
        Recurrence::Monthly => expand_monthly(tables, event, from, to),
        Recurrence::Yearly => expand_yearly(tables, event, from, to),
    };
    dates
        .into_iter()
        .filter(|date| *date >= from && *date <= to)
        .filter(|date| !event.skip_gregorian.contains(date))
        .map(|civil| {
            let enabled = occurrence_enabled(event, civil);
            let show_reminder = reminder_visible(event, civil);
            Occurrence {
                event_id: event.id,
                profile_id: event.profile_id,
                civil,
                title: event.title.clone(),
                all_day: event.all_day,
                start_time: event.start_time,
                end_time: event.end_time,
                reminder: event.reminder.as_ref().filter(|_| show_reminder).map(|r| r.clock),
                enabled,
                reminder_enabled: enabled && reminder_is_enabled(event, civil),
            }
        })
        .collect()
}

fn one_shot_civil(tables: &mut CalendarTables, event: &Event) -> Option<CivilDate> {
    match event.basis {
        CalendarBasis::Gregorian => event.gregorian,
        CalendarBasis::Lunar => event.lunar.and_then(|ymd| civil_from_lunar_exact(tables, ymd).ok()),
    }
}

fn expand_civil_step(
    tables: &mut CalendarTables,
    event: &Event,
    from: CivilDate,
    to: CivilDate,
    step_days: i64,
) -> Vec<CivilDate> {
    let Some(start) = one_shot_civil(tables, event) else {
        return Vec::new();
    };
    if step_days <= 0 || start > to {
        return Vec::new();
    }
    let mut dates = Vec::new();
    let mut offset = 0i64;
    if start < from {
        let gap = from.julian_day_number() - start.julian_day_number();
        offset = gap.div_euclid(step_days) * step_days;
        if start.julian_day_number() + offset < from.julian_day_number() {
            offset += step_days;
        }
    }
    loop {
        let Ok(date) = start.add_days(offset) else {
            break;
        };
        if date > to {
            break;
        }
        if date >= from {
            dates.push(date);
        }
        offset += step_days;
        if dates.len() > 400 {
            break;
        }
    }
    dates
}

fn expand_monthly(tables: &mut CalendarTables, event: &Event, from: CivilDate, to: CivilDate) -> Vec<CivilDate> {
    match event.basis {
        CalendarBasis::Gregorian => {
            let Some(start) = event.gregorian else {
                return Vec::new();
            };
            gregorian_nth_months(start, from, to)
        }
        CalendarBasis::Lunar => lunar_nth_months(tables, event, from, to),
    }
}

fn expand_yearly(tables: &mut CalendarTables, event: &Event, from: CivilDate, to: CivilDate) -> Vec<CivilDate> {
    match event.basis {
        CalendarBasis::Gregorian => {
            let Some(template) = event.gregorian else {
                return Vec::new();
            };
            let mut dates = Vec::new();
            for year in from.year()..=to.year() {
                if let Some(date) = gregorian_on_day(year, template.month(), template.day()) {
                    dates.push(date);
                }
            }
            dates
        }
        CalendarBasis::Lunar => {
            let Some(template) = event.lunar else {
                return Vec::new();
            };
            let mut dates = Vec::new();
            for year_fn in (from.year() - 1)..=(to.year() + 1) {
                let ymd = LunarYmd {
                    year_fn,
                    month: template.month,
                    leap: event.leap_policy == LeapPolicy::LeapOnly,
                    day: template.day,
                };
                if let Ok(date) = civil_from_lunar_exact(tables, ymd) {
                    dates.push(date);
                }
            }
            dates
        }
    }
}

fn gregorian_on_day(year: i32, month: u8, day: u8) -> Option<CivilDate> {
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    CivilDate::from_ymd_in_range(year, month, day).ok()
}

fn gregorian_nth_months(start: CivilDate, from: CivilDate, to: CivilDate) -> Vec<CivilDate> {
    let mut dates = Vec::new();
    let start_index = start.year() * 12 + i32::from(start.month());
    let from_index = from.year() * 12 + i32::from(from.month());
    let to_index = to.year() * 12 + i32::from(to.month());
    let mut index = start_index;
    if index < from_index {
        index = from_index;
    }
    while index <= to_index {
        let year = if index > 0 { (index - 1) / 12 } else { 0 };
        let month = {
            let m = index.rem_euclid(12);
            if m == 0 { 12 } else { m }
        } as u8;
        if let Some(date) = gregorian_on_day(year, month, start.day())
            && date >= from
            && date <= to
        {
            dates.push(date);
        }
        index += 1;
        if dates.len() > 400 {
            break;
        }
    }
    dates
}

fn lunar_nth_months(tables: &mut CalendarTables, event: &Event, from: CivilDate, to: CivilDate) -> Vec<CivilDate> {
    let Some(template) = event.lunar else {
        return Vec::new();
    };
    for year in (from.year() - 1)..=(to.year() + 1) {
        let _ = tables.month_starts_covering(year);
    }
    let starts = tables.starts_for_lookup();
    let Some(origin) = starts.iter().position(|row| {
        row.lunar_year_fn == template.year_fn && row.lunar_month == template.month && row.leap == template.leap
    }) else {
        return Vec::new();
    };
    let mut dates = Vec::new();
    for start in starts.iter().skip(origin) {
        let ymd = LunarYmd {
            year_fn: start.lunar_year_fn,
            month: start.lunar_month,
            leap: start.leap,
            day: template.day,
        };
        if let Ok(date) = civil_from_lunar_exact(tables, ymd) {
            if date > to {
                break;
            }
            if date >= from {
                dates.push(date);
            }
        }
        if dates.len() > 400 {
            break;
        }
    }
    dates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> CalendarTables {
        CalendarTables::new(2026).unwrap()
    }

    #[test]
    fn create_gregorian_with_reminder() {
        let event = create_gregorian_event(NewGregorianEvent {
            title: "  Quốc khánh  ".into(),
            description: None,
            date: CivilDate::new(2026, 9, 2).unwrap(),
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime::REMINDER_DEFAULT),
            recurrence: Recurrence::Yearly,
            birthday: false,
        })
        .unwrap();
        assert_eq!(event.title, "Quốc khánh");
        assert_eq!(event.basis, CalendarBasis::Gregorian);
        assert_eq!(event.reminder.as_ref().unwrap().clock, ClockTime::REMINDER_DEFAULT);
        let mut tables = tables();
        let occ = expand_event(
            &mut tables,
            &event,
            CivilDate::new(2026, 9, 1).unwrap(),
            CivilDate::new(2026, 9, 30).unwrap(),
        );
        assert_eq!(occ.len(), 1);
        assert_eq!(occ[0].civil, CivilDate::new(2026, 9, 2).unwrap());
    }

    #[test]
    fn create_lunar_trung_thu() {
        let mut tables = tables();
        let event = create_lunar_event(
            &mut tables,
            NewLunarEvent {
                title: "Tết Trung Thu".into(),
                description: None,
                date: LunarYmd {
                    year_fn: 2026,
                    month: 8,
                    leap: false,
                    day: 15,
                },
                all_day: true,
                start_time: None,
                end_time: None,
                timezone: "+07:00".into(),
                reminder: Some(ClockTime { hour: 7, minute: 30 }),
                recurrence: Recurrence::Yearly,
                birthday: false,
            },
        )
        .unwrap();
        assert_eq!(event.leap_policy, LeapPolicy::NonLeapOnly);
        let occ = expand_event(
            &mut tables,
            &event,
            CivilDate::new(2026, 9, 1).unwrap(),
            CivilDate::new(2026, 9, 30).unwrap(),
        );
        assert_eq!(occ[0].civil, CivilDate::new(2026, 9, 25).unwrap());
        assert_eq!(occ[0].reminder.unwrap().hour, 7);
    }

    #[test]
    fn lunar_day_30_skipped_in_short_month() {
        let mut tables = tables();
        let event = create_lunar_event(
            &mut tables,
            NewLunarEvent {
                title: "Ngày 30".into(),
                description: None,
                date: LunarYmd {
                    year_fn: 2026,
                    month: 1,
                    leap: false,
                    day: 30,
                },
                all_day: true,
                start_time: None,
                end_time: None,
                timezone: "+07:00".into(),
                reminder: None,
                recurrence: Recurrence::Monthly,
                birthday: false,
            },
        )
        .unwrap();
        let occ = expand_event(
            &mut tables,
            &event,
            CivilDate::new(2026, 2, 1).unwrap(),
            CivilDate::new(2026, 4, 30).unwrap(),
        );
        let days: Vec<_> = occ.iter().map(|o| o.civil).collect();
        assert!(days.contains(&CivilDate::new(2026, 3, 18).unwrap()));
        assert!(!days.iter().any(|d| d.month() == 4 && d.day() == 16));
    }

    #[test]
    fn timed_end_before_start_rejected() {
        let err = create_gregorian_event(NewGregorianEvent {
            title: "X".into(),
            description: None,
            date: CivilDate::new(2026, 9, 18).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 10, minute: 0 }),
            end_time: Some(ClockTime { hour: 9, minute: 0 }),
            timezone: "+07:00".into(),
            reminder: None,
            recurrence: Recurrence::None,
            birthday: false,
        })
        .unwrap_err();
        assert_eq!(err, EventError::EndBeforeStart);
    }

    #[test]
    fn set_and_clear_reminder() {
        let mut event = create_gregorian_event(NewGregorianEvent {
            title: "Họp".into(),
            description: None,
            date: CivilDate::new(2026, 9, 18).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: Some(ClockTime { hour: 10, minute: 0 }),
            timezone: "+07:00".into(),
            reminder: None,
            recurrence: Recurrence::None,
            birthday: false,
        })
        .unwrap();
        set_reminder(&mut event, ClockTime { hour: 8, minute: 0 });
        assert!(event.reminder.is_some());
        clear_reminder(&mut event);
        assert!(event.reminder.is_none());
        assert_eq!(event.rev, 3);
    }

    #[test]
    fn monthly_31st_skips_short_months() {
        let event = create_gregorian_event(NewGregorianEvent {
            title: "Ngày 31".into(),
            description: None,
            date: CivilDate::new(2026, 1, 31).unwrap(),
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime::REMINDER_DEFAULT),
            recurrence: Recurrence::Monthly,
            birthday: false,
        })
        .unwrap();
        let mut tables = tables();
        let occ = expand_event(
            &mut tables,
            &event,
            CivilDate::new(2026, 1, 1).unwrap(),
            CivilDate::new(2026, 4, 30).unwrap(),
        );
        let days: Vec<_> = occ.iter().map(|o| o.civil).collect();
        assert_eq!(
            days,
            vec![
                CivilDate::new(2026, 1, 31).unwrap(),
                CivilDate::new(2026, 3, 31).unwrap(),
            ]
        );
    }

    #[test]
    fn birthday_feb_29_skips_common_years() {
        let event = create_gregorian_event(NewGregorianEvent {
            title: "Sinh nhật".into(),
            description: None,
            date: CivilDate::new(2024, 2, 29).unwrap(),
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "+07:00".into(),
            reminder: None,
            recurrence: Recurrence::Yearly,
            birthday: true,
        })
        .unwrap();
        assert_eq!(event.recurrence, Recurrence::Yearly);
        let mut tables = tables();
        let occ = expand_event(
            &mut tables,
            &event,
            CivilDate::new(2025, 1, 1).unwrap(),
            CivilDate::new(2029, 12, 31).unwrap(),
        );
        let days: Vec<_> = occ.iter().map(|o| o.civil).collect();
        assert_eq!(days, vec![CivilDate::new(2028, 2, 29).unwrap()]);
    }

    #[test]
    fn daily_and_weekly_step_civil_days() {
        let daily = create_gregorian_event(NewGregorianEvent {
            title: "Thuốc".into(),
            description: None,
            date: CivilDate::new(2026, 9, 18).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 8, minute: 0 }),
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime { hour: 8, minute: 0 }),
            recurrence: Recurrence::Daily,
            birthday: false,
        })
        .unwrap();
        let mut tables = tables();
        let occ = expand_event(
            &mut tables,
            &daily,
            CivilDate::new(2026, 9, 18).unwrap(),
            CivilDate::new(2026, 9, 20).unwrap(),
        );
        assert_eq!(occ.len(), 3);

        let weekly = create_gregorian_event(NewGregorianEvent {
            title: "Họp tuần".into(),
            description: None,
            date: CivilDate::new(2026, 9, 18).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: None,
            timezone: "+07:00".into(),
            reminder: None,
            recurrence: Recurrence::Weekly,
            birthday: false,
        })
        .unwrap();
        let occ = expand_event(
            &mut tables,
            &weekly,
            CivilDate::new(2026, 9, 18).unwrap(),
            CivilDate::new(2026, 10, 9).unwrap(),
        );
        assert_eq!(occ.len(), 4);
        assert_eq!(occ[1].civil, CivilDate::new(2026, 9, 25).unwrap());
    }

    fn daily_event() -> Event {
        create_gregorian_event(NewGregorianEvent {
            title: "Họp".into(),
            description: None,
            date: CivilDate::new(2026, 10, 5).unwrap(),
            all_day: false,
            start_time: Some(ClockTime { hour: 9, minute: 0 }),
            end_time: Some(ClockTime { hour: 10, minute: 0 }),
            timezone: "+07:00".into(),
            reminder: Some(ClockTime::REMINDER_DEFAULT),
            recurrence: Recurrence::Daily,
            birthday: false,
        })
        .unwrap()
    }

    fn on(event: &Event, day: u8) -> Occurrence {
        let mut tables = tables();
        expand_event(
            &mut tables,
            event,
            CivilDate::new(2026, 10, day).unwrap(),
            CivilDate::new(2026, 10, day).unwrap(),
        )
        .into_iter()
        .next()
        .expect("occurrence")
    }

    #[test]
    fn disable_and_delete_one_day_keeps_the_series() {
        let mut event = daily_event();
        let mut tables = tables();
        let day = CivilDate::new(2026, 10, 5).unwrap();
        let effect = apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Event,
            ItemAction::Disable,
            ItemScope::Day,
            day,
            day,
            day,
        )
        .unwrap();
        assert_eq!(effect, ActionEffect::Updated);
        assert!(!on(&event, 5).enabled);
        assert!(!on(&event, 5).reminder_enabled);
        assert!(on(&event, 5).reminder.is_some());
        assert!(on(&event, 6).enabled);
        assert!(on(&event, 6).reminder_enabled);

        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Event,
            ItemAction::Enable,
            ItemScope::Day,
            day,
            day,
            day,
        )
        .unwrap();
        assert!(on(&event, 5).enabled);

        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Event,
            ItemAction::Delete,
            ItemScope::Day,
            day,
            day,
            day,
        )
        .unwrap();
        assert!(
            expand_event(&mut tables, &event, day, day).is_empty(),
            "deleted day is gone"
        );
        assert_eq!(on(&event, 6).civil, CivilDate::new(2026, 10, 6).unwrap());
    }

    #[test]
    fn series_disable_then_one_day_back_on() {
        let mut event = daily_event();
        let mut tables = tables();
        let day = CivilDate::new(2026, 10, 6).unwrap();
        assert_eq!(
            apply_item_action(
                &mut tables,
                &mut event,
                ItemKind::Event,
                ItemAction::Disable,
                ItemScope::Series,
                day,
                day,
                day,
            )
            .unwrap(),
            ActionEffect::Updated
        );
        assert!(!on(&event, 5).enabled);
        assert!(!on(&event, 6).enabled);
        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Event,
            ItemAction::Enable,
            ItemScope::Day,
            day,
            day,
            day,
        )
        .unwrap();
        assert!(!on(&event, 5).enabled);
        assert!(on(&event, 6).enabled);
        assert!(on(&event, 6).reminder_enabled);
    }

    #[test]
    fn period_disable_only_covers_that_span() {
        let mut event = daily_event();
        let mut tables = tables();
        let from = CivilDate::new(2026, 10, 5).unwrap();
        let to = CivilDate::new(2026, 10, 7).unwrap();
        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Reminder,
            ItemAction::Disable,
            ItemScope::Period,
            from,
            from,
            to,
        )
        .unwrap();
        assert!(on(&event, 5).enabled);
        assert!(!on(&event, 5).reminder_enabled);
        assert!(!on(&event, 7).reminder_enabled);
        assert!(on(&event, 8).reminder_enabled);
        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Reminder,
            ItemAction::Delete,
            ItemScope::Day,
            from,
            from,
            from,
        )
        .unwrap();
        assert!(on(&event, 5).reminder.is_none());
        assert!(on(&event, 6).reminder.is_some());
        assert_eq!(
            apply_item_action(
                &mut tables,
                &mut event,
                ItemKind::Event,
                ItemAction::Delete,
                ItemScope::Series,
                from,
                from,
                from,
            )
            .unwrap(),
            ActionEffect::Deleted
        );
    }

    #[test]
    fn delete_one_shot_removes_the_record() {
        let mut event = create_gregorian_event(NewGregorianEvent {
            title: "Một lần".into(),
            description: None,
            date: CivilDate::new(2026, 10, 5).unwrap(),
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime { hour: 8, minute: 0 }),
            recurrence: Recurrence::None,
            birthday: false,
        })
        .unwrap();
        let mut tables = tables();
        let day = CivilDate::new(2026, 10, 5).unwrap();
        assert_eq!(
            apply_item_action(
                &mut tables,
                &mut event,
                ItemKind::Event,
                ItemAction::Delete,
                ItemScope::Day,
                day,
                day,
                day,
            )
            .unwrap(),
            ActionEffect::Deleted
        );
        let mut event = create_gregorian_event(NewGregorianEvent {
            title: "Một lần".into(),
            description: None,
            date: day,
            all_day: true,
            start_time: None,
            end_time: None,
            timezone: "+07:00".into(),
            reminder: Some(ClockTime { hour: 8, minute: 0 }),
            recurrence: Recurrence::None,
            birthday: false,
        })
        .unwrap();
        apply_item_action(
            &mut tables,
            &mut event,
            ItemKind::Reminder,
            ItemAction::Delete,
            ItemScope::Series,
            day,
            day,
            day,
        )
        .unwrap();
        assert!(event.reminder.is_none());
    }
}
