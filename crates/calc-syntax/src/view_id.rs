//! Stable identifiers shared by the authored view part and skin protocol.

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
