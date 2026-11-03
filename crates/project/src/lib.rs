//! PlanCraft's project model: what a project *is*, independent of how it is stored or synced.
//!
//! A [`Project`] is a plain value: project information, tasks in WBS order, dependencies,
//! resources, assignments and the working calendar. `plancraft-project-sync` builds it from the
//! merged Loro document (the stored form) and turns edits into document operations; the scheduler
//! and the views read it. Nothing here does I/O, so it compiles to wasm unchanged.
//!
//! Ids are stable strings chosen by the storage layer (for tasks, Loro tree node ids such as
//! `12@3456`), so links from other apps (RevuCraft punch items, documents) survive tasks being
//! moved, renamed or renumbered. WBS codes (`1.2.3`) are labels computed from the outline.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod date;
mod validate;

use serde::{Deserialize, Serialize};

pub use date::{Date, DateError};
pub use validate::Problem;

/// Working minutes in a default working day (8 h). Durations are stored in working minutes.
pub const MINUTES_PER_DAY: u32 = 8 * 60;

/// The whole project as one value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub info: ProjectInfo,
    /// Every task, in outline (WBS) order: a parent comes before its children.
    pub tasks: Vec<Task>,
    pub dependencies: Vec<Dependency>,
    pub resources: Vec<Resource>,
    pub assignments: Vec<Assignment>,
    pub calendar: Calendar,
    /// The board's columns, left to right.
    #[serde(default = "default_columns")]
    pub board_columns: Vec<String>,
    /// Drawings (PDFs, paths relative to the job folder) whose RevuCraft punch items feed this plan.
    #[serde(default)]
    pub drawings: Vec<String>,
}

/// The project-level information the new project wizard asks for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub title: String,
    pub purpose: String,
    pub client: String,
    pub site_address: String,
    pub start: Option<Date>,
    pub deadline: Option<Date>,
    pub manager: String,
    pub sponsor: String,
    /// Total budget in `currency`; `None` when not set.
    pub budget: Option<f64>,
    pub currency: String,
    pub method: Method,
}

/// How the project is run; picks the starting views and templates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[default]
    Waterfall,
    Agile,
    Hybrid,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Waterfall => "waterfall",
            Method::Agile => "agile",
            Method::Hybrid => "hybrid",
        }
    }

    /// Reads a stored or typed name; anything unknown is waterfall.
    pub fn parse(s: &str) -> Method {
        match s.trim().to_ascii_lowercase().as_str() {
            "agile" | "scrum" => Method::Agile,
            "hybrid" => Method::Hybrid,
            _ => Method::Waterfall,
        }
    }
}

/// A stable task id (see the crate docs).
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One task (or summary task, or milestone).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    /// The summary task above this one; `None` at the top level.
    pub parent: Option<TaskId>,
    /// 1 for top-level tasks.
    pub outline_level: u32,
    /// Computed from the outline: `1`, `1.2`, `1.2.3`.
    pub wbs: String,
    /// True when the task has subtasks (its dates and work roll up from them).
    pub summary: bool,
    pub name: String,
    /// Working minutes (a working day is [`MINUTES_PER_DAY`]).
    pub duration_minutes: u32,
    pub start: Option<Date>,
    pub finish: Option<Date>,
    pub milestone: bool,
    /// 0–100.
    pub percent_complete: u8,
    pub constraint: Constraint,
    pub constraint_date: Option<Date>,
    pub deadline: Option<Date>,
    /// 0–1000, 500 by default (as in MS Project).
    pub priority: u16,
    pub notes: String,
    /// Kept for history instead of deleting (e.g. its RevuCraft punch item was deleted).
    pub cancelled: bool,
    /// Where the task came from when another app made it, e.g. `revucraft:<pdf>#nm=<name>`.
    pub source: String,
    /// The board column the task's card is in (empty: worked out from % complete, see
    /// [`Project::status_of`]).
    pub status: String,
    /// Where the work happens (a room or zone), e.g. a RevuCraft space.
    pub location: String,
}

/// When a task may be scheduled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Constraint {
    /// As soon as possible.
    #[default]
    Asap,
    /// As late as possible.
    Alap,
    /// Start no earlier than.
    Snet,
    /// Start no later than.
    Snlt,
    /// Finish no earlier than.
    Fnet,
    /// Finish no later than.
    Fnlt,
    /// Must start on.
    Mso,
    /// Must finish on.
    Mfo,
}

impl Constraint {
    pub const ALL: [Constraint; 8] = [
        Constraint::Asap,
        Constraint::Alap,
        Constraint::Snet,
        Constraint::Snlt,
        Constraint::Fnet,
        Constraint::Fnlt,
        Constraint::Mso,
        Constraint::Mfo,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Constraint::Asap => "ASAP",
            Constraint::Alap => "ALAP",
            Constraint::Snet => "SNET",
            Constraint::Snlt => "SNLT",
            Constraint::Fnet => "FNET",
            Constraint::Fnlt => "FNLT",
            Constraint::Mso => "MSO",
            Constraint::Mfo => "MFO",
        }
    }

    /// Reads a stored or typed code; anything unknown is ASAP.
    pub fn parse(s: &str) -> Constraint {
        let s = s.trim();
        Constraint::ALL.into_iter().find(|c| c.as_str().eq_ignore_ascii_case(s)).unwrap_or_default()
    }

    /// Whether the constraint needs a date.
    pub fn needs_date(self) -> bool {
        !matches!(self, Constraint::Asap | Constraint::Alap)
    }
}

/// A link between two tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub predecessor: TaskId,
    pub successor: TaskId,
    pub kind: DependencyKind,
    /// Lag in working minutes; negative is lead time.
    pub lag_minutes: i32,
}

/// Finish-to-start and the other three link types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DependencyKind {
    #[default]
    Fs,
    Ss,
    Ff,
    Sf,
}

impl DependencyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DependencyKind::Fs => "FS",
            DependencyKind::Ss => "SS",
            DependencyKind::Ff => "FF",
            DependencyKind::Sf => "SF",
        }
    }

    /// Reads a stored or typed code; anything unknown is FS.
    pub fn parse(s: &str) -> DependencyKind {
        match s.trim().to_ascii_uppercase().as_str() {
            "SS" => DependencyKind::Ss,
            "FF" => DependencyKind::Ff,
            "SF" => DependencyKind::Sf,
            _ => DependencyKind::Fs,
        }
    }
}

/// A person, piece of equipment, material or cost that tasks use.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub kind: ResourceKind,
    /// 1.0 = one full-time person.
    pub max_units: f64,
    pub role: String,
    pub email: String,
    pub phone: String,
    /// Bar colour on the Gantt chart, `#rrggbb`.
    pub color: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    #[default]
    Work,
    Material,
    Cost,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceKind::Work => "work",
            ResourceKind::Material => "material",
            ResourceKind::Cost => "cost",
        }
    }

    pub fn parse(s: &str) -> ResourceKind {
        match s.trim().to_ascii_lowercase().as_str() {
            "material" => ResourceKind::Material,
            "cost" => ResourceKind::Cost,
            _ => ResourceKind::Work,
        }
    }
}

/// A resource working on a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: String,
    pub task: TaskId,
    pub resource: String,
    /// 1.0 = full time.
    pub units: f64,
}

/// The project's working time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    /// Monday … Sunday.
    pub working_days: [bool; 7],
    pub hours_per_day: f64,
    /// Non-working days (holidays, shutdowns), sorted.
    pub holidays: Vec<Date>,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar { working_days: [true, true, true, true, true, false, false], hours_per_day: 8.0, holidays: Vec::new() }
    }
}

impl Calendar {
    /// Whether work happens on `d`.
    pub fn is_working_day(&self, d: Date) -> bool {
        self.working_days.get(d.weekday_from_monday()).copied().unwrap_or(false) && self.holidays.binary_search(&d).is_err()
    }
}

/// The board columns a new project starts with.
pub const DEFAULT_COLUMNS: [&str; 5] = ["Backlog", "To Do", "In Progress", "Review", "Done"];

fn default_columns() -> Vec<String> {
    DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect()
}

impl Project {
    /// The board's columns (the defaults when none are set). The last column means done.
    pub fn columns(&self) -> Vec<String> {
        if self.board_columns.is_empty() { default_columns() } else { self.board_columns.clone() }
    }

    /// The board column a task is in: its own status when that names a column, otherwise from its
    /// progress: done (100%) is the last column; started is a column named like "In Progress" or
    /// "Doing" (else the middle one); not started is one named like "To Do" (else the first).
    pub fn status_of(&self, t: &Task) -> String {
        let cols = self.columns();
        if let Some(c) = cols.iter().find(|c| c.eq_ignore_ascii_case(t.status.trim())) {
            return c.clone();
        }
        let named = |words: &[&str]| cols.iter().find(|c| words.iter().any(|w| c.to_lowercase().replace(['-', ' '], "").contains(w))).cloned();
        let pick = match t.percent_complete {
            0 => named(&["todo", "ready"]).or_else(|| cols.first().cloned()),
            100.. => cols.last().cloned(),
            _ => named(&["progress", "doing", "active"]).or_else(|| cols.get(cols.len() / 2).cloned()),
        };
        pick.unwrap_or_default()
    }

    /// The task with this id.
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.iter().find(|t| &t.id == id)
    }

    /// The task with this WBS code (`1.2`) or id.
    pub fn find_task(&self, key: &str) -> Option<&Task> {
        let key = key.trim();
        self.tasks.iter().find(|t| t.wbs == key).or_else(|| self.tasks.iter().find(|t| t.id.0 == key))
    }

    /// The direct subtasks of `parent` (top-level tasks for `None`), in outline order.
    pub fn children<'a>(&'a self, parent: Option<&'a TaskId>) -> impl Iterator<Item = &'a Task> + 'a {
        self.tasks.iter().filter(move |t| t.parent.as_ref() == parent)
    }

    /// The resource with this id or name (case-insensitive).
    pub fn find_resource(&self, key: &str) -> Option<&Resource> {
        let key = key.trim();
        self.resources.iter().find(|r| r.id == key).or_else(|| self.resources.iter().find(|r| r.name.eq_ignore_ascii_case(key)))
    }

    /// Everything inconsistent about the project (dangling links, dependency loops…), for a
    /// warning panel and for tests. Empty for a healthy project.
    pub fn problems(&self) -> Vec<Problem> {
        validate::problems(self)
    }
}

#[cfg(test)]
mod tests;
