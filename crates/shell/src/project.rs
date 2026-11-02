//! An open project: its folder, the merged project, the schedule, and the view state.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use plancraft_project::{Date, Project, TaskId};
use plancraft_project_sync::ProjectFolder;
use plancraft_schedule::Schedule;

/// How often other devices' files are checked, in milliseconds.
const POLL_MS: f64 = 2000.0;
/// The largest `markups.json` read (a real punch list is a few KiB).
const MAX_MARKUPS_BYTES: u64 = 4 * 1024 * 1024;

pub struct OpenProject {
    folder: ProjectFolder,
    pub project: Project,
    pub schedule: Schedule,
    /// Selected tasks (the first is the Inspector's), in click order.
    pub selected: Vec<TaskId>,
    /// Summary tasks shown collapsed.
    pub collapsed: HashSet<TaskId>,
    /// Gantt scale: pixels per day.
    pub day_px: f32,
    /// Gantt horizontal scroll, in days from the project start.
    pub scroll_days: f32,
    /// Width of the task table left of the chart.
    pub table_w: f32,
    /// The row being edited in place: (task, column, text).
    pub editing: Option<(TaskId, &'static str, String)>,
    /// The month the Calendar view shows (year, month); `None` follows the project start.
    pub cal_month: Option<(i32, u32)>,
    /// The Calendar view shows summary tasks too.
    pub cal_summaries: bool,
    /// The Board shows only the tasks under this summary task.
    pub board_under: Option<TaskId>,
    last_poll: f64,
    /// Each linked drawing's `markups.json` as last read: (size, modified time).
    markups_seen: HashMap<String, (u64, Option<SystemTime>)>,
}

impl OpenProject {
    pub fn open(path: &Path, device: &str, today: Date) -> Result<OpenProject, String> {
        let folder = ProjectFolder::open(path, device).map_err(|e| e.to_string())?;
        Ok(Self::from_folder(folder, today))
    }

    pub fn from_folder(folder: ProjectFolder, today: Date) -> OpenProject {
        let project = folder.doc().project();
        let schedule = plancraft_schedule::schedule(&project, today);
        OpenProject {
            folder,
            project,
            schedule,
            selected: Vec::new(),
            collapsed: HashSet::new(),
            day_px: 26.0,
            scroll_days: -1.0,
            table_w: 470.0,
            editing: None,
            cal_month: None,
            cal_summaries: false,
            board_under: None,
            last_poll: 0.0,
            markups_seen: HashMap::new(),
        }
    }

    pub fn dir(&self) -> &Path {
        self.folder.dir()
    }

    pub fn entry_path(&self) -> PathBuf {
        let stem = self.folder.dir().file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.folder.dir().join(format!("{stem}.{}", plancraft_project_sync::ENTRY_EXTENSION))
    }

    pub fn folder(&self) -> &ProjectFolder {
        &self.folder
    }

    /// The job folder: the folder that holds the `.plan` folder, where linked drawings live.
    pub fn job_root(&self) -> Option<PathBuf> {
        self.folder.dir().parent().map(Path::to_path_buf)
    }

    /// The linked drawings whose `markups.json` changed since they were last read, with the
    /// file's path. Reading them is the caller's job.
    pub fn take_changed_markups(&mut self) -> Vec<(String, PathBuf)> {
        let Some(root) = self.job_root() else { return Vec::new() };
        let mut out = Vec::new();
        for drawing in &self.project.drawings {
            let path = markups_path(&root, drawing);
            let Ok(meta) = std::fs::metadata(&path) else {
                self.markups_seen.remove(drawing);
                continue;
            };
            let seen = (meta.len(), meta.modified().ok());
            if self.markups_seen.get(drawing) != Some(&seen) {
                self.markups_seen.insert(drawing.clone(), seen);
                out.push((drawing.clone(), path));
            }
        }
        out
    }

    /// Forgets every punch list read so far, so the next check reads them all again.
    pub fn forget_markups(&mut self) {
        self.markups_seen.clear();
    }

    pub fn title(&self) -> String {
        if self.project.info.title.is_empty() { self.folder.entry().title.clone() } else { self.project.info.title.clone() }
    }

    /// The first selected task.
    pub fn current(&self) -> Option<&TaskId> {
        self.selected.first()
    }

    pub fn select_only(&mut self, id: TaskId) {
        self.selected = vec![id];
    }

    /// Runs a command, saves this device's file and refreshes the view.
    pub fn run(&mut self, id: &str, params: &serde_json::Value, today: Date) -> Result<serde_json::Value, String> {
        let r = plancraft_commands::run(self.folder.doc(), id, params)?;
        self.folder.save().map_err(|e| format!("couldn't save: {e}"))?;
        self.refresh(today);
        Ok(r)
    }

    pub fn refresh(&mut self, today: Date) {
        self.project = self.folder.doc().project();
        self.schedule = plancraft_schedule::schedule(&self.project, today);
        let alive: HashSet<&TaskId> = self.project.tasks.iter().map(|t| &t.id).collect();
        self.selected.retain(|s| alive.contains(s));
        self.collapsed.retain(|s| alive.contains(s));
    }

    /// Picks up other devices' saves (at most every couple of seconds).
    pub fn poll(&mut self, now_ms: f64, today: Date) {
        if now_ms - self.last_poll < POLL_MS {
            return;
        }
        self.last_poll = now_ms;
        if let Ok(true) = self.folder.reload() {
            self.refresh(today);
        }
    }

    /// Rows to show, in outline order: tasks under a collapsed summary are hidden, and the filter
    /// keeps matching tasks with their summaries.
    pub fn visible_rows(&self, filter: crate::Filter) -> Vec<usize> {
        let mut out = Vec::new();
        let mut hidden_under: Vec<&TaskId> = Vec::new();
        let keep = |i: usize| -> bool {
            let Some(t) = self.project.tasks.get(i) else { return false };
            let d = self.schedule.get(&t.id);
            match filter {
                crate::Filter::All => true,
                crate::Filter::Critical => !t.summary && d.is_some_and(|d| d.critical),
                crate::Filter::Late => !t.summary && d.is_some_and(|d| d.total_slack < 0),
                crate::Filter::Milestones => t.milestone || (t.duration_minutes == 0 && !t.summary),
                crate::Filter::Incomplete => !t.summary && t.percent_complete < 100,
                crate::Filter::Completed => !t.summary && t.percent_complete >= 100,
            }
        };
        // With a filter, show matching tasks and the summaries above them.
        let mut wanted: HashSet<&TaskId> = HashSet::new();
        if filter != crate::Filter::All {
            for (i, t) in self.project.tasks.iter().enumerate() {
                if keep(i) {
                    wanted.insert(&t.id);
                    let mut p = t.parent.as_ref();
                    while let Some(pid) = p {
                        if !wanted.insert(pid) {
                            break;
                        }
                        p = self.project.task(pid).and_then(|x| x.parent.as_ref());
                    }
                }
            }
        }
        for (i, t) in self.project.tasks.iter().enumerate() {
            while let Some(h) = hidden_under.last() {
                if t.parent.as_ref().is_some_and(|p| self.is_under(p, h)) {
                    break;
                }
                hidden_under.pop();
            }
            if !hidden_under.is_empty() {
                continue;
            }
            if filter == crate::Filter::All || wanted.contains(&t.id) {
                out.push(i);
            }
            if self.collapsed.contains(&t.id) {
                hidden_under.push(&t.id);
            }
        }
        out
    }

    /// Whether `id` is `ancestor` or under it.
    fn is_under(&self, id: &TaskId, ancestor: &TaskId) -> bool {
        let mut cur = Some(id);
        for _ in 0..=self.project.tasks.len() {
            match cur {
                Some(c) if c == ancestor => return true,
                Some(c) => cur = self.project.task(c).and_then(|t| t.parent.as_ref()),
                None => return false,
            }
        }
        false
    }
}

/// Where RevuCraft writes a drawing's punch list: `markups.json` in the drawing's job folder, which
/// sits beside the PDF and has the PDF's name without `.pdf` (`Floor/Plan.pdf` → `Floor/Plan/`).
pub fn markups_path(root: &Path, drawing: &str) -> PathBuf {
    root.join(Path::new(drawing).with_extension("")).join("markups.json")
}

/// A drawing's punch list as text. `Ok(None)` when RevuCraft hasn't written it yet.
pub fn read_markups(path: &Path) -> Result<Option<String>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if meta.len() > MAX_MARKUPS_BYTES {
        return Err(format!("{} is larger than 4 MB, so it isn't a punch list", path.display()));
    }
    std::fs::read_to_string(path).map(Some).map_err(|e| format!("couldn't read {}: {e}", path.display()))
}
