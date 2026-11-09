//! WEBSERVICE and IMAGE through a host's fetch route, end to end: formulas in a package, the
//! ordinary async path, and a mock route that plays Excel's local mock server's replies. No test
//! touches a network. Every expected result is Excel 16.0.20527's (see `calc_func::fetch`).

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

#[allow(dead_code)]
mod chost_m1_1_support;

use calc_func::{FetchKind, FetchReply, FetchRequest, FetchRoute};
use calc_host::value::{CalcValue, CellKey, CoreRef, ImageFormat, WorksheetErrorCode};
use calc_host::{
    CalcMode, HostOptions, HostProgress, HostState, MemorySource, OpenPolicy, PackageRead,
    ProjectedValue, ProjectionRequest, Unobserved, WorkBudget, calculate, calculate_full, project,
    step,
};
use chost_m1_1_support::stored_zip;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest of a picture";

#[derive(Debug, Default)]
struct MockRoute {
    open: RefCell<BTreeMap<u64, FetchRequest>>,
    seen: RefCell<Vec<FetchRequest>>,
}

impl FetchRoute for MockRoute {
    fn begin(&self, request: FetchRequest) -> u64 {
        let mut open = self.open.borrow_mut();
        let id = self.seen.borrow().len() as u64 + 1;
        open.insert(id, request.clone());
        self.seen.borrow_mut().push(request);
        id
    }
}

/// The replies Excel's local mock server gave, by URL path.
fn mock_reply(url: &str) -> FetchReply {
    let path = url.split_once("//").map_or(url, |(_, rest)| {
        rest.split_once('/').map_or("", |(_, path)| path)
    });
    match path {
        "hello" => FetchReply::Body(b"hello from mock".to_vec()),
        "xml" => FetchReply::Body(b"<r><a>1</a><a>2</a></r>".to_vec()),
        "num" => FetchReply::Body(b"42".to_vec()),
        "empty" => FetchReply::Body(Vec::new()),
        "utf8" => FetchReply::Body("h\u{e9}llo \u{20ac}".as_bytes().to_vec()),
        "latin1" => FetchReply::Body(b"h\xe9llo".to_vec()),
        "utf16" => FetchReply::Body(b"\xff\xfeh\0".to_vec()),
        "long" => FetchReply::Body(vec![b'x'; 32767]),
        "toolong" => FetchReply::Body(vec![b'x'; 32768]),
        "404" => FetchReply::Status(404),
        "500" => FetchReply::Status(500),
        "png" => FetchReply::Body(PNG.to_vec()),
        "page" => FetchReply::Body(b"<html></html>".to_vec()),
        _ => FetchReply::Failed,
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Sheet1 with `formulas` in column A, each as the file stores it, and the text of `texts` in
/// column C.
fn workbook(formulas: &[&str], texts: &[&str]) -> Vec<u8> {
    let mut rows = String::new();
    for row in 1..=formulas.len().max(texts.len()) {
        rows.push_str(&format!(r#"<row r="{row}">"#));
        if let Some(formula) = formulas.get(row - 1) {
            rows.push_str(&format!(r#"<c r="A{row}"><f>{}</f></c>"#, escape(formula)));
        }
        if let Some(text) = texts.get(row - 1) {
            rows.push_str(&format!(
                r#"<c r="C{row}" t="inlineStr"><is><t>{}</t></is></c>"#,
                escape(text)
            ));
        }
        rows.push_str("</row>");
    }
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{MAIN}"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let book = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let book_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{RELATIONSHIPS}"><Relationship Id="rId1" Type="{R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    stored_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", book.as_bytes()),
        ("xl/_rels/workbook.xml.rels", book_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

fn open(bytes: Vec<u8>) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    HostState::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..HostOptions::default()
        },
    )
    .expect("the package opens")
}

/// Steps the calculation, answering each pending fetch from the mock.
fn settle(host: &mut HostState, route: &MockRoute) {
    for _ in 0..100_000 {
        let progress = step(host, WorkBudget::UNLIMITED, &mut Unobserved);
        let mut delivered = false;
        for token in host.pending_async() {
            let request = route
                .open
                .borrow_mut()
                .remove(&token.request)
                .expect("a pending fetch was begun");
            let value = mock_reply(&request.url).into_value(request.kind);
            host.complete_async(&token, value);
            delivered = true;
        }
        match progress {
            HostProgress::Failed(why) => panic!("{why}"),
            HostProgress::Calculating { .. } | HostProgress::Waiting { .. } => {}
            HostProgress::Published(_) if delivered => {}
            HostProgress::Published(_) => {}
            _ if delivered => {}
            _ => return,
        }
    }
    panic!("the calculation did not settle");
}

/// The results of `formulas` with a mock route bound, after a full calculation.
fn results(formulas: &[&str], texts: &[&str]) -> (Vec<CalcValue>, Rc<MockRoute>) {
    let route = Rc::new(MockRoute::default());
    let mut host = open(workbook(formulas, texts));
    host.set_fetch_route(route.clone());
    calculate_full(&mut host);
    settle(&mut host, &route);
    (values(&host, formulas.len()), route)
}

fn values(host: &HostState, rows: usize) -> Vec<CalcValue> {
    let sheet = host.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=rows as u32)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect();
    let projection = project(host, &ProjectionRequest { cells }).expect("projected");
    projection
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::Calculated { value, .. } => value.clone(),
            other => panic!("not calculated: {other:?}"),
        })
        .collect()
}

fn text(value: &str) -> CalcValue {
    CalcValue::text(calc_host::value::ExcelText::from_utf16_code_units(
        value.encode_utf16().collect(),
    ))
}

const VALUE: WorksheetErrorCode = WorksheetErrorCode::Value;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__webservice_gives_excels_results_for_each_reply() {
    let cases: [(&str, CalcValue); 12] = [
        (
            r#"_xlfn.WEBSERVICE("http://mock/hello")"#,
            text("hello from mock"),
        ),
        (r#"_xlfn.WEBSERVICE("http://mock/num")"#, text("42")),
        (r#"_xlfn.WEBSERVICE("http://mock/empty")"#, text("")),
        (
            r#"_xlfn.WEBSERVICE("http://mock/utf8")"#,
            text("h\u{e9}llo \u{20ac}"),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/latin1")"#,
            text("h\u{9b2c}o"),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/utf16")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/404")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/500")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/toolong")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/refused")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("http://mock/num")+1"#,
            CalcValue::number(43.0),
        ),
        (
            r#"LEN(_xlfn.WEBSERVICE("http://mock/long"))"#,
            CalcValue::number(32767.0),
        ),
    ];
    let formulas: Vec<&str> = cases.iter().map(|(formula, _)| *formula).collect();
    let (got, route) = results(&formulas, &[]);
    for ((formula, expected), got) in cases.iter().zip(&got) {
        assert_eq!(got, expected, "{formula}");
    }
    assert_eq!(route.seen.borrow().len(), cases.len());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__webservice_refuses_without_a_request() {
    let long = format!("http://mock/q?{}", "a".repeat(2049 - 14));
    let fits = format!("http://mock/q?{}", "a".repeat(2048 - 14));
    let cases = [
        ("_xlfn.WEBSERVICE(123)", CalcValue::error(VALUE)),
        ("_xlfn.WEBSERVICE(TRUE)", CalcValue::error(VALUE)),
        ("_xlfn.WEBSERVICE(E9)", CalcValue::error(VALUE)),
        (r#"_xlfn.WEBSERVICE("")"#, CalcValue::error(VALUE)),
        (
            r#"_xlfn.WEBSERVICE("ftp://mock/x")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("file:///C:/x.txt")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("example.org/x")"#,
            CalcValue::error(VALUE),
        ),
        (r#"_xlfn.WEBSERVICE("//mock:1/x")"#, CalcValue::error(VALUE)),
        (
            r#"_xlfn.WEBSERVICE("http:mock:1/x")"#,
            CalcValue::error(VALUE),
        ),
        (
            r#"_xlfn.WEBSERVICE("about:blank")"#,
            CalcValue::error(VALUE),
        ),
        (
            "_xlfn.WEBSERVICE(NA())",
            CalcValue::error(WorksheetErrorCode::NA),
        ),
        ("_xlfn.WEBSERVICE(C1)", CalcValue::error(VALUE)),
    ];
    let formulas: Vec<&str> = cases.iter().map(|(formula, _)| *formula).collect();
    let (got, route) = results(&formulas, &[&long]);
    for ((formula, expected), got) in cases.iter().zip(&got) {
        assert_eq!(got, expected, "{formula}");
    }
    assert!(route.seen.borrow().is_empty(), "{:?}", route.seen.borrow());
    let (got, route) = results(&["_xlfn.WEBSERVICE(C1)"], &[&fits]);
    assert_eq!(got[0], CalcValue::error(VALUE), "an unknown path fails");
    assert_eq!(route.seen.borrow().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__the_wire_url_is_excels() {
    let (_, route) = results(
        &[
            r#"_xlfn.WEBSERVICE("http://mock/a b")"#,
            r#"_xlfn.WEBSERVICE("http://mock/hello#frag")"#,
            "_xlfn.WEBSERVICE(\"http://mock/\u{e9}?q=\u{e9}&r=a b\")",
            r#"_xlfn.WEBSERVICE("mock:1/hello")"#,
            r#"_xlfn.WEBSERVICE("HTTP://mock/hello")"#,
            r#"_xlfn.WEBSERVICE(" http://mock/hello ")"#,
        ],
        &[],
    );
    let urls: Vec<String> = route.seen.borrow().iter().map(|r| r.url.clone()).collect();
    assert_eq!(
        urls,
        [
            "http://mock/a%20b",
            "http://mock/hello%23frag",
            "http://mock/%C3%A9?q=\u{e9}&r=a%20b",
            "http://mock:1/hello",
            "HTTP://mock/hello",
            "http://mock/hello",
        ]
    );
}

/// The common pattern: FILTERXML has no fetch of its own and reads WEBSERVICE's text.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__filterxml_reads_webservice() {
    let (got, _) = results(
        &[
            r#"_xlfn.FILTERXML(_xlfn.WEBSERVICE("http://mock/xml"),"//a[2]")"#,
            r#"SUM(_xlfn.FILTERXML(_xlfn.WEBSERVICE("http://mock/xml"),"//a"))"#,
        ],
        &[],
    );
    assert_eq!(got[0], CalcValue::number(2.0));
    // A plain `<f>` is a legacy formula, and Excel's SUM(FILTERXML(...)) there takes the first
    // node (catalogue dcuar_text!D1293).
    assert_eq!(got[1], CalcValue::number(1.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__webservice_is_volatile() {
    let route = Rc::new(MockRoute::default());
    let mut host = open(workbook(&[r#"_xlfn.WEBSERVICE("http://mock/hello")"#], &[]));
    host.set_fetch_route(route.clone());
    calculate_full(&mut host);
    settle(&mut host, &route);
    assert_eq!(route.seen.borrow().len(), 1);
    calculate(&mut host);
    settle(&mut host, &route);
    assert_eq!(
        route.seen.borrow().len(),
        2,
        "a plain Calculate fetches again"
    );
    assert_eq!(values(&host, 1)[0], text("hello from mock"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__no_route_gives_the_failed_fetch() {
    let mut host = open(workbook(
        &[
            r#"_xlfn.WEBSERVICE("http://mock/hello")"#,
            "_xlfn.WEBSERVICE(NA())",
            r#"_xlfn.IMAGE("https://mock/png")"#,
            r#"_xlfn.IMAGE("http://mock/png")"#,
        ],
        &[],
    ));
    calculate_full(&mut host);
    loop {
        match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
            HostProgress::Calculating { .. } | HostProgress::Published(_) => {}
            _ => break,
        }
    }
    assert_eq!(
        values(&host, 4),
        [
            CalcValue::error(VALUE),
            CalcValue::error(WorksheetErrorCode::NA),
            CalcValue::error(VALUE),
            CalcValue::error(WorksheetErrorCode::Blocked),
        ]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__image_is_a_picture_value_that_reads_as_excel_reads_it() {
    let (got, route) = results(
        &[
            r#"_xlfn.IMAGE("https://mock/png","alt text",3,20,30)"#,
            r#"TYPE(_xlfn.IMAGE("https://mock/png"))"#,
            r#"ISERROR(_xlfn.IMAGE("https://mock/png"))"#,
        ],
        &[],
    );
    let image = got[0].image_value().expect("a picture");
    assert_eq!(image.source, "https://mock/png");
    assert_eq!(image.alt_text.as_deref(), Some("alt text"));
    assert_eq!(
        (image.sizing, image.height, image.width),
        (3, Some(20.0), Some(30.0))
    );
    assert_eq!(image.format, ImageFormat::Png);
    let (sizes, _) = results(
        &[
            r#"_xlfn.IMAGE("https://mock/png","a",3,20)"#,
            r#"_xlfn.IMAGE("https://mock/png","a",3,,20)"#,
            r#"_xlfn.IMAGE("https://mock/png","",1.5)"#,
            r#"_xlfn.IMAGE("https://mock/png",TRUE,"2")"#,
        ],
        &[],
    );
    let sized = |row: usize| sizes[row].image_value().expect("a picture");
    assert_eq!((sized(0).height, sized(0).width), (Some(20.0), None));
    assert_eq!((sized(1).height, sized(1).width), (Some(0.0), Some(20.0)));
    assert_eq!((sized(2).alt_text.as_deref(), sized(2).sizing), (None, 1));
    assert_eq!(
        (sized(3).alt_text.as_deref(), sized(3).sizing),
        (Some("TRUE"), 2)
    );
    assert_eq!(&*image.bytes, PNG);
    assert!(matches!(got[0].core_ref(), CoreRef::Error(VALUE)));
    assert_eq!(got[1], CalcValue::number(128.0));
    assert_eq!(got[2], CalcValue::logical(false));
    assert!(
        route
            .seen
            .borrow()
            .iter()
            .all(|r| r.kind == FetchKind::Picture)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__image_errors_are_excels() {
    let connect = WorksheetErrorCode::Connect;
    let cases = [
        (r#"_xlfn.IMAGE("https://mock/404")"#, connect),
        (r#"_xlfn.IMAGE("https://mock/500")"#, VALUE),
        (r#"_xlfn.IMAGE("https://mock/refused")"#, VALUE),
        (r#"_xlfn.IMAGE("https://mock/page")"#, VALUE),
        (
            r#"_xlfn.IMAGE("http://mock/png")"#,
            WorksheetErrorCode::Blocked,
        ),
        (r#"_xlfn.IMAGE("ftp://mock/png")"#, VALUE),
        (r#"_xlfn.IMAGE("")"#, VALUE),
        ("_xlfn.IMAGE(123)", VALUE),
        ("_xlfn.IMAGE(TRUE)", VALUE),
        ("_xlfn.IMAGE(E9)", VALUE),
        ("_xlfn.IMAGE(NA())", WorksheetErrorCode::NA),
        (r#"_xlfn.IMAGE("https://mock/png","a",4)"#, VALUE),
        (r#"_xlfn.IMAGE("https://mock/png","a",3)"#, VALUE),
        (r#"_xlfn.IMAGE("https://mock/png","a",3,0,0)"#, VALUE),
        (r#"_xlfn.IMAGE("https://mock/png","a",0,20,30)"#, VALUE),
    ];
    let formulas: Vec<&str> = cases.iter().map(|(formula, _)| *formula).collect();
    let (got, _) = results(&formulas, &[]);
    for ((formula, code), got) in cases.iter().zip(&got) {
        assert_eq!(got, &CalcValue::error(*code), "{formula}");
    }
}

/// A good IMAGE does not fetch again on a plain recalculation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__image_is_not_volatile() {
    let route = Rc::new(MockRoute::default());
    let mut host = open(workbook(&[r#"_xlfn.IMAGE("https://mock/png")"#], &[]));
    host.set_fetch_route(route.clone());
    calculate_full(&mut host);
    settle(&mut host, &route);
    assert_eq!(route.seen.borrow().len(), 1);
    calculate(&mut host);
    settle(&mut host, &route);
    assert_eq!(route.seen.borrow().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__an_array_of_urls_lifts_one_fetch_each() {
    let (got, route) = results(
        &[r#"SUMPRODUCT(LEN(_xlfn.WEBSERVICE({"http://mock/hello";"http://mock/num"})))"#],
        &[],
    );
    assert_eq!(got[0], CalcValue::number(17.0));
    assert_eq!(route.seen.borrow().len(), 2);
}

/// How a picture in a cell reads, Excel's result beside each formula.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__a_picture_flows_and_reads_as_excel_measured() {
    let formulas = [
        r#"_xlfn.IMAGE("https://mock/png","one")"#,
        r#"_xlfn.IMAGE("https://mock/png","two")"#,
        r#"_xlfn.IMAGE("https://mock/png","one",1)"#,
        "A1",
        "IF(TRUE,A1)",
        "IFERROR(A1,\"x\")",
        "_xlfn.IFNA(A1,3)",
        "INDEX(A1:A2,1)",
        "CHOOSE(1,A1)",
        "A1=A2",
        "A1=A3",
        "A1<>A2",
        "A1<A2",
        "A1&\"\"",
        "A1+1",
        "LEN(A1)",
        "SUM(A1)",
        "COUNT(A1)",
        "COUNTA(A1)",
        "ISERROR(A1)",
        "ISERR(A1)",
        "ISTEXT(A1)",
        "ISNONTEXT(A1)",
        "ISBLANK(A1)",
        "TYPE(A1)",
        "ERROR.TYPE(A1)",
        "T(A1)",
        "N(A1)",
        "_xlfn.IMAGE(A1)",
    ];
    let (got, _) = results(&formulas, &[]);
    for row in 3..=7 {
        let image = got[row]
            .image_value()
            .unwrap_or_else(|| panic!("{}", formulas[row]));
        assert_eq!(image.alt_text.as_deref(), Some("one"), "{}", formulas[row]);
    }
    let logical = CalcValue::logical;
    let number = CalcValue::number;
    let value = CalcValue::error(VALUE);
    let expected = [
        (8, got[0].clone()),
        (9, logical(false)),
        (10, logical(true)),
        (11, logical(true)),
        (12, value.clone()),
        (13, value.clone()),
        (14, value.clone()),
        (15, value.clone()),
        (16, value.clone()),
        (17, number(0.0)),
        (18, number(1.0)),
        (19, logical(false)),
        (20, logical(false)),
        (21, logical(false)),
        (22, logical(true)),
        (23, logical(false)),
        (24, number(128.0)),
        (25, CalcValue::error(WorksheetErrorCode::NA)),
        (26, value.clone()),
        (27, value.clone()),
        (28, value.clone()),
    ];
    for (row, excel) in expected {
        assert_eq!(got[row], excel, "{}", formulas[row]);
    }
}

/// A calculated picture saves as Excel writes one (a `#VALUE!` fallback, a `_webimage` rich value,
/// the media part), and the reopened workbook shows it from the file with no fetch.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_fetch_f0tp__a_picture_saves_and_reopens() {
    let route = Rc::new(MockRoute::default());
    let mut host = open(workbook(
        &[
            r#"_xlfn.IMAGE("https://mock/png","alt & text",3,20,30)"#,
            r#"_xlfn.IMAGE("https://mock/png")"#,
            "A1",
        ],
        &[],
    ));
    host.set_fetch_route(route.clone());
    calculate_full(&mut host);
    settle(&mut host, &route);
    let mut saved = Vec::new();
    calc_host::save(&host, &calc_host::SavePolicy::default(), &mut saved).expect("saved");
    let reopened = open(saved.clone());
    let sheet = reopened.sheets()[0].id();
    let cells: Vec<CellKey> = (1..=3)
        .map(|row| CellKey::new(sheet, row, 1).expect("in the grid"))
        .collect();
    let projection = project(&reopened, &ProjectionRequest { cells }).expect("projected");
    let shown: Vec<CalcValue> = projection
        .cells()
        .iter()
        .map(|cell| match cell.value() {
            ProjectedValue::FileCached(file) => file.value().to_value(),
            other => panic!("not cached: {other:?}"),
        })
        .collect();
    let first = shown[0].image_value().expect("a picture");
    assert_eq!(first.source, "https://mock/png");
    assert_eq!(first.alt_text.as_deref(), Some("alt & text"));
    assert_eq!(
        (first.sizing, first.height, first.width),
        (3, Some(20.0), Some(30.0))
    );
    assert_eq!(&*first.bytes, PNG);
    let second = shown[1].image_value().expect("a picture");
    assert_eq!((second.alt_text.as_deref(), second.sizing), (None, 0));
    assert_eq!(shown[2], shown[0], "a cell reading a picture saves it too");
    assert_eq!(route.seen.borrow().len(), 2, "reopening fetches nothing");
    // A second save keeps every picture.
    let mut again = Vec::new();
    calc_host::save(&reopened, &calc_host::SavePolicy::default(), &mut again).expect("saved");
    let third = open(again);
    let cells = vec![CellKey::new(third.sheets()[0].id(), 1, 1).expect("in the grid")];
    match project(&third, &ProjectionRequest { cells })
        .expect("projected")
        .cells()[0]
        .value()
    {
        ProjectedValue::FileCached(file) => {
            assert_eq!(file.value().to_value().image_value(), Some(first));
        }
        other => panic!("not cached: {other:?}"),
    }
}
