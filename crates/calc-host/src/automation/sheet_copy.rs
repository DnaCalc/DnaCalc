//! Worksheet.Copy (dc-oxh8w.43): a sheet clone made of the host's own commands. The new sheet is added
//! and placed by the Sheets.Add route; its cells and cell styles go in through the one paste planner
//! (`prepare_paste_oriented`, as `Range.Copy` does), and the sheet-level state rides in the same paste
//! command as further edits (one undo step): column and row sizes, merges, validations, conditional
//! rules, page setup, the AutoFilter, frozen panes and the authored charts. State the clone cannot yet
//! carry (file-backed charts, drawing objects, sparklines, notes, hyperlinks, tables, pivots, sheet-local
//! names) refuses the copy before anything changes, typed, naming the owning bead.
use super::*;
use crate::{CellEdit, CommandIngress, CopySnapshot, PasteMode, PasteSource, SheetChange, SkinIntent, WorkbookCommand, prepare_paste_oriented};
use calc_doc_model::AxisFormatEdit;

const OWNER: &str = "dc-oxh8w.43";
/// A sheet name has at most 31 UTF-16 code units.
const NAME_LIMIT: usize = 31;

impl WorkbookAutomation {
    pub(super) fn register_sheet_copy(handlers: &mut Handlers<Self>) {
        handlers.on("Worksheet", "Copy", InvokeKind::Method, Self::worksheet_copy);
        handlers.on("Sheets", "Copy", InvokeKind::Method, Self::sheets_copy);
    }

    /// Excel names the clone `<name> (n)`, n from 2 up past names in use, the base cut so the whole fits.
    fn copy_name(model: &ModelRoot, source: &str) -> String {
        // A copy of "Sheet1 (2)" is "Sheet1 (3)": a trailing " (n)" is not part of the base (unmeasured: RUN-ON-WINDOWS).
        let source = match source.rfind(" (") {
            Some(at) if source.ends_with(')') && source[at + 2..source.len() - 1].chars().all(|c| c.is_ascii_digit())
                && source.len() - 1 > at + 2 => &source[..at],
            _ => source,
        };
        (2u32..)
            .map(|n| {
                let suffix = format!(" ({n})");
                let room = NAME_LIMIT - suffix.encode_utf16().count();
                let mut base = String::new();
                for ch in source.chars() {
                    if base.encode_utf16().count() + ch.len_utf16() > room {
                        break;
                    }
                    base.push(ch);
                }
                base + &suffix
            })
            .find(|name| {
                !model
                    .sheets()
                    .iter()
                    .any(|s| calc_value::workbook_names_equal(&s.name().text().to_string_lossy(), name))
            })
            .expect("a free sheet name")
    }

    fn worksheet_copy(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let source = Self::worksheet_sheet(target)?;
        let (before, after) = self.copy_anchors(call)?;
        if before.is_none() && after.is_none() {
            return self.with_write(|session| {
                let meta = self.metadata()?;
                self.validate(target, &meta)?;
                self.copy_to_new_book(session, &[source], false, call)
            });
        }
        let reply = self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            Self::copy_one(session, source, before, after, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        });
        reply
    }

    /// Sheets.Copy / Worksheets.Copy: every worksheet the collection lists is copied, in order, to the
    /// Before or After anchor (each copy lands after the previous one, or before the same anchor).
    fn sheets_copy(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (before, after) = self.copy_anchors(call)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let listed: Vec<SheetId> = self.listed(target, &meta).iter().map(|s| s.id()).collect();
            if matches!(target.key, ObjectKey::Charts(_)) || listed.iter().any(|s| meta.chart_sheets.contains(s)) {
                return Err(AutoError::not_modelled("Sheets.Copy of chart sheets", OWNER));
            }
            if before.is_none() && after.is_none() {
                return self.copy_to_new_book(session, &listed, false, call);
            }
            // Refuse before anything changes.
            for source in &listed {
                Self::check_copyable(session.host().model(), *source)?;
            }
            let mut after = after;
            for source in listed {
                let added = Self::copy_one(session, source, before, after, call)?;
                if after.is_some() {
                    after = Some(added);
                }
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// Copy with no Before and After: a new workbook (`Workbooks.Add`, active) made from this workbook as it
    /// stands, kept to the named sheets, in their tab order. Every part of those sheets comes along (tables,
    /// notes, drawing objects, sheet-local names); references from a kept sheet to a sheet left behind are
    /// #REF! (Excel makes external links: dc-oxh8w.43).
    pub(super) fn copy_to_new_book(&self, session: &mut HostSession, sheets: &[SheetId], remove: bool, call: &Call<'_>) -> Reply {
        let model = session.host().model();
        let names: Vec<String> = sheets
            .iter()
            .filter_map(|s| model.sheet_by_id(*s))
            .map(|s| s.name().text().to_string_lossy())
            .collect();
        if remove && model.definitions().macro_enabled() {
            return Err(AutoError::not_modelled("Move of sheets out of a macro-enabled workbook", "dc-2g5fn"));
        }
        if remove && model.sheets().len() <= sheets.len() {
            // Excel keeps at least one sheet in the workbook the sheets leave.
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        // A workbook with a VBA project cannot be written as .xlsx; its package carries the project, which the
        // new workbook does not run (the application gives it no macro grant).
        let mut bytes = Vec::new();
        let mut written = Err(AutoError::excel(1004, "Copy method of Worksheet class failed"));
        for format in [crate::SpreadsheetFormat::Xlsx, crate::SpreadsheetFormat::Xlsm] {
            let policy = crate::SavePolicy { format: crate::SaveFormat::Spreadsheet(format), ..Default::default() };
            bytes.clear();
            written = crate::prepare_save(session.host(), &policy)
                .and_then(|prepared| prepared.write(&mut bytes))
                .map_err(|e| AutoError::excel(1004, format!("Copy method of Worksheet class failed: {e}")));
            if written.is_ok() {
                break;
            }
        }
        written?;
        let id = self.factory()?.add_package(bytes)?;
        let runner = self
            .open_books()?
            .into_iter()
            .find(|book| book.id == id)
            .and_then(|book| book.runner)
            .ok_or_else(|| AutoError::not_modelled("Sheet copy to a workbook the application does not run", OWNER))?;
        // Any failure from here on leaves no new workbook behind and the active workbook as it was.
        let abandon = |error: AutoError| {
            let _ = runner.close();
            Self::unregister(id);
            error
        };
        runner.keep_sheets(names).map_err(abandon)?;
        if remove {
            let mut progress = None;
            for sheet in sheets {
                match Self::sheet_command(session, Some(*sheet), SheetChange::Remove) {
                    Ok(next) => progress = next.or(progress),
                    Err(error) => return Err(abandon(error)),
                }
            }
            Self::settle(session, progress, call).map_err(abandon)?;
            self.context_changed(session);
        }
        Ok(AutoValue::Empty)
    }

    /// Reduces this workbook to the named sheets and makes the first of them current (see [`BookRunner::keep_sheets`]).
    pub fn keep_sheets(&self, names: &[String]) -> Result<(), AutoError> {
        self.with_write(|session| {
            let sheets: Vec<(SheetId, String)> =
                session.host().model().sheets().iter().map(|s| (s.id(), s.name().text().to_string_lossy())).collect();
            let kept = |name: &str| names.iter().any(|n| calc_value::workbook_names_equal(n, name));
            // The new workbook is made from a macro-enabled package only to carry the sheets; it runs no project, so
            // it is not macro-enabled and its sheets can be removed.
            let identity = session.host().model().definitions().workbook_name().map(str::to_owned);
            if let Some(name) = identity.filter(|_| session.host().model().definitions().macro_enabled()) {
                session.set_file_identity(&name, false);
            }
            let mut progress = None;
            for (sheet, name) in &sheets {
                if !kept(name) {
                    progress = Self::sheet_command(session, Some(*sheet), SheetChange::Remove)?.or(progress);
                }
            }
            if let Some((first, _)) = sheets.iter().find(|(_, name)| kept(name)) {
                let at = CellKey::new(*first, 1, 1).map_err(|_| geometry::type_mismatch())?;
                session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(crate::Selection::cell(at)));
            }
            Self::settle_as(session, progress, "Worksheet.Copy")?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
        .map(|_| ())
    }

    fn copy_anchors(&self, call: &Call<'_>) -> Result<(Option<SheetId>, Option<SheetId>), AutoError> {
        if call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let before = self.local_sheet_argument(call.arg(0), "Worksheet.Copy Before as other than a worksheet")?;
        let after = self.local_sheet_argument(call.arg(1), "Worksheet.Copy After as other than a worksheet")?;
        if before.is_some() && after.is_some() {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        Ok((before, after))
    }

    /// Sheet state the clone cannot carry refuses the copy, typed.
    fn check_copyable(model: &ModelRoot, source: SheetId) -> Result<(), AutoError> {
        let refuse = |what: &str| Err(AutoError::not_modelled(format!("Worksheet.Copy of a sheet with {what}"), OWNER));
        let defs = model.definitions();
        if defs.charts(source).iter().any(|c| !c.part.is_empty()) {
            return refuse("charts read from a file");
        }
        if !defs.shapes(source).is_empty() {
            return refuse("drawing objects");
        }
        if !defs.sparklines(source).is_empty() {
            return refuse("sparklines");
        }
        // Notes follow through the paste planner (copy_metadata::append_notes); hyperlinks do not (dc-oxh8w.43).
        if !defs.review_links(source).is_empty() {
            return refuse("hyperlinks");
        }
        if defs.tables().iter().any(|t| t.sheet == source) || defs.auto_filter(source).is_some_and(|f| f.unsupported) {
            return refuse("tables or an AutoFilter the host does not model");
        }
        if defs.names().iter().any(|n| n.scope == calc_doc_model::NameScope::Sheet(source)) {
            return refuse("sheet-local names");
        }
        Ok(())
    }

    /// Copies one worksheet next to an anchor; returns the new sheet.
    fn copy_one(
        session: &mut HostSession,
        source: SheetId,
        before: Option<SheetId>,
        after: Option<SheetId>,
        call: &Call<'_>,
    ) -> Result<SheetId, AutoError> {
        let model = session.host().model();
        Self::check_copyable(model, source)?;
        let defs = model.definitions();
            let order: Vec<SheetId> = model.sheets().iter().map(|s| s.id()).collect();
            let position_of = |sheet: SheetId| {
                order
                    .iter()
                    .position(|s| *s == sheet)
                    .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))
            };
            let position = match (before, after) {
                (Some(sheet), _) => position_of(sheet)? + 1,
                (_, Some(sheet)) => position_of(sheet)? + 2,
                _ => unreachable!("one of Before and After is given"),
            };
            let source_name = model.sheet_by_id(source).ok_or_else(geometry::type_mismatch)?.name().text().to_string_lossy();
            let name = Self::copy_name(model, &source_name);
            // The used extent: every authored cell and every formatted cell.
            let mut extent: Option<(u32, u32, u32, u32)> = None;
            let occupied = model.sheet_cells(source).into_iter().flatten().map(|(at, _)| at);
            let formatted = defs.cell_formats().map(|(at, _)| at).filter(|at| at.sheet() == source);
            for at in occupied.chain(formatted) {
                let (r, c) = (at.row(), at.column());
                extent = Some(match extent {
                    None => (r, c, r, c),
                    Some((r0, c0, r1, c1)) => (r0.min(r), c0.min(c), r1.max(r), c1.max(c)),
                });
            }
            let (r0, c0, r1, c1) = extent.unwrap_or((1, 1, 1, 1));
            let key = |sheet, row, column| CellKey::new(sheet, row, column).map_err(|_| geometry::type_mismatch());
            let used = GridArea::new(key(source, r0, c0)?, key(source, r1, c1)?).map_err(|_| geometry::type_mismatch())?;
            let snapshot = CopySnapshot::capture(session.host(), used)
                .map_err(|e| AutoError::excel(1004, format!("Copy method of Worksheet class failed: {e}")))?;

            // The sheet-level state is read now, before the new sheet exists.
            let remap = |area: GridArea, sheet: SheetId| -> Result<GridArea, AutoError> {
                GridArea::new(
                    key(sheet, area.first().row(), area.first().column())?,
                    key(sheet, area.last().row(), area.last().column())?,
                )
                .map_err(|_| geometry::type_mismatch())
            };
            let layouts = defs.axis_layouts();
            let axes = [calc_doc_model::Axis::Rows, calc_doc_model::Axis::Columns];
            let axis_formats: Vec<(calc_doc_model::Axis, u32, calc_doc_model::AxisFormat)> = axes
                .into_iter()
                .flat_map(|axis| layouts.bands(source, axis).map(move |(i, f)| (axis, i, f)))
                .collect();
            let summary_before: Vec<calc_doc_model::Axis> =
                axes.into_iter().filter(|axis| !layouts.summary_after(source, *axis)).collect();
            let merges: Vec<GridArea> = defs.merges(source).map(<[_]>::to_vec).unwrap_or_default();
            let validations: Vec<calc_doc_model::Validation> = defs.validations(source).to_vec();
            let rules: Vec<calc_doc_model::ConditionalRuleSource> = defs.conditional_rules(source).to_vec();
            let setup = defs.page_setup(source);
            let filter = defs.auto_filter(source).cloned();
            let panes = defs.frozen_panes(source);
            let layout = defs.pane_layout(source);
            let charts: Vec<calc_doc_model::Chart> = defs.charts(source).to_vec();
            let band_one: Vec<(calc_doc_model::Axis, calc_doc_model::AxisFormat)> =
                summary_before.iter().map(|axis| (*axis, layouts.get(source, *axis, 1))).collect();

            // Add the sheet and place it.
            let mut progress = Self::sheet_command(session, None, SheetChange::Add { name: name.clone() })?;
            let added = session
                .host()
                .model()
                .sheets()
                .iter()
                .map(|s| s.id())
                .find(|id| !order.contains(id))
                .expect("the added sheet");
            if position != order.len() + 1 {
                progress = Self::sheet_command(session, Some(added), SheetChange::Move { position: position as u32 })?.or(progress);
            }
            let rollback = |session: &mut HostSession, why: AutoError| {
                let _ = Self::sheet_command(session, Some(added), SheetChange::Remove);
                why
            };
            // Cells and styles through the paste planner, then the sheet-level edits in the same command.
            let to = GridArea::new(key(added, 1, 1)?, key(added, 1, 1)?).map_err(|_| geometry::type_mismatch())?;
            let mut plan = match prepare_paste_oriented(
                session.host(),
                PasteSource::Snapshot { snapshot: &snapshot, cut: false },
                to,
                PasteMode::All,
                false,
            ) {
                Ok(plan) => plan,
                Err(why) => {
                    return Err(rollback(session, AutoError::excel(1004, format!("Copy method of Worksheet class failed: {why}"))));
                }
            };
            let edits = plan.edits_mut();
            for (axis, index, format) in axis_formats {
                if let Some(edit) = AxisFormatEdit::new(added, axis, index, format) {
                    edits.push(CellEdit::AxisFormat(edit));
                }
            }
            for (axis, format) in band_one {
                if let Some(edit) = AxisFormatEdit::new(added, axis, 1, format) {
                    edits.push(CellEdit::AxisFormat(edit.with_summary_after(false)));
                }
            }
            if !merges.is_empty() {
                let areas = merges.into_iter().map(|a| remap(a, added)).collect::<Result<Vec<_>, _>>()?;
                edits.push(CellEdit::Merges { sheet: added, areas });
            }
            if !validations.is_empty() {
                edits.push(CellEdit::Validations { sheet: added, rules: validations });
            }
            if !rules.is_empty() {
                let rules = rules
                    .into_iter()
                    .map(|mut rule| {
                        rule.areas = rule.areas.iter().map(|a| remap(*a, added)).collect::<Result<_, _>>()?;
                        Ok::<_, AutoError>(rule)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                edits.push(CellEdit::ConditionalRules { sheet: added, rules });
            }
            edits.push(CellEdit::PageSetup { sheet: added, setup });
            if let Some(mut filter) = filter {
                filter.area = remap(filter.area, added)?;
                edits.push(CellEdit::AutoFilter { sheet: added, filter: Some(filter) });
            }
            if panes.is_some() {
                edits.push(CellEdit::FrozenPanes { sheet: added, panes });
            }
            if layout.is_some() {
                edits.push(CellEdit::PaneLayout { sheet: added, layout });
            }
            if !charts.is_empty() {
                // A chart on the sheet reads the clone's cells: series formulas that name the source sheet name the clone.
                let charts = charts.into_iter().map(|c| retarget_chart(c, &source_name, &name)).collect();
                edits.push(CellEdit::Charts { sheet: added, charts });
            }
            let receipt = session.dispatch_from(CommandIngress::Automation, SkinIntent::Command(WorkbookCommand::Paste(plan)));
            if let Some(error) = receipt.error {
                return Err(rollback(session, AutoError::excel(1004, format!("Copy method of Worksheet class failed: {error}"))));
            }
            progress = receipt.recalc.or(progress);
            // The clone becomes the active sheet.
            let first = CellKey::new(added, 1, 1).map_err(|_| geometry::type_mismatch())?;
            session.dispatch_from(CommandIngress::Automation, SkinIntent::Select(crate::Selection::cell(first)));
            Self::settle(session, progress, call)?;
        Ok(added)
    }
}

/// A sheet name as a reference prefix: bare when it is a plain identifier, else quoted with doubled apostrophes.
fn reference_prefix(name: &str) -> String {
    let plain = name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.chars().any(|c| c.is_ascii_digit()) ;
    if plain { format!("{name}!") } else { format!("'{}'!", name.replace('\'', "''")) }
}

/// A chart whose series formulas that begin with the source sheet's name begin with the clone's.
fn retarget_chart(mut chart: calc_doc_model::Chart, from: &str, to: &str) -> calc_doc_model::Chart {
    let (quoted_from, bare_from, to) = (format!("'{}'!", from.replace('\'', "''")), format!("{from}!"), reference_prefix(to));
    let swap = |formula: &mut Option<String>| {
        if let Some(text) = formula.as_mut() {
            let rest = text.strip_prefix(&quoted_from).or_else(|| text.strip_prefix(&bare_from));
            if let Some(rest) = rest {
                *text = format!("{to}{rest}");
            }
        }
    };
    swap(&mut chart.title_formula);
    for plot in &mut chart.plots {
        for series in &mut plot.series {
            for data in [&mut series.name, &mut series.categories, &mut series.values].into_iter().flatten() {
                swap(&mut data.formula);
            }
        }
    }
    chart
}
