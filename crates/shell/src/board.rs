//! The Board (Kanban) view: one card per task (summary tasks are left out), in columns that are
//! the project's board columns. Drag a card to another column to move it (moving into the last
//! column marks it done; moving out of it from 100% resets progress). Click a card to see it in
//! the Inspector; "+ Add card" adds a task straight into a column.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use gridcraft_ui_egui::theme::Tokens;
use plancraft_project::{Project, Task, TaskId};
use serde_json::json;

use crate::{Shell, critical_color};

const COL_W: f32 = 270.0;
const CARD_GAP: f32 = 8.0;

/// What dragging a card carries.
#[derive(Debug, Clone)]
struct Card(TaskId);

pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, pi: usize) {
    let t = Tokens::get(ui.ctx());
    let today = shell.today;
    let Some(p) = shell.projects.get_mut(pi) else { return };
    let project = p.project.clone();
    let columns = project.columns();
    let mut actions: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let mut select: Option<TaskId> = None;

    // Toolbar: which tasks, and the columns' names.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let summaries: Vec<(TaskId, String)> =
            project.tasks.iter().filter(|x| x.summary).map(|x| (x.id.clone(), format!("{} {}", x.wbs, x.name))).collect();
        let current = p
            .board_under
            .as_ref()
            .and_then(|id| summaries.iter().find(|(s, _)| s == id))
            .map(|(_, l)| l.clone())
            .unwrap_or_else(|| "All tasks".into());
        craft_ui::ComboBox::new(("pc_board_under", pi), "Show").selected_text(current).width(240.0).show_ui(ui, |ui| {
            if ui.selectable_label(p.board_under.is_none(), "All tasks").clicked() {
                p.board_under = None;
            }
            for (id, label) in &summaries {
                if ui.selectable_label(p.board_under.as_ref() == Some(id), label).clicked() {
                    p.board_under = Some(id.clone());
                }
            }
        });
        ui.add_space(16.0);
        let key = ui.scope_id().with(("pc_board_cols", pi));
        let joined = columns.join(", ");
        let mut text: String = ui.ctx().data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| joined.clone());
        let l = ui.label("Columns");
        let r = ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(360.0).background_color(t.input_bg))
            .labelled_by(l.id)
            .on_hover_text("Comma-separated, left to right; the last column means done");
        if r.has_focus() {
            ui.ctx().data_mut(|d| d.insert_temp(key, text.clone()));
        } else {
            ui.ctx().data_mut(|d| d.remove::<String>(key));
        }
        if r.lost_focus() && text != joined {
            let cols: Vec<&str> = text.split(',').map(str::trim).filter(|c| !c.is_empty()).collect();
            actions.push(("board.setColumns", json!({ "columns": cols })));
        }
    });
    ui.add_space(6.0);

    let under = p.board_under.clone();
    let cards: Vec<&Task> =
        project.tasks.iter().filter(|x| !x.summary && !x.cancelled).filter(|x| under.as_ref().is_none_or(|u| is_under(&project, &x.id, u))).collect();
    let selected = p.selected.clone();
    let schedule = p.schedule.clone();

    egui::ScrollArea::horizontal().id_salt(("pc_board_scroll", pi)).auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.add_space(10.0);
            for col in &columns {
                let in_col: Vec<&&Task> = cards.iter().filter(|x| project.status_of(x) == *col).collect();
                let frame =
                    egui::Frame::NONE.fill(t.ribbon).corner_radius(8).inner_margin(egui::Margin::same(8)).stroke(Stroke::new(1.0, t.separator));
                let (_, dropped) = ui.dnd_drop_zone::<Card, _>(frame, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(COL_W);
                        ui.set_min_height(ui.available_height().max(200.0) - 30.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(col).strong().color(t.text));
                            ui.label(RichText::new(in_col.len().to_string()).color(t.text_dim));
                        });
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .id_salt(("pc_board_col", pi, col.as_str()))
                            .auto_shrink([false, true])
                            .max_height(ui.available_height() - 34.0)
                            .show(ui, |ui| {
                                for task in &in_col {
                                    let is_sel = selected.contains(&task.id);
                                    let resp = ui.dnd_drag_source(ui.scope_id().with(("pc_card", &task.id.0)), Card(task.id.clone()), |ui| {
                                        card(ui, &t, &project, task, schedule.get(&task.id), is_sel, today);
                                    });
                                    if resp.response.clicked() {
                                        select = Some(task.id.clone());
                                    }
                                    ui.add_space(CARD_GAP);
                                }
                            });
                        if ui.add(egui::Button::new(RichText::new("+ Add card").color(t.text_dim)).frame(false)).clicked() {
                            actions.push(("__add", json!({ "status": col })));
                        }
                    });
                });
                if let Some(c) = dropped {
                    let Card(id) = &*c;
                    let now = project.task(id).map(|x| project.status_of(x));
                    if now.as_deref() != Some(col.as_str()) {
                        actions.push(("task.setStatus", json!({ "task": id.0, "status": col })));
                    }
                }
                ui.add_space(10.0);
            }
        });
    });

    if let Some(id) = select
        && let Some(p) = shell.projects.get_mut(pi)
    {
        p.select_only(id);
    }
    for (cmd, params) in actions {
        if cmd == "__add" {
            add_card(shell, pi, params.get("status").and_then(|s| s.as_str()).unwrap_or(""));
        } else {
            shell.run(pi, cmd, params);
        }
    }
}

/// A card: name, where it sits in the outline, due date, people, progress.
fn card(
    ui: &mut Ui,
    t: &Tokens,
    project: &Project,
    task: &Task,
    d: Option<&plancraft_schedule::TaskDates>,
    selected: bool,
    today: plancraft_project::Date,
) {
    let width = COL_W - 4.0;
    let crit = d.is_some_and(|d| d.critical);
    let late = d.is_some_and(|d| d.finish < today) && task.percent_complete < 100;
    let frame = egui::Frame::NONE
        .fill(t.window)
        .corner_radius(6)
        .inner_margin(egui::Margin { left: 12, right: 10, top: 8, bottom: 8 })
        .stroke(Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { t.accent } else { t.separator }));
    let inner = frame.show(ui, |ui| {
        ui.set_width(width - 22.0);
        let parent = task.parent.as_ref().and_then(|pid| project.task(pid)).map(|x| x.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&task.wbs).size(11.0).color(t.text_dim));
            if !parent.is_empty() {
                ui.label(RichText::new(parent).size(11.0).color(t.text_dim));
            }
        });
        let name = if task.name.is_empty() { "(no name)".to_string() } else { task.name.clone() };
        ui.add(egui::Label::new(RichText::new(name).size(14.0).color(t.text)).wrap());
        ui.horizontal(|ui| {
            if let Some(d) = d {
                let due = if task.milestone || d.duration_days == 0 { format!("Milestone {}", d.start) } else { format!("Due {}", d.finish) };
                ui.label(RichText::new(due).size(11.5).color(if late { critical_color(t) } else { t.text_dim }));
            }
            for a in project.assignments.iter().filter(|a| a.task == task.id) {
                if let Some(r) = project.resources.iter().find(|r| r.id == a.resource) {
                    chip(ui, t, &r.name, &r.color);
                }
            }
        });
        if task.percent_complete > 0 {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, t.separator);
            let w = rect.width() * f32::from(task.percent_complete.min(100)) / 100.0;
            ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(w, rect.height())), 2.0, t.accent);
        }
    });
    let r = inner.response.rect;
    if crit {
        ui.painter().rect_filled(
            Rect::from_min_max(r.min, pos2(r.left() + 4.0, r.bottom())),
            egui::CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 },
            critical_color(t),
        );
    }
}

/// A person's initials in their Gantt colour (or the accent).
fn chip(ui: &mut Ui, t: &Tokens, name: &str, color: &str) {
    let fill = parse_hex(color).unwrap_or(t.accent_soft);
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 10.5, fill);
    ui.painter().circle_stroke(rect.center(), 10.5, Stroke::new(1.0, t.separator));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, initials, FontId::proportional(10.0), t.text);
    resp.on_hover_text(name);
}

fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn is_under(project: &Project, id: &TaskId, ancestor: &TaskId) -> bool {
    let mut cur = project.task(id).and_then(|x| x.parent.as_ref());
    for _ in 0..=project.tasks.len() {
        match cur {
            Some(c) if c == ancestor => return true,
            Some(c) => cur = project.task(c).and_then(|x| x.parent.as_ref()),
            None => return false,
        }
    }
    false
}

/// "+ Add card": a new task (under the summary the board is showing), straight into `status`.
fn add_card(shell: &mut Shell, pi: usize, status: &str) {
    let Some(p) = shell.projects.get(pi) else { return };
    let n = p.project.tasks.len() + 1;
    let mut params = json!({ "name": format!("Task {n}"), "duration": "1d" });
    if let Some(u) = &p.board_under {
        params["under"] = json!(u.0);
    }
    let Some(v) = shell.run(pi, "task.add", params) else { return };
    let Some(id) = v.get("id").and_then(|x| x.as_str()).map(str::to_string) else { return };
    shell.run(pi, "task.setStatus", json!({ "task": id, "status": status }));
    if let Some(p) = shell.projects.get_mut(pi) {
        p.select_only(TaskId(id));
    }
    shell.right_open = true;
}
