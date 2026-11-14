//! Writes enter the session once, so transaction, undo, recalculation and events are shared
//! with every other Automation client. Invocation contexts never reach this ingress.
use super::*;
use crate::{CellEdit, CellInput, CommandIngress, HostProgress, SkinIntent};

impl WorkbookAutomation {
    pub(super) fn register_mutation(handlers: &mut Handlers<Self>) {
        for name in ["Value", "Value2"] {
            handlers.on("Range", name, InvokeKind::Put, Self::mutate);
        }
        for (interface, name) in [
            ("Range", "ClearContents"),
            ("Range", "Calculate"),
            ("Worksheet", "Calculate"),
            ("Application", "Calculate"),
            ("Application", "CalculateFull"),
            ("Application", "CalculateFullRebuild"),
        ] {
            handlers.on(interface, name, InvokeKind::Method, Self::mutate);
        }
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

    pub(super) fn mutate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let name = call.member.name.as_str();
            let reference = match &target.key {
                ObjectKey::Range(held)
                | ObjectKey::RangeView {
                    reference: held, ..
                } => Some(self.resolve(&mut Access::Session(session), held)?),
                _ => None,
            };
            let intent = match name {
                "Value" | "Value2" | "ClearContents" => {
                    let range = reference.as_ref().ok_or_else(geometry::type_mismatch)?;
                    if range.area_count() != 1 {
                        return Err(AutoError::not_modelled(
                            format!("Range.{name} multi-area write"),
                            "dc-1nbp.6",
                        ));
                    }
                    let area = range.areas()[0];
                    let value = if name == "ClearContents" {
                        if call.args.len() > 1 {
                            return Err(geometry::bad_count());
                        }
                        if !call.arg(0).is_missing() {
                            return Err(AutoError::not_modelled(
                                "Range.ClearContents RemoveControls",
                                "dc-1nbp.6",
                            ));
                        }
                        None
                    } else {
                        let maximum = if name == "Value" { 2 } else { 1 };
                        if call.args.is_empty() || call.args.len() > maximum {
                            return Err(geometry::bad_count());
                        }
                        if call.args.len() == 2
                            && !matches!(call.arg(0), AutoValue::Missing | AutoValue::Empty)
                        {
                            return Err(AutoError::not_modelled(
                                "Range.Value RangeValueDataType put",
                                "dc-1nbp.6",
                            ));
                        }
                        Some(call.args.last().unwrap())
                    };
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
                    SkinIntent::Edit { edits }
                }
                _ => {
                    no_args(call)?;
                    match (&target.key, name) {
                        (ObjectKey::Application, "Calculate") => SkinIntent::Recalculate,
                        (ObjectKey::Application, "CalculateFull") => SkinIntent::Calculate {
                            scope: crate::CalcScope::Workbook,
                            selection: crate::EvalSelection::ForceAll,
                        },
                        (ObjectKey::Application, "CalculateFullRebuild") => SkinIntent::Rebuild,
                        (ObjectKey::Worksheet { sheet, .. }, "Calculate") => {
                            SkinIntent::Calculate {
                                scope: crate::CalcScope::Sheet(*sheet),
                                selection: crate::EvalSelection::ForceAll,
                            }
                        }
                        (_, "Calculate") if reference.is_some() => SkinIntent::CalculateRange {
                            areas: reference.unwrap().areas().to_vec(),
                        },
                        _ => return Err(AutoError::not_modelled(call.label(), OWNER)),
                    }
                }
            };
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
