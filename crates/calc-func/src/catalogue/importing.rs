//! IMPORTCSV and IMPORTTEXT as the catalogue holds them: what Excel gives when the file cannot be
//! read (#VALUE!), for a host with no [`crate::ImportRoute`]. A host with a route binds these
//! names to [`crate::ImportFunction`] instead; the rules and their measurements are in
//! [`crate::import`].

use super::kit::fail;
use crate::kernel::Step;
use crate::value::{CalcValue, WorksheetErrorCode};
use crate::ImportKind;

use calc_catalogue::ported::importing::{CODE, ENTRIES as DECLARATIONS};

fn no_route(arguments: &[CalcValue], kind: ImportKind) -> Step<CalcValue> {
    if let Some(answer) = crate::import::scalar_preflight(kind, arguments) {
        return Ok(answer);
    }
    fail(WorksheetErrorCode::Value)
}

ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 importcsv_fn |s, a, sv| { no_route(a, ImportKind::Csv) }
    2 importtext_fn |s, a, sv| { no_route(a, ImportKind::Text) }
}
