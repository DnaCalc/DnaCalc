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
            Check::Attr("grid", "aria-rowcount", "1048577"),
            Check::Attr("grid", "aria-colcount", "16385"),
            Check::Attr("grid", "aria-multiselectable", "true"),
            Check::Text("[role=columnheader][aria-colindex=\"2\"]", "A"),
            Check::Text("[role=rowheader][aria-colindex=\"1\"]", "1"),
            Check::Attr("#dc-grid-cell-0-1-1", "aria-colindex", "2"),
            Check::Attr("grid-announcement", "aria-live", "polite"),
            Check::Attr("result-announcement", "aria-live", "off"),
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
            Check::Text("grid-announcement", "A3, 7, calculated, formula =A1*A2+1. Selection A3"),
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
            Action::Wait(Check::Text(cell!("A1"), "1")),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            Action::Wait(Check::Text(cell!("A2"), "2")),
            Action::Wait(Check::Value("name-box", "A3")),
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
            // Column E is 4 characters (33 px). The largest hash mask fitting the actual
            // browser font signals numeric overflow; text uses the measured font too.
            Check::HashMask("E1"),
            Check::PlanCell("E1", "kind", "number"),
            Check::PlanCell("E1", "format", "applied"),
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
            Check::PlanCell("A2", "text", ""),
            Check::PlanCell("A2", "provenance", "empty"),
            Check::PlanCell("A2", "formula", "false"),
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
            Check::PlanCell("B1", "text", ""),
            Check::PlanCell("B1", "provenance", "empty"),
            Check::PlanCell("B1", "formula", "false"),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Check::Style("[data-dnacalc=cell-decoration][data-cell=A2]", "background-color:#ffc7ce"),
            Check::Style(cell!("A2"), "color:#9c0006"),
            Check::Style(cell!("B1"), "font-weight:700"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=C2]", "background-color:#ffeb9c"),
            Check::Style(cell!("D2"), "font-style:italic"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=E2]", "background-color:#86c97e"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=J2]", "background-color:#83cceb"),
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
        checks: &[Check::Style("[data-dnacalc=cell-decoration][data-cell=A3]", "background-color:#ffc7ce")],
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
            Check::Absent("[data-dnacalc=cell-decoration][data-cell=A2]"),
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
            Check::Style("[data-dnacalc=cell-decoration][data-cell=A2]", "background-color:#ffc7ce"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=A3]", "background-color:#ffc7ce"),
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
            Check::Style("[data-dnacalc=cell-decoration][data-cell=A2]", "background-color:#ffc7ce"),
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
            Check::Style("[data-dnacalc=cell-decoration][data-cell=P1]", "background-color:#ffeb9c"),
            Check::Text(cell!("P1"), ""),
            Check::PlanCell("P1", "provenance", "empty"),
        ],
    },
    Case {
        name: "undo removes the blank-cell conditional appearance through the subscription",
        actions: &[Action::Click("undo")],
        checks: &[Check::Absent(cell!("P1")),Check::Absent("[data-dnacalc=cell-decoration][data-cell=P1]")],
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),
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
            Action::Wait(Check::Text("validation-prompt", "ColourPick a colour")),
            Action::Wait(Check::Absent("dropdown-list")),
            Action::Wait(Check::Focused("grid")),
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
            Action::Window(1440, 1400),
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
            // The second seed belongs to the next cell, after the first commit retires its editor.
            Action::Wait(Check::Text(cell!("A1"), "1")),
            Action::Wait(Check::Value("name-box", "A2")),
            Action::Wait(Check::Focused("grid")),
            Action::Keys("grid", "2"),
            Action::Keys("cell-editor", ENTER),
            // Finish publishing the seed before locating the name field for the completion test.
            Action::Wait(Check::Text(cell!("A2"), "2")),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Check::Attr("result-announcement", "aria-live", "off"),
            Check::Attr("grid-announcement", "aria-live", "polite"),
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
            Check::Attr("result-announcement", "aria-live", "polite"),
            Check::TextContains("result-announcement", "Refused"),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Check::TextContains("grid-announcement", "Selection E1:E1048576, 1048576 rows by 1 columns"),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Action::Window(1280, 1400),
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
            Check::HashMask("G3"),
            Check::Value(BAR, "45323"),
            Check::PlanCell("G3", "kind", "number"),
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
        checks: &[
            Check::HashMask("G3"),
            Check::Value(BAR, "45323"),
            Check::PlanCell("G3", "kind", "number"),
            Check::PlanCell("G3", "format", "applied"),
        ],
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            // The preceding drag can fail before restoring zoom. Keep C3 in this viewport.
            Action::Click("zoom-reset"),
            Action::Wait(Check::Text("zoom", "100%")),
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
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-font"),
            Action::Into("format-font", "monospace"),
            Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("B2", "style.family", "monospace")),
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
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-font"),
            Action::Into("format-font", "monospace"),
            Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("B5", "style.family", "monospace")),
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
        name: "axis: AutoFit uses the stored bold font variant that the Grid renders",
        actions: &[
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-font"),
            Action::Into("format-font", "monospace"),
            Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("A1", "style.family", "monospace")),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Action::Into("name-box", "A1\u{E007}"),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Check::Value(BAR, "=SEQUENCE(3,2)"),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
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
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-font"),
            Action::Into("format-font", "monospace"),
            Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("B30", "style.family", "monospace")),
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
        actions: &[
            Action::OpenDetails("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-actions]"),
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-grid]"),
        ],
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
            Action::OpenDetails("[data-dnacalc=block][data-cell=C1] [data-dnacalc=block-actions]"),
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
            Action::Window(1440, 1200),
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
        actions: &[
            Action::Into("name-box", "B6:J6\u{E007}"),
            Action::Click("format-cells"),
            Action::Click("format-tab-font"),
            Action::Choose("format-underline", "true"),
            Action::Choose("format-strike", "true"),
            Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("D6", "style.underline", "true")),
        ],
        checks: &[
            Check::PlanCell("D6", "style.strike", "true"),
            Check::PlanCell("J6", "style.underline", "true"),
            Check::PlanCell("J6", "style.strike", "true"),
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
            Check::PlanCell("D6", "style.underline", "true"),
            Check::PlanCell("D6", "style.strike", "true"),
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
            Action::Wait(Check::ActiveDescendant("grid")),
            Action::Wait(Check::Name("#dc-grid-cell-0-1-1", "A1, 42, literal")),
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
    Case {
        name: "presentation: a real font load updates the plan without recalculating",
        actions: &[Action::RememberPresentation, Action::LoadLocalFont],
        checks: &[
            Check::PresentationChanged("font"),
            Check::PlanCell("A1", "provenance", "calculated"),
        ],
    },
    Case {
        name: "renderer: switching to Canvas2D keeps the committed plan and paints actual text",
        actions: &[
            Action::Window(1440, 1400),
            Action::Into(BAR, "=6*7\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::PlanCell("A1", "text", "42")),
            Action::OpenDetails("pane-tools"),
            Action::RememberPresentation,
            Action::SetPainter("canvas"),
        ],
        checks: &[
            Check::PainterStable,
            Check::CanvasInk("A1"),
            Check::ActiveDescendant("grid"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "renderer: Canvas2D edits and recalculates through the shared keyboard and worker route",
        actions: &[
            Action::Keys("grid", "5"),
            Action::Keys("cell-editor", ENTER),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::PlanCell("A1", "text", "5"),
            Check::CanvasInk("A1"),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "renderer: returning to DOM preserves the same result and focus",
        actions: &[Action::RememberPresentation, Action::SetPainter("dom")],
        checks: &[
            Check::PainterStable,
            Check::Text(cell!("A1"), "5"),
            Check::ActiveDescendant("grid"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "layout: a two-dimensional merge has one paint and semantic anchor",
        actions: &[
            Action::Window(1440, 1000),
            Action::ClearDownloads,
            Action::AlignmentFixture,
            Action::UploadDownload("alignment.xlsx"),
            Action::DiscardReplacement,
            Action::Into("zoom-percent", "100\u{E007}"),
            Action::Into("name-box", "M2\u{E007}"),
        ],
        checks: &[
            Check::MergedCell("L1", 2, 3),
            Check::PlanCell("L1", "text", "merged"),
            Check::Editability("L1", true),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "layout: Canvas uses the same merge and keyboard navigation skips its covered cells",
        actions: &[
            Action::OpenDetails("pane-tools"),
            Action::RememberPresentation,
            Action::SetPainter("canvas"),
            Action::Wait(Check::PainterStable),
            Action::Keys("grid", "\u{E014}"),
            Action::Wait(Check::Value("name-box", "O1")),
            Action::Into("name-box", "K1:O4\u{E007}"),
            Action::Keys("grid", "\u{E004}"),
            Action::Wait(Check::ActiveSemantic("L1")),
            Action::Keys("grid", "\u{E004}"),
            Action::Wait(Check::ActiveSemantic("O1")),
            Action::Wait(Check::Attr("selection", "data-area", "K1:O4")),
            Action::Keys("grid", "\u{E008}\u{E004}\u{E000}"),
            Action::Wait(Check::ActiveSemantic("L1")),
            Action::Into("name-box", "L1:N4\u{E007}"),
            Action::Keys("grid", ENTER),
            Action::Wait(Check::ActiveSemantic("L3")),
            Action::Wait(Check::Attr("selection", "data-area", "L1:N4")),
            Action::Keys("grid", "\u{E008}\u{E007}\u{E000}"),
            Action::Wait(Check::ActiveSemantic("L1")),
            Action::Into("name-box", "L1:N2\u{E007}"),
            Action::Keys("grid", "\u{E008}\u{E012}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "K1:N2")),
            Action::Keys("grid", "\u{E008}\u{E014}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "L1:N2")),
            Action::Keys("grid", "\u{E008}\u{E014}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "L1:O2")),
            Action::Keys("grid", "\u{E008}\u{E012}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "L1:N2")),
            Action::Keys("grid", "\u{E008}\u{E015}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "L1:N3")),
            Action::Keys("grid", "\u{E008}\u{E013}\u{E000}"),
            Action::Wait(Check::Attr("selection", "data-area", "L1:N2")),
            Action::Into("name-box", "O1\u{E007}"),
        ],
        checks: &[
            Check::MergedCell("L1", 2, 3),
            Check::CanvasInk("L1"),
            Check::Value("name-box", "O1"),
            Check::ActiveDescendant("grid"),
        ],
    },
    Case {
        name: "layout: a real applied font changes measured overflow and actual text geometry only",
        actions: &[
            // Keep every synthetic input in view before and after the missing-font
            // notice disappears, so the strict value-list comparison has one coverage.
            Action::Window(1440, 2000),
            Action::SetPainter("dom"),
            Action::Into("name-box", "B10\u{E007}"),
            Action::Wait(Check::PlanCell("B10", "text", "iiiiiiiiiiiiiiiiiiiiiiii")),
            Action::Wait(Check::PlanCell("D18", "text", "double X")),
            Action::RememberPresentation,
            Action::LoadAppliedFont,
            Action::Wait(Check::FontReflow("B10")),
            Action::Wait(Check::PresentationChanged("font")),
            Action::RememberPresentation,
            Action::SetPainter("canvas"),
        ],
        checks: &[
            Check::PainterStable,
            Check::CanvasInk("B10"),
            Check::Editability("B10", true),
        ],
    },
    Case {
        name: "layout: populated and blank protected cells publish the same editor preflight refusal",
        actions: &[
            Action::Window(1440, 1000),
            Action::SetPainter("dom"),
            Action::Click("new"),
            Action::DiscardReplacement,
            Action::Into(BAR, "7\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::ProtectedDownloadFixture,
            Action::UploadDownload("protected.xlsx"),
            Action::DiscardReplacement,
            Action::Into("name-box", "A1\u{E007}"),
            Action::RememberPresentation,
            Action::Keys("grid", "\u{E032}"),
        ],
        checks: &[
            Check::Editability("A1", false),
            Check::Editability("B1", false),
            Check::PlanCell("A1", "text", "7"),
            Check::Absent("cell-editor"),
            Check::TextContains("status", "protected"),
            Check::PainterStable,
        ],
    },
    Case {
        name: "format cells: Ctrl+1 opens the shared command and previews the selected number",
        actions: &[
            Action::Into(BAR, "1234.5\u{E007}"),
            Action::Into("name-box", "A1:B2\u{E007}"),
            Action::OpenDetails("axis-tools"),
            Action::Into("column-size", "20"),
            Action::Click("set-column-size"),
            Action::Into("row-size", "30"),
            Action::Click("set-row-size"),
            Action::Keys("grid", "\u{E009}1\u{E000}"),
            Action::Wait(Check::Focused("format-number")),
            Action::Choose("format-preset", "#,##0.00"),
            Action::Click("format-tab-alignment"),
            Action::Choose("format-horizontal", "left"),
            Action::Choose("format-vertical", "top"),
            Action::Choose("format-wrap", "true"),
            Action::Click("format-tab-font"),
            Action::Into("format-font", "monospace"),
            Action::Into("format-size", "14"),
            Action::Choose("format-bold", "true"),
            Action::Choose("format-italic", "true"),
            Action::Choose("format-strike", "true"),
            Action::Wait(Check::Computed("format-preview", "text-decoration-line:line-through")),
            Action::Choose("format-underline", "true"),
            Action::Into("format-font-color", "#123456"),
            Action::Click("format-tab-fill"),
            Action::Into("format-fill", "#ffddaa"),
            Action::Click("format-tab-borders"),
            Action::Click("format-border-replace"),
            Action::Click("format-border-left"),
            Action::Click("format-border-right"),
            Action::Click("format-border-top"),
            Action::Click("format-border-bottom"),
            Action::Choose("format-border-style", "double"),
            Action::Into("format-border-color", "#cc0000"),
        ],
        checks: &[
            Check::Text("format-preview", "1,234.50"),
            Check::Computed("format-preview", "text-decoration-line:underline line-through"),
            Check::Text("format-message", ""),
            Check::PlanCell("A1", "text", "1234.5"),
        ],
    },
    Case {
        name: "format cells: one command paints number, alignment, font, fill and borders, including blank cells",
        actions: &[Action::Click("format-apply")],
        checks: &[
            Check::Absent("[data-dnacalc=format-dialog][open]"),
            Check::PlanCell("A1", "text", "1,234.50"),
            Check::PlanCell("A1", "style.bold", "true"),
            Check::PlanCell("A1", "style.italic", "true"),
            Check::PlanCell("A1", "style.underline", "true"),
            Check::PlanCell("A1", "style.strike", "true"),
            Check::PlanCell("A1", "style.horizontal", "left"),
            Check::PlanCell("A1", "style.vertical", "top"),
            Check::PlanCell("B2", "provenance", "empty"),
            Check::PlanCell("B2", "text", ""),
            Check::PlanCell("B2", "style.fill", "#ffddaa"),
            Check::PlanCell("A1", "style.font_color", "#123456"),
            Check::Computed(
                cell!("A1"),
                "font-family:monospace;font-weight:700;font-style:italic;color:#123456",
            ),
            Check::Style(
                "[data-dnacalc=cell-decoration][data-cell=B2]",
                "background-color:#ffddaa",
            ),
            Check::Style(
                "[data-dnacalc=cell-borders][data-cell=B2]",
                "border-left:3px double #cc0000;border-right:3px double #cc0000;border-top:3px double #cc0000;border-bottom:3px double #cc0000",
            ),
        ],
    },
    Case {
        name: "format cells: Undo restores the whole previous style without altering inputs",
        actions: &[Action::Click("undo")],
        checks: &[
            Check::PlanCell("A1", "text", "1234.5"),
            Check::PlanCell("A1", "style.bold", "false"),
            Check::PlanAbsent("B2"),
            Check::Absent("[data-dnacalc=cell-decoration][data-cell=B2]"),
        ],
    },
    Case {
        name: "format cells: Redo restores all style properties as one history step",
        actions: &[Action::Click("redo")],
        checks: &[
            Check::PlanCell("A1", "text", "1,234.50"),
            Check::PlanCell("A1", "style.family", "monospace"),
            Check::PlanCell("B2", "style.fill", "#ffddaa"),
            Check::PlanCell("B2", "style.borders.0.style", "double"),
        ],
    },
    Case {
        name: "format cells: saved and reopened xlsx retains authored styles on numbers and blank cells",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::PlanCell("A1", "text", "1,234.50"),
            Check::PlanCell("A1", "style.bold", "true"),
            Check::PlanCell("A1", "style.underline", "true"),
            Check::PlanCell("A1", "style.strike", "true"),
            Check::PlanCell("B2", "style.fill", "#ffddaa"),
            Check::PlanCell("B2", "style.borders.0.color", "#cc0000"),
        ],
    },
    Case {
        name: "format cells: mixed selections retain unedited number formats, fonts and fills",
        actions: &[
            Action::Into("name-box", "C1\u{E007}"),
            Action::Into(BAR, "2\u{E007}"),
            Action::Into("name-box", "A1:C2\u{E007}"),
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Wait(Check::Value("format-number", "")),
            Action::Wait(Check::Value("format-font", "")),
            Action::Click("format-tab-font"),
            Action::Choose("format-italic", "false"),
            Action::Click("format-apply"),
        ],
        checks: &[
            Check::PlanCell("A1", "text", "1,234.50"),
            Check::PlanCell("A1", "style.family", "monospace"),
            Check::PlanCell("A1", "style.italic", "false"),
            Check::PlanCell("B2", "style.fill", "#ffddaa"),
            Check::PlanCell("C1", "text", "2"),
            Check::PlanCell("C1", "style.italic", "false"),
        ],
    },
    Case {
        name: "format cells: Cancel leaves the published plan and workbook command count unchanged",
        actions: &[
            Action::RememberPresentation,
            Action::Click("format-cells"),
            Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-fill"),
            Action::Into("format-fill", "#ffffff"),
            Action::Click("format-cancel"),
        ],
        checks: &[
            Check::PainterStable,
            Check::PlanCell("B2", "style.fill", "#ffddaa"),
            Check::Focused("grid"),
        ],
    },
    Case {
        name: "format cells: physical fill and borders remain visible across a merged cell after save and reopen",
        actions: &[
            Action::ClearDownloads,Action::AlignmentFixture,Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,
            Action::Into("name-box","L1:N2\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-fill"),Action::Into("format-fill","#ffeecc"),Action::Click("format-tab-borders"),Action::Click("format-border-replace"),
            Action::Click("format-border-left"),Action::Click("format-border-right"),Action::Click("format-border-top"),Action::Click("format-border-bottom"),
            Action::Choose("format-border-style","thick"),Action::Into("format-border-color","#006600"),Action::Click("format-apply"),
            Action::Wait(Check::PlanCell("L1","style.fill","#ffeecc")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","L1\u{E007}"),
        ],
        checks: &[
            Check::MergedCell("L1",2,3),Check::PlanCell("L1","text","merged"),Check::PlanCell("L1","style.fill","#ffeecc"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=L1]","background-color:#ffeecc"),
            Check::Style("[data-dnacalc=cell-borders][data-cell=L1]","border-left:3px solid #006600;border-right:3px solid #006600;border-top:3px solid #006600;border-bottom:3px solid #006600"),
        ],
    },
    Case {
        name: "names: add a workbook definition through Name Manager without implicit evaluation",
        actions: &[
            Action::Click("new"), Action::DiscardReplacement,
            Action::Click("name-manager"), Action::Click("name-new"),
            Action::Into("name-name", "Choice"), Action::Into("name-formula", "=40+2"), Action::Click("name-save"),
            Action::Wait(Check::Count("[data-dnacalc=name-row]",1)),
            Action::Click("[data-name=Choice][data-name-scope=Workbook] [data-dnacalc=name-select]"),
        ],
        checks: &[Check::Text("name-value","Not evaluated"),Check::Value("name-formula","=40+2"),Check::TextContains("names-count","1 names")],
    },
    Case {
        name: "names: explicit evaluation carries a command identity, caller and binding version",
        actions: &[Action::Click("name-evaluate")],
        checks: &[Check::Text("name-value","42"),Check::TextContains("name-evaluation-context","Explicit evaluation"),Check::TextContains("name-evaluation-context","definition version")],
    },
    Case {
        name: "names: a worksheet definition can shadow the workbook spelling",
        actions: &[
            Action::Click("name-new"),Action::Into("name-name","Choice"),Action::Keys("name-scope","\u{E010}\u{E007}"),Action::Into("name-formula","=99"),Action::Click("name-save"),
            Action::Wait(Check::Count("[data-dnacalc=name-row]",2)),Action::Click("[data-name=Choice][data-name-scope=Sheet1] [data-dnacalc=name-select]"),Action::Click("name-evaluate"),
        ],
        checks: &[Check::Text("name-value","99"),Check::Value("name-formula","=99")],
    },
    Case {
        name: "names: selected workbook identity remains evaluable under a local shadow",
        actions: &[Action::Click("[data-name=Choice][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Wait(Check::Text("name-value","Not evaluated")),Action::Click("name-evaluate")],
        checks: &[Check::Text("name-value","42")],
    },
    Case {
        name: "names: editing the definition recalculates its cell users through the normal command route",
        actions: &[
            Action::Click("names-close"),Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"=Choice*2\u{E007}"),Action::Wait(Check::PlanCell("A1","text","198")),
            Action::Click("name-manager"),Action::Click("[data-name=Choice][data-name-scope=Sheet1] [data-dnacalc=name-select]"),Action::Into("name-formula","=100"),Action::Click("name-save"),
        ],
        checks: &[Check::PlanCell("A1","text","200"),Check::PlanCell("A1","provenance","calculated"),Check::Text("name-value","Not evaluated")],
    },
    Case {
        name: "names: existing name history drives Undo and Redo",
        actions: &[Action::Click("names-undo"),Action::Wait(Check::PlanCell("A1","text","198")),Action::Click("names-redo")],
        checks: &[Check::PlanCell("A1","text","200"),Check::TextContains("[data-name=Choice][data-name-scope=Sheet1]","=100")],
    },
    Case {
        name: "names: delete exposes scoped fallback, and Undo restores the definition",
        actions: &[
            Action::Click("[data-name=Choice][data-name-scope=Sheet1] [data-dnacalc=name-select]"),Action::Click("name-delete"),Action::Wait(Check::PlanCell("A1","text","84")),Action::Wait(Check::Count("[data-dnacalc=name-row]",1)),Action::Click("names-undo"),
        ],
        checks: &[Check::PlanCell("A1","text","200"),Check::Count("[data-dnacalc=name-row]",2)],
    },
    Case {
        name: "names: relative reference evaluation uses the explicit caller anchor",
        actions: &[
            Action::Click("name-new"),Action::Into("name-name","Relative"),Action::Into("name-formula","=B1"),Action::Click("name-save"),Action::Wait(Check::Count("[data-dnacalc=name-row]",3)),Action::Click("[data-name=Relative] [data-dnacalc=name-select]"),Action::Into("name-anchor","C3"),Action::Click("name-evaluate"),
        ],
        checks: &[Check::TextContains("name-value","D3"),Check::TextContains("name-evaluation-context","C3")],
    },
    Case {
        name: "names: arrays are explicit evaluation results rather than shared cached name values",
        actions: &[
            Action::Click("name-new"),Action::Into("name-name","Pair"),Action::Into("name-formula","={1,2;3,4}"),Action::Click("name-save"),Action::Wait(Check::Count("[data-dnacalc=name-row]",4)),Action::Click("[data-name=Pair] [data-dnacalc=name-select]"),Action::Click("name-evaluate"),
        ],
        checks: &[Check::Text("name-value","2×2 array: 1; 2; 3; 4")],
    },
    Case {
        name: "names: scoped definitions survive xlsx save and reopen without an implicit value",
        actions: &[
            Action::Click("names-close"),Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Click("name-manager"),Action::Wait(Check::Count("[data-dnacalc=name-row]",4)),Action::Click("[data-name=Choice][data-name-scope=Sheet1] [data-dnacalc=name-select]"),
        ],
        checks: &[Check::Value("name-formula","=100"),Check::Text("name-value","Not evaluated"),Check::PlanCell("A1","text","200")],
    },

    Case {
        name: "large formatting: a whole column formats values and visible blank cells",
        actions: &[
            Action::Click("new"),Action::DiscardReplacement,Action::Into("name-box","B2\u{E007}"),Action::Into(BAR,"12.5\u{E007}"),
            Action::Into("name-box","B1:B1048576\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),
            Action::Into("format-number","0.00"),Action::Click("format-tab-font"),Action::Into("format-font","monospace"),
            Action::Click("format-tab-fill"),Action::Into("format-fill","#ffeedd"),Action::Click("format-apply"),
        ],
        checks: &[Check::PlanCell("B2","text","12.50"),Check::PlanCell("B10","style.fill","#ffeedd"),Check::PlanCell("B10","provenance","empty"),Check::PlanCell("B2","style.family","monospace")],
    },
    Case {
        name: "large formatting: a whole row preserves the column intersection's other properties",
        actions: &[
            Action::Into("name-box","A2:XFD2\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-font"),Action::Choose("format-bold","true"),Action::Click("format-apply"),
        ],
        checks: &[Check::PlanCell("B2","text","12.50"),Check::PlanCell("B2","style.fill","#ffeedd"),Check::PlanCell("B2","style.bold","true"),Check::PlanCell("C2","style.bold","true")],
    },
    Case {
        name: "large formatting: a 40002-cell rectangle uses the same command as a small selection",
        actions: &[
            Action::Into("name-box","D10:E20010\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-fill"),Action::Into("format-fill","#00ff00"),Action::Click("format-apply"),
        ],
        checks: &[Check::PlanCell("D10","style.fill","#00ff00"),Check::PlanCell("E10","style.fill","#00ff00"),Check::PlanCell("B10","style.fill","#ffeedd"),Check::PlanAbsent("C10")],
    },
    Case {
        name: "large formatting: Undo and Redo restore the rectangle without changing axis defaults",
        actions: &[Action::Click("undo"),Action::Wait(Check::PlanAbsent("D10")),Action::Wait(Check::PlanCell("B10","style.fill","#ffeedd")),Action::Click("redo")],
        checks: &[Check::PlanCell("D10","style.fill","#00ff00"),Check::PlanCell("B2","style.bold","true")],
    },
    Case {
        name: "large formatting: row, column and rectangle formats survive actual xlsx save and reopen",
        actions: &[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady),Action::Into("name-box","D10:E20010\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Choose("format-italic","true"),Action::Click("format-apply")],
        checks: &[Check::PlanCell("B2","text","12.50"),Check::PlanCell("B2","style.bold","true"),Check::PlanCell("B10","style.fill","#ffeedd"),Check::PlanCell("D10","style.fill","#00ff00"),Check::PlanAbsent("C10"),Check::PlanCell("D10","style.italic","true")],
    },
    Case {
        name: "large formatting: reopened whole-axis defaults reach the final row and column",
        actions: &[
            Action::Into("name-box","XFD2\u{E007}"),Action::Wait(Check::PlanCell("XFD2","style.bold","true")),
            Action::Into("name-box","B1048576\u{E007}"),
        ],
        checks: &[Check::PlanCell("B1048576","style.fill","#ffeedd"),Check::PlanCell("B1048576","style.family","monospace"),Check::PlanCell("B1048576","provenance","empty")],
    },

    Case {
        name: "physical styles: imported pattern fills reach the DOM as repeating physical tiles",
        actions: &[
            Action::SetPainter("dom"),Action::ClearDownloads,Action::AlignmentFixture,Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,
            Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady),Action::Into("name-box","B13\u{E007}"),Action::Wait(Check::PlanCell("B13","style.brush.Pattern.name","darkDown")),Action::Into("name-box","B12\u{E007}"),
        ],
        checks: &[Check::PlanCell("B12","style.brush.Pattern.name","darkGray"),Check::PlanCell("E12","style.brush.Pattern.name","gray125"),Check::PlanCell("B13","style.brush.Pattern.name","darkDown"),Check::PlanCell("I13","style.brush.Pattern.name","lightTrellis"),Check::Style("[data-dnacalc=cell-decoration][data-cell=B12]","background-repeat:repeat")],
    },
    Case {
        name: "physical styles: linear and path gradients share resolved stops and merged geometry",
        actions: &[Action::Into("name-box","B15\u{E007}")],
        checks: &[Check::MergedCell("B15",2,3),Check::MergedCell("F15",2,3),Check::PlanCell("B15","style.brush.Gradient.degree","90"),Check::PlanCell("B15","style.brush.Gradient.stops.0.1","#ff0000"),Check::PlanCell("F15","style.brush.Gradient.kind","path"),Check::PlanCell("F15","style.brush.Gradient.rectangle.0","0.2"),Check::Style("[data-dnacalc=cell-decoration][data-cell=B15]","background-repeat:no-repeat;background-size:100% 100%")],
    },
    Case {
        name: "physical styles: both diagonal directions retain their colour and double strokes",
        actions: &[Action::Into("name-box","B18\u{E007}")],
        checks: &[Check::PlanCell("B18","style.diagonal.0.style","thin"),Check::PlanCell("D18","style.diagonal.0.style","double"),Check::PlanCell("D18","style.diagonal.1.color","#008000"),Check::Count("[data-dnacalc=cell-borders][data-cell=B18] line",1),Check::Count("[data-dnacalc=cell-borders][data-cell=D18] line",4)],
    },
    Case {
        name: "physical styles: merged glyphs clip to one anchor and covered perimeter styles replace wider borders",
        actions: &[Action::Into("name-box","L5\u{E007}")],
        checks: &[Check::MergedCell("L5",2,3),Check::Text(cell!("L5"),"MERGED GLYPHS WWWWWWWWWWWWWW"),Check::Computed(cell!("L5"),"font-family:monospace"),Check::Count("[data-dnacalc=cell-borders][data-cell=L5] line",7),Check::Style("[data-dnacalc=cell-borders][data-cell=L5]","border-top:3px solid #cc0000"),Check::Computed("[data-dnacalc=cell-borders][data-cell=L5]","border-right-width:0px;border-bottom-width:0px")],
    },
    Case {
        name: "physical styles: Canvas draws the same merged glyphs without submitting a workbook command",
        actions: &[Action::RememberPresentation,Action::SetPainter("canvas")],
        checks: &[Check::PainterStable,Check::CanvasInk("L5"),Check::PlanCell("L5","style.fill","#ffffff"),Check::MergedCell("L5",2,3)],
    },
    Case {
        name: "physical styles: an unrelated font edit and real xlsx reopen preserve gradients diagonals and merged edges",
        actions: &[
            Action::Into("name-box","B15:D16\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Choose("format-bold","true"),Action::Click("format-apply"),
            Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady),
            Action::Into("name-box","B15\u{E007}"),Action::Wait(Check::PlanCell("B15","style.brush.Gradient.degree","90")),Action::Into("name-box","D18\u{E007}"),Action::Wait(Check::PlanCell("D18","style.diagonal.1.style","double")),Action::Into("name-box","L5\u{E007}"),Action::SetPainter("dom"),
        ],
        checks: &[Check::MergedCell("L5",2,3),Check::Count("[data-dnacalc=cell-borders][data-cell=L5] line",7),Check::Style("[data-dnacalc=cell-borders][data-cell=L5]","border-top:3px solid #cc0000")],
    },


    Case {
        name: "physical styles: gradient and glyph merges retain their four clips across both frozen boundaries",
        actions: &[
            Action::ViewDownloadFixtureFor("alignment.xlsx","<sheetViews><sheetView workbookViewId=\"0\" topLeftCell=\"B15\"><pane xSplit=\"1\" ySplit=\"1\" topLeftCell=\"C16\" activePane=\"bottomRight\" state=\"frozen\"/><selection pane=\"bottomRight\" activeCell=\"B15\" sqref=\"B15:D16\"/></sheetView></sheetViews>"),
            Action::UploadDownload("view.xlsx"),Action::DiscardReplacement,Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady),Action::Wait(Check::Count("[data-dnacalc=cell-decoration][data-cell=B15]",4)),
            Action::RememberPresentation,Action::SetPainter("canvas"),Action::Wait(Check::PainterStable),Action::SetPainter("dom"),
            Action::ViewDownloadFixtureFor("alignment.xlsx","<sheetViews><sheetView workbookViewId=\"0\" topLeftCell=\"L5\"><pane xSplit=\"1\" ySplit=\"1\" topLeftCell=\"M6\" activePane=\"bottomRight\" state=\"frozen\"/><selection pane=\"bottomRight\" activeCell=\"L5\" sqref=\"L5:N6\"/></sheetView></sheetViews>"),
            Action::UploadDownload("view.xlsx"),Action::DiscardReplacement,Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady),Action::Wait(Check::Count("[data-dnacalc=cell-borders][data-cell=L5]",4)),Action::Wait(Check::Count("[data-dnacalc=cell-borders][data-cell=L5] line",28)),
            Action::RememberPresentation,Action::SetPainter("canvas"),
        ],
        checks: &[Check::PainterStable,Check::CanvasInk("L5"),Check::Attr("grid","data-frozen-rows","5"),Check::Attr("grid","data-frozen-columns","12"),Check::ActiveSemantic("L5")],
    },

    Case {
        name: "clipboard history: pasted formats retain their captured colour after Undo and a new style branch",
        actions: &[
            Action::Into(BAR,"17\u{E007}"),Action::Into("name-box","A1\u{E007}"),
            Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-fill"),
            Action::Into("format-fill","#ff0000"),Action::Click("format-apply"),
            // Applying a format closes the modal only after the worker accepts it.
            Action::Wait(Check::Absent("[data-dnacalc=format-dialog][open]")),
            Action::Wait(Check::Focused("grid")),
            Action::Keys("grid","\u{E009}c\u{E000}"),Action::Click("undo"),
            Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-fill"),
            Action::Into("format-fill","#0000ff"),Action::Click("format-apply"),
            Action::Into("name-box","B1\u{E007}"),Action::OpenDetails("clipboard-tools"),Action::Choose("paste-mode","formats"),
            Action::Click("paste-snapshot"),
        ],
        checks:&[Check::PlanCell("A1","text","17"),Check::PlanCell("A1","style.fill","#0000ff"),Check::PlanCell("B1","text",""),Check::PlanCell("B1","style.fill","#ff0000")],
    },
    Case {
        name: "clipboard history: Undo removes restored clipboard definitions and Redo restores them atomically",
        actions:&[Action::Click("undo"),Action::Wait(Check::PlanAbsent("B1")),Action::Click("redo")],
        checks:&[Check::PlanCell("A1","style.fill","#0000ff"),Check::PlanCell("B1","style.fill","#ff0000")],
    },
    Case {
        name: "clipboard history: the two style branches remain distinct after xlsx save and reopen",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Wait(Check::Absent("[data-dnacalc=app][inert]")),Action::Wait(Check::PlanReady)],
        checks:&[Check::PlanCell("A1","text","17"),Check::PlanCell("A1","style.fill","#0000ff"),Check::PlanCell("B1","style.fill","#ff0000")],
    },
    Case {
        name:"array recovery: phone details show current spill bounds and the actual blocker",
        actions:&[Action::Window(390,844),Action::Into("name-box","B2\u{E007}"),Action::Into(BAR,"99\u{E007}"),Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"=SEQUENCE(3,2)\u{E007}"),Action::Into("name-box","A1\u{E007}")],
        checks:&[Check::Fits,Check::Text("result-state","Worksheet error"),Check::PlanCell("A1","provenance","calculated"),Check::Text(cell!("A1"),"#SPILL!"),Check::TextContains("array-detail-summary","cannot spill into Sheet1!A1:B3"),Check::Name("array-blocker","Open blocker Sheet1!B2"),Check::Reachable("array-blocker"),Check::Contained("array-blocker","[data-dnacalc=result-details] .dc-result-body")],
    },
    Case {
        name:"array recovery: keyboard navigation to a blocker and clearing it refreshes details",
        actions:&[Action::Keys("array-blocker","\u{E007}"),Action::Wait(Check::Value("name-box","B2")),Action::Keys("grid",DELETE),Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("result-details")],
        checks:&[Check::TextContains("array-detail-summary","actual range Sheet1!A1:B3"),Check::Absent("array-blocker"),Check::PlanCell("B3","text","6"),Check::PlanCell("B3","provenance","calculated")],
    },
    Case {
        name:"array recovery: a spill child opens its real owner through shared source navigation",
        actions:&[Action::Into("name-box","B2\u{E007}"),Action::OpenDetails("result-details"),Action::Wait(Check::TextContains("array-detail-summary","Array anchor Sheet1!A1")),Action::Keys("array-owner","\u{E007}")],
        checks:&[Check::Value("name-box","A1"),Check::Focused("grid"),Check::Absent("array-blocker")],
    },
    Case {
        name:"array recovery: a new blocker replaces recovered details at tablet width",
        actions:&[Action::Window(1024,768),Action::Into(BAR,"=SEQUENCE(1,2)\u{E007}"),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::TextContains("array-detail-summary","actual range Sheet1!A1:B1")),Action::Into("name-box","B2\u{E007}"),Action::Into(BAR,"101\u{E007}"),Action::Into("name-box","B2\u{E007}"),Action::Wait(Check::Value(BAR,"101")),Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"=SEQUENCE(3,2)\u{E007}"),Action::Into("name-box","A1\u{E007}")],
        checks:&[Check::Fits,Check::TextContains("array-detail-summary","cannot spill into Sheet1!A1:B3"),Check::Count("array-blocker",1),Check::Name("array-blocker","Open blocker Sheet1!B2")],
    },
    Case {
        name:"array recovery: details follow source selection and do not retain stale blockers",
        actions:&[Action::Window(1440,1000),Action::Into("name-box","D10\u{E007}"),Action::OpenDetails("result-details")],
        checks:&[Check::Fits,Check::Absent("array-detail"),Check::Absent("array-blocker"),Check::Value("name-box","D10")],
    },
    Case {
        name:"phone ledger: address, calculated value, formula and provenance fit one audit row",
        actions:&[Action::Window(390,844),Action::Into(BAR,"=2+3\u{E007}"),Action::Into("name-box","A2\u{E007}"),Action::Into(BAR,"=NoSheet!A1\u{E007}"),Action::Into("name-box","A3\u{E007}"),Action::Into(BAR,"17\u{E007}"),Action::Into("name-box","A1\u{E007}"),Action::Click("skin-ledger")],
        checks:&[Check::Fits,Check::Count("ledger-row",2),Check::Contained("[data-cell='Sheet1!A1'] [data-dnacalc=ledger-select]","ledger-table"),Check::Contained("[data-cell='Sheet1!A1'] [data-dnacalc=ledger-value]","ledger-table"),Check::Contained("[data-cell='Sheet1!A1'] .dc-chip","ledger-table"),Check::Text("[data-cell='Sheet1!A1'] [data-dnacalc=ledger-value]","5"),Check::Name("[data-cell='Sheet1!A1'] [data-dnacalc=ledger-value]","5"),Check::Name("[data-cell='Sheet1!A1'] [data-field=provenance]","calculated"),Check::Attr("[data-cell='Sheet1!A2']","data-provenance","refused")],
    },
    Case {
        name:"phone ledger: refused source actions work by keyboard and retain controls on return",
        actions:&[Action::Choose("ledger-filter","refused"),Action::Wait(Check::Count("ledger-row",1)),Action::Keys("ledger-open-grid","\u{E007}"),Action::Wait(Check::Value("name-box","A2")),Action::Click("skin-ledger")],
        checks:&[Check::Fits,Check::Value("ledger-filter","refused"),Check::Count("ledger-row",1),Check::TextContains("ledger-reason","NoSheet"),Check::Contained("ledger-value","ledger-table"),Check::Contained("[data-field=provenance]","ledger-table")],
    },
    Case {
        name:"phone ledger: stale values stay paired with their state and literal inputs stay in Grid",
        actions:&[Action::Click("ledger-open-grid"),Action::Into("name-box","A1\u{E007}"),Action::Choose("calc-mode","manual"),Action::Into(BAR,"=3+4\u{E007}"),Action::Click("skin-ledger"),Action::Choose("ledger-filter","stale")],
        checks:&[Check::Fits,Check::Text("ledger-value","5"),Check::Attr("ledger-row","data-provenance","stale"),Check::TextContains("ledger-reason","awaiting recalculation"),Check::Contained("ledger-value","ledger-table"),Check::Contained("[data-field=provenance]","ledger-table")],
    },
    Case {
        name:"phone shell: a long workbook title keeps accessible identity and commands",
        actions:&[Action::Window(390,844),Action::RememberPresentation,Action::UploadAs("open-file","crates/calc-host/tests/fixtures/dc_09v/formats.xlsx","November forecast with a very long descriptive workbook name and review notes.xlsx"),Action::Wait(Check::Text("document","November forecast with a very long descriptive workbook name and review notes.xlsx"))],
        checks:&[Check::Fits,Check::HeaderHeightRetained,Check::Narrow(390),Check::Name("document","November forecast with a very long descriptive workbook name and review notes.xlsx"),Check::Reachable("document"),Check::Reachable("save"),Check::Reachable("undo"),Check::Reachable("[data-dnacalc=settings] summary")],
    },
    Case {
        name:"phone shell: changed titles retain their full accessible label and changed indicator",
        actions:&[Action::Into(BAR,"18\u{E007}")],
        checks:&[Check::Fits,Check::HeaderHeightRetained,Check::Name("document","November forecast with a very long descriptive workbook name and review notes.xlsx (changed)"),Check::Attr("document","data-changed","true"),Check::Reachable("document"),Check::Reachable("save"),Check::Reachable("redo")],
    },
    Case {
        name: "friendly functions: imported modern calls use entry spelling without changing their values",
        actions: &[
            Action::Into(BAR, "=_xlfn.SEQUENCE(2,2)\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[Check::Value(BAR, "=SEQUENCE(2,2)"), Check::Text(cell!("B2"), "4"), Check::PlanCell("A1", "provenance", "calculated")],
    },
    Case {
        name: "friendly functions: every formula skin uses the same display source",
        actions: &[
            Action::Click("skin-ledger"),
            Action::Wait(Check::Text("[data-dnacalc=ledger-row] [data-field=formula]", "=SEQUENCE(2,2)")),
            Action::Click("skin-canvas"),
            Action::Wait(Check::Text("canvas-formula", "=SEQUENCE(2,2)")),
            Action::Click("skin-notebook"),
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-select]"),
        ],
        checks: &[Check::Value("[data-dnacalc=block][data-cell=A1] textarea", "=SEQUENCE(2,2)"), Check::Text("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]", "1")],
    },
    Case {
        name: "friendly functions: accepting an unchanged displayed formula preserves the saved document",
        actions: &[
            Action::Keys("[data-dnacalc=block][data-cell=A1] textarea", "\u{E007}"),
            Action::Click("skin-grid"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys(BAR, "\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[Check::Text("document", "Book1.xlsx"), Check::Value(BAR, "=SEQUENCE(2,2)"), Check::Text(cell!("B2"), "4")],
    },
    Case {
        name: "friendly functions: strings retain their spelling through editing and history",
        actions: &[
            Action::Into("name-box", "D1\u{E007}"),
            Action::Into(BAR, "=\"_xlfn.SEQUENCE(\"&_xlfn.SUM(1,2)\u{E007}"),
            Action::Into("name-box", "D1\u{E007}"),
            Action::Click("undo"),
            Action::Wait(Check::PlanAbsent("D1")),
            Action::Click("redo"),
            Action::Into("name-box", "D1\u{E007}"),
        ],
        checks: &[Check::Value(BAR, "=\"_xlfn.SEQUENCE(\"&SUM(1,2)"), Check::Text(cell!("D1"), "_xlfn.SEQUENCE(3")],
    },
    Case {
        name: "Notebook actions: inactive blocks retain visible source results and provenance with folded controls",
        actions: &[
            Action::Window(390,844),
            Action::Into(BAR, "=2+3\u{E007}"),
            Action::Into(BAR, "=NoSheet!A1\u{E007}"),
            Action::Click("skin-notebook"),
            Action::Click("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-select]"),
        ],
        checks: &[
            Check::Attr("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-actions]","open",""),
            Check::Absent("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-actions][open]"),
            Check::Count("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-details]",1),
            Check::Rendered("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-details]",false),
            Check::Rendered("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-details]",true),
            Check::Rendered("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-value]",true),
            Check::Text("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-source]","=NoSheet!A1"),
            Check::Text("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-value]","5"),
            Check::Attr("[data-dnacalc=block][data-cell=A2]","data-provenance","refused"),
            Check::Fits,
        ],
    },
    Case {
        name: "Notebook actions: inactive controls open by keyboard and selection unfolds the chosen block",
        actions: &[
            Action::Keys("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-actions] summary","\u{E007}"),
            Action::Wait(Check::Rendered("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-details]",true)),
            Action::Wait(Check::Reachable("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-details]")),
            Action::Click("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-select]"),
            Action::Window(1024,768),
        ],
        checks: &[
            Check::Attr("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-actions]","open",""),
            Check::Absent("[data-dnacalc=block][data-cell=A1] [data-dnacalc=block-actions][open]"),
            Check::Value("[data-dnacalc=block][data-cell=A2] textarea","=NoSheet!A1"),
            Check::Fits,
        ],
    },
    Case {
        name: "Notebook actions: shared edit Escape retains the selected block focus and source",
        actions: &[
            Action::Into("[data-dnacalc=block][data-cell=A2] textarea","=17"),
            Action::Keys("[data-dnacalc=block][data-cell=A2] textarea",ESCAPE),
            Action::Window(1440,1000),
        ],
        checks: &[
            Check::Focused("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-select]"),
            Check::Value(BAR,"=NoSheet!A1"),
            Check::Attr("[data-dnacalc=block][data-cell=A2]","data-provenance","refused"),
            Check::Reachable("[data-dnacalc=block][data-cell=A2] [data-dnacalc=block-details]"),
        ],
    },

    Case {
        name: "transpose: a copied two-by-three matrix becomes three-by-two through the shared paste command",
        actions: &[
            Action::ExternalClipboard("1\t2\t3\n4\t5\t6",false),
            Action::Keys("grid","\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("C2"),"6")),
            Action::Into("name-box","A1:C2\u{E007}"),
            Action::Keys("grid","\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline",1)),
            Action::OpenDetails("clipboard-tools"),
            Action::Click("paste-transpose"),
            Action::Into("name-box","E1\u{E007}"),
            Action::Click("paste-snapshot"),
        ],
        checks: &[
            Check::Text(cell!("E1"),"1"),Check::Text(cell!("F1"),"4"),
            Check::Text(cell!("E2"),"2"),Check::Text(cell!("F2"),"5"),
            Check::Text(cell!("E3"),"3"),Check::Text(cell!("F3"),"6"),
            Check::Text(cell!("C2"),"6"),Check::PlanCell("F3","provenance","literal"),
        ],
    },
    Case {
        name: "transpose: one Undo removes the complete matrix and one Redo restores it",
        actions: &[
            Action::Click("undo"),Action::Wait(Check::PlanAbsent("E1")),Action::Wait(Check::PlanAbsent("F3")),
            Action::Wait(Check::Text(cell!("C2"),"6")),Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("E1"),"1"),Check::Text(cell!("F1"),"4"),Check::Text(cell!("E3"),"3"),Check::Text(cell!("F3"),"6")],
    },
    Case {
        name: "transpose: the saved matrix reopens and the command remains reachable on a phone",
        actions: &[
            Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,
            Action::Wait(Check::Text(cell!("F1"),"4")),Action::Wait(Check::Text(cell!("F3"),"6")),
            Action::Window(390,844),Action::OpenDetails("clipboard-tools"),
        ],
        checks: &[Check::Fits,Check::Narrow(390),Check::MinHeight("grid",120),Check::Reachable("grid"),Check::Reachable("paste-transpose"),Check::Name("paste-transpose","Transpose rows and columns")],
    },
    Case {
        name: "transpose: native external paste exchanges axes and keeps the ordinary history route",
        actions: &[
            Action::Window(1440,1000),Action::ExternalClipboard("11\t12\n13\t14\n15\t16",false),Action::Into("name-box","H1\u{E007}"),Action::Keys("grid","\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("H1"),"11")),Action::Wait(Check::Text(cell!("I1"),"13")),Action::Wait(Check::Text(cell!("J1"),"15")),
            Action::Wait(Check::Text(cell!("H2"),"12")),Action::Wait(Check::Text(cell!("I2"),"14")),Action::Wait(Check::Text(cell!("J2"),"16")),
            Action::Click("undo"),Action::Wait(Check::PlanAbsent("J2")),Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("H1"),"11"),Check::Text(cell!("J1"),"15"),Check::Text(cell!("H2"),"12"),Check::Text(cell!("J2"),"16"),Check::PlanCell("J2","provenance","literal")],
    },
    Case {
        name: "transpose modes: array formulas refuse atomically while preserving their source",
        actions: &[
            Action::Into(BAR,"=SEQUENCE(2,3)\u{E007}"),
            Action::Into("name-box","A1:C2\u{E007}"),Action::Keys("grid","\u{E009}c\u{E000}"),
            Action::Wait(Check::Count("copy-outline",1)),Action::OpenDetails("clipboard-tools"),Action::Click("paste-transpose"),
            Action::Into("name-box","E1\u{E007}"),Action::Click("paste-snapshot"),
        ],
        checks: &[Check::TextContains("status","Transposing array formulas is not modelled"),Check::PlanAbsent("E1"),Check::PlanAbsent("F3"),Check::Text(cell!("C2"),"6")],
    },
    Case {
        name: "transpose modes: Values uses the calculated snapshot after the array source changes",
        actions: &[
            Action::Choose("paste-mode","values"),Action::Into("name-box","A1\u{E007}"),
            Action::Into(BAR,"=SEQUENCE(2,3,10)\u{E007}"),Action::Into("name-box","E1\u{E007}"),Action::Click("paste-snapshot"),
        ],
        checks: &[Check::Text(cell!("E1"),"1"),Check::Text(cell!("F1"),"4"),Check::Text(cell!("E3"),"3"),Check::Text(cell!("F3"),"6"),Check::Text(cell!("C2"),"15"),Check::PlanCell("E1","formula","false"),Check::PlanCell("E1","provenance","literal")],
    },
    Case {
        name: "transpose modes: Values Undo and Redo retain the changed array source",
        actions: &[
            Action::Click("undo"),Action::Wait(Check::PlanAbsent("F3")),Action::Wait(Check::Text(cell!("C2"),"15")),Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("F3"),"6"),Check::Text(cell!("C2"),"15"),Check::PlanCell("F3","formula","false")],
    },
    Case {
        name: "transpose modes: cell formats exchange axes without replacing content and keep one Undo group",
        actions: &[
            Action::Into("name-box","A1:A2\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),
            Action::Click("format-tab-fill"),Action::Into("format-fill","#ff0000"),Action::Click("format-apply"),
            Action::Into("name-box","A1:C2\u{E007}"),Action::Keys("grid","\u{E009}c\u{E000}"),Action::Wait(Check::Count("copy-outline",1)),
            Action::Choose("paste-mode","formats"),Action::Into("name-box","J1\u{E007}"),Action::Into(BAR,"77\u{E007}"),
            Action::Into("name-box","J1\u{E007}"),Action::Click("paste-snapshot"),
            Action::Wait(Check::PlanCell("J1","style.fill","#ff0000")),Action::Wait(Check::PlanCell("K1","style.fill","#ff0000")),
            Action::Click("undo"),Action::Wait(Check::PlanAbsent("K1")),Action::Wait(Check::Text(cell!("J1"),"77")),Action::Click("redo"),
        ],
        checks: &[Check::Text(cell!("J1"),"77"),Check::PlanCell("J1","style.fill","#ff0000"),Check::PlanCell("K1","style.fill","#ff0000"),Check::PlanCell("K1","provenance","empty"),Check::Text(cell!("C2"),"15")],
    },
    Case {
        name: "transpose modes: Cut refuses without removing the array or writing the target",
        actions: &[
            Action::Choose("paste-mode","all"),Action::Into("name-box","A1:C2\u{E007}"),Action::Keys("grid","\u{E009}x\u{E000}"),
            Action::Wait(Check::Count("copy-outline",1)),Action::Into("name-box","M1\u{E007}"),Action::Click("paste-snapshot"),
        ],
        checks: &[Check::TextContains("status","Transposing a cut is not modelled"),Check::PlanAbsent("M1"),Check::Text(cell!("C2"),"15"),Check::Text(cell!("F3"),"6")],
    },
    Case {
        name: "transpose modes: saved values and cell formats reopen alongside the original calculated array",
        actions: &[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement],
        checks: &[Check::Text(cell!("C2"),"15"),Check::PlanCell("C2","provenance","calculated"),Check::Text(cell!("F3"),"6"),Check::PlanCell("F3","provenance","literal"),Check::Text(cell!("J1"),"77"),Check::PlanCell("K1","style.fill","#ff0000")],
    },

    Case {
        name:"name changes: rename updates bound users and nested aliases while retaining a local shadow",
        actions:&[
            Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("name-new"),Action::Into("name-name","Rate"),Action::Into("name-formula","=2"),Action::Click("name-save"),Action::Wait(Check::Count("[data-name=Rate][data-name-scope=Workbook]",1)),
            Action::Click("name-new"),Action::Into("name-name","Taken"),Action::Into("name-formula","=99"),Action::Click("name-save"),Action::Wait(Check::Count("[data-name=Taken]",1)),
            Action::Click("name-new"),Action::Into("name-name","Rate"),Action::Keys("name-scope","\u{E010}\u{E007}"),Action::Into("name-formula","=10"),Action::Click("name-save"),Action::Wait(Check::Count("[data-name=Rate][data-name-scope=Sheet1]",1)),
            Action::Click("names-close"),Action::Into(BAR,"=Rate\u{E007}"),Action::Wait(Check::Text(cell!("A1"),"10")),
            Action::Click("[data-dnacalc=sheet-tools] summary"),Action::Into("sheet-new-name","Other sheet"),Action::Click("sheet-add"),Action::Wait(Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]","Other sheet")),Action::Click("[data-dnacalc=sheet-tools] summary"),
            Action::Into(BAR,"=Rate*3\u{E007}"),Action::Into(BAR,"=LET(Rate,5,Rate+1)\u{E007}"),Action::Into(BAR,"=\"Rate\"\u{E007}"),Action::Into(BAR,"=\"😀\"&Rate\u{E007}"),
            Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("name-new"),Action::Into("name-name","Alias"),Action::Keys("name-scope","\u{E010}\u{E007}"),Action::Into("name-formula","=Rate+1"),Action::Click("name-save"),Action::Wait(Check::Count("[data-name=Alias]",1)),
            Action::Click("[data-name=Rate][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Into("name-name","YieldRate"),Action::Click("name-save"),
        ],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1),Check::Count("[data-name=Rate][data-name-scope=Sheet1]",1),Check::TextContains("[data-name=Alias]","=YieldRate+1"),Check::Text("name-value","Not evaluated"),Check::Text(cell!("A1"),"6"),Check::Text(cell!("A2"),"6"),Check::Text(cell!("A3"),"Rate"),Check::Text(cell!("A4"),"😀2"),Check::PlanCell("A1","provenance","calculated")],
    },
    Case {
        name:"name changes: source tokens and one Undo Redo preserve LET parameters and string literals",
        actions:&[
            Action::Click("names-close"),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::Value(BAR,"=YieldRate*3")),Action::Into("name-box","A2\u{E007}"),Action::Wait(Check::Value(BAR,"=LET(_xlpm.Rate,5,_xlpm.Rate+1)")),Action::Into("name-box","A3\u{E007}"),Action::Wait(Check::Value(BAR,"=\"Rate\"")),Action::Into("name-box","A4\u{E007}"),Action::Wait(Check::Value(BAR,"=\"😀\"&YieldRate")),
            Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("names-undo"),Action::Wait(Check::Count("[data-name=Rate][data-name-scope=Workbook]",1)),Action::Wait(Check::TextContains("[data-name=Alias]","=Rate+1")),Action::Click("names-redo"),
        ],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1),Check::TextContains("[data-name=Alias]","=YieldRate+1"),Check::Text(cell!("A1"),"6"),Check::Text(cell!("A2"),"6"),Check::Text(cell!("A3"),"Rate"),Check::Text(cell!("A4"),"😀2")],
    },
    Case {
        name:"name changes: a combined definition rewrite refuses atomically and retains the draft",
        actions:&[Action::Click("[data-name=YieldRate][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Into("name-name","TrialRate"),Action::Into("name-formula","=9"),Action::Click("name-save")],
        checks:&[Check::TextContains("names-message","Rename or move the name first"),Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1),Check::Count("[data-name=TrialRate]",0),Check::Value("name-name","TrialRate"),Check::Value("name-formula","=9"),Check::Text(cell!("A1"),"6")],
    },
    Case {
        name:"name changes: workbook to worksheet scope qualifies users on another worksheet",
        actions:&[
            Action::Click("names-close"),Action::Into("name-box","Sheet1!B1\u{E007}"),Action::Into(BAR,"=YieldRate+4\u{E007}"),Action::Into("name-box","B1\u{E007}"),
            Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("[data-name=YieldRate][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Keys("name-scope","\u{E010}\u{E007}"),Action::Click("name-save"),
        ],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=\"Other sheet\"]",1),Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",0),Check::Value(BAR,"='Other sheet'!YieldRate+4"),Check::Text(cell!("B1"),"6"),Check::PlanCell("B1","provenance","calculated"),Check::Text("name-value","Not evaluated")],
    },
    Case {
        name:"name changes: a scope move and every qualified formula share one Undo Redo group",
        actions:&[Action::Click("names-undo"),Action::Wait(Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1)),Action::Wait(Check::Value(BAR,"=YieldRate+4")),Action::Click("names-redo")],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=\"Other sheet\"]",1),Check::Value(BAR,"='Other sheet'!YieldRate+4"),Check::Text(cell!("B1"),"6")],
    },
    Case {
        name:"name changes: a worksheet to worksheet move retains the canonical referring value",
        actions:&[Action::Click("[data-name=YieldRate][data-name-scope=\"Other sheet\"] [data-dnacalc=name-select]"),Action::Keys("name-scope","\u{E011}\u{E015}\u{E007}"),Action::Click("name-save")],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=Sheet1]",1),Check::Value(BAR,"='Sheet1'!YieldRate+4"),Check::Text(cell!("B1"),"6"),Check::TextContains("[data-name=Alias]","='Sheet1'!YieldRate+1")],
    },
    Case {
        name:"name changes: returning to workbook scope removes the temporary qualification",
        actions:&[Action::Click("[data-name=YieldRate][data-name-scope=Sheet1] [data-dnacalc=name-select]"),Action::Choose("name-scope",""),Action::Click("name-save")],
        checks:&[Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1),Check::Value(BAR,"=YieldRate+4"),Check::Text(cell!("B1"),"6"),Check::TextContains("[data-name=Alias]","=YieldRate+1"),Check::Count("[data-name=Rate][data-name-scope=Sheet1]",1)],
    },
    Case {
        name:"name changes: destination collisions refuse without replacing either definition",
        actions:&[Action::Click("[data-name=YieldRate][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Into("name-name","Taken"),Action::Click("name-save")],
        checks:&[Check::TextContains("names-message","destination name already exists"),Check::Count("[data-name=YieldRate][data-name-scope=Workbook]",1),Check::TextContains("[data-name=Taken]","=99"),Check::Value(BAR,"=YieldRate+4"),Check::Text(cell!("B1"),"6")],
    },
    Case {
        name:"name changes: renamed definitions and rewritten users survive xlsx while values remain explicit",
        actions:&[
            Action::Click("names-close"),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Wait(Check::Count("[data-dnacalc=name-row]",4)),Action::Click("[data-name=YieldRate][data-name-scope=Workbook] [data-dnacalc=name-select]"),Action::Wait(Check::Text("name-value","Not evaluated")),Action::Click("name-evaluate"),Action::Wait(Check::Text("name-value","2")),Action::Click("names-close"),
            Action::Into("name-box","Sheet1!A1\u{E007}"),Action::Wait(Check::Text(cell!("A1"),"10")),Action::Into("name-box","Sheet1!B1\u{E007}"),Action::Wait(Check::Value(BAR,"=YieldRate+4")),Action::Wait(Check::Text(cell!("B1"),"6")),Action::Into("name-box","'Other sheet'!A1\u{E007}"),Action::Wait(Check::Value(BAR,"=YieldRate*3")),Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("[data-name=Alias] [data-dnacalc=name-select]"),Action::Wait(Check::Text("name-value","Not evaluated")),Action::Click("name-evaluate"),
        ],
        checks:&[Check::Text("name-value","3"),Check::Value("name-formula","=YieldRate+1"),Check::Text(cell!("A1"),"6"),Check::PlanCell("A1","provenance","calculated")],
    },
    Case {
        name:"name changes: editable name and scope remain keyboard reachable on a phone",
        actions:&[Action::Window(390,844),Action::Click("name-name"),Action::Keys("name-name",TAB),Action::Wait(Check::Focused("name-scope")),Action::Wait(Check::Reachable("name-scope")),Action::Wait(Check::Name("name-scope","Scope")),Action::Wait(Check::Fits),Action::Window(1440,1000)],
        checks:&[Check::Focused("name-scope"),Check::Value("name-name","Alias"),Check::Value("name-formula","=YieldRate+1")],
    },

    Case {
        name:"numeric sort: complete rows and cell formats move stably while the header stays",
        actions:&[
            Action::ExternalClipboard("Key\tPayload\n3\tthree\n1\tone first\n1\tone second\n\tblank\n2\ttwo",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("B6"),"two")),
            Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=A2\u{E007}"),
            Action::Into("name-box","A2:B2\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-fill"),Action::Into("format-fill","#ff0000"),Action::Click("format-apply"),
            Action::Into("name-box","A1:B6\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),
        ],
        checks:&[Check::Text(cell!("A1"),"Key"),Check::Text(cell!("B2"),"one first"),Check::Text(cell!("B3"),"one second"),Check::Text(cell!("B4"),"two"),Check::Text(cell!("B5"),"three"),Check::Text(cell!("B6"),"blank"),Check::PlanAbsent("A6"),Check::PlanCell("A5","style.fill","#ff0000"),Check::Text(cell!("D1"),"1"),Check::PlanCell("D1","provenance","calculated")],
    },
    Case {
        name:"numeric sort: one Undo restores the entire order and formats and Redo replays it",
        actions:&[Action::Click("undo"),Action::Wait(Check::Text(cell!("B2"),"three")),Action::Wait(Check::Text(cell!("B6"),"two")),Action::Wait(Check::PlanCell("A2","style.fill","#ff0000")),Action::Wait(Check::Text(cell!("D1"),"3")),Action::Click("redo")],
        checks:&[Check::Text(cell!("B2"),"one first"),Check::Text(cell!("B5"),"three"),Check::PlanCell("A5","style.fill","#ff0000"),Check::Text(cell!("D1"),"1")],
    },
    Case {
        name:"numeric sort: descending keeps blank keys last and preserves equal-key row order",
        actions:&[Action::Click("sort-descending")],
        checks:&[Check::Text(cell!("A2"),"3"),Check::Text(cell!("B2"),"three"),Check::Text(cell!("B3"),"two"),Check::Text(cell!("B4"),"one first"),Check::Text(cell!("B5"),"one second"),Check::Text(cell!("B6"),"blank"),Check::PlanAbsent("A6"),Check::PlanCell("A2","style.fill","#ff0000"),Check::Text(cell!("D1"),"3")],
    },
    Case {
        name:"numeric sort: XLSX reopens the authored order and moved cell formats",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement],
        checks:&[Check::Text(cell!("B2"),"three"),Check::Text(cell!("B4"),"one first"),Check::Text(cell!("B5"),"one second"),Check::Text(cell!("B6"),"blank"),Check::PlanCell("A2","style.fill","#ff0000"),Check::Text(cell!("D1"),"3"),Check::PlanCell("D1","provenance","calculated")],
    },
    Case {
        name:"numeric sort: another selected key column sorts only the selected rectangle",
        actions:&[
            Action::Into("name-box","F1\u{E007}"),Action::ExternalClipboard("right\t2\nleft\t1",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("G2"),"1")),
            Action::Into("name-box","F1:G2\u{E007}"),Action::OpenDetails("sort-tools"),Action::Into("sort-column","2"),Action::Click("sort-header"),Action::Click("sort-ascending"),
        ],
        checks:&[Check::Text(cell!("F1"),"left"),Check::Text(cell!("G1"),"1"),Check::Text(cell!("F2"),"right"),Check::Text(cell!("G2"),"2"),Check::Text(cell!("B2"),"three")],
    },
    Case {
        name:"numeric sort: phone controls wrap with accessible names and retain keyboard access",
        actions:&[Action::Window(390,844),Action::OpenDetails("sort-tools"),Action::Click("sort-column"),Action::Keys("sort-column",TAB)],
        checks:&[Check::Fits,Check::Narrow(390),Check::Name("sort-column","Column within selection"),Check::Reachable("sort-header"),Check::Focused("sort-header"),Check::Reachable("sort-ascending"),Check::Reachable("sort-descending")],
    },
    Case {
        name:"sort formulas: ordinary relative formulas move and recalculate with their rows",
        actions:&[Action::Window(1440,1000),Action::Into(BAR,"3\u{E007}"),Action::Into("name-box","A2\u{E007}"),Action::Into(BAR,"1\u{E007}"),Action::Into("name-box","B1\u{E007}"),Action::Into(BAR,"=A1+1\u{E007}"),Action::Into("name-box","A1:B2\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-header"),Action::Click("sort-ascending")],
        checks:&[Check::Text(cell!("A1"),"1"),Check::Text(cell!("A2"),"3"),Check::Text(cell!("B2"),"4"),Check::PlanCell("B2","formula","true"),Check::PlanCell("B2","provenance","calculated")],
    },
    Case {
        name:"sort mixed keys: numbers precede text and literal rows stay together",
        actions:&[Action::Into("name-box","B1\u{E007}"),Action::Into(BAR,"payload\u{E007}"),Action::Into("name-box","A2\u{E007}"),Action::Into(BAR,"text\u{E007}"),Action::Into("name-box","A1:B2\u{E007}"),Action::Click("sort-ascending")],
        checks:&[Check::Text(cell!("A1"),"1"),Check::Text(cell!("A2"),"text"),Check::Text(cell!("B1"),"payload")],
    },
    Case {
        name:"numeric sort limits: array cells refuse without flattening the source",
        actions:&[Action::Keys("grid",DELETE),Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"=SEQUENCE(2,2)\u{E007}"),Action::Into("name-box","A1:B2\u{E007}"),Action::Click("sort-ascending")],
        checks:&[Check::TextContains("status","Sorting array formulas"),Check::Text(cell!("A1"),"1"),Check::Text(cell!("B2"),"4"),Check::PlanCell("B2","provenance","calculated")],
    },

    Case {
        name:"shrink: text and formatted formula values fit without changing their authored font",
        actions:&[
            Action::Into(BAR,"ABCDEFGHIJKL\u{E007}"),Action::Into(BAR,"=123456\u{E007}"),Action::Into("name-box","A1:A2\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Into("format-number","0.00"),Action::Click("format-tab-font"),Action::Into("format-font","monospace"),Action::Into("format-size","18"),Action::Click("format-tab-alignment"),Action::Choose("format-shrink","true"),Action::Click("format-apply"),
        ],
        checks:&[Check::Text(cell!("A1"),"ABCDEFGHIJKL"),Check::Text(cell!("A2"),"123456.00"),Check::PlanCell("A2","provenance","calculated"),Check::PlanCell("A1","style.shrink","true"),Check::PlanCell("A1","style.pixels","24"),Check::FitsText(cell!("A1")),Check::FitsText(cell!("A2")),Check::Spill(cell!("A1"),"A","A")],
    },
    Case {
        name:"shrink: one format Undo Redo and the dialog retain the original 18 point font",
        actions:&[Action::Click("undo"),Action::Wait(Check::PlanCell("A1","style.shrink","false")),Action::Click("redo"),Action::Wait(Check::PlanCell("A1","style.shrink","true")),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Wait(Check::Value("format-size","18")),Action::Click("format-tab-alignment"),Action::Wait(Check::Value("format-shrink","true")),Action::Click("format-cancel")],
        checks:&[Check::Text(cell!("A1"),"ABCDEFGHIJKL"),Check::FitsText(cell!("A1")),Check::FitsText(cell!("A2")),Check::PlanCell("A1","style.pixels","24")],
    },
    Case {
        name:"shrink: Canvas paints the same fitted values and zoom keeps the complete text",
        actions:&[Action::OpenDetails("pane-tools"),Action::Wait(Check::PlanReady),Action::RememberPresentation,Action::SetPainter("canvas"),Action::Wait(Check::PainterStable),Action::Wait(Check::CanvasInk("A1")),Action::Wait(Check::CanvasInk("A2")),Action::SetPainter("dom"),Action::Into("zoom-percent","200\u{E007}"),Action::Wait(Check::FitsText(cell!("A1"))),Action::Wait(Check::FitsText(cell!("A2"))),Action::Click("zoom-reset")],
        checks:&[Check::Text(cell!("A2"),"123456.00"),Check::FitsText(cell!("A1")),Check::FitsText(cell!("A2")),Check::PlanCell("A1","style.pixels","24")],
    },
    Case {
        name:"shrink: widening restores the full display font and width history recalculates the fit",
        actions:&[Action::OpenDetails("axis-tools"),Action::Into("row-size","30"),Action::Click("set-row-size"),Action::Into("column-size","40"),Action::Click("set-column-size"),Action::Wait(Check::Computed(cell!("A1"),"font-size:24px")),Action::Click("undo"),Action::Wait(Check::FitsText(cell!("A1"))),Action::Click("redo")],
        checks:&[Check::Computed(cell!("A1"),"font-size:24px"),Check::FitsText(cell!("A1")),Check::FitsText(cell!("A2")),Check::Text(cell!("A2"),"123456.00")],
    },
    Case {
        name:"shrink: XLSX retains the setting and the phone dialog exposes the original font",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Into("name-box","A1:A2\u{E007}"),Action::OpenDetails("axis-tools"),Action::Into("column-size","8.43"),Action::Click("set-column-size"),Action::Window(390,844),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Wait(Check::Value("format-size","18")),Action::Click("format-tab-alignment"),Action::Wait(Check::Value("format-shrink","true")),Action::Wait(Check::Reachable("format-shrink")),Action::Wait(Check::Name("format-shrink","Shrink to fit")),Action::Wait(Check::Fits),Action::Click("format-cancel"),Action::Window(1440,1000)],
        checks:&[Check::PlanCell("A1","style.shrink","true"),Check::PlanCell("A1","style.pixels","24"),Check::FitsText(cell!("A1")),Check::FitsText(cell!("A2")),Check::Text(cell!("A2"),"123456.00"),Check::PlanCell("A2","provenance","calculated")],
    },

    Case {
        name:"AutoFilter: range headers and independent manual row hiding use the workbook command",
        actions:&[
            Action::ExternalClipboard("Category\tAmount\nAlpha\t10\nBeta\t=B2*2\nAlpha\t30\nBeta\t40\n\t50",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("B6"),"50")),
            Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=SUBTOTAL(9,B2:B6)\u{E007}"),Action::Into("name-box","E1\u{E007}"),Action::Into(BAR,"=SUBTOTAL(109,B2:B6)\u{E007}"),
            Action::Into("name-box","A4\u{E007}"),Action::OpenDetails("axis-tools"),Action::Click("hide-rows"),Action::Wait(Check::PlanAbsent("A4")),
            Action::Into("name-box","A1:B6\u{E007}"),Action::OpenDetails("filter-tools"),Action::Click("filter-enable"),
        ],
        checks:&[Check::Count("filter-header",2),Check::TextContains("filter-summary","0 rows filtered out"),Check::Text(cell!("D1"),"150"),Check::Text(cell!("E1"),"120"),Check::PlanCell("B3","provenance","calculated")],
    },
    Case {
        name:"AutoFilter: a header opens choices and selected values exclude rows without flattening formulas",
        actions:&[Action::Keys("[data-dnacalc=filter-header][data-filter-column=\"1\"]",ENTER),Action::Wait(Check::Count("filter-choice",2)),Action::Click("[data-dnacalc=filter-choice][data-filter-value=Beta]"),Action::Click("filter-blank"),Action::Click("filter-apply")],
        checks:&[Check::TextContains("filter-summary","3 rows filtered out"),Check::PlanAbsent("A3"),Check::PlanAbsent("A4"),Check::PlanAbsent("A5"),Check::PlanAbsent("B6"),Check::Text(cell!("B2"),"10"),Check::Text(cell!("D1"),"40"),Check::Text(cell!("E1"),"10")],
    },
    Case {
        name:"AutoFilter: one Undo restores criteria and visibility and Redo applies both",
        actions:&[Action::Click("undo"),Action::Wait(Check::TextContains("filter-summary","0 rows filtered out")),Action::Wait(Check::Text(cell!("B3"),"20")),Action::Wait(Check::Text(cell!("D1"),"150")),Action::Click("redo")],
        checks:&[Check::TextContains("filter-summary","3 rows filtered out"),Check::Text(cell!("D1"),"40"),Check::Text(cell!("E1"),"10"),Check::PlanAbsent("A3")],
    },
    Case {
        name:"AutoFilter: XLSX retains the criteria and two independent causes of hidden rows",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("filter-tools")],
        checks:&[Check::Count("filter-header",2),Check::TextContains("filter-summary","3 rows filtered out"),Check::PlanAbsent("A3"),Check::PlanAbsent("A4"),Check::Text(cell!("D1"),"40"),Check::Text(cell!("E1"),"10")],
    },
    Case {
        name:"AutoFilter: Clear restores filtered rows while preserving manual hiding and formula provenance",
        actions:&[Action::Click("filter-clear")],
        checks:&[Check::TextContains("filter-summary","0 rows filtered out"),Check::PlanAbsent("A4"),Check::Text(cell!("B3"),"20"),Check::PlanCell("B3","provenance","calculated"),Check::Text(cell!("D1"),"150"),Check::Text(cell!("E1"),"120")],
    },
    Case {
        name:"AutoFilter: two custom comparisons combine on a calculated numeric column",
        actions:&[Action::Into("filter-column","2"),Action::Click("filter-load"),Action::Wait(Check::Count("filter-choice",5)),Action::Wait(Check::Count("[data-dnacalc=filter-choice][data-filter-value=\"20\"]",1)),Action::OpenDetails("filter-custom-tools"),Action::Choose("filter-operator","greaterThanOrEqual"),Action::Into("filter-value","20"),Action::Choose("filter-operator-2","lessThanOrEqual"),Action::Into("filter-value-2","40"),Action::Click("filter-custom-apply")],
        checks:&[Check::TextContains("filter-summary","2 rows filtered out"),Check::PlanAbsent("A2"),Check::PlanAbsent("B6"),Check::PlanAbsent("A4"),Check::Text(cell!("B3"),"20"),Check::PlanCell("B3","provenance","calculated"),Check::Text(cell!("D1"),"90"),Check::Text(cell!("E1"),"60"),Check::PlanAbsent("D2")],
    },
    Case {
        name:"AutoFilter: phone controls wrap and remain named and keyboard reachable",
        actions:&[Action::Window(390,844),Action::OpenDetails("filter-tools"),Action::Click("filter-column"),Action::Keys("filter-column",TAB)],
        checks:&[Check::Fits,Check::Narrow(390),Check::Name("filter-column","Column within filter range"),Check::Focused("filter-load"),Check::Reachable("filter-load"),Check::Reachable("filter-clear")],
    },
    Case {
        name:"AutoFilter: removing the filter restores excluded rows and retains manual hiding",
        actions:&[Action::Window(1440,1000),Action::Click("filter-remove")],
        checks:&[Check::Count("filter-header",0),Check::TextContains("filter-summary","Select one rectangle"),Check::PlanAbsent("A4"),Check::Text(cell!("B2"),"10"),Check::Text(cell!("B3"),"20"),Check::Text(cell!("D1"),"150"),Check::Text(cell!("E1"),"120")],
    },

    Case {
        name:"general sort: text collation and multiple keys move formulas with their rows",
        actions:&[
            Action::ExternalClipboard("Category\tScore\tTotal\nBeta\t2\t=B2*2\nalpha\t3\t=B3*2\nAlpha\t1\t=B4*2\nco-op\t4\t=B5*2\ncoop\t5\t=B6*2",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("C6"),"10")),
            Action::Into("name-box","A1:C6\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-add-key"),Action::Wait(Check::Count("sort-level-order-2",1)),Action::Choose("sort-level-order-2","desc"),Action::Click("sort-ascending"),
        ],
        checks:&[Check::Text(cell!("A2"),"alpha"),Check::Text(cell!("A3"),"Alpha"),Check::Text(cell!("A4"),"Beta"),Check::Text(cell!("A5"),"coop"),Check::Text(cell!("A6"),"co-op"),Check::Text(cell!("C2"),"6"),Check::Text(cell!("C3"),"2"),Check::Text(cell!("C4"),"4"),Check::PlanCell("C2","formula","true"),Check::PlanCell("C2","provenance","calculated")],
    },
    Case {
        name:"general sort: one history group restores row order and relocated formulas",
        actions:&[Action::Click("undo"),Action::Wait(Check::Text(cell!("A2"),"Beta")),Action::Wait(Check::Text(cell!("C2"),"4")),Action::Click("redo")],
        checks:&[Check::Text(cell!("A2"),"alpha"),Check::Text(cell!("C2"),"6"),Check::Text(cell!("A6"),"co-op"),Check::Text(cell!("C6"),"8"),Check::PlanCell("C6","provenance","calculated")],
    },
    Case {
        name:"general sort: real XLSX reopen retains moved formulas and calculated values",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement],
        checks:&[Check::Text(cell!("A2"),"alpha"),Check::Text(cell!("A5"),"coop"),Check::Text(cell!("C2"),"6"),Check::Text(cell!("C5"),"10"),Check::PlanCell("C5","formula","true"),Check::PlanCell("C5","provenance","calculated")],
    },
    Case {
        name:"general sort: each key has its own direction after reopen",
        actions:&[Action::Into("name-box","A1:C6\u{E007}"),Action::OpenDetails("sort-tools"),Action::Wait(Check::Count("sort-level-order-2",1)),Action::Choose("sort-level-order-2","asc"),Action::Click("sort-descending")],
        checks:&[Check::Text(cell!("A2"),"co-op"),Check::Text(cell!("A3"),"coop"),Check::Text(cell!("A4"),"Beta"),Check::Text(cell!("A5"),"Alpha"),Check::Text(cell!("A6"),"alpha"),Check::Text(cell!("C5"),"2"),Check::Text(cell!("C6"),"6")],
    },
    Case {
        name:"general sort: duplicate keys refuse atomically and extra keys can be removed",
        actions:&[Action::Into("sort-level-column-2","1"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","each sort column only once")),Action::Click("sort-level-remove-2")],
        checks:&[Check::Text(cell!("A2"),"co-op"),Check::Text(cell!("C2"),"8"),Check::Count("sort-level-column-2",0)],
    },
    Case {
        name:"general sort: phone access includes multiple key columns and independent order",
        actions:&[Action::Window(390,844),Action::OpenDetails("sort-tools"),Action::Click("sort-add-key"),Action::Wait(Check::Count("sort-level-order-2",1)),Action::Click("sort-level-column-2"),Action::Keys("sort-level-column-2",TAB)],
        checks:&[Check::Fits,Check::Narrow(390),Check::Name("sort-level-column-2","Then column 2"),Check::Focused("sort-level-order-2"),Check::Reachable("sort-level-order-2"),Check::Reachable("sort-level-remove-2"),Check::Reachable("sort-add-key")],
    },

    Case {
        name: "session view: authored splits create four independent worksheet views",
        actions: &[
            Action::Keys("grid", "top-left\u{E007}"),
            Action::Wait(Check::Text(cell!("A1"), "top-left")),
            Action::Into("name-box", "H20\u{E007}"),
            Action::OpenDetails("pane-tools"),
            Action::Into("split-x", "120"),
            Action::Into("split-y", "90"),
            Action::Click("split-apply"),
        ],
        checks: &[Check::Count("grid", 4), Check::Reachable("split-remove")],
    },
    Case {
        name: "session view: remove split and one Undo restore the complete layout",
        actions: &[
            Action::OpenDetails("pane-tools"),
            Action::Click("split-remove"),
            Action::Wait(Check::Count("grid", 1)),
            Action::Click("undo"),
        ],
        checks: &[Check::Count("grid", 4)],
    },
    Case {
        name: "session view: split origins zoom and active pane survive actual XLSX reopen",
        actions: &[
            Action::Click("[data-pane=\"0\"][data-dnacalc=grid]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("[data-pane=\"3\"][data-dnacalc=grid]"),
            Action::Into("name-box", "M100\u{E007}"),
            Action::Into("zoom-percent", "175\u{E007}"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("Book1.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Count("grid", 4),
            Check::Value("name-box", "M100"),
            Check::Value("zoom-percent", "175"),
            Check::Attr("[data-pane=\"3\"]", "data-grid-active", "true"),
            Check::Text("[data-pane=\"0\"] [data-dnacalc=cell][data-cell=A1]", "top-left"),
        ],
    },
    Case {
        name: "session view: pane controls fit a phone",
        actions: &[Action::Window(390, 844), Action::OpenDetails("pane-tools")],
        checks: &[
            Check::Fits,
            Check::Narrow(390),
            Check::Name("split-x", "Horizontal split position in points"),
            Check::Reachable("split-apply"),
            Check::Reachable("split-remove"),
        ],
    },
    Case {
        name: "session view: imported active sheet opens and a chosen sheet survives XLSX reopen",
        actions: &[
            Action::Window(1280, 1000),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=\"Target Sheet\"]",
                "aria-selected",
                "true",
            )),
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Other]"),
            Action::Wait(Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            )),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("cut-base.xlsx"),
            Action::DiscardReplacement,
        ],
        checks: &[
            Check::Attr(
                "[data-dnacalc=sheet-tab][data-sheet=Other]",
                "aria-selected",
                "true",
            ),
            Check::Text(cell!("A1"), "100"),
        ],
    },

    Case {
        name:"outline: selected detail rows form a group with a visible summary control",
        actions:&[Action::ExternalClipboard("Amount\n10\n20\n30\n40\n50\n=SUM(A2:A6)",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("A7"),"150")),Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=SUBTOTAL(9,A2:A6)\u{E007}"),Action::Into("name-box","E1\u{E007}"),Action::Into(BAR,"=SUBTOTAL(109,A2:A6)\u{E007}"),Action::Into("name-box","A2:A6\u{E007}"),Action::OpenDetails("axis-tools"),Action::Click("outline-group"),Action::Click("[data-dnacalc=axis-tools] > summary")],
        checks:&[Check::Count("outline-toggle",1),Check::Count("outline-gutter",1),Check::Attr("outline-toggle","aria-expanded","true"),Check::Text(cell!("D1"),"150"),Check::Text(cell!("E1"),"150")],
    },
    Case {
        name:"outline: nested collapse retains independently hidden detail and calculated totals",
        actions:&[Action::Into("name-box","A3\u{E007}"),Action::OpenDetails("axis-tools"),Action::Click("hide-rows"),Action::Wait(Check::PlanAbsent("A3")),Action::Into("name-box","A3:A4\u{E007}"),Action::Click("outline-group"),Action::Wait(Check::Count("outline-toggle",2)),Action::Click("[data-dnacalc=outline-toggle][data-first=\"3\"]")],
        checks:&[Check::PlanAbsent("A3"),Check::PlanAbsent("A4"),Check::Text(cell!("A2"),"10"),Check::Text(cell!("D1"),"150"),Check::Text(cell!("E1"),"100"),Check::Attr("[data-dnacalc=outline-toggle][data-first=\"3\"]","aria-expanded","false")],
    },
    Case {
        name:"outline: outer collapse Undo Redo and expansion preserve the nested state",
        actions:&[Action::Click("[data-dnacalc=outline-toggle][data-first=\"2\"]"),Action::Wait(Check::Text(cell!("E1"),"0")),Action::Click("undo"),Action::Wait(Check::Text(cell!("E1"),"100")),Action::Click("redo"),Action::Wait(Check::Text(cell!("E1"),"0")),Action::Click("[data-dnacalc=outline-gutter][data-first=\"2\"]")],
        checks:&[Check::Text(cell!("A2"),"10"),Check::PlanAbsent("A4"),Check::Text(cell!("E1"),"100"),Check::PlanCell("A7","provenance","calculated")],
    },
    Case {
        name:"outline: summary above and independent manual hiding survive XLSX reopen",
        actions:&[Action::OpenDetails("axis-tools"),Action::Click("outline-summary-after"),Action::Wait(Check::Attr("[data-dnacalc=outline-toggle][data-first=\"3\"]","aria-expanded","false")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("axis-tools"),Action::Wait(Check::Count("outline-toggle",2)),Action::Click("[data-dnacalc=outline-level][data-level=\"3\"]"),Action::Click("[data-dnacalc=axis-tools] > summary")],
        checks:&[Check::Text(cell!("A4"),"30"),Check::PlanAbsent("A3"),Check::Text(cell!("E1"),"130"),Check::Text(cell!("A7"),"150"),Check::Count("[data-dnacalc=outline-summary-after]:not(:checked)",1)],
    },
    Case {
        name:"outline: level controls and column groups share the authored visibility route",
        actions:&[Action::OpenDetails("axis-tools"),Action::Click("[data-dnacalc=outline-level][data-level=\"1\"]"),Action::Wait(Check::PlanAbsent("A2")),Action::Wait(Check::Text(cell!("E1"),"0")),Action::Click("[data-dnacalc=outline-level][data-level=\"3\"]"),Action::Wait(Check::Text(cell!("E1"),"130")),Action::Into("name-box","G1\u{E007}"),Action::Into(BAR,"column detail\u{E007}"),Action::Into("name-box","G1:I1\u{E007}"),Action::OpenDetails("axis-tools"),Action::Choose("outline-axis","columns"),Action::Wait(Check::Attr("selection","data-area","G1:I1")),Action::Wait(Check::ActiveSemantic("G1")),Action::Click("outline-group"),Action::Wait(Check::Count("outline-toggle",1)),Action::Wait(Check::Attr("outline-toggle","data-first","7")),Action::Wait(Check::Attr("outline-toggle","data-last","9")),Action::Click("outline-toggle"),Action::Click("[data-dnacalc=axis-tools] > summary")],
        checks:&[Check::PlanAbsent("G1"),Check::Attr("outline-toggle","aria-expanded","false"),Check::Text(cell!("A7"),"150")],
    },
    Case {
        name:"outline: column XLSX reopen and ungroup controls fit a phone",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("axis-tools"),Action::Choose("outline-axis","columns"),Action::Wait(Check::Attr("outline-toggle","aria-expanded","false")),Action::Click("outline-toggle"),Action::Wait(Check::Text(cell!("G1"),"column detail")),Action::Into("name-box","G1:I1\u{E007}"),Action::Wait(Check::Attr("selection","data-area","G1:I1")),Action::Wait(Check::ActiveSemantic("G1")),Action::Click("outline-ungroup"),Action::Wait(Check::Count("outline-toggle",0)),Action::Click("undo"),Action::Wait(Check::Count("outline-toggle",1)),Action::Window(390,844),Action::Keys("outline-axis","\u{E004}\u{E004}\u{E004}\u{E004}\u{E004}")],
        checks:&[Check::Text(cell!("G1"),"column detail"),Check::Fits,Check::Narrow(390),Check::Reachable("outline-axis"),Check::Reachable("outline-group"),Check::Focused("outline-summary-after"),Check::Reachable("outline-summary-after")],
    },
    Case {
        name:"date and top filters: top two uses numeric values and includes ties",
        actions:&[Action::ExternalClipboard("Date\tAmount\n=DATE(2026,11,1)\t10\n=DATE(2026,11,3)+TIME(14,30,0)\t50\n=DATE(2025,11,3)\t50\n=DATE(2026,12,1)\t20\ntext\t99\n\t30",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::PlanCell("B7","text","30")),Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=SUBTOTAL(109,B2:B7)\u{E007}"),Action::Into("name-box","A1:B7\u{E007}"),Action::OpenDetails("filter-tools"),Action::Click("filter-enable"),Action::Wait(Check::TextContains("filter-summary","A1:B7")),Action::Into("filter-column","2"),Action::OpenDetails("filter-top-tools"),Action::Into("filter-top-count","2"),Action::Click("filter-top-apply")],
        checks:&[Check::PlanCell("D1","text","199"),Check::PlanCell("D1","provenance","calculated"),Check::PlanAbsent("A2"),Check::PlanCell("B3","text","50"),Check::PlanCell("B4","text","50"),Check::PlanCell("B6","text","99"),Check::TextContains("filter-summary","3 rows filtered out")],
    },
    Case {
        name:"date and top filters: Undo Redo restores the entire cutoff application",
        actions:&[Action::Click("undo"),Action::Wait(Check::PlanCell("D1","text","259")),Action::Click("redo")],
        checks:&[Check::PlanCell("D1","text","199"),Check::PlanAbsent("A2"),Check::TextContains("filter-summary","3 rows filtered out")],
    },
    Case {
        name:"date and top filters: month groups combine with a cutoff over the whole numeric column",
        actions:&[Action::Into("filter-column","1"),Action::OpenDetails("filter-date-tools"),Action::Into("filter-dates","2026-11"),Action::Click("filter-date-apply")],
        checks:&[Check::PlanCell("D1","text","50"),Check::PlanCell("B3","text","50"),Check::PlanAbsent("A2"),Check::PlanAbsent("A4"),Check::TextContains("filter-summary","5 rows filtered out")],
    },
    Case {
        name:"date and top filters: both criteria and their hidden rows survive real XLSX reopening",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("filter-tools"),Action::Click("filter-reapply")],
        checks:&[Check::PlanCell("D1","text","50"),Check::PlanCell("D1","provenance","calculated"),Check::PlanAbsent("A2"),Check::TextContains("filter-summary","5 rows filtered out")],
    },
    Case {
        name:"date and top filters: date-time precision and a percentage use the same authored commands",
        actions:&[Action::Click("filter-clear"),Action::Into("filter-column","1"),Action::OpenDetails("filter-date-tools"),Action::Into("filter-dates","2026-11-03T14:30"),Action::Click("filter-date-apply"),Action::Wait(Check::PlanCell("D1","text","50")),Action::Click("filter-clear"),Action::Into("filter-column","2"),Action::OpenDetails("filter-top-tools"),Action::Choose("filter-top-direction","top"),Action::Choose("filter-top-unit","percent"),Action::Into("filter-top-count","50"),Action::Click("filter-top-apply")],
        checks:&[Check::PlanCell("D1","text","199"),Check::TextContains("filter-summary","3 rows filtered out")],
    },
    Case {
        name:"date and top filters: bottom items reject invalid requests without changing calculated visibility",
        actions:&[Action::Choose("filter-top-direction","bottom"),Action::Choose("filter-top-unit","items"),Action::Into("filter-top-count","1"),Action::Click("filter-top-apply"),Action::Wait(Check::PlanCell("D1","text","10")),Action::Into("filter-top-count","0"),Action::Click("filter-top-apply"),Action::Window(390,844)],
        checks:&[Check::PlanCell("D1","text","10"),Check::TextContains("status","criteria exceed supported limits"),Check::Fits,Check::Narrow(390),Check::TextContains("filter-summary","5 rows filtered out")],
    },

    Case {
        name:"colour filters: solid fill criteria use authored styles and include every matching row",
        actions:&[Action::Window(1440,1000),Action::Click("filter-clear"),Action::Click("[data-dnacalc=filter-tools] > summary"),Action::Into("name-box","B3\u{E007}"),Action::Click("format-cells"),Action::Click("format-tab-fill"),Action::Into("format-fill","#ff0000"),Action::Click("format-apply"),Action::Into("name-box","B6\u{E007}"),Action::Click("format-cells"),Action::Click("format-tab-fill"),Action::Into("format-fill","#ff0000"),Action::Click("format-apply"),Action::Into("name-box","B4\u{E007}"),Action::Click("format-cells"),Action::Click("format-tab-font"),Action::Into("format-font-color","#ff0000"),Action::Click("format-apply"),Action::OpenDetails("filter-tools"),Action::Into("filter-column","2"),Action::OpenDetails("filter-color-tools"),Action::Choose("filter-color-kind","fill"),Action::Into("filter-color","#ff0000"),Action::Click("filter-color-apply")],
        checks:&[Check::PlanCell("D1","text","149"),Check::PlanCell("D1","provenance","calculated"),Check::PlanCell("B3","style.fill","#ff0000"),Check::PlanCell("B6","text","99"),Check::PlanAbsent("B4"),Check::TextContains("filter-summary","4 rows filtered out")],
    },
    Case {
        name:"colour filters: one history step and actual XLSX reopen retain the differential colour reference",
        actions:&[Action::Click("undo"),Action::Wait(Check::PlanCell("D1","text","259")),Action::Click("redo"),Action::Wait(Check::PlanCell("D1","text","149")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("filter-tools"),Action::Click("filter-reapply")],
        checks:&[Check::PlanCell("D1","text","149"),Check::PlanCell("D1","provenance","calculated"),Check::PlanAbsent("B4"),Check::TextContains("filter-summary","4 rows filtered out")],
    },
    Case {
        name:"colour filters: font colour and no fill use the same command on a phone",
        actions:&[Action::Into("filter-column","2"),Action::OpenDetails("filter-color-tools"),Action::Choose("filter-color-kind","font"),Action::Into("filter-color","#ff0000"),Action::Click("filter-color-apply"),Action::Wait(Check::PlanCell("D1","text","50")),Action::Choose("filter-color-kind","fill"),Action::Into("filter-color",""),Action::Click("filter-color-apply"),Action::Window(390,844)],
        checks:&[Check::PlanCell("D1","text","110"),Check::PlanCell("B4","style.font_color","#ff0000"),Check::PlanAbsent("B3"),Check::PlanAbsent("B6"),Check::TextContains("filter-summary","2 rows filtered out"),Check::Fits,Check::Narrow(390)],
    },

    Case {
        name:"qualified sort: fixed other-sheet cells remain fixed while local formulas follow their rows",
        actions:&[
            Action::Click("[data-dnacalc=sheet-tools] summary"),Action::Into("sheet-new-name","Tax rates"),Action::Click("sheet-add"),Action::Wait(Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]","Tax rates")),Action::Click("[data-dnacalc=sheet-tools] summary"),Action::Into(BAR,"10\u{E007}"),Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=Sheet1!B2\u{E007}"),Action::Click("[data-dnacalc=sheet-tab][data-sheet=Sheet1]"),Action::Wait(Check::Text("[data-dnacalc=sheet-tab][aria-selected=true]","Sheet1")),Action::Into("name-box","A1\u{E007}"),
            Action::ExternalClipboard("Key\tAmount\tTotal\n3\t30\t=B2*'Tax rates'!$A$1\n1\t10\t=B3*'Tax rates'!$A$1\n2\t20\t=B4*'Tax rates'!$A$1",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::PlanCell("C4","text","200")),Action::Into("name-box","E1\u{E007}"),Action::Into(BAR,"=B2\u{E007}"),Action::Into("name-box","A1:C4\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),Action::Wait(Check::PlanCell("C2","text","100")),Action::Into("name-box","C2\u{E007}"),
        ],
        checks:&[Check::Text(cell!("A2"),"1"),Check::Text(cell!("C4"),"300"),Check::Text(cell!("E1"),"10"),Check::PlanCell("C2","provenance","calculated"),Check::Value(BAR,"=B2*'Tax rates'!$A$1")],
    },
    Case {
        name:"qualified sort: Undo and Redo restore formulas and dependent calculations together",
        actions:&[Action::Click("undo"),Action::Wait(Check::PlanCell("C2","text","300")),Action::Wait(Check::PlanCell("E1","text","30")),Action::Click("redo")],
        checks:&[Check::Text(cell!("C2"),"100"),Check::Text(cell!("C4"),"300"),Check::Text(cell!("E1"),"10")],
    },
    Case {
        name:"qualified sort: actual XLSX reopening and reverse sort retain fixed coordinates",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Into("name-box","A1:C4\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-descending"),Action::Wait(Check::PlanCell("C2","text","300")),Action::Into("name-box","C2\u{E007}")],
        checks:&[Check::Text(cell!("A2"),"3"),Check::Text(cell!("E1"),"30"),Check::Value(BAR,"=B2*'Tax rates'!$A$1"),Check::PlanCell("C2","provenance","calculated")],
    },
    Case {
        name:"qualified sort: relative qualifiers refuse without changing row order",
        actions:&[Action::Into(BAR,"=B2*'Tax rates'!A1\u{E007}"),Action::Into("name-box","A1:C4\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","qualified or unsupported formula references")),Action::Into("name-box","C2\u{E007}")],
        checks:&[Check::Text(cell!("A2"),"3"),Check::Text(cell!("C2"),"300"),Check::Text(cell!("E1"),"30"),Check::Value(BAR,"=B2*'Tax rates'!A1")],
    },
    Case {
        name:"qualified sort: expanded outlines refuse atomically until their grouping is undone",
        actions:&[Action::Click("undo"),Action::Into("name-box","A2:A3\u{E007}"),Action::OpenDetails("axis-tools"),Action::Click("outline-group"),Action::Wait(Check::Count("outline-toggle",1)),Action::Into("name-box","A1:C4\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","outline")),Action::Wait(Check::PlanCell("A2","text","3")),Action::Click("undo"),Action::Wait(Check::Count("outline-toggle",0)),Action::Into("name-box","A1:C4\u{E007}"),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","Sorted"))],
        checks:&[Check::Text(cell!("A2"),"1"),Check::Text(cell!("C2"),"100"),Check::Text(cell!("E1"),"10"),Check::Count("outline-toggle",0)],
    },

    Case {
        name:"conditional authoring: cell-value conditions paint through the shared command",
        actions:&[
            Action::ExternalClipboard("1\n2\n3\n4\n5\n6\n7\n8\n9\n10",false),Action::Keys("grid","\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("A10"),"10")),Action::Into("name-box","A1:A10\u{E007}"),Action::Click("review-tools"),
            Action::Choose("rule-kind","cellIs"),Action::Into("rule-new-formulas","6"),Action::Into("rule-color","123456"),Action::Click("rule-add"),
        ],
        checks:&[Check::Count("rule-row",1),Check::Style("[data-dnacalc=cell-decoration][data-cell=A7]","background-color:#123456"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=A1]")],
    },
    Case {
        name:"conditional authoring: Undo and Redo restore both the condition and its fill",
        actions:&[
            Action::Click("review-undo"),Action::Wait(Check::Count("rule-row",0)),Action::Wait(Check::Absent("[data-dnacalc=cell-decoration][data-cell=A7]")),
            Action::Click("review-redo"),
        ],
        checks:&[Check::Count("rule-row",1),Check::Style("[data-dnacalc=cell-decoration][data-cell=A7]","background-color:#123456")],
    },
    Case {
        name:"conditional authoring: a relative formula highlights its own rows",
        actions:&[
            Action::Click("review-undo"),Action::Wait(Check::Count("rule-row",0)),Action::Choose("rule-kind","expression"),Action::Into("rule-new-formulas","A1>=8"),Action::Click("rule-add"),
        ],
        checks:&[Check::Count("rule-row",1),Check::Style("[data-dnacalc=cell-decoration][data-cell=A8]","background-color:#123456"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=A7]")],
    },
    Case {
        name:"conditional authoring: top-percent conditions include the highest values",
        actions:&[
            Action::Click("review-undo"),Action::Wait(Check::Count("rule-row",0)),Action::Choose("rule-kind","top10"),Action::Into("rule-rank","20"),Action::Click("rule-percent"),Action::Click("rule-add"),
        ],
        checks:&[Check::Count("rule-row",1),Check::Style("[data-dnacalc=cell-decoration][data-cell=A9]","background-color:#123456"),Check::Style("[data-dnacalc=cell-decoration][data-cell=A10]","background-color:#123456"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=A8]")],
    },
    Case {
        name:"conditional authoring: the added condition and differential fill survive XLSX",
        actions:&[
            Action::Click("review-close"),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,
            Action::Wait(Check::Text(cell!("A10"),"10")),Action::Click("review-tools"),
        ],
        checks:&[Check::Count("rule-row",1),Check::Style("[data-dnacalc=cell-decoration][data-cell=A9]","background-color:#123456"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=A8]")],
    },
    Case {
        name:"conditional authoring: average conditions use the published workbook values",
        actions:&[
            Action::Choose("rule-kind","belowAverage"),Action::Into("rule-applies","A1:A10"),Action::Into("rule-color","56789a"),Action::Click("rule-add"),
        ],
        checks:&[Check::Count("rule-row",2),Check::Style("[data-dnacalc=cell-decoration][data-cell=A5]","background-color:#56789a"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=A6]")],
    },
    Case {
        name:"conditional authoring: duplicate text matches case and updates with ordinary edits",
        actions:&[
            Action::Click("review-close"),Action::ExternalClipboard("North\nnorth\nSouth",false),Action::Into("name-box","B1\u{E007}"),Action::Keys("grid","\u{E009}v\u{E000}"),
            Action::Wait(Check::Text(cell!("B3"),"South")),Action::Into("name-box","B1:B3\u{E007}"),Action::Click("review-tools"),
            Action::Choose("rule-kind","duplicateValues"),Action::Into("rule-color","56789a"),Action::Click("rule-add"),
        ],
        checks:&[Check::Count("rule-row",3),Check::Style("[data-dnacalc=cell-decoration][data-cell=B1]","background-color:#56789a"),Check::Style("[data-dnacalc=cell-decoration][data-cell=B2]","background-color:#56789a"),Check::Absent("[data-dnacalc=cell-decoration][data-cell=B3]")],
    },
    Case {
        name:"conditional authoring: Manage Rules stays usable at phone width",
        actions:&[Action::Window(390,844),Action::Wait(Check::Reachable("rule-kind")),Action::Wait(Check::Reachable("review-close")),Action::Keys("rule-kind","\u{E004}\u{E004}\u{E004}")],
        checks:&[Check::Fits,Check::Focused("rule-add"),Check::Reachable("rule-add")],
    },

Case {
        name: "review editing: an imported note opens with its original author and text",
        actions: &[
            Action::Upload(
                "open-file",
                "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("review-tools"),
        ],
        checks: &[
            Check::Count("review-note", 1),
            Check::TextContains("review-note", "Govert van Drimmelen"),
            Check::TextContains("review-note", "a comment"),
            Check::Rendered("note-edit", true),
        ],
    },
    Case {
        name: "review editing: ordinary typing edits note text without rebuilding the field",
        actions: &[
            Action::Click("note-edit"),
            Action::Wait(Check::Focused("note-text")),
            Action::Into("note-text", "Updated note & <plain> 🧬\nsecond line"),
            Action::Click("note-save"),
        ],
        checks: &[
            Check::Absent("note-editor"),
            Check::TextContains("review-note", "Updated note & <plain> 🧬\nsecond line"),
        ],
    },
    Case {
        name: "review editing: notes use the ordinary Undo and Redo commands",
        actions: &[
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains("review-note", "a comment")),
            Action::Click("review-redo"),
        ],
        checks: &[
            Check::TextContains("review-note", "Updated note & <plain> 🧬\nsecond line"),
            Check::TextContains("review-note", "Govert van Drimmelen"),
        ],
    },
    Case {
        name: "review editing: saved note text reopens from the actual XLSX package",
        actions: &[
            Action::Click("review-close"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("features.xlsx"),
            Action::DiscardReplacement,
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("review-tools"),
        ],
        checks: &[
            Check::Count("review-note", 1),
            Check::TextContains("review-note", "Updated note & <plain> 🧬\nsecond line"),
        ],
    },
    Case {
        name: "review editing: threaded comments show their author and edit the authoritative thread",
        actions: &[
            Action::Click("review-close"),
            Action::Upload(
                "open-file",
                "crates/calc-host/tests/fixtures/d2/structural-base.xlsx",
            ),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=U]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("review-tools"),
            Action::Wait(Check::Count("review-note", 1)),
            Action::Wait(Check::TextContains("review-note", "thread")),
            Action::Click("note-edit"),
            Action::Into("note-text", "Edited thread & Ω"),
            Action::Click("note-save"),
        ],
        checks: &[
            Check::Count("review-note", 1),
            Check::TextContains("review-note", "Govert van Drimmelen"),
            Check::TextContains("review-note", "Edited thread & Ω"),
            Check::Absent("note-editor"),
        ],
    },
    Case {
        name: "review editing: threaded text survives saved reopening and a phone edit can cancel",
        actions: &[
            Action::Click("review-close"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("structural-base.xlsx"),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=U]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("review-tools"),
            Action::Wait(Check::TextContains("review-note", "Edited thread & Ω")),
            Action::Window(390, 844),
            Action::Click("note-edit"),
            Action::Wait(Check::Focused("note-text")),
            Action::Into("note-text", "Cancelled draft"),
            Action::Wait(Check::Reachable("note-text")),
            Action::Click("note-cancel"),
        ],
        checks: &[
            Check::Fits,
            Check::Absent("note-editor"),
            Check::TextContains("review-note", "Edited thread & Ω"),
        ],
    },
    Case {
        name: "validation audit: published formula results can be circled without an entry alert",
        actions: &[
            Action::ValidationFixture("audit"),
            Action::UploadDownload("validation-audit.xlsx"),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::Text(cell!("A1"), "1")),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Click("review-tools"),
            Action::Click("validation-circle"),
            Action::Wait(Check::TextContains("validation-scan", "invalid")),
            Action::Click("review-close"),
        ],
        checks: &[
            Check::Count("[data-dnacalc=validation-invalid][data-cell=A1]", 1),
            Check::PlanCell("A1", "text", "1"),
            Check::PlanCell("A1", "provenance", "calculated"),
        ],
    },
    Case {
        name: "validation audit: correcting an invalid value removes its circle automatically",
        actions: &[
            Action::Into(BAR, "Red\u{E007}"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Absent("[data-dnacalc=validation-invalid][data-cell=A1]"),
            Check::Text(cell!("A1"), "Red"),
            Check::TextContains("validation-prompt", "Pick a colour"),
        ],
    },
    Case {
        name: "validation audit: view circles add no Undo entry and follow ordinary Undo Redo",
        actions: &[
            Action::Click("undo"),
            Action::Wait(Check::Text(cell!("A1"), "1")),
            Action::Wait(Check::Count(
                "[data-dnacalc=validation-invalid][data-cell=A1]",
                1,
            )),
            Action::Click("redo"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "Red"),
            Check::Absent("[data-dnacalc=validation-invalid][data-cell=A1]"),
        ],
    },
    Case {
        name: "validation audit: saved validation rules and prompts reopen without saving view circles",
        actions: &[
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("validation-audit.xlsx"),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),
            Action::Into("name-box", "A1\u{E007}"),
        ],
        checks: &[
            Check::Text(cell!("A1"), "Red"),
            Check::TextContains("validation-prompt", "Pick a colour"),
            Check::Count("validation-invalid", 0),
            Check::Attr(
                "grid",
                "aria-describedby",
                "dc-grid-keyboard-0 dc-validation-prompt-0",
            ),
        ],
    },
    Case {
        name: "validation list: End reveals the last option inside a long keyboard list",
        actions: &[
            Action::ValidationFixture("list"),
            Action::UploadDownload("validation-list.xlsx"),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Wait(Check::Focused("dropdown-list")),
            Action::Keys("dropdown-list", "\u{E010}"),
        ],
        checks: &[
            Check::Count("dropdown-item", 40),
            Check::Text("[data-dnacalc=dropdown-item][aria-selected=true]", "V40"),
            Check::Contained(
                "[data-dnacalc=dropdown-item][aria-selected=true]",
                "dropdown-list",
            ),
            Check::Reachable("dropdown-list"),
            Check::ActiveDescendant("dropdown-list"),
        ],
    },
    Case {
        name: "validation list: a zoomed phone list opens within the pane above its lower edge",
        actions: &[
            Action::Keys("dropdown-list", ESCAPE),
            Action::Window(390, 844),
            Action::Into("zoom-percent", "200\u{E007}"),
            Action::Into("name-box", "A25\u{E007}"),
            Action::Wait(Check::Rendered("validation-prompt", true)),
            Action::Keys("grid", "\u{E00A}\u{E015}\u{E000}"),
            Action::Keys("dropdown-list", "\u{E010}"),
            Action::Wait(Check::Contained(
                "[data-dnacalc=dropdown-item][aria-selected=true]",
                "dropdown-list",
            )),
            Action::Wait(Check::Reachable("dropdown-list")),
            Action::Keys("dropdown-list", ENTER),
        ],
        checks: &[
            Check::Fits,
            Check::Text(cell!("A25"), "V40"),
            Check::Absent("dropdown-list"),
            Check::Reachable("validation-prompt"),
        ],
    },
    Case {
        name: "validation list: the long saved list still rejects a typed value with a real Stop decision",
        actions: &[
            Action::Window(1440, 1000),
            Action::Click("zoom-reset"),
            Action::ClearDownloads,
            Action::Click("save"),
            Action::UploadDownload("validation-list.xlsx"),
            Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),
            Action::Into("name-box", "A25\u{E007}"),
            Action::Into(BAR, "Wrong\u{E007}"),
            Action::Wait(Check::Rendered("validation-dialog", true)),
            Action::Wait(Check::TextContains(
                "validation-message",
                "Choose an item from the list",
            )),
            Action::Click("validation-cancel"),
            Action::Wait(Check::Reachable("validation-prompt")),
            Action::Keys("validation-prompt", "\u{E011}\u{E00F}\u{E010}"),
        ],
        checks: &[
            Check::Reachable("validation-prompt"),
            Check::Reachable("active-cell"),
            Check::Value("name-box", "A25"),
            Check::Absent("[data-dnacalc=validation-dialog][open]"),
            Check::Text(cell!("A25"), "V40"),
            Check::Value(BAR, "V40"),
        ],
    },
    Case {
        name: "candidate validation: custom Stop inspects the proposed formula and retains the rejected draft",
        actions: &[
            Action::ValidationFixture("candidate"),Action::UploadDownload("validation-candidate.xlsx"),Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),Action::Into("name-box","M1\u{E007}"),
            Action::Into(BAR,"=-2\u{E007}"),Action::Wait(Check::Text("validation-message","Enter a positive result")),
            Action::Click("validation-retry"),
        ],
        checks:&[Check::Value(BAR,"=-2"),Check::Absent("[data-dnacalc=validation-dialog][open]")],
    },
    Case {
        name: "candidate validation: accepted formulas calculate and Undo restores the prior cell",
        actions:&[Action::Into(BAR,"=2+2\u{E007}"),Action::Wait(Check::Text(cell!("M1"),"4")),Action::Click("undo"),Action::Wait(Check::Absent(cell!("M1"))),Action::Click("redo")],
        checks:&[Check::Text(cell!("M1"),"4"),Check::PlanCell("M1","provenance","calculated")],
    },
    Case {
        name: "candidate validation: formula results use a whole number rule and cancel preserves the cell",
        actions:&[Action::Into("name-box","C1\u{E007}"),Action::Into(BAR,"=3/2\u{E007}"),Action::Wait(Check::Text("validation-message","A whole number from 1 to 10")),Action::Click("validation-cancel"),Action::Into("name-box","M1\u{E007}")],
        checks:&[Check::Text(cell!("M1"),"4"),Check::Absent("[data-dnacalc=validation-dialog][open]")],
    },
    Case {
        name: "candidate validation: Warning keeps a rejected formula only after an explicit decision",
        actions:&[Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=-1\u{E007}"),Action::Wait(Check::TextContains("validation-dialog","Warning")),Action::Click("validation-keep")],
        checks:&[Check::Text(cell!("D1"),"-1"),Check::PlanCell("D1","provenance","calculated")],
    },
    Case {
        name: "candidate validation: custom rules survive save reopen and a phone rejection fits",
        actions:&[Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("validation-candidate.xlsx"),Action::DiscardReplacement,Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),Action::Window(390,844),Action::Into("name-box","M1\u{E007}"),Action::Into(BAR,"-3\u{E007}"),Action::Wait(Check::Text("validation-message","Enter a positive result"))],
        checks:&[Check::Fits,Check::Reachable("validation-retry"),Check::Reachable("validation-cancel"),Check::Text(cell!("M1"),"4")],
    },
    Case {
        name:"new review: a legacy note retains its author, text and ordinary Undo",
        actions:&[Action::Into("name-box","A1\u{E007}"),Action::Click("review-tools"),Action::Click("note-new"),Action::Wait(Check::Focused("note-text")),Action::Into("note-author","Local author & Ω"),Action::Into("note-text","New note <text> 🧬"),Action::Click("note-save")],
        checks:&[Check::Absent("note-editor"),Check::Count("review-note",1),Check::TextContains("review-note","Local author & Ω"),Check::TextContains("note-body","New note <text> 🧬")],
    },
    Case {
        name:"new review: rich note formatting uses the current text selection and keeps the editor",
        actions:&[Action::Click("note-edit"),Action::Click("note-bold"),Action::Wait(Check::Computed("note-run","font-weight:700")),Action::Click("note-italic"),Action::Wait(Check::Computed("note-run","font-style:italic")),Action::Click("note-save")],
        checks:&[Check::Absent("note-editor"),Check::Computed("note-run","font-weight:700"),Check::Computed("note-run","font-style:italic")],
    },
    Case {
        name:"new review: Undo Redo and cancelling later text retain the saved rich note",
        actions:&[Action::Click("review-undo"),Action::Wait(Check::Computed("note-run","font-style:normal")),Action::Click("review-redo"),Action::Wait(Check::Computed("note-run","font-style:italic")),Action::Click("note-edit"),Action::Into("note-text","Cancelled text"),Action::Click("note-cancel")],
        checks:&[Check::Text("note-body","New note <text> 🧬"),Check::Computed("note-run","font-weight:700"),Check::Computed("note-run","font-style:italic")],
    },
    Case {
        name:"new review: a new threaded comment accepts a reply with a separate author",
        actions:&[Action::Click("review-close"),Action::Into("name-box","C3\u{E007}"),Action::Click("review-tools"),Action::Click("comment-new"),Action::Into("note-author","Alice"),Action::Into("note-text","Root comment"),Action::Click("note-save"),Action::Wait(Check::Count("review-note",1)),Action::Click("note-reply"),Action::Into("note-author","Bob"),Action::Into("note-text","Reply & Ω"),Action::Click("note-save")],
        checks:&[Check::Count("review-note",2),Check::TextContains("review-dialog","Root comment"),Check::TextContains("review-dialog","Reply & Ω"),Check::TextContains("review-dialog","Bob")],
    },
    Case {
        name:"new review: resolved threads reopen and actual XLSX preserves notes, runs, people and replies",
        actions:&[Action::Click("note-resolve"),Action::Wait(Check::Text("note-resolve","Reopen thread")),Action::Click("note-resolve"),Action::Wait(Check::Text("note-resolve","Resolve thread")),Action::Click("review-close"),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Into("name-box","C3\u{E007}"),Action::Click("review-tools")],
        checks:&[Check::Count("review-note",2),Check::TextContains("review-dialog","Alice"),Check::TextContains("review-dialog","Bob"),Check::TextContains("review-dialog","Reply & Ω")],
    },
    Case {
        name:"new review: reopened rich notes and phone controls retain keyboard access",
        actions:&[Action::Click("review-close"),Action::Into("name-box","A1\u{E007}"),Action::Window(390,844),Action::Click("review-tools"),Action::Click("note-edit"),Action::Wait(Check::Focused("note-text")),Action::Keys("note-text","\u{E004}")],
        checks:&[Check::Fits,Check::Computed("note-run","font-weight:700"),Check::Computed("note-run","font-style:italic"),Check::TextContains("review-note","Local author & Ω"),Check::Focused("note-save"),Check::Reachable("note-save")],
    },

    Case {name:"tables: create a banded ListObject with real header buttons",actions:&[
        Action::Window(1440,1000),Action::Click("skin-grid"),Action::ExternalClipboard("Item\tAmount\nFirst\t5\nSecond\t9\nThird\t2",false),Action::Into("name-box","A1\u{E007}"),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("B4"),"2")),Action::Into("name-box","A1:B3\u{E007}"),Action::OpenDetails("table-tools"),Action::Into("table-name","Sales"),Action::Click("table-create")],checks:&[Check::TextContains("table-summary","Sales"),Check::Count("filter-header",2),Check::Value("table-headers","true"),Check::Value("table-bands","true"),Check::Computed(cell!("A1"),"font-weight:700")]},
    Case {name:"tables: resize rebinds structured references through Undo and Redo",actions:&[
        Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=SUM(Sales[Amount])\u{E007}"),Action::Wait(Check::Text(cell!("D1"),"14")),Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("table-tools"),Action::Into("table-rows","4"),Action::Click("table-resize"),Action::Wait(Check::Text(cell!("D1"),"16")),Action::Click("undo"),Action::Wait(Check::Text(cell!("D1"),"14")),Action::Click("redo")],checks:&[Check::Text(cell!("D1"),"16"),Check::TextContains("table-summary","A1:B4")]},
    Case {name:"tables: total rows and their functions use ordinary calculated formulas",actions:&[
        Action::Click("table-totals"),Action::Wait(Check::Text(cell!("B5"),"16")),Action::Into("table-total-column","2"),Action::Choose("table-total-function","max"),Action::Click("table-total-apply")],checks:&[Check::Text(cell!("B5"),"9"),Check::Text(cell!("D1"),"16"),Check::Value("table-totals","true")]},
    Case {name:"tables: header and banding options keep names and data",actions:&[
        Action::Click("table-headers"),Action::Wait(Check::Count("filter-header",0)),Action::Wait(Check::Text(cell!("D1"),"16")),Action::Click("table-headers"),Action::Wait(Check::Count("filter-header",2)),Action::Click("table-bands"),Action::Wait(Check::Value("table-bands","false")),Action::Click("table-bands")],checks:&[Check::Count("filter-header",2),Check::Value("table-bands","true"),Check::Text(cell!("B5"),"9"),Check::Text(cell!("A1"),"Item")]},
    Case {name:"tables: filter buttons and metadata survive actual XLSX reopening",actions:&[
        Action::Click("[data-dnacalc=filter-header][data-filter-column=\"2\"]"),Action::Wait(Check::Count("filter-choice",3)),Action::Click("filter-select-none"),Action::Click("[data-dnacalc=filter-choice][data-filter-value=\"9\"]"),Action::Click("filter-apply"),Action::Wait(Check::PlanAbsent("A2")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::OpenDetails("filter-tools")],checks:&[Check::Count("filter-header",2),Check::TextContains("filter-summary","2 rows filtered out"),Check::PlanAbsent("A2"),Check::PlanAbsent("A4"),Check::Text(cell!("B5"),"9")]},
    Case {name:"tables: clearing a table filter restores rows and phone controls stay reachable",actions:&[
        Action::Click("filter-clear"),Action::Wait(Check::Text(cell!("B2"),"5")),Action::Wait(Check::Text(cell!("B4"),"2")),Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("table-tools"),Action::Window(390,844),Action::Click("table-name"),Action::Keys("table-name",TAB)],checks:&[Check::Fits,Check::Focused("table-create"),Check::Reachable("table-create"),Check::Name("table-create","Create from selection")]},
    Case {name:"tables: author a calculated column with current-row structured selectors",actions:&[
        Action::Window(1440,1400),Action::OpenDetails("table-tools"),Action::Into("table-columns","3"),Action::Click("table-resize"),Action::Wait(Check::Count("filter-header",3)),Action::Choose("table-column","3"),Action::Into("table-column-name","Double"),Action::Click("table-column-rename"),Action::Wait(Check::Text(cell!("C1"),"Double")),Action::Into("table-formula","=[@Amount]*2"),Action::Click("table-formula-apply")],checks:&[Check::Text(cell!("C2"),"10"),Check::Text(cell!("C3"),"18"),Check::Text(cell!("C4"),"4")]},
    Case {name:"tables: column rename updates dependent formulas and calculated rules through history",actions:&[
        Action::Choose("table-column","2"),Action::Into("table-column-name","Revenue"),Action::Click("table-column-rename"),Action::Wait(Check::Text(cell!("B1"),"Revenue")),Action::Wait(Check::Text(cell!("D1"),"16")),Action::Click("undo"),Action::Wait(Check::Text(cell!("B1"),"Amount")),Action::Click("redo"),Action::Wait(Check::Text(cell!("B1"),"Revenue")),Action::Choose("table-column","3")],checks:&[Check::Text(cell!("C3"),"18"),Check::Text(cell!("D1"),"16"),Check::Value("table-formula","=[[#This Row],[Revenue]]*2")]},
    Case {name:"tables: calculated authoring copies relative A1 rows and retains absolute anchors",actions:&[
        Action::Into("name-box","F1\u{E007}"),Action::Into(BAR,"3\u{E007}"),Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("table-tools"),Action::Choose("table-column","3"),Action::Into("table-formula","=B2*$F$1"),Action::Click("table-formula-apply")],checks:&[Check::Text(cell!("C2"),"15"),Check::Text(cell!("C3"),"27"),Check::Text(cell!("C4"),"6")]},
    Case {name:"tables: resize fills new data rows while moving the ordinary total formulas",actions:&[
        Action::Into("table-rows","6"),Action::Click("table-resize"),Action::Wait(Check::TextContains("table-summary","A1:C6")),Action::Into("name-box","B5\u{E007}"),Action::Into(BAR,"7\u{E007}")],checks:&[Check::Text(cell!("C5"),"21"),Check::Text(cell!("D1"),"23"),Check::Text(cell!("B6"),"9")]},
    Case {name:"tables: removing a calculated rule keeps cells and Undo restores its metadata",actions:&[
        Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("table-tools"),Action::Choose("table-column","3"),Action::Click("table-formula-remove"),Action::Wait(Check::Value("table-formula","")),Action::Wait(Check::Text(cell!("C5"),"21")),Action::Click("undo")],checks:&[Check::Value("table-formula","=B2*$F$1"),Check::Text(cell!("C5"),"21")]},
    Case {name:"tables: additional medium presets paint their own accent and survive XLSX reopening",actions:&[
        Action::Choose("table-style","TableStyleMedium4"),Action::Wait(Check::Computed("[data-dnacalc=cell-decoration][data-cell=A1]","background-color:rgb(165, 165, 165)")),Action::Choose("table-style","TableStyleMedium7"),Action::Wait(Check::Computed("[data-dnacalc=cell-decoration][data-cell=A1]","background-color:rgb(112, 173, 71)")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Into("name-box","A1\u{E007}"),Action::OpenDetails("table-tools"),Action::Choose("table-column","3")],checks:&[Check::Value("table-style","TableStyleMedium7"),Check::Value("table-formula","=B2*$F$1"),Check::Text(cell!("C5"),"21"),Check::Text(cell!("B1"),"Revenue")]},
    Case {name:"tables: formatted header conversion uses visible number text and unique column names",actions:&[
        Action::Click("new"),Action::DiscardReplacement,Action::ExternalClipboard("0.25\t0.25\n1\t2",false),Action::Into("name-box","A1\u{E007}"),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Text(cell!("B2"),"2")),Action::Into("name-box","A1:B1\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Into("format-number","0.0%"),Action::Click("format-apply"),Action::Wait(Check::Text(cell!("A1"),"25.0%")),Action::Into("name-box","A1:B2\u{E007}"),Action::OpenDetails("table-tools"),Action::Into("table-name","Percentages"),Action::Click("table-create")],checks:&[Check::Text(cell!("A1"),"25.0%"),Check::Text(cell!("B1"),"25.0%2"),Check::TextContains("table-summary","Percentages")]},
    Case {name:"charts: imported column bar line pie and scatter plots render read-only",actions:&[
        Action::Window(1600,1800),Action::Upload("open-file","crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx"),Action::DiscardReplacement,Action::Click("skin-grid")],checks:&[
        Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 1\"]","data-kind","column"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 2\"]","data-kind","bar"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 3\"]","data-kind","line"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 5\"]","data-kind","pie"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 6\"]","data-kind","scatter"),Check::Attr("[data-chart=\"Chart 5\"] [data-series=\"0\"][data-point=\"0\"]","data-value","12"),Check::Attr("[data-chart=\"Chart 6\"] [data-series=\"0\"][data-point=\"0\"]","data-x","12")]},
    Case {name:"charts: columns bars pie slices and scatter X values follow the existing evaluator",actions:&[
        Action::Into("name-box","B2\u{E007}"),Action::Into(BAR,"40\u{E007}")],checks:&[
        Check::Attr("[data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]","data-value","40"),Check::Attr("[data-chart=\"Chart 2\"] [data-series=\"0\"][data-point=\"0\"]","data-value","40"),Check::Attr("[data-chart=\"Chart 3\"] [data-series=\"0\"][data-point=\"0\"]","data-value","48"),Check::Attr("[data-chart=\"Chart 5\"] [data-series=\"0\"][data-point=\"0\"]","data-value","40"),Check::Attr("[data-chart=\"Chart 6\"] [data-series=\"0\"][data-point=\"0\"]","data-x","40")]},
    Case {name:"charts: all five requested plot families survive XLSX save and reopen",actions:&[
        Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("charts.xlsx"),Action::DiscardReplacement],checks:&[
        Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 2\"]","data-kind","bar"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 3\"]","data-kind","line"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 5\"]","data-kind","pie"),Check::Attr("[data-dnacalc=chart][data-chart=\"Chart 6\"]","data-kind","scatter"),Check::Attr("[data-chart=\"Chart 1\"] [data-series=\"0\"][data-point=\"0\"]","data-value","40")]},

    Case {name:"font variants: imported double underline and stacked graphemes paint from stored styles",actions:&[
        Action::Window(1440,1400),Action::SetPainter("dom"),Action::ClearDownloads,Action::AlignmentFixture,Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","B20\u{E007}")],checks:&[
        Check::Text(cell!("B20"),"Double"),Check::PlanCell("B20","style.underline_style","double"),Check::Computed("[data-cell=B20] .dc-cell-text","text-decoration-style:double"),Check::PlanCell("D20","style.rotation","255"),Check::Count("[data-cell=D20] .dc-cell-text > span",3),Check::Count("[data-cell=D21] .dc-cell-text > span",3)]},
    Case {name:"font variants: Canvas paints the same stacked text without a workbook command",actions:&[
        Action::Into("name-box","D21\u{E007}"),Action::Wait(Check::TextBounds(cell!("D21"))),Action::RememberPresentation,Action::SetPainter("canvas")],checks:&[Check::PainterStable,Check::CanvasInk("B20"),Check::CanvasInk("D20"),Check::CanvasInk("D21")]},
    Case {name:"font variants: mixed underline styles author and preview double while strike stays single",actions:&[
        Action::SetPainter("dom"),Action::Into("name-box","B20:D21\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Wait(Check::Value("format-underline-style","")),Action::Choose("format-underline-style","none"),Action::Wait(Check::Value("format-underline","false")),Action::Choose("format-underline-style","double"),Action::Wait(Check::Value("format-underline","true")),Action::Choose("format-underline","false"),Action::Wait(Check::Value("format-underline-style","none")),Action::Choose("format-underline-style","double"),Action::Wait(Check::Value("format-underline","true")),Action::Choose("format-strike","true"),Action::Wait(Check::Computed("format-preview-text","text-decoration-style:double")),Action::Wait(Check::Computed("format-preview","text-decoration-line:line-through")),Action::Click("format-apply"),Action::Wait(Check::PlanCell("D21","style.underline_style","double")),Action::Click("undo"),Action::Wait(Check::PlanCell("D21","style.underline","false")),Action::Click("redo")],checks:&[Check::PlanCell("D21","style.underline_style","double"),Check::PlanCell("D21","style.strike","true"),Check::PlanCell("D21","style.rotation","255"),Check::Text(cell!("B20"),"Double")]},
    Case {name:"font variants: author stacked orientation through the ordinary format transaction and history",actions:&[
        Action::Into("name-box","B20\u{E007}"),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-alignment"),Action::Choose("format-orientation","stacked"),Action::Click("format-apply"),Action::Wait(Check::PlanCell("B20","style.rotation","255")),Action::Click("undo"),Action::Wait(Check::PlanCell("B20","style.rotation","0")),Action::Click("redo")],checks:&[Check::PlanCell("B20","style.rotation","255"),Check::Count("[data-cell=B20] .dc-cell-text > span",6),Check::PlanCell("B20","style.underline_style","double")]},
    Case {name:"font variants: real XLSX reopening retains double underline strike and stacked orientation",actions:&[
        Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","B20\u{E007}")],checks:&[Check::PlanCell("B20","style.rotation","255"),Check::PlanCell("D21","style.underline_style","double"),Check::PlanCell("D21","style.strike","true"),Check::Count("[data-cell=D21] .dc-cell-text > span",3),Check::Text(cell!("D21"),"é👩‍🔬Z")]},
    Case {name:"font variants: unsupported accounting effects stay labelled and controls fit three widths",actions:&[
        Action::Into("name-box","F20\u{E007}"),Action::Wait(Check::TextContains("style-note","underline variants")),Action::Window(390,844),Action::Click("format-cells"),Action::Wait(Check::Focused("format-number")),Action::Click("format-tab-font"),Action::Wait(Check::Fits),Action::Wait(Check::Reachable("format-underline-style")),Action::Click("format-cancel"),Action::Window(900,900),Action::Wait(Check::Fits),Action::Window(1440,1400)],checks:&[Check::Fits,Check::PlanCell("F20","style.underline_style","singleAccounting"),Check::PlanCell("F20","style.underline","false"),Check::TextContains("style-note","preserved but not painted")]},

    Case {name:"stacked fitting: row AutoFit measures three rendered graphemes in the stored font",actions:&[
        Action::Window(1440,1400),Action::SetPainter("dom"),Action::ClearDownloads,Action::AlignmentFixture,Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","D21\u{E007}"),Action::Wait(Check::Style(cell!("D21"),"width:84px")),Action::OpenDetails("axis-tools"),Action::Click("autofit-rows")],checks:&[Check::TextContains("status","Fitted contents"),Check::Style(cell!("D21"),"height:60px"),Check::TextBounds(cell!("D21")),Check::Count("[data-cell=D21] .dc-cell-text > span",3)]},
    Case {name:"stacked fitting: column AutoFit uses the widest grapheme and ordinary Undo Redo",actions:&[
        Action::Click("autofit-columns"),Action::Wait(Check::MaxWidth(cell!("D21"),64)),Action::Click("undo"),Action::Wait(Check::Style(cell!("D21"),"width:84px")),Action::Click("redo")],checks:&[Check::MaxWidth(cell!("D21"),64),Check::TextBounds(cell!("D21")),Check::Style(cell!("D21"),"height:60px"),Check::Text(cell!("D21"),"é👩‍🔬Z")]},
    Case {name:"stacked fitting: fitted dimensions orientation and source survive actual XLSX reopening",actions:&[
        Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","D21\u{E007}")],checks:&[Check::MaxWidth(cell!("D21"),64),Check::Style(cell!("D21"),"height:60px"),Check::TextBounds(cell!("D21")),Check::PlanCell("D21","style.rotation","255"),Check::Count("[data-cell=D21] .dc-cell-text > span",3)]},
    Case {name:"stacked fitting: wrapped stacked combinations refuse without an authored command",actions:&[
        Action::Into("name-box","G20\u{E007}"),Action::OpenDetails("axis-tools"),Action::Wait(Check::ActiveSemantic("G20")),Action::Wait(Check::TextBounds(cell!("G20"))),Action::Wait(Check::TextContains("style-note","wrapping and rotation is unavailable")),Action::RememberPresentation,Action::Click("autofit-rows")],checks:&[Check::TextContains("status","wrapping and rotation is unavailable"),Check::PainterStable,Check::Style(cell!("G20"),"height:120px"),Check::PlanCell("G20","style.rotation","255")]},
    Case {name:"stacked fitting: an embedded source newline occupies one fixed grapheme row",actions:&[
        Action::Into("name-box","H22\u{E007}"),Action::Wait(Check::Count("[data-cell=H22] .dc-cell-text > span",3)),Action::Click("autofit-rows")],checks:&[Check::Style(cell!("H22"),"height:60px"),Check::TextBounds(cell!("H22")),Check::Text(cell!("H22"),"A\nB"),Check::Count("[data-cell=H22] .dc-cell-text > span",3)]},
    Case {name:"stacked fitting: a trailing newline keeps its fourth grapheme row through fitting and XLSX",actions:&[
        Action::Into("name-box","H23\u{E007}"),Action::Wait(Check::Count("[data-cell=H23] .dc-cell-text > span",4)),Action::Click("autofit-rows"),Action::Wait(Check::Style(cell!("H23"),"height:79px")),Action::Click("autofit-columns"),Action::Wait(Check::MaxWidth(cell!("H23"),64)),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("alignment.xlsx"),Action::DiscardReplacement,Action::Into("name-box","H23\u{E007}")],checks:&[Check::Style(cell!("H23"),"height:79px"),Check::MaxWidth(cell!("H23"),64),Check::TextBounds(cell!("H23")),Check::Text(cell!("H23"),"A\nB\n"),Check::PlanCell("H23","style.rotation","255"),Check::Count("[data-cell=H23] .dc-cell-text > span",4)]},

    Case {
        name: "conditional colour filters: imported solid rules filter rows and recalculate both aggregate functions",
        actions: &[
            Action::Window(1440,1400),
            Action::Upload("open-file", "crates/calc-host/tests/fixtures/dc_6yw/conditional-formats.xlsx"),
            Action::DiscardReplacement, Action::Click("skin-grid"),
            Action::Into("name-box", "A12\u{E007}"), Action::Wait(Check::ActiveSemantic("A12")), Action::Into(BAR, "=SUBTOTAL(9,D2:D10)\u{E007}"), Action::Wait(Check::PlanCell("A12", "text", "102")),
            Action::Into("name-box", "B12\u{E007}"), Action::Wait(Check::ActiveSemantic("B12")), Action::Into(BAR, "=SUBTOTAL(109,D2:D10)\u{E007}"), Action::Wait(Check::PlanCell("B12", "text", "102")),
            Action::Into("name-box", "C12\u{E007}"), Action::Wait(Check::ActiveSemantic("C12")), Action::Into(BAR, "=AGGREGATE(9,4,D2:D10)\u{E007}"), Action::Wait(Check::PlanCell("C12", "text", "102")),
            Action::Into("name-box", "D12\u{E007}"), Action::Wait(Check::ActiveSemantic("D12")), Action::Into(BAR, "=AGGREGATE(9,5,D2:D10)\u{E007}"), Action::Wait(Check::PlanCell("D12", "text", "102")),
            Action::Into("name-box", "A4\u{E007}"), Action::Wait(Check::ActiveSemantic("A4")), Action::OpenDetails("axis-tools"), Action::Click("hide-rows"), Action::Wait(Check::PlanAbsent("A4")), Action::Wait(Check::PlanCell("B12", "text", "88")),
            Action::Into("name-box", "A1:D10\u{E007}"), Action::Wait(Check::ActiveSemantic("A1")), Action::OpenDetails("filter-tools"), Action::Click("filter-enable"),
            Action::Wait(Check::TextContains("filter-summary", "A1:D10")),
            Action::OpenDetails("filter-color-tools"), Action::Into("filter-color", "#ffc7ce"), Action::Click("filter-color-apply"),
        ],
        checks: &[Check::TextContains("filter-summary", "4 rows filtered out"),
            Check::PlanCell("A12", "text", "80"), Check::PlanCell("B12", "text", "66"),
            Check::PlanCell("C12", "text", "102"), Check::PlanCell("D12", "text", "66"),
            Check::PlanCell("D12", "provenance", "calculated"), Check::PlanAbsent("A3"), Check::PlanAbsent("A4"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=A2]", "background-color:#ffc7ce")],
    },
    Case {
        name: "conditional colour filters: Undo and Redo preserve the independently hidden row",
        actions: &[Action::Click("undo"), Action::Wait(Check::TextContains("filter-summary", "0 rows filtered out")),
            Action::Wait(Check::PlanCell("B12", "text", "88")), Action::Click("redo")],
        checks: &[Check::PlanCell("A12", "text", "80"), Check::PlanCell("B12", "text", "66"),
            Check::PlanAbsent("A3"), Check::PlanAbsent("A4"), Check::TextContains("filter-summary", "4 rows filtered out")],
    },
    Case {
        name: "conditional colour filters: saved criteria and source rules reapply after real XLSX reopening",
        actions: &[Action::ClearDownloads, Action::Click("save"), Action::UploadDownload("conditional-formats.xlsx"),
            Action::DiscardReplacement, Action::OpenDetails("filter-tools"), Action::Click("filter-reapply")],
        checks: &[Check::PlanCell("A12", "text", "80"), Check::PlanCell("D12", "text", "66"),
            Check::PlanCell("D12", "provenance", "calculated"), Check::PlanAbsent("A4"),
            Check::TextContains("filter-summary", "4 rows filtered out")],
    },
    Case {
        name: "conditional colour filters: a relative other-column formula supplies the displayed font colour",
        actions: &[Action::Click("filter-clear"), Action::Into("filter-column", "4"),
            Action::OpenDetails("filter-color-tools"), Action::Choose("filter-color-kind", "font"),
            Action::Into("filter-color", "#0000ff"), Action::Click("filter-color-apply")],
        checks: &[Check::PlanCell("A12", "text", "68"), Check::PlanCell("B12", "text", "54"),
            Check::PlanCell("C12", "text", "102"), Check::PlanCell("D12", "text", "54"),
            Check::Style(cell!("D2"), "color:#0000ff"), Check::PlanAbsent("A4"), Check::PlanAbsent("A7"),
            Check::TextContains("filter-summary", "5 rows filtered out")],
    },
    Case {
        name: "conditional colour filters: no fill excludes coloured rows and phone controls remain reachable",
        actions: &[Action::Click("filter-clear"), Action::Into("filter-column", "1"),
            Action::OpenDetails("filter-color-tools"), Action::Choose("filter-color-kind", "fill"),
            Action::Into("filter-color", ""), Action::Click("filter-color-apply"), Action::Window(390,844),
            Action::Into("name-box", "A12\u{E007}")],
        checks: &[Check::PlanCell("A12", "text", "22"), Check::PlanCell("D12", "text", "22"),
            Check::PlanAbsent("A2"), Check::PlanAbsent("A4"), Check::TextContains("filter-summary", "5 rows filtered out"),
            Check::Fits, Check::Narrow(390), Check::Reachable("filter-color"), Check::Reachable("filter-color-apply")],
    },
    Case {
        name: "conditional colour filters: an unsupported scale refuses without changing row visibility",
        actions: &[Action::Window(1440,1400), Action::Click("filter-remove"),
            Action::Into("name-box", "A1:N10\u{E007}"), Action::OpenDetails("filter-tools"), Action::Click("filter-enable"),
            Action::Wait(Check::TextContains("filter-summary", "A1:N10")), Action::Into("filter-column", "5"),
            Action::OpenDetails("filter-color-tools"), Action::Into("filter-color", "#86c97e"), Action::Click("filter-color-apply")],
        checks: &[Check::TextContains("status", "this conditional rule is not modelled"),
            Check::TextContains("filter-summary", "0 rows filtered out"), Check::PlanCell("A12", "text", "102"),
            Check::PlanCell("D12", "text", "88"), Check::PlanAbsent("A4"),
            Check::Style("[data-dnacalc=cell-decoration][data-cell=E2]", "background-color:#86c97e")],
    },

    Case {
        name:"sort metadata: row formulas validation and authored notes move together",
        actions:&[
            Action::Window(1440,1000),Action::ValidationFixture("sort"),Action::UploadDownload("validation-sort.xlsx"),Action::DiscardReplacement,
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),Action::Wait(Check::PlanCell("C5","text","2")),
            Action::Into("name-box","B2\u{E007}"),Action::Wait(Check::ActiveSemantic("B2")),Action::Click("review-tools"),Action::Click("note-new"),Action::Wait(Check::Focused("note-text")),Action::Into("note-author","Row author"),Action::Into("note-text","Three 🧬"),Action::Click("note-save"),Action::Wait(Check::TextContains("review-note","Three 🧬")),Action::Click("review-close"),
            Action::Into("name-box","B3\u{E007}"),Action::Wait(Check::ActiveSemantic("B3")),Action::Click("review-tools"),Action::Click("note-new"),Action::Wait(Check::Focused("note-text")),Action::Into("note-author","Row author"),Action::Into("note-text","One note"),Action::Click("note-save"),Action::Wait(Check::TextContains("review-note","One note")),Action::Click("review-close"),
            Action::Into("name-box","A1:C5\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("sort-tools"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","Sorted")),
            Action::Into("name-box","B5\u{E007}"),Action::Wait(Check::ActiveSemantic("B5")),Action::Click("review-tools"),
        ],
        checks:&[Check::TextContains("review-note","Three 🧬"),Check::TextContains("review-validation","custom"),Check::TextContains("validation-prompt","Row rule"),Check::PlanCell("B2","text","one"),Check::PlanCell("B3","text","one second"),Check::PlanCell("C5","text","6"),Check::PlanCell("C5","formula","true"),Check::PlanCell("C5","provenance","calculated")],
    },
    Case {
        name:"sort metadata: Undo and Redo restore the note and validation origins",
        actions:&[Action::Click("review-close"),Action::Click("undo"),Action::Wait(Check::PlanCell("B2","text","three")),Action::Into("name-box","B2\u{E007}"),Action::Wait(Check::ActiveSemantic("B2")),Action::Click("review-tools"),Action::Wait(Check::TextContains("review-note","Three 🧬")),Action::Wait(Check::TextContains("review-validation","custom")),Action::Click("review-close"),Action::Click("redo"),Action::Wait(Check::PlanCell("B5","text","three")),Action::Into("name-box","B2\u{E007}"),Action::Wait(Check::ActiveSemantic("B2")),Action::Click("review-tools")],
        checks:&[Check::TextContains("review-note","One note"),Check::Absent("review-validation"),Check::Absent("validation-prompt"),Check::PlanCell("C2","text","2")],
    },
    Case {
        name:"sort metadata: imported notes and rules can sort again after real XLSX reopening",
        actions:&[Action::Click("review-close"),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("validation-sort.xlsx"),Action::DiscardReplacement,Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),Action::Into("name-box","A1:C5\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("sort-tools"),Action::Click("sort-descending"),Action::Wait(Check::TextContains("status","Sorted")),Action::Into("name-box","B2\u{E007}"),Action::Wait(Check::ActiveSemantic("B2")),Action::Click("review-tools")],
        checks:&[Check::TextContains("review-note","Three 🧬"),Check::TextContains("review-validation","custom"),Check::TextContains("validation-prompt","Row rule"),Check::PlanCell("C2","text","6"),Check::PlanCell("C2","formula","true"),Check::PlanCell("B4","text","one"),Check::PlanCell("B5","text","one second")],
    },
    Case {
        name:"sort metadata: the moved custom rule still rejects the proposed entry",
        actions:&[Action::Click("review-close"),Action::Into(BAR,"wrong\u{E007}"),Action::Wait(Check::Rendered("validation-dialog",true)),Action::Wait(Check::TextContains("validation-message","Keep the row payload")),Action::Click("validation-cancel"),Action::Wait(Check::Absent("[data-dnacalc=validation-dialog][open]")),Action::Click("review-tools")],
        checks:&[Check::TextContains("review-note","Three 🧬"),Check::PlanCell("B2","text","three"),Check::PlanCell("C2","text","6"),Check::TextContains("validation-prompt","Row rule")],
    },
    Case {
        name:"sort metadata: an independently moved note is retained on a narrow saved sheet",
        actions:&[Action::Click("review-close"),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("validation-sort.xlsx"),Action::DiscardReplacement,Action::Click("[data-dnacalc=sheet-tab][data-sheet=DV]"),Action::Window(390,844),Action::Into("name-box","B4\u{E007}"),Action::Wait(Check::ActiveSemantic("B4")),Action::Click("review-tools")],
        checks:&[Check::TextContains("review-note","One note"),Check::Absent("review-validation"),Check::Fits,Check::Narrow(390),Check::Reachable("review-close"),Check::PlanCell("C4","text","2")],
    },

    Case {name:"table sorting: complete data rows move formulas while headers and totals stay",actions:&[
        Action::Window(1440,1000),Action::ExternalClipboard("Item\tAmount\tDouble\nThree\t3\t=B2*2\nOne\t1\t=B3*2\nTwo\t2\t=B4*2",false),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::PlanCell("C4","text","4")),
        Action::Into("name-box","A1:C4\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("table-tools"),Action::Into("table-name","SortSales"),Action::Click("table-create"),Action::Wait(Check::Count("filter-header",3)),Action::Click("table-totals"),Action::Wait(Check::PlanCell("C5","text","12")),Action::Into("table-total-column","2"),Action::Choose("table-total-function","sum"),Action::Click("table-total-apply"),Action::Wait(Check::PlanCell("B5","text","6")),
        Action::Into("name-box","D1\u{E007}"),Action::Wait(Check::ActiveSemantic("D1")),Action::Into(BAR,"=SUM(SortSales[Amount])\u{E007}"),Action::Wait(Check::PlanCell("D1","text","6")),Action::Into("name-box","A1:C4\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("sort-tools"),Action::Into("sort-column","2"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","Sorted"))
    ],checks:&[Check::PlanCell("A1","text","Item"),Check::PlanCell("A2","text","One"),Check::PlanCell("A4","text","Three"),Check::PlanCell("C2","text","2"),Check::PlanCell("C4","text","6"),Check::PlanCell("C4","formula","true"),Check::PlanCell("B5","text","6"),Check::PlanCell("C5","text","12"),Check::PlanCell("D1","text","6"),Check::Count("filter-header",3)]},
    Case {name:"table sorting: one Undo and Redo restore every row and keep table definitions",actions:&[
        Action::Click("undo"),Action::Wait(Check::PlanCell("A2","text","Three")),Action::Wait(Check::PlanCell("C2","text","6")),Action::Click("redo"),Action::Wait(Check::PlanCell("A2","text","One")),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("table-tools")
    ],checks:&[Check::TextContains("table-summary","SortSales"),Check::TextContains("table-summary","A1:C5"),Check::PlanCell("C2","text","2"),Check::PlanCell("C5","text","12"),Check::Count("filter-header",3)]},
    Case {name:"table sorting: imported table data sort after real XLSX reopening",actions:&[
        Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Wait(Check::PlanCell("C5","text","12")),Action::Into("name-box","A1:C4\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("sort-tools"),Action::Into("sort-column","2"),Action::Click("sort-descending"),Action::Wait(Check::PlanCell("A2","text","Three"))
    ],checks:&[Check::PlanCell("A4","text","One"),Check::PlanCell("C2","text","6"),Check::PlanCell("C4","text","2"),Check::PlanCell("C4","formula","true"),Check::PlanCell("B5","text","6"),Check::PlanCell("C5","text","12"),Check::PlanCell("D1","text","6")]},
    Case {name:"table sorting: partial columns and total rows refuse before changing cells",actions:&[
        Action::Into("name-box","A2:B4\u{E007}"),Action::Wait(Check::ActiveSemantic("A2")),Action::OpenDetails("sort-tools"),Action::Click("sort-header"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","partial table")),Action::Wait(Check::PlanCell("A2","text","Three")),Action::Into("name-box","A1:C5\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::Click("sort-header"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","partial table"))
    ],checks:&[Check::PlanCell("A2","text","Three"),Check::PlanCell("A4","text","One"),Check::PlanCell("C2","text","6"),Check::PlanCell("C5","text","12"),Check::Count("filter-header",3)]},
    Case {name:"table sorting: complete body selection sorts from phone controls and saves",actions:&[
        Action::Into("name-box","A2:C4\u{E007}"),Action::Wait(Check::ActiveSemantic("A2")),Action::Window(390,844),Action::OpenDetails("sort-tools"),Action::Click("sort-header"),Action::Click("sort-ascending"),Action::Wait(Check::PlanCell("A2","text","One")),Action::Wait(Check::Fits),Action::Wait(Check::Reachable("sort-ascending")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Wait(Check::PlanCell("C5","text","12")),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),Action::OpenDetails("table-tools")
    ],checks:&[Check::PlanCell("A2","text","One"),Check::PlanCell("C2","text","2"),Check::PlanCell("B5","text","6"),Check::PlanCell("C5","text","12"),Check::Count("filter-header",3),Check::TextContains("table-summary","A1:C5"),Check::Fits,Check::Narrow(390),Check::Reachable("table-totals")]},


    Case {name:"table colours: automatic text stays readable in dark DOM and Canvas with explicit colours retained",actions:&[
        Action::Window(1440,1000),Action::Click("settings-toggle"),Action::Choose("theme","cockpit-dark"),Action::Click("settings-toggle"),Action::Into("name-box","A1\u{E007}"),Action::Wait(Check::ActiveSemantic("A1")),
        Action::Wait(Check::PlanCell("A2","style.font_color","#000000")),Action::Wait(Check::Computed(cell!("A2"),"color:#000000")),Action::Wait(Check::PlanCell("A1","style.font_color","#ffffff")),Action::Wait(Check::PlanCell("C5","style.font_color","#000000")),
        Action::Into("name-box","A2\u{E007}"),Action::Wait(Check::ActiveSemantic("A2")),Action::Click("format-cells"),Action::Click("format-tab-font"),Action::Into("format-font-color","#123456"),Action::Click("format-apply"),Action::Wait(Check::PlanCell("A2","style.font_color","#123456")),
        Action::OpenDetails("pane-tools"),Action::SetPainter("canvas"),Action::Wait(Check::CanvasInk("A2")),Action::Wait(Check::PlanCell("B2","style.font_color","#000000")),Action::SetPainter("dom")
    ],checks:&[Check::Computed(cell!("A2"),"color:#123456"),Check::Computed(cell!("B2"),"color:#000000"),Check::PlanCell("C5","text","12"),Check::PlanCell("A1","style.font_color","#ffffff")]},

    Case {
        name:"templates: Save As template writes the actual authored workbook (dc-bctq)",
        actions:&[Action::Window(1440,1000),Action::ClearDownloads,
            Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"6\u{E007}"),
            Action::Into("name-box","B1\u{E007}"),Action::Into(BAR,"=A1*3\u{E007}"),Action::Wait(Check::Text(cell!("B1"),"18")),
            Action::OpenDetails("file-menu"),Action::Click("save-as"),Action::Wait(Check::Rendered("save-as-dialog",true)),
            Action::Into("save-as-name","Budget"),Action::Choose("save-as-format","xltx"),Action::Click("save-as-confirm")],
        checks:&[Check::Text("document","Budget.xltx"),Check::Attr("document","data-template","true"),Check::Attr("document","data-unsaved","false"),Check::Absent("[data-dnacalc=save-as-dialog][open]")],
    },
    Case {
        name:"templates: ordinary Open creates an unsaved workbook with calculated formulas",
        actions:&[Action::UploadDownload("Budget.xltx"),Action::DiscardReplacement],
        checks:&[Check::Text("document","Book1.xlsx"),Check::Attr("document","data-unsaved","true"),Check::Attr("document","data-template","false"),Check::Text(cell!("A1"),"6"),Check::Text(cell!("B1"),"18"),Check::PlanCell("B1","provenance","calculated")],
    },
    Case {
        name:"templates: Save of the unsaved workbook prompts Save As and cancellation keeps it",
        actions:&[Action::Click("save"),Action::Wait(Check::Rendered("save-as-dialog",true)),Action::Wait(Check::Value("save-as-format","xlsx")),Action::Click("save-as-cancel")],
        checks:&[Check::Attr("document","data-unsaved","true"),Check::Text(cell!("B1"),"18"),Check::Absent("[data-dnacalc=save-as-dialog][open]")],
    },
    Case {
        name:"templates: Open for editing retains the template identity and its formula",
        actions:&[Action::OpenDetails("file-menu"),Action::UploadDownloadTo("Budget.xltx","open-editing-file"),Action::DiscardReplacement,
            Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"7\u{E007}"),Action::Wait(Check::Text(cell!("B1"),"21"))],
        checks:&[Check::Text("document","Budget.xltx (changed)"),Check::Attr("document","data-unsaved","false"),Check::Attr("document","data-template","true")],
    },
    Case {
        name:"templates: personal templates create independent workbooks and can be removed",
        actions:&[Action::OpenDetails("file-menu"),Action::UploadDownloadTo("Budget.xltx","template-file"),Action::Wait(Check::Value("template-list","Budget.xltx")),Action::Click("template-new"),Action::DiscardReplacement,
            Action::Wait(Check::Text("document","Book1.xlsx")),Action::OpenDetails("file-menu"),Action::Choose("template-list","Budget.xltx"),Action::Click("template-remove")],
        checks:&[Check::Value("template-list",""),Check::Text(cell!("B1"),"18"),Check::Attr("document","data-unsaved","true")],
    },
    Case {
        name:"templates: a macro-enabled choice writes a template and opens it as an xlsm workbook",
        actions:&[Action::OpenDetails("file-menu"),Action::Click("save-as"),Action::Into("save-as-name","MacroBudget"),Action::Choose("save-as-format","xltm"),Action::Click("save-as-confirm"),Action::Wait(Check::Text("document","MacroBudget.xltm")),Action::UploadDownload("MacroBudget.xltm"),Action::DiscardReplacement],
        checks:&[Check::Text("document","Book1.xlsm"),Check::Attr("document","data-unsaved","true"),Check::Text(cell!("B1"),"18")],
    },
    Case {
        name:"templates: VBA loss is warned and the macro-free download is refused atomically",
        actions:&[Action::Upload("open-file","crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"),Action::DiscardReplacement,Action::Wait(Check::Text("document","local.xlsm")),
            Action::OpenDetails("file-menu"),Action::Click("save-as"),Action::Choose("save-as-format","xltx"),Action::Wait(Check::TextContains("save-as-warning","VBA would be lost")),Action::Click("save-as-confirm")],
        checks:&[Check::Text("document","local.xlsm"),Check::Rendered("save-as-dialog",true),Check::TextContains("status","would be lost")],
    },
    Case {
        name:"templates: phone Save As and template controls remain reachable",
        actions:&[Action::Click("save-as-cancel"),Action::Window(390,844),Action::OpenDetails("file-menu"),Action::Wait(Check::Reachable("template-add")),Action::Wait(Check::Reachable("template-list")),Action::Click("save-as")],
        checks:&[Check::Reachable("save-as-name"),Check::Reachable("save-as-format"),Check::Reachable("save-as-confirm"),Check::Reachable("save-as-cancel")],
    },

    Case {
        name:"templates: inserted worksheets calculate renamed qualified references and undo together",
        actions:&[Action::Click("save-as-cancel"),Action::Window(1440,1000),Action::UploadDownload("Budget.xltx"),Action::DiscardReplacement,
            Action::OpenDetails("file-menu"),Action::UploadDownloadTo("Budget.xltx","template-file"),Action::Wait(Check::Value("template-list","Budget.xltx")),Action::Click("template-insert"),
            Action::Wait(Check::Count("[data-dnacalc=sheet-jump] option",2)),Action::Wait(Check::Text(cell!("B1"),"18")),Action::Click("undo"),Action::Wait(Check::Count("[data-dnacalc=sheet-jump] option",1)),Action::Click("redo")],
        checks:&[Check::Count("[data-dnacalc=sheet-jump] option",2),Check::Text(cell!("B1"),"18"),Check::PlanCell("B1","provenance","calculated")],
    },
    Case {
        name:"templates: Sheet default supplies new worksheets and clearing restores blank insertion",
        actions:&[Action::OpenDetails("file-menu"),Action::Choose("template-list","Budget.xltx"),Action::Click("template-default-sheet"),Action::Click("file-toggle"),
            Action::OpenDetails("sheet-tools"),Action::Click("sheet-add"),Action::Wait(Check::Count("[data-dnacalc=sheet-jump] option",3)),Action::Wait(Check::Text(cell!("B1"),"18")),
            Action::OpenDetails("file-menu"),Action::Click("template-clear-defaults"),Action::Click("file-toggle"),Action::OpenDetails("sheet-tools"),Action::Click("sheet-add")],
        checks:&[Check::Count("[data-dnacalc=sheet-jump] option",4),Check::PlanAbsent("B1")],
    },
    Case {
        name:"templates: Book default survives startup and can be edited without creating a workbook",
        actions:&[Action::OpenDetails("file-menu"),Action::Choose("template-list","Budget.xltx"),Action::Click("template-default-book"),Action::Reload,
            Action::Wait(Check::Text("document","Book1.xlsx")),Action::Wait(Check::Text(cell!("B1"),"18")),Action::OpenDetails("file-menu"),Action::Choose("template-list","Book.xltx"),Action::Click("template-edit"),Action::DiscardReplacement],
        checks:&[Check::Text("document","Book.xltx"),Check::Attr("document","data-template","true"),Check::Text(cell!("B1"),"18")],
    },
    Case {
        name:"templates: saving an edited personal default updates its stored contents",
        actions:&[Action::Into("name-box","A1\u{E007}"),Action::Into(BAR,"9\u{E007}"),Action::Wait(Check::Text(cell!("B1"),"27")),Action::Click("save"),Action::Wait(Check::Text("document","Book.xltx")),Action::OpenDetails("file-menu"),Action::Choose("template-list","Budget.xltx"),Action::Click("template-default-sheet"),Action::Choose("template-list","Sheet.xltx"),Action::Click("template-remove"),Action::Reload],
        checks:&[Check::Text("document","Book1.xlsx"),Check::Text(cell!("B1"),"27"),Check::Attr("document","data-unsaved","true")],
    },
    Case {
        name:"templates: removing defaults persists and keeps the original personal template",
        actions:&[Action::OpenDetails("file-menu"),Action::Click("template-clear-defaults"),Action::Reload,Action::Wait(Check::Text("document","Book1.xlsx")),Action::OpenDetails("file-menu"),Action::Choose("template-list","Budget.xltx")],
        checks:&[Check::PlanAbsent("B1"),Check::Value("template-list","Budget.xltx"),Check::Count("[data-dnacalc=template-list] option",2)],
    },

    Case {name:"case sorting: character positions distinguish aZ from Aa and move formulas",actions:&[
        Action::Window(1440,1000),Action::ExternalClipboard("Key\tAmount\tTotal\naZ\t1\t=B2*10\nAa\t2\t=B3*10\nab\t3\t=B4*10\naB\t4\t=B5*10",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::PlanCell("C5","text","40")),Action::Into("name-box","A1:C5\u{E007}"),Action::OpenDetails("sort-tools"),Action::OpenDetails("sort-options"),Action::Click("sort-case-sensitive"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","Sorted"))
    ],checks:&[Check::PlanCell("A2","text","ab"),Check::PlanCell("A3","text","aB"),Check::PlanCell("A4","text","aZ"),Check::PlanCell("A5","text","Aa"),Check::PlanCell("C2","text","30"),Check::PlanCell("C5","text","20")]},
    Case {name:"case sorting: undo redo and descending preserve row contents",actions:&[
        Action::Click("undo"),Action::Wait(Check::PlanCell("A2","text","aZ")),Action::Click("redo"),Action::Wait(Check::PlanCell("A2","text","ab")),Action::Into("name-box","A1:C5\u{E007}"),Action::Click("sort-descending"),Action::Wait(Check::PlanCell("A2","text","Aa"))
    ],checks:&[Check::PlanCell("A5","text","ab"),Check::PlanCell("C2","text","20"),Check::PlanCell("C5","text","30")]},
    Case {name:"custom sorting: list order and secondary descending use the normal command",actions:&[
        Action::Click("sort-case-sensitive"),Action::Into("name-box","A1\u{E007}"),Action::ExternalClipboard("Key\tAmount\tTotal\nLow\t1\t=B2*10\nHigh\t2\t=B3*10\nMedium\t3\t=B4*10\nHIGH\t4\t=B5*10",false),Action::Keys("grid","\u{E009}v\u{E000}"),Action::Wait(Check::Reachable("paste-confirm")),Action::Click("paste-confirm"),Action::Wait(Check::PlanCell("A5","text","HIGH")),Action::Into("name-box","A1:C5\u{E007}"),Action::Into("sort-custom-order","High\nMedium\nLow"),Action::Click("sort-add-key"),Action::Wait(Check::Count("sort-level-order-2",1)),Action::Choose("sort-level-order-2","desc"),Action::Click("sort-ascending"),Action::Wait(Check::PlanCell("C2","text","40"))
    ],checks:&[Check::PlanCell("A2","text","HIGH"),Check::PlanCell("C3","text","20"),Check::PlanCell("A4","text","Medium"),Check::PlanCell("A5","text","Low"),Check::PlanCell("C5","text","10")]},
    Case {name:"custom sorting: unlisted and duplicate entries refuse atomically",actions:&[
        Action::Into("sort-custom-order","High\nLow"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","Every nonblank")),Action::Into("sort-custom-order","High\nHIGH\nMedium\nLow"),Action::Click("sort-ascending"),Action::Wait(Check::TextContains("status","distinct"))
    ],checks:&[Check::PlanCell("A2","text","HIGH"),Check::PlanCell("C2","text","40"),Check::PlanCell("A5","text","Low")]},
    Case {name:"custom sorting: results reopen and controls fit a phone",actions:&[
        Action::Into("sort-custom-order","Low\nMedium\nHigh"),Action::Click("sort-ascending"),Action::Wait(Check::PlanCell("A2","text","Low")),Action::ClearDownloads,Action::Click("save"),Action::UploadDownload("Book1.xlsx"),Action::DiscardReplacement,Action::Wait(Check::PlanCell("C2","text","10")),Action::Wait(Check::PlanCell("A5","text","High")),Action::Wait(Check::PlanCell("C5","text","20")),Action::Window(390,844),Action::OpenDetails("sort-tools"),Action::OpenDetails("sort-options"),Action::Click("sort-custom-order")
    ],checks:&[Check::Fits,Check::Narrow(390),Check::Name("sort-custom-order","Custom order for first key"),Check::Reachable("sort-case-sensitive"),Check::Reachable("sort-custom-order")]},

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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
            Action::Wait(Check::Attr("[data-dnacalc=sheet-tab][data-sheet=Data]", "aria-selected", "true")),
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
            Action::Click("[data-dnacalc=sheet-tab][data-sheet=Data]"),
            Action::Wait(Check::Attr("[data-dnacalc=sheet-tab][data-sheet=Data]", "aria-selected", "true")),
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

    Case {
        name:"worker latency: a saved name preserves a newer new-name draft",
        actions:&[
            Action::Click("skin-grid"),Action::Click("new"),Action::DiscardReplacement,Action::PauseMillis(1000),
            Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("name-new"),Action::Into("name-name","First"),Action::Into("name-formula","=2"),Action::Click("name-save"),
            Action::Click("name-new"),Action::Into("name-name","Second"),Action::Into("name-formula","=3"),Action::PauseMillis(1200),
        ],
        checks:&[Check::Count("[data-name=First]",1),Check::Value("name-name","Second"),Check::Value("name-formula","=3")],
    },
    Case {
        name:"worker latency: a renamed name preserves a newer selected definition",
        actions:&[
            Action::Click("name-save"),Action::Wait(Check::Count("[data-name=Second]",1)),Action::Click("[data-name=First] [data-dnacalc=name-select]"),Action::Into("name-name","Renamed"),Action::Click("name-save"),
            Action::Click("[data-name=Second] [data-dnacalc=name-select]"),Action::PauseMillis(1200),
        ],
        checks:&[Check::Count("[data-name=Renamed]",1),Check::Count("[data-name=First]",0),Check::Value("name-name","Second"),Check::Value("name-formula","=3")],
    },
    Case {
        name:"worker latency: closing and reopening Names waits for its save and keeps the new draft",
        actions:&[
            Action::Click("[data-name=Renamed] [data-dnacalc=name-select]"),Action::Into("name-name","FinalName"),Action::Click("name-save"),Action::Click("names-close"),Action::Click("name-manager"),Action::Wait(Check::Rendered("names-dialog",true)),Action::Click("name-new"),Action::Into("name-name","Third"),Action::Into("name-formula","=4"),Action::PauseMillis(1200),
        ],
        checks:&[Check::Count("[data-name=FinalName]",1),Check::Value("name-name","Third"),Check::Value("name-formula","=4")],
    },

    Case {
        name: "worker latency: opening array details before selection publication keeps the owner's query alive",
        actions: &[
            Action::Click("skin-grid"),Action::Click("new"),Action::DiscardReplacement,
            Action::Wait(Check::Value("name-box","A1")),Action::PauseMillis(800),
            Action::Into(BAR,"=SEQUENCE(3,2)\u{E007}"),Action::Wait(Check::Text(cell!("B2"),"4")),
            Action::Into("name-box","D10\u{E007}"),Action::PauseMillis(800),
            Action::Into("name-box","B2\u{E007}"),Action::Click("result-toggle"),
            Action::Wait(Check::TextContains("array-detail-summary","Array anchor Sheet1!A1")),
            Action::PauseMillis(700),
        ],
        checks: &[Check::Attr("result-details","open",""),Check::Text("selected-value","4"),Check::Rendered("array-owner",true)],
    },
    Case {
        name: "worker latency: the retained array owner opens the real anchor and a new error still opens recovery",
        actions: &[
            Action::Click("array-owner"),Action::Wait(Check::Value("name-box","A1")),
            Action::Click("result-toggle"),Action::PauseMillis(700),Action::Wait(Check::Absent("[data-dnacalc=result-details][open]")),
            Action::Into("name-box","D1\u{E007}"),Action::Into(BAR,"=1/0\u{E007}"),
            Action::Into("name-box","D1\u{E007}"),Action::Wait(Check::Text("selected-value","#DIV/0!")),
        ],
        checks: &[Check::Attr("result-details","open",""),Check::Rendered("recover-edit",true)],
    },

    Case {
        name: "review latency: a saved note retains text typed before its reply",
        actions: &[
            Action::Wait(Check::PlanReady),
            Action::Upload("open-file", "crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx"),
            Action::DiscardReplacement,
            Action::Wait(Check::Text("document", "features.xlsx")),
            Action::Wait(Check::PlanReady),
            Action::Into("name-box", "A1\u{E007}"),
            Action::Wait(Check::ActiveSemantic("A1")),
            Action::Click("review-tools"),
            Action::Wait(Check::TextContains("review-note", "a comment")),
            Action::Click("note-edit"),
            Action::Wait(Check::Focused("note-text")),
            Action::Into("note-text", "First text"),
            Action::Click("note-save"),
            Action::Keys("note-text", " later"),
            // Position the caret after returning to the textarea, before the save reply.
            Action::Keys("note-text", "\u{E011}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}"),
            Action::PauseMillis(1200),
            Action::Keys("note-text", "!"),
        ],
        checks: &[
            Check::Value("note-text", "First! text later"),
            Check::Focused("note-text"),
            Check::TextContains("review-note", "First text"),
            Check::Rendered("note-editor", true),
        ],
    },
    Case {
        name: "review latency: the retained text can be saved with ordinary Undo and Redo",
        actions: &[
            Action::Click("note-save"),
            Action::Wait(Check::Absent("note-editor")),
            Action::Wait(Check::TextContains("review-note", "First! text later")),
            Action::Click("review-undo"),
            Action::Wait(Check::TextContains("review-note", "First text")),
            Action::Click("review-redo"),
        ],
        checks: &[Check::TextContains("review-note", "First! text later")],
    },
    Case {
        name: "review latency: cancelling and reopening the same note starts a new edit",
        actions: &[
            Action::Click("note-edit"),
            Action::Wait(Check::Focused("note-text")),
            Action::Into("note-text", "Submitted text"),
            Action::Click("note-save"),
            Action::Click("note-cancel"),
            Action::Click("note-edit"),
            Action::Into("note-text", "Submitted text"),
            Action::PauseMillis(1200),
        ],
        checks: &[
            Check::Value("note-text", "Submitted text"),
            Check::Focused("note-text"),
            Check::Rendered("note-editor", true),
            Check::TextContains("review-note", "Submitted text"),
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
        end: 449,
        core: 3,
        setup: &[],
    },
    CaseGroup {
        name: "renderer",
        start: 449,
        end: 452,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "layout",
        start: 452,
        end: 456,
        core: 4,
        setup: &[],
    },
    CaseGroup {
        name: "format-cells",
        start: 456,
        end: 464,
        core: 8,
        setup: &[],
    },
    CaseGroup {name:"name-manager",start:464,end:474,core:10,setup:&[]},
    CaseGroup {name:"large-format",start:474,end:480,core:6,setup:&[]},
    CaseGroup {name:"physical-styles",start:480,end:487,core:7,setup:&[]},
    CaseGroup {name:"clipboard-history",start:487,end:490,core:3,setup:&[]},
    CaseGroup {name:"array-recovery",start:490,end:495,core:5,setup:&[]},
    CaseGroup {name:"phone-ledger",start:495,end:498,core:3,setup:&[]},
    CaseGroup {name:"phone-title",start:498,end:500,core:2,setup:&[]},
    CaseGroup {name:"friendly-functions",start:500,end:504,core:4,setup:&[]},
    CaseGroup {name:"notebook-actions",start:504,end:507,core:3,setup:&[]},
    CaseGroup {name:"transpose",start:507,end:511,core:4,setup:&[]},
    CaseGroup {name:"transpose-modes",start:511,end:517,core:6,setup:&[]},
    CaseGroup {name:"name-changes",start:517,end:527,core:10,setup:&[]},
    CaseGroup {name:"numeric-sort",start:527,end:533,core:6,setup:&[]},
    CaseGroup {name:"numeric-sort-limits",start:533,end:536,core:3,setup:&[]},
    CaseGroup {name:"shrink",start:536,end:541,core:5,setup:&[]},
    CaseGroup {name:"autofilter",start:541,end:549,core:8,setup:&[]},
    CaseGroup {name:"general-sort",start:549,end:555,core:6,setup:&[]},
    CaseGroup {name:"session-view",start:555,end:560,core:5,setup:&[]},
    CaseGroup {name:"axis-outline",start:560,end:566,core:6,setup:&[]},
    CaseGroup {name:"date-top-filters",start:566,end:575,core:9,setup:&[]},
    CaseGroup {name:"qualified-sort",start:575,end:580,core:5,setup:&[]},
    CaseGroup {name:"conditional-authoring",start:580,end:588,core:8,setup:&[]},
CaseGroup {
        name: "review-editing",
        start: 588,
        end: 594,
        core: 6,
        setup: &[],
    },
    CaseGroup {
        name: "validation-audit",
        start: 594,
        end: 601,
        core: 7,
        setup: &[],
    },
    CaseGroup {name:"candidate-validation",start:601,end:606,core:5,setup:&[]},
    CaseGroup {name:"review-create",start:606,end:612,core:6,setup:&[]},
    CaseGroup {name:"tables",start:612,end:625,core:13,setup:&[]},
    CaseGroup {name:"chart-import",start:625,end:628,core:3,setup:&[]},
    CaseGroup {name:"font-variants",start:628,end:634,core:6,setup:&[]},
    CaseGroup {name:"stacked-fitting",start:634,end:640,core:6,setup:&[]},
    CaseGroup { name: "conditional-colour-filters", start: 640, end: 646, core: 6, setup: &[] },
    CaseGroup {name:"sort-metadata",start:646,end:651,core:5,setup:&[]},
    CaseGroup {name:"table-sorting",start:651,end:657,core:6,setup:&[]},
    CaseGroup {name:"templates",start:657,end:670,core:13,setup:&[]},
    CaseGroup {name:"sort-options",start:670,end:675,core:5,setup:&[]},
];
