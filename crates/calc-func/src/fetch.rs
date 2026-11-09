//! The small route by which WEBSERVICE and IMAGE reach the URL a workbook names.
//!
//! calc-func does no network itself. A host that can fetch implements [`FetchRoute`] and registers
//! a [`FetchFunction`] for each function ([`FETCH_FUNCTIONS`]) under [`FETCH_PROVIDER`]. The call
//! then runs on the existing async user-function path: the activation asks the route for one URL,
//! parks on `Awaiting`, and the host delivers the reply through the ordinary async token, as
//! [`FetchReply::into_value`] turns it into a value. A host with no route calculates the catalogue
//! entries instead, which give what Excel gives when the fetch fails.
//!
//! Measured in Excel 16.0.20527 (64-bit) on 2026-11-06 and 2026-11-07, against a local mock server
//! and public throwaway URLs:
//!
//! WEBSERVICE (`_xlfn.WEBSERVICE`, one argument, refused at entry otherwise):
//! - It is volatile: Calculate, CalculateFull and opening the file each fetch again, and a cell
//!   holds its last text in the file as `t="str"` with no volatile mark. Excel blocks while it
//!   fetches; here the cell waits on the async path.
//! - An error argument is that error. A number, a logical or a blank cell is #VALUE!, as is empty
//!   text. An array or a range lifts and spills, one fetch per element.
//! - A URL of more than 2048 characters is #VALUE! with no request. Spaces round it go. `http` and
//!   `https` in any case are fetched, `host:port` with no scheme is `http`, and every other form
//!   is #VALUE!: another scheme, `example.org/x`, `//host/x`, `http:host/x`, `data:`, `about:`.
//! - The request is a GET whose path sends space as `%20`, `#` as `%23` and other non-ASCII as
//!   UTF-8 percent escapes ([`wire_url`]).
//! - HTTP status 400 and above, and any failure to connect, resolve or agree TLS, is #VALUE!, as
//!   are too many redirects (25) and a slow reply (2 s answered, 8 s did not; 4 to 7 s varied
//!   within one calculation). Status 204 and an empty body are empty text. A redirect from `https`
//!   to `http` is not followed, and its empty body is the result. A body over 32767 UTF-16 code
//!   units is #VALUE!. A compressed body is not decompressed.
//! - The body is UTF-8 whatever the charset header says, read leniently ([`decode_body`]): a lead
//!   byte takes its count of following bytes unchecked (`E9 6C 6C` is U+9B2C, `C0 80` is NUL, and
//!   four bytes give two UTF-16 units up to `F7 BF BF BF`), NUL is kept, a byte that cannot lead
//!   is dropped, an unfinished sequence at the end is dropped, and `EF BB BF` at the start goes. A
//!   body starting `FF FE` or `FE FF`, or one with nothing left, is #VALUE!.
//!   Not reproduced: some bodies of binary bytes (a PNG, a `deflate` body, the bytes 00 to FF in
//!   order) are #VALUE! in Excel and text here; their parts alone decode by these rules.
//!
//! IMAGE (`_xlfn.IMAGE`, one to five arguments):
//! - The value shows the picture, in a cell or spilled, and flows as a value through `=A1`, IF,
//!   IFERROR, IFNA, INDEX, CHOOSE, LET, XLOOKUP, VLOOKUP, SORTBY, FILTER, HSTACK. Read as a worksheet
//!   value it is #VALUE!: arithmetic, `&`, LEN, TEXT, SUM, AND, T and N give #VALUE!, COUNT counts 0
//!   and COUNTA 1. It is no error for the IS functions (ISERROR FALSE, ISNONTEXT TRUE), TYPE is
//!   128 and ERROR.TYPE #N/A. `=` compares source and alt text (sizing is ignored); `<` is #VALUE!.
//!   A file holds it as `t="e"` with a `_webimage` rich value.
//! - An error at source or alt_text is that error. Source must be `https`, in any case and of any
//!   length (8192 measured): `http` is #BLOCKED! with no fetch; another scheme, no scheme, empty,
//!   a number, a logical, a blank, a picture are #VALUE!. An `http` redirect target is followed.
//! - alt_text is text; a number or logical reads as its text; empty is none. sizing reads
//!   logicals and numeric text as numbers and truncates: 0 fit, 1 fill, 2 original, 3 custom, and
//!   others are #VALUE!. Any height or width argument with sizing other than 3 is #VALUE!, even
//!   an empty one. Custom needs one of them above 0 and neither below (`3,20` and `3,,20` are good).
//! - HTTP status 400 to 499 is #CONNECT!. A failure to connect, status 500 and above, status 204,
//!   and a body that is no picture are #VALUE!. The content type does not matter; the bytes do.
//!   PNG, JPEG, GIF, WEBP, ICO, BMP and TIFF show, served as anything (a GIF sent as text/html
//!   shows); SVG is #VALUE!.
//! - A good cell does not fetch again on Calculate or CalculateFull (no #BUSY! shows); a saved
//!   picture shows on reopening with no fetch. Excel stores PNG, JPEG and TIFF bytes as fetched and
//!   converts WEBP, BMP and ICO to PNG; here the bytes stay as fetched.

use crate::udf::{
    UserFunction, UserFunctionContext, UserFunctionInvocation, UserFunctionStep,
    UserInvocationFrame,
};
use crate::value::{
    ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, ImageFormat, ImageValue,
    WorksheetErrorCode,
};
use std::{fmt, rc::Rc, sync::Arc};

/// The provider key the fetching functions are registered under in a callable table.
pub use calc_catalogue::{FETCH_PROVIDER, FetchKind};
/// The functions that use the route, by catalogue spelling.
pub const FETCH_FUNCTIONS: [&str; 2] = [
    calc_catalogue::ported::find(calc_catalogue::ported::fetching::ENTRIES,
        calc_catalogue::FunctionId::new(calc_catalogue::ported::fetching::CODE, 1)).spelling,
    calc_catalogue::ported::find(calc_catalogue::ported::fetching::ENTRIES,
        calc_catalogue::FunctionId::new(calc_catalogue::ported::fetching::CODE, 2)).spelling,
];

/// The most characters of a URL Excel requests.
const URL_LIMIT: usize = 2048;
/// The most UTF-16 code units of a worksheet text.
const TEXT_LIMIT: usize = 32767;

/// One GET of one URL, as it goes on the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    pub kind: FetchKind,
    pub url: String,
}

/// What a route learned from one fetch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchReply {
    /// A final status below 400, redirects followed, and its body.
    Body(Vec<u8>),
    /// A final status of 400 or above.
    Status(u16),
    /// No response: connection, name resolution, TLS, a scheme the host cannot use, or a timeout.
    Failed,
}

impl FetchReply {
    /// The value the host delivers through the request's async token.
    pub fn into_value(self, kind: FetchKind) -> CalcValue {
        match (kind, self) {
            (FetchKind::Text, FetchReply::Body(body)) => decode_body(&body).map_or_else(
                || CalcValue::error(WorksheetErrorCode::Value),
                |text| CalcValue::text(ExcelText::from_utf16_code_units(text)),
            ),
            (FetchKind::Text, _) => CalcValue::error(WorksheetErrorCode::Value),
            (FetchKind::Picture, FetchReply::Body(body)) => match ImageFormat::sniff(&body) {
                Some(format) => CalcValue::image(ImageValue::picture(format, body.into())),
                None => CalcValue::error(WorksheetErrorCode::Value),
            },
            (FetchKind::Picture, FetchReply::Status(400..=499)) => {
                CalcValue::error(WorksheetErrorCode::Connect)
            }
            (FetchKind::Picture, FetchReply::Status(_)) => {
                CalcValue::error(WorksheetErrorCode::Value)
            }
            (FetchKind::Picture, FetchReply::Failed) => CalcValue::error(WorksheetErrorCode::Value),
        }
    }
}

/// A host's way to fetch. `begin` starts one fetch and names it; the host later finds the pending
/// async token with that request number and delivers [`FetchReply::into_value`] through it.
/// A route follows redirects to a limit of its own and sends no `Accept-Encoding`. For
/// [`FetchKind::Text`] it does not follow a redirect from `https` to `http`, replying with that
/// response's empty body; for [`FetchKind::Picture`] it does.
pub trait FetchRoute: fmt::Debug {
    fn begin(&self, request: FetchRequest) -> u64;
}

/// A response body as worksheet text, or `None` when Excel gives #VALUE!. See the module notes.
pub fn decode_body(body: &[u8]) -> Option<Vec<u16>> {
    if body.starts_with(&[0xff, 0xfe]) || body.starts_with(&[0xfe, 0xff]) {
        return None;
    }
    let bytes = body.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(body);
    let mut units = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let lead = bytes[at];
        let (count, bits) = match lead {
            0x00..=0x7f => (0, u32::from(lead)),
            0xc0..=0xdf => (1, u32::from(lead & 0x1f)),
            0xe0..=0xef => (2, u32::from(lead & 0x0f)),
            0xf0..=0xf7 => (3, u32::from(lead & 0x07)),
            _ => {
                at += 1;
                continue;
            }
        };
        let Some(following) = bytes.get(at + 1..at + 1 + count) else {
            break;
        };
        let code = following
            .iter()
            .fold(bits, |code, byte| (code << 6) | u32::from(byte & 0x3f));
        at += 1 + count;
        match u16::try_from(code) {
            Ok(unit) => units.push(unit),
            Err(_) => {
                let code = code - 0x10000;
                units.push(0xd800 | (code >> 10) as u16);
                units.push(0xdc00 | (code & 0x3ff) as u16);
            }
        }
        if units.len() > TEXT_LIMIT {
            return None;
        }
    }
    (units.len() > 0 || body.is_empty()).then_some(units)
}

/// The URL Excel puts on the wire: `http` when it names no scheme, space as `%20`, `#` as `%23`,
/// and non-ASCII before any `?` as UTF-8 percent escapes.
pub fn wire_url(url: &str) -> String {
    let url = url.trim_matches(' ');
    let mut out = String::with_capacity(url.len() + 8);
    if scheme(url).is_none() {
        out.push_str("http://");
    }
    let mut query = false;
    for c in url.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => {
                query = true;
                out.push(c);
            }
            c if c.is_ascii() || query => out.push(c),
            c => {
                let mut bytes = [0; 4];
                for byte in c.encode_utf8(&mut bytes).as_bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}

fn scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once("://")?;
    (!scheme.is_empty() && scheme.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(scheme)
}

/// Whether a URL with no scheme is `host:port`, the one form Excel fetches without a scheme.
fn names_a_port(url: &str) -> bool {
    url.split_once(':').is_some_and(|(host, rest)| {
        let port = rest.split('/').next().unwrap_or_default();
        !host.is_empty()
            && !host.contains('/')
            && !port.is_empty()
            && port.bytes().all(|b| b.is_ascii_digit())
    })
}

/// Redirects a route follows before the fetch fails (measured: 25).
pub const REDIRECT_LIMIT: usize = 25;

/// One response as a transport primitive returns it: a single GET with no redirect followed, no
/// `Accept-Encoding` sent and nothing decompressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawResponse {
    pub status: u16,
    /// The `Location` header, when the response carries one.
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// The absolute URL a `Location` names, relative to the URL that answered. `None` for a target
/// that is not `http` or `https`.
pub fn resolve_location(base: &str, location: &str) -> Option<String> {
    let location = location.trim();
    let location = location.split('#').next().unwrap_or_default();
    let base = base.split('#').next().unwrap_or_default();
    let (base_scheme, rest) = base.split_once("://")?;
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path_and_query) = rest.split_at(authority_end);
    let resolved = if let Some(found) = scheme(location) {
        if !(found.eq_ignore_ascii_case("http") || found.eq_ignore_ascii_case("https")) {
            return None;
        }
        location.to_owned()
    } else if let Some(network) = location.strip_prefix("//") {
        format!("{base_scheme}://{network}")
    } else if location.starts_with('/') {
        format!("{base_scheme}://{authority}{location}")
    } else if location.starts_with('?') {
        let path = path_and_query.split('?').next().unwrap_or_default();
        let path = if path.is_empty() { "/" } else { path };
        format!("{base_scheme}://{authority}{path}{location}")
    } else if location.is_empty() {
        format!("{base_scheme}://{authority}{path_and_query}")
    } else {
        let path = path_and_query.split('?').next().unwrap_or_default();
        let directory = path.rfind('/').map_or("/", |at| &path[..=at]);
        format!("{base_scheme}://{authority}{directory}{location}")
    };
    Some(resolved)
}

/// A fetch whose redirects the route follows by the measured rules, over a transport that does
/// one GET at a time: `get` returns `None` when no response came (connection, name resolution,
/// TLS, timeout). Redirect statuses 301, 302, 303, 307 and 308 with a `Location` are followed to
/// [`REDIRECT_LIMIT`]; more is a failed fetch. WEBSERVICE does not follow `https` to `http` and
/// replies with that response's empty body; IMAGE does. The same rules serve every host, so a
/// shell supplies only `get`.
pub fn fetch_following(
    kind: FetchKind,
    url: &str,
    get: &mut dyn FnMut(&str) -> Option<RawResponse>,
) -> FetchReply {
    let mut current = url.to_owned();
    for _ in 0..=REDIRECT_LIMIT {
        let Some(response) = get(&current) else {
            return FetchReply::Failed;
        };
        let redirect = matches!(response.status, 301 | 302 | 303 | 307 | 308);
        let Some(location) = response.location.as_deref().filter(|_| redirect) else {
            return if response.status >= 400 {
                FetchReply::Status(response.status)
            } else {
                FetchReply::Body(response.body)
            };
        };
        let Some(next) = resolve_location(&current, location) else {
            return FetchReply::Failed;
        };
        let downgrade = scheme(&current).is_some_and(|s| s.eq_ignore_ascii_case("https"))
            && scheme(&next).is_some_and(|s| s.eq_ignore_ascii_case("http"));
        if downgrade && kind == FetchKind::Text {
            return FetchReply::Body(Vec::new());
        }
        current = next;
    }
    FetchReply::Failed
}

/// What one element of a call does: answer at once, or fetch and then finish with its value.
pub(crate) enum Prepared {
    Answer(CalcValue),
    Fetch(FetchRequest, Option<ImageValue>),
}

fn value_error() -> CalcValue {
    CalcValue::error(WorksheetErrorCode::Value)
}

/// WEBSERVICE's one element.
pub(crate) fn prepare_text(url: &CalcValue) -> Prepared {
    let text = match url.core_ref() {
        CoreRef::Error(_) if url.image_value().is_none() => return Prepared::Answer(url.clone()),
        CoreRef::Text(text) => text.to_string_lossy(),
        _ => return Prepared::Answer(value_error()),
    };
    let trimmed = text.trim_matches(' ');
    let allowed = match scheme(trimmed) {
        Some(scheme) => {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        }
        None => names_a_port(trimmed),
    };
    if text.is_empty() || text.chars().count() > URL_LIMIT || !allowed {
        return Prepared::Answer(value_error());
    }
    Prepared::Fetch(
        FetchRequest {
            kind: FetchKind::Text,
            url: wire_url(&text),
        },
        None,
    )
}

/// A number argument of IMAGE: a logical is 1 or 0 and numeric text reads as its number. `None`
/// for an argument the call leaves empty or a blank cell.
fn number_of(value: &CalcValue) -> Result<Option<f64>, CalcValue> {
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Ok(None),
        CoreRef::Number(number) => Ok(Some(number)),
        CoreRef::Logical(logical) => Ok(Some(f64::from(u8::from(logical)))),
        CoreRef::Text(text) => match text.to_string_lossy().trim().parse() {
            Ok(number) => Ok(Some(number)),
            Err(_) => Err(value_error()),
        },
        CoreRef::Error(_) if value.image_value().is_none() => Err(value.clone()),
        _ => Err(value_error()),
    }
}

/// IMAGE's one element: source, alt_text, sizing, height, width.
pub(crate) fn prepare_picture(arguments: &[CalcValue]) -> Prepared {
    let empty = CalcValue::missing();
    let at = |index: usize| arguments.get(index).unwrap_or(&empty);
    let source = at(0);
    let url = match source.core_ref() {
        CoreRef::Error(_) if source.image_value().is_none() => {
            return Prepared::Answer(source.clone());
        }
        CoreRef::Text(text) => text.to_string_lossy(),
        _ => return Prepared::Answer(value_error()),
    };
    let alt_text = match at(1).core_ref() {
        CoreRef::Missing | CoreRef::Empty => None,
        CoreRef::Text(text) if text.utf16_code_units().is_empty() => None,
        CoreRef::Text(text) => Some(text.to_string_lossy()),
        CoreRef::Number(number) => Some(format!("{number}")),
        CoreRef::Logical(logical) => Some(if logical { "TRUE" } else { "FALSE" }.to_owned()),
        CoreRef::Error(_) if at(1).image_value().is_none() => {
            return Prepared::Answer(at(1).clone());
        }
        _ => return Prepared::Answer(value_error()),
    };
    let numbers = (number_of(at(2)), number_of(at(3)), number_of(at(4)));
    let (sizing, height, width) = match numbers {
        (Ok(sizing), Ok(height), Ok(width)) => (sizing.unwrap_or(0.0).trunc(), height, width),
        (Err(answer), _, _) | (_, Err(answer), _) | (_, _, Err(answer)) => {
            return Prepared::Answer(answer);
        }
    };
    if !matches!(sizing, 0.0..=3.0) {
        return Prepared::Answer(value_error());
    }
    // Sizes need custom sizing, even a size left empty. Custom needs one above 0 and none below.
    let (given_height, given_width) = (arguments.len() > 3, arguments.len() > 4);
    let custom = sizing == 3.0;
    let (h, w) = (height.unwrap_or(0.0), width.unwrap_or(0.0));
    let sizes_fit = if custom {
        h >= 0.0 && w >= 0.0 && (h > 0.0 || w > 0.0)
    } else {
        !given_height
    };
    if !sizes_fit {
        return Prepared::Answer(value_error());
    }
    let scheme = scheme(&url).map(str::to_ascii_lowercase);
    match scheme.as_deref() {
        Some("https") => {}
        Some("http") => return Prepared::Answer(CalcValue::error(WorksheetErrorCode::Blocked)),
        _ => return Prepared::Answer(value_error()),
    }
    let image = ImageValue {
        source: url.clone(),
        alt_text,
        sizing: sizing as u8,
        height: given_height.then_some(h),
        width: given_width.then_some(w),
        format: ImageFormat::Png,
        bytes: Arc::from(Vec::new()),
    };
    Prepared::Fetch(
        FetchRequest {
            kind: FetchKind::Picture,
            url: wire_url(&url),
        },
        Some(image),
    )
}

/// One of the fetching functions as a host registers it, bound to the host's route.
#[derive(Debug)]
pub struct FetchFunction {
    kind: FetchKind,
    route: Rc<dyn FetchRoute>,
}

impl FetchFunction {
    /// The function of [`FETCH_FUNCTIONS`] spelled `spelling`, on `route`.
    pub fn new(spelling: &str, route: Rc<dyn FetchRoute>) -> Option<Self> {
        let target = crate::catalogue().resolve_surface(crate::Notation::Call, spelling).ok()?;
        if target.spelling() != spelling { return None; }
        let crate::EntryKind::HostRoute { route: crate::HostRouteKind::Fetch(kind), .. } = target.kind() else { return None; };
        Some(Self { kind, route })
    }
}

/// The shape every argument lifts to: arrays must agree or be single, and a missing position in
/// a larger shape is #N/A, as lifting pads.
pub(crate) fn lifted(arguments: &[CalcValue]) -> (ArrayShape, Vec<Vec<CalcValue>>) {
    let shape_of = |value: &CalcValue| value.as_array().map(CalcArray::shape);
    let rows = arguments
        .iter()
        .filter_map(shape_of)
        .map(|shape| shape.rows)
        .max()
        .unwrap_or(1);
    let cols = arguments
        .iter()
        .filter_map(shape_of)
        .map(|shape| shape.cols)
        .max()
        .unwrap_or(1);
    let mut elements = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for col in 0..cols {
            elements.push(
                arguments
                    .iter()
                    .map(|argument| match argument.as_array() {
                        Some(array) => {
                            let shape = array.shape();
                            let at = (
                                if shape.rows == 1 { 0 } else { row },
                                if shape.cols == 1 { 0 } else { col },
                            );
                            array
                                .get(at.0, at.1)
                                .cloned()
                                .unwrap_or_else(|| CalcValue::error(WorksheetErrorCode::NA))
                        }
                        None => argument.clone(),
                    })
                    .collect(),
            );
        }
    }
    (ArrayShape { rows, cols }, elements)
}

impl UserFunction for FetchFunction {
    fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
        Err("the fetching functions run only as a cooperative activation".to_owned())
    }

    fn begin_call(
        &self,
        arguments: &[CalcValue],
        _: UserInvocationFrame,
        context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
        let arguments = arguments
            .iter()
            .map(|value| match value.as_reference() {
                Some(reference) => context.argument_values_of(reference),
                None => Ok(value.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (shape, elements) = lifted(&arguments);
        Ok(Some(Box::new(FetchCall {
            kind: self.kind,
            route: self.route.clone(),
            shape,
            elements,
            done: Vec::new(),
            awaiting: None,
        })))
    }
}

struct FetchCall {
    kind: FetchKind,
    route: Rc<dyn FetchRoute>,
    shape: ArrayShape,
    elements: Vec<Vec<CalcValue>>,
    done: Vec<CalcValue>,
    awaiting: Option<(u64, Option<ImageValue>)>,
}

impl fmt::Debug for FetchCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FetchCall")
            .field("kind", &self.kind)
            .field("done", &self.done.len())
            .field("of", &self.elements.len())
            .finish_non_exhaustive()
    }
}

impl FetchCall {
    fn finish(&mut self, value: CalcValue, image: Option<ImageValue>) {
        let value = match (image, value.image_value()) {
            (Some(mut image), Some(picture)) => {
                image.format = picture.format;
                image.bytes = picture.bytes.clone();
                CalcValue::image(image)
            }
            _ => value,
        };
        self.done.push(value);
    }
}

impl UserFunctionInvocation for FetchCall {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        _: u64,
    ) -> Result<UserFunctionStep, String> {
        if let Some((request, image)) = self.awaiting.take() {
            let Some(value) = context.take_async_result(request) else {
                return Err("a fetch resumed without its reply".to_owned());
            };
            self.finish(value, image);
        }
        while self.done.len() < self.elements.len() {
            let element = &self.elements[self.done.len()];
            let prepared = match self.kind {
                FetchKind::Text => prepare_text(&element[0]),
                FetchKind::Picture => prepare_picture(element),
            };
            match prepared {
                Prepared::Answer(value) => self.finish(value, None),
                Prepared::Fetch(request, image) => {
                    let id = self.route.begin(request);
                    self.awaiting = Some((id, image));
                    return Ok(UserFunctionStep::Awaiting { request: id });
                }
            }
        }
        context.set_volatile(self.kind == FetchKind::Text);
        let cells = std::mem::take(&mut self.done);
        let value = if cells.len() == 1 {
            cells.into_iter().next().expect("one element")
        } else {
            match CalcArray::new(self.shape, cells) {
                Some(array) => CalcValue::array(array),
                None => return Err("a fetch's results do not fill its shape".to_owned()),
            }
        };
        Ok(UserFunctionStep::Complete(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .map(|at| u8::from_str_radix(&text[at * 2..at * 2 + 2], 16).unwrap())
            .collect()
    }

    /// Excel 16.0.20527's readings of these bodies from the local mock server.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bodies_decode_as_excel_decodes_them() {
        let cases: &[(&str, Option<&[u16]>)] = &[
            ("e96c6c", Some(&[0x9b2c])),
            ("c0", None),
            ("80", None),
            ("41ff42", Some(&[0x41, 0x42])),
            ("fe41", Some(&[0x41])),
            ("f8888080", None),
            ("c3", None),
            ("e282", None),
            ("ffef", None),
            ("efbbbf41", Some(&[0x41])),
            ("c080", Some(&[0])),
            ("eda080", Some(&[0xd800])),
            ("f4908080", Some(&[0xdc00, 0xdc00])),
            ("4100", Some(&[0x41, 0])),
            ("fffe4100", None),
            ("feff0041", None),
            ("41c0", Some(&[0x41])),
            ("41e9", Some(&[0x41])),
            ("41f09080", Some(&[0x41])),
            ("f7bfbfbf", Some(&[0xdfbf, 0xdfff])),
            ("41bf42", Some(&[0x41, 0x42])),
            ("00", Some(&[0])),
            ("", Some(&[])),
        ];
        for (body, expected) in cases {
            assert_eq!(decode_body(&hex(body)).as_deref(), *expected, "{body}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_longest_text_is_excels() {
        assert_eq!(decode_body(&[b'x'; 32767]).map(|text| text.len()), Some(32767));
        assert_eq!(decode_body(&[b'x'; 32768]), None);
    }

    /// A scripted transport: each URL answers from the table, and a miss is no response.
    fn follow(kind: FetchKind, url: &str, table: &[(&str, u16, Option<&str>, &str)]) -> (FetchReply, usize) {
        let mut asked = 0;
        let reply = fetch_following(kind, url, &mut |asked_url| {
            asked += 1;
            table.iter().find(|row| row.0 == asked_url).map(|row| RawResponse {
                status: row.1,
                location: row.2.map(str::to_owned),
                body: row.3.as_bytes().to_vec(),
            })
        });
        (reply, asked)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn locations_resolve_against_the_asking_url() {
        let base = "https://example.org/a/b?q=1#top";
        for (location, expected) in [
            ("https://other.net/x", Some("https://other.net/x")),
            ("//cdn.net/y", Some("https://cdn.net/y")),
            ("/root", Some("https://example.org/root")),
            ("next", Some("https://example.org/a/next")),
            ("?p=2", Some("https://example.org/a/b?p=2")),
            ("", Some("https://example.org/a/b?q=1")),
            ("next#frag", Some("https://example.org/a/next")),
            ("ftp://example.org/x", None),
        ] {
            assert_eq!(resolve_location(base, location).as_deref(), expected, "{location}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn redirects_follow_by_the_measured_rules() {
        let table = [
            ("http://a.test/one", 302, Some("/two"), ""),
            ("http://a.test/two", 200, None, "ok"),
            ("https://a.test/secure", 301, Some("http://a.test/two"), ""),
            ("https://a.test/gone", 404, None, "no"),
            ("https://a.test/empty", 204, None, ""),
            ("http://a.test/loop", 302, Some("/loop"), ""),
            ("http://a.test/odd", 302, Some("ftp://a.test/x"), ""),
            ("http://a.test/plain", 302, None, "body"),
        ];
        // A redirect is followed, relative to the URL that asked.
        assert_eq!(follow(FetchKind::Text, "http://a.test/one", &table), (FetchReply::Body(b"ok".to_vec()), 2));
        // WEBSERVICE stops at https to http and answers the empty body; IMAGE follows.
        assert_eq!(follow(FetchKind::Text, "https://a.test/secure", &table), (FetchReply::Body(Vec::new()), 1));
        assert_eq!(follow(FetchKind::Picture, "https://a.test/secure", &table), (FetchReply::Body(b"ok".to_vec()), 2));
        assert_eq!(follow(FetchKind::Text, "https://a.test/gone", &table).0, FetchReply::Status(404));
        assert_eq!(follow(FetchKind::Text, "https://a.test/empty", &table).0, FetchReply::Body(Vec::new()));
        // 25 redirects are followed; the 26th response ends the fetch as failed.
        assert_eq!(follow(FetchKind::Text, "http://a.test/loop", &table), (FetchReply::Failed, REDIRECT_LIMIT + 1));
        assert_eq!(follow(FetchKind::Text, "http://a.test/odd", &table).0, FetchReply::Failed);
        // A redirect status with no Location is a final response.
        assert_eq!(follow(FetchKind::Text, "http://a.test/plain", &table).0, FetchReply::Body(b"body".to_vec()));
        // No response at all is a failed fetch.
        assert_eq!(follow(FetchKind::Text, "http://nowhere.test/", &table), (FetchReply::Failed, 1));
    }
}
