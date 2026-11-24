//! Workbook structure members: Worksheets.Add, Worksheet.Name, Worksheet.Delete, Worksheet.Move
//! (dc-oxh8w.4). Each is one session command (`WorkbookCommand::Sheet`), so undo, recalculation and
//! events are the ones every other Automation write uses.
use super::*;
use crate::{CommandIngress, HostProgress, IntentError, SheetChange, SkinIntent, WorkbookCommand};

/// xlWorksheet, the only sheet type Worksheets.Add makes here.
const XL_WORKSHEET: i64 = -4167;

impl WorkbookAutomation {
    pub(super) fn register_structure(handlers: &mut Handlers<Self>) {
        handlers.on("Sheets", "Add", InvokeKind::Method, Self::sheets_add);
        handlers.on("Worksheet", "Name", InvokeKind::Put, Self::worksheet_set_name);
        handlers.on("Worksheet", "Delete", InvokeKind::Method, Self::worksheet_delete);
        handlers.on("Worksheet", "Move", InvokeKind::Method, Self::worksheet_move);
    }

    /// One sheet command; a refusal is Excel's 1004 with the host's reason (a name Excel rejects, the
    /// last visible sheet, ...).
    fn sheet_command(
        session: &mut HostSession,
        sheet: Option<SheetId>,
        change: SheetChange,
    ) -> Result<Option<HostProgress>, AutoError> {
        let receipt = session.dispatch_from(
            CommandIngress::Automation,
            SkinIntent::Command(WorkbookCommand::Sheet { sheet, change }),
        );
        match receipt.error {
            None => Ok(receipt.recalc),
            Some(IntentError::Workbook(why)) => Err(AutoError::excel(1004, why)),
            Some(other) => Err(AutoError::excel(1004, other.to_string())),
        }
    }

    fn sheet_argument(arg: &AutoValue, what: &str) -> Result<Option<SheetId>, AutoError> {
        match arg {
            AutoValue::Missing | AutoValue::Empty => Ok(None),
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Worksheet { sheet, .. },
                ..
            }) => Ok(Some(*sheet)),
            _ => Err(AutoError::not_modelled(what, OWNER)),
        }
    }

    /// Excel names a new sheet `Sheet<n>`, n starting one above the sheet count and rising past names in use.
    fn new_sheet_name(model: &ModelRoot) -> String {
        (model.sheets().len() as u32 + 1..)
            .map(|n| format!("Sheet{n}"))
            .find(|name| {
                !model
                    .sheets()
                    .iter()
                    .any(|s| calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), name))
            })
            .expect("a free sheet name")
    }

    fn sheets_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let mut result = Ok(AutoValue::Empty);
        let reply = self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let before = Self::sheet_argument(call.arg(0), "Sheets.Add Before as other than a worksheet")?;
            let after = Self::sheet_argument(call.arg(1), "Sheets.Add After as other than a worksheet")?;
            if before.is_some() && after.is_some() {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            match call.arg(2) {
                AutoValue::Missing | AutoValue::Empty => {}
                other if geometry::integer(other)? == 1 => {}
                _ => return Err(AutoError::not_modelled("Sheets.Add Count above one", OWNER)),
            }
            match call.arg(3) {
                AutoValue::Missing | AutoValue::Empty => {}
                other if geometry::integer(other)? == XL_WORKSHEET => {}
                _ => return Err(AutoError::not_modelled("Sheets.Add of a sheet type other than xlWorksheet", OWNER)),
            }
            let model = session.host().model();
            let order: Vec<SheetId> = model.sheets().iter().map(|s| s.id()).collect();
            let position_of = |sheet: SheetId| {
                order
                    .iter()
                    .position(|s| *s == sheet)
                    .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))
            };
            // Before the active sheet unless told otherwise.
            let position = match (before, after) {
                (Some(sheet), _) => position_of(sheet)? + 1,
                (_, Some(sheet)) => position_of(sheet)? + 2,
                _ => position_of(
                    meta.active
                        .map(|at| at.sheet())
                        .ok_or_else(|| AutoError::not_modelled("Sheets.Add without an active sheet", OWNER))?,
                )? + 1,
            };
            let name = Self::new_sheet_name(model);
            let mut progress = Self::sheet_command(session, None, SheetChange::Add { name })?;
            let added = session
                .host()
                .model()
                .sheets()
                .iter()
                .map(|s| s.id())
                .find(|id| !order.contains(id))
                .expect("the added sheet");
            if position != order.len() + 1 {
                progress = Self::sheet_command(session, Some(added), SheetChange::Move { position: position as u32 })?
                    .or(progress);
            }
            // The new sheet becomes the active one.
            let first = CellKey::new(added, 1, 1).map_err(|_| geometry::type_mismatch())?;
            session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(crate::Selection::cell(first)));
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            result = Ok(AutoValue::Object(object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: self.metadata()?.workbook,
                    sheet: added,
                },
            )));
            Ok(AutoValue::Empty)
        });
        reply?;
        result
    }

    fn worksheet_sheet(target: &ObjectRef) -> Result<SheetId, AutoError> {
        match target.key {
            ObjectKey::Worksheet { sheet, .. } => Ok(sheet),
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn sheet_change(&self, target: &ObjectRef, call: &Call<'_>, change: SheetChange) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let progress = Self::sheet_command(session, Some(sheet), change.clone())?;
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn worksheet_set_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let [name] = call.args.as_slice() else {
            return Err(geometry::bad_count());
        };
        let name = name.as_text().ok_or_else(geometry::type_mismatch)?;
        self.sheet_change(target, call, SheetChange::Rename { name })
    }

    fn worksheet_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.sheet_change(target, call, SheetChange::Remove)
    }

    fn worksheet_move(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        let before = Self::sheet_argument(call.arg(0), "Worksheet.Move Before as other than a worksheet")?;
        let after = Self::sheet_argument(call.arg(1), "Worksheet.Move After as other than a worksheet")?;
        if before.is_some() == after.is_some() {
            return Err(AutoError::not_modelled("Worksheet.Move to a new workbook", OWNER));
        }
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let order: Vec<SheetId> = session.host().model().sheets().iter().map(|s| s.id()).collect();
            let index = |id: SheetId| order.iter().position(|s| *s == id);
            let from = index(sheet).ok_or_else(geometry::type_mismatch)?;
            let anchor = index(before.or(after).unwrap()).ok_or_else(geometry::type_mismatch)?;
            // Positions are those after the move: moving later removes the sheet from before the anchor.
            let slot = if after.is_some() { anchor + 1 } else { anchor };
            let position = if from < slot { slot } else { slot + 1 };
            let progress = Self::sheet_command(session, Some(sheet), SheetChange::Move { position: position as u32 })?;
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }
}
