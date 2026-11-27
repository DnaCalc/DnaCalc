//! Classic preview cases driven through the real worker exchange.
use super::*;

pub(crate) const PREVIEW_CASES: &[Case] = &[
    Case {
        name: "Classic draws the workbook and follows a click",
        actions: &[
            Action::Wait(Check::Attr("[data-cell=A1]", "aria-selected", "true")),
            Action::Click("[data-cell=B2]"),
        ],
        checks: &[
            Check::Attr("[data-cell=B2]", "aria-selected", "true"),
            Check::Value("name-box", "B2"),
        ],
    },
    Case {
        name: "Classic previews and commits a formula through the Bridge",
        actions: &[
            Action::Into("formula-bar", "=21*2"),
            Action::Keys("formula-bar", "\u{e007}"),
        ],
        checks: &[
            Check::Text("[data-cell=B2]", "42"),
            Check::Attr("[data-cell=B2]", "data-provenance", "calculated"),
            Check::Js(
                "the host advanced the model",
                "window.__dnaProbe().stamp.model > 0",
            ),
        ],
    },
    Case {
        name: "Classic undo restores the same cell through workbook history",
        actions: &[Action::Click("[data-command=\"edit.undo\"]")],
        checks: &[
            Check::Text("[data-cell=B2]", ""),
            Check::Attr("[data-cell=B2]", "data-provenance", "empty"),
        ],
    },
    Case {
        name: "Classic opens a local workbook through the same host",
        actions: &[
            Action::WritePlainSession("Classic preview.xlsx", "A1+A2"),
            Action::UploadDownloadTo("Classic preview.xlsx", "open-file"),
        ],
        checks: &[
            Check::Text("[data-cell=A1]", "3"),
            Check::Text("[data-cell=A2]", "4"),
        ],
    },
    Case {
        name: "S06 Sales traces all Orders rows in the dock and enlarges in place",
        actions: &[
            Action::WriteCast("Sales"),
            Action::UploadDownloadTo("Sales.xlsx", "open-file"),
            Action::Wait(Check::Js(
                "Sales opened with its Report worksheet",
                "Array.from(document.querySelectorAll('.dna-book-sheets button')).some(button => button.textContent === 'Report')",
            )),
            Action::Into("name-box", "Report!B5"),
            Action::Keys("name-box", "\u{e007}"),
            Action::Wait(Check::Value(
                "formula-bar",
                r#"=SUMIFS(Orders[Amount],Orders[Region],"West")"#,
            )),
            Action::Click("[data-command='calc.calculate']"),
            Action::Click("[data-command='inspector.dock']"),
            Action::Click("[data-command='inspector.precedents']"),
            Action::Click("[data-command='inspector.trace']"),
            Action::Wait(Check::TextContains(
                "[data-inspector='trace-result']",
                "128450",
            )),
            Action::Click("[data-command='inspector.enlarge']"),
        ],
        checks: &[
            Check::Js(
                "enlarging the dock keeps the same cell on the Inspector stage",
                "window.__dnaProbe().view === 'inspector' && window.__dnaProbe().active.slice(2).join(',') === '5,2'",
            ),
            Check::Text(
                "[data-inspector='trace-coverage']",
                "25000 inspected · 126 match · 0 undetermined",
            ),
            Check::TextContains("[data-inspector='trace-result']", "128450"),
            Check::Absent("[data-inspector-dock]"),
        ],
    },
    Case {
        name: "S07 Sales previews one write, applies, and shows the Pending order",
        actions: &[
            Action::OpenDetails("[data-inspector-tab='Precedents'] details:has(#inspector-repair)"),
            Action::Into(
                "#inspector-repair",
                r#"=SUMIFS(Orders[Amount],Orders[Region],"West",Orders[Status],"Paid")"#,
            ),
            Action::Click("[data-command='inspector.repair']"),
            Action::Wait(Check::TextContains("seam-review", "1 cell entry changed")),
            Action::Wait(Check::TextContains(
                "[data-inspector='trace-result']",
                "128450",
            )),
            Action::Click("[data-command='edit.apply-review']"),
            Action::Wait(Check::TextContains(
                "[data-inspector='trace-result']",
                "124250",
            )),
            Action::Click("[data-criterion='Status'] [data-command='inspector.filter']"),
        ],
        checks: &[
            Check::Text(
                "[data-inspector='trace-coverage']",
                "25000 inspected · 125 match · 0 undetermined",
            ),
            Check::TextContains(
                "[data-inspector='trace-filter']",
                "View filter from Trace · Orders",
            ),
            Check::TextContains("[data-trace-row='10483']", "SO10482"),
            Check::TextContains("[data-trace-row='10483']", "Pending"),
            Check::TextContains("[data-trace-row='10483']", "4200"),
        ],
    },
    Case {
        name: "S07 undo restores the original Sales formula while Inspector stays open",
        actions: &[
            Action::Click("[data-command='inspector.clear-filter']"),
            Action::Click("[data-command='edit.undo']"),
        ],
        checks: &[
            Check::Js(
                "undo leaves Inspector on stage",
                "window.__dnaProbe().view === 'inspector'",
            ),
            Check::Absent("[data-inspector='trace-filter']"),
            Check::TextContains("[data-inspector='trace-result']", "128450"),
            Check::Value(
                "formula-bar",
                r#"=SUMIFS(Orders[Amount],Orders[Region],"West")"#,
            ),
        ],
    },
    Case {
        name: "S02 Inherited traces Summary F18 to the broken name",
        actions: &[
            Action::WriteCast("Inherited"),
            Action::UploadDownloadTo("Inherited.xlsx", "open-file"),
            Action::Wait(Check::Js(
                "Inherited opened with its Summary worksheet",
                "Array.from(document.querySelectorAll('.dna-book-sheets button')).some(button => button.textContent === 'Summary')",
            )),
            Action::Into("name-box", "Summary!F18"),
            Action::Keys("name-box", "\u{e007}"),
            Action::Wait(Check::Value("formula-bar", "=SUM(Revenue.Window)")),
            Action::Click("[data-command='view.inspector']"),
            Action::Click("[data-command='inspector.details']"),
            Action::Wait(Check::TextContains(
                "[data-inspector='saved-formula']",
                "2401300",
            )),
            Action::Click("[data-command='calc.calculate']"),
            Action::WaitWorkbook(Check::TextContains(
                "[data-inspector='issues-count']",
                "1 error",
            )),
            Action::Click("[data-inspector-tab='Overview'] [data-command='inspector.reveal']"),
            Action::Click("[data-command='inspector.precedents']"),
        ],
        checks: &[
            Check::Value("name-box", "F18"),
            Check::TextContains(
                "[data-inspector='name-evidence']",
                "OFFSET(Revenue!#REF!,0,0,12,1)",
            ),
            Check::TextContains("[data-inspector='name-evidence']", "Revenue.Window"),
            Check::TextContains("[data-inspector='cell-details']", "#REF!"),
            Check::TextContains("[data-inspector='saved-formula']", "2401300"),
            Check::TextContains(
                "[data-inspector-tab='Precedents']",
                "Dependency explanation; execution not recorded",
            ),
        ],
    },
    Case {
        name: "S02 records and undoes an anchored audit finding",
        actions: &[
            Action::Click("[data-command='inspector.notes']"),
            Action::Into(
                "[aria-label='New finding']",
                "Revenue.Window contains a broken reference.",
            ),
            Action::Click("[data-command='inspector.note-put']"),
            Action::Wait(Check::Value(
                "[aria-label='Edit finding']",
                "Revenue.Window contains a broken reference.",
            )),
            Action::Click("[data-command='edit.undo']"),
        ],
        checks: &[
            Check::Absent("[aria-label='Edit finding']"),
            Check::Value("formula-bar", "=SUM(Revenue.Window)"),
        ],
    },
];

pub(crate) const PREVIEW_GROUPS: &[CaseGroup] = &[
    CaseGroup {
        name: "classic",
        start: 0,
        end: 4,
        core: 4,
        setup: &[],
    },
    CaseGroup {
        name: "sales",
        start: 4,
        end: 7,
        core: 3,
        setup: &[],
    },
    CaseGroup {
        name: "inherited",
        start: 7,
        end: 9,
        core: 2,
        setup: &[],
    },
];
