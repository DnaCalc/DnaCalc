//! Workbook inputs shared by the Design Book scenes. Generation writes ordinary Excel data.

use calc_doc::model::value::{CellKey, ExcelText, GridArea, NameScope};
use calc_doc::model::{
    ArrayFormula, ArrayKind, CachedValue, CellInput, CellText, FiniteNumber, FormulaForm,
    FormulaText, IterationSettings, ModelRoot, OpBatch, SheetEdit, SheetVisibility, Table,
    TableStyle, Validation, WorkbookOp,
};
use calc_skin::{AnchorId, Bound, DocId, InputPolicy, Page, PageNode, PageSchema};
use std::collections::BTreeMap;
use std::path::Path;
use std::num::{NonZeroU8, NonZeroU128};
use std::sync::Arc;

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [flag, out] = args else {
        return Err("expected app cast calc --out <directory>".into());
    };
    if flag != "--out" {
        return Err("expected app cast calc --out <directory>".into());
    }
    let out = Path::new(out);
    std::fs::create_dir_all(out)?;
    for (name, generate) in FIXTURES {
        let (bytes, formulas) = generate()?;
        std::fs::write(out.join(format!("{name}.xlsx")), &bytes)?;
        println!("Cast {name}: {formulas} formulas, {} bytes", bytes.len());
    }
    Ok(())
}

pub(super) fn fixture(name: &str) -> Result<Vec<u8>, String> {
    let (_, generate) = FIXTURES.iter().find(|(key, _)| *key == name)
        .ok_or_else(|| format!("Unknown cast workbook {name}"))?;
    let (bytes, _) = generate().map_err(|error| error.to_string())?;
    Ok(bytes)
}

type BuildResult = Result<(Vec<u8>, usize), Box<dyn std::error::Error>>;

const FIXTURES: [(&str, fn() -> BuildResult); 7] = [
    ("Budget", budget),
    ("Sales", sales),
    ("SalesNotebook", sales_notebook),
    ("SalesBlocked", sales_blocked),
    ("Loan", loan),
    ("Stock", stock),
    ("Inherited", inherited),
];

struct Fixture {
    workbook: calc_doc::LoadedWorkbook,
    edits: Vec<WorkbookOp>,
    caches: BTreeMap<CellKey, CachedValue>,
    formulas: usize,
}

impl Fixture {
    fn new(sheets: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = calc_doc::blank_package_for_sheets(sheets)?;
        let package = calc_doc::open(
            Arc::new(calc_doc::MemorySource::new(bytes)),
            &calc_doc::OpenPolicy::default(),
        )?;
        let workbook = calc_doc::load(&package, &mut Default::default())?;
        Ok(Self {
            workbook,
            edits: Vec::new(),
            caches: BTreeMap::new(),
            formulas: 0,
        })
    }

    fn model(&self) -> &Arc<ModelRoot> {
        self.workbook.model()
    }

    fn at(&self, sheet: usize, row: u32, col: u32) -> CellKey {
        CellKey::new(self.model().sheets()[sheet].id(), row, col)
            .expect("cast coordinates fit Excel")
    }

    fn input(&mut self, sheet: usize, row: u32, col: u32, input: CellInput) {
        self.edits.push(WorkbookOp::SetCell {
            at: self.at(sheet, row, col),
            input,
        });
    }

    fn text(&mut self, sheet: usize, row: u32, col: u32, text: impl AsRef<str>) {
        self.input(
            sheet,
            row,
            col,
            CellInput::Text(CellText::new(excel_text(text.as_ref())).unwrap()),
        );
    }

    fn number(&mut self, sheet: usize, row: u32, col: u32, number: f64) {
        self.input(
            sheet,
            row,
            col,
            CellInput::Number(FiniteNumber::new(number).unwrap()),
        );
    }

    fn formula(&mut self, sheet: usize, row: u32, col: u32, formula: impl AsRef<str>) {
        self.formulas += 1;
        self.input(
            sheet,
            row,
            col,
            CellInput::Formula(FormulaText::new(excel_text(formula.as_ref())).unwrap()),
        );
    }

    fn spill(&mut self, sheet: usize, row: u32, column: u32, formula: &str) {
        self.formulas += 1;
        self.edits.push(WorkbookOp::SetArray {
            array: ArrayFormula::new(
                GridArea::cell(self.at(sheet, row, column)),
                ArrayKind::Dynamic { collapsed: false },
            ),
            input: CellInput::Formula(
                FormulaText::with_form(excel_text(formula), FormulaForm::Dynamic).unwrap(),
            ),
        });
    }

    fn row(&mut self, sheet: usize, row: u32, columns: &[&str]) {
        for (i, label) in columns.iter().enumerate() {
            self.text(sheet, row, i as u32 + 1, label);
        }
    }

    fn finish(self) -> BuildResult {
        let model = self.workbook.model();
        let (sheets, cells): (Vec<_>, _) = self
            .edits
            .into_iter()
            .partition(|op| matches!(op, WorkbookOp::EditSheet(_)));
        let mut model = calc_doc::prepare_edit(
            model,
            &OpBatch {
                base: model.revision(),
                operations: cells,
            },
        )?
        .into_model();
        if !sheets.is_empty() {
            model = calc_doc::prepare_sheet_edit(
                &model,
                &OpBatch {
                    base: model.revision(),
                    operations: sheets,
                },
                &calc_engine::ReferenceRewriter,
                &mut || unreachable!("cast sheets are already issued"),
            )?
            .into_model();
        }
        let snapshot = self.workbook.snapshot_with_values(&model, self.caches);
        let mut bytes = Vec::new();
        calc_doc::write(
            calc_doc::prepare_save(&snapshot, &Default::default())?,
            &mut bytes,
        )?;
        Ok((bytes, self.formulas))
    }
}

fn excel_text(text: &str) -> ExcelText {
    ExcelText::from_utf16_code_units(text.encode_utf16().collect())
}

fn budget() -> BuildResult {
    let mut w = Fixture::new(&["Inputs", "Monthly", "Summary"])?;
    w.row(0, 1, &["Assumption", "Base", "Stretch"]);
    let inputs = [
        ("Unit volume", 1_000., 1_100.),
        ("Unit price", 120., 125.),
        ("Variable unit cost", 70., 72.),
        ("Fixed costs", 32_000., 32_000.),
        ("Opening cash", 140_000., 140_000.),
        ("Tax rate", 0.27, 0.27),
        ("Debtor days", 30., 30.),
        ("Creditor days", 30., 30.),
        ("Inventory days", 20., 20.),
        ("Capex", 4_000., 5_000.),
        ("Interest rate", 0.10, 0.10),
        ("Working capital buffer", 15_000., 15_000.),
    ];
    for (i, (label, base, stretch)) in inputs.into_iter().enumerate() {
        let row = i as u32 + 2;
        w.text(0, row, 1, label);
        w.number(0, row, 2, base);
        w.number(0, row, 3, stretch);
    }
    w.row(
        1,
        1,
        &[
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
    );
    for (row, label) in [
        (2, "Unit volume"),
        (8, "Revenue"),
        (9, "Variable cost"),
        (10, "Fixed costs"),
        (11, "Operating profit"),
    ] {
        w.text(1, row, 1, label);
    }
    for col in 2..=13 {
        let c = char::from(b'A' + (col - 1) as u8);
        w.formula(1, 2, col, "Inputs!$B$2");
        w.formula(1, 8, col, format!("{c}2*Inputs!$B$3"));
        w.formula(1, 9, col, format!("{c}2*Inputs!$B$4"));
        w.formula(1, 10, col, "Inputs!$B$5");
        w.formula(1, 11, col, format!("{c}8-{c}9-{c}10"));
    }
    for (row, caption, source) in [
        (4, "Revenue", 8),
        (5, "Variable cost", 9),
        (6, "Fixed costs", 10),
        (7, "Operating profit", 11),
    ] {
        w.text(2, row, 1, caption);
        w.formula(2, row, 2, format!("SUM(Monthly!B{source}:M{source})"));
    }
    w.spill(2, 20, 1, "TRANSPOSE(Monthly!B1:M2)");
    w.text(2, 21, 1, "Remove this note to free the spill");
    w.formula(2, 14, 4, "DNA.DISCOUNT(Inputs!B3,0.1)");
    w.finish()
}

fn sales() -> BuildResult {
    sales_workbook(&["Orders", "Report", "Pivot"], r#"RTD("DnaCalc.Clock",,"LARK")"#)?.finish()
}

fn sales_workbook(sheets: &[&str], price: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
    let mut w = Fixture::new(sheets)?;
    let headers = [
        "Order", "Customer", "Region", "Status", "Owner", "Date", "Amount",
    ];
    w.row(0, 1, &headers);
    let groups = [
        ("East", "Paid", 8_291, 9_326_050),
        ("North", "Pending", 4_146, 4_663_500),
        ("North", "Paid", 4_146, 4_663_775),
        ("South", "Paid", 8_291, 9_326_075),
        ("West", "Paid", 125, 124_250),
    ];
    let mut synthetic = groups
        .into_iter()
        .flat_map(|(region, status, rows, total)| {
            (0..rows).map(move |i| (region, status, total / rows + i32::from(i < total % rows)))
        });
    for i in 1..=25_000 {
        let row = i + 1;
        let special = i == 10_482;
        let (region, status, amount) = if special {
            ("West", "Pending", 4_200)
        } else {
            synthetic.next().unwrap()
        };
        w.text(0, row, 1, format!("SO{i:05}"));
        w.text(
            0,
            row,
            2,
            if special {
                "Lark Hardware"
            } else {
                "Synthetic customer"
            },
        );
        w.text(0, row, 3, region);
        w.text(0, row, 4, status);
        w.text(0, row, 5, if special { "Maria" } else { "Analyst" });
        w.number(
            0,
            row,
            6,
            if special {
                46_331.
            } else {
                46_327. + f64::from(i % 28)
            },
        );
        w.number(0, row, 7, f64::from(amount));
    }
    let first = w.at(0, 1, 1);
    w.edits.push(WorkbookOp::SetTables(vec![Table {
        id: 1,
        style: TableStyle::default(),
        filter: None,
        totals_functions: Vec::new(),
        totals_labels: Vec::new(),
        opaque: false,
        axis_opaque: false,
        column_formulas: Vec::new(),
        name: "Orders".into(),
        display_name: "Orders".into(),
        part: "xl/tables/table1.xml".into(),
        sheet: first.sheet(),
        area: GridArea::new(first, w.at(0, 25_001, 7)).unwrap(),
        header_rows: 1,
        totals_rows: 0,
        columns: headers.into_iter().map(str::to_owned).collect(),
    }]));
    w.text(1, 5, 1, "West paid sales");
    w.formula(1, 5, 2, r#"SUMIFS(Orders[Amount],Orders[Region],"West")"#);
    w.formula(
        1,
        7,
        2,
        r#"XLOOKUP("SO10482",Orders[Order],Orders[Customer])"#,
    );
    w.spill(1, 12, 1, r#"FILTER(Orders,Orders[Status]="Pending")"#);
    w.formula(1, 2, 5, price);
    Ok(w)
}

fn sales_notebook() -> BuildResult {
    sales_analysis(false)
}

fn sales_blocked() -> BuildResult {
    sales_analysis(true)
}

fn sales_analysis(blocked: bool) -> BuildResult {
    const REPORT: usize = 1;
    const ANALYSIS: usize = 3;
    let mut w = sales_workbook(
        &["Orders", "Report", "Pivot", "Analysis"],
        r#"RTD("DnaCalc.Price",,"LARK","last")"#,
    )?;
    for (row, label) in [(2, "Region"), (3, "Status"), (4, "First day"), (5, "Last day"), (6, "Held units")] {
        w.text(ANALYSIS, row, 1, label);
    }
    w.text(ANALYSIS, 2, 2, if blocked { "West" } else { "All" });
    w.text(ANALYSIS, 3, 2, "Pending");
    for (row, number) in [(4, 1.), (5, 28.), (6, 40.)] {
        w.number(ANALYSIS, row, 2, number);
    }
    let define = |name: &str, formula: &str| WorkbookOp::SetNameFormula {
        name: name.into(),
        scope: NameScope::Workbook,
        formula: Some(formula.into()),
    };
    if blocked {
        w.formula(ANALYSIS, 16, 1, r#""Review with Sam""#);
        w.edits.push(define("reviewNote", "Analysis!$A$16"));
        w.text(ANALYSIS, 10, 25, "Reserved");
    } else {
        w.text(ANALYSIS, 10, 25, "Review with Sam");
    }
    let spill = [
        (1, 7, "scopeOrders", "$A$10", r#"_xlfn._xlws.FILTER(Orders,((Orders[Region]=$B$2)+($B$2="All"))*((Orders[Status]=$B$3)+($B$3="All"))*(Orders[Date]>=DATE(2026,11,$B$4))*(Orders[Date]<=DATE(2026,11,$B$5)))"#),
        (10, 2, "byRegion", "$J$10", "IF(ISERROR($A$10),$A$10,_xlfn.GROUPBY(_xlfn.CHOOSECOLS(scopeOrders,3),_xlfn.CHOOSECOLS(scopeOrders,7),_xleta.SUM,0,0,1))"),
        (13, 2, "byMonth", "$M$10", r#"IF(ISERROR($A$10),$A$10,_xlfn.PIVOTBY(_xlfn.CHOOSECOLS(scopeOrders,3),TEXT(_xlfn.CHOOSECOLS(scopeOrders,6),"yyyy-mm"),_xlfn.CHOOSECOLS(scopeOrders,7),_xleta.SUM,0,0,1,0,1))"#),
        (17, 2, "rankedOrders", "$Q$10", "IF(ISERROR($J$10),$J$10,_xlfn.SORTBY(byRegion,_xlfn.CHOOSECOLS(byRegion,2),-1,_xlfn.CHOOSECOLS(byRegion,1),1))"),
        (21, 2, "chartData", "$U$10", "IF(ISERROR($Q$10),$Q$10,rankedOrders)"),
    ];
    let mut part = calc_doc::model::ViewPart::default();
    let mut cells = Vec::new();
    for (row, label, control) in [
        (2, "Region", serde_json::json!({"type":"choice", "options":["All","East","North","South","West"]})),
        (3, "Status", serde_json::json!({"type":"choice", "options":["Pending","Paid","All"]})),
        (4, "First November day", serde_json::json!({"type":"integer", "min":1, "max":28})),
        (5, "Last November day", serde_json::json!({"type":"integer", "min":1, "max":28})),
    ] {
        let anchor = AnchorId(NonZeroU128::new(cells.len() as u128 + 1).unwrap());
        part = part.with_anchor(calc_doc::model::ViewAnchor::cells(anchor, GridArea::cell(w.at(ANALYSIS, row, 2))));
        cells.push(serde_json::json!({"id":anchor, "kind":"control", "label":label, "address":format!("Analysis!B{row}"), "control":control}));
    }
    let mut chart_data = None;
    for (column, width, name, address, formula) in spill {
        let anchor = AnchorId(NonZeroU128::new(cells.len() as u128 + 1).unwrap());
        if name == "chartData" { chart_data = Some(anchor); }
        w.spill(ANALYSIS, 10, column, formula);
        w.edits.push(define(name, &format!("_xlfn.ANCHORARRAY(Analysis!{address})")));
        part = part.with_anchor(calc_doc::model::ViewAnchor::spill(anchor, w.at(ANALYSIS, 10, column)));
        cells.push(serde_json::json!({"id": anchor, "kind": "formula", "owned": true, "lane_columns": width, "output_name": name}));
    }
    w.formula(ANALYSIS, 2, 10, "IF(ISERROR($A$10),$A$10,_xlfn.LET(items,scopeOrders,amounts,_xlfn.CHOOSECOLS(items,7),SUM(amounts)))");
    w.formula(ANALYSIS, 5, 10, "Report!E2*$B$6");
    for (sheet, row, column, name, address) in [
        (ANALYSIS, 2, 10, "scopeTotal", "Analysis!$J$2"),
        (REPORT, 2, 5, "latestPrice", "Report!$E$2"),
        (ANALYSIS, 5, 10, "positionValue", "Analysis!$J$5"),
    ] {
        let anchor = AnchorId(NonZeroU128::new(cells.len() as u128 + 1).unwrap());
        w.edits.push(define(name, address));
        let area = GridArea::cell(w.at(sheet, row, column));
        part = part.with_anchor(calc_doc::model::ViewAnchor::cells(anchor, area));
        cells.push(serde_json::json!({"id": anchor, "kind": "formula", "owned": sheet == ANALYSIS, "output_name": name}));
    }
    w.edits.push(define("Chart.Region", "Analysis!$U$10:INDEX(Analysis!$U:$U,9+ROWS(rankedOrders))"));
    w.edits.push(define("Chart.Amount", "Analysis!$V$10:INDEX(Analysis!$V:$V,9+ROWS(rankedOrders))"));
    let sheet = w.at(ANALYSIS, 1, 1).sheet();
    let chart_part = w.workbook.available_part_name("charts/dnaChart", "xml", &[]);
    w.edits.push(WorkbookOp::SetCharts { sheet, charts: vec![sales_chart(chart_part)] });
    let chart = AnchorId(NonZeroU128::new(cells.len() as u128 + 1).unwrap());
    let location = w.model().sheets()[ANALYSIS].key();
    part = part.with_anchor(calc_doc::model::ViewAnchor::object(chart, location, "SalesByRegion".into()));
    cells.push(serde_json::json!({
        "id": chart, "kind": "chart", "address": "obj:Analysis/SalesByRegion",
        "requires": chart_data,
    }));
    let prose = AnchorId(NonZeroU128::new(cells.len() as u128 + 1).unwrap());
    cells.push(serde_json::json!({"id": prose, "kind": "prose", "markdown": "### Sales in scope\nAmount: {=scopeTotal}"}));
    let document = calc_doc::model::ViewDocument {
        id: DocId(NonZeroU128::new(1).unwrap()),
        view: "notebook".into(),
        sheet: None,
        schema: 1,
        body: serde_json::to_string(&serde_json::json!({"layout": "vertical", "cells": cells}))?,
    };
    w.edits.push(WorkbookOp::EditView(Arc::new(part.with_document(document))));
    w.finish()
}

fn sales_chart(part: String) -> calc_doc::model::Chart {
    use calc_doc::model::{Chart, ChartAnchor, ChartAxis, ChartData, ChartPlot, ChartSeries, DrawingPoint};
    let axis = |id, cross_axis, kind: &str, position: &str| ChartAxis {
        id,
        cross_axis: Some(cross_axis),
        kind: kind.into(),
        position: Some(position.into()),
        ..ChartAxis::default()
    };
    let data = |formula: &str, numeric| ChartData { formula: Some(formula.into()), numeric, ..ChartData::default() };
    Chart {
        part,
        name: "SalesByRegion".into(),
        anchor: ChartAnchor::TwoCell {
            from: DrawingPoint { column: 23, row: 2, ..DrawingPoint::default() },
            to: DrawingPoint { column: 31, row: 17, ..DrawingPoint::default() },
        },
        edit_as: None,
        drawing_id: None,
        lang: None,
        unique_next: 3,
        title: Some("Sales by region".into()),
        title_formula: None,
        auto_title_deleted: false,
        legend: None,
        plots: vec![ChartPlot {
            kind: "barChart".into(),
            bar_direction: Some("bar".into()),
            grouping: Some("clustered".into()),
            series: vec![ChartSeries {
                categories: Some(data("Chart.Region", false)),
                values: Some(data("Chart.Amount", true)),
                ..ChartSeries::default()
            }],
            axis_ids: vec![1, 2],
            ..ChartPlot::default()
        }],
        axes: vec![axis(1, 2, "catAx", "l"), axis(2, 1, "valAx", "b")],
        text_style: Default::default(),
        area_fill: None,
        plot_fill: None,
        area_border: Default::default(),
        plot_border: Default::default(),
        element_text_complex: Default::default(),
        element_text: Default::default(),
    }
}

fn stop_rule(row: u32, kind: &str, op: &str, min: &str, max: Option<&str>) -> Validation {
    Validation {
        authored_copy: true,
        source: None,
        areas: vec![(row, 2, row, 2)],
        kind: kind.into(),
        operator: op.into(),
        allow_blank: false,
        show_input: false,
        show_error: true,
        in_cell_dropdown: false,
        error_style: "stop".into(),
        error_title: None,
        error: None,
        prompt_title: None,
        prompt: None,
        formula1: Some(min.into()),
        formula2: max.map(str::to_owned),
    }
}

fn loan() -> BuildResult {
    let mut w = Fixture::new(&["Loan"])?;
    for (row, label, value) in [
        (2, "Principal", 1_000_000.),
        (3, "Annual rate", 0.105),
        (4, "Term in months", 360.),
    ] {
        w.text(0, row, 1, label);
        w.number(0, row, 2, value);
    }
    w.edits.push(WorkbookOp::SetValidations {
        sheet: w.at(0, 1, 1).sheet(),
        rules: vec![
            stop_rule(2, "decimal", "greaterThan", "0", None),
            Validation {
                error: Some("Enter an annual rate between 0% and 100%.".into()),
                ..stop_rule(3, "decimal", "between", "0", Some("1"))
            },
            stop_rule(4, "whole", "between", "1", Some("360")),
        ],
    });
    w.text(0, 6, 1, "Payment");
    w.formula(0, 6, 2, "-PMT(B3/12,B4,B2)");
    w.text(0, 7, 1, "Total interest");
    w.formula(0, 7, 2, "B6*B4-B2");
    w.row(
        0,
        11,
        &[
            "Month",
            "Opening balance",
            "Payment",
            "Interest",
            "Principal",
            "Closing balance",
        ],
    );
    for row in 12..=371 {
        let prior = row - 1;
        let opening = if row == 12 {
            "$B$2".into()
        } else {
            format!("F{prior}")
        };
        w.formula(0, row, 1, format!(r#"IF(ROW()-11<=$B$4,ROW()-11,"")"#));
        for (col, value) in [
            (2, opening),
            (3, "$B$6".into()),
            (4, format!("B{row}*$B$3/12")),
            (5, format!("C{row}-D{row}")),
            (6, format!("B{row}-E{row}")),
        ] {
            w.formula(0, row, col, format!(r#"IF(A{row}="","",{value})"#));
        }
    }
    let mut view = calc_doc::model::ViewPart::default();
    let mut bind = |id: &str, label: &str, component: &str, slot: &str, address: &str, area: GridArea| {
        let next = view.anchors().count() as u128 + 1;
        let anchor = AnchorId(NonZeroU128::new(next).unwrap());
        view = view.clone().with_anchor(calc_doc::model::ViewAnchor::cells(anchor, area));
        PageNode::Part {
            id: id.into(),
            component: component.into(),
            bind: BTreeMap::from([(slot.into(), Bound { anchor, address: address.into() })]),
            props: BTreeMap::from([("label".into(), serde_json::Value::String(label.into()))]),
        }
    };
    let principal = bind("principal", "Principal", "input", "value", "Loan!B2", GridArea::cell(w.at(0, 2, 2)));
    let rate = bind("rate", "Annual rate", "input", "value", "Loan!B3", GridArea::cell(w.at(0, 3, 2)));
    let term = bind("term", "Term in months", "input", "value", "Loan!B4", GridArea::cell(w.at(0, 4, 2)));
    let payment = bind("payment", "Monthly payment", "value", "value", "Loan!B6", GridArea::cell(w.at(0, 6, 2)));
    let interest = bind("interest", "Total interest", "value", "value", "Loan!B7", GridArea::cell(w.at(0, 7, 2)));
    let schedule = bind("schedule", "Repayment schedule", "data-frame", "rows", "Loan!A12:F371",
        GridArea::new(w.at(0, 12, 1), w.at(0, 371, 6))?);
    let document = Page {
        schema: PageSchema::V1,
        title: "Loan Repayment Planner".into(),
        input: InputPolicy::Batched,
        root: PageNode::Grid {
            columns: NonZeroU8::new(2).unwrap(),
            children: vec![
                PageNode::Section { title: "Assumptions".into(), folded: false, children: vec![principal, rate, term] },
                PageNode::Section { title: "Repayment Summary".into(), folded: false, children: vec![payment, interest, schedule] },
            ],
        },
    };
    let view = view.with_document(calc_doc::model::ViewDocument {
        id: DocId(NonZeroU128::new(1).unwrap()),
        view: "app".into(), sheet: None, schema: 1,
        body: serde_json::to_string(&document)?,
    });
    w.edits.push(WorkbookOp::EditView(Arc::new(view)));
    w.finish()
}

fn stock() -> BuildResult {
    let mut w = Fixture::new(&["Stock"])?;
    for (row, label, value) in [
        (2, "Opening stock", 400.),
        (3, "Daily demand", 100.),
        (4, "Delivery lead time", 3.),
        (5, "Reorder point", 260.),
        (6, "Order quantity", 420.),
        (7, "Target reserve", 20.),
    ] {
        w.text(0, row, 1, label);
        w.number(0, row, 2, value);
    }
    w.text(0, 8, 1, "Iterative reserve");
    w.formula(0, 8, 2, "0.5*(B8+B7)");
    w.edits
        .push(WorkbookOp::SetIteration(IterationSettings::new(
            true, 100, 0.001,
        )));
    w.edits.push(WorkbookOp::SetValidations {
        sheet: w.at(0, 1, 1).sheet(),
        rules: vec![stop_rule(4, "whole", "between", "1", Some("30"))],
    });
    w.row(
        0,
        12,
        &[
            "Day",
            "Opening",
            "Arrivals",
            "Demand",
            "Fulfilled",
            "Closing",
            "Pipeline",
            "Order",
        ],
    );
    w.number(0, 13, 1, 0.);
    w.formula(0, 13, 6, "$B$2");
    w.number(0, 13, 8, 0.);
    for row in 14..=43 {
        let prior = row - 1;
        w.formula(0, row, 1, format!("A{prior}+1"));
        w.formula(0, row, 2, format!("F{prior}"));
        w.formula(
            0,
            row,
            3,
            format!("IF(A{row}<$B$4,0,INDEX($H$13:H{prior},A{row}-$B$4+1))"),
        );
        w.formula(0, row, 4, "$B$3");
        w.formula(0, row, 5, format!("MIN(B{row}+C{row},D{row})"));
        w.formula(0, row, 6, format!("B{row}+C{row}-E{row}"));
        w.formula(
            0,
            row,
            7,
            format!("SUM(INDEX($H$13:H{prior},MAX(1,A{row}-$B$4+2)):H{prior})"),
        );
        w.formula(0, row, 8, format!("IF(AND(F{row}<=$B$5,G{row}=0),$B$6,0)"));
    }
    w.finish()
}

fn inherited() -> BuildResult {
    let mut names = vec![
        "Summary".to_owned(),
        "Revenue".to_owned(),
        "Archive".to_owned(),
    ];
    names.extend((1..=37).map(|i| format!("Schedule {i:02}")));
    let mut w = Fixture::new(&names.iter().map(String::as_str).collect::<Vec<_>>())?;
    w.text(0, 18, 5, "Revenue");
    w.formula(0, 18, 6, "SUM(Revenue.Window)");
    w.caches.insert(
        w.at(0, 18, 6),
        CachedValue::Number(FiniteNumber::new(2_401_300.).unwrap()),
    );
    w.text(0, 19, 5, "Operating costs");
    w.number(0, 19, 6, 1_720_000.);
    // The saved value above is a deliberate stale fixture for the audit scene.
    w.edits.push(WorkbookOp::SetNameFormula {
        scope: NameScope::Workbook,
        name: "Revenue.Window".into(),
        formula: Some("OFFSET(Revenue!#REF!,0,0,12,1)".into()),
    });
    for sheet in 2..40 {
        if sheet >= 34 {
            w.edits
                .push(WorkbookOp::EditSheet(SheetEdit::SetVisibility {
                    sheet: w.at(sheet, 1, 1).sheet(),
                    visibility: SheetVisibility::VeryHidden,
                }));
        }
        for row in 1..=790 {
            for col in 1..=10 {
                w.formula(sheet, row, col, format!("ROW()*{col}+{sheet}"));
            }
        }
    }
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::sales;
    use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy};
    use calc_skin::grid::{Area, Key, Sheet};
    use calc_skin::{
        Batch, Budget, Change, Coverage, ModelChange, Origin, PartialWhy, RawValue, RowFilter,
        RowPage, RowSort, RowWindow, SourceRef, TablePart, Update, ViewId, WorkbookPort,
        WorkbookQuery, WritePolicy,
    };
    use calc_skin_host::SkinHost;
    use std::sync::Arc;

    #[test]
    fn sales_notebook_recalculates_the_full_chain_and_preserves_the_blocked_layout() {
        use calc_skin::query::{Cells, Query, Rows};
        use calc_skin::{Interest, Operation, RawValue, SpillExtent};

        for (fixture, blocked) in [("SalesNotebook", false), ("SalesBlocked", true)] {
            let bytes = super::fixture(fixture).unwrap();
            let session = HostSession::open(
                Arc::new(MemorySource::new(bytes)),
                &OpenPolicy::default(),
                HostOptions {
                    locale: calc_fml::FormatProfile::for_culture("en-US"),
                    ..HostOptions::default()
                },
            ).unwrap();
            let sheet = Sheet::of(session.host().sheets()[3].id());
            let at = |row, column| Key(sheet.0, sheet.1, row, column);
            let host = SkinHost::new(session);
            while host.advance() {}
            let port = host.connect(Origin::User { view: ViewId("notebook".into()) }, WritePolicy::Commit);
            let budget = Budget {
                cells: 100_000, nodes: 25_000, edges: 500, bytes: 1024 * 1024, millis: 5000,
            };
            let cell = |at| {
                let query = Cells { interest: Interest::Area(Area(at, at)) };
                Cells::decode(port.ask(query.request(), budget).unwrap()).unwrap()
                    .body.cells.into_iter().next().unwrap_or_else(|| panic!("expected a current cell at {at:?}"))
            };
            let rows = |anchor| {
                let query = Rows {
                    source: SourceRef::Spill { anchor },
                    filter: vec![],
                    sort: vec![],
                    window: RowWindow { offset: 0, limit: 20 },
                };
                Rows::decode(port.ask(query.request(), budget).unwrap()).unwrap().body
            };
            let number = |value: f64| RawValue::Number { bits: value.to_bits() };
            let text = |value: &str| RawValue::Text(value.encode_utf16().collect());
            let enter = |key, at, value: &str| {
                port.commit(Batch {
                    id: format!("{key}"),
                    workbook: None,
                    base: None,
                    expects: vec![],
                    selection_seq: None,
                    label: None,
                    changes: vec![Change::Model(ModelChange::Enter { at, text: value.into() })],
                }).unwrap();
                while host.advance() {}
            };
            let anchor = at(10, 1);
            let total = at(2, 10);
            if blocked {
                assert_eq!(cell(total).shown.value, number(4_200.0));
                enter(1, at(2, 2), "All");
                assert_eq!(cell(anchor).spill, Some(SpillExtent::Blocked(Area(anchor, at(4156, 7)))));
                for at in [anchor, at(10, 10), at(10, 13), at(10, 17), at(10, 21), total] {
                    assert_eq!(cell(at).shown.value, RawValue::Error("#SPILL!".into()));
                }
                let stamp = port.ask(WorkbookQuery::Summary, budget).unwrap().stamp;
                let cut = Batch {
                    id: "2".into(), workbook: Some(stamp.workbook), base: Some(stamp.model),
                    expects: vec![SourceRef::Workbook], selection_seq: None, label: None,
                    changes: vec![Change::Model(ModelChange::MoveSpillCell {
                        anchor, from: at(16, 1), to: at(12, 25), expected: stamp,
                    })],
                };
                let occupied = Batch {
                    id: "3".into(),
                    changes: vec![Change::Model(ModelChange::MoveSpillCell {
                        anchor, from: at(16, 1), to: at(10, 25), expected: stamp,
                    })],
                    ..cut.clone()
                };
                assert!(port.preview(&occupied, calc_skin::PreviewDepth::WritesOnly).unwrap().refusal.is_some());
                assert_eq!(cell(anchor).shown.value, RawValue::Error("#SPILL!".into()));
                assert_eq!(port.preview(&cut, calc_skin::PreviewDepth::WritesOnly).unwrap().writes.len(), 2);
                port.commit(cut).unwrap();
                while host.advance() {}
                assert_eq!(rows(anchor).matched_rows, 4147);
                assert_eq!(cell(total).shown.value, number(4_667_700.0));
                port.run(Operation::Undo).unwrap();
                while host.advance() {}
                assert_eq!(cell(total).shown.value, RawValue::Error("#SPILL!".into()));
            } else {
                let chart = || {
                    let query = calc_skin::query::ChartImageOf {
                        object: SourceRef::Object { sheet, name: "SalesByRegion".into() },
                        comparison: None,
                    };
                    let result = calc_skin::query::ChartImageOf::decode(port.ask(query.request(), budget).expect("the chart is reloadable")).unwrap();
                    assert!(!result.body.uses_saved_values, "the workbook chart can resolve its named sources");
                    assert_eq!(result.body.label, "Sales by region");
                    result.body.data_url
                };
                let pending_chart = chart();
                let pending = rows(anchor);
                assert_eq!(pending.matched_rows, 4147);
                assert_eq!(pending.columns, 7);
                assert_eq!(pending.rows.len(), 20);
                assert_eq!(cell(total).shown.value, number(4_667_700.0));
                eprintln!("PIVOTBY anchor: {:?}", cell(at(10, 13)));
                assert_eq!(cell(at(10, 14)).shown.value, text("2026-11"));
                assert_eq!(rows(at(10, 13)).matched_rows, 3);
                enter(1, at(3, 2), "Paid");
                assert_eq!(rows(anchor).matched_rows, 20853);
                assert_ne!(chart(), pending_chart);
                assert_eq!(cell(total).shown.value, number(23_440_150.0));
                assert_eq!(cell(at(10, 17)).shown.value, text("South"));
                assert_eq!(cell(at(10, 18)).shown.value, number(9_326_075.0));
                enter(2, at(3, 2), "No matching status");
                for at in [anchor, at(10, 10), at(10, 13), at(10, 17), at(10, 21), total] {
                    assert_eq!(cell(at).shown.value, RawValue::Error("#CALC!".into()));
                }
            }
        }
    }

    fn sales_host() -> (SkinHost, Sheet, Sheet) {
        let (bytes, _) = sales().unwrap();
        let session = HostSession::open(
            Arc::new(MemorySource::new(bytes)),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheets = session.host().sheets();
        let orders = Sheet::of(sheets[0].id());
        let report = Sheet::of(sheets[1].id());
        let host = SkinHost::new(session);
        while host.advance() {}
        (host, orders, report)
    }

    #[test]
    fn sales_rows_watch_a_table_and_live_spill_over_the_full_source() {
        let (host, orders, report) = sales_host();
        let port = host.connect(
            Origin::User {
                view: ViewId("notebook".into()),
            },
            WritePolicy::Commit,
        );
        let budget = Budget {
            cells: 65_536,
            nodes: 25_000,
            edges: 0,
            bytes: 1024 * 1024,
            millis: 5000,
        };
        let window = RowWindow {
            offset: 0,
            limit: 20,
        };
        let table = WorkbookQuery::Rows {
            source: SourceRef::Table {
                name: "Orders".into(),
                part: TablePart::Data,
            },
            filter: vec![RowFilter {
                column: 3,
                equals: RawValue::Text("Pending".encode_utf16().collect()),
            }],
            sort: vec![RowSort {
                column: 6,
                descending: true,
            }],
            window,
        };
        let spill = WorkbookQuery::Rows {
            source: SourceRef::Spill {
                anchor: Key(report.0, report.1, 12, 1),
            },
            filter: vec![],
            sort: vec![],
            window,
        };
        let answer = port.ask(table.clone(), budget).unwrap();
        let page: RowPage = serde_json::from_value(answer.body).unwrap();
        assert_eq!(page.headers.unwrap()[6], "Amount");
        assert_eq!(page.source_rows, 25_000);
        assert_eq!(page.matched_rows, 4_147);
        assert_eq!(page.rows[0].cells[0].text, "SO10482");
        assert_eq!(
            page.rows[0].cells[6].shown.value,
            RawValue::Number {
                bits: 4200.0f64.to_bits()
            }
        );
        let spill_answer = port.ask(spill.clone(), budget).unwrap();
        assert_eq!(spill_answer.stamp, answer.stamp);
        assert_eq!(
            spill_answer.coverage,
            Coverage::Partial {
                done: 20,
                total: Some(4_147),
                why: PartialWhy::Window,
            }
        );

        let table_watch = port.watch(table, budget).unwrap();
        let spill_watch = port.watch(spill, budget).unwrap();
        assert_eq!(port.next_frame().unwrap().unwrap().updates.len(), 2);
        port.commit(Batch {
            id: "1".into(),
            workbook: Some(answer.stamp.workbook),
            base: Some(answer.stamp.model),
            expects: vec![],
            selection_seq: None,
            label: None,
            changes: vec![Change::Model(ModelChange::Enter {
                at: Key(orders.0, orders.1, 10_483, 4),
                text: "Paid".into(),
            })],
        })
        .unwrap();
        while host.advance() {}

        let frame = port.next_frame().unwrap().unwrap();
        for watch in [table_watch, spill_watch] {
            assert!(frame.updates.iter().any(|update| matches!(
                update,
                Update::Ready { watch: id, coverage: Coverage::Partial { done: 20, total: Some(4_146), why: PartialWhy::Window }, .. }
                    if *id == watch
            )));
        }
    }

    #[test]
    fn sales_contributions_find_the_pending_west_order_before_and_after_repair() {
        use calc_skin::query::{ContributionsAt, Query};
        use calc_skin::{Evidence, PreviewDepth};

        let (host, orders, report) = sales_host();
        let port = host.connect(
            Origin::User {
                view: ViewId("inspector".into()),
            },
            WritePolicy::Commit,
        );
        let formula = Key(report.0, report.1, 5, 2);
        let pending = Key(orders.0, orders.1, 10_483, 7);
        let query = ContributionsAt { at: formula };
        let budget = Budget {
            cells: 75_000,
            nodes: 32,
            edges: 0,
            bytes: 1024 * 1024,
            millis: 5000,
        };
        let read = || {
            let started = std::time::Instant::now();
            let answer =
                ContributionsAt::decode(port.ask(query.request(), budget).unwrap()).unwrap();
            eprintln!("Sales contribution query took {:?}", started.elapsed());
            let Evidence::Derived { body, .. } = answer.body else {
                panic!("expected derived contributions");
            };
            assert_eq!(body.inspected, 25_000);
            body
        };

        let before = read();
        assert_eq!(before.matching_count, 126);
        assert!(before.matching.contains(&Area(pending, pending)));
        assert_eq!(
            before.result,
            RawValue::Number {
                bits: 128_450.0f64.to_bits()
            }
        );
        let edit = Batch {
            id: "1".into(),
            workbook: None,
            base: None,
            expects: vec![],
            selection_seq: None,
            label: None,
            changes: vec![Change::Model(ModelChange::RewriteFormula {
                at: formula,
                expected_old: r#"=SUMIFS(Orders[Amount],Orders[Region],"West")"#.into(),
                new_text: r#"=SUMIFS(Orders[Amount],Orders[Region],"West",Orders[Status],"Paid")"#
                    .into(),
            })],
        };
        let preview = port.preview(&edit, PreviewDepth::WritesOnly).unwrap();
        assert_eq!(preview.writes.len(), 1);
        assert_eq!(preview.writes[0].at, formula);
        port.commit(edit).unwrap();
        while host.advance() {}

        let after = read();
        assert_eq!(after.matching_count, 125);
        assert_eq!(
            after.criteria[1].failing_only_this,
            vec![Area(pending, pending)]
        );
        assert_eq!(
            after.result,
            RawValue::Number {
                bits: 124_250.0f64.to_bits()
            }
        );
    }
}
