//! The skinnable application model: state, commands and keyboard grammar, without a DOM,
//! reactive toolkit, evaluator or host implementation. Browser adapters execute its effects.
pub mod command;
pub mod geometry;
pub mod selection;
mod state;
mod view;
pub mod viewport;
pub use calc_value::{CellKey, GridArea, Selection, SheetId};
pub use command::{
    Command, CommandContext, CommandId, Effect, RevealTarget, SHELL_KEYMAP, Target, Unavailable,
    resolve_shell_key,
};
pub use state::{EditSession, FindMode, Model, PointPick, SelectionState};
pub use view::{
    AtlasState, Branch, CanvasState, FocusRegistry, GridState, LedgerState, NotebookState,
    TreeFocus, TreeState, ViewState, ViewStateStore,
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
pub struct SurfaceId(&'static str);
#[allow(non_upper_case_globals)]
impl SurfaceId {
    pub const Cell: Self = Self("cell");
    pub const Bar: Self = Self("formula-bar");
    pub const Notebook: Self = Self("notebook-source");
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
    pub const fn stable_id(self) -> &'static str {
        self.0
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
    fn shared_command_grammar_and_availability_do_not_depend_on_the_skin() {
        for skin in SkinId::ALL.into_iter().chain([SkinId::new("a-new-skin")]) {
            let mut model = Model::default();
            model.active_skin = Some(skin.stable_id().into());
            for (key, ctrl, id, expected) in [
                ("Z", true, CommandId::Undo, Effect::Undo { editor: false }),
                ("Y", true, CommandId::Redo, Effect::Redo { editor: false }),
                ("f", true, CommandId::Find, Effect::Dialog),
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
        model.edit.draft = "=1+2".into();
        let session = model.edit.clone();
        assert_eq!(
            model.dispatch(Command::Find, context()),
            Err(Unavailable::EditInProgress)
        );
        assert_eq!(model.edit, session);
        model
            .edit
            .begin(SurfaceId::Bar, at(2), "ignored".into(), None, Vec::new());
        assert_eq!(model.edit.target, Some(at(1)));
        assert_eq!(model.edit.draft, "=1+2");
        assert_eq!(
            model.dispatch(Command::Undo, context()),
            Ok(Effect::Undo { editor: true })
        );
        model.edit.finish();
        assert_eq!(
            model.dispatch(Command::Undo, context()),
            Ok(Effect::Undo { editor: false })
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
