//! Browser smoke definitions consumed by xtask; not a standalone test target.
use super::*;
pub(super) mod fixtures;

/// The formula field DNA Calc shares with DNA OneCalc, which carries OneCalc's role attribute.
const BAR: &str = "[data-onecalc=\"formula\"]";

/// DNA Calc's original case order; CALC_GROUPS marks fresh-page boundaries and setup.
/// Cases open the existing calc-doc and calc-host fixtures through the application page.
pub(crate) const CALC_CASES: &[Case] = &[
    Case {
        name: "the page loads with a new workbook, Sheet1, at A1",
        actions: &[],
        checks: &[
            Check::Text("document", "Book1.xlsx"),
            Check::Text("[data-dnacalc=\"sheet-tab\"]", "Sheet1"),
            Check::Value("name-box", "A1"),
            Check::Attr("[data-dnacalc=\"skin-grid\"]", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "the calculation worker starts from its own script and answers (dc-p96)",
        actions: &[],
        checks: &[Check::Attr(
            "[data-dnacalc-worker]",
            "data-dnacalc-worker",
            "ready",
        )],
    },
    Case {
        name: "typed numbers replace the cell, Enter enters them and moves down",
        actions: &[
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "3"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("A1", "text", "2"),
            Check::PlanCell("A2", "text", "3"),
            Check::PlanCell("A1", "provenance", "literal"),
            Check::Value("name-box", "A3"),
        ],
    },
    Case {
        name: "a formula entered in the cell is calculated: =A1*A2+1 is 7",
        actions: &[
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "A1*A2+1"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("A3", "text", "7"),
            Check::PlanCell("A3", "provenance", "calculated"),
            Check::PlanCell("A3", "formula", "true"),
        ],
    },
    Case {
        name: "the name box goes to a cell; the formula bar shows its formula and provenance",
        actions: &[Action::Into("name-box", "a3\u{E007}")],
        checks: &[
            Check::Value("name-box", "A3"),
            Check::Value(BAR, "=A1*A2+1"),
            Check::Starts(
                "cell-provenance",
                "Sheet1!A3: calculated by DNA Calc's engine",
            ),
        ],
    },
    Case {
        name: "an edit recalculates its dependents: A1 = 5 makes A3 16",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "5"),
            Action::Wait(Check::Value("cell-editor", "5")),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("A1", "text", "5"),
            Check::PlanCell("A3", "text", "16"),
        ],
    },
    Case {
        name: "Undo takes the edit back and Redo makes it again",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::PlanCell("A1", "text", "2"),
            Check::PlanCell("A3", "text", "7"),
        ],
    },
    Case {
        name: "Redo",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::PlanCell("A1", "text", "5"),
            Check::PlanCell("A3", "text", "16"),
        ],
    },
    Case {
        name: "a formula the engine refuses shows as refused, with the reason",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "WEBSERVICE(\"http://example.com\")"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::PlanCell("B1", "text", "refused"),
            Check::PlanCell("B1", "provenance", "refused"),
            Check::Starts("cell-provenance", "Sheet1!B1: refused: "),
        ],
    },
    Case {
        name: "a dynamic array spills: ={1;2;3} in C1 fills C1:C3",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2;3}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("C1", "text", "1"),
            Check::PlanCell("C3", "text", "3"),
            Check::PlanCell("C3", "provenance", "calculated"),
        ],
    },
    Case {
        name: "the formula bar edits with signature help and enters with Enter",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click(BAR),
            Action::Keys(BAR, "=SUM("),
        ],
        checks: &[
            Check::Text("[data-onecalc=\"active-parameter\"]", "number1"),
            Check::Value(BAR, "=SUM("),
        ],
    },
    Case {
        name: "the formula bar's entry is calculated: =SUM(A1:A3) is 24",
        actions: &[Action::Keys(BAR, "A1:A3)"), Action::Keys(BAR, ENTER)],
        checks: &[
            Check::PlanCell("D1", "text", "24"),
            Check::Value("name-box", "D2"),
            Check::Value(BAR, ""),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "in manual calculation an edit leaves its dependent stale",
        actions: &[
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", "0"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("A2", "text", "10"),
            Check::PlanCell("A3", "provenance", "stale"),
            Check::PlanCell("A3", "text", "16"),
        ],
    },
    Case {
        name: "Calculate recalculates: A3 is 51",
        actions: &[
            Action::Click("calculate"),
            Action::Choose("calc-mode", "automatic"),
        ],
        checks: &[
            Check::PlanCell("A3", "text", "51"),
            Check::PlanCell("A3", "provenance", "calculated"),
        ],
    },
    Case {
        name: "Delete clears the active cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", DELETE),
        ],
        checks: &[Check::PlanAbsent("D1")],
    },
    Case {
        name: "Save writes the workbook's package",
        actions: &[Action::Click("save")],
        checks: &[
            Check::Starts("status", "Download started: Book1.xlsx."),
            Check::Text("document", "Book1.xlsx"),
        ],
    },
    Case {
        name: "the notebook lists the sheet's cells as blocks",
        actions: &[Action::Click("skin-notebook")],
        checks: &[
            Check::Starts("notebook-page", "7 blocks; page 1 of 1"),
            Check::Attr(
                "[data-dnacalc=\"block\"][data-cell=\"B1\"]",
                "data-provenance",
                "refused",
            ),
        ],
    },
    Case {
        name: "a block's entry is entered and its dependents recalculate",
        actions: &[
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-select]"),
            Action::Into(
                "[data-dnacalc=\"block\"][data-cell=\"A1\"] [data-dnacalc=\"block-input\"]",
                "6\u{E007}",
            ),
            Action::Wait(Check::Text(
                "[data-dnacalc=\"block\"][data-cell=\"A3\"] [data-dnacalc=\"block-value\"]",
                "61",
            )),
            // The editor can still have browser focus after committing. A subsequent input
            // must open a new edit session even when no focus event is delivered.
            Action::Keys(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "0\u{E007}",
            ),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=A3] [data-dnacalc=block-value]",
                "601",
            )),
            Action::Into(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "6\u{E007}",
            ),
        ],
        checks: &[Check::Text(
            "[data-dnacalc=\"block\"][data-cell=\"A3\"] [data-dnacalc=\"block-value\"]",
            "61",
        )],
    },
    Case {
        name: "the atlas counts every formula cell, the refused one as refused",
        actions: &[Action::Click("skin-atlas")],
        checks: &[
            Check::Text(
                "atlas-totals",
                "3 formula cells: 2 calculated, 1 refused, 0 file cache, 0 stale, 0 pending; 66.67% calculated",
            ),
            Check::Text("atlas-mode", "Calculation: automatic"),
        ],
    },
    Case {
        name: "back in the grid, the active cell and the values are as they were",
        actions: &[Action::Click("skin-grid")],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Text(cell!("A3"), "61"),
        ],
    },
    Case {
        name: "an Excel-saved workbook opens through the file input, calculated",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx",
            ),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text("document", "d1.saved.xlsx"),
            Check::Starts("status", "Opened d1.saved.xlsx: "),
            Check::Text("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]", "Calc"),
        ],
    },
    Case {
        name: "its sheet shows the engine's values, its dynamic array's spill included",
        actions: &[
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]"),
            Action::Into("name-box", "U3\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("U3"), "21"),
            Check::PlanCell("U3", "provenance", "calculated"),
            Check::Value(BAR, "=U2*3"),
        ],
    },
    Case {
        name: "the spill of SEQUENCE(3,2) at A1",
        actions: &[Action::Into("name-box", "A1\u{E007}")],
        checks: &[
            Check::Text(cell!("B2"), "4"),
            Check::PlanCell("B2", "provenance", "calculated"),
        ],
    },
    Case {
        name: "an undo of an edit on another sheet shows that sheet and the restored cell",
        actions: &[
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "5"),
            Action::Wait(Check::Value("cell-editor", "5")),
            Action::Keys("cell-editor", ENTER),
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Calc\"]"),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]",
                "aria-selected",
                "true",
            ),
            Check::Value("name-box", "A1"),
            Check::Text(cell!("A1"), "1"),
            Check::Value(BAR, "1"),
            Check::Starts("cell-provenance", "Data!A1: a literal value"),
        ],
    },
    Case {
        name: "F2 and Enter on the restored cell enter its own text again",
        actions: &[Action::Keys("grid", F2), Action::Keys("cell-editor", ENTER)],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "New makes an empty workbook",
        actions: &[Action::Click("new"), Action::DiscardReplacement],
        checks: &[
            Check::Text("document", "Book1.xlsx"),
            Check::Absent("[data-dnacalc=\"cell\"]"),
            Check::Value("name-box", "A1"),
        ],
    },
    Case {
        name: "the grid scrolls past its first block of rows and enters a cell there",
        actions: &[
            Action::Into("name-box", "B200\u{E007}"),
            Action::Keys("grid", "9"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("B200"), "9"),
            Check::Value("name-box", "B201"),
        ],
    },
    Case {
        name: "back at the top, row 200 is out of view",
        actions: &[Action::Into("name-box", "A1\u{E007}")],
        checks: &[Check::Absent(cell!("B200")), Check::Value("name-box", "A1")],
    },
    Case {
        name: "at row 200 again, the entry is there",
        actions: &[Action::Into("name-box", "B200\u{E007}")],
        checks: &[Check::Text(cell!("B200"), "9")],
    },
    Case {
        name: "a spill fills E1:E3",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2;3}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("E3"), "3")],
    },
    Case {
        name: "the spill shrinks to E1:E2 and E3 is empty again",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "{1;2}"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("E2"), "2"), Check::Absent(cell!("E3"))],
    },
    Case {
        name: "a new workbook for the range cases: A1 1, A2 2, B1 =A1*10",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "A1*10"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Text(cell!("B1"), "10")],
    },
    Case {
        name: "Shift+Down selects a range and Ctrl+D fills it down, formulas relocated",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            // Shift down, Down twice, every key released (WebDriver's modifier and NULL keys).
            Action::Keys(
                "grid",
                concat!("\u{E008}", "\u{E015}", "\u{E015}", "\u{E000}"),
            ),
            Action::Keys("grid", concat!("\u{E009}", "d", "\u{E000}")),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "B1:B3"),
            Check::Text(cell!("B2"), "20"),
            Check::Text(cell!("B3"), "0"),
        ],
    },
    Case {
        name: "Ctrl+C copies a range and Ctrl+V pastes it with its formula relocated",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", concat!("\u{E008}", "\u{E014}", "\u{E000}")),
            Action::Keys("grid", concat!("\u{E009}", "c", "\u{E000}")),
            Action::Into("name-box", "D5\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
        ],
        checks: &[
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
            Check::Value(BAR, "1"),
        ],
    },
    Case {
        name: "a click on a column header selects the column; Delete clears it",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"D\"]"),
            Action::Keys("grid", DELETE),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "D1:D1048576"),
            Check::Absent(cell!("D5")),
            Check::Text(cell!("E5"), "0"),
        ],
    },
    Case {
        name: "Undo brings the column's cell back",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
        ],
    },
    Case {
        name: "a row header and Insert insert a row: cells and references move down",
        actions: &[
            Action::Click("[data-dnacalc=\"row-header\"][data-row=\"2\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "A2:XFD2"),
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "2"),
            Check::Text(cell!("B3"), "20"),
            Check::Text(cell!("D6"), "1"),
            Check::Text(cell!("E6"), "10"),
        ],
    },
    Case {
        name: "Delete deletes the row again: the cells and references move back",
        actions: &[Action::Click("delete-cells")],
        checks: &[
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("B2"), "20"),
            Check::Text(cell!("D5"), "1"),
            Check::Text(cell!("E5"), "10"),
        ],
    },
    Case {
        name: "a column inserted before A moves everything right, and Undo takes it back",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"A\"]"),
            Action::Keys("grid", concat!("\u{E009}", "\u{E008}", "=", "\u{E000}")),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Text(cell!("B1"), "1"),
            Check::Text(cell!("C1"), "10"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "Undo of the column insertion",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("B1"), "10"),
        ],
    },
    Case {
        name: "Ctrl+X and Ctrl+V move cells, and the formulas that read them follow",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", concat!("\u{E008}", "\u{E015}", "\u{E000}")),
            Action::Keys("grid", concat!("\u{E009}", "x", "\u{E000}")),
            Action::Into("name-box", "H1\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Text(cell!("H1"), "1"),
            Check::Text(cell!("H2"), "2"),
            Check::Text(cell!("B1"), "10"),
            Check::Value(BAR, "=H1*10"),
        ],
    },
    Case {
        name: "Undo takes the move back",
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Absent(cell!("H1")),
            Check::Value(BAR, "=A1*10"),
        ],
    },
    Case {
        name: "a workbook's number formats show as Excel shows them",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text("document", "formats.xlsx"),
            Check::Text(cell!("A1"), "1,234.50"),
            Check::PlanCell("A1", "format", "applied"),
            Check::Text(cell!("A2"), "25%"),
            Check::Text(cell!("A3"), "2024-01-31"),
            Check::Text(cell!("A4"), "(5.00)"),
            Check::Text(cell!("A10"), "1.50E+06"),
            Check::Text(cell!("A11"), "Qty 12"),
        ],
    },
    Case {
        name: "its column widths, row heights and hidden column and row lay out the grid",
        actions: &[],
        checks: &[
            // A is 117 px (16.71 characters), B 180 px (25.71), C hidden, D the default 64 px;
            // row 3 is 40 px (30 points), row 13 hidden, the others 20 px (15 points).
            Check::Style(cell!("B1"), "left:181px;top:22px;width:180px;height:20px"),
            Check::Style(cell!("D1"), "left:361px;top:22px;width:64px;height:20px"),
            Check::Style(cell!("A3"), "left:64px;top:62px;width:117px;height:40px"),
            Check::Style(cell!("A14"), "left:64px;top:282px;width:117px;height:20px"),
            // A15's negative section is [Color10]: Excel's palette entry 10.
            Check::Style(
                cell!("A15"),
                "left:64px;top:302px;width:117px;height:20px;color:#008000",
            ),
            // Column E is 4 characters (33 px): a formatted number too wide shows ####, as in
            // Excel, and a text overflows over the empty F to I.
            Check::Text(cell!("E1"), "####"),
            Check::Text(cell!("E2"), "12"),
            Check::Style(cell!("E3"), "left:425px;top:62px;width:289px;height:40px"),
            // A4's negative section is [Red].
            Check::Style(
                cell!("A4"),
                "left:64px;top:102px;width:117px;height:20px;color:#ff0000",
            ),
        ],
    },
    Case {
        name: "a row inserted at 2 moves the cells below with their formats and row heights",
        actions: &[
            Action::Click("[data-dnacalc=\"row-header\"][data-row=\"2\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "25%"),
            Check::Text(cell!("A4"), "2024-01-31"),
            // The file's 30-point row 3 is now row 4.
            Check::Style(cell!("A4"), "left:64px;top:82px;width:117px;height:40px"),
        ],
    },
    Case {
        name: "a column inserted before B takes A's width, and B's cells and widths move right",
        actions: &[
            Action::Click("[data-dnacalc=\"column-header\"][data-column=\"B\"]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            // B is 117 px like A; the file's B (180 px) is C, its hidden C is D.
            Check::Text(cell!("C1"), "wide"),
            Check::Style(cell!("C1"), "left:298px;top:22px;width:180px;height:20px"),
            Check::Text(cell!("E1"), "after hidden"),
            Check::Style(cell!("E1"), "left:478px;top:22px;width:64px;height:20px"),
        ],
    },
    Case {
        name: "Ctrl+X and Ctrl+V move a formatted cell with its format",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "x", "\u{E000}")),
            Action::Into("name-box", "I3\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "v", "\u{E000}")),
        ],
        checks: &[Check::Absent(cell!("A3")), Check::Text(cell!("I3"), "25%")],
    },
    Case {
        name: "Save after the insertions and the move writes the package",
        actions: &[Action::Click("save")],
        checks: &[Check::Starts("status", "Download started: formats.xlsx.")],
    },
    // dc-gf0: the tree, canvas and ledger skins over calc-host's dc_gf0 workbook (Data, Other and
    // Target Sheet, six defined names, formulas across the sheets).
    Case {
        name: "the tree outlines the workbook: its names, its sheets, their rows and cells",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-tree"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-key=\"names\"] .dc-count",
                "6",
            ),
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] .dc-count",
                "13 cells",
            ),
            Check::Text(
                "[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"] [data-dnacalc=\"tree-value\"]",
                "108",
            ),
        ],
    },
    Case {
        name: "a sheet of the tree folds and unfolds",
        actions: &[Action::Click(
            "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] [data-dnacalc=\"tree-toggle\"]",
        )],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"]",
                "data-folded",
                "true",
            ),
            Check::Absent("[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"]"),
        ],
    },
    Case {
        name: "unfolded again, a click on a cell makes it the active cell",
        actions: &[
            Action::Click(
                "[data-dnacalc=\"tree-row\"][data-key=\"sheet:Data\"] [data-dnacalc=\"tree-toggle\"]",
            ),
            Action::Click("[data-dnacalc=\"tree-row\"][data-sheet=\"Data\"][data-cell=\"B3\"]"),
            Action::Click("tree-details-toggle"),
        ],
        checks: &[
            Check::Value("name-box", "B3"),
            Check::Value(BAR, "=A1+Other!A1+D1"),
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-focused=\"true\"]",
                "data-cell",
                "B3",
            ),
            // Other!B4, B12 and B13 sum ranges that hold Data!B3.
            Check::Text("tree-dependents", "read by 3 formulas"),
        ],
    },
    Case {
        name: "Down and Enter in the tree move to the next cell and select it",
        actions: &[Action::Keys("tree", concat!("\u{E015}", "\u{E007}"))],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"tree-row\"][data-focused=\"true\"]",
                "data-cell",
                "C3",
            ),
            Check::Value("name-box", "C3"),
        ],
    },
    Case {
        name: "the canvas centres on the active cell, what it reads to the left, what reads it to the right",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-center=\"true\"]",
                "data-node",
                "B1",
            ),
            Check::Text(
                "[data-dnacalc=\"canvas-node\"][data-node=\"B1\"] [data-dnacalc=\"canvas-value\"]",
                "10",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]",
                "data-column",
                "-1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"C1\"]",
                "data-column",
                "1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"Other!B3\"]",
                "data-column",
                "1",
            ),
            Check::Text(
                "canvas-read-by",
                "Data!C1, Other!B3, Other!B4, Other!B12, Other!B13",
            ),
        ],
    },
    Case {
        name: "a click on a chip moves the canvas, and [ reaches one step further back",
        actions: &[
            Action::Click("[data-dnacalc=\"canvas-node\"][data-node=\"C1\"]"),
            Action::Keys("canvas", "["),
        ],
        checks: &[
            Check::Value("name-box", "C1"),
            Check::Text("canvas-depth", "2 back, 1 forward"),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"B1\"]",
                "data-column",
                "-1",
            ),
            Check::Attr(
                "[data-dnacalc=\"canvas-node\"][data-node=\"A1\"]",
                "data-column",
                "-2",
            ),
        ],
    },
    Case {
        name: "the ledger lists every formula cell of the workbook",
        actions: &[Action::Click("skin-ledger")],
        checks: &[Check::Starts("ledger-count", "22 of 22 formula cells")],
    },
    Case {
        name: "the ledger keeps the cells holding a text",
        actions: &[
            Action::Choose("ledger-filter", "text"),
            Action::Into("ledger-text", "Other!"),
        ],
        checks: &[Check::Starts("ledger-count", "14 of 22 formula cells")],
    },
    Case {
        name: "sorted by value, the largest comes first, and a click selects it on its sheet",
        actions: &[
            Action::Choose("ledger-filter", ""),
            Action::Choose("ledger-sort", "value-desc"),
            Action::Click("[data-dnacalc=\"ledger-row\"] td"),
        ],
        checks: &[
            Check::Attr("[data-dnacalc=\"ledger-row\"]", "data-cell", "Other!B12"),
            Check::Value("name-box", "B12"),
            Check::Value(BAR, "=SUM(Data!A1:C3)"),
        ],
    },
    // dc-6yw: conditional formats as Excel shows them (calc-host's dc_6yw/cf-excel.txt).
    Case {
        name: "conditional formats colour cells as Excel shows them",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/conditional-formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
            Check::Style(cell!("A2"), "color:#9c0006"),
            Check::Style(cell!("B1"), "font-weight:700"),
            Check::Style(cell!("C2"), "background-color:#ffeb9c"),
            Check::Style(cell!("D2"), "font-style:italic"),
            Check::Style(cell!("E2"), "background-color:#86c97e"),
            Check::Style(cell!("J2"), "background-color:#83cceb"),
            Check::Attr(cell!("H2"), "data-icon", "3Arrows:2"),
            Check::Attr(cell!("H3"), "data-icon", "3Arrows:0"),
            Check::Attr(cell!("G6"), "data-bar", "1.0000"),
            Check::Text(cell!("N1"), "400.0%"),
            Check::Text(cell!("N2"), "9"),
        ],
    },
    Case {
        name: "a conditional format follows an edit: A3 set to 8 turns red",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "8"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[Check::Style(cell!("A3"), "background-color:#ffc7ce")],
    },
    Case {
        name: "rule review explains the imported priority and active-cell condition",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='0']"),
        ],
        checks: &[
            Check::Count("rule-row", 15),
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            ),
            Check::Value("rule-applies", "A1:A10"),
            Check::Value("rule-formulas", "5"),
            Check::Reachable("review-dialog"),
        ],
    },
    Case {
        name: "rule conditions edit through the worker and explain a false condition",
        actions: &[
            Action::Into("rule-formulas", "50"),
            Action::Click("rule-apply"),
        ],
        checks: &[
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Condition false for this cell",
            ),
            Check::Computed(cell!("A2"), "background-color:transparent"),
        ],
    },
    Case {
        name: "applies-to edits explain an excluded cell and retain its input",
        actions: &[
            Action::Click("[data-dnacalc=rule-row][data-index='0']"),
            Action::Into("rule-applies", "A1:A2"),
            Action::Into("rule-formulas", "5"),
            Action::Click("rule-apply"),
        ],
        checks: &[Check::TextContains(
            "[data-dnacalc=rule-row][data-index='0']",
            "Outside this cell",
        )],
    },
    Case {
        name: "rule edits have ordinary undo and restore imported appearance",
        actions: &[
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Condition false",
            )),
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            )),
            Action::Click("review-close"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
            Check::Style(cell!("A3"), "background-color:#ffc7ce"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "users author a visual rule without replacing imported formats or artwork",
        actions: &[
            Action::Into("name-box", "E1:E10\u{E007}"),
            Action::Click("review-tools"),
            Action::Into("rule-applies", "E1:E10"),
            Action::Click("rule-add"),
            Action::Wait(Check::Count("rule-row", 16)),
            Action::Click("review-close"),
        ],
        checks: &[
            Check::Attr(cell!("E6"), "data-bar", "1.0000"),
            Check::Attr(cell!("H2"), "data-icon", "3Arrows:2"),
        ],
    },
    Case {
        name: "rule priority edits and removal are undoable in the same manager",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='15']"),
            Action::Click("rule-up"),
            Action::Wait(Check::TextContains(
                "[data-dnacalc=rule-row][data-index='15']",
                "15 · dataBar",
            )),
            Action::Click("[data-dnacalc=rule-row][data-index='15']"),
            Action::Click("rule-remove"),
            Action::Wait(Check::Count("rule-row", 15)),
            Action::Click("review-undo"),
            Action::Wait(Check::Count("rule-row", 16)),
            Action::Click("review-close"),
        ],
        checks: &[Check::Attr(cell!("E6"), "data-bar", "1.0000")],
    },
    Case {
        name: "rule edits and imported extended bars survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("conditional-formats.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "conditional-formats.xlsx")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("review-tools"),
        ],
        checks: &[
            Check::Count("rule-row", 16),
            Check::TextContains(
                "[data-dnacalc=rule-row][data-index='0']",
                "Applies appearance",
            ),
        ],
    },
    Case {
        name: "review close and keyboard escape restore the worksheet",
        actions: &[Action::Keys("review-close", ESCAPE)],
        checks: &[
            Check::Focused("grid"),
            Check::Attr(cell!("G6"), "data-bar", "1.0000"),
            Check::Style(cell!("A2"), "background-color:#ffc7ce"),
        ],
    },
    Case {
        name: "formula rules can visibly format a blank cell without inventing a value",
        actions: &[
            Action::Click("review-tools"),
            Action::Click("[data-dnacalc=rule-row][data-index='2']"),
            Action::Into("rule-applies", "P1"),
            Action::Into("rule-formulas", "TRUE"),
            Action::Click("rule-apply"),
            Action::Wait(Check::Absent("rule-formulas")),
            Action::Click("review-close"),
            Action::Into("name-box", "P1\u{E007}"),
        ],
        checks: &[
            Check::Style(cell!("P1"), "background-color:#ffeb9c"),
            Check::Text(cell!("P1"), ""),
            Check::PlanCell("P1", "provenance", "empty"),
        ],
    },
    Case {
        name: "undo removes the blank-cell conditional appearance through the subscription",
        actions: &[Action::Click("undo")],
        checks: &[Check::Absent(cell!("P1"))],
    },
    // dc-6yw: data validation (calc-host's dc_6yw/dv-excel.txt).
    Case {
        name: "a list cell shows its drop-down and input message",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/data-validation.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text("validation-prompt", "ColourPick a colour"),
            Check::Attr("dropdown", "title", "Pick from the list"),
        ],
    },
    Case {
        name: "the drop-down lists the rule's items",
        actions: &[Action::Click("dropdown")],
        checks: &[Check::Items("dropdown-list", &["Red", "Green", "Blue"])],
    },
    Case {
        name: "a picked item is the cell's entry",
        actions: &[Action::Click(
            "[data-dnacalc=\"dropdown-list\"] li:nth-child(2)",
        )],
        checks: &[
            Check::Text(cell!("A1"), "Green"),
            Check::Absent("dropdown-list"),
        ],
    },
    Case {
        name: "a list read from cells lists their values",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("dropdown"),
        ],
        checks: &[Check::Items("dropdown-list", &["North", "South", "East"])],
    },
    Case {
        name: "validation list uses semantic keyboard options and Enter selects a value",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Wait(Check::Focused("dropdown-list")),
            Action::Keys("dropdown-list", "\u{E010}\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "Blue"),
            Check::Absent("dropdown-list"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "validation list arrows change the semantic option and Escape keeps the cell",
        actions: &[
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Wait(Check::Focused("dropdown-list")),
            Action::Keys("dropdown-list", "\u{E015}"),
        ],
        checks: &[
            Check::Role("dropdown-list", "listbox"),
            Check::Attr("#dc-validation-option-0-1", "aria-selected", "true"),
            Check::ActiveDescendant("dropdown-list"),
        ],
    },
    Case {
        name: "validation list Escape cancels without entering its option",
        actions: &[Action::Keys("dropdown-list", ESCAPE)],
        checks: &[
            Check::Text(cell!("A1"), "Blue"),
            Check::Absent("dropdown-list"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "a stop rule refuses an entry it does not allow, with its message",
        actions: &[
            Action::Into("name-box", "A2\u{E007}"),
            Action::Keys("grid", "P"),
            Action::Keys("cell-editor", "urple\u{E007}"),
        ],
        checks: &[
            Check::Text("status", "Not a colour: Choose Red, Green or Blue"),
            Check::Attr("status", "data-refused", "true"),
            Check::Absent(cell!("A2")),
        ],
    },
    Case {
        name: "Stop Retry retains and focuses the rejected draft",
        actions: &[Action::Click("validation-retry")],
        checks: &[
            Check::Value("cell-editor", "Purple"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "a retried validation draft can be corrected and committed",
        actions: &[Action::Into("cell-editor", "Red\u{E007}")],
        checks: &[
            Check::Text(cell!("A2"), "Red"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Warning asks before changing the cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "1\u{E007}"),
        ],
        checks: &[
            Check::TextContains("validation-dialog", "Warning"),
            Check::Text("validation-message", "A positive number is expected"),
            Check::Absent(cell!("D1")),
        ],
    },
    Case {
        name: "Warning Edit retains the draft and Cancel discards it",
        actions: &[
            Action::Click("validation-retry"),
            Action::Wait(Check::Focused("cell-editor")),
            Action::Keys("cell-editor", ENTER),
            Action::Click("validation-cancel"),
        ],
        checks: &[
            Check::Absent(cell!("D1")),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Warning Keep is an explicit undoable entry",
        actions: &[
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "1\u{E007}"),
            Action::Wait(Check::Attr("validation-dialog", "open", "")),
            Action::Click("validation-keep"),
        ],
        checks: &[
            Check::Text(cell!("D1"), "-1"),
            Check::Text(
                "status",
                "Warning, entry kept: Microsoft Excel: A positive number is expected",
            ),
        ],
    },
    Case {
        name: "Warning acceptance has ordinary Undo and Redo",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("D1"))),
            Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("D1"), "-1")],
    },
    Case {
        name: "Information waits for OK or Cancel and Escape cancels",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef\u{E007}"),
            Action::Wait(Check::TextContains("validation-dialog", "Information")),
            Action::Keys("validation-cancel", ESCAPE),
        ],
        checks: &[
            Check::Absent(cell!("E1")),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "Information Keep stores the chosen entry",
        actions: &[
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef\u{E007}"),
            Action::Wait(Check::Attr("validation-dialog", "open", "")),
            Action::Click("validation-keep"),
        ],
        checks: &[
            Check::Text(cell!("E1"), "abcdef"),
            Check::Text(
                "status",
                "Information, entry kept: Microsoft Excel: At most five characters",
            ),
        ],
    },
    Case {
        name: "range entry also asks before accepting warning cells",
        actions: &[
            Action::Into("name-box", "D2:D3\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "2\u{E009}\u{E007}\u{E000}"),
            Action::Wait(Check::TextContains(
                "validation-message",
                "2 selected cells",
            )),
        ],
        checks: &[Check::Absent(cell!("D2")), Check::Absent(cell!("D3"))],
    },
    Case {
        name: "range warning acceptance remains one Undo step",
        actions: &[
            Action::Click("validation-keep"),
            Action::Wait(Check::Text(cell!("D2"), "-2")),
            Action::Click("undo"),
        ],
        checks: &[Check::Absent(cell!("D2")), Check::Absent(cell!("D3"))],
    },
    Case {
        name: "validation decisions preserve rules through save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("data-validation.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "data-validation.xlsx")),
            Action::Into("name-box", "D2\u{E007}"),
            Action::Keys("grid", "-"),
            Action::Keys("cell-editor", "3\u{E007}"),
            Action::Wait(Check::TextContains("validation-dialog", "Warning")),
            Action::Click("validation-cancel"),
        ],
        checks: &[
            Check::Text(cell!("D1"), "-1"),
            Check::Text(cell!("E1"), "abcdef"),
            Check::Absent(cell!("D2")),
            Check::Focused("grid"),
        ],
    },
    // dc-6yw: charts drawn read-only (calc-host's dc_6yw/charts-excel.txt).
    Case {
        name: "charts are drawn where Excel puts them, with their series' values",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"]",
                "data-kind",
                "column",
            ),
            Check::Text(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-dnacalc=\"chart-title\"]",
                "Column",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "12",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"1\"][data-point=\"5\"]",
                "data-value",
                "13",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 3\"]",
                "data-kind",
                "line",
            ),
        ],
    },
    Case {
        name: "a chart follows an edit: B2 set to 40 moves its column and the total's line",
        actions: &[
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "4"),
            Action::Wait(Check::Value("cell-editor", "4")),
            Action::Keys("cell-editor", "0\u{E007}"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "40",
            ),
            Check::Attr(
                "[data-dnacalc=\"chart\"][data-chart=\"Chart 3\"] [data-series=\"0\"][data-point=\"0\"]",
                "data-value",
                "48",
            ),
        ],
    },
    // dc-xib: Strand's themes and a narrow screen.
    Case {
        name: "the theme switch shows the dark theme: petrol chrome, charcoal paper",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", "cockpit-dark"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Computed("app", "background-color:#1A262B;color:#E2ECEF"),
            Check::Computed(
                "[data-dnacalc=\"app\"] .dc-mast",
                "background-color:#0F2229",
            ),
        ],
    },
    Case {
        name: "the system's theme again: light paper",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", ""),
            Action::Click("settings-toggle"),
        ],
        checks: &[Check::Computed(
            "app",
            "background-color:#FFFFFF;color:#17282E",
        )],
    },
    Case {
        name: "on a 500-pixel screen (the narrowest a desktop Firefox window goes) the page fits its width and the grid still shows",
        actions: &[Action::Window(500, 844), Action::Click("skin-grid")],
        checks: &[
            Check::Narrow(500),
            Check::Fits,
            Check::Attr("grid", "tabindex", "0"),
        ],
    },
    Case {
        name: "back on a wide screen",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    // dc-971p.2: the cell and bar share the same draft, assistance and commit contract.
    Case {
        name: "edit session: cell completion opens on typed replacement",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("status", "New workbook")),
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "SUM"),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM"),
            Check::Text("[data-onecalc=\"completion-label\"]", "SUM"),
        ],
    },
    Case {
        name: "edit session: completion Escape dismisses only the popup",
        actions: &[Action::Keys("cell-editor", ESCAPE)],
        checks: &[
            Check::Value("cell-editor", "=SUM"),
            Check::Absent("[data-onecalc=\"completions\"]"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: completion Tab accepts before worksheet Tab can commit",
        actions: &[
            Action::Keys("cell-editor", "\u{E003}M"),
            Action::Keys("cell-editor", TAB),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM("),
            Check::Value("name-box", "B1"),
            Check::Text("[data-onecalc=\"active-parameter\"]", "number1"),
        ],
    },
    Case {
        name: "edit session: cell Enter commits once and returns focus below",
        actions: &[
            Action::Keys("cell-editor", "A1:A2)"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::Text(cell!("B1"), "3"),
            Check::Value("name-box", "B2"),
            Check::Focused("grid"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: F2 seeds authored formula and Escape keeps exact source",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", "+100"),
            Action::Keys("cell-editor", ESCAPE),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)"),
            Check::Text(cell!("B1"), "3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: double click opens the authored source",
        actions: &[Action::DoubleClick(cell!("B1"))],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: local Ctrl+Z and Redo never enter the workbook journal",
        actions: &[
            Action::Keys("cell-editor", "+9"),
            Action::Keys("cell-editor", "\u{E009}z\u{E000}"),
        ],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)+"),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "edit session: toolbar Redo belongs to the current text edit",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::Value("cell-editor", "=SUM(A1:A2)+9"),
            Check::Focused("cell-editor"),
            Check::Text(cell!("B1"), "3"),
        ],
    },
    Case {
        name: "edit session: bar point click retains the target and inserts a reference",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B3\u{E007}"),
            // Firefox 151 can return from its download UI without a focusin event.
            Action::Click("save"),
            Action::Into(BAR, "=A1+"),
            Action::Click(cell!("A2")),
        ],
        checks: &[
            Check::Value(BAR, "=A1+A2"),
            Check::Value("name-box", "B3"),
            Check::Focused(BAR),
            Check::Attr("point-reference", "class", "dc-reference"),
            Check::Text("edit-mode", "Point · F2 to edit text"),
        ],
    },
    Case {
        name: "edit session: F4 cycles a picked reference through four anchor forms",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[
            Check::Value(BAR, "=A1+$A$2"),
            Check::Value("name-box", "B3"),
        ],
    },
    Case {
        name: "edit session: F4 row absolute",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[Check::Value(BAR, "=A1+A$2")],
    },
    Case {
        name: "edit session: F4 column absolute",
        actions: &[Action::Keys(BAR, "\u{E034}")],
        checks: &[Check::Value(BAR, "=A1+$A2")],
    },
    Case {
        name: "edit session: F4 relative again and Ctrl+F2 transfers the draft",
        actions: &[
            Action::Keys(BAR, "\u{E034}"),
            Action::Keys(BAR, "\u{E009}\u{E032}\u{E000}"),
        ],
        checks: &[
            Check::Value("cell-editor", "=A1+A2"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: transferred draft commits to its original address with one undo step",
        actions: &[Action::Keys("cell-editor", ENTER)],
        checks: &[
            Check::Text(cell!("B3"), "3"),
            Check::Value("name-box", "B4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: workbook Undo removes only that completed entry",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Absent(cell!("B3")),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "2"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "edit session: workbook Redo restores the exact pointed formula",
        actions: &[Action::Click("redo")],
        checks: &[Check::Text(cell!("B3"), "3"), Check::Value(BAR, "=A1+A2")],
    },
    Case {
        name: "edit session: bar Tab commits and moves right",
        actions: &[
            Action::Into("name-box", "B4\u{E007}"),
            Action::Into(BAR, "25"),
            Action::Keys(BAR, TAB),
        ],
        checks: &[
            Check::Text(cell!("B4"), "25"),
            Check::Value("name-box", "C4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Tab commits and moves left",
        actions: &[
            Action::Into(BAR, "27"),
            Action::Keys(BAR, "\u{E008}\u{E004}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("C4"), "27"),
            Check::Value("name-box", "B4"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Enter commits and moves up",
        actions: &[
            Action::Into(BAR, "28"),
            Action::Keys(BAR, "\u{E008}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B4"), "28"),
            Check::Value("name-box", "B3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: cell Shift+Enter commits and moves up",
        actions: &[
            Action::Keys("grid", "4"),
            Action::Wait(Check::Value("cell-editor", "4")),
            Action::Keys("cell-editor", "\u{E008}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B3"), "4"),
            Check::Value("name-box", "B2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: cell Shift+Tab commits and moves left",
        actions: &[
            Action::Keys("grid", "5"),
            Action::Wait(Check::Value("cell-editor", "5")),
            Action::Keys("cell-editor", "\u{E008}\u{E004}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("B2"), "5"),
            Check::Value("name-box", "A2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: rejected bar entry keeps the draft target source and focus",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1+"),
            Action::Keys(BAR, ENTER),
        ],
        checks: &[
            Check::Value(BAR, "=A1+"),
            Check::Value("name-box", "B1"),
            Check::Text(cell!("B1"), "3"),
            Check::Focused(BAR),
            Check::Attr("status", "data-refused", "true"),
        ],
    },
    Case {
        name: "edit session: cancelling a rejected entry restores authored source",
        actions: &[Action::Keys(BAR, ESCAPE)],
        checks: &[Check::Value(BAR, "=SUM(A1:A2)"), Check::Focused("grid")],
    },
    Case {
        name: "edit session: formula pointing can drag a range without changing active cell",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "=SUM("),
            Action::Drag(cell!("A1"), cell!("A2")),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2"),
            Check::Value("name-box", "D1"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: completion leaves Enter to worksheet entry",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into(BAR, "=TRUE"),
            Action::Keys(BAR, ENTER),
        ],
        checks: &[
            Check::Text(cell!("D1"), "TRUE"),
            Check::Value("name-box", "D2"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: point mode arrows create references and F2 returns to caret movement",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "\u{E012}"),
            Action::Keys("cell-editor", F2),
        ],
        checks: &[
            Check::Value("cell-editor", "=C1"),
            Check::Text("edit-mode", "Edit · F2 to point to references"),
        ],
    },
    Case {
        name: "edit session: switching skins keeps the draft and returns it to the bar",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", "+4"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)+4"),
            Check::Value("name-box", "B1"),
            Check::Focused(BAR),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: another skin uses the same commit and undo route",
        actions: &[
            Action::Keys(BAR, ENTER),
            Action::Click("skin-grid"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=SUM(A1:A2)+4"),
            Check::Text(cell!("B1"), "7"),
        ],
    },
    Case {
        name: "edit session: synthetic in-cell composition Enter cannot commit",
        actions: &[
            Action::Into("name-box", "B7\u{E007}"),
            Action::Keys("grid", "x"),
            Action::Composing("cell-editor", "漢"),
        ],
        checks: &[
            Check::Value("cell-editor", "x漢"),
            Check::Value("name-box", "B7"),
            Check::Absent(cell!("B7")),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: composition ends then Escape cancels without a workbook edit",
        actions: &[
            Action::CompositionEnd("cell-editor"),
            Action::Keys("cell-editor", ESCAPE),
        ],
        checks: &[
            Check::Value(BAR, ""),
            Check::Absent(cell!("B7")),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: synthetic bar composition Enter cannot commit",
        actions: &[Action::Into(BAR, "abc"), Action::Composing(BAR, "字")],
        checks: &[
            Check::Value(BAR, "abc字"),
            Check::Value("name-box", "B7"),
            Check::Absent(cell!("B7")),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: after composition a normal Enter commits the complete text",
        actions: &[
            Action::CompositionEnd(BAR),
            Action::Keys(BAR, ENTER),
            Action::Into("name-box", "B7\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "abc字"),
            Check::Text(cell!("B7"), "abc字"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "edit session: a reference picked on another sheet retains the original edit target",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "F5\u{E007}"),
            Action::Into(BAR, "="),
            Action::Click("[data-dnacalc=\"sheet-tab\"][data-sheet=\"Other\"]"),
            Action::Wait(Check::Text(cell!("A1"), "100")),
            Action::Click(cell!("A1")),
        ],
        checks: &[
            Check::Value(BAR, "=Other!A1"),
            Check::Starts("edit-session", "Editing Data!F5"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: a cross-sheet point commits back on the edited sheet",
        actions: &[Action::Keys(BAR, ENTER)],
        checks: &[
            Check::Text(cell!("F5"), "100"),
            Check::Value("name-box", "F6"),
            Check::Focused("grid"),
            Check::Attr(
                "[data-dnacalc=\"sheet-tab\"][data-sheet=\"Data\"]",
                "aria-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "edit session: cross-sheet entry undo restores the source selection and removes only that formula",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Value("name-box", "F5"),
            Check::Value(BAR, ""),
            Check::Absent(cell!("F5")),
            Check::Text(cell!("A1"), "1"),
        ],
    },
    Case {
        name: "edit session: entering unchanged source adds no workbook undo step",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Keys("grid", "1"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", F2),
            Action::Keys("cell-editor", ENTER),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Value("name-box", "A1"),
            Check::Value(BAR, ""),
        ],
    },
    Case {
        name: "edit session: cell Shift+Left keeps the whole text selection for replacement",
        actions: &[
            Action::Keys("grid", "a"),
            Action::Keys("cell-editor", "bcdef"),
            Action::Keys("cell-editor", "\u{E008}\u{E012}\u{E012}\u{E012}\u{E000}"),
            Action::Keys("cell-editor", "Z"),
        ],
        checks: &[
            Check::Value("cell-editor", "abcZ"),
            Check::Absent(cell!("A1")),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "edit session: bar Shift+Left keeps the whole text selection for replacement",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into(BAR, "abcdef"),
            Action::Keys(BAR, "\u{E008}\u{E012}\u{E012}\u{E012}\u{E000}"),
            Action::Keys(BAR, "Z"),
        ],
        checks: &[
            Check::Value(BAR, "abcZ"),
            Check::Absent(cell!("A1")),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "edit session: name-box navigation commits an accepted draft before moving",
        actions: &[Action::Into("name-box", "C3\u{E007}")],
        checks: &[
            Check::Text(cell!("A1"), "abcZ"),
            Check::Value("name-box", "C3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "edit session: name-box navigation preserves rejected draft and focus",
        actions: &[
            Action::Into(BAR, "=1+"),
            Action::Into("name-box", "D4\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Value("name-box", "C3"),
            Check::Starts("edit-session", "Editing Sheet1!C3"),
            Check::Absent(cell!("C3")),
        ],
    },
    Case {
        name: "shared states: Save preserves a rejected draft and returns keyboard focus",
        actions: &[Action::Click("save")],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Attr("status", "data-refused", "true"),
        ],
    },
    Case {
        name: "shared states: New protects a rejected draft and restores focus on Keep working",
        actions: &[Action::Click("new")],
        checks: &[
            Check::Role("replace-dialog", "dialog"),
            Check::Name("replace-dialog", "Keep your current work?"),
            Check::Focused("keep-working"),
            Check::Reachable("keep-working"),
        ],
    },
    Case {
        name: "shared states: keeping the workbook preserves source, draft and its editor",
        actions: &[Action::Click("keep-working")],
        checks: &[
            Check::Value(BAR, "=1+"),
            Check::Focused(BAR),
            Check::Absent("[data-dnacalc=replace-dialog][open]"),
        ],
    },
    Case {
        name: "shared states: discard explicitly replaces the workbook and clears history",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Click("new"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Value(BAR, ""),
            Check::Attr("undo", "disabled", ""),
            Check::Attr("redo", "disabled", ""),
            Check::Text("result-state", "Empty"),
        ],
    },
    Case {
        name: "shared states: entered literals are distinct from calculated values",
        actions: &[
            Action::Keys("grid", "8"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Literal"),
            Check::Text("selected-value", "8"),
            Check::Role("grid", "grid"),
            Check::Name("grid", "Sheet1 worksheet"),
            Check::Role("#dc-grid-cell-0-1-1", "gridcell"),
            Check::Name("#dc-grid-cell-0-1-1", "A1, 8, literal"),
            Check::Bounded("[role=gridcell]", 5000),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "shared states: Save enters an accepted draft before preparing its package",
        actions: &[
            Action::Into("name-box", "E1\u{E007}"),
            Action::Into(BAR, "=1+2"),
            Action::Click("save"),
        ],
        checks: &[
            Check::Starts("status", "Download started: Book1.xlsx."),
            Check::Text(cell!("E1"), "3"),
            Check::Absent("edit-session"),
        ],
    },
    Case {
        name: "shared states: a worksheet error has meaning and calculated provenance",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=1/0\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Worksheet error"),
            Check::Text("error-meaning", "division by zero"),
            Check::Starts(
                "cell-provenance",
                "Sheet1!B1: calculated by DNA Calc's engine",
            ),
            Check::Reachable("recover-edit"),
            Check::Attr("result-announcement", "aria-live", "polite"),
        ],
    },
    Case {
        name: "shared states: a refusal opens the exact reason without inventing an owner",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=WEBSERVICE(\"https://example.com\")\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Starts("cell-provenance", "Sheet1!C1: refused: "),
            Check::Absent("refusal-owner"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared states: backend-supplied ownership is visible with the refusal reason",
        actions: &[
            Action::Into("name-box", "F1\u{E007}"),
            Action::Into(BAR, "='[Missing.xlsx]Data'!A1\u{E007}"),
            Action::Into("name-box", "F1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Text("refusal-owner", "Work package: dc-ylr"),
            Check::Starts("cell-provenance", "Sheet1!F1: refused: "),
        ],
    },
    Case {
        name: "shared refusal in Canvas",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared refusal in Atlas",
        actions: &[Action::Click("skin-atlas")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Reachable("recover-edit"),
            Check::Absent("[data-dnacalc=atlas-diagnostics][open]"),
        ],
    },
    Case {
        name: "shared refusal in Ledger",
        actions: &[Action::Click("skin-ledger")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Name("ledger-filter", "Formula filter"),
            Check::Reachable("recover-edit"),
        ],
    },
    Case {
        name: "shared refusal in Tree",
        actions: &[Action::Click("skin-tree")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Role("tree", "tree"),
            Check::Name("tree", "Workbook outline"),
            Check::Bounded("[role=treeitem]", 3000),
            Check::ActiveDescendant("tree"),
        ],
    },
    Case {
        name: "shared refusal in Notebook",
        actions: &[Action::Click("skin-notebook")],
        checks: &[
            Check::Text("result-state", "Refused"),
            Check::Name(
                "[data-dnacalc=block][data-cell=C1] textarea",
                "Input for C1",
            ),
        ],
    },
    Case {
        name: "shared states: recovery opens the selected source without replacing it",
        actions: &[Action::Click("recover-edit")],
        checks: &[
            Check::Value(BAR, "=WEBSERVICE(\"https://example.com\")"),
            Check::Focused(BAR),
        ],
    },
    Case {
        name: "shared states: manual edits show Stale with an explicit Calculate recovery",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Click("skin-grid"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "=A1*2\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "9\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click("result-toggle"),
        ],
        checks: &[
            Check::Text("result-state", "Stale"),
            Check::Text("selected-value", "16"),
            Check::Reachable("recover-calculate"),
        ],
    },
    Case {
        name: "shared states: recovery updates the selected result through the worker",
        actions: &[Action::Click("recover-calculate")],
        checks: &[
            Check::Text("result-state", "Calculated"),
            Check::Text("selected-value", "18"),
            Check::Starts(
                "state-explanation",
                "Calculated by DNA Calc. Agreement with Excel has not been checked here.",
            ),
        ],
    },
    Case {
        name: "shared states: Open protects edits; cancelling keeps the current workbook and history",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::Wait(Check::Focused("keep-working")),
            Action::Keys("keep-working", ESCAPE),
        ],
        checks: &[
            Check::Starts("document", "Book1.xlsx"),
            Check::Value("name-box", "D1"),
            Check::Value(BAR, "=A1*2"),
            Check::Text("selected-value", "18"),
        ],
    },
    Case {
        name: "shared states: a downloaded package is not confirmation of durable storage",
        actions: &[
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::Click("new"),
        ],
        checks: &[
            Check::Reachable("keep-working"),
            Check::Text(
                "download-warning",
                "Download unconfirmed · check your files",
            ),
        ],
    },
    Case {
        name: "shared states: F6 exits the worksheet and returns without losing selection",
        actions: &[Action::Click("keep-working"), Action::Keys("grid", F6)],
        checks: &[
            Check::Focused("[data-dnacalc=sheet-tab][aria-selected=true]"),
            Check::Value("name-box", "D1"),
        ],
    },
    Case {
        name: "shared states: Shift+F6 returns to the workspace",
        actions: &[Action::Keys(
            "[data-dnacalc=sheet-tab][aria-selected=true]",
            "\u{E008}\u{E036}\u{E000}",
        )],
        checks: &[Check::Focused("grid"), Check::Value("name-box", "D1")],
    },
    Case {
        name: "shared shell: commands, worksheet tabs, status and editor fit at 390 pixels",
        actions: &[Action::Window(390, 844)],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("new"),
            Check::Reachable("open"),
            Check::Reachable("save"),
            Check::Reachable(BAR),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
            Check::Contrast("result-state"),
        ],
    },
    Case {
        name: "shared shell: commands, tabs and status fit at 1024 pixels",
        actions: &[Action::Window(1024, 768)],
        checks: &[
            Check::Fits,
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
            Check::Reachable(BAR),
        ],
    },
    Case {
        name: "shared shell: dark theme state text has measured contrast",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("settings-toggle"),
            Action::Choose("theme", "cockpit-dark"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Fits,
            Check::Contrast("result-state"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared shell: high-contrast theme state text has measured contrast",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", "high-contrast"),
            Action::Click("settings-toggle"),
        ],
        checks: &[
            Check::Contrast("result-state"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared states: a downloaded package explicitly declaring Manual opens with file-cache provenance",
        actions: &[
            Action::Click("settings-toggle"),
            Action::Choose("theme", ""),
            Action::Click("settings-toggle"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Into("name-box", "D1\u{E007}"),
        ],
        checks: &[
            Check::Text("result-state", "File cache"),
            Check::Text("selected-value", "18"),
            Check::Starts("cell-provenance", "Sheet1!D1: the value the file stored"),
        ],
    },
    Case {
        name: "shared states: an uncommitted Notebook draft is protected by New",
        actions: &[
            Action::Click("skin-notebook"),
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-select]"),
            Action::Wait(Check::Value(
                "[data-dnacalc=block][data-cell=A1] textarea",
                "9",
            )),
            Action::Into("[data-dnacalc=block][data-cell=A1] textarea", "123"),
            Action::Click("new"),
        ],
        checks: &[
            Check::Focused("keep-working"),
            Check::Reachable("keep-working"),
        ],
    },
    Case {
        name: "shared states: a Notebook draft survives Keep working and a skin round trip",
        actions: &[
            Action::Click("keep-working"),
            Action::Click("skin-grid"),
            Action::Click("skin-notebook"),
        ],
        checks: &[Check::Value(
            "[data-dnacalc=block][data-cell=A1] textarea",
            "123",
        )],
    },
    Case {
        name: "shared states: Save commits the shared Notebook draft",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts(
                "status",
                "Download started: manual-cache.xlsx.",
            )),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "123",
            ),
            Check::Value(BAR, "123"),
        ],
    },
    Case {
        name: "shared states: cancelling a Notebook draft restores authored input",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Value(
                "[data-dnacalc=block][data-cell=A1] textarea",
                "9",
            )),
            Action::Into("[data-dnacalc=block][data-cell=A1] textarea", "456"),
            Action::Keys("[data-dnacalc=block][data-cell=A1] textarea", ESCAPE),
        ],
        checks: &[
            Check::Value("[data-dnacalc=block][data-cell=A1] textarea", "9"),
            Check::Value(BAR, "9"),
            Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "9",
            ),
        ],
    },
    Case {
        name: "shared sheet tabs: arrows activate and focus other sheets",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Keys("[data-dnacalc=sheet-tab][aria-selected=true]", "\u{E014}"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            ),
            Check::Focused("[data-dnacalc=sheet-tab][data-sheet=Other]"),
        ],
    },
    Case {
        name: "shared sheet tabs: Home returns keyboard focus to the first sheet",
        actions: &[Action::Keys(
            "[data-dnacalc=sheet-tab][aria-selected=true]",
            "\u{E011}",
        )],
        checks: &[
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Data]",
                "aria-selected",
                "true",
            ),
            Check::Focused("[data-dnacalc=sheet-tab][data-sheet=Data]"),
        ],
    },
    Case {
        name: "shared shell: F6 leaves settings and closes the overlay",
        actions: &[
            Action::Click("skin-grid"),
            Action::Click("settings-toggle"),
            Action::Keys("settings-toggle", F6),
        ],
        checks: &[
            Check::Focused(BAR),
            Check::Absent("[data-dnacalc=settings][open]"),
        ],
    },
    Case {
        name: "shared shell: workspace resizing keeps the active cell editor visible",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into("name-box", "F200\u{E007}"),
            Action::Keys("grid", F2),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Reachable("cell-editor"),
            Check::Focused("cell-editor"),
            Check::Reachable("sheet-tabs"),
            Check::Reachable("worker-status"),
        ],
    },
    Case {
        name: "shared shell: leaving a resized editor keeps its authored source",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Window(1440, 1000),
        ],
        checks: &[Check::Value("name-box", "F200"), Check::Value(BAR, "")],
    },
    Case {
        name: "shared tree semantics: an empty active cell still has a rendered outline focus",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-tree"),
        ],
        checks: &[Check::Role("tree", "tree"), Check::ActiveDescendant("tree")],
    },
    Case {
        name: "navigation: contiguous cells stop at their edge",
        actions: &[
            Action::Click("skin-grid"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into(BAR, "1\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "A5\u{E007}"),
            Action::Into(BAR, "5\u{E007}"),
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "6\u{E007}"),
            Action::Into("name-box", "A8\u{E007}"),
            Action::Into(BAR, "=\"\"\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E015}\u{E000}"),
        ],
        checks: &[Check::Value("name-box", "A3"), Check::Value(BAR, "3")],
    },
    Case {
        name: "navigation: a blank gap lands at the next occupied cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E015}\u{E000}")],
        checks: &[Check::Value("name-box", "A5")],
    },
    Case {
        name: "navigation: repeated data-edge keys use canonical worker selection",
        actions: &[Action::Keys("grid", "\u{E009}\u{E015}\u{E015}\u{E000}")],
        checks: &[Check::Value("name-box", "A8"), Check::Value(BAR, "=\"\"")],
    },
    Case {
        name: "navigation: an empty formula result still occupies its cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E013}\u{E000}")],
        checks: &[Check::Value("name-box", "A6")],
    },
    Case {
        name: "navigation: Ctrl Shift extends to the same data edge",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E008}\u{E015}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "selection: upper-right anchored extension keeps both selected columns",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E012}\u{E015}\u{E015}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "A1:B3")),
            Action::Keys("grid", "\u{E008}\u{E015}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "B1"),
            Check::Attr("selection", "data-area", "A1:B4"),
        ],
    },
    Case {
        name: "selection: lower-left anchored extension keeps both selected rows",
        actions: &[
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E013}\u{E013}\u{E014}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "A1:B3")),
            Action::Keys("grid", "\u{E008}\u{E014}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A3"),
            Check::Attr("selection", "data-area", "A1:C3"),
        ],
    },
    Case {
        name: "navigation: a blank row can reach the physical last column",
        actions: &[
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "\u{E009}\u{E014}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "XFD2"),
            Check::Reachable("active-cell"),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "navigation: Ctrl Left finds the preceding occupied cell",
        actions: &[Action::Keys("grid", "\u{E009}\u{E012}\u{E000}")],
        checks: &[Check::Value("name-box", "A2")],
    },
    Case {
        name: "selection: the name box accepts a rectangle",
        actions: &[Action::Into("name-box", "C10:D12\u{E007}")],
        checks: &[
            Check::Value("name-box", "C10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Enter retains the rectangle while moving the active cell",
        actions: &[Action::Keys("grid", ENTER)],
        checks: &[
            Check::Value("name-box", "C11"),
            Check::Attr("selection", "data-area", "C10:D12"),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='10']",
                "class",
                "dc-head dc-on",
            ),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='12']",
                "class",
                "dc-head dc-on",
            ),
            Check::Attr("#dc-grid-cell-0-10-3", "aria-selected", "true"),
        ],
    },
    Case {
        name: "selection: unmeasured interior-anchor extension keeps the existing range",
        actions: &[Action::Keys("grid", "\u{E008}\u{E015}\u{E000}")],
        checks: &[
            Check::Value("name-box", "C11"),
            Check::Attr("selection", "data-area", "C10:D12"),
            Check::Starts(
                "status",
                "Range extension from an interior active cell awaits Excel measurement",
            ),
        ],
    },
    Case {
        name: "selection: Enter wraps down columns inside the rectangle",
        actions: &[Action::Keys("grid", "\u{E007}\u{E007}")],
        checks: &[
            Check::Value("name-box", "D10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Shift Enter reverses the range walk",
        actions: &[Action::Keys("grid", "\u{E008}\u{E007}\u{E000}")],
        checks: &[
            Check::Value("name-box", "C12"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Tab walks across rows then wraps",
        actions: &[Action::Keys("grid", "\u{E004}\u{E004}")],
        checks: &[
            Check::Value("name-box", "C10"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Shift Tab reverses the horizontal walk",
        actions: &[Action::Keys("grid", "\u{E008}\u{E004}\u{E000}")],
        checks: &[
            Check::Value("name-box", "D12"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: Ctrl Enter enters a rectangle as one workbook edit",
        actions: &[
            Action::Into("name-box", "B10:B12\u{E007}"),
            Action::Into(BAR, "7\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "B10"),
            Check::Attr("selection", "data-area", "B10:B12"),
            Check::Text(cell!("B10"), "7"),
            Check::Text(cell!("B11"), "7"),
            Check::Text(cell!("B12"), "7"),
            Check::Absent("edit-session"),
        ],
    },
    Case {
        name: "selection: one Undo removes the entire Ctrl Enter entry",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Absent(cell!("B10")),
            Check::Absent(cell!("B11")),
            Check::Absent(cell!("B12")),
            Check::Attr("selection", "data-area", "B10:B12"),
        ],
    },
    Case {
        name: "selection: one Redo restores the entire entry",
        actions: &[Action::Keys("grid", "\u{E009}y\u{E000}")],
        checks: &[
            Check::Text(cell!("B10"), "7"),
            Check::Text(cell!("B12"), "7"),
        ],
    },
    Case {
        name: "selection: Ctrl Enter relocates formula references through the host",
        actions: &[
            Action::Into("name-box", "C10:D12\u{E007}"),
            Action::Into(BAR, "=A1+$A$2\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("C10"), "3"),
            Check::Text(cell!("C11"), "4"),
            Check::Text(cell!("C12"), "5"),
            Check::Text(cell!("D10"), "2"),
            Check::Attr("selection", "data-area", "C10:D12"),
        ],
    },
    Case {
        name: "selection: moved range source is authored as a relocated formula",
        actions: &[Action::Into("name-box", "C11\u{E007}")],
        checks: &[Check::Value(BAR, "=A2+$A$2")],
    },
    Case {
        name: "selection: Ctrl Space selects a column and retains the active cell",
        actions: &[
            Action::Into("name-box", "E4\u{E007}"),
            Action::Keys("grid", "\u{E009} \u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "E4"),
            Check::Attr("selection", "data-area", "E1:E1048576"),
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "selection: Shift Space selects a row and retains the active cell",
        actions: &[
            Action::Into("name-box", "E4\u{E007}"),
            Action::Keys("grid", "\u{E008} \u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "E4"),
            Check::Attr("selection", "data-area", "A4:XFD4"),
        ],
    },
    Case {
        name: "selection: Ctrl Space expands every column touched by a rectangle",
        actions: &[
            Action::Into("name-box", "B2:C3\u{E007}"),
            Action::Keys("grid", "\u{E009} \u{E000}"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "B1:C1048576"),
            Check::Value("name-box", "B2"),
        ],
    },
    Case {
        name: "selection: Shift Space expands every row touched by a rectangle",
        actions: &[
            Action::Into("name-box", "B2:C3\u{E007}"),
            Action::Keys("grid", "\u{E008} \u{E000}"),
        ],
        checks: &[
            Check::Attr("selection", "data-area", "A2:XFD3"),
            Check::Value("name-box", "B2"),
        ],
    },
    Case {
        name: "selection: Ctrl A selects the current region",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E009}a\u{E000}"),
        ],
        checks: &[Check::Attr("selection", "data-area", "A1:A3")],
    },
    Case {
        name: "selection: another Ctrl A selects the worksheet",
        actions: &[Action::Keys("grid", "\u{E009}a\u{E000}")],
        checks: &[
            Check::Attr("selection", "data-area", "A1:XFD1048576"),
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "selection: Ctrl click adds a nonadjacent area",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::CtrlClick(cell!("A3")),
        ],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Attr("#dc-grid-cell-0-1-1", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-0-3-1", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-0-2-1", "aria-selected", "false"),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='2']",
                "class",
                "dc-head",
            ),
            Check::Attr(
                "[data-dnacalc=row-header][data-row='3']",
                "class",
                "dc-head dc-on",
            ),
        ],
    },
    Case {
        name: "selection: disjoint axis expansion is diagnosed without losing areas",
        actions: &[Action::Keys("grid", "\u{E009} \u{E000}")],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Starts(
                "status",
                "Axis expansion for disjoint selections is not modelled",
            ),
        ],
    },
    Case {
        name: "selection: disjoint copying diagnoses the one-rectangle requirement",
        actions: &[Action::Keys("grid", "\u{E009}c\u{E000}")],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 2),
            Check::Starts("status", "Select one rectangle for this command"),
        ],
    },
    Case {
        name: "selection: Ctrl click can remove a selected cell",
        actions: &[Action::CtrlClick(cell!("A1"))],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 1),
            Check::Attr("#dc-grid-cell-0-1-1", "aria-selected", "false"),
            Check::Attr("#dc-grid-cell-0-3-1", "aria-selected", "true"),
        ],
    },
    Case {
        name: "selection: Delete clears disjoint areas with one Undo",
        actions: &[
            Action::CtrlClick(cell!("A1")),
            Action::Keys("grid", "\u{E017}"),
        ],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Absent(cell!("A3")),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "selection: disjoint clearing Undo restores both sources and areas",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A3"), "3"),
            Check::Count("[data-dnacalc=selection]", 2),
        ],
    },
    Case {
        name: "selection: a real drag beyond the viewport scrolls and extends",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::DragOutside(cell!("A1")),
        ],
        checks: &[Check::DragExtended, Check::Focused("grid")],
    },
    Case {
        name: "navigation: PageDown moves a screen while preserving relative position",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("name-box", "A10\u{E007}"),
            Action::Keys("grid", "\u{E00F}"),
        ],
        checks: &[Check::PageAligned, Check::Reachable("active-cell")],
    },
    Case {
        name: "navigation: PageUp returns to the original cell",
        actions: &[Action::Keys("grid", "\u{E00E}")],
        checks: &[
            Check::Value("name-box", "A10"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: Ctrl End uses the current used corner",
        actions: &[Action::Keys("grid", "\u{E009}\u{E010}\u{E000}")],
        checks: &[
            Check::Value("name-box", "D12"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: a defined range resolves through the existing workbook catalogue",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "InputRange\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Value(BAR, "1"),
        ],
    },
    Case {
        name: "navigation: a sheet-scoped name resolves on its sheet",
        actions: &[Action::Into("name-box", "LocalIn\u{E007}")],
        checks: &[Check::Value("name-box", "A2"), Check::Value(BAR, "2")],
    },
    Case {
        name: "navigation: a qualified range changes sheets",
        actions: &[Action::Into("name-box", "'Target Sheet'!B2:C3\u{E007}")],
        checks: &[
            Check::Value("name-box", "B2"),
            Check::Attr("selection", "data-area", "B2:C3"),
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet='Target Sheet']",
                "aria-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "navigation: invalid input diagnoses the name without changing selection",
        actions: &[Action::Into("name-box", "XFE1\u{E007}")],
        checks: &[
            Check::Value("name-box", "B2"),
            Check::Starts("status", "\"XFE1\" is not a valid"),
            Check::Attr("selection", "data-area", "B2:C3"),
        ],
    },
    Case {
        name: "selection: a pointed cross-sheet edit restores its original range after returning",
        actions: &[
            Action::Into("name-box", "Data!A1:A3\u{E007}"),
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Data]",
                "aria-selected",
                "true",
            )),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::Into(BAR, "=1+"),
            Action::Wait(Check::Value(BAR, "=1+")),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Other]"),
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            )),
            Action::Wait(Check::Text(cell!("A1"), "100")),
            Action::Click(cell!("A1")),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
            Action::Keys(BAR, "\u{E009}\u{E007}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Text(cell!("A1"), "101"),
            Check::Text(cell!("A2"), "1"),
            Check::Text(cell!("A3"), "1"),
        ],
    },
    Case {
        name: "selection: cross-sheet range entry Undo restores all original cells",
        actions: &[Action::Keys("grid", "\u{E009}z\u{E000}")],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "navigation: hidden columns are skipped",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "B12\u{E007}"),
            Action::Keys("grid", "\u{E014}"),
        ],
        checks: &[Check::Value("name-box", "D12")],
    },
    Case {
        name: "navigation: hidden rows are skipped",
        actions: &[Action::Keys("grid", "\u{E015}")],
        checks: &[Check::Value("name-box", "D14")],
    },
    Case {
        name: "navigation: unmeasured table selection stages are explicitly diagnosed",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "Data!D2\u{E007}"),
            Action::Keys("grid", "\u{E009}a\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "D2"),
            Check::Attr("selection", "data-area", "D2:D2"),
            Check::Starts("status", "Table selection stages are not modelled"),
        ],
    },
    Case {
        name: "navigation: physical last cell remains reachable in both browsers",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into("name-box", "XFD1048576\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Reachable("active-cell"),
            Check::ActiveDescendant("grid"),
            Check::ScrollAtEnd,
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "navigation: arrows clamp at the physical worksheet edge",
        actions: &[Action::Keys("grid", "\u{E014}\u{E015}")],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: Home and Ctrl Home use physical worksheet coordinates",
        actions: &[
            Action::Keys("grid", "\u{E011}"),
            Action::Wait(Check::Value("name-box", "A1048576")),
            Action::Keys("grid", "\u{E009}\u{E011}\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "navigation: the last row is reachable at maximum zoom",
        actions: &[
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Click("zoom-in"),
            Action::Into("name-box", "XFD1048576\u{E007}"),
        ],
        checks: &[
            Check::Text("zoom", "400%"),
            Check::Reachable("active-cell"),
            Check::ScrollAtEnd,
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "navigation: entry at the last physical cell stays in bounds",
        actions: &[Action::Into(BAR, "9\u{E007}")],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::Value(BAR, "9"),
            Check::Text("selected-value", "9"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "clipboard: a real external TSV matrix pastes into a new workbook",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::ExternalClipboard("2\t3\n4\t5", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "2"),
            Check::Text(cell!("B1"), "3"),
            Check::Text(cell!("A2"), "4"),
            Check::Text(cell!("B2"), "5"),
            Check::Value(BAR, "2"),
        ],
    },
    Case {
        name: "clipboard: one Undo removes the whole external paste, Redo restores it",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Absent(cell!("A1")),
            Check::Absent(cell!("B1")),
            Check::Absent(cell!("A2")),
            Check::Absent(cell!("B2")),
        ],
    },
    Case {
        name: "clipboard: real HTML table paste decodes entities, formulas and empty cells",
        actions: &[
            Action::ExternalClipboard(
                "<table><tr><td>7</td><td>=A1*2</td></tr><tr><td>A &amp; B</td><td></td></tr></table>",
                true,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::Text(cell!("A2"), "A & B"),
            Check::Absent(cell!("B2")),
        ],
    },
    Case {
        name: "clipboard: a conflict previews without changing cells; Escape cancels",
        actions: &[
            Action::ExternalClipboard("99\t100", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Keys("paste-confirm", ESCAPE),
        ],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "clipboard: confirming replaces the destination as one undo group",
        actions: &[
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "99"),
            Check::Text(cell!("B1"), "100"),
        ],
    },
    Case {
        name: "clipboard: Undo restores the overwritten formula and value",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("A1"), "7"),
            Check::Text(cell!("B1"), "14"),
            Check::PlanCell("B1", "formula", "true"),
        ],
    },
    Case {
        name: "clipboard: copied formulas are immutable snapshots with mixed anchors",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=$A1+A$1+$A$1\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline", 1)),
            Action::Into(BAR, "=999\u{E007}"),
            Action::Into("name-box", "D2\u{E007}"),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Value(BAR, "=$A2+B$1+$A$1"),
            Check::PlanCell("D2", "formula", "true"),
            Check::Text(cell!("C1"), "999"),
        ],
    },
    Case {
        name: "clipboard: Paste Special values uses the value captured at copy time",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline", 1)),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "10\u{E007}"),
            Action::Click("[data-dnacalc=clipboard-tools] summary"),
            Action::Choose("paste-mode", "values"),
            Action::Into("name-box", "F1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::Text(cell!("F1"), "14"),
            Check::PlanCell("F1", "formula", "false"),
            Check::Text(cell!("B1"), "20"),
        ],
    },
    Case {
        name: "clipboard: the explicit text route needs no clipboard API permissions",
        actions: &[
            Action::Choose("paste-mode", "all"),
            Action::Into("name-box", "F3\u{E007}"),
            Action::ExternalClipboard("8\t9\n10\t11", false),
            Action::Click("paste-fallback"),
            Action::Keys("clipboard-text", "\u{E009}v\u{E000}"),
            Action::Click("paste-text"),
        ],
        checks: &[
            Check::Text(cell!("F3"), "8"),
            Check::Text(cell!("G3"), "9"),
            Check::Text(cell!("F4"), "10"),
            Check::Text(cell!("G4"), "11"),
        ],
    },
    Case {
        name: "clipboard: a malformed external matrix fails atomically",
        actions: &[
            Action::Into("name-box", "F3\u{E007}"),
            Action::ExternalClipboard("1\t2\n3", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Starts("status", "Paste a rectangular table"),
            Check::Text(cell!("F3"), "8"),
            Check::Text(cell!("G4"), "11"),
        ],
    },
    Case {
        name: "clipboard: a partial spill write is refused before any neighboring edit",
        actions: &[
            Action::Into("name-box", "I1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(3)\u{E007}"),
            Action::Into("name-box", "H2\u{E007}"),
            Action::ExternalClipboard("5\t6", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::TextContains("status", "part of the array"),
            Check::Absent(cell!("H2")),
            Check::Text(cell!("I2"), "2"),
        ],
    },
    Case {
        name: "fill: the handle previews numeric series and cancellation leaves cells empty",
        actions: &[
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "1\u{E007}"),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "A6:A7\u{E007}"),
            Action::DragToCell("fill-handle", 10, "A"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-cancel"),
        ],
        checks: &[
            Check::Absent(cell!("A8")),
            Check::Absent(cell!("A10")),
            Check::Attr("selection", "data-area", "A6:A7"),
        ],
    },
    Case {
        name: "fill: a numeric series extends its seeds with one Undo",
        actions: &[
            Action::DragToCell("fill-handle", 10, "A"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A8"), "5"),
            Check::Text(cell!("A9"), "7"),
            Check::Text(cell!("A10"), "9"),
        ],
    },
    Case {
        name: "fill: one Undo removes the series and retains its seeds",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("A7"), "3"),
            Check::Absent(cell!("A8")),
            Check::Absent(cell!("A10")),
        ],
    },
    Case {
        name: "fill: keyboard users preview and apply the same series through an end address",
        actions: &[
            Action::Into("fill-through", "A10\u{E007}"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A8"), "5"),
            Check::Text(cell!("A10"), "9"),
        ],
    },
    Case {
        name: "fill: keyboard series is one Undo and compact commands fit a phone viewport",
        actions: &[Action::Click("undo"), Action::Window(390, 844)],
        checks: &[
            Check::Absent(cell!("A10")),
            Check::Fits,
            Check::Reachable("paste"),
            Check::Reachable("fill-through-preview"),
        ],
    },
    Case {
        name: "clipboard: native copy exports a table readable in a plain text editor",
        actions: &[
            Action::Window(1440, 1000),
            Action::Into("name-box", "F3:G4\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::PauseMillis(150),
            Action::Click("paste-fallback"),
            Action::Keys("clipboard-text", "\u{E009}v\u{E000}"),
        ],
        checks: &[Check::Value("clipboard-text", "8\t9\n10\t11")],
    },
    Case {
        name: "clipboard: external blanks clear a selected rectangle and repeat as a whole matrix",
        actions: &[
            Action::Click("paste-cancel"),
            Action::Into("name-box", "F3:G4\u{E007}"),
            Action::ExternalClipboard("\t12", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::TextContains("paste-preview", "4 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Absent(cell!("F3")),
            Check::Absent(cell!("F4")),
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
        ],
    },
    Case {
        name: "clipboard: a failed cut paste retains the source and its snapshot",
        actions: &[
            Action::Into("name-box", "G3:G4\u{E007}"),
            Action::Keys("grid", "\u{E009}x\u{E000}"),
            Action::Into("name-box", "K1:L3\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "not a whole multiple"),
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
            Check::Attr("copy-outline", "data-cut", "true"),
            Check::Absent(cell!("K1")),
        ],
    },
    Case {
        name: "clipboard: cancelling a cut overwrite retains both source and destination",
        actions: &[
            Action::Into("name-box", "A6:A7\u{E007}"),
            Action::Click("paste-snapshot"),
            Action::Wait(Check::TextContains("paste-preview", "2 existing inputs")),
            Action::Click("paste-cancel"),
        ],
        checks: &[
            Check::Text(cell!("G3"), "12"),
            Check::Text(cell!("G4"), "12"),
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("A7"), "3"),
        ],
    },
    Case {
        name: "clipboard: an edited cut source is refused without clearing either range",
        actions: &[
            Action::Into("name-box", "G3\u{E007}"),
            Action::Into(BAR, "15\u{E007}"),
            Action::Into("name-box", "K1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "cut source changed"),
            Check::Text(cell!("G3"), "15"),
            Check::Absent(cell!("K1")),
        ],
    },
    Case {
        name: "fill: formula handle fill keeps relative and mixed references",
        actions: &[
            Action::Into("name-box", "C6\u{E007}"),
            Action::Into(BAR, "=$A6+A$6\u{E007}"),
            Action::Into("name-box", "C6\u{E007}"),
            Action::DragToCell("fill-handle", 9, "C"),
            Action::Wait(Check::TextContains("paste-preview", "3 cells")),
            Action::Click("paste-confirm"),
            Action::Into("name-box", "C9\u{E007}"),
        ],
        checks: &[
            Check::Value(BAR, "=$A9+A$6"),
            Check::Text(cell!("C7"), "4"),
            Check::Text(cell!("C9"), "1"),
        ],
    },
    Case {
        name: "fill: Ctrl R repeats the first column as one operation",
        actions: &[
            Action::Into("name-box", "A6:B7\u{E007}"),
            Action::Keys("grid", "\u{E009}r\u{E000}"),
        ],
        checks: &[Check::Text(cell!("B6"), "1"), Check::Text(cell!("B7"), "3")],
    },
    Case {
        name: "fill: an open formula draft disables the keyboard fill commands",
        actions: &[
            Action::Into("name-box", "A6\u{E007}"),
            Action::Into(BAR, "2"),
        ],
        checks: &[
            Check::Value(BAR, "2"),
            Check::Attr("fill-through", "disabled", ""),
            Check::Attr("fill-through-preview", "disabled", ""),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
            Check::Text(cell!("A6"), "1"),
        ],
    },
    Case {
        name: "clipboard: toolbar Paste refuses several selected areas before reading the clipboard",
        actions: &[
            Action::Keys(BAR, ESCAPE),
            Action::Into("name-box", "A6\u{E007}"),
            Action::CtrlClick(cell!("B7")),
            Action::Click("paste"),
        ],
        checks: &[
            Check::TextContains("status", "Select one rectangle"),
            Check::Text(cell!("A6"), "1"),
            Check::Text(cell!("B7"), "3"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
        ],
    },
    Case {
        name: "clipboard: Paste Special formats retains content and uses the copied cell format",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Into(BAR, "45323\u{E007}"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Choose("paste-mode", "formats"),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::Text(cell!("G3"), "########"),
            Check::Value(BAR, "45323"),
            Check::PlanCell("G3", "format", "applied"),
            Check::PlanCell("G3", "formula", "false"),
        ],
    },
    Case {
        name: "clipboard: Undo repaints a format-only paste without changing its input",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("G3"), "45323"),
            Check::Value(BAR, "45323"),
            Check::PlanCell("G3", "format", "general"),
        ],
    },
    Case {
        name: "clipboard: Redo repaints a format-only paste",
        actions: &[Action::Click("redo")],
        checks: &[Check::Text(cell!("G3"), "########")],
    },
    Case {
        name: "clipboard: Paste Special formulas keeps the destination format",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=45000+323\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Choose("paste-mode", "formulas"),
            Action::Into("name-box", "G3\u{E007}"),
            Action::Click("paste-snapshot"),
            Action::Wait(Check::TextContains("paste-preview", "1 existing inputs")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::PlanCell("G3", "format", "applied"),
            Check::PlanCell("G3", "formula", "true"),
        ],
    },
    Case {
        name: "fill: date handle fill extends daily dates with the source format",
        actions: &[
            Action::Choose("paste-mode", "all"),
            Action::Into("name-box", "A3\u{E007}"),
            Action::DragToCell("fill-handle", 5, "A"),
            Action::Wait(Check::TextContains("paste-preview", "2 cells")),
            Action::Click("paste-confirm"),
        ],
        checks: &[
            Check::Text(cell!("A4"), "2024-02-01"),
            Check::Text(cell!("A5"), "2024-02-02"),
        ],
    },
    Case {
        name: "clipboard: special formats and a filled date survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("formats.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text(cell!("A5"), "2024-02-02"),
            Check::PlanCell("G3", "format", "applied"),
            Check::PlanCell("G3", "formula", "true"),
        ],
    },
    Case {
        name: "clipboard: stale spill-child copy refuses an unavailable value without clearing the target",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "3\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(A1)\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "99\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "B2\u{E007}"),
            Action::Keys("grid", "\u{E009}c\u{E000}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::TextContains("status", "no current calculated value"),
            Check::Text(cell!("D1"), "99"),
            Check::PlanCell("B2", "provenance", "stale"),
            Check::Absent("[data-dnacalc=clipboard-dialog][open]"),
        ],
    },
    Case {
        name: "clipboard: a protected sheet refuses a paste atomically",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "7\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ProtectedDownloadFixture,
            Action::UploadDownload("protected.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "protected.xlsx")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::ExternalClipboard("9\t10", false),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::TextContains("status", "protected sheet"),
            Check::Text(cell!("A1"), "7"),
            Check::Absent(cell!("B1")),
            Check::Text("document", "protected.xlsx"),
        ],
    },
    Case {
        name: "panes: imported frozen rows and columns remain fixed during a real wheel scroll",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
            Action::Wait(Check::Attr("grid", "data-frozen-rows", "1")),
            Action::Wait(Check::Attr("grid", "data-frozen-columns", "1")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Reachable(cell!("A1"))),
            Action::RememberView(cell!("A1")),
            Action::ScrollGrid(180, 300),
        ],
        checks: &[Check::PinnedCell, Check::Bounded("[role=gridcell]", 1800)],
    },
    Case {
        name: "panes: a click in the frozen corner edits the original cell after scrolling",
        actions: &[Action::Click(cell!("A1")), Action::Keys("grid", F2)],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Focused("cell-editor"),
        ],
    },
    Case {
        name: "panes: Unfreeze changes the viewport through a worker edit",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Click("[data-dnacalc=pane-tools] summary"),
            Action::Click("unfreeze"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "0"),
            Check::Style(cell!("A1"), "width:145px"),
            Check::Style(cell!("A3"), "height:40px"),
        ],
    },
    Case {
        name: "panes: Undo restores both imported frozen boundaries",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "1"),
            Check::Attr("grid", "data-frozen-columns", "1"),
        ],
    },
    Case {
        name: "panes: Redo removes the restored boundaries",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "0"),
        ],
    },
    Case {
        name: "panes: Freeze above and left resolves the active cell after pending navigation",
        actions: &[
            Action::Into("name-box", "C3\u{E007}"),
            Action::Click("freeze-active"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "2"),
            Check::Attr("grid", "data-frozen-columns", "2"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "panes: authored frozen boundaries survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("features.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "2"),
            Check::Attr("grid", "data-frozen-columns", "2"),
        ],
    },
    Case {
        name: "panes: first-row and first-column commands replace the frozen axes",
        actions: &[
            Action::Click("freeze-row"),
            Action::Wait(Check::Attr("grid", "data-frozen-rows", "1")),
            Action::Wait(Check::Attr("grid", "data-frozen-columns", "0")),
            Action::Click("freeze-column"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "1"),
        ],
    },
    Case {
        name: "view: an offscreen active selection retains scroll and direct zoom across skins",
        actions: &[
            Action::Click("unfreeze"),
            Action::Click("[data-dnacalc=pane-tools] summary"),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "M100:N102\u{E007}"),
            Action::Into("zoom-percent", "150\u{E007}"),
            Action::Wait(Check::Text("zoom", "150%")),
            Action::ScrollGrid(40, 100),
            Action::RememberView("active-cell"),
            Action::Click("skin-atlas"),
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::ViewRetained,
            Check::Attr("selection", "data-area", "M100:N102"),
            Check::Text("zoom", "150%"),
        ],
    },
    Case {
        name: "view: each sheet retains its own selection, viewport and zoom",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Other]"),
            Action::Into("name-box", "C8:D9\u{E007}"),
            Action::Into("zoom-percent", "75\u{E007}"),
            Action::Wait(Check::Text("zoom", "75%")),
            Action::ScrollGrid(30, 80),
            Action::RememberView("active-cell"),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
        ],
        checks: &[
            Check::ViewRetained,
            Check::Value("name-box", "M100"),
            Check::Text("zoom", "150%"),
        ],
    },
    Case {
        name: "view: Ctrl PageDown restores the next worksheet view",
        actions: &[Action::Keys("grid", "\u{E009}\u{E00F}\u{E000}")],
        checks: &[
            Check::ViewRetained,
            Check::Value("name-box", "C8"),
            Check::Text("zoom", "75%"),
            Check::Attr("selection", "data-area", "C8:D9"),
        ],
    },
    Case {
        name: "view: invalid percentages retain zoom and Reset returns to one hundred percent",
        actions: &[
            Action::Into("zoom-percent", "999\u{E007}"),
            Action::Wait(Check::TextContains(
                "status",
                "zoom percentage from 50 to 400",
            )),
            Action::Wait(Check::Text("zoom", "75%")),
            Action::Click("zoom-reset"),
        ],
        checks: &[
            Check::Text("zoom", "100%"),
            Check::Value("zoom-percent", "100"),
        ],
    },
    Case {
        name: "view: the full last-row label and address fit at phone width",
        actions: &[
            Action::Into("name-box", "XFD1048576\u{E007}"),
            Action::Wait(Check::Value("name-box", "XFD1048576")),
            Action::Wait(Check::Reachable("active-cell")),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Value("name-box", "XFD1048576"),
            Check::ScrollAtEnd,
            Check::ActiveDescendant("grid"),
            Check::FitsText("name-box"),
            Check::FitsText("[data-dnacalc=row-header][data-row='1048576']"),
            Check::Reachable("active-cell"),
            Check::Fits,
        ],
    },
    Case {
        name: "view: restore the desktop viewport after layout cases",
        actions: &[
            Action::Window(1440, 1000),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Reachable("active-cell"),
        ],
    },
    Case {
        name: "axis: typed dimensions resize every selected row and column through the worker",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
            Action::Into("name-box", "A1:B3\u{E007}"),
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "width:145px")),
            Action::Into("row-size", "30\u{E007}"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "left:64px;top:22px;width:145px;height:40px"),
            Check::Style(cell!("B1"), "left:209px;top:22px;width:145px;height:40px"),
            Check::Style(cell!("A3"), "top:102px;height:40px"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: one Undo restores the whole row-height command and keeps widths",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Style(cell!("A1"), "width:145px;height:20px"),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
        ],
    },
    Case {
        name: "axis: Redo restores heights and Hide removes a column without clearing its cells",
        actions: &[
            Action::Click("redo"),
            Action::Wait(Check::Style(cell!("A1"), "height:40px")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("hide-columns"),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            Check::Style(cell!("D1"), "left:209px"),
            Check::Value("name-box", "B1"),
        ],
    },
    Case {
        name: "axis: Show returns the retained custom column width",
        actions: &[Action::Click("show-columns")],
        checks: &[
            Check::Style(cell!("B1"), "left:209px;width:145px"),
            Check::Style(cell!("D1"), "left:354px"),
        ],
    },
    Case {
        name: "axis: Hide and Show each occupy exactly one Undo step",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("B1"))),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(cell!("B1"), "width:145px")],
    },
    Case {
        name: "axis: hiding a row closes its gap and keeps the following row's size",
        actions: &[
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
        ],
    },
    Case {
        name: "axis: Show, Undo and Redo preserve the hidden row's custom height",
        actions: &[
            Action::Click("show-rows"),
            Action::Wait(Check::Style(cell!("A2"), "height:40px")),
            Action::Click("undo"),
            Action::Wait(Check::Absent(cell!("A2"))),
            Action::Click("redo"),
        ],
        checks: &[
            Check::Style(cell!("A2"), "height:40px"),
            Check::Style(cell!("A3"), "top:102px;height:40px"),
        ],
    },
    Case {
        name: "axis: a zero width hides and Show retains the last nonzero width",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "0\u{E007}"),
            Action::Wait(Check::Absent(cell!("B1"))),
            Action::Click("show-columns"),
        ],
        checks: &[Check::Style(cell!("B1"), "width:145px")],
    },
    Case {
        name: "axis: invalid height diagnoses the limit and leaves geometry unchanged",
        actions: &[Action::Into("row-size", "410\u{E007}")],
        checks: &[
            Check::TextContains("status", "409 points"),
            Check::Style(cell!("A1"), "height:40px"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: physical widths, heights and hidden bands survive save and reopen",
        actions: &[
            Action::Click("hide-columns"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("formats.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
        ],
        checks: &[
            Check::Absent(cell!("B1")),
            Check::Absent(cell!("A2")),
            Check::Style(cell!("A1"), "width:145px;height:40px"),
            Check::Style(cell!("A3"), "top:62px;height:40px"),
            Check::Style(cell!("D1"), "left:209px"),
        ],
    },
    Case {
        name: "axis: a rectangle spanning hidden bands shows them, including an imported zero-width column",
        actions: &[
            Action::Into("name-box", "A1:C3\u{E007}"),
            Action::Click("show-columns"),
            Action::Click("show-rows"),
        ],
        checks: &[
            Check::Style(cell!("B1"), "width:145px"),
            Check::Style("[data-dnacalc=column-header][data-column=C]", "width:64px"),
            Check::Style(cell!("A2"), "height:40px"),
            Check::Style(cell!("D1"), "left:418px"),
        ],
    },
    Case {
        name: "axis: whole-column insertion moves the authored widths and Undo restores them",
        actions: &[
            Action::Click("[data-dnacalc=column-header][data-column=B]"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Style(cell!("C1"), "width:145px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:145px"),
        ],
    },
    Case {
        name: "axis: dimension controls and labels remain reachable at phone width",
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Reachable("column-size"),
            Check::Reachable("row-size"),
            Check::Reachable("show-columns"),
            Check::Reachable("show-rows"),
            Check::Fits,
        ],
    },
    Case {
        name: "axis: hidden rows and column widths recalculate SUBTOTAL and CELL through the app route",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=SUBTOTAL(109,A1:A2)\u{E007}"),
            Action::Into("name-box", "C3\u{E007}"),
            Action::Into(BAR, "=SUM(CELL(\"width\",B1))\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Text(cell!("C3"), "20")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
        ],
        checks: &[
            Check::Text(cell!("C1"), "1"),
            Check::Text(cell!("C3"), "20"),
        ],
    },
    Case {
        name: "axis: Undo of hiding recalculates the dependent total without changing inputs",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Text(cell!("C1"), "3"),
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "axis: dragging a selected header edge resizes all selected columns in one command",
        actions: &[
            Action::Into("name-box", "A1:B1048576\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                70,
                false,
            ),
        ],
        checks: &[
            Check::Style(cell!("A1"), "width:215px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:215px"),
            Check::Text(cell!("C3"), "30"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "axis: Undo restores the different widths before a group header drag",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::Style(cell!("A1"), "width:64px"),
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:145px"),
            Check::Text(cell!("C3"), "20"),
        ],
    },
    Case {
        name: "axis: dragging a row boundary resizes selected rows in point units",
        actions: &[
            Action::Into("name-box", "A1:XFD2\u{E007}"),
            Action::ResizeHeader("[data-dnacalc=row-header][data-row='1']", true, 20, false),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:40px"),
            Check::Style(cell!("A2"), "height:40px"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "axis: Escape cancels header resizing without an Undo entry",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Style(cell!("A1"), "height:20px")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                35,
                true,
            ),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=B]",
                "width:145px",
            )),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Absent("resize-preview"),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "axis: dragging to zero hides a column and Show restores its retained width",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                -64,
                false,
            ),
            Action::Wait(Check::Absent("[data-dnacalc=column-header][data-column=B]")),
            Action::Wait(Check::Style(cell!("C1"), "left:128px")),
            Action::Click("show-columns"),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Style(cell!("C1"), "left:192px"),
        ],
    },
    Case {
        name: "axis: a zoomed header drag changes physical width independently of zoom",
        actions: &[
            Action::Into("zoom-percent", "200%\u{E007}"),
            Action::Wait(Check::Text("zoom", "200%")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=A]",
                false,
                70,
                false,
            ),
            Action::Wait(Check::Style(cell!("A1"), "width:198px")),
            Action::Click("zoom-reset"),
            Action::Wait(Check::Style(cell!("A1"), "width:99px")),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "width:64px"),
            Check::Text("zoom", "100%"),
        ],
    },
    Case {
        name: "axis: a dimension edit invalidates imported manual-mode caches without calculating them",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "manual-cache.xlsx")),
            Action::Wait(Check::PlanCell("C3", "provenance", "file-cache")),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
        ],
        checks: &[
            Check::PlanCell("C3", "provenance", "stale"),
            Check::PlanCell("C1", "provenance", "stale"),
            Check::Value("calc-mode", "manual"),
        ],
    },
    Case {
        name: "axis: calculation publishes current width and hidden-row results after manual edits",
        actions: &[
            Action::Click("calculate"),
            Action::Wait(Check::Text(cell!("C3"), "20")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("hide-rows"),
            Action::Wait(Check::PlanCell("C1", "provenance", "stale")),
            Action::Click("calculate"),
        ],
        checks: &[
            Check::Text(cell!("C1"), "1"),
            Check::Text(cell!("C3"), "20"),
            Check::PlanCell("C1", "provenance", "calculated"),
            Check::PlanCell("C3", "provenance", "calculated"),
        ],
    },
    Case {
        name: "axis: AutoFit measures selected displayed text and formula results in the grid font",
        actions: &[
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "'WWWWWWWWWWWWWWWWWWWW\u{E007}"),
            Action::Into(BAR, "short\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "12345\u{E007}"),
            Action::Into(BAR, "=REPT(\"x\",30)\u{E007}"),
            Action::Into("name-box", "A1:B2\u{E007}"),
            Action::Click("[data-dnacalc=axis-tools] summary"),
            Action::RememberView("[data-dnacalc=column-header][data-column=A]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[
            Check::TextContains("status", "Fitted contents"),
            Check::FitsText(cell!("A1")),
            Check::FitsText(cell!("B1")),
            Check::FitsText(cell!("B2")),
            Check::Text(cell!("B2"), "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
            Check::Text(cell!("B1"), "12345"),
        ],
    },
    Case {
        name: "axis: one Undo restores every width before AutoFit and preserves the contents",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::ViewRetained,
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Text(cell!("B2"), "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
        ],
    },
    Case {
        name: "axis: AutoFit height shrinks selected unwrapped rows in one command",
        actions: &[
            Action::Click("autofit-columns"),
            Action::Wait(Check::TextContains("status", "Fitted contents")),
            Action::Into("row-size", "40\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "height:53px")),
            Action::Click("autofit-rows"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:20px"),
            Check::Style(cell!("A2"), "height:20px"),
            Check::TextContains("status", "Fitted contents"),
        ],
    },
    Case {
        name: "axis: Undo and Redo preserve the height fitting as a group",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Style(cell!("A1"), "height:53px")),
            Action::Wait(Check::Style(cell!("A2"), "height:53px")),
            Action::Click("redo"),
        ],
        checks: &[
            Check::Style(cell!("A1"), "height:20px"),
            Check::Style(cell!("A2"), "height:20px"),
        ],
    },
    Case {
        name: "axis: fitted physical dimensions survive a saved package round trip",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
        ],
        checks: &[
            Check::FitsText(cell!("A1")),
            Check::FitsText(cell!("B2")),
            Check::Style(cell!("A1"), "height:20px"),
        ],
    },
    Case {
        name: "axis: fitting an empty band retains its size without an Undo entry",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into("column-size", "20\u{E007}"),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=D]",
                "width:145px",
            )),
            Action::Click("autofit-columns"),
            Action::Wait(Check::TextContains("status", "No visible nonempty")),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=D]",
                "width:145px",
            )),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(
            "[data-dnacalc=column-header][data-column=D]",
            "width:64px",
        )],
    },
    Case {
        name: "axis: double-clicking a column boundary fits its whole column with one Undo",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("column-size", "8.43\u{E007}"),
            Action::Wait(Check::Style(
                "[data-dnacalc=column-header][data-column=A]",
                "width:64px",
            )),
            Action::DoubleClickEdge("[data-dnacalc=column-header][data-column=A]", false),
            Action::Wait(Check::FitsText(cell!("A1"))),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(
            "[data-dnacalc=column-header][data-column=A]",
            "width:64px",
        )],
    },
    Case {
        name: "axis: double-clicking a row boundary fits its content with one Undo",
        actions: &[
            Action::Into("row-size", "30\u{E007}"),
            Action::Wait(Check::Style(cell!("A1"), "height:40px")),
            Action::DoubleClickEdge("[data-dnacalc=row-header][data-row='1']", true),
            Action::Wait(Check::Style(cell!("A1"), "height:20px")),
            Action::Click("undo"),
        ],
        checks: &[Check::Style(cell!("A1"), "height:40px")],
    },
    Case {
        name: "axis: AutoFit refuses merged content atomically and explains the limit",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "H1:I1\u{E007}"),
            Action::RememberView("[data-dnacalc=column-header][data-column=H]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[Check::TextContains("status", "merged"), Check::ViewRetained],
    },
    Case {
        name: "axis: AutoFit rows measures the rendered wrapped lines",
        actions: &[
            Action::Into("name-box", "B5\u{E007}"),
            Action::Click("autofit-rows"),
        ],
        checks: &[
            Check::TextContains("status", "Fitted contents"),
            Check::Computed("[data-cell=B5] .dc-cell-text", "white-space:pre-wrap"),
            Check::TextBounds(cell!("B5")),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "axis: AutoFit uses the base font bold variant that the Grid renders",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::RememberView("[data-dnacalc=column-header][data-column=A]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[
            Check::TextContains("status", "Fitted contents"),
            Check::Computed(cell!("A1"), "font-weight:700"),
            Check::FitsText(cell!("A1")),
        ],
    },
    Case {
        name: "axis: AutoFit commands remain reachable at phone width",
        actions: &[Action::Window(390, 844)],
        checks: &[
            Check::Reachable("autofit-columns"),
            Check::Reachable("autofit-rows"),
            Check::Fits,
        ],
    },
    Case {
        name: "shift: a rectangular selection offers explicit insertion choices and Escape keeps cells",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "2\u{E007}"),
            Action::Into("[data-onecalc=formula]", "3\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=SUM(A1:A3)\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("insert-cells"),
            Action::Wait(Check::Attr("structure-dialog", "open", "")),
        ],
        checks: &[
            Check::Text("shift-down", "Shift cells down"),
            Check::Text("shift-right", "Shift cells right"),
            Check::Reachable("shift-rows"),
            Check::Role("structure-dialog", "dialog"),
        ],
    },
    Case {
        name: "shift: cancelling the insertion keeps the source and Undo depth",
        actions: &[Action::Keys("shift-down", ESCAPE)],
        checks: &[
            Check::Absent("[data-dnacalc=structure-dialog][open]"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("B1"), "6"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "shift: down insertion moves only the stripe and expands its range references",
        actions: &[
            Action::Keys("grid", concat!("\u{E009}", "\u{E008}", "=", "\u{E000}")),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::Absent(cell!("A2")),
            Check::Text(cell!("A3"), "2"),
            Check::Text(cell!("A4"), "3"),
            Check::Text(cell!("B1"), "6"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "shift: a shifted reference is authored source in the formula bar",
        actions: &[Action::Into("name-box", "B1\u{E007}")],
        checks: &[Check::Value("[data-onecalc=formula]", "=SUM(A1:A4)")],
    },
    Case {
        name: "shift: Undo restores the whole shift together",
        actions: &[
            Action::Click("undo"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
            Check::Absent(cell!("A4")),
            Check::Value("[data-onecalc=formula]", "=SUM(A1:A3)"),
        ],
    },
    Case {
        name: "shift: right insertion keeps the moved formula reading its original cells",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::Click("insert-cells"),
            Action::Click("shift-right"),
        ],
        checks: &[Check::Absent(cell!("B1")), Check::Text(cell!("C1"), "6")],
    },
    Case {
        name: "shift: left deletion closes the gap without deleting adjacent rows",
        actions: &[Action::Click("delete-cells"), Action::Click("shift-right")],
        checks: &[
            Check::Text(cell!("B1"), "6"),
            Check::Absent(cell!("C1")),
            Check::Text(cell!("A2"), "2"),
        ],
    },
    Case {
        name: "shift: deleting a referenced cell authors a worksheet reference error",
        actions: &[
            Action::Into("name-box", "C2\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=$A$2\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("delete-cells"),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::Text(cell!("A2"), "3"),
            Check::Text(cell!("C2"), "#REF!"),
            Check::PlanCell("C2", "provenance", "calculated"),
        ],
    },
    Case {
        name: "shift: saved cell shifts reopen with changed formulas",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Text(cell!("A2"), "3"),
            Check::Text(cell!("C2"), "#REF!"),
        ],
    },
    Case {
        name: "shift: unsupported crossing ranges refuse the command with the draft selection intact",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "=SUM(A1:B4)\u{E007}"),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("insert-cells"),
            Action::Click("shift-down"),
        ],
        checks: &[
            Check::TextContains("structure-dialog", "crossing ranges"),
            Check::Attr("structure-dialog", "open", ""),
            Check::Text(cell!("A2"), "3"),
        ],
    },
    Case {
        name: "sheets: add is available from the keyboard-reachable worksheet manager",
        actions: &[
            Action::Click("structure-cancel"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("sheet-new-name", "Inputs"),
            Action::Click("sheet-add"),
            Action::Wait(Check::Count("[data-dnacalc=sheet-tab]", 2)),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Value("name-box", "A1"),
            Check::Value("sheet-rename-name", "Inputs"),
        ],
    },
    Case {
        name: "sheets: new sheets accept engine formulas and keep prior sheet view",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("[data-onecalc=formula]", "=Sheet1!A1*10\u{E007}"),
            Action::Into("name-box", "D5\u{E007}"),
            Action::Keys("grid", concat!("\u{E009}", "\u{E00E}", "\u{E000}")),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Sheet1"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "sheets: keyboard sheet switching returns to each retained selection",
        actions: &[
            Action::Keys("grid", concat!("\u{E009}", "\u{E00F}", "\u{E000}")),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Value("name-box", "D5"),
        ],
    },
    Case {
        name: "sheets: invalid and duplicate names are refused without losing the typed name",
        actions: &[
            Action::Into("sheet-rename-name", "Sheet1"),
            Action::Click("sheet-rename"),
        ],
        checks: &[
            Check::TextContains("status", "already exists"),
            Check::Value("sheet-rename-name", "Sheet1"),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
        ],
    },
    Case {
        name: "sheets: rename quotes apostrophes and rewrites sheet references",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Sheet1]"),
            Action::Into("sheet-rename-name", "October's inputs"),
            Action::Click("sheet-rename"),
            Action::Wait(Check::Text(
                "[data-dnacalc=sheet-tab][aria-selected=true]",
                "October's inputs",
            )),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Inputs]"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: moving a worksheet changes order and preserves its source",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("sheet-position", "1"),
            Action::Click("sheet-move"),
        ],
        checks: &[
            Check::Text("[data-dnacalc=sheet-tab]:first-child", "Inputs"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: hide selects a visible neighbour and exposes the retained hidden sheet",
        actions: &[Action::Click("sheet-hide")],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 1),
            Check::TextContains("hidden-sheets", "Inputs"),
            Check::Text(
                "[data-dnacalc=sheet-tab][aria-selected=true]",
                "October's inputs",
            ),
        ],
    },
    Case {
        name: "sheets: the last visible sheet cannot be hidden or deleted",
        actions: &[Action::Click("sheet-hide")],
        checks: &[
            Check::TextContains("status", "at least one visible"),
            Check::Count("[data-dnacalc=sheet-tab]", 1),
        ],
    },
    Case {
        name: "sheets: unhide restores the saved sheet and its previous selection",
        actions: &[Action::Click("sheet-unhide")],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 2),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Inputs"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: delete asks for a reviewable decision and Escape cancels",
        actions: &[
            Action::Click("sheet-delete"),
            Action::Wait(Check::Attr("sheet-delete-dialog", "open", "")),
            Action::Keys("sheet-delete-cancel", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=sheet-delete-dialog][open]"),
            Check::Count("[data-dnacalc=sheet-tab]", 2),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "sheets: deleting a sheet rewrites readers and one Undo restores it",
        actions: &[
            Action::Click("sheet-delete"),
            Action::Click("sheet-delete-confirm"),
            Action::Wait(Check::Count("[data-dnacalc=sheet-tab]", 1)),
            Action::Click("undo"),
        ],
        checks: &[Check::Count("[data-dnacalc=sheet-tab]", 2)],
    },
    Case {
        name: "sheets: added data and the reordered hidden sheet survive save and reopen",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Inputs]"),
            Action::Click("sheet-hide"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Count("[data-dnacalc=sheet-tab]", 1),
            Check::Text("[data-dnacalc=sheet-tab]", "October's inputs"),
        ],
    },
    Case {
        name: "sheets: the hidden worksheet returns from the saved package with its calculated source",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Click("sheet-unhide"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "sheets: the full manager and long tab names remain reachable at phone width",
        actions: &[
            Action::Window(390, 844),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
        ],
        checks: &[
            Check::Reachable("sheet-add"),
            Check::Reachable("sheet-rename-name"),
            Check::Fits,
        ],
    },
    Case {
        name: "sheets: pending formula drafts block structural edits without losing text",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into("[data-onecalc=formula]", "42"),
            Action::Click("insert-cells"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "42"),
            Check::Absent("[data-dnacalc=structure-dialog][open]"),
            Check::Text(cell!("A1"), "10"),
            Check::TextContains("status", "Enter or cancel"),
        ],
    },
    Case {
        name: "sheets: structural refusal leaves the formula draft cancellable",
        actions: &[Action::Keys("[data-onecalc=formula]", ESCAPE)],
        checks: &[
            Check::Value("[data-onecalc=formula]", "='October''s inputs'!A1*10"),
            Check::Text(cell!("A1"), "10"),
        ],
    },
    Case {
        name: "review markers expose imported notes with accessible content",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Role(
                "[data-dnacalc=review-marker][data-cell=A1]",
                "button",
            )),
            Action::Click("[data-dnacalc=review-marker][data-cell=A1]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Role("review-dialog", "dialog"),
            Check::Reachable("review-dialog"),
        ],
    },
    Case {
        name: "hyperlink details expose a deliberate external link and its tooltip",
        actions: &[
            Action::Click("review-close"),
            Action::Into("name-box", "K1\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=K1]"),
        ],
        checks: &[
            Check::Count("review-link", 1),
            Check::Attr("[data-dnacalc=review-link] a", "rel", "noopener noreferrer"),
            Check::Attr("[data-dnacalc=review-link] a", "title", "a tip"),
        ],
    },
    Case {
        name: "physical row insertion moves notes and links without annotating the new row",
        actions: &[
            Action::Click("review-close"),
            Action::Into("name-box", "1:1\u{E007}"),
            Action::Click("insert-cells"),
            Action::Wait(Check::Absent("[data-dnacalc=review-marker][data-cell=A1]")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=A2]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Count("[data-dnacalc=review-marker][data-cell=K2]", 1),
        ],
    },
    Case {
        name: "review content and moved references survive save and reopen",
        actions: &[
            Action::Click("review-close"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::TextContains("status", "Download started:")),
            Action::UploadDownload("features.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Into("name-box", "A2\u{E007}"),
            Action::Click("[data-dnacalc=review-marker][data-cell=A2]"),
        ],
        checks: &[
            Check::TextContains("review-note", "a comment"),
            Check::Count("[data-dnacalc=review-marker][data-cell=K2]", 1),
        ],
    },
    Case {
        name: "review manager remains bounded and its note is reachable on a phone",
        actions: &[
            Action::Click("review-close"),
            Action::Window(390, 844),
            Action::Click("review-tools"),
            Action::Keys("review-close", "\u{E010}"),
        ],
        checks: &[
            Check::Reachable("review-dialog"),
            Check::Reachable("review-note"),
            Check::Contrast("review-note"),
            Check::Fits,
        ],
    },
    Case {
        name: "review Escape closes the phone manager and returns to the grid",
        actions: &[
            Action::Keys("review-close", ESCAPE),
            Action::Window(1440, 1000),
        ],
        checks: &[Check::Focused("grid")],
    },
    Case {
        name: "summary: mixed current values show six aggregates and explicit exclusions",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("status", "New workbook")),
            Action::ExternalClipboard(
                "1\t=SUM(A1:A2)\ttext\n2\t=1/0\tTRUE\n\t=WEBSERVICE(\"https://example.com\")\t=\"\"",
                false,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("B1"), "3")),
            Action::Into("name-box", "A1:C4\u{E007}"),
            Action::Click("selection-summary-toggle"),
        ],
        checks: &[
            Check::Text("summary-count", "7"),
            Check::Text("summary-numeric", "3"),
            Check::Text("summary-sum", "6"),
            Check::Text("summary-average", "2"),
            Check::Text("summary-min", "1"),
            Check::Text("summary-max", "3"),
            Check::TextContains(
                "summary-provenance",
                "4 blanks · 1 empty text values · 1 errors",
            ),
            Check::TextContains("summary-provenance", "1 refused"),
        ],
    },
    Case {
        name: "summary: stale results are counted apart after a manual edit",
        actions: &[
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "10\u{E007}"),
            Action::Wait(Check::Text(cell!("A1"), "10")),
            Action::Into("name-box", "A1:C4\u{E007}"),
        ],
        checks: &[
            Check::Text("summary-count", "6"),
            Check::Text("summary-numeric", "2"),
            Check::Text("summary-sum", "12"),
            Check::TextContains("summary-provenance", "1 stale · 1 refused"),
        ],
    },
    Case {
        name: "summary: downloaded manual caches never enter current numeric totals",
        actions: &[
            Action::Choose("calc-mode", "automatic"),
            Action::Click("calculate"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "manual-cache.xlsx")),
            Action::Into("name-box", "A1:C4\u{E007}"),
        ],
        checks: &[
            Check::Text("summary-count", "4"),
            Check::Text("summary-numeric", "2"),
            Check::Text("summary-sum", "12"),
            Check::TextContains("summary-provenance", "3 file caches"),
        ],
    },
    Case {
        name: "local menu: protected worksheets disclose why editing commands are disabled",
        actions: &[
            Action::ProtectedDownloadFixture,
            Action::UploadDownload("protected.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "protected.xlsx")),
            Action::Keys("grid", "\u{E008}\u{E03A}\u{E000}"),
            Action::Wait(Check::TextContains("context-reason", "protected")),
        ],
        checks: &[
            Check::Attr("context-cut", "disabled", ""),
            Check::Attr("context-clear", "disabled", ""),
            Check::Focused("context-copy"),
        ],
    },
    Case {
        name: "local menu: disabled commands are skipped by keyboard navigation",
        actions: &[
            Action::Keys("context-copy", "\u{E015}"),
            Action::Wait(Check::Focused("context-find")),
            Action::Keys("context-find", ESCAPE),
        ],
        checks: &[
            Check::Focused("grid"),
            Check::Absent("[data-dnacalc=context-menu][open]"),
        ],
    },
    Case {
        name: "summary: whole-column requests explain their bounded cost without expanding the grid",
        actions: &[Action::Into("name-box", "A:A\u{E007}")],
        checks: &[
            Check::TextContains("selection-summary-toggle", "limited to 10000 cells"),
            Check::Absent("[data-dnacalc=summary-sum]"),
        ],
    },
    Case {
        name: "find: Ctrl F opens a focused workbook dialog and searches authored formulas in the original range",
        actions: &[
            Action::Click("selection-summary-toggle"),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "A1:C3\u{E007}"),
            Action::Keys("grid", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Choose("find-scope", "selection"),
            Action::Into("find-query", "SUM("),
            Action::Keys("find-query", ENTER),
        ],
        checks: &[
            Check::Role("find-dialog", "dialog"),
            Check::Count("find-hit", 2),
            Check::TextContains("find-results", "Data!B2"),
            Check::TextContains("find-results", "Data!C2"),
            Check::Reachable("find-dialog"),
        ],
    },
    Case {
        name: "find: displayed values and hit navigation retain the captured selection scope",
        actions: &[
            Action::Choose("find-look", "values"),
            Action::Into("find-query", "6"),
            Action::Click("find-whole"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 3)),
            Action::Click("[data-dnacalc=find-results] li:last-child [data-dnacalc=find-hit]"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Count("find-hit", 3),
            Check::Value("name-box", "C3"),
            Check::TextContains("find-results", "calculated"),
        ],
    },
    Case {
        name: "find: a completed search explains no matches",
        actions: &[
            Action::Into("find-query", "not present anywhere"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Text("find-message", "No matches"),
            Check::Count("find-hit", 0),
        ],
    },
    Case {
        name: "find: workbook scope can navigate to another worksheet",
        actions: &[
            Action::Choose("find-scope", "workbook"),
            Action::Into("find-query", "100"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Click("find-hit"),
        ],
        checks: &[
            Check::TextContains("find-results", "Other!A1"),
            Check::Value("name-box", "A1"),
            Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]", "Other"),
        ],
    },
    Case {
        name: "find: Escape returns focus and Ctrl G opens the existing reference resolver",
        actions: &[
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Focused("grid")),
            Action::Keys("grid", "\u{E009}g\u{E000}"),
            Action::Wait(Check::Focused("goto-target")),
            Action::Into("goto-target", "Data!A1:C3\u{E007}"),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=find-dialog][open]"),
            Check::Value("name-box", "A1"),
            Check::Attr("selection", "data-area", "A1:C3"),
        ],
    },
    Case {
        name: "replace: Ctrl H replaces matching formulas as one workbook command",
        actions: &[
            Action::Keys("grid", "\u{E009}h\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Choose("find-scope", "selection"),
            Action::Choose("find-look", "formulas"),
            Action::Click("find-whole"),
            Action::Into("find-query", "SUM("),
            Action::Into("replace-text", "AVERAGE("),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 2)),
            Action::Click("replace-all"),
            Action::Wait(Check::Text("find-message", "No matches")),
            Action::Click("find-close"),
        ],
        checks: &[
            Check::Text(cell!("B2"), "2"),
            Check::Text(cell!("C2"), "2"),
            Check::Text(cell!("D3"), "6"),
        ],
    },
    Case {
        name: "replace: one Undo restores every replacement and Redo restores the formulas",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Text(cell!("B2"), "6")),
            Action::Wait(Check::Text(cell!("C2"), "6")),
            Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("B2"), "2"), Check::Text(cell!("C2"), "2")],
    },
    Case {
        name: "replace: edited formulas survive the application's saved package route",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: cut-base.xlsx.")),
            Action::UploadDownload("cut-base.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::Into("name-box", "B2\u{E007}"),
        ],
        checks: &[
            Check::Value("[data-onecalc=formula]", "=AVERAGE(A1:A3)"),
            Check::Text(cell!("B2"), "2"),
            Check::Text(cell!("C2"), "2"),
        ],
    },
    Case {
        name: "local menu: Shift F10 exposes named commands with keyboard focus",
        actions: &[
            Action::Into("name-box", "A1:A3\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E03A}\u{E000}"),
        ],
        checks: &[
            Check::Role("context-menu", "menu"),
            Check::Focused("context-copy"),
            Check::Role("context-clear", "menuitem"),
            Check::Reachable("context-menu"),
        ],
    },
    Case {
        name: "local menu: arrows move between enabled commands and Escape restores the grid",
        actions: &[
            Action::Wait(Check::Text("context-reason", "")),
            Action::Keys("context-copy", "\u{E015}"),
            Action::Wait(Check::Focused("context-cut")),
            Action::Keys("context-cut", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=context-menu][open]"),
            Check::Focused("grid"),
            Check::Attr("selection", "data-area", "A1:A3"),
        ],
    },
    Case {
        name: "local menu: a pointer menu inside a selected range keeps that range",
        actions: &[Action::ContextClick(cell!("A2"), false)],
        checks: &[
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Focused("context-copy"),
            Check::Value("name-box", "A1"),
        ],
    },
    Case {
        name: "local menu: Clear uses the existing selected-range command and one Undo",
        actions: &[
            Action::Click("context-clear"),
            Action::Wait(Check::Absent(cell!("A1"))),
            Action::Wait(Check::Absent(cell!("A2"))),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "2"),
            Check::Text(cell!("A3"), "3"),
        ],
    },
    Case {
        name: "local menu: right click outside the range makes the pointed cell active",
        actions: &[Action::ContextClick(cell!("B1"), false)],
        checks: &[
            Check::Value("name-box", "B1"),
            Check::TextContains("context-menu", "Data!B1"),
        ],
    },
    Case {
        name: "local menu: Shift right click preserves the browser command alternative",
        actions: &[
            Action::Keys("context-copy", ESCAPE),
            Action::ContextClick(cell!("B1"), true),
            Action::Keys("grid", ESCAPE),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=context-menu][open]"),
            Check::Text(cell!("B1"), "10"),
        ],
    },
    Case {
        name: "find: the dialog and complete results remain reachable at phone width",
        actions: &[
            Action::Window(390, 844),
            Action::Click("find-tools"),
            Action::Choose("find-scope", "sheet"),
            Action::Choose("find-look", "formulas"),
            Action::Into("find-query", "AVERAGE"),
            Action::Click("find-all"),
        ],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("find-dialog"),
            Check::Count("find-hit", 2),
        ],
    },
    Case {
        name: "find: phone Escape restores the workbook",
        actions: &[
            Action::Keys("find-query", ESCAPE),
            Action::Window(1280, 1000),
        ],
        checks: &[Check::Focused("grid"), Check::Fits],
    },
    Case {
        name: "atlas: zero formulas gives useful copy and explicitly unmeasured Excel agreement",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-atlas"),
        ],
        checks: &[
            Check::Starts("atlas-totals", "0 formula cells: 0 calculated"),
            Check::TextContains("atlas-empty", "choose a sheet"),
            Check::TextContains(
                "atlas-agreement",
                "Agreement with Excel has not been measured",
            ),
            Check::Absent("[data-dnacalc=atlas-diagnostics][open]"),
        ],
    },
    Case {
        name: "atlas: a keyboard sheet jump opens the shared Grid selection",
        actions: &[Action::Keys("atlas-sheet-jump", "\u{E007}")],
        checks: &[Check::Value("name-box", "A1"), Check::Focused("grid")],
    },
    Case {
        name: "atlas: all 33 actual refusal reasons are counted, with an explicit first page",
        actions: &[
            Action::ExternalClipboard(
                "=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Lost!A1\n=Missing00!A1\n=Missing01!A1\n=Missing02!A1\n=Missing03!A1\n=Missing04!A1\n=Missing05!A1\n=Missing06!A1\n=Missing07!A1\n=Missing08!A1\n=Missing09!A1\n=Missing10!A1\n=Missing11!A1\n=Missing12!A1\n=Missing13!A1\n=Missing14!A1\n=Missing15!A1\n=Missing16!A1\n=Missing17!A1\n=Missing18!A1\n=Missing19!A1\n=Missing20!A1\n=Missing21!A1\n=Missing22!A1\n=Missing23!A1\n=Missing24!A1\n=Missing25!A1\n=Missing26!A1\n=Missing27!A1\n=Missing28!A1\n=Missing29!A1\n=Missing30!A1\n=Missing31!A1\n=1+1",
                false,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::Absent("[data-dnacalc=skin-atlas][disabled]")),
            Action::Click("skin-atlas"),
        ],
        checks: &[
            Check::Text(
                "atlas-totals",
                "93 formula cells: 1 calculated, 92 refused, 0 file cache, 0 stale, 0 pending; 1.08% calculated",
            ),
            Check::Count("atlas-reason", 30),
            Check::TextContains("atlas-reason-count", "Reasons 1–30 of 33"),
        ],
    },
    Case {
        name: "atlas: Enter opens an actual refusal group and its first 50 affected cells",
        actions: &[Action::Keys("atlas-reason", "\u{E007}")],
        checks: &[
            Check::Count("atlas-affected-cell", 50),
            Check::Text("atlas-affected-count", "60 affected cells · page 1 of 2"),
        ],
    },
    Case {
        name: "atlas: the second affected-cell page exposes its remaining ten cells",
        actions: &[Action::Click("atlas-cells-next")],
        checks: &[
            Check::Count("atlas-affected-cell", 10),
            Check::Text("atlas-affected-count", "60 affected cells · page 2 of 2"),
            Check::TextContains("atlas-affected-cell", "Sheet1!A51"),
        ],
    },
    Case {
        name: "atlas: an affected-cell link enters Grid at the actual refused source",
        actions: &[Action::Keys("atlas-open-grid", "\u{E007}")],
        checks: &[
            Check::Value("name-box", "A51"),
            Check::Value(BAR, "=Lost!A1"),
            Check::Text("result-state", "Refused"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "atlas: reasons after the first thirty remain keyboard reachable",
        actions: &[
            Action::Click("skin-atlas"),
            Action::Click("atlas-reasons-next"),
        ],
        checks: &[
            Check::Count("atlas-reason", 3),
            Check::TextContains("atlas-reason-count", "Reasons 31–33 of 33"),
        ],
    },
    Case {
        name: "atlas: counts and a later reason stay useful at phone width",
        actions: &[
            Action::Window(390, 844),
            Action::Keys("atlas-reason", "\u{E007}"),
        ],
        checks: &[
            Check::Fits,
            Check::Narrow(390),
            Check::Text("atlas-affected-count", "1 affected cell · page 1 of 1"),
            Check::Count("atlas-affected-cell", 1),
            Check::Reachable("atlas-totals"),
        ],
    },
    Case {
        name: "atlas: hidden refused sources are counted and explicitly unavailable until unhidden",
        actions: &[
            Action::Window(1280, 1000),
            Action::Click("skin-grid"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into("sheet-new-name", "Hidden recovery"),
            Action::Click("sheet-add"),
            Action::Into(BAR, "=NoSheet!A1\u{E007}"),
            Action::Click("sheet-hide"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Click("skin-atlas"),
            Action::Click("[data-dnacalc=atlas-reason][data-reason*=NoSheet]"),
        ],
        checks: &[
            Check::Count("atlas-sheet", 2),
            Check::Count("sheet-tab", 1),
            Check::TextContains("atlas-affected-cell", "hidden; unhide in Sheet tools"),
            Check::Attr("atlas-open-grid", "disabled", ""),
        ],
    },
    Case {
        name: "atlas: unhiding enables recovery to the same actual source",
        actions: &[
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Click("sheet-unhide"),
            Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Wait(Check::Absent("[data-dnacalc=atlas-open-grid][disabled]")),
            Action::Keys("atlas-open-grid", "\u{E007}"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=sheet-tab][aria-selected=true]",
                "Hidden recovery",
            ),
            Check::Value(BAR, "=NoSheet!A1"),
            Check::Text("result-state", "Refused"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "atlas: a manual workbook reports file-cache provenance and never calculated coverage",
        actions: &[
            Action::Window(1280, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into(BAR, "=2+3\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Click("skin-atlas"),
        ],
        checks: &[
            Check::Text(
                "atlas-totals",
                "1 formula cells: 0 calculated, 0 refused, 1 file cache, 0 stale, 0 pending; 0.00% calculated",
            ),
            Check::Text("atlas-mode", "Calculation: manual"),
        ],
    },
    Case {
        name: "atlas: editing that manual formula changes the census to stale until Calculate",
        actions: &[
            Action::Click("skin-grid"),
            Action::Into(BAR, "=3+4\u{E007}"),
            Action::Click("skin-atlas"),
        ],
        checks: &[Check::Text(
            "atlas-totals",
            "1 formula cells: 0 calculated, 0 refused, 0 file cache, 1 stale, 0 pending; 0.00% calculated",
        )],
    },
    Case {
        name: "atlas: Calculate publishes a result without claiming Excel agreement",
        actions: &[Action::Click("calculate")],
        checks: &[
            Check::Text(
                "atlas-totals",
                "1 formula cells: 1 calculated, 0 refused, 0 file cache, 0 stale, 0 pending; 100.00% calculated",
            ),
            Check::TextContains(
                "atlas-agreement",
                "Agreement with Excel has not been measured",
            ),
        ],
    },
    Case {
        name: "ledger: a mixed calculation/error/refusal list exposes primary fields and keyboard controls",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::ExternalClipboard(
                "=1/0\n=NoSheet!A1\n=3\n=4\n=5\n=6\n=7\n=8\n=9\n=10\n=11\n=12\n=13\n=14\n=15\n=16\n=17\n=18\n=19\n=20\n=21\n=22\n=23\n=24\n=25\n=26\n=27\n=28\n=29\n=30\n=31\n=32\n=33\n=34\n=35\n=36\n=37\n=38\n=39\n=40\n=41\n=42\n=43\n=44\n=45\n=46\n=47\n=48\n=49\n=50\n=51\n=52\n=53\n=54\n=55\n=56\n=57\n=58\n=59\n=60\n=61\n=62\n=63\n=64\n=65\n=66\n=67\n=68\n=69\n=70\n=71\n=72\n=73\n=74\n=75\n=76\n=77\n=78\n=79\n=80\n=81\n=82\n=83\n=84\n=85\n=86\n=87\n=88\n=89\n=90\n=91\n=92\n=93\n=94\n=95\n=96\n=97\n=98\n=99\n=100\n=101\n=102\n=103\n=104\n=105",
                false,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::Absent("[data-dnacalc=skin-ledger][disabled]")),
            Action::Click("skin-ledger"),
        ],
        checks: &[
            Check::Starts("ledger-count", "105 of 105 formula cells; page 1 of 2"),
            Check::Text(
                "ledger-population",
                "105 formulas in this workbook · 105 in scope · 105 match the active filters",
            ),
            Check::Name("ledger-filter", "Formula filter"),
            Check::Name("ledger-select", "Inspect Sheet1!A1"),
            Check::Text(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A1'] .dc-chip",
                "calculated error",
            ),
            Check::TextContains(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A1'] [data-dnacalc=ledger-reason]",
                "division by zero",
            ),
            Check::TextContains(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A2'] [data-dnacalc=ledger-reason]",
                "NoSheet",
            ),
            Check::Absent("[data-dnacalc=ledger-details][open]"),
        ],
    },
    Case {
        name: "ledger: full details on a different row keep their disclosure and keyboard focus",
        actions: &[Action::Keys(
            "[data-dnacalc=ledger-row][data-cell='Sheet1!A2'] summary",
            "\u{E007}",
        )],
        checks: &[
            Check::Attr(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A2'] [data-dnacalc=ledger-details]",
                "open",
                "",
            ),
            Check::Value("name-box", "A1"),
            Check::Value(BAR, "=1/0"),
            Check::Focused("[data-dnacalc=ledger-row][data-cell='Sheet1!A2'] summary"),
        ],
    },
    Case {
        name: "ledger: inspecting the already-active row still permits the next page",
        actions: &[
            Action::Keys("ledger-select", "\u{E007}"),
            Action::Wait(Check::Absent("[data-dnacalc=ledger-next][disabled]")),
            Action::Keys("ledger-next", "\u{E007}"),
            Action::Wait(Check::Count("ledger-row", 5)),
            Action::Keys("ledger-previous", "\u{E007}"),
        ],
        checks: &[
            Check::Count("ledger-row", 100),
            Check::Starts("ledger-count", "105 of 105 formula cells; page 1 of 2"),
        ],
    },
    Case {
        name: "ledger: keyboard paging reaches every remaining formula and selects its actual source",
        actions: &[
            Action::Keys("ledger-next", "\u{E007}"),
            Action::Wait(Check::Count("ledger-row", 5)),
            Action::Keys("ledger-select", "\u{E007}"),
        ],
        checks: &[
            Check::Starts("ledger-count", "105 of 105 formula cells; page 2 of 2"),
            Check::Value(BAR, "=101"),
            Check::Attr(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A101']",
                "data-active",
                "true",
            ),
        ],
    },
    Case {
        name: "ledger: source recovery opens Grid at the current row",
        actions: &[Action::Keys("ledger-open-grid", "\u{E007}")],
        checks: &[
            Check::Value("name-box", "A101"),
            Check::Value(BAR, "=101"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "ledger: returning preserves the page and reveals the active row",
        actions: &[Action::Click("skin-ledger")],
        checks: &[
            Check::Starts("ledger-count", "105 of 105 formula cells; page 2 of 2"),
            Check::Count("ledger-row", 5),
            Check::Focused(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A101'] [data-dnacalc=ledger-select]",
            ),
        ],
    },
    Case {
        name: "ledger: name-box navigation reveals an active row on another page",
        actions: &[Action::Into("name-box", "A1\u{E007}")],
        checks: &[
            Check::Starts("ledger-count", "105 of 105 formula cells; page 1 of 2"),
            Check::Focused(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A1'] [data-dnacalc=ledger-select]",
            ),
        ],
    },
    Case {
        name: "ledger: a refused row keeps its reason and opens the same source in Canvas",
        actions: &[
            Action::Choose("ledger-filter", "refused"),
            Action::Wait(Check::Count("ledger-row", 1)),
            Action::Keys("ledger-open-canvas", "\u{E007}"),
        ],
        checks: &[
            Check::Text("canvas-cell", "Sheet1!A2"),
            Check::Text("result-state", "Refused"),
            Check::Focused("canvas"),
        ],
    },
    Case {
        name: "ledger: refusal filters persist after Canvas inspection",
        actions: &[Action::Click("skin-ledger")],
        checks: &[
            Check::Value("ledger-filter", "refused"),
            Check::Starts("ledger-count", "1 of 105 formula cells"),
            Check::TextContains("ledger-reason", "NoSheet"),
        ],
    },
    Case {
        name: "ledger: scope, text, sort and grouping expose active-filter and workbook populations",
        actions: &[
            Action::Choose("ledger-filter", "text"),
            Action::Into("ledger-text", "=10"),
            Action::Choose("ledger-sort", "address-desc"),
            Action::Choose("ledger-scope", "sheet"),
            Action::Click("ledger-group-toggle"),
        ],
        checks: &[
            Check::Starts("ledger-count", "7 of 105 formula cells"),
            Check::TextContains("ledger-population", "105 in scope · 7 match"),
            Check::Count("ledger-group", 1),
            Check::Attr("ledger-row", "data-cell", "Sheet1!A105"),
        ],
    },
    Case {
        name: "ledger: all audit controls survive a source round trip",
        actions: &[
            Action::Click("ledger-open-grid"),
            Action::Click("skin-ledger"),
        ],
        checks: &[
            Check::Value("ledger-filter", "text"),
            Check::Value("ledger-text", "=10"),
            Check::Value("ledger-sort", "address-desc"),
            Check::Value("ledger-scope", "sheet"),
            Check::Count("ledger-group", 1),
            Check::Attr(
                "[data-dnacalc=ledger-row][data-cell='Sheet1!A105']",
                "data-active",
                "true",
            ),
        ],
    },
    Case {
        name: "ledger: phone scrolling retains an address and visible population controls",
        actions: &[
            Action::Click("ledger-clear-filter"),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Fits,
            Check::Narrow(390),
            Check::Role("ledger-table", "region"),
            Check::TextContains("ledger-population", "105 match the active filters"),
            Check::Name("ledger-open-grid", "Open Sheet1!A105 in Grid"),
        ],
    },
    Case {
        name: "ledger: cached manual values have an explicit provenance reason",
        actions: &[
            Action::Window(1280, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into(BAR, "=2+3\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ManualDownloadFixture,
            Action::UploadDownload("manual-cache.xlsx"),
            Action::DiscardReplacement,
            Action::Click("skin-ledger"),
            Action::Choose("ledger-filter", "file-cache"),
        ],
        checks: &[
            Check::Text("ledger-value", "5"),
            Check::TextContains("ledger-reason", "not calculated by DNA Calc"),
            Check::Attr("ledger-row", "data-provenance", "file-cache"),
        ],
    },
    Case {
        name: "ledger: stale recovery exposes the old value and recalculation reason",
        actions: &[
            Action::Click("ledger-open-grid"),
            Action::Into(BAR, "=3+4\u{E007}"),
            Action::Click("skin-ledger"),
            Action::Choose("ledger-filter", "stale"),
        ],
        checks: &[
            Check::Text("ledger-value", "5"),
            Check::TextContains("ledger-reason", "awaiting recalculation"),
            Check::Attr("ledger-row", "data-provenance", "stale"),
        ],
    },
    Case {
        name: "canvas: actual dependency chips are labelled keyboard buttons with one literal value",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "B1\u{E007}"),
            Action::Wait(Check::Absent("[data-dnacalc=skin-canvas][disabled]")),
            Action::Click("skin-canvas"),
            Action::Wait(Check::Text("canvas-cell", "Data!B1")),
            Action::Keys("[data-dnacalc=canvas-details] summary", "\u{E007}"),
            Action::Wait(Check::Reachable("canvas-reason")),
            Action::Wait(Check::TextContains("canvas-reason", "calculated")),
            Action::Keys("[data-dnacalc=canvas-details] summary", "\u{E007}"),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=canvas-details][open]"),
            Check::TextContains("canvas-reason", "calculated"),
            Check::Role("[data-dnacalc=canvas-node][data-node=A1]", "button"),
            Check::Name(
                "[data-dnacalc=canvas-node][data-node=A1]",
                "Inspect Data!A1",
            ),
            Check::Attr(
                "[data-dnacalc=canvas-node][data-node=A1]",
                "aria-description",
                "1; literal",
            ),
            Check::Absent("[data-dnacalc=canvas-node][data-node=A1] [data-dnacalc=canvas-formula]"),
            Check::Text(
                "[data-dnacalc=canvas-node][data-node=A1] [data-dnacalc=canvas-value]",
                "1",
            ),
            Check::Text(
                "[data-dnacalc=canvas-node][data-node=B1] [data-dnacalc=canvas-state]",
                "calculated",
            ),
            Check::Attr(
                "[data-dnacalc=canvas-node][data-node=B1]",
                "data-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "canvas: Enter follows a cross-sheet chip and restores focus to the selected node",
        actions: &[Action::Keys(
            "[data-dnacalc=canvas-node][data-node='Other!B3']",
            "\u{E007}",
        )],
        checks: &[
            Check::Text("canvas-cell", "Other!B3"),
            Check::Value(BAR, "=Data!B1*2"),
            Check::Focused("[data-dnacalc=canvas-node][data-center=true]"),
        ],
    },
    Case {
        name: "canvas: a range can be inspected by keyboard without changing the active formula",
        actions: &[
            Action::Into("name-box", "Data!B2\u{E007}"),
            Action::Wait(Check::Text("canvas-cell", "Data!B2")),
            Action::Keys("[data-dnacalc=canvas-node][data-kind=area]", "\u{E007}"),
        ],
        checks: &[
            Check::TextContains("canvas-range", "Data!A1:A3"),
            Check::TextContains("canvas-range", "3 cells"),
            Check::Value(BAR, "=SUM(A1:A3)"),
            Check::Focused("[data-dnacalc=canvas-node][data-kind=area]"),
            Check::Attr(
                "[data-dnacalc=canvas-node][data-kind=area]",
                "data-selected",
                "true",
            ),
        ],
    },
    Case {
        name: "canvas: Show source selects the actual range in Grid",
        actions: &[Action::Keys("canvas-show-source", "\u{E007}")],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Value(BAR, "1"),
            Check::Attr("selection", "data-area", "A1:A3"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "canvas: depth, zoom, pan and reset controls remain keyboard reachable",
        actions: &[
            Action::Click("skin-canvas"),
            Action::Keys("canvas-zoom-in", "\u{E007}"),
            Action::Keys("canvas-pan-right", "\u{E007}"),
            Action::Keys("canvas-back-more", "\u{E007}"),
        ],
        checks: &[
            Check::Text("canvas-zoom", "110%"),
            Check::Text("canvas-depth", "2 back, 1 forward"),
            Check::Name("canvas-center", "Center active cell"),
            Check::Role("canvas-viewport", "region"),
        ],
    },
    Case {
        name: "canvas: Reset restores scale and depth and the inspector fits a phone",
        actions: &[
            Action::Keys("canvas-reset", "\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Wait(Check::Text("canvas-cell", "Data!B1")),
            Action::Window(390, 844),
            Action::Keys("canvas-center", "\u{E007}"),
        ],
        checks: &[
            Check::Text("canvas-zoom", "100%"),
            Check::Text("canvas-depth", "1 back, 1 forward"),
            Check::Fits,
            Check::Narrow(390),
            Check::Reachable("canvas-show-source"),
            Check::Reachable("canvas-center"),
            Check::Reachable("[data-dnacalc=canvas-node][data-center=true]"),
            Check::Contained(
                "[data-dnacalc=canvas-node][data-center=true]",
                "[data-dnacalc=canvas] .dc-stage",
            ),
        ],
    },
    Case {
        name: "canvas: a literal without dependencies remains inspectable and editable through the shared formula field",
        actions: &[
            Action::Window(1280, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=skin-grid][disabled]")),
            Action::Into(BAR, "42\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Count("canvas-node", 1),
            Check::Text("canvas-inspector-value", "42"),
            Check::Absent("canvas-formula"),
            Check::Text("canvas-reads", "nothing"),
            Check::Text("canvas-read-by", "nothing"),
        ],
    },
    Case {
        name: "canvas: cycles are finite in the actual worker trace",
        actions: &[
            Action::Into(BAR, "=B1\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Count("canvas-node", 2),
            Check::Text("canvas-cell", "Sheet1!A1"),
            Check::Absent("canvas-hidden"),
        ],
    },
    Case {
        name: "canvas: the blocked spill is an actual worksheet error with an explanation",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=skin-grid][disabled]")),
            Action::Into("name-box", "D2\u{E007}"),
            Action::Into(BAR, "99\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(3,2)\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
        ],
        checks: &[
            Check::Text("canvas-inspector-value", "#SPILL!"),
            Check::TextContains("canvas-reason", "cannot spill"),
            Check::Text("result-state", "Worksheet error"),
            Check::Value(BAR, "=_xlfn.SEQUENCE(3,2)"),
        ],
    },
    Case {
        name: "canvas: omitted actual readers are counted with their direction and depth",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=skin-grid][disabled]")),
            Action::Click("skin-grid"),
            Action::ExternalClipboard(
                "1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1\t=A1",
                false,
            ),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
            Action::Wait(Check::Absent("[data-dnacalc=skin-canvas][disabled]")),
            Action::Click("skin-canvas"),
        ],
        checks: &[
            Check::Count("canvas-node", 13),
            Check::Text("canvas-hidden", "8 additional readers at depth 1 not shown"),
            Check::Text("canvas-cell", "Sheet1!A1"),
        ],
    },
    Case {
        name: "tree: source search and reveal reach a distant cell without projecting its prefix",
        actions: &[
            Action::Window(1440, 1000),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("skin-tree"),
            Action::Into("name-box", "AZ10000\u{E007}"),
            Action::Into(BAR, "distant source\u{E007}"),
            Action::Into("tree-search", "distant source"),
        ],
        checks: &[
            Check::Count("[data-dnacalc=tree-row][data-kind=cell]", 1),
            Check::Text("tree-input", "distant source"),
            Check::ActiveDescendant("tree"),
            Check::Bounded("tree-row", 12),
        ],
    },
    Case {
        name: "tree: reveal active clears search and keeps the distant source in its window",
        actions: &[
            Action::Into("name-box", "AZ10000\u{E007}"),
            Action::Click("tree-reveal"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=tree-row][data-cell=AZ10000]",
                "aria-level",
                "4",
            ),
            Check::Value("tree-search", ""),
            Check::Attr(
                "[data-dnacalc=tree-row][data-focused=true]",
                "data-cell",
                "AZ10000",
            ),
            Check::Reachable("[data-dnacalc=tree-row][data-cell=AZ10000]"),
            Check::Bounded("tree-row", 80),
        ],
    },
    Case {
        name: "tree: expanded Right enters the first child",
        actions: &[Action::Keys("tree", "\u{E011}\u{E014}")],
        checks: &[Check::Attr(
            "[data-dnacalc=tree-row][data-focused=true]",
            "data-key",
            "names",
        )],
    },
    Case {
        name: "tree: Left moves a folded branch to its parent",
        actions: &[Action::Keys("tree", "\u{E012}")],
        checks: &[Check::Attr(
            "[data-dnacalc=tree-row][data-focused=true]",
            "data-key",
            "workbook",
        )],
    },
    Case {
        name: "tree: folds and focus survive a skin return",
        actions: &[
            Action::Click(
                "[data-dnacalc=tree-row][data-key='sheet:Data'] [data-dnacalc=tree-toggle]",
            ),
            Action::Click("skin-canvas"),
            Action::Click("skin-tree"),
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=tree-row][data-key='sheet:Data']",
                "data-folded",
                "true",
            ),
            Check::Attr(
                "[data-dnacalc=tree-row][data-focused=true]",
                "data-key",
                "sheet:Data",
            ),
            Check::Absent("[data-dnacalc=tree-row][data-cell=AZ10000]"),
        ],
    },
    Case {
        name: "tree: a published array exposes actual spill children and narrow source columns",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=skin-tree][disabled]")),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(3,2)\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=tree-row][data-cell=D3] [data-dnacalc=tree-value]",
                "6",
            ),
            Check::Text(
                "[data-dnacalc=tree-row][data-cell=D3] [data-dnacalc=tree-input]",
                "Child value",
            ),
            Check::Fits,
            Check::Narrow(390),
            Check::Reachable("tree-search"),
            Check::Reachable("tree-reveal"),
        ],
    },
    Case {
        name: "tree: exact refusal and worksheet error details stay attached to real sources",
        actions: &[
            Action::Window(1440, 1000),
            Action::Into("name-box", "E20\u{E007}"),
            Action::Into(BAR, "=NoSheet!A1\u{E007}"),
            Action::Into("name-box", "E20\u{E007}"),
            Action::Click("tree-details-toggle"),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Contained("[data-dnacalc=tree-row][data-cell=E20]", "tree"),
            Check::Reachable("tree-open-grid"),
            Check::Contained("tree-open-grid", "tree-details"),
            Check::Attr(
                "[data-dnacalc=tree-row][data-cell=E20]",
                "data-provenance",
                "refused",
            ),
            Check::TextContains("tree-details", "NoSheet"),
            Check::Text("result-state", "Refused"),
        ],
    },
    Case {
        name: "tree: the shared editor updates the selected source",
        actions: &[
            Action::Into(BAR, "=1/0\u{E007}"),
            Action::Into("name-box", "E20\u{E007}"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=tree-row][data-cell=E20] [data-dnacalc=tree-value]",
                "#DIV/0!",
            ),
            Check::TextContains("tree-details", "zero"),
        ],
    },
    Case {
        name: "tree: one workbook Undo restores the refusal source and Redo restores the error",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Value(BAR, "=NoSheet!A1")),
            Action::Wait(Check::Text("result-state", "Refused")),
            Action::Click("redo"),
            Action::Wait(Check::Value(BAR, "=1/0")),
        ],
        checks: &[Check::Text(
            "[data-dnacalc=tree-row][data-cell=E20] [data-dnacalc=tree-value]",
            "#DIV/0!",
        )],
    },
    Case {
        name: "tree: Grid source opens through the shared workbook selection",
        actions: &[Action::Click("tree-open-grid")],
        checks: &[
            Check::Value("name-box", "E20"),
            Check::Value(BAR, "=1/0"),
            Check::Text(cell!("E20"), "#DIV/0!"),
        ],
    },
    Case {
        name: "panes: documented split positions import four real worksheet views",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::Into(BAR, "top\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "corner\u{E007}"),
            Action::Into("name-box", "A100\u{E007}"),
            Action::Into(BAR, "bottom\u{E007}"),
            Action::Into("name-box", "D100\u{E007}"),
            Action::Into(BAR, "bottom-right\u{E007}"),
            Action::Into("name-box", "C5\u{E007}"),
            Action::Into(BAR, "fixed\u{E007}"),
            Action::Into("name-box", "H100\u{E007}"),
            Action::Into(BAR, "scroll\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ViewDownloadFixture(
                "<sheetViews><sheetView workbookViewId=\"0\" topLeftCell=\"A1\"><pane xSplit=\"2400.5\" ySplit=\"2400.5\" topLeftCell=\"D100\" activePane=\"bottomRight\" state=\"split\"/><selection pane=\"bottomRight\" activeCell=\"D100\" sqref=\"D100\"/></sheetView></sheetViews>",
            ),
            Action::UploadDownload("view.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Count("grid", 4)),
        ],
        checks: &[
            Check::Text("[data-pane=\"0\"] [data-cell=A1]", "top"),
            Check::Text("[data-pane=\"1\"] [data-cell=D1]", "corner"),
            Check::Text("[data-pane=\"2\"] [data-cell=A100]", "bottom"),
            Check::Text("[data-pane=\"3\"] [data-cell=D100]", "bottom-right"),
            Check::Attr("[data-pane=\"3\"]", "data-grid-active", "true"),
            Check::ActiveDescendant("[data-pane=\"3\"]"),
        ],
    },
    Case {
        name: "panes: navigation moves the active lower band and retains the upper band",
        actions: &[
            Action::Click("[data-pane=\"3\"] [data-cell=D100]"),
            Action::Into("name-box", "H500\u{E007}"),
        ],
        checks: &[
            Check::Text("[data-pane=\"0\"] [data-cell=A1]", "top"),
            Check::Text("[data-pane=\"1\"] [data-cell=D1]", "corner"),
            Check::Attr("[data-pane=\"3\"]", "data-grid-active", "true"),
            Check::ActiveDescendant("[data-pane=\"3\"]"),
            Check::Bounded("[role=gridcell]", 5000),
        ],
    },
    Case {
        name: "panes: skin return retains the selected split and its independent scroll",
        actions: &[
            Action::RememberView("[data-pane=\"3\"] [data-dnacalc=active-cell]"),
            Action::Click("skin-tree"),
            Action::Click("skin-grid"),
        ],
        checks: &[
            Check::Count("grid", 4),
            Check::Attr("[data-pane=\"3\"]", "data-grid-active", "true"),
            Check::ActiveDescendant("[data-pane=\"3\"]"),
            Check::Text("[data-pane=\"0\"] [data-cell=A1]", "top"),
        ],
    },
    Case {
        name: "panes: scrolled active-cell freezing explains its current limit",
        actions: &[
            Action::OpenDetails("pane-tools"),
            Action::Click("freeze-active"),
        ],
        checks: &[
            Check::TextContains("status", "Return to the top-left"),
            Check::Count("grid", 4),
        ],
    },
    Case {
        name: "panes: freezing an active split leaves a working single pane and Undo restores splits",
        actions: &[
            Action::OpenDetails("pane-tools"),
            Action::Click("freeze-row"),
            Action::Wait(Check::Count("grid", 1)),
            Action::Wait(Check::Attr("grid", "data-grid-active", "true")),
            Action::Keys("grid", "\u{E015}"),
            Action::Click("undo"),
        ],
        checks: &[
            Check::Count("grid", 4),
            Check::Attr("[data-pane=\"0\"]", "data-grid-active", "true"),
        ],
    },
    Case {
        name: "panes: a non-leading frozen view imports both fixed and scrolling origins",
        actions: &[
            Action::ViewDownloadFixture(
                "<sheetViews><sheetView workbookViewId=\"0\" topLeftCell=\"C5\"><pane xSplit=\"2\" ySplit=\"2\" topLeftCell=\"H100\" activePane=\"bottomRight\" state=\"frozen\"/><selection pane=\"bottomRight\" activeCell=\"H100\" sqref=\"H100\"/></sheetView></sheetViews>",
            ),
            Action::UploadDownload("view.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Count("grid", 1)),
        ],
        checks: &[
            Check::Text(cell!("C5"), "fixed"),
            Check::Text(cell!("H100"), "scroll"),
            Check::ActiveDescendant("grid"),
            Check::Attr("grid", "data-frozen-rows", "6"),
            Check::Attr("grid", "data-frozen-columns", "4"),
        ],
    },
    Case {
        name: "panes: offset freeze survives saving and reopening without moving sources",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("view.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "view.xlsx")),
        ],
        checks: &[
            Check::Text(cell!("C5"), "fixed"),
            Check::Text(cell!("H100"), "scroll"),
            Check::Attr("grid", "data-frozen-rows", "6"),
        ],
    },
    Case {
        name: "panes: deleting through an offset frozen band refuses without clearing its source",
        actions: &[
            Action::Into("name-box", "5:5\u{E007}"),
            Action::Click("delete-cells"),
            Action::Wait(Check::TextContains("status", "Unfreeze this imported view")),
        ],
        checks: &[
            Check::Text(cell!("C5"), "fixed"),
            Check::Attr("grid", "data-frozen-rows", "6"),
        ],
    },
    Case {
        name: "panes: unfreezing an imported offset view preserves its cell sources",
        actions: &[
            Action::OpenDetails("pane-tools"),
            Action::Click("unfreeze"),
            Action::Into("name-box", "C5\u{E007}"),
        ],
        checks: &[
            Check::Attr("grid", "data-frozen-rows", "0"),
            Check::Attr("grid", "data-frozen-columns", "0"),
            Check::Text(cell!("C5"), "fixed"),
        ],
    },
    Case {
        name: "axis: unavailable explicit fonts refuse AutoFit without changing geometry",
        actions: &[
            Action::StyledFixture,
            Action::UploadDownload("styled.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "styled.xlsx")),
            Action::Into("name-box", "D30\u{E007}"),
            Action::OpenDetails("axis-tools"),
            Action::RememberView("[data-dnacalc=column-header][data-column=D]"),
            Action::Click("autofit-columns"),
        ],
        checks: &[
            Check::TextContains("status", "unavailable font"),
            Check::Style("[data-dnacalc=column-header][data-column=D]", "width:64px"),
        ],
    },
    Case {
        name: "axis: indentation and rotation use the displayed font in fitting",
        actions: &[
            Action::Into("name-box", "A30:B30\u{E007}"),
            Action::Click("autofit-columns"),
            Action::Wait(Check::TextContains("status", "Fitted contents")),
            Action::Click("autofit-rows"),
        ],
        checks: &[
            Check::TextContains("status", "Fitted contents"),
            Check::Text(cell!("A30"), "rotated wide text"),
            Check::Text(cell!("B30"), "indented text"),
            Check::Computed("[data-cell=A30] .dc-cell-text", "white-space:pre"),
            Check::TextBounds(cell!("A30")),
            Check::TextBounds(cell!("B30")),
        ],
    },
    Case {
        name: "axis: fitted styled dimensions and source formatting survive save and reopen",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("styled.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "styled.xlsx")),
            Action::Into("name-box", "A30:B30\u{E007}"),
        ],
        checks: &[
            Check::TextBounds(cell!("A30")),
            Check::TextBounds(cell!("B30")),
            Check::Computed("[data-cell=A30] .dc-cell-text", "white-space:pre"),
        ],
    },
    Case {
        name: "Notebook: a blank workbook offers visible cell-backed insert and link actions",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-notebook"),
        ],
        checks: &[
            Check::Count("notebook-empty", 1),
            Check::Reachable("notebook-link"),
            Check::Reachable("notebook-insert-tools"),
        ],
    },
    Case {
        name: "Notebook: prose is ordinary visible cell text, including a leading equals sign",
        actions: &[
            Action::Into("notebook-address", "A1"),
            Action::Choose("notebook-kind", "prose"),
            Action::OpenDetails("notebook-insert-tools"),
            Action::Into("notebook-content", "=visible prose"),
            Action::Click("notebook-insert"),
        ],
        checks: &[
            Check::Value(
                "[data-dnacalc=block][data-cell=A1] textarea",
                "'=visible prose",
            ),
            Check::Reachable("[data-dnacalc=block][data-cell=A1] textarea"),
            Check::Computed(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "display:none",
            ),
            Check::Attr(
                "[data-dnacalc=block][data-cell=A1]",
                "data-provenance",
                "literal",
            ),
        ],
    },
    Case {
        name: "Notebook: calculation insertion uses the workbook evaluator",
        actions: &[
            Action::Into("notebook-address", "A2"),
            Action::Choose("notebook-kind", "calculation"),
            Action::Into("notebook-content", "=3*7"),
            Action::Click("notebook-insert"),
            Action::Wait(Check::Focused(
                "[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-select]",
            )),
            Action::Window(1440, 1000),
            Action::Window(390, 844),
        ],
        checks: &[
            Check::Contained(
                "[data-dnacalc=block][data-selected=true] [data-dnacalc=block-select]",
                "[data-skin=notebook]",
            ),
            Check::Text(
                "[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-value]",
                "21",
            ),
            Check::Value(BAR, "=3*7"),
        ],
    },
    Case {
        name: "Notebook: reordering links leaves worksheet sources in place",
        actions: &[Action::Click(
            "[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-up]",
        )],
        checks: &[
            Check::Attr("[data-dnacalc=block]:first-child", "data-cell", "A2"),
            Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "=visible prose",
            ),
        ],
    },
    Case {
        name: "Notebook: saved links and prose reopen in their explicit order",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::Wait(Check::Starts("status", "Download started: Book1.xlsx.")),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
            Action::Click("skin-notebook"),
        ],
        checks: &[
            Check::Attr("[data-dnacalc=block]:first-child", "data-cell", "A2"),
            Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "=visible prose",
            ),
            Check::Text(
                "[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-value]",
                "21",
            ),
        ],
    },
    Case {
        name: "Notebook: linking existing cells preserves unfinished insertion content",
        actions: &[
            Action::OpenDetails("notebook-insert-tools"),
            Action::Into("notebook-content", "unfinished block"),
            Action::Into("notebook-address", "A1"),
            Action::Click("notebook-link"),
            Action::Click("skin-grid"),
            Action::Click("skin-notebook"),
            Action::OpenDetails("notebook-insert-tools"),
            Action::Click("save"),
        ],
        checks: &[
            Check::Value("notebook-content", "unfinished block"),
            Check::Starts("status", "Return to Notebook to insert or clear"),
        ],
    },
    Case {
        name: "Notebook: duplicate source links restore focus to the chosen placement",
        actions: &[
            Action::Into("notebook-content", ""),
            Action::Into("notebook-address", "A1"),
            Action::Click("notebook-link"),
            Action::Wait(Check::Count("[data-dnacalc=block]", 4)),
            Action::Click("[data-dnacalc=block]:last-child [data-dnacalc=block-select]"),
            Action::Wait(Check::Count("[data-dnacalc=block]:last-child textarea", 1)),
            Action::Into("[data-dnacalc=block]:last-child textarea", "draft"),
            Action::Keys("[data-dnacalc=block]:last-child textarea", ESCAPE),
        ],
        checks: &[
            Check::Focused("[data-dnacalc=block]:last-child [data-dnacalc=block-select]"),
            Check::Count("[data-dnacalc=block] textarea", 1),
            Check::Value(BAR, "'=visible prose"),
        ],
    },
    Case {
        name: "Notebook: recalculation preserves command focus after selecting a duplicate source",
        actions: &[
            Action::Click("calculate"),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-value]",
                "21",
            )),
        ],
        checks: &[
            Check::Focused("calculate"),
            Check::Count("[data-dnacalc=block] textarea", 1),
        ],
    },
    Case {
        name: "Notebook: source navigation shares selection and source with Grid",
        actions: &[Action::Click(
            "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-grid]",
        )],
        checks: &[
            Check::Value("name-box", "A1"),
            Check::Value(BAR, "'=visible prose"),
            Check::Text(cell!("A1"), "=visible prose"),
        ],
    },
    Case {
        name: "Notebook: a blocked array shows its actual worksheet blocker",
        actions: &[
            Action::Into("name-box", "D2\u{E007}"),
            Action::Into(BAR, "99\u{E007}"),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "=SEQUENCE(3,2)\u{E007}"),
            Action::Click("skin-notebook"),
            Action::Click("[data-dnacalc=block][data-cell=C1] [data-dnacalc=block-details]"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=block][data-cell=C1] [data-dnacalc=block-value]",
                "#SPILL!",
            ),
            Check::Text("block-spill-blocker", "Open blocker D2:D2"),
        ],
    },
    Case {
        name: "Notebook: open array details refresh when another cell releases the spill",
        actions: &[
            Action::Into("name-box", "D2\u{E007}"),
            Action::Into(BAR, "\u{E007}"),
        ],
        checks: &[
            Check::Absent("block-spill-blocker"),
            Check::Text(
                "[data-dnacalc=block][data-cell=D2] [data-dnacalc=block-value]",
                "4",
            ),
        ],
    },
    Case {
        name: "Notebook: open array details refresh when another cell blocks the spill again",
        actions: &[
            Action::Into("name-box", "D2\u{E007}"),
            Action::Into(BAR, "99\u{E007}"),
        ],
        checks: &[Check::Text("block-spill-blocker", "Open blocker D2:D2")],
    },
    Case {
        name: "Notebook: opening a blocker navigates to the actual source",
        actions: &[Action::Click("block-spill-blocker")],
        checks: &[Check::Value("name-box", "D2"), Check::Value(BAR, "99")],
    },
    Case {
        name: "Notebook: clearing the blocker shows canonical array values and a read-only child",
        actions: &[
            Action::Keys("grid", DELETE),
            Action::Click("skin-notebook"),
            Action::Click("[data-dnacalc=block][data-cell=D2] [data-dnacalc=block-select]"),
            Action::Click("[data-dnacalc=block][data-cell=D2] [data-dnacalc=block-details]"),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=block][data-cell=D2] [data-dnacalc=block-value]",
                "4",
            ),
            Check::Absent("[data-dnacalc=block][data-cell=D2] textarea"),
            Check::Count("block-owner", 1),
        ],
    },
    Case {
        name: "Notebook: a large authored sheet is paged and restores the viewed page",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1:A85\u{E007}"),
            Action::Into(BAR, "7\u{E009}\u{E007}\u{E000}"),
            Action::Click("skin-notebook"),
            Action::Click("notebook-next"),
            Action::Click("skin-grid"),
            Action::Click("skin-notebook"),
        ],
        checks: &[
            Check::Text("notebook-page", "85 blocks; page 2 of 3"),
            Check::Bounded("[data-dnacalc=block]", 40),
            Check::Attr("[data-dnacalc=block]:first-child", "data-cell", "A41"),
        ],
    },
    Case {
        name: "alignment: stored defaults, top and middle apply to text and explicit numeric alignment",
        actions: &[
            Action::AlignmentFixture,
            Action::UploadDownload("alignment.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::PlanCell("H1", "provenance", "calculated")),
        ],
        checks: &[
            Check::Aligned(cell!("B4"), "left", "bottom"),
            Check::Aligned(cell!("D4"), "left", "top"),
            Check::Aligned(cell!("F4"), "left", "center"),
            Check::Attr(cell!("B8"), "data-icon", "3Arrows:2"),
            Check::Attr(cell!("F8"), "data-icon", "3Arrows:0"),
            Check::Aligned(cell!("B8"), "left", "bottom"),
            Check::Aligned(cell!("D8"), "right", "bottom"),
            Check::Aligned(cell!("F8"), "center", "bottom"),
            Check::Aligned(cell!("H8"), "right", "bottom"),
        ],
    },
    Case {
        name: "alignment: text spills left, right or both ways while keeping its physical anchor",
        actions: &[],
        checks: &[
            Check::Spill(cell!("D1"), "B", "D"),
            Check::Aligned(cell!("D1"), "right", "bottom"),
            Check::Spill(cell!("D2"), "C", "D"),
            Check::Aligned(cell!("D2"), "center", "bottom"),
            Check::Spill(cell!("B3"), "B", "E"),
            Check::Aligned(cell!("B3"), "left", "bottom"),
            Check::Spill(cell!("E9"), "E", "E"),
        ],
    },
    Case {
        name: "alignment: fill repeats complete text, indentation follows its side and CELL reads the same facts",
        actions: &[],
        checks: &[
            Check::TextContains(cell!("B5"), "xyxyxy"),
            Check::FitsText(cell!("B5")),
            Check::Aligned(cell!("D5"), "left", "bottom"),
            Check::Aligned(cell!("F5"), "right", "bottom"),
            Check::Aligned(cell!("H5"), "spread", "bottom"),
            Check::Text(cell!("H1"), "\""),
            Check::Text(cell!("H2"), "^"),
            Check::Text(cell!("H3"), "'"),
            Check::Text(cell!("H4"), "\\"),
        ],
    },
    Case {
        name: "alignment: center across matching blank cells, justify and distributed use their cell width",
        actions: &[],
        checks: &[
            Check::Spill(cell!("F6"), "F", "H"),
            Check::Aligned(cell!("F6"), "across:H", "bottom"),
            Check::Computed(
                "[data-dnacalc=cell][data-cell=B6] .dc-cell-text",
                "text-align:justify;text-align-last:left",
            ),
            Check::Aligned(cell!("D6"), "spread", "bottom"),
            Check::Aligned(cell!("J6"), "spread", "bottom"),
            Check::Spill(cell!("B6"), "B", "B"),
            Check::Spill(cell!("D6"), "D", "D"),
        ],
    },
    Case {
        name: "alignment: vertical justify and distributed place actual wrapped lines throughout the row",
        actions: &[],
        checks: &[
            Check::Aligned(cell!("B7"), "left", "justify"),
            Check::Aligned(cell!("D7"), "left", "distributed"),
        ],
    },
    Case {
        name: "alignment: editing a neighbour releases and blocks overflow; saved alignment reopens",
        actions: &[
            Action::Into("name-box", "D9\u{E007}"),
            Action::Keys("grid", DELETE),
            Action::Wait(Check::Spill(cell!("E9"), "B", "E")),
            Action::Into(BAR, "stop\u{E007}"),
            Action::Wait(Check::Spill(cell!("E9"), "E", "E")),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("alignment.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Aligned(cell!("B4"), "left", "bottom"),
            Check::Aligned(cell!("D4"), "left", "top"),
            Check::Aligned(cell!("F6"), "across:H", "bottom"),
            Check::Spill(cell!("D2"), "C", "D"),
            Check::Text(cell!("D9"), "stop"),
            Check::Spill(cell!("E9"), "E", "E"),
            Check::Text(cell!("H1"), "\""),
        ],
    },
    Case {
        name: "model commands: grid shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-grid"),
            Action::Wait(Check::Attr("skin-grid", "aria-pressed", "true")),
            Action::Keys("grid", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("grid", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("grid", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("grid", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-grid", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model commands: notebook shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-notebook"),
            Action::Wait(Check::Attr("skin-notebook", "aria-pressed", "true")),
            Action::Keys("#dc-workspace", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("#dc-workspace", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("#dc-workspace", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("#dc-workspace", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-notebook", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model commands: atlas shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-atlas"),
            Action::Wait(Check::Attr("skin-atlas", "aria-pressed", "true")),
            Action::Keys("#dc-workspace", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("#dc-workspace", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("#dc-workspace", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("#dc-workspace", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-atlas", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model commands: tree shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-tree"),
            Action::Wait(Check::Attr("skin-tree", "aria-pressed", "true")),
            Action::Keys("tree", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("tree", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("tree", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("tree", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-tree", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model commands: canvas shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-canvas"),
            Action::Wait(Check::Attr("skin-canvas", "aria-pressed", "true")),
            Action::Keys("canvas", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("canvas", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("canvas", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("canvas", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-canvas", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model commands: ledger shares Undo, Redo, Find and F9",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Choose("calc-mode", "automatic"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Into(BAR, "=A1*3\u{E007}"),
            Action::Choose("calc-mode", "manual"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "4\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("skin-ledger"),
            Action::Wait(Check::Attr("skin-ledger", "aria-pressed", "true")),
            Action::Keys("#dc-workspace", "\u{E009}z\u{E000}"),
            Action::Wait(Check::Value(BAR, "2")),
            Action::Keys("#dc-workspace", "\u{E009}y\u{E000}"),
            Action::Wait(Check::Value(BAR, "4")),
            Action::Keys("#dc-workspace", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Into("find-query", "4"),
            Action::Click("find-all"),
            Action::Wait(Check::Count("find-hit", 1)),
            Action::Keys("find-all", "\u{E009}f\u{E000}"),
            Action::Wait(Check::Focused("find-query")),
            Action::Keys("find-query", ESCAPE),
            Action::Wait(Check::Absent("[data-dnacalc=find-dialog][open]")),
            Action::Keys("#dc-workspace", "\u{E039}"),
            Action::Into("name-box", "B1\u{E007}"),
        ],
        checks: &[
            Check::Text("selected-value", "12"),
            Check::Text("result-state", "Calculated"),
            Check::Value(BAR, "=A1*3"),
            Check::Attr("skin-ledger", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "model navigation: reveal the same active cell after scrolling",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Into(BAR, "42\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Text(cell!("A1"), "42")),
            Action::ScrollGrid(0, 1200),
            Action::Wait(Check::Absent(cell!("A1"))),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "42"),
            Check::Value("name-box", "A1"),
        ],
    },
    Case {
        name: "recalculation contract: a real worker services Ping between finite calculation slices",
        actions: &[Action::WorkerCalculation],
        checks: &[],
    },
    Case {
        name: "presentation: the live RenderPlan contains a calculated volatile formula",
        actions: &[
            Action::Keys("grid", "="),
            Action::Keys("cell-editor", "RAND()"),
            Action::Keys("cell-editor", ENTER),
        ],
        checks: &[
            Check::PlanCell("A1", "kind", "number"),
            Check::PlanCell("A1", "provenance", "calculated"),
            Check::PlanCell("A1", "formula", "true"),
            Check::PlanReady,
        ],
    },
    Case {
        name: "presentation: resizing rebuilds the plan without a calculation command",
        actions: &[Action::RememberPresentation, Action::Window(900, 700)],
        checks: &[Check::PresentationChanged("resize")],
    },
    Case {
        name: "presentation: zoom and a font completion rebuild only presentation",
        actions: &[
            Action::RememberPresentation,
            Action::Click("zoom-in"),
            Action::Wait(Check::PresentationChanged("zoom")),
            Action::RememberPresentation,
            Action::FontLoaded,
        ],
        checks: &[
            Check::PresentationChanged("font"),
            Check::PlanCell("A1", "provenance", "calculated"),
        ],
    },
];

/// The script that reads what one check looks at.
// Only the timing of actual Worker deliveries changes; the product and workbook path stay intact.
pub(crate) const CALC_LATENCY_CASES: &[Case] = &[
    Case {
        name: "worker latency: typing after navigation edits the destination",
        actions: &[
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(800),
            Action::Keys("grid", "1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Keys("grid", "2\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::Keys("grid", "\u{E015}9\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "9"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: consecutive entry preserves every key and destination",
        actions: &[
            Action::Keys("grid", "78\u{E007}56\u{E007}"),
            Action::Wait(Check::Value("name-box", "A5")),
        ],
        checks: &[
            Check::Text(cell!("A3"), "78"),
            Check::Text(cell!("A4"), "56"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: F2 immediately after navigation reads its destination source",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::Keys("grid", "\u{E015}\u{E032}"),
            Action::Wait(Check::Value("cell-editor", "9")),
        ],
        checks: &[
            Check::Value("cell-editor", "9"),
            Check::Value("name-box", "A2"),
        ],
    },
    Case {
        name: "worker latency: rapid Ctrl clicks retain every area despite earlier replies",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::Into("name-box", "B1\u{E007}"),
            Action::Wait(Check::Value("name-box", "B1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "10\u{E007}"),
            Action::Wait(Check::Value("name-box", "B2")),
            Action::Into("name-box", "C1\u{E007}"),
            Action::Wait(Check::Value("name-box", "C1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "20\u{E007}"),
            Action::Wait(Check::Value("name-box", "C2")),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Wait(Check::Value("name-box", "D1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "30\u{E007}"),
            Action::Wait(Check::Value("name-box", "D2")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(400),
            Action::CtrlClick(cell!("B1")),
            Action::PauseMillis(180),
            Action::CtrlClick(cell!("C1")),
            Action::PauseMillis(180),
            Action::CtrlClick(cell!("D1")),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Count("[data-dnacalc=selection]", 4),
            Check::Attr("#dc-grid-cell-0-1-2", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-0-1-3", "aria-selected", "true"),
            Check::Attr("#dc-grid-cell-0-1-4", "aria-selected", "true"),
        ],
    },
    Case {
        name: "worker latency: Ctrl Enter closes its draft and one Undo restores every area",
        actions: &[
            Action::Keys("grid", "9\u{E009}\u{E007}\u{E000}"),
            Action::Wait(Check::Absent("cell-editor")),
            Action::Wait(Check::Text(cell!("D1"), "9")),
            Action::Keys("grid", "\u{E009}z\u{E000}"),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("B1"), "10"),
            Check::Text(cell!("C1"), "20"),
            Check::Text(cell!("D1"), "30"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: a click after a queued formula seed points into its original destination",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "\u{E015}="),
            Action::Click(cell!("B1")),
            Action::Wait(Check::Value("cell-editor", "=B1")),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Keys("cell-editor", "\u{E007}"),
            Action::Wait(Check::Value("name-box", "A3")),
        ],
        checks: &[
            Check::Text(cell!("A1"), "1"),
            Check::Text(cell!("A2"), "10"),
            Check::Text(cell!("B1"), "10"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: Escape cancels a header drag while selection replies are pending",
        actions: &[
            Action::Into("name-box", "B1\u{E007}"),
            Action::ResizeHeader(
                "[data-dnacalc=column-header][data-column=B]",
                false,
                35,
                true,
            ),
            Action::PauseMillis(900),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=B]", "width:64px"),
            Check::Absent("resize-preview"),
        ],
    },
    Case {
        name: "worker latency: a pending clear protects a clean workbook from replacement",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::PauseMillis(800),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", DELETE),
            Action::Click("new"),
            Action::Wait(Check::Attr("replace-dialog", "open", "")),
            Action::Click("keep-working"),
            Action::PauseMillis(700),
        ],
        checks: &[
            Check::Text("document", "cut-base.xlsx (changed)"),
            Check::Absent(cell!("A1")),
            Check::Absent("[data-dnacalc=replace-dialog][open]"),
        ],
    },
    Case {
        name: "worker latency: unsupported queued text shortcuts retain the draft and diagnose retry",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(700),
            Action::Keys("grid", "\u{E015}9\u{E009}\u{E003}\u{E000}"),
            Action::Wait(Check::Value("cell-editor", "9")),
        ],
        checks: &[
            Check::Value("cell-editor", "9"),
            Check::Value("name-box", "A2"),
            Check::Starts("status", "Workbook reply delayed this editor shortcut"),
        ],
    },
    Case {
        name: "worker latency: native paste data survives pending selection replies",
        actions: &[
            Action::Keys("cell-editor", ESCAPE),
            Action::ExternalClipboard("42\t43", false),
            Action::Into("name-box", "M4\u{E007}"),
            Action::Keys("grid", "\u{E009}v\u{E000}"),
        ],
        checks: &[
            Check::Value("name-box", "M4"),
            Check::Text(cell!("M4"), "42"),
            Check::Text(cell!("N4"), "43"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "worker latency: a registered Notebook editor retains typing while selection replies arrive",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "cut-base.xlsx")),
            Action::PauseMillis(800),
            Action::Click("skin-notebook"),
            Action::Wait(Check::Count("[data-dnacalc=block][data-cell=A1]", 1)),
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-select]"),
            Action::Into(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "5\u{E007}",
            ),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "5",
            )),
            Action::Into(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "7\u{E007}",
            ),
            // Release the modifier at the WebDriver call boundary: Firefox 151 treats an
            // embedded NULL as Unidentified and otherwise keeps Control for the following text.
            Action::Keys(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "\u{E009}a\u{E000}",
            ),
            Action::Keys(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "\u{E003}9\u{E007}",
            ),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "9",
            )),
            Action::Into(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-input]",
                "5\u{E007}",
            ),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "5",
            )),
        ],
        checks: &[
            Check::Text(
                "[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]",
                "5",
            ),
            Check::Value("name-box", "A1"),
            Check::Attr("skin-notebook", "aria-pressed", "true"),
        ],
    },
    Case {
        name: "worker latency: registered formula bar preserves consecutive multi-character entries",
        actions: &[
            Action::Click("skin-grid"),
            Action::Into("name-box", "D7\u{E007}"),
            Action::Wait(Check::Value("name-box", "D7")),
            Action::PauseMillis(800),
            Action::Into(BAR, "12\u{E007}34\u{E007}"),
            Action::Wait(Check::Value("name-box", "D9")),
        ],
        checks: &[
            Check::Text(cell!("D7"), "12"),
            Check::Text(cell!("D8"), "34"),
            Check::Absent("cell-editor"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "worker latency: a delayed skin focus request preserves a newer native field focus",
        actions: &[
            Action::PauseMillis(800),
            Action::Click("skin-notebook"),
            Action::PauseMillis(800),
            Action::Click("skin-grid"),
            Action::Into("name-box", "M4"),
            Action::PauseMillis(900),
        ],
        checks: &[
            Check::Focused("name-box"),
            Check::Value("name-box", "M4"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "worker latency: drag selection and its next entry reach the same worker target",
        actions: &[
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::PauseMillis(800),
            Action::Drag(cell!("A1"), cell!("C3")),
            Action::Keys("grid", "17\u{E009}\u{E007}\u{E000}"),
            Action::Wait(Check::Text(cell!("C3"), "17")),
        ],
        checks: &[
            Check::Text(cell!("A1"), "17"),
            Check::Text(cell!("B2"), "17"),
            Check::Text(cell!("C3"), "17"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "worker latency: a Notebook insertion at a different address follows its selection receipt",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::PauseMillis(800),
            Action::Click("skin-notebook"),
            Action::Into("notebook-address", "B7"),
            Action::Choose("notebook-kind", "calculation"),
            Action::OpenDetails("notebook-insert-tools"),
            Action::Into("notebook-content", "=6*7"),
            Action::Click("notebook-insert"),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=B7] [data-dnacalc=block-value]",
                "42",
            )),
            Action::Click("undo"),
            Action::Wait(Check::Count("[data-dnacalc=block]", 0)),
            Action::Click("redo"),
            Action::Wait(Check::Text(
                "[data-dnacalc=block][data-cell=B7] [data-dnacalc=block-value]",
                "42",
            )),
        ],
        checks: &[
            Check::Count("[data-dnacalc=block]", 1),
            Check::Value("name-box", "B7"),
        ],
    },
    Case {
        name: "worker latency: an old AutoFit preflight cannot change the replacement workbook",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_09v/formats.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "formats.xlsx")),
            Action::PauseMillis(1000),
            Action::Click("skin-grid"),
            Action::PauseMillis(800),
            Action::OpenDetails("axis-tools"),
            Action::Click("autofit-columns"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "Book1.xlsx")),
            Action::PauseMillis(1200),
        ],
        checks: &[
            Check::Style("[data-dnacalc=column-header][data-column=A]", "width:64px"),
            Check::Value("name-box", "A1"),
            Check::Attr("undo", "disabled", ""),
        ],
    },
    Case {
        name: "worker latency: a completed entry preserves newer native field focus",
        actions: &[
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Value("name-box", "A1")),
            Action::PauseMillis(800),
            Action::Into(BAR, "17\u{E009}\u{E007}\u{E000}"),
            Action::Into("name-box", "M4"),
            Action::PauseMillis(1200),
        ],
        checks: &[
            Check::Focused("name-box"),
            Check::Value("name-box", "M4"),
            Check::Text(cell!("A1"), "17"),
            Check::Absent("cell-editor"),
        ],
    },
    Case {
        name: "worker latency: a Notebook projection preserves newer native field focus",
        actions: &[
            Action::Click("skin-notebook"),
            Action::PauseMillis(1000),
            Action::Click("block-select"),
            Action::Into("name-box", "Q12"),
            Action::PauseMillis(1200),
        ],
        checks: &[
            Check::Focused("name-box"),
            Check::Value("name-box", "Q12"),
            Check::Count("block", 1),
        ],
    },
    Case {
        name: "worker latency: Ledger page correction preserves newer native field focus",
        actions: &[
            Action::Click("skin-grid"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=replace-dialog][open]")),
            Action::PauseMillis(800),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Value("name-box", "A1")),
            Action::Into("name-box", "A1:A150\u{E007}"),
            Action::PauseMillis(800),
            Action::Into(BAR, "=1\u{E009}\u{E007}\u{E000}"),
            Action::PauseMillis(800),
            Action::Into("name-box", "A150\u{E007}"),
            Action::PauseMillis(800),
            Action::Click("skin-ledger"),
            Action::PauseMillis(100),
            Action::Into("name-box", "M4"),
            Action::PauseMillis(1500),
        ],
        checks: &[
            Check::Focused("name-box"),
            Check::Value("name-box", "M4"),
            Check::Count("ledger-row", 50),
        ],
    },
];

pub(crate) const CALC_WORKER_FAULT_CASES: &[Case] = &[
    Case {
        name: "worker transport: outstanding startup requests visibly say Pending",
        actions: &[],
        checks: &[
            Check::Starts("worker-status", "Pending"),
            Check::Text("result-state", "Pending"),
        ],
    },
    Case {
        name: "worker transport: a real Worker startup error is visible and releases pending requests",
        actions: &[
            Action::Wait(Check::Starts("worker-status", "Worker failed:")),
            Action::Window(1024, 768),
            Action::PauseMillis(350),
        ],
        checks: &[
            Check::Starts("worker-status", "Worker failed:"),
            Check::Starts("status", "New workbook failed:"),
            Check::Text("result-state", "Empty"),
            Check::Reachable("worker-status"),
        ],
    },
];

pub(crate) const CALC_GROUPS: &[CaseGroup] = &[
    CaseGroup {
        name: "basics",
        start: 0,
        end: 20,
        core: 6,
        setup: &[],
    },
    CaseGroup {
        name: "files",
        start: 20,
        end: 25,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "scroll",
        start: 25,
        end: 31,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "ranges",
        start: 31,
        end: 42,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "formats",
        start: 42,
        end: 48,
        core: 2,
        setup: &[],
    },
    CaseGroup {
        name: "outline",
        start: 48,
        end: 57,
        core: 1,
        setup: &[],
    },
    CaseGroup {
        name: "conditional",
        start: 57,
        end: 69,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "validation",
        start: 69,
        end: 88,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "charts-theme",
        start: 88,
        end: 94,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "edit-state",
        start: 94,
        end: 170,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "selection",
        start: 170,
        end: 207,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "names",
        start: 207,
        end: 213,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "edges",
        start: 213,
        end: 221,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "clipboard",
        start: 221,
        end: 254,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "panes",
        start: 254,
        end: 268,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "axes",
        start: 268,
        end: 303,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "shift-sheets",
        start: 303,
        end: 329,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "review-find",
        start: 329,
        end: 357,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "atlas",
        start: 357,
        end: 370,
        core: 2,
        setup: &[],
    },
    CaseGroup {
        name: "ledger",
        start: 370,
        end: 384,
        core: 1,
        setup: &[],
    },
    CaseGroup {
        name: "canvas",
        start: 384,
        end: 394,
        core: 1,
        setup: &[],
    },
    CaseGroup {
        name: "tree",
        start: 394,
        end: 404,
        core: 2,
        setup: &[],
    },
    CaseGroup {
        name: "split-panes",
        start: 404,
        end: 413,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "styled-fit",
        start: 413,
        end: 416,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "notebook",
        start: 416,
        end: 431,
        core: 3,
        setup: &[],
    },
    CaseGroup {
        name: "alignment",
        start: 431,
        end: 437,
        core: 6,
        setup: &[],
    },
    CaseGroup {
        name: "model-commands",
        start: 437,
        end: 444,
        core: 1,
        setup: &[],
    },
    CaseGroup {
        name: "recalculation-contract",
        start: 444,
        end: 445,
        core: 1,
        setup: &[],
    },
    CaseGroup {
        name: "presentation",
        start: 445,
        end: 448,
        core: 3,
        setup: &[],
    },
];
