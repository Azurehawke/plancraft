//! PlanCraft's scheduler (critical path method), working in whole working days.
//!
//! [`schedule`] takes a [`Project`] and returns each task's early and late dates, total slack and
//! whether it is critical, plus the project's finish. It never changes the project: the dates are
//! computed on every device from the merged project, so they always agree and never clash in sync.
//!
//! Rules, MS Project's in day granularity:
//! - Durations are working days: working minutes ÷ the calendar's hours per day, rounded up. Zero
//!   (or a milestone) is a point in time at the start of a day.
//! - Links: finish-to-start, start-to-start, finish-to-finish, start-to-finish, with lag in working
//!   days (negative is lead). A link to or from a summary task applies to every task under it.
//! - Constraints: start/finish no earlier than (forward pass), no later than (backward pass), must
//!   start/finish on (both). A task with no links starts at the project start.
//! - Deadlines limit late dates (a missed deadline shows as negative slack).
//! - As late as possible tasks are placed at their late dates.
//! - Summary tasks span their subtasks; cancelled tasks are skipped.
//! - Links that form a loop are left out (and reported) so the rest still schedules.
//!
//! Internally a task occupies working days `[start, end)`; the finish shown is the last working
//! day it occupies (or its start, for zero-length tasks).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod calendar;

use std::collections::{HashMap, VecDeque};

use plancraft_project::{Constraint, Date, DependencyKind, Project, TaskId};
use serde::Serialize;

pub use calendar::WorkCalendar;

/// One task's computed dates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskDates {
    pub start: Date,
    pub finish: Date,
    pub late_start: Date,
    pub late_finish: Date,
    /// Working days the task can slip without delaying the project (negative: already late
    /// against a deadline or constraint).
    pub total_slack: i64,
    /// On the critical path (no slack).
    pub critical: bool,
    /// Working days.
    pub duration_days: i64,
}

/// The result of scheduling a project.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Schedule {
    pub start: Date,
    pub finish: Date,
    /// Keyed by task id; cancelled tasks are absent.
    pub tasks: HashMap<TaskId, TaskDates>,
    /// Things the person should know (a link loop left out, a deadline that can't be met…).
    pub warnings: Vec<String>,
}

impl Schedule {
    pub fn get(&self, id: &TaskId) -> Option<&TaskDates> {
        self.tasks.get(id)
    }
}

/// Internal per-leaf state, in working-day indexes from the project start.
#[derive(Debug, Clone, Copy)]
struct Work {
    dur: i64,
    es: i64,
    ls: i64,
}

/// Bound on working-day indexes (about 150 years), so hostile lags or constraints can't run away.
const LIMIT: i64 = 40_000;

/// Schedules `p`. `today` is the start when the project has no start date (the app passes the
/// local date; the scheduler never reads the clock, so it runs the same everywhere).
pub fn schedule(p: &Project, today: Date) -> Schedule {
    let start = p.info.start.unwrap_or(today);
    let mut cal = WorkCalendar::new(&p.calendar, start);
    let mut warnings = Vec::new();
    if cal.no_working_days() {
        warnings.push("the calendar has no working days; every day is treated as working".to_string());
    }
    let minutes_per_day = (p.calendar.hours_per_day * 60.0).max(1.0);

    // Leaves (tasks that do the work) and, for each task, the leaves under it.
    let active: Vec<&plancraft_project::Task> = active_tasks(p);
    let index: HashMap<&TaskId, usize> = active.iter().enumerate().map(|(i, t)| (&t.id, i)).collect();
    let leaves_under = leaves_under(&active, &index);
    let is_leaf: Vec<bool> = active.iter().enumerate().map(|(i, t)| leaves_under.get(&t.id).is_some_and(|v| v.as_slice() == [i])).collect();

    let mut work: Vec<Work> = active
        .iter()
        .map(|t| {
            let dur = if t.milestone { 0 } else { (f64::from(t.duration_minutes) / minutes_per_day).ceil() as i64 };
            Work { dur: dur.clamp(0, LIMIT), es: 0, ls: 0 }
        })
        .collect();

    // Links between leaves (summary links expanded), lag in working days.
    let mut edges: Vec<(usize, usize, DependencyKind, i64)> = Vec::new();
    for d in &p.dependencies {
        let (Some(a), Some(b)) = (leaves_under.get(&d.predecessor), leaves_under.get(&d.successor)) else { continue };
        let lag = ((f64::from(d.lag_minutes) / minutes_per_day).round() as i64).clamp(-LIMIT, LIMIT);
        for &x in a {
            for &y in b {
                if x != y {
                    edges.push((x, y, d.kind, lag));
                }
            }
        }
    }
    let (order, looped) = topo_order(active.len(), &edges);
    if looped > 0 {
        warnings.push(format!("{looped} task(s) are in a loop of links; the links that close the loop are ignored"));
    }
    let pos: Vec<usize> = {
        let mut v = vec![0; active.len()];
        for (i, &n) in order.iter().enumerate() {
            if let Some(slot) = v.get_mut(n) {
                *slot = i;
            }
        }
        v
    };
    // Only links that go forward in the order (the rest close loops).
    let forward: Vec<&(usize, usize, DependencyKind, i64)> = edges.iter().filter(|e| pos.get(e.0) < pos.get(e.1)).collect();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); active.len()];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); active.len()];
    for (i, e) in forward.iter().enumerate() {
        if let Some(v) = preds.get_mut(e.1) {
            v.push(i);
        }
        if let Some(v) = succs.get_mut(e.0) {
            v.push(i);
        }
    }

    // Forward pass.
    for &n in &order {
        if !is_leaf.get(n).copied().unwrap_or(false) {
            continue;
        }
        let Some(t) = active.get(n) else { continue };
        let dur = work.get(n).map(|w| w.dur).unwrap_or(0);
        let mut es: i64 = 0;
        for &ei in preds.get(n).map(Vec::as_slice).unwrap_or_default() {
            let Some(&&(a, _, kind, lag)) = forward.get(ei) else { continue };
            let Some(pw) = work.get(a) else { continue };
            let bound = match kind {
                DependencyKind::Fs => pw.es + pw.dur + lag,
                DependencyKind::Ss => pw.es + lag,
                DependencyKind::Ff => pw.es + pw.dur + lag - dur,
                DependencyKind::Sf => pw.es + lag - dur,
            };
            es = es.max(bound);
        }
        if let Some(c) = t.constraint_date.map(|d| cal.index_on_or_after(d)) {
            match t.constraint {
                Constraint::Snet => es = es.max(c),
                Constraint::Fnet => es = es.max(start_for_finish(c, dur)),
                Constraint::Mso => es = c,
                Constraint::Mfo => es = start_for_finish(c, dur),
                _ => {}
            }
        }
        if let Some(w) = work.get_mut(n) {
            w.es = es.clamp(-LIMIT, LIMIT);
        }
    }
    let project_end = order
        .iter()
        .filter(|&&n| is_leaf.get(n).copied().unwrap_or(false))
        .filter_map(|&n| work.get(n))
        .map(|w| w.es + w.dur)
        .max()
        .unwrap_or(0)
        .max(0);

    // Backward pass (late ends exclusive).
    for &n in order.iter().rev() {
        if !is_leaf.get(n).copied().unwrap_or(false) {
            continue;
        }
        let Some(t) = active.get(n) else { continue };
        let dur = work.get(n).map(|w| w.dur).unwrap_or(0);
        let mut lf = project_end;
        for &ei in succs.get(n).map(Vec::as_slice).unwrap_or_default() {
            let Some(&&(_, b, kind, lag)) = forward.get(ei) else { continue };
            let Some(sw) = work.get(b) else { continue };
            let bound = match kind {
                DependencyKind::Fs => sw.ls - lag,
                DependencyKind::Ss => sw.ls - lag + dur,
                DependencyKind::Ff => sw.ls + sw.dur - lag,
                DependencyKind::Sf => sw.ls + sw.dur - lag + dur,
            };
            lf = lf.min(bound);
        }
        if let Some(d) = t.deadline {
            lf = lf.min(end_for_finish(cal.index_on_or_before(d), dur));
        }
        if let Some(c) = t.constraint_date.map(|d| cal.index_on_or_after(d)) {
            match t.constraint {
                Constraint::Snlt => lf = lf.min(c + dur),
                Constraint::Fnlt => lf = lf.min(end_for_finish(c, dur)),
                Constraint::Mso => lf = c + dur,
                Constraint::Mfo => lf = end_for_finish(c, dur),
                _ => {}
            }
        }
        if let Some(w) = work.get_mut(n) {
            w.ls = (lf - dur).clamp(-LIMIT, LIMIT);
            if t.constraint == Constraint::Alap && w.ls > w.es {
                w.es = w.ls;
            }
        }
    }

    let tasks = collect_dates(&active, &leaves_under, &is_leaf, &work, &mut cal, &mut warnings);
    let finish = if project_end > 0 { cal.date(project_end - 1) } else { cal.date(0) };
    Schedule { start: cal.date(0), finish, tasks, warnings }
}

/// Dates for leaves, then summaries from their leaves.
fn collect_dates(
    active: &[&plancraft_project::Task],
    leaves_under: &HashMap<TaskId, Vec<usize>>,
    is_leaf: &[bool],
    work: &[Work],
    cal: &mut WorkCalendar,
    warnings: &mut Vec<String>,
) -> HashMap<TaskId, TaskDates> {
    let mut tasks = HashMap::new();
    for (n, t) in active.iter().enumerate() {
        let Some(under) = leaves_under.get(&t.id) else { continue };
        let leaves: Vec<usize> = under.iter().copied().filter(|&i| is_leaf.get(i).copied().unwrap_or(false)).collect();
        let (es, ef, ls, lf, dur) = if is_leaf.get(n).copied().unwrap_or(false) {
            let w = work.get(n).copied().unwrap_or(Work { dur: 0, es: 0, ls: 0 });
            (w.es, w.es + w.dur, w.ls, w.ls + w.dur, w.dur)
        } else {
            let ws: Vec<Work> = leaves.iter().filter_map(|&i| work.get(i).copied()).collect();
            let es = ws.iter().map(|w| w.es).min().unwrap_or(0);
            let ef = ws.iter().map(|w| w.es + w.dur).max().unwrap_or(es);
            let ls = ws.iter().map(|w| w.ls).min().unwrap_or(es);
            let lf = ws.iter().map(|w| w.ls + w.dur).max().unwrap_or(ef);
            (es, ef, ls, lf, ef - es)
        };
        let slack = ls - es;
        let finish_of = |cal: &mut WorkCalendar, s: i64, e: i64| if e > s { cal.date(e - 1) } else { cal.date(s) };
        let dates = TaskDates {
            start: cal.date(es),
            finish: finish_of(cal, es, ef),
            late_start: cal.date(ls),
            late_finish: finish_of(cal, ls, lf),
            total_slack: slack,
            critical: slack <= 0,
            duration_days: dur,
        };
        if slack < 0 {
            warnings.push(format!("{} \"{}\" is {} working day(s) late against a deadline or constraint", t.wbs, t.name, -slack));
        }
        tasks.insert(t.id.clone(), dates);
    }
    tasks
}

/// The start index of a task of `dur` days whose finish (last working day) is `c`.
fn start_for_finish(c: i64, dur: i64) -> i64 {
    if dur == 0 { c } else { c + 1 - dur }
}

/// The exclusive end index of a task of `dur` days whose finish is `c`.
fn end_for_finish(c: i64, dur: i64) -> i64 {
    if dur == 0 { c } else { c + 1 }
}

fn active_tasks(p: &Project) -> Vec<&plancraft_project::Task> {
    // A cancelled summary cancels everything under it.
    let mut cancelled: Vec<&TaskId> = Vec::new();
    p.tasks
        .iter()
        .filter(|t| {
            let gone = t.cancelled || t.parent.as_ref().is_some_and(|pa| cancelled.contains(&pa));
            if gone {
                cancelled.push(&t.id);
            }
            !gone
        })
        .collect()
}

/// For each task, the indexes of the leaf tasks under it (itself for a leaf).
fn leaves_under(active: &[&plancraft_project::Task], index: &HashMap<&TaskId, usize>) -> HashMap<TaskId, Vec<usize>> {
    let mut out: HashMap<TaskId, Vec<usize>> = HashMap::new();
    let has_child: std::collections::HashSet<&TaskId> = active.iter().filter_map(|t| t.parent.as_ref()).filter(|p| index.contains_key(p)).collect();
    for (i, t) in active.iter().enumerate() {
        if has_child.contains(&t.id) {
            out.entry(t.id.clone()).or_default();
            continue;
        }
        out.entry(t.id.clone()).or_default().push(i);
        // Add to every ancestor (bounded: outline depth can't exceed the task count).
        let mut cur = t.parent.as_ref();
        for _ in 0..active.len() {
            let Some(pid) = cur else { break };
            let Some(&pi) = index.get(pid) else { break };
            out.entry(pid.clone()).or_default().push(i);
            cur = active.get(pi).and_then(|x| x.parent.as_ref());
        }
    }
    out
}

/// Kahn's order; nodes left in loops are appended in outline order. Returns (order, nodes in loops).
fn topo_order(n: usize, edges: &[(usize, usize, DependencyKind, i64)]) -> (Vec<usize>, usize) {
    let mut indeg = vec![0usize; n];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b, _, _) in edges {
        if let (Some(d), Some(o)) = (indeg.get_mut(b), out.get_mut(a)) {
            *d += 1;
            o.push(b);
        }
    }
    let mut q: VecDeque<usize> = (0..n).filter(|&i| indeg.get(i) == Some(&0)).collect();
    let mut order = Vec::with_capacity(n);
    let mut done = vec![false; n];
    while let Some(x) = q.pop_front() {
        order.push(x);
        if let Some(d) = done.get_mut(x) {
            *d = true;
        }
        for &y in out.get(x).map(Vec::as_slice).unwrap_or_default() {
            if let Some(d) = indeg.get_mut(y) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    q.push_back(y);
                }
            }
        }
    }
    let looped: Vec<usize> = (0..n).filter(|&i| !done.get(i).copied().unwrap_or(true)).collect();
    let count = looped.len();
    order.extend(looped);
    (order, count)
}

#[cfg(test)]
mod tests;
