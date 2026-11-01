//! A project on disk:
//!
//! ```text
//! Acme-Network.plan\
//!     Acme-Network.plancraft     entry file: opens the project (written once, at creation)
//!     devices\
//!         laptop-3f9a.loro       everything the laptop knows (only the laptop writes it)
//!         surface-pro-7-1c20.loro
//! ```
//!
//! Each device writes only its own change file, as a complete Loro snapshot of what it knows, so
//! any one file is enough to rebuild the project and the files never conflict in a sync tool.
//! Opening imports every `.loro` file in `devices\`, including sync-conflict copies (merging them
//! is harmless and recovers anything in them). A file that can't be read (half-synced, damaged)
//! is skipped with a warning and tried again on the next [`ProjectFolder::reload`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::{Error, ProjectDoc, Result, random_id};

/// The project folder's extension (`Name.plan`).
pub const FOLDER_EXTENSION: &str = "plan";
/// The entry file's extension (`Name.plancraft`).
pub const ENTRY_EXTENSION: &str = "plancraft";
/// The folder format this code writes.
pub const FORMAT_VERSION: u32 = 1;
const DEVICES: &str = "devices";
const CHANGES_EXTENSION: &str = "loro";
/// Largest change file read (hostile input; a real project is far smaller).
const MAX_FILE: u64 = 512 * 1024 * 1024;

/// The entry file's contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryFile {
    /// Always `"plancraft"`.
    pub format: String,
    pub format_version: u32,
    /// Stable id of the project (survives renaming the folder).
    pub project_id: String,
    /// The title at creation, for file browsers; the live title is in the project itself.
    pub title: String,
}

/// One device's change file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFile {
    /// The device name (the file name without `.loro`).
    pub device: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
}

/// An open project folder.
#[derive(Debug)]
pub struct ProjectFolder {
    dir: PathBuf,
    entry: EntryFile,
    device: String,
    doc: ProjectDoc,
    /// Size and time of each change file when last imported, so reload reads only new ones.
    seen: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
    warnings: Vec<String>,
}

impl ProjectFolder {
    /// Makes `parent\<name>.plan\` with its entry file, and opens it as `device`.
    pub fn create(parent: &Path, name: &str, device: &str) -> Result<ProjectFolder> {
        let stem = file_stem_for(name);
        let dir = parent.join(format!("{stem}.{FOLDER_EXTENSION}"));
        if dir.exists() {
            return Err(Error::Exists(dir.display().to_string()));
        }
        mkdir(&dir.join(DEVICES))?;
        let entry = EntryFile { format: "plancraft".into(), format_version: FORMAT_VERSION, project_id: random_id(), title: name.trim().to_string() };
        let json = serde_json::to_string_pretty(&entry).map_err(|e| Error::Data(e.to_string()))?;
        write_atomic(&dir.join(format!("{stem}.{ENTRY_EXTENSION}")), json.as_bytes())?;
        let mut f = ProjectFolder::open(&dir, device)?;
        f.doc.set_info("title", name.trim())?;
        f.save()?;
        Ok(f)
    }

    /// Opens a project from its `.plan` folder or its `.plancraft` entry file.
    pub fn open(path: &Path, device: &str) -> Result<ProjectFolder> {
        let device = crate::sanitize_device_name(device);
        if device.is_empty() {
            return Err(Error::BadValue { field: "device".into(), reason: "a device name is needed".into() });
        }
        let (dir, entry_path) = locate(path)?;
        let text = std::fs::read_to_string(&entry_path).map_err(|source| Error::Io { path: entry_path.display().to_string(), source })?;
        let entry: EntryFile = serde_json::from_str(&text).map_err(|e| Error::Data(format!("{}: {e}", entry_path.display())))?;
        if entry.format != "plancraft" {
            return Err(Error::NotAProject(entry_path.display().to_string()));
        }
        if entry.format_version > FORMAT_VERSION {
            return Err(Error::Data(format!(
                "{} was made by a newer PlanCraft (format {}); update PlanCraft to open it",
                entry_path.display(),
                entry.format_version
            )));
        }
        let mut f = ProjectFolder { dir, entry, device, doc: ProjectDoc::new(), seen: BTreeMap::new(), warnings: Vec::new() };
        f.reload()?;
        Ok(f)
    }

    /// Imports change files that are new or changed since the last load (another device's save
    /// that the sync tool just delivered). Returns whether anything was imported.
    pub fn reload(&mut self) -> Result<bool> {
        self.warnings.clear();
        let mut changed = false;
        for file in self.device_files()? {
            let stamp = (file.bytes, file.modified);
            if self.seen.get(&file.path) == Some(&stamp) {
                continue;
            }
            if file.bytes > MAX_FILE {
                self.warnings.push(format!("{} is too large to read ({} bytes); skipped", file.path.display(), file.bytes));
                continue;
            }
            match std::fs::read(&file.path) {
                Ok(bytes) => match self.doc.import(&bytes) {
                    Ok(()) => {
                        self.seen.insert(file.path.clone(), stamp);
                        changed = true;
                    }
                    Err(e) => self.warnings.push(format!("{}: {e}; skipped (will retry)", file.path.display())),
                },
                Err(e) => self.warnings.push(format!("{}: {e}; skipped (will retry)", file.path.display())),
            }
        }
        if !self.warnings.is_empty() {
            for w in &self.warnings {
                log::warn!("{w}");
            }
        }
        Ok(changed)
    }

    /// Writes this device's change file. Reads it back first, so a second window on the same
    /// device that saved in the meantime isn't overwritten.
    pub fn save(&mut self) -> Result<()> {
        let path = self.own_file();
        if let Ok(bytes) = std::fs::read(&path) {
            // A damaged own file is replaced by the good snapshot below.
            let _ = self.doc.import(&bytes);
        }
        let bytes = self.doc.export_snapshot()?;
        write_atomic(&path, &bytes)?;
        let meta = std::fs::metadata(&path).ok();
        self.seen.insert(path, (meta.as_ref().map(|m| m.len()).unwrap_or(0), meta.and_then(|m| m.modified().ok())));
        Ok(())
    }

    /// The project document (edit through it, then [`ProjectFolder::save`]).
    pub fn doc(&self) -> &ProjectDoc {
        &self.doc
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entry(&self) -> &EntryFile {
        &self.entry
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    /// Files skipped by the last reload.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    fn own_file(&self) -> PathBuf {
        self.dir.join(DEVICES).join(format!("{}.{CHANGES_EXTENSION}", self.device))
    }

    /// Every change file in `devices\`, sorted by name.
    pub fn device_files(&self) -> Result<Vec<DeviceFile>> {
        let dir = self.dir.join(DEVICES);
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(Error::Io { path: dir.display().to_string(), source }),
        };
        let mut out = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // `.syncthing.*` and other dot files are temporary files being written.
            if name.starts_with('.') || path.extension().and_then(|e| e.to_str()) != Some(CHANGES_EXTENSION) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let device = name.trim_end_matches(&format!(".{CHANGES_EXTENSION}")).to_string();
            out.push(DeviceFile { device, path, bytes: meta.len(), modified: meta.modified().ok() });
        }
        out.sort_by(|a, b| a.device.cmp(&b.device));
        Ok(out)
    }
}

/// The folder and entry file for a `.plan` folder or `.plancraft` file.
fn locate(path: &Path) -> Result<(PathBuf, PathBuf)> {
    let not = || Error::NotAProject(path.display().to_string());
    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) != Some(ENTRY_EXTENSION) {
            return Err(not());
        }
        let dir = path.parent().ok_or_else(not)?.to_path_buf();
        return Ok((dir, path.to_path_buf()));
    }
    if path.is_dir() {
        let rd = std::fs::read_dir(path).map_err(|source| Error::Io { path: path.display().to_string(), source })?;
        let mut entries: Vec<PathBuf> =
            rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some(ENTRY_EXTENSION)).collect();
        entries.sort();
        // The one named after the folder first, then any other.
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let pick = entries.iter().find(|p| p.file_stem().map(|s| s.to_string_lossy() == stem).unwrap_or(false)).or_else(|| entries.first());
        return pick.map(|p| (path.to_path_buf(), p.clone())).ok_or_else(not);
    }
    Err(not())
}

/// A file-name-safe version of a project name (keeps spaces, letters and digits from any script).
fn file_stem_for(name: &str) -> String {
    let s: String =
        name.trim().chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') { c } else { '-' }).collect();
    let s = s.trim_matches(|c: char| c == '.' || c == ' ').chars().take(80).collect::<String>();
    if s.is_empty() { "Project".to_string() } else { s }
}

fn mkdir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|source| Error::Io { path: dir.display().to_string(), source })
}

/// Writes a whole file or nothing: into a temporary file in the same folder, then renamed over the
/// target. The temporary name starts with `.syncthing.`, which Syncthing treats as its own
/// temporary file and never syncs.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = dir.join(format!(".syncthing.{name}.{}.tmp", random_id().get(..8).unwrap_or("0")));
    let io = |source| Error::Io { path: path.display().to_string(), source };
    std::fs::write(&tmp, bytes).map_err(io)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems() {
        assert_eq!(file_stem_for("Acme Network"), "Acme Network");
        assert_eq!(file_stem_for("a/b\\c:d"), "a-b-c-d");
        assert_eq!(file_stem_for("  ..  "), "Project");
        assert_eq!(file_stem_for("Ünïcode Job"), "Ünïcode Job");
    }

    #[test]
    fn create_open_save_reload() {
        let root = tempfile::tempdir().unwrap();
        let mut a = ProjectFolder::create(root.path(), "Acme Network", "laptop").unwrap();
        assert!(root.path().join("Acme Network.plan/Acme Network.plancraft").is_file());
        let t = a.doc().add_task("Survey", &crate::Placement::End { parent: None }).unwrap();
        a.save().unwrap();

        // Open through the entry file, as a second device.
        let mut b = ProjectFolder::open(&root.path().join("Acme Network.plan/Acme Network.plancraft"), "tablet").unwrap();
        assert_eq!(b.doc().project().info.title, "Acme Network");
        assert_eq!(b.doc().project().tasks.len(), 1);
        b.doc().set_task(&t, "duration", "2d").unwrap();
        b.save().unwrap();

        assert!(a.reload().unwrap());
        assert_eq!(a.doc().project().tasks[0].duration_minutes, 960);
        assert!(!a.reload().unwrap(), "nothing new");
        let names: Vec<String> = a.device_files().unwrap().into_iter().map(|f| f.device).collect();
        assert_eq!(names, ["laptop", "tablet"]);
    }

    #[test]
    fn damaged_and_temporary_files_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        let mut a = ProjectFolder::create(root.path(), "Job", "laptop").unwrap();
        a.doc().add_task("One", &crate::Placement::End { parent: None }).unwrap();
        a.save().unwrap();
        let devices = root.path().join("Job.plan/devices");
        std::fs::write(devices.join("broken.loro"), b"not loro at all").unwrap();
        std::fs::write(devices.join(".syncthing.tablet.loro.tmp"), b"half").unwrap();
        let b = ProjectFolder::open(&root.path().join("Job.plan"), "tablet").unwrap();
        assert_eq!(b.doc().project().tasks.len(), 1);
        assert_eq!(b.warnings().len(), 1, "{:?}", b.warnings());
    }

    #[test]
    fn conflict_copies_are_merged() {
        let root = tempfile::tempdir().unwrap();
        let mut a = ProjectFolder::create(root.path(), "Job", "laptop").unwrap();
        a.doc().add_task("Kept only in the conflict copy", &crate::Placement::End { parent: None }).unwrap();
        a.save().unwrap();
        let devices = root.path().join("Job.plan/devices");
        std::fs::rename(devices.join("laptop.loro"), devices.join("laptop.sync-conflict-20261101-120000-ABCDEFG.loro")).unwrap();
        let b = ProjectFolder::open(&root.path().join("Job.plan"), "tablet").unwrap();
        assert_eq!(b.doc().project().tasks.len(), 1);
    }

    #[test]
    fn refuses_non_projects_and_existing_folders() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(ProjectFolder::open(root.path(), "x"), Err(Error::NotAProject(_))));
        ProjectFolder::create(root.path(), "Job", "laptop").unwrap();
        assert!(matches!(ProjectFolder::create(root.path(), "Job", "laptop"), Err(Error::Exists(_))));
        let newer = root.path().join("New.plan");
        std::fs::create_dir_all(&newer).unwrap();
        std::fs::write(newer.join("New.plancraft"), r#"{"format":"plancraft","format_version":99,"project_id":"x","title":"New"}"#).unwrap();
        assert!(matches!(ProjectFolder::open(&newer, "laptop"), Err(Error::Data(_))));
        assert!(ProjectFolder::open(&root.path().join("Job.plan"), "!!!").is_err());
    }
}
