//! The application's clipboard for macros (dc-oxh8w.15): `Range.Copy` and `Range.Cut` without a
//! Destination capture the source as the host's immutable copy snapshot, and `Range.PasteSpecial` and
//! `Worksheet.Paste` write it through the host's one paste planner (`prepare_paste_oriented`, the route the
//! applications' paste takes). A Copy stays on the clipboard for further pastes; a pasted Cut is spent.
use super::*;
use crate::{CopySnapshot, PasteMode, PasteSource, SkinIntent, WorkbookCommand, prepare_paste_oriented};

/// The open bead that owns the clipboard remainders this file refuses.
const OWNER: &str = "dc-oxh8w.33";

const XL_PASTE_ALL: i64 = -4104;
const XL_PASTE_VALUES: i64 = -4163;
const XL_PASTE_FORMULAS: i64 = -4123;
const XL_PASTE_FORMATS: i64 = -4122;
const XL_PASTE_OPERATION_NONE: i64 = -4142;
const XL_PASTE_OPERATION_ADD: i64 = 2;
const XL_PASTE_OPERATION_SUBTRACT: i64 = 3;
const XL_PASTE_OPERATION_MULTIPLY: i64 = 4;
const XL_PASTE_OPERATION_DIVIDE: i64 = 5;
const XL_PASTE_OPERATIONS: [i64; 5] = [
    XL_PASTE_OPERATION_NONE,
    XL_PASTE_OPERATION_ADD,
    XL_PASTE_OPERATION_SUBTRACT,
    XL_PASTE_OPERATION_MULTIPLY,
    XL_PASTE_OPERATION_DIVIDE,
];

const NO_COMBINE: Combine = Combine { operation: None, skip_blanks: false };

/// What Paste Special does on top of the paste plan.
#[derive(Clone, Copy)]
struct Combine {
    /// One of the xlPasteSpecialOperation constants other than xlNone, when given.
    operation: Option<i64>,
    skip_blanks: bool,
}

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
        let operation = if given(1) { geometry::integer(call.arg(1))? } else { XL_PASTE_OPERATION_NONE };
        if !XL_PASTE_OPERATIONS.contains(&operation) {
            return Err(AutoError::excel(1004, "PasteSpecial method of Range class failed"));
        }
        let flag = |index: usize| match call.arg(index) {
            AutoValue::Missing | AutoValue::Empty => Ok(false),
            AutoValue::Bool(value) => Ok(*value),
            _ => Err(geometry::type_mismatch()),
        };
        let combine = Combine {
            operation: (operation != XL_PASTE_OPERATION_NONE).then_some(operation),
            skip_blanks: flag(2)?,
        };
        let transpose = flag(3)?;
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
            cut = clip.cut;
            let mut plan = prepare_paste_oriented(
                session.host(),
                PasteSource::Snapshot { snapshot: &clip.snapshot, cut: clip.cut },
                to,
                mode,
                transpose,
            )
            .map_err(|why| AutoError::excel(1004, format!("{member} failed: {why}")))?;
            if combine.skip_blanks || combine.operation.is_some() {
                if clip.cut {
                    return Err(failed("PasteSpecial"));
                }
                Self::combine_plan(session.host(), &mut plan, combine)?;
            }
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

impl WorkbookAutomation {
    /// SkipBlanks leaves the destination cell where the source is blank; an Operation combines the
    /// destination with the source number (an empty destination counts as zero). Sources or destinations
    /// that are not literal numbers are refused: the formula-wrapping and text results are unmeasured (dc-oxh8w.33).
    fn combine_plan(host: &crate::HostState, plan: &mut crate::PastePlan, combine: Combine) -> Result<(), AutoError> {
        use crate::{CellEdit, CellInput, ProjectedValue, ProjectionRequest};
        if combine.skip_blanks {
            plan.edits_mut().retain(|edit| !matches!(edit, CellEdit::Clear { .. }));
        }
        let Some(operation) = combine.operation else {
            return Ok(());
        };
        let refuse = |what: &str| AutoError::not_modelled(format!("Range.PasteSpecial Operation {what}"), OWNER);
        let cells: Vec<CellKey> = plan
            .edits_mut()
            .iter()
            .filter_map(|edit| match edit {
                CellEdit::Set { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        let page = crate::project(host, &ProjectionRequest { cells }).map_err(|_| failed("PasteSpecial"))?;
        let mut current = HashMap::new();
        for cell in page.cells() {
            let number = match cell.value() {
                ProjectedValue::Empty => 0.0,
                ProjectedValue::Literal(v) => match v.core_ref() {
                    calc_value::CoreRef::Number(n) => n,
                    _ => return Err(refuse("onto a destination cell that is not a number")),
                },
                _ => return Err(refuse("onto a formula destination")),
            };
            current.insert(cell.at(), number);
        }
        for edit in plan.edits_mut().iter_mut() {
            match edit {
                CellEdit::Set { at, input } => {
                    let CellInput::Number(source) = input else {
                        return Err(refuse("with a source that is not a number"));
                    };
                    let (dest, source) = (current[at], source.get());
                    let result = match operation {
                        XL_PASTE_OPERATION_ADD => dest + source,
                        XL_PASTE_OPERATION_SUBTRACT => dest - source,
                        XL_PASTE_OPERATION_MULTIPLY => dest * source,
                        _ if source == 0.0 => {
                            *input = CellInput::Error(WorksheetErrorCode::Div0);
                            continue;
                        }
                        _ => dest / source,
                    };
                    *input = crate::FiniteNumber::new(result)
                        .map(CellInput::Number)
                        .unwrap_or(CellInput::Error(WorksheetErrorCode::Num));
                }
                CellEdit::Clear { .. } => return Err(refuse("with a blank source cell (use SkipBlanks)")),
                _ => {}
            }
        }
        Ok(())
    }
}
