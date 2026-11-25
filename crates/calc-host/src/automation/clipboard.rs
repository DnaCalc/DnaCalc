//! The application's clipboard for macros (dc-oxh8w.15): `Range.Copy` and `Range.Cut` without a
//! Destination capture the source as the host's immutable copy snapshot, and `Range.PasteSpecial` and
//! `Worksheet.Paste` write it through the host's one paste planner (`prepare_paste_oriented`, the route the
//! applications' paste takes). A Copy stays on the clipboard for further pastes; a pasted Cut is spent.
use super::*;
use crate::{CopySnapshot, PasteMode, PasteSource, SkinIntent, WorkbookCommand, prepare_paste_oriented};

const XL_PASTE_ALL: i64 = -4104;
const XL_PASTE_VALUES: i64 = -4163;
const XL_PASTE_FORMULAS: i64 = -4123;
const XL_PASTE_FORMATS: i64 = -4122;
const XL_PASTE_OPERATION_NONE: i64 = -4142;

/// What the last Copy or Cut left.
pub(super) struct Clip {
    snapshot: CopySnapshot,
    cut: bool,
}

fn failed(member: &str) -> AutoError {
    AutoError::excel(1004, format!("{member} method of Range class failed"))
}

impl WorkbookAutomation {
    pub(super) fn register_clipboard(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "PasteSpecial", InvokeKind::Method, Self::range_paste_special);
        handlers.on("Worksheet", "Paste", InvokeKind::Method, Self::worksheet_paste);
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
            XL_PASTE_ALL => PasteMode::All,
            XL_PASTE_VALUES => PasteMode::Values,
            XL_PASTE_FORMULAS => PasteMode::Formulas,
            XL_PASTE_FORMATS => PasteMode::Formats,
            _ => {
                return Err(AutoError::not_modelled(
                    format!("Range.PasteSpecial Paste:={paste} (comments, validation, widths, number-format mixes, themes)"),
                    OWNER,
                ));
            }
        };
        if given(1) && geometry::integer(call.arg(1))? != XL_PASTE_OPERATION_NONE {
            return Err(AutoError::not_modelled("Range.PasteSpecial Operation (add, subtract, multiply, divide)", OWNER));
        }
        let flag = |index: usize| match call.arg(index) {
            AutoValue::Missing | AutoValue::Empty => Ok(false),
            AutoValue::Bool(value) => Ok(*value),
            _ => Err(geometry::type_mismatch()),
        };
        if flag(2)? {
            return Err(AutoError::not_modelled("Range.PasteSpecial SkipBlanks", OWNER));
        }
        let transpose = flag(3)?;
        self.paste_clip(target, call, mode, transpose, false)
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
            AutoValue::Missing | AutoValue::Empty => self.paste_clip(target, call, PasteMode::All, false, true),
            AutoValue::Object(destination) => {
                let destination = destination.clone();
                self.paste_clip(&destination, call, PasteMode::All, false, false)
            }
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn paste_clip(&self, target: &ObjectRef, call: &Call<'_>, mode: PasteMode, transpose: bool, at_selection: bool) -> Reply {
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
            cut = clip.cut;
            let plan = prepare_paste_oriented(
                session.host(),
                PasteSource::Snapshot { snapshot: &clip.snapshot, cut: clip.cut },
                to,
                mode,
                transpose,
            )
            .map_err(|why| AutoError::excel(1004, format!("{member} failed: {why}")))?;
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
