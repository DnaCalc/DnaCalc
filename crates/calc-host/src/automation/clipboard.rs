//! The application's clipboard for macros (dc-oxh8w.15): `Range.Copy` and `Range.Cut` without a
//! Destination capture the source as the host's immutable copy snapshot, and `Range.PasteSpecial` and
//! `Worksheet.Paste` write it through the host's one paste planner (`prepare_paste_oriented`, the route the
//! applications' paste takes). A Copy stays on the clipboard for further pastes; a pasted Cut is spent.
use super::*;
use crate::{
    CopySnapshot, PasteArithmetic, PasteCellOptions, PasteMode, PasteOptionsRefusal, PasteSource,
    SkinIntent, WorkbookCommand, prepare_paste_column_widths, prepare_paste_oriented,
};

/// The open bead that owns the clipboard remainders this file refuses.
const OWNER: &str = "dc-oxh8w.43";

const XL_PASTE_ALL: i64 = -4104;
const XL_PASTE_VALUES: i64 = -4163;
const XL_PASTE_FORMULAS: i64 = -4123;
const XL_PASTE_FORMATS: i64 = -4122;
const XL_PASTE_COMMENTS: i64 = -4144;
const XL_PASTE_VALIDATION: i64 = 6;
const XL_PASTE_ALL_EXCEPT_BORDERS: i64 = 7;
const XL_PASTE_FORMULAS_AND_NUMBER_FORMATS: i64 = 11;
const XL_PASTE_VALUES_AND_NUMBER_FORMATS: i64 = 12;
const XL_PASTE_COLUMN_WIDTHS: i64 = 8;
const XL_PASTE_COMMENTS_AND_NOTES: i64 = 21;
const XL_PASTE_ALL_USING_SOURCE_THEME: i64 = 13;
const XL_PASTE_ALL_MERGING_CONDITIONAL_FORMATS: i64 = 14;
const XL_PASTE_OPERATION_NONE: i64 = -4142;
const XL_PASTE_OPERATION_ADD: i64 = 2;
const XL_PASTE_OPERATION_SUBTRACT: i64 = 3;
const XL_PASTE_OPERATION_MULTIPLY: i64 = 4;
const XL_PASTE_OPERATION_DIVIDE: i64 = 5;
const NO_COMBINE: Combine = Combine {
    options: PasteCellOptions { arithmetic: None, skip_blanks: false },
    column_widths: false,
};

/// What Paste Special does on top of the paste plan.
#[derive(Clone, Copy)]
struct Combine {
    options: PasteCellOptions,
    /// xlPasteColumnWidths: only the source columns' widths are pasted.
    column_widths: bool,
}

/// What the last Copy or Cut left.
pub(super) struct Clip {
    snapshot: CopySnapshot,
    cut: bool,
}

impl Clip {
    pub(super) fn is_cut(&self) -> bool {
        self.cut
    }
}

fn failed(member: &str) -> AutoError {
    AutoError::excel(1004, format!("{member} method of Range class failed"))
}

impl WorkbookAutomation {
    pub(super) fn register_clipboard(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "PasteSpecial", InvokeKind::Method, Self::range_paste_special);
        handlers.on("Worksheet", "Paste", InvokeKind::Method, Self::worksheet_paste);
        handlers.on("Worksheet", "PasteSpecial", InvokeKind::Method, Self::worksheet_paste_special);
    }

    /// Copy or Cut without a Destination: the source goes to the clipboard.
    pub(super) fn copy_to_clipboard(&self, target: &ObjectRef, cut: bool) -> Reply {
        self.with_range(target, |access, _, range, _| {
            if range.area_count() != 1 {
                return Err(AutoError::not_modelled("Range.Copy of several areas to the clipboard", OWNER));
            }
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Range.Copy inside a worksheet invocation", OWNER));
            };
            let snapshot = CopySnapshot::capture(session.host(), range.areas()[0]).map_err(|e| {
                AutoError::excel(1004, format!("{} method of Range class failed: {e}", if cut { "Cut" } else { "Copy" }))
            })?;
            *self.clipboard.borrow_mut() = Some(Clip { snapshot, cut });
            Ok(AutoValue::Bool(true))
        })
    }

    /// Range.PasteSpecial(Paste, Operation, SkipBlanks, Transpose) onto the range.
    fn range_paste_special(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 4 {
            return Err(geometry::bad_count());
        }
        let given = |index: usize| !matches!(call.arg(index), AutoValue::Missing | AutoValue::Empty);
        let paste = if given(0) { geometry::integer(call.arg(0))? } else { XL_PASTE_ALL };
        let mode = match paste {
            // Within one workbook the source theme is the destination's.
            XL_PASTE_ALL | XL_PASTE_ALL_USING_SOURCE_THEME => PasteMode::All,
            XL_PASTE_ALL_MERGING_CONDITIONAL_FORMATS => PasteMode::AllMergingConditionalFormats,
            // Excel ww22: xlPasteCommentsAndNotes is a 1004 and leaves the destination's note.
            XL_PASTE_COMMENTS_AND_NOTES => return Err(failed("PasteSpecial")),
            XL_PASTE_COLUMN_WIDTHS => PasteMode::Formats,
            XL_PASTE_VALIDATION => PasteMode::Validation,
            XL_PASTE_COMMENTS => PasteMode::Comments,
            XL_PASTE_VALUES => PasteMode::Values,
            XL_PASTE_FORMULAS => PasteMode::Formulas,
            XL_PASTE_FORMATS => PasteMode::Formats,
            XL_PASTE_ALL_EXCEPT_BORDERS => PasteMode::AllExceptBorders,
            XL_PASTE_FORMULAS_AND_NUMBER_FORMATS => PasteMode::FormulasAndNumberFormats,
            XL_PASTE_VALUES_AND_NUMBER_FORMATS => PasteMode::ValuesAndNumberFormats,
            _ => {
                return Err(AutoError::not_modelled(
                    format!("Range.PasteSpecial Paste:={paste} "),
                    "dc-2rjye",
                ));
            }
        };
        let operation = if given(1) { geometry::integer(call.arg(1))? } else { XL_PASTE_OPERATION_NONE };
        let arithmetic = match operation {
            XL_PASTE_OPERATION_NONE => None,
            XL_PASTE_OPERATION_ADD => Some(PasteArithmetic::Add),
            XL_PASTE_OPERATION_SUBTRACT => Some(PasteArithmetic::Subtract),
            XL_PASTE_OPERATION_MULTIPLY => Some(PasteArithmetic::Multiply),
            XL_PASTE_OPERATION_DIVIDE => Some(PasteArithmetic::Divide),
            _ => return Err(failed("PasteSpecial")),
        };
        let flag = |index: usize| match call.arg(index) {
            AutoValue::Missing | AutoValue::Empty => Ok(false),
            AutoValue::Bool(value) => Ok(*value),
            _ => Err(geometry::type_mismatch()),
        };
        let combine = Combine {
            options: PasteCellOptions { arithmetic, skip_blanks: flag(2)? },
            column_widths: false,
        };
        let transpose = flag(3)?;
        if paste == XL_PASTE_COLUMN_WIDTHS {
            // Excel ww21/ww24: Transpose and every Operation are ignored when pasting column widths.
            return self.paste_clip(target, call, mode, false, false, Combine { column_widths: true, ..combine });
        }
        self.paste_clip(target, call, mode, transpose, false, combine)
    }

    /// Worksheet.PasteSpecial(Format, Link, DisplayAsIcon, IconFileName, IconIndex, IconLabel, NoHTMLFormatting):
    /// the clipboard's cells pasted at the selection; the other formats and Link are not modelled.
    fn worksheet_paste_special(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 7 {
            return Err(geometry::bad_count());
        }
        let given = |index: usize| !matches!(call.arg(index), AutoValue::Missing | AutoValue::Empty);
        if given(0) {
            call.arg(0).as_text().ok_or_else(geometry::type_mismatch)?;
            return Err(AutoError::not_modelled("Worksheet.PasteSpecial Format (clipboard formats other than cells)", OWNER));
        }
        for (index, what) in [(1, "Link"), (2, "DisplayAsIcon")] {
            match call.arg(index) {
                AutoValue::Missing | AutoValue::Empty | AutoValue::Bool(false) => {}
                AutoValue::Bool(true) => {
                    return Err(AutoError::not_modelled(format!("Worksheet.PasteSpecial {what}"), OWNER));
                }
                _ => return Err(geometry::type_mismatch()),
            }
        }
        if (3..7).any(given) {
            return Err(AutoError::not_modelled("Worksheet.PasteSpecial icon and HTML options", OWNER));
        }
        self.paste_clip(target, call, PasteMode::All, false, true, NO_COMBINE)
    }

    /// Worksheet.Paste(Destination, Link): everything, at Destination or at the selection.
    fn worksheet_paste(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        if matches!(call.arg(1), AutoValue::Bool(true)) {
            return Err(AutoError::not_modelled("Worksheet.Paste Link", OWNER));
        }
        match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => self.paste_clip(target, call, PasteMode::All, false, true, NO_COMBINE),
            AutoValue::Object(destination) => {
                let destination = destination.clone();
                self.paste_clip(&destination, call, PasteMode::All, false, false, NO_COMBINE)
            }
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn paste_clip(&self, target: &ObjectRef, call: &Call<'_>, mode: PasteMode, transpose: bool, at_selection: bool, combine: Combine) -> Reply {
        let member = call.label().to_string();
        let mut cut = false;
        let intent = |session: &HostSession, range: Option<ReferenceLike>| {
            let clipboard = self.clipboard.borrow();
            let Some(clip) = clipboard.as_ref() else {
                return Err(failed("PasteSpecial"));
            };
            let to = if at_selection {
                *session.selection().areas.first().ok_or_else(|| failed("Paste"))?
            } else {
                let range = range.ok_or_else(geometry::type_mismatch)?;
                if range.area_count() != 1 {
                    return Err(AutoError::not_modelled(format!("{member} onto several areas"), OWNER));
                }
                range.areas()[0]
            };
            // The paste writes its destination, and a Cut also clears its source.
            self.guard_area(session, Self::written_area(clip.snapshot.area(), to, transpose))?;
            if clip.cut {
                self.guard_area(session, clip.snapshot.area())?;
            }
            cut = clip.cut;
            if combine.column_widths {
                if clip.cut {
                    return Err(failed("PasteSpecial"));
                }
                let (target, edits) = prepare_paste_column_widths(session.host(), &clip.snapshot, to)
                    .map_err(|_| failed("PasteSpecial"))?;
                self.guard_area(session, target)?;
                return Ok(SkinIntent::Edit { edits });
            }
            let plan = prepare_paste_oriented(
                session.host(),
                PasteSource::Snapshot { snapshot: &clip.snapshot, cut: clip.cut },
                to,
                mode,
                transpose,
            )
            .map_err(|why| AutoError::excel(1004, format!("{member} failed: {why}")))?;
            let plan = if combine.options != PasteCellOptions::default() {
                if clip.cut {
                    return Err(failed("PasteSpecial"));
                }
                plan.with_cell_options(session.host(), combine.options).map_err(|reason| match reason {
                    PasteOptionsRefusal::NotModelled(what) => {
                        AutoError::not_modelled(format!("Range.PasteSpecial Operation {what}"), OWNER)
                    }
                    _ => failed("PasteSpecial"),
                })?
            } else {
                plan
            };
            Ok(SkinIntent::Command(WorkbookCommand::Paste(plan)))
        };
        self.mutate(target, call, intent)?;
        if cut {
            // A pasted Cut is spent; a Copy stays for further pastes.
            *self.clipboard.borrow_mut() = None;
        }
        Ok(AutoValue::Bool(true))
    }
}
