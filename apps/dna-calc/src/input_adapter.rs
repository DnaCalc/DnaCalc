//! Browser input and focus effects over registered model surfaces. No workbook rules live here.
use super::*;
use dna_calc_model::{ClipboardOwner, SurfaceDescriptor, SurfaceId};

#[cfg(target_arch = "wasm32")]
fn text_selection(field: &web_sys::HtmlTextAreaElement) -> dna_calc_model::TextSelection {
    dna_calc_model::TextSelection {
        start: field.selection_start().ok().flatten().unwrap_or(0) as usize,
        end: field.selection_end().ok().flatten().unwrap_or(0) as usize,
        backward: field.selection_direction().ok().flatten().as_deref() == Some("backward"),
    }
}
#[cfg(target_arch = "wasm32")]
use dna_calc_model::{EditorContinuation, SurfaceRole};

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub(super) struct Surfaces {
    editors: Vec<(u64, SurfaceId, FormulaAssist)>,
    next: u64,
    restoring: bool,
    replayed_edit: Option<(u64, u64)>,
}
#[cfg(target_arch = "wasm32")]
impl Surfaces {
    fn token(&mut self) -> u64 {
        self.next = self
            .next
            .checked_add(1)
            .expect("input registration identity exhausted");
        self.next
    }
    fn bind(&mut self, registration: Option<u64>, id: SurfaceId, assist: FormulaAssist) {
        let Some(field) = assist.field.try_get_untracked().flatten() else {
            return;
        };
        let existing = self.editors.iter().find_map(|(token, _, prior)| {
            (prior.field.try_get_untracked().flatten().as_ref() == Some(&field)).then_some(*token)
        });
        let token = registration.or(existing).unwrap_or_else(|| self.token());
        self.editors.retain(|(_, _, prior)| {
            prior
                .field
                .try_get_untracked()
                .flatten()
                .is_some_and(|node| node.is_connected() && node != field)
        });
        self.editors.push((token, id, assist));
    }
    fn editor(
        &self,
        target: &web_sys::Element,
    ) -> Option<(SurfaceId, FormulaAssist, web_sys::HtmlTextAreaElement)> {
        use wasm_bindgen::JsCast;
        self.editors.iter().find_map(|(_, id, assist)| {
            let node = assist.field.try_get_untracked().flatten()?;
            (node.is_connected() && node.unchecked_ref::<web_sys::Element>() == target)
                .then(|| (*id, *assist, node))
        })
    }
    fn owns(&self, id: SurfaceId, field: &web_sys::HtmlTextAreaElement) -> bool {
        use wasm_bindgen::JsCast;
        self.editor(field.unchecked_ref())
            .is_some_and(|(registered, _, _)| registered == id)
    }
    fn editor_for(&self, id: SurfaceId) -> Option<(FormulaAssist, web_sys::HtmlTextAreaElement)> {
        self.editors
            .iter()
            .rev()
            .filter(|(_, registered, _)| *registered == id)
            .find_map(|(_, _, assist)| {
                let node = assist.field.try_get_untracked().flatten()?;
                node.is_connected().then_some((*assist, node))
            })
    }
}
#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
enum InputOrigin {
    Navigation {
        skin: SkinId,
    },
    Editor {
        id: SurfaceId,
        assist: FormulaAssist,
        node: web_sys::HtmlTextAreaElement,
        target: CellKey,
        skin: Option<SkinId>,
        follows_selection: bool,
        caret: dna_calc_model::TextSelection,
    },
}
#[cfg(target_arch = "wasm32")]
impl InputOrigin {
    fn surface(&self) -> SurfaceId {
        match self {
            Self::Navigation { skin, .. } => SurfaceId::navigation(*skin),
            Self::Editor { id, .. } => *id,
        }
    }
}

impl Controller {
    pub(super) fn remember_editor_selection(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(surface) = self.editing.try_get_untracked().flatten() else {
                return;
            };
            let Some(assist) = self.edit_field.try_get_value() else {
                return;
            };
            let Some(field) = assist
                .field
                .try_get_untracked()
                .flatten()
                .filter(|node| node.is_connected())
            else {
                return;
            };
            // Opening an overlay can precede its mount: the cached field may still be the
            // formula bar. Only the active registered surface can contribute a native range.
            if !self
                .input_surfaces
                .with_value(|surfaces| surfaces.owns(surface, &field))
            {
                return;
            }
            let caret = text_selection(&field);
            self.model.maybe_update(|model| {
                let caret = Some(caret.bounded(model.edit.draft.encode_utf16().count()));
                if model.edit.caret == caret {
                    false
                } else {
                    model.edit.caret = caret;
                    true
                }
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn restoring_editor(&self, action: impl FnOnce()) {
        let prior = self
            .input_surfaces
            .with_value(|surfaces| surfaces.restoring);
        self.input_surfaces
            .update_value(|surfaces| surfaces.restoring = true);
        action();
        self.input_surfaces
            .update_value(|surfaces| surfaces.restoring = prior);
    }

    #[cfg(target_arch = "wasm32")]
    fn restore_editor_selection(
        &self,
        assist: FormulaAssist,
        field: &web_sys::HtmlTextAreaElement,
    ) {
        let Some(caret) = self.model.with_untracked(|model| model.edit.caret) else {
            return;
        };
        let text = self.edit_text.get_untracked();
        let caret = caret.bounded(text.encode_utf16().count());
        self.restoring_editor(|| {
            // Repaint model state without creating a local history entry or moving focus.
            field.set_value(&text);
            let _ = field.set_selection_range_with_direction(
                caret.start as u32,
                caret.end as u32,
                if caret.backward {
                    "backward"
                } else {
                    "forward"
                },
            );
            assist.sync_from_field(false);
        });
    }

    /// A newly mounted editor initializes its caret once; a live session restores its range.
    pub(crate) fn focus_mounted_editor(&self, assist: FormulaAssist) {
        #[cfg(target_arch = "wasm32")]
        if let Some(field) = assist.field.try_get_untracked().flatten() {
            self.restoring_editor(|| {
                let _ = field.focus();
                if self
                    .model
                    .with_untracked(|model| model.edit.caret.is_some())
                {
                    self.restore_editor_selection(assist, &field);
                } else {
                    let end = assist.caret.get_untracked() as u32;
                    let _ = field.set_selection_range(end, end);
                    self.remember_editor_selection();
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = assist;
    }

    /// Register any skin's shared formula component. The surface id is model data; its mounted
    /// field stays in this browser adapter. Multiple panes/blocks may use the same semantic role.
    pub fn register_editor_surface(&self, descriptor: SurfaceDescriptor, assist: FormulaAssist) {
        let id = descriptor.id;
        self.model
            .maybe_update(|model| model.focus.register_surface(descriptor));
        #[cfg(target_arch = "wasm32")]
        {
            let controller = *self;
            let mut token = 0;
            self.input_surfaces
                .update_value(|surfaces| token = surfaces.token());
            Effect::new(move |_| {
                if let Some(field) = assist.field.get() {
                    untrack(|| {
                        controller
                            .input_surfaces
                            .update_value(|surfaces| surfaces.bind(Some(token), id, assist));
                        if controller.editing.get_untracked() == Some(id) {
                            controller.edit_field.set_value(assist);
                            controller.restore_editor_selection(assist, &field);
                        }
                    });
                }
            });
            on_cleanup(move || {
                controller.input_surfaces.try_update_value(|surfaces| {
                    surfaces
                        .editors
                        .retain(|(registered, _, _)| *registered != token)
                });
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = assist;
    }
    pub(super) fn bind_editor_surface(&self, id: SurfaceId, assist: FormulaAssist) {
        self.model.maybe_update(|model| model.focus.register(id));
        #[cfg(target_arch = "wasm32")]
        self.input_surfaces
            .update_value(|surfaces| surfaces.bind(None, id, assist));
        #[cfg(not(target_arch = "wasm32"))]
        let _ = assist;
    }
    #[cfg(target_arch = "wasm32")]
    fn navigation_surface(&self) -> Option<(SkinId, &'static str)> {
        let active = self.active_skin.get_untracked();
        self.skin_registry.with_value(|registry| {
            let skin = match active.as_deref() {
                Some(id) => registry
                    .skins()
                    .iter()
                    .find(|skin| skin.id().stable_id() == id),
                None => registry.skins().first(),
            }?;
            Some((skin.id(), skin.input_target()))
        })
    }
    #[cfg(target_arch = "wasm32")]
    fn input_origin(&self, target: &web_sys::Element) -> Option<InputOrigin> {
        if let Some((id, assist, node)) = self
            .input_surfaces
            .with_value(|surfaces| surfaces.editor(target))
        {
            let continuation =
                self.model
                    .with_untracked(|model| match model.focus.surface(id)?.role {
                        SurfaceRole::Editor { continuation } => Some(continuation),
                        _ => None,
                    })?;
            if let EditorContinuation::Navigation(skin) = continuation {
                let (active, selector) = self.navigation_surface()?;
                if active != skin {
                    return None;
                }
                target.closest(selector).ok().flatten()?;
                return Some(InputOrigin::Navigation { skin });
            }
            return Some(InputOrigin::Editor {
                id,
                assist,
                node: node.clone(),
                target: self
                    .edit_target
                    .get_untracked()
                    .or_else(|| self.active_key_untracked())?,
                skin: self.active_skin_id(),
                follows_selection: continuation == EditorContinuation::Selection,
                caret: text_selection(&node),
            });
        }
        if target.closest("input, textarea, select, button, a[href], summary, [contenteditable=true], [role=button], [role=combobox], [role=slider]").ok().flatten().is_some() { return None; }
        let (skin, selector) = self.navigation_surface()?;
        target.closest(selector).ok().flatten()?;
        Some(InputOrigin::Navigation { skin })
    }
    /// The model declares workbook clipboard ownership; native text fields always retain theirs.
    pub fn workbook_owns_clipboard(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            if self.editing.get_untracked().is_some() {
                return false;
            }
            let Some(target) = document().active_element() else {
                return false;
            };
            self.input_origin(&target).is_some_and(|origin| {
                self.model.with_untracked(|model| {
                    model
                        .focus
                        .surface(origin.surface())
                        .is_some_and(|surface| surface.clipboard == ClipboardOwner::Workbook)
                })
            })
        }
        #[cfg(not(target_arch = "wasm32"))]
        false
    }
}

impl Controller {
    pub fn register_skins(&self, registry: crate::skin::SkinRegistry) {
        self.model.maybe_update(|model| {
            let mut changed = false;
            for skin in registry.skins() {
                changed |= model.focus.register_surface(SurfaceDescriptor::navigation(
                    skin.id(),
                    skin.clipboard_owner(),
                ));
            }
            changed
        });
        self.skin_registry.set_value(registry);
    }
    /// Serialize input behind worker selection and edit replies. A timer between keys lets the
    /// actual editor mount before its next event; all replay uses the application's handlers.
    pub fn install_input_queue(self) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::{JsCast, closure::Closure};
            // Native fields own their clipboard; registered workbook surfaces use one adapter.
            let copied = window_event_listener(leptos::ev::copy, move |event| {
                if self.workbook_owns_clipboard() {
                    crate::clipboard::native_copy(self, event, false);
                } else {
                    self.system_clipboard_changed();
                }
            });
            let cut = window_event_listener(leptos::ev::cut, move |event| {
                if self.workbook_owns_clipboard() {
                    crate::clipboard::native_copy(self, event, true);
                } else {
                    self.system_clipboard_changed();
                }
            });
            let paste = window_event_listener(leptos::ev::paste, move |event| {
                if self.workbook_owns_clipboard() {
                    crate::clipboard::native_paste(self, event);
                }
            });
            on_cleanup(move || {
                copied.remove();
                cut.remove();
                paste.remove();
            });
            // Selection-only native events do not emit input. Keep the model range current
            // before an asynchronously refreshed projection replaces its textarea.
            let selection =
                Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                    if self.replaying.get_value()
                        || self.has_deferred()
                        || self
                            .input_surfaces
                            .with_value(|surfaces| surfaces.restoring)
                    {
                        return;
                    }
                    let Some(target) = event
                        .target()
                        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                    else {
                        return;
                    };
                    let active = self
                        .edit_field
                        .with_value(|assist| assist.field.try_get_untracked().flatten());
                    if active.is_some_and(|field| {
                        field.is_connected() && field.unchecked_ref::<web_sys::Element>() == &target
                    }) {
                        self.remember_editor_selection();
                    }
                });
            for name in ["select", "keyup", "pointerup"] {
                let _ = document().add_event_listener_with_callback_and_bool(
                    name,
                    selection.as_ref().unchecked_ref(),
                    true,
                );
            }
            let selection = StoredValue::new_local(selection);
            on_cleanup(move || {
                selection.with_value(|selection| {
                    for name in ["select", "keyup", "pointerup"] {
                        let _ = document().remove_event_listener_with_callback_and_bool(
                            name,
                            selection.as_ref().unchecked_ref(),
                            true,
                        );
                    }
                })
            });
            Effect::new(move |_| {
                let pending = self.pending_actions.get();
                self.deferred_version.track();
                if pending == 0
                    && self.validation_decision.get().is_none()
                    && self.has_deferred()
                    && !self.flush_scheduled.get_value()
                {
                    self.flush_scheduled.set_value(true);
                    set_timeout(
                        move || {
                            if self.pending_actions.try_get_untracked() != Some(0)
                                || self
                                    .validation_decision
                                    .try_get_untracked()
                                    .flatten()
                                    .is_some()
                            {
                                let _ = self.flush_scheduled.try_set_value(false);
                                return;
                            }
                            let mut action = None;
                            self.deferred
                                .update_value(|queue| action = queue.pop_front());
                            if let Some(action) = action {
                                self.perform(action);
                            }
                            self.flush_scheduled.set_value(false);
                            self.deferred_version.update(|v| *v += 1);
                        },
                        std::time::Duration::ZERO,
                    );
                }
            });
            let keys = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |event: web_sys::KeyboardEvent| {
                    if self.replaying.get_value() {
                        return;
                    }
                    if matches!(
                        event.key().as_str(),
                        "F6" | "Unidentified"
                            | "Control"
                            | "Shift"
                            | "Alt"
                            | "Meta"
                            | "CapsLock"
                            | "NumLock"
                            | "ScrollLock"
                    ) {
                        return;
                    }
                    let Some(target) = event
                        .target()
                        .and_then(|v| v.dyn_into::<web_sys::Element>().ok())
                    else {
                        return;
                    };
                    // A live local pointer gesture owns Escape even during a worker reply.
                    if target.get_attribute("data-resizing").as_deref() == Some("true") {
                        return;
                    }
                    if target.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some()
                        && !self.has_deferred()
                    {
                        self.remember_editor_selection();
                    }
                    let Some(origin) = self.input_origin(&target) else {
                        return;
                    };
                    let navigation = matches!(origin, InputOrigin::Navigation { .. });
                    let opening_editor = self.editing.get_untracked().is_some() && navigation;
                    // Let the real clipboard event carry its data even while a worker reply is pending.
                    if (event.ctrl_key() || event.meta_key())
                        && matches!(event.key().to_ascii_lowercase().as_str(), "c" | "v" | "x")
                    {
                        return;
                    }
                    if !self.waiting_input() && !opening_editor {
                        return;
                    }
                    if event.is_composing() || event.key_code() == 229 {
                        return;
                    }
                    if target.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some()
                        && self.editing.get_untracked().is_some()
                        && !self.assist.readonly.get_untracked()
                        && !self.has_deferred()
                    {
                        return;
                    }
                    let init = web_sys::KeyboardEventInit::new();
                    init.set_key(&event.key());
                    init.set_code(&event.code());
                    init.set_ctrl_key(event.ctrl_key());
                    init.set_meta_key(event.meta_key());
                    init.set_shift_key(event.shift_key());
                    init.set_alt_key(event.alt_key());
                    init.set_bubbles(true);
                    init.set_cancelable(true);
                    event.prevent_default();
                    event.stop_immediate_propagation();
                    let generation = self.generation.get_untracked();
                    let started = None;
                    self.enqueue(move || self.replay_key(init, origin, generation, started));
                },
            );
            let _ = document().add_event_listener_with_callback_and_bool(
                "keydown",
                keys.as_ref().unchecked_ref(),
                true,
            );
            let keys = StoredValue::new_local(keys);
            on_cleanup(move || {
                keys.with_value(|keys| {
                    let _ = document().remove_event_listener_with_callback_and_bool(
                        "keydown",
                        keys.as_ref().unchecked_ref(),
                        true,
                    );
                })
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn replay_key(
        self,
        init: web_sys::KeyboardEventInit,
        mut origin: InputOrigin,
        generation: u64,
        started: Option<f64>,
    ) {
        use wasm_bindgen::JsCast;
        if self.generation.try_get_untracked() != Some(generation) {
            return;
        }
        if matches!(&origin, InputOrigin::Navigation { skin, .. }
            if self.navigation_surface().map(|(id, _)| id) != Some(*skin))
        {
            self.say(
                "The navigation surface changed; queued input cancelled",
                true,
            );
            return;
        }
        let surface = self.editing.get_untracked();
        if let InputOrigin::Editor {
            id,
            assist,
            node,
            target,
            skin,
            follows_selection,
            caret,
        } = &mut origin
        {
            let current = if surface.is_some() {
                self.edit_target.get_untracked()
            } else {
                self.active_key_untracked()
            };
            if *follows_selection {
                let Some(at) = current else {
                    return;
                };
                if *target != at {
                    let len = if surface.is_some() {
                        self.edit_text.get_untracked().encode_utf16().count()
                    } else {
                        self.selection.with_untracked(|mirror| {
                            mirror
                                .cell(at)
                                .map_or(0, |cell| cell.input.encode_utf16().count())
                        })
                    };
                    *caret = dna_calc_model::TextSelection {
                        start: len,
                        end: len,
                        backward: false,
                    };
                }
                *target = at;
            }
            if current != Some(*target) {
                self.say("The editing target changed; queued input cancelled and the current draft retained", true);
                return;
            }
            if surface.is_some() {
                let session = (generation, self.model.with_untracked(|model| model.edit.id));
                let first = self
                    .input_surfaces
                    .try_update_value(|surfaces| {
                        let first = surfaces.replayed_edit != Some(session);
                        surfaces.replayed_edit = Some(session);
                        first
                    })
                    .unwrap_or(false);
                if first {
                    let caret = *caret;
                    self.model.update(|model| {
                        model.edit.caret =
                            Some(caret.bounded(model.edit.draft.encode_utf16().count()))
                    });
                }
            }
            if surface.is_none_or(|surface| surface == *id) {
                let mounted = assist
                    .field
                    .try_get_untracked()
                    .flatten()
                    .is_some_and(|current| current == *node && current.is_connected());
                if !mounted {
                    if self.active_skin_id() != *skin {
                        self.say("The editing surface closed; queued input cancelled and the current draft retained", true);
                        return;
                    }
                    let found = self
                        .input_surfaces
                        .with_value(|surfaces| surfaces.editor_for(*id));
                    let Some((fresh, field)) = found else {
                        self.retry_key(init, origin, generation, started);
                        return;
                    };
                    *assist = fresh;
                    *node = field;
                    if surface.is_some() {
                        self.restore_editor_selection(fresh, &node);
                    }
                }
                self.edit_field.set_value(*assist);
            }
        }
        let Ok(event) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        else {
            return;
        };
        if matches!(
            event.key().as_str(),
            "Unidentified"
                | "Control"
                | "Shift"
                | "Alt"
                | "Meta"
                | "CapsLock"
                | "NumLock"
                | "ScrollLock"
        ) {
            return;
        }
        let editing = self.editing.get_untracked().is_some();
        if !editing && let InputOrigin::Editor { id, assist, .. } = &origin {
            self.activate_editor(*id, *assist);
            self.retry_key(init, origin, generation, started);
            return;
        }
        let mut field = if editing {
            self.edit_field
                .get_value()
                .field
                .try_get_untracked()
                .flatten()
                .filter(|node| {
                    surface.is_some_and(|id| {
                        self.input_surfaces
                            .with_value(|surfaces| surfaces.owns(id, node))
                    })
                })
        } else {
            None
        };
        if editing && field.is_none() {
            if let Some(id) = surface
                && let Some((assist, node)) = self
                    .input_surfaces
                    .with_value(|surfaces| surfaces.editor_for(id))
            {
                self.edit_field.set_value(assist);
                self.restore_editor_selection(assist, &node);
                field = Some(node);
            } else {
                self.retry_key(init, origin, generation, started);
                return;
            }
        }
        let target: Option<web_sys::Element> = field
            .as_ref()
            .map(|v| v.clone().unchecked_into::<web_sys::Element>())
            .or_else(|| {
                self.active_skin_element()
                    .map(|node| node.unchecked_into::<web_sys::Element>())
            });
        let Some(target) = target else {
            self.retry_key(init, origin, generation, started);
            return;
        };
        if let Some(field) = &field {
            self.restore_editor_selection(self.edit_field.get_value(), field);
        }
        let _ = target.dispatch_event(&event);
        // A navigation-origin key may open or advance this same editor before a later
        // editor-origin event arrives. Its range must also supersede captured snapshots.
        if self.editing.get_untracked().is_some() {
            let session = (generation, self.model.with_untracked(|model| model.edit.id));
            self.input_surfaces
                .update_value(|surfaces| surfaces.replayed_edit = Some(session));
        }
        self.remember_editor_selection();
        if event.default_prevented() || !editing {
            return;
        }
        let Some(field) = field else {
            return;
        };
        let assist = self.edit_field.get_value();
        let text = field.value();
        let (mut start, mut end) = field_selection(assist);
        let len = text.encode_utf16().count();
        let key = event.key();
        let ctrl = event.ctrl_key() || event.meta_key();
        let insert = if !ctrl && !event.alt_key() && key.chars().count() == 1 {
            Some(key.as_str())
        } else if event.alt_key() && key == "Enter" {
            Some("\n")
        } else if !ctrl && !event.alt_key() && matches!(key.as_str(), "Backspace" | "Delete") {
            if start == end {
                let boundaries: Vec<usize> = std::iter::once(0)
                    .chain(text.chars().scan(0, |n, c| {
                        *n += c.len_utf16();
                        Some(*n)
                    }))
                    .collect();
                if key == "Backspace" {
                    start = boundaries
                        .iter()
                        .copied()
                        .filter(|n| *n < start)
                        .next_back()
                        .unwrap_or(0);
                } else {
                    end = boundaries.iter().copied().find(|n| *n > end).unwrap_or(len);
                }
            }
            Some("")
        } else {
            None
        };
        if let Some(insert) = insert {
            let (next, caret) = replace_text(&text, start, end, insert);
            assist.place(next, caret);
            assist.sync_from_field(false);
            self.edit_changed();
        } else if ctrl && key.eq_ignore_ascii_case("a") {
            let _ = field.set_selection_range(0, len as u32);
            assist.sync_from_field(false);
            self.remember_editor_selection();
        } else if matches!(key.as_str(), "ArrowLeft" | "ArrowRight" | "Home" | "End")
            && (!ctrl || matches!(key.as_str(), "Home" | "End"))
        {
            let boundaries: Vec<usize> = std::iter::once(0)
                .chain(text.chars().scan(0, |n, c| {
                    *n += c.len_utf16();
                    Some(*n)
                }))
                .collect();
            let backward =
                field.selection_direction().ok().flatten().as_deref() == Some("backward");
            let (anchor, focus) = if backward { (end, start) } else { (start, end) };
            let units: Vec<u16> = text.encode_utf16().collect();
            let caret = match key.as_str() {
                "Home" if ctrl => 0,
                "End" if ctrl => len,
                "Home" => units[..focus]
                    .iter()
                    .rposition(|u| *u == 10)
                    .map_or(0, |n| n + 1),
                "End" => units[focus..]
                    .iter()
                    .position(|u| *u == 10)
                    .map_or(len, |n| focus + n),
                "ArrowLeft" if start != end && !event.shift_key() => start,
                "ArrowRight" if start != end && !event.shift_key() => end,
                "ArrowLeft" => boundaries
                    .iter()
                    .copied()
                    .filter(|n| *n < focus)
                    .next_back()
                    .unwrap_or(0),
                _ => boundaries
                    .iter()
                    .copied()
                    .find(|n| *n > focus)
                    .unwrap_or(len),
            };
            let (a, b, direction) = if event.shift_key() {
                (
                    anchor.min(caret),
                    anchor.max(caret),
                    if caret < anchor {
                        "backward"
                    } else {
                        "forward"
                    },
                )
            } else {
                (caret, caret, "none")
            };
            let _ = field.set_selection_range_with_direction(a as u32, b as u32, direction);
            assist.sync_from_field(false);
            self.remember_editor_selection();
        } else if !matches!(key.as_str(), "Control" | "Shift" | "Alt" | "Meta") {
            self.say(
                "Workbook reply delayed this editor shortcut; repeat it after Pending clears",
                true,
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn retry_key(
        self,
        init: web_sys::KeyboardEventInit,
        origin: InputOrigin,
        generation: u64,
        started: Option<f64>,
    ) {
        let started = started.unwrap_or_else(now_ms);
        if now_ms() - started >= 1000.0 {
            self.say("The input surface did not mount; queued input cancelled and the current draft retained", true);
            return;
        }
        self.deferred.update_value(|queue| {
            queue.push_front(Box::new(move || {
                self.replay_key(init, origin, generation, Some(started))
            }))
        });
        self.deferred_version.update(|v| *v += 1);
    }
    pub(super) fn focus_after_mount(&self, skin: SkinId, reveal: bool) {
        #[cfg(target_arch = "wasm32")]
        self.focus_skin_frame(
            skin,
            reveal,
            self.generation.get_untracked(),
            self.selection_epoch.get_value(),
            self.reveal_epoch.get_untracked(),
            now_ms() + 3000.0,
        );
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (skin, reveal);
    }

    #[cfg(target_arch = "wasm32")]
    fn focus_skin_frame(
        &self,
        skin: SkinId,
        reveal: bool,
        generation: u64,
        selection: u64,
        epoch: u64,
        until: f64,
    ) {
        let controller = *self;
        request_animation_frame(move || {
            if controller.generation.try_get_untracked() != Some(generation)
                || controller.active_skin_id() != Some(skin)
                || controller.selection_epoch.get_value() != selection
                || (reveal && controller.reveal_epoch.get_untracked() != epoch)
            {
                return;
            }
            if controller.editing.get_untracked().is_some() {
                controller.focus_editor();
            } else if let Some(element) = controller.active_skin_element() {
                if reveal {
                    controller
                        .model
                        .update(|model| model.selection.request_reveal());
                }
                let _ = element.focus();
            } else if now_ms() < until {
                controller.focus_skin_frame(skin, reveal, generation, selection, epoch, until);
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn active_skin_id(&self) -> Option<SkinId> {
        let active = self.active_skin.try_get_untracked()?;
        self.skin_registry
            .try_with_value(|registry| {
                match active.as_deref() {
                    Some(id) => registry
                        .skins()
                        .iter()
                        .find(|skin| skin.id().stable_id() == id),
                    None => registry.skins().first(),
                }
                .map(|skin| skin.id())
            })
            .flatten()
    }

    #[cfg(target_arch = "wasm32")]
    fn active_skin_element(&self) -> Option<web_sys::HtmlElement> {
        use wasm_bindgen::JsCast;
        let active = self.active_skin.get_untracked();
        let selector = self.skin_registry.with_value(|registry| {
            let skin = match active.as_deref() {
                Some(id) => registry
                    .skins()
                    .iter()
                    .find(|skin| skin.id().stable_id() == id),
                None => registry.skins().first(),
            };
            skin.map_or("#dc-workspace", |skin| skin.focus_target())
        });
        selector
            .split(", ")
            .find_map(|selector| document().query_selector(selector).ok().flatten())
            .and_then(|element| element.dyn_into().ok())
    }

    pub fn focus_active_skin(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = self.active_skin_element() {
            let _ = element.focus();
        } else if let Some(skin) = self.active_skin_id() {
            self.focus_after_mount(skin, false);
        }
    }
    pub(super) fn focus_command_dialog(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let controller = *self;
            request_animation_frame(move || {
                if let Some(mode) = controller.find_open.try_get_untracked().flatten() {
                    let selector = if mode == FindMode::GoTo {
                        "[data-dnacalc=goto-target]"
                    } else {
                        "[data-dnacalc=find-query]"
                    };
                    use wasm_bindgen::JsCast;
                    if let Some(field) = document()
                        .query_selector(selector)
                        .ok()
                        .flatten()
                        .and_then(|field| field.dyn_into::<web_sys::HtmlInputElement>().ok())
                    {
                        let _ = field.focus();
                        field.select();
                    }
                }
            });
        }
    }
}
