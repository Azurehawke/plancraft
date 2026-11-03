//! The project document and its edits.

use loro::{ExportMode, LoroDoc, LoroMap, LoroTree, LoroValue, TreeID, ValueOrContainer};
use plancraft_project::{
    Assignment, Calendar, Constraint, Date, Dependency, DependencyKind, Method, Project, ProjectInfo, Resource, ResourceKind, Task, TaskId,
};

use crate::{Error, Result, random_id};

const INFO: &str = "info";
const CALENDAR: &str = "calendar";
const HOLIDAYS: &str = "holidays";
const TASKS: &str = "tasks";
const DEPS: &str = "deps";
const RESOURCES: &str = "resources";
const ASSIGNMENTS: &str = "assignments";
const BOARD: &str = "board";
const DRAWINGS: &str = "drawings";
const MAX_COLUMNS: usize = 12;

/// Upper bounds for values read from files or typed by people (hostile input).
const MAX_DURATION_MINUTES: i64 = 100 * 365 * 24 * 60;
const MAX_LAG_MINUTES: i64 = 10 * 365 * 24 * 60;
const MAX_TEXT: usize = 100_000;

/// Where a task goes when it is added or moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// The last subtask of `parent` (the last top-level task for `None`).
    End { parent: Option<TaskId> },
    /// Subtask number `index` (0-based) of `parent`; past the end means last.
    At { parent: Option<TaskId>, index: usize },
    /// Right after this task, under the same summary task.
    After(TaskId),
    /// Right before this task, under the same summary task.
    Before(TaskId),
}

/// One project as a Loro document. Cheap to clone (clones share the document).
#[derive(Clone)]
pub struct ProjectDoc {
    doc: LoroDoc,
}

impl Default for ProjectDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProjectDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectDoc").field("peer", &self.doc.peer_id()).finish()
    }
}

impl ProjectDoc {
    /// An empty project. Each document (each session) edits as a new random Loro peer, so two
    /// windows on one device can never be mistaken for one editor.
    pub fn new() -> Self {
        let doc = LoroDoc::new();
        // Siblings are ordered by fractional index; jitter keeps two devices inserting at the same
        // place from producing equal keys.
        doc.get_tree(TASKS).enable_fractional_index(8);
        ProjectDoc { doc }
    }

    /// Merges a device's saved bytes (a snapshot or updates) into this document. Merging the same
    /// bytes twice, or in any order with other devices' bytes, gives the same result.
    pub fn import(&self, bytes: &[u8]) -> Result<()> {
        self.doc.import(bytes).map_err(|e| Error::Data(format!("can't read project data: {e}")))?;
        Ok(())
    }

    /// Everything this document knows, for this device's change file.
    pub fn export_snapshot(&self) -> Result<Vec<u8>> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).map_err(|e| Error::Data(e.to_string()))
    }

    /// The underlying Loro document (for version history and tests).
    pub fn loro(&self) -> &LoroDoc {
        &self.doc
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree(TASKS)
    }

    fn map(&self, name: &str) -> LoroMap {
        self.doc.get_map(name)
    }

    // ---- reading ---------------------------------------------------------------------------

    /// The project as it stands after every merge so far.
    pub fn project(&self) -> Project {
        Project {
            info: self.info(),
            tasks: self.tasks(),
            dependencies: self.dependencies(),
            resources: self.resources(),
            assignments: self.assignments(),
            calendar: self.calendar(),
            board_columns: self.board_columns(),
            drawings: self.drawings(),
        }
    }

    fn drawings(&self) -> Vec<String> {
        let mut v: Vec<String> = self.map(DRAWINGS).keys().map(|k| k.to_string()).collect();
        v.sort();
        v
    }

    fn board_columns(&self) -> Vec<String> {
        let raw = text(&self.map(BOARD), "columns");
        let cols: Vec<String> = raw.lines().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string).take(MAX_COLUMNS).collect();
        if cols.is_empty() { plancraft_project::DEFAULT_COLUMNS.iter().map(|c| c.to_string()).collect() } else { cols }
    }

    /// Sets the board's columns, left to right (the last one means done). Names are trimmed; empty
    /// and repeated names are dropped.
    pub fn set_board_columns(&self, columns: &[String]) -> Result<()> {
        let mut seen: Vec<String> = Vec::new();
        for c in columns.iter().map(|c| c.trim()).filter(|c| !c.is_empty()) {
            if c.contains('\n') || c.len() > 60 {
                return Err(Error::BadValue {
                    field: "columns".into(),
                    reason: format!("`{c}` is not a column name (one line, at most 60 characters)"),
                });
            }
            if !seen.iter().any(|s| s.eq_ignore_ascii_case(c)) {
                seen.push(c.to_string());
            }
        }
        if seen.len() < 2 || seen.len() > MAX_COLUMNS {
            return Err(Error::BadValue { field: "columns".into(), reason: format!("a board needs 2 to {MAX_COLUMNS} columns") });
        }
        self.map(BOARD).insert("columns", seen.join("\n"))?;
        self.doc.commit();
        Ok(())
    }

    fn info(&self) -> ProjectInfo {
        let m = self.map(INFO);
        ProjectInfo {
            title: text(&m, "title"),
            purpose: text(&m, "purpose"),
            client: text(&m, "client"),
            site_address: text(&m, "site_address"),
            start: date(&m, "start"),
            deadline: date(&m, "deadline"),
            manager: text(&m, "manager"),
            sponsor: text(&m, "sponsor"),
            budget: number(&m, "budget").filter(|b| b.is_finite()),
            currency: text(&m, "currency"),
            method: Method::parse(&text(&m, "method")),
        }
    }

    fn calendar(&self) -> Calendar {
        let m = self.map(CALENDAR);
        let mut c = Calendar::default();
        let days = text(&m, "working_days");
        if days.len() == 7 && days.bytes().all(|b| b == b'0' || b == b'1') {
            for (slot, b) in c.working_days.iter_mut().zip(days.bytes()) {
                *slot = b == b'1';
            }
        }
        if let Some(h) = number(&m, "hours_per_day").filter(|h| h.is_finite() && *h > 0.0 && *h <= 24.0) {
            c.hours_per_day = h;
        }
        let hol = self.map(HOLIDAYS);
        let mut days: Vec<Date> = hol.keys().filter(|k| truthy(&hol, k)).filter_map(|k| k.parse().ok()).collect();
        days.sort();
        days.dedup();
        c.holidays = days;
        c
    }

    /// Tasks in outline order with WBS codes, walked without recursion.
    fn tasks(&self) -> Vec<Task> {
        let tree = self.tree();
        let mut out = Vec::new();
        // (node, parent, level, wbs)
        let mut stack: Vec<(TreeID, Option<TaskId>, u32, String)> = Vec::new();
        let roots = tree.children(None::<TreeID>).unwrap_or_default();
        for (i, id) in roots.into_iter().enumerate().rev() {
            stack.push((id, None, 1, (i + 1).to_string()));
        }
        while let Some((id, parent, level, wbs)) = stack.pop() {
            let kids = tree.children(id).unwrap_or_default();
            let task_id = TaskId(id.to_string());
            for (i, k) in kids.iter().enumerate().rev() {
                stack.push((*k, Some(task_id.clone()), level.saturating_add(1), format!("{wbs}.{}", i + 1)));
            }
            let meta = tree.get_meta(id).ok();
            out.push(read_task(task_id, parent, level, wbs, !kids.is_empty(), meta.as_ref()));
        }
        out
    }

    fn dependencies(&self) -> Vec<Dependency> {
        let mut v: Vec<Dependency> = entries(&self.map(DEPS))
            .into_iter()
            .map(|(id, m)| Dependency {
                id,
                predecessor: TaskId(text(&m, "pred")),
                successor: TaskId(text(&m, "succ")),
                kind: DependencyKind::parse(&text(&m, "kind")),
                lag_minutes: int(&m, "lag").map(|l| l.clamp(-MAX_LAG_MINUTES, MAX_LAG_MINUTES)).unwrap_or(0) as i32,
            })
            .filter(|d| !d.predecessor.0.is_empty() && !d.successor.0.is_empty())
            .collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    fn resources(&self) -> Vec<Resource> {
        let mut v: Vec<Resource> = entries(&self.map(RESOURCES))
            .into_iter()
            .map(|(id, m)| Resource {
                id,
                name: text(&m, "name"),
                kind: ResourceKind::parse(&text(&m, "kind")),
                max_units: number(&m, "max_units").filter(|u| u.is_finite() && *u >= 0.0).unwrap_or(1.0),
                role: text(&m, "role"),
                email: text(&m, "email"),
                phone: text(&m, "phone"),
                color: text(&m, "color"),
            })
            .collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        v
    }

    fn assignments(&self) -> Vec<Assignment> {
        let mut v: Vec<Assignment> = entries(&self.map(ASSIGNMENTS))
            .into_iter()
            .map(|(id, m)| Assignment {
                id,
                task: TaskId(text(&m, "task")),
                resource: text(&m, "resource"),
                units: number(&m, "units").filter(|u| u.is_finite() && *u >= 0.0).unwrap_or(1.0),
            })
            .collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    // ---- project information and calendar ---------------------------------------------------

    /// Sets one project-information field from text, as typed (`budget` takes a number, `start`
    /// and `deadline` a date, empty clears). Fields: title, purpose, client, site_address, start,
    /// deadline, manager, sponsor, budget, currency, method.
    pub fn set_info(&self, field: &str, value: &str) -> Result<()> {
        let m = self.map(INFO);
        match field {
            "title" | "purpose" | "client" | "site_address" | "manager" | "sponsor" | "currency" => put_text(&m, field, value)?,
            "start" | "deadline" => put_date(&m, field, value)?,
            "budget" => put_number(&m, field, value, 0.0, 1e15)?,
            "method" => m.insert(field, Method::parse(value).as_str())?,
            other => return Err(Error::UnknownField(other.to_string())),
        }
        self.doc.commit();
        Ok(())
    }

    /// Writes the fields of `info` that differ from what is stored (what the new project wizard
    /// does), so unchanged fields don't overwrite another device's edits.
    pub fn update_info(&self, info: &ProjectInfo) -> Result<()> {
        let cur = self.info();
        let m = self.map(INFO);
        let texts = [
            ("title", &cur.title, &info.title),
            ("purpose", &cur.purpose, &info.purpose),
            ("client", &cur.client, &info.client),
            ("site_address", &cur.site_address, &info.site_address),
            ("manager", &cur.manager, &info.manager),
            ("sponsor", &cur.sponsor, &info.sponsor),
            ("currency", &cur.currency, &info.currency),
        ];
        for (k, old, new) in texts {
            if old != new {
                put_text(&m, k, new)?;
            }
        }
        for (k, old, new) in [("start", cur.start, info.start), ("deadline", cur.deadline, info.deadline)] {
            if old != new {
                put_date(&m, k, &new.map(|d| d.to_string()).unwrap_or_default())?;
            }
        }
        if cur.budget != info.budget {
            match info.budget.filter(|b| b.is_finite()) {
                Some(b) => m.insert("budget", b)?,
                None => m.delete("budget")?,
            }
        }
        if cur.method != info.method {
            m.insert("method", info.method.as_str())?;
        }
        self.doc.commit();
        Ok(())
    }

    /// Working days, Monday first, e.g. `[true, true, true, true, true, false, false]`.
    pub fn set_working_days(&self, days: [bool; 7]) -> Result<()> {
        let s: String = days.iter().map(|d| if *d { '1' } else { '0' }).collect();
        self.map(CALENDAR).insert("working_days", s)?;
        self.doc.commit();
        Ok(())
    }

    pub fn set_hours_per_day(&self, hours: f64) -> Result<()> {
        if !(hours.is_finite() && hours > 0.0 && hours <= 24.0) {
            return Err(Error::BadValue { field: "hours_per_day".into(), reason: "must be more than 0 and at most 24".into() });
        }
        self.map(CALENDAR).insert("hours_per_day", hours)?;
        self.doc.commit();
        Ok(())
    }

    pub fn add_holiday(&self, day: Date) -> Result<()> {
        self.map(HOLIDAYS).insert(&day.to_string(), true)?;
        self.doc.commit();
        Ok(())
    }

    pub fn remove_holiday(&self, day: Date) -> Result<()> {
        self.map(HOLIDAYS).delete(&day.to_string())?;
        self.doc.commit();
        Ok(())
    }

    // ---- tasks -----------------------------------------------------------------------------

    fn node(&self, id: &TaskId) -> Result<TreeID> {
        let tid = TreeID::try_from(id.0.as_str()).map_err(|_| Error::UnknownTask(id.0.clone()))?;
        let tree = self.tree();
        if !tree.contains(tid) || tree.is_node_deleted(&tid).unwrap_or(true) {
            return Err(Error::UnknownTask(id.0.clone()));
        }
        Ok(tid)
    }

    fn parent_node(&self, parent: &Option<TaskId>) -> Result<Option<TreeID>> {
        parent.as_ref().map(|p| self.node(p)).transpose()
    }

    /// Adds a task named `name` and returns its id.
    pub fn add_task(&self, name: &str, placement: &Placement) -> Result<TaskId> {
        let tree = self.tree();
        let id = match placement {
            Placement::End { parent } => tree.create(self.parent_node(parent)?)?,
            Placement::At { parent, index } => {
                let p = self.parent_node(parent)?;
                let n = tree.children_num(p).unwrap_or(0);
                tree.create_at(p, (*index).min(n))?
            }
            Placement::After(t) | Placement::Before(t) => {
                let anchor = self.node(t)?;
                let p = tree.parent(anchor).and_then(parent_tree_id);
                let id = tree.create(p)?;
                if matches!(placement, Placement::After(_)) {
                    tree.mov_after(id, anchor)?;
                } else {
                    tree.mov_before(id, anchor)?;
                }
                id
            }
        };
        let meta = tree.get_meta(id)?;
        put_text(&meta, "name", name)?;
        self.doc.commit();
        Ok(TaskId(id.to_string()))
    }

    /// Moves a task (with its subtasks).
    pub fn move_task(&self, id: &TaskId, placement: &Placement) -> Result<()> {
        let tree = self.tree();
        let node = self.node(id)?;
        match placement {
            Placement::End { parent } => {
                let p = self.parent_node(parent)?;
                let n = tree.children_num(p).unwrap_or(0);
                let already_here = tree.parent(node).and_then(parent_tree_id) == p;
                tree.mov_to(node, p, if already_here { n.saturating_sub(1) } else { n })?
            }
            Placement::At { parent, index } => {
                let p = self.parent_node(parent)?;
                let n = tree.children_num(p).unwrap_or(0);
                let already_here = tree.parent(node).and_then(parent_tree_id) == p;
                let max = if already_here { n.saturating_sub(1) } else { n };
                tree.mov_to(node, p, (*index).min(max))?
            }
            Placement::After(t) => {
                let anchor = self.node(t)?;
                if anchor != node {
                    tree.mov_after(node, anchor)?
                }
            }
            Placement::Before(t) => {
                let anchor = self.node(t)?;
                if anchor != node {
                    tree.mov_before(node, anchor)?
                }
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Makes a task a subtask of the task above it (MS Project's Indent).
    pub fn indent(&self, id: &TaskId) -> Result<()> {
        let tree = self.tree();
        let node = self.node(id)?;
        let parent = tree.parent(node).and_then(parent_tree_id);
        let siblings = tree.children(parent).unwrap_or_default();
        let pos = siblings.iter().position(|s| *s == node).unwrap_or(0);
        let Some(above) = pos.checked_sub(1).and_then(|i| siblings.get(i)).copied() else {
            return Err(Error::BadValue { field: "indent".into(), reason: "the first task under a summary can't be indented".into() });
        };
        let n = tree.children_num(above).unwrap_or(0);
        tree.mov_to(node, above, n)?;
        self.doc.commit();
        Ok(())
    }

    /// Moves a task up one outline level, right after its summary task (MS Project's Outdent).
    pub fn outdent(&self, id: &TaskId) -> Result<()> {
        let tree = self.tree();
        let node = self.node(id)?;
        let Some(parent) = tree.parent(node).and_then(parent_tree_id) else {
            return Err(Error::BadValue { field: "outdent".into(), reason: "the task is already at the top level".into() });
        };
        tree.mov_after(node, parent)?;
        self.doc.commit();
        Ok(())
    }

    /// Deletes a task with its subtasks, and the links and assignments that pointed at them.
    pub fn delete_task(&self, id: &TaskId) -> Result<()> {
        let tree = self.tree();
        let node = self.node(id)?;
        let mut gone = vec![id.0.clone()];
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            for k in tree.children(n).unwrap_or_default() {
                gone.push(k.to_string());
                stack.push(k);
            }
        }
        tree.delete(node)?;
        let deps = self.map(DEPS);
        for (dep_id, m) in entries(&deps) {
            if gone.contains(&text(&m, "pred")) || gone.contains(&text(&m, "succ")) {
                deps.delete(&dep_id)?;
            }
        }
        let asg = self.map(ASSIGNMENTS);
        for (a_id, m) in entries(&asg) {
            if gone.contains(&text(&m, "task")) {
                asg.delete(&a_id)?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Sets one task field from text, as typed. Fields: name, duration (working minutes, or a
    /// number with `d`, `h`, `w` or `m`), start, finish, deadline, constraint_date (dates), milestone,
    /// cancelled (true/false), percent (0–100), constraint (ASAP, SNET…), priority (0–1000), notes,
    /// source. An empty value clears the field.
    pub fn set_task(&self, id: &TaskId, field: &str, value: &str) -> Result<()> {
        let node = self.node(id)?;
        let m = self.tree().get_meta(node)?;
        match field {
            "name" | "notes" | "source" | "status" | "location" => put_text(&m, field, value)?,
            "start" | "finish" | "deadline" | "constraint_date" => put_date(&m, field, value)?,
            "milestone" | "cancelled" => match parse_bool(value) {
                Some(b) => m.insert(field, b)?,
                None => return Err(Error::BadValue { field: field.into(), reason: format!("`{value}` is not true or false") }),
            },
            "percent" => put_int(&m, field, value, 0, 100)?,
            "priority" => put_int(&m, field, value, 0, 1000)?,
            "duration" => {
                if value.trim().is_empty() {
                    m.delete("duration")?;
                } else {
                    let minutes = parse_duration(value).ok_or_else(|| Error::BadValue {
                        field: "duration".into(),
                        reason: format!("`{value}` is not a duration (e.g. 3d, 4h, 2w, 90m)"),
                    })?;
                    m.insert("duration", minutes)?;
                }
            }
            "constraint" => {
                let c = Constraint::parse(value);
                if !value.trim().is_empty() && !c.as_str().eq_ignore_ascii_case(value.trim()) {
                    return Err(Error::BadValue {
                        field: "constraint".into(),
                        reason: format!("`{value}` is not ASAP, ALAP, SNET, SNLT, FNET, FNLT, MSO or MFO"),
                    });
                }
                m.insert("constraint", c.as_str())?
            }
            other => return Err(Error::UnknownField(other.to_string())),
        }
        self.doc.commit();
        Ok(())
    }

    /// Links a drawing whose RevuCraft punch items feed this plan. `path` is the PDF's path
    /// relative to the job folder, with `/` separators.
    pub fn add_drawing(&self, path: &str) -> Result<()> {
        check_drawing_path(path)?;
        self.map(DRAWINGS).insert(path, true)?;
        self.doc.commit();
        Ok(())
    }

    /// Unlinks a drawing. Its tasks stay in the plan, cancelled, so history is kept.
    pub fn remove_drawing(&self, path: &str) -> Result<()> {
        self.map(DRAWINGS).delete(path)?;
        self.doc.commit();
        Ok(())
    }

    // ---- dependencies ----------------------------------------------------------------------

    /// Links two tasks and returns the link's id. Loops and duplicate links are allowed here
    /// (another device can make them anyway) and reported by `Project::problems`.
    pub fn link(&self, predecessor: &TaskId, successor: &TaskId, kind: DependencyKind, lag_minutes: i32) -> Result<String> {
        self.node(predecessor)?;
        self.node(successor)?;
        if predecessor == successor {
            return Err(Error::BadValue { field: "link".into(), reason: "a task can't be linked to itself".into() });
        }
        let id = random_id();
        let m = self.map(DEPS).insert_container(&id, LoroMap::new())?;
        m.insert("pred", predecessor.0.as_str())?;
        m.insert("succ", successor.0.as_str())?;
        m.insert("kind", kind.as_str())?;
        m.insert("lag", i64::from(lag_minutes).clamp(-MAX_LAG_MINUTES, MAX_LAG_MINUTES))?;
        self.doc.commit();
        Ok(id)
    }

    pub fn unlink(&self, dependency: &str) -> Result<()> {
        let deps = self.map(DEPS);
        if deps.get(dependency).is_none() {
            return Err(Error::UnknownDependency(dependency.to_string()));
        }
        deps.delete(dependency)?;
        self.doc.commit();
        Ok(())
    }

    // ---- resources and assignments ---------------------------------------------------------

    pub fn add_resource(&self, name: &str) -> Result<String> {
        let id = random_id();
        let m = self.map(RESOURCES).insert_container(&id, LoroMap::new())?;
        put_text(&m, "name", name)?;
        self.doc.commit();
        Ok(id)
    }

    /// Sets one resource field: name, kind (work/material/cost), max_units, role, email, phone,
    /// color (`#rrggbb`).
    pub fn set_resource(&self, id: &str, field: &str, value: &str) -> Result<()> {
        let m = self.resource_map(id)?;
        match field {
            "name" | "role" | "email" | "phone" => put_text(&m, field, value)?,
            "kind" => m.insert(field, ResourceKind::parse(value).as_str())?,
            "max_units" => put_number(&m, field, value, 0.0, 1e6)?,
            "color" => {
                let v = value.trim();
                let ok = v.is_empty() || (v.len() == 7 && v.starts_with('#') && v.bytes().skip(1).all(|b| b.is_ascii_hexdigit()));
                if !ok {
                    return Err(Error::BadValue { field: "color".into(), reason: format!("`{value}` is not #rrggbb") });
                }
                put_text(&m, field, v)?
            }
            other => return Err(Error::UnknownField(other.to_string())),
        }
        self.doc.commit();
        Ok(())
    }

    fn resource_map(&self, id: &str) -> Result<LoroMap> {
        match self.map(RESOURCES).get(id) {
            Some(ValueOrContainer::Container(loro::Container::Map(m))) => Ok(m),
            _ => Err(Error::UnknownResource(id.to_string())),
        }
    }

    /// Deletes a resource and its assignments.
    pub fn delete_resource(&self, id: &str) -> Result<()> {
        self.resource_map(id)?;
        self.map(RESOURCES).delete(id)?;
        let asg = self.map(ASSIGNMENTS);
        for (a_id, m) in entries(&asg) {
            if text(&m, "resource") == id {
                asg.delete(&a_id)?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    pub fn assign(&self, task: &TaskId, resource: &str, units: f64) -> Result<String> {
        self.node(task)?;
        self.resource_map(resource)?;
        if !(units.is_finite() && (0.0..=1e6).contains(&units)) {
            return Err(Error::BadValue { field: "units".into(), reason: "must be a number from 0".into() });
        }
        let id = random_id();
        let m = self.map(ASSIGNMENTS).insert_container(&id, LoroMap::new())?;
        m.insert("task", task.0.as_str())?;
        m.insert("resource", resource)?;
        m.insert("units", units)?;
        self.doc.commit();
        Ok(id)
    }

    pub fn unassign(&self, assignment: &str) -> Result<()> {
        let asg = self.map(ASSIGNMENTS);
        if asg.get(assignment).is_none() {
            return Err(Error::UnknownField(assignment.to_string()));
        }
        asg.delete(assignment)?;
        self.doc.commit();
        Ok(())
    }
}

// ---- helpers ---------------------------------------------------------------------------------

fn parent_tree_id(p: loro::TreeParentId) -> Option<TreeID> {
    match p {
        loro::TreeParentId::Node(n) => Some(n),
        _ => None,
    }
}

fn read_task(id: TaskId, parent: Option<TaskId>, level: u32, wbs: String, summary: bool, m: Option<&LoroMap>) -> Task {
    let Some(m) = m else {
        return Task { id, parent, outline_level: level, wbs, summary, priority: 500, ..Task::default() };
    };
    Task {
        id,
        parent,
        outline_level: level,
        wbs,
        summary,
        name: text(m, "name"),
        duration_minutes: int(m, "duration").map(|d| d.clamp(0, MAX_DURATION_MINUTES)).unwrap_or(0) as u32,
        start: date(m, "start"),
        finish: date(m, "finish"),
        milestone: truthy(m, "milestone"),
        percent_complete: int(m, "percent").map(|p| p.clamp(0, 100)).unwrap_or(0) as u8,
        constraint: Constraint::parse(&text(m, "constraint")),
        constraint_date: date(m, "constraint_date"),
        deadline: date(m, "deadline"),
        priority: int(m, "priority").map(|p| p.clamp(0, 1000)).unwrap_or(500) as u16,
        notes: text(m, "notes"),
        cancelled: truthy(m, "cancelled"),
        source: text(m, "source"),
        location: text(m, "location"),
        status: text(m, "status"),
    }
}

/// The child maps of a map container (`deps`, `resources`, `assignments`), by key.
fn entries(m: &LoroMap) -> Vec<(String, LoroMap)> {
    m.keys()
        .filter_map(|k| match m.get(&k) {
            Some(ValueOrContainer::Container(loro::Container::Map(c))) => Some((k.to_string(), c)),
            _ => None,
        })
        .collect()
}

fn value(m: &LoroMap, key: &str) -> Option<LoroValue> {
    match m.get(key) {
        Some(ValueOrContainer::Value(v)) => Some(v),
        _ => None,
    }
}

fn text(m: &LoroMap, key: &str) -> String {
    match value(m, key) {
        Some(LoroValue::String(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn number(m: &LoroMap, key: &str) -> Option<f64> {
    match value(m, key) {
        Some(LoroValue::Double(d)) => Some(d),
        Some(LoroValue::I64(i)) => Some(i as f64),
        _ => None,
    }
}

fn int(m: &LoroMap, key: &str) -> Option<i64> {
    match value(m, key) {
        Some(LoroValue::I64(i)) => Some(i),
        Some(LoroValue::Double(d)) if d.is_finite() => Some(d.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64),
        _ => None,
    }
}

fn truthy(m: &LoroMap, key: &str) -> bool {
    matches!(value(m, key), Some(LoroValue::Bool(true)))
}

fn date(m: &LoroMap, key: &str) -> Option<Date> {
    text(m, key).parse().ok()
}

fn put_text(m: &LoroMap, key: &str, value: &str) -> Result<()> {
    if value.len() > MAX_TEXT {
        return Err(Error::BadValue { field: key.into(), reason: format!("longer than {MAX_TEXT} bytes") });
    }
    if value.is_empty() {
        m.delete(key)?;
    } else {
        m.insert(key, value)?;
    }
    Ok(())
}

fn put_date(m: &LoroMap, key: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        m.delete(key)?;
        return Ok(());
    }
    let d: Date = value.parse().map_err(|e: plancraft_project::DateError| Error::BadValue { field: key.into(), reason: e.to_string() })?;
    m.insert(key, d.to_string())?;
    Ok(())
}

fn put_number(m: &LoroMap, key: &str, value: &str, min: f64, max: f64) -> Result<()> {
    if value.trim().is_empty() {
        m.delete(key)?;
        return Ok(());
    }
    let n: f64 =
        value.trim().replace(',', "").parse().map_err(|_| Error::BadValue { field: key.into(), reason: format!("`{value}` is not a number") })?;
    if !(n.is_finite() && (min..=max).contains(&n)) {
        return Err(Error::BadValue { field: key.into(), reason: format!("must be from {min} to {max}") });
    }
    m.insert(key, n)?;
    Ok(())
}

fn put_int(m: &LoroMap, key: &str, value: &str, min: i64, max: i64) -> Result<()> {
    if value.trim().is_empty() {
        m.delete(key)?;
        return Ok(());
    }
    let n: i64 = value
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| Error::BadValue { field: key.into(), reason: format!("`{value}` is not a whole number") })?;
    if !(min..=max).contains(&n) {
        return Err(Error::BadValue { field: key.into(), reason: format!("must be from {min} to {max}") });
    }
    m.insert(key, n)?;
    Ok(())
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "y" | "on" => Some(true),
        "false" | "no" | "0" | "n" | "off" | "" => Some(false),
        _ => None,
    }
}

/// Working minutes from `90`, `90m`, `4h`, `3d`, `2w` or `1.5d` (8-hour days, 5-day weeks).
pub fn parse_duration(s: &str) -> Option<i64> {
    let t = s.trim().to_ascii_lowercase();
    let (num, unit) = match t.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        Some((i, _)) => (t.get(..i)?.trim(), t.get(i..)?.trim()),
        None => (t.as_str(), "m"),
    };
    let n: f64 = num.parse().ok()?;
    let per = match unit {
        "m" | "min" | "mins" | "minute" | "minutes" => 1.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60.0,
        "d" | "day" | "days" => f64::from(plancraft_project::MINUTES_PER_DAY),
        "w" | "wk" | "wks" | "week" | "weeks" => 5.0 * f64::from(plancraft_project::MINUTES_PER_DAY),
        _ => return None,
    };
    let minutes = (n * per).round();
    (minutes.is_finite() && minutes >= 0.0 && minutes <= MAX_DURATION_MINUTES as f64).then_some(minutes as i64)
}

#[cfg(test)]
mod tests;

/// A linked drawing is a PDF inside the job folder: a relative path with `/` separators, no `..`,
/// no drive or absolute path, and a `.pdf` name.
fn check_drawing_path(path: &str) -> Result<()> {
    let bad = |reason: &str| Error::BadValue { field: "drawing".into(), reason: reason.into() };
    if path.is_empty() || path.len() > 1024 {
        return Err(bad("a drawing path must be 1 to 1024 bytes"));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return Err(bad("a drawing must be inside the job folder (a relative path with / separators)"));
    }
    if path.split('/').any(|part| part.is_empty() || part == "..") {
        return Err(bad("a drawing path can't contain empty parts or `..`"));
    }
    if !path.to_ascii_lowercase().ends_with(".pdf") {
        return Err(bad("a linked drawing is a .pdf file"));
    }
    Ok(())
}
