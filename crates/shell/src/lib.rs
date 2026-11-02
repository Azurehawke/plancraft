//! PlanCraft's window: an Obsidian-style shell.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │ tabs: [Acme Network ▸ Gantt] [Budget.xlsx] [+]                        │
//! │ ribbon for the active tab (Task · Resource · Project · View)         │
//! ├──┬────────────┬───────────────────────────────────────┬──────────────┤
//! │▣ │ Explorer   │  Gantt: task table │ timescale + bars  │  Inspector   │
//! │⌕ │ Search     │                                       │  (selected   │
//! │★ │ …panel     │                                       │   task or    │
//! │  │            │                                       │   project)   │
//! └──┴────────────┴───────────────────────────────────────┴──────────────┘
//! ```
//!
//! The shell draws project tabs itself; a spreadsheet tab is drawn by the host (GridCraft's
//! spreadsheet UI) through the closure passed to [`Shell::show`]. Every edit goes through
//! `plancraft_commands::run`, is saved to this device's change file at once, and other devices'
//! changes are picked up every couple of seconds. File pickers belong to the host: the shell
//! asks for them through [`Shell::take_requests`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod board;
mod calendar;
mod gantt;
mod inspector;
mod project;
mod ribbon;
mod sidebar;
mod wizard;

use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};
use gridcraft_ui_egui::theme::Tokens;
use plancraft_project::Date;

pub use project::OpenProject;
pub use wizard::Wizard;

/// Something only the host can do (a file dialog, opening a spreadsheet).
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Pick a `.plancraft` file or `.plan` folder, then call [`Shell::open_project`].
    OpenProject,
    /// Pick the folder a new project goes in, then call [`Shell::set_wizard_folder`].
    PickWizardFolder,
    /// Show the spreadsheet tab (and open this file there, when given).
    OpenSpreadsheet(Option<PathBuf>),
    /// Pick a drawing (PDF) inside the job folder, then call [`Shell::link_drawing`].
    LinkDrawing,
}

/// The left sidebar's panels, in icon-rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Panel {
    Explorer,
    Search,
    Bookmarks,
    Outline,
    Filters,
    Recent,
    Sync,
}

impl Panel {
    pub const ALL: [Panel; 7] = [Panel::Explorer, Panel::Search, Panel::Bookmarks, Panel::Outline, Panel::Filters, Panel::Recent, Panel::Sync];

    pub fn label(self) -> &'static str {
        match self {
            Panel::Explorer => "Explorer",
            Panel::Search => "Search",
            Panel::Bookmarks => "Bookmarks",
            Panel::Outline => "Outline",
            Panel::Filters => "Filters",
            Panel::Recent => "Recent",
            Panel::Sync => "Sync",
        }
    }
}

/// A project tab's view (the switcher in the tab's corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum View {
    Gantt,
    TrackingGantt,
    Network,
    ResourceSheet,
    TaskUsage,
    ResourceUsage,
    Calendar,
    Kanban,
}

impl View {
    pub const ALL: [View; 8] =
        [View::Gantt, View::TrackingGantt, View::Network, View::ResourceSheet, View::TaskUsage, View::ResourceUsage, View::Calendar, View::Kanban];

    pub fn label(self) -> &'static str {
        match self {
            View::Gantt => "Gantt Chart",
            View::TrackingGantt => "Tracking Gantt",
            View::Network => "Network Diagram",
            View::ResourceSheet => "Resource Sheet",
            View::TaskUsage => "Task Usage",
            View::ResourceUsage => "Resource Usage",
            View::Calendar => "Calendar",
            View::Kanban => "Board",
        }
    }

    /// Built so far (the rest arrive in later phases).
    pub fn available(self) -> bool {
        matches!(self, View::Gantt | View::Calendar | View::Kanban)
    }
}

/// Task filters (the Filters panel), applied to the Gantt's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Filter {
    All,
    Critical,
    Late,
    Milestones,
    Incomplete,
    Completed,
}

impl Filter {
    pub const ALL: [Filter; 6] = [Filter::All, Filter::Critical, Filter::Late, Filter::Milestones, Filter::Incomplete, Filter::Completed];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All tasks",
            Filter::Critical => "Critical",
            Filter::Late => "Late (negative slack)",
            Filter::Milestones => "Milestones",
            Filter::Incomplete => "Incomplete",
            Filter::Completed => "Completed",
        }
    }
}

/// An open tab.
#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    /// A project (index into [`Shell::projects`]) in one of its views.
    Project { project: usize, view: View },
    /// The spreadsheet UI (drawn by the host).
    Spreadsheet,
}

/// A pinned task (the Bookmarks panel).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bookmark {
    pub project: PathBuf,
    pub task: String,
    pub label: String,
}

/// What the shell remembers between runs on this device (`shell.json` in the settings folder).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShellPrefs {
    pub recent: Vec<PathBuf>,
    pub bookmarks: Vec<Bookmark>,
    pub left_open: bool,
    pub right_open: bool,
    pub panel: Option<Panel>,
    /// Link this device's projects to RevuCraft's punch lists (off by default).
    pub revucraft_link: bool,
}

/// The whole shell.
pub struct Shell {
    pub projects: Vec<OpenProject>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub panel: Panel,
    pub left_open: bool,
    pub right_open: bool,
    pub filter: Filter,
    pub search: String,
    pub prefs: ShellPrefs,
    pub wizard: Option<Wizard>,
    /// A message for the person (errors from commands, files that couldn't be read).
    pub message: Option<String>,
    device: String,
    settings_dir: Option<PathBuf>,
    today: Date,
    requests: Vec<Request>,
}

impl Shell {
    /// A shell for this device. `settings_dir` holds `shell.json` (recent projects, bookmarks);
    /// `today` is the local date (the shell never reads the clock, so it runs anywhere).
    pub fn new(device: &str, settings_dir: Option<PathBuf>, today: Date) -> Shell {
        let prefs: ShellPrefs = settings_dir
            .as_ref()
            .and_then(|d| std::fs::read(d.join("shell.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(ShellPrefs { left_open: true, right_open: true, ..ShellPrefs::default() });
        Shell {
            projects: Vec::new(),
            tabs: Vec::new(),
            active: 0,
            panel: prefs.panel.unwrap_or(Panel::Explorer),
            left_open: prefs.left_open,
            right_open: prefs.right_open,
            filter: Filter::All,
            search: String::new(),
            prefs,
            wizard: None,
            message: None,
            device: device.to_string(),
            settings_dir,
            today,
            requests: Vec::new(),
        }
    }

    /// Updates the date used for "today" (call once a day, or every frame).
    pub fn set_today(&mut self, today: Date) {
        self.today = today;
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// Requests for the host since the last call.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    pub(crate) fn request(&mut self, r: Request) {
        if !self.requests.contains(&r) {
            self.requests.push(r);
        }
    }

    /// Opens a project (its `.plan` folder or `.plancraft` file) in a new tab, or switches to it.
    pub fn open_project(&mut self, path: &Path) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.projects.iter().position(|p| p.dir() == canon || p.dir() == path || p.entry_path() == canon) {
            self.focus_project(i);
            return;
        }
        match OpenProject::open(path, &self.device, self.today) {
            Ok(p) => {
                self.remember_recent(p.dir().to_path_buf());
                self.projects.push(p);
                let i = self.projects.len() - 1;
                self.tabs.push(Tab::Project { project: i, view: View::Gantt });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => self.message = Some(e),
        }
    }

    fn focus_project(&mut self, i: usize) {
        match self.tabs.iter().position(|t| matches!(t, Tab::Project { project, .. } if *project == i)) {
            Some(t) => self.active = t,
            None => {
                self.tabs.push(Tab::Project { project: i, view: View::Gantt });
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Shows the spreadsheet tab (adding it if needed).
    pub fn show_spreadsheet_tab(&mut self) {
        match self.tabs.iter().position(|t| *t == Tab::Spreadsheet) {
            Some(i) => self.active = i,
            None => {
                self.tabs.push(Tab::Spreadsheet);
                self.active = self.tabs.len() - 1;
            }
        }
    }

    /// Starts the new project wizard.
    pub fn start_wizard(&mut self) {
        let folder = self.prefs.recent.first().and_then(|p| p.parent().map(Path::to_path_buf));
        self.wizard = Some(Wizard::new(folder));
    }

    /// The folder the host's picker returned for the wizard.
    pub fn set_wizard_folder(&mut self, dir: PathBuf) {
        if let Some(w) = &mut self.wizard {
            w.folder = Some(dir);
        }
    }

    pub(crate) fn remember_recent(&mut self, dir: PathBuf) {
        self.prefs.recent.retain(|p| *p != dir);
        self.prefs.recent.insert(0, dir);
        self.prefs.recent.truncate(12);
        self.save_prefs();
    }

    pub(crate) fn save_prefs(&mut self) {
        self.prefs.left_open = self.left_open;
        self.prefs.right_open = self.right_open;
        self.prefs.panel = Some(self.panel);
        let Some(dir) = &self.settings_dir else { return };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_vec_pretty(&self.prefs) {
            let _ = std::fs::write(dir.join("shell.json"), json);
        }
    }

    /// The project shown in the active tab.
    pub fn active_project(&self) -> Option<usize> {
        match self.tabs.get(self.active) {
            Some(Tab::Project { project, .. }) => Some(*project),
            _ => None,
        }
    }

    /// Runs a project command on project `i` (see `plancraft_commands`), saves, and refreshes.
    pub fn run(&mut self, i: usize, id: &str, params: serde_json::Value) -> Option<serde_json::Value> {
        let today = self.today;
        let p = self.projects.get_mut(i)?;
        match p.run(id, &params, today) {
            Ok(v) => Some(v),
            Err(e) => {
                self.message = Some(e);
                None
            }
        }
    }

    /// Reloads projects whose files another device changed (cheap: compares file sizes and times),
    /// and, with the RevuCraft link on, brings in any punch list that changed.
    pub fn poll(&mut self, now_ms: f64) {
        let today = self.today;
        for p in &mut self.projects {
            p.poll(now_ms, today);
        }
        if self.prefs.revucraft_link {
            for i in 0..self.projects.len() {
                self.sync_revucraft(i);
            }
        }
    }

    /// Whether this device links projects to RevuCraft.
    pub fn revucraft_link(&self) -> bool {
        self.prefs.revucraft_link
    }

    /// Turns the RevuCraft link on or off on this device. Turning it on reads every linked
    /// drawing's punch list again.
    pub fn set_revucraft_link(&mut self, on: bool) {
        self.prefs.revucraft_link = on;
        self.save_prefs();
        for p in &mut self.projects {
            p.forget_markups();
        }
        if on {
            for i in 0..self.projects.len() {
                self.sync_revucraft(i);
            }
        }
    }

    /// Links a drawing (a PDF inside the active project's job folder) and brings in its punch list.
    pub fn link_drawing(&mut self, file: &Path) -> bool {
        let Some(i) = self.active_project() else { return false };
        let Some(root) = self.projects.get(i).and_then(|p| p.job_root()) else { return false };
        let Ok(rel) = file.strip_prefix(&root) else {
            self.message = Some("Pick a drawing inside the job folder, beside the .plan folder.".into());
            return false;
        };
        let rel: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let path = rel.join("/");
        if self.run(i, "revucraft.addDrawing", serde_json::json!({ "path": path })).is_none() {
            return false;
        }
        self.sync_revucraft(i);
        true
    }

    /// Brings project `i`'s changed punch lists into its plan. Does nothing with the link off.
    fn sync_revucraft(&mut self, i: usize) {
        if !self.prefs.revucraft_link {
            return;
        }
        let changed = match self.projects.get_mut(i) {
            Some(p) => p.take_changed_markups(),
            None => return,
        };
        for (drawing, path) in changed {
            let json = match project::read_markups(&path) {
                Ok(Some(json)) => json,
                Ok(None) => continue,
                Err(e) => {
                    self.message = Some(format!("RevuCraft: {e}"));
                    continue;
                }
            };
            if let Some(r) = self.run(i, "revucraft.sync", serde_json::json!({ "drawing": drawing, "json": json })) {
                let n = |k: &str| r.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
                if n("added") + n("updated") + n("cancelled") + n("restored") > 0 {
                    self.message = Some(format!(
                        "RevuCraft punch list: {} added, {} updated, {} cancelled, {} restored",
                        n("added"),
                        n("updated"),
                        n("cancelled"),
                        n("restored")
                    ));
                }
            }
        }
    }

    /// Draws the shell. `spreadsheet` draws the spreadsheet tab's contents into the given Ui.
    pub fn show(&mut self, ui: &mut Ui, spreadsheet: &mut dyn FnMut(&mut Ui)) {
        let t = Tokens::get(ui.ctx());
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        self.tab_bar(ui, &t);
        let on_sheet = matches!(self.tabs.get(self.active), Some(Tab::Spreadsheet));
        if !on_sheet {
            ribbon::show(self, ui, &t);
        }
        self.message_bar(ui, &t);
        sidebar::show(self, ui, &t);
        if !on_sheet && self.right_open {
            inspector::show(self, ui, &t);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(if on_sheet { t.window } else { t.grid_bg })).show(ui, |ui| {
            match self.tabs.get(self.active).cloned() {
                Some(Tab::Spreadsheet) => spreadsheet(ui),
                Some(Tab::Project { project, view }) => {
                    if view == View::Calendar {
                        calendar::show(self, ui, project);
                    } else if view == View::Kanban {
                        board::show(self, ui, project);
                    } else if view.available() {
                        gantt::show(self, ui, project);
                    } else {
                        coming_soon(ui, &t, view);
                    }
                }
                None => self.start_page(ui, &t),
            }
        });
        if self.wizard.is_some() {
            wizard::show(self, ui.ctx());
        }
    }

    fn tab_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        egui::Panel::top("pc_tabs")
            .exact_size(34.0)
            .frame(egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 0 }))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if icon_button(ui, t, gridcraft_ui_egui::icons::Icon::SheetList, self.left_open)
                        .on_hover_text("Show or hide the sidebar")
                        .clicked()
                    {
                        self.left_open = !self.left_open;
                        self.save_prefs();
                    }
                    ui.add_space(4.0);
                    let mut close: Option<usize> = None;
                    let mut pick: Option<usize> = None;
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let title = match tab {
                            Tab::Spreadsheet => "Spreadsheet".to_string(),
                            Tab::Project { project, view } => {
                                let name = self.projects.get(*project).map(|p| p.title()).unwrap_or_default();
                                format!("{name} · {}", view.label())
                            }
                        };
                        let active = i == self.active;
                        let fill = if active { t.tab_active } else { t.tab_bar };
                        egui::Frame::NONE
                            .fill(fill)
                            .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                            .inner_margin(egui::Margin::symmetric(10, 5))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let r = ui.add(
                                        egui::Label::new(RichText::new(title).color(if active { t.text } else { t.text_dim }))
                                            .sense(egui::Sense::click()),
                                    );
                                    if r.clicked() {
                                        pick = Some(i);
                                    }
                                    if ui
                                        .add(egui::Button::new(RichText::new("×").color(t.text_dim)).frame(false).small())
                                        .on_hover_text("Close tab")
                                        .clicked()
                                    {
                                        close = Some(i);
                                    }
                                });
                            });
                    }
                    if ui.add(egui::Button::new(RichText::new("+").size(16.0)).frame(false)).on_hover_text("New project").clicked() {
                        self.start_wizard();
                    }
                    if let Some(i) = pick {
                        self.active = i;
                    }
                    if let Some(i) = close {
                        self.tabs.remove(i);
                        if self.active >= i && self.active > 0 {
                            self.active -= 1;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, t, gridcraft_ui_egui::icons::Icon::Info, self.right_open)
                            .on_hover_text("Show or hide the Inspector")
                            .clicked()
                        {
                            self.right_open = !self.right_open;
                            self.save_prefs();
                        }
                    });
                });
            });
    }

    fn message_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(msg) = self.message.clone() else { return };
        egui::Panel::top("pc_message").frame(egui::Frame::NONE.fill(t.notice_warn_bg).inner_margin(egui::Margin::symmetric(12, 6))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(msg).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Dismiss").clicked() {
                        self.message = None;
                    }
                });
            });
        });
    }

    fn start_page(&mut self, ui: &mut Ui, t: &Tokens) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.label(RichText::new("PlanCraft").size(30.0).color(t.text).strong());
            ui.label(RichText::new("Plan projects on every device, from one shared folder.").color(t.text_dim));
            ui.add_space(18.0);
            if ui.add(egui::Button::new(RichText::new("  New project…  ").size(15.0)).fill(t.accent)).clicked() {
                self.start_wizard();
            }
            ui.add_space(6.0);
            if ui.button("Open project…").clicked() {
                self.request(Request::OpenProject);
            }
            if ui.button("Open a spreadsheet").clicked() {
                self.request(Request::OpenSpreadsheet(None));
            }
            if !self.prefs.recent.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new("Recent").color(t.text_dim));
                let mut open = None;
                for r in self.prefs.recent.iter().take(8) {
                    let name = r.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    if ui.link(name).on_hover_text(r.display().to_string()).clicked() {
                        open = Some(r.clone());
                    }
                }
                if let Some(r) = open {
                    self.open_project(&r);
                }
            }
        });
    }
}

fn coming_soon(ui: &mut Ui, t: &Tokens, view: View) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.3);
        ui.label(RichText::new(view.label()).size(20.0).color(t.text));
        ui.label(
            RichText::new("This view arrives in a later phase (see PLAN.md). Switch back to the Gantt Chart in the view menu.").color(t.text_dim),
        );
    });
}

/// A small icon toggle (the tab bar's sidebar and Inspector switches).
fn icon_button(ui: &mut Ui, t: &Tokens, icon: gridcraft_ui_egui::icons::Icon, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
    if on {
        ui.painter().rect_filled(rect, 4.0, t.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    gridcraft_ui_egui::icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)),
        icon,
        if on { t.text } else { t.text_dim },
    );
    resp
}

/// Colour for critical tasks (the Gantt's red).
pub(crate) fn critical_color(t: &Tokens) -> Color32 {
    if t.dark { Color32::from_rgb(0xf0, 0x6a, 0x6a) } else { Color32::from_rgb(0xd0, 0x3a, 0x3a) }
}

pub(crate) fn section(ui: &mut Ui, t: &Tokens, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title.to_uppercase()).size(11.0).color(t.text_dim).strong());
    ui.add_space(2.0);
}

pub(crate) const ROW_H: f32 = 24.0;

#[cfg(test)]
mod tests;
