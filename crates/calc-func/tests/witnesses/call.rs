//! Shared native-judge and portable-pin invocation, using the recorded transport when known.
//! This prepares arguments and calls calc_func::invoke; it contains no formula evaluator.
use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
use calc_func::value::{
    CalcArray, CalcValue, CellKey, CoreRef, GridArea, NumericMode, ReferenceLike, SheetId,
};
use calc_func::{
    CellReader, EvalServices, FunctionCallScratch, Notation, ParameterClass, PreparedArguments,
    SubtotalExclusions, Unreadable, catalogue, invoke,
};
use calc_verdict::{WitnessInvocation, decode_argument, encode_result, select_result};
use serde_json::Value;
use std::ops::ControlFlow;

struct Given(Vec<CalcValue>);
impl PreparedArguments for Given {
    fn count(&self) -> usize {
        self.0.len()
    }
    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

// A relocated row preserves the scalar cells' values and reference identity. The capture
// did not record their absolute addresses. Calls needing those addresses are declined.
struct InputCells(Vec<CalcValue>, bool);
impl CellReader for InputCells {
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        if area.sheet() != SheetId::from_parts(1, 0)
            || area.first().row() != 1
            || area.last().row() != 1
            || area.first().column() < 1
            || area.last().column() as usize > self.0.len()
        {
            return Err(Unreadable);
        }
        for column in area.first().column()..=area.last().column() {
            if visit(
                CellKey::new(area.sheet(), 1, column).expect("bounded input cell"),
                &self.0[column as usize - 1],
            )
            .is_break()
            {
                break;
            }
        }
        Ok(())
    }
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        (self.1
            && area.sheet() == SheetId::from_parts(1, 0)
            && area.first().row() == 1
            && area.last().row() == 1
            && area.last().column() as usize <= self.0.len())
        .then(SubtotalExclusions::default)
    }
}

fn environment(context: Option<&WitnessInvocation>) -> Result<CalcEnvironment, &'static str> {
    let mut environment = CalcEnvironment::dates(DateSystem::System1900);
    let Some(context) = context else {
        return Ok(environment);
    };
    environment.compatibility_version = match context.compatibility_version.as_deref() {
        None => 1,
        Some(value) => value
            .parse()
            .map_err(|_| "fault:WitnessCompatibilityVersion")?,
    };
    environment.date_system = match context.date_system.as_deref() {
        None | Some("1900") => DateSystem::System1900,
        Some("1904") => DateSystem::System1904,
        Some(_) => return Err("fault:WitnessDateSystem"),
    };
    if let Some(tag) = &context.locale {
        if let Some(locale) = LocaleId::from_bcp47_language_tag(tag).filter(|id| {
            id.stable_name()
                .eq_ignore_ascii_case(&tag.replace('_', "-"))
        }) {
            let mut profile = format_profile(locale);
            if context.decimal_separator.is_some() || context.thousands_separator.is_some() {
                profile = profile
                    .with_separators(
                        context
                            .decimal_separator
                            .as_deref()
                            .unwrap_or(profile.decimal_separator),
                        context
                            .thousands_separator
                            .as_deref()
                            .unwrap_or(profile.thousands_separator),
                    )
                    .ok_or("fault:WitnessLocale")?;
            }
            environment.profile = Some(profile);
        } else {
            return Err("fault:WitnessLocale");
        }
    }
    // A separator alone does not identify a complete locale profile. In particular the
    // legacy FIXED bank records neither its profile nor its separators; no guess is made.
    Ok(environment)
}

fn dereference(value: CalcValue, inputs: &InputCells) -> Option<CalcValue> {
    let CoreRef::Reference(reference) = value.core_ref() else {
        return Some(value);
    };
    let [area] = reference.areas() else {
        return None;
    };
    if area.sheet() != SheetId::from_parts(1, 0)
        || area.first().row() != 1
        || area.last().row() != 1
        || area.first().column() < 1
        || area.last().column() as usize > inputs.0.len()
    {
        return None;
    }
    let values =
        inputs.0[area.first().column() as usize - 1..area.last().column() as usize].to_vec();
    if values.len() == 1 {
        return values.into_iter().next();
    }
    CalcArray::from_rows(vec![values]).map(CalcValue::array)
}

pub fn calculate(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&WitnessInvocation>,
    apply_context: bool,
) -> String {
    let Ok(target) = catalogue().resolve_surface(Notation::Call, function) else {
        // A bare call of an add-in's export (EUROCONVERT) is #NAME? while the add-in is not loaded.
        if catalogue().is_unloaded_addin_export(function) {
            return "error:Name".to_owned();
        }
        return "fault:NotCatalogued".to_owned();
    };
    let Ok(mut values) = args
        .iter()
        .enumerate()
        .map(|(i, a)| decode_argument(a, &format!("args[{i}]")))
        .collect::<Result<Vec<_>, _>>()
    else {
        return "fault:Unreadable".to_owned();
    };
    let context = if apply_context { context } else { None };
    let environment = match environment(context) {
        Ok(env) => env,
        Err(fault) => return fault.to_owned(),
    };
    // Bulk's matrix delegate used a different layout. A global scalar transport label is
    // insufficient to reconstruct it: arrays keep their explicit value form.
    let cell_transport = context
        .is_some_and(|c| c.input_transport.as_deref() == Some("cell_value2_bulk"))
        && values
            .iter()
            .all(|value| !matches!(value.core_ref(), CoreRef::Array(_)));
    let value2_transport = context.is_some_and(|c| {
        matches!(
            c.input_transport.as_deref(),
            Some("cell_value2_bulk" | "cell_value2")
        )
    }) && values
        .iter()
        .all(|value| !matches!(value.core_ref(), CoreRef::Array(_)));
    if value2_transport {
        // Excel's Value2 cell write stores signed subnormals and negative zero as +0.
        // Q1-subnormal-inputs.json, dc-1943: 179 affected bank calls replayed.
        for value in &mut values {
            if let Some(number) = value.as_number()
                && (number.is_subnormal() || number == 0.0)
            {
                *value = CalcValue::number(0.0);
            }
        }
    }
    let mut prepared = values.clone();
    let mut has_reference = false;
    if cell_transport {
        for (position, argument) in prepared.iter_mut().enumerate() {
            if matches!(
                target.parameter_class(position),
                Some(ParameterClass::Reference | ParameterClass::ValueOrReference)
            ) {
                has_reference = true;
                *argument = CalcValue::reference(ReferenceLike::area(GridArea::cell(
                    CellKey::new(SheetId::from_parts(1, 0), 1, position as u32 + 1)
                        .expect("argument fits the grid"),
                )));
            }
        }
    }
    if has_reference && matches!(target.spelling(), "ROW" | "COLUMN" | "CELL" | "OFFSET") {
        return "fault:WitnessReferencePosition".to_owned();
    }
    let mode = NumericMode::Fast;
    let visible_constants =
        context.is_some_and(|c| c.input_sheet_state.as_deref() == Some("visible-constant-inputs"));
    let mut cells = InputCells(values, visible_constants);
    let result = if has_reference {
        invoke(
            target,
            &mut Given(prepared),
            &mut EvalServices::with_cells(&mode, &mut cells).with_environment(&environment),
            &mut FunctionCallScratch::new(),
        )
    } else {
        invoke(
            target,
            &mut Given(prepared),
            &mut EvalServices::new(&mode).with_environment(&environment),
            &mut FunctionCallScratch::new(),
        )
    };
    match result {
        Err(fault) => {
            let text = format!("{fault:?}");
            let end = text.find([' ', '{', '(']).unwrap_or(text.len());
            format!("fault:{}", &text[..end])
        }
        Ok(value) => dereference(value, &cells)
            .and_then(|v| select_result(&v, index))
            .and_then(|v| encode_result(&v))
            .unwrap_or_else(|| "fault:ResultShape".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cells() -> WitnessInvocation {
        WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            ..Default::default()
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn captured_cells_keep_identity_and_held_text() {
        let context = cells();
        // Retained w111 ISREF and fn1 MATCH readings, through the existing call route.
        assert_eq!(
            calculate(
                "ISREF",
                &[json!("0x0000000000000000")],
                None,
                Some(&context),
                true
            ),
            "logical:True"
        );
        assert_eq!(
            calculate(
                "MATCH",
                &[json!("é*"), json!("Élan"), json!("0x0000000000000000")],
                None,
                Some(&context),
                true
            ),
            "0x3ff0000000000000"
        );
        assert_eq!(
            calculate(
                "ISREF",
                &[json!("0x0000000000000000")],
                None,
                Some(&context),
                false
            ),
            "logical:False"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn value2_write_flushes_subnormals_but_literals_remain_unchanged() {
        let context = cells();
        for bits in ["0x0000000000000001", "0x8000000000000001"] {
            assert_eq!(
                calculate("SIGN", &[json!(bits)], None, Some(&context), true),
                "0x0000000000000000"
            );
        }
        let negative = [json!("0x8000000000000001")];
        assert_ne!(
            calculate("SQRT", &negative, None, None, true),
            "0x0000000000000000"
        );
        assert_eq!(
            calculate("SQRT", &negative, None, Some(&context), false),
            calculate("SQRT", &negative, None, None, true)
        );
        assert_eq!(
            calculate(
                "SIGN",
                &[json!("0x0010000000000000")],
                None,
                Some(&context),
                true
            ),
            "0x3ff0000000000000"
        );
        let scalar = WitnessInvocation {
            input_transport: Some("cell_value2".into()),
            ..Default::default()
        };
        assert_eq!(
            calculate(
                "SIGN",
                &[json!("0x8000000000000001")],
                None,
                Some(&scalar),
                true
            ),
            "0x0000000000000000"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn unknown_positions_and_matrix_transport_do_not_gain_invented_context() {
        let context = cells();
        for (function, args) in [
            ("ROW", vec![json!("0x0000000000000000")]),
            ("COLUMN", vec![json!("0x0000000000000000")]),
            (
                "OFFSET",
                vec![
                    json!("0x0000000000000000"),
                    json!("0xbff0000000000000"),
                    json!("0x0000000000000000"),
                ],
            ),
            ("CELL", vec![json!("address"), json!("0x0000000000000000")]),
        ] {
            assert_eq!(
                calculate(function, &args, None, Some(&context), true),
                "fault:WitnessReferencePosition",
                "{function}"
            );
        }
        let matrix = [json!([["0x0000000000000000", "0x3ff0000000000000"]])];
        assert_eq!(
            calculate("ISREF", &matrix, None, Some(&context), true),
            calculate("ISREF", &matrix, None, None, true)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn separators_do_not_establish_a_locale_and_explicit_profiles_are_applied() {
        let context = WitnessInvocation {
            decimal_separator: Some(",".into()),
            thousands_separator: Some(".".into()),
            ..Default::default()
        };
        assert!(environment(Some(&context)).unwrap().profile.is_none());
        let known = WitnessInvocation {
            locale: Some("en-ZA".into()),
            ..context
        };
        let env = environment(Some(&known)).unwrap();
        let profile = env.profile.unwrap();
        assert_eq!(profile.decimal_separator, ",");
        assert_eq!(profile.format_code_group_token, ".");
        assert_eq!(
            environment(Some(&WitnessInvocation {
                date_system: Some("1904".into()),
                ..Default::default()
            }))
            .unwrap()
            .date_system,
            DateSystem::System1904
        );
    }
}
