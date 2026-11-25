//! Workbook structure members: Worksheets.Add, Worksheet.Name, Worksheet.Delete, Worksheet.Move
//! (dc-oxh8w.4). Each is one session command (`WorkbookCommand::Sheet`), so undo, recalculation and
//! events are the ones every other Automation write uses.
use super::*;
use calc_doc_model::SheetVisibility;
use calc_object_model::ReportItem;
use crate::{NameEdit, CommandIngress, HostProgress, IntentError, SheetChange, SkinIntent, WorkbookCommand};

/// xlWorksheet, and xlChart (a chart sheet): the sheet types Sheets.Add makes here.
const XL_WORKSHEET: i64 = -4167;
const XL_CHART: i64 = -4109;
/// The open bead that owns the structural remainders this file refuses.
const OWNER: &str = "dc-oxh8w.43";

impl WorkbookAutomation {
    pub(super) fn register_structure(handlers: &mut Handlers<Self>) {
        handlers.on("Sheets", "Add", InvokeKind::Method, Self::sheets_add);
        handlers.on("Sheets", "Move", InvokeKind::Method, Self::sheets_move);
        handlers.on("Worksheet", "Name", InvokeKind::Put, Self::worksheet_set_name);
        handlers.on("Worksheet", "Visible", InvokeKind::Get, Self::worksheet_visible);
        handlers.on("Worksheet", "Visible", InvokeKind::Put, Self::worksheet_set_visible);
        handlers.on("Worksheet", "Delete", InvokeKind::Method, Self::worksheet_delete);
        handlers.on("Worksheet", "Move", InvokeKind::Method, Self::worksheet_move);
        handlers.on("Names", "Add", InvokeKind::Method, Self::names_add);
    }

    /// One sheet command; a refusal is Excel's 1004 with the host's reason (a name Excel rejects, the
    /// last visible sheet, ...).
    pub(super) fn sheet_command(
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

    pub(super) fn sheet_argument(arg: &AutoValue, what: &str) -> Result<Option<SheetId>, AutoError> {
        match arg {
            AutoValue::Missing | AutoValue::Empty => Ok(None),
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Worksheet { sheet, .. },
                ..
            }) => Ok(Some(*sheet)),
            _ => Err(AutoError::not_modelled(what, OWNER)),
        }
    }

    /// A Before or After worksheet of this workbook. One of another workbook is a typed refusal: its sheet identity
    /// means nothing here (Excel copies or moves into that workbook).
    pub(super) fn local_sheet_argument(&self, arg: &AutoValue, what: &str) -> Result<Option<SheetId>, AutoError> {
        if let AutoValue::Object(ObjectRef { key: ObjectKey::Worksheet { workbook, .. }, .. }) = arg {
            if *workbook != self.workbook_identity()? {
                return Err(AutoError::not_modelled("Worksheet.Copy or Move Before/After a worksheet of another workbook", "dc-fa8cx"));
            }
        }
        Self::sheet_argument(arg, what)
    }

    /// A Before or After argument: a worksheet or a chart sheet's Chart.
    fn sheet_or_chart_argument(meta: &Snapshot, arg: &AutoValue, what: &str) -> Result<Option<SheetId>, AutoError> {
        match arg {
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Report { sheet, item: ReportItem::Chart(_), .. },
                ..
            }) if meta.chart_sheets.contains(sheet) => Ok(Some(*sheet)),
            other => Self::sheet_argument(other, what),
        }
    }

    /// A new chart sheet is named `Chart<n>`, n the lowest not in use (an assumption until the Windows run).
    pub(super) fn new_chart_sheet_name(model: &ModelRoot) -> String {
        (1u32..)
            .map(|n| format!("Chart{n}"))
            .find(|name| {
                !model
                    .sheets()
                    .iter()
                    .any(|s| calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), name))
            })
            .expect("a free sheet name")
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
        if call.args.len() > 4 {
            return Err(geometry::bad_count());
        }
        let mut result = Ok(AutoValue::Empty);
        let reply = self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let before = Self::sheet_or_chart_argument(&meta, call.arg(0), "Sheets.Add Before as other than a worksheet")?;
            let after = Self::sheet_or_chart_argument(&meta, call.arg(1), "Sheets.Add After as other than a worksheet")?;
            if before.is_some() && after.is_some() {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            let count = match call.arg(2) {
                AutoValue::Missing | AutoValue::Empty => 1,
                other => geometry::integer(other)?,
            };
            if !(1..=255).contains(&count) {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            // Charts.Add makes chart sheets whatever its (absent) Type; Sheets.Add takes xlWorksheet or xlChart.
            let chart = match (&target.key, call.arg(3)) {
                (ObjectKey::Charts(_), _) => true,
                (_, AutoValue::Missing | AutoValue::Empty) => false,
                (_, other) => match geometry::integer(other)? {
                    XL_WORKSHEET => false,
                    XL_CHART => true,
                    _ => return Err(AutoError::not_modelled("Sheets.Add of a sheet type other than xlWorksheet or xlChart", OWNER)),
                },
            };
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
            // Count sheets go in as one block before or after the anchor; the first becomes the active sheet.
            let mut progress = None;
            let mut first_added = None;
            // Excel numbers a new chart's series from its automatic source: the selection of the sheet it is added from.
            let from_sheet = meta.active.map(|at| at.sheet());
            let unique_start = from_sheet
                .and_then(|sheet| Self::selection_data(session, sheet))
                .map_or(0, |area| Self::automatic_series(session, area));
            for step in 0..count as usize {
                let known: Vec<SheetId> = session.host().model().sheets().iter().map(|s| s.id()).collect();
                let change = if chart {
                    SheetChange::AddChart { name: Self::new_chart_sheet_name(session.host().model()) }
                } else {
                    SheetChange::Add { name: Self::new_sheet_name(session.host().model()) }
                };
                progress = Self::sheet_command(session, None, change)?.or(progress);
                let added = session
                    .host()
                    .model()
                    .sheets()
                    .iter()
                    .map(|s| s.id())
                    .find(|id| !known.contains(id))
                    .expect("the added sheet");
                let at = position + step;
                if at != known.len() + 1 {
                    progress =
                        Self::sheet_command(session, Some(added), SheetChange::Move { position: at as u32 })?.or(progress);
                }
                if chart {
                    let receipt = session.dispatch_from(
                        CommandIngress::Automation,
                        SkinIntent::Command(WorkbookCommand::ChartCreateEmpty {
                            sheet: added,
                            revision: session.host().revision().get(),
                            kind: "column".into(),
                            place: [0.0; 4].map(crate::Points::new),
                            title: None,
                            legend: true,
                            unique_start,
                        }),
                    );
                    if let Some(error) = receipt.error {
                        return Err(AutoError::excel(1004, error.to_string()));
                    }
                }
                session.notify_document_event(crate::DocumentEvent::NewSheet { sheet: added });
                first_added.get_or_insert(added);
            }
            let added = first_added.expect("at least one sheet");
            // The new sheet becomes the active one.
            let first = CellKey::new(added, 1, 1).map_err(|_| geometry::type_mismatch())?;
            session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(crate::Selection::cell(first)));
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            result = if chart {
                Self::chart_of_sheet(&*self.metadata()?, added)
            } else {
                Ok(AutoValue::Object(object(
                    "Worksheet",
                    ObjectKey::Worksheet {
                        workbook: self.metadata()?.workbook,
                        sheet: added,
                    },
                )))
            };
            Ok(AutoValue::Empty)
        });
        reply?;
        result
    }

    pub(super) fn worksheet_sheet(target: &ObjectRef) -> Result<SheetId, AutoError> {
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

    /// `Worksheet.Visible`: xlSheetVisible -1, xlSheetHidden 0, xlSheetVeryHidden 2.
    fn worksheet_visible(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            let visibility = meta.model.sheet_by_id(sheet).ok_or_else(geometry::type_mismatch)?.visibility();
            Ok(AutoValue::I4(match visibility {
                SheetVisibility::Visible => -1,
                SheetVisibility::Hidden => 0,
                SheetVisibility::VeryHidden => 2,
            }))
        })
    }

    /// `Worksheet.Visible = v`: True/False or an xlSheetVisibility value. Hiding the last visible
    /// sheet, or any other value, is 1004 "Unable to set the Visible property of the Worksheet class".
    fn worksheet_set_visible(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let [value] = call.args.as_slice() else {
            return Err(geometry::bad_count());
        };
        let failed = || AutoError::excel(1004, "Unable to set the Visible property of the Worksheet class");
        let visibility = match value {
            AutoValue::Bool(true) => SheetVisibility::Visible,
            AutoValue::Bool(false) => SheetVisibility::Hidden,
            AutoValue::I2(_) | AutoValue::I4(_) | AutoValue::R4(_) | AutoValue::R8(_) => {
                match crate::automation::report_draw::number(value)? {
                    n if n == -1.0 => SheetVisibility::Visible,
                    n if n == 0.0 => SheetVisibility::Hidden,
                    n if n == 2.0 => SheetVisibility::VeryHidden,
                    _ => return Err(failed()),
                }
            }
            _ => return Err(geometry::type_mismatch()),
        };
        let sheet = Self::worksheet_sheet(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let model = session.host().model();
            let sheets = model.sheets();
            let pos = sheets.iter().position(|s| s.id() == sheet).ok_or_else(failed)?;
            let replacement = if visibility != SheetVisibility::Visible {
                sheets[pos + 1..]
                    .iter()
                    .find(|s| s.visibility() == SheetVisibility::Visible)
                    .or_else(|| {
                        sheets[..pos]
                            .iter()
                            .rev()
                            .find(|s| s.visibility() == SheetVisibility::Visible)
                    })
                    .map(|s| s.id())
            } else {
                None
            };
            if visibility != SheetVisibility::Visible && replacement.is_none() {
                return Err(failed());
            }
            let was_active = Self::session_active_sheet(session) == Some(sheet);
            let progress = Self::sheet_command(session, Some(sheet), SheetChange::SetVisibility(visibility))?;
            Self::settle(session, progress, call)?;
            if was_active
                && let Some(next_sheet) = replacement
            {
                let selection = session
                    .remembered_selection(next_sheet)
                    .cloned()
                    .unwrap_or_else(|| {
                        let a1 = CellKey::new(next_sheet, 1, 1).expect("A1 is valid");
                        crate::Selection::cell(a1)
                    });
                session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(selection));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn worksheet_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.sheet_change(target, call, SheetChange::Remove)
    }

    fn worksheet_move(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        let (before, after) = self.move_anchors(call)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            if before.is_none() && after.is_none() {
                return self.copy_to_new_book(session, &[sheet], true, call);
            }
            Self::move_one(session, sheet, before, after, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// Sheets.Move / Worksheets.Move: every sheet the collection lists moves, in order, to the Before or
    /// After anchor, or into a new workbook when neither is given.
    fn sheets_move(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (before, after) = self.move_anchors(call)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let listed: Vec<SheetId> = self.listed(target, &meta).iter().map(|s| s.id()).collect();
            if matches!(target.key, ObjectKey::Charts(_)) || listed.iter().any(|s| meta.chart_sheets.contains(s)) {
                return Err(AutoError::not_modelled("Sheets.Move of chart sheets", OWNER));
            }
            if before.is_none() && after.is_none() {
                return self.copy_to_new_book(session, &listed, true, call);
            }
            if before.or(after).is_some_and(|anchor| listed.contains(&anchor)) {
                return Err(AutoError::not_modelled("Sheets.Move with a Before or After sheet inside the moved group", "dc-fa8cx"));
            }
            let mut after = after;
            for sheet in listed {
                Self::move_one(session, sheet, before, after, call)?;
                if after.is_some() {
                    after = Some(sheet);
                }
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn move_anchors(&self, call: &Call<'_>) -> Result<(Option<SheetId>, Option<SheetId>), AutoError> {
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let before = self.local_sheet_argument(call.arg(0), "Worksheet.Move Before as other than a worksheet")?;
        let after = self.local_sheet_argument(call.arg(1), "Worksheet.Move After as other than a worksheet")?;
        if before.is_some() && after.is_some() {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        Ok((before, after))
    }

    fn move_one(
        session: &mut HostSession,
        sheet: SheetId,
        before: Option<SheetId>,
        after: Option<SheetId>,
        call: &Call<'_>,
    ) -> Result<(), AutoError> {
        let order: Vec<SheetId> = session.host().model().sheets().iter().map(|s| s.id()).collect();
        let index = |id: SheetId| order.iter().position(|s| *s == id);
        let from = index(sheet).ok_or_else(geometry::type_mismatch)?;
        let anchor = index(before.or(after).unwrap()).ok_or_else(geometry::type_mismatch)?;
        // Positions are those after the move: moving later removes the sheet from before the anchor.
        let slot = if after.is_some() { anchor + 1 } else { anchor };
        let position = if from < slot { slot } else { slot + 1 };
        let progress = Self::sheet_command(session, Some(sheet), SheetChange::Move { position: position as u32 })?;
        Self::settle(session, progress, call)
    }
}

impl WorkbookAutomation {
    /// Names.Add(Name, RefersTo): a workbook-level name, redefining the name when it exists, as Excel does.
    fn names_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 11 {
            return Err(geometry::bad_count());
        }
        // Visible, MacroType, ShortcutKey, Category, the Local forms and the R1C1 forms are not modelled.
        for (index, what) in [
            (2, "Visible"),
            (3, "MacroType"),
            (4, "ShortcutKey"),
            (5, "Category"),
            (6, "NameLocal"),
            (7, "RefersToLocal"),
            (8, "CategoryLocal"),
            (9, "RefersToR1C1"),
            (10, "RefersToR1C1Local"),
        ] {
            let missing = call.arg(index).is_missing();
            let default_visible = index == 2 && matches!(call.arg(2), AutoValue::Bool(true));
            if !missing && !default_visible {
                return Err(AutoError::not_modelled(format!("Names.Add {what}"), OWNER));
            }
        }
        let name = call.arg(0).as_text().ok_or_else(|| {
            AutoError::not_modelled("Names.Add without a Name text", OWNER)
        })?;
        // "Sheet1!Local" (or 'My Sheet'!Local) is a worksheet-scoped name; the sheet must exist.
        let (sheet_part, name) = match name.rfind('!') {
            Some(at) => (Some(name[..at].to_owned()), name[at + 1..].to_owned()),
            None => (None, name),
        };
        let formula = call
            .arg(1)
            .as_text()
            .filter(|text| text.starts_with('='))
            .ok_or_else(|| AutoError::not_modelled("Names.Add RefersTo other than formula text", OWNER))?;
        let mut result = Ok(AutoValue::Empty);
        let reply = self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let scope = match &sheet_part {
                None => match target.key {
                    // Worksheet.Names.Add scopes a bare name to that worksheet.
                    ObjectKey::SheetNames { sheet, .. } => calc_doc_model::NameScope::Sheet(sheet),
                    _ => calc_doc_model::NameScope::Workbook,
                },
                Some(part) => {
                    let wanted = match part.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')) {
                        Some(quoted) => quoted.replace("''", "'"),
                        None => part.clone(),
                    };
                    let sheet = session
                        .host()
                        .model()
                        .sheets()
                        .iter()
                        .find(|s| calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), &wanted))
                        .map(|s| s.id())
                        .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))?;
                    calc_doc_model::NameScope::Sheet(sheet)
                }
            };
            let existing = session
                .host()
                .model()
                .definitions()
                .name_table()
                .lookup(scope, &name)
                .map(|node| (node.id(), node.binding_version(), node.definition().name.clone()));
            if existing.as_ref().is_some_and(|(_, _, spelling)| *spelling != name) {
                return Err(AutoError::not_modelled(
                    "Names.Add over an existing name spelled in another case",
                    OWNER,
                ));
            }
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                    id: existing.as_ref().map(|(id, _, _)| *id),
                    version: existing.as_ref().map(|(_, version, _)| *version),
                    scope,
                    name: name.clone(),
                    formula: Some(formula.clone()),
                })),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::excel(1004, error.to_string()));
            }
            Self::settle(session, receipt.recalc, call)?;
            self.context_changed(session);
            let id = session
                .host()
                .model()
                .definitions()
                .name_table()
                .lookup(scope, &name)
                .map(|node| node.id())
                .ok_or_else(|| AutoError::not_modelled("Names.Add result lookup", OWNER))?;
            result = Ok(AutoValue::Object(object(
                "Name",
                ObjectKey::Name {
                    workbook: self.metadata()?.workbook,
                    id,
                },
            )));
            Ok(AutoValue::Empty)
        });
        reply?;
        result
    }
}
