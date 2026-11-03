//! Shell tests: building a project through the shell, and offscreen renders of the window
//! (`PLANCRAFT_SNAPSHOT_DIR=/some/dir` keeps the PNGs for a look).

use super::*;
use serde_json::json;

fn d(s: &str) -> Date {
    s.parse().unwrap()
}

/// A shell with a small network-install job open.
fn sample(dir: &Path) -> Shell {
    let mut shell = Shell::new("laptop", Some(dir.join("settings")), d("2026-11-04"));
    let f = plancraft_project_sync::ProjectFolder::create(dir, "Acme Network", "laptop").unwrap();
    drop(f);
    shell.open_project(&dir.join("Acme Network.plan"));
    let run = |s: &mut Shell, id: &str, p: serde_json::Value| s.run(0, id, p).unwrap_or_else(|| panic!("{id}: {:?}", s.message));
    run(&mut shell, "project.setInfo", json!({"field": "start", "value": "2026-11-02"}));
    run(&mut shell, "project.setInfo", json!({"field": "deadline", "value": "2026-11-27"}));
    run(&mut shell, "project.setInfo", json!({"field": "manager", "value": "Steve"}));
    for (name, under, dur) in [
        ("Survey", None, ""),
        ("Walk the site", Some("1"), "1d"),
        ("Map closets and drops", Some("1"), "2d"),
        ("Install", None, ""),
        ("Pull cable", Some("2"), "4d"),
        ("Terminate and label", Some("2"), "2d"),
        ("Mount rack and switches", Some("2"), "1d"),
        ("Closeout", None, ""),
        ("Test and certify", Some("3"), "2d"),
    ] {
        let mut p = json!({"name": name, "duration": dur});
        if let Some(u) = under {
            p["under"] = json!(u);
        }
        run(&mut shell, "task.add", p);
    }
    run(&mut shell, "task.addMilestone", json!({"name": "Handover", "under": "3"}));
    for (a, b) in [("1.1", "1.2"), ("1", "2.1"), ("2.1", "2.2"), ("1", "2.3"), ("2.2", "3.1"), ("2.3", "3.1"), ("3.1", "3.2")] {
        run(&mut shell, "task.link", json!({"from": a, "to": b}));
    }
    run(&mut shell, "resource.add", json!({"name": "Tech 1"}));
    run(&mut shell, "resource.add", json!({"name": "Tech 2"}));
    run(&mut shell, "task.assign", json!({"task": "2.1", "resource": "Tech 1"}));
    run(&mut shell, "task.assign", json!({"task": "2.2", "resource": "Tech 2"}));
    run(&mut shell, "task.set", json!({"task": "1.1", "field": "percent", "value": "100"}));
    run(&mut shell, "task.set", json!({"task": "1.2", "field": "percent", "value": "50"}));
    shell
}

#[test]
fn shell_runs_commands_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let shell = sample(dir.path());
    let p = &shell.projects[0];
    assert_eq!(p.project.tasks.len(), 10);
    assert_eq!(p.schedule.finish.to_string(), "2026-11-16");
    // Saved: a second device opening the folder sees it all.
    let other = OpenProject::open(&dir.path().join("Acme Network.plan"), "tablet", d("2026-11-04")).unwrap();
    assert_eq!(other.project, p.project);
    assert!(dir.path().join("settings/shell.json").is_file(), "recent projects remembered");
}

#[test]
fn filters_keep_summaries_and_collapse_hides_subtasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    let p = &mut shell.projects[0];
    let crit = p.visible_rows(Filter::Critical);
    assert!(crit.iter().any(|&i| p.project.tasks[i].summary), "summaries above matches stay");
    let ms = p.visible_rows(Filter::Milestones);
    assert!(ms.iter().any(|&i| p.project.tasks[i].name == "Handover"));
    let install = p.project.find_task("2").unwrap().id.clone();
    p.collapsed.insert(install);
    assert_eq!(p.visible_rows(Filter::All).len(), 10 - 3);
}

#[test]
fn wizard_creates_a_project_with_its_answers() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = Shell::new("laptop", None, d("2026-11-04"));
    shell.start_wizard();
    if let Some(w) = &mut shell.wizard {
        w.folder = Some(dir.path().to_path_buf());
        w.title = "Smith Residence".into();
        w.deadline = "2026-12-18".into();
        w.manager = "Steve".into();
        w.budget = "12,500".into();
        w.stakeholders = "Dana Lee, Owner, dana@example.com\n\nTech 1, Installer".into();
        w.method = plancraft_project::Method::Hybrid;
    }
    wizard::create_for_test(&mut shell);
    assert!(shell.wizard.is_none(), "{:?}", shell.message);
    let p = &shell.projects[0].project;
    assert_eq!(p.info.title, "Smith Residence");
    assert_eq!((p.info.budget, p.info.method), (Some(12500.0), plancraft_project::Method::Hybrid));
    assert_eq!(p.resources.len(), 2);
    assert!(p.resources.iter().any(|r| r.name == "Dana Lee" && r.role == "Owner" && r.email == "dana@example.com"));
    assert!(dir.path().join("Smith Residence.plan/Smith Residence.plancraft").is_file());
}

#[test]
fn wizard_without_a_folder_asks_for_one() {
    let mut shell = Shell::new("laptop", None, d("2026-11-04"));
    shell.start_wizard();
    wizard::create_for_test(&mut shell);
    assert!(shell.wizard.as_ref().and_then(|w| w.error.clone()).is_some());
}

fn render(shell: &mut Shell, name: &str, size: egui::Vec2, dark: bool) {
    let mut harness = egui_kittest::Harness::builder().with_size(size).with_pixels_per_point(1.0).with_max_steps(8).wgpu().build_ui_state(
        |ui, shell: &mut Shell| {
            ui.ctx().set_theme(if dark { egui::Theme::Dark } else { egui::Theme::Light });
            if !ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::unique("pc_fonts")).unwrap_or(false)) {
                ui.ctx().set_fonts(gridcraft_ui_egui::theme::font_definitions());
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("pc_fonts"), true));
            }
            shell.show(ui, &mut |ui: &mut Ui| {
                ui.label("spreadsheet");
            });
        },
        std::mem::replace(shell, Shell::new("x", None, d("2026-11-04"))),
    );
    harness.run();
    let Ok(img) = harness.render() else {
        eprintln!("no GPU to render {name}; skipped");
        *shell = harness.into_state();
        return;
    };
    if let Some(dir) = std::env::var_os("PLANCRAFT_SNAPSHOT_DIR") {
        let _ = img.save(std::path::Path::new(&dir).join(format!("{name}.png")));
    }
    *shell = harness.into_state();
}

#[test]
fn renders_gantt_inspector_and_sidebar() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    let pick = shell.projects[0].project.find_task("2.1").unwrap().id.clone();
    shell.projects[0].select_only(pick);
    shell.panel = Panel::Outline;
    render(&mut shell, "gantt", egui::vec2(1500.0, 760.0), false);
    shell.projects[0].selected.clear();
    shell.panel = Panel::Sync;
    render(&mut shell, "gantt-dark-project-info", egui::vec2(1500.0, 760.0), true);
    shell.start_wizard();
    render(&mut shell, "wizard", egui::vec2(1200.0, 700.0), false);
    let mut empty = Shell::new("laptop", None, d("2026-11-04"));
    render(&mut empty, "start", egui::vec2(1100.0, 650.0), false);
}

#[test]
fn renders_the_spreadsheet_ui_inside_a_tab() {
    // GridCraft's whole window (ribbon, formula bar, grid) drawn in the shell's central area.
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    shell.show_spreadsheet_tab();
    let mut session = gridcraft_engine::Session::new();
    session.execute("file.new", json!({"sample": "budget"})).unwrap();
    let sheet = gridcraft_ui_egui::SheetApp::new(session, Default::default());
    let mut harness =
        egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 800.0)).with_pixels_per_point(1.0).with_max_steps(14).wgpu().build_ui_state(
            |ui, (shell, sheet): &mut (Shell, gridcraft_ui_egui::SheetApp)| {
                ui.ctx().set_theme(egui::Theme::Light);
                let ctx = ui.ctx().clone();
                sheet.logic(&ctx);
                shell.show(ui, &mut |ui: &mut Ui| sheet.ui(ui));
            },
            (shell, sheet),
        );
    harness.run_steps(14);
    let Ok(img) = harness.render() else { return };
    if let Some(dir) = std::env::var_os("PLANCRAFT_SNAPSHOT_DIR") {
        let _ = img.save(std::path::Path::new(&dir).join("spreadsheet-tab.png"));
    }
}

#[test]
fn renders_calendar_and_board() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    shell.run(0, "task.setStatus", json!({"task": "2.1", "status": "In Progress"})).unwrap();
    shell.run(0, "task.setStatus", json!({"task": "2.3", "status": "Review"})).unwrap();
    shell.run(0, "task.setStatus", json!({"task": "3.1", "status": "Backlog"})).unwrap();
    shell.run(0, "resource.set", json!({"resource": "Tech 1", "field": "color", "value": "#f2a93b"})).unwrap();
    shell.tab = Tab::View(View::Calendar);
    shell.projects[0].cal_month = Some((2026, 11));
    render(&mut shell, "calendar", egui::vec2(1500.0, 820.0), false);
    shell.tab = Tab::View(View::Kanban);
    let pick = shell.projects[0].project.find_task("2.2").unwrap().id.clone();
    shell.projects[0].select_only(pick);
    render(&mut shell, "board", egui::vec2(1600.0, 820.0), false);
    let p = &shell.projects[0].project;
    let done: Vec<&str> = p.tasks.iter().filter(|t| !t.summary && p.status_of(t) == "Done").map(|t| t.name.as_str()).collect();
    assert_eq!(done, ["Walk the site"], "100% tasks sit in Done");
}

#[test]
fn a_linked_drawings_punch_list_becomes_tasks_and_follows_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    shell.set_revucraft_link(true);
    // RevuCraft's job folder for "Floor/Plan.pdf" is "Floor/Plan/", beside the PDF.
    let pdf = dir.path().join("Floor").join("Plan.pdf");
    assert!(shell.link_drawing(&pdf));
    assert_eq!(shell.projects[0].project.drawings, ["Floor/Plan.pdf"]);
    let markups = dir.path().join("Floor").join("Plan").join("markups.json");
    std::fs::create_dir_all(markups.parent().unwrap()).unwrap();
    let punch = r#"{"format":"revucraft-markups","version":1,"punch":[{"name":"n1","no":1,"assignee":"Tech 1","due":"2026-11-10","status":"Open"}]}"#;
    std::fs::write(&markups, punch).unwrap();
    shell.poll(1e9);
    let p = &shell.projects[0].project;
    let t = p.tasks.iter().find(|t| t.source == "revucraft:Floor/Plan.pdf#nm=n1").unwrap();
    assert_eq!((t.name.as_str(), t.deadline.map(|d| d.to_string())), ("Punch #1", Some("2026-11-10".into())));
    assert!(p.assignments.iter().any(|a| a.task == t.id), "assigned to the named person");
    // A file nobody changed is not read again.
    let before = shell.projects[0].project.clone();
    shell.poll(2e9);
    assert_eq!(shell.projects[0].project, before);
    // With the link off, changes are not brought in.
    shell.set_revucraft_link(false);
    std::fs::write(&markups, punch.replace("Open", "Closed")).unwrap();
    shell.poll(3e9);
    let t = shell.projects[0].project.tasks.iter().find(|t| t.source.ends_with("#nm=n1")).unwrap();
    assert_eq!(t.status, "To Do");
    // Turning it back on reads the punch list again: closed in RevuCraft means done here.
    shell.set_revucraft_link(true);
    let t = shell.projects[0].project.tasks.iter().find(|t| t.source.ends_with("#nm=n1")).unwrap();
    assert_eq!((t.status.as_str(), t.percent_complete), ("Done", 100));
}

#[test]
fn a_drawing_outside_the_job_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    let elsewhere = tempfile::tempdir().unwrap();
    assert!(!shell.link_drawing(&elsewhere.path().join("Plan.pdf")));
    assert!(shell.projects[0].project.drawings.is_empty());
    assert!(shell.message.as_deref().is_some_and(|m| m.contains("job folder")));
}

#[test]
fn renders_a_punch_task_from_revucraft() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    shell.set_revucraft_link(true);
    let pdf = dir.path().join("Floor").join("Plan.pdf");
    assert!(shell.link_drawing(&pdf));
    let markups = dir.path().join("Floor").join("Plan").join("markups.json");
    std::fs::create_dir_all(markups.parent().unwrap()).unwrap();
    let punch = r#"{"format":"revucraft-markups","version":1,"punch":[{"name":"n1","no":3,"assignee":"Electrician","due":"2026-11-10","status":"InProgress"},{"name":"n2","no":4,"assignee":"Tech 1","due":"2026-11-12","status":"Open"}]}"#;
    std::fs::write(&markups, punch).unwrap();
    shell.poll(1e9);
    let pick = shell.projects[0].project.tasks.iter().find(|t| t.source.ends_with("#nm=n1")).unwrap().id.clone();
    shell.projects[0].select_only(pick);
    render(&mut shell, "revucraft-punch", egui::vec2(1500.0, 820.0), false);
    shell.panel = crate::Panel::Settings;
    render(&mut shell, "revucraft-settings", egui::vec2(1500.0, 820.0), false);
}

#[test]
fn one_project_is_shown_at_a_time_and_switching_keeps_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut shell = sample(dir.path());
    drop(plancraft_project_sync::ProjectFolder::create(dir.path(), "Other Job", "laptop").unwrap());
    shell.open_project(&dir.path().join("Other Job.plan"));
    assert_eq!(shell.active_project(), Some(1), "a newly opened project is the one shown");
    shell.focus_project(0);
    shell.tab = Tab::View(View::Kanban);
    shell.focus_project(1);
    assert_eq!(shell.active_project(), Some(1));
    assert_eq!(shell.tab, Tab::View(View::Kanban), "switching project keeps the view");
    shell.show_spreadsheet_tab();
    assert!(shell.sheet_open);
    assert_eq!(shell.tab, Tab::Spreadsheet);
    shell.focus_project(99);
    assert_eq!(shell.active_project(), Some(1), "an unknown project index is ignored");
}
