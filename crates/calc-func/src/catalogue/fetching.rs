//! WEBSERVICE and IMAGE as the catalogue holds them: what Excel gives when the fetch fails, for a
//! host with no [`crate::FetchRoute`]. A host with a route binds these names to
//! [`crate::FetchFunction`] instead; the rules and their measurements are in [`crate::fetch`].

use super::kit::fail;
use crate::fetch::{Prepared, prepare_picture, prepare_text};
use crate::kernel::Step;
use crate::value::{CalcValue, WorksheetErrorCode};

use calc_catalogue::ported::fetching::{CODE, ENTRIES as DECLARATIONS};

fn without_route(prepared: Prepared) -> Step<CalcValue> {
    match prepared {
        Prepared::Answer(value) => Ok(value),
        Prepared::Fetch(..) => fail(WorksheetErrorCode::Value),
    }
}

ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 webservice_fn |s, a, sv| { without_route(prepare_text(&a[0])) }
    2 image_fn |s, a, sv| { without_route(prepare_picture(a)) }
}
