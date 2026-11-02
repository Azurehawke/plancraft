//! RevuCraft's punch list, read from `markups.json` (format 1; `docs/sidecar.md` in RevuCraft's
//! repository). RevuCraft writes the file beside each linked drawing on every save, and PlanCraft
//! only reads it. Each punch item becomes a task, identified by the markup's permanent name
//! (its PDF `/NM`), so renaming or renumbering a markup keeps its task.
//!
//! What RevuCraft owns, PlanCraft copies on every sync: the task's name, due date, assignee and
//! status (and progress, when the item is closed). What PlanCraft owns, a sync never touches: the
//! outline position, dependencies, duration and constraints. A punch item that disappears from the
//! file cancels its task; one that comes back restores it.
//!
//! Two devices can each create the task for the same punch item before they sync. Every sync keeps
//! the task with the smallest id for each punch item and cancels the others, so devices converge.

use plancraft_project::{Date, Task, TaskId};
use serde::Deserialize;

use crate::{Error, Placement, ProjectDoc, Result};

/// The `format` value every sidecar carries.
pub const FORMAT: &str = "revucraft-markups";
/// The only sidecar version this reader understands.
pub const VERSION: u32 = 1;
/// A drawing's punch list or spaces, past this many rows, is refused (hostile input).
const MAX_ROWS: usize = 50_000;
/// The prefix of a task's `source` when RevuCraft made it.
const SOURCE_PREFIX: &str = "revucraft:";

/// The file as RevuCraft writes it. Unknown fields are ignored, so a newer RevuCraft that adds
/// fields still reads; a newer `version` is refused.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Sidecar {
    pub format: String,
    pub version: u32,
    pub punch: Vec<PunchRow>,
    pub spaces: Vec<SpaceRow>,
}

/// One punch item.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PunchRow {
    /// The markup's permanent name (its PDF `/NM`).
    pub name: String,
    pub page: u32,
    /// The punch number drawn on the sheet.
    pub no: u32,
    pub assignee: String,
    /// `YYYY-MM-DD`, or empty.
    pub due: String,
    /// `Open`, `InProgress`, `Ready` or `Closed`.
    pub status: String,
    pub closed: bool,
    pub condition: Option<String>,
    /// The space the item is in, by its label, or none.
    pub space: Option<String>,
}

/// One space (a named room or zone).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SpaceRow {
    pub name: String,
    pub page: u32,
    pub label: String,
}

/// What a sync changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Tasks made for new punch items.
    pub added: usize,
    /// Tasks whose name, due date, assignee, status or progress changed.
    pub updated: usize,
    /// Tasks cancelled: their punch item is gone, or a duplicate from another device.
    pub cancelled: usize,
    /// Cancelled tasks whose punch item came back.
    pub restored: usize,
}

/// Reads a sidecar. Refuses a file that isn't one, or one in a format this PlanCraft can't read.
pub fn parse(json: &str) -> Result<Sidecar> {
    let sidecar: Sidecar = serde_json::from_str(json).map_err(|e| Error::Sidecar(e.to_string()))?;
    if sidecar.format != FORMAT {
        return Err(Error::Sidecar("not a RevuCraft markups file".into()));
    }
    if sidecar.version != VERSION {
        return Err(Error::Sidecar(format!("format version {} isn't supported (this PlanCraft reads {VERSION})", sidecar.version)));
    }
    if sidecar.punch.len() > MAX_ROWS || sidecar.spaces.len() > MAX_ROWS {
        return Err(Error::Sidecar(format!("more than {MAX_ROWS} rows")));
    }
    Ok(sidecar)
}

/// The `source` a task made from a punch item carries: `revucraft:<drawing>#nm=<name>`.
pub fn source_of(drawing: &str, name: &str) -> String {
    format!("{SOURCE_PREFIX}{drawing}#nm={name}")
}

/// The `source` prefix every punch item of one drawing shares.
fn drawing_prefix(drawing: &str) -> String {
    format!("{SOURCE_PREFIX}{drawing}#nm=")
}

/// The board column a RevuCraft status goes to (the default column names).
fn column_of(status: &str, closed: bool) -> &'static str {
    if closed {
        return "Done";
    }
    match status {
        "InProgress" => "In Progress",
        "Ready" => "Review",
        "Closed" => "Done",
        _ => "To Do",
    }
}

/// The task name for a punch item: its number, and its space when it has one.
fn name_of(row: &PunchRow) -> String {
    match row.space.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(space) => format!("Punch #{} ({space})", row.no),
        None => format!("Punch #{}", row.no),
    }
}

impl ProjectDoc {
    /// Brings the tasks of one drawing up to date with its punch list. Only changed fields are
    /// written, so running the same sync twice writes nothing the second time.
    pub fn apply_punch(&self, drawing: &str, sidecar: &Sidecar) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        let prefix = drawing_prefix(drawing);
        let project = self.project();

        // Every task this drawing made, grouped by punch item, lowest id first.
        let mut mine: Vec<&Task> = project.tasks.iter().filter(|t| t.source.starts_with(&prefix)).collect();
        mine.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        let mut keep: Vec<(String, TaskId)> = Vec::new();
        for task in &mine {
            match keep.iter().find(|(source, _)| *source == task.source) {
                None => keep.push((task.source.clone(), task.id.clone())),
                Some(_) => {
                    if !task.cancelled {
                        self.set_task(&task.id, "cancelled", "true")?;
                        report.cancelled += 1;
                    }
                }
            }
        }

        let mut seen: Vec<String> = Vec::new();
        for row in sidecar.punch.iter().filter(|r| !r.name.trim().is_empty()) {
            let source = source_of(drawing, &row.name);
            seen.push(source.clone());
            let existing = keep.iter().find(|(s, _)| *s == source).map(|(_, id)| id.clone());
            let id = match existing {
                Some(id) => id,
                None => {
                    let id = self.add_task(&name_of(row), &Placement::End { parent: None })?;
                    self.set_task(&id, "source", &source)?;
                    // A punch item is a day's work until its schedule says otherwise.
                    self.set_task(&id, "duration", "1d")?;
                    report.added += 1;
                    id
                }
            };
            let current = self.project().task(&id).cloned();
            let Some(current) = current else { continue };
            let mut changed = false;
            if current.cancelled {
                self.set_task(&id, "cancelled", "false")?;
                report.restored += 1;
                changed = true;
            }
            let name = name_of(row);
            if current.name != name {
                self.set_task(&id, "name", &name)?;
                changed = true;
            }
            let due = row.due.trim();
            let deadline = if due.is_empty() { None } else { due.parse::<Date>().ok() };
            let want_deadline = deadline.map(|d| d.to_string()).unwrap_or_default();
            if current.deadline.map(|d| d.to_string()).unwrap_or_default() != want_deadline {
                self.set_task(&id, "deadline", &want_deadline)?;
                changed = true;
            }
            let column = column_of(&row.status, row.closed);
            if current.status != column {
                self.set_task(&id, "status", column)?;
                changed = true;
            }
            let percent = if row.closed || column == "Done" {
                100
            } else if current.percent_complete == 100 {
                0
            } else {
                current.percent_complete
            };
            if current.percent_complete != percent {
                self.set_task(&id, "percent", &percent.to_string())?;
                changed = true;
            }
            if self.sync_assignee(&id, row.assignee.trim())? {
                changed = true;
            }
            if changed {
                report.updated += 1;
            }
        }

        // Punch items that were deleted in RevuCraft: their tasks are cancelled, not removed.
        for (source, id) in &keep {
            if !seen.contains(source) && !self.project().task(id).is_some_and(|t| t.cancelled) {
                self.set_task(id, "cancelled", "true")?;
                report.cancelled += 1;
            }
        }
        Ok(report)
    }

    /// Makes the task's only assignment the named person (adding the person when new), or none
    /// when the name is empty. Returns whether anything changed.
    fn sync_assignee(&self, task: &TaskId, name: &str) -> Result<bool> {
        let project = self.project();
        let wanted = if name.is_empty() {
            None
        } else {
            match project.resources.iter().find(|r| r.name.trim().eq_ignore_ascii_case(name)) {
                Some(r) => Some(r.id.clone()),
                None => Some(self.add_resource(name)?),
            }
        };
        let mut changed = false;
        let mut has_wanted = false;
        for a in project.assignments.iter().filter(|a| a.task == *task) {
            if Some(&a.resource) == wanted.as_ref() {
                has_wanted = true;
            } else {
                self.unassign(&a.id)?;
                changed = true;
            }
        }
        if let (Some(resource), false) = (wanted, has_wanted) {
            self.assign(task, &resource, 1.0)?;
            changed = true;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> ProjectDoc {
        let d = ProjectDoc::new();
        d.set_info("title", "Office fit-out").unwrap();
        d
    }

    fn sidecar(punch: &str) -> Sidecar {
        parse(&format!(r#"{{"format":"revucraft-markups","version":1,"punch":[{punch}],"spaces":[]}}"#)).unwrap()
    }

    fn item(name: &str, no: u32, assignee: &str, due: &str, status: &str) -> String {
        let closed = status == "Closed";
        format!(r#"{{"name":"{name}","page":0,"no":{no},"assignee":"{assignee}","due":"{due}","status":"{status}","closed":{closed}}}"#)
    }

    fn task_for<'a>(p: &'a plancraft_project::Project, source: &str) -> &'a Task {
        p.tasks.iter().find(|t| t.source == source).unwrap()
    }

    #[test]
    fn a_punch_item_becomes_an_assigned_task_with_its_due_date() {
        let d = doc();
        let s = sidecar(&item("n1", 3, "Electrician", "2026-11-20", "Open"));
        let report = d.apply_punch("Floor plan.pdf", &s).unwrap();
        assert_eq!(report, SyncReport { added: 1, updated: 1, ..SyncReport::default() });
        let p = d.project();
        let t = task_for(&p, "revucraft:Floor plan.pdf#nm=n1");
        assert_eq!(t.name, "Punch #3");
        assert_eq!(t.deadline.map(|d| d.to_string()).as_deref(), Some("2026-11-20"));
        assert_eq!(t.status, "To Do");
        let r = p.resources.iter().find(|r| r.name == "Electrician").unwrap();
        assert!(p.assignments.iter().any(|a| a.task == t.id && a.resource == r.id));
    }

    #[test]
    fn a_second_identical_sync_writes_nothing() {
        let d = doc();
        let s = sidecar(&item("n1", 3, "Electrician", "2026-11-20", "Open"));
        d.apply_punch("Floor plan.pdf", &s).unwrap();
        let before = d.loro().oplog_vv();
        let report = d.apply_punch("Floor plan.pdf", &s).unwrap();
        assert_eq!(report, SyncReport::default());
        assert_eq!(d.loro().oplog_vv(), before, "no operations for an unchanged punch list");
    }

    #[test]
    fn closing_the_item_finishes_the_task_and_reopening_resets_it() {
        let d = doc();
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Closed"))).unwrap();
        let p = d.project();
        let t = task_for(&p, "revucraft:F.pdf#nm=n1");
        assert_eq!((t.status.as_str(), t.percent_complete), ("Done", 100));
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        let p = d.project();
        assert_eq!(task_for(&p, "revucraft:F.pdf#nm=n1").percent_complete, 0);
    }

    #[test]
    fn a_renumbered_or_renamed_markup_keeps_its_task() {
        let d = doc();
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        let id = task_for(&d.project(), "revucraft:F.pdf#nm=n1").id.clone();
        // Punch #1 is now #7, with the same name (its identity).
        let report = d.apply_punch("F.pdf", &sidecar(&item("n1", 7, "", "", "Open"))).unwrap();
        assert_eq!(report.added, 0);
        assert_eq!(task_for(&d.project(), "revucraft:F.pdf#nm=n1").id, id);
        assert_eq!(task_for(&d.project(), "revucraft:F.pdf#nm=n1").name, "Punch #7");
    }

    #[test]
    fn a_deleted_punch_item_cancels_its_task_and_its_return_restores_it() {
        let d = doc();
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        let report = d.apply_punch("F.pdf", &sidecar("")).unwrap();
        assert_eq!(report.cancelled, 1);
        assert!(task_for(&d.project(), "revucraft:F.pdf#nm=n1").cancelled);
        let report = d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        assert_eq!((report.restored, report.added), (1, 0));
        assert!(!task_for(&d.project(), "revucraft:F.pdf#nm=n1").cancelled);
    }

    #[test]
    fn two_devices_that_each_made_the_task_keep_one_after_they_merge() {
        // Neither device had the other's task when it synced.
        let a = doc();
        a.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        let b = ProjectDoc::new();
        b.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        a.import(&b.export_snapshot().unwrap()).unwrap();
        a.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        let live = |d: &ProjectDoc| d.project().tasks.into_iter().filter(|t| t.source == "revucraft:F.pdf#nm=n1" && !t.cancelled).count();
        assert_eq!(live(&a), 1);
        // The other device reaches the same decision.
        b.import(&a.export_snapshot().unwrap()).unwrap();
        b.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "", "Open"))).unwrap();
        assert_eq!(live(&b), 1);
        assert_eq!(
            a.project().tasks.iter().find(|t| !t.cancelled && t.source.ends_with("n1")).map(|t| t.id.clone()),
            b.project().tasks.iter().find(|t| !t.cancelled && t.source.ends_with("n1")).map(|t| t.id.clone())
        );
    }

    #[test]
    fn a_file_that_is_not_a_markups_file_or_is_a_newer_format_is_refused() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"format":"something-else","version":1}"#).is_err());
        assert!(parse(r#"{"format":"revucraft-markups","version":2}"#).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s = parse(r#"{"format":"revucraft-markups","version":1,"future":{"x":1},"punch":[{"name":"n","no":2,"extra":true}]}"#).unwrap();
        assert_eq!(s.punch.len(), 1);
        assert_eq!(s.punch[0].no, 2);
    }

    #[test]
    fn a_bad_due_date_is_left_empty_rather_than_failing_the_sync() {
        let d = doc();
        d.apply_punch("F.pdf", &sidecar(&item("n1", 1, "", "soon", "Open"))).unwrap();
        assert!(task_for(&d.project(), "revucraft:F.pdf#nm=n1").deadline.is_none());
    }
}
