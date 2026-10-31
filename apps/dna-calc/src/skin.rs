//! The skin framework: the contract every DNA Calc skin meets, the registry of skins, and which
//! skin is shown.
//!
//! A skin is one way of looking at and editing the one open workbook: the Excel grid, the
//! notebook, the atlas, the tree, the canvas and the ledger. The application shows exactly one
//! skin at a time.
//!
//! - **Switching re-projects, it never reloads.** [`switch_skin`] dispatches the model's skin
//!   command. A live edit transfers to the shared formula bar; workbook state and history stay
//!   intact. Mounted views release and renew their subscriptions.
//! - **A skin reaches the workbook only through its [`SkinContext`]**: the controller, whose every
//!   read is a calc-host projection or a subscription's delta and whose every change is a
//!   [`calc_host::SkinIntent`].
//! - **A skin that cannot be shown says so**: [`resolve_active_skin`] gives
//!   [`SkinResolution::Unavailable`] for an identity the registry does not hold, which the
//!   application shows as a card naming it, never a blank or another skin.
//!
//! U1's current authoring surface:
//! - Implement `Skin` with a stable `SkinId::new("your-id")`, `mount` and a focus target.
//!   Declare the module and register it together in `app.rs`; an additional skin does not extend
//!   a central identity enum or the controller's state/reset list.
//! - Register pure, workbook-relative view data with `Controller::view_field` and a `ViewState`
//!   schema. The model retains it across mounts and resets it on workbook replacement.
//! - Submit `dna_calc_model::Command` for common actions and `Controller::reveal` for source
//!   navigation. The shell supplies Undo/Redo, Find/Replace/Go to and F9 to every mounted skin.
//! - For a shared formula editor, call `Controller::register_editor_surface` with an open
//!   `SurfaceDescriptor` and its `FormulaAssist`. Declare whether later keys retain the target,
//!   follow the active selection, or continue through the skin's navigation surface.
//!   The browser adapter binds each mount and queues its input without Grid or formula-bar DOM
//!   markers. Give simultaneously mounted logical editors distinct surface ids. Native controls
//!   retain their own keys.
//! - Declare the navigation surface's clipboard ownership and optional input target when it
//!   differs from the readiness-aware focus target. The model stores those surface roles.
//! - Reuse existing projections through the controller. Scoped CSS under `web/css` is included
//!   automatically. New projection families and the protocol/transport split remain U1 work.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-shell/src/stage.rs` (`StageId`,
//! `StageSurface`, `StageRegistry`, `StageResolution`, `resolve_active_stage`, `switch_stage`,
//! with their tests) and the registration of `src/dnacalc-app/src/app.rs`. Left behind: the
//! profiles that decided which stages a product showed, the shared skin state and its audit log,
//! the continuity halo and the preview service.

use crate::controller::Controller;
use leptos::prelude::*;
use std::sync::Arc;

pub use dna_calc_model::SkinId;

/// What a skin is given when it is shown.
#[derive(Clone, Copy)]
pub struct SkinContext {
    pub controller: Controller,
}

/// The contract of a skin.
pub trait Skin: Send + Sync + 'static {
    fn id(&self) -> SkinId;
    fn title(&self) -> &'static str;
    /// Input surface while worker-backed content is still mounting. Defaults to the focus target.
    fn input_target(&self) -> &'static str { self.focus_target() }
    /// Clipboard routing for this skin's navigation surface; registered editors retain native text.
    fn clipboard_owner(&self) -> dna_calc_model::ClipboardOwner { dna_calc_model::ClipboardOwner::Native }
    /// A focusable surface belonging to this skin. The shell owns focus timing.
    fn focus_target(&self) -> &'static str { "#dc-workspace" }
    /// The skin's view over the workbook. It is dropped when another skin is shown, so a skin
    /// releases what it holds (its subscriptions) in `on_cleanup`.
    fn mount(&self, ctx: SkinContext) -> AnyView;
}

/// The skins, in the switcher's order: the order they were registered in. A keyboard slot (1 to
/// 9) is a skin's place in that order.
#[derive(Clone, Default)]
pub struct SkinRegistry {
    skins: Vec<Arc<dyn Skin>>,
}

impl SkinRegistry {
    pub fn new() -> SkinRegistry {
        SkinRegistry::default()
    }

    pub fn register(&mut self, skin: Arc<dyn Skin>) {
        self.skins.push(skin);
    }

    pub fn with_skin(mut self, skin: Arc<dyn Skin>) -> SkinRegistry {
        self.register(skin);
        self
    }

    pub fn skins(&self) -> &[Arc<dyn Skin>] {
        &self.skins
    }

    /// The skin at 1-based keyboard `slot`.
    pub fn slot(&self, slot: u8) -> Option<Arc<dyn Skin>> {
        let index = usize::from(slot).checked_sub(1)?;
        self.skins.get(index).cloned()
    }

    pub fn by_id(&self, id: SkinId) -> Option<Arc<dyn Skin>> {
        self.skins.iter().find(|skin| skin.id() == id).cloned()
    }
}

/// What the application shows for the active skin.
pub enum SkinResolution {
    Mount(Arc<dyn Skin>),
    /// The active identity names no registered skin: shown as a card naming it.
    Unavailable {
        active: Option<String>,
    },
}

/// The skin to show: the one `active` names, the first registered when none is named yet, and
/// [`SkinResolution::Unavailable`] when the name is not a registered skin.
pub fn resolve_active_skin(registry: &SkinRegistry, active: Option<&str>) -> SkinResolution {
    match active {
        Some(id) => match registry.skins().iter().find(|skin| skin.id().stable_id() == id).cloned() {
            Some(skin) => SkinResolution::Mount(skin),
            None => SkinResolution::Unavailable {
                active: Some(id.to_owned()),
            },
        },
        None => match registry.skins().first() {
            Some(skin) => SkinResolution::Mount(Arc::clone(skin)),
            None => SkinResolution::Unavailable { active: None },
        },
    }
}

/// Shows the skin at 1-based `slot` through the shared model dispatch path.
pub fn switch_skin(
    controller: Controller,
    registry: &SkinRegistry,
    slot: u8,
) -> Option<SkinId> {
    let skin = registry.slot(slot)?;
    controller.command(dna_calc_model::Command::SwitchSkin(skin.id()));
    Some(skin.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSkin(SkinId);

    impl Skin for TestSkin {
        fn id(&self) -> SkinId {
            self.0
        }

        fn title(&self) -> &'static str {
            "test skin"
        }

        fn mount(&self, _ctx: SkinContext) -> AnyView {
            ().into_any()
        }
    }

    fn registry() -> SkinRegistry {
        SkinRegistry::new()
            .with_skin(Arc::new(TestSkin(SkinId::Grid)))
            .with_skin(Arc::new(TestSkin(SkinId::Notebook)))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn skin_ids_round_trip() {
        for id in SkinId::ALL {
            assert_eq!(SkinId::from_stable_id(id.stable_id()), Some(id));
        }
        assert_eq!(SkinId::from_stable_id("lens:tree"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn slots_follow_the_registration_order() {
        let registry = registry();
        assert_eq!(registry.slot(1).map(|skin| skin.id()), Some(SkinId::Grid));
        assert_eq!(
            registry.slot(2).map(|skin| skin.id()),
            Some(SkinId::Notebook)
        );
        assert!(registry.slot(3).is_none());
        assert!(registry.slot(0).is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resolve_falls_back_to_the_first_then_says_what_it_cannot_show() {
        let registry = registry();
        assert!(matches!(
            resolve_active_skin(&registry, None),
            SkinResolution::Mount(skin) if skin.id() == SkinId::Grid
        ));
        assert!(matches!(
            resolve_active_skin(&registry, Some("notebook")),
            SkinResolution::Mount(skin) if skin.id() == SkinId::Notebook
        ));
        // A skin DNA Calc has but this registry does not hold.
        assert!(matches!(
            resolve_active_skin(&registry, Some("atlas")),
            SkinResolution::Unavailable { active: Some(id) } if id == "atlas"
        ));
        // A name that is no skin at all.
        assert!(matches!(
            resolve_active_skin(&registry, Some("lens:tree")),
            SkinResolution::Unavailable { active: Some(id) } if id == "lens:tree"
        ));
        assert!(matches!(
            resolve_active_skin(&SkinRegistry::new(), None),
            SkinResolution::Unavailable { active: None }
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn switching_writes_only_the_active_skin() {
        let owner = Owner::new();
        owner.with(|| {
            let controller = Controller::new();
            let active = controller.active_skin;
            assert_eq!(switch_skin(controller, &registry(), 2), Some(SkinId::Notebook));
            assert_eq!(active.get_untracked().as_deref(), Some("notebook"));
            assert_eq!(switch_skin(controller, &registry(), 5), None);
            assert_eq!(active.get_untracked().as_deref(), Some("notebook"));
        });
    }
}
