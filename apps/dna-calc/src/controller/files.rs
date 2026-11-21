//! Open and save operations, including existing workbook window metadata.
use super::*;

impl Controller {
    pub fn new_workbook(&self) {
        if self.replacing.get_untracked() {
            return;
        }
        self.replacing.set(true);
        self.deferred.update_value(VecDeque::clear);
        self.deferred_version.update(|v| *v += 1);
        self.pending_actions.update(|n| *n += 1);
        let controller = *self;
        self.send_command(Request::New, None, move |reply, _| {
            controller
                .pending_actions
                .update(|n| *n = n.saturating_sub(1));
            controller.replacing.set(false);
            match reply {
                Ok(Reply::Done { outcome, .. }) => {
                    controller.installed(outcome.summary, "New workbook".to_owned())
                }
                Err(error) => controller.say(format!("New workbook failed: {error}"), true),
                _ => controller.say("New workbook did not answer", true),
            }
        });
    }

    /// Opens a package's bytes under `name`; the current workbook stays when it is refused.
    /// `opened` hears whether it opened.
    pub fn open_native(&self) {
        self.open_kind("", Vec::new(), false, false, |_| {});
    }
    pub fn open(&self, name: &str, bytes: Vec<u8>, opened: impl FnOnce(bool) + 'static) {
        self.open_mode(name, bytes, false, opened);
    }
    pub fn open_mode(
        &self,
        name: &str,
        bytes: Vec<u8>,
        edit_template: bool,
        opened: impl FnOnce(bool) + 'static,
    ) {
        self.open_kind(name, bytes, edit_template, false, opened);
    }
    pub fn open_template(&self, name: &str, bytes: Vec<u8>, opened: impl FnOnce(bool) + 'static) {
        self.open_kind(name, bytes, false, true, opened);
    }
    pub fn edit_personal_template(&self, name: String, bytes: Vec<u8>) {
        let controller = *self;
        let owned = name.clone();
        self.open_mode(&name, bytes, true, move |accepted| {
            if accepted {
                controller.personal_template.set_value(Some(owned));
            }
        });
    }
    pub(super) fn open_kind(
        &self,
        name: &str,
        bytes: Vec<u8>,
        edit_template: bool,
        template_only: bool,
        opened: impl FnOnce(bool) + 'static,
    ) {
        if self.replacing.get_untracked() {
            return;
        }
        self.replacing.set(true);
        self.deferred.update_value(VecDeque::clear);
        self.deferred_version.update(|v| *v += 1);
        self.pending_actions.update(|n| *n += 1);
        let controller = *self;
        let size = bytes.len();
        let name = name.to_owned();
        let native = name.is_empty() && bytes.is_empty();
        let request = if native {
            Request::OpenNative
        } else if template_only {
            Request::OpenTemplate { name: name.clone() }
        } else if edit_template {
            Request::OpenForEditing { name: name.clone() }
        } else {
            Request::Open { name: name.clone() }
        };
        self.send_command(
            request,
            Some(bytes),
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                controller.replacing.set(false);
                match reply {
                    Ok(Reply::Done {
                        outcome,
                        opened: Some((elapsed, calculated)),
                        ..
                    }) => {
                        controller
                            .timings
                            .update(|timings| timings.open_ms = Some(elapsed));
                        let name = &outcome.summary.name;
                        let message = if native {
                            format!("Opened {name} in {elapsed:.0} ms")
                        } else { match calculated {
                            Some(calculated) => format!(
                                "Opened {name}: {size} bytes in {elapsed:.0} ms: read in {:.0} ms, calculated in {calculated:.0} ms",
                                elapsed - calculated
                            ),
                            None if outcome.summary.manual => format!(
                                "Opened {name}: {size} bytes in {elapsed:.0} ms; the file calculates manually, \
                                so its formulas show the values it stored until Calculate"
                            ),
                            None => format!("Opened {name}: {size} bytes in {elapsed:.0} ms; calculation continues in the worker"),
                        } };
                        let message = match outcome.summary.open_repairs.as_slice() {
                            [] => message,
                            repairs => format!(
                                "{message}. Excel would repair {} problem{} in this file ({}); \
                                 a save writes the repaired workbook",
                                repairs.len(),
                                if repairs.len() == 1 { "" } else { "s" },
                                repairs.join("; ")
                            ),
                        };
                        controller.installed(outcome.summary, message);
                        opened(true);
                    }
                    Ok(Reply::OpenCancelled) => { opened(false); }
                    Ok(Reply::Done { outcome, .. }) => {
                        controller.say(
                            format!("{name} did not open: {}", outcome.error.unwrap_or_default()),
                            true,
                        );
                        opened(false);
                    }
                    Ok(_) | Err(_) => {
                        controller.say(format!("{name} did not open"), true);
                        opened(false);
                    }
                }
            },
        );
    }

    /// Validate personal-template package content without changing the current workbook.
    pub fn inspect_template(
        &self,
        bytes: Vec<u8>,
        done: impl FnOnce(Result<wire::FileFormat, String>) + 'static,
    ) {
        self.call(Request::InspectTemplate, Some(bytes), move |reply, _| {
            done(match reply {
                Ok(Reply::TemplateFormat(f)) => Ok(f),
                Err(e) => Err(e),
                _ => Err("Template inspection did not answer".into()),
            })
        });
    }
    pub fn insert_template(
        &self,
        bytes: Vec<u8>,
        name: Option<String>,
        done: impl FnOnce(bool) + 'static,
    ) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.insert_template(bytes, name, done)
                } else {
                    done(false)
                }
            });
            return;
        }
        if self.editing.get_untracked().is_some() {
            self.say(
                "Enter or cancel the current edit before inserting worksheets",
                true,
            );
            done(false);
            return;
        }
        let controller = *self;
        let before: Vec<_> = self.sheets().into_iter().map(|s| s.id).collect();
        let revision = self.summary.get_untracked().revision;
        let epoch = self.selection_epoch.get_value();
        self.pending_changes.update(|n| *n += 1);
        self.pending_actions.update(|n| *n += 1);
        self.send_command(
            Request::InsertTemplate { name, revision },
            Some(bytes),
            move |reply, _| {
                controller
                    .pending_changes
                    .update(|n| *n = n.saturating_sub(1));
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                match reply {
                    Ok(Reply::Done { outcome, .. }) => {
                        controller
                            .applied(&outcome, epoch == controller.selection_epoch.get_value());
                        let accepted = outcome.accepted;
                        controller.perform(move || {
                            if accepted {
                                if let Some(sheet) = controller
                                    .sheets()
                                    .into_iter()
                                    .find(|s| !before.contains(&s.id))
                                {
                                    controller.show_sheet(sheet.id);
                                }
                                controller.say("Inserted template worksheets", false);
                            }
                            done(accepted);
                        });
                    }
                    Err(e) => {
                        controller.say(e, true);
                        controller.perform(move || done(false));
                    }
                    _ => {
                        controller.say("Template insertion did not answer", true);
                        controller.perform(move || done(false));
                    }
                }
            },
        );
    }

    /// A worksheet command through the same worker/history path as cell commands.

    /// Saves the workbook as a package's bytes; `saved` gets its name and bytes, and the status
    /// line says how it went.
    pub fn save(&self, saved: impl FnOnce(String, Vec<u8>) + 'static) {
        self.save_choice(None, saved);
    }
    pub fn prepare_save_as(&self, ready: impl FnOnce() + 'static) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_none() {
                ready();
            } else {
                controller.commit_edit(move |accepted| {
                    if accepted {
                        ready();
                    }
                });
            }
        });
    }
    pub fn save_as(
        &self,
        name: String,
        format: wire::FileFormat,
        saved: impl FnOnce(String, Vec<u8>) + 'static,
    ) {
        // The modal opens only after its preceding cell entry has committed. A later
        // pending input needs a fresh user action, never an abandoned modal continuation.
        if self.waiting_input() || self.editing.get_untracked().is_some() {
            self.say("Finish the current cell entry before Save As", true);
            return;
        }
        if let Some(reason) = self.model.with_untracked(|m| m.views.pending_reason()) {
            self.say(reason, true);
            return;
        }
        self.save_package(Some((name, format)), saved);
    }
    pub(super) fn save_choice(
        &self,
        target: Option<(String, wire::FileFormat)>,
        saved: impl FnOnce(String, Vec<u8>) + 'static,
    ) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.save_choice(target, saved));
            return;
        }
        if let Some(reason) = self.model.with_untracked(|m| m.views.pending_reason()) {
            self.say(reason, true);
            return;
        }
        let controller = *self;
        if self.editing.get_untracked().is_some() {
            self.commit_edit(move |accepted| {
                if accepted {
                    controller.save_package(target, saved);
                } else {
                    controller.focus_editor();
                }
            });
        } else {
            self.save_package(target, saved);
        }
    }

    pub(super) fn save_package(
        &self,
        target: Option<(String, wire::FileFormat)>,
        saved: impl FnOnce(String, Vec<u8>) + 'static,
    ) {
        if self.save_download.with_value(|pending| pending.is_some()) {
            self.say("A workbook save is still being prepared", false);
            return;
        }
        let controller = *self;
        let generation = self.generation.get_untracked();
        let Some(active_sheet) = self.sheet.get_untracked() else {
            return;
        };
        let views = self
            .sheet_windows
            .get_value()
            .map_or_else(Vec::new, |snapshot| snapshot(*self, active_sheet));
        let views = Some(wire::SaveWindow {
            revision: self.summary.get_untracked().revision,
            active_sheet: wire::Sheet::of(active_sheet),
            sheets: views,
            sheet_view_modes: self.model.with_untracked(|m| {
                m.runtime
                    .tools
                    .page
                    .views
                    .iter()
                    .filter(|(sheet, _)| {
                        self.summary
                            .with_untracked(|s| s.sheets.iter().any(|(s, _)| s.id() == **sheet))
                    })
                    .map(|(&sheet, &mode)| (wire::Sheet::of(sheet), mode))
                    .collect()
            }),
        });
        // Applying the worker reply updates Summary before this callback runs. Keep the
        // download's original protection here, including saves which finish asynchronously.
        let unconfirmed =
            target.is_some() || self.summary.with_untracked(|s| s.changed || s.unsaved);
        let request = match target {
            Some((name, format)) => Request::SaveAs {
                name,
                format,
                views,
            },
            None => Request::SaveWithContext {
                calculate_before_save: None,
                views,
            },
        };
        self.saving.set(true);
        self.send_command(request, None, move |reply, bytes| {
            if controller.generation.get_untracked() == generation {
                controller.save_reply(reply, bytes, Box::new(move |name, bytes| {
                    if unconfirmed {
                        controller.download_unconfirmed.set(true);
                    }
                    let personal = controller
                        .personal_template
                        .get_value()
                        .filter(|old| old.eq_ignore_ascii_case(&name));
                    let updated = personal.map(|_| crate::files::update_personal_template(&name, &bytes));
                    saved(name, bytes);
                    if let Some(Err(error)) = updated {
                        controller.say(
                            format!(
                                "The download was prepared, but the personal template was not updated: {error}. \
                                Add the downloaded template again."
                            ),
                            true,
                        );
                    }
                }));
            }
        });
    }
    pub(super) fn save_reply(
        &self,
        reply: Result<Reply, String>,
        bytes: Option<Vec<u8>>,
        saved: SaveDownload,
    ) {
        self.saving
            .set(matches!(&reply, Ok(Reply::SavePending { .. })));
        let controller = *self;
        controller.revision.update(|n| *n += 1);
        match (reply, bytes) {
            (
                Ok(Reply::Done {
                    outcome,
                    saved: Some(name),
                    ..
                }),
                Some(bytes),
            )
            | (
                Ok(Reply::SaveImage {
                    outcome,
                    saved: name,
                    ..
                }),
                Some(bytes),
            ) => {
                if controller
                    .summary
                    .with_untracked(|s| s.changed || s.unsaved)
                {
                    controller.download_unconfirmed.set(true);
                }
                controller.summary.set(outcome.summary.into());
                controller.say(
                    format!("Download package prepared: {name}, {} bytes", bytes.len()),
                    false,
                );
                saved(name, bytes);
            }
            (Ok(Reply::SavePending { token, .. }), _) => {
                let workbook = controller.summary.with_untracked(|s| s.workbook);
                controller
                    .save_download
                    .set_value(Some((workbook, token, saved)));
                controller.say("Preparing workbook download", false);
            }
            (Ok(Reply::Done { outcome, .. }), _) => controller.say(
                format!(
                    "The workbook was not saved: {}",
                    outcome.error.unwrap_or_default()
                ),
                true,
            ),
            (Ok(_), _) => controller.say("The workbook was not saved", true),
            (Err(message), _) => {
                controller.say(format!("The workbook was not saved: {message}"), true)
            }
        }
    }
}
