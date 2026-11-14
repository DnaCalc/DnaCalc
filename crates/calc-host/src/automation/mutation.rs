//! Writes enter the session once, so transaction, undo, recalculation and events are shared
//! with every other Automation client. Invocation contexts never reach this ingress.
use super::*;
use crate::{CellEdit, CellInput, CommandIngress, HostProgress, SkinIntent};

impl WorkbookAutomation {
    pub(super) fn register_mutation(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Value", InvokeKind::Put, Self::put_value);
        handlers.on("Range", "Value2", InvokeKind::Put, Self::put_value2);
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

    fn with_write(&self, mut call: impl FnMut(&mut HostSession) -> Reply) -> Reply {
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
        self.mutate_range(target, call, |session, range| {
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
        self.mutate_range(target, call, |session, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            self.write_range(session, range, call.args.last())
        })
    }
    fn clear_contents(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate_range(target, call, |session, range| {
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
    fn mutate_range(
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
        let area = range.areas()[0];
        let edits = if value.is_none() || matches!(value, Some(AutoValue::Empty)) {
            vec![CellEdit::Clear { area }]
        } else {
            if area.cell_count() > 1_048_576 {
                return Err(AutoError::not_modelled(
                    "Range.Value write materialization limit",
                    OWNER,
                ));
            }
            let value = value.unwrap();
            let array = match value {
                AutoValue::Array(a) => Some(a),
                _ => None,
            };
            if array.is_some_and(|a| {
                a.rows != area.row_count()
                    || a.columns != area.column_count()
                    || u64::from(a.rows) * u64::from(a.columns) != a.values.len() as u64
            }) {
                return Err(AutoError::not_modelled(
                    "Range.Value array/target shape difference (E11)",
                    "dc-1nbp.6",
                ));
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
            let mut edits = Vec::with_capacity(area.cell_count() as usize);
            for row in 0..area.row_count() {
                for column in 0..area.column_count() {
                    let at = CellKey::new(
                        area.sheet(),
                        area.first().row() + row,
                        area.first().column() + column,
                    )
                    .unwrap();
                    let value = &values[if array.is_some() {
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
            edits
        };
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
        mut intent: impl FnMut(&HostSession, Option<ReferenceLike>) -> Result<SkinIntent, AutoError>,
    ) -> Reply {
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let reference = match &target.key {
                ObjectKey::Range(held)
                | ObjectKey::RangeView {
                    reference: held, ..
                } => Some(self.resolve(&mut Access::Session(session), held)?),
                _ => None,
            };
            let intent = intent(session, reference)?;
            let receipt = session.dispatch_from(CommandIngress::Automation, intent);
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(
                    format!("{}: {error}", call.label()),
                    OWNER,
                ));
            }
            let mut progress = receipt.recalc;
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
