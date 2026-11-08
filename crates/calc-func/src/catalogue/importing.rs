//! IMPORTCSV and IMPORTTEXT as the catalogue holds them: what Excel gives when the file cannot be
//! read (#VALUE!), for a host with no [`crate::ImportRoute`]. A host with a route binds these
//! names to [`crate::ImportFunction`] instead; the rules and their measurements are in
//! [`crate::import`].

use super::kit::fail;
use crate::value::WorksheetErrorCode;

const CODE: u16 = 326;

ported_family! {
    code: CODE;
    1 importcsv_fn "IMPORTCSV" (1, 5) [V "path", V "skip_rows", V "take_rows", V "text_only", V "locale"] import_route (crate::IMPORT_PROVIDER, crate::ImportKind::Csv);
    |s, a, sv| { fail(WorksheetErrorCode::Value) }
    2 importtext_fn "IMPORTTEXT" (1, 7) [V "path", V "delimiter", V "skip_rows", V "take_rows", V "encoding", V "text_only", V "locale"] import_route (crate::IMPORT_PROVIDER, crate::ImportKind::Text);
    |s, a, sv| { fail(WorksheetErrorCode::Value) }
}
