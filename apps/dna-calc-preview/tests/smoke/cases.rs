//! Workbook preview cases driven through the real worker exchange.
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
        name: "Notebook first visit leaves Sales unchanged",
        actions: &[
            Action::WriteCast("Sales"),
            Action::UploadDownloadTo("Sales.xlsx", "open-file"),
            Action::Wait(Check::Text("[class~='dna-book-title']", "Sales.xlsx")),
            Action::Into("name-box", "Report!A12"),
            Action::Keys("name-box", "\u{e007}"),
            Action::Wait(Check::Js(
                "the Sales FILTER is selected",
                "__dnaProbe().formula.includes('Orders[Status]')",
            )),
            Action::Click("[data-command='view.notebook']"),
        ],
        checks: &[
            Check::Text(
                "[data-command='notebook.link']",
                "Start with selected cells",
            ),
            Check::Text("mode", "Command"),
            Check::Js(
                "first visit leaves both revisions unchanged",
                "__dnaProbe().stamp.model === 0 && __dnaProbe().stamp.view === 0",
            ),
        ],
    },
    Case {
        name: "Notebook links the full Sales FILTER and windows its output",
        actions: &[Action::Click("[data-command='notebook.link']")],
        checks: &[
            Check::Text(
                "[data-notebook-formula]",
                "=FILTER(Orders,Orders[Status]=\"Pending\")",
            ),
            Check::Attr("[data-notebook-rows]", "data-notebook-rows", "4147"),
            Check::Js(
                "the first window renders only 20 result rows",
                "document.querySelectorAll('[data-notebook-rows] tbody tr').length === 20",
            ),
            Check::Js(
                "linking changes only the layout revision",
                "__dnaProbe().stamp.model === 0 && __dnaProbe().stamp.view > 0",
            ),
        ],
    },
    Case {
        name: "Notebook refreshes when a Sales source row changes",
        actions: &[
            Action::Click("[data-command='view.classic']"),
            Action::Into("name-box", "Orders!D10483"),
            Action::Keys("name-box", "\u{e007}"),
            Action::Wait(Check::Value("name-box", "D10483")),
            Action::Into("formula-bar", "Paid"),
            Action::Keys("formula-bar", "\u{e007}"),
            Action::Wait(Check::Js(
                "the source edit committed",
                "__dnaProbe().stamp.model > 0 && __dnaProbe().seam === 'idle'",
            )),
            Action::Click("[data-command='view.notebook']"),
        ],
        checks: &[Check::Attr(
            "[data-notebook-rows]",
            "data-notebook-rows",
            "4146",
        )],
    },
    Case {
        name: "Notebook Escape holds an edit and Ctrl+Enter runs it",
        actions: &[
            Action::Keys("notebook", "\u{e007}"),
            Action::Into(
                "[data-dna-editor] textarea",
                "=FILTER(Orders,Orders[Status]=\"Paid\")",
            ),
            Action::Keys("[data-dna-editor] textarea", "\u{e00c}"),
            Action::Wait(Check::Js(
                "the draft is held",
                "document.querySelector('.dna-notebook-draft')?.textContent.includes('Edited · not run')",
            )),
            Action::Wait(Check::Attr(
                "[data-notebook-rows]",
                "data-notebook-rows",
                "4146",
            )),
            Action::Keys("notebook", "\u{e007}"),
            Action::Keys("[data-dna-editor] textarea", "\u{e009}\u{e007}\u{e000}"),
        ],
        checks: &[
            Check::Text("mode", "Command"),
            Check::Attr("[data-notebook-rows]", "data-notebook-rows", "20854"),
        ],
    },
];

const CLASSIC_END: usize = 4;
pub(crate) const PREVIEW_GROUPS: &[CaseGroup] = &[
    CaseGroup {
        name: "classic",
        start: 0,
        end: CLASSIC_END,
        core: CLASSIC_END,
        setup: &[],
    },
    CaseGroup {
        name: "notebook",
        start: CLASSIC_END,
        end: PREVIEW_CASES.len(),
        core: PREVIEW_CASES.len() - CLASSIC_END,
        setup: &[],
    },
];
