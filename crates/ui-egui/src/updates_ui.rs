//! Updates, as RevuCraft does them: one quiet check when the app starts (a dialog opens only when a
//! newer release exists), and Help ▸ Check for Updates… which always reports the outcome. Update
//! opens the release page in the browser and closes PlanCraft once its open work is saved. Nothing
//! is installed here: the download is the user's, from the browser.

use std::sync::mpsc::{Receiver, TryRecvError};

use craft_update::Release;
#[cfg(not(target_arch = "wasm32"))]
use craft_update::{Http, check_for_update};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::channel;

use crate::SheetApp;
#[cfg(not(target_arch = "wasm32"))]
use crate::UPDATE_APP;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The answer to one check: a newer release, nothing newer, or why the check failed.
type Answer = Result<Option<Release>, String>;

#[derive(Default)]
pub(crate) struct Runtime {
    /// A check in progress; its answer comes on this channel.
    check: Option<Receiver<Answer>>,
    /// The check at start was asked for and has not run yet.
    start_pending: bool,
    /// The check at start has been asked for (once per run).
    started: bool,
    /// Help ▸ Check for Updates… was chosen and has not run yet.
    manual_pending: bool,
    /// The running check reports every outcome (Help), not only a newer release (start).
    manual: bool,
    /// A newer release to offer in the dialog.
    offer: Option<Release>,
    /// What a check Help asked for found, when it isn't a newer release.
    report: Option<String>,
    /// Update was chosen: the browser has the release page; the app closes once its work is saved.
    quit_for_update: bool,
}

/// Ask for a check; the next frame's `tick` starts it. The check at start runs once; Help's always runs.
pub(crate) fn start(app: &mut SheetApp, manual: bool) {
    let rt = &mut app.updates_rt;
    if manual {
        rt.manual_pending = true;
    } else if !rt.started {
        rt.started = true;
        rt.start_pending = true;
    }
}

pub(crate) fn tick(app: &mut SheetApp, ctx: &egui::Context) {
    let asked = {
        let rt = &mut app.updates_rt;
        if std::mem::take(&mut rt.manual_pending) {
            rt.manual = true;
            true
        } else if std::mem::take(&mut rt.start_pending) {
            rt.manual = false;
            true
        } else {
            false
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    if asked {
        spawn(app, ctx);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = asked;
    let rt = &mut app.updates_rt;
    if let Some(rx) = rt.check.take() {
        match rx.try_recv() {
            Ok(answer) => finish(rt, answer),
            Err(TryRecvError::Empty) => {
                rt.check = Some(rx);
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(TryRecvError::Disconnected) => finish(rt, Err("the update check stopped unexpectedly".into())),
        }
    }
}

/// Records a finished check: a newer release opens the dialog; nothing else is shown unless Help asked.
fn finish(rt: &mut Runtime, answer: Answer) {
    match answer {
        Ok(Some(release)) => rt.offer = Some(release),
        Ok(None) if rt.manual => rt.report = Some(format!("PlanCraft {VERSION} is up to date.")),
        Err(e) if rt.manual => rt.report = Some(format!("Couldn't check for updates: {e}")),
        _ => {}
    }
    rt.manual = false;
}

/// A fixed id for one of the update dialogs (one of each, so a salt is not needed).
#[allow(deprecated)]
fn modal_id(name: &'static str) -> egui::Id {
    egui::Id::new(name)
}

/// Whether Update was chosen and PlanCraft should close now (once its open work is saved). Taken
/// by the app each frame, so it is reported once.
pub fn take_update_quit(app: &mut SheetApp) -> bool {
    std::mem::take(&mut app.updates_rt.quit_for_update)
}

pub(crate) fn show(app: &mut SheetApp, ctx: &egui::Context) {
    // Only a spreadsheet with unsaved changes holds the update back: Update is greyed until it's saved.
    let clean = !app.session.documents().iter().any(|d| d.is_dirty());
    let rt = &mut app.updates_rt;
    if let Some(release) = rt.offer.as_ref() {
        let version = release.tag.trim_start_matches(['v', 'V']).to_string();
        let url = release.html_url.clone();
        let mut update = false;
        let mut later = false;
        let modal = egui::Modal::new(modal_id("pc_update")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(egui::RichText::new("Check for updates").strong().size(16.0));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(format!("PlanCraft {version} is available.")).strong());
            ui.label(format!(
                "You have version {VERSION}. Update opens the release page in your browser and closes PlanCraft, so the installer can replace it."
            ));
            if !clean {
                ui.add_space(4.0);
                ui.label("Save your open spreadsheet first.");
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(clean, egui::Button::new("Update")).clicked() {
                    update = true;
                }
                if ui.button("Later").clicked() {
                    later = true;
                }
            });
        });
        if modal.should_close() {
            later = true;
        }
        if update {
            ctx.open_url(egui::OpenUrl::new_tab(&url));
            rt.quit_for_update = true;
            rt.offer = None;
        } else if later {
            rt.offer = None;
        }
    } else if let Some(message) = rt.report.clone() {
        let mut close = false;
        let modal = egui::Modal::new(modal_id("pc_update_report")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(egui::RichText::new("Check for updates").strong().size(16.0));
            ui.add_space(8.0);
            ui.label(message);
            ui.add_space(12.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });
        if close || modal.should_close() {
            rt.report = None;
        }
    }
}

/// Starts a check on its own thread, unless one is running already.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn(app: &mut SheetApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    if rt.check.is_some() {
        return;
    }
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let answer = Http::new(&format!("PlanCraft/{VERSION}")).and_then(|http| check_for_update(&UPDATE_APP, &http, VERSION));
        // The receiver may be gone if the app is quitting; nothing to report to then.
        let _ = tx.send(answer);
        repaint.request_repaint();
    });
    rt.check = Some(rx);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// Live: asks GitHub for PlanCraft's releases as if this were 0.0.1 (`cargo test -p gridcraft-ui-egui -- --ignored`).
    #[test]
    #[ignore = "needs network access"]
    fn github_offers_a_newer_release_to_an_old_build() {
        let http = Http::new("PlanCraft/0.0.1").unwrap();
        let answer = check_for_update(&UPDATE_APP, &http, "0.0.1");
        let release = answer.unwrap().expect("a newer release exists");
        assert!(release.html_url.starts_with("https://github.com/Azurehawke/plancraft/"), "{release:?}");
    }
}
