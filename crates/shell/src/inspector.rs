//! The Properties tab: the selected task's properties, or the project's information when no task
//! is selected. Text fields apply when they lose focus or on Enter.

use egui::{RichText, Ui};
use gridcraft_ui_egui::theme::Tokens;
use plancraft_project::{Constraint, Task};
use serde_json::json;

use crate::{Shell, section};

/// The Properties tab: the selected task's details, or the project's information. It fills the
/// pane it is in.
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        let Some(pi) = shell.active_project() else {
            ui.label(RichText::new("No project open.").color(t.text_dim));
            return;
        };
        let task = shell.projects.get(pi).and_then(|p| p.current().and_then(|id| p.project.task(id)).cloned());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match task {
            Some(task) => task_props(shell, ui, t, pi, &task),
            None => project_info(shell, ui, t, pi),
        });
    });
}

/// A text field that runs `cmd` with the new value when edited and committed. Keeps its draft in
/// egui memory under `key`, so typing isn't overwritten by the stored value each frame.
#[allow(clippy::too_many_arguments)]
fn field(
    shell: &mut Shell,
    ui: &mut Ui,
    pi: usize,
    label: &str,
    key: &str,
    current: &str,
    multiline: bool,
    on_commit: impl FnOnce(&str) -> (&'static str, serde_json::Value),
) {
    let label_resp = ui.label(RichText::new(if label.is_empty() { key } else { label }).size(12.0));
    let id = ui.scope_id().with(("pc_insp", key));
    let mut text: String = ui.ctx().data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let edit = if multiline { egui::TextEdit::multiline(&mut text).desired_rows(4) } else { egui::TextEdit::singleline(&mut text) };
    let bg = gridcraft_ui_egui::theme::Tokens::get(ui.ctx()).input_bg;
    let resp = ui.add(edit.desired_width(f32::INFINITY).background_color(bg)).labelled_by(label_resp.id);
    if resp.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(id, text.clone()));
    } else {
        ui.ctx().data_mut(|d| d.remove::<String>(id));
    }
    if resp.lost_focus() && text != current {
        let (cmd, params) = on_commit(&text);
        shell.run(pi, cmd, params);
    }
}

fn task_props(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize, task: &Task) {
    let id = task.id.0.clone();
    let k = |f: &str| format!("{id}:{f}");
    ui.label(
        RichText::new(format!(
            "{}  {}",
            task.wbs,
            if task.summary {
                "Summary task"
            } else if task.milestone {
                "Milestone"
            } else {
                "Task"
            }
        ))
        .color(t.text_dim)
        .size(12.0),
    );
    // A punch item's name, due date, assignee and status belong to RevuCraft: shown here, edited there.
    let from_rc = task.source.starts_with("revucraft:");
    if from_rc {
        let drawing = task.source.trim_start_matches("revucraft:").split("#nm=").next().unwrap_or_default().to_string();
        ui.label(RichText::new(format!("From RevuCraft: punch item in {drawing}")).color(t.text_dim).size(12.0));
        ui.label(RichText::new("Edit its name, due date, assignee and status in RevuCraft.").color(t.text_dim).size(11.5));
    }
    ui.add_enabled_ui(!from_rc, |ui| {
        field(shell, ui, pi, "Name", &k("name"), &task.name, false, |v| ("task.set", json!({"task": id, "field": "name", "value": v})));
        field(shell, ui, pi, "Location (room or zone)", &k("location"), &task.location, false, |v| {
            ("task.set", json!({"task": id, "field": "location", "value": v}))
        });
    });

    let dates = shell.projects.get(pi).and_then(|p| p.schedule.get(&task.id).cloned());
    if let Some(d) = &dates {
        section(ui, t, "Schedule");
        egui::Grid::new("pc_dates").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (l, v) in [
                ("Start", d.start.to_string()),
                ("Finish", d.finish.to_string()),
                ("Duration", format!("{} working day(s)", d.duration_days)),
                ("Total slack", format!("{} day(s)", d.total_slack)),
            ] {
                ui.label(RichText::new(l).color(t.text_dim).size(12.0));
                ui.label(RichText::new(v).size(12.5));
                ui.end_row();
            }
        });
        if d.critical && !task.summary {
            ui.label(RichText::new("On the critical path").color(crate::critical_color(t)).size(12.0));
        }
    }
    if !task.summary {
        section(ui, t, "Work");
        let days = f64::from(task.duration_minutes) / f64::from(plancraft_project::MINUTES_PER_DAY);
        let dur = format!("{}d", (days * 100.0).round() / 100.0);
        field(shell, ui, pi, "Duration (e.g. 3d, 4h, 2w)", &k("duration"), &dur, false, |v| {
            ("task.set", json!({"task": id, "field": "duration", "value": v}))
        });
        let mut pct = f32::from(task.percent_complete);
        let l = ui.label(RichText::new("% complete").size(12.0));
        let r = ui.add_enabled(!from_rc, egui::Slider::new(&mut pct, 0.0..=100.0).step_by(5.0).suffix("%").show_value(false)).labelled_by(l.id);
        ui.label(RichText::new(format!("{}%", pct.round())).size(12.0));
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            shell.run(pi, "task.set", json!({"task": id, "field": "percent", "value": (pct.round() as u8).to_string()}));
        }
        let mut ms = task.milestone;
        if ui.checkbox(&mut ms, "Milestone").changed() {
            shell.run(pi, "task.set", json!({"task": id, "field": "milestone", "value": ms.to_string()}));
        }

        section(ui, t, "Constraint");
        let mut c = task.constraint;
        craft_ui::ComboBox::new(("pc_constraint", &id), "Type").selected_text(constraint_label(c)).width(200.0).show_ui(ui, |ui| {
            for x in Constraint::ALL {
                ui.selectable_value(&mut c, x, constraint_label(x));
            }
        });
        if c != task.constraint {
            shell.run(pi, "task.set", json!({"task": id, "field": "constraint", "value": c.as_str()}));
        }
        if c.needs_date() {
            let cd = task.constraint_date.map(|d| d.to_string()).unwrap_or_default();
            field(shell, ui, pi, "Constraint date (YYYY-MM-DD)", &k("cdate"), &cd, false, |v| {
                ("task.set", json!({"task": id, "field": "constraint_date", "value": v}))
            });
        }
        let dl = task.deadline.map(|d| d.to_string()).unwrap_or_default();
        ui.add_enabled_ui(!from_rc, |ui| {
            field(shell, ui, pi, "Deadline (YYYY-MM-DD)", &k("deadline"), &dl, false, |v| {
                ("task.set", json!({"task": id, "field": "deadline", "value": v}))
            });
        });
    }

    links(shell, ui, t, pi, task);
    resources(shell, ui, t, pi, task);

    section(ui, t, "Notes");
    field(shell, ui, pi, "Notes and site details", &k("notes"), &task.notes, true, |v| {
        ("task.set", json!({"task": id, "field": "notes", "value": v}))
    });
    let mut cancelled = task.cancelled;
    if ui.checkbox(&mut cancelled, "Cancelled (kept for history, left out of the schedule)").changed() {
        shell.run(pi, "task.set", json!({"task": id, "field": "cancelled", "value": cancelled.to_string()}));
    }
    if !task.source.is_empty() {
        ui.label(RichText::new(format!("From: {}", task.source)).size(11.5).color(t.text_dim));
    }
}

fn constraint_label(c: Constraint) -> &'static str {
    match c {
        Constraint::Asap => "As soon as possible",
        Constraint::Alap => "As late as possible",
        Constraint::Snet => "Start no earlier than",
        Constraint::Snlt => "Start no later than",
        Constraint::Fnet => "Finish no earlier than",
        Constraint::Fnlt => "Finish no later than",
        Constraint::Mso => "Must start on",
        Constraint::Mfo => "Must finish on",
    }
}

fn links(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize, task: &Task) {
    let Some(p) = shell.projects.get(pi) else { return };
    let label = |id: &plancraft_project::TaskId| p.project.task(id).map(|x| format!("{} {}", x.wbs, x.name)).unwrap_or_else(|| "(deleted)".into());
    let preds: Vec<(String, String)> = p
        .project
        .dependencies
        .iter()
        .filter(|d| d.successor == task.id)
        .map(|d| (d.id.clone(), format!("{} ({})", label(&d.predecessor), d.kind.as_str())))
        .collect();
    let succs: Vec<(String, String)> = p
        .project
        .dependencies
        .iter()
        .filter(|d| d.predecessor == task.id)
        .map(|d| (d.id.clone(), format!("{} ({})", label(&d.successor), d.kind.as_str())))
        .collect();
    let others: Vec<(String, String)> =
        p.project.tasks.iter().filter(|x| x.id != task.id).map(|x| (x.id.0.clone(), format!("{} {}", x.wbs, x.name))).collect();
    section(ui, t, "Predecessors");
    let mut unlink = None;
    for (lid, text) in &preds {
        ui.horizontal(|ui| {
            if ui.small_button("×").on_hover_text("Remove link").clicked() {
                unlink = Some(lid.clone());
            }
            ui.label(RichText::new(text).size(12.5));
        });
    }
    let add_id = ui.scope_id().with(("pc_addpred", &task.id.0));
    let mut chosen: Option<String> = None;
    craft_ui::ComboBox::new(add_id, "Add").selected_text("Predecessor…").width(200.0).show_ui(ui, |ui| {
        for (oid, text) in &others {
            if ui.selectable_label(false, text).clicked() {
                chosen = Some(oid.clone());
            }
        }
    });
    if !succs.is_empty() {
        section(ui, t, "Successors");
        for (lid, text) in &succs {
            ui.horizontal(|ui| {
                if ui.small_button("×").on_hover_text("Remove link").clicked() {
                    unlink = Some(lid.clone());
                }
                ui.label(RichText::new(text).size(12.5));
            });
        }
    }
    if let Some(l) = unlink {
        shell.run(pi, "task.unlink", json!({"id": l}));
    }
    if let Some(from) = chosen {
        shell.run(pi, "task.link", json!({"from": from, "to": task.id.0}));
    }
}

fn resources(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize, task: &Task) {
    let Some(p) = shell.projects.get(pi) else { return };
    let assigned: Vec<(String, String)> = p
        .project
        .assignments
        .iter()
        .filter(|a| a.task == task.id)
        .map(|a| (a.id.clone(), p.project.resources.iter().find(|r| r.id == a.resource).map(|r| r.name.clone()).unwrap_or_default()))
        .collect();
    let all: Vec<(String, String)> = p.project.resources.iter().map(|r| (r.id.clone(), r.name.clone())).collect();
    section(ui, t, "Resources");
    let mut remove = None;
    for (aid, name) in &assigned {
        ui.horizontal(|ui| {
            if ui.small_button("×").on_hover_text("Remove assignment").clicked() {
                remove = Some(aid.clone());
            }
            ui.label(RichText::new(name).size(12.5));
        });
    }
    let mut chosen: Option<String> = None;
    let new_id = ui.scope_id().with(("pc_newres", &task.id.0));
    craft_ui::ComboBox::new(("pc_assign", &task.id.0), "Assign").selected_text("Resource…").width(190.0).show_ui(ui, |ui| {
        for (rid, name) in &all {
            if ui.selectable_label(false, name).clicked() {
                chosen = Some(rid.clone());
            }
        }
    });
    let mut new_name: String = ui.ctx().data_mut(|d| d.get_temp::<String>(new_id)).unwrap_or_default();
    let r = ui.add(
        egui::TextEdit::singleline(&mut new_name)
            .hint_text("New resource name, then Enter")
            .desired_width(f32::INFINITY)
            .background_color(t.input_bg),
    );
    ui.ctx().data_mut(|d| d.insert_temp(new_id, new_name.clone()));
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !new_name.trim().is_empty() {
        if let Some(v) = shell.run(pi, "resource.add", json!({"name": new_name.trim()}))
            && let Some(rid) = v.get("id").and_then(|x| x.as_str())
        {
            chosen = Some(rid.to_string());
        }
        ui.ctx().data_mut(|d| d.remove::<String>(new_id));
    }
    if let Some(a) = remove {
        shell.run(pi, "task.unassign", json!({"id": a}));
    }
    if let Some(rid) = chosen {
        shell.run(pi, "task.assign", json!({"task": task.id.0, "resource": rid}));
    }
}

fn project_info(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize) {
    let Some(info) = shell.projects.get(pi).map(|p| p.project.info.clone()) else { return };
    ui.label(RichText::new("Project Information").strong());
    ui.label(RichText::new("Select a task to see its properties.").size(12.0).color(t.text_dim));
    let s = |d: Option<plancraft_project::Date>| d.map(|d| d.to_string()).unwrap_or_default();
    let fields: [(&str, &str, String, bool); 10] = [
        ("Title", "title", info.title.clone(), false),
        ("Purpose", "purpose", info.purpose.clone(), true),
        ("Client", "client", info.client.clone(), false),
        ("Site address", "site_address", info.site_address.clone(), false),
        ("Start (YYYY-MM-DD)", "start", s(info.start), false),
        ("Deadline (YYYY-MM-DD)", "deadline", s(info.deadline), false),
        ("Project manager", "manager", info.manager.clone(), false),
        ("Sponsor", "sponsor", info.sponsor.clone(), false),
        ("Budget", "budget", info.budget.map(|b| b.to_string()).unwrap_or_default(), false),
        ("Currency", "currency", info.currency.clone(), false),
    ];
    for (label, key, cur, multi) in fields {
        field(shell, ui, pi, label, &format!("info:{key}"), &cur, multi, |v| ("project.setInfo", json!({"field": key, "value": v})));
    }
    let sched = shell.projects.get(pi).map(|p| (p.schedule.start, p.schedule.finish));
    if let Some((a, b)) = sched {
        section(ui, t, "Scheduled");
        ui.label(format!("{a} → {b}"));
        if let Some(d) = info.deadline {
            let left = b.days_until(d);
            let msg = if left >= 0 { format!("{left} day(s) to spare before the deadline") } else { format!("{} day(s) past the deadline", -left) };
            ui.label(RichText::new(msg).size(12.0).color(if left >= 0 { t.text_dim } else { crate::critical_color(t) }));
        }
    }
    section(ui, t, "Method");
    let mut m = info.method;
    ui.horizontal(|ui| {
        for x in [plancraft_project::Method::Waterfall, plancraft_project::Method::Agile, plancraft_project::Method::Hybrid] {
            ui.radio_value(&mut m, x, x.as_str());
        }
    });
    if m != info.method {
        shell.run(pi, "project.setInfo", json!({"field": "method", "value": m.as_str()}));
    }
}
