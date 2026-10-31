//! The skin framework: the contract every DNA Calc skin meets, the registry of skins, and which
//! skin is shown.
//!
//! A skin is one way of looking at and editing the one open workbook: the Excel grid, the
//! notebook, the atlas, the tree, the canvas and the ledger. The application shows exactly one
//! skin at a time.
//!
//! - **Switching re-projects, it never reloads.** [`switch_skin`] writes the active skin's
//!   identity and nothing else: no intent reaches the workbook, so the session's intent log does
//!   not grow, and the active sheet, the active cell, the edit history and the subscriptions of
//!   the application stay as they were.
//! - **A skin reaches the workbook only through its [`SkinContext`]**: the controller, whose every
//!   read is a calc-host projection or a subscription's delta and whose every change is a
//!   [`calc_host::SkinIntent`].
//! - **A skin that cannot be shown says so**: [`resolve_active_skin`] gives
//!   [`SkinResolution::Unavailable`] for an identity the registry does not hold, which the
//!   application shows as a card naming it, never a blank or another skin.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-shell/src/stage.rs` (`StageId`,
//! `StageSurface`, `StageRegistry`, `StageResolution`, `resolve_active_stage`, `switch_stage`,
//! with their tests) and the registration of `src/dnacalc-app/src/app.rs`. Left behind: the
//! profiles that decided which stages a product showed, the shared skin state and its audit log,
//! the continuity halo and the preview service.

use crate::controller::Controller;
use leptos::prelude::*;
use std::sync::Arc;

/// The skins DNA Calc has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkinId {
    /// The Excel grid: sheets, cells, the formula bar.
    Grid,
    /// The workbook's cells as a column of blocks, each with its formula and value.
    Notebook,
    /// The workbook's map: its sheets, what each holds, and how its formulas were calculated.
    Atlas,
    /// The workbook as a foldable outline: its names, sheets, rows and cells.
    Tree,
    /// The active cell's calculation as a flow: what it reads to the left, what reads it to the
    /// right.
    Canvas,
    /// The formula cells as one audit table, filtered and sorted.
    Ledger,
}

impl SkinId {
    /// Every skin, in the order the switcher lists them.
    pub const ALL: [SkinId; 6] = [
        SkinId::Grid,
        SkinId::Notebook,
        SkinId::Atlas,
        SkinId::Tree,
        SkinId::Canvas,
        SkinId::Ledger,
    ];

    /// The skin's stable name: the page's `data-skin` and the switcher's value.
    pub const fn stable_id(self) -> &'static str {
        match self {
            SkinId::Grid => "grid",
            SkinId::Notebook => "notebook",
            SkinId::Atlas => "atlas",
            SkinId::Tree => "tree",
            SkinId::Canvas => "canvas",
            SkinId::Ledger => "ledger",
        }
    }

    /// The skin a stable name names.
    pub fn from_stable_id(id: &str) -> Option<SkinId> {
        SkinId::ALL.into_iter().find(|skin| skin.stable_id() == id)
    }
}

/// What a skin is given when it is shown.
#[derive(Clone, Copy)]
pub struct SkinContext {
    pub controller: Controller,
    /// The shell owns the active skin; navigation keeps the shared workbook selection.
    pub active_skin: RwSignal<Option<String>>,
}

/// The contract of a skin.
pub trait Skin: Send + Sync + 'static {
    fn id(&self) -> SkinId;
    fn title(&self) -> &'static str;
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
        Some(id) => match SkinId::from_stable_id(id).and_then(|skin| registry.by_id(skin)) {
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

/// Shows the skin at 1-based `slot`: writes its stable name to `active` and nothing else. It takes
/// no controller, so it cannot reach the workbook.
pub fn switch_skin(
    active: RwSignal<Option<String>>,
    registry: &SkinRegistry,
    slot: u8,
) -> Option<SkinId> {
    let skin = registry.slot(slot)?;
    active.set(Some(skin.id().stable_id().to_owned()));
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
            let active = RwSignal::new(None);
            assert_eq!(switch_skin(active, &registry(), 2), Some(SkinId::Notebook));
            assert_eq!(active.get_untracked().as_deref(), Some("notebook"));
            assert_eq!(switch_skin(active, &registry(), 5), None);
            assert_eq!(active.get_untracked().as_deref(), Some("notebook"));
        });
    }
}
