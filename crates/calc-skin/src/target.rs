use crate::grid::{Area, Key, Sheet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::num::NonZeroU128;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub NonZeroU128);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let n = self.0.get();
                write!(
                    f,
                    "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                    n >> 96,
                    (n >> 80) & 0xffff,
                    (n >> 64) & 0xffff,
                    (n >> 48) & 0xffff,
                    n & 0xffff_ffff_ffff
                )
            }
        }

        impl std::str::FromStr for $name {
            type Err = InvalidId;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let groups: Vec<_> = text.split('-').collect();
                if groups.iter().map(|group| group.len()).ne([8, 4, 4, 4, 12])
                    || !groups
                        .iter()
                        .all(|group| group.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Err(InvalidId);
                }
                u128::from_str_radix(&groups.concat(), 16)
                    .ok()
                    .and_then(NonZeroU128::new)
                    .map(Self)
                    .ok_or(InvalidId)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidId;

impl std::fmt::Display for InvalidId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("expected a nonzero identifier in UUID notation")
    }
}

impl std::error::Error for InvalidId {}

stable_id!(AnchorId);
stable_id!(DocId);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameScope {
    Workbook,
    Sheet(Sheet),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TablePart {
    All,
    Data,
    Headers,
    Totals,
    Columns { first: String, last: String },
}

/// An Excel workbook entity. Session sheet identities can be checked against a host's summary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceRef {
    Cells { sheet: Sheet, areas: Vec<Area> },
    Spill { anchor: Key },
    Name { scope: NameScope, name: String },
    Table { name: String, part: TablePart },
    Object { sheet: Sheet, name: String },
    Sheet(Sheet),
    Workbook,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewItemRef {
    pub view: ViewId,
    pub doc: DocId,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Source(SourceRef),
    Anchor(AnchorId),
    Item(ViewItemRef),
}

/// Both identities must resolve to the same source before a binding can write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub anchor: AnchorId,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub seq: u64,
    pub target: SourceRef,
    pub active: Option<Key>,
    pub grouped: Vec<Sheet>,
    pub item: Option<ViewItemRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionCause {
    ClientEcho { client_seq: u64 },
    Macro { procedure: String },
    Agent { agent: String },
    History,
    Open,
}
