//! PlanCraft's stored project: a Loro CRDT document, saved as one change file per device in a
//! shared folder, merged on open.
//!
//! - [`ProjectDoc`]: the document and every edit, on bytes only (no I/O), so it also runs in the
//!   browser and in the Obsidian plugin. [`ProjectDoc::project`] reads it as a
//!   [`plancraft_project::Project`].
//! - [`ProjectFolder`]: the on-disk form (`Name.plan\` with a `Name.plancraft` entry file and a
//!   `devices\` folder). Each device writes only its own `devices\<device>.loro`, so a sync tool
//!   such as Syncthing never sees two writers on one file; opening a project imports every
//!   device's file and Loro merges them.
//! - [`device_name`]: the name a device writes under, kept outside the shared folder.
//!
//! Layout of the document (root containers):
//!
//! | Container | Type | Holds |
//! |---|---|---|
//! | `info` | map | project information (title, purpose, dates, manager, budget…) |
//! | `calendar` | map | `working_days` (`"1111100"`, Monday first), `hours_per_day` |
//! | `holidays` | map | `YYYY-MM-DD` → `true` (a map, so two devices adding days both keep theirs) |
//! | `tasks` | movable tree | one node per task, in outline order; fields in the node's map |
//! | `deps` | map | dependency id → map (`pred`, `succ`, `kind`, `lag`) |
//! | `resources` | map | resource id → map |
//! | `assignments` | map | assignment id → map (`task`, `resource`, `units`) |
//! | `board` | map | `columns`: the board's column names, one per line |
//!
//! The tree is Loro's movable tree: indenting, outdenting and reordering on two devices at once
//! merge without duplicating a task or making a task its own ancestor.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod device;
mod doc;
mod folder;
pub mod revucraft;

pub use device::{device_name, sanitize_device_name};
pub use doc::{Placement, ProjectDoc, parse_duration};
pub use folder::{DeviceFile, ENTRY_EXTENSION, EntryFile, FOLDER_EXTENSION, FORMAT_VERSION, ProjectFolder};

/// What can go wrong reading, editing or saving a project.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no task {0}")]
    UnknownTask(String),
    #[error("no resource {0}")]
    UnknownResource(String),
    #[error("no link {0}")]
    UnknownDependency(String),
    #[error("no field `{0}`")]
    UnknownField(String),
    #[error("{field}: {reason}")]
    BadValue { field: String, reason: String },
    #[error("a task can't be moved under itself or one of its own subtasks")]
    WouldCycle,
    #[error("project data: {0}")]
    Data(String),
    #[error("RevuCraft markups file: {0}")]
    Sidecar(String),
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("{0} is not a PlanCraft project (expected a .plan folder or a .plancraft file)")]
    NotAProject(String),
    #[error("{0} already exists")]
    Exists(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<loro::LoroError> for Error {
    fn from(e: loro::LoroError) -> Self {
        match e {
            loro::LoroError::TreeError(loro::LoroTreeError::CyclicMoveError) => Error::WouldCycle,
            other => Error::Data(other.to_string()),
        }
    }
}

/// A random id as 16 hex digits. Loro already draws a random 64-bit peer id for every new
/// document (from the OS, or the browser's crypto on wasm), so this borrows that instead of
/// adding another source of randomness.
pub(crate) fn random_id() -> String {
    format!("{:016x}", loro::LoroDoc::new().peer_id())
}
