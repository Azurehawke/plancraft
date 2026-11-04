//! Markdown documents (Phase B): plain `.md` files in a project's `documents` folder, with the
//! Documents tab's file list and a text editor. The files are ordinary text, so any editor opens
//! them and Syncthing carries them with the project. Two devices editing the same file at once
//! produce a Syncthing conflict copy; the editor never merges them.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use egui::{RichText, Ui};
use gridcraft_ui_egui::theme::Tokens;

use crate::Shell;

/// The folder inside a `.plan` folder that holds the documents.
pub const FOLDER: &str = "documents";
const EXT: &str = "md";
/// Largest document the editor opens (a guard against a huge or binary file).
const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// How long the editor waits after the last keystroke before it saves.
const SAVE_DELAY_MS: f64 = 800.0;

/// The editor's state: which document is open, its text, and what is unsaved.
#[derive(Debug, Default)]
pub struct DocState {
    /// The project (index into `Shell::projects`) the state belongs to.
    pub project: Option<usize>,
    /// The open document's file name, e.g. `Charter.md`.
    pub selected: Option<String>,
    pub text: String,
    /// The text as last read from or written to the file.
    pub saved: String,
    /// When the text last changed and is still unsaved (the shell's clock, in milliseconds).
    pub dirty_since: Option<f64>,
    /// The file's modification time when it was last read or written.
    pub seen: Option<SystemTime>,
    pub new_name: String,
    /// The name being typed for the open document's rename, when renaming.
    pub rename_to: Option<String>,
    pub confirm_delete: bool,
    pub error: Option<String>,
    /// Whether the editor shows the rendered preview instead of the text.
    pub preview: bool,
    /// The open document's editor, for the Format group's commands and the live preview.
    pub edit_id: Option<egui::Id>,
    /// The markers copied by the Format group's Copy format, for Paste format.
    pub painter: Option<Vec<&'static str>>,
}

/// The documents folder of a `.plan` folder.
pub fn folder(project_dir: &Path) -> PathBuf {
    project_dir.join(FOLDER)
}

/// A typed document name, turned into a safe file name (`Name.md`).
pub fn file_name(name: &str) -> Result<String, String> {
    let typed = name.trim();
    let stem = typed.strip_suffix(".md").unwrap_or(typed).trim();
    if stem.is_empty() {
        return Err("Give the document a name.".into());
    }
    if stem.chars().count() > 100 {
        return Err("That name is too long (100 characters at most).".into());
    }
    if stem.starts_with('.') || stem.chars().any(|c| "/\\:*?\"<>|".contains(c) || c.is_control()) {
        return Err("A document name can't contain / \\ : * ? \" < > | or start with a dot.".into());
    }
    Ok(format!("{stem}.{EXT}"))
}

/// Whether `name` is a file name this module created (no folders, no tricks).
fn checked(name: &str) -> Result<(), String> {
    match file_name(name.strip_suffix(&format!(".{EXT}")).unwrap_or(name)) {
        Ok(ok) if ok == name => Ok(()),
        _ => Err(format!("`{name}` is not a document name.")),
    }
}

/// The document file names in a project, sorted without regard to case.
pub fn list(project_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder(project_dir))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == EXT) && e.file_type().is_ok_and(|t| t.is_file()))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// The text of a document.
pub fn read(project_dir: &Path, name: &str) -> Result<String, String> {
    checked(name)?;
    let path = folder(project_dir).join(name);
    let len = std::fs::metadata(&path).map(|m| m.len()).map_err(|e| format!("Can't open {name}: {e}"))?;
    if len > MAX_BYTES {
        return Err(format!("{name} is too large to edit here ({} MB).", len / (1024 * 1024)));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("Can't read {name}: {e}"))
}

/// When a document was last changed on disk.
pub fn modified(project_dir: &Path, name: &str) -> Option<SystemTime> {
    std::fs::metadata(folder(project_dir).join(name)).and_then(|m| m.modified()).ok()
}

/// Writes a document. The text goes to a temporary file first, so a crash mid-save can't leave
/// half a document behind.
pub fn write(project_dir: &Path, name: &str, text: &str) -> Result<(), String> {
    checked(name)?;
    let dir = folder(project_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Can't make the documents folder: {e}"))?;
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("Can't save {name}: {e}"))?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| format!("Can't save {name}: {e}"))
}

/// Makes a new document with a heading, and returns its file name. Refuses to overwrite.
pub fn create(project_dir: &Path, typed: &str) -> Result<String, String> {
    let name = file_name(typed)?;
    if folder(project_dir).join(&name).exists() {
        return Err(format!("There's already a document called {name}."));
    }
    let title = name.strip_suffix(".md").unwrap_or(&name);
    write(project_dir, &name, &format!("# {title}\n\n"))?;
    Ok(name)
}

/// Renames a document, and returns its new file name. Refuses to overwrite another document.
pub fn rename(project_dir: &Path, from: &str, typed: &str) -> Result<String, String> {
    checked(from)?;
    let to = file_name(typed)?;
    let dir = folder(project_dir);
    if dir.join(&to).exists() {
        return Err(format!("There's already a document called {to}."));
    }
    std::fs::rename(dir.join(from), dir.join(&to)).map_err(|e| format!("Can't rename {from}: {e}"))?;
    Ok(to)
}

/// Deletes a document.
pub fn delete(project_dir: &Path, name: &str) -> Result<(), String> {
    checked(name)?;
    std::fs::remove_file(folder(project_dir).join(name)).map_err(|e| format!("Can't delete {name}: {e}"))
}

impl Shell {
    /// Keeps the editor pointed at the current project (switching projects closes its document).
    fn sync_documents(&mut self) {
        if self.documents.project != self.current {
            self.flush_document();
            self.documents = DocState { project: self.current, ..DocState::default() };
        }
    }

    /// Saves the open document if it has unsaved changes.
    pub(crate) fn flush_document(&mut self) {
        let Some(pi) = self.documents.project else { return };
        let (Some(name), Some(p)) = (self.documents.selected.clone(), self.projects.get(pi)) else { return };
        if self.documents.text == self.documents.saved {
            self.documents.dirty_since = None;
            return;
        }
        let dir = p.dir().to_path_buf();
        match write(&dir, &name, &self.documents.text) {
            Ok(()) => {
                self.documents.saved = self.documents.text.clone();
                self.documents.dirty_since = None;
                self.documents.seen = modified(&dir, &name);
                self.documents.error = None;
            }
            Err(e) => self.documents.error = Some(e),
        }
    }

    /// Opens a document in the editor, saving the one that was open first.
    pub(crate) fn open_document(&mut self, name: &str) {
        self.sync_documents();
        self.flush_document();
        let Some(pi) = self.documents.project else { return };
        let Some(dir) = self.projects.get(pi).map(|p| p.dir().to_path_buf()) else { return };
        match read(&dir, name) {
            Ok(text) => {
                self.documents.selected = Some(name.to_string());
                self.documents.saved = text.clone();
                self.documents.text = text;
                self.documents.seen = modified(&dir, name);
                self.documents.dirty_since = None;
                self.documents.confirm_delete = false;
                self.documents.rename_to = None;
                self.documents.error = None;
            }
            Err(e) => self.documents.error = Some(e),
        }
    }
}

/// Draws the Documents tab for project `pi`: the file list on the left, the editor on the right.
pub(crate) fn show(shell: &mut Shell, ui: &mut Ui, t: &Tokens) {
    shell.sync_documents();
    let Some(pi) = shell.active_project() else { return };
    let dir = shell.projects[pi].dir().to_path_buf();
    let names = list(&dir);
    let mut open: Option<String> = None;
    let mut created: Option<Result<String, String>> = None;
    let mut deleted = false;
    let mut renamed: Option<Result<String, String>> = None;

    egui::Panel::left(ui.scope_id().with("docs_list")).resizable(false).exact_size(230.0).show(ui, |ui| {
        ui.add_space(6.0);
        ui.label(RichText::new("Documents").strong().color(t.text));
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut shell.documents.new_name).hint_text("New document name").desired_width(140.0));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("New").clicked() || enter {
                created = Some(create(&dir, &shell.documents.new_name));
                if matches!(created, Some(Ok(_))) {
                    shell.documents.new_name.clear();
                }
            }
        });
        ui.separator();
        if names.is_empty() {
            ui.label(RichText::new("No documents yet. Type a name above and press New.").color(t.text_dim).size(12.0));
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for n in &names {
                let selected = shell.documents.selected.as_deref() == Some(n.as_str());
                let label = n.strip_suffix(".md").unwrap_or(n);
                if ui.selectable_label(selected, label).clicked() {
                    open = Some(n.clone());
                }
            }
        });
        if let Some(name) = shell.documents.selected.clone() {
            ui.separator();
            if let Some(to) = shell.documents.rename_to.clone() {
                let mut to = to;
                let r = ui.add(egui::TextEdit::singleline(&mut to).desired_width(150.0));
                shell.documents.rename_to = Some(to.clone());
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if (ui.button("Rename").clicked() || enter) && !to.trim().is_empty() {
                        renamed = Some(rename(&dir, &name, &to));
                    }
                    if ui.button("Cancel").clicked() {
                        shell.documents.rename_to = None;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        shell.documents.rename_to = Some(name.strip_suffix(".md").unwrap_or(&name).to_string());
                    }
                    if shell.documents.confirm_delete {
                        if ui.button(RichText::new("Confirm delete").color(t.danger)).clicked() {
                            match delete(&dir, &name) {
                                Ok(()) => deleted = true,
                                Err(e) => shell.documents.error = Some(e),
                            }
                        }
                        if ui.button("Keep").clicked() {
                            shell.documents.confirm_delete = false;
                        }
                    } else if ui.button("Delete").clicked() {
                        shell.documents.confirm_delete = true;
                    }
                });
            }
        }
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.grid_bg).inner_margin(egui::Margin::same(10))).show(ui, |ui| {
        if let Some(e) = shell.documents.error.clone() {
            ui.label(RichText::new(e).color(t.danger));
        }
        let Some(name) = shell.documents.selected.clone() else {
            ui.label(RichText::new("Choose a document on the left, or make a new one.").color(t.text_dim));
            return;
        };
        let edit_id = ui.scope_id().with(("doc_text", &name));
        shell.documents.edit_id = Some(edit_id);
        let title = ui.label(RichText::new(name.strip_suffix(".md").unwrap_or(&name)).strong().color(t.text));
        ui.separator();
        if shell.documents.preview {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                crate::markdown::show(ui, t, &shell.documents.text);
            });
        } else {
            // The live preview styles the text as it is typed; the cursor's line keeps its markers.
            let cursor_line = egui::TextEdit::load_state(ui.ctx(), edit_id)
                .and_then(|st| st.cursor.char_range())
                .map(|r| line_of(&shell.documents.text, usize::from(r.primary.index)));
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let mut job = crate::markdown::live_job(buf.as_str(), cursor_line, t);
                job.wrap.max_width = wrap_width;
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let r = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::TextEdit::multiline(&mut shell.documents.text)
                        .id(edit_id)
                        .layouter(&mut layouter)
                        .desired_width(f32::INFINITY)
                        .desired_rows(40)
                        .frame(egui::Frame::NONE)
                        .lock_focus(true)
                        .show(ui)
                        .response
                        .response
                        .labelled_by(title.id)
                })
                .inner;
            if r.changed() {
                shell.documents.dirty_since.get_or_insert(shell.clock_ms);
            }
        }
        let now = shell.clock_ms;
        if let Some(since) = shell.documents.dirty_since {
            let waited = (now - since).max(0.0);
            if waited >= SAVE_DELAY_MS {
                shell.flush_document();
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis((SAVE_DELAY_MS - waited) as u64));
            }
        }
        // Another device may have changed the file: take its text when nothing is unsaved here.
        if shell.documents.dirty_since.is_none()
            && let Some(seen) = shell.documents.seen
            && let Some(changed) = modified(&dir, &name)
            && changed != seen
        {
            open = Some(name.clone());
        }
    });

    if let Some(Ok(name)) = &created {
        open = Some(name.clone());
    } else if let Some(Err(e)) = created {
        shell.documents.error = Some(e);
    }
    if let Some(Err(e)) = renamed.take() {
        shell.documents.error = Some(e);
    } else if let Some(Ok(to)) = renamed {
        shell.documents.rename_to = None;
        shell.documents.selected = None;
        open = Some(to);
    }
    if deleted {
        shell.documents.selected = None;
        shell.documents.text.clear();
        shell.documents.saved.clear();
        shell.documents.confirm_delete = false;
    }
    if let Some(name) = open {
        shell.open_document(&name);
    }
}

/// Makes a new document in project `pi` named "Untitled" (or "Untitled 2", …) and opens it.
pub(crate) fn new_untitled(shell: &mut Shell, pi: usize) {
    let Some(dir) = shell.projects.get(pi).map(|p| p.dir().to_path_buf()) else { return };
    let free = (1..1000)
        .map(|n| if n == 1 { "Untitled".to_string() } else { format!("Untitled {n}") })
        .find(|typed| !folder(&dir).join(format!("{typed}.md")).exists());
    match free {
        Some(typed) => match create(&dir, &typed) {
            Ok(name) => shell.open_document(&name),
            Err(e) => shell.documents.error = Some(e),
        },
        None => shell.documents.error = Some("There are too many untitled documents; rename or delete some.".to_string()),
    }
}

/// The line (0-based) that character `char_index` is on.
fn line_of(text: &str, char_index: usize) -> usize {
    text.chars().take(char_index).filter(|c| *c == '\n').count()
}

/// The editor's selection in characters, (start, end). With no editor showing, the end of the text.
pub(crate) fn selection(ctx: &egui::Context, shell: &Shell) -> (usize, usize) {
    let len = shell.documents.text.chars().count();
    let range = shell.documents.edit_id.and_then(|id| egui::TextEdit::load_state(ctx, id)).and_then(|st| st.cursor.char_range());
    match range {
        Some(range) => {
            let (a, b) = (usize::from(range.primary.index), usize::from(range.secondary.index));
            (a.min(b), a.max(b))
        }
        None => (len, len),
    }
}

/// Applies a Format group command to the open document at the editor's selection, and keeps the
/// selection over the change.
pub(crate) fn apply(shell: &mut Shell, ctx: &egui::Context, edit: impl FnOnce(&str, usize, usize) -> crate::markdown::Edit) {
    use egui::text::CCursor;
    let Some(id) = shell.documents.edit_id else { return };
    let (start, end) = selection(ctx, shell);
    let result = edit(&shell.documents.text, start, end);
    shell.documents.text = result.text;
    shell.documents.dirty_since.get_or_insert(shell.clock_ms);
    let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(CCursor::new(result.start), CCursor::new(result.end))));
    st.store(ctx, id);
}
