use calc_skin::grid::Area;
use calc_skin::{
    Batch, Change, ModelChange, Operation, PasteContents, SourceRef, ViewId, WorkbookId,
};
use dna_kit::{Session, ViewCommand};
use leptos::prelude::*;

fn single_range(session: Session) -> Result<Area, String> {
    if let Some(selection) = session.selection()
        && let SourceRef::Cells { areas, .. } = selection.target
        && let [area] = areas.as_slice()
    {
        return Ok(*area);
    }
    Err("Select one rectangular range of worksheet cells.".into())
}

pub(crate) fn commands(view: &ViewId, session: Session) -> [ViewCommand; 4] {
    let copied = StoredValue::new(None::<(WorkbookId, u64)>);
    let paste = |id, label, chords: &[&'static str], contents| {
        ViewCommand::new(view, id, label, chords, move |_| {
            let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
            let (_, copy) = copied
                .get_value()
                .filter(|(workbook, _)| *workbook == stamp.workbook)
                .ok_or("Copy worksheet cells before pasting.")?;
            let to = single_range(session)?;
            session.submit(Batch {
                id: String::new(),
                workbook: Some(stamp.workbook),
                base: Some(stamp.model),
                expects: Vec::new(),
                selection_seq: None,
                label: Some(label.into()),
                changes: vec![Change::Model(ModelChange::PasteCopied {
                    copy,
                    to,
                    contents,
                })],
            });
            Ok(())
        })
        .when("mode:ready && writable")
    };
    [
        ViewCommand::new(
            view,
            "classic.copy-cells",
            "Copy cells",
            &["Ctrl+C"],
            move |_| {
                let workbook = session
                    .stamp()
                    .ok_or("The workbook is unavailable.")?
                    .workbook;
                let from = single_range(session)?;
                let copy = copied.get_value().map_or(1, |(_, previous)| previous + 1);
                copied.set_value(Some((workbook, copy)));
                session.operate(
                    Operation::CopyCells {
                        workbook,
                        copy,
                        from,
                    },
                    Some("Copied worksheet cells.".into()),
                );
                Ok(())
            },
        )
        .when("mode:ready"),
        paste(
            "classic.paste-values",
            "Paste Values",
            &["Ctrl+Shift+V"],
            PasteContents::Values,
        ),
        paste(
            "classic.paste-formulas",
            "Paste Formulas",
            &[],
            PasteContents::Formulas,
        ),
        paste(
            "classic.paste-transpose",
            "Paste Transpose",
            &[],
            PasteContents::AllTransposed,
        ),
    ]
}
