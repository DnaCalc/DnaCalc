//! Identity of a call spelling at a formula-entry or workbook-file boundary.
use crate::{FilePrefix, FunctionId, Notation};

/// Excel's file spelling for the linked-data field accessor (dc-ifc, Excel 16.0.20430).
pub const FIELD_ACCESS_FILE_NAME: &str = "_FV";
/// Stable identity of the worksheet field accessor.
pub const FIELD_ACCESS_ID: FunctionId = FunctionId::new(crate::ported::host_facts::CODE, 9);

/// The invariant worksheet spelling for the field accessor.
pub const fn field_access_name() -> &'static str {
    match crate::declaration(FIELD_ACCESS_ID) {
        Some(entry) => entry.spelling(),
        None => panic!("field accessor declaration is missing"),
    }
}

/// Where the call spelling was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallOrigin {
    FormulaEntry,
    LoadedFile,
}

/// The identity established from a spelling and its origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCallIdentity<'a> {
    BuiltIn { id: FunctionId, prefix: FilePrefix, origin: CallOrigin },
    ForcedUserDefined { spelling: &'a str, origin: CallOrigin },
    Other { spelling: &'a str, origin: CallOrigin },
}

/// Strip an ASCII file prefix in any ASCII case, preserving the original remainder.
pub fn strip_ascii_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len()).filter(|start| start.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

/// Remove a future-function prefix and its optional worksheet component.
pub fn file_unprefixed(spelling: &str) -> &str {
    match strip_ascii_prefix(spelling, crate::FUTURE_FUNCTION_PREFIX) {
        Some(rest) => strip_ascii_prefix(rest, crate::WORKSHEET_FUNCTION_PREFIX).unwrap_or(rest),
        None => spelling,
    }
}

/// Classify the observed spelling without turning an unrecognised name into a built-in.
/// A loaded future function lacking its measured file prefix remains an ordinary name.
pub fn file_call_identity(spelling: &str, origin: CallOrigin) -> FileCallIdentity<'_> {
    let other = || FileCallIdentity::Other { spelling, origin };
    if origin == CallOrigin::LoadedFile {
        if let Some(name) = strip_ascii_prefix(spelling, crate::USER_DEFINED_FUNCTION_PREFIX) {
            return FileCallIdentity::ForcedUserDefined { spelling: name, origin };
        }
    }
    let (base, prefix) = match strip_ascii_prefix(spelling, crate::FUTURE_FUNCTION_PREFIX) {
        Some(rest) => match strip_ascii_prefix(rest, crate::WORKSHEET_FUNCTION_PREFIX) {
            Some(name) => (name, FilePrefix::XlfnXlws),
            None => (rest, FilePrefix::Xlfn),
        },
        None => (spelling, FilePrefix::Bare),
    };
    if prefix == FilePrefix::Bare && base.eq_ignore_ascii_case(FIELD_ACCESS_FILE_NAME) {
        return FileCallIdentity::BuiltIn { id: FIELD_ACCESS_ID, prefix, origin };
    }
    let Some(entry) = crate::declarations().find(|entry| {
        entry.notation() == Notation::Call && entry.spelling().eq_ignore_ascii_case(base)
    }) else { return other(); };
    if origin == CallOrigin::LoadedFile && prefix == FilePrefix::Bare
        && matches!(entry.file_prefix(), Some(FilePrefix::Xlfn | FilePrefix::XlfnXlws)) {
        return other();
    }
    FileCallIdentity::BuiltIn { id: entry.id(), prefix, origin }
}

#[cfg(test)]
mod tests {
    use super::{CallOrigin, FileCallIdentity, file_call_identity};
    use crate::{FilePrefix, FunctionId};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn loaded_and_entered_names_keep_their_origin_and_identity() {
        let sum = crate::named("SUM").unwrap().id();
        let maxifs = crate::named("MAXIFS").unwrap().id();
        assert_eq!(file_call_identity("SUM", CallOrigin::LoadedFile),
            FileCallIdentity::BuiltIn { id: sum, prefix: FilePrefix::Bare, origin: CallOrigin::LoadedFile });
        assert_eq!(file_call_identity("_XLUDF.SUM", CallOrigin::LoadedFile),
            FileCallIdentity::ForcedUserDefined { spelling: "SUM", origin: CallOrigin::LoadedFile });
        assert_eq!(file_call_identity("_xludf.SUM", CallOrigin::FormulaEntry),
            FileCallIdentity::Other { spelling: "_xludf.SUM", origin: CallOrigin::FormulaEntry });
        assert_eq!(file_call_identity("MAXIFS", CallOrigin::LoadedFile),
            FileCallIdentity::Other { spelling: "MAXIFS", origin: CallOrigin::LoadedFile });
        assert_eq!(file_call_identity("MAXIFS", CallOrigin::FormulaEntry),
            FileCallIdentity::BuiltIn { id: maxifs, prefix: FilePrefix::Bare, origin: CallOrigin::FormulaEntry });
        assert_eq!(file_call_identity("_FV", CallOrigin::LoadedFile),
            FileCallIdentity::BuiltIn { id: crate::FIELD_ACCESS_ID, prefix: FilePrefix::Bare, origin: CallOrigin::LoadedFile });
        assert_eq!(file_call_identity("_FV", CallOrigin::FormulaEntry),
            FileCallIdentity::BuiltIn { id: crate::FIELD_ACCESS_ID, prefix: FilePrefix::Bare, origin: CallOrigin::FormulaEntry });
        assert_eq!(file_call_identity("_xlfn._FV", CallOrigin::LoadedFile),
            FileCallIdentity::Other { spelling: "_xlfn._FV", origin: CallOrigin::LoadedFile });
        assert_eq!(file_call_identity("_XLFN.MAXIFS", CallOrigin::LoadedFile),
            FileCallIdentity::BuiltIn { id: maxifs, prefix: FilePrefix::Xlfn, origin: CallOrigin::LoadedFile });
        assert!(crate::declaration(FunctionId::from_raw(u32::MAX)).is_none());
    }
}
