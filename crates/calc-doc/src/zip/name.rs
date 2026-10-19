//! Entry names that could be read as paths outside the package.

use crate::error::UnsafeName;

/// Checks one entry name. A name ending in `/` is a directory entry; its other segments follow
/// the same rules.
pub(crate) fn check(name: &str) -> Result<(), UnsafeName> {
    if name.is_empty() {
        return Err(UnsafeName::Empty);
    }
    if name.starts_with('/') {
        return Err(UnsafeName::Absolute);
    }
    if name.contains('\\') {
        return Err(UnsafeName::Backslash);
    }
    if name.contains(':') {
        return Err(UnsafeName::Colon);
    }
    if name.chars().any(|c| c < '\u{20}' || c == '\u{7f}') {
        return Err(UnsafeName::ControlCharacter);
    }
    let body = name.strip_suffix('/').unwrap_or(name);
    for segment in body.split('/') {
        if segment.is_empty() {
            return Err(UnsafeName::EmptySegment);
        }
        if segment == "." || segment == ".." {
            return Err(UnsafeName::DotSegment);
        }
    }
    Ok(())
}
