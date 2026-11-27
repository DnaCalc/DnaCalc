//! Workbook inputs shared by the Design Book scenes. Generation writes ordinary Excel data.

use calc_doc::model::value::{CellKey, ExcelText, GridArea, NameScope};
use calc_doc::model::{
    ArrayFormula, ArrayKind, CachedValue, CellInput, CellText, FiniteNumber, FormulaForm,
    FormulaText, IterationSettings, ModelRoot, OpBatch, SheetEdit, SheetVisibility, Table,
    TableStyle, Validation, WorkbookOp,
};
use std::collections::BTreeMap;
use std::path::Path;
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

const FIXTURES: [(&str, fn() -> BuildResult); 5] = [
    ("Budget", budget),
    ("Sales", sales),
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
    let mut w = Fixture::new(&["Orders", "Report", "Pivot"])?;
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
    w.formula(1, 2, 5, r#"RTD("DnaCalc.Clock",,"LARK")"#);
    w.finish()
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
            stop_rule(3, "decimal", "between", "0", Some("1")),
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
