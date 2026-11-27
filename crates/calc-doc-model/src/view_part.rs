//! Authored view metadata. Sheet identities are mapped to stable keys by the package codec.

use calc_syntax::AnchorId;
use calc_value::GridArea;
use std::{collections::BTreeMap, sync::Arc};

pub const VIEW_PART_NAMESPACE: &str = "urn:dnacalc:views:v1";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewAnchor {
    pub id: AnchorId,
    pub spill: bool,
    /// An orphan has no live location. It can be rebound by using the same ID in an edit.
    location: Option<GridArea>,
}

impl ViewAnchor {
    pub fn cells(id: AnchorId, area: GridArea) -> Self {
        Self {
            id,
            spill: false,
            location: Some(area),
        }
    }

    pub fn spill(id: AnchorId, at: calc_value::CellKey) -> Self {
        Self {
            id,
            spill: true,
            location: Some(GridArea::cell(at)),
        }
    }

    pub fn orphan(id: AnchorId, spill: bool) -> Self {
        Self {
            id,
            spill,
            location: None,
        }
    }

    pub fn area(&self) -> Option<GridArea> {
        self.location
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ViewPart {
    source: Option<Arc<[u8]>>,
    opaque: bool,
    anchors: BTreeMap<AnchorId, ViewAnchor>,
}

impl ViewPart {
    pub fn loaded(source: Vec<u8>, anchors: impl IntoIterator<Item = ViewAnchor>) -> Self {
        Self {
            source: Some(source.into()),
            opaque: false,
            anchors: anchors
                .into_iter()
                .map(|anchor| (anchor.id, anchor))
                .collect(),
        }
    }

    pub fn opaque(source: Vec<u8>) -> Self {
        Self {
            source: Some(source.into()),
            opaque: true,
            anchors: BTreeMap::new(),
        }
    }

    pub fn is_opaque(&self) -> bool {
        self.opaque
    }

    pub fn source(&self) -> Option<&[u8]> {
        self.source.as_deref()
    }

    pub fn anchor(&self, id: AnchorId) -> Option<&ViewAnchor> {
        self.anchors.get(&id)
    }

    pub fn anchors(&self) -> impl Iterator<Item = &ViewAnchor> {
        self.anchors.values()
    }

    pub fn with_anchor(mut self, anchor: ViewAnchor) -> Self {
        self.anchors.insert(anchor.id, anchor);
        self
    }

    pub fn without_anchor(mut self, id: AnchorId) -> Self {
        self.anchors.remove(&id);
        self
    }

    pub fn mapped(&self, mut map: impl FnMut(GridArea, bool) -> Option<GridArea>) -> Option<Self> {
        let mut next = None;
        for anchor in self.anchors() {
            if let Some(area) = anchor.location {
                let location = map(area, anchor.spill);
                if location != Some(area) {
                    let mapped = ViewAnchor {
                        location,
                        ..anchor.clone()
                    };
                    next.get_or_insert_with(|| self.clone())
                        .anchors
                        .insert(anchor.id, mapped);
                }
            }
        }
        next
    }
}
