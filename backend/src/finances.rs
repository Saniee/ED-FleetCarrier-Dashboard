//! A carrier's balance over time, in upkeep weeks. Pure, so the arithmetic is
//! tested without a database; `db::finances` gathers the snapshots.
//!
//! The journal only reports the balance when the owner opens carrier management
//! (`CarrierStats`), changes tax or reserve (`CarrierFinance`) or moves credits
//! (`CarrierBankTransfer`), so the history is a series of irregular snapshots.
//! A week's figure is the change between snapshots, less the owner's own
//! transfers: what the carrier earned from its markets and services, minus
//! upkeep, jump fees and buy orders.

use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc, Weekday};
use serde::Serialize;

/// Upkeep is taken in the weekly server maintenance, Thursday 07:00 UTC.
const UPKEEP_DAY: Weekday = Weekday::Thu;
const UPKEEP_HOUR: u32 = 7;

/// One reported balance. `transfer` is the owner's deposit (positive) or
/// withdrawal (negative) that produced it; 0 for anything but a bank transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub at: DateTime<Utc>,
    pub balance: i64,
    pub transfer: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Week {
    /// The upkeep tick that opens the week.
    pub start: DateTime<Utc>,
    /// Balance change not explained by the owner's transfers. `None` without a
    /// snapshot in the week to measure against.
    pub earned: Option<i64>,
    /// Net deposits less withdrawals.
    pub transferred: i64,
    /// The last balance reported in the week.
    pub closing: Option<i64>,
    /// `earned` reaches back past `start`, because the snapshot before this
    /// week's first one is from an earlier week. It then includes those weeks'
    /// changes too.
    pub spans_weeks: bool,
}

/// The upkeep tick at or before `t`.
pub fn week_start(t: DateTime<Utc>) -> DateTime<Utc> {
    let back = (t.weekday().num_days_from_monday() + 7 - UPKEEP_DAY.num_days_from_monday()) % 7;
    let tick = (t.date_naive() - Duration::days(i64::from(back)))
        .and_time(NaiveTime::from_hms_opt(UPKEEP_HOUR, 0, 0).expect("valid time"))
        .and_utc();
    // Earlier on upkeep day than the tick: that week started a week ago.
    if tick > t { tick - Duration::weeks(1) } else { tick }
}

/// The next time upkeep is taken after `t`.
pub fn next_upkeep(t: DateTime<Utc>) -> DateTime<Utc> {
    week_start(t) + Duration::weeks(1)
}

/// The last `weeks` upkeep weeks up to `now`, oldest first. `snapshots` may
/// start with one from before the window, to measure the first week against.
pub fn weekly(mut snapshots: Vec<Snapshot>, now: DateTime<Utc>, weeks: usize) -> Vec<Week> {
    snapshots.sort_by_key(|s| s.at);
    // The plugin resends events (replays, several commanders at one carrier) and
    // they are logged as they come. A repeated transfer would otherwise be
    // subtracted twice.
    snapshots.dedup();

    let last = week_start(now);
    let mut out: Vec<Week> = (0..weeks)
        .rev()
        .map(|back| Week {
            start: last - Duration::weeks(back as i64),
            earned: None,
            transferred: 0,
            closing: None,
            spans_weeks: false,
        })
        .collect();

    for pair in snapshots.windows(2) {
        let (prev, cur) = (&pair[0], &pair[1]);
        let start = week_start(cur.at);
        let Some(week) = out.iter_mut().find(|w| w.start == start) else { continue };
        week.earned = Some(week.earned.unwrap_or(0) + cur.balance - prev.balance - cur.transfer);
        week.transferred += cur.transfer;
        week.closing = Some(cur.balance);
        week.spans_weeks |= prev.at < start;
    }
    out
}

/// Whole weeks of upkeep the balance covers; `None` without an upkeep figure.
pub fn runway_weeks(balance: i64, upkeep: Option<i64>) -> Option<i64> {
    let upkeep = upkeep.filter(|u| *u > 0)?;
    Some(balance.max(0) / upkeep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().to_utc()
    }

    fn snap(t: &str, balance: i64, transfer: i64) -> Snapshot {
        Snapshot { at: at(t), balance, transfer }
    }

    #[test]
    fn weeks_start_at_the_thursday_tick() {
        // 2026-10-29 is a Thursday.
        assert_eq!(week_start(at("2026-10-29T07:00:00Z")), at("2026-10-29T07:00:00Z"));
        assert_eq!(week_start(at("2026-10-29T06:59:59Z")), at("2026-10-22T07:00:00Z"));
        assert_eq!(week_start(at("2026-11-03T12:00:00Z")), at("2026-10-29T07:00:00Z"));
        assert_eq!(next_upkeep(at("2026-10-27T12:00:00Z")), at("2026-10-29T07:00:00Z"));
    }

    #[test]
    fn transfers_are_not_earnings() {
        let now = at("2026-11-04T12:00:00Z");
        let weeks = weekly(
            vec![
                snap("2026-10-29T10:00:00Z", 1_000, 0),
                // Deposit of 500: balance rises by exactly that.
                snap("2026-10-30T10:00:00Z", 1_500, 500),
                // Sales bring in 300.
                snap("2026-11-01T10:00:00Z", 1_800, 0),
            ],
            now,
            1,
        );
        assert_eq!(weeks.len(), 1);
        assert_eq!(weeks[0].earned, Some(300));
        assert_eq!(weeks[0].transferred, 500);
        assert_eq!(weeks[0].closing, Some(1_800));
        assert!(!weeks[0].spans_weeks);
    }

    #[test]
    fn repeated_events_count_once() {
        let now = at("2026-11-04T12:00:00Z");
        let transfer = snap("2026-10-30T10:00:00Z", 1_500, 500);
        let weeks = weekly(vec![snap("2026-10-29T10:00:00Z", 1_000, 0), transfer.clone(), transfer], now, 1);
        assert_eq!(weeks[0].earned, Some(0));
        assert_eq!(weeks[0].transferred, 500);
    }

    #[test]
    fn upkeep_shows_across_the_tick_and_quiet_weeks_stay_empty() {
        let now = at("2026-11-12T12:00:00Z");
        let weeks = weekly(
            vec![
                snap("2026-10-28T10:00:00Z", 100_000_000, 0),
                // Two ticks later, 2 × 10M upkeep has gone.
                snap("2026-11-06T10:00:00Z", 80_000_000, 0),
            ],
            now,
            3,
        );
        let starts: Vec<_> = weeks.iter().map(|w| w.start).collect();
        assert_eq!(
            starts,
            [at("2026-10-29T07:00:00Z"), at("2026-11-05T07:00:00Z"), at("2026-11-12T07:00:00Z")]
        );
        assert_eq!(weeks[0].earned, None);
        assert_eq!(weeks[1].earned, Some(-20_000_000));
        assert!(weeks[1].spans_weeks);
        assert_eq!(weeks[2].earned, None);
    }

    #[test]
    fn runway_needs_an_upkeep_figure() {
        assert_eq!(runway_weeks(95, Some(10)), Some(9));
        assert_eq!(runway_weeks(-5, Some(10)), Some(0));
        assert_eq!(runway_weeks(95, None), None);
        assert_eq!(runway_weeks(95, Some(0)), None);
    }
}
