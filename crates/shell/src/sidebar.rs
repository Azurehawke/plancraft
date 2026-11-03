//! The left sidebar: an icon rail that picks a panel (Explorer, Search, Bookmarks, Outline,
//! Filters, Recent, Sync), as in Obsidian. The rail stays when the panel is collapsed.

use egui::{RichText, Ui};
use gridcraft_ui_egui::icons::{self, Icon};
use gridcraft_ui_egui::theme::Tokens;
use plancraft_project::TaskId;

use crate::{Bookmark, Filter, Panel, Request, Shell, section};

fn icon(p: Panel) -> Icon {
    match p {
        Panel::Explorer => Icon::Folder,
        Panel::Search => Icon::Search,
        Panel::Bookmarks => Icon::Note,
        Panel::Outline => Icon::Group,
        Panel::Filters => Icon::Filter,
        Panel::Recent => Icon::Clock,
        Panel::Sync => Icon::Share,
        Panel::Settings => Icon::Settings,
    }
}

pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    egui::Panel::left("pc_rail").exact_size(40.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(4, 8))).show(
        ui,
        |ui| {
            ui.vertical_centered(|ui| {
                for p in Panel::ALL {
                    let on = shell.left_open && shell.panel == p;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
                    if on {
                        ui.painter().rect_filled(rect, 6.0, t.pressed);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 6.0, t.hover);
                    }
                    icons::paint(
                        ui.painter(),
                        egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0)),
                        icon(p),
                        if on { t.text } else { t.text_dim },
                    );
                    if resp.on_hover_text(p.label()).clicked() {
                        if on {
                            shell.left_open = false;
                        } else {
                            shell.left_open = true;
                            shell.panel = p;
                        }
                        shell.save_prefs();
                    }
                    ui.add_space(4.0);
                }
            });
        },
    );
    if !shell.left_open {
        return;
    }
    egui::Panel::left("pc_sidebar")
        .default_size(250.0)
        .size_range(180.0..=480.0)
        .resizable(true)
        .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(10, 8)))
        .show(ui, |ui| {
            ui.label(RichText::new(shell.panel.label()).strong().color(t.text));
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match shell.panel {
                Panel::Explorer => explorer(shell, ui, t),
                Panel::Search => search(shell, ui, t),
                Panel::Bookmarks => bookmarks(shell, ui, t),
                Panel::Outline => outline(shell, ui, t),
                Panel::Filters => filters(shell, ui, t),
                Panel::Recent => recent(shell, ui, t),
                Panel::Sync => sync(shell, ui, t),
                Panel::Settings => settings(shell, ui, t),
            });
        });
}

fn explorer(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        if ui.button("New…").clicked() {
            shell.start_wizard();
        }
        if ui.button("Open…").clicked() {
            shell.request(Request::OpenProject);
        }
    });
    if shell.projects.is_empty() {
        ui.label(RichText::new("No project open.").color(t.text_dim));
        return;
    }
    let mut focus = None;
    for (i, p) in shell.projects.iter().enumerate() {
        egui::CollapsingHeader::new(RichText::new(p.title()).strong()).id_salt(("pc_exp", i)).default_open(true).show(ui, |ui| {
            if ui.selectable_label(shell.active_project() == Some(i), "Schedule (Gantt)").clicked() {
                focus = Some(i);
            }
            let dir = p.dir().to_path_buf();
            let mut entries: Vec<(String, bool)> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path().is_dir()))
                        .filter(|(n, _)| !n.starts_with('.'))
                        .collect()
                })
                .unwrap_or_default();
            entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            for (name, is_dir) in entries {
                let label = if is_dir { format!("{name}/") } else { name.clone() };
                let r = ui.label(RichText::new(label).color(t.text_dim));
                if is_dir && name == "devices" {
                    r.on_hover_text("One change file per device (Sync panel)");
                }
            }
        });
    }
    if let Some(i) = focus {
        shell.focus_project(i);
    }
}

fn search(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    ui.add(egui::TextEdit::singleline(&mut shell.search).hint_text("Search tasks and notes").desired_width(f32::INFINITY));
    let q = shell.search.trim().to_lowercase();
    if q.is_empty() {
        ui.label(RichText::new("Searches task names and notes in every open project.").color(t.text_dim).size(12.0));
        return;
    }
    let mut pick: Option<(usize, TaskId)> = None;
    let mut found = 0;
    for (i, p) in shell.projects.iter().enumerate() {
        for task in &p.project.tasks {
            let hit_name = task.name.to_lowercase().contains(&q);
            let hit_notes = task.notes.to_lowercase().contains(&q);
            if hit_name || hit_notes {
                found += 1;
                let mut label = format!("{}  {}", task.wbs, task.name);
                if !hit_name {
                    label.push_str("  (in notes)");
                }
                if ui.selectable_label(false, label).on_hover_text(p.title()).clicked() {
                    pick = Some((i, task.id.clone()));
                }
            }
        }
    }
    if found == 0 {
        ui.label(RichText::new("No matches.").color(t.text_dim));
    }
    if let Some((i, id)) = pick {
        go_to(shell, i, id);
    }
}

fn go_to(shell: &mut Shell, i: usize, id: TaskId) {
    shell.focus_project(i);
    if let Some(p) = shell.projects.get_mut(i) {
        // Expand the summaries above it.
        let mut cur = p.project.task(&id).and_then(|t| t.parent.clone());
        while let Some(pid) = cur {
            p.collapsed.remove(&pid);
            cur = p.project.task(&pid).and_then(|t| t.parent.clone());
        }
        p.select_only(id);
    }
}

fn bookmarks(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    let current = shell.active_project().and_then(|i| {
        shell.projects.get(i).and_then(|p| {
            p.current().and_then(|id| p.project.task(id)).map(|tk| (p.dir().to_path_buf(), tk.id.0.clone(), format!("{} {}", tk.wbs, tk.name)))
        })
    });
    ui.add_enabled_ui(current.is_some(), |ui| {
        if ui.button("Bookmark the selected task").clicked()
            && let Some((project, task, label)) = current.clone()
            && !shell.prefs.bookmarks.iter().any(|b| b.project == project && b.task == task)
        {
            shell.prefs.bookmarks.push(Bookmark { project, task, label });
            shell.save_prefs();
        }
    });
    if shell.prefs.bookmarks.is_empty() {
        ui.label(RichText::new("Pinned tasks appear here (kept on this device).").color(t.text_dim).size(12.0));
    }
    let mut remove = None;
    let mut open = None;
    for (k, b) in shell.prefs.bookmarks.iter().enumerate() {
        ui.horizontal(|ui| {
            if ui.small_button("×").on_hover_text("Remove bookmark").clicked() {
                remove = Some(k);
            }
            let project = b.project.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if ui.selectable_label(false, b.label.clone()).on_hover_text(project).clicked() {
                open = Some(b.clone());
            }
        });
    }
    if let Some(k) = remove {
        shell.prefs.bookmarks.remove(k);
        shell.save_prefs();
    }
    if let Some(b) = open {
        shell.open_project(&b.project);
        if let Some(i) = shell.projects.iter().position(|p| p.dir() == b.project) {
            go_to(shell, i, TaskId(b.task));
        }
    }
}

fn outline(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    let Some(i) = shell.active_project() else {
        ui.label(RichText::new("Open a project to see its outline.").color(t.text_dim));
        return;
    };
    let mut pick = None;
    if let Some(p) = shell.projects.get(i) {
        for task in &p.project.tasks {
            let indent = "    ".repeat(task.outline_level.saturating_sub(1) as usize);
            let text = format!("{indent}{} {}", task.wbs, task.name);
            let label = if task.summary { RichText::new(text).strong() } else { RichText::new(text) };
            if ui.selectable_label(p.selected.contains(&task.id), label).clicked() {
                pick = Some(task.id.clone());
            }
        }
    }
    if let Some(id) = pick {
        go_to(shell, i, id);
    }
}

fn filters(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    for f in Filter::ALL {
        let count = shell.active_project().and_then(|i| shell.projects.get(i)).map(|p| p.visible_rows(f).len());
        let label = match count {
            Some(n) if f != Filter::All => format!("{}  ({n})", f.label()),
            _ => f.label().to_string(),
        };
        if ui.selectable_label(shell.filter == f, label).clicked() {
            shell.filter = f;
        }
    }
    ui.add_space(6.0);
    ui.label(RichText::new("A filter keeps the summary tasks above each match.").color(t.text_dim).size(12.0));
}

fn recent(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    section(ui, t, "Projects");
    let mut open = None;
    for r in &shell.prefs.recent {
        let name = r.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if ui.selectable_label(false, name).on_hover_text(r.display().to_string()).clicked() {
            open = Some(r.clone());
        }
    }
    if shell.prefs.recent.is_empty() {
        ui.label(RichText::new("Projects you open appear here.").color(t.text_dim).size(12.0));
    }
    if let Some(r) = open {
        shell.open_project(&r);
    }
}

/// Settings for this device: the RevuCraft link, and each project's linked drawings.
fn settings(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    section(ui, t, "RevuCraft");
    let mut on = shell.revucraft_link();
    if ui.checkbox(&mut on, "Link to RevuCraft").changed() {
        shell.set_revucraft_link(on);
    }
    ui.label(
        RichText::new("Each device has its own setting. When it's on, PlanCraft reads the punch list (markups.json) of each linked drawing, and punch items become tasks.")
            .color(t.text_dim)
            .size(12.0),
    );
    let mut unlink: Option<(usize, String)> = None;
    for (i, p) in shell.projects.iter().enumerate() {
        section(ui, t, &format!("Linked drawings: {}", p.title()));
        if p.project.drawings.is_empty() {
            ui.label(RichText::new("None. Use Link drawing on the RevuCraft ribbon group.").color(t.text_dim).size(12.0));
        }
        for d in &p.project.drawings {
            ui.horizontal(|ui| {
                ui.label(RichText::new(d).size(12.5).color(t.text));
                if ui.small_button("Unlink").on_hover_text("Stop reading this drawing's punch list (its tasks stay, cancelled)").clicked() {
                    unlink = Some((i, d.clone()));
                }
            });
        }
    }
    if let Some((i, path)) = unlink {
        shell.run(i, "revucraft.removeDrawing", serde_json::json!({ "path": path }));
    }
}

fn sync(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(format!("This device: {}", shell.device())).color(t.text));
    ui.label(
        RichText::new("Each device writes only its own file; PlanCraft merges them and checks for new ones every 2 seconds.")
            .color(t.text_dim)
            .size(12.0),
    );
    for p in &shell.projects {
        section(ui, t, &p.title());
        match p.folder().device_files() {
            Ok(files) => {
                for f in files {
                    let me = f.device == shell.device();
                    let age = f.modified.and_then(|m| m.elapsed().ok()).map(ago).unwrap_or_else(|| "?".into());
                    let line = format!("{}{}  ·  {:.0} KB  ·  {age}", f.device, if me { " (this device)" } else { "" }, f.bytes as f64 / 1024.0);
                    ui.label(RichText::new(line).size(12.5).color(if me { t.text } else { t.text_dim }));
                }
            }
            Err(e) => {
                ui.label(RichText::new(e.to_string()).color(t.danger));
            }
        }
        for w in p.folder().warnings() {
            ui.label(RichText::new(w).size(12.0).color(t.danger));
        }
        let problems = p.project.problems();
        if !problems.is_empty() {
            ui.label(RichText::new(format!("{} thing(s) to check after merging:", problems.len())).size(12.0).color(t.text));
            for x in problems.iter().take(8) {
                ui.label(RichText::new(format!("• {x}")).size(12.0).color(t.text_dim));
            }
        }
        for w in &p.schedule.warnings {
            ui.label(RichText::new(format!("• {w}")).size(12.0).color(t.text_dim));
        }
    }
}

fn ago(d: std::time::Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}
