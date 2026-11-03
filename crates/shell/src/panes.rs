//! The window's central area as panes, as in Obsidian: a tree of splits whose leaves are tab
//! groups. Each leaf has its own tabs and shows one of them; the focused leaf is where the
//! ribbon's commands go. Splits go right (side by side) or down (stacked); the divider between
//! two panes drags to resize them.

use egui::{Rect, Sense, Ui, UiBuilder, pos2};
use gridcraft_ui_egui::theme::Tokens;
use serde::{Deserialize, Serialize};

use crate::{Shell, Tab, View, board, calendar, gantt, tab_button};

/// How a split divides its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dir {
    /// The two panes sit side by side.
    Right,
    /// The second pane sits below the first.
    Down,
}

/// A tab group: its tabs, and which one it shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leaf {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

/// A pane: one tab group, or two panes divided by a split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pane {
    Leaf(Leaf),
    Split { dir: Dir, ratio: f32, a: Box<Pane>, b: Box<Pane> },
}

/// Width of the draggable divider between two panes, in points.
const DIVIDER: f32 = 6.0;

impl Default for Pane {
    fn default() -> Self {
        Pane::Leaf(Leaf { tabs: vec![Tab::View(View::Gantt)], active: 0 })
    }
}

impl Leaf {
    /// The tab the leaf shows.
    pub fn tab(&self) -> Tab {
        self.tabs.get(self.active).copied().unwrap_or(Tab::View(View::Gantt))
    }

    /// Replaces the shown tab (adds one if the leaf is empty).
    pub fn set_tab(&mut self, tab: Tab) {
        match self.tabs.get_mut(self.active) {
            Some(slot) => *slot = tab,
            None => {
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Removes tab `k`, keeping `active` on a tab that exists.
    pub fn remove_tab(&mut self, k: usize) {
        if k >= self.tabs.len() {
            return;
        }
        self.tabs.remove(k);
        if k < self.active {
            self.active -= 1;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
    }
}

impl Pane {
    /// How many leaves (tab groups) the layout has. Always at least one.
    pub fn leaf_count(&self) -> usize {
        match self {
            Pane::Leaf(_) => 1,
            Pane::Split { a, b, .. } => a.leaf_count() + b.leaf_count(),
        }
    }

    /// Leaf `i`, counting from the top-left (the first pane of each split comes first).
    pub fn leaf(&self, i: usize) -> Option<&Leaf> {
        match self {
            Pane::Leaf(l) => (i == 0).then_some(l),
            Pane::Split { a, b, .. } => {
                let n = a.leaf_count();
                if i < n { a.leaf(i) } else { b.leaf(i - n) }
            }
        }
    }

    /// Leaf `i`, mutably.
    pub fn leaf_mut(&mut self, i: usize) -> Option<&mut Leaf> {
        match self {
            Pane::Leaf(l) => (i == 0).then_some(l),
            Pane::Split { a, b, .. } => {
                let n = a.leaf_count();
                if i < n { a.leaf_mut(i) } else { b.leaf_mut(i - n) }
            }
        }
    }

    /// Splits leaf `i` in two; both halves start with the leaf's current tab.
    pub fn split(&mut self, i: usize, dir: Dir) -> bool {
        match self {
            Pane::Leaf(l) => {
                if i != 0 {
                    return false;
                }
                let first = l.clone();
                let second = l.clone();
                *self = Pane::Split { dir, ratio: 0.5, a: Box::new(Pane::Leaf(first)), b: Box::new(Pane::Leaf(second)) };
                true
            }
            Pane::Split { a, b, .. } => {
                let n = a.leaf_count();
                if i < n { a.split(i, dir) } else { b.split(i - n, dir) }
            }
        }
    }

    /// Closes leaf `i`; its sibling takes the place of the split. The last leaf is never closed.
    pub fn close(&mut self, i: usize) -> bool {
        match self {
            Pane::Leaf(_) => false,
            Pane::Split { a, b, .. } => {
                let n = a.leaf_count();
                if i < n {
                    if matches!(**a, Pane::Leaf(_)) {
                        let keep = (**b).clone();
                        *self = keep;
                        true
                    } else {
                        a.close(i)
                    }
                } else if matches!(**b, Pane::Leaf(_)) {
                    let keep = (**a).clone();
                    *self = keep;
                    true
                } else {
                    b.close(i - n)
                }
            }
        }
    }
}

/// A change the panes ask for while they are drawn; applied once the drawing is done.
enum Op {
    Focus(usize),
    Split(usize, Dir),
    CloseLeaf(usize),
    CloseTab(usize, usize),
}

/// Draws the central area: every pane, its tab strip and its content.
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens, spreadsheet: &mut dyn FnMut(&mut Ui)) {
    let mut layout = std::mem::take(&mut shell.layout);
    let mut ops = Vec::new();
    let mut counter = 0usize;
    let area = ui.max_rect();
    let id = ui.scope_id().with("pc_panes");
    let multi = layout.leaf_count() > 1;
    show_node(shell, ui, t, &mut layout, area, &mut counter, &mut ops, spreadsheet, (id, multi));
    shell.layout = layout;
    let mut changed = false;
    for op in ops {
        match op {
            Op::Focus(i) => {
                if shell.focus != i {
                    shell.focus = i;
                }
            }
            Op::Split(i, dir) => {
                changed |= shell.layout.split(i, dir);
            }
            Op::CloseLeaf(i) => {
                changed |= shell.layout.close(i);
            }
            Op::CloseTab(i, k) => {
                let Some(leaf) = shell.layout.leaf_mut(i) else { continue };
                leaf.remove_tab(k);
                let empty = leaf.tabs.is_empty();
                if empty && shell.layout.leaf_count() > 1 {
                    changed |= shell.layout.close(i);
                } else if empty {
                    if let Some(leaf) = shell.layout.leaf_mut(i) {
                        leaf.tabs.push(Tab::View(View::Gantt));
                    }
                    changed = true;
                } else {
                    changed = true;
                }
            }
        }
    }
    let count = shell.layout.leaf_count();
    if shell.focus >= count {
        shell.focus = count.saturating_sub(1);
    }
    if changed {
        shell.save_prefs();
    }
}

#[allow(clippy::too_many_arguments)]
fn show_node(
    shell: &mut Shell,
    ui: &mut Ui,
    t: &Tokens,
    node: &mut Pane,
    area: Rect,
    counter: &mut usize,
    ops: &mut Vec<Op>,
    spreadsheet: &mut dyn FnMut(&mut Ui),
    key: (egui::Id, bool),
) {
    let (id, multi) = key;
    match node {
        Pane::Leaf(leaf) => {
            let index = *counter;
            *counter += 1;
            let focused = shell.focus == index;
            ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
                leaf_ui(shell, ui, t, leaf, index, focused, multi, ops, spreadsheet);
            });
        }
        Pane::Split { dir, ratio, a, b } => {
            let (first, divider, second) = split_rects(area, *dir, *ratio);
            show_node(shell, ui, t, a, first, counter, ops, spreadsheet, (id.with("a"), multi));
            let resp = ui.interact(divider, id.with(("divider", *counter)), Sense::drag());
            let size = match dir {
                Dir::Right => area.width(),
                Dir::Down => area.height(),
            };
            if resp.dragged() {
                let d = match dir {
                    Dir::Right => resp.drag_delta().x,
                    Dir::Down => resp.drag_delta().y,
                };
                *ratio = (*ratio + d / size.max(1.0)).clamp(0.12, 0.88);
            }
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(match dir {
                    Dir::Right => egui::CursorIcon::ResizeHorizontal,
                    Dir::Down => egui::CursorIcon::ResizeVertical,
                });
            }
            let fill = if resp.hovered() || resp.dragged() { t.accent_soft } else { t.separator };
            ui.painter().rect_filled(divider, 0.0, fill);
            show_node(shell, ui, t, b, second, counter, ops, spreadsheet, (id.with("b"), multi));
        }
    }
}

/// The two panes and the divider between them, for a split of `area`.
fn split_rects(area: Rect, dir: Dir, ratio: f32) -> (Rect, Rect, Rect) {
    match dir {
        Dir::Right => {
            let x = area.left() + area.width() * ratio;
            (
                Rect::from_min_max(area.min, pos2(x - DIVIDER / 2.0, area.max.y)),
                Rect::from_min_max(pos2(x - DIVIDER / 2.0, area.min.y), pos2(x + DIVIDER / 2.0, area.max.y)),
                Rect::from_min_max(pos2(x + DIVIDER / 2.0, area.min.y), area.max),
            )
        }
        Dir::Down => {
            let y = area.top() + area.height() * ratio;
            (
                Rect::from_min_max(area.min, pos2(area.max.x, y - DIVIDER / 2.0)),
                Rect::from_min_max(pos2(area.min.x, y - DIVIDER / 2.0), pos2(area.max.x, y + DIVIDER / 2.0)),
                Rect::from_min_max(pos2(area.min.x, y + DIVIDER / 2.0), area.max),
            )
        }
    }
}

/// One leaf: its tab strip on top, its shown tab below.
#[allow(clippy::too_many_arguments)]
fn leaf_ui(
    shell: &mut Shell,
    ui: &mut Ui,
    t: &Tokens,
    leaf: &mut Leaf,
    index: usize,
    focused: bool,
    multi: bool,
    ops: &mut Vec<Op>,
    spreadsheet: &mut dyn FnMut(&mut Ui),
) {
    let pane_rect = ui.max_rect();
    if ui.ctx().input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| pane_rect.contains(p))) {
        ops.push(Op::Focus(index));
    }
    egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin { left: 4, right: 4, top: 3, bottom: 0 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let tabs = leaf.tabs.clone();
            for (k, tab) in tabs.iter().enumerate() {
                let title = match tab {
                    Tab::Spreadsheet => "Spreadsheet".to_string(),
                    Tab::Documents => "Documents".to_string(),
                    Tab::View(v) => v.label().to_string(),
                };
                let (resp, closed) = tab_button(ui, t, &title, k == leaf.active, true);
                if resp.clicked() {
                    leaf.active = k;
                    ops.push(Op::Focus(index));
                }
                if closed {
                    ops.push(Op::CloseTab(index, k));
                }
                resp.context_menu(|ui| {
                    if ui.button("Split right").clicked() {
                        ops.push(Op::Split(index, Dir::Right));
                        ui.close();
                    }
                    if ui.button("Split down").clicked() {
                        ops.push(Op::Split(index, Dir::Down));
                        ui.close();
                    }
                    if multi && ui.button("Close pane").clicked() {
                        ops.push(Op::CloseLeaf(index));
                        ui.close();
                    }
                    if ui.button("Close tab").clicked() {
                        ops.push(Op::CloseTab(index, k));
                        ui.close();
                    }
                });
            }
            ui.menu_button("+", |ui| {
                for v in View::ALL.into_iter().filter(|v| v.available()) {
                    if ui.button(v.label()).clicked() {
                        leaf.tabs.push(Tab::View(v));
                        leaf.active = leaf.tabs.len() - 1;
                        ui.close();
                    }
                }
                if ui.button("Documents").clicked() {
                    leaf.tabs.push(Tab::Documents);
                    leaf.active = leaf.tabs.len() - 1;
                    ui.close();
                }
                if ui.button("Spreadsheet").clicked() {
                    leaf.tabs.push(Tab::Spreadsheet);
                    leaf.active = leaf.tabs.len() - 1;
                    ui.close();
                }
            });
        });
    });
    let rect = ui.max_rect();
    let strip = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + 2.0));
    if focused {
        ui.painter().rect_filled(strip, 0.0, t.accent);
    }
    match leaf.tab() {
        Tab::Spreadsheet => spreadsheet(ui),
        Tab::Documents => {
            if shell.active_project().is_some() {
                crate::documents::show(shell, ui, t);
            } else {
                shell.start_page(ui, t);
            }
        }
        Tab::View(view) => match shell.active_project() {
            Some(project) => {
                if view == View::Calendar {
                    calendar::show(shell, ui, project);
                } else if view == View::Kanban {
                    board::show(shell, ui, project);
                } else if view.available() {
                    gantt::show(shell, ui, project);
                } else {
                    crate::coming_soon(ui, t, view);
                }
            }
            None => shell.start_page(ui, t),
        },
    }
}
