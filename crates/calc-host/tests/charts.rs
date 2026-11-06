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

fn series_command(book:&HostSession, plot:u32, series:Option<u32>,name:Option<&str>,categories:Option<&str>,values:Option<&str>,remove:bool)->calc_host::WorkbookCommand {
    let sheet=book.host().sheets()[0].id();
    calc_host::WorkbookCommand::ChartSeries{sheet,revision:book.host().revision().get(),part:book.host().charts(sheet).0[0].part.clone(),plot,series,name:name.map(str::to_owned),categories:categories.map(str::to_owned),values:values.map(str::to_owned),remove}
}
fn reopened(book:&mut HostSession)->HostSession {
    let mut bytes=Vec::new();book.save(&calc_host::SavePolicy::default(),&mut bytes).unwrap();
    let mut next=HostSession::open(Arc::new(MemorySource::new(bytes)),&OpenPolicy::default(),HostOptions::default()).unwrap();
    assert!(next.dispatch(SkinIntent::Recalculate).accepted());next
}

/// User edits use live references, one history entry, and the same retained writer as Save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__series_sources_history_and_saved_reopening() {
    let mut book=session();let sheet=book.host().sheets()[0].id();
    let before=book.host().charts(sheet).0;let history=book.history_depth();
    let command=series_command(&book,0,Some(0),Some("New & Ω <series>"),Some("Data!A2:A7"),Some("Data!C2:C7"),false);
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let after=book.host().charts(sheet).0;
    assert_eq!(after[0].plots[0].series[0].name,"New & Ω <series>");
    assert_eq!(after[0].plots[0].series[0].values,before[0].plots[0].series[1].values);
    assert!(!after[0].plots[0].series[0].cached);
    assert_eq!(after[0].plots[0].series[1],before[0].plots[0].series[1]);assert_eq!(after[1..],before[1..]);
    assert_eq!(book.history_depth(),(history.0+1,0));
    assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0,before);
    assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(book.host().charts(sheet).0,after);
    let mut next=reopened(&mut book);assert_eq!(next.host().charts(sheet).0,after);
    assert!(next.dispatch(SkinIntent::EnterCell{at:at(sheet,"C2"),text:"99".into()}).accepted());
    assert_eq!(next.host().charts(sheet).0[0].plots[0].series[0].values[0],Some(99.));
    assert!(next.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,calc_host::AxisChange::Insert,2,1).unwrap())).accepted());
    let shifted=next.host().charts(sheet).0;
    assert_eq!(reopened(&mut next).host().charts(sheet).0,shifted,"new sources compose with structural rewrites");
    let original=book.host().loaded().snapshot(book.host().loaded().model());
    let saved=next.host().loaded().snapshot(next.host().model());
    for chart in &before[1..] {assert_eq!(original.package().read_xml(&chart.part).unwrap().bytes(),saved.package().read_xml(&chart.part).unwrap().bytes());}
}

/// Add/remove maintain stable series identities and valid order through repeated saves.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__add_remove_series_with_linked_name() {
    let mut book=session();let sheet=book.host().sheets()[0].id();
    let command=series_command(&book,0,None,Some("=Data!B1"),Some("Data!A2:A7"),Some("Data!B2:B7"),false);
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(book.host().charts(sheet).0[0].plots[0].series.len(),3);
    let added=book.host().charts(sheet).0[0].plots[0].series[2].clone();
    assert_eq!(added.name,book.host().charts(sheet).0[0].plots[0].series[0].name);
    assert_eq!(added.values,book.host().charts(sheet).0[0].plots[0].series[0].values);
    let command=series_command(&book,0,Some(0),None,None,None,true);
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let before=book.host().charts(sheet).0;
    let mut next=reopened(&mut book);assert_eq!(next.host().charts(sheet).0,before);
    let snapshot=next.host().loaded().snapshot(next.host().model());
    let bytes=snapshot.package().read_xml(&before[0].part).unwrap();
    let xml=String::from_utf8(bytes.bytes().to_vec()).unwrap();
    assert_eq!(xml.matches("<c:order").count(),2);
    let mut reader=bytes.reader();let mut ids=Vec::new();let mut index=None;
    while let Some(event)=reader.next_event().unwrap() {
        if let calc_doc::XmlEvent::Start(e)=event {
            if e.name.local=="idx" {index=e.attribute("val").and_then(|v|v.parse::<u32>().ok());}
            if e.name.local=="order" {ids.push((index.unwrap(),e.attribute("val").unwrap().parse::<u32>().unwrap()));}
        }
    }
    assert_eq!(ids,vec![(1,0),(2,1)],"indices remain stable; ordering is contiguous after removal");
    assert!(next.dispatch(SkinIntent::EnterCell{at:at(sheet,"B1"),text:"Linked series".into()}).accepted());
    assert_eq!(next.host().charts(sheet).0[0].plots[0].series[1].name,"Linked series");
    let command=series_command(&next,0,Some(1),None,None,None,true);assert!(next.dispatch(SkinIntent::Command(command)).accepted());
    let last=next.host().model().definitions().charts(sheet)[0].plots[0].series[0].index;
    let revision=next.host().revision();let history=next.history_depth();
    let command=series_command(&next,0,Some(last),None,None,None,true);assert!(!next.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(next.host().revision(),revision);assert_eq!(next.history_depth(),history);
    let mut next=reopened(&mut next);assert_eq!(next.host().charts(sheet).0[0].plots[0].series.len(),1);
    assert_eq!(reopened(&mut next).host().charts(sheet).0[0].plots[0].series.len(),1);
}

/// Invalid, external, oversized, stale and two-dimensional sources refuse atomically.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__invalid_series_sources_do_not_mutate() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;
    let revision=book.host().revision();let history=book.history_depth();
    for source in ["", "Missing!A1:A3","[other.xlsx]Data!A1:A3","Data!A1:B3","Data!A1:A32768","SUM(Data!A1:A3)"] {
        let command=series_command(&book,0,Some(0),Some("Should not commit"),None,Some(source),false);
        assert!(!book.dispatch(SkinIntent::Command(command)).accepted(),"{source}");
        assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,before);
    }
    let mut command=series_command(&book,0,Some(0),Some("Stale"),None,None,false);
    if let calc_host::WorkbookCommand::ChartSeries{revision,..}=&mut command {*revision+=1;}
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    let command=series_command(&book,0,Some(0),Some("=Data!B1:B2"),None,None,false);
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
}

/// A newly authored source is transformed once when structural edits happen before its first Save.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__new_series_and_ranges_compose_with_unsaved_axis_edits() {
    let mut book=session();let sheet=book.host().sheets()[0].id();
    for series in [Some(0),None] {
        let command=series_command(&book,0,series,Some("Unsaved source"),Some("Data!A2:A7"),Some("Data!C2:C7"),false);
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    }
    for (change,position) in [(calc_host::AxisChange::Insert,2),(calc_host::AxisChange::Delete,4)] {
        assert!(book.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,change,position,1).unwrap())).accepted());
    }
    let shifted=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    let command=series_command(&book,0,Some(0),Some("Refuse after structural edit"),None,Some("Data!B2:B7"),false);
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,shifted);
    assert_eq!(reopened(&mut book).host().charts(sheet).0,shifted);
}

fn axis_command(book:&HostSession,chart:usize,id:u32,min:Option<&str>,max:Option<&str>,unit:Option<&str>,reversed:bool)->calc_host::WorkbookCommand {
    let sheet=book.host().sheets()[0].id();
    calc_host::WorkbookCommand::ChartAxis{sheet,revision:book.host().revision().get(),part:book.host().charts(sheet).0[chart].part.clone(),axis:id,minimum:min.map(str::to_owned),maximum:max.map(str::to_owned),major_unit:unit.map(str::to_owned),reversed}
}
/// Bounds, unit and direction are one authored edit; other axes and chart records stay intact.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__axis_settings_history_and_saved_reopening() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;let history=book.history_depth();
    let id=before[0].axes.iter().find(|a|a.kind=="valAx").unwrap().id;
    let command=axis_command(&book,0,id,Some("0"),Some("20"),Some("5"),true);
    let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");
    let after=book.host().charts(sheet).0;let axis=after[0].axes.iter().find(|a|a.id==id).unwrap();
    assert_eq!(axis.minimum.as_deref(),Some("0"));assert_eq!(axis.maximum.as_deref(),Some("20"));assert_eq!(axis.major_unit.as_deref(),Some("5"));assert!(axis.reversed);
    assert_eq!(after[0].plots,before[0].plots);assert_eq!(after[1..],before[1..]);assert_eq!(book.history_depth(),(history.0+1,0));
    let scale=calc_host::chart::chart_scale(0.,27.,Some(axis)).unwrap();assert_eq!((scale.minimum,scale.maximum,scale.step,scale.reversed),(0.,20.,5.,true));
    assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0,before);
    assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(book.host().charts(sheet).0,after);
    let mut next=reopened(&mut book);assert_eq!(next.host().charts(sheet).0,after);
    let command=axis_command(&next,0,id,None,None,None,false);assert!(next.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(reopened(&mut next).host().charts(sheet).0,before,"Automatic restores absent bounds and units after a saved reopen");
}
/// Scatter X and Y axes remain independent, and both settings compose with later source shifts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__scatter_axes_and_unsaved_structural_edits_compose() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let view=book.host().charts(sheet).0[5].clone();
    let x=view.axes.iter().find(|a|a.position.as_deref()==Some("b")).unwrap().id;let y=view.axes.iter().find(|a|a.position.as_deref()==Some("l")).unwrap().id;
    for (id,max,unit) in [(x,"50","10"),(y,"20","5")] {let command=axis_command(&book,5,id,Some("0"),Some(max),Some(unit),false);let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");}
    assert!(book.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,calc_host::AxisChange::Insert,2,1).unwrap())).accepted());
    let after=book.host().charts(sheet).0;assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
    let axes=&after[5].axes;assert_eq!(axes.iter().find(|a|a.id==x).unwrap().maximum.as_deref(),Some("50"));assert_eq!(axes.iter().find(|a|a.id==y).unwrap().maximum.as_deref(),Some("20"));
}
/// Invalid scales and stale identities cannot change revision, history or persisted chart data.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__invalid_axis_settings_refuse_atomically() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;let history=book.history_depth();let revision=book.host().revision();let id=before[0].axes.iter().find(|a|a.kind=="valAx").unwrap().id;
    for (min,max,unit) in [("20","10","5"),("0","20","0"),("0","20","-1"),("NaN","20","5"),("0","inf","5"),("0","20","0.000001")] {
        let command=axis_command(&book,0,id,Some(min),Some(max),Some(unit),true);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
        assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,before);
    }
    let mut command=axis_command(&book,0,id,Some("0"),Some("20"),Some("5"),false);if let calc_host::WorkbookCommand::ChartAxis{revision,..}=&mut command{*revision+=1;}
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    let command=axis_command(&book,0,u32::MAX,None,None,None,false);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
}

/// Axis validation uses exactly the paired points and numbered-X fallback used for drawing.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__scatter_fallback_extents_validate_the_actual_display() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let chart=book.host().charts(sheet).0[5].clone();
    let command=calc_host::WorkbookCommand::ChartSeries{sheet,revision:book.host().revision().get(),part:chart.part.clone(),plot:0,series:Some(0),name:None,categories:Some(String::new()),values:None,remove:false};
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let plot=&book.host().charts(sheet).0[5].plots[0];
    assert_eq!(calc_host::chart::scatter_ranges(plot).0,(0.,6.));
    let x=chart.axes.iter().find(|a|a.position.as_deref()==Some("b")).unwrap().id;
    let revision=book.host().revision();let history=book.history_depth();
    let command=axis_command(&book,5,x,Some("10"),None,Some("0.005"),false);
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
    let command=axis_command(&book,5,x,Some("0.5"),Some("10.5"),Some("2.5"),false);
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let view=book.host().charts(sheet).0;assert_eq!(reopened(&mut book).host().charts(sheet).0,view);
}

fn type_command(book:&HostSession,chart:usize,kind:&str,stacked:bool)->calc_host::WorkbookCommand {
    let sheet=book.host().sheets()[0].id();calc_host::WorkbookCommand::ChartType{sheet,revision:book.host().revision().get(),part:book.host().charts(sheet).0[chart].part.clone(),plot:0,kind:kind.into(),stacked}
}
/// Category conversions preserve sources, use one history step and save the requested type and axes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__category_types_history_and_saved_reopening() {
    let mut book=session();let sheet=book.host().sheets()[0].id();
    for (kind,stacked,expected) in [("line",false,ChartKind::Line),("area",true,ChartKind::Area),("bar",true,ChartKind::Bar),("column",false,ChartKind::Column)] {
        let before=book.host().charts(sheet).0;let history=book.history_depth();let command=type_command(&book,0,kind,stacked);
        let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");
        let after=book.host().charts(sheet).0;assert_eq!(after[0].plots[0].kind,expected);assert_eq!(after[0].plots[0].stacked,stacked);assert_eq!(after[0].plots[0].series,before[0].plots[0].series);assert_eq!(after[1..],before[1..]);
        assert_eq!(after[0].axes.iter().find(|a|a.kind=="valAx").unwrap().position.as_deref(),Some(if kind=="bar"{"b"}else{"l"}));
        assert_eq!(book.history_depth(),(history.0+1,0));assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0,before);assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(book.host().charts(sheet).0,after);
        book=reopened(&mut book);assert_eq!(book.host().charts(sheet).0,after);
    }
}
/// Pie/doughnut conversion writes valid type-specific records without disturbing data or other charts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__pie_doughnut_defaults_and_line_specific_options() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let original=book.host().charts(sheet).0;
    for (kind,expected) in [("doughnut",ChartKind::Doughnut),("pie",ChartKind::Pie)] {
        let command=type_command(&book,4,kind,false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let after=book.host().charts(sheet).0;assert_eq!(after[4].plots[0].kind,expected);assert_eq!(after[4].plots[0].series,original[4].plots[0].series);
        book=reopened(&mut book);assert_eq!(book.host().charts(sheet).0,after);
        let snapshot=book.host().loaded().snapshot(book.host().model());let xml=snapshot.package().read_xml(&after[4].part).unwrap();let xml=String::from_utf8(xml.bytes().to_vec()).unwrap();
        assert_eq!(xml.contains("holeSize"),kind=="doughnut");assert!(xml.contains(if kind=="doughnut"{"<c:doughnutChart"}else{"<c:pieChart"}));
    }
    let command=type_command(&book,2,"bar",false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    book=reopened(&mut book);let snapshot=book.host().loaded().snapshot(book.host().model());let xml=snapshot.package().read_xml(&original[2].part).unwrap();let xml=String::from_utf8(xml.bytes().to_vec()).unwrap();
    assert!(xml.contains("<c:barChart"));assert!(!xml.contains("<c:smooth"));assert!(!xml.contains("<c:marker"));
}
/// A type change composes with new sources, added series and the existing structural save pass.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__types_sources_and_unsaved_row_insertion_compose() {
    let mut book=session();let sheet=book.host().sheets()[0].id();
    for series in [Some(0),None]{let command=series_command(&book,0,series,Some("Typed & linked data"),Some("Data!A2:A7"),Some("Data!C2:C7"),false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());}
    let command=type_command(&book,0,"line",false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    assert!(book.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,calc_host::AxisChange::Insert,2,1).unwrap())).accepted());
    let after=book.host().charts(sheet).0;assert_eq!(after[0].plots[0].series.len(),3);assert_eq!(after[0].plots[0].series[0].values[0],Some(8.));assert_eq!(after[0].plots[0].kind,ChartKind::Line);
    assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
}
/// Unsupported families and stale targets refuse before any source or history mutation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_46__unsupported_type_changes_refuse_atomically() {
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    for (chart,kind,stacked) in [(0,"scatter",false),(0,"pie",false),(5,"line",false),(4,"column",false),(4,"pie",true),(0,"radar",false)] {
        let command=type_command(&book,chart,kind,stacked);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().charts(sheet).0,before);assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
    }
    let mut command=type_command(&book,0,"line",false);if let calc_host::WorkbookCommand::ChartType{revision,..}=&mut command{*revision+=1;}assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    let command=calc_host::WorkbookCommand::ChartSeries{sheet,revision:revision.get(),part:before[4].part.clone(),plot:0,series:None,name:Some("Not added".into()),categories:Some("Data!A2:A7".into()),values:Some("Data!B2:B7".into()),remove:false};assert!(!book.dispatch(SkinIntent::Command(command)).accepted());
    assert_eq!(book.host().charts(sheet).0,before);assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
}

fn create_command(book:&HostSession,range:&str,kind:&str,rows:bool,headers:bool,categories:bool)->calc_host::WorkbookCommand {
    let sheet=book.host().sheets()[0].id();let (first,last)=range.split_once(':').unwrap_or((range,range));
    calc_host::WorkbookCommand::ChartCreate{revision:book.host().revision().get(),area:calc_value::GridArea::new(at(sheet,first),at(sheet,last)).unwrap(),kind:kind.into(),rows,headers,categories}
}

/// Creating a chart appends to an existing drawing, follows live edits, and participates in history.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_49__selection_creation_keeps_imported_frames_and_history(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let original=book.host().charts(sheet).0;
    let original_package=book.host().loaded().snapshot(book.host().model());
    let history=book.history_depth();
    let command=create_command(&book,"A1:C7","column",false,true,true);
    let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");
    assert_eq!(book.history_depth(),(history.0+1,0));
    let charts=book.host().charts(sheet).0;assert_eq!(charts.len(),7);assert_eq!(&charts[..6],original);
    let added=&charts[6];assert_eq!(added.plots[0].series[0].name,"North");assert_eq!(labels(added,0),"Jan;Feb;Mar;Apr;May;Jun");assert_eq!(numbers(&added.plots[0].series[1].values),"8;11;14;10;19;13");
    assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0,original);
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,"B2"),text:"43".into()}).accepted());
    assert_eq!(book.host().charts(sheet).0[6].plots[0].series[0].values[0],Some(43.));
    let reopened=reopened(&mut book);let charts=reopened.host().charts(sheet).0;
    assert_eq!(charts.len(),7);assert_eq!(charts[6].plots[0].series[0].values[0],Some(43.));assert!(!charts[6].plots[0].series[0].cached);
    let saved=reopened.host().loaded().snapshot(reopened.host().model());
    for chart in &original {
        assert_eq!(original_package.package().read_xml(&chart.part).unwrap().bytes(),saved.package().read_xml(&chart.part).unwrap().bytes(),"imported chart part {}",chart.part);
    }
}

/// The first drawing and subsequent frames save through the application's ordinary blank package.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_49__first_chart_multiple_types_and_row_orientation_save(){
    let mut book=HostSession::open(Arc::new(MemorySource::new(calc_host::blank_package())),&OpenPolicy::default(),HostOptions::default()).unwrap();
    let sheet=book.host().sheets()[0].id();
    for (address,text) in [("A1","Month"),("B1","North"),("C1","South"),("A2","Jan"),("B2","2"),("C2","=B2*3"),("A3","Feb"),("B3","8"),("C3","=B3/2")]{assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,address),text:text.into()}).accepted());}
    for (kind,range,rows,headers,cats) in [("column","A1:C3",false,true,true),("bar","A1:C3",false,true,true),("line","A1:C3",true,true,true),("area","A1:C3",false,true,true),("pie","A1:B3",false,true,true),("doughnut","A1:C2",true,true,true),("scatter","B1:C3",false,true,true)]{
        let command=create_command(&book,range,kind,rows,headers,cats);let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{kind}: {outcome:?}");
    }
    let before=book.host().charts(sheet).0;assert_eq!(before.len(),7);assert_eq!(numbers(&before[2].plots[0].series[0].values),"2;6");assert_eq!(before[2].plots[0].series[0].name,"Jan");
    assert_eq!(before[6].plots[0].series[0].values,vec![Some(6.),Some(4.)]);
    let next=reopened(&mut book);let after=next.host().charts(sheet).0;assert_eq!(before,after);assert_eq!(next.host().charts(sheet).1,0);
}

/// Invalid selections and stale commands cannot mutate state or history.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_49__creation_refusals_are_atomic(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let original=book.host().charts(sheet).0;let history=book.history_depth();let revision=book.host().revision();
    for (range,kind,rows,headers,cats) in [("A1:C7","pie",false,true,true),("A1","column",false,true,false),("A1","column",false,false,true),("A1:C7","scatter",false,true,false),("A1:C7","unknown",false,true,true),("A1:JX32768","column",false,true,true)]{
        let command=create_command(&book,range,kind,rows,headers,cats);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,original);
    }
    let command=create_command(&book,"A1:C7","column",false,true,true);assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,"Z1"),text:"1".into()}).accepted());let history=book.history_depth();
    assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,original);
}

/// New chart references and anchors compose with later structural edits through the retained passes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_49__created_chart_edits_and_row_insertion_save(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let command=create_command(&book,"A1:C7","column",false,true,true);
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let part=book.host().charts(sheet).0[6].part.clone();
    let command=calc_host::WorkbookCommand::ChartProperties{sheet,revision:book.host().revision().get(),part,title:Some("Sales & forecast".into()),set_title:true,legend:Some(false)};
    assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    assert!(book.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,calc_host::AxisChange::Insert,1,2).unwrap())).accepted());
    let expected=book.host().charts(sheet).0[6].clone();
    let next=reopened(&mut book);assert_eq!(next.host().charts(sheet).0[6],expected);
    assert_eq!(expected.title.as_deref(),Some("Sales & forecast"));assert_eq!(expected.plots[0].series[0].values[0],Some(12.));
    let chart=next.host().model().definitions().charts(sheet)[6].clone();
    assert_eq!(chart.plots[0].series[0].values.as_ref().unwrap().formula.as_deref(),Some("'Data'!$B$4:$B$9"));
}

/// Newly authored references receive the same rename/remove protection as imported charts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_49__new_chart_blocks_unmodelled_sheet_rename_and_remove(){
    let mut book=HostSession::open(Arc::new(MemorySource::new(calc_host::blank_package())),&OpenPolicy::default(),HostOptions::default()).unwrap();
    assert!(book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet{sheet:None,change:calc_host::SheetChange::Add{name:"Other".into()}})).accepted());
    let sheet=book.host().sheets()[0].id();
    assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,"A1"),text:"3".into()}).accepted());
    let command=create_command(&book,"A1","column",false,false,false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let charts=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    for change in [calc_host::SheetChange::Rename{name:"Renamed".into()},calc_host::SheetChange::Remove]{
        let outcome=book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet{sheet:Some(sheet),change}));assert!(!outcome.accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,charts);
    }
    assert_eq!(reopened(&mut book).host().charts(sheet).0,charts);
}

fn log_command(book:&HostSession,chart:usize,axis:u32,base:Option<&str>)->calc_host::WorkbookCommand{
    let sheet=book.host().sheets()[0].id();calc_host::WorkbookCommand::ChartLogAxis{sheet,revision:book.host().revision().get(),part:book.host().charts(sheet).0[chart].part.clone(),axis,base:base.map(str::to_owned)}
}
/// Log scales, authored bounds, direction and automatic linear restoration use ordinary history.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_50__logarithmic_scale_history_and_saved_reopening(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;let id=before[0].axes.iter().find(|a|a.kind=="valAx").unwrap().id;
    let command=log_command(&book,0,id,Some("10"));let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");
    let command=axis_command(&book,0,id,Some("1"),Some("1000"),None,true);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let after=book.host().charts(sheet).0;let axis=after[0].axes.iter().find(|a|a.id==id).unwrap();assert_eq!(axis.log_base.as_deref(),Some("10"));
    let scale=calc_host::chart::chart_scale(12.,40.,Some(axis)).unwrap();assert_eq!(scale.ticks(),vec![1.,10.,100.,1000.]);assert!((scale.position(10.).unwrap()-2./3.).abs()<1e-12);assert!((scale.position(100.).unwrap()-1./3.).abs()<1e-12);assert_eq!(scale.position(0.),None);assert_eq!(scale.position(-5.),None);
    assert!(book.dispatch(SkinIntent::Undo).accepted());assert!(!book.host().charts(sheet).0[0].axes.iter().find(|a|a.id==id).unwrap().reversed);assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(book.host().charts(sheet).0,after);
    let mut next=reopened(&mut book);assert_eq!(next.host().charts(sheet).0,after);let revision=next.host().revision();let history=next.history_depth();let command=type_command(&next,0,"column",true);assert!(!next.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(next.host().revision(),revision);assert_eq!(next.history_depth(),history);assert_eq!(next.host().charts(sheet).0,after);let command=log_command(&next,0,id,None);assert!(next.dispatch(SkinIntent::Command(command)).accepted());let command=axis_command(&next,0,id,None,None,None,false);assert!(next.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(reopened(&mut next).host().charts(sheet).0,before);
}
/// Invalid logarithmic settings refuse atomically; scatter X/Y keep their own scales.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_50__logarithmic_invalid_and_scatter_axes(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let chart=book.host().charts(sheet).0[5].clone();let x=chart.axes.iter().find(|a|a.position.as_deref()==Some("b")).unwrap().id;let y=chart.axes.iter().find(|a|a.position.as_deref()==Some("l")).unwrap().id;let revision=book.host().revision();let history=book.history_depth();
    for base in ["1","0","-10","1001","NaN","inf",""]{let command=log_command(&book,5,x,Some(base));assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);}
    let command=log_command(&book,5,x,Some("2"));assert!(book.dispatch(SkinIntent::Command(command)).accepted());let command=log_command(&book,5,y,Some("10"));assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let after=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    for min in ["0","-1"]{let command=axis_command(&book,5,x,Some(min),Some("100"),None,false);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,after);}
    assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
}
/// A newly created chart saves logarithmic scaling and filters only the impossible values.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_50__new_log_chart_and_positive_extents(){
    let mut book=HostSession::open(Arc::new(MemorySource::new(calc_host::blank_package())),&OpenPolicy::default(),HostOptions::default()).unwrap();let sheet=book.host().sheets()[0].id();
    for (address,text) in [("A1","1"),("A2","10"),("A3","100"),("A4","0"),("A5","-5")]{assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,address),text:text.into()}).accepted());}
    let command=create_command(&book,"A1:A5","line",false,false,false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());let command=log_command(&book,0,2,Some("10"));assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let after=book.host().charts(sheet).0;assert_eq!(calc_host::chart::plot_scale_range(&after[0].plots[0],true).unwrap(),(1.,100.));assert_eq!(after[0].plots[0].series[0].values,vec![Some(1.),Some(10.),Some(100.),Some(0.),Some(-5.)]);assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
    let mut p=after[0].plots[0].clone();p.stacked=true;assert!(calc_host::chart::plot_scale_range(&p,true).is_err());
}


/// One created combination has live linked sources, two reciprocal pairs and independent scales.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_51__create_combination_axes_and_saved_reopening(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;
    let command=create_command(&book,"A1:C7","combo",false,true,true);let outcome=book.dispatch(SkinIntent::Command(command));assert!(outcome.accepted(),"{outcome:?}");
    let charts=book.host().charts(sheet).0;let c=&charts[6];assert!(calc_host::chart::combination_supported(c));assert_eq!(c.axes.len(),4);assert_eq!(c.plots.len(),2);assert_eq!(c.plots[0].kind,ChartKind::Column);assert_eq!(c.plots[1].kind,ChartKind::Line);assert_eq!(c.plots[0].series[0].name,"North");assert_eq!(c.plots[1].series[0].name,"South");assert_eq!(&charts[..6],before);
    let command=axis_command(&book,6,2,None,Some("50"),Some("10"),false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());let command=axis_command(&book,6,4,None,Some("200"),Some("50"),false);assert!(book.dispatch(SkinIntent::Command(command)).accepted());
    let chart=book.host().charts(sheet).0[6].clone();assert_eq!(calc_host::chart::category_scale(&chart,&chart.plots[0]).unwrap().maximum,50.);assert_eq!(calc_host::chart::category_scale(&chart,&chart.plots[1]).unwrap().maximum,200.);
    let command=log_command(&book,6,4,Some("10"));assert!(book.dispatch(SkinIntent::Command(command)).accepted());let after=book.host().charts(sheet).0;assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
    assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0[6].axes.iter().find(|a|a.id==4).unwrap().log_base,None);assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(book.host().charts(sheet).0,after);
    assert!(book.dispatch(SkinIntent::EnterCell{at:at(sheet,"C2"),text:"125".into()}).accepted());assert_eq!(book.host().charts(sheet).0[6].plots[1].series[0].values[0],Some(125.));
    assert!(book.dispatch(SkinIntent::EditAxis(calc_host::AxisEdit::new(sheet,calc_host::Axis::Rows,calc_host::AxisChange::Insert,1,2).unwrap())).accepted());let after=book.host().charts(sheet).0;assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
}
/// A one-series combination cannot silently become a different type or change history.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_51__combination_selection_refuses_atomically(){
    let mut book=session();let sheet=book.host().sheets()[0].id();let before=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    let command=create_command(&book,"A1:B7","combo",false,true,true);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,before);
    let command=type_command(&book,0,"combo",false);assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);
}

/// Legend placement is an ordinary retained property across history, saving and invalid input.
#[cfg_attr(not(target_arch="wasm32"),test)]
#[cfg_attr(target_arch="wasm32",wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_52__legend_placement_history_and_saved_reopening(){
    let mut book=session();let sheet=book.host().sheets()[0].id();
    for position in ["l","t","b","tr","r"]{
        let before=book.host().charts(sheet).0;let command=calc_host::WorkbookCommand::ChartLegend{sheet,revision:book.host().revision().get(),part:before[0].part.clone(),position:position.into()};assert!(book.dispatch(SkinIntent::Command(command)).accepted());let after=book.host().charts(sheet).0;assert_eq!(after[0].legend.as_deref(),Some(position));assert!(book.dispatch(SkinIntent::Undo).accepted());assert_eq!(book.host().charts(sheet).0,before);assert!(book.dispatch(SkinIntent::Redo).accepted());assert_eq!(reopened(&mut book).host().charts(sheet).0,after);
    }
    let before=book.host().charts(sheet).0;let revision=book.host().revision();let history=book.history_depth();
    for (rev,position) in [(revision.get(),"unknown"),(revision.get().saturating_sub(1),"l")]{let command=calc_host::WorkbookCommand::ChartLegend{sheet,revision:rev,part:before[0].part.clone(),position:position.into()};assert!(!book.dispatch(SkinIntent::Command(command)).accepted());assert_eq!(book.host().revision(),revision);assert_eq!(book.history_depth(),history);assert_eq!(book.host().charts(sheet).0,before);}
}
