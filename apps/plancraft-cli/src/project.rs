//! `plancraft-cli project …`: create, read and edit PlanCraft projects (`.plan` folders) from the
//! command line, until the app has project views. Every edit opens the project, merges every
//! device's change file, makes the change and saves this device's file.

use std::path::{Path, PathBuf};

use plancraft_project::{DependencyKind, Project, Task, TaskId};
use plancraft_project_sync::{Placement, ProjectFolder};
use serde_json::json;

use crate::Args;

pub(crate) const USAGE: &str = "usage:
  plancraft-cli project new <folder> <name>                 makes <folder>/<name>.plan
  plancraft-cli project show <project> [--json] [--all]       tasks, links, problems (--all: also ids, resources, calendar)
  plancraft-cli project info <project> <field> <value>        title purpose client site_address start deadline
                                                              manager sponsor budget currency method
  plancraft-cli project add <project> <name> [--under T | --after T | --before T] [--duration 3d]
  plancraft-cli project set <project> <task> <field> <value>  name duration start finish deadline percent
                                                              milestone constraint constraint_date priority notes cancelled
  plancraft-cli project move <project> <task> (--under T | --top | --after T | --before T)
  plancraft-cli project indent|outdent|delete <project> <task>
  plancraft-cli project link <project> <from-task> <to-task> [--type FS|SS|FF|SF] [--lag 1d]
  plancraft-cli project unlink <project> <link-id>
  plancraft-cli project resource <project> add <name> | set <resource> <field> <value> | delete <resource>
  plancraft-cli project assign <project> <task> <resource> [--units 1]
  plancraft-cli project holiday <project> add|remove <YYYY-MM-DD>
  plancraft-cli project devices <project>                     each device's change file
  <project> is a .plan folder or its .plancraft file; <task> is a WBS code (1.2) or a task id.
  This computer saves as the device in PLANCRAFT_DEVICE, or a name kept in PlanCraft's settings folder.";

pub(crate) fn main(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or_default();
    match sub {
        "new" => new(rest),
        "show" => show(rest),
        "info" => edit(rest, 3, |f, a| f.doc().set_info(&a[1], &a[2]).map_err(err)),
        "add" => add(rest),
        "set" => edit(rest, 4, |f, a| {
            let t = task(f, &a[1])?;
            f.doc().set_task(&t, &a[2], &a[3]).map_err(err)
        }),
        "move" => move_(rest),
        "indent" => edit(rest, 2, |f, a| f.doc().indent(&task(f, &a[1])?).map_err(err)),
        "outdent" => edit(rest, 2, |f, a| f.doc().outdent(&task(f, &a[1])?).map_err(err)),
        "delete" => edit(rest, 2, |f, a| f.doc().delete_task(&task(f, &a[1])?).map_err(err)),
        "link" => link(rest),
        "unlink" => edit(rest, 2, |f, a| f.doc().unlink(&a[1]).map_err(err)),
        "resource" => resource(rest),
        "assign" => assign(rest),
        "holiday" => holiday(rest),
        "devices" => devices(rest),
        "" | "help" | "--help" | "-h" => {
            outln!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown project command `{other}`\n{USAGE}")),
    }
}

fn err(e: plancraft_project_sync::Error) -> String {
    e.to_string()
}

/// This computer's device name (see `plancraft_project_sync::device_name`).
fn device() -> Result<String, String> {
    if let Some(d) = std::env::var("PLANCRAFT_DEVICE").ok().filter(|d| !d.trim().is_empty()) {
        return Ok(plancraft_project_sync::sanitize_device_name(&d));
    }
    let state = settings_dir().ok_or("can't find a settings folder for the device name; set PLANCRAFT_DEVICE")?.join("device");
    let hint = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_else(|| "device".into());
    plancraft_project_sync::device_name(&state, &hint).map_err(err)
}

/// The desktop app's settings folder (the same one the app uses).
fn settings_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/PlanCraft"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("PlanCraft"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("plancraft"))
    }
}

fn open(path: &str) -> Result<ProjectFolder, String> {
    let f = ProjectFolder::open(Path::new(path), &device()?).map_err(err)?;
    for w in f.warnings() {
        eprintln!("warning: {w}");
    }
    Ok(f)
}

/// The task a WBS code or id names.
fn task(f: &ProjectFolder, key: &str) -> Result<TaskId, String> {
    let p = f.doc().project();
    p.find_task(key).map(|t| t.id.clone()).ok_or_else(|| format!("no task `{key}` (use a WBS code such as 1.2, or a task id)"))
}

/// Opens the project in `a[0]`, runs `change` with exactly `n` positionals, saves.
fn edit(args: &[String], n: usize, change: impl Fn(&ProjectFolder, &[String]) -> Result<(), String>) -> Result<(), String> {
    let a = Args::parse(args, &[], &[])?;
    if a.positional.len() != n {
        return Err(format!("expected {n} arguments\n{USAGE}"));
    }
    let mut f = open(&a.positional[0])?;
    change(&f, &a.positional)?;
    f.save().map_err(err)
}

fn new(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &[], &[])?;
    let (dir, name) = (a.pos(0, "folder")?, a.pos(1, "project name")?);
    a.no_extra(2)?;
    let f = ProjectFolder::create(Path::new(dir), name, &device()?).map_err(err)?;
    outln!("{}", f.dir().display());
    Ok(())
}

fn add(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["under", "after", "before", "duration"], &[])?;
    let (path, name) = (a.pos(0, "project")?, a.pos(1, "task name")?);
    a.no_extra(2)?;
    let mut f = open(path)?;
    let placement = placement(&f, &a)?.unwrap_or(Placement::End { parent: None });
    let id = f.doc().add_task(name, &placement).map_err(err)?;
    if let Some(d) = a.opt("duration") {
        f.doc().set_task(&id, "duration", d).map_err(err)?;
    }
    f.save().map_err(err)?;
    let p = f.doc().project();
    outln!("{}\t{}", p.task(&id).map(|t| t.wbs.as_str()).unwrap_or("?"), id);
    Ok(())
}

fn placement(f: &ProjectFolder, a: &Args) -> Result<Option<Placement>, String> {
    let chosen: Vec<&str> = ["under", "after", "before"].into_iter().filter(|o| a.opt(o).is_some()).collect();
    if chosen.len() + usize::from(a.has("top")) > 1 {
        return Err("give only one of --under, --top, --after, --before".into());
    }
    Ok(if a.has("top") {
        Some(Placement::End { parent: None })
    } else if let Some(t) = a.opt("under") {
        Some(Placement::End { parent: Some(task(f, t)?) })
    } else if let Some(t) = a.opt("after") {
        Some(Placement::After(task(f, t)?))
    } else if let Some(t) = a.opt("before") {
        Some(Placement::Before(task(f, t)?))
    } else {
        None
    })
}

fn move_(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["under", "after", "before"], &["top"])?;
    let (path, key) = (a.pos(0, "project")?, a.pos(1, "task")?);
    a.no_extra(2)?;
    let mut f = open(path)?;
    let t = task(&f, key)?;
    let p = placement(&f, &a)?.ok_or("say where: --under T, --top, --after T or --before T")?;
    f.doc().move_task(&t, &p).map_err(err)?;
    f.save().map_err(err)
}

fn link(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["type", "lag"], &[])?;
    let (path, from, to) = (a.pos(0, "project")?, a.pos(1, "from-task")?, a.pos(2, "to-task")?);
    a.no_extra(3)?;
    let kind = a.opt("type").map(DependencyKind::parse).unwrap_or_default();
    let lag = match a.opt("lag") {
        Some(l) => {
            let neg = l.trim_start().starts_with('-');
            let m = plancraft_project_sync::parse_duration(l.trim_start().trim_start_matches('-'))
                .ok_or_else(|| format!("`{l}` is not a lag (e.g. 1d, -4h)"))?;
            let m = i32::try_from(m).map_err(|_| "lag is too long".to_string())?;
            if neg { -m } else { m }
        }
        None => 0,
    };
    let mut f = open(path)?;
    let (pt, st) = (task(&f, from)?, task(&f, to)?);
    let id = f.doc().link(&pt, &st, kind, lag).map_err(err)?;
    f.save().map_err(err)?;
    outln!("{id}");
    Ok(())
}

fn resource(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &[], &[])?;
    let path = a.pos(0, "project")?;
    let mut f = open(path)?;
    let find =
        |f: &ProjectFolder, key: &str| f.doc().project().find_resource(key).map(|r| r.id.clone()).ok_or_else(|| format!("no resource `{key}`"));
    match a.pos(1, "add, set or delete")? {
        "add" => {
            let name = a.pos(2, "name")?;
            a.no_extra(3)?;
            let id = f.doc().add_resource(name).map_err(err)?;
            outln!("{id}");
        }
        "set" => {
            let (key, field, value) = (a.pos(2, "resource")?, a.pos(3, "field")?, a.pos(4, "value")?);
            a.no_extra(5)?;
            let id = find(&f, key)?;
            f.doc().set_resource(&id, field, value).map_err(err)?;
        }
        "delete" => {
            let key = a.pos(2, "resource")?;
            a.no_extra(3)?;
            let id = find(&f, key)?;
            f.doc().delete_resource(&id).map_err(err)?;
        }
        other => return Err(format!("unknown resource command `{other}`\n{USAGE}")),
    }
    f.save().map_err(err)
}

fn assign(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &["units"], &[])?;
    let (path, t, r) = (a.pos(0, "project")?, a.pos(1, "task")?, a.pos(2, "resource")?);
    a.no_extra(3)?;
    let units: f64 = a.opt("units").map(|u| u.trim().parse::<f64>().map_err(|_| format!("`{u}` is not a number"))).transpose()?.unwrap_or(1.0);
    let mut f = open(path)?;
    let tid = task(&f, t)?;
    let rid = f.doc().project().find_resource(r).map(|x| x.id.clone()).ok_or_else(|| format!("no resource `{r}`"))?;
    let id = f.doc().assign(&tid, &rid, units).map_err(err)?;
    f.save().map_err(err)?;
    outln!("{id}");
    Ok(())
}

fn holiday(args: &[String]) -> Result<(), String> {
    edit(args, 3, |f, a| {
        let day = a[2].parse().map_err(|e: plancraft_project::DateError| e.to_string())?;
        match a[1].as_str() {
            "add" => f.doc().add_holiday(day).map_err(err),
            "remove" => f.doc().remove_holiday(day).map_err(err),
            other => Err(format!("unknown holiday command `{other}` (add or remove)")),
        }
    })
}

fn devices(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &[], &[])?;
    let path = a.pos(0, "project")?;
    a.no_extra(1)?;
    let f = open(path)?;
    outln!("this device: {}", f.device());
    for d in f.device_files().map_err(err)? {
        let age = d.modified.and_then(|m| m.elapsed().ok()).map(ago).unwrap_or_else(|| "?".into());
        outln!("{}\t{:.1} KB\tsaved {age}", d.device, d.bytes as f64 / 1024.0);
    }
    Ok(())
}

fn ago(d: std::time::Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} s ago"),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

fn show(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args, &[], &["json", "all"])?;
    let path = a.pos(0, "project")?;
    a.no_extra(1)?;
    let f = open(path)?;
    let p = f.doc().project();
    if a.has("json") {
        let problems: Vec<String> = p.problems().iter().map(|x| x.to_string()).collect();
        let v = json!({ "project": p, "problems": problems, "device": f.device() });
        outln!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
        return Ok(());
    }
    print_project(&p, a.has("all"));
    Ok(())
}

fn print_project(p: &Project, all: bool) {
    print_header(p);
    if p.tasks.is_empty() {
        outln!("no tasks yet (plancraft-cli project add <project> <name>)");
    }
    let wbs_w = p.tasks.iter().map(|t| t.wbs.len()).max().unwrap_or(3).max(3);
    for t in &p.tasks {
        outln!("{}", task_line(t, wbs_w, all));
    }
    print_links(p, all);
    if all {
        print_resources_and_calendar(p);
    }
    let problems = p.problems();
    if !problems.is_empty() {
        outln!();
        outln!("problems:");
        for x in problems {
            outln!("  {x}");
        }
    }
}

fn print_header(p: &Project) {
    let i = &p.info;
    outln!(
        "{}{}",
        if i.title.is_empty() { "(untitled)" } else { &i.title },
        if i.purpose.is_empty() { String::new() } else { format!(": {}", i.purpose) }
    );
    let mut facts = Vec::new();
    for (k, v) in [
        ("start", i.start.map(|d| d.to_string())),
        ("deadline", i.deadline.map(|d| d.to_string())),
        ("manager", Some(i.manager.clone()).filter(|s| !s.is_empty())),
        ("client", Some(i.client.clone()).filter(|s| !s.is_empty())),
        ("budget", i.budget.map(|b| format!("{b} {}", i.currency).trim().to_string())),
        ("method", Some(i.method.as_str().to_string())),
    ] {
        if let Some(v) = v {
            facts.push(format!("{k} {v}"));
        }
    }
    outln!("{}", facts.join(" · "));
    outln!();
}

fn print_links(p: &Project, all: bool) {
    if p.dependencies.is_empty() {
        return;
    }
    outln!();
    outln!("links:");
    for d in &p.dependencies {
        let label = |id: &TaskId| p.task(id).map(|t| t.wbs.clone()).unwrap_or_else(|| format!("(deleted {id})"));
        let lag = if d.lag_minutes == 0 { String::new() } else { format!(" {:+}h", f64::from(d.lag_minutes) / 60.0) };
        let id = if all { format!("  [{}]", d.id) } else { String::new() };
        outln!("  {} → {} {}{lag}{id}", label(&d.predecessor), label(&d.successor), d.kind.as_str());
    }
}

fn print_resources_and_calendar(p: &Project) {
    if !p.resources.is_empty() {
        outln!();
        outln!("resources:");
        for r in &p.resources {
            let role = if r.role.is_empty() { String::new() } else { format!(", {}", r.role) };
            outln!("  {} ({}, {}){role}  [{}]", r.name, r.kind.as_str(), r.max_units, r.id);
        }
    }
    let c = &p.calendar;
    let days: Vec<&str> = c.working_days.iter().zip(["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]).filter(|(w, _)| **w).map(|(_, d)| d).collect();
    let holidays = if c.holidays.is_empty() { String::new() } else { format!(", {} holidays", c.holidays.len()) };
    outln!();
    outln!("calendar: {}, {} h/day{holidays}", days.join(" "), c.hours_per_day);
}

fn task_line(t: &Task, wbs_w: usize, all: bool) -> String {
    let indent = "  ".repeat(t.outline_level.saturating_sub(1) as usize);
    let days = f64::from(t.duration_minutes) / f64::from(plancraft_project::MINUTES_PER_DAY);
    let dur = if t.milestone {
        "◆".to_string()
    } else if t.duration_minutes == 0 {
        String::new()
    } else {
        format!("{days}d")
    };
    let dates = match (t.start, t.finish) {
        (Some(s), Some(f)) => format!("{s} → {f}"),
        (Some(s), None) => format!("{s} →"),
        (None, Some(f)) => format!("→ {f}"),
        (None, None) => String::new(),
    };
    let mut line = format!("{:<wbs_w$}  {indent}{}", t.wbs, if t.name.is_empty() { "(no name)" } else { &t.name });
    for extra in [dur, dates, if t.percent_complete > 0 { format!("{}%", t.percent_complete) } else { String::new() }] {
        if !extra.is_empty() {
            line.push_str("  ");
            line.push_str(&extra);
        }
    }
    if t.cancelled {
        line.push_str("  (cancelled)");
    }
    if all {
        line.push_str(&format!("  [{}]", t.id));
    }
    line
}
