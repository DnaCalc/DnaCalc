//! The application's clipboard for macros (dc-oxh8w.15): `Range.Copy` and `Range.Cut` without a
//! Destination capture the source as the host's immutable copy snapshot, and `Range.PasteSpecial` and
//! `Worksheet.Paste` write it through the host's one paste planner (`prepare_paste_oriented`, the route the
//! applications' paste takes). A Copy stays on the clipboard for further pastes; a pasted Cut is spent.
use super::*;
use crate::{CopySnapshot, PasteMode, PasteSource, SkinIntent, WorkbookCommand, prepare_paste_oriented};

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
const XL_PASTE_OPERATIONS: [i64; 5] = [
    XL_PASTE_OPERATION_NONE,
    XL_PASTE_OPERATION_ADD,
    XL_PASTE_OPERATION_SUBTRACT,
    XL_PASTE_OPERATION_MULTIPLY,
    XL_PASTE_OPERATION_DIVIDE,
];

const NO_COMBINE: Combine = Combine { operation: None, skip_blanks: false, column_widths: false };

/// What Paste Special does on top of the paste plan.
#[derive(Clone, Copy)]
struct Combine {
    /// One of the xlPasteSpecialOperation constants other than xlNone, when given.
    operation: Option<i64>,
    skip_blanks: bool,
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
            column_widths: false,
        };
        let transpose = flag(3)?;
        if paste == XL_PASTE_COLUMN_WIDTHS {
            // Excel ww21 ignores Transpose and Add when pasting column widths.
            if !matches!(combine.operation, None | Some(XL_PASTE_OPERATION_ADD)) {
                return Err(AutoError::not_modelled(
                    "Range.PasteSpecial column widths with this Operation",
                    "dc-2rjye",
                ));
            }
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
                let src_cols = clip.snapshot.area().column_count();
                let dst_cols = to.column_count();
                // Excel ww22: whole copies of the source tile across the destination; a destination narrower than
                // the source still takes one copy.
                let tiles = (dst_cols / src_cols).max(1);
                let total_cols = tiles * src_cols;
                let end_col = to
                    .first()
                    .column()
                    .checked_add(total_cols - 1)
                    .filter(|&c| c <= calc_value::MAX_COLUMNS)
                    .ok_or_else(|| failed("PasteSpecial"))?;
                let target_area = GridArea::new(
                    to.first(),
                    CellKey::new(to.sheet(), to.last().row(), end_col).map_err(|_| failed("PasteSpecial"))?,
                )
                .map_err(|_| failed("PasteSpecial"))?;
                self.guard_area(session, target_area)?;
                let layouts = session.host().model().definitions().axis_layouts();
                let mut edits = Vec::with_capacity(total_cols as usize);
                for tile in 0..tiles {
                    for (offset, index) in
                        (clip.snapshot.area().first().column()..=clip.snapshot.area().last().column()).enumerate()
                    {
                        let src_fmt = clip.snapshot.axis_format(calc_doc_model::Axis::Columns, index);
                        let column = to.first().column() + tile * src_cols + offset as u32;
                        let dst_fmt = layouts.get(to.sheet(), calc_doc_model::Axis::Columns, column);
                        // The width and hidden state transfer; the outline level does not.
                        let merged = dst_fmt
                            .with_size(src_fmt.size(), src_fmt.custom())
                            .ok_or_else(|| failed("PasteSpecial"))?
                            .with_hidden(src_fmt.hidden());
                        let edit = calc_doc_model::AxisFormatEdit::new(
                            to.sheet(),
                            calc_doc_model::Axis::Columns,
                            column,
                            merged,
                        )
                        .ok_or_else(|| failed("PasteSpecial"))?;
                        edits.push(crate::CellEdit::AxisFormat(edit));
                    }
                }
                return Ok(SkinIntent::Edit { edits });
            }
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
    /// that are not literal numbers are refused: the formula-wrapping and text results are unmeasured (dc-oxh8w.43).
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
                CellEdit::Clear { area } => Some(area.first()),
                _ => None,
            })
            .collect();
        let page = crate::project(host, &ProjectionRequest { cells }).map_err(|_| failed("PasteSpecial"))?;
        let mut current = HashMap::new();
        let mut dest_empty = std::collections::HashSet::new();
        for cell in page.cells() {
            let number = match cell.value() {
                ProjectedValue::Empty => {
                    dest_empty.insert(cell.at());
                    0.0
                }
                ProjectedValue::Literal(v) => match v.core_ref() {
                    calc_value::CoreRef::Number(n) => n,
                    _ => return Err(refuse("onto a destination cell that is not a number")),
                },
                _ => return Err(refuse("onto a formula destination")),
            };
            current.insert(cell.at(), number);
        }
        for edit in plan.edits_mut().iter_mut() {
            // A blank source cell counts as zero onto a numeric destination; blank-on-blank is uncaptured (dc-2rjye).
            let (at, source) = match edit {
                CellEdit::Set { at, input } => match input {
                    CellInput::Number(source) => (*at, source.get()),
                    _ => return Err(refuse("with a source that is not a number")),
                },
                CellEdit::Clear { area } => {
                    let at = area.first();
                    // Excel ww22: blank on blank stays empty for every Operation.
                    if dest_empty.contains(&at) {
                        continue;
                    }
                    (at, 0.0)
                }
                _ => continue,
            };
            let dest = current[&at];
            let result = match operation {
                XL_PASTE_OPERATION_ADD => dest + source,
                XL_PASTE_OPERATION_SUBTRACT => dest - source,
                XL_PASTE_OPERATION_MULTIPLY => dest * source,
                _ if source == 0.0 => {
                    *edit = CellEdit::Set { at, input: CellInput::Error(WorksheetErrorCode::Div0) };
                    continue;
                }
                _ => dest / source,
            };
            *edit = CellEdit::Set {
                at,
                input: crate::FiniteNumber::new(result)
                    .map(CellInput::Number)
                    .unwrap_or(CellInput::Error(WorksheetErrorCode::Num)),
            };
        }
        Ok(())
    }
}
