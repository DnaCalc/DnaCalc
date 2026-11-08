//! WEBSERVICE and IMAGE as the catalogue holds them: what Excel gives when the fetch fails, for a
//! host with no [`crate::FetchRoute`]. A host with a route binds these names to
//! [`crate::FetchFunction`] instead; the rules and their measurements are in [`crate::fetch`].

use super::kit::fail;
use crate::fetch::{Prepared, prepare_picture, prepare_text};
use crate::kernel::Step;
use crate::value::{CalcValue, WorksheetErrorCode};

const CODE: u16 = 323;

fn without_route(prepared: Prepared) -> Step<CalcValue> {
    match prepared {
        Prepared::Answer(value) => Ok(value),
        Prepared::Fetch(..) => fail(WorksheetErrorCode::Value),
    }
}

ported_family! {
    code: CODE;
    1 webservice_fn "WEBSERVICE" (1, 1) [V "url"] volatile true; host_route (crate::FETCH_PROVIDER, crate::FetchKind::Text); |s, a, sv| { without_route(prepare_text(&a[0])) }
    2 image_fn "IMAGE" (1, 5) [V "source", V "alt_text", V "sizing", V "height", V "width"] host_route (crate::FETCH_PROVIDER, crate::FetchKind::Picture);
    |s, a, sv| { without_route(prepare_picture(a)) }
}
