//! Shared native-judge and portable-pin invocation, using the recorded transport when known.
//! This prepares arguments and calls calc_func::invoke; it contains no formula evaluator.
use calc_func::locale::{CalcEnvironment, DateSystem, LocaleId, format_profile};
use calc_func::value::{
    CalcArray, CalcValue, CellKey, CoreRef, GridArea, NumericMode, ReferenceLike, SheetId,
};
use calc_func::{
    CellReader, EvalServices, FunctionCallScratch, FunctionFault, Notation, ParameterClass, PreparedArguments,
    ReferenceText, SubtotalExclusions, Unreadable, catalogue, invoke,
};
use calc_verdict::{WitnessInvocation, decode_argument, encode_result, select_result};
use serde_json::Value;
use std::collections::BTreeMap;
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

const SHEET_S: SheetId = SheetId::from_parts(1, 0);
const SHEET_Q: SheetId = SheetId::from_parts(3, 0);

// A relocated row preserves the scalar cells' values and reference identity. The capture
// did not record their absolute addresses. Calls needing those addresses are declined, except
// that an attested bulk layout fixes the sheet and column (not the row) for the functions
// that read only those (the third field). Explicit rRxC ranges live on sheet Q (sheet 3),
// matching Touchstone's EvaluateSpecial layout; unwritten cells on Q are blank worksheet cells
// when `self.4` is true (e.g. SUMIF/AVERAGEIF extending a smaller sum_range/average_range).
struct InputCells(BTreeMap<(u32, u32, u32), CalcValue>, bool, bool, bool, bool);
impl CellReader for InputCells {
    fn reference_positions_known(&self) -> bool {
        self.2
    }

    fn reference_columns_known(&self) -> bool {
        self.3
    }

    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable> {
        let sheet_num = if area.sheet() == SHEET_S {
            1u32
        } else if area.sheet() == SHEET_Q && self.4 {
            3u32
        } else {
            return Err(Unreadable);
        };
        let empty = CalcValue::empty();
        for row in area.first().row()..=area.last().row() {
            for column in area.first().column()..=area.last().column() {
                let value = match self.0.get(&(sheet_num, row, column)) {
                    Some(v) => v,
                    None if sheet_num == 3 => &empty,
                    None => return Err(Unreadable),
                };
                if visit(
                    CellKey::new(area.sheet(), row, column).expect("bounded input cell"),
                    value,
                )
                .is_break()
                {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        if !self.1 {
            return None;
        }
        if area.sheet() == SHEET_S {
            (area.first().row()..=area.last().row())
                .all(|r| {
                    (area.first().column()..=area.last().column())
                        .all(|c| self.0.contains_key(&(1, r, c)))
                })
                .then(SubtotalExclusions::default)
        } else if area.sheet() == SHEET_Q && self.4 {
            Some(SubtotalExclusions::default())
        } else {
            None
        }
    }
}

/// The sheet a Touchstone/1 bulk batch writes (dc-ymjf.2.84/.85, attested layout
/// `touchstone/1-bulk`): the scratch book's sheets are S, A and Q, in that order; call i's
/// arguments are cells of row i of S from column B (`RC[k+1]` from the formula in column A), an
/// error argument entered as its error formula (`=#N/A`) and every other argument as a constant.
/// The row depends on the batch, so a fact needing it is refused.
struct BulkSheet {
    /// The error each argument column's cell holds as a formula.
    errors: BTreeMap<u32, calc_func::value::WorksheetErrorCode>,
}

const BULK_SHEETS: [&str; 3] = ["S", "A", "Q"];

impl BulkSheet {
    fn sheet(index: u32) -> SheetId {
        SheetId::from_parts(index, 0)
    }
    fn argument_cell(&self, cell: CellKey) -> bool {
        cell.sheet() == Self::sheet(1) && cell.row() == 1 && cell.column() >= 2
    }
}

impl calc_func::HostFacts for BulkSheet {
    fn pivot_data_cell(
        &mut self,
        _data_field: &str,
        _pivot_table: &[GridArea],
        _fields: &[(String, CalcValue)],
    ) -> calc_func::PivotLookup {
        calc_func::PivotLookup::NotModelled
    }
    fn is_external_sheet(&mut self, _sheet: SheetId) -> bool {
        false
    }
    fn caller(&mut self) -> Option<CellKey> {
        None
    }
    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        self.argument_cell(cell).then(|| self.errors.contains_key(&cell.column()))
    }
    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        self.argument_cell(cell).then(|| {
            self.errors
                .get(&cell.column())
                .map(|code| format!("={}", code.excel_spelling()))
        })
    }
    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        (1..=3).find(|&i| Self::sheet(i) == sheet)
    }
    fn sheet_named(&mut self, name: &str) -> Option<SheetId> {
        BULK_SHEETS
            .iter()
            .position(|s| s.eq_ignore_ascii_case(name))
            .map(|i| Self::sheet(i as u32 + 1))
    }
    fn sheet_name(&mut self, sheet: SheetId) -> Option<String> {
        self.sheet_index(sheet).map(|i| BULK_SHEETS[i as usize - 1].to_owned())
    }
    fn sheet_count(&mut self) -> Option<u32> {
        Some(3)
    }
    fn workbook_facts(&mut self) -> Option<std::sync::Arc<dyn calc_func::WorkbookFacts>> {
        None
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
    environment.now = context.clock_serial()?;
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
    let sheet_num = if area.sheet() == SHEET_S {
        1u32
    } else if area.sheet() == SHEET_Q && inputs.4 {
        3u32
    } else {
        return None;
    };
    let mut rows = Vec::new();
    for r in area.first().row()..=area.last().row() {
        let mut row = Vec::new();
        for c in area.first().column()..=area.last().column() {
            let cell = match inputs.0.get(&(sheet_num, r, c)) {
                Some(v) => v.clone(),
                None if sheet_num == 3 => CalcValue::empty(),
                None => return None,
            };
            row.push(cell);
        }
        rows.push(row);
    }
    if rows.len() == 1 && rows[0].len() == 1 {
        return rows.pop()?.pop();
    }
    CalcArray::from_rows(rows).map(CalcValue::array)
}

fn flush_value2(value: &mut CalcValue) {
    match value.core_ref() {
        CoreRef::Number(number)
            if number.is_subnormal() || (number == 0.0 && number.is_sign_negative()) =>
        {
            *value = CalcValue::number(0.0);
        }
        CoreRef::Array(array) => {
            let shape = array.shape();
            let mut changed = false;
            let mut rows = Vec::with_capacity(shape.rows);
            for r in 0..shape.rows {
                let mut row = Vec::with_capacity(shape.cols);
                for c in 0..shape.cols {
                    let mut cell = array.get(r, c).unwrap().clone();
                    if let Some(number) = cell.as_number()
                        && (number.is_subnormal()
                            || (number == 0.0 && number.is_sign_negative()))
                    {
                        cell = CalcValue::number(0.0);
                        changed = true;
                    }
                    row.push(cell);
                }
                rows.push(row);
            }
            if changed {
                *value = CalcValue::array(CalcArray::from_rows(rows).expect("same shape"));
            }
        }
        _ => {}
    }
}

pub fn calculate(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&WitnessInvocation>,
    apply_context: bool,
) -> String {
    calculate_with_reference_text(function, args, index, context, apply_context, None)
}

/// A caller that owns the reference-text binder may supply it without making CalcFunc depend on
/// that higher layer. The portable witness test continues to call the same evaluator without one.
pub fn calculate_with_reference_text(
    function: &str,
    args: &[Value],
    index: Option<(usize, usize)>,
    context: Option<&WitnessInvocation>,
    apply_context: bool,
    reference_text: Option<&mut dyn ReferenceText>,
) -> String {
    // The add-in files the capture's session had open, when its environment attests them.
    let addins_open = context
        .filter(|_| apply_context)
        .and_then(|c| c.addins_open.as_deref())
        .map(|list| list.split(',').filter(|f| !f.is_empty()).collect::<Vec<_>>());
    let library_open = |library: &str| {
        addins_open
            .as_ref()
            .map(|open| open.iter().any(|f| f.eq_ignore_ascii_case(library)))
    };
    let resolved = catalogue().resolve_surface(Notation::Call, function).ok().or_else(|| {
        // A call Excel resolved through the add-in: its export, or the catalogue's qualified
        // spelling, with the library open in the session.
        let (library, export) = catalogue().addin_surface(function).or_else(|| {
            catalogue()
                .addin_library_of_export(function)
                .map(|library| (library, function))
        })?;
        (library_open(library) == Some(true))
            .then(|| catalogue().resolve_addin(library, export))
            .flatten()
    });
    let Some(target) = resolved else {
        // A bare call of an add-in's export (EUROCONVERT) is #NAME? while the add-in is not loaded.
        if catalogue().is_unloaded_addin_export(function) {
            return "error:Name".to_owned();
        }
        // The qualified spelling is #NAME? as well when the session is attested to have no
        // such library open (dc-ymjf.2.65).
        if catalogue()
            .addin_surface(function)
            .is_some_and(|(library, _)| library_open(library) == Some(false))
        {
            return "error:Name".to_owned();
        }
        return "fault:NotCatalogued".to_owned();
    };
    let Ok(mut values) = args
        .iter()
        .enumerate()
        .map(|(i, a)| decode_argument(a.get("range").unwrap_or(a), &format!("args[{i}]")))
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
    });
    if value2_transport {
        // Excel's Value2 cell write stores signed subnormals and negative zero as +0
        // across scalars, CHOOSE arrays, and rRxC ranges (Program.cs:403, 430, 850).
        // Q1-subnormal-inputs.json, dc-1943: 179 affected bank calls replayed.
        for value in &mut values {
            flush_value2(value);
        }
    }
    let mut prepared = values.clone();
    let mut grid = BTreeMap::new();
    let mut range_column = 1u32;
    let mut has_reference = false;
    let mut has_range = false;
    for (position, arg) in args.iter().enumerate() {
        if arg.get("range").is_none() {
            continue;
        }
        let Some(array) = values[position].as_array() else {
            return "fault:Unreadable".to_owned();
        };
        let shape = array.shape();
        for r in 0..shape.rows {
            for c in 0..shape.cols {
                grid.insert(
                    (3, r as u32 + 1, range_column + c as u32),
                    array.get(r, c).unwrap().clone(),
                );
            }
        }
        let first = CellKey::new(SHEET_Q, 1, range_column).unwrap();
        let last = CellKey::new(
            SHEET_Q,
            shape.rows as u32,
            range_column + shape.cols as u32 - 1,
        )
        .unwrap();
        prepared[position] =
            CalcValue::reference(ReferenceLike::area(GridArea::new(first, last).unwrap()));
        range_column += shape.cols as u32;
        has_reference = true;
        has_range = true;
    }
    // An attested Touchstone/1 bulk layout places the argument cells on sheet S from column B
    // (their row stays unknown); without it the cells' place is not known at all.
    let bulk_layout = cell_transport
        && !has_range
        && context.is_some_and(|c| c.cell_layout.as_deref() == Some("touchstone/1-bulk"));
    let first_column = if bulk_layout { 2 } else { 1 };
    let mut bulk_sheet = BulkSheet { errors: BTreeMap::new() };
    if cell_transport {
        for (position, value) in values.iter().enumerate() {
            grid.insert((1, 1, position as u32 + first_column), value.clone());
            if let CoreRef::Error(code) = value.core_ref() {
                bulk_sheet.errors.insert(position as u32 + first_column, code);
            }
        }
        for (position, argument) in prepared.iter_mut().enumerate() {
            if matches!(
                target.parameter_class(position),
                Some(ParameterClass::Reference | ParameterClass::ValueOrReference)
            ) {
                has_reference = true;
                *argument = CalcValue::reference(ReferenceLike::area(GridArea::cell(
                    CellKey::new(SHEET_S, 1, position as u32 + first_column)
                        .expect("argument fits the grid"),
                )));
            }
        }
    }
    // The attested Touchstone worker places every scalar/blank/error argument in a worksheet cell
    // on sheet S — both beside CHOOSE arrays (`RC[...]` in EvaluateBatch, Program.cs:237) and
    // beside rRxC ranges (`S!$<col>$1` in EvaluateSpecial, Program.cs:417-442). Preserve that
    // cell-reference origin on sheet S without colliding with ranges on sheet Q or claiming known
    // row/column positions.
    let mixed_bulk_scalar_cells = !cell_transport
        && values.iter().any(|v| matches!(v.core_ref(), CoreRef::Array(_)))
        && context.is_some_and(|c| {
            c.input_transport.as_deref() == Some("cell_value2_bulk")
                && c.cell_layout.as_deref() == Some("touchstone/1-bulk")
        });
    // Excel reads a database field held in a cell as one referenced cell: an error there is
    // #VALUE! (dc-j9k.272-.276; matching calc-fml's Run::admit). A blank cell stays Empty so
    // DCOUNT/DCOUNTA still treat it as optional, and literal formula arguments (no cell
    // transport) keep their own error.
    if cell_transport || mixed_bulk_scalar_cells {
        for (position, argument) in prepared.iter_mut().enumerate() {
            if target.id().is_database_field(position)
                && args[position].get("range").is_none()
                && matches!(values[position].core_ref(), CoreRef::Error(_))
            {
                *argument = CalcValue::error(calc_func::value::WorksheetErrorCode::Value);
            }
        }
    }
    if mixed_bulk_scalar_cells {
        for (position, argument) in prepared.iter_mut().enumerate() {
            if args[position].get("range").is_none()
                && !matches!(values[position].core_ref(), CoreRef::Array(_))
                && matches!(
                    target.parameter_class(position),
                    Some(ParameterClass::Reference | ParameterClass::ValueOrReference)
                )
            {
                let column = position as u32 + 1;
                grid.insert((1, 1, column), values[position].clone());
                let cell = CellKey::new(SHEET_S, 1, column).expect("argument fits the grid");
                *argument = CalcValue::reference(ReferenceLike::area(GridArea::cell(cell)));
                has_reference = true;
            }
        }
    }
    // A range argument sits at a known place (Q!$A$1 for the first, call at a time), so its row and
    // column are known; a scalar argument's row is known only through the bulk layout's column.
    if has_reference
        && !has_range
        && (target.spelling() == "ROW" || (target.spelling() == "COLUMN" && !bulk_layout))
    {
        return "fault:WitnessReferencePosition".to_owned();
    }
    let mode = NumericMode::Fast;
    let visible_constants = context
        .is_some_and(|c| c.input_sheet_state.as_deref() == Some("visible-constant-inputs"))
        || has_range;
    let row_free = bulk_layout && matches!(target.spelling(), "SHEET" | "ISFORMULA" | "FORMULATEXT" | "COLUMN");
    let mut cells = InputCells(grid, visible_constants, row_free, bulk_layout, has_range);
    // A capture can know its separators without knowing its complete locale.
    // Preserve only those facts; VALUE and date parsing still require a profile.
    let separators = context.and_then(|c| {
        c.decimal_separator
            .as_deref()
            .zip(c.thousands_separator.as_deref())
    });
    let mut services = if has_reference {
        EvalServices::with_cells(&mode, &mut cells).with_environment(&environment)
    } else {
        EvalServices::new(&mode).with_environment(&environment)
    };
    if let Some((decimal, group)) = separators {
        services = services.with_number_separators(decimal, group);
    }
    if let Some(reference_text) = reference_text {
        services = services.with_reference_text(reference_text);
    }
    if row_free {
        services = services.with_host(&mut bulk_sheet);
    }
    let result = invoke(
        target,
        &mut Given(prepared),
        &mut services,
        &mut FunctionCallScratch::new(),
    );
    drop(services);
    match result {
        Err(FunctionFault::NotImplemented { argument: Some(0), .. })
            if has_reference && target.spelling() == "OFFSET" =>
        {
            "fault:WitnessReferencePosition".to_owned()
        }
        Err(FunctionFault::NotImplemented { argument: Some(1), .. })
            if has_reference && target.spelling() == "CELL" =>
        {
            "fault:WitnessReferencePosition".to_owned()
        }
        Err(fault) => {
            let text = format!("{fault:?}");
            let end = text.find([' ', '{', '(']).unwrap_or(text.len());
            format!("fault:{}", &text[..end])
        }
        Ok(value) => dereference(value, &cells)
            .and_then(|v| select_result(&v, index))
            // A blank remains blank while functions consume it; a final worksheet
            // result displays it as zero, as calc-fml's result path does.
            .map(|v| {
                if v.is_empty() {
                    CalcValue::number(0.0)
                } else {
                    v
                }
            })
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

    /// Excel 16.0.20527: the selected blank is displayed as a zero by the formula cell.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn final_blank_is_zero_after_the_function_call() {
        let context = cells();
        for (function, args) in [
            ("IF", vec![json!(true), json!(null)]),
            ("IFERROR", vec![json!(null), json!("fallback")]),
        ] {
            assert_eq!(
                calculate(function, &args, None, Some(&context), true),
                "0x0000000000000000"
            );
        }
        assert_eq!(
            calculate("ISBLANK", &[json!(null)], None, Some(&context), true),
            "logical:True"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rectangular_ranges_keep_reference_identity_and_cells() {
        let args = [
            json!({"range": [["0x3ff0000000000000"], ["0x4000000000000000"], ["0x4008000000000000"]]}),
            json!(">1"),
        ];
        assert_eq!(
            calculate("SUMIF", &args, None, None, true),
            "0x4014000000000000"
        );
        assert_eq!(
            calculate("ROWS", &args[..1], None, None, true),
            "0x4008000000000000"
        );
        assert_eq!(
            calculate("ISREF", &args[..1], None, None, true),
            "logical:True"
        );
        let database = [
            json!({"range": [["Name", "Value"], ["a", "0x4000000000000000"], ["b", "0x4008000000000000"]]}),
            json!("Value"),
            json!({"range": [["Name"], ["a"]]}),
        ];
        assert_eq!(
            calculate("DSUM", &database, None, None, true),
            "0x4000000000000000"
        );
        assert_eq!(
            calculate(
                "ISREF",
                &[json!([["0x3ff0000000000000"]])],
                None,
                None,
                true
            ),
            "logical:False"
        );
    }

    /// dc-y7tqj (rw12 finding 16): witness database_field admission matches calc-fml:
    /// literal formula errors (no cell transport) propagate instead of turning into #VALUE!.
    /// A blank field cell is #VALUE! for DCOUNT and DCOUNTA too (Excel 20625, dc-j9k.555 and .557:
    /// only a field left out of the formula counts the records).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn database_field_matches_calc_fml_admission() {
        let bulk = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let db = json!({"range": [["Name", "Value"], ["a", "0x4000000000000000"], ["b", "0x4008000000000000"]]});
        let crit = json!({"range": [["Name"], [null]]});
        // Blank field held in a cell: #VALUE! for DCOUNT, DCOUNTA and DPRODUCT alike.
        let blank_args = [db.clone(), json!(null), crit.clone()];
        for name in ["DCOUNT", "DCOUNTA"] {
            assert_eq!(calculate(name, &blank_args, None, Some(&bulk), true), "error:Value");
        }
        assert_eq!(
            calculate("DPRODUCT", &blank_args, None, Some(&bulk), true),
            "error:Value"
        );
        // Held cell error under bulk transport is #VALUE! (dc-j9k.272), while a literal formula
        // error (no cell transport) propagates as #NAME?.
        let err_args = [db, json!({"error": "Name"}), crit];
        assert_eq!(
            calculate("DPRODUCT", &err_args, None, Some(&bulk), true),
            "error:Value"
        );
        assert_eq!(
            calculate("DPRODUCT", &err_args, None, None, true),
            "error:Name"
        );
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

    /// Retained Excel controls admit2-enrich-OFFSET-1025 and
    /// admit2-special-OFFSET-0127 validate arguments before looking up a position.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn offset_validates_arguments_without_inventing_a_position() {
        let context = cells();
        let zero = json!("0x0000000000000000");
        assert_eq!(
            calculate(
                "OFFSET",
                &[json!("0x3119b479952404dc"), json!("0x40e5f90000000000"),
                  json!("0x4052bfffffffffff"), json!("<>"), json!("0x54b249ad2594c37d")],
                None, Some(&context), true,
            ),
            "error:Value"
        );
        assert_eq!(
            calculate(
                "OFFSET",
                &[json!("0x3fbb6c919f634628"), json!("0x40808d8f333535c0"),
                  json!("0x3fe269f47182f815"), json!({"error": "NA"})],
                None, Some(&context), true,
            ),
            "error:NA"
        );
        assert_eq!(
            calculate(
                "OFFSET", &[zero.clone(), zero.clone(), zero.clone(), zero],
                None, Some(&context), true,
            ),
            "fault:WitnessReferencePosition"
        );
    }

    /// Retained independent Excel controls admit2-scalar-OFFSET-0108 and
    /// admit2-more-OFFSET-0126 shift by more than a full grid dimension.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn offset_can_reject_a_single_cell_shift_outside_every_position() {
        let context = cells();
        for args in [
            ["0x2e15b33c4672ef20", "0xcf3db05ed98b5f04", "0xbfe0000000000000"],
            ["0x3359a466a356a20a", "0x40e5f90000000000", "0xc53ed78ec297d6a6"],
        ] {
            assert_eq!(
                calculate("OFFSET", &args.map(|arg| json!(arg)), None, Some(&context), true),
                "error:Ref"
            );
        }
    }

    /// Excel's retained dcuar_cell capture validates these info types before reading B2/A1.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cell_validates_info_without_a_reference_position() {
        let context = cells();
        for info in [json!("x"), json!(" format "), json!("format "), json!("0x3ff0000000000000")] {
            assert_eq!(
                calculate("CELL", &[info, json!("0x4000000000000000")], None, Some(&context), true),
                "error:Value"
            );
        }
        assert_eq!(
            calculate("CELL", &[json!({"error": "NA"}), json!("0x4000000000000000")], None, Some(&context), true),
            "error:NA"
        );
        for info in ["address", "row", "col", "contents", "type", "filename", "format", "color", "parentheses", "prefix", "protect", "width"] {
            assert_eq!(
                calculate("CELL", &[json!(info), json!("0x4000000000000000")], None, Some(&context), true),
                "fault:WitnessReferencePosition",
                "{info}"
            );
        }
    }

    /// Independent Excel controls distinguish cell errors from formed errors in mixed regressions.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mixed_regression_error_origin_uses_attested_cell_transport() {
        let attested = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let unknown = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            ..Default::default()
        };
        let y = json!([["0x3ff0000000000000"], ["0x4000000000000000"],
            ["0x4008000000000000"], ["0x4010000000000000"], ["0x4014000000000000"]]);
        let err = json!({"error": "Div0"});
        for name in ["LINEST", "LOGEST"] {
            assert_eq!(calculate(name, &[y.clone(), err.clone()], None, Some(&attested), true), "error:Ref");
            assert_eq!(calculate(name, &[err.clone(), y.clone()], None, Some(&attested), true), "error:Value");
            assert_eq!(calculate(name, &[y.clone(), err.clone()], None, Some(&unknown), true), "error:Div0");
            assert_eq!(calculate(name, &[json!({"range": y.clone()}), err.clone()], None, Some(&attested), true), "error:Ref");
            assert_eq!(calculate(name, &[json!({"range": y.clone()}), err.clone()], None, Some(&unknown), true), "error:Div0");
        }
    }

    /// Touchstone/1 bulk passes scalar blank args as cell references (`RC[...]`) even alongside
    /// `CHOOSE` array args; AVERAGE ignores blank referenced cells instead of counting them as 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mixed_bulk_scalar_blanks_keep_cell_reference_identity() {
        let attested = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let arr = json!([["0x4024000000000000"], ["0x4034000000000000"]]); // [10, 20]
        let args = [arr, Value::Null, Value::Null];
        assert_eq!(
            calculate("AVERAGE", &args, None, Some(&attested), true),
            format!("0x{:016x}", 15.0_f64.to_bits()),
        );
        // Without attested bulk cell transport, direct Empty arguments count as 0: (10+20+0+0)/4 = 7.5.
        assert_eq!(
            calculate("AVERAGE", &args, None, None, true),
            format!("0x{:016x}", 7.5_f64.to_bits()),
        );
    }

    /// dc-ymjf.2.84/.85: an attested Touchstone/1 bulk layout fixes an argument cell's sheet
    /// and column, not its row.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_bulk_layout_fixes_sheet_and_column_but_not_row() {
        let bulk = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let cells = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            ..Default::default()
        };
        let text = [Value::String("abc".into())];
        let error = [serde_json::json!({"error": "NA"})];
        let one = format!("0x{:016x}", 1f64.to_bits());
        let two = format!("0x{:016x}", 2f64.to_bits());
        assert_eq!(calculate("COLUMN", &text, None, Some(&bulk), true), two);
        assert_eq!(calculate("SHEET", &text, None, Some(&bulk), true), one);
        assert_eq!(calculate("ISFORMULA", &text, None, Some(&bulk), true), "logical:False");
        assert_eq!(calculate("ISFORMULA", &error, None, Some(&bulk), true), "logical:True");
        assert_eq!(calculate("FORMULATEXT", &text, None, Some(&bulk), true), "error:NA");
        assert_eq!(calculate("FORMULATEXT", &error, None, Some(&bulk), true), "text:=#N/A");
        assert_eq!(calculate("ROW", &text, None, Some(&bulk), true), "fault:WitnessReferencePosition");
        assert_eq!(calculate("COLUMN", &text, None, Some(&cells), true), "fault:WitnessReferencePosition");
        assert!(calculate("FORMULATEXT", &text, None, Some(&cells), true).starts_with("fault:"));
    }

    /// dc-mlhgn: the call-at-a-time worker places an `rRxC` range at `Q!$A$1` (row 1, column 1 for
    /// the first range), so ROW and COLUMN of a range argument are known without a bulk layout.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn row_and_column_of_a_range_argument_use_its_known_placement() {
        let bulk = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let one = format!("0x{:016x}", 1f64.to_bits());
        let range = [json!({"range": [["0x3ff0000000000000", "0x4000000000000000"], ["0x4008000000000000", "0x4010000000000000"]]})];
        for context in [Some(&bulk), None] {
            // A 2 x 2 range answers its columns as a 1 x 2 row and its rows as a 2 x 1 column.
            let column = calculate("COLUMN", &range, None, context, true);
            assert!(column.contains("\"rows\":1,\"columns\":2"), "{column}");
            assert!(column.contains("3ff0000000000000") && column.contains("4000000000000000"), "{column}");
            let row = calculate("ROW", &range, None, context, true);
            assert!(row.contains("\"rows\":2,\"columns\":1"), "{row}");
            let single = [json!({"range": [["0x4008000000000000"]]})];
            assert_eq!(calculate("COLUMN", &single, None, context, true), one);
            assert_eq!(calculate("ROW", &single, None, context, true), one);
        }
    }

    /// The Lists-in-Cells beta functions are #NAME? below compatibility version 3 (Excel
    /// 16.0.20527 bank rows) and stay deferred at version 3.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lists_in_cells_functions_follow_the_compatibility_version() {
        let args = [Value::String("abc".into()), Value::String("b".into())];
        let version = |v: &str| WitnessInvocation {
            compatibility_version: Some(v.to_owned()),
            ..Default::default()
        };
        for function in ["HAS", "HASANY", "HASALL"] {
            assert_eq!(calculate(function, &args, None, Some(&version("2")), true), "error:Name");
            assert_eq!(calculate(function, &args, None, Some(&version("3")), true), "fault:Deferred");
        }
        assert_eq!(calculate("FLATTEN", &args[..1], None, Some(&version("1")), true), "error:Name");
    }

    /// dc-ymjf.2.65: an add-in call resolves as the session's recorded open add-ins allow.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn addin_calls_follow_the_recorded_open_addins() {
        let args = [
            Value::String(format!("0x{:016x}", 1000f64.to_bits())),
            Value::String("DEM".into()),
            Value::String("EUR".into()),
        ];
        let with = |open: Option<&str>| WitnessInvocation {
            addins_open: open.map(str::to_owned),
            ..Default::default()
        };
        let none = with(Some(""));
        let euro = with(Some("EUROTOOL.XLAM"));
        let unknown = with(None);
        // 1000 DEM is 511.29 EUR (crates/calc-host/tests/fixtures/euroconvert/literals.tsv).
        let converted = format!("0x{:016x}", 511.29f64.to_bits());
        assert_eq!(calculate("_EUROTOOL.EUROCONVERT", &args, None, Some(&none), true), "error:Name");
        assert_eq!(calculate("EUROCONVERT", &args, None, Some(&none), true), "error:Name");
        assert_eq!(calculate("EUROCONVERT", &args, None, Some(&euro), true), converted);
        assert_eq!(calculate("_EUROTOOL.EUROCONVERT", &args, None, Some(&euro), true), converted);
        assert_eq!(
            calculate("_EUROTOOL.EUROCONVERT", &args, None, Some(&unknown), true),
            "fault:NotCatalogued"
        );
        assert_eq!(calculate("EUROCONVERT", &args, None, Some(&unknown), true), "error:Name");
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
        assert_eq!(
            calculate(
                "NUMBERVALUE",
                &[Value::String("1.234,5".into())],
                None,
                Some(&context),
                true
            ),
            "0x40934a0000000000"
        );
        assert_eq!(
            calculate(
                "VALUE",
                &[Value::String("1.234,5".into())],
                None,
                Some(&context),
                true
            ),
            "fault:NotImplemented"
        );
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

    /// Excel 20527 en-ZA answers of 2026-11-20 (dc-iuwct.35, 33 fresh Touchstone answers, the pinned
    /// rows' invocation): with a separator outside "." "," " " U+00A0, text NUMBERVALUE's own
    /// reader refuses is read by the VALUE grammar after the group separator is removed before the
    /// first decimal separator and each decimal separator becomes the host's.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbervalue_letter_separators_fall_back_to_the_value_grammar() {
        let context = WitnessInvocation {
            compatibility_version: Some("2".into()),
            decimal_separator: Some(".".into()),
            input_sheet_state: Some("visible-constant-inputs".into()),
            input_transport: Some("cell_value2_bulk".into()),
            locale: Some("en-ZA".into()),
            thousands_separator: Some(" ".into()),
            ..Default::default()
        };
        let call = |function: &str, args: &[&str]| {
            let args: Vec<Value> = args.iter().map(|a| Value::String((*a).into())).collect();
            calculate(function, &args, None, Some(&context), true)
        };
        let n = |x: f64| format!("0x{:016x}", x.to_bits());
        for (args, excel) in [
            (["%1e3", ".", "e"], n(0.13)),
            (["%1e3", "e", ","], n(0.013)),
            (["1e-3", "e", ","], "error:Value".to_owned()),
            (["1x5", ".", "x"], n(15.0)),
            (["%25", ".", ","], n(0.25)),
            (["%1x5", ".", "x"], n(0.15)),
            (["$1q5", ".", "q"], "error:Value".to_owned()),
            (["(1q5)", ".", "q"], n(-15.0)),
            (["1q5%", ".", "q"], n(0.15)),
            (["%1q5", "q", ","], n(0.015)),
            (["1q-3", "q", ","], "error:Value".to_owned()),
            (["1e3q", ".", "q"], n(1000.0)),
            (["%%1x5", ".", "x"], "error:Value".to_owned()),
            (["-%1x5", ".", "x"], n(-0.15)),
            (["1x5e2", ".", "x"], n(1500.0)),
            (["1x5", ".", "xyz"], n(15.0)),
            (["2q2026-3", ".", "q"], "error:Value".to_owned()),
            (["%1e3", ".", "e%"], n(0.13)),
            (["1e3,0", "e", ","], "error:Value".to_owned()),
            (["1,2e3", "e", ","], n(12.3)),
            (["1q2.5", ".", "q"], n(12.5)),
            (["1.5q2", ".", "q"], "error:Value".to_owned()),
            (["%1.5q2", ".", "q"], "error:Value".to_owned()),
            (["%1q2.5", ".", "q"], n(0.125)),
            (["1q2e3", ".", "q"], n(12000.0)),
            (["%1,2e3", "e", ","], n(0.123)),
            (["(1,2e3)", "e", ","], n(-12.3)),
            (["1e3.0", "e", ","], "error:Value".to_owned()),
            (["1e1e2", "e", ","], "error:Value".to_owned()),
        ] {
            assert_eq!(call("NUMBERVALUE", &args), excel, "{args:?}");
        }
        // "1-3" is 3 January of the calculation's year: 46025 in Excel's answers of 2026-11-20
        // (serial 46346), through both separator routes. Without an attested clock the year is
        // unknown and the call is refused.
        assert_eq!(
            call("NUMBERVALUE", &["1q-3", ".", "q"]),
            "fault:NotImplemented"
        );
        let clocked = WitnessInvocation {
            calculation_clock: Some(format!("{:016x}", 46346.5_f64.to_bits())),
            ..context.clone()
        };
        let captured = WitnessInvocation {
            capture_date: Some("2026-11-06".into()),
            ..context.clone()
        };
        let leap_year = WitnessInvocation {
            capture_date: Some("2024-06-15".into()),
            ..context.clone()
        };
        for args in [
            ["1e-3", ".", "e"],
            ["1q-3", ".", "q"],
            ["1-3", ".", "q"],
            ["1-3", ".", ","],
            ["1/3", ".", ","],
            ["1e/3", ".", "e"],
        ] {
            let args: Vec<Value> = args.iter().map(|a| Value::String((*a).into())).collect();
            assert_eq!(
                calculate("NUMBERVALUE", &args, None, Some(&clocked), true),
                n(46025.0),
                "{args:?}"
            );
            assert_eq!(
                calculate("NUMBERVALUE", &args, None, Some(&captured), true),
                n(46025.0),
                "{args:?}"
            );
            // Moving the capture date to 2024 shifts the completed year to 2024-01-03 (45294).
            assert_eq!(
                calculate("NUMBERVALUE", &args, None, Some(&leap_year), true),
                n(45294.0),
                "{args:?}"
            );
        }
        // Leap-day year-less date: #VALUE! in non-leap 2026, 45351 (2024-02-29) in leap year 2024.
        let feb29: Vec<Value> = ["2-29", ".", ","]
            .iter()
            .map(|a| Value::String((*a).into()))
            .collect();
        assert_eq!(
            calculate("NUMBERVALUE", &feb29, None, Some(&captured), true),
            "error:Value"
        );
        assert_eq!(
            calculate("NUMBERVALUE", &feb29, None, Some(&leap_year), true),
            n(45351.0)
        );
    }

    /// rw7 items 2 & 3:
    /// - EvaluateSpecial passes scalar args beside rRxC ranges as S!$<col>$1 cell references.
    /// - Value2 writes flush subnormals and -0.0 inside arrays and ranges as well as scalars.
    /// - Sheet Q cells past a smaller SUMIF/AVERAGEIF sum_range/average_range are empty, not unreadable.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn range_with_scalar_transport_value2_flushing_and_sumif_empty_cells() {
        let attested = WitnessInvocation {
            input_transport: Some("cell_value2_bulk".into()),
            cell_layout: Some("touchstone/1-bulk".into()),
            ..Default::default()
        };
        let value2_only = WitnessInvocation {
            input_transport: Some("cell_value2".into()),
            ..Default::default()
        };
        let n = |x: f64| format!("0x{:016x}", x.to_bits());

        // 1. Scalar blank/error arguments beside an rRxC range keep cell-reference identity
        //    under attested touchstone/1-bulk transport (matching Program.cs EvaluateSpecial).
        let r_avg = json!({"range": [[n(10.0)], [n(20.0)]]});
        assert_eq!(
            calculate("AVERAGE", &[r_avg.clone(), Value::Null, Value::Null], None, Some(&attested), true),
            n(15.0),
            "AVERAGE(range, blank, blank) must ignore blank cell refs on sheet S"
        );
        assert_eq!(
            calculate("AVERAGE", &[r_avg, Value::Null, Value::Null], None, None, true),
            n(7.5),
            "without attested bulk transport, direct Empty args count as 0"
        );
        let y = json!([[n(1.0)], [n(2.0)], [n(3.0)], [n(4.0)], [n(5.0)]]);
        let err = json!({"error": "Div0"});
        for name in ["LINEST", "LOGEST"] {
            assert_eq!(
                calculate(name, &[json!({"range": y.clone()}), err.clone()], None, Some(&attested), true),
                "error:Ref",
                "{name}(range, err) under attested bulk transport passes err via S!$C$1"
            );
            assert_eq!(
                calculate(name, &[json!({"range": y.clone()}), err.clone()], None, Some(&value2_only), true),
                "error:Div0",
                "{name}(range, err) without touchstone/1-bulk keeps direct error"
            );
        }

        // 2. Value2 subnormal and -0.0 flushing applies inside arrays and ranges.
        let neg_zero = "0x8000000000000000";
        let subnormal = "0x0000000000000001";
        let pos_zero = "0x0000000000000000";
        let arr_sub = json!([[subnormal, neg_zero]]);
        let rng_sub = json!({"range": [[subnormal, neg_zero]]});
        for inv in [&attested, &value2_only] {
            assert_eq!(
                calculate("INDEX", &[arr_sub.clone(), json!(n(1.0)), json!(n(1.0))], None, Some(inv), true),
                pos_zero,
                "Value2 transport must flush subnormals inside arrays to +0.0"
            );
            assert_eq!(
                calculate("INDEX", &[arr_sub.clone(), json!(n(1.0)), json!(n(2.0))], None, Some(inv), true),
                pos_zero,
                "Value2 transport must flush -0.0 inside arrays to +0.0"
            );
            assert_eq!(
                calculate("INDEX", &[rng_sub.clone(), json!(n(1.0)), json!(n(1.0))], None, Some(inv), true),
                pos_zero,
                "Value2 transport must flush subnormals inside ranges to +0.0"
            );
            assert_eq!(
                calculate("INDEX", &[rng_sub.clone(), json!(n(1.0)), json!(n(2.0))], None, Some(inv), true),
                pos_zero,
                "Value2 transport must flush -0.0 inside ranges to +0.0"
            );
        }
        // Without Value2 transport, exact bits are preserved.
        assert_eq!(
            calculate("INDEX", &[arr_sub, json!(n(1.0)), json!(n(1.0))], None, None, true),
            subnormal
        );

        // 3. SUMIF and AVERAGEIF extending a smaller sum_range/average_range on sheet Q read
        //    unwritten cells as empty instead of fault:ReferenceUnreadable (and scalar criteria
        //    on sheet S never collides with extended ranges on sheet Q).
        let criteria_range = json!({"range": [[n(1.0), n(2.0)], [n(3.0), n(4.0)]]});
        let small_sum_range = json!({"range": [[n(10.0)]]});
        assert_eq!(
            calculate(
                "SUMIF",
                &[criteria_range.clone(), json!(">0"), small_sum_range.clone()],
                None,
                Some(&attested),
                true,
            ),
            n(10.0),
            "SUMIF with 1x1 sum_range extended to 2x2 must treat unwritten Q cells as empty"
        );
        assert_eq!(
            calculate(
                "SUMIF",
                &[criteria_range.clone(), json!("=4"), small_sum_range.clone()],
                None,
                Some(&attested),
                true,
            ),
            pos_zero,
            "SUMIF matching only an unwritten extended Q cell must return 0"
        );
        assert_eq!(
            calculate(
                "AVERAGEIF",
                &[criteria_range, json!(">0"), small_sum_range],
                None,
                Some(&attested),
                true,
            ),
            n(10.0),
            "AVERAGEIF ignores empty cells in extended average_range on sheet Q"
        );
    }
}
