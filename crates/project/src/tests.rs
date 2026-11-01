use super::*;

fn task(id: &str, parent: Option<&str>) -> Task {
    Task { id: TaskId(id.into()), parent: parent.map(|p| TaskId(p.into())), name: id.into(), priority: 500, ..Task::default() }
}

fn link(id: &str, a: &str, b: &str) -> Dependency {
    Dependency { id: id.into(), predecessor: TaskId(a.into()), successor: TaskId(b.into()), kind: DependencyKind::Fs, lag_minutes: 0 }
}

#[test]
fn healthy_project_has_no_problems() {
    let p = Project {
        tasks: vec![task("a", None), task("b", Some("a")), task("c", Some("a")), task("d", None)],
        dependencies: vec![link("1", "b", "c"), link("2", "c", "d")],
        ..Project::default()
    };
    assert!(p.problems().is_empty(), "{:?}", p.problems());
}

#[test]
fn finds_loops_dangling_links_and_duplicates() {
    let p = Project {
        tasks: vec![task("a", None), task("b", None), task("c", None), task("s", None), task("k", Some("s"))],
        dependencies: vec![
            link("1", "a", "b"),
            link("2", "b", "c"),
            link("3", "c", "a"),
            link("4", "a", "gone"),
            link("5", "a", "a"),
            link("6", "s", "k"),
            link("7", "a", "b"),
        ],
        ..Project::default()
    };
    let probs = p.problems();
    assert!(probs.iter().any(|x| matches!(x, Problem::DependencyLoop { tasks } if tasks.len() == 3)), "{probs:?}");
    assert!(probs.contains(&Problem::DanglingDependency { dependency: "4".into(), missing: TaskId("gone".into()) }));
    assert!(probs.iter().any(|x| matches!(x, Problem::SelfDependency { .. })));
    assert!(probs.contains(&Problem::DependencyOnSummary { dependency: "6".into() }));
    assert!(probs.contains(&Problem::DuplicateDependency { dependencies: vec!["1".into(), "7".into()] }));
    for x in &probs {
        assert!(!x.to_string().is_empty());
    }
}

#[test]
fn long_link_chains_dont_overflow() {
    let n = 50_000;
    let tasks: Vec<Task> = (0..n).map(|i| task(&i.to_string(), None)).collect();
    let mut deps: Vec<Dependency> = (1..n).map(|i| link(&format!("d{i}"), &(i - 1).to_string(), &i.to_string())).collect();
    deps.push(link("back", &(n - 1).to_string(), "0"));
    let p = Project { tasks, dependencies: deps, ..Project::default() };
    let loops: Vec<_> = p.problems().into_iter().filter(|x| matches!(x, Problem::DependencyLoop { .. })).collect();
    assert_eq!(loops.len(), 1);
}

#[test]
fn parent_cycles_dont_hang() {
    // Can't come from the tree store, but a hand-made value must not hang the checks.
    let p = Project { tasks: vec![task("a", Some("b")), task("b", Some("a"))], dependencies: vec![link("1", "a", "b")], ..Project::default() };
    let _ = p.problems();
}

#[test]
fn codes_round_trip() {
    for c in Constraint::ALL {
        assert_eq!(Constraint::parse(c.as_str()), c);
    }
    assert_eq!(Constraint::parse("mso"), Constraint::Mso);
    assert_eq!(Constraint::parse("nonsense"), Constraint::Asap);
    for k in [DependencyKind::Fs, DependencyKind::Ss, DependencyKind::Ff, DependencyKind::Sf] {
        assert_eq!(DependencyKind::parse(k.as_str()), k);
    }
    assert_eq!(Method::parse("Scrum"), Method::Agile);
    assert_eq!(ResourceKind::parse("COST"), ResourceKind::Cost);
}

#[test]
fn calendar_skips_weekends_and_holidays() {
    let mut c = Calendar::default();
    let fri: Date = "2026-11-27".parse().unwrap();
    assert!(c.is_working_day(fri));
    assert!(!c.is_working_day(fri.add_days(1).unwrap()));
    c.holidays = vec![fri];
    assert!(!c.is_working_day(fri));
}

#[test]
fn project_serializes_to_json() {
    let p = Project { tasks: vec![task("a", None)], ..Project::default() };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["tasks"][0]["id"], "a");
    assert_eq!(v["info"]["method"], "waterfall");
    let back: Project = serde_json::from_value(v).unwrap();
    assert_eq!(back, p);
}
