//! `plancraft-cli project`: two devices editing one project offline, then synced.

use std::path::Path;
use std::process::{Command, Output};

fn run(device: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_plancraft-cli")).env("PLANCRAFT_DEVICE", device).args(args).output().expect("run plancraft-cli")
}

fn ok(device: &str, args: &[&str]) -> String {
    let out = run(device, args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        if e.path().is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()));
        } else {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

#[test]
fn two_devices_edit_offline_and_merge() {
    let root = tempfile::tempdir().unwrap();
    let (laptop, surface) = (root.path().join("laptop"), root.path().join("surface"));
    std::fs::create_dir_all(&laptop).unwrap();
    let made = ok("laptop", &["project", "new", laptop.to_str().unwrap(), "Acme Network"]);
    let lp = made.trim().to_string();
    assert!(Path::new(&lp).join("Acme Network.plancraft").is_file());
    ok("laptop", &["project", "info", &lp, "deadline", "2026-12-18"]);
    ok("laptop", &["project", "add", &lp, "Survey"]);
    let added = ok("laptop", &["project", "add", &lp, "Walk the site", "--under", "1", "--duration", "4h"]);
    assert!(added.starts_with("1.1\t"), "{added}");
    ok("laptop", &["project", "add", &lp, "Install"]);
    ok("laptop", &["project", "add", &lp, "Pull cable", "--under", "2", "--duration", "3d"]);
    ok("laptop", &["project", "link", &lp, "1.1", "2.1", "--lag", "1d"]);
    ok("laptop", &["project", "resource", &lp, "add", "Tech 1"]);
    ok("laptop", &["project", "assign", &lp, "2.1", "tech 1", "--units", "0.5"]);

    copy_dir(&laptop, &surface);
    let sp = surface.join("Acme Network.plan");
    let sp = sp.to_str().unwrap();

    // Offline on both.
    ok("laptop", &["project", "set", &lp, "2.1", "percent", "25"]);
    ok("laptop", &["project", "add", &lp, "Label the drops", "--under", "2"]);
    ok("surface", &["project", "set", sp, "2.1", "notes", "drop ceiling"]);
    ok("surface", &["project", "move", sp, "1.1", "--top"]);

    // Syncthing copies each device's file to the other.
    std::fs::copy(Path::new(&lp).join("devices/laptop.loro"), Path::new(sp).join("devices/laptop.loro")).unwrap();
    std::fs::copy(Path::new(sp).join("devices/surface.loro"), Path::new(&lp).join("devices/surface.loro")).unwrap();

    let a = ok("laptop", &["project", "show", &lp, "--json"]);
    let b = ok("surface", &["project", "show", sp, "--json"]);
    let (mut a, mut b): (serde_json::Value, serde_json::Value) = (serde_json::from_str(&a).unwrap(), serde_json::from_str(&b).unwrap());
    a["device"] = serde_json::Value::Null;
    b["device"] = serde_json::Value::Null;
    assert_eq!(a, b, "both devices see the same project");
    let tasks = a["project"]["tasks"].as_array().unwrap();
    let names: Vec<&str> = tasks.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Survey", "Install", "Pull cable", "Label the drops", "Walk the site"]);
    let pull = &tasks[2];
    assert_eq!((pull["percent_complete"].as_u64(), pull["notes"].as_str()), (Some(25), Some("drop ceiling")));
    assert_eq!(a["project"]["dependencies"][0]["lag_minutes"], 480);
    assert_eq!(a["project"]["assignments"][0]["units"], 0.5);
    assert_eq!(a["project"]["info"]["deadline"], "2026-12-18");

    let text = ok("laptop", &["project", "show", &lp]);
    assert!(text.contains("Acme Network") && text.contains("Pull cable") && text.contains("scheduled ") && text.contains("links:"), "{text}");
    let devs = ok("laptop", &["project", "devices", &lp]);
    assert!(devs.contains("this device: laptop") && devs.contains("surface"), "{devs}");
}

#[test]
fn bad_input_is_an_error_not_a_crash() {
    let root = tempfile::tempdir().unwrap();
    let p = ok("pc", &["project", "new", root.path().to_str().unwrap(), "Job"]);
    let p = p.trim();
    for args in [
        vec!["project", "set", p, "9.9", "name", "x"],
        vec!["project", "set", p, "1", "name", "x"],
        vec!["project", "info", p, "colour", "red"],
        vec!["project", "move", p, "1"],
        vec!["project", "nonsense"],
        vec!["project", "show", "/no/such/project.plan"],
        vec!["project", "link", p, "1", "1"],
        vec!["project", "holiday", p, "add", "christmas"],
    ] {
        let out = run("pc", &args);
        assert!(!out.status.success(), "{args:?} should fail");
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"), "{args:?}");
    }
    ok("pc", &["project", "add", p, "A"]);
    let out = run("pc", &["project", "set", p, "1", "start", "31/12/2026"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("YYYY-MM-DD"));
    assert!(ok("pc", &["project", "help"]).contains("project new"));
}
