//! The skinnable application model: state, commands and keyboard grammar, without a DOM,
//! reactive toolkit, evaluator or host implementation. Browser adapters execute its effects.
pub mod address;
mod editor;
pub mod format;
pub mod names;
pub use editor::{EditorState, PointEdit, PointSyntax, TextHistory, replace_text};
pub mod command;
pub mod geometry;
mod outbox;
pub mod presentation;
pub mod projection;
pub mod render_plan;
mod tile_mirror;
pub mod tiles;
pub use tile_mirror::{TileApply, TileMirrorError, ViewMirror};
mod runtime;
pub mod selection;
mod state;
pub use outbox::{CommandDelivery, CommandQueue};
pub use runtime::{Health, ReviewDraft, ReviewCreation, RuntimeState, Snapshot, Status, Timings, ToolState};
mod view;
pub mod viewport;
pub use calc_value::{CellKey, GridArea, Selection, SheetId};
pub use command::{
    Command, CommandContext, CommandId, CommandRequest, Effect, RevealTarget, SHELL_KEYMAP, Target,
    Unavailable, resolve_shell_key,
};
pub use state::{EditSession, FindMode, Model, PointPick, SelectionState, TextSelection};
pub use view::{
    AtlasState, Branch, CanvasState, ClipboardDraftState, ClipboardOwner, EditorContinuation,
    FocusRegistry, FocusTicket, GridState, LedgerState, NotebookState, SurfaceDescriptor,
    SurfaceRole, TreeFocus, TreeState, ViewState, ViewStateStore,
};

/// Stable registry identity. A new skin declares its own id without editing this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkinId(&'static str);
#[allow(non_upper_case_globals)]
impl SkinId {
    pub const Grid: Self = Self("grid");
    pub const Notebook: Self = Self("notebook");
    pub const Atlas: Self = Self("atlas");
    pub const Tree: Self = Self("tree");
    pub const Canvas: Self = Self("canvas");
    pub const Ledger: Self = Self("ledger");
    pub const ALL: [Self; 6] = [
        Self::Grid,
        Self::Notebook,
        Self::Atlas,
        Self::Tree,
        Self::Canvas,
        Self::Ledger,
    ];
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
    pub const fn stable_id(self) -> &'static str {
        self.0
    }
    /// Resolves built-in ids; registries resolve all registered skins by their stable string.
    pub fn from_stable_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|skin| skin.0 == id)
    }
}
/// An editor surface's registered identity; it describes a role, never a DOM selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId {
    id: &'static str,
    navigation: bool,
}
#[allow(non_upper_case_globals)]
impl SurfaceId {
    pub const Cell: Self = Self::new("cell");
    pub const Bar: Self = Self::new("formula-bar");
    pub const Notebook: Self = Self::new("notebook-source");
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            navigation: false,
        }
    }
    pub const fn navigation(skin: SkinId) -> Self {
        Self {
            id: skin.stable_id(),
            navigation: true,
        }
    }
    pub const fn stable_id(self) -> &'static str {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> CommandContext {
        CommandContext {
            ready: true,
            readonly: false,
            undo: true,
            redo: true,
        }
    }
    fn at(row: u32) -> CellKey {
        CellKey::new(SheetId::from_parts(1, 0), row, 1).unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn deferred_focus_preserves_submission_order_and_newer_native_attention() {
        let mut model = Model::default();
        let surface = SurfaceId::new("new-skin-editor");
        model.focus.register(surface);
        let request = model.prepare_command(Command::SwitchSkin(SkinId::Grid));
        model.focus.observe(Some(surface));
        let Effect::FocusSkin { ticket, .. } = model.dispatch_request(request, context()).unwrap()
        else {
            panic!("focus effect");
        };
        assert!(!model.focus.accepts(ticket));
        assert_eq!(model.focus.observed(), Some(surface));
        let raw_gesture = model.focus.snapshot();
        model.focus.observe(None);
        let request =
            model.prepare_command_with_focus(Command::SwitchSkin(SkinId::Grid), Some(raw_gesture));
        let Effect::FocusSkin { ticket, .. } = model.dispatch_request(request, context()).unwrap()
        else {
            panic!("focus effect");
        };
        assert!(!model.focus.accepts(ticket));
        let request = model.prepare_command(Command::Reveal {
            target: RevealTarget::Cell(at(1)),
            skin: SkinId::Grid,
        });
        let Effect::Reveal { ticket, .. } = model.dispatch_request(request, context()).unwrap()
        else {
            panic!("reveal effect");
        };
        assert!(model.focus.accepts(ticket));
        model.focus.observe(Some(surface));
        assert!(!model.focus.accepts(ticket));
        let earlier = model.focus.request();
        model.focus.observe(None);
        assert!(!model.focus.accepts(earlier));
        let earlier = model.focus.request();
        let newer = model.focus.request();
        assert!(!model.focus.accepts(earlier));
        assert!(model.focus.accepts(newer));
        model.reset_workbook();
        assert!(!model.focus.accepts(newer));
        assert_eq!(model.focus.observed(), None);
        assert!(model.focus.contains(surface));
        assert_ne!(model.focus.request(), earlier);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn editor_and_navigation_roles_register_independently_without_central_skin_ids() {
        let skin = SkinId::new("same-id");
        let editor = SurfaceDescriptor::editor(SurfaceId::new("same-id"));
        let navigation = SurfaceDescriptor::navigation(skin, ClipboardOwner::Workbook);
        assert_ne!(editor.id, navigation.id);
        let mut model = Model::default();
        assert!(model.focus.register_surface(editor));
        assert!(model.focus.register_surface(navigation));
        assert!(!model.focus.register_surface(navigation));
        assert_eq!(model.focus.surface(editor.id), Some(editor));
        assert_eq!(model.focus.surface(navigation.id), Some(navigation));
        let overlay = SurfaceDescriptor::editor_with_navigation(SurfaceId::new("overlay"), skin);
        assert!(model.focus.register_surface(overlay));
        assert!(!model.focus.register(overlay.id));
        assert_eq!(model.focus.surface(overlay.id), Some(overlay));
        let shared = SurfaceDescriptor::selection_editor(SurfaceId::new("shared"));
        assert!(model.focus.register_surface(shared));
        assert!(!model.focus.register(shared.id));
        assert_eq!(model.focus.surface(shared.id), Some(shared));
        model.reset_workbook();
        assert_eq!(model.focus.surface(editor.id), Some(editor));
        assert_eq!(model.focus.surface(navigation.id), Some(navigation));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn shared_command_grammar_and_availability_do_not_depend_on_the_skin() {
        for skin in SkinId::ALL.into_iter().chain([SkinId::new("a-new-skin")]) {
            let mut model = Model::default();
            model.active_skin = Some(skin.stable_id().into());
            for (key, ctrl, id, expected) in [
                ("Z", true, CommandId::Undo, Effect::Undo { editor: false }),
                ("Y", true, CommandId::Redo, Effect::Redo { editor: false }),
                ("f", true, CommandId::Find, Effect::Dialog),
                ("e", true, CommandId::FlashFill, Effect::FlashFill),
                ("F9", false, CommandId::Recalculate, Effect::Recalculate),
            ] {
                assert_eq!(
                    resolve_shell_key(&selection::KeyChord::new(key, false, ctrl, false)),
                    Some(id)
                );
                assert_eq!(
                    model.dispatch(Command::simple(id).unwrap(), context()),
                    Ok(expected)
                );
            }
            assert_eq!(resolve_shell_key(&selection::KeyChord::new("V",false,true,true)),Some(CommandId::PasteSpecial));
            assert_eq!(model.dispatch(Command::PasteSpecial,context()),Ok(Effect::PasteSpecial));
            let before = model.active_skin.clone();
            assert_eq!(
                model.availability(
                    CommandId::Undo,
                    CommandContext {
                        undo: false,
                        ..context()
                    }
                ),
                Err(Unavailable::NothingToUndo)
            );
            assert_eq!(model.active_skin, before);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn registered_editor_surfaces_share_the_target_and_refused_commands_keep_the_draft() {
        let mut model = Model::default();
        let first = SurfaceId::new("new-skin-source");
        model.focus.register(first);
        assert!(model.focus.contains(first));
        model.selection.select(at(1), vec![GridArea::cell(at(1))]);
        model.edit.begin(
            first,
            at(1),
            "=1+1".into(),
            None,
            model.selection.areas.clone(),
        );
        assert_eq!(
            model.edit.caret,
            Some(TextSelection {
                start: 4,
                end: 4,
                backward: false
            })
        );
        model.edit.draft = "=1+2".into();
        model.edit.caret = Some(TextSelection {
            start: 1,
            end: 3,
            backward: true,
        });
        let session = model.edit.clone();
        assert_eq!(
            model.dispatch(
                Command::SwitchSkin(SkinId::Grid),
                CommandContext {
                    readonly: true,
                    ..context()
                }
            ),
            Err(Unavailable::ReadOnly)
        );
        assert_eq!(model.edit, session);
        assert_eq!(
            model.dispatch(Command::Find, context()),
            Err(Unavailable::EditInProgress)
        );
        assert_eq!(model.edit, session);
        assert_eq!(model.dispatch(Command::FlashFill, context()),Err(Unavailable::EditInProgress));
        assert_eq!(model.dispatch(Command::PasteSpecial, context()),Err(Unavailable::EditInProgress));
        assert_eq!(model.edit,session);
        model
            .dispatch(Command::SwitchSkin(SkinId::new("new-skin")), context())
            .unwrap();
        assert_eq!(model.edit.surface, Some(SurfaceId::Bar));
        assert_eq!(model.edit.target, Some(at(1)));
        assert_eq!(model.edit.draft, "=1+2");
        assert_eq!(model.edit.caret, session.caret);
        assert_eq!(model.edit.id, session.id);
        assert_eq!(
            model.dispatch(Command::Undo, context()),
            Ok(Effect::Undo { editor: true })
        );
        model.edit.finish();
        assert_eq!(
            model.dispatch(Command::Undo, context()),
            Ok(Effect::Undo { editor: false })
        );
        model
            .edit
            .begin(first, at(2), String::new(), Some("😀".into()), Vec::new());
        assert_ne!(model.edit.id, session.id);
        assert_eq!(
            model.edit.caret,
            Some(TextSelection {
                start: 2,
                end: 2,
                backward: false
            })
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn skin_state_survives_switches_and_resets_by_registered_schema() {
        #[derive(Default)]
        struct Custom {
            zoom: u32,
        }
        impl ViewState for Custom {}
        let id = SkinId::new("custom");
        let mut model = Model::default();
        assert!(model.views.register::<Custom>(id));
        model.views.get_mut::<Custom>(id).unwrap().zoom = 125;
        model.dispatch(Command::SwitchSkin(id), context()).unwrap();
        model
            .dispatch(Command::SwitchSkin(SkinId::Grid), context())
            .unwrap();
        assert!(!model.views.register::<Custom>(id));
        assert_eq!(model.views.get::<Custom>(id).unwrap().zoom, 125);
        model.views.register::<NotebookState>(SkinId::Notebook);
        model
            .views
            .get_mut::<NotebookState>(SkinId::Notebook)
            .unwrap()
            .insert_text = "draft".into();
        assert!(model.views.pending_reason().is_some());
        model.reset_workbook();
        assert_eq!(model.views.get::<Custom>(id).unwrap().zoom, 0);
        assert!(model.views.pending_reason().is_none());
        assert_eq!(model.active_skin.as_deref(), Some("grid"));
    }
}

pub mod canvas;
pub mod clipboard;
pub mod ledger;
pub mod tree;

mod transport;
pub use transport::{TransportAnswer, WorkbookTransport};
