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
mod documents;
mod gantt;
mod inspector;
pub mod markdown;
pub mod panes;
mod project;
mod ribbon;
mod sidebar;
mod theme;
mod toolbar;
mod wizard;

use gridcraft_ui_egui::theme::Tokens;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};
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
    Settings,
}

impl Panel {
    pub const ALL: [Panel; 8] =
        [Panel::Explorer, Panel::Search, Panel::Bookmarks, Panel::Outline, Panel::Filters, Panel::Recent, Panel::Sync, Panel::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Panel::Explorer => "Explorer",
            Panel::Search => "Search",
            Panel::Bookmarks => "Bookmarks",
            Panel::Outline => "Outline",
            Panel::Filters => "Filters",
            Panel::Recent => "Recent",
            Panel::Sync => "Sync",
            Panel::Settings => "Settings",
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

/// What a pane shows: a view of the current project, or the spreadsheet UI (drawn by the host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    View(View),
    Spreadsheet,
    /// The Markdown documents of the current project: file list and editor.
    Documents,
    /// The Properties: the selected task's details, or the project's information.
    Inspector,
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
    pub panel: Option<Panel>,
    /// Link this device's projects to RevuCraft's punch lists (off by default).
    pub revucraft_link: bool,
    /// The pane layout (splits and the tabs in each pane), kept between runs.
    pub layout: Option<panes::Pane>,
    /// RevuCraft's accent colour for this device (amber unless changed).
    pub accent: theme::Accent,
    /// What the tool icons are drawn in (RevuCraft's choice; the accent unless changed).
    pub icon_color: theme::IconColor,
}

/// The whole shell.
pub struct Shell {
    pub projects: Vec<OpenProject>,
    /// The project the window shows (an index into `projects`); none on the start page.
    pub current: Option<usize>,
    /// The panes: a tree of splits whose leaves are tab groups.
    pub layout: panes::Pane,
    /// The leaf (pane) the ribbon acts on, counting panes from the top-left.
    pub focus: usize,
    /// Panes floating over the window (not kept between runs).
    pub floating: Vec<panes::Float>,
    /// The Documents tab's editor state.
    pub documents: documents::DocState,
    /// The app's clock at the last frame, in milliseconds (the shell never reads the clock itself).
    pub clock_ms: f64,
    pub panel: Panel,
    pub left_open: bool,
    /// Read mode: the ribbon steps away, so the window is for reading (not kept between runs).
    pub read_mode: bool,
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
            .unwrap_or(ShellPrefs { left_open: true, ..ShellPrefs::default() });
        let layout = prefs.layout.clone().filter(|l| l.leaf_count() > 0).unwrap_or_default();
        Shell {
            projects: Vec::new(),
            current: None,
            layout,
            focus: 0,
            floating: Vec::new(),
            documents: documents::DocState::default(),
            clock_ms: 0.0,
            panel: prefs.panel.unwrap_or(Panel::Explorer),
            left_open: prefs.left_open,
            read_mode: false,
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
                self.current = Some(self.projects.len() - 1);
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Makes project `i` the one the window shows.
    pub(crate) fn focus_project(&mut self, i: usize) {
        if i < self.projects.len() {
            self.current = Some(i);
        }
    }

    /// Closes the project the window shows: its open document is saved, and the project is
    /// unloaded from this window. Its files stay where they are. The window returns to the
    /// launch page (a standalone spreadsheet tab is kept).
    pub fn close_project(&mut self) {
        let Some(pi) = self.active_project() else { return };
        self.flush_document();
        self.projects.remove(pi);
        self.current = None;
        self.documents = documents::DocState::default();
        let keep_sheet = (0..self.layout.leaf_count()).any(|i| self.layout.leaf(i).is_some_and(|l| l.tabs.contains(&Tab::Spreadsheet)));
        self.layout =
            if keep_sheet { panes::Pane::Leaf(panes::Leaf { tabs: vec![Tab::Spreadsheet], active: 0, pinned: 0 }) } else { panes::Pane::default() };
        self.focus = 0;
        self.floating.clear();
        self.save_prefs();
    }

    /// Saves the open Markdown document, so nothing is lost when the app closes.
    pub fn save_open_work(&mut self) {
        self.flush_document();
    }

    /// Shows the Properties tab: focuses one that is open, or adds one to the focused pane.
    pub fn open_inspector(&mut self) {
        self.open_tab(Tab::Inspector);
    }

    /// Shows a tab: focuses the pane that has it, or adds it to the focused pane.
    pub fn open_tab(&mut self, tab: Tab) {
        let found = (0..self.layout.leaf_count()).find(|&i| self.layout.leaf(i).is_some_and(|l| l.tabs.contains(&tab)));
        match found {
            Some(i) => self.focus = i,
            None => {
                if let Some(leaf) = self.layout.leaf_mut(self.focus) {
                    leaf.tabs.push(tab);
                    leaf.active = leaf.tabs.len() - 1;
                }
            }
        }
        self.save_prefs();
    }

    /// Shows the spreadsheet tab (adding it if needed).
    pub fn show_spreadsheet_tab(&mut self) {
        let found = (0..self.layout.leaf_count()).find(|&i| self.layout.leaf(i).is_some_and(|l| l.tabs.contains(&Tab::Spreadsheet)));
        match found {
            Some(i) => self.focus = i,
            None => {
                if let Some(leaf) = self.layout.leaf_mut(self.focus) {
                    leaf.tabs.push(Tab::Spreadsheet);
                    leaf.active = leaf.tabs.len() - 1;
                }
            }
        }
    }

    /// The tab the focused pane shows.
    /// Shows a view tab in the focused pane: switches to it there, or adds it to the pane.
    pub fn show_tab(&mut self, tab: Tab) {
        let found = (0..self.layout.leaf_count()).find(|&i| self.layout.leaf(i).is_some_and(|l| l.tabs.contains(&tab)));
        if let Some(i) = found {
            self.focus = i;
        }
        if let Some(leaf) = self.layout.leaf_mut(self.focus) {
            match leaf.tabs.iter().position(|t| *t == tab) {
                Some(idx) => leaf.active = idx,
                None => {
                    leaf.tabs.push(tab);
                    leaf.active = leaf.tabs.len() - 1;
                }
            }
        }
        self.save_prefs();
    }

    pub fn focused_tab(&self) -> Tab {
        self.layout.leaf(self.focus).map(|l| l.tab()).unwrap_or(Tab::View(View::Gantt))
    }

    /// Changes the tab the focused pane shows (the ribbon's View picker uses this).
    pub fn set_focused_tab(&mut self, tab: Tab) {
        if let Some(leaf) = self.layout.leaf_mut(self.focus) {
            leaf.set_tab(tab);
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
        self.prefs.layout = Some(self.layout.clone());
        self.prefs.left_open = self.left_open;
        self.prefs.panel = Some(self.panel);
        let Some(dir) = &self.settings_dir else { return };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_vec_pretty(&self.prefs) {
            let _ = std::fs::write(dir.join("shell.json"), json);
        }
    }

    /// The project shown in the active tab.
    pub fn active_project(&self) -> Option<usize> {
        self.current.filter(|&i| i < self.projects.len())
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
        self.clock_ms = now_ms;
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
        let t = crate::theme::tokens(ui.ctx());
        self.title_bar(ui, &t);
        self.menu_bar(ui, &t);
        theme::remember(ui.ctx(), self.prefs.accent, self.prefs.icon_color);
        let on_sheet = self.focused_tab() == Tab::Spreadsheet;
        ribbon::show(self, ui, &t);
        self.message_bar(ui, &t);
        self.status_bar(ui, &t);
        sidebar::show(self, ui, &t);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(if on_sheet { t.window } else { t.grid_bg })).show(ui, |ui| {
            panes::show(self, ui, &t, spreadsheet);
        });
        if self.wizard.is_some() {
            wizard::show(self, ui.ctx());
        }
    }

    /// The title bar along the top: the project's title and the app's name.
    fn title_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let title = self
            .active_project()
            .and_then(|i| self.projects.get(i))
            .map(|p| format!("{} - PlanCraft", p.title()))
            .unwrap_or_else(|| "PlanCraft".to_string());
        egui::Panel::top("pc_title").exact_size(24.0).frame(egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin::symmetric(10, 2))).show(
            ui,
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(title).size(12.0).color(t.text_dim));
                });
            },
        );
    }

    /// The menu row: home (the launch page), the menus, the view tabs of the open project, and at
    /// the right the search box, undo and redo (greyed out until they exist), and save.
    fn menu_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let has_project = self.active_project().is_some();
        egui::Panel::top("pc_menu").exact_size(32.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(egui::Margin::symmetric(8, 2))).show(
            ui,
            |ui| {
                ui.horizontal_centered(|ui| {
                    if icon_button(ui, t, gridcraft_ui_egui::icons::Icon::Home, false)
                        .on_hover_text("Home: the launch page, to open or create projects")
                        .clicked()
                    {
                        self.flush_document();
                        self.current = None;
                        self.documents = documents::DocState::default();
                    }
                    ui.menu_button("File", |ui| {
                        if ui.button("New project…").clicked() {
                            self.start_wizard();
                            ui.close();
                        }
                        if ui.button("Open project…").clicked() {
                            self.request(Request::OpenProject);
                            ui.close();
                        }
                        ui.add_enabled_ui(has_project, |ui| {
                            if ui.button("Close project").clicked() {
                                self.close_project();
                                ui.close();
                            }
                        });
                    });
                    ui.menu_button("Edit", |ui| {
                        let pi = self.active_project();
                        if ui.add_enabled(pi.is_some(), egui::Button::new("Indent")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::on_current(self, pi, "task.indent");
                            }
                            ui.close();
                        }
                        if ui.add_enabled(pi.is_some(), egui::Button::new("Outdent")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::on_current(self, pi, "task.outdent");
                            }
                            ui.close();
                        }
                        if ui.add_enabled(pi.is_some(), egui::Button::new("Delete")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::delete_selected(self, pi);
                            }
                            ui.close();
                        }
                        ui.separator();
                        if ui.add_enabled(pi.is_some(), egui::Button::new("New task")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::add_task(self, pi, false);
                            }
                            ui.close();
                        }
                        if ui.add_enabled(pi.is_some(), egui::Button::new("New milestone")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::add_task(self, pi, true);
                            }
                            ui.close();
                        }
                        if ui.add_enabled(pi.is_some(), egui::Button::new("Link tasks")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::link_selected(self, pi, false);
                            }
                            ui.close();
                        }
                        if ui.add_enabled(pi.is_some(), egui::Button::new("Unlink tasks")).clicked() {
                            if let Some(pi) = pi {
                                ribbon::link_selected(self, pi, true);
                            }
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        let sidebar = if self.left_open { "Hide sidebar" } else { "Show sidebar" };
                        if ui.button(sidebar).clicked() {
                            self.left_open = !self.left_open;
                            self.save_prefs();
                            ui.close();
                        }
                        let read = if self.read_mode { "Leave read mode" } else { "Read mode" };
                        if ui.add_enabled(has_project, egui::Button::new(read)).clicked() {
                            self.read_mode = !self.read_mode;
                            ui.close();
                        }
                        ui.separator();
                        ui.add_enabled_ui(has_project, |ui| {
                            if ui.button("Properties").clicked() {
                                self.open_inspector();
                                ui.close();
                            }
                            if ui.button("Documents").clicked() {
                                self.open_tab(Tab::Documents);
                                ui.close();
                            }
                            if ui.button("Spreadsheet").clicked() {
                                self.show_spreadsheet_tab();
                                ui.close();
                            }
                        });
                        ui.separator();
                        ui.menu_button("Accent colour", |ui| {
                            for accent in theme::Accent::ALL {
                                if ui.selectable_label(self.prefs.accent == accent, accent.label()).clicked() {
                                    self.prefs.accent = accent;
                                    self.save_prefs();
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("Tool icons", |ui| {
                            for choice in theme::IconColor::all() {
                                if ui.selectable_label(self.prefs.icon_color == choice, choice.label()).clicked() {
                                    self.prefs.icon_color = choice;
                                    self.save_prefs();
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About PlanCraft").clicked() {
                            self.message = Some(format!("PlanCraft {}: a project planner built on GridCraft.", env!("CARGO_PKG_VERSION")));
                            ui.close();
                        }
                    });
                    if has_project {
                        ui.add_space(8.0);
                        let views = [
                            ("Documents", Tab::Documents),
                            ("GANTT", Tab::View(View::Gantt)),
                            ("Kanban", Tab::View(View::Kanban)),
                            ("Spreadsheet", Tab::Spreadsheet),
                            ("Calendar", Tab::View(View::Calendar)),
                        ];
                        for (label, tab) in views {
                            let on = self.focused_tab() == tab;
                            if ui.selectable_label(on, label).clicked() {
                                self.show_tab(tab);
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, t, gridcraft_ui_egui::icons::Icon::Save, false)
                            .on_hover_text(if self.documents.dirty_since.is_some() { "Save the open document" } else { "Everything is saved" })
                            .clicked()
                        {
                            self.flush_document();
                        }
                        ui.add_enabled(false, egui::Button::new("Redo")).on_disabled_hover_text("Redo is not in PlanCraft yet");
                        ui.add_enabled(false, egui::Button::new("Undo")).on_disabled_hover_text("Undo is not in PlanCraft yet");
                        let search = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search").desired_width(200.0));
                        if search.changed() || search.gained_focus() {
                            self.panel = Panel::Search;
                            self.left_open = true;
                        }
                    });
                });
            },
        );
    }

    /// The status bar along the bottom: the project's title, read mode, and the device.
    fn status_bar(&mut self, ui: &mut Ui, t: &Tokens) {
        let title = self.active_project().and_then(|i| self.projects.get(i)).map(|p| p.title()).unwrap_or_else(|| "No project open".to_string());
        let read = self.read_mode;
        let device = self.device.clone();
        let facts = self.active_project().and_then(|i| self.projects.get(i)).map(|p| (p.schedule.tasks.len(), p.schedule.finish.to_string()));
        let unsaved = self.documents.dirty_since.is_some();
        egui::Panel::bottom("pc_status").exact_size(26.0).frame(egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin::symmetric(10, 2))).show(
            ui,
            |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(title).size(12.0).color(t.text_dim));
                    if let Some((tasks, finish)) = facts {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("{tasks} tasks")).size(12.0).color(t.text_dim));
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("Finish {finish}")).size(12.0).color(t.text_dim));
                    }
                    if read {
                        ui.add_space(12.0);
                        ui.label(RichText::new("Read mode").size(12.0).color(t.text_dim));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(device).size(12.0).color(t.text_dim));
                        ui.add_space(12.0);
                        let state = if unsaved { "Unsaved" } else { "Saved" };
                        ui.label(RichText::new(state).size(12.0).color(t.text_dim));
                    });
                });
            },
        );
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

    pub(crate) fn start_page(&mut self, ui: &mut Ui, t: &Tokens) {
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

pub(crate) fn coming_soon(ui: &mut Ui, t: &Tokens, view: View) {
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
        theme::tool_icon(ui.ctx()),
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

/// One tab in the tab bar: its title, and a × to close it when `closable`.
/// Returns (title clicked, × clicked).
/// One tab in a pane's tab strip: its title, and a × to close it when `closable`.
/// Returns the title's response (for clicks and menus) and whether the × was clicked.
/// A tab in a pane's strip: its title and a × to close it. A pinned tab has no ×, but keeps the
/// ×'s space (blank), so pinning doesn't shrink it.
pub(crate) fn tab_button(ui: &mut Ui, t: &Tokens, title: &str, active: bool, pinned: bool) -> (egui::Response, bool) {
    let mut label = None;
    let mut closed = false;
    let fill = if active { t.tab_active } else { t.tab_bar };
    egui::Frame::NONE.fill(fill).corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(10, 5)).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                label =
                    Some(ui.add(egui::Label::new(RichText::new(title).color(if active { t.text } else { t.text_dim })).sense(egui::Sense::click())));
                let close = egui::Button::new(RichText::new("×").color(t.text_dim)).frame(false).small();
                if pinned {
                    ui.add_visible(false, close);
                } else if ui.add(close).on_hover_text("Close tab").clicked() {
                    closed = true;
                }
            });
        },
    );
    let resp = label.unwrap_or_else(|| ui.label(""));
    (resp, closed)
}
