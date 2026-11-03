//! The Gantt Chart view: a task table on the left and a timescale with bars on the right.
//!
//! - Click selects a task (Ctrl+click adds to the selection, for Link Tasks); double-click a
//!   name, duration or % cell to edit it in place (F2 edits the name).
//! - Keys: ↑/↓ move the selection, Insert adds a task, Delete deletes, Alt+Shift+→/← indent and
//!   outdent.
//! - Drag a bar to move its start (sets "start no earlier than", as MS Project does); drag its
//!   right end to change its duration. Shift+wheel or drag the background scrolls the timescale,
//!   Ctrl+wheel zooms.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_ui_egui::theme::Tokens;
use plancraft_project::{Date, DependencyKind, Task, TaskId};
use serde_json::json;

use crate::{ROW_H, Shell, critical_color, ribbon};

const HEADER_H: f32 = 40.0;
/// Columns of the task table: (key, title, width; 0 = takes the remaining width).
const COLS: [(&str, &str, f32); 6] = [
    ("wbs", "WBS", 46.0),
    ("name", "Task Name", 0.0),
    ("duration", "Duration", 66.0),
    ("start", "Start", 84.0),
    ("finish", "Finish", 84.0),
    ("percent", "%", 40.0),
];

pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, pi: usize) {
    let t = Tokens::get(ui.ctx());
    let filter = shell.filter;
    let today = shell.today;
    let Some(p) = shell.projects.get_mut(pi) else { return };
    let rows = p.visible_rows(filter);
    let full = ui.available_rect_before_wrap();
    let table_w = p.table_w.clamp(260.0, (full.width() - 120.0).max(260.0));
    let chart_left = full.left() + table_w + 4.0;

    // Splitter between table and chart.
    let split = Rect::from_min_max(pos2(full.left() + table_w, full.top()), pos2(chart_left, full.bottom()));
    let sr = ui.interact(split, ui.scope_id().with(("pc_split", pi)), Sense::drag());
    if sr.dragged() {
        p.table_w = (p.table_w + sr.drag_delta().x).clamp(260.0, 1200.0);
    }
    ui.painter().rect_filled(split, 0.0, if sr.hovered() || sr.dragged() { t.accent_soft } else { t.separator });
    if sr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    // Header.
    let header = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    let painter = ui.painter_at(header);
    painter.rect_filled(header, 0.0, t.header_bg);
    let mut x = full.left();
    let name_w = (table_w - COLS.iter().map(|c| c.2).sum::<f32>()).max(80.0);
    for (_, title, w) in COLS {
        let w = if w == 0.0 { name_w } else { w };
        painter.text(pos2(x + 6.0, header.bottom() - 12.0), Align2::LEFT_CENTER, title, FontId::proportional(12.5), t.header_text);
        x += w;
        painter.vline(x, header.y_range(), Stroke::new(1.0, t.header_line));
    }
    let chart_rect_h = Rect::from_min_max(pos2(chart_left, header.top()), pos2(full.right(), header.bottom()));
    timescale(ui, &t, p, chart_rect_h);
    painter.hline(full.x_range(), header.bottom(), Stroke::new(1.0, t.header_line));

    // Body.
    let body = Rect::from_min_max(pos2(full.left(), header.bottom()), full.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    let mut actions: Vec<Action> = Vec::new();
    egui::ScrollArea::vertical().id_salt(("pc_gantt_scroll", pi)).auto_shrink([false, false]).show(&mut child, |ui| {
        let total_h = (rows.len() as f32 * ROW_H).max(body.height() - 2.0);
        let (area, bg) = ui.allocate_exact_size(vec2(full.width(), total_h), Sense::click_and_drag());
        let ctx = Ctx { t: &t, area, table_w, name_w, chart_left, today };
        draw_rows(ui, &ctx, p, &rows, &mut actions);
        draw_links(ui, &ctx, p, &rows);
        chart_scroll(ui, &ctx, p, &bg);
        if bg.clicked()
            && !ui.input(|i| i.modifiers.command)
            && let Some(pos) = bg.interact_pointer_pos()
            && pos.x < chart_left
            && ((pos.y - area.top()) / ROW_H) as usize >= rows.len()
        {
            p.selected.clear();
        }
    });
    keyboard(ui, p, &rows, &mut actions);
    for a in actions {
        apply(shell, pi, a);
    }
    if shell.projects.get(pi).is_some_and(|p| p.project.tasks.is_empty()) {
        let c = full.center();
        ui.painter().text(
            c,
            Align2::CENTER_CENTER,
            "No tasks yet. Click New Task (or press Insert) to add the first one.",
            FontId::proportional(14.0),
            t.text_dim,
        );
    }
}

struct Ctx<'a> {
    t: &'a Tokens,
    area: Rect,
    table_w: f32,
    name_w: f32,
    chart_left: f32,
    today: Date,
}

enum Action {
    Run(&'static str, serde_json::Value),
    Edit(TaskId, &'static str, String),
    Commit,
    Cancel,
    AddTask,
    Delete,
}

fn x_of(p: &crate::OpenProject, chart_left: f32, date: Date) -> f32 {
    let d = p.schedule.start.days_until(date) as f32;
    chart_left + (d - p.scroll_days) * p.day_px
}

fn timescale(ui: &Ui, t: &Tokens, p: &crate::OpenProject, r: Rect) {
    let painter = ui.painter_at(r);
    let days = ((r.width() / p.day_px).ceil() as i64).clamp(1, 4000);
    let first = p.scroll_days.floor() as i64;
    let mid = r.top() + 16.0;
    painter.hline(r.x_range(), mid + 4.0, Stroke::new(1.0, t.header_line));
    for k in 0..=days {
        let Some(d) = p.schedule.start.add_days(first + k) else { continue };
        let x = x_of(p, r.left(), d);
        let wd = d.weekday_from_monday();
        // Top line: months when zoomed out, weeks (Mondays) otherwise.
        if d.day() == 1 || (p.day_px >= 12.0 && wd == 0) {
            painter.vline(x, r.y_range(), Stroke::new(1.0, t.header_line));
            let label =
                if p.day_px >= 12.0 { format!("{} {:02}-{:02}", if wd == 0 { "Week of" } else { "" }, d.month(), d.day()) } else { month_name(d) };
            painter.text(pos2(x + 4.0, r.top() + 9.0), Align2::LEFT_CENTER, label.trim(), FontId::proportional(11.5), t.header_text);
        }
        if p.day_px >= 16.0 {
            let c = if p.project.calendar.is_working_day(d) { t.header_text } else { t.text_disabled };
            painter.text(pos2(x + p.day_px / 2.0, r.bottom() - 9.0), Align2::CENTER_CENTER, d.day().to_string(), FontId::proportional(10.5), c);
        }
    }
}

fn month_name(d: Date) -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {}", M.get(d.month().saturating_sub(1) as usize).copied().unwrap_or(""), d.year())
}

fn draw_rows(ui: &mut Ui, c: &Ctx, p: &mut crate::OpenProject, rows: &[usize], actions: &mut Vec<Action>) {
    let t = c.t;
    let clip = ui.clip_rect();
    let chart = Rect::from_min_max(pos2(c.chart_left, c.area.top()), pos2(c.area.right(), c.area.bottom()));
    // Non-working days and today in the chart background.
    {
        let painter = ui.painter_at(chart.intersect(clip));
        let first = p.scroll_days.floor() as i64;
        let days = ((chart.width() / p.day_px).ceil() as i64).clamp(1, 4000);
        for k in 0..=days {
            let Some(d) = p.schedule.start.add_days(first + k) else { continue };
            if !p.project.calendar.is_working_day(d) && p.day_px >= 6.0 {
                let x = x_of(p, c.chart_left, d);
                painter.rect_filled(Rect::from_min_max(pos2(x, chart.top()), pos2(x + p.day_px, chart.bottom())), 0.0, t.header_bg);
            }
        }
        let tx = x_of(p, c.chart_left, c.today) + p.day_px / 2.0;
        painter.vline(tx, chart.y_range(), Stroke::new(1.5, t.accent));
    }
    let crit = critical_color(t);
    for (r, &i) in rows.iter().enumerate() {
        let Some(task) = p.project.tasks.get(i).cloned() else { continue };
        let y = c.area.top() + r as f32 * ROW_H;
        let row = Rect::from_min_size(pos2(c.area.left(), y), vec2(c.table_w, ROW_H));
        if !row.intersects(clip) && !Rect::from_min_size(pos2(c.chart_left, y), vec2(chart.width(), ROW_H)).intersects(clip) {
            continue;
        }
        let selected = p.selected.contains(&task.id);
        let painter = ui.painter();
        if selected {
            painter.rect_filled(row, 0.0, t.sel_fill);
        }
        painter.hline(row.x_range(), row.bottom(), Stroke::new(1.0, t.gridline));
        let d = p.schedule.get(&task.id).cloned();
        let mut x = row.left();
        for (key, _, w) in COLS {
            let w = if w == 0.0 { c.name_w } else { w };
            let cell = Rect::from_min_size(pos2(x, y), vec2(w, ROW_H));
            x += w;
            ui.painter().vline(cell.right(), cell.y_range(), Stroke::new(1.0, t.gridline));
            let editing = p.editing.as_ref().is_some_and(|(id, col, _)| *id == task.id && *col == key);
            if editing {
                edit_cell(ui, cell, p, actions);
                continue;
            }
            let resp = ui.interact(cell, ui.scope_id().with(("pc_cell", &task.id.0, key)), Sense::click());
            // Hover a task's name to peek at its notes, drawn as the Markdown preview.
            if key == "name" && !task.notes.trim().is_empty() {
                let notes = task.notes.clone();
                let _ = resp.clone().on_hover_ui(|ui| {
                    ui.set_max_width(340.0);
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| crate::markdown::show(ui, t, &notes));
                });
            }
            if resp.clicked() {
                if ui.input(|i| i.modifiers.command) {
                    if let Some(pos) = p.selected.iter().position(|s| *s == task.id) {
                        p.selected.remove(pos);
                    } else {
                        p.selected.push(task.id.clone());
                    }
                } else {
                    p.select_only(task.id.clone());
                }
                if p.editing.is_some() {
                    actions.push(Action::Commit);
                }
            }
            if resp.double_clicked() && matches!(key, "name" | "duration" | "percent" | "start") {
                actions.push(Action::Edit(task.id.clone(), key, cell_text(&task, d.as_ref(), key)));
            }
            draw_cell(ui, t, cell, &task, d.as_ref(), key, p, actions);
        }
        // The bar.
        let Some(d) = d else { continue };
        let row_mid = y + ROW_H / 2.0;
        let x0 = x_of(p, c.chart_left, d.start);
        let x1 = x_of(p, c.chart_left, d.finish) + p.day_px;
        let painter = ui.painter_at(chart.intersect(clip));
        let critical = d.critical && !task.summary;
        if task.summary {
            let bar = Rect::from_min_max(pos2(x0, row_mid - 4.0), pos2(x1, row_mid + 2.0));
            painter.rect_filled(bar, 1.0, t.text);
            for xe in [x0, x1] {
                painter.add(Shape::convex_polygon(
                    vec![
                        pos2(xe - 5.0 * (xe == x1) as i32 as f32, row_mid + 2.0),
                        pos2(xe + 5.0 * (xe == x0) as i32 as f32, row_mid + 2.0),
                        pos2(xe, row_mid + 8.0),
                    ],
                    t.text,
                    Stroke::NONE,
                ));
            }
        } else if task.milestone || d.duration_days == 0 {
            let cx = x0;
            let s = 7.0;
            painter.add(Shape::convex_polygon(
                vec![pos2(cx, row_mid - s), pos2(cx + s, row_mid), pos2(cx, row_mid + s), pos2(cx - s, row_mid)],
                if critical { crit } else { t.text },
                Stroke::NONE,
            ));
            painter.text(
                pos2(cx + 11.0, row_mid),
                Align2::LEFT_CENTER,
                format!("{:02}-{:02}", d.start.month(), d.start.day()),
                FontId::proportional(11.0),
                t.text_dim,
            );
        } else {
            let bar = Rect::from_min_max(pos2(x0 + 1.0, row_mid - 7.0), pos2(x1 - 1.0, row_mid + 7.0));
            let fill = if critical { crit } else { t.accent };
            painter.rect_filled(bar, 3.0, fill.gamma_multiply(0.85));
            if task.percent_complete > 0 {
                let pw = bar.width() * f32::from(task.percent_complete.min(100)) / 100.0;
                painter.rect_filled(Rect::from_min_size(pos2(bar.left(), row_mid - 2.5), vec2(pw, 5.0)), 1.0, Color32::from_black_alpha(140));
            }
            let names: Vec<&str> = p
                .project
                .assignments
                .iter()
                .filter(|a| a.task == task.id)
                .filter_map(|a| p.project.resources.iter().find(|r| r.id == a.resource).map(|r| r.name.as_str()))
                .collect();
            if !names.is_empty() {
                painter.text(pos2(bar.right() + 6.0, row_mid), Align2::LEFT_CENTER, names.join(", "), FontId::proportional(11.0), t.text_dim);
            }
            bar_drag(ui, p, &task, bar, c, actions);
        }
    }
}

fn cell_text(task: &Task, d: Option<&plancraft_schedule::TaskDates>, key: &str) -> String {
    match key {
        "wbs" => task.wbs.clone(),
        "name" => task.name.clone(),
        "duration" => {
            if task.summary {
                d.map(|d| format!("{}d", d.duration_days)).unwrap_or_default()
            } else if task.milestone {
                "0d".into()
            } else {
                let days = f64::from(task.duration_minutes) / f64::from(plancraft_project::MINUTES_PER_DAY);
                format!("{}d", (days * 100.0).round() / 100.0)
            }
        }
        "start" => d.map(|d| d.start.to_string()).unwrap_or_default(),
        "finish" => d.map(|d| d.finish.to_string()).unwrap_or_default(),
        "percent" => format!("{}%", task.percent_complete),
        _ => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    ui: &mut Ui,
    t: &Tokens,
    cell: Rect,
    task: &Task,
    d: Option<&plancraft_schedule::TaskDates>,
    key: &str,
    p: &mut crate::OpenProject,
    actions: &mut Vec<Action>,
) {
    let _ = actions;
    let painter = ui.painter_at(cell);
    let text = cell_text(task, d, key);
    let strong = task.summary;
    let color = if task.cancelled { t.text_disabled } else { t.cell_text };
    let font = FontId::proportional(13.0);
    if key == "name" {
        let indent = (task.outline_level.saturating_sub(1)) as f32 * 16.0;
        let mut tx = cell.left() + 6.0 + indent;
        if task.summary {
            let collapsed = p.collapsed.contains(&task.id);
            let tri = Rect::from_center_size(pos2(tx + 5.0, cell.center().y), vec2(14.0, 14.0));
            let r = ui.interact(tri, ui.scope_id().with(("pc_tri", &task.id.0)), Sense::click());
            let pts = if collapsed {
                vec![pos2(tri.left() + 3.0, tri.top() + 2.0), pos2(tri.right() - 3.0, tri.center().y), pos2(tri.left() + 3.0, tri.bottom() - 2.0)]
            } else {
                vec![pos2(tri.left() + 2.0, tri.top() + 4.0), pos2(tri.right() - 2.0, tri.top() + 4.0), pos2(tri.center().x, tri.bottom() - 3.0)]
            };
            ui.painter().add(Shape::convex_polygon(pts, t.text_dim, Stroke::NONE));
            if r.clicked() {
                if collapsed {
                    p.collapsed.remove(&task.id);
                } else {
                    p.collapsed.insert(task.id.clone());
                }
            }
            tx += 14.0;
        }
        let label = if task.name.is_empty() { "(no name)".to_string() } else { text };
        let galley = painter.layout_no_wrap(label, if strong { FontId::new(13.0, egui::FontFamily::Proportional) } else { font }, color);
        let pos = pos2(tx, cell.center().y - galley.size().y / 2.0);
        if strong {
            // Bold by drawing twice, a pixel apart (egui's default fonts have no bold face).
            painter.galley(pos + vec2(0.6, 0.0), galley.clone(), color);
        }
        painter.galley(pos, galley, color);
        if task.cancelled {
            painter.hline(tx..=cell.right() - 4.0, cell.center().y, Stroke::new(1.0, t.text_disabled));
        }
    } else {
        let align = if key == "wbs" { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER };
        let x = if key == "wbs" { cell.left() + 6.0 } else { cell.right() - 6.0 };
        painter.text(pos2(x, cell.center().y), align, text, font, if key == "wbs" { t.text_dim } else { color });
    }
}

fn edit_cell(ui: &mut Ui, cell: Rect, p: &mut crate::OpenProject, actions: &mut Vec<Action>) {
    let Some((_, key, text)) = p.editing.as_mut() else { return };
    let id = ui.scope_id().with(("pc_edit", *key));
    let resp =
        ui.put(cell.shrink(1.0), egui::TextEdit::singleline(text).id(id).font(FontId::proportional(13.0)).margin(egui::Margin::symmetric(4, 2)));
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    if resp.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            actions.push(Action::Cancel);
        } else {
            actions.push(Action::Commit);
        }
    }
}

fn bar_drag(ui: &mut Ui, p: &crate::OpenProject, task: &Task, bar: Rect, c: &Ctx, actions: &mut Vec<Action>) {
    let edge = Rect::from_min_max(pos2(bar.right() - 5.0, bar.top()), pos2(bar.right() + 3.0, bar.bottom()));
    let er = ui.interact(edge, ui.scope_id().with(("pc_bar_end", &task.id.0)), Sense::drag());
    let br = ui.interact(bar.shrink2(vec2(4.0, 0.0)), ui.scope_id().with(("pc_bar", &task.id.0)), Sense::click_and_drag());
    if er.hovered() || er.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    } else if br.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let total = |id: egui::Id| ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, 0.0f32));
    for (r, kind) in [(&er, "end"), (&br, "move")] {
        let acc = r.id.with("acc");
        if r.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(acc, 0.0f32));
        }
        if r.dragged() {
            ui.ctx().data_mut(|d| *d.get_temp_mut_or(acc, 0.0f32) += r.drag_delta().x);
            let dx = total(acc);
            let ghost = if kind == "end" { Rect::from_min_max(bar.min, pos2(bar.right() + dx, bar.bottom())) } else { bar.translate(vec2(dx, 0.0)) };
            ui.painter().rect_stroke(ghost, 3.0, Stroke::new(1.5, c.t.text), StrokeKind::Outside);
        }
        if r.drag_stopped() {
            let moved = (total(acc) / p.day_px).round() as i64;
            if moved != 0 {
                let Some(d) = p.schedule.get(&task.id) else { continue };
                if kind == "move" {
                    if let Some(new) = d.start.add_days(moved) {
                        actions.push(Action::Run("task.set", json!({"task": task.id.0, "field": "constraint", "value": "SNET"})));
                        actions.push(Action::Run("task.set", json!({"task": task.id.0, "field": "constraint_date", "value": new.to_string()})));
                    }
                } else {
                    // Calendar days dragged → working days, at least one.
                    let mut working = 0i64;
                    let end = d.finish.add_days(moved).unwrap_or(d.finish);
                    let mut cur = d.start;
                    for _ in 0..4000 {
                        if cur > end {
                            break;
                        }
                        if p.project.calendar.is_working_day(cur) {
                            working += 1;
                        }
                        let Some(n) = cur.add_days(1) else { break };
                        cur = n;
                    }
                    actions.push(Action::Run("task.set", json!({"task": task.id.0, "field": "duration", "value": format!("{}d", working.max(1))})));
                }
            }
        }
    }
    if br.clicked() {
        actions.push(Action::Run("__select", json!({"task": task.id.0})));
    }
}

fn draw_links(ui: &Ui, c: &Ctx, p: &crate::OpenProject, rows: &[usize]) {
    let row_of = |id: &TaskId| rows.iter().position(|&i| p.project.tasks.get(i).is_some_and(|t| t.id == *id));
    let chart = Rect::from_min_max(pos2(c.chart_left, c.area.top()), c.area.max);
    let painter = ui.painter_at(chart.intersect(ui.clip_rect()));
    let stroke = Stroke::new(1.2, c.t.text_dim);
    for d in &p.project.dependencies {
        let (Some(ra), Some(rb)) = (row_of(&d.predecessor), row_of(&d.successor)) else { continue };
        let (Some(da), Some(db)) = (p.schedule.get(&d.predecessor), p.schedule.get(&d.successor)) else { continue };
        let ya = c.area.top() + ra as f32 * ROW_H + ROW_H / 2.0;
        let yb = c.area.top() + rb as f32 * ROW_H + ROW_H / 2.0;
        let end_a = x_of(p, c.chart_left, da.finish) + if da.duration_days == 0 { 0.0 } else { p.day_px };
        let start_a = x_of(p, c.chart_left, da.start);
        let start_b = x_of(p, c.chart_left, db.start);
        let end_b = x_of(p, c.chart_left, db.finish) + if db.duration_days == 0 { 0.0 } else { p.day_px };
        let (from, to, into_left) = match d.kind {
            DependencyKind::Fs => (end_a, start_b, true),
            DependencyKind::Ss => (start_a, start_b, true),
            DependencyKind::Ff => (end_a, end_b, false),
            DependencyKind::Sf => (start_a, end_b, false),
        };
        let out = if matches!(d.kind, DependencyKind::Fs | DependencyKind::Ff) { from + 6.0 } else { from - 6.0 };
        let approach = if into_left { to - 8.0 } else { to + 8.0 };
        let ymid = if yb > ya { yb - ROW_H / 2.0 + 2.0 } else { yb + ROW_H / 2.0 - 2.0 };
        let pts: Vec<Pos2> = if (into_left && out <= approach) || (!into_left && out >= approach) {
            vec![pos2(from, ya), pos2(out, ya), pos2(out, yb), pos2(to, yb)]
        } else {
            vec![pos2(from, ya), pos2(out, ya), pos2(out, ymid), pos2(approach, ymid), pos2(approach, yb), pos2(to, yb)]
        };
        painter.add(Shape::line(pts, stroke));
        let dir = if into_left { 1.0 } else { -1.0 };
        painter.add(Shape::convex_polygon(
            vec![pos2(to, yb), pos2(to - 6.0 * dir, yb - 4.0), pos2(to - 6.0 * dir, yb + 4.0)],
            c.t.text_dim,
            Stroke::NONE,
        ));
    }
}

fn chart_scroll(ui: &Ui, c: &Ctx, p: &mut crate::OpenProject, bg: &egui::Response) {
    let chart = Rect::from_min_max(pos2(c.chart_left, c.area.top()), c.area.max);
    let hovered = ui.rect_contains_pointer(chart.intersect(ui.clip_rect()));
    if bg.dragged() && bg.interact_pointer_pos().is_some_and(|q| q.x >= c.chart_left) {
        p.scroll_days -= bg.drag_delta().x / p.day_px;
    }
    if hovered {
        let (scroll, zoom, shift) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.modifiers.shift));
        if zoom != 1.0 {
            p.day_px = (p.day_px * zoom).clamp(3.0, 90.0);
        }
        if shift && scroll.y != 0.0 {
            p.scroll_days -= scroll.y / p.day_px;
        }
        if scroll.x != 0.0 {
            p.scroll_days -= scroll.x / p.day_px;
        }
    }
    p.scroll_days = p.scroll_days.clamp(-4000.0, 40_000.0);
}

fn keyboard(ui: &Ui, p: &mut crate::OpenProject, rows: &[usize], actions: &mut Vec<Action>) {
    if p.editing.is_some() || ui.ctx().memory(|m| m.focused().is_some()) {
        return;
    }
    let (up, down, ins, del, f2, alt_shift, right, left) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Insert),
            i.key_pressed(egui::Key::Delete),
            i.key_pressed(egui::Key::F2),
            i.modifiers.alt && i.modifiers.shift,
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::ArrowLeft),
        )
    });
    let cur = p.current().and_then(|id| rows.iter().position(|&i| p.project.tasks.get(i).is_some_and(|t| t.id == *id)));
    if up || down {
        let next = match cur {
            Some(r) if up => r.saturating_sub(1),
            Some(r) => (r + 1).min(rows.len().saturating_sub(1)),
            None => 0,
        };
        if let Some(t) = rows.get(next).and_then(|&i| p.project.tasks.get(i)) {
            p.select_only(t.id.clone());
        }
    }
    if ins {
        actions.push(Action::AddTask);
    }
    if del && !p.selected.is_empty() {
        actions.push(Action::Delete);
    }
    if f2 && let Some(t) = p.current().and_then(|id| p.project.task(id)) {
        actions.push(Action::Edit(t.id.clone(), "name", t.name.clone()));
    }
    if alt_shift
        && (right || left)
        && let Some(id) = p.current()
    {
        actions.push(Action::Run(if right { "task.indent" } else { "task.outdent" }, json!({"task": id.0})));
    }
}

fn apply(shell: &mut Shell, pi: usize, a: Action) {
    match a {
        Action::Run("__select", v) => {
            if let (Some(p), Some(id)) = (shell.projects.get_mut(pi), v.get("task").and_then(|x| x.as_str())) {
                p.select_only(TaskId(id.to_string()));
            }
        }
        Action::Run(cmd, params) => {
            shell.run(pi, cmd, params);
        }
        Action::Edit(id, col, text) => {
            if let Some(p) = shell.projects.get_mut(pi) {
                p.select_only(id.clone());
                p.editing = Some((id, col, text));
            }
        }
        Action::Cancel => {
            if let Some(p) = shell.projects.get_mut(pi) {
                p.editing = None;
            }
        }
        Action::Commit => {
            let Some((id, col, text)) = shell.projects.get_mut(pi).and_then(|p| p.editing.take()) else { return };
            let (field, value) = match col {
                "percent" => ("percent", text.trim().trim_end_matches('%').to_string()),
                "start" => ("constraint_date", text.trim().to_string()),
                other => (other, text.trim().to_string()),
            };
            if col == "start" {
                let constraint = if value.is_empty() { "ASAP" } else { "SNET" };
                shell.run(pi, "task.set", json!({"task": id.0, "field": "constraint", "value": constraint}));
            }
            shell.run(pi, "task.set", json!({"task": id.0, "field": field, "value": value}));
        }
        Action::AddTask => ribbon::add_task(shell, pi, false),
        Action::Delete => ribbon::delete_selected(shell, pi),
    }
}
