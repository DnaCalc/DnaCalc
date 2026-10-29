//! Skin layout (plan sections 8.1 and 8.2): the semantic blocks of a workbook and where each view
//! places them.
//!
//! A block is a stable, opaque identity over one visible backing cell or range of a worksheet,
//! its anchor, which the document reaches through the block's locator: a hidden, workbook-scoped
//! defined name. The formulas and prose a block shows are its backing cells' ordinary contents
//! and are never held here. Each of the four views (the notebook, the tree, the canvas and the
//! Excel grid) places blocks through placements with identities of their own, so one block can
//! appear in several views, or more than once in one view, without changing its identity. A
//! layout holds no calculated answer and no cell content: it is document-only metadata, which
//! calculation never reads.

use crate::input::FiniteNumber;
use calc_value::GridArea;
use std::fmt;
use std::num::{NonZeroU32, NonZeroU128};

/// The text form of a 128-bit ID: 32 lowercase hex digits in groups of 8, 4, 4, 4 and 12 joined
/// by `-`, as a UUID is written.
fn write_uuid(f: &mut fmt::Formatter<'_>, value: u128) -> fmt::Result {
    write!(
        f,
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (value >> 96) as u32,
        (value >> 80) as u16,
        (value >> 64) as u16,
        (value >> 48) as u16,
        value & 0xFFFF_FFFF_FFFF
    )
}

/// The value of a text in the form [`write_uuid`] writes, or `None` for any other text: upper-case
/// digits, braces and a missing or misplaced `-` are refused, so each value has one text.
fn parse_uuid(text: &str) -> Option<u128> {
    let bytes = text.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    let mut value = 0u128;
    for (index, &byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return None;
            }
            continue;
        }
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        value = (value << 4) | u128::from(digit);
    }
    Some(value)
}

/// Two ID types with the same form and different roles.
macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(NonZeroU128);

        impl $name {
            /// The ID with this value, or `None` for 0, which no ID takes.
            pub const fn new(value: u128) -> Option<Self> {
                match NonZeroU128::new(value) {
                    Some(value) => Some($name(value)),
                    None => None,
                }
            }

            pub const fn get(self) -> u128 {
                self.0.get()
            }

            /// The ID its text form gives: 32 lowercase hex digits in groups of 8, 4, 4, 4 and 12
            /// joined by `-`, as [`fmt::Display`] writes it. `None` for any other text and for
            /// the value 0.
            pub fn parse(text: &str) -> Option<Self> {
                parse_uuid(text).and_then(Self::new)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_uuid(f, self.get())
            }
        }
    };
}

opaque_id! {
    /// A semantic block's stable, opaque identity: a nonzero 128-bit value, written as a UUID is.
    /// The caller that creates a block issues its ID; this package never generates one, and checks
    /// only that no two blocks of one layout share it.
    BlockId
}

opaque_id! {
    /// One placement's identity, apart from the identity of the block it places: a nonzero 128-bit
    /// value, written as a UUID is, issued by the caller that adds the placement.
    PlacementId
}

/// What a block presents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// Prose: its backing cells hold the text as ordinary cell text.
    Prose,
    /// A calculation: its backing cell or range holds formulas or values.
    Calculation,
}

impl BlockKind {
    /// The kind's name in a manifest and on the wire.
    pub const fn name(self) -> &'static str {
        match self {
            BlockKind::Prose => "prose",
            BlockKind::Calculation => "calculation",
        }
    }

    /// The kind named `name`, compared exactly.
    pub fn from_name(name: &str) -> Option<BlockKind> {
        match name {
            "prose" => Some(BlockKind::Prose),
            "calculation" => Some(BlockKind::Calculation),
            _ => None,
        }
    }
}

/// The four views a layout places blocks in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewKind {
    /// An ordered list of blocks.
    Notebook,
    /// A hierarchy of placements: each has a parent placement or is a root. The hierarchy organizes
    /// the view only; it is not a scope and it changes no formula.
    Tree,
    /// Placements at world coordinates; their order is their z-order, a later one drawn above an
    /// earlier one.
    Canvas,
    /// The Excel grid's own view of the blocks: the order in which it steps through them. A block
    /// shows in the grid at its anchor, so a grid placement holds no position of its own.
    Grid,
}

impl ViewKind {
    /// Every view, in the order a layout keeps them.
    pub const ALL: [ViewKind; 4] = [
        ViewKind::Notebook,
        ViewKind::Tree,
        ViewKind::Canvas,
        ViewKind::Grid,
    ];

    /// The view's name in a manifest and on the wire.
    pub const fn name(self) -> &'static str {
        match self {
            ViewKind::Notebook => "notebook",
            ViewKind::Tree => "tree",
            ViewKind::Canvas => "canvas",
            ViewKind::Grid => "grid",
        }
    }

    /// The view named `name`, compared exactly.
    pub fn from_name(name: &str) -> Option<ViewKind> {
        ViewKind::ALL.into_iter().find(|view| view.name() == name)
    }

    const fn index(self) -> usize {
        match self {
            ViewKind::Notebook => 0,
            ViewKind::Tree => 1,
            ViewKind::Canvas => 2,
            ViewKind::Grid => 3,
        }
    }
}

/// The name of a block's locator: the hidden, workbook-scoped defined name that refers to the
/// block's anchor. It is kept apart from every name a user sees or writes; a user alias of a block
/// is a separate name.
///
/// A locator name is 1 to 255 ASCII characters: a letter or `_` first, then letters, digits, `_`
/// and `.`, and not a cell address (one to three letters followed by digits) or an R1C1 address
/// (`R`, `C`, `R<n>`, `C<n>` or `R<n>C<n>`, either case). That is a subset of the names Excel
/// admits, chosen so a locator never reads as a reference.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocatorName(String);

impl LocatorName {
    /// What every locator name [`LocatorName::for_block`] makes starts with.
    pub const PREFIX: &'static str = "_dna.block.";

    /// The locator name of a new block: [`LocatorName::PREFIX`] followed by the block ID's 32
    /// lowercase hex digits, so two blocks never get the same name.
    pub fn for_block(block: BlockId) -> LocatorName {
        LocatorName(format!("{}{:032x}", Self::PREFIX, block.get()))
    }

    /// `text` as a locator name, or `None` when it does not follow the rule of [`LocatorName`].
    pub fn new(text: &str) -> Option<LocatorName> {
        let bytes = text.as_bytes();
        let first = *bytes.first()?;
        let shaped = bytes.len() <= 255
            && (first.is_ascii_alphabetic() || first == b'_')
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'));
        (shaped && !reads_as_address(bytes)).then(|| LocatorName(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `name` is this name, compared as defined names compare (dc-cpr).
    pub fn is(&self, name: &str) -> bool {
        calc_value::workbook_names_equal(&self.0, name)
    }
}

/// Whether ASCII `text` is a cell address (one to three letters, then digits) or an R1C1 address.
fn reads_as_address(text: &[u8]) -> bool {
    let letters = text.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    let digits_after = |at: usize| text[at..].iter().all(u8::is_ascii_digit);
    if (1..=3).contains(&letters) && letters < text.len() && digits_after(letters) {
        return true;
    }
    // R, C, R<n>, C<n>, RC, R<n>C, RC<n>, R<n>C<n>.
    let mut at = 0;
    if text.get(at).is_some_and(|b| b.eq_ignore_ascii_case(&b'R')) {
        at += 1;
        while text.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
    }
    if text.get(at).is_some_and(|b| b.eq_ignore_ascii_case(&b'C')) {
        at += 1;
        while text.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
    }
    at > 0 && at == text.len()
}

/// One semantic block: its ID, its kind, the name of its locator and its anchor, the backing cell
/// or range its locator refers to. The anchor is `None` for a block whose locator a loaded
/// workbook does not hold or does not resolve to a cell or range of one of its sheets: the block
/// is kept, and nothing is invented in its place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    id: BlockId,
    kind: BlockKind,
    locator: LocatorName,
    anchor: Option<GridArea>,
}

impl Block {
    pub fn new(
        id: BlockId,
        kind: BlockKind,
        locator: LocatorName,
        anchor: Option<GridArea>,
    ) -> Block {
        Block {
            id,
            kind,
            locator,
            anchor,
        }
    }

    pub fn id(&self) -> BlockId {
        self.id
    }

    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    pub fn locator(&self) -> &LocatorName {
        &self.locator
    }

    pub fn anchor(&self) -> Option<GridArea> {
        self.anchor
    }
}

/// A canvas placement's rectangle in world coordinates: its top-left corner and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanvasArea {
    pub x: FiniteNumber,
    pub y: FiniteNumber,
    pub width: FiniteNumber,
    pub height: FiniteNumber,
}

/// One placement of a block in one view: its ID, the block, the view, and what that view keeps
/// for it (a tree placement's parent, a canvas placement's area). Its place in the view's order is
/// where the layout lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    id: PlacementId,
    block: BlockId,
    view: ViewKind,
    parent: Option<PlacementId>,
    area: Option<CanvasArea>,
}

impl Placement {
    pub fn notebook(id: PlacementId, block: BlockId) -> Placement {
        Placement::of(id, block, ViewKind::Notebook, None, None)
    }

    /// A tree placement under the tree placement `parent`, or a root for `None`.
    pub fn tree(id: PlacementId, block: BlockId, parent: Option<PlacementId>) -> Placement {
        Placement::of(id, block, ViewKind::Tree, parent, None)
    }

    pub fn canvas(id: PlacementId, block: BlockId, area: CanvasArea) -> Placement {
        Placement::of(id, block, ViewKind::Canvas, None, Some(area))
    }

    pub fn grid(id: PlacementId, block: BlockId) -> Placement {
        Placement::of(id, block, ViewKind::Grid, None, None)
    }

    fn of(
        id: PlacementId,
        block: BlockId,
        view: ViewKind,
        parent: Option<PlacementId>,
        area: Option<CanvasArea>,
    ) -> Placement {
        Placement {
            id,
            block,
            view,
            parent,
            area,
        }
    }

    pub fn id(&self) -> PlacementId {
        self.id
    }

    pub fn block(&self) -> BlockId {
        self.block
    }

    pub fn view(&self) -> ViewKind {
        self.view
    }

    /// A tree placement's parent; `None` for a root and for a placement of another view.
    pub fn parent(&self) -> Option<PlacementId> {
        self.parent
    }

    /// A canvas placement's area; `None` for a placement of another view.
    pub fn canvas_area(&self) -> Option<CanvasArea> {
        self.area
    }
}

/// Where a placement goes in its view. A position is the 1-based place the placement takes after
/// the edit: in the notebook's order, the canvas's z-order or the grid's order, or, in the tree,
/// among the children of its parent (among the roots for no parent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spot {
    Notebook {
        position: NonZeroU32,
    },
    Tree {
        parent: Option<PlacementId>,
        position: NonZeroU32,
    },
    Canvas {
        position: NonZeroU32,
        area: CanvasArea,
    },
    Grid {
        position: NonZeroU32,
    },
}

impl Spot {
    pub fn view(self) -> ViewKind {
        match self {
            Spot::Notebook { .. } => ViewKind::Notebook,
            Spot::Tree { .. } => ViewKind::Tree,
            Spot::Canvas { .. } => ViewKind::Canvas,
            Spot::Grid { .. } => ViewKind::Grid,
        }
    }

    pub fn position(self) -> NonZeroU32 {
        match self {
            Spot::Notebook { position }
            | Spot::Tree { position, .. }
            | Spot::Canvas { position, .. }
            | Spot::Grid { position } => position,
        }
    }

    fn placement(self, id: PlacementId, block: BlockId) -> Placement {
        match self {
            Spot::Notebook { .. } => Placement::notebook(id, block),
            Spot::Tree { parent, .. } => Placement::tree(id, block, parent),
            Spot::Canvas { area, .. } => Placement::canvas(id, block, area),
            Spot::Grid { .. } => Placement::grid(id, block),
        }
    }
}

/// The structural-edit descriptor for skin layout: create or remove a block with its locator, and
/// add, move or remove a block's placement in a view. Moving a placement changes the view only;
/// it never moves a cell.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SkinLayoutEdit {
    /// A new block over `anchor`, with the locator name [`LocatorName::for_block`] gives it.
    CreateBlock {
        block: BlockId,
        kind: BlockKind,
        anchor: GridArea,
    },
    /// Removes a block, and with it its locator. A block that is still placed is not removed.
    RemoveBlock { block: BlockId },
    AddPlacement {
        placement: PlacementId,
        block: BlockId,
        spot: Spot,
    },
    /// Moves a placement within its own view: a new position, and a new parent in the tree or a
    /// new area on the canvas.
    MovePlacement { placement: PlacementId, spot: Spot },
    /// Removes a placement. A tree placement that still has children is not removed.
    RemovePlacement { placement: PlacementId },
}

/// Why a layout, or an edit of one, was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutError {
    /// Two blocks have this ID.
    DuplicateBlock(BlockId),
    /// No block has this ID.
    UnknownBlock(BlockId),
    /// The block is still placed in a view, so it is not removed.
    BlockPlaced(BlockId),
    /// The locator name of `block` is the locator name of `other`, compared ignoring ASCII case.
    LocatorInUse { block: BlockId, other: BlockId },
    /// Two placements have this ID; IDs are unique across all four views.
    DuplicatePlacement(PlacementId),
    /// No placement has this ID.
    UnknownPlacement(PlacementId),
    /// A move gives `placement` a spot in another view than its own, `view`.
    OtherView {
        placement: PlacementId,
        view: ViewKind,
    },
    /// A tree placement's parent is not a placement of the tree.
    ParentNotInTree(PlacementId),
    /// `parent` is `placement` itself or one of its descendants.
    ParentCycle {
        placement: PlacementId,
        parent: PlacementId,
    },
    /// The tree placement still has children, so it is not removed.
    HasChildren(PlacementId),
    /// The position is past `last`, the last place the placement can take.
    PositionPastEnd { position: NonZeroU32, last: u32 },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::DuplicateBlock(block) => write!(f, "two blocks have the ID {block}"),
            LayoutError::UnknownBlock(block) => write!(f, "no block has the ID {block}"),
            LayoutError::BlockPlaced(block) => {
                write!(f, "the block {block} is still placed in a view")
            }
            LayoutError::LocatorInUse { block, other } => write!(
                f,
                "the block {block} would have the locator name of the block {other}"
            ),
            LayoutError::DuplicatePlacement(placement) => {
                write!(f, "two placements have the ID {placement}")
            }
            LayoutError::UnknownPlacement(placement) => {
                write!(f, "no placement has the ID {placement}")
            }
            LayoutError::OtherView { placement, view } => write!(
                f,
                "the placement {placement} is in the {} view and cannot move to another",
                view.name()
            ),
            LayoutError::ParentNotInTree(parent) => {
                write!(f, "the parent {parent} is not a placement of the tree")
            }
            LayoutError::ParentCycle { placement, parent } => write!(
                f,
                "the placement {placement} cannot go under {parent}, which is itself or one of its descendants"
            ),
            LayoutError::HasChildren(placement) => {
                write!(f, "the tree placement {placement} still has children")
            }
            LayoutError::PositionPastEnd { position, last } => {
                write!(f, "position {position} is past the last place, {last}")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

/// A workbook's skin layout: its blocks, in the order they were created, and each view's
/// placements, in that view's order. Every placement's block is a block of the layout, every
/// placement ID is unique across the views, no two blocks share an ID or a locator name, and every
/// tree placement's parent is a tree placement, without a cycle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkinLayout {
    blocks: Vec<Block>,
    /// The placements of each view, indexed as [`ViewKind::ALL`] lists the views.
    views: [Vec<Placement>; 4],
}

impl SkinLayout {
    /// The layout of these blocks and placements: each view's placements in the order they come.
    /// Refused when it breaks a rule of [`SkinLayout`].
    pub fn new(blocks: Vec<Block>, placements: Vec<Placement>) -> Result<SkinLayout, LayoutError> {
        let mut layout = SkinLayout::default();
        for block in blocks {
            layout.check_new_block(block.id, &block.locator)?;
            layout.blocks.push(block);
        }
        for placement in &placements {
            if layout.placement(placement.id).is_some() {
                return Err(LayoutError::DuplicatePlacement(placement.id));
            }
            if layout.block(placement.block).is_none() {
                return Err(LayoutError::UnknownBlock(placement.block));
            }
            layout.views[placement.view.index()].push(placement.clone());
        }
        for placement in &layout.views[ViewKind::Tree.index()] {
            if let Some(parent) = placement.parent {
                if layout.tree_placement(parent).is_none() {
                    return Err(LayoutError::ParentNotInTree(parent));
                }
                if layout.is_under(parent, placement.id) {
                    return Err(LayoutError::ParentCycle {
                        placement: placement.id,
                        parent,
                    });
                }
            }
        }
        Ok(layout)
    }

    /// This layout with each block's anchor replaced by what `moved` gives for it: the anchors
    /// after a structural edit, `None` for an anchor whose cells it deleted (L3, dc-m3u).
    pub(crate) fn with_anchors(&self, moved: impl Fn(GridArea) -> Option<GridArea>) -> SkinLayout {
        let mut layout = self.clone();
        for block in &mut layout.blocks {
            block.anchor = block.anchor.and_then(&moved);
        }
        layout
    }

    /// The blocks, in the order they were created.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|block| block.id == id)
    }

    /// One view's placements, in its order.
    pub fn view(&self, view: ViewKind) -> &[Placement] {
        &self.views[view.index()]
    }

    /// The placement with this ID, in whichever view holds it.
    pub fn placement(&self, id: PlacementId) -> Option<&Placement> {
        self.views
            .iter()
            .flatten()
            .find(|placement| placement.id == id)
    }

    /// Whether the layout holds no block (and so no placement).
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    fn tree_placement(&self, id: PlacementId) -> Option<&Placement> {
        self.view(ViewKind::Tree)
            .iter()
            .find(|placement| placement.id == id)
    }

    /// Whether the tree placement `id` is `ancestor` or lies under it. Each step follows a parent,
    /// and a walk longer than the tree has placements has met a cycle, which counts as under.
    fn is_under(&self, id: PlacementId, ancestor: PlacementId) -> bool {
        let mut at = Some(id);
        for _ in 0..=self.view(ViewKind::Tree).len() {
            match at {
                None => return false,
                Some(current) if current == ancestor => return true,
                Some(current) => at = self.tree_placement(current).and_then(|p| p.parent),
            }
        }
        true
    }

    fn check_new_block(&self, id: BlockId, locator: &LocatorName) -> Result<(), LayoutError> {
        if self.block(id).is_some() {
            return Err(LayoutError::DuplicateBlock(id));
        }
        if let Some(other) = self.blocks.iter().find(|b| b.locator.is(locator.as_str())) {
            return Err(LayoutError::LocatorInUse {
                block: id,
                other: other.id,
            });
        }
        Ok(())
    }

    fn view_of(&self, id: PlacementId) -> Result<ViewKind, LayoutError> {
        self.placement(id)
            .map(Placement::view)
            .ok_or(LayoutError::UnknownPlacement(id))
    }

    /// Inserts `placement` into its view at `position`: the 1-based place among the view's
    /// placements, or, in the tree, among the children of its parent.
    fn insert(&mut self, placement: Placement, position: NonZeroU32) -> Result<(), LayoutError> {
        let view = placement.view;
        if let Some(parent) = placement.parent
            && self.tree_placement(parent).is_none()
        {
            return Err(LayoutError::ParentNotInTree(parent));
        }
        let list = &mut self.views[view.index()];
        // The list indices of the placements the position counts among.
        let peers: Vec<usize> = list
            .iter()
            .enumerate()
            .filter(|(_, other)| view != ViewKind::Tree || other.parent == placement.parent)
            .map(|(index, _)| index)
            .collect();
        let place = position.get() as usize;
        let last = peers.len() + 1;
        if place > last {
            return Err(LayoutError::PositionPastEnd {
                position,
                last: u32::try_from(last).unwrap_or(u32::MAX),
            });
        }
        let at = peers.get(place - 1).copied().unwrap_or(list.len());
        list.insert(at, placement);
        Ok(())
    }

    /// Takes the placement `id` out of its view.
    fn take(&mut self, id: PlacementId) -> Result<Placement, LayoutError> {
        let view = self.view_of(id)?;
        let list = &mut self.views[view.index()];
        let index = list
            .iter()
            .position(|placement| placement.id == id)
            .expect("the placement is in its view");
        Ok(list.remove(index))
    }

    /// Applies one edit. On a refusal the layout may be partly changed; the caller applies a
    /// batch to a copy and drops the copy when any edit is refused.
    pub(crate) fn edit(&mut self, edit: &SkinLayoutEdit) -> Result<(), LayoutError> {
        match *edit {
            SkinLayoutEdit::CreateBlock {
                block,
                kind,
                anchor,
            } => {
                let locator = LocatorName::for_block(block);
                self.check_new_block(block, &locator)?;
                self.blocks
                    .push(Block::new(block, kind, locator, Some(anchor)));
            }
            SkinLayoutEdit::RemoveBlock { block } => {
                let index = self
                    .blocks
                    .iter()
                    .position(|b| b.id == block)
                    .ok_or(LayoutError::UnknownBlock(block))?;
                if self.views.iter().flatten().any(|p| p.block == block) {
                    return Err(LayoutError::BlockPlaced(block));
                }
                self.blocks.remove(index);
            }
            SkinLayoutEdit::AddPlacement {
                placement,
                block,
                spot,
            } => {
                if self.placement(placement).is_some() {
                    return Err(LayoutError::DuplicatePlacement(placement));
                }
                if self.block(block).is_none() {
                    return Err(LayoutError::UnknownBlock(block));
                }
                self.insert(spot.placement(placement, block), spot.position())?;
            }
            SkinLayoutEdit::MovePlacement { placement, spot } => {
                let view = self.view_of(placement)?;
                if spot.view() != view {
                    return Err(LayoutError::OtherView { placement, view });
                }
                if let Spot::Tree {
                    parent: Some(parent),
                    ..
                } = spot
                {
                    if self.tree_placement(parent).is_none() {
                        return Err(LayoutError::ParentNotInTree(parent));
                    }
                    if self.is_under(parent, placement) {
                        return Err(LayoutError::ParentCycle { placement, parent });
                    }
                }
                let moved = self.take(placement)?;
                self.insert(spot.placement(placement, moved.block), spot.position())?;
            }
            SkinLayoutEdit::RemovePlacement { placement } => {
                self.view_of(placement)?;
                let has_children = self
                    .view(ViewKind::Tree)
                    .iter()
                    .any(|p| p.parent == Some(placement));
                if has_children {
                    return Err(LayoutError::HasChildren(placement));
                }
                self.take(placement)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ids_have_one_text() {
        let id = BlockId::new(0x0123_4567_89ab_cdef_0011_2233_4455_6677).unwrap();
        assert_eq!(id.to_string(), "01234567-89ab-cdef-0011-223344556677");
        assert_eq!(BlockId::parse(&id.to_string()), Some(id));
        for text in [
            "01234567-89AB-cdef-0011-223344556677",
            "0123456789abcdef0011223344556677",
            "{01234567-89ab-cdef-0011-223344556677}",
            "01234567-89ab-cdef-0011-22334455667",
            "00000000-0000-0000-0000-000000000000",
            "01234567_89ab-cdef-0011-223344556677",
        ] {
            assert_eq!(BlockId::parse(text), None, "{text}");
        }
        assert_eq!(
            LocatorName::for_block(id).as_str(),
            "_dna.block.0123456789abcdef0011223344556677"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn locator_names() {
        for good in ["_dna.block.1", "Locator", "a.b_c", "R1C1x", "ABCD1", "RC_"] {
            assert!(LocatorName::new(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "1a",
            ".a",
            "a b",
            "a-b",
            "A1",
            "xfd1048576",
            "R",
            "c",
            "R1C1",
            "r12",
            "RC",
            "C7",
            "é",
        ] {
            assert!(LocatorName::new(bad).is_none(), "{bad}");
        }
        assert!(LocatorName::new(&"a".repeat(255)).is_some());
        assert!(LocatorName::new(&"a".repeat(256)).is_none());
    }
}
