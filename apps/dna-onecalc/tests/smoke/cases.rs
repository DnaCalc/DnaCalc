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
        core: 2,
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
];
