//! The Calendar view: a month, Monday first, with each task drawn as a bar across the days it
//! runs (wrapping onto the next week), milestones as diamonds, non-working days shaded and today
//! ringed. Click a bar to see the task in the Inspector; drag it to another day to move it (sets
//! "start no earlier than", as in the Gantt); double-click a day to add a task starting that day.

use egui::{Align2, FontId, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use plancraft_project::{Date, TaskId};
use serde_json::json;

use crate::{Shell, critical_color};

const DAY_HEADER: f32 = 22.0;
const LANE_H: f32 = 19.0;
const WEEKS: i64 = 6;
const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// A task's run of days within one week row: (task, first column, last column, starts here, ends here).
struct Segment {
    task: TaskId,
    from: usize,
    to: usize,
    starts: bool,
    ends: bool,
}

// One pass lays out the weeks and their bars; split, the pieces would share a dozen locals.
#[allow(clippy::cognitive_complexity)]
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, pi: usize) {
    let t = crate::theme::tokens(ui.ctx());
    let today = shell.today;
    let Some(p) = shell.projects.get_mut(pi) else { return };
    let (y, m) = p.cal_month.unwrap_or((p.schedule.start.year(), p.schedule.start.month()));
    let mut actions: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let mut select: Option<TaskId> = None;

    // Month bar.
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        if ui.button("‹").on_hover_text("Previous month").clicked() {
            p.cal_month = Some(if m == 1 { (y - 1, 12) } else { (y, m - 1) });
        }
        if ui.button("Today").clicked() {
            p.cal_month = Some((today.year(), today.month()));
        }
        if ui.button("›").on_hover_text("Next month").clicked() {
            p.cal_month = Some(if m == 12 { (y + 1, 1) } else { (y, m + 1) });
        }
        ui.add_space(10.0);
        ui.label(RichText::new(format!("{} {y}", MONTHS.get(m.saturating_sub(1) as usize).copied().unwrap_or(""))).size(18.0).strong().color(t.text));
        ui.add_space(16.0);
        ui.checkbox(&mut p.cal_summaries, "Summary tasks");
        ui.add_space(10.0);
        if ui.button("Project start").clicked() {
            p.cal_month = Some((p.schedule.start.year(), p.schedule.start.month()));
        }
    });
    ui.add_space(4.0);

    let Some(first) = Date::from_ymd(y, m, 1) else { return };
    let grid_start = first.add_days(-(first.weekday_from_monday() as i64)).unwrap_or(first);
    let area = ui.available_rect_before_wrap().shrink2(vec2(10.0, 4.0));
    let (area, bg) = ui.allocate_exact_size(area.size(), Sense::click());
    let col_w = area.width() / 7.0;
    let row_h = ((area.height() - DAY_HEADER) / WEEKS as f32).max(60.0);
    let painter = ui.painter_at(area);

    // Day names.
    for (c, name) in DAY_NAMES.iter().enumerate() {
        let x = area.left() + c as f32 * col_w;
        painter.text(pos2(x + 8.0, area.top() + DAY_HEADER / 2.0), Align2::LEFT_CENTER, *name, FontId::proportional(12.0), t.text_dim);
    }

    // Which tasks to draw, in a stable order (start, then longer first, then outline order).
    let mut tasks: Vec<(usize, Date, Date)> = p
        .project
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, x)| !x.cancelled && (p.cal_summaries || !x.summary))
        .filter_map(|(i, x)| p.schedule.get(&x.id).map(|d| (i, d.start, d.finish)))
        .collect();
    tasks.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));

    for w in 0..WEEKS {
        let top = area.top() + DAY_HEADER + w as f32 * row_h;
        let Some(week_start) = grid_start.add_days(w * 7) else { continue };
        let Some(week_end) = week_start.add_days(6) else { continue };
        // Day cells.
        for c in 0..7usize {
            let Some(day) = week_start.add_days(c as i64) else { continue };
            let cell = Rect::from_min_size(pos2(area.left() + c as f32 * col_w, top), vec2(col_w, row_h));
            let in_month = day.month() == m;
            if !p.project.calendar.is_working_day(day) {
                painter.rect_filled(cell, 0.0, t.header_bg);
            }
            painter.rect_stroke(cell, 0.0, Stroke::new(1.0, t.gridline), egui::StrokeKind::Inside);
            let num_pos = pos2(cell.left() + 12.0, cell.top() + 11.0);
            if day == today {
                painter.circle_filled(num_pos, 10.0, t.accent);
                painter.text(num_pos, Align2::CENTER_CENTER, day.day().to_string(), FontId::proportional(12.0), t.on_accent);
            } else {
                painter.text(
                    num_pos,
                    Align2::CENTER_CENTER,
                    day.day().to_string(),
                    FontId::proportional(12.0),
                    if in_month { t.text } else { t.text_disabled },
                );
            }
            if bg.double_clicked() && bg.interact_pointer_pos().is_some_and(|q| cell.contains(q)) {
                actions.push(("__add", json!({ "date": day.to_string() })));
            }
        }
        // Bars, packed into lanes.
        let mut segs: Vec<Segment> = Vec::new();
        for (i, s, f) in &tasks {
            if *f < week_start || *s > week_end {
                continue;
            }
            let Some(task) = p.project.tasks.get(*i) else { continue };
            let from = if *s < week_start { 0 } else { week_start.days_until(*s).clamp(0, 6) as usize };
            let to = if *f > week_end { 6 } else { week_start.days_until(*f).clamp(0, 6) as usize };
            segs.push(Segment { task: task.id.clone(), from, to, starts: *s >= week_start, ends: *f <= week_end });
        }
        let lanes_fit = (((row_h - 24.0) / LANE_H).floor() as usize).max(1);
        let mut lane_end: Vec<usize> = Vec::new(); // last column used per lane
        let mut hidden = [0usize; 7];
        for seg in segs {
            let lane = lane_end.iter().position(|&e| e < seg.from).unwrap_or(lane_end.len());
            if lane >= lanes_fit {
                for c in seg.from..=seg.to {
                    if let Some(h) = hidden.get_mut(c) {
                        *h += 1;
                    }
                }
                continue;
            }
            if lane == lane_end.len() {
                lane_end.push(seg.to);
            } else if let Some(e) = lane_end.get_mut(lane) {
                *e = seg.to;
            }
            let y0 = top + 24.0 + lane as f32 * LANE_H;
            let x0 = area.left() + seg.from as f32 * col_w + if seg.starts { 4.0 } else { 0.0 };
            let x1 = area.left() + (seg.to + 1) as f32 * col_w - if seg.ends { 4.0 } else { 0.0 };
            let bar = Rect::from_min_max(pos2(x0, y0), pos2(x1, y0 + LANE_H - 3.0));
            let Some(task) = p.project.task(&seg.task) else { continue };
            let d = p.schedule.get(&seg.task);
            let crit = d.is_some_and(|d| d.critical) && !task.summary;
            let selected = p.selected.contains(&seg.task);
            let color = if task.summary {
                t.text
            } else if crit {
                critical_color(&t)
            } else {
                t.accent
            };
            let milestone = task.milestone || d.is_some_and(|d| d.duration_days == 0);
            let id = ui.scope_id().with(("pc_cal_bar", &seg.task.0, w));
            let resp = ui.interact(bar, id, Sense::click_and_drag());
            if milestone {
                let c = pos2(bar.left() + 7.0, bar.center().y);
                painter.add(Shape::convex_polygon(
                    vec![pos2(c.x, c.y - 6.0), pos2(c.x + 6.0, c.y), pos2(c.x, c.y + 6.0), pos2(c.x - 6.0, c.y)],
                    color,
                    Stroke::NONE,
                ));
                painter.with_clip_rect(bar).text(pos2(c.x + 10.0, c.y), Align2::LEFT_CENTER, &task.name, FontId::proportional(12.0), t.text);
            } else {
                let radius = egui::CornerRadius {
                    nw: if seg.starts { 4 } else { 0 },
                    sw: if seg.starts { 4 } else { 0 },
                    ne: if seg.ends { 4 } else { 0 },
                    se: if seg.ends { 4 } else { 0 },
                };
                painter.rect_filled(bar, radius, color.gamma_multiply(if task.summary { 0.25 } else { 0.85 }));
                if task.percent_complete > 0 && !task.summary {
                    let w = bar.width() * f32::from(task.percent_complete.min(100)) / 100.0;
                    painter.rect_filled(
                        Rect::from_min_size(pos2(bar.left(), bar.bottom() - 3.0), vec2(w, 3.0)),
                        0.0,
                        egui::Color32::from_black_alpha(120),
                    );
                }
                let label = if seg.starts { task.name.clone() } else { format!("… {}", task.name) };
                let ink = if task.summary { t.text } else { t.on_accent };
                painter.with_clip_rect(bar.shrink2(vec2(4.0, 0.0))).text(
                    pos2(bar.left() + 6.0, bar.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    ink,
                );
            }
            if selected {
                painter.rect_stroke(bar.expand(1.5), 4.0, Stroke::new(2.0, t.text), egui::StrokeKind::Outside);
            }
            let resp =
                resp.on_hover_text(format!("{} {}{}", task.wbs, task.name, d.map(|d| format!("\n{} → {}", d.start, d.finish)).unwrap_or_default()));
            if resp.clicked() {
                select = Some(seg.task.clone());
            }
            // Drag to another day.
            let acc = id.with("acc");
            if resp.drag_started() {
                ui.ctx().data_mut(|dm| dm.insert_temp(acc, vec2(0.0, 0.0)));
            }
            if resp.dragged() {
                let total = ui.ctx().data_mut(|dm| {
                    let v = dm.get_temp_mut_or(acc, vec2(0.0, 0.0));
                    *v += resp.drag_delta();
                    *v
                });
                painter.rect_stroke(bar.translate(total), 4.0, Stroke::new(1.5, t.text), egui::StrokeKind::Outside);
            }
            if resp.drag_stopped() && !task.summary {
                let total = ui.ctx().data_mut(|dm| dm.get_temp::<egui::Vec2>(acc)).unwrap_or_default();
                let days = (total.x / col_w).round() as i64 + (total.y / row_h).round() as i64 * 7;
                if days != 0
                    && let Some(new) = d.and_then(|d| d.start.add_days(days))
                {
                    actions.push(("task.set", json!({"task": seg.task.0, "field": "constraint", "value": "SNET"})));
                    actions.push(("task.set", json!({"task": seg.task.0, "field": "constraint_date", "value": new.to_string()})));
                }
            }
        }
        for (c, n) in hidden.iter().enumerate() {
            if *n > 0 {
                let x = area.left() + (c + 1) as f32 * col_w - 6.0;
                painter.text(pos2(x, top + row_h - 9.0), Align2::RIGHT_CENTER, format!("+{n} more"), FontId::proportional(11.0), t.text_dim);
            }
        }
    }

    if let Some(id) = select
        && let Some(p) = shell.projects.get_mut(pi)
    {
        p.select_only(id);
    }
    for (cmd, params) in actions {
        if cmd == "__add" {
            let date = params.get("date").and_then(|d| d.as_str()).unwrap_or_default().to_string();
            add_on(shell, pi, &date);
        } else {
            shell.run(pi, cmd, params);
        }
    }
}

/// A new one-day task that starts no earlier than `date`.
fn add_on(shell: &mut Shell, pi: usize, date: &str) {
    let Some(p) = shell.projects.get(pi) else { return };
    let n = p.project.tasks.len() + 1;
    let Some(v) = shell.run(pi, "task.add", json!({ "name": format!("Task {n}"), "duration": "1d" })) else { return };
    let Some(id) = v.get("id").and_then(|x| x.as_str()).map(str::to_string) else { return };
    shell.run(pi, "task.set", json!({"task": id, "field": "constraint", "value": "SNET"}));
    shell.run(pi, "task.set", json!({"task": id, "field": "constraint_date", "value": date}));
    if let Some(p) = shell.projects.get_mut(pi) {
        p.select_only(TaskId(id));
    }
    shell.open_inspector();
}
