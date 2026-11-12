//! dc-6yw: conditional formats as calc-host works them out, against what Excel 16.0.20430 shows for
//! the same workbook (tests/fixtures/dc_6yw: conditional-formats.xlsx, authored in Excel by
//! make-cf.ps1, which recorded each cell's DisplayFormat in cf-excel.txt and printed the sheet to
//! cf-excel.pdf, the reference for the data bars and icons DisplayFormat does not report).

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, SheetId};
use calc_host::{
    CellLook, HostOptions, HostSession, LookColor, MemorySource, OpenPolicy, PackageRead,
    SkinIntent,
};
use std::sync::Arc;

const WORKBOOK: &[u8] = include_bytes!("fixtures/dc_6yw/conditional-formats.xlsx");
const EXCEL: &str = include_str!("fixtures/dc_6yw/cf-excel.txt");

fn session() -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(WORKBOOK.to_vec()));
    let mut session = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    session
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}

fn rgb(color: &Option<LookColor>) -> Option<String> {
    match color {
        Some(LookColor::Rgb(text)) => Some(text.clone()),
        Some(LookColor::Indexed(index)) => Some(format!("indexed {index}")),
        None => None,
    }
}

/// Every cell of A1:N10: the fill, the font colour, bold and italic Excel shows, and for column N
/// the number format the rule gives (Excel shows 400.0% for 4).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__conditional_formats_look_as_excel_shows_them() {
    let session = session();
    let host = session.host();
    let sheet = host.sheets()[0].id();
    let formats = host.conditional_formats(sheet);
    assert_eq!(formats.rule_count(), 15);
    let mut compared = 0;
    for line in EXCEL.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [address, fill, font, bold, italic, text] = fields[..] else {
            panic!("a line of six fields: {line}");
        };
        let cell = at(sheet, address);
        let look: CellLook = formats.look(host, cell).unwrap_or_default();
        // Column G's data bars and H's icons draw no fill or font in DisplayFormat.
        let expected_fill = (fill != "-").then(|| fill.to_owned());
        assert_eq!(rgb(&look.fill), expected_fill, "{address} fill");
        let font_shown = rgb(&look.font_color).unwrap_or_else(|| "#000000".to_owned());
        assert_eq!(font_shown, font, "{address} font colour");
        assert_eq!(look.bold.unwrap_or(false), bold == "1", "{address} bold");
        assert_eq!(
            look.italic.unwrap_or(false),
            italic == "1",
            "{address} italic"
        );
        if address.starts_with('N') {
            let percent = text.ends_with('%');
            assert_eq!(
                look.number_format.as_deref() == Some("0.0%"),
                percent,
                "{address} format"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 140);
    assert_eq!(formats.refused(), 0);
}

/// Column H's three arrows and column G's data bars, as cf-excel.pdf shows them (Excel draws
/// them; DisplayFormat does not report them): the icon each value gets with the thresholds at 33%
/// and 67% of the range, and each bar's length from the lowest value (none, the extension's
/// minLength 0) to the highest (the whole cell).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__icons_and_data_bars_as_excel_draws_them() {
    let session = session();
    let host = session.host();
    let sheet = host.sheets()[0].id();
    let formats = host.conditional_formats(sheet);
    // Values 4, 9, 1, 7, 3, 10, 6, 2, 8, 5; the PDF shows right, up, down, right, down, up,
    // right, down, up, right (0 down, 1 right, 2 up).
    let icons: Vec<usize> = (1..=10)
        .map(|row| {
            formats
                .look(host, at(sheet, &format!("H{row}")))
                .and_then(|look| look.icon)
                .map(|icon| icon.index)
                .expect("an icon")
        })
        .collect();
    assert_eq!(icons, vec![1, 2, 0, 1, 0, 2, 1, 0, 2, 1]);
    let bar = |row: u32| {
        formats
            .look(host, at(sheet, &format!("G{row}")))
            .and_then(|look| look.bar)
            .expect("a bar")
    };
    assert_eq!(bar(3).fraction, 0.0, "1, the lowest");
    assert_eq!(bar(6).fraction, 1.0, "10, the highest");
    assert!((bar(1).fraction - 3.0 / 9.0).abs() < 1e-12, "4");
    assert!(bar(1).gradient);
    assert_eq!(bar(1).color, Some(LookColor::Rgb("#638ec6".to_owned())));
}

/// dc-qgth: a differential format's solid fill is its bgColor. circular-reference.xlsx's three
/// rules on B7:Q18 carry only a `fgColor`, and Excel 16.0.20527 shows no fill on those cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qgth__a_solid_dxf_fill_with_only_a_fg_color_paints_nothing() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/circular-reference.xlsx").to_vec(),
    ));
    let mut session = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    let host = session.host();
    let sheet = host.sheets()[0].id();
    let formats = host.conditional_formats(sheet);
    assert!(formats.rule_count() >= 3);
    for row in 7..=18u32 {
        for column in 2..=17u32 {
            let look = formats
                .look(host, CellKey::new(sheet, row, column).unwrap())
                .unwrap_or_default();
            assert_eq!(rgb(&look.fill), None, "row {row} column {column}");
        }
    }
}
