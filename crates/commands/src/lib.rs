//! PlanCraft's project commands. Every project action is a command: an id (`task.add`,
//! `task.indent`, `project.setInfo`…) and JSON parameters, run against a [`ProjectDoc`]. The UI's
//! buttons and menus, the CLI and agents (MCP) all go through [`run`], so they can't drift apart.
//!
//! Tasks are named by id or WBS code (`"1.2"`); resources by id or name. A command with missing
//! or bad parameters returns an error and changes nothing; none of them opens a dialog.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use plancraft_project::{Date, DependencyKind, Project, TaskId};
use plancraft_project_sync::{Placement, ProjectDoc};
use serde_json::{Value, json};

/// A command's id, menu label and parameters (for help, the command palette and agents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub params: &'static str,
}

/// Every command.
pub const CATALOG: &[CommandSpec] = &[
    CommandSpec {
        id: "project.setInfo",
        label: "Project Information",
        params: "{field, value}: title purpose client site_address start deadline manager sponsor budget currency method",
    },
    CommandSpec { id: "task.add", label: "New Task", params: "{name, under?|after?|before?: task, duration?: \"3d\"} → {id}" },
    CommandSpec { id: "task.addMilestone", label: "Milestone", params: "{name, under?|after?|before?: task} → {id}" },
    CommandSpec {
        id: "task.set",
        label: "Task Information",
        params: "{task, field, value}: name duration start finish deadline percent milestone constraint constraint_date priority notes cancelled",
    },
    CommandSpec { id: "task.move", label: "Move Task", params: "{task, under?|top?|after?|before?}" },
    CommandSpec { id: "task.moveUp", label: "Move Up", params: "{task}" },
    CommandSpec { id: "task.moveDown", label: "Move Down", params: "{task}" },
    CommandSpec { id: "task.indent", label: "Indent Task", params: "{task}" },
    CommandSpec { id: "task.outdent", label: "Outdent Task", params: "{task}" },
    CommandSpec { id: "task.delete", label: "Delete Task", params: "{task}" },
    CommandSpec { id: "task.link", label: "Link Tasks", params: "{from, to, type?: FS|SS|FF|SF, lag?: \"1d\"} → {id}" },
    CommandSpec { id: "task.unlink", label: "Unlink Tasks", params: "{id} or {from, to}" },
    CommandSpec { id: "resource.add", label: "New Resource", params: "{name} → {id}" },
    CommandSpec { id: "resource.set", label: "Resource Information", params: "{resource, field, value}: name kind max_units role email phone color" },
    CommandSpec { id: "resource.delete", label: "Delete Resource", params: "{resource}" },
    CommandSpec { id: "task.assign", label: "Assign Resource", params: "{task, resource, units?: 1} → {id}" },
    CommandSpec { id: "task.unassign", label: "Remove Assignment", params: "{id}" },
    CommandSpec { id: "calendar.addHoliday", label: "Add Holiday", params: "{date: YYYY-MM-DD}" },
    CommandSpec { id: "calendar.removeHoliday", label: "Remove Holiday", params: "{date}" },
    CommandSpec { id: "calendar.setWorkingDays", label: "Working Days", params: "{days: \"1111100\"} (Monday first)" },
];

/// Runs command `id` with `params` on `doc`. Returns the command's result (a new id, or `{}`).
pub fn run(doc: &ProjectDoc, id: &str, params: &Value) -> Result<Value, String> {
    let p = Params(params);
    let e = |r: plancraft_project_sync::Result<()>| r.map(|()| json!({})).map_err(|e| e.to_string());
    match id {
        "project.setInfo" => e(doc.set_info(p.str("field")?, p.text("value"))),
        "task.add" | "task.addMilestone" => {
            let project = doc.project();
            let placement = placement(&project, &p)?.unwrap_or(Placement::End { parent: None });
            let new = doc.add_task(p.text("name"), &placement).map_err(|e| e.to_string())?;
            if id == "task.addMilestone" {
                doc.set_task(&new, "milestone", "true").map_err(|e| e.to_string())?;
            } else if let Some(d) = p.opt("duration") {
                doc.set_task(&new, "duration", d).map_err(|e| e.to_string())?;
            }
            Ok(json!({ "id": new.0 }))
        }
        "task.set" => e(doc.set_task(&task(doc, &p, "task")?, p.str("field")?, p.text("value"))),
        "task.move" => {
            let t = task(doc, &p, "task")?;
            let pl = placement(&doc.project(), &p)?.ok_or("say where: under, top, after or before")?;
            e(doc.move_task(&t, &pl))
        }
        "task.moveUp" | "task.moveDown" => {
            let project = doc.project();
            let t = task(doc, &p, "task")?;
            let parent = project.task(&t).and_then(|x| x.parent.clone());
            let sibs: Vec<TaskId> = project.children(parent.as_ref()).map(|x| x.id.clone()).collect();
            let i = sibs.iter().position(|s| *s == t).unwrap_or(0);
            let target = if id == "task.moveUp" { i.checked_sub(1).and_then(|j| sibs.get(j)) } else { sibs.get(i + 1) };
            match target {
                Some(other) if id == "task.moveUp" => e(doc.move_task(&t, &Placement::Before(other.clone()))),
                Some(other) => e(doc.move_task(&t, &Placement::After(other.clone()))),
                None => Ok(json!({})),
            }
        }
        "task.indent" => e(doc.indent(&task(doc, &p, "task")?)),
        "task.outdent" => e(doc.outdent(&task(doc, &p, "task")?)),
        "task.delete" => e(doc.delete_task(&task(doc, &p, "task")?)),
        "task.link" => {
            let (a, b) = (task(doc, &p, "from")?, task(doc, &p, "to")?);
            let kind = p.opt("type").map(DependencyKind::parse).unwrap_or_default();
            let lag = match p.opt("lag") {
                Some(l) => {
                    let neg = l.trim_start().starts_with('-');
                    let m = plancraft_project_sync::parse_duration(l.trim_start().trim_start_matches('-'))
                        .ok_or_else(|| format!("`{l}` is not a lag"))?;
                    let m = i32::try_from(m).map_err(|_| "lag is too long".to_string())?;
                    if neg { -m } else { m }
                }
                None => 0,
            };
            doc.link(&a, &b, kind, lag).map(|id| json!({ "id": id })).map_err(|e| e.to_string())
        }
        "task.unlink" => {
            if let Some(link) = p.opt("id") {
                return e(doc.unlink(link));
            }
            let (a, b) = (task(doc, &p, "from")?, task(doc, &p, "to")?);
            let ids: Vec<String> = doc.project().dependencies.into_iter().filter(|d| d.predecessor == a && d.successor == b).map(|d| d.id).collect();
            if ids.is_empty() {
                return Err("those tasks aren't linked".into());
            }
            for l in ids {
                doc.unlink(&l).map_err(|e| e.to_string())?;
            }
            Ok(json!({}))
        }
        "resource.add" => doc.add_resource(p.str("name")?).map(|id| json!({ "id": id })).map_err(|e| e.to_string()),
        "resource.set" => e(doc.set_resource(&resource(doc, &p)?, p.str("field")?, p.text("value"))),
        "resource.delete" => e(doc.delete_resource(&resource(doc, &p)?)),
        "task.assign" => {
            let t = task(doc, &p, "task")?;
            let r = resource(doc, &p)?;
            let units = params.get("units").and_then(Value::as_f64).unwrap_or(1.0);
            doc.assign(&t, &r, units).map(|id| json!({ "id": id })).map_err(|e| e.to_string())
        }
        "task.unassign" => e(doc.unassign(p.str("id")?)),
        "calendar.addHoliday" => e(doc.add_holiday(date(&p)?)),
        "calendar.removeHoliday" => e(doc.remove_holiday(date(&p)?)),
        "calendar.setWorkingDays" => {
            let s = p.str("days")?;
            let mut days = [false; 7];
            if s.len() != 7 || !s.bytes().all(|b| b == b'0' || b == b'1') {
                return Err("days is seven 0s and 1s, Monday first (\"1111100\")".into());
            }
            for (d, b) in days.iter_mut().zip(s.bytes()) {
                *d = b == b'1';
            }
            e(doc.set_working_days(days))
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

struct Params<'a>(&'a Value);

impl Params<'_> {
    fn opt(&self, k: &str) -> Option<&str> {
        self.0.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
    }
    fn str(&self, k: &str) -> Result<&str, String> {
        self.opt(k).ok_or_else(|| format!("`{k}` is needed"))
    }
    /// A text value; missing or null is empty (clears the field).
    fn text(&self, k: &str) -> &str {
        self.0.get(k).and_then(Value::as_str).unwrap_or("")
    }
    fn flag(&self, k: &str) -> bool {
        self.0.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
}

fn task(doc: &ProjectDoc, p: &Params, key: &str) -> Result<TaskId, String> {
    let k = p.str(key)?;
    doc.project().find_task(k).map(|t| t.id.clone()).ok_or_else(|| format!("no task `{k}`"))
}

fn resource(doc: &ProjectDoc, p: &Params) -> Result<String, String> {
    let k = p.str("resource")?;
    doc.project().find_resource(k).map(|r| r.id.clone()).ok_or_else(|| format!("no resource `{k}`"))
}

fn date(p: &Params) -> Result<Date, String> {
    p.str("date")?.parse().map_err(|e: plancraft_project::DateError| e.to_string())
}

fn placement(project: &Project, p: &Params) -> Result<Option<Placement>, String> {
    let find = |k: &str| project.find_task(k).map(|t| t.id.clone()).ok_or_else(|| format!("no task `{k}`"));
    Ok(if p.flag("top") {
        Some(Placement::End { parent: None })
    } else if let Some(t) = p.opt("under") {
        Some(Placement::End { parent: Some(find(t)?) })
    } else if let Some(t) = p.opt("after") {
        Some(Placement::After(find(t)?))
    } else if let Some(t) = p.opt("before") {
        Some(Placement::Before(find(t)?))
    } else {
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(doc: &ProjectDoc) -> Vec<String> {
        doc.project().tasks.into_iter().map(|t| format!("{} {}", t.wbs, t.name)).collect()
    }

    #[test]
    fn task_commands() {
        let d = ProjectDoc::new();
        run(&d, "task.add", &json!({"name": "A", "duration": "2d"})).unwrap();
        run(&d, "task.add", &json!({"name": "B"})).unwrap();
        run(&d, "task.add", &json!({"name": "C"})).unwrap();
        run(&d, "task.indent", &json!({"task": "2"})).unwrap();
        run(&d, "task.addMilestone", &json!({"name": "Done", "after": "1"})).unwrap();
        assert_eq!(names(&d), ["1 A", "1.1 B", "2 Done", "3 C"]);
        run(&d, "task.moveUp", &json!({"task": "3"})).unwrap();
        assert_eq!(names(&d), ["1 A", "1.1 B", "2 C", "3 Done"]);
        run(&d, "task.moveDown", &json!({"task": "3"})).unwrap(); // already last: no-op
        run(&d, "task.link", &json!({"from": "1.1", "to": "2", "lag": "-1d"})).unwrap();
        assert_eq!(d.project().dependencies[0].lag_minutes, -480);
        run(&d, "task.unlink", &json!({"from": "1.1", "to": "2"})).unwrap();
        assert!(d.project().dependencies.is_empty());
        run(&d, "task.set", &json!({"task": "2", "field": "percent", "value": "50"})).unwrap();
        run(&d, "resource.add", &json!({"name": "Tech"})).unwrap();
        run(&d, "task.assign", &json!({"task": "2", "resource": "tech", "units": 0.5})).unwrap();
        run(&d, "calendar.setWorkingDays", &json!({"days": "1111110"})).unwrap();
        run(&d, "project.setInfo", &json!({"field": "title", "value": "Job"})).unwrap();
        let p = d.project();
        assert!(p.tasks[3].milestone && p.tasks[2].percent_complete == 50 && p.calendar.working_days[5]);
        assert_eq!((p.info.title.as_str(), p.assignments.len()), ("Job", 1));
        run(&d, "task.delete", &json!({"task": "1"})).unwrap();
        assert_eq!(names(&d), ["1 C", "2 Done"]);
    }

    #[test]
    fn every_command_survives_empty_params() {
        let d = ProjectDoc::new();
        run(&d, "task.add", &json!({"name": "A"})).unwrap();
        for c in CATALOG {
            let _ = run(&d, c.id, &json!({}));
            let _ = run(&d, c.id, &json!({"task": "999", "from": 1, "date": "x", "days": "7"}));
        }
        assert!(run(&d, "nope", &json!({})).is_err());
    }
}
