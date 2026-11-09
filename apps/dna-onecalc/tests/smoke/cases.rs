//! Browser smoke definitions consumed by xtask; not a standalone test target.
use super::*;

/// WebDriver's code for F1.
const F1: &str = "\u{E031}";

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
        name: "Ctrl+Shift+Enter enters the legacy array form, which takes one value: ={1,2;3,4}*10 is 10, shown as {=...}",
        actions: &[
            Action::Formula("={1,2;3,4}*10"),
            Action::Keys("formula", CTRL_SHIFT_ENTER),
        ],
        checks: &[
            Check::Text("result", "number 10"),
            Check::Text("array-formula", "{={1,2;3,4}*10}"),
            Check::Rendered("array-formula", true),
        ],
    },
    Case {
        name: "editing the formula leaves the legacy form: the same text is a 2 x 2 array again",
        actions: &[Action::Formula("={1,2;3,4}*10")],
        checks: &[
            Check::Array(&[&["10", "20"], &["30", "40"]]),
            Check::Rendered("array-formula", false),
        ],
    },
    Case {
        name: "there is no form or numeric-mode selector on the page",
        actions: &[],
        checks: &[
            Check::Absent("[data-onecalc=formula-form]"),
            Check::Absent("[data-onecalc=numeric-mode]"),
        ],
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
            Check::Text("formula-cell-status", "Sheet1!H1"),
        ],
    },
    Case {
        name: "back on a wide screen",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    // dc-snz.3: the shipped page uses the same registry and bounded interpreter as the controller.
    Case {
        name: "load a VBA module and evaluate Module1.Add2(A1,2)",
        actions: &[
            Action::Click("vba-tools"),
            Action::Grid("A1", "40"),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Module1.Add2(A1,2)"),
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
    // The files group runs in the browsers and in the desktop exe's WebView2: a file reaches the
    // session through the page (the file input, as a drop does) and the link answers.
    Case {
        name: "files: a session workbook opens through the page: formula, calculation and file state",
        actions: &[
            Action::WriteSession("opened.xlsx", "SUM(1,2)+3"),
            Action::OpenDetails("file-controls"),
            Action::UploadDownload("opened.xlsx"),
            Action::Wait(Check::Text("result-state", "Calculated")),
        ],
        checks: &[
            Check::Value("formula", "=SUM(1,2)+3"),
            Check::Text("result", "number 6"),
            Check::Starts("file-status", "Opened opened.xlsx"),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },
    Case {
        name: "files: opening over unsaved changes asks, and Escape keeps them",
        actions: &[
            Action::Formula("=1/0"),
            Action::WriteSession("second.xlsx", "LEN(\"abc\")"),
            Action::UploadDownload("second.xlsx"),
            Action::Wait(Check::Role("file-dialog", "dialog")),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Keys("file-keep", ESCAPE),
        ],
        checks: &[
            Check::Value("formula", "=1/0"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("file-state", "Unsaved changes"),
        ],
    },
    Case {
        name: "files: Replace opens the file over the unsaved changes",
        actions: &[
            Action::UploadDownload("second.xlsx"),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Value("formula", "=LEN(\"abc\")"),
            Check::Text("result", "number 3"),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },
    Case {
        name: "files: a file outside the constraints is refused with the offer to open it in DNA Calc, and nothing changes",
        actions: &[
            Action::WriteDownload("not-a-session.txt", b"this is not a workbook"),
            Action::UploadDownload("not-a-session.txt"),
        ],
        checks: &[
            Check::Starts(
                "file-status",
                "This file is outside what a OneCalc session holds:",
            ),
            Check::TextContains("file-status", "Open in DNA Calc."),
            Check::Value("formula", "=LEN(\"abc\")"),
            Check::Text("result", "number 3"),
            Check::Absent("[data-onecalc=file-dialog][open]"),
        ],
    },
    Case {
        name: "sessions: replacing unsaved changes asks and Escape preserves them",
        actions: &[
            Action::OpenDetails("file-controls"),
            Action::Click("file-new"),
            Action::Wait(Check::Role("file-dialog", "dialog")),
            Action::Keys("file-keep", ESCAPE),
        ],
        checks: &[
            Check::Value("formula", "=Spin()"),
            Check::Focused("formula"),
            Check::Text("file-state", "Unsaved changes"),
        ],
    },
    Case {
        name: "workbench: a blank session offers a working SUM example with blank A3",
        actions: &[
            Action::Click("file-new"),
            Action::Click("file-replace"),
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
            Check::Text("file-state", "Unsaved changes"),
        ],
    },
    Case {
        name: "sessions: Save starts a real .xlsx download that the codec reads as the session, and records its snapshot",
        actions: &[Action::ClearDownloads, Action::Click("file-save")],
        checks: &[
            Check::Starts("file-status", "Download started: onecalc-session.xlsx."),
            Check::Text("file-state", "Unchanged since opened or saved"),
            Check::DownloadedSession("onecalc-session.xlsx", "/formula", "SUM(A1:A3)"),
            Check::DownloadedSession("onecalc-session.xlsx", "/sheet", "Sheet1"),
            Check::DownloadedSession("onecalc-session.xlsx", "/cell/A1", "1.5"),
            Check::DownloadedSession("onecalc-session.xlsx", "/cell/A2", "2"),
            Check::DownloadedSession("onecalc-session.xlsx", "/cell/A3", ""),
            Check::DownloadedSession("onecalc-session.xlsx", "/locale", "de-DE"),
            Check::DownloadedSession("onecalc-session.xlsx", "/number-format", "0,00"),
        ],
    },
    Case {
        name: "workbench: a worksheet error is a calculated error with its meaning",
        actions: &[Action::Formula("=1/0")],
        checks: &[
            Check::Text("result-state", "Worksheet error"),
            Check::Text("result-value", "#DIV/0!"),
            Check::Text("result-error", "division by zero"),
            Check::Text("file-state", "Unsaved changes"),
        ],
    },
    Case {
        name: "sessions: opening a file waits before discarding a changed formula",
        actions: &[
            Action::UploadDownload("onecalc-session.xlsx"),
            Action::Wait(Check::Role("file-dialog", "dialog")),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-keep"),
        ],
        checks: &[
            Check::Value("formula", "=1/0"),
            Check::Text("result-value", "#DIV/0!"),
        ],
    },
    Case {
        name: "sessions: opening the file restores grid constants, formula, locale and format and recalculates",
        actions: &[
            Action::UploadDownload("onecalc-session.xlsx"),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Value("formula", "=SUM(A1:A3)"),
            Check::Value("locale", "de-DE"),
            Check::Value("number-format", "0,00"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A1]", "1.5"),
            Check::Text("result-value", "3,50"),
            Check::Text("result-state", "Calculated"),
            Check::Text("file-state", "Unchanged since opened or saved"),
            Check::Starts("file-status", "Opened onecalc-session.xlsx"),
        ],
    },
    Case {
        name: "sessions: the formula's cell is saved with the file and restored from it",
        actions: &[
            Action::Into("formula-cell", "C7"),
            Action::ClearDownloads,
            Action::Click("file-save"),
            Action::Into("formula-cell", "B9"),
            Action::UploadDownload("onecalc-session.xlsx"),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Value("formula-cell", "C7"),
            Check::Text("formula-cell-status", "Sheet1!C7"),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },
    Case {
        name: "sessions: an invalid formula-cell draft keeps the effective anchor, and the file keeps it",
        actions: &[
            Action::Into("formula-cell", "invalid!"),
            Action::ClearDownloads,
            Action::Click("file-save"),
            Action::UploadDownload("onecalc-session.xlsx"),
            Action::Wait(Check::Value("formula-cell", "C7")),
        ],
        checks: &[
            Check::Value("formula-cell", "C7"),
            Check::Text("formula-cell-status", "Sheet1!C7"),
            Check::Text("result-value", "3,50"),
        ],
    },
    Case {
        name: "workbench: the array example uses the shared bounded result window",
        actions: &[
            Action::OpenDetails("file-controls"),
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
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Text("result-value", "42"),
            Check::Text("result-state", "Calculated"),
            Check::Highlighted("function", "LAMBDA"),
        ],
    },
    Case {
        name: "workbench: a missing input names the cell that needs a value",
        actions: &[
            Action::Click("example-missing"),
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Text("result-state", "Missing input"),
            Check::Contains("diagnostics", "missing input: Sheet1!Z99 has no input"),
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
            Action::Formula("=A1*2"),
            Action::Grid("A1", "21"),
            Action::OpenDetails("file-controls"),
            Action::Window(390, 844),
            Action::Click("result-value"),
        ],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Reachable("primary-result"),
            Check::Before("grid", "functions"),
            Check::Text("result-value", "42"),
        ],
    },
    Case {
        name: "workbench: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    Case {
        name: "evaluation context: ROW and COLUMN use the formula instance in C7",
        actions: &[
            Action::OpenDetails("file-controls"),
            Action::Into("formula-cell", "C7"),
            Action::Formula("=ROW()*100+COLUMN()"),
        ],
        checks: &[
            Check::Text("formula-cell-status", "Sheet1!C7"),
            Check::Text("result-value", "703"),
            Check::Starts("provenance", "calculated in Sheet1!C7"),
        ],
    },
    Case {
        name: "friendly functions: an opened session keeps the file's call spelling while showing the entry spelling",
        actions: &[
            Action::WriteSession("friendly.xlsx", "_xlfn.SEQUENCE(2,2)"),
            Action::OpenDetails("file-controls"),
            Action::UploadDownload("friendly.xlsx"),
            Action::Wait(Check::Text("result-state", "Calculated")),
        ],
        checks: &[
            Check::Value("formula", "=SEQUENCE(2,2)"),
            Check::Array(&[&["1", "2"], &["3", "4"]]),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },
    Case {
        name: "friendly functions: an unchanged field saves the file's spelling",
        actions: &[Action::Click("formula"), Action::Click("file-status"), Action::ClearDownloads, Action::Click("file-save")],
        checks: &[
            Check::DownloadedSession("onecalc-session.xlsx", "/formula", "_xlfn.SEQUENCE(2,2)"),
            Check::DownloadedSession("onecalc-session.xlsx", "/array", "Dynamic"),
            Check::Value("formula", "=SEQUENCE(2,2)"),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },
    Case {
        name: "friendly functions: editing and saving writes the new entry with the file's prefix and the actual calculation",
        actions: &[
            Action::Formula("=SEQUENCE(1,3)"),
            Action::ClearDownloads,
            Action::Click("file-save"),
        ],
        checks: &[
            Check::DownloadedSession("onecalc-session.xlsx", "/formula", "_xlfn.SEQUENCE(1,3)"),
            Check::Value("formula", "=SEQUENCE(1,3)"),
            Check::Array(&[&["1", "2", "3"]]),
            Check::Text("file-state", "Unchanged since opened or saved"),
        ],
    },

    Case {
        name: "completion repair: an edited function name retains its argument list",
        actions: &[Action::Formula("=SUMX(4,5)"), Action::Keys("formula", "\u{E011}\u{E014}\u{E014}\u{E014}\u{E014}\u{E004}")],
        checks: &[Check::Value("formula", "=SUM(4,5)"), Check::Text("active-parameter", "number1"), Check::Text("result", "number 9")],
    },
    Case {
        name: "completion repair: accepting an existing function keeps one opening parenthesis",
        actions: &[Action::Formula("=SUM(6,7)"), Action::Keys("formula", "\u{E011}\u{E014}\u{E014}\u{E014}\u{E014}\u{E004}")],
        checks: &[Check::Value("formula", "=SUM(6,7)"), Check::Text("active-parameter", "number1"), Check::Text("result", "number 13")],
    },

    // Milestone 3, formula work: completion over names, locals and registered functions, function
    // status, F9 on a part, the evaluation trace and point mode.
    Case {
        name: "completion: a LET name is offered in the body and accepted without a parenthesis",
        actions: &[Action::Formula("=LET(alpha,1,al")],
        checks: &[Check::Contains("completions", "alpha")],
    },
    Case {
        name: "completion accepted: the LET name replaces the prefix",
        actions: &[Action::Keys("formula", TAB)],
        checks: &[Check::Value("formula", "=LET(alpha,1,alpha"), Check::Absent("completions")],
    },
    Case {
        name: "completion: a LAMBDA parameter is offered in its body, not after the call",
        actions: &[Action::Formula("=LAMBDA(width,wi")],
        checks: &[Check::Contains("completions", "width")],
    },
    Case {
        name: "completion: a registered function is offered beside the catalogue's",
        actions: &[
            Action::Click("vba-tools"),
            Action::Click("[data-onecalc=\"vba-load\"]"),
            Action::Formula("=Module1.Ad"),
        ],
        checks: &[Check::Contains("completions", "MODULE1.ADD2")],
    },
    Case {
        name: "function status: a held function says so with its availability",
        actions: &[Action::Formula("=SUM")],
        checks: &[Check::Starts("help-status", "Held; Availability not established")],
    },
    Case {
        name: "function status: a function this build does not hold names its owner",
        actions: &[Action::Formula("=RTD")],
        checks: &[Check::Starts("help-status", "Not held; owned by")],
    },
    Case {
        name: "F9: the selected part is replaced by its value, selected, and the result follows",
        actions: &[
            Action::Formula("=SUM(1,2)+3"),
            Action::Keys(
                "formula",
                "\u{E011}\u{E014}\u{E008}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E014}\u{E000}\u{E039}",
            ),
        ],
        checks: &[
            Check::Value("formula", "=3+3"),
            Check::SelectedText("formula", "3"),
            Check::Text("result", "number 6"),
            Check::TextContains("part-status", "SUM(1,2) evaluated to 3"),
        ],
    },
    Case {
        name: "F9: Escape restores the formula and selects the part again",
        actions: &[Action::Keys("formula", ESCAPE)],
        checks: &[
            Check::Value("formula", "=SUM(1,2)+3"),
            Check::SelectedText("formula", "SUM(1,2)"),
            Check::Text("part-status", "Formula restored."),
            Check::Text("result", "number 6"),
        ],
    },
    Case {
        name: "F9: with nothing selected the page says to select a part, and the formula is unchanged",
        actions: &[
            Action::Formula("=1+1/0"),
            Action::Keys("formula", "\u{E039}"),
        ],
        checks: &[
            Check::Value("formula", "=1+1/0"),
            Check::TextContains("part-status", "Select the part"),
        ],
    },
    Case {
        name: "F9: a part inside LET sees the bindings before it",
        actions: &[
            Action::Formula("=LET(a,2,b,a*10,b+a)"),
            Action::Keys(
                "formula",
                "\u{E012}\u{E008}\u{E012}\u{E012}\u{E012}\u{E000}\u{E039}",
            ),
        ],
        checks: &[
            Check::Value("formula", "=LET(a,2,b,a*10,22)"),
            Check::Text("result", "number 22"),
        ],
    },
    Case {
        name: "trace: every part of the formula is a step, innermost first, with its value",
        actions: &[
            Action::Grid("A1", "1.5"),
            Action::Grid("A2", "2"),
            Action::Formula("=SUM(A1:A2)*2"),
            Action::Click("trace-start"),
        ],
        checks: &[
            Check::Items(
                "trace-steps",
                &["A1:A2 = {1.5;2}", "SUM(A1:A2) = 3.5", "SUM(A1:A2)*2 = 7"],
            ),
            Check::Text("trace-status", "Step 1 of 3"),
            Check::Count("[data-onecalc=highlight] .oc-focus", 3),
        ],
    },
    Case {
        name: "trace: Next step moves the formula's highlight to the part and its value",
        actions: &[Action::Click("trace-next")],
        checks: &[
            Check::Text("trace-status", "Step 2 of 3"),
            Check::Count("[data-onecalc=highlight] .oc-trace-current", 0),
            Check::Count("[data-onecalc=trace-steps] .oc-trace-current", 1),
        ],
    },
    Case {
        name: "trace: editing the formula marks the trace stale",
        actions: &[Action::Formula("=1+2")],
        checks: &[
            Check::TextContains("trace-status", "changed since this trace"),
            Check::Count("[data-onecalc=trace-step]", 0),
            Check::Count("[data-onecalc=highlight] .oc-focus", 0),
        ],
    },
    Case {
        name: "point mode: arrow keys after = insert the cell next to the formula's cell, and keep moving it",
        actions: &[
            Action::Formula("="),
            Action::Keys("formula", "\u{E012}"),
        ],
        checks: &[Check::Value("formula", "=G1"), Check::Count("[data-onecalc=grid] td.oc-pointed", 1)],
    },
    Case {
        name: "point mode: Down moves it, Shift+Down extends it to a range, and the grid marks the cells",
        actions: &[
            Action::Keys("formula", "\u{E015}"),
            Action::Keys("formula", "\u{E008}\u{E015}\u{E000}"),
        ],
        checks: &[
            Check::Value("formula", "=G2:G3"),
            Check::Count("[data-onecalc=grid] td.oc-pointed", 2),
        ],
    },
    Case {
        name: "point mode: typing an operator ends it, and a click on a grid cell then adds that cell",
        actions: &[
            Action::Keys("formula", "+"),
            Action::Click("[data-onecalc=grid-cell][data-cell=B3]"),
        ],
        checks: &[
            Check::Value("formula", "=G2:G3+B3"),
            Check::Focused("formula"),
            Check::Value("[data-onecalc=grid-cell][data-cell=B3]", ""),
        ],
    },
    Case {
        name: "point mode: a second click replaces the pointed cell, and Shift+click extends it",
        actions: &[
            Action::Click("[data-onecalc=grid-cell][data-cell=C2]"),
        ],
        checks: &[Check::Value("formula", "=G2:G3+C2"), Check::Count("[data-onecalc=grid] td.oc-pointed", 1)],
    },
    Case {
        name: "point mode: a click on a cell where no reference belongs edits the cell",
        actions: &[
            Action::Formula("=1"),
            Action::Into("[data-onecalc=grid-cell][data-cell=A1]", "7"),
        ],
        checks: &[
            Check::Value("formula", "=1"),
            Check::Value("[data-onecalc=grid-cell][data-cell=A1]", "7"),
        ],
    },

    Case {
        name: "entry: a formula Excel refuses on entry is refused with the reason, and one it takes is not",
        actions: &[Action::Formula("=ABS(3)(1)")],
        checks: &[
            Check::Contains("diagnostics", "Excel refuses this formula at"),
            Check::Text("result", "not calculated"),
        ],
    },
    // Format picker and date system.
    Case {
        name: "format picker: choosing a format sets the format box and the result shows it",
        actions: &[
            Action::Formula("=1234.5678"),
            Action::Click("format-tools"),
            Action::Choose("format-picker", "#,##0.00"),
        ],
        checks: &[
            Check::Value("number-format", "#,##0.00"),
            Check::Text("result-value", "1,234.57"),
            Check::Value("format-picker", "#,##0.00"),
        ],
    },
    Case {
        name: "format picker: another locale offers its own separators",
        actions: &[Action::Choose("locale", "de-DE")],
        checks: &[Check::Value("format-picker", "custom")],
    },
    Case {
        name: "format picker: the German choice formats with its own separators; the date system moves the date",
        actions: &[
            Action::Choose("format-picker", "#.##0,00"),
            Action::Formula("=DATE(2024,1,31)"),
            Action::Choose("format-picker", "yyyy-mm-dd"),
            Action::Choose("date-system", "1904"),
        ],
        checks: &[
            Check::Value("number-format", "yyyy-mm-dd"),
            Check::Text("result", "number 43860"),
            Check::Text("result-value", "2024-01-31"),
            Check::Value("date-system", "1904"),
        ],
    },
    // A plain <f> in a file is the legacy form (implicit intersection).
    Case {
        name: "files: a plain formula of a file is the legacy form, shown without braces and calculated by implicit intersection",
        actions: &[
            Action::WritePlainSession("plain.xlsx", "A1:A2*2"),
            Action::OpenDetails("file-controls"),
            Action::UploadDownload("plain.xlsx"),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-replace"),
            Action::Wait(Check::Text("result-state", "Calculated")),
        ],
        checks: &[
            Check::Value("formula", "=A1:A2*2"),
            Check::Text("result", "number 6"),
            Check::Rendered("plain-formula", true),
            Check::Rendered("array-formula", false),
        ],
    },
    Case {
        name: "files: editing a plain file formula enters it in the dynamic form, which spills",
        actions: &[Action::Formula("=A1:A2*2")],
        checks: &[
            Check::Array(&[&["6"], &["8"]]),
            Check::Rendered("plain-formula", false),
        ],
    },
    Case {
        name: "format files: Save keeps the 1904 date system and the format in the workbook",
        actions: &[
            Action::Click("format-tools"),
            Action::Choose("date-system", "1904"),
            Action::Formula("=DATE(2024,1,31)"),
            Action::Into("number-format", "yyyy-mm-dd"),
            Action::OpenDetails("file-controls"),
            Action::ClearDownloads,
            Action::Click("file-save"),
        ],
        checks: &[
            Check::DownloadedSession("onecalc-session.xlsx", "/date-system", "1904"),
            Check::DownloadedSession("onecalc-session.xlsx", "/number-format", "yyyy-mm-dd"),
            Check::DownloadedSession("onecalc-session.xlsx", "/formula", "DATE(2024,1,31)"),
        ],
    },
    Case {
        name: "format files: opening that workbook restores the 1904 date system",
        actions: &[
            Action::Choose("date-system", "1900"),
            Action::UploadDownload("onecalc-session.xlsx"),
            Action::Wait(Check::Rendered("file-dialog", true)),
            Action::Click("file-replace"),
        ],
        checks: &[
            Check::Value("date-system", "1904"),
            Check::Text("result", "number 43860"),
            Check::Text("result-value", "2024-01-31"),
        ],
    },
    Case {
        name: "add-ins: the panel lists none until one is loaded",
        actions: &[Action::OpenDetails("addin-controls")],
        checks: &[
            Check::Text("addin-count", "0 loaded"),
            Check::Items("addins-loaded", &[]),
            Check::Items("addins-offered", &[]),
        ],
    },
    // dc-0f1k.9: the page's wasm stack is 8 MiB (xtask's PAGE_STACK_BYTES) and its mount may spend
    // half of it.
    Case {
        name: "the wasm shadow stack is 8 MiB and the mount spent at most half of it (dc-0f1k.9)",
        actions: &[],
        checks: &[Check::StackMargin("__onecalcMemory", 8_000_000)],
    },
    // dc-0f1k (onecalc-web-1): the Inputs area, the array result that scrolls, the completion pop-up
    // at the caret, and the Formatting and Conditional Formatting panels.
    Case {
        name: "inputs: the scratch grid is a compact Inputs area, closed, with a one-line explanation",
        actions: &[],
        checks: &[
            Check::Rendered("grid", false),
            Check::Text("inputs-count", "no values entered"),
            Check::TextContains("inputs-note", "Type constants here, then refer to them, for example =SUM(A1:A3)"),
        ],
    },
    Case {
        name: "inputs: a formula that reads cells opens the Inputs area",
        actions: &[Action::Formula("=A1+1")],
        checks: &[Check::Rendered("grid", true), Check::Text("result", "number 1")],
    },
    Case {
        name: "inputs: a click closes it",
        actions: &[Action::Click("inputs-tools")],
        checks: &[Check::Rendered("grid", false)],
    },
    Case {
        name: "inputs: a click opens it again, a value typed there is counted and the formula reads it",
        actions: &[Action::Click("inputs-tools"), Action::Grid("A1", "41")],
        checks: &[
            Check::Rendered("grid", true),
            Check::Text("inputs-count", "1 cell(s) with values"),
            Check::Text("result-value", "42"),
        ],
    },
    Case {
        name: "inputs: closed by hand, an edit that still reads cells leaves it closed",
        actions: &[Action::Click("inputs-tools"), Action::Keys("formula", "+1")],
        checks: &[Check::Rendered("grid", false), Check::Text("result-value", "43")],
    },
    Case {
        name: "inputs: point mode opens it when the formula takes a reference",
        actions: &[Action::Formula("=")],
        checks: &[Check::Rendered("grid", false)],
    },
    Case {
        name: "inputs: an arrow key in the formula field puts a reference in and opens the Inputs area",
        actions: &[Action::Keys("formula", "\u{E012}")],
        checks: &[Check::Value("formula", "=G1"), Check::Rendered("grid", true)],
    },
    Case {
        name: "array result: a long spill draws only the rows in view, under fixed headers",
        actions: &[Action::Formula("=SEQUENCE(5000,3)")],
        checks: &[
            Check::Starts("result-shape", "5000 x 3 array"),
            Check::Bounded("[data-onecalc=result-array] tbody tr", 90),
            Check::Computed("[data-onecalc=result-array] thead th", "position:sticky"),
            Check::Computed("[data-onecalc=result-array] tbody th", "position:sticky"),
            Check::Text("[data-onecalc=result-array] tbody th", "1"),
            Check::Js("the panel is a scroll region of its own, no taller than 360px", "(function(){var p=document.querySelector('[data-onecalc=\"result-scroll\"]');var s=getComputedStyle(p);return p.clientHeight<=361&&(s.overflowY==='auto'||s.overflowY==='scroll')&&s.overscrollBehaviorY==='contain';})()"),
        ],
    },
    Case {
        name: "array result: scrolling to row 2,500 draws those rows, keeps the headers in view and does not move the page",
        actions: &[Action::ScrollTo("result-scroll", 60_000, 0)],
        checks: &[
            Check::Text("[data-onecalc=result-array] tbody tr:not([aria-hidden=true]) th", "2489"),
            Check::Bounded("[data-onecalc=result-array] tbody tr", 90),
            Check::ScrollTop("body", 0),
            Check::Js("the column headers stay at the top of the panel", "(function(){var p=document.querySelector('[data-onecalc=\"result-scroll\"]').getBoundingClientRect(),h=document.querySelector('[data-onecalc=\"result-array\"] thead th:nth-child(2)').getBoundingClientRect();return h.top>=p.top-1&&h.top<=p.top+2;})()"),
            Check::Js("rows near 2,500 are drawn at the scroll position", "(function(){var p=document.querySelector('[data-onecalc=\"result-scroll\"]').getBoundingClientRect(),r=Array.from(document.querySelectorAll('[data-onecalc=\"result-array\"] tbody th')).find(function(t){return t.textContent==='2501';});if(!r)return false;var b=r.getBoundingClientRect();return b.top>=p.top&&b.bottom<=p.bottom+1;})()"),
        ],
    },
    Case {
        name: "array result: editing the formula keeps the place in the result",
        actions: &[Action::Keys("formula", "+0")],
        checks: &[
            Check::Value("formula", "=SEQUENCE(5000,3)+0"),
            Check::Text("[data-onecalc=result-array] tbody tr:not([aria-hidden=true]) th", "2489"),
            Check::ScrollTop("result-scroll", 60_000),
        ],
    },
    Case {
        name: "array result: a wide result scrolls sideways with the row numbers fixed at the left",
        actions: &[Action::Formula("=SEQUENCE(4,60)"), Action::ScrollTo("result-scroll", 0, 500)],
        checks: &[
            Check::Fits,
            Check::Js("the row numbers stay at the left edge of the panel", "(function(){var p=document.querySelector('[data-onecalc=\"result-scroll\"]').getBoundingClientRect(),h=document.querySelector('[data-onecalc=\"result-array\"] tbody th').getBoundingClientRect();return Math.abs(h.left-p.left)<=2;})()"),
            Check::Js("the panel scrolled sideways, not the page", "document.querySelector('[data-onecalc=\"result-scroll\"]').scrollLeft>=400&&document.documentElement.scrollWidth<=innerWidth+1"),
        ],
    },
    Case {
        name: "array result: on a phone the result panel still scrolls inside the page",
        actions: &[Action::Window(390, 844), Action::ScrollTo("result-scroll", 0, 300)],
        checks: &[
            Check::Narrow(390),
            Check::Fits,
            Check::Js("the result panel fits the phone's width and scrolled by itself", "(function(){var p=document.querySelector('[data-onecalc=\"result-scroll\"]'),r=p.getBoundingClientRect();return r.right<=innerWidth+1&&p.scrollLeft>=250;})()"),
        ],
    },
    Case {
        name: "array result: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    Case {
        name: "pop-up: the completion list opens under the caret inside the formula field, not as a panel beside it",
        actions: &[Action::Formula("=1+1+1+1+1+SUM")],
        checks: &[
            Check::Text("completion-label", "SUM"),
            Check::Count("[data-onecalc=\"caret-overlay\"] [data-onecalc=\"completions\"]", 1),
            Check::Count("[data-onecalc=\"completions\"]", 1),
            Check::Js("the list is anchored at the caret: right of the field's left edge, within its bottom", "(function(){var p=document.querySelector('[data-onecalc=\"completions\"]').getBoundingClientRect(),e=document.querySelector('.oc-editor').getBoundingClientRect();return p.left>e.left+60&&p.top>=e.top+10&&p.top<=e.bottom+8;})()"),
        ],
    },
    Case {
        name: "pop-up: Down chooses the next proposal and Enter accepts it with its parenthesis",
        actions: &[Action::Formula("=SUM"), Action::Keys("formula", "\u{E015}"), Action::Keys("formula", "\u{E007}")],
        checks: &[
            Check::Absent("completions"),
            Check::Value("formula", "=SUMIF("),
        ],
    },
    Case {
        name: "pop-up: Tab accepts the first proposal and the signature help is a pop-up at the caret",
        actions: &[Action::Formula("=SUM"), Action::Keys("formula", TAB)],
        checks: &[
            Check::Value("formula", "=SUM("),
            Check::Count("[data-onecalc=\"caret-overlay\"] [data-onecalc=\"signature\"]", 1),
            Check::Text("active-parameter", "number1"),
            Check::Js("the signature sits just above the caret line of the field", "(function(){var s=document.querySelector('[data-onecalc=\"signature\"]').getBoundingClientRect(),e=document.querySelector('.oc-editor').getBoundingClientRect();return s.bottom<=e.top+40&&s.left>=e.left;})()"),
        ],
    },
    Case {
        name: "pop-up: Escape closes the list and the text stays",
        actions: &[Action::Formula("=SU"), Action::Keys("formula", ESCAPE)],
        checks: &[Check::Absent("completions"), Check::Value("formula", "=SU")],
    },
    Case {
        name: "pop-up: function help is closed until asked for",
        actions: &[Action::Formula("=ROUND(A1,")],
        checks: &[Check::Rendered("help-name", false), Check::Text("help-name", "ROUND")],
    },
    Case {
        name: "pop-up: F1 opens the function help panel",
        actions: &[Action::Keys("formula", F1)],
        checks: &[Check::Rendered("help-name", true), Check::Text("help-name", "ROUND")],
    },
    Case {
        name: "pop-up: on a phone the list fits the screen",
        actions: &[Action::Window(390, 844), Action::Formula("=SU")],
        checks: &[
            Check::Fits,
            Check::Js("the list is inside the phone's width", "(function(){var p=document.querySelector('[data-onecalc=\"completions\"]').getBoundingClientRect();return p.left>=0&&p.right<=innerWidth+1;})()"),
        ],
    },
    Case {
        name: "pop-up: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
    Case {
        name: "formatting: the Formatting panel draws the result, and the effective display says how",
        actions: &[
            Action::Formula("=1234.5"),
            Action::OpenDetails("formatting-panel"),
            Action::Into("style-fill", "#ffeb9c"),
            Action::Click("style-bold"),
            Action::Choose("style-align", "right"),
            Action::OpenDetails("effective-controls"),
        ],
        checks: &[
            Check::Value("style-align", "right"),
            Check::Style("result-value", "background-color:#ffeb9c;font-weight:700;text-align:right"),
            Check::Contains("effective-display", "Fill: #ffeb9c"),
            Check::Contains("effective-display", "Font: bold"),
            Check::Contains("effective-display", "Alignment: right"),
            Check::Contains("effective-display", "Number format: General"),
        ],
    },
    Case {
        name: "formatting: the number format's colour token is the result's text colour",
        actions: &[
            Action::Formula("=-5"),
            Action::OpenDetails("format-controls"),
            Action::Into("number-format", "0;[Red]-0"),
        ],
        checks: &[Check::Text("result-value", "-5"), Check::Attr("result-value", "data-color", "#ff0000")],
    },
    Case {
        name: "conditional formatting: a Cell value rule colours the result cell as Excel's preset does",
        actions: &[
            Action::Formula("=1234.5"),
            Action::Into("number-format", "General"),
            Action::OpenDetails("cf-panel"),
            Action::Choose("cf-new-kind", "cellIs"),
            Action::Click("cf-add"),
            Action::Into("cf-operand1", "1000"),
        ],
        checks: &[
            Check::Text("cf-count", "1 rule(s)"),
            Check::Attr("result-value", "data-fill", "#ffc7ce"),
            Check::Attr("result-value", "data-color", "#9c0006"),
            Check::Contains("effective-display", "1. Cell value > 1000: holds for 1 of 1 cell(s)"),
        ],
    },
    Case {
        name: "conditional formatting: a rule that does not hold leaves the result as it is",
        actions: &[Action::Into("cf-operand1", "5000")],
        checks: &[
            Check::Attr("result-value", "data-fill", ""),
            Check::Contains("effective-display", "1. Cell value > 5000: holds for 0 of 1 cell(s)"),
        ],
    },
    Case {
        name: "conditional formatting: over an array result the rule is tested at each cell",
        actions: &[Action::Formula("=SEQUENCE(3,2)"), Action::Into("cf-operand1", "3")],
        checks: &[
            Check::Array(&[&["1", "2"], &["3", "4"], &["5", "6"]]),
            Check::Js("exactly the cells 4, 5 and 6 are filled light red", "Array.from(document.querySelectorAll('[data-onecalc=\"result-array\"] td[data-fill=\"#ffc7ce\"]')).map(function(c){return c.textContent;}).join()==='4,5,6'"),
            Check::Contains("effective-display", "1. Cell value > 3: holds for 3 of 6 cell(s)"),
        ],
    },
    Case {
        name: "conditional formatting: a rule's number format shows the cell's text",
        actions: &[Action::Into("cf-operand1", "4"), Action::Into("cf-number-format", "0.0%")],
        checks: &[Check::Array(&[&["1", "2"], &["3", "4"], &["500.0%", "600.0%"]])],
    },
    Case {
        name: "conditional formatting: a formula rule reads the cell and its row, as Excel's formula rules do",
        actions: &[
            Action::Click("cf-clear"),
            Action::Choose("cf-new-kind", "expression"),
            Action::Click("cf-add"),
            Action::Into("cf-operand1", "MOD(ROW(),2)=0"),
        ],
        checks: &[Check::Js("the cells of row 2 only are filled", "Array.from(document.querySelectorAll('[data-onecalc=\"result-array\"] td[data-fill=\"#ffc7ce\"]')).map(function(c){return c.textContent;}).join()==='3,4'")],
    },
    Case {
        name: "conditional formatting: a colour scale colours the array from its lowest to its highest value",
        actions: &[Action::Click("cf-clear"), Action::Choose("cf-new-kind", "colorScale"), Action::Click("cf-add")],
        checks: &[Check::Js("the first cell is Excel's red, the last its green", "(function(){var c=document.querySelectorAll('[data-onecalc=\"result-array\"] td');return c[0].getAttribute('data-fill')==='#f8696b'&&c[5].getAttribute('data-fill')==='#63be7b';})()")],
    },
    Case {
        name: "conditional formatting: a data bar fills each cell by its share of the range",
        actions: &[Action::Click("cf-clear"), Action::Choose("cf-new-kind", "dataBar"), Action::Click("cf-add")],
        checks: &[Check::Js("the lowest cell has no bar and the highest a whole one", "(function(){var c=document.querySelectorAll('[data-onecalc=\"result-array\"] td');return c[0].getAttribute('data-bar')==='0'&&c[5].getAttribute('data-bar')==='100'&&c[5].style.backgroundImage.indexOf('linear-gradient')===0;})()")],
    },
    Case {
        name: "conditional formatting: an icon set puts an icon in each cell",
        actions: &[Action::Click("cf-clear"), Action::Choose("cf-new-kind", "iconSet"), Action::Click("cf-add")],
        checks: &[
            Check::Count("[data-onecalc=\"result-array\"] td[data-icon]", 6),
            Check::Js("the lowest cell has the first icon of the set and the highest the third", "(function(){var c=document.querySelectorAll('[data-onecalc=\"result-array\"] td');return c[0].getAttribute('data-icon')==='3Arrows:1'&&c[5].getAttribute('data-icon')==='3Arrows:3';})()"),
        ],
    },
    Case {
        name: "conditional formatting: a Dates rule reads the page's clock",
        actions: &[Action::Click("cf-clear"), Action::Formula("=TODAY()"), Action::Choose("cf-new-kind", "dates"), Action::Click("cf-add")],
        checks: &[Check::Attr("result-value", "data-fill", "#ffc7ce")],
    },
    Case {
        name: "conditional formatting: a rule missing its value is named and not applied",
        actions: &[Action::Click("cf-clear"), Action::Formula("=7"), Action::Choose("cf-new-kind", "cellIs"), Action::Click("cf-add")],
        checks: &[
            Check::Text("result-value", "7"),
            Check::TextContains("effective-display", "needs a value"),
            Check::Attr("result-value", "data-fill", ""),
        ],
    },
    Case {
        name: "conditional formatting: the rule list removes a rule",
        actions: &[Action::Click("cf-remove")],
        checks: &[Check::Count("[data-onecalc=\"cf-rule\"]", 0), Check::Text("cf-count", "0 rule(s)")],
    },
    Case {
        name: "conditional formatting: on a phone the panels fit the screen",
        actions: &[Action::Window(390, 844), Action::Choose("cf-new-kind", "cellIs"), Action::Click("cf-add")],
        checks: &[Check::Fits, Check::Reachable("cf-tools")],
    },
    Case {
        name: "conditional formatting: restore desktop after the phone case",
        actions: &[Action::Window(1280, 1000)],
        checks: &[Check::Fits],
    },
];

/// Groups whose name ends in `-browser` need the browser's own download (Save writes a file the
/// page offers); the desktop exe's Save is a native dialog, which needs a person, so the
/// WebView2 run leaves those groups out.
pub(crate) const ONECALC_GROUPS: &[CaseGroup] = &[
    CaseGroup {
        name: "formulas",
        start: 0,
        end: 28,
        core: 5,
        setup: &[],
    },
    CaseGroup {
        name: "vba",
        start: 28,
        end: 32,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "files",
        start: 32,
        end: 36,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "sessions-browser",
        start: 36,
        end: 51,
        core: 3,
        setup: &[Action::Formula("=Spin()")],
    },
    CaseGroup {
        name: "evaluation-context",
        start: 51,
        end: 52,
        core: 1,
        setup: &[],
    },
    CaseGroup {name:"friendly-functions-browser",start:52,end:55,core:3,setup:&[]},
    CaseGroup {name:"completion-repair",start:55,end:57,core:2,setup:&[]},
    CaseGroup {
        name: "formula-work",
        start: 57,
        end: 76,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "format-work",
        start: 76,
        end: 79,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "file-forms",
        start: 79,
        end: 81,
        core: 0,
        setup: &[Action::Grid("A1", "7")],
    },
    CaseGroup {
        name: "format-files-browser",
        start: 81,
        end: 83,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "addins",
        start: 83,
        end: 84,
        core: 0,
        setup: &[],
    },
    CaseGroup {
        name: "stack-margin",
        start: 84,
        end: 85,
        core: 1,
        setup: &[],
    },
    CaseGroup {name:"inputs-area",start:85,end:92,core:0,setup:&[]},
    CaseGroup {name:"array-scroll",start:92,end:98,core:0,setup:&[]},
    CaseGroup {name:"caret-popup",start:98,end:106,core:0,setup:&[]},
    CaseGroup {name:"formatting-panels",start:106,end:121,core:0,setup:&[]},
];
