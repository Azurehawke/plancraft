//! The ribbon for a project tab: Task, Schedule, Resource, View and Project groups. Every button
//! runs a project command (or a view change), so it does what the CLI and agents can do.

use egui::{RichText, Ui};
use gridcraft_ui_egui::icons::Icon;
use gridcraft_ui_egui::theme::Tokens;
use gridcraft_ui_egui::widgets::{big_button, small_button};
use serde_json::json;

use crate::{Request, Shell, Tab, View};

pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    egui::Panel::top("pc_ribbon").exact_size(96.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(8, 4))).show(
        ui,
        |ui| {
            egui::ScrollArea::horizontal().id_salt("pc_ribbon_scroll").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let Some(pi) = shell.active_project() else {
                        file_group(shell, ui, t);
                        return;
                    };
                    file_group(shell, ui, t);
                    sep(ui, t);
                    task_group(shell, ui, t, pi);
                    sep(ui, t);
                    schedule_group(shell, ui, t, pi);
                    sep(ui, t);
                    view_group(shell, ui, t, pi);
                    sep(ui, t);
                    revucraft_group(shell, ui, t);
                });
            });
        },
    );
}

fn sep(ui: &mut Ui, t: &Tokens) {
    let r = ui.available_rect_before_wrap();
    ui.add_space(4.0);
    ui.painter().vline(ui.cursor().left(), r.y_range().shrink(6.0), egui::Stroke::new(1.0, t.separator));
    ui.add_space(6.0);
}

fn group(ui: &mut Ui, t: &Tokens, name: &str, body: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        let inner = ui.horizontal(|ui| {
            ui.set_min_height(72.0);
            body(ui)
        });
        let w = inner.response.rect.width();
        ui.allocate_ui_with_layout(egui::vec2(w, 14.0), egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.label(RichText::new(name).size(11.0).color(t.text_dim));
        });
    });
}

fn file_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    group(ui, t, "Project", |ui| {
        if big_button(ui, Icon::NewFile, "New\nProject", "Start a new project (wizard)", false).clicked() {
            shell.start_wizard();
        }
        if big_button(ui, Icon::OpenFolder, "Open\nProject", "Open a .plancraft project", false).clicked() {
            shell.request(Request::OpenProject);
        }
        ui.vertical(|ui| {
            if small_button(ui, Icon::Info, "Information", "Project information in the Inspector", false).clicked() {
                if let Some(p) = shell.active_project().and_then(|i| shell.projects.get_mut(i)) {
                    p.selected.clear();
                }
                shell.right_open = true;
            }
            if small_button(ui, Icon::Sheet, "Spreadsheet", "Open the spreadsheet tab", false).clicked() {
                shell.request(Request::OpenSpreadsheet(None));
            }
        });
    });
}

fn task_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize) {
    let current = shell.projects.get(pi).and_then(|p| p.current().cloned());
    let has = current.is_some();
    group(ui, t, "Task", |ui| {
        if big_button(ui, Icon::Plus, "New\nTask", "Add a task below the selected one (Insert)", false).clicked() {
            add_task(shell, pi, false);
        }
        if big_button(ui, Icon::Symbol, "Mile-\nstone", "Add a milestone below the selected task", false).clicked() {
            add_task(shell, pi, true);
        }
        ui.vertical(|ui| {
            ui.add_enabled_ui(has, |ui| {
                if small_button(ui, Icon::IndentInc, "Indent", "Make it a subtask of the task above (Alt+Shift+→)", false).clicked() {
                    on_current(shell, pi, "task.indent");
                }
                if small_button(ui, Icon::IndentDec, "Outdent", "Move it up one outline level (Alt+Shift+←)", false).clicked() {
                    on_current(shell, pi, "task.outdent");
                }
                if small_button(ui, Icon::Delete, "Delete", "Delete the selected tasks (Delete)", false).clicked() {
                    delete_selected(shell, pi);
                }
            });
        });
        ui.vertical(|ui| {
            ui.add_enabled_ui(has, |ui| {
                if small_button(ui, Icon::ChevronUp, "Move Up", "Move it above the task before it", false).clicked() {
                    on_current(shell, pi, "task.moveUp");
                }
                if small_button(ui, Icon::Chevron, "Move Down", "Move it below the task after it", false).clicked() {
                    on_current(shell, pi, "task.moveDown");
                }
            });
        });
    });
}

fn schedule_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize) {
    let n = shell.projects.get(pi).map(|p| p.selected.len()).unwrap_or(0);
    let current = shell.projects.get(pi).and_then(|p| p.current().cloned());
    group(ui, t, "Schedule", |ui| {
        ui.add_enabled_ui(n >= 2, |ui| {
            if big_button(
                ui,
                Icon::Link,
                "Link\nTasks",
                "Link the selected tasks finish-to-start, in the order you selected them (Ctrl+click)",
                false,
            )
            .clicked()
            {
                link_selected(shell, pi, false);
            }
        });
        ui.vertical(|ui| {
            ui.add_enabled_ui(n >= 2, |ui| {
                if small_button(ui, Icon::Close, "Unlink", "Remove links between the selected tasks", false).clicked() {
                    link_selected(shell, pi, true);
                }
            });
            ui.add_enabled_ui(current.is_some(), |ui| {
                ui.horizontal(|ui| {
                    for pct in [0, 25, 50, 75, 100] {
                        if ui
                            .add(egui::Button::new(RichText::new(format!("{pct}%")).size(11.0)).small())
                            .on_hover_text(format!("Mark {pct}% complete"))
                            .clicked()
                            && let Some(id) = &current
                        {
                            shell.run(pi, "task.set", json!({"task": id.0, "field": "percent", "value": pct.to_string()}));
                        }
                    }
                });
            });
        });
    });
}

fn view_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize) {
    group(ui, t, "View", |ui| {
        let current = match shell.focused_tab() {
            Tab::View(v) => v,
            Tab::Spreadsheet | Tab::Documents => View::Gantt,
        };
        ui.vertical(|ui| {
            let mut chosen = current;
            craft_ui::ComboBox::new("pc_view", "View").selected_text(current.label()).width(150.0).show_ui(ui, |ui| {
                for v in View::ALL {
                    let label = if v.available() { v.label().to_string() } else { format!("{} (later)", v.label()) };
                    ui.selectable_value(&mut chosen, v, label);
                }
            });
            if chosen != current {
                shell.set_focused_tab(Tab::View(chosen));
            }
            ui.horizontal(|ui| {
                let Some(p) = shell.projects.get_mut(pi) else { return };
                if ui.button("-").on_hover_text("Zoom out").clicked() {
                    p.day_px = (p.day_px / 1.4).max(3.0);
                }
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    p.day_px = (p.day_px * 1.4).min(90.0);
                }
                if ui.button("Today").on_hover_text("Scroll the chart to today").clicked() {
                    p.scroll_days = p.schedule.start.days_until(shell.today) as f32 - 3.0;
                }
            });
        });
        ui.vertical(|ui| {
            craft_ui::ComboBox::new("pc_filter", "Filter").selected_text(shell.filter.label()).width(150.0).show_ui(ui, |ui| {
                for f in crate::Filter::ALL {
                    ui.selectable_value(&mut shell.filter, f, f.label());
                }
            });
        });
    });
}

fn revucraft_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    let on = shell.revucraft_link();
    group(ui, t, "RevuCraft", |ui| {
        ui.add_enabled_ui(on, |ui| {
            if big_button(ui, Icon::OpenFolder, "Link\ndrawing", "Link a drawing in the job folder: its punch items become tasks", false).clicked() {
                shell.request(Request::LinkDrawing);
            }
        });
        if !on {
            ui.label(RichText::new("Turn it on in Settings").size(11.0).color(t.text_dim));
        }
    });
}

pub(crate) fn add_task(shell: &mut Shell, pi: usize, milestone: bool) {
    let Some(p) = shell.projects.get(pi) else { return };
    let after = p.current().map(|id| id.0.clone());
    let n = p.project.tasks.len() + 1;
    let name = if milestone { "New milestone".to_string() } else { format!("Task {n}") };
    let mut params = json!({ "name": name, "duration": if milestone { "" } else { "1d" } });
    if let Some(a) = after {
        params["after"] = json!(a);
    }
    let id = if milestone { "task.addMilestone" } else { "task.add" };
    if let Some(v) = shell.run(pi, id, params)
        && let (Some(p), Some(new)) = (shell.projects.get_mut(pi), v.get("id").and_then(|x| x.as_str()))
    {
        let new = plancraft_project::TaskId(new.to_string());
        p.select_only(new.clone());
        p.editing = Some((new, "name", name));
    }
}

pub(crate) fn on_current(shell: &mut Shell, pi: usize, cmd: &str) {
    let Some(id) = shell.projects.get(pi).and_then(|p| p.current().cloned()) else { return };
    shell.run(pi, cmd, json!({ "task": id.0 }));
}

pub(crate) fn delete_selected(shell: &mut Shell, pi: usize) {
    let ids: Vec<String> = shell.projects.get(pi).map(|p| p.selected.iter().map(|s| s.0.clone()).collect()).unwrap_or_default();
    for id in ids {
        // A subtask of a task deleted a moment ago is already gone: ignore that error.
        let exists = shell.projects.get(pi).is_some_and(|p| p.project.tasks.iter().any(|t| t.id.0 == id));
        if exists {
            shell.run(pi, "task.delete", json!({ "task": id }));
        }
    }
}

fn link_selected(shell: &mut Shell, pi: usize, unlink: bool) {
    let ids: Vec<String> = shell.projects.get(pi).map(|p| p.selected.iter().map(|s| s.0.clone()).collect()).unwrap_or_default();
    for pair in ids.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        if unlink {
            let _ = shell.projects.get_mut(pi).map(|p| p.run("task.unlink", &json!({"from": a, "to": b}), shell.today));
            let _ = shell.projects.get_mut(pi).map(|p| p.run("task.unlink", &json!({"from": b, "to": a}), shell.today));
        } else {
            shell.run(pi, "task.link", json!({ "from": a, "to": b }));
        }
    }
}
