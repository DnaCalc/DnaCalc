//! Selection and activation members: Range.Select, Worksheet.Select, Worksheet.Activate and
//! Application.Goto (dc-oxh8w.46). Each moves the session's selection through `SkinIntent::Select`,
//! so selection events, interests and the active sheet follow the one route every user selection takes.
use super::*;
use crate::{CommandIngress, IntentError, Selection, SkinIntent};
use calc_doc_model::SheetVisibility;

/// The open bead that owns what this file refuses (grouped sheets, text references for Goto).
const OWNER: &str = "dc-oxh8w.48";

impl WorkbookAutomation {
    pub(super) fn register_select(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Select", InvokeKind::Method, Self::range_select);
        handlers.on("Worksheet", "Select", InvokeKind::Method, Self::worksheet_select);
        handlers.on("Worksheet", "Activate", InvokeKind::Method, Self::worksheet_activate);
        handlers.on("Application", "Goto", InvokeKind::Method, Self::app_goto);
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
    /// DNA Calc does not have, so it is read and ignored. A text reference or an omitted Reference needs the
    /// R1C1 and name parser and Excel's last-Goto memory (dc-oxh8w.48).
    fn app_goto(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        match call.arg(1) {
            AutoValue::Missing | AutoValue::Empty | AutoValue::Bool(_) => {}
            _ => return Err(geometry::type_mismatch()),
        }
        let destination = match call.arg(0) {
            AutoValue::Object(object) => object,
            AutoValue::Missing | AutoValue::Empty => {
                return Err(AutoError::not_modelled("Application.Goto without a Reference", OWNER));
            }
            AutoValue::Text(_) => {
                return Err(AutoError::not_modelled("Application.Goto with a text reference", OWNER));
            }
            _ => return Err(geometry::type_mismatch()),
        };
        let held = Self::held_of(destination)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let range = self.resolve(&mut Access::Session(session, false), &held)?;
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
