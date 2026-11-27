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
];
pub(crate) const PREVIEW_GROUPS: &[CaseGroup] = &[CaseGroup {
    name: "classic",
    start: 0,
    end: PREVIEW_CASES.len(),
    core: PREVIEW_CASES.len(),
    setup: &[],
}];
