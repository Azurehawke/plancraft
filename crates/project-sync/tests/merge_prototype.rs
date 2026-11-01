//! Phase 1 prototype test for Loro (PLAN.md § Multi-device design): a project with a few thousand
//! tasks is edited offline on three devices, with edits that clash on purpose, then synced the
//! way Syncthing does it (each device's change file copied to the others, in different orders and
//! at different times). Every device must end with the same project, with no task duplicated or
//! lost and no task under its own subtask. Sizes and timings are printed for the plan
//! (`cargo test -p plancraft-project-sync --release --test merge_prototype -- --nocapture`).
#![allow(clippy::disallowed_methods)] // Instant::now, for timings; this test never runs on wasm

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use plancraft_project::{DependencyKind, Problem, Project, TaskId};
use plancraft_project_sync::{Placement, ProjectFolder};

const DEVICES: [&str; 3] = ["laptop", "tablet", "surface"];

/// Three copies of one shared folder, one per device, and a Syncthing stand-in between them.
struct Lab {
    _root: tempfile::TempDir,
    dirs: HashMap<&'static str, PathBuf>,
    /// How many times each device has saved its own file; a copy of a file only replaces an older one.
    saves: HashMap<&'static str, u64>,
    /// Which save of each device's file each replica holds.
    held: HashMap<(&'static str, &'static str), u64>,
}

impl Lab {
    fn new() -> Lab {
        let root = tempfile::tempdir().unwrap();
        let dirs = DEVICES.iter().map(|d| (*d, root.path().join(d))).collect::<HashMap<_, _>>();
        for p in dirs.values() {
            std::fs::create_dir_all(p).unwrap();
        }
        Lab { _root: root, dirs, saves: HashMap::new(), held: HashMap::new() }
    }

    fn plan(&self, device: &str) -> PathBuf {
        self.dirs[device].join("Job.plan")
    }

    fn open(&self, device: &'static str) -> ProjectFolder {
        ProjectFolder::open(&self.plan(device), device).unwrap()
    }

    fn save(&mut self, f: &mut ProjectFolder, device: &'static str) {
        f.save().unwrap();
        let n = self.saves.entry(device).or_insert(0);
        *n += 1;
        self.held.insert((device, device), *n);
    }

    /// Copies every device file `from` holds that is newer than what `to` holds (Syncthing's rule).
    fn sync(&mut self, from: &'static str, to: &'static str) {
        for owner in DEVICES {
            let have = self.held.get(&(from, owner)).copied().unwrap_or(0);
            let there = self.held.get(&(to, owner)).copied().unwrap_or(0);
            if have > there {
                let name = format!("devices/{owner}.loro");
                std::fs::copy(self.plan(from).join(&name), self.plan(to).join(&name)).unwrap();
                self.held.insert((to, owner), have);
            }
        }
    }

    fn sync_all(&mut self) {
        for _ in 0..2 {
            for a in DEVICES {
                for b in DEVICES {
                    if a != b {
                        self.sync(a, b);
                    }
                }
            }
        }
    }

    /// Copies the whole folder from the first device to the others (the first sync of a new project).
    fn share_from(&mut self, device: &'static str) {
        for other in DEVICES {
            if other != device {
                copy_dir(&self.plan(device), &self.plan(other));
                for owner in DEVICES {
                    if let Some(v) = self.held.get(&(device, owner)).copied() {
                        self.held.insert((other, owner), v);
                    }
                }
            }
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// xorshift64*: a fixed seed gives the same "random" edits on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// The tree must be whole: ids unique, every parent present and listed before its subtasks,
/// WBS codes consistent with the outline.
fn check_tree(p: &Project) {
    let mut seen = HashSet::new();
    let mut level: HashMap<&TaskId, u32> = HashMap::new();
    for t in &p.tasks {
        assert!(seen.insert(&t.id), "task {} appears twice", t.id);
        match &t.parent {
            Some(parent) => {
                let pl = level.get(parent).copied().unwrap_or_else(|| panic!("parent {parent} of {} missing or after it", t.id));
                assert_eq!(t.outline_level, pl + 1);
                let pw = &p.task(parent).unwrap().wbs;
                assert!(t.wbs.starts_with(&format!("{pw}.")), "{} under {}", t.wbs, pw);
            }
            None => assert_eq!(t.outline_level, 1),
        }
        level.insert(&t.id, t.outline_level);
    }
}

fn same_everywhere(lab: &Lab) -> Project {
    let projects: Vec<(&str, Project)> = DEVICES.iter().map(|d| (*d, lab.open(d).doc().project())).collect();
    let first = &projects[0].1;
    for (d, p) in &projects[1..] {
        if p != first {
            let a = serde_json::to_string_pretty(first).unwrap();
            let b = serde_json::to_string_pretty(p).unwrap();
            let line = a.lines().zip(b.lines()).position(|(x, y)| x != y).unwrap_or(0);
            panic!(
                "{} and {d} differ near line {line}:\n{}\nvs\n{}",
                projects[0].0,
                a.lines().nth(line).unwrap_or(""),
                b.lines().nth(line).unwrap_or("")
            );
        }
    }
    check_tree(first);
    first.clone()
}

/// A job of `phases × groups × tasks` tasks (plus summaries) with finish-to-start chains,
/// resources and assignments.
fn build_base(f: &ProjectFolder, phases: usize, groups: usize, tasks: usize) {
    let d = f.doc();
    d.set_info("start", "2026-11-02").unwrap();
    let crew: Vec<String> = (1..=8).map(|i| d.add_resource(&format!("Tech {i}")).unwrap()).collect();
    let mut prev: Option<TaskId> = None;
    for p in 0..phases {
        let ph = d.add_task(&format!("Phase {}", p + 1), &Placement::End { parent: None }).unwrap();
        for g in 0..groups {
            let gr = d.add_task(&format!("Area {}.{}", p + 1, g + 1), &Placement::End { parent: Some(ph.clone()) }).unwrap();
            for t in 0..tasks {
                let id = d.add_task(&format!("Drop {}-{}-{}", p + 1, g + 1, t + 1), &Placement::End { parent: Some(gr.clone()) }).unwrap();
                d.set_task(&id, "duration", "4h").unwrap();
                if let Some(pv) = &prev {
                    d.link(pv, &id, DependencyKind::Fs, 0).unwrap();
                }
                d.assign(&id, &crew[(p + g + t) % crew.len()], 1.0).unwrap();
                prev = Some(id);
            }
        }
    }
}

#[test]
fn three_devices_offline_with_clashing_edits_converge() {
    let mut lab = Lab::new();
    let t0 = Instant::now();
    let mut laptop = ProjectFolder::create(&lab.dirs["laptop"], "Job", "laptop").unwrap();
    build_base(&laptop, 10, 10, 30); // 3,000 tasks + 110 summaries
    lab.save(&mut laptop, "laptop");
    let built = t0.elapsed();
    lab.share_from("laptop");
    let base = lab.open("tablet").doc().project();
    assert_eq!(base.tasks.len(), 3110);
    let summaries: Vec<TaskId> = base.tasks.iter().filter(|t| t.outline_level == 2).map(|t| t.id.clone()).collect();
    let leaves: Vec<TaskId> = base.tasks.iter().filter(|t| t.outline_level == 3).map(|t| t.id.clone()).collect();
    let phases: Vec<TaskId> = base.tasks.iter().filter(|t| t.outline_level == 1).map(|t| t.id.clone()).collect();

    let mut laptop = lab.open("laptop");
    let mut tablet = lab.open("tablet");
    let mut surface = lab.open("surface");
    let (l, t, s) = (laptop.doc().clone(), tablet.doc().clone(), surface.doc().clone());
    let (x, y) = (summaries[0].clone(), summaries[1].clone());

    // 1. Crossed moves: X under Y on the laptop, Y under X on the tablet.
    l.move_task(&x, &Placement::End { parent: Some(y.clone()) }).unwrap();
    t.move_task(&y, &Placement::End { parent: Some(x.clone()) }).unwrap();
    // 2. Same field, different values.
    l.set_task(&leaves[5], "name", "Laptop's name").unwrap();
    t.set_task(&leaves[5], "name", "Tablet's name").unwrap();
    s.set_task(&leaves[5], "name", "Surface's name").unwrap();
    // 3. Different fields of one task.
    l.set_task(&leaves[6], "duration", "3d").unwrap();
    t.set_task(&leaves[6], "percent", "50").unwrap();
    s.set_task(&leaves[6], "notes", "Ceiling tiles are asbestos-free (tested).").unwrap();
    // 4. Delete on one device, edit and add underneath on another.
    let doomed = summaries[20].clone();
    let doomed_child = base.children(Some(&doomed)).next().unwrap().id.clone();
    l.delete_task(&doomed).unwrap();
    t.set_task(&doomed_child, "percent", "100").unwrap();
    t.add_task("Added under a deleted area", &Placement::End { parent: Some(doomed.clone()) }).unwrap();
    // 5. The same tasks reordered differently on each device.
    let area = summaries[30].clone();
    let kids: Vec<TaskId> = base.children(Some(&area)).map(|k| k.id.clone()).collect();
    for (i, k) in kids.iter().enumerate() {
        l.move_task(k, &Placement::At { parent: Some(area.clone()), index: 0 }).unwrap();
        t.move_task(k, &Placement::At { parent: Some(area.clone()), index: kids.len() - i }).unwrap();
        s.move_task(&kids[(i * 7) % kids.len()], &Placement::At { parent: Some(area.clone()), index: i % 3 }).unwrap();
    }
    // 6. Fifty new tasks at the same place on each device.
    for i in 0..50 {
        for (dev, d) in [("laptop", &l), ("tablet", &t), ("surface", &s)] {
            d.add_task(&format!("{dev} extra {i}"), &Placement::End { parent: Some(phases[9].clone()) }).unwrap();
        }
    }
    // 7. Crossed links (a loop only after merging) and a link to a task deleted elsewhere.
    let (a, b) = (leaves[100].clone(), leaves[200].clone());
    l.link(&a, &b, DependencyKind::Fs, 0).unwrap();
    t.link(&b, &a, DependencyKind::Fs, 0).unwrap();
    t.link(&leaves[300], &doomed_child, DependencyKind::Fs, 0).unwrap();
    // 8. Indent and outdent the same task at once.
    l.indent(&leaves[401]).unwrap();
    s.outdent(&leaves[401]).unwrap();
    // 9. Project information on two devices.
    l.set_info("manager", "Steve").unwrap();
    t.set_info("deadline", "2026-12-18").unwrap();

    // 10. Seeded random edits: 1,500 per device.
    let t1 = Instant::now();
    for (seed, d) in [(1u64, &l), (2, &t), (3, &s)] {
        random_edits(d, seed, 1500);
    }
    let edited = t1.elapsed();

    lab.save(&mut laptop, "laptop");
    lab.save(&mut tablet, "tablet");
    lab.save(&mut surface, "surface");

    // Partial syncs in different orders before everyone has everything.
    lab.sync("tablet", "surface");
    lab.sync("surface", "laptop");
    let mid = lab.open("laptop").doc().project();
    check_tree(&mid);
    lab.sync_all();

    let t2 = Instant::now();
    let merged = same_everywhere(&lab);
    let opened = t2.elapsed() / DEVICES.len() as u32;

    // What each clash became (the random edits may have changed these tasks again afterwards,
    // so only what must hold whatever they did is checked).
    let xt = merged.task(&x).unwrap();
    let yt = merged.task(&y).unwrap();
    assert!(!(xt.parent.as_ref() == Some(&y) && yt.parent.as_ref() == Some(&x)), "crossed moves made a loop");
    assert!(merged.task(&doomed).is_none(), "deleted area came back");
    assert!(merged.tasks.iter().all(|t| t.name != "Added under a deleted area" || t.parent.is_none()));
    assert!(
        merged.problems().iter().any(|p| matches!(p, Problem::DependencyLoop { .. } | Problem::DanglingDependency { .. })),
        "crossed links and the link to a deleted task should be reported"
    );
    assert_eq!(merged.info.manager, "Steve");
    assert_eq!(merged.info.deadline.map(|d| d.to_string()).as_deref(), Some("2026-12-18"));

    let sizes: Vec<(String, u64)> = lab.open("laptop").device_files().unwrap().into_iter().map(|f| (f.device, f.bytes)).collect();
    eprintln!("--- Loro prototype ---");
    eprintln!("tasks after merge: {} (base 3110)", merged.tasks.len());
    eprintln!("links: {}, assignments: {}, problems: {}", merged.dependencies.len(), merged.assignments.len(), merged.problems().len());
    eprintln!("build base: {built:?}; 4,500 random edits: {edited:?}; open+merge per device: {opened:?}");
    for (d, b) in sizes {
        eprintln!("devices/{d}.loro: {:.1} KB", b as f64 / 1024.0);
    }
}

fn random_edits(d: &plancraft_project_sync::ProjectDoc, seed: u64, n: usize) {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0x1000_0000_01b3));
    let mut tasks: Vec<TaskId> = Vec::new();
    for i in 0..n {
        // Re-read the task list now and then; edits on a task deleted since then just fail.
        if i % 50 == 0 {
            tasks = d.project().tasks.into_iter().map(|t| t.id).collect();
        }
        if tasks.is_empty() {
            break;
        }
        let pick = |r: &mut Rng| tasks[r.below(tasks.len())].clone();
        let _ = match r.below(10) {
            0 => d.add_task(&format!("r{seed}-{i}"), &Placement::After(pick(&mut r))).map(|_| ()),
            1 => d.move_task(&pick(&mut r), &Placement::End { parent: Some(pick(&mut r)) }),
            2 => d.move_task(&pick(&mut r), &Placement::Before(pick(&mut r))),
            3 => d.indent(&pick(&mut r)),
            4 => d.outdent(&pick(&mut r)),
            5 => d.set_task(&pick(&mut r), "percent", &(r.below(101)).to_string()),
            6 => d.set_task(&pick(&mut r), "name", &format!("renamed {seed}-{i}")),
            7 => d.link(&pick(&mut r), &pick(&mut r), DependencyKind::Fs, 0).map(|_| ()),
            8 if r.below(10) == 0 => d.delete_task(&pick(&mut r)),
            _ => d.set_task(&pick(&mut r), "duration", &format!("{}h", r.below(40))),
        };
    }
}

#[test]
fn the_order_of_syncing_does_not_matter() {
    // Each device's file merged in every order gives the same project.
    let mut lab = Lab::new();
    let mut laptop = ProjectFolder::create(&lab.dirs["laptop"], "Job", "laptop").unwrap();
    build_base(&laptop, 2, 3, 10);
    lab.save(&mut laptop, "laptop");
    lab.share_from("laptop");
    let mut files = Vec::new();
    for (seed, dev) in [(7u64, "laptop"), (8, "tablet"), (9, "surface")] {
        let mut f = lab.open(dev);
        random_edits(f.doc(), seed, 400);
        f.save().unwrap();
        files.push(std::fs::read(lab.plan(dev).join(format!("devices/{dev}.loro"))).unwrap());
    }
    let orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let results: Vec<Project> = orders
        .iter()
        .map(|o| {
            let d = plancraft_project_sync::ProjectDoc::new();
            for i in o {
                d.import(&files[*i]).unwrap();
            }
            d.project()
        })
        .collect();
    for r in &results[1..] {
        assert_eq!(r, &results[0]);
    }
    check_tree(&results[0]);
}

#[test]
fn a_half_synced_file_is_retried_not_lost() {
    let mut lab = Lab::new();
    let mut laptop = ProjectFolder::create(&lab.dirs["laptop"], "Job", "laptop").unwrap();
    build_base(&laptop, 1, 1, 5);
    lab.save(&mut laptop, "laptop");
    lab.share_from("laptop");
    let mut tablet = lab.open("tablet");
    tablet.doc().add_task("From the tablet", &Placement::End { parent: None }).unwrap();
    tablet.save().unwrap();
    let full = std::fs::read(lab.plan("tablet").join("devices/tablet.loro")).unwrap();
    // The laptop sees the file while it is still arriving.
    let target = lab.plan("laptop").join("devices/tablet.loro");
    std::fs::write(&target, &full[..full.len() / 2]).unwrap();
    let mut laptop = lab.open("laptop");
    assert!(!laptop.warnings().is_empty());
    assert!(!laptop.doc().project().tasks.iter().any(|t| t.name == "From the tablet"));
    std::fs::write(&target, &full).unwrap();
    assert!(laptop.reload().unwrap());
    assert!(laptop.doc().project().tasks.iter().any(|t| t.name == "From the tablet"));
}

#[test]
fn each_kind_of_clash_resolves_predictably() {
    use plancraft_project_sync::ProjectDoc;
    let base = ProjectDoc::new();
    let x = base.add_task("X", &Placement::End { parent: None }).unwrap();
    let y = base.add_task("Y", &Placement::End { parent: None }).unwrap();
    let t = base.add_task("T", &Placement::End { parent: None }).unwrap();
    let gone = base.add_task("Gone", &Placement::End { parent: None }).unwrap();
    let kid = base.add_task("Kid", &Placement::End { parent: Some(gone.clone()) }).unwrap();
    let snap = base.export_snapshot().unwrap();
    let (a, b) = (ProjectDoc::new(), ProjectDoc::new());
    a.import(&snap).unwrap();
    b.import(&snap).unwrap();

    a.set_task(&t, "name", "From A").unwrap();
    b.set_task(&t, "name", "From B").unwrap();
    a.set_task(&t, "duration", "2d").unwrap();
    b.set_task(&t, "percent", "25").unwrap();
    a.move_task(&x, &Placement::End { parent: Some(y.clone()) }).unwrap();
    b.move_task(&y, &Placement::End { parent: Some(x.clone()) }).unwrap();
    a.delete_task(&gone).unwrap();
    b.set_task(&kid, "notes", "edited after the delete").unwrap();

    let (sa, sb) = (a.export_snapshot().unwrap(), b.export_snapshot().unwrap());
    a.import(&sb).unwrap();
    b.import(&sa).unwrap();
    let (pa, pb) = (a.project(), b.project());
    assert_eq!(pa, pb, "both devices end the same");
    check_tree(&pa);

    let tt = pa.task(&t).unwrap();
    assert!(tt.name == "From A" || tt.name == "From B", "one of the two names wins: {}", tt.name);
    assert_eq!((tt.duration_minutes, tt.percent_complete), (960, 25), "edits to different fields both land");
    let (xp, yp) = (pa.task(&x).unwrap().parent.clone(), pa.task(&y).unwrap().parent.clone());
    assert!(xp == Some(y.clone()) || yp == Some(x.clone()), "one of the crossed moves wins");
    assert!(!(xp == Some(y.clone()) && yp == Some(x.clone())), "never both (a loop)");
    assert!(pa.task(&gone).is_none() && pa.task(&kid).is_none(), "a delete wins over an edit underneath it");
}
