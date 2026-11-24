//! Writes enter the session once, so transaction, undo, recalculation and events are shared
//! with every other Automation client. Invocation contexts never reach this ingress.
use super::*;
use crate::{
    CellEdit, CellInput, CommandIngress, HostProgress, SkinIntent, TableChange, WorkbookCommand,
};
use std::collections::HashSet;

/// What a table-aware pre-step leaves of a `Range.Value` write.
enum PreWrite {
    /// Write the range as given.
    Same,
    /// Write this range instead (the part the table's own commands did not take).
    Replace(ReferenceLike),
    /// Nothing is left to write.
    Done,
}

impl WorkbookAutomation {
    pub(super) fn register_mutation(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Value", InvokeKind::Put, Self::put_value);
        handlers.on("Range", "Value2", InvokeKind::Put, Self::put_value2);
        handlers.on("Range", "_Default", InvokeKind::Put, Self::put_default);
        handlers.on("Range", "Item", InvokeKind::Put, Self::put_item);
        handlers.on(
            "Range",
            "ClearContents",
            InvokeKind::Method,
            Self::clear_contents,
        );
        handlers.on(
            "Range",
            "Calculate",
            InvokeKind::Method,
            Self::calculate_range,
        );
        handlers.on(
            "Worksheet",
            "Calculate",
            InvokeKind::Method,
            Self::calculate_sheet,
        );
        handlers.on(
            "Application",
            "Calculate",
            InvokeKind::Method,
            Self::calculate,
        );
        handlers.on(
            "Application",
            "CalculateFull",
            InvokeKind::Method,
            Self::calculate_full,
        );
        handlers.on(
            "Application",
            "CalculateFullRebuild",
            InvokeKind::Method,
            Self::calculate_rebuild,
        );
    }

    pub(super) fn with_write(&self, mut call: impl FnMut(&mut HostSession) -> Reply) -> Reply {
        if let Some(reply) = self.host.with_invocation(&mut |_| {
            Err(AutoError::excel(
                1004,
                "Application-defined or object-defined error",
            ))
        }) {
            return reply;
        }
        if EVENT_SESSION.is_set() {
            if let Some(reply) = EVENT_SESSION.with(|(model, loan)| {
                if !std::ptr::eq(model, self) {
                    return None;
                }
                Some(loan.write(&mut |session| {
                    self.context_changed(session);
                    call(session)
                }))
            }) {
                return reply;
            }
        }
        self.host.with_session_mut(&mut |session| {
            self.context_changed(session);
            call(session)
        })
    }

    fn put_value(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate_values(target, call, |session, range| {
            if call.args.is_empty() || call.args.len() > 2 {
                return Err(geometry::bad_count());
            }
            if call.args.len() == 2 && !matches!(call.arg(0), AutoValue::Missing | AutoValue::Empty)
            {
                return Err(AutoError::not_modelled(
                    "Range.Value RangeValueDataType put",
                    "dc-1nbp.6",
                ));
            }
            self.write_range(session, range, call.args.last())
        })
    }
    fn put_value2(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate_values(target, call, |session, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            self.write_range(session, range, call.args.last())
        })
    }
    fn put_default(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_indexed(target, call, true)
    }
    fn put_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_indexed(target, call, false)
    }
    fn put_indexed(&self, target: &ObjectRef, call: &Call<'_>, allow_bare: bool) -> Reply {
        let axis = match &target.key {
            ObjectKey::RangeView { axis, .. } => *axis,
            _ => RangeAxis::Cells,
        };
        let resolve_target = |range: &ReferenceLike| -> Result<ReferenceLike, AutoError> {
            match call.args.as_slice() {
                [_] if allow_bare => Ok(range.clone()),
                [row, _] if allow_bare && row.is_missing() => Ok(range.clone()),
                [row, _] if !row.is_missing() => {
                    geometry::item(range, axis, row, &AutoValue::Missing)
                }
                [row, col, _] if allow_bare && row.is_missing() && col.is_missing() => {
                    Ok(range.clone())
                }
                [row, col, _] if !row.is_missing() => geometry::item(range, axis, row, col),
                _ => Err(geometry::bad_count()),
            }
        };
        self.mutate_pre(
            target,
            call,
            |session, reference| {
                let reference = reference.ok_or_else(geometry::type_mismatch)?;
                let indexed = resolve_target(reference)?;
                match Self::table_pre_step(session, &indexed, call.args.last())? {
                    PreWrite::Same => Ok(PreWrite::Replace(indexed)),
                    other => Ok(other),
                }
            },
            |session, reference| {
                self.write_range(
                    session,
                    reference.ok_or_else(geometry::type_mismatch)?,
                    call.args.last(),
                )
            },
        )
    }
    /// A value write: a table the target touches is first brought to what Excel does (dc-piw0.10 Q13):
    /// a write over a whole table range deletes the table, a write over header cells renames the columns
    /// (made unique) and leaves the cells below to the ordinary write.
    fn mutate_values(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        mut intent: impl FnMut(&HostSession, ReferenceLike) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.mutate_pre(
            target,
            call,
            |session, reference| match reference {
                Some(reference) => Self::table_pre_step(session, reference, call.args.last()),
                None => Ok(PreWrite::Same),
            },
            |session, reference| intent(session, reference.ok_or_else(geometry::type_mismatch)?),
        )
    }
    fn dispatch_table(session: &mut HostSession, change: TableChange) -> Result<(), AutoError> {
        let revision = session.host().revision().get();
        let receipt = session.dispatch_from(
            CommandIngress::Automation,
            SkinIntent::Command(WorkbookCommand::Table { revision, change }),
        );
        match receipt.error {
            Some(error) => Err(AutoError::not_modelled(
                format!("Range.Value over a table: {error:?}"),
                OWNER,
            )),
            None => Ok(()),
        }
    }
    fn table_pre_step(
        session: &mut HostSession,
        reference: &ReferenceLike,
        value: Option<&AutoValue>,
    ) -> Result<PreWrite, AutoError> {
        if value.is_none_or(|v| matches!(v, AutoValue::Empty)) || reference.area_count() != 1 {
            return Ok(PreWrite::Same);
        }
        let area = reference.areas()[0];
        let Some(table) = session
            .host()
            .model()
            .definitions()
            .tables()
            .iter()
            .find(|t| t.sheet == area.sheet() && t.area.intersection(area).is_some())
            .cloned()
        else {
            return Ok(PreWrite::Same);
        };
        if table.area == area {
            Self::dispatch_table(session, TableChange::Unlist { part: table.part })?;
            return Ok(PreWrite::Same);
        }
        let header = table.area.first().row();
        if table.header_rows == 0 || area.first().row() > header || area.last().row() < header {
            return Ok(PreWrite::Same);
        }
        let value = value.unwrap();
        let array = match value {
            AutoValue::Array(a) => Some(a),
            _ => None,
        };
        let header_only = area.first().row() == area.last().row();
        if array.is_some() && !header_only {
            return Err(AutoError::not_modelled(
                "Range.Value array over a table header and body",
                OWNER,
            ));
        }
        let from = area.first().column().max(table.area.first().column());
        let to = area.last().column().min(table.area.last().column());
        let mut columns = table.columns.clone();
        for column in from..=to {
            let at = (column - table.area.first().column()) as usize;
            let element = match array {
                Some(a) => a
                    .values
                    .get((column - area.first().column()) as usize)
                    .ok_or_else(geometry::type_mismatch)?,
                None => value,
            };
            let mut name = match element {
                AutoValue::Text(_) => element.as_text().ok_or_else(geometry::type_mismatch)?,
                AutoValue::R8(v) => v.to_string(),
                AutoValue::I4(v) => v.to_string(),
                AutoValue::I2(v) => v.to_string(),
                AutoValue::Bool(v) => if *v { "TRUE" } else { "FALSE" }.into(),
                _ => {
                    return Err(AutoError::not_modelled(
                        "Range.Value into a table header of this type",
                        OWNER,
                    ));
                }
            };
            let others: Vec<String> = columns
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != at)
                .map(|(_, c)| c.clone())
                .collect();
            if name.is_empty() {
                name = calc_doc_model::free_column_name(&others);
            } else if others
                .iter()
                .any(|c| calc_value::workbook_names_equal(c, &name))
            {
                name = calc_doc_model::numbered_duplicate(&name, &others);
            }
            if name != columns[at] {
                Self::dispatch_table(
                    session,
                    TableChange::RenameColumn {
                        part: table.part.clone(),
                        column,
                        name: name.clone(),
                    },
                )?;
                columns[at] = name;
            }
        }
        if area.last().row() == header {
            return Ok(PreWrite::Done);
        }
        let rest = GridArea::new(
            CellKey::new(area.sheet(), header + 1, area.first().column())
                .map_err(|_| geometry::type_mismatch())?,
            area.last(),
        )
        .map_err(|_| geometry::type_mismatch())?;
        Ok(PreWrite::Replace(
            ReferenceLike::from_areas(vec![rest]).map_err(|_| geometry::type_mismatch())?,
        ))
    }
    fn clear_contents(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate_areas(target, call, |session, range| {
            if call.args.len() > 1 {
                return Err(geometry::bad_count());
            }
            if !call.arg(0).is_missing() {
                return Err(AutoError::not_modelled(
                    "Range.ClearContents RemoveControls",
                    "dc-1nbp.6",
                ));
            }
            self.write_range(session, range, None)
        })
    }
    /// Clear every area of the range in one committed edit.
    pub(super) fn mutate_areas(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        mut intent: impl FnMut(&HostSession, ReferenceLike) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.mutate(target, call, |session, reference| {
            intent(session, reference.ok_or_else(geometry::type_mismatch)?)
        })
    }
    pub(super) fn mutate_range(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        mut intent: impl FnMut(&HostSession, ReferenceLike) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.mutate(target, call, |session, reference| {
            let range = reference.ok_or_else(geometry::type_mismatch)?;
            if range.area_count() != 1 {
                return Err(AutoError::not_modelled(
                    format!("{} multi-area write", call.label()),
                    "dc-1nbp.6",
                ));
            }
            intent(session, range)
        })
    }
    fn write_range(
        &self,
        session: &HostSession,
        reference: ReferenceLike,
        value: Option<&AutoValue>,
    ) -> Result<SkinIntent, AutoError> {
        let range = &reference;
        let areas = range.areas();
        if value.is_none() || matches!(value, Some(AutoValue::Empty)) {
            return Ok(SkinIntent::Edit {
                edits: areas.iter().map(|&area| CellEdit::Clear { area }).collect(),
            });
        }
        let value = value.unwrap();
        let array = match value {
            AutoValue::Array(a) => Some(a),
            _ => None,
        };
        if areas.iter().map(|area| u64::from(area.cell_count())).sum::<u64>() > 1_048_576 {
            return Err(AutoError::not_modelled(
                "Range.Value write materialization limit",
                OWNER,
            ));
        }
        let multi_area = areas.len() > 1;
        let broadcast = multi_area && array.is_some_and(|a| a.rows == 1 && a.columns == 1);
        if let Some(a) = array {
            // E11 broadcasts 1×1 or repeats a conforming array across equal-sized areas.
            let shape = if broadcast {
                (areas[0].row_count(), areas[0].column_count())
            } else {
                (a.rows, a.columns)
            };
            if u64::from(a.rows) * u64::from(a.columns) != a.values.len() as u64
                || areas
                    .iter()
                    .any(|area| (area.row_count(), area.column_count()) != shape)
            {
                return Err(AutoError::not_modelled(
                    "Range.Value array/target shape difference (E11)",
                    "dc-1nbp.6",
                ));
            }
            if multi_area
                && session.host().model().definitions().tables().iter().any(|table| {
                    areas
                        .iter()
                        .any(|area| table.area.intersection(*area).is_some())
                })
            {
                return Err(AutoError::not_modelled(
                    "Range.Value array into multiple areas touching a table (E11)",
                    "dc-1nbp.6",
                ));
            }
        }
        // Convert every element before the first mutation: a refused element
        // cannot leave a partially written rectangle or emit an event.
        let environment = session.host().entry_environment();
        let values = if let Some(a) = array {
            a.values
                .iter()
                .map(|value| input(value, &environment))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![input(value, &environment)?]
        };
        let mut edits = Vec::new();
        let mut array_targets = (multi_area && array.is_some() && !broadcast).then(HashSet::new);
        for area in areas {
            for row in 0..area.row_count() {
                for column in 0..area.column_count() {
                    let at = CellKey::new(
                        area.sheet(),
                        area.first().row() + row,
                        area.first().column() + column,
                    )
                    .unwrap();
                    if array_targets
                        .as_mut()
                        .is_some_and(|targets| !targets.insert(at))
                    {
                        return Err(AutoError::not_modelled(
                            "Range.Value array into overlapping areas (E11)",
                            "dc-1nbp.6",
                        ));
                    }
                    let value = &values[if array.is_some() && !broadcast {
                        (row * area.column_count() + column) as usize
                    } else {
                        0
                    }];
                    edits.push(match value {
                        Some(input) => CellEdit::Set {
                            at,
                            input: input.clone(),
                        },
                        None => CellEdit::Clear {
                            area: GridArea::cell(at),
                        },
                    });
                }
            }
        }
        Ok(SkinIntent::Edit { edits })
    }
    fn calculate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, _| {
            no_args(call)?;
            Ok(SkinIntent::Recalculate)
        })
    }
    fn calculate_full(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, _| {
            no_args(call)?;
            Ok(SkinIntent::Calculate {
                scope: crate::CalcScope::Workbook,
                selection: crate::EvalSelection::ForceAll,
            })
        })
    }
    fn calculate_rebuild(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, _| {
            no_args(call)?;
            Ok(SkinIntent::Rebuild)
        })
    }
    fn calculate_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, _| {
            no_args(call)?;
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            Ok(SkinIntent::Calculate {
                scope: crate::CalcScope::Sheet(sheet),
                selection: crate::EvalSelection::ForceAll,
            })
        })
    }
    fn calculate_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate(target, call, |_, range| {
            no_args(call)?;
            Ok(SkinIntent::CalculateRange {
                areas: range.ok_or_else(geometry::type_mismatch)?.areas().to_vec(),
            })
        })
    }
    fn mutate(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        intent: impl FnMut(&HostSession, Option<ReferenceLike>) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.mutate_pre(target, call, |_, _| Ok(PreWrite::Same), intent)
    }
    /// Runs a started calculation to its end, as Excel's synchronous automation call does.
    pub(super) fn settle(
        session: &mut HostSession,
        mut progress: Option<HostProgress>,
        call: &Call<'_>,
    ) -> Result<(), AutoError> {
        loop {
            match progress {
                Some(HostProgress::Failed(ref e)) => {
                    return Err(AutoError::not_modelled(
                        format!("{} calculation failed: {e}", call.label()),
                        OWNER,
                    ));
                }
                Some(HostProgress::Waiting { .. } | HostProgress::Cancelled) => {
                    return Err(AutoError::not_modelled(
                        format!("{} synchronous calculation is unfinished", call.label()),
                        OWNER,
                    ));
                }
                _ => {}
            }
            if !session.has_pending_calculation() {
                break;
            }
            progress = Some(session.advance_calculation());
        }
        if session.has_unfinished_calculation() {
            return Err(AutoError::not_modelled(
                format!("{} synchronous calculation is unfinished", call.label()),
                OWNER,
            ));
        }
        Ok(())
    }
    fn mutate_pre(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        mut pre: impl FnMut(&mut HostSession, Option<&ReferenceLike>) -> Result<PreWrite, AutoError>,
        mut intent: impl FnMut(&HostSession, Option<ReferenceLike>) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let reference = match &target.key {
                ObjectKey::Range(held)
                | ObjectKey::RangeView {
                    reference: held, ..
                } => Some(self.resolve(&mut Access::Session(session, false), held)?),
                _ => None,
            };
            let (reference, done) = match pre(session, reference.as_ref())? {
                PreWrite::Same => (reference, false),
                PreWrite::Replace(replacement) => (Some(replacement), false),
                PreWrite::Done => (reference, true),
            };
            let mut progress = None;
            if !done {
                let intent = intent(session, reference)?;
                let receipt = session.dispatch_from(CommandIngress::Automation, intent);
                if let Some(error) = receipt.error {
                    return Err(AutoError::not_modelled(
                        format!("{}: {error}", call.label()),
                        OWNER,
                    ));
                }
                progress = receipt.recalc;
            }
            Self::settle(session, progress, call)?;
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }
}

fn input(
    value: &AutoValue,
    environment: &calc_func::locale::CalcEnvironment,
) -> Result<Option<CellInput>, AutoError> {
    let value = worksheet_function::to_calc(value)?;
    Ok(match value.core_ref() {
        CoreRef::Empty => None,
        CoreRef::Number(v) => Some(CellInput::Number(
            calc_doc_model::FiniteNumber::new(v).ok_or_else(geometry::type_mismatch)?,
        )),
        CoreRef::Text(v) => {
            if v.utf16_code_units().len() > 32_767 {
                return Err(AutoError::not_modelled(
                    "Range.Value2 text exceeds the cell text limit",
                    OWNER,
                ));
            }
            // Value assignment's entry/format coercions have not been measured by E11.
            // Use the existing entry parser only to detect that ambiguity, never to invent
            // Automation's conversion. Admit a string only when it remains identical text.
            let text = String::from_utf16(v.utf16_code_units()).map_err(|_| {
                AutoError::not_modelled("Range.Value text assignment conversion (E11)", "dc-1nbp.6")
            })?;
            match crate::interpret_entry_in(&text, Some(environment)) {
                Ok(Some(CellInput::Text(input))) if input.text() == v => {
                    Some(CellInput::Text(input))
                }
                _ => {
                    return Err(AutoError::not_modelled(
                        "Range.Value text assignment conversion (E11)",
                        "dc-1nbp.6",
                    ));
                }
            }
        }
        CoreRef::Logical(v) => Some(CellInput::Logical(v)),
        CoreRef::Error(v) => Some(CellInput::Error(v)),
        _ => return Err(geometry::type_mismatch()),
    })
}
