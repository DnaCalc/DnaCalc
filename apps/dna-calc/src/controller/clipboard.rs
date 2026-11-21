//! Clipboard, paste preview and fill handling for the workbook session.
use super::*;

impl Controller {
    pub fn copy(&self) {
        self.copy_pending.update(|n| *n += 1);
        self.take(false, self.copy_epoch.get_value());
    }

    /// Cuts the selected range (Ctrl+X): the next paste moves it there.
    pub fn cut(&self) {
        self.copy_pending.update(|n| *n += 1);
        self.take(true, self.copy_epoch.get_value());
    }

    pub(super) fn take(&self, cut: bool, epoch: u64) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue_admitted(move |accepted| {
                if accepted {
                    controller.take(cut, epoch);
                } else {
                    controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                }
            });
            return;
        }
        if epoch != self.copy_epoch.get_value() {
            return;
        }
        if self.areas.with_untracked(|areas| areas.len() > 1) {
            self.copy_pending.update(|n| *n = n.saturating_sub(1));
            self.say("Select one rectangle for this command", true);
            return;
        }
        if let Some(area) = self.selected_area_untracked() {
            let controller = *self;
            let generation = self.generation.get_untracked();
            self.pending_actions.update(|n| *n += 1);
            let keep = self.copied.get_untracked().map(|c| c.token);
            self.call(
                Request::Capture {
                    area: Area::of(area),
                    keep,
                },
                None,
                move |reply, _| {
                    controller
                        .pending_actions
                        .update(|n| *n = n.saturating_sub(1));
                    if generation != controller.generation.get_untracked()
                        || epoch != controller.copy_epoch.get_value()
                    {
                        return;
                    }
                    match reply {
                        Ok(Reply::Clipboard(payload)) => {
                            controller.clipboard.set(Some(area));
                            controller.cut.set(cut);
                            controller.copied.set(Some(payload.clone().into()));
                            controller.say(
                                format!(
                                    "{} {} — snapshot captured; select where to paste",
                                    if cut { "Cut" } else { "Copied" },
                                    controller.area_label(area)
                                ),
                                false,
                            );
                            wasm_bindgen_futures::spawn_local(async move {
                                if crate::clipboard::write_system(&payload).await.is_err() {
                                    if generation != controller.generation.get_untracked()
                                        || epoch != controller.copy_epoch.get_value()
                                    {
                                        return;
                                    }
                                    controller.clipboard_fallback.set(Some(true));
                                    controller.say("Browser clipboard access was denied. Copy table text in the dialog, or use Paste copied cells.",true);
                                }
                                if generation == controller.generation.get_untracked()
                                    && epoch == controller.copy_epoch.get_value()
                                {
                                    controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                                }
                            });
                        }
                        Err(reason) => {
                            controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                            controller.say(reason, true);
                        }
                        _ => {
                            controller.copy_pending.update(|n| *n = n.saturating_sub(1));
                            controller.say("The clipboard did not answer", true);
                        }
                    }
                },
            );
        } else {
            self.copy_pending.update(|n| *n = n.saturating_sub(1));
        }
    }

    /// A native copy outside the grid supersedes an in-flight system write, while retaining the
    /// explicit copied-cell snapshot. It also cancels a pending cut move.
    pub fn system_clipboard_changed(&self) {
        self.copy_epoch.update_value(|n| *n += 1);
        self.copy_pending.set(0);
        self.clipboard.set(None);
        self.cut.set(false);
    }

    pub fn copy_requested(&self) -> bool {
        self.copy_pending.get() > 0
    }

    // Construct this small adapter on demand. Keeping it out of the copied Controller avoids
    // multiplying its size through every retained view closure on the wasm stack.
    pub fn paste_transpose(&self) -> ModelField<bool> {
        Field::new(
            self.model,
            |m| &m.runtime.paste_transpose,
            |m| &mut m.runtime.paste_transpose,
        )
    }

    pub(super) fn chosen_paste_mode(&self) -> wire::PasteMode {
        match self.paste_mode.get_untracked().as_str() {
            "values" => wire::PasteMode::Values,
            "formulas" => wire::PasteMode::Formulas,
            "formats" => wire::PasteMode::Formats,
            _ => wire::PasteMode::All,
        }
    }

    /// Move or copy a rectangle through the same captured, revision-bound paste as the clipboard.
    /// A drag owns no system clipboard data and leaves an existing copied snapshot available.
    pub fn paste(&self) {
        let controller = *self;
        self.after_input(move || {
            if let Some(payload) = controller.copied.get_untracked() {
                controller.prepare_paste(
                    wire::PasteSource::Internal {
                        token: payload.token,
                        cut: controller.cut.get_untracked(),
                    },
                    false,
                );
            } else {
                controller.say("Nothing is copied to paste", true);
            }
        });
    }

    pub fn paste_system(&self) {
        let controller = *self;
        self.after_input(move || {
            if controller.areas.with_untracked(|areas| areas.len() != 1) {
                controller.say("Select one rectangle for this command", true);
                return;
            }
            let Some(to)=controller.selected_area_untracked() else {return;};
            let generation=controller.generation.get_untracked();
            let mode=controller.chosen_paste_mode();
            let transpose=controller.model.with_untracked(|m| m.runtime.paste_transpose);
            controller.pending_actions.update(|n|*n+=1);
            controller.pending_changes.update(|n|*n+=1);
            wasm_bindgen_futures::spawn_local(async move {
                let result=crate::clipboard::read_system().await;
                controller.pending_actions.update(|n|*n=n.saturating_sub(1));
                controller.pending_changes.update(|n|*n=n.saturating_sub(1));
                if generation != controller.generation.get_untracked() {
                    return;
                }
                match result {
                    Ok((text, html)) => match controller.clipboard_source(&text, &html) {
                        Ok(source) => {
                            controller.prepare_paste_at_mode(source, to, false, mode, transpose, false)
                        }
                        Err(reason) => controller.say(reason, true),
                    },
                    Err(_) => {
                        controller.clipboard_fallback.set(Some(false));
                        controller.say(
                            "Browser clipboard access was denied. Paste table text in the dialog, or use Paste copied cells.",
                            true,
                        );
                    }
                }
            });
        });
    }

    pub(super) fn clipboard_source(
        &self,
        text: &str,
        html: &str,
    ) -> Result<wire::PasteSource, String> {
        if text.is_empty() && html.is_empty() {
            return Err("The clipboard contains no text table".to_owned());
        }
        let (rows, token) = if !html.is_empty() {
            match crate::clipboard::decode_html(html) {
                Err(reason) if reason == "The clipboard HTML has no table" && !text.is_empty() => {
                    (crate::clipboard::decode_tsv(text)?, None)
                }
                other => other?,
            }
        } else {
            (crate::clipboard::decode_tsv(text)?, None)
        };
        let internal = self.copied.get_untracked().filter(|p| {
            !p.session.is_empty()
                && Some((p.session.clone(), p.token)) == token
                && (text.is_empty()
                    || crate::clipboard::decode_tsv(&p.text).ok()
                        == crate::clipboard::decode_tsv(text).ok())
        });
        Ok(internal.map_or(wire::PasteSource::External(rows), |p| {
            wire::PasteSource::Internal {
                token: p.token,
                cut: self.cut.get_untracked(),
            }
        }))
    }

    /// Own the native event's data before queueing; replaying Ctrl+V supplies no system data.
    pub fn paste_external(&self, text: String, html: String) {
        let controller = *self;
        if self.copy_pending.get_untracked() > 0 {
            self.after_input(move || controller.paste());
            return;
        }
        self.after_input(move || match controller.clipboard_source(&text, &html) {
            Ok(source) => controller.prepare_paste(source, false),
            Err(reason) => controller.say(reason, true),
        });
    }

    pub fn prepare_paste(&self, source: wire::PasteSource, force_preview: bool) {
        if self.areas.with_untracked(|a| a.len() > 1) {
            self.say("Select one rectangle for this command", true);
            return;
        }
        let Some(to) = self.selected_area_untracked() else {
            return;
        };
        self.prepare_paste_at(source, to, force_preview);
    }

    pub fn prepare_paste_at(&self, source: wire::PasteSource, to: GridArea, force_preview: bool) {
        self.prepare_paste_at_mode(
            source,
            to,
            force_preview,
            self.chosen_paste_mode(),
            self.model.with_untracked(|m| m.runtime.paste_transpose),
            false,
        );
    }

    pub(super) fn prepare_paste_at_mode(
        &self,
        source: wire::PasteSource,
        to: GridArea,
        force_preview: bool,
        chosen_mode: wire::PasteMode,
        transpose: bool,
        drag: bool,
    ) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let selection_epoch = self.selection_epoch.get_value();
        let cut = matches!(source, wire::PasteSource::Internal { cut: true, .. });
        let mode = if matches!(source, wire::PasteSource::Fill { .. }) {
            wire::PasteMode::All
        } else {
            if matches!(source, wire::PasteSource::FlashFill) {
                wire::PasteMode::Values
            } else {
                chosen_mode
            }
        };
        self.pending_actions.update(|n| *n += 1);
        self.pending_changes.update(|n| *n += 1);
        self.call(
            Request::PreparePaste {
                transpose: transpose
                    && !matches!(
                        source,
                        wire::PasteSource::Fill { .. } | wire::PasteSource::FlashFill
                    ),
                source,
                to: Area::of(to),
                mode,
            },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                controller
                    .pending_changes
                    .update(|n| *n = n.saturating_sub(1));
                if generation != controller.generation.get_untracked() {
                    return;
                }
                if drag
                    && (selection_epoch != controller.selection_epoch.get_value()
                        || controller.editing.get_untracked().is_some())
                {
                    controller.say("The selection changed during the drag; drag again", true);
                    return;
                }
                match reply {
                    Ok(Reply::PastePreview(preview)) => {
                        controller.paste_cut.set_value(cut && !drag);
                        controller.model.update(|m| m.runtime.paste_drag = drag);
                        controller.paste_preview.set(Some(preview.clone()));
                        if preview.replaces == 0 && !force_preview {
                            controller.apply_paste(preview.id);
                        }
                    }
                    Err(reason) => controller.say(reason, true),
                    _ => controller.say("The paste preview did not answer", true),
                }
            },
        );
    }

    pub fn apply_paste(&self, id: u64) {
        let select_target = self.model.with_untracked(|m| m.runtime.paste_drag);
        self.paste_preview.set(None);
        let controller = *self;
        let cut = self.paste_cut.get_value();
        self.dispatch(
            WorkbookCommand::ApplyPaste { id, select_target },
            move |outcome| {
                if outcome.accepted && cut {
                    controller.clipboard.set(None);
                    controller.copied.set(None);
                    controller.cut.set(false);
                }
                controller.focus_active_skin();
            },
        );
    }

    /// Infer basic text extraction from examples, then use the ordinary paste preview.
    pub fn flash_fill(&self) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say("Finish or cancel the current entry before Flash Fill", true);
                return;
            }
            if controller.areas.with_untracked(|areas| areas.len() != 1) {
                controller.say("Select one output column for Flash Fill", true);
                return;
            }
            if let Some(to) = controller.selected_area_untracked() {
                controller.prepare_paste_at(wire::PasteSource::FlashFill, to, true);
            }
        });
    }

    /// Keyboard equivalent of the handle: the end cell is resolved by the workbook catalogue.
    pub fn fill_through(&self, text: String) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before filling cells",
                    true,
                );
                return;
            }
            if controller.areas.with_untracked(|a| a.len() != 1) {
                controller.say("Select one rectangle for this command", true);
                return;
            }
            let Some(from) = controller.selected_area_untracked() else {
                return;
            };
            let generation = controller.generation.get_untracked();
            let series = controller.fill_series.get_untracked() == "series";
            controller.pending_actions.update(|n| *n += 1);
            controller.call(
                Request::ResolveReference {
                    anchor: Key::of(from.first()),
                    text,
                },
                None,
                move |reply, _| {
                    controller
                        .pending_actions
                        .update(|n| *n = n.saturating_sub(1));
                    if generation != controller.generation.get_untracked() {
                        return;
                    }
                    match reply {
                        Ok(Reply::ResolvedReference(Ok(area))) => {
                            let Some(area) = area
                                .area()
                                .filter(|a| a.sheet() == from.sheet() && a.cell_count() == 1)
                            else {
                                controller
                                    .say("Fill through needs one end cell on this worksheet", true);
                                return;
                            };
                            let at = area.first();
                            if !(from.first().row()..=from.last().row()).contains(&at.row())
                                && !(from.first().column()..=from.last().column())
                                    .contains(&at.column())
                            {
                                controller
                                    .say("Choose a fill end in the selected rows or columns", true);
                                return;
                            }
                            if let Some((to, down)) =
                                crate::clipboard::fill_extension(from, at.row(), at.column())
                            {
                                controller.prepare_paste_at(
                                    wire::PasteSource::Fill {
                                        from: Area::of(from),
                                        down,
                                        series,
                                    },
                                    to,
                                    true,
                                );
                            } else {
                                controller
                                    .say("Choose an end cell outside the seed rectangle", true);
                            }
                        }
                        Ok(Reply::ResolvedReference(Err(reason))) | Err(reason) => {
                            controller.say(reason, true)
                        }
                        _ => controller.say("The fill end did not resolve", true),
                    }
                },
            );
        });
    }

    /// Fills the selected range down from its first row (Ctrl+D) or right from its first column
    /// (Ctrl+R): a calc-host copy of that row or column over the rest.
    pub fn fill(&self, down: bool) {
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.fill(down));
            return;
        }
        if self.areas.with_untracked(|areas| areas.len() > 1) {
            self.say("Select one rectangle for this command", true);
            return;
        }
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        match fill_parts(area, down) {
            Some((from, to)) => self.dispatch(
                WorkbookCommand::Copy {
                    from: area_target(from),
                    to: area_target(to),
                    cut: false,
                },
                |_| {},
            ),
            None => self.say(
                if down {
                    "Fill down needs a range of more than one row"
                } else {
                    "Fill right needs a range of more than one column"
                },
                true,
            ),
        }
    }
}

fn fill_parts(area: GridArea, down: bool) -> Option<(GridArea, GridArea)> {
    let (first, last) = (area.first(), area.last());
    let sheet = area.sheet();
    if down {
        if first.row() == last.row() {
            return None;
        }
        let source =
            GridArea::new(first, CellKey::new(sheet, first.row(), last.column()).ok()?).ok()?;
        let target = GridArea::new(
            CellKey::new(sheet, first.row() + 1, first.column()).ok()?,
            last,
        )
        .ok()?;
        Some((source, target))
    } else {
        if first.column() == last.column() {
            return None;
        }
        let source =
            GridArea::new(first, CellKey::new(sheet, last.row(), first.column()).ok()?).ok()?;
        let target = GridArea::new(
            CellKey::new(sheet, first.row(), first.column() + 1).ok()?,
            last,
        )
        .ok()?;
        Some((source, target))
    }
}
