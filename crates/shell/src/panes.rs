//! The window's central area as panes, as in Obsidian: a tree of splits whose leaves are tab
//! groups. Each leaf has its own tabs and shows one of them; the focused leaf is where the
//! ribbon's commands go. Splits go right (side by side) or down (stacked); the divider between
//! two panes drags to resize them. A pane can also float over the window in its own frame, and
//! dock back into the focused pane. Floating panes are not kept between runs.

use egui::{Pos2, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
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

/// A tab group: its tabs, and which one it shows. The first `pinned` tabs are pinned: they sit at
/// the front of the strip and can't be closed until unpinned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leaf {
    pub tabs: Vec<Tab>,
    pub active: usize,
    #[serde(default)]
    pub pinned: usize,
}

/// A pane: one tab group, or two panes divided by a split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pane {
    Leaf(Leaf),
    Split { dir: Dir, ratio: f32, a: Box<Pane>, b: Box<Pane> },
}

/// A pane floating over the window in its own frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Float {
    pub id: u64,
    pub leaf: Leaf,
    pub pos: Pos2,
    pub size: Vec2,
}

/// Width of the draggable divider between two panes, in points.
const DIVIDER: f32 = 6.0;

impl Default for Pane {
    fn default() -> Self {
        Pane::Leaf(Leaf { tabs: vec![Tab::View(View::Gantt)], active: 0, pinned: 0 })
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

    /// Removes tab `k`, keeping `active` on a tab that exists. A pinned tab can't be removed.
    pub fn remove_tab(&mut self, k: usize) {
        if k >= self.tabs.len() || k < self.pinned {
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

    /// Moves tab `from` to position `to`, keeping `active` on the same tab.
    fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = if self.active == from {
            to
        } else {
            let after_remove = if self.active > from { self.active - 1 } else { self.active };
            if after_remove >= to { after_remove + 1 } else { after_remove }
        };
    }

    /// Pins tab `k`: it moves to the end of the pinned tabs at the front of the strip.
    pub fn pin_tab(&mut self, k: usize) {
        if k < self.tabs.len() && k >= self.pinned {
            self.move_tab(k, self.pinned);
            self.pinned += 1;
        }
    }

    /// Unpins tab `k`: it moves to just after the tabs that stay pinned.
    pub fn unpin_tab(&mut self, k: usize) {
        if k < self.pinned {
            self.pinned -= 1;
            self.move_tab(k, self.pinned);
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
    Float(usize),
    Dock(u64),
    CloseFloat(u64),
}

/// Where a leaf is drawn: in the split layout, or in a floating frame.
#[derive(Debug, Clone, Copy)]
enum Slot {
    Docked { index: usize, focused: bool, multi: bool },
    Floating(u64),
}

/// The text on a tab.
fn tab_title(tab: &Tab) -> String {
    match tab {
        Tab::Spreadsheet => "Spreadsheet".to_string(),
        Tab::Documents => "Documents".to_string(),
        Tab::Inspector => "Properties".to_string(),
        Tab::View(v) => v.label().to_string(),
    }
}

/// Draws the central area: every docked pane with its tab strip and content, then the floating
/// panes on top of them.
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens, spreadsheet: &mut dyn FnMut(&mut Ui)) {
    let mut layout = std::mem::take(&mut shell.layout);
    let mut ops = Vec::new();
    let mut counter = 0usize;
    let area = ui.max_rect();
    let id = ui.scope_id().with("pc_panes");
    let multi = layout.leaf_count() > 1;
    show_node(shell, ui, t, &mut layout, area, &mut counter, &mut ops, spreadsheet, (id, multi));
    shell.layout = layout;

    // Each floating pane is a window of its own.
    let mut floats = std::mem::take(&mut shell.floating);
    for f in floats.iter_mut() {
        let mut open = true;
        let win = egui::Window::new(tab_title(&f.leaf.tab()))
            .id(ui.scope_id().with(("pc_float", f.id)))
            .default_pos(f.pos)
            .default_size(f.size)
            .resizable(true)
            .collapsible(false)
            .open(&mut open);
        let fid = f.id;
        let _ = win.show(ui.ctx(), |ui| {
            leaf_ui(&mut *shell, ui, t, &mut f.leaf, Slot::Floating(fid), &mut ops, &mut *spreadsheet);
        });
        if !open {
            ops.push(Op::CloseFloat(fid));
        }
    }
    shell.floating = floats;

    let mut changed = false;
    for op in ops {
        match op {
            Op::Focus(i) => shell.focus = i,
            Op::Split(i, dir) => changed |= shell.layout.split(i, dir),
            Op::CloseLeaf(i) => changed |= shell.layout.close(i),
            Op::CloseTab(i, k) => {
                let Some(leaf) = shell.layout.leaf_mut(i) else { continue };
                leaf.remove_tab(k);
                if leaf.tabs.is_empty() {
                    if shell.layout.leaf_count() > 1 {
                        shell.layout.close(i);
                    } else if let Some(leaf) = shell.layout.leaf_mut(i) {
                        leaf.tabs.push(Tab::View(View::Gantt));
                    }
                }
                changed = true;
            }
            Op::Float(i) => {
                let Some(leaf) = shell.layout.leaf(i).cloned() else { continue };
                if shell.layout.leaf_count() > 1 {
                    shell.layout.close(i);
                } else {
                    shell.layout = Pane::default();
                }
                let id = shell.floating.iter().map(|f| f.id).max().unwrap_or(0) + 1;
                let n = shell.floating.len() as f32;
                shell.floating.push(Float { id, leaf, pos: pos2(260.0 + 28.0 * n, 170.0 + 28.0 * n), size: vec2(440.0, 320.0) });
                changed = true;
            }
            Op::Dock(id) => {
                let Some(at) = shell.floating.iter().position(|f| f.id == id) else { continue };
                let float = shell.floating.remove(at);
                let focus = shell.focus;
                shell.layout.split(focus, Dir::Right);
                if let Some(leaf) = shell.layout.leaf_mut(focus + 1) {
                    *leaf = float.leaf;
                }
                shell.focus = focus + 1;
                changed = true;
            }
            Op::CloseFloat(id) => shell.floating.retain(|f| f.id != id),
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
            let slot = Slot::Docked { index, focused: shell.focus == index, multi };
            ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
                leaf_ui(shell, ui, t, leaf, slot, ops, spreadsheet);
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

/// Closes tab `k` of a leaf. A floating pane with no tabs left closes itself.
fn close_tab(slot: Slot, leaf: &mut Leaf, k: usize, ops: &mut Vec<Op>) {
    if k < leaf.pinned {
        return;
    }
    match slot {
        Slot::Docked { index, .. } => ops.push(Op::CloseTab(index, k)),
        Slot::Floating(id) => {
            leaf.remove_tab(k);
            if leaf.tabs.is_empty() {
                ops.push(Op::CloseFloat(id));
            }
        }
    }
}

/// One leaf: its tab strip on top, its shown tab below.
#[allow(clippy::too_many_arguments)]
fn leaf_ui(shell: &mut Shell, ui: &mut Ui, t: &Tokens, leaf: &mut Leaf, slot: Slot, ops: &mut Vec<Op>, spreadsheet: &mut dyn FnMut(&mut Ui)) {
    let pane_rect = ui.max_rect();
    if let Slot::Docked { index, .. } = slot
        && ui.ctx().input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| pane_rect.contains(p)))
    {
        ops.push(Op::Focus(index));
    }
    egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin { left: 4, right: 4, top: 3, bottom: 0 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let tabs = leaf.tabs.clone();
            for (k, tab) in tabs.iter().enumerate() {
                let pinned = k < leaf.pinned;
                let (resp, closed) = tab_button(ui, t, &tab_title(tab), k == leaf.active, pinned);
                if resp.clicked() {
                    leaf.active = k;
                    if let Slot::Docked { index, .. } = slot {
                        ops.push(Op::Focus(index));
                    }
                }
                if closed {
                    close_tab(slot, leaf, k, ops);
                }
                resp.context_menu(|ui| {
                    match slot {
                        Slot::Docked { index, multi, .. } => {
                            if ui.button("Split right").clicked() {
                                ops.push(Op::Split(index, Dir::Right));
                                ui.close();
                            }
                            if ui.button("Split down").clicked() {
                                ops.push(Op::Split(index, Dir::Down));
                                ui.close();
                            }
                            if ui.button("Float pane").clicked() {
                                ops.push(Op::Float(index));
                                ui.close();
                            }
                            if multi && ui.button("Close pane").clicked() {
                                ops.push(Op::CloseLeaf(index));
                                ui.close();
                            }
                        }
                        Slot::Floating(id) => {
                            if ui.button("Dock pane").clicked() {
                                ops.push(Op::Dock(id));
                                ui.close();
                            }
                        }
                    }
                    if ui.button(if pinned { "Unpin tab" } else { "Pin tab" }).clicked() {
                        if pinned {
                            leaf.unpin_tab(k);
                        } else {
                            leaf.pin_tab(k);
                        }
                        ui.close();
                    }
                    let close = ui.add_enabled(!pinned, egui::Button::new("Close tab"));
                    if close.on_disabled_hover_text("Unpin the tab to close it").clicked() {
                        close_tab(slot, leaf, k, ops);
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
                if ui.button("Properties").clicked() {
                    leaf.tabs.push(Tab::Inspector);
                    leaf.active = leaf.tabs.len() - 1;
                    ui.close();
                }
            });
        });
    });
    let rect = ui.max_rect();
    if matches!(slot, Slot::Docked { focused: true, .. }) {
        let strip = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + 2.0));
        ui.painter().rect_filled(strip, 0.0, t.accent);
    }
    match leaf.tab() {
        Tab::Spreadsheet => spreadsheet(ui),
        Tab::Inspector => crate::inspector::show(shell, ui, t),
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

#[cfg(test)]
mod pin_tests {
    use super::*;

    fn leaf(tabs: &[View], active: usize) -> Leaf {
        Leaf { tabs: tabs.iter().map(|v| Tab::View(*v)).collect(), active, pinned: 0 }
    }

    #[test]
    fn pinning_moves_the_tab_to_the_front_and_keeps_the_active_tab() {
        let mut l = leaf(&[View::Gantt, View::Calendar, View::Kanban], 2);
        l.pin_tab(1);
        assert_eq!(l.pinned, 1);
        assert_eq!(l.tabs[0], Tab::View(View::Calendar));
        assert_eq!(l.tab(), Tab::View(View::Kanban));
        assert_eq!(l.active, 2);
    }

    #[test]
    fn a_pinned_tab_cannot_be_removed_until_unpinned() {
        let mut l = leaf(&[View::Gantt, View::Calendar], 0);
        l.pin_tab(0);
        l.remove_tab(0);
        assert_eq!(l.tabs.len(), 2);
        l.unpin_tab(0);
        assert_eq!(l.pinned, 0);
        l.remove_tab(0);
        assert_eq!(l.tabs, vec![Tab::View(View::Calendar)]);
    }

    #[test]
    fn unpinning_places_the_tab_after_the_pinned_ones() {
        let mut l = leaf(&[View::Gantt, View::Calendar, View::Kanban], 0);
        l.pin_tab(1);
        l.pin_tab(2);
        assert_eq!(l.pinned, 2);
        l.unpin_tab(0);
        assert_eq!(l.pinned, 1);
        assert_eq!(l.tabs[0], Tab::View(View::Kanban));
        assert_eq!(l.tab(), Tab::View(View::Gantt));
    }
}
