//! The optional new project wizard. Every step can be skipped: "Skip and create" makes the
//! project with whatever has been filled in. The answers become the project's information
//! (editable later in the Inspector's Project Information).

use gridcraft_ui_egui::theme::Tokens;
use std::path::PathBuf;

use egui::{RichText, Ui};
use plancraft_project::Method;
use plancraft_project_sync::ProjectFolder;
use serde_json::json;

use crate::{OpenProject, Request, Shell};

const STEPS: [&str; 6] = ["Basics", "Dates", "People", "Budget", "Method", "Review"];

/// The wizard's answers.
#[derive(Debug, Clone, Default)]
pub struct Wizard {
    pub step: usize,
    pub folder: Option<PathBuf>,
    pub title: String,
    pub purpose: String,
    pub client: String,
    pub site_address: String,
    pub start: String,
    pub deadline: String,
    pub manager: String,
    pub sponsor: String,
    /// One per line: `Name, role, email`.
    pub stakeholders: String,
    pub budget: String,
    pub currency: String,
    pub method: Method,
    pub error: Option<String>,
}

impl Wizard {
    pub fn new(folder: Option<PathBuf>) -> Wizard {
        Wizard { folder, currency: "USD".into(), ..Wizard::default() }
    }
}

pub(crate) fn show(shell: &mut Shell, ctx: &egui::Context) {
    let t = crate::theme::tokens(ctx);
    let mut open = true;
    let mut action: Option<&str> = None;
    egui::Window::new("New Project")
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            let Some(w) = shell.wizard.as_mut() else { return };
            // Step strip.
            ui.horizontal(|ui| {
                for (i, s) in STEPS.iter().enumerate() {
                    let label = RichText::new(format!("{}. {s}", i + 1)).size(12.5).color(if i == w.step { t.text } else { t.text_dim });
                    if ui.selectable_label(i == w.step, label).clicked() {
                        w.step = i;
                    }
                }
            });
            ui.separator();
            ui.set_min_height(250.0);
            match w.step {
                0 => basics(ui, w, &mut action),
                1 => {
                    text(ui, "Start date (YYYY-MM-DD; empty = the day you start)", &mut w.start, false);
                    text(ui, "Deadline (YYYY-MM-DD)", &mut w.deadline, false);
                    ui.label(RichText::new("The work week starts as Monday–Friday, 8 hours; change it later.").size(12.0).color(t.text_dim));
                }
                2 => {
                    text(ui, "Project manager", &mut w.manager, false);
                    text(ui, "Sponsor", &mut w.sponsor, false);
                    ui.label(RichText::new("Stakeholders and team, one per line: name, role, email").size(12.5));
                    ui.add(
                        egui::TextEdit::multiline(&mut w.stakeholders)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY)
                            .hint_text("Dana Lee, Facilities manager, dana@example.com"),
                    );
                    ui.label(RichText::new("Each line becomes a resource you can assign to tasks.").size(12.0).color(t.text_dim));
                }
                3 => {
                    text(ui, "Total budget", &mut w.budget, false);
                    text(ui, "Currency", &mut w.currency, false);
                }
                4 => {
                    ui.label("How will this project be run?");
                    for (m, d) in [
                        (Method::Waterfall, "Phases in order, with a schedule and a Gantt chart."),
                        (Method::Agile, "Sprints and a backlog (the board arrives in a later phase)."),
                        (Method::Hybrid, "A schedule, with some phases run as sprints."),
                    ] {
                        ui.radio_value(&mut w.method, m, RichText::new(m.as_str()).strong());
                        ui.label(RichText::new(d).size(12.0).color(t.text_dim));
                    }
                }
                _ => review(ui, &t, w),
            }
            if let Some(e) = &w.error {
                ui.label(RichText::new(e).color(t.danger));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Skip and create").on_hover_text("Create the project now with what's filled in").clicked() {
                    action = Some("create");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w.step + 1 < STEPS.len() {
                        if ui.button("Next ›").clicked() {
                            w.step += 1;
                        }
                    } else if ui.add(egui::Button::new(RichText::new("Create project").color(t.on_accent)).fill(t.accent)).clicked() {
                        action = Some("create");
                    }
                    if w.step > 0 && ui.button("‹ Back").clicked() {
                        w.step -= 1;
                    }
                });
            });
        });
    if !open {
        shell.wizard = None;
        return;
    }
    match action {
        Some("browse") => shell.request(Request::PickWizardFolder),
        Some("create") => create(shell),
        _ => {}
    }
}

fn text(ui: &mut Ui, label: &str, value: &mut String, multiline: bool) {
    let l = ui.label(RichText::new(label).size(12.5));
    let e = if multiline { egui::TextEdit::multiline(value).desired_rows(3) } else { egui::TextEdit::singleline(value) };
    let bg = crate::theme::tokens(ui.ctx()).input_bg;
    ui.add(e.desired_width(f32::INFINITY).background_color(bg)).labelled_by(l.id);
}

fn basics(ui: &mut Ui, w: &mut Wizard, action: &mut Option<&str>) {
    text(ui, "Project title", &mut w.title, false);
    text(ui, "Purpose", &mut w.purpose, true);
    text(ui, "Client", &mut w.client, false);
    text(ui, "Site address", &mut w.site_address, false);
    ui.label(RichText::new("Save in folder (a shared folder, e.g. your Syncthing jobs folder)").size(12.5));
    ui.horizontal(|ui| {
        let mut f = w.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut f).desired_width(360.0).hint_text("M:\\Jobs")).changed() {
            w.folder = if f.trim().is_empty() { None } else { Some(PathBuf::from(f.trim())) };
        }
        if ui.button("Browse…").clicked() {
            *action = Some("browse");
        }
    });
}

fn review(ui: &mut Ui, t: &Tokens, w: &Wizard) {
    let rows = [
        ("Title", w.title.clone()),
        ("Folder", w.folder.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(choose one in Basics)".into())),
        ("Start", w.start.clone()),
        ("Deadline", w.deadline.clone()),
        ("Manager", w.manager.clone()),
        ("Budget", if w.budget.is_empty() { String::new() } else { format!("{} {}", w.budget, w.currency) }),
        ("Method", w.method.as_str().to_string()),
        ("People", format!("{} listed", w.stakeholders.lines().filter(|l| !l.trim().is_empty()).count())),
    ];
    egui::Grid::new("pc_wiz_review").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(RichText::new(k).color(t.text_dim));
            ui.label(if v.is_empty() { "—".to_string() } else { v });
            ui.end_row();
        }
    });
}

#[cfg(test)]
pub(crate) fn create_for_test(shell: &mut Shell) {
    create(shell);
}

/// Makes the project folder, writes the answers, opens it in a tab.
fn create(shell: &mut Shell) {
    let Some(w) = shell.wizard.clone() else { return };
    let Some(folder) = w.folder.clone() else {
        if let Some(wz) = &mut shell.wizard {
            wz.step = 0;
            wz.error = Some("Choose the folder to save the project in.".into());
        }
        return;
    };
    let title = if w.title.trim().is_empty() { "New Project".to_string() } else { w.title.trim().to_string() };
    let made = ProjectFolder::create(&folder, &title, shell.device());
    let f = match made {
        Ok(f) => f,
        Err(e) => {
            if let Some(wz) = &mut shell.wizard {
                wz.error = Some(e.to_string());
            }
            return;
        }
    };
    let doc = f.doc().clone();
    let mut errors = Vec::new();
    for (k, v) in [
        ("purpose", &w.purpose),
        ("client", &w.client),
        ("site_address", &w.site_address),
        ("start", &w.start),
        ("deadline", &w.deadline),
        ("manager", &w.manager),
        ("sponsor", &w.sponsor),
        ("budget", &w.budget),
        ("currency", &w.currency),
    ] {
        if !v.trim().is_empty()
            && let Err(e) = plancraft_commands::run(&doc, "project.setInfo", &json!({"field": k, "value": v.trim()}))
        {
            errors.push(format!("{k}: {e}"));
        }
    }
    let _ = plancraft_commands::run(&doc, "project.setInfo", &json!({"field": "method", "value": w.method.as_str()}));
    for line in w.stakeholders.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split(',').map(str::trim);
        let name = parts.next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        if let Ok(v) = plancraft_commands::run(&doc, "resource.add", &json!({"name": name})) {
            let rid = v.get("id").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            if let Some(role) = parts.next().filter(|s| !s.is_empty()) {
                let _ = plancraft_commands::run(&doc, "resource.set", &json!({"resource": rid, "field": "role", "value": role}));
            }
            if let Some(email) = parts.next().filter(|s| !s.is_empty()) {
                let _ = plancraft_commands::run(&doc, "resource.set", &json!({"resource": rid, "field": "email", "value": email}));
            }
        }
    }
    let mut f = f;
    if let Err(e) = f.save() {
        errors.push(e.to_string());
    }
    let p = OpenProject::from_folder(f, shell.today);
    shell.remember_recent(p.dir().to_path_buf());
    shell.projects.push(p);
    shell.focus_project(shell.projects.len() - 1);
    shell.wizard = None;
    if !errors.is_empty() {
        shell.message = Some(format!("Project created; some answers weren't saved: {}", errors.join("; ")));
    }
}
