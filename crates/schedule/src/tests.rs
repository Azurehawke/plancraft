//! Hand-checked schedules. 2026-11-02 is a Monday; the default calendar is Monday–Friday, 8 h.

use plancraft_project::{Calendar, Dependency, DependencyKind, Project, ProjectInfo, Task};

use super::*;

fn d(s: &str) -> Date {
    s.parse().unwrap()
}

fn task(id: &str, days: u32) -> Task {
    Task { id: TaskId(id.into()), name: id.into(), duration_minutes: days * 480, priority: 500, outline_level: 1, wbs: id.into(), ..Task::default() }
}

fn under(mut t: Task, parent: &str) -> Task {
    t.parent = Some(TaskId(parent.into()));
    t.outline_level = 2;
    t
}

fn link(a: &str, b: &str, kind: DependencyKind, lag_days: i32) -> Dependency {
    Dependency { id: format!("{a}-{b}"), predecessor: TaskId(a.into()), successor: TaskId(b.into()), kind, lag_minutes: lag_days * 480 }
}

fn project(tasks: Vec<Task>, deps: Vec<Dependency>) -> Project {
    let mut tasks = tasks;
    let parents: Vec<TaskId> = tasks.iter().filter_map(|t| t.parent.clone()).collect();
    for t in &mut tasks {
        t.summary = parents.contains(&t.id);
    }
    Project { info: ProjectInfo { start: Some(d("2026-11-02")), ..ProjectInfo::default() }, tasks, dependencies: deps, ..Project::default() }
}

fn dates(s: &Schedule, id: &str) -> (String, String, i64, bool) {
    let t = s.get(&TaskId(id.into())).unwrap();
    (t.start.to_string(), t.finish.to_string(), t.total_slack, t.critical)
}

#[test]
fn finish_to_start_chain_skips_weekends() {
    let p =
        project(vec![task("a", 3), task("b", 4), task("c", 1)], vec![link("a", "b", DependencyKind::Fs, 0), link("b", "c", DependencyKind::Fs, 0)]);
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "a"), ("2026-11-02".into(), "2026-11-04".into(), 0, true));
    assert_eq!(dates(&s, "b"), ("2026-11-05".into(), "2026-11-10".into(), 0, true)); // Thu, Fri, Mon, Tue
    assert_eq!(dates(&s, "c"), ("2026-11-11".into(), "2026-11-11".into(), 0, true));
    assert_eq!(s.finish.to_string(), "2026-11-11");
}

#[test]
fn parallel_branch_has_slack() {
    // a(5) → c(1); b(2) → c. b can slip 3 days.
    let p =
        project(vec![task("a", 5), task("b", 2), task("c", 1)], vec![link("a", "c", DependencyKind::Fs, 0), link("b", "c", DependencyKind::Fs, 0)]);
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "b"), ("2026-11-02".into(), "2026-11-03".into(), 3, false));
    let b = s.get(&TaskId("b".into())).unwrap();
    assert_eq!((b.late_start.to_string(), b.late_finish.to_string()), ("2026-11-05".into(), "2026-11-06".into()));
    assert!(dates(&s, "a").3 && dates(&s, "c").3);
}

#[test]
fn link_types_and_lag() {
    let p = project(
        vec![task("a", 3), task("ss", 2), task("ff", 2), task("lag", 1), task("lead", 1)],
        vec![
            link("a", "ss", DependencyKind::Ss, 1),
            link("a", "ff", DependencyKind::Ff, 0),
            link("a", "lag", DependencyKind::Fs, 2),
            link("a", "lead", DependencyKind::Fs, -1),
        ],
    );
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "ss").0, "2026-11-03"); // a's start + 1
    assert_eq!(dates(&s, "ff").1, "2026-11-04"); // finishes with a
    assert_eq!(dates(&s, "lag").0, "2026-11-09"); // a ends Wed, +2 working days → Mon
    assert_eq!(dates(&s, "lead").0, "2026-11-04"); // overlaps a's last day
}

#[test]
fn summaries_span_subtasks_and_links_to_summaries_apply_to_all() {
    let p = project(
        vec![task("s", 0), under(task("x", 2), "s"), under(task("y", 3), "s"), task("after", 1)],
        vec![link("x", "y", DependencyKind::Fs, 0), link("s", "after", DependencyKind::Fs, 0)],
    );
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "s").0, "2026-11-02");
    assert_eq!(dates(&s, "s").1, "2026-11-06");
    assert_eq!(dates(&s, "after").0, "2026-11-09");
    assert_eq!(s.get(&TaskId("s".into())).unwrap().duration_days, 5);
}

#[test]
fn constraints_and_deadlines() {
    let mut snet = task("snet", 1);
    snet.constraint = Constraint::Snet;
    snet.constraint_date = Some(d("2026-11-07")); // a Saturday → the next Monday
    let mut mfo = task("mfo", 2);
    mfo.constraint = Constraint::Mfo;
    mfo.constraint_date = Some(d("2026-11-13"));
    let mut late = task("late", 5);
    late.deadline = Some(d("2026-11-04"));
    let p = project(vec![snet, mfo, late], vec![]);
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "snet").0, "2026-11-09");
    assert_eq!((dates(&s, "mfo").0, dates(&s, "mfo").1), ("2026-11-12".into(), "2026-11-13".into()));
    let l = dates(&s, "late");
    assert_eq!(l.2, -2, "five days can't end by Wednesday");
    assert!(l.3 && s.warnings.iter().any(|w| w.contains("late")));
}

#[test]
fn milestones_alap_holidays_and_cancelled() {
    let mut m = task("m", 0);
    m.milestone = true;
    let mut alap = task("alap", 1);
    alap.constraint = Constraint::Alap;
    let mut gone = task("gone", 10);
    gone.cancelled = true;
    let mut p = project(vec![task("a", 2), m, alap, task("end", 5), gone], vec![link("a", "m", DependencyKind::Fs, 0)]);
    p.calendar = Calendar { holidays: vec![d("2026-11-03")], ..Calendar::default() };
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(dates(&s, "a").1, "2026-11-04", "skips the holiday");
    assert_eq!((dates(&s, "m").0, dates(&s, "m").1), ("2026-11-05".into(), "2026-11-05".into()));
    assert_eq!(dates(&s, "alap").0, dates(&s, "end").1, "placed as late as possible");
    assert!(s.get(&TaskId("gone".into())).is_none());
}

#[test]
fn loops_are_reported_not_fatal() {
    let p = project(vec![task("a", 1), task("b", 1)], vec![link("a", "b", DependencyKind::Fs, 0), link("b", "a", DependencyKind::Fs, 0)]);
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(s.tasks.len(), 2);
    assert!(s.warnings.iter().any(|w| w.contains("loop")));
}

#[test]
fn hostile_values_stay_bounded() {
    let mut big = task("big", 0);
    big.duration_minutes = u32::MAX;
    let mut far = task("far", 1);
    far.constraint = Constraint::Mso;
    far.constraint_date = Some(d("9999-12-31"));
    let mut p = project(vec![big, far, task("c", 1)], vec![link("big", "c", DependencyKind::Fs, i32::MAX / 480)]);
    p.calendar.working_days = [false; 7];
    p.calendar.hours_per_day = 0.0;
    let s = schedule(&p, d("2000-01-01"));
    assert_eq!(s.tasks.len(), 3);
    assert!(s.warnings.iter().any(|w| w.contains("no working days")));
}

#[test]
fn no_start_date_uses_today_and_empty_project_is_fine() {
    let mut p = project(vec![task("a", 1)], vec![]);
    p.info.start = None;
    assert_eq!(dates(&schedule(&p, d("2026-11-04")), "a").0, "2026-11-04");
    let s = schedule(&Project::default(), d("2026-11-04"));
    assert!(s.tasks.is_empty());
    assert_eq!(s.finish.to_string(), "2026-11-04");
}

#[test]
fn large_project_is_quick() {
    let tasks: Vec<Task> = (0..5000).map(|i| task(&i.to_string(), 1 + (i % 4) as u32)).collect();
    let deps: Vec<Dependency> = (1..5000).map(|i| link(&(i - 1).to_string(), &i.to_string(), DependencyKind::Fs, 0)).collect();
    let s = schedule(&project(tasks, deps), d("2000-01-01"));
    assert_eq!(s.tasks.len(), 5000);
    assert!(s.tasks.values().all(|t| t.critical));
}
