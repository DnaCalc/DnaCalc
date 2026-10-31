//! Browser smoke definitions consumed by xtask; not a standalone test target.
use super::*;

/// DNA OneCalc's original case order; ONECALC_GROUPS marks fresh-page boundaries.
/// The formulas group starts with A1 = 1.5, A2 = 2, B1 = hello, B2 = TRUE, A3 blank.
pub(crate) const ONECALC_CASES: &[Case] = &[
    Case {
        name: "the page loads with the surface and an empty formula",
        actions: &[
            Action::Grid("A1", "1.5"),
            Action::Grid("A2", "2"),
            Action::Grid("B1", "hello"),
            Action::Grid("B2", "TRUE"),
        ],
        checks: &[
            Check::Text("result", "no formula"),
            Check::Text("sheets", "Sheet1"),
            Check::Text("formula-cell-status", "Sheet1!H1"),
        ],
    },
    Case {
        name: "arithmetic: =1+2 is the number 3",
        actions: &[Action::Formula("=1+2")],
        checks: &[
            Check::Text("result", "number 3"),
            Check::Text("result-type", "number"),
            Check::Text("result-value", "3"),
            Check::Starts(
                "provenance",
                "calculated in Sheet1!H1 by calc-fml's evaluator with calc-func's functions; no cells read",
            ),
            Check::Items("diagnostics", &[]),
        ],
    },
    Case {
        name: "a grid range with a blank cell: =SUM(A1:A3) is 3.5",
        actions: &[Action::Formula("=SUM(A1:A3)")],
        checks: &[
            Check::Text("result", "number 3.5"),
            Check::Items(
                "reads",
                &["Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 = number 2; Sheet1!A3 blank"],
            ),
            Check::Starts("provenance", "calculated in Sheet1!H1"),
            Check::Highlighted("function", "SUM"),
        ],
    },
    Case {
        name: "text from the grid: =B1&\" world\"",
        actions: &[Action::Formula("=B1&\" world\"")],
        checks: &[
            Check::Text("result-type", "text"),
            Check::Text("result-value", "hello world"),
        ],
    },
    Case {
        name: "a logical from the grid: =IF(B2,A1*10,0) is 15",
        actions: &[Action::Formula("=IF(B2,A1*10,0)")],
        checks: &[Check::Text("result", "number 15")],
    },
    Case {
        name: "an array constant times 10 is a 2 x 2 grid",
        actions: &[Action::Formula("={1,2;3,4}*10")],
        checks: &[
            Check::Text("result-type", "array"),
            Check::Starts("result-shape", "2 x 2 array"),
            Check::Array(&[&["10", "20"], &["30", "40"]]),
        ],
    },
    Case {
        name: "a grid range lifted: =A1:A2*2 is a 2 x 1 grid",
        actions: &[Action::Formula("=A1:A2*2")],
        checks: &[Check::Array(&[&["3"], &["4"]])],
    },
    Case {
        name: "an error value: =1/0 is #DIV/0!, shown with its meaning",
        actions: &[Action::Formula("=1/0")],
        checks: &[
            Check::Text("result-type", "error"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("result-error", "division by zero"),
        ],
    },
    Case {
        name: "a syntax error is a diagnostic and no value",
        actions: &[Action::Formula("=A1*")],
        checks: &[
            Check::Text("result", "not calculated"),
            Check::Text("result-type", "no value"),
            Check::Contains("diagnostics", "syntax at 4..4: "),
        ],
    },
    Case {
        name: "the complete user-function registry gives an unknown name #NAME?",
        actions: &[Action::Formula("=NOSUCH(1)")],
        checks: &[
            Check::Text("result", "error #NAME?"),
            Check::Text("result-value", "#NAME?"),
            Check::Items("diagnostics", &[]),
        ],
    },
    Case {
        name: "completion: =SUM lists SUM first; Tab accepts it with its parenthesis",
        actions: &[Action::Formula("=SUM")],
        checks: &[
            Check::Text("completion-label", "SUM"),
            Check::Text("help-name", "SUM"),
        ],
    },
    Case {
        name: "completion accepted: signature help marks the first argument",
        actions: &[Action::Keys("formula", TAB)],
        checks: &[
            Check::Value("formula", "=SUM("),
            Check::Absent("completions"),
            Check::Text("active-parameter", "number1"),
            Check::Text("signature", "SUM(number1, [number2], ...)"),
            Check::Text("help-name", "SUM"),
            Check::Text(
                "help-description",
                "Use this function to add the values in cells.",
            ),
            Check::Text("help-source", "Source: Microsoft"),
        ],
    },
    Case {
        name: "the arguments typed after completion calculate",
        actions: &[Action::Keys("formula", "A1:A2,10)")],
        checks: &[
            Check::Value("formula", "=SUM(A1:A2,10)"),
            Check::Text("result", "number 13.5"),
        ],
    },
    Case {
        name: "signature help follows the argument the caret is in",
        actions: &[Action::Formula("=ROUND(A1,")],
        checks: &[
            Check::Text("active-parameter", "num_digits"),
            Check::Text("help-name", "ROUND"),
        ],
    },
    Case {
        name: "a number shows as a cell shows it, with the exact decimal beside",
        actions: &[Action::Formula("=0.1+0.2")],
        checks: &[
            Check::Text("result-value", "0.3"),
            Check::Text("result-exact", " exactly 0.30000000000000004"),
        ],
    },
    Case {
        name: "a date function reads the 1900 date system: =DATE(2024,1,31) is 45322",
        actions: &[Action::Formula("=DATE(2024,1,31)")],
        checks: &[Check::Text("result", "number 45322")],
    },
    Case {
        name: "a number format reads the en-US profile: =TEXT(1234.5,\"#,##0.00\")",
        actions: &[Action::Formula("=TEXT(1234.5,\"#,##0.00\")")],
        checks: &[Check::Text("result", "text \"1,234.50\"")],
    },
    Case {
        name: "NOW reads the browser's clock: =NOW()>45000 is TRUE",
        actions: &[Action::Formula("=NOW()>45000")],
        checks: &[Check::Text("result", "logical TRUE")],
    },
    Case {
        name: "the legacy form takes one value: ={1,2;3,4}*10 is 10",
        actions: &[
            Action::Choose("formula-form", "legacy"),
            Action::Formula("={1,2;3,4}*10"),
        ],
        checks: &[Check::Text("result", "number 10")],
    },
    Case {
        name: "back in the dynamic-array form the same formula is a 2 x 2 array",
        actions: &[Action::Choose("formula-form", "dynamic")],
        checks: &[Check::Array(&[&["10", "20"], &["30", "40"]])],
    },
    Case {
        name: "the function list finds ROUND by name and MROUND too",
        actions: &[
            Action::Formula("=1+"),
            Action::Into("function-filter", "round"),
        ],
        checks: &[
            Check::Contains("functions", "ROUND(number, num_digits)"),
            Check::Contains("functions", "MROUND(number, multiple)"),
        ],
    },
    Case {
        name: "a click in the function list inserts the call at the caret",
        actions: &[Action::Click(
            "[data-onecalc=\"function-entry\"][data-name=\"ROUND\"]",
        )],
        checks: &[
            Check::Value("formula", "=1+ROUND("),
            Check::Text("active-parameter", "number"),
            Check::Text("help-name", "ROUND"),
        ],
    },
    Case {
        name: "Escape dismisses the completion list",
        actions: &[Action::Formula("=AVER"), Action::Keys("formula", ESCAPE)],
        checks: &[
            Check::Absent("completions"),
            Check::Value("formula", "=AVER"),
        ],
    },
    // dc-xib: Strand's themes and a narrow screen.
    Case {
        name: "the theme switch shows the dark theme, and the system's again",
        actions: &[Action::Choose("theme", "cockpit-dark")],
        checks: &[
            Check::Computed("[data-onecalc=\"surface\"]", "color:#E2ECEF"),
            Check::Computed("body", "background-color:#1A262B"),
        ],
    },
    Case {
        name: "high contrast, then the system's theme",
        actions: &[
            Action::Choose("theme", "high-contrast"),
            Action::Choose("theme", ""),
        ],
        checks: &[Check::Computed("body", "background-color:#FFFFFF")],
    },
    Case {
        name: "on a 500-pixel screen (the narrowest a desktop Firefox window goes) the page fits its width",
        actions: &[Action::Window(500, 844)],
        checks: &[
            Check::Narrow(500),
            Check::Fits,
            Check::Text("sheets", "Sheet1"),
        ],
    },
    Case {
        name: "back on a wide screen",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    // dc-snz.3: the shipped page uses the same registry and bounded interpreter as the controller.
    Case {
        name: "load a VBA module and evaluate Add2(A1,2)",
        actions: &[
            Action::Click("vba-tools"),
            Action::Grid("A1", "40"),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Add2(A1,2)"),
        ],
        checks: &[
            Check::Text("result", "number 42"),
            Check::Starts("vba-status", "Loaded:"),
            Check::Items("diagnostics", &[]),
        ],
    },
    Case {
        name: "reload the VBA module and replace its code",
        actions: &[
            Action::Into(
                "vba-source",
                "Public Function Add2(a, b)\n Add2 = a + b + 10\nEnd Function\n",
            ),
            Action::Click("[data-onecalc=\"vba-load\"]"),
        ],
        checks: &[Check::Text("result", "number 52")],
    },
    Case {
        name: "an infinite VBA loop stops with a worksheet error and diagnostic",
        actions: &[
            Action::Into(
                "vba-source",
                "Public Function Spin()\n On Error Resume Next\n Do\n Loop\nEnd Function\n",
            ),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Spin()"),
        ],
        checks: &[
            Check::Text("result-value", "#VALUE!"),
            Check::Contains("diagnostics", "SPIN: execution budget"),
        ],
    },
    Case {
        name: "unloading a VBA module removes its functions",
        actions: &[Action::Click("[data-onecalc=\"vba-clear\"]")],
        checks: &[
            Check::Text("result-value", "#NAME?"),
            Check::Text("vba-status", "No VBA module loaded."),
        ],
    },
    Case {
        name: "sessions: replacing unexported inputs asks and Escape preserves them",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Click("fixture-new"),
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Keys("fixture-keep", ESCAPE),
        ],
        checks: &[
            Check::Value("formula", "=Spin()"),
            Check::Focused("formula"),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "workbench: a blank session offers a working SUM example with blank A3",
        actions: &[
            Action::Click("fixture-new"),
            Action::Click("fixture-replace"),
            Action::Wait(Check::Text("result-state", "Ready")),
            Action::Click("try-example"),
        ],
        checks: &[
            Check::Text("result-value", "3.5"),
            Check::Text("result-state", "Calculated"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A3]", ""),
            Check::Absent("[data-onecalc=result-details][open]"),
            Check::Count(
                "[data-onecalc=primary-result] [data-onecalc=result-value]",
                1,
            ),
            Check::Reachable("result-value"),
        ],
    },
    Case {
        name: "workbench: format and locale controls render the real calculation through the shared formatter",
        actions: &[
            Action::Click("format-tools"),
            Action::Choose("locale", "de-DE"),
            Action::Into("number-format", "0,00"),
        ],
        checks: &[
            Check::Text("result-value", "3,50"),
            Check::Text("result", "number 3.5"),
            Check::Text("format-note", ""),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "sessions: JSON export starts a real file download and records its snapshot",
        actions: &[Action::ClearDownloads, Action::Click("fixture-export")],
        checks: &[
            Check::Starts("fixture-status", "Download started: onecalc-session.json."),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "workbench: a worksheet error is a calculated error with its meaning",
        actions: &[Action::Formula("=1/0")],
        checks: &[
            Check::Text("result-state", "Worksheet error"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("result-error", "division by zero"),
            Check::Text("fixture-state", "Unexported changes"),
        ],
    },
    Case {
        name: "sessions: file import waits before discarding a changed formula",
        actions: &[
            Action::UploadDownload("onecalc-session.json"),
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Click("fixture-keep"),
        ],
        checks: &[
            Check::Value("formula", "=1/0"),
            Check::Text("result-value", "#DIV/0!"),
        ],
    },
    Case {
        name: "sessions: file restore restores grid inputs, formula, locale and format and recalculates",
        actions: &[
            Action::UploadDownload("onecalc-session.json"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Value("locale", "de-DE"),
            Check::Value("number-format", "0,00"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A1]", "1.5"),
            Check::Text("result-value", "3,50"),
            Check::Text("result-state", "Calculated"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "sessions: opening a shared fragment in this tab protects and replaces changed inputs",
        actions: &[
            Action::Click("fixture-share"),
            Action::RememberFieldLink("share-link"),
            Action::Formula("=1/0"),
            Action::OpenRememberedLink,
            Action::Wait(Check::Role("fixture-dialog", "dialog")),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Text("result-value", "3,50"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "sessions: a generated link restores its authored session in a fresh page",
        actions: &[
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
            Action::Wait(Check::Text("result-value", "3,50")),
            Action::Keys("formula", "\u{E009}z\u{E000}"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Value("locale", "de-DE"),
            Check::Text("result-state", "Calculated"),
            Check::Text("fixture-state", "Unchanged snapshot"),
        ],
    },
    Case {
        name: "workbench: the array example uses the shared bounded result window",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Click("example-array"),
        ],
        checks: &[
            Check::Array(&[&["1", "2"], &["3", "4"], &["5", "6"]]),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "workbench: the LAMBDA example executes with shared formula assistance",
        actions: &[
            Action::Click("example-lambda"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Text("result-value", "42"),
            Check::Text("result-state", "Calculated"),
            Check::Highlighted("function", "LAMBDA"),
        ],
    },
    Case {
        name: "workbench: a missing fixture input names the cell that needs a value",
        actions: &[
            Action::Click("example-missing"),
            Action::Click("fixture-replace"),
        ],
        checks: &[
            Check::Text("result-state", "Missing input"),
            Check::Contains("diagnostics", "missing input: Sheet1!Z99 has no input"),
        ],
    },
    Case {
        name: "workbench: typed input controls have names and resolve the missing value",
        actions: &[
            Action::Click("add-input"),
            Action::Into("input-cell", "Z99"),
            Action::Into("input-value", "4"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::Name("input-cell", "Input 1 cell"),
            Check::Name("input-value", "Input 1 value"),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "sessions: links also restore typed inputs through the normal reader",
        actions: &[
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
            Action::Wait(Check::Text("result-value", "5")),
        ],
        checks: &[
            Check::Value("input-cell", "Z99"),
            Check::Value("input-value", "4"),
            Check::Text("result-state", "Calculated"),
        ],
    },
    Case {
        name: "sessions: an unqualified formula-address draft restores its effective sheet",
        actions: &[
            Action::Into("sheet-name", "Inputs"),
            Action::Click("add-sheet"),
            Action::Into("formula-cell", "Inputs!H1"),
            Action::Into("formula-cell", "H1"),
            Action::Click("fixture-tools"),
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::Text("formula-cell-status", "Inputs!H1"),
            Check::Value("formula-cell", "H1"),
        ],
    },
    Case {
        name: "sessions: an invalid formula-address draft also retains its effective anchor",
        actions: &[
            Action::Into("formula-cell", "invalid!"),
            Action::Click("fixture-tools"),
            Action::Click("fixture-share"),
            Action::OpenFieldLink("share-link"),
        ],
        checks: &[
            Check::Text("result-value", "5"),
            Check::TextContains("formula-cell-status", "the formula stays in Inputs!H1"),
            Check::Value("formula-cell", "invalid!"),
        ],
    },
    Case {
        name: "sessions: invalid JSON is refused before the current formula or inputs change",
        actions: &[
            Action::Click("fixture-tools"),
            Action::Into("fixture-json", "this is not JSON"),
            Action::Click("fixture-restore"),
        ],
        checks: &[
            Check::Starts("fixture-status", "Invalid formula-session JSON:"),
            Check::Text("result-value", "5"),
            Check::Value("input-value", "4"),
            Check::Absent("[data-onecalc=fixture-dialog][open]"),
        ],
    },
    Case {
        name: "workbench: unknown names or calls, missing input and other worksheet errors remain distinct",
        actions: &[Action::Formula("=NO_SUCH_ONECALC_FUNCTION()")],
        checks: &[
            Check::Text("result-state", "Unknown name or function"),
            Check::Text("result-value", "#NAME?"),
            Check::Text("result-error", "a name that is not recognised"),
        ],
    },
    Case {
        name: "workbench: phone layout keeps the primary result and inputs ahead of the catalogue",
        actions: &[
            Action::Formula("=Z99+1"),
            Action::Click("fixture-tools"),
            Action::Window(390, 844),
            Action::Click("result-value"),
        ],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("primary-result"),
            Check::Before("grid", "functions"),
            Check::Text("result-value", "5"),
        ],
    },
    Case {
        name: "workbench: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
];

pub(crate) const ONECALC_GROUPS: &[CaseGroup] = &[
    CaseGroup {
        name: "formulas",
        start: 0,
        end: 27,
        core: 5,
        setup: &[],
    },
    CaseGroup {
        name: "vba",
        start: 27,
        end: 31,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "sessions",
        start: 31,
        end: 51,
        core: 3,
        setup: &[Action::Formula("=Spin()")],
    },
];
