//! dc-6yw: charts as calc-host gives them to a skin, against what Excel 16.0.20430 draws for the
//! same workbook (tests/fixtures/dc_6yw: charts.xlsx, authored in Excel by make-charts.ps1, which
//! recorded each chart's type, title, corner cells and series (`Chart.SeriesCollection(i).Values`
//! and `.XValues`) in charts-excel.txt, before and after B2 is set to 40, and printed the sheet to
//! charts-excel.pdf).

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::chart::text_of;
use calc_host::value::{CellKey, SheetId};
use calc_host::{
    ChartKind, ChartPlace, ChartView, HostOptions, HostSession, MemorySource, OpenPolicy,
    PackageRead, SkinIntent,
};
use std::sync::Arc;

const WORKBOOK: &[u8] = include_bytes!("fixtures/dc_6yw/charts.xlsx");
const EXCEL: &str = include_str!("fixtures/dc_6yw/charts-excel.txt");

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

/// Excel's `XlChartType` for a plot as calc-host gives it.
fn chart_type(chart: &ChartView) -> i32 {
    let plot = &chart.plots[0];
    match (&plot.kind, plot.stacked, plot.percent) {
        (ChartKind::Column, false, _) => 51,
        (ChartKind::Column, true, false) => 52,
        (ChartKind::Bar, false, _) => 57,
        (ChartKind::Bar, true, false) => 58,
        (ChartKind::Line, false, _) => 4,
        (ChartKind::Area, false, _) => 1,
        (ChartKind::Area, true, false) => 76,
        (ChartKind::Pie, ..) => 5,
        (ChartKind::Doughnut, ..) => -4120,
        (ChartKind::Scatter, ..) => -4169,
        _ => 0,
    }
}

fn joined(values: impl Iterator<Item = String>) -> String {
    values.collect::<Vec<_>>().join(";")
}

fn numbers(values: &[Option<f64>]) -> String {
    joined(
        values
            .iter()
            .map(|v| v.map(|n| format!("{n}")).unwrap_or_default()),
    )
}

fn labels(chart: &ChartView, series: usize) -> String {
    joined(
        chart.plots[0].series[series]
            .categories
            .iter()
            .map(|v| v.as_ref().map(text_of).unwrap_or_default()),
    )
}

/// Every chart's type, title and corners, and every series' name, values and X values, as Excel
/// gives them; then the values after an edit and the recalculation it brings.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__charts_show_what_excel_draws() {
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let (charts, unread) = session.host().charts(sheet);
    assert_eq!(unread, 0);
    let mut compared = 0;
    for line in EXCEL.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields[..] {
            ["chart", n, kind, title, top_left, bottom_right] => {
                let chart = &charts[n.parse::<usize>().unwrap() - 1];
                assert_eq!(chart_type(chart).to_string(), kind, "chart {n}");
                assert_eq!(chart.title.as_deref(), Some(title), "chart {n}");
                let ChartPlace::Cells { from, to } = chart.place else {
                    panic!("chart {n} is anchored at two cells");
                };
                assert_eq!(from.cell, at(sheet, top_left), "chart {n}");
                assert_eq!(to.cell, at(sheet, bottom_right), "chart {n}");
            }
            ["series", n, i, name, values, x_values] => {
                let chart = &charts[n.parse::<usize>().unwrap() - 1];
                let i = i.parse::<usize>().unwrap() - 1;
                let series = &chart.plots[0].series[i];
                assert_eq!(series.name, name, "chart {n} series {i}");
                assert_eq!(numbers(&series.values), values, "chart {n} series {i}");
                assert_eq!(labels(chart, i), x_values, "chart {n} series {i}");
                assert!(!series.cached);
            }
            ["after", ..] => continue,
            _ => panic!("an unexpected line: {line}"),
        }
        compared += 1;
    }
    assert_eq!(compared, 15);

    assert!(
        session
            .dispatch(SkinIntent::EnterCell {
                at: at(sheet, "B2"),
                text: "40".to_owned(),
            })
            .accepted()
    );
    let (charts, _) = session.host().charts(sheet);
    let mut after = 0;
    for line in EXCEL.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if let ["after", n, i, values, x_values] = fields[..] {
            let chart = &charts[n.parse::<usize>().unwrap() - 1];
            let i = i.parse::<usize>().unwrap() - 1;
            assert_eq!(
                numbers(&chart.plots[0].series[i].values),
                values,
                "chart {n}"
            );
            assert_eq!(labels(chart, i), x_values, "chart {n}");
            after += 1;
        }
    }
    assert_eq!(after, 9);
}

/// The plots' grouping and colours: the theme's accents by series, by point for a pie.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_6yw__chart_colours_follow_the_theme() {
    let session = session();
    let sheet = session.host().sheets()[0].id();
    let (charts, _) = session.host().charts(sheet);
    assert_eq!(charts.len(), 6);
    let column = &charts[0].plots[0];
    assert_eq!(column.series[0].color, charts[0].palette[0]);
    assert_eq!(column.series[1].color, charts[0].palette[1]);
    assert!(charts[4].plots[0].vary_colors);
    assert_eq!(charts[0].legend.as_deref(), Some("r"));
    assert_eq!(
        charts[0].palette[0], "156082",
        "the Office 2023 theme's accent1"
    );
}

/// Authored title/legend edits retain plot records and use ordinary history and Save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_e5z5__properties_save_undo_and_preserve_other_parts() {
    let mut book = session();
    let sheet = book.host().sheets()[0].id();
    let before = book.host().charts(sheet).0;
    let source = book.host().loaded().snapshot(book.host().model());
    let history = book.history_depth();
    let title = "Quarterly & Ω <2026>";
    let command = calc_host::WorkbookCommand::ChartProperties {
        sheet,
        revision: book.host().revision().get(),
        part: before[0].part.clone(),
        title: Some(title.into()),
        set_title: true,
        legend: Some(false),
    };
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let batch = calc_doc_model::OpBatch {
        base: book.host().revision(),
        operations: vec![calc_doc_model::WorkbookOp::SetCharts {
            sheet,
            charts: book.host().model().definitions().charts(sheet).to_vec(),
        }],
    };
    assert!(
        matches!(
            calc_doc_model::encode_batch(&batch, book.host().model()),
            Err(calc_doc_model::EncodeError::Charts { operation: 0 })
        ),
        "wire version 1 must explicitly refuse chart authoring"
    );
    let after = book.host().charts(sheet).0;
    assert_eq!(after[0].title.as_deref(), Some(title));
    assert_eq!(after[0].legend, None);
    assert_eq!(after[0].plots, before[0].plots);
    assert_eq!(after[1..], before[1..]);
    assert_eq!(book.history_depth(), (history.0 + 1, 0));
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().charts(sheet).0, before);
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    assert_eq!(book.host().charts(sheet).0, after);
    let mut bytes = Vec::new();
    book.save(&calc_host::SavePolicy::default(), &mut bytes)
        .unwrap();
    let mut reopened = HostSession::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    assert!(reopened.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(reopened.host().charts(sheet).0, after);
    let saved = reopened.host().loaded().snapshot(reopened.host().model());
    let read = |package: &calc_doc::Package, part: &str| {
        String::from_utf8(package.read_xml(part).unwrap().bytes().to_vec()).unwrap()
    };
    let original = read(source.package(), &before[0].part);
    let edited = read(saved.package(), &before[0].part);
    let plot = |xml: &str| {
        let start = xml.find("<c:plotArea").unwrap();
        let end = xml.find("</c:plotArea>").unwrap() + "</c:plotArea>".len();
        xml[start..end].to_owned()
    };
    assert_eq!(plot(&original), plot(&edited));
    let title_xml = &edited[edited.find("<c:title").unwrap()..edited.find("</c:title>").unwrap()];
    assert!(title_xml.contains("<a:rPr lang=\"en-ZA\"/>"));
    assert!(title_xml.contains("<a:pPr><a:defRPr/></a:pPr>"));
    assert!(title_xml.contains("<c:overlay val=\"0\"/>"));
    for chart in &before[1..] {
        assert_eq!(
            read(source.package(), &chart.part),
            read(saved.package(), &chart.part)
        );
    }
    assert!(
        reopened
            .dispatch(SkinIntent::EnterCell {
                at: at(sheet, "B2"),
                text: "40".into()
            })
            .accepted()
    );
    assert_eq!(
        reopened.host().charts(sheet).0[0].plots[0].series[0].values[0],
        Some(40.)
    );
    assert!(
        reopened
            .dispatch(SkinIntent::EditAxis(
                calc_host::AxisEdit::new(
                    sheet,
                    calc_host::Axis::Rows,
                    calc_host::AxisChange::Insert,
                    2,
                    1
                )
                .unwrap()
            ))
            .accepted()
    );
    let shifted = reopened.host().charts(sheet).0;
    assert_eq!(shifted[0].title.as_deref(), Some(title));
    assert_eq!(shifted[0].plots[0].series[0].values[0], Some(40.));
    let mut bytes = Vec::new();
    reopened
        .save(&calc_host::SavePolicy::default(), &mut bytes)
        .unwrap();
    let mut final_book = HostSession::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap();
    assert!(final_book.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(
        final_book.host().charts(sheet).0,
        shifted,
        "chart metadata composes with the structural writer"
    );
}

/// Clearing a one-series title disables the automatic fallback; adding it after reopen persists.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_e5z5__deleted_title_and_legend_can_be_added_after_reopen() {
    let mut book = session();
    let sheet = book.host().sheets()[0].id();
    let part = book.host().charts(sheet).0[4].part.clone();
    for (title, legend) in [
        (None, false),
        (Some("Restored title"), true),
        (Some("Revised title"), true),
    ] {
        let command = calc_host::WorkbookCommand::ChartProperties {
            sheet,
            revision: book.host().revision().get(),
            part: part.clone(),
            title: title.map(str::to_owned),
            set_title: true,
            legend: Some(legend),
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let mut bytes = Vec::new();
        book.save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        book = HostSession::open(
            Arc::new(MemorySource::new(bytes)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let chart = &book.host().charts(sheet).0[4];
        assert_eq!(chart.title.as_deref(), title);
        assert_eq!(chart.legend.is_some(), legend);
    }
}

/// Invalid or stale edits refuse before revision, history or the authored chart changes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_e5z5__stale_and_invalid_properties_do_not_mutate() {
    let mut book = session();
    let sheet = book.host().sheets()[0].id();
    let before = book.host().charts(sheet).0;
    let revision = book.host().revision();
    let history = book.history_depth();
    for (expected, part, title) in [
        (revision.get() + 1, before[0].part.clone(), "Stale".into()),
        (
            revision.get(),
            "xl/charts/missing.xml".into(),
            "Missing".into(),
        ),
        (
            revision.get(),
            before[0].part.clone(),
            "Invalid\u{1}title".into(),
        ),
    ] {
        assert!(
            !book
                .dispatch(SkinIntent::Command(
                    calc_host::WorkbookCommand::ChartProperties {
                        sheet,
                        revision: expected,
                        part,
                        title: Some(title),
                        set_title: true,
                        legend: Some(false)
                    }
                ))
                .accepted()
        );
        assert_eq!(book.host().revision(), revision);
        assert_eq!(book.history_depth(), history);
        assert_eq!(book.host().charts(sheet).0, before);
    }
}
