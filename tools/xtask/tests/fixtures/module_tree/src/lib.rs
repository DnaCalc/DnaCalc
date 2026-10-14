//! Fixture for the module tree walk: which files and items count as non-test sources.

mod nested;

#[path = "renamed.rs"]
mod by_path;

// A test-only module declaration is never opened: its file does not exist.
#[cfg(test)]
mod tests_only;

pub fn kept_in_release() {}

#[cfg(test)]
pub fn only_in_tests() {}

#[test]
fn test_helper() {}

#[cfg(all(test, feature = "extra"))]
pub fn all_with_test() {}

#[cfg(any(test, test))]
pub fn any_of_tests() {}

#[cfg(any(test, feature = "extra"))]
pub fn test_or_feature() {}
