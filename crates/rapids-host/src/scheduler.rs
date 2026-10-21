//! When waiting work may start: retry backoff and start windows.
//!
//! Everything here is a function of rows and instants. The manager owns the
//! clock and the database; this module only answers questions, which is what
//! lets the rules be tested exactly instead of by sleeping.

use crate::store::schedules::{ScheduleKind, ScheduleRow};

/// Milliseconds in one minute.
pub const MINUTE_MS: i64 = 60_000;

/// Milliseconds in one day.
///
/// Windows repeat on fixed 24-hour days. A zone that shifts its clocks moves a
/// daily window by an hour for one day, exactly as a wall-clock alarm does —
/// and, unlike a stored local timestamp, a fixed period never ends up in a
/// state that cannot be reached again.
pub const DAY_MS: i64 = 86_400_000;

/// How long to wait before the next automatic attempt.
///
/// `attempts` counts the attempts that have already failed, so the first
/// failure waits one base delay, the second two, then four, and so on, never
/// above `cap_ms`. A cap below the base is read as the base, so a mis-typed
/// setting cannot make a retry arrive sooner than the first attempt would.
pub fn retry_delay_ms(base_ms: i64, cap_ms: i64, attempts: i64) -> i64 {
    let base = base_ms.max(0);
    if base == 0 {
        return 0;
    }
    let cap = cap_ms.max(base);
    let mut delay = base;
    let mut spent = 1;
    while spent < attempts.max(1) {
        if delay >= cap {
            break;
        }
        delay = delay.saturating_mul(2).min(cap);
        spent += 1;
    }
    delay
}

/// What a window says about starting work at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The window has not opened yet.
    Later,
    /// Work may start now.
    Open,
    /// The window opened and closed. A one-off window never opens again; a
    /// repeating one reports `Open` again at its next opening.
    Shut,
}

/// The wall clock schedules are read against.
///
/// This crate never reads the machine's clock settings: the offset is supplied
/// by the caller that owns the platform, so scheduling stays testable and the
/// host stays free of platform code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    offset_minutes: i32,
}

impl Clock {
    /// The clock with no offset from UTC.
    pub const fn utc() -> Self {
        Self { offset_minutes: 0 }
    }

    /// A clock `offset_minutes` ahead of UTC; negative values are behind it.
    pub const fn at_offset_minutes(offset_minutes: i32) -> Self {
        Self { offset_minutes }
    }

    pub const fn offset_minutes(self) -> i32 {
        self.offset_minutes
    }

    /// The instant `at_ms` reads as on this clock.
    pub fn local_ms(self, at_ms: i64) -> i64 {
        at_ms.saturating_add(i64::from(self.offset_minutes) * MINUTE_MS)
    }
}

/// Whether `schedule` allows work to start at `at_ms`.
///
/// An unknown kind reports `Later`: refusing is safer than guessing and
/// starting work at the wrong hour. Whether a schedule is enabled is the
/// caller's question — this only describes the window itself.
pub fn window_state(schedule: &ScheduleRow, at_ms: i64, clock: Clock) -> Window {
    let window_ms = schedule.window_minutes.max(0).saturating_mul(MINUTE_MS);
    if at_ms < schedule.start_at {
        return Window::Later;
    }

    let Some(kind) = schedule.kind_enum() else {
        return Window::Later;
    };

    if kind == ScheduleKind::Once {
        return if window_ms == 0 || at_ms < schedule.start_at.saturating_add(window_ms) {
            Window::Open
        } else {
            Window::Shut
        };
    }

    // A repeating window with no length never closes once it has opened.
    if window_ms == 0 {
        return Window::Open;
    }

    let local = clock.local_ms(at_ms);
    let anchor_tod = clock.local_ms(schedule.start_at).rem_euclid(DAY_MS);
    let day_start = local - local.rem_euclid(DAY_MS);

    // A daily window's most recent opening is today's or yesterday's; a weekly
    // one's is somewhere in the last seven days.
    let days_back = if kind == ScheduleKind::Daily { 1 } else { 7 };
    for back in 0..=days_back {
        let opening = day_start - back * DAY_MS + anchor_tod;
        if opening > local {
            continue;
        }
        if kind == ScheduleKind::Weekly && schedule.weekdays & weekday_bit(opening) == 0 {
            continue;
        }
        // The most recent opening decides. An earlier one is strictly older,
        // so it cannot be the window that is still open.
        return if local < opening.saturating_add(window_ms) {
            Window::Open
        } else {
            Window::Later
        };
    }
    Window::Later
}

/// A one-bit mask for the weekday `at_ms` falls on, with bit 0 as Monday.
///
/// 1970-01-01 was a Thursday, which is bit 3, so a day number shifted by three
/// lands on the mask position the schema documents.
fn weekday_bit(at_ms: i64) -> i64 {
    1 << (at_ms.div_euclid(DAY_MS) + 3).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monday 1970-01-05, 00:00 UTC: a Monday so weekday masks read clearly.
    const MONDAY: i64 = 4 * DAY_MS;
    const HOUR_MS: i64 = 60 * MINUTE_MS;

    fn schedule(
        kind: ScheduleKind,
        start_at: i64,
        window_minutes: i64,
        weekdays: i64,
    ) -> ScheduleRow {
        ScheduleRow {
            id: 1,
            label: "window".to_string(),
            kind: kind.as_str().to_string(),
            start_at,
            window_minutes,
            weekdays,
            enabled: 1,
            created_at: start_at,
        }
    }

    #[test]
    fn the_first_failure_waits_one_base_delay() {
        assert_eq!(retry_delay_ms(5_000, 300_000, 1), 5_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, 2), 10_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, 3), 20_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, 4), 40_000);
    }

    #[test]
    fn the_delay_doubles_until_the_cap_and_stays_there() {
        assert_eq!(retry_delay_ms(5_000, 300_000, 6), 160_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, 7), 300_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, 40), 300_000);
        assert_eq!(retry_delay_ms(5_000, 300_000, i64::MAX), 300_000);
    }

    #[test]
    fn a_misconfigured_cap_cannot_shorten_the_first_wait() {
        assert_eq!(retry_delay_ms(5_000, 1_000, 1), 5_000);
        assert_eq!(retry_delay_ms(5_000, 1_000, 2), 5_000);
        assert_eq!(retry_delay_ms(5_000, 0, 3), 5_000);
    }

    #[test]
    fn a_non_positive_attempt_count_is_read_as_the_first() {
        assert_eq!(retry_delay_ms(2_000, 60_000, 0), 2_000);
        assert_eq!(retry_delay_ms(2_000, 60_000, -3), 2_000);
        assert_eq!(retry_delay_ms(0, 60_000, 5), 0);
    }

    #[test]
    fn a_one_off_window_opens_once_and_then_stays_shut() {
        let start = MONDAY + 2 * HOUR_MS;
        let window = schedule(ScheduleKind::Once, start, 60, 0);
        let clock = Clock::utc();

        assert_eq!(window_state(&window, start - 1, clock), Window::Later);
        assert_eq!(window_state(&window, start, clock), Window::Open);
        assert_eq!(
            window_state(&window, start + 59 * MINUTE_MS, clock),
            Window::Open
        );
        assert_eq!(
            window_state(&window, start + 61 * MINUTE_MS, clock),
            Window::Shut
        );
        assert_eq!(
            window_state(&window, start + 40 * DAY_MS, clock),
            Window::Shut
        );
    }

    #[test]
    fn an_open_ended_one_off_window_never_closes() {
        let start = MONDAY + 2 * HOUR_MS;
        let window = schedule(ScheduleKind::Once, start, 0, 0);
        assert_eq!(
            window_state(&window, start + 400 * DAY_MS, Clock::utc()),
            Window::Open
        );
    }

    #[test]
    fn a_daily_window_opens_at_the_same_time_every_day() {
        let start = MONDAY + 2 * HOUR_MS;
        let window = schedule(ScheduleKind::Daily, start, 60, 0);
        let clock = Clock::utc();

        assert_eq!(window_state(&window, start - 1, clock), Window::Later);
        assert_eq!(
            window_state(&window, start + 30 * MINUTE_MS, clock),
            Window::Open
        );
        assert_eq!(
            window_state(&window, start + 90 * MINUTE_MS, clock),
            Window::Later
        );
        let tomorrow = start + DAY_MS;
        assert_eq!(window_state(&window, tomorrow - 1, clock), Window::Later);
        assert_eq!(
            window_state(&window, tomorrow + 5 * MINUTE_MS, clock),
            Window::Open
        );
    }

    #[test]
    fn a_daily_window_that_crosses_midnight_keeps_its_earlier_opening() {
        // Opens 23:30 Monday for two hours, so it is still open on Tuesday.
        let start = MONDAY + 23 * HOUR_MS + 30 * MINUTE_MS;
        let window = schedule(ScheduleKind::Daily, start, 120, 0);
        let clock = Clock::utc();

        assert_eq!(
            window_state(&window, MONDAY + DAY_MS + 30 * MINUTE_MS, clock),
            Window::Open,
            "Tuesday 00:30 is inside Monday's window"
        );
        assert_eq!(
            window_state(&window, MONDAY + DAY_MS + 90 * MINUTE_MS, clock),
            Window::Later,
            "two hours after Monday 23:30 the window has closed"
        );
    }

    #[test]
    fn a_daily_window_is_read_on_the_supplied_clock() {
        let start = MONDAY + 2 * HOUR_MS;
        let window = schedule(ScheduleKind::Daily, start, 60, 0);
        // Two hours behind UTC: the window opens at 04:00 UTC.
        let behind = Clock::at_offset_minutes(-120);

        assert_eq!(
            window_state(&window, start + 30 * MINUTE_MS, behind),
            Window::Open,
            "02:30 UTC is 00:30 on a clock two hours behind, inside the window"
        );
        assert_eq!(
            window_state(&window, start + 2 * HOUR_MS + 5 * MINUTE_MS, behind),
            Window::Later,
            "02:05 local is past the one-hour window that opened at 00:00"
        );
        assert_eq!(behind.offset_minutes(), -120);
        assert_eq!(behind.local_ms(start), start - 2 * HOUR_MS);
    }

    #[test]
    fn a_weekly_window_only_opens_on_its_days() {
        // Mondays only: bit 0.
        let start = MONDAY + 2 * HOUR_MS;
        let window = schedule(ScheduleKind::Weekly, start, 60, 1);
        let clock = Clock::utc();

        assert_eq!(
            window_state(&window, start + 10 * MINUTE_MS, clock),
            Window::Open
        );
        assert_eq!(
            window_state(&window, start + DAY_MS + 10 * MINUTE_MS, clock),
            Window::Later,
            "Tuesday is not selected"
        );
        assert_eq!(
            window_state(&window, start + 7 * DAY_MS + 10 * MINUTE_MS, clock),
            Window::Open,
            "the following Monday is"
        );
    }

    #[test]
    fn a_weekly_window_with_no_days_never_opens() {
        let window = schedule(ScheduleKind::Weekly, MONDAY, 60, 0);
        assert_eq!(
            window_state(&window, MONDAY + 7 * DAY_MS, Clock::utc()),
            Window::Later
        );
    }

    #[test]
    fn weekday_bits_start_at_monday() {
        assert_eq!(weekday_bit(MONDAY), 1);
        assert_eq!(weekday_bit(MONDAY + DAY_MS), 2);
        assert_eq!(weekday_bit(MONDAY + 6 * DAY_MS), 64);
        // The epoch itself was a Thursday.
        assert_eq!(weekday_bit(0), 8);
    }

    #[test]
    fn an_unknown_kind_does_not_open() {
        let mut window = schedule(ScheduleKind::Once, MONDAY, 0, 0);
        window.kind = "fortnightly".to_string();
        assert_eq!(
            window_state(&window, MONDAY + HOUR_MS, Clock::utc()),
            Window::Later
        );
    }
}
