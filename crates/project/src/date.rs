//! Calendar dates (no time of day), stored and shown as ISO `YYYY-MM-DD`.

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A calendar date, as in a task's start or a project deadline.
/// `Default` is 1970-01-01, only as a last-resort placeholder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(NaiveDate);

/// A date that isn't `YYYY-MM-DD` or doesn't exist (`2026-02-30`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a date (expected YYYY-MM-DD)")]
pub struct DateError(pub String);

impl Date {
    /// The date for a year, month (1–12) and day (1–31), if it exists.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Date> {
        NaiveDate::from_ymd_opt(year, month, day).map(Date)
    }

    pub fn year(self) -> i32 {
        self.0.year()
    }

    pub fn month(self) -> u32 {
        self.0.month()
    }

    pub fn day(self) -> u32 {
        self.0.day()
    }

    /// Day of the week, Monday = 0 … Sunday = 6.
    pub fn weekday_from_monday(self) -> usize {
        self.0.weekday().num_days_from_monday() as usize
    }

    /// The date `days` days later (earlier when negative); `None` past chrono's range.
    pub fn add_days(self, days: i64) -> Option<Date> {
        self.0.checked_add_signed(chrono::Duration::try_days(days)?).map(Date)
    }

    /// Days from `self` to `other` (negative when `other` is earlier).
    pub fn days_until(self, other: Date) -> i64 {
        other.0.signed_duration_since(self.0).num_days()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year(), self.month(), self.day())
    }
}

impl FromStr for Date {
    type Err = DateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        // Only the strict ISO form: no times, no other orders, so every device reads it the same way.
        let mut parts = t.splitn(3, '-');
        let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(DateError(s.to_string()));
        };
        let ok = y.len() == 4 && m.len() == 2 && d.len() == 2 && [y, m, d].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
        if !ok {
            return Err(DateError(s.to_string()));
        }
        let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>()) else {
            return Err(DateError(s.to_string()));
        };
        Date::from_ymd(y, m, d).ok_or_else(|| DateError(s.to_string()))
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints_iso() {
        let d: Date = "2026-11-01".parse().unwrap();
        assert_eq!(d.to_string(), "2026-11-01");
        assert_eq!(d.weekday_from_monday(), 6); // a Sunday
        assert_eq!(d.add_days(30).unwrap().to_string(), "2026-12-01");
        assert_eq!(d.days_until("2026-12-25".parse().unwrap()), 54);
    }

    #[test]
    fn rejects_other_forms() {
        for bad in ["", "2026-2-1", "11/01/2026", "2026-02-30", "2026-13-01", "20260101", "2026-01-01T00:00", "x026-01-01"] {
            assert!(bad.parse::<Date>().is_err(), "{bad}");
        }
    }

    #[test]
    fn huge_offsets_dont_panic() {
        let d: Date = "2026-11-01".parse().unwrap();
        assert!(d.add_days(i64::MAX).is_none());
        assert!(d.add_days(i64::MIN).is_none());
    }
}
