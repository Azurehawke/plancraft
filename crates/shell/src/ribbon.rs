//! The ribbon for a project tab: Task, Schedule, Resource, View and Project groups. Every button
//! runs a project command (or a view change), so it does what the CLI and agents can do.

use crate::toolbar::{big as big_button, small as small_button};
use egui::{RichText, Ui};
use gridcraft_ui_egui::icons::Icon;
use gridcraft_ui_egui::theme::Tokens;
use serde_json::json;

use crate::{Request, Shell, Tab, View};

/// The Tool Properties bar: the mode switch, the file group, then the tools for the selected view.
/// Groups that do not apply to the selected view are greyed out, not hidden.
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    egui::Panel::top("pc_ribbon").exact_size(96.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(8, 4))).show(
        ui,
        |ui| {
            egui::ScrollArea::horizontal().id_salt("pc_ribbon_scroll").show(ui, |ui| {
                ui.horizontal(|ui| {
                    mode_group(shell, ui, t);
                    sep(ui, t);
                    file_group(shell, ui, t);
                    sep(ui, t);
                    let Some(pi) = shell.active_project() else { return };
                    let focus = shell.focused_tab();
                    let editing = !shell.read_mode;
                    if focus == Tab::Documents {
                        docs_tools(shell, ui, t, pi, editing);
                        sep(ui, t);
                        plan_tools(shell, ui, t, pi, false);
                    } else {
                        plan_tools(shell, ui, t, pi, editing && matches!(focus, Tab::View(_)));
                        sep(ui, t);
                        docs_tools(shell, ui, t, pi, false);
                    }
                });
            });
        },
    );
}

/// The document tools: the document group and the format group.
fn docs_tools(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        document_group(shell, ui, t, pi);
        sep(ui, t);
        format_group(shell, ui, t);
    });
}

/// The plan tools: task, schedule, view and the RevuCraft group.
fn plan_tools(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        task_group(shell, ui, t, pi);
        sep(ui, t);
        schedule_group(shell, ui, t, pi);
        sep(ui, t);
        view_group(shell, ui, t, pi);
        sep(ui, t);
        revucraft_group(shell, ui, t);
    });
}

/// The Plan / Read switch: Read greys the editing tools and leaves the window for reading.
fn mode_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    group(ui, t, "Mode", |ui| {
        ui.vertical(|ui| {
            if ui.selectable_label(!shell.read_mode, "Plan").on_hover_text("Edit the plan, with the tools").clicked() {
                shell.read_mode = false;
            }
            if ui.selectable_label(shell.read_mode, "Read").on_hover_text("Read the plan: the editing tools are greyed out").clicked() {
                shell.read_mode = true;
            }
        });
    });
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
            if small_button(ui, Icon::Info, "Information", "Open the Properties tab: the project's information", false).clicked() {
                if let Some(p) = shell.active_project().and_then(|i| shell.projects.get_mut(i)) {
                    p.selected.clear();
                }
                shell.open_inspector();
            }
            if small_button(ui, Icon::Sheet, "Spreadsheet", "Open the spreadsheet tab", false).clicked() {
                shell.request(Request::OpenSpreadsheet(None));
            }
        });
    });
}

/// The ribbon while a Documents tab is focused: the document commands, in place of the task
/// and schedule commands. The document list on the left does the typing.
fn document_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens, pi: usize) {
    let selected = shell.documents.selected.clone();
    group(ui, t, "Document", |ui| {
        if big_button(ui, Icon::NewFile, "New\nDocument", "Make a new Markdown document in the project", false).clicked() {
            crate::documents::new_untitled(shell, pi);
        }
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if big_button(ui, Icon::Note, "Rename", "Rename the open document", false).clicked()
                && let Some(name) = &selected
            {
                shell.documents.rename_to = Some(name.strip_suffix(".md").unwrap_or(name).to_string());
            }
            if big_button(ui, Icon::Delete, "Delete", "Delete the open document (asks first)", false).clicked() {
                shell.documents.confirm_delete = true;
            }
        });
        let preview = shell.documents.preview;
        let (label, tip) = if preview { ("Edit", "Back to the text") } else { ("Preview", "Show the document as it will read") };
        if big_button(ui, Icon::Table, label, tip, false).clicked() {
            shell.documents.preview = !preview;
        }
    });
}

/// One Markdown command button in the Format group: it acts on the open document's selection.
fn fmt_button(shell: &mut Shell, ui: &mut Ui, label: &str, tip: &str, f: fn(&str, usize, usize) -> crate::markdown::Edit) {
    if ui.add(egui::Button::new(RichText::new(label).size(12.0)).min_size(egui::vec2(34.0, 20.0))).on_hover_text(tip).clicked() {
        crate::documents::apply(shell, ui.ctx(), f);
    }
}

/// The Format group while a document is open: the Markdown commands on the editor's selection,
/// after Obsidian's editing toolbar. Off in the reading view.
fn format_group(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    use crate::markdown::{self as md, BOLD, CODE, HIGHLIGHT, ITALIC, ListKind, STRIKE};
    let open = shell.documents.selected.is_some() && !shell.documents.preview;
    group(ui, t, "Format", |ui| {
        ui.add_enabled_ui(open, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "B", "Bold", |t, s, e| md::wrap(t, s, e, BOLD));
                    fmt_button(shell, ui, "I", "Italic", |t, s, e| md::wrap(t, s, e, ITALIC));
                    fmt_button(shell, ui, "S", "Strikethrough", |t, s, e| md::wrap(t, s, e, STRIKE));
                });
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "`", "Code", |t, s, e| md::wrap(t, s, e, CODE));
                    fmt_button(shell, ui, "==", "Highlight", |t, s, e| md::wrap(t, s, e, HIGHLIGHT));
                    fmt_button(shell, ui, "Clear", "Clear formatting", md::clear);
                });
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "H1", "Heading 1", |t, s, e| md::set_heading(t, s, e, 1));
                    fmt_button(shell, ui, "H2", "Heading 2", |t, s, e| md::set_heading(t, s, e, 2));
                    fmt_button(shell, ui, "H3", "Heading 3", |t, s, e| md::set_heading(t, s, e, 3));
                });
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "•", "Bulleted list", |t, s, e| md::toggle_list(t, s, e, ListKind::Bullet));
                    fmt_button(shell, ui, "1.", "Numbered list", |t, s, e| md::toggle_list(t, s, e, ListKind::Number));
                    fmt_button(shell, ui, "[ ]", "Checklist", |t, s, e| md::toggle_list(t, s, e, ListKind::Task));
                });
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "Link", "Link (the address is selected to type over)", |t, s, e| md::link(t, s, e, false));
                    fmt_button(shell, ui, "[[ ]]", "Link to another document", |t, s, e| md::link(t, s, e, true));
                    fmt_button(shell, ui, "\u{201C}", "Quote", md::quote);
                });
                ui.horizontal(|ui| {
                    fmt_button(shell, ui, "```", "Code block", md::code_block);
                    fmt_button(shell, ui, "—", "Horizontal rule", md::horizontal_rule);
                });
            });
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Copy format").size(12.0)))
                        .on_hover_text("Copy the formatting at the selection")
                        .clicked()
                    {
                        let (s, e) = crate::documents::selection(ui.ctx(), shell);
                        shell.documents.painter = Some(md::marks_around(&shell.documents.text, s, e));
                    }
                    let paste = shell.documents.painter.clone();
                    ui.add_enabled_ui(paste.is_some(), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Paste format").size(12.0)))
                            .on_hover_text("Apply the copied formatting to the selection")
                            .clicked()
                            && let Some(marks) = paste
                        {
                            crate::documents::apply(shell, ui.ctx(), |t, s, e| md::apply_marks(t, s, e, &marks));
                        }
                    });
                });
            });
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
            Tab::Spreadsheet | Tab::Documents | Tab::Inspector => View::Gantt,
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

pub(crate) fn link_selected(shell: &mut Shell, pi: usize, unlink: bool) {
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
