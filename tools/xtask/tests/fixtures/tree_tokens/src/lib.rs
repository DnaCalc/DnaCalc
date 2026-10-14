// Fixture for check (d). Plain comments are not tokens, so this tree reference is not found.

/// A doc comment naming a tree reference.
pub struct Holder;
pub struct TreeRef;

pub const TREE_REF: u8 = 0;

pub struct StrictReferenceProfile;

pub struct ExcelReferenceSystemProvider;

pub fn ids() -> u8 {
    let profile_id = 1;
    1
}
pub const NOTE: &str = "a Host-Reference grammar";

pub fn texts() -> &'static str {
    "tree-references"
}

// Words that only resemble denied phrases: street references, tree refresh, profile identity.
pub fn street_reference_tree_refresh_profile_identity() {}

mod profiles;

#[cfg(test)]
mod tests {
    pub struct TreeRefInTests;
}

pub const INCLUDED: &str = include_str!("profiles.rs");
