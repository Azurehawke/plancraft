//! Consistency checks. Two devices editing offline can each make a valid change whose merge is
//! not (one links A→B while the other deletes B; both link B→A and A→B). The merged project is
//! still loaded and shown; these problems say what needs the person's attention.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::{Project, TaskId};

/// Something in a project that needs fixing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    /// A dependency names a task that no longer exists.
    DanglingDependency { dependency: String, missing: TaskId },
    /// A task depends on itself.
    SelfDependency { dependency: String, task: TaskId },
    /// A task is linked to its own summary task (or the other way round).
    DependencyOnSummary { dependency: String },
    /// The same two tasks are linked more than once.
    DuplicateDependency { dependencies: Vec<String> },
    /// Dependencies that go round in a loop; the tasks in it, in order.
    DependencyLoop { tasks: Vec<TaskId> },
    /// An assignment names a task or resource that no longer exists.
    DanglingAssignment { assignment: String },
    /// A constraint such as "must start on" without its date.
    ConstraintWithoutDate { task: TaskId },
    /// Percent complete above 100.
    PercentOutOfRange { task: TaskId },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::DanglingDependency { dependency, missing } => write!(f, "link {dependency} points at deleted task {missing}"),
            Problem::SelfDependency { task, .. } => write!(f, "task {task} is linked to itself"),
            Problem::DependencyOnSummary { dependency } => write!(f, "link {dependency} joins a task to its own summary task"),
            Problem::DuplicateDependency { dependencies } => write!(f, "the same two tasks are linked more than once ({})", dependencies.join(", ")),
            Problem::DependencyLoop { tasks } => {
                let names: Vec<&str> = tasks.iter().map(|t| t.0.as_str()).collect();
                write!(f, "links go round in a loop: {}", names.join(" → "))
            }
            Problem::DanglingAssignment { assignment } => write!(f, "assignment {assignment} points at a deleted task or resource"),
            Problem::ConstraintWithoutDate { task } => write!(f, "task {task} has a date constraint without a date"),
            Problem::PercentOutOfRange { task } => write!(f, "task {task} is more than 100% complete"),
        }
    }
}

pub(crate) fn problems(p: &Project) -> Vec<Problem> {
    let mut out = Vec::new();
    let tasks: HashMap<&TaskId, &crate::Task> = p.tasks.iter().map(|t| (&t.id, t)).collect();

    for t in &p.tasks {
        if t.constraint.needs_date() && t.constraint_date.is_none() {
            out.push(Problem::ConstraintWithoutDate { task: t.id.clone() });
        }
        if t.percent_complete > 100 {
            out.push(Problem::PercentOutOfRange { task: t.id.clone() });
        }
    }

    let mut pairs: HashMap<(&TaskId, &TaskId), Vec<String>> = HashMap::new();
    let mut edges: HashMap<&TaskId, Vec<&TaskId>> = HashMap::new();
    for d in &p.dependencies {
        let missing = [&d.predecessor, &d.successor].into_iter().find(|id| !tasks.contains_key(id));
        if let Some(m) = missing {
            out.push(Problem::DanglingDependency { dependency: d.id.clone(), missing: m.clone() });
            continue;
        }
        if d.predecessor == d.successor {
            out.push(Problem::SelfDependency { dependency: d.id.clone(), task: d.predecessor.clone() });
            continue;
        }
        if is_ancestor(&tasks, &d.predecessor, &d.successor) || is_ancestor(&tasks, &d.successor, &d.predecessor) {
            out.push(Problem::DependencyOnSummary { dependency: d.id.clone() });
        }
        pairs.entry((&d.predecessor, &d.successor)).or_default().push(d.id.clone());
        edges.entry(&d.predecessor).or_default().push(&d.successor);
    }
    let mut dups: Vec<Vec<String>> = pairs.into_values().filter(|v| v.len() > 1).collect();
    dups.sort();
    out.extend(dups.into_iter().map(|dependencies| Problem::DuplicateDependency { dependencies }));
    out.extend(loops(&p.tasks, &edges).into_iter().map(|tasks| Problem::DependencyLoop { tasks }));

    let resources: HashSet<&str> = p.resources.iter().map(|r| r.id.as_str()).collect();
    for a in &p.assignments {
        if !tasks.contains_key(&a.task) || !resources.contains(a.resource.as_str()) {
            out.push(Problem::DanglingAssignment { assignment: a.id.clone() });
        }
    }
    out
}

/// Whether `a` is a summary task above `b`. Bounded by the number of tasks, so a corrupt parent
/// chain that loops can't hang it.
fn is_ancestor(tasks: &HashMap<&TaskId, &crate::Task>, a: &TaskId, b: &TaskId) -> bool {
    let mut cur = tasks.get(b).and_then(|t| t.parent.as_ref());
    for _ in 0..=tasks.len() {
        match cur {
            Some(p) if p == a => return true,
            Some(p) => cur = tasks.get(p).and_then(|t| t.parent.as_ref()),
            None => return false,
        }
    }
    false
}

/// Each dependency loop once, found with an iterative depth-first search (no recursion, so a
/// long chain of links can't overflow the stack). Tasks are visited in outline order, so the
/// result is the same on every device.
fn loops(order: &[crate::Task], edges: &HashMap<&TaskId, Vec<&TaskId>>) -> Vec<Vec<TaskId>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Active,
        Done,
    }
    let mut mark: HashMap<&TaskId, Mark> = order.iter().map(|t| (&t.id, Mark::New)).collect();
    let mut found = Vec::new();
    for start in order.iter().map(|t| &t.id) {
        if mark.get(start) != Some(&Mark::New) {
            continue;
        }
        // (node, index of the next edge to follow)
        let mut stack: Vec<(&TaskId, usize)> = vec![(start, 0)];
        mark.insert(start, Mark::Active);
        while let Some((node, next)) = stack.last().copied() {
            let succ = edges.get(node).and_then(|v| v.get(next)).copied();
            match succ {
                Some(s) => {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                    match mark.get(s).copied() {
                        Some(Mark::New) => {
                            mark.insert(s, Mark::Active);
                            stack.push((s, 0));
                        }
                        Some(Mark::Active) => {
                            let from = stack.iter().position(|(n, _)| *n == s).unwrap_or(0);
                            found.push(stack.get(from..).unwrap_or_default().iter().map(|(n, _)| (*n).clone()).collect());
                        }
                        _ => {}
                    }
                }
                None => {
                    mark.insert(node, Mark::Done);
                    stack.pop();
                }
            }
        }
    }
    found
}
