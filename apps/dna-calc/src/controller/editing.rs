//! The formula-edit session shared by the formula bar and skin editors.
use super::*;
use dna_formula_ui::editor::{cycle_anchors, expects_reference, point_reference_span};

pub(super) fn field_selection(assist: FormulaAssist) -> (usize, usize) {
    assist.field.try_get_untracked().flatten().map_or_else(
        || {
            let at = assist.caret.get_untracked();
            (at, at)
        },
        |field| {
            (
                field.selection_start().ok().flatten().unwrap_or(0) as usize,
                field.selection_end().ok().flatten().unwrap_or(0) as usize,
            )
        },
    )
}

impl Controller {
    pub fn begin_edit(&self, origin: EditOrigin, seed: Option<String>) {
        if self.editing.get_untracked().is_some() {
            if self.editing.get_untracked() != Some(origin) {
                self.model.maybe_update(|model| model.edit.transfer(origin));
            }
            return;
        }
        if self.waiting_input() {
            let controller = *self;
            self.enqueue(move || controller.begin_edit(origin, seed));
            return;
        }
        if self.active_key_untracked().is_none() {
            self.say("Wait for the workbook before editing", true);
            return;
        }
        let target = self.active_key_untracked().expect("checked active cell");
        let (refusal, original) = untrack(|| {
            self.with_active_cell_mirror(target, |mirror| {
                let cell = mirror.cell(target);
                let refusal = cell
                    .and_then(wire::Cell::spill_child)
                    .map(wire::spill_child_message)
                    .or_else(|| mirror.geometry(target.sheet()).and_then(|g| g.edit_refusal.clone()));
                let original = cell.map(|cell| cell.input.clone()).unwrap_or_default();
                (refusal, original)
            })
        });
        if let Some(reason) = refusal {
            self.say(reason, true);
            return;
        }
        let areas = self.areas.get_untracked();
        self.model
            .update(|m| m.edit.begin(origin, target, original.clone(), seed, areas));
        self.assist.start_edit(original);
        self.offer_defined_names();
    }

    /// The cell the formula being edited belongs to: the edit's target, else the active cell.
    pub(super) fn edit_anchor(&self) -> Option<CellKey> {
        self.edit_target
            .get_untracked()
            .or_else(|| self.active_key_untracked())
    }

    /// Asks the workbook for the evaluation trace of the formula being edited; `done` gets it.
    pub fn trace_formula(&self, done: Box<dyn FnOnce(dna_calc_model::parts::Trace)>) {
        let Some(anchor) = self.edit_anchor() else {
            self.say("No cell is being edited", true);
            return;
        };
        self.ask(
            Request::TraceFormula {
                formula: self.assist.text.get_untracked(),
                anchor: wire::Key::of(anchor),
            },
            move |reply| {
                if let Reply::Trace(found) = reply {
                    done(found);
                }
            },
        );
    }

    /// Asks the workbook for its defined names (no sheet rows) and its registered functions, and
    /// offers them to the formula field's completion beside the catalogue's functions.
    pub(crate) fn offer_defined_names(&self) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let shown = self
            .summary
            .with_untracked(|s| s.sheets.iter().map(|(sheet, _)| (*sheet, 0)).collect());
        self.ask(
            wire::Request::Outline {
                shown,
                open: Vec::new(),
                reveal: None,
                query: String::new(),
            },
            move |reply| {
                if controller.generation.get_untracked() != generation {
                    return;
                }
                if let wire::Reply::Outline(found) = reply {
                    let names: Vec<String> = found
                        .names
                        .iter()
                        .map(|text| text.split(" = ").next().unwrap_or(text).trim().to_owned())
                        .collect();
                    // The registered functions complete beside the catalogue, with the argument names
                    // and help their providers gave.
                    let described: Vec<calc_fml::CallableDescriptor> = found
                        .functions
                        .iter()
                        .map(|f| calc_fml::CallableDescriptor {
                            name: f.name.clone(),
                            origin: f.origin.clone(),
                            category: f.category.clone(),
                            description: f.description.clone(),
                            parameters: f
                                .parameters
                                .iter()
                                .map(|(name, description)| calc_fml::CallableParameter {
                                    name: name.clone(),
                                    description: description.clone(),
                                })
                                .collect(),
                            volatile: f.volatile,
                            thread_safe: f.thread_safe,
                            asynchronous: f.asynchronous,
                        })
                        .collect();
                    let functions: Vec<String> = described.iter().map(|d| d.name.clone()).collect();
                    controller.assist.scope().try_update(|scope| {
                        scope.names = names;
                        scope.functions = functions;
                        scope.described = described;
                    });
                    controller.refresh_completions();
                }
            },
        );
        let controller = *self;
        let generation = self.generation.get_untracked();
        self.ask(Request::UserFunctions, move |reply| {
            if controller.generation.get_untracked() != generation {
                return;
            }
            if let wire::Reply::UserFunctions(functions) = reply {
                controller.assist.scope().try_update(|scope| {
                    if scope.functions != functions {
                        scope.functions = functions;
                    }
                });
                controller.refresh_completions();
            }
        });
    }

    /// The workbook's names and functions arrive from the worker after it opens. A completion
    /// list computed from the scope before they arrived would otherwise stay as it was until
    /// the next key (dc-0f1k.11.3: a formula typed just after Open missed the file's function).
    pub(super) fn refresh_completions(&self) {
        if self.editing.get_untracked().is_some() {
            self.assist.sync_from_field(false);
        }
    }

    pub fn activate_editor(&self, origin: EditOrigin, assist: FormulaAssist) {
        self.bind_editor_surface(origin, assist);
        self.begin_edit(origin, None);
        self.edit_field.set_value(assist);
    }

    /// Keep the draft and its target when its cell overlay is about to unmount.
    pub fn carry_edit_to_bar(&self) {
        if self.editing.get_untracked().is_some() {
            self.model
                .maybe_update(|model| model.edit.transfer(EditOrigin::Bar));
            self.edit_field.set_value(self.assist);
            #[cfg(target_arch = "wasm32")]
            {
                let controller = *self;
                let generation = self.generation.get_untracked();
                let session = self.model.with_untracked(|model| model.edit.id);
                leptos::prelude::set_timeout(
                    move || {
                        if controller.generation.try_get_untracked() != Some(generation)
                            || controller.editing.try_get_untracked().flatten()
                                != Some(EditOrigin::Bar)
                            || controller.model.with_untracked(|model| model.edit.id) != session
                        {
                            return;
                        }
                        controller.focus_mounted_editor(controller.assist);
                    },
                    std::time::Duration::ZERO,
                );
            }
        }
    }

    pub fn focus_editor(&self) {
        if self
            .captured_command_focus()
            .is_some_and(|ticket| !self.model.with_untracked(|m| m.focus.accepts(ticket)))
        {
            return;
        }
        let assist = self.edit_field.get_value();
        if let Some(field) = assist.field.try_get_untracked().flatten() {
            #[cfg(target_arch = "wasm32")]
            self.restoring_editor(|| {
                let _ = field.focus();
            });
            #[cfg(not(target_arch = "wasm32"))]
            let _ = field.focus();
        }
    }

    /// Both editors use this route after their shared completion and local undo handlers.
    pub fn editor_key(&self, ev: leptos::ev::KeyboardEvent, assist: FormulaAssist) {
        if assist.composing.get_untracked() || ev.is_composing() || ev.key_code() == 229 {
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let key = ev.key();
        if ctrl && !ev.shift_key() && !ev.alt_key() && key == "Enter" {
            ev.prevent_default();
            ev.stop_propagation();
            let controller = *self;
            self.commit_edit_as(true, move |accepted| {
                if accepted {
                    controller.focus_active_skin();
                } else {
                    controller.focus_editor();
                }
            });
            return;
        }
        let movement = match key.as_str() {
            "Enter" if !ctrl && !ev.alt_key() => Some(if ev.shift_key() {
                NavAction::Up
            } else {
                NavAction::CommitDown
            }),
            "Tab" if !ctrl && !ev.alt_key() => Some(if ev.shift_key() {
                NavAction::PrevColumn
            } else {
                NavAction::CommitRight
            }),
            _ => None,
        };
        if let Some(movement) = movement {
            ev.prevent_default();
            ev.stop_propagation();
            self.finish_edit(movement);
            return;
        }
        match key.as_str() {
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                self.cancel_edit();
                self.focus_active_skin();
            }
            "F2" if !ev.alt_key() => {
                ev.prevent_default();
                ev.stop_propagation();
                if ctrl {
                    match self.editing.get_untracked() {
                        Some(EditOrigin::Cell) => self.carry_edit_to_bar(),
                        _ => {
                            if document()
                                .query_selector("[data-dnacalc=\"grid\"]")
                                .ok()
                                .flatten()
                                .is_some()
                            {
                                self.editing.set(Some(EditOrigin::Cell));
                            }
                        }
                    }
                } else {
                    self.point_mode.update(|point| *point = !*point);
                    self.pointed.set(None);
                }
            }
            "F4" if !ctrl && !ev.alt_key() => {
                ev.prevent_default();
                ev.stop_propagation();
                let (start, end) = field_selection(assist);
                let picked_range = self
                    .model
                    .with_untracked(|model| model.point_at_caret(start, end))
                    .filter(|pick| pick.anchor != pick.corner);
                let (start, end) = picked_range.map_or((start, end), |pick| (pick.start, pick.end));
                if let Some((text, start, end)) =
                    cycle_anchors(&self.edit_text.get_untracked(), start, end)
                {
                    self.model.update(|model| {
                        model.edit.pointed = picked_range.map(|pick| PointPick { end, ..pick });
                        if picked_range.is_some() {
                            model.edit.pointed_text = text.clone();
                        }
                    });
                    assist.place(text, end);
                    if picked_range.is_none() && let Some(field) = assist.field.get_untracked() {
                        let _ = field.set_selection_range(start as u32, end as u32);
                    }
                }
            }
            "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Home" | "End"
                if self.point_mode.get_untracked() && !ev.alt_key() =>
            {
                let direction = match key.as_str() {
                    "ArrowUp" => Some(NavAction::Up),
                    "ArrowDown" => Some(NavAction::Down),
                    "ArrowLeft" => Some(NavAction::Left),
                    "ArrowRight" => Some(NavAction::Right),
                    _ => None,
                };
                if !ctrl && let Some(action) = direction {
                    let from = self
                        .pointed
                        .get_untracked()
                        .map(|p| p.corner)
                        .or_else(|| {
                            self.edit_target
                                .get_untracked()
                                .map(|at| (at.row(), at.column()))
                        })
                        .unwrap_or((1, 1));
                    let next = next_active(from, action, GridExtent::new(1_048_576, 16_384));
                    if self.point_at(next.0, next.1, ev.shift_key()) {
                        ev.prevent_default();
                        ev.stop_propagation();
                        return;
                    }
                }
                // Native caret navigation leaves the editor in text editing mode.
                self.model.update(|model| {
                    model.edit.point_mode = false;
                    model.edit.pointed = None;
                });
            }
            _ => {}
        }
    }

    pub fn edit_changed(&self) {
        self.model.update(|model| {
            let edit = &mut model.edit;
            // Starting a formula in a blank cell enables pointing from either editor.
            if edit.surface == Some(EditOrigin::Bar)
                && !edit.replacement
                && edit.original.is_empty()
                && edit.draft.starts_with('=')
            {
                edit.point_mode = true;
            }
            edit.replacement = true;
            edit.pointed = None;
        });
        self.remember_editor_selection();
    }

    pub fn edit_label(&self) -> String {
        self.edit_target
            .get()
            .map(|at| self.cell_label(at))
            .unwrap_or_default()
    }

    /// Syntax and DOM reads feed the model's one pointed-edit transition.
    pub fn point_at(&self, row: u32, column: u32, extend: bool) -> bool {
        if !self.model.with_untracked(Model::can_point) {
            return false;
        }
        let assist = self.edit_field.get_value();
        let text = self.edit_text.get_untracked();
        let (start, end) = field_selection(assist);
        let Some(sheet) = self.sheet.get_untracked() else {
            return false;
        };
        let Ok(at) = CellKey::new(sheet, row, column) else {
            return false;
        };
        let syntax = if expects_reference(&text, start, end) {
            Some(dna_calc_model::PointSyntax::Insert { start, end })
        } else if self.point_mode.get_untracked() {
            point_reference_span(&text, start, end).map(|span| {
                dna_calc_model::PointSyntax::ReplaceReference {
                    start: span.start,
                    end: span.end(),
                }
            })
        } else {
            None
        };
        let next = self
            .model
            .try_update(|model| {
                model.point_at(
                    at,
                    extend,
                    dna_calc_model::TextSelection {
                        start,
                        end,
                        backward: false,
                    },
                    syntax,
                )
            })
            .flatten();
        let Some(next) = next else {
            return false;
        };
        assist.reset();
        assist.reflect(&next.text, next.caret);
        true
    }

    pub fn finish_edit(&self, movement: NavAction) {
        let controller = *self;
        self.commit_edit(move |accepted| {
            if accepted {
                let horizontal = matches!(
                    movement,
                    NavAction::CommitRight | NavAction::NextColumn | NavAction::PrevColumn
                );
                let reverse = matches!(movement, NavAction::Up | NavAction::PrevColumn);
                controller.navigate(
                    Navigation::Walk {
                        horizontal,
                        reverse,
                    },
                    false,
                    0,
                );
                controller.focus_active_skin();
            } else {
                controller.focus_editor();
            }
        });
    }

    /// Ends an edit without entering it.
    pub fn cancel_edit(&self) {
        if self.assist.readonly.get_untracked() {
            return;
        }
        if self.editing.get_untracked().is_some() {
            let target = self.edit_target.get_untracked();
            self.model.update(|model| model.edit.finish());
            self.assist.reset();
            if let Some(at) = target {
                self.switch_sheet(at.sheet());
                self.select_areas(at, self.edit_areas.get_value());
            }
            self.say("Edit cancelled", false);
        }
    }

    /// Enters the edited text into the active cell. `then` hears whether the entry was
    /// accepted; a refused entry keeps the edit open with the reason on the status line.
    pub fn commit_edit(&self, then: impl FnOnce(bool) + 'static) {
        self.commit_edit_as(false, then);
    }

    pub(super) fn commit_edit_as(&self, fill_selection: bool, then: impl FnOnce(bool) + 'static) {
        if self.assist.readonly.get_untracked() {
            return;
        }
        let Some(at) = self.edit_target.get_untracked() else {
            then(false);
            return;
        };
        let text = self.edit_text.get_untracked();
        if self.active_key_untracked() != Some(at)
            || self.areas.get_untracked() != self.edit_areas.get_value()
        {
            self.switch_sheet(at.sheet());
            self.select_areas(at, self.edit_areas.get_value());
        }
        if !fill_selection
            && !self.model.with_untracked(|model| model.edit.replacement)
            && text == self.edit_original.get_value()
        {
            self.model.update(|model| model.edit.finish());
            self.assist.reset();
            then(true);
            return;
        }
        self.assist.readonly.set(true);
        let controller = *self;
        self.dispatch_result(
            if fill_selection {
                WorkbookCommand::EnterSelection { text }
            } else {
                WorkbookCommand::EnterCell {
                    at: cell_target(at),
                    text,
                }
            },
            move |result| {
                controller.assist.readonly.set(false);
                let accepted = result.is_ok_and(|outcome| outcome.accepted);
                if accepted {
                    controller.model.update(|model| model.edit.finish());
                    controller.assist.reset();
                }
                then(accepted);
            },
        );
    }

    /// Text entered into any cell, one undoable edit (the notebook's blocks, a drop-down's item);
    /// `then` gets the outcome.
    pub fn enter(&self, at: CellKey, text: String, then: impl FnOnce(&Outcome) + 'static) {
        self.dispatch(
            WorkbookCommand::EnterCell {
                at: cell_target(at),
                text,
            },
            then,
        );
    }
}
