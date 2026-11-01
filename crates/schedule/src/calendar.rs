//! Working days ↔ dates: index 0 is the project start (moved to the next working day), 1 the
//! next working day, -1 the working day before it.

use plancraft_project::{Calendar, Date};

/// Furthest the calendar walks from the start (about 150 years), so a bad date can't hang it.
const MAX_STEPS: usize = 60_000;

/// A project's working days, extended lazily in both directions.
#[derive(Debug, Clone)]
pub struct WorkCalendar {
    cal: Calendar,
    all_days: bool,
    /// The project start, as given (fallback when the calendar runs out).
    origin: Date,
    /// `forward[i]` is working day `i` (from 0).
    forward: Vec<Date>,
    /// `back[i]` is working day `-(i + 1)`.
    back: Vec<Date>,
}

impl WorkCalendar {
    pub fn new(cal: &Calendar, start: Date) -> WorkCalendar {
        let all_days = !cal.working_days.iter().any(|d| *d);
        let mut c = WorkCalendar { cal: cal.clone(), all_days, origin: start, forward: Vec::new(), back: Vec::new() };
        let first = c.step_until_working(start, 1).unwrap_or(start);
        c.forward.push(first);
        c
    }

    /// True when the calendar marks no day as working (then every day counts).
    pub fn no_working_days(&self) -> bool {
        self.all_days
    }

    fn working(&self, d: Date) -> bool {
        self.all_days || self.cal.is_working_day(d)
    }

    /// The first working day from `d` (inclusive) going in `dir` (+1 or -1).
    fn step_until_working(&self, d: Date, dir: i64) -> Option<Date> {
        let mut cur = d;
        for _ in 0..MAX_STEPS {
            if self.working(cur) {
                return Some(cur);
            }
            cur = cur.add_days(dir)?;
        }
        None
    }

    /// The date of working day `i` (clamped to the calendar's range).
    pub fn date(&mut self, i: i64) -> Date {
        if i >= 0 {
            let i = (i as usize).min(MAX_STEPS);
            while self.forward.len() <= i {
                let Some(last) = self.forward.last().copied() else { break };
                match last.add_days(1).and_then(|n| self.step_until_working(n, 1)) {
                    Some(n) => self.forward.push(n),
                    None => break,
                }
            }
            self.forward.get(i).or(self.forward.last()).copied().unwrap_or(self.origin)
        } else {
            let i = ((-i - 1) as usize).min(MAX_STEPS);
            while self.back.len() <= i {
                let Some(last) = self.back.last().or(self.forward.first()).copied() else { break };
                match last.add_days(-1).and_then(|n| self.step_until_working(n, -1)) {
                    Some(n) => self.back.push(n),
                    None => break,
                }
            }
            self.back.get(i).or(self.back.last()).or(self.forward.first()).copied().unwrap_or(self.origin)
        }
    }

    /// The index of the first working day on or after `d`.
    pub fn index_on_or_after(&mut self, d: Date) -> i64 {
        self.index_near(d, true)
    }

    /// The index of the last working day on or before `d`.
    pub fn index_on_or_before(&mut self, d: Date) -> i64 {
        self.index_near(d, false)
    }

    fn index_near(&mut self, d: Date, after: bool) -> i64 {
        let start = self.date(0);
        if d >= start {
            let mut i = 0i64;
            for _ in 0..MAX_STEPS {
                let cur = self.date(i);
                if cur == d {
                    return i;
                }
                if cur > d {
                    return if after { i } else { i - 1 };
                }
                let next = self.date(i + 1);
                if next == cur {
                    return i; // calendar ran out
                }
                i += 1;
            }
            i
        } else {
            let mut i = -1i64;
            for _ in 0..MAX_STEPS {
                let cur = self.date(i);
                if cur == d {
                    return i;
                }
                if cur < d {
                    return if after { i + 1 } else { i };
                }
                let prev = self.date(i - 1);
                if prev == cur {
                    return i;
                }
                i -= 1;
            }
            i
        }
    }
}
