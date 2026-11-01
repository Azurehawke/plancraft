use super::*;

fn top() -> Placement {
    Placement::End { parent: None }
}

fn names(d: &ProjectDoc) -> Vec<(String, String)> {
    d.project().tasks.into_iter().map(|t| (t.wbs, t.name)).collect()
}

#[test]
fn outline_with_wbs_codes() {
    let d = ProjectDoc::new();
    let a = d.add_task("Survey", &top()).unwrap();
    let b = d.add_task("Install", &top()).unwrap();
    d.add_task("Walk the site", &Placement::End { parent: Some(a.clone()) }).unwrap();
    d.add_task("Photos", &Placement::End { parent: Some(a.clone()) }).unwrap();
    d.add_task("Before survey", &Placement::Before(a.clone())).unwrap();
    d.add_task("First of install", &Placement::At { parent: Some(b.clone()), index: 0 }).unwrap();
    assert_eq!(
        names(&d),
        [
            ("1".into(), "Before survey".into()),
            ("2".into(), "Survey".into()),
            ("2.1".into(), "Walk the site".into()),
            ("2.2".into(), "Photos".into()),
            ("3".into(), "Install".into()),
            ("3.1".into(), "First of install".into()),
        ]
    );
    let p = d.project();
    assert!(p.tasks[1].summary && !p.tasks[2].summary);
    assert_eq!(p.tasks[2].outline_level, 2);
    assert_eq!(p.tasks[2].parent.as_ref(), Some(&a));
}

#[test]
fn indent_outdent_and_moves() {
    let d = ProjectDoc::new();
    let a = d.add_task("A", &top()).unwrap();
    let b = d.add_task("B", &top()).unwrap();
    let c = d.add_task("C", &top()).unwrap();
    assert!(d.indent(&a).is_err(), "nothing above A");
    d.indent(&b).unwrap();
    d.indent(&c).unwrap();
    assert_eq!(names(&d), [("1".into(), "A".into()), ("1.1".into(), "B".into()), ("1.2".into(), "C".into())]);
    d.outdent(&b).unwrap();
    assert_eq!(names(&d), [("1".into(), "A".into()), ("1.1".into(), "C".into()), ("2".into(), "B".into())]);
    assert!(d.outdent(&a).is_err());
    assert!(matches!(d.move_task(&a, &Placement::End { parent: Some(c.clone()) }), Err(Error::WouldCycle)));
    d.move_task(&b, &Placement::At { parent: None, index: 0 }).unwrap();
    assert_eq!(names(&d)[0].1, "B");
    d.move_task(&b, &Placement::End { parent: None }).unwrap();
    assert_eq!(names(&d).last().unwrap().1, "B");
    d.move_task(&b, &Placement::After(b.clone())).unwrap();
}

#[test]
fn fields_parse_and_clear() {
    let d = ProjectDoc::new();
    let t = d.add_task("Pull cable", &top()).unwrap();
    d.set_task(&t, "duration", "1.5d").unwrap();
    d.set_task(&t, "start", "2026-11-02").unwrap();
    d.set_task(&t, "percent", "40%").unwrap();
    d.set_task(&t, "constraint", "snet").unwrap();
    d.set_task(&t, "constraint_date", "2026-11-02").unwrap();
    d.set_task(&t, "milestone", "yes").unwrap();
    let x = &d.project().tasks[0];
    assert_eq!((x.duration_minutes, x.percent_complete, x.constraint, x.milestone), (720, 40, Constraint::Snet, true));
    assert_eq!(x.start.unwrap().to_string(), "2026-11-02");
    d.set_task(&t, "start", "").unwrap();
    assert!(d.project().tasks[0].start.is_none());
    for (f, v) in [
        ("percent", "101"),
        ("start", "11/02/2026"),
        ("duration", "-1d"),
        ("duration", "3 parsecs"),
        ("constraint", "SOON"),
        ("milestone", "maybe"),
        ("priority", "5000"),
    ] {
        assert!(matches!(d.set_task(&t, f, v), Err(Error::BadValue { .. })), "{f}={v}");
    }
    assert!(matches!(d.set_task(&t, "colour", "x"), Err(Error::UnknownField(_))));
    assert!(matches!(d.set_task(&TaskId("1@2".into()), "name", "x"), Err(Error::UnknownTask(_))));
    assert!(matches!(d.set_task(&TaskId("garbage".into()), "name", "x"), Err(Error::UnknownTask(_))));
}

#[test]
fn info_calendar_resources_links() {
    let d = ProjectDoc::new();
    d.set_info("title", "Acme").unwrap();
    d.set_info("budget", "12,500").unwrap();
    d.set_info("deadline", "2026-12-18").unwrap();
    d.set_info("method", "Scrum").unwrap();
    assert!(d.set_info("nope", "x").is_err());
    let mut info = d.project().info;
    assert_eq!((info.title.as_str(), info.budget, info.method), ("Acme", Some(12500.0), Method::Agile));
    info.manager = "Steve".into();
    info.budget = None;
    d.update_info(&info).unwrap();
    assert_eq!(d.project().info, info);

    d.set_working_days([true, true, true, true, false, false, false]).unwrap();
    d.add_holiday("2026-11-26".parse().unwrap()).unwrap();
    d.set_hours_per_day(10.0).unwrap();
    assert!(d.set_hours_per_day(30.0).is_err());
    let c = d.project().calendar;
    assert!(!c.working_days[4] && c.hours_per_day == 10.0 && c.holidays.len() == 1);

    let a = d.add_task("A", &top()).unwrap();
    let b = d.add_task("B", &top()).unwrap();
    let l = d.link(&a, &b, DependencyKind::Ss, 60).unwrap();
    assert!(d.link(&a, &a, DependencyKind::Fs, 0).is_err());
    let r = d.add_resource("Tech 1").unwrap();
    d.set_resource(&r, "color", "#2b6cd4").unwrap();
    assert!(d.set_resource(&r, "color", "blue").is_err());
    d.assign(&b, &r, 0.5).unwrap();
    let p = d.project();
    assert_eq!(p.dependencies.len(), 1);
    assert_eq!(p.dependencies[0].kind, DependencyKind::Ss);
    assert_eq!(p.resources[0].color, "#2b6cd4");
    assert_eq!(p.assignments[0].units, 0.5);
    assert!(p.problems().is_empty());

    // Deleting B removes its link and assignment.
    d.delete_task(&b).unwrap();
    let p = d.project();
    assert!(p.dependencies.is_empty() && p.assignments.is_empty());
    assert!(d.unlink(&l).is_err());
    d.delete_resource(&r).unwrap();
    assert!(d.project().resources.is_empty());
}

#[test]
fn durations() {
    assert_eq!(parse_duration("90"), Some(90));
    assert_eq!(parse_duration("4h"), Some(240));
    assert_eq!(parse_duration("2 w"), Some(4800));
    assert_eq!(parse_duration("1e300d"), None);
    assert_eq!(parse_duration("NaN"), None);
    assert_eq!(parse_duration(""), None);
}

#[test]
fn snapshot_round_trip() {
    let d = ProjectDoc::new();
    d.add_task("A", &top()).unwrap();
    let e = ProjectDoc::new();
    e.import(&d.export_snapshot().unwrap()).unwrap();
    assert_eq!(e.project(), d.project());
    assert!(e.import(b"garbage").is_err());
    assert!(e.import(&[]).is_err());
}
