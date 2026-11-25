//! Selection and activation members: Range.Select, Worksheet.Select, Worksheet.Activate and
//! Application.Goto (dc-oxh8w.46). Each moves the session's selection through `SkinIntent::Select`,
//! so selection events, interests and the active sheet follow the one route every user selection takes.
use super::*;
use crate::{CommandIngress, IntentError, Selection, SkinIntent};
use calc_doc_model::SheetVisibility;

/// The open bead that owns what this file refuses (grouped sheets, Goto without a Reference).
const OWNER: &str = "dc-oxh8w.48";

impl WorkbookAutomation {
    pub(super) fn register_select(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Select", InvokeKind::Method, Self::range_select);
        handlers.on("Range", "Activate", InvokeKind::Method, Self::range_activate);
        handlers.on("Worksheet", "Select", InvokeKind::Method, Self::worksheet_select);
        handlers.on("Worksheet", "Activate", InvokeKind::Method, Self::worksheet_activate);
        handlers.on("Application", "Goto", InvokeKind::Method, Self::app_goto);
        handlers.on("Range", "Range", InvokeKind::Get, Self::range_range);
        handlers.on("Worksheet", "Rows", InvokeKind::Get, |m, t, c| m.sheet_axis(t, c, RangeAxis::Rows));
        handlers.on("Worksheet", "Columns", InvokeKind::Get, |m, t, c| m.sheet_axis(t, c, RangeAxis::Columns));
        handlers.on("Application", "Rows", InvokeKind::Get, |m, t, c| m.sheet_axis(t, c, RangeAxis::Rows));
        handlers.on("Application", "Columns", InvokeKind::Get, |m, t, c| m.sheet_axis(t, c, RangeAxis::Columns));
    }

    /// `Worksheet.Rows`/`Columns`, and `Application.Rows`/`Columns` on the active sheet: every row or column.
    fn sheet_axis(&self, target: &ObjectRef, call: &Call<'_>, axis: RangeAxis) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            let sheet = match &target.key {
                ObjectKey::Worksheet { sheet, .. } => *sheet,
                _ => match access {
                    Access::Invocation(context) => context
                        .user_view()
                        .map(|v| v.active_sheet)
                        .or_else(|| meta.active.map(|at| at.sheet())),
                    _ => meta.active.map(|at| at.sheet()),
                }
                .ok_or_else(|| AutoError::not_modelled("_Application.Rows without an active worksheet", OWNER))?,
            };
            sheet_name(meta, sheet)?;
            self.acquire(access, &ReferenceLike::area(GridArea::full_sheet(sheet)), axis)
        })
    }

    fn failed(class: &str, member: &str) -> AutoError {
        AutoError::excel(1004, format!("{member} method of {class} class failed"))
    }

    /// The sheet the user is on: the selection's sheet, else the document's active sheet, else the first.
    fn session_active_sheet(session: &HostSession) -> Option<SheetId> {
        session
            .selection()
            .active
            .map(|at| at.sheet())
            .or_else(|| session.selection().areas.first().map(|area| area.sheet()))
            .or_else(|| session.host().loaded().active_sheet())
            .or_else(|| session.host().sheets().first().map(|s| s.id()))
    }

    fn select_intent(session: &mut HostSession, selection: Selection, class: &str, member: &str) -> Result<(), AutoError> {
        let receipt = session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(selection));
        match receipt.error {
            None => Ok(()),
            Some(IntentError::UnknownSheet(_)) => Err(Self::failed(class, member)),
            Some(other) => Err(AutoError::excel(1004, other.to_string())),
        }
    }

    fn visible_sheet(session: &HostSession, sheet: SheetId) -> bool {
        session
            .host()
            .model()
            .sheets()
            .iter()
            .find(|s| s.id() == sheet)
            .is_some_and(|s| s.visibility() == SheetVisibility::Visible)
    }

    /// The selection that selecting `held` makes: all its areas, the active cell the first area's first cell.
    fn range_selection(areas: &[GridArea]) -> Selection {
        Selection {
            active: areas.first().map(|a| a.first()),
            areas: areas.to_vec(),
        }
    }

    fn held_of(target: &ObjectRef) -> Result<StoredReference, AutoError> {
        match &target.key {
            ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. } => Ok(held.clone()),
            _ => Err(geometry::type_mismatch()),
        }
    }

    /// Range.Select: the range's sheet must be the active one, as in Excel (1004 otherwise).
    fn range_select(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let held = Self::held_of(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let range = self.resolve(&mut Access::Session(session, false), &held)?;
            let areas = range.areas();
            let sheet = areas.first().map(|a| a.sheet()).ok_or_else(geometry::type_mismatch)?;
            if Self::session_active_sheet(session) != Some(sheet) {
                return Err(Self::failed("Range", "Select"));
            }
            Self::select_intent(session, Self::range_selection(areas), "Range", "Select")?;
            self.context_changed(session);
            Ok(AutoValue::Bool(true))
        })
    }

    /// Range.Activate: the range's first cell becomes the active cell. Inside the current selection the selection
    /// stays; a cell outside it is not captured from Excel yet (dc-oxh8w.48).
    fn range_activate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let held = Self::held_of(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let range = self.resolve(&mut Access::Session(session, false), &held)?;
            let cell = range.areas().first().ok_or_else(geometry::type_mismatch)?.first();
            if Self::session_active_sheet(session) != Some(cell.sheet()) {
                return Err(Self::failed("Range", "Activate"));
            }
            let current = session.selection().clone();
            let inside = current.areas.iter().any(|a| {
                a.sheet() == cell.sheet()
                    && (a.first().row()..=a.last().row()).contains(&cell.row())
                    && (a.first().column()..=a.last().column()).contains(&cell.column())
            });
            if !inside {
                return Err(AutoError::not_modelled("Range.Activate outside the current selection", OWNER));
            }
            Self::select_intent(session, Selection { active: Some(cell), areas: current.areas }, "Range", "Activate")?;
            self.context_changed(session);
            Ok(AutoValue::Bool(true))
        })
    }

    /// Makes `sheet` the active sheet with the selection it last held (A1 when it never held one).
    fn activate_sheet(&self, target: &ObjectRef, class: &str, member: &str) -> Result<(), AutoError> {
        let sheet = Self::worksheet_sheet(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            if !Self::visible_sheet(session, sheet) {
                return Err(Self::failed(class, member));
            }
            let selection = match session.remembered_selection(sheet) {
                Some(kept) if Self::session_active_sheet(session) != Some(sheet) => kept.clone(),
                _ if Self::session_active_sheet(session) == Some(sheet) => return Ok(AutoValue::Empty),
                _ => {
                    let a1 = CellKey::new(sheet, 1, 1).map_err(|_| Self::failed(class, member))?;
                    Selection { active: Some(a1), areas: vec![GridArea::cell(a1)] }
                }
            };
            Self::select_intent(session, selection, class, member)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
        .map(|_| ())
    }

    /// Worksheet.Select(Replace): Replace False adds the sheet to a group, which is not modelled.
    fn worksheet_select(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty | AutoValue::Bool(true) => {}
            AutoValue::Bool(false) => {
                return Err(AutoError::not_modelled("Worksheet.Select Replace:=False (sheet groups)", OWNER));
            }
            _ => return Err(geometry::type_mismatch()),
        }
        self.activate_sheet(target, "Worksheet", "Select")?;
        Ok(AutoValue::Bool(true))
    }

    fn worksheet_activate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.activate_sheet(target, "Worksheet", "Activate")?;
        Ok(AutoValue::Empty)
    }

    /// Application.Goto(Reference, Scroll): selects the range, activating its sheet; Scroll moves a window
    /// DNA Calc does not have, so it is read and ignored. Text is R1C1 (relative to the active cell), A1 or a
    /// defined name; an omitted Reference needs Excel's last-Goto memory (dc-oxh8w.48).
    fn app_goto(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        match call.arg(1) {
            AutoValue::Missing | AutoValue::Empty | AutoValue::Bool(_) => {}
            _ => return Err(geometry::type_mismatch()),
        }
        let held = match call.arg(0) {
            AutoValue::Object(object) => Some(Self::held_of(object)?),
            AutoValue::Missing | AutoValue::Empty => {
                return Err(AutoError::not_modelled("Application.Goto without a Reference", OWNER));
            }
            AutoValue::Text(_) => None,
            _ => return Err(geometry::type_mismatch()),
        };
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let range = match &held {
                Some(held) => self.resolve(&mut Access::Session(session, false), held)?,
                None => {
                    // R1C1 text, relative to the active cell; anything else is read as A1 or a defined name.
                    let text = call.arg(0).as_text().unwrap_or_default();
                    let active = Self::session_active_sheet(session)
                        .and_then(|sheet| session.selection().active.filter(|at| at.sheet() == sheet))
                        .or(meta.active);
                    let spelled = crate::c_api_commands::r1c1_text_to_a1(&text, || {
                        active.map(|at| (at.row(), at.column())).ok_or_else(|| crate::c_api_commands::XlcError::Operand("active cell"))
                    })
                    .unwrap_or(text);
                    let mut access = Access::Session(session, false);
                    self.range_argument(&mut access, &meta, &AutoValue::text(&spelled), None)?
                }
            };
            let areas = range.areas();
            let sheet = areas.first().map(|a| a.sheet()).ok_or_else(geometry::type_mismatch)?;
            if !Self::visible_sheet(session, sheet) {
                return Err(Self::failed("Application", "Goto"));
            }
            Self::select_intent(session, Self::range_selection(areas), "Application", "Goto")?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }
}

/// Whether `text` is a plain A1 cell or cell:cell reference (`B2`, `$B$2:C3`), which Range.Range reads relative
/// to the range's top-left cell; names, whole rows and columns and unions are not.
fn plain_a1_area(text: &str) -> bool {
    let cell = |part: &str| {
        let part = part.strip_prefix('$').unwrap_or(part);
        let letters = part.chars().take_while(|c| c.is_ascii_alphabetic()).count();
        let rest = part[letters..].strip_prefix('$').unwrap_or(&part[letters..]);
        (1..=3).contains(&letters) && !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    };
    let mut parts = text.split(':');
    let (first, second) = (parts.next().unwrap_or(""), parts.next());
    cell(first) && second.is_none_or(cell) && parts.next().is_none()
}

impl WorkbookAutomation {
    /// Range.Range(Cell1, Cell2): a cell or area given as text is read relative to this range's top-left cell
    /// (`Range("B2:C3").Range("A1")` is B2); a Range object is used as it is; two arguments span both.
    fn range_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        self.with_range(target, |access, meta, range, _| {
            let origin = range.areas().first().ok_or_else(geometry::type_mismatch)?.first();
            let mut spans = Vec::new();
            for arg in call.args.iter().take(2) {
                if matches!(arg, AutoValue::Missing) {
                    continue;
                }
                if matches!(arg, AutoValue::Empty | AutoValue::Null | AutoValue::Error(_) | AutoValue::Bool(_)) {
                    return Err(AutoError::excel(1004, ""));
                }
                let area = match arg {
                    AutoValue::Text(_) if !plain_a1_area(&arg.as_text().unwrap_or_default().trim().replace(' ', "")) => {
                        return Err(AutoError::not_modelled("Range.Range with a name, whole row or column or union", OWNER));
                    }
                    _ => {
                        let found = self.range_argument(access, meta, arg, Some(origin.sheet()))?;
                        if found.area_count() != 1 {
                            return Err(AutoError::excel(1004, ""));
                        }
                        found.areas()[0]
                    }
                };
                let area = if matches!(arg, AutoValue::Text(_)) {
                    let (dr, dc) = (origin.row() - 1, origin.column() - 1);
                    let first = CellKey::new(area.sheet(), area.first().row() + dr, area.first().column() + dc)
                        .map_err(|_| AutoError::excel(1004, ""))?;
                    let last = CellKey::new(area.sheet(), area.last().row() + dr, area.last().column() + dc)
                        .map_err(|_| AutoError::excel(1004, ""))?;
                    GridArea::new(first, last).map_err(|_| AutoError::excel(1004, ""))?
                } else {
                    area
                };
                if area.sheet() != origin.sheet() {
                    return Err(AutoError::excel(1004, ""));
                }
                spans.push(area);
            }
            let first = spans.first().ok_or_else(geometry::bad_count)?;
            let (mut top, mut left, mut bottom, mut right) =
                (first.first().row(), first.first().column(), first.last().row(), first.last().column());
            for a in &spans {
                top = top.min(a.first().row());
                left = left.min(a.first().column());
                bottom = bottom.max(a.last().row());
                right = right.max(a.last().column());
            }
            let sheet = origin.sheet();
            let area = GridArea::new(
                CellKey::new(sheet, top, left).map_err(|_| AutoError::excel(1004, ""))?,
                CellKey::new(sheet, bottom, right).map_err(|_| AutoError::excel(1004, ""))?,
            )
            .map_err(|_| AutoError::excel(1004, ""))?;
            self.acquire(access, &ReferenceLike::area(area), RangeAxis::Cells)
        })
    }
}
