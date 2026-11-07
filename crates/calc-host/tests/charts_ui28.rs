//! DNA Calc-created combination charts against the Excel 16.0.20527 combination charts of UI28
//! (tests/fixtures/ui28/combos-skeleton.txt, from files/charts/combos-created.xlsx): the order of
//! the plots, the grouping and overlap of each, and the order and settings of the axes.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CellKey, GridArea};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, SavePolicy, SkinIntent,
    WorkbookCommand,
};
use std::sync::Arc;

const WORKBOOK: &[u8] = include_bytes!("fixtures/dc_6yw/charts.xlsx");
const EXCEL: &str = include_str!("fixtures/ui28/combos-skeleton.txt");

fn part_text(package: &[u8], name: &str) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("the saved package opens");
    String::from_utf8(document.package().read_part(name).expect("the part")).unwrap()
}

/// The chart XML of the chart `kind` creates over the first columns of the sheet.
fn created(kind: &str) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(WORKBOOK.to_vec()));
    let mut book = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("it opens");
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let sheet = book.host().sheets()[0].id();
    let area = GridArea::new(
        CellKey::new(sheet, 1, 1).unwrap(),
        CellKey::new(sheet, 7, 4).unwrap(),
    )
    .unwrap();
    let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::ChartCreate {
        revision: book.host().revision().get(),
        area,
        kind: kind.into(),
        rows: false,
        headers: true,
        categories: true,
    }));
    assert!(receipt.accepted(), "{kind}: {receipt:?}");
    let part = book.host().charts(sheet).0.pop().unwrap().part;
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).expect("it saves");
    if let Ok(dir) = std::env::var("DNACALC_UI30_CHARTS") {
        std::fs::write(format!("{dir}/{kind}.xlsx"), &saved).unwrap();
    }
    part_text(&saved, part.trim_start_matches('/'))
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)?;
    let rest = &text[start + open.len()..];
    Some(&rest[..rest.find(close)?])
}
fn value<'a>(element: &'a str, tag: &str) -> Option<&'a str> {
    // The writer repeats the namespace declaration on child elements: find val= inside the tag.
    let start = element.find(&format!("<c:{tag} "))?;
    let tag_text = &element[start..start + element[start..].find('>')?];
    between(tag_text, "val=\"", "\"")
}

/// `plots|axes`: each plot as `kind:grouping:overlap:axis,axis` and each axis as
/// `kind:position:deleted:crosses:crossBetween:crossAxis`, axis ids replaced by their order.
pub fn skeleton(xml: &str) -> String {
    let plot_area = between(xml, "<c:plotArea>", "</c:plotArea>").expect("a plot area");
    let mut ids = Vec::<String>::new();
    let mut id = |text: &str| -> usize {
        match ids.iter().position(|x| x == text) {
            Some(p) => p,
            None => {
                ids.push(text.to_owned());
                ids.len() - 1
            }
        }
    };
    let axes_start = ["<c:catAx>", "<c:valAx>", "<c:dateAx>"]
        .iter()
        .filter_map(|t| plot_area.find(t))
        .min()
        .unwrap_or(plot_area.len());
    // Number axis ids by the order the axes are written, so plots name them consistently.
    let mut axes = Vec::new();
    let mut rest = &plot_area[axes_start..];
    while let Some(at) = rest.find("<c:") {
        let tag_end = rest[at + 3..].find('>').unwrap() + at + 3;
        let tag = &rest[at + 3..tag_end];
        if !matches!(tag, "catAx" | "valAx" | "dateAx") {
            rest = &rest[tag_end..];
            continue;
        }
        let close = format!("</c:{tag}>");
        let end = rest.find(&close).unwrap();
        let body = &rest[at..end];
        let me = id(value(body, "axId").unwrap());
        let cross = value(body, "crossAx").unwrap().to_owned();
        axes.push((tag.to_owned(), me, cross, body.to_owned()));
        rest = &rest[end + close.len()..];
    }
    let mut axis_text = Vec::new();
    for (tag, me, cross, body) in &axes {
        let cross = id(cross);
        axis_text.push(format!(
            "{tag}{me}:{}:{}:{}:{}:{cross}",
            value(body, "axPos").unwrap_or("-"),
            value(body, "delete").unwrap_or("0"),
            value(body, "crosses").unwrap_or("-"),
            value(body, "crossBetween").unwrap_or("-"),
        ));
    }
    let mut plots = Vec::new();
    let mut rest = &plot_area[..axes_start];
    while let Some(at) = rest.find("<c:") {
        let tag_end = rest[at + 3..].find('>').unwrap() + at + 3;
        let tag = &rest[at + 3..tag_end];
        if !tag.ends_with("Chart") {
            rest = &rest[tag_end..];
            continue;
        }
        let close = format!("</c:{tag}>");
        let end = rest.find(&close).unwrap();
        let body = &rest[at..end];
        let mut ax = Vec::new();
        let mut scan = body;
        while let Some(i) = scan.find("<c:axId ") {
            let tail = &scan[i..];
            let tail = &tail[tail.find("val=\"").unwrap() + 5..];
            let v = &tail[..tail.find('"').unwrap()];
            ax.push(id(v).to_string());
            scan = &tail[v.len()..];
        }
        plots.push(format!(
            "{tag}:{}:{}:{}",
            value(body, "grouping").unwrap_or("-"),
            value(body, "overlap").unwrap_or("-"),
            ax.join(",")
        ));
        rest = &rest[end + close.len()..];
    }
    format!("{}|{}", plots.join(";"), axis_text.join(";"))
}

/// Which Excel combination (by `name` in the fixture) each DNA Calc kind corresponds to when its
/// plots take the same axes: separate axes, the second plot on the secondary pair.
const KINDS: &[(&str, &str)] = &[
    ("combo", "column-line-normal-ab-separate"),
    ("combo-line-column", "column-line-normal-ba-separate"),
    ("combo-area-line", "area-line-normal-ab-separate"),
    ("combo-column-area", "column-area-normal-ab-separate"),
    ("combo-area-column", "column-area-normal-ba-separate"),
];

/// The skeleton with its plots in the order Excel writes them: area, bar, line.
fn excel_plot_order(skeleton: &str) -> String {
    let (plots, axes) = skeleton.split_once('|').unwrap();
    let mut plots: Vec<_> = plots.split(';').collect();
    plots.sort_by_key(|p| match p.split(':').next().unwrap() {
        "areaChart" => 0,
        "barChart" => 1,
        "lineChart" => 2,
        _ => 3,
    });
    format!("{}|{axes}", plots.join(";"))
}

#[test]
/// Native-only: the report prints every kind; the matching kinds are asserted.
fn ui28__created_combination_charts_against_excel() {
    let excel: std::collections::BTreeMap<_, _> =
        EXCEL.lines().filter_map(|l| l.split_once(char::from(9u8))).collect();
    let (mut exact, mut reordered) = (Vec::new(), Vec::new());
    for (kind, name) in KINDS {
        let want = excel[name];
        let xml = created(kind);
        if std::env::var("DNACALC_UI30_XML").is_ok() {
            println!("{xml}");
        }
        let got = skeleton(&xml);
        println!("{kind}
  excel {want}
  dna   {got}");
        if got == want {
            exact.push(*kind);
        } else if excel_plot_order(&got) == want {
            reordered.push(*kind);
        }
    }
    println!("exact {exact:?}, plot order only {reordered:?}");
    // Axes, groupings, overlap and axis ids are Excel's for all five kinds. DNA Calc writes the
    // plots in the order they were made, so two kinds differ from Excel in plot order alone:
    // Excel sorts them (area, bar, line) and DNA Calc draws them in that order regardless.
    assert_eq!(exact, ["combo", "combo-area-line", "combo-area-column"]);
    assert_eq!(reordered, ["combo-line-column", "combo-column-area"]);
}

/// UI30 P10: the 100% stacked column, bar, line and area charts Excel saved
/// (tests/fixtures/ui30/charts-ui30-excel.xlsx, charts 1 to 4 over A1:D7 with three series) against
/// the charts DNA Calc creates over the same cells: plots, groupings, overlap, axes, and the
/// percent number format linked to the value axis.
#[test]
/// Native-only: builds charts through the host, one session each.
fn ui30__percent_stacked_charts_against_excel() {
    const EXCEL_CHARTS: &[u8] = include_bytes!("fixtures/ui30/charts-ui30-excel.xlsx");
    for (index, kind) in [(1, "column-percent"), (2, "bar-percent"), (3, "line-percent"), (4, "area-percent")] {
        let want = skeleton(&part_text(EXCEL_CHARTS, &format!("xl/charts/chart{index}.xml")));
        let xml = created(kind);
        let got = skeleton(&xml);
        assert_eq!(got, want, "{kind}");
        assert!(xml.contains("formatCode=\"0%\""), "{kind}: {xml}");
        assert!(xml.contains("percentStacked"), "{kind}");
    }
}
