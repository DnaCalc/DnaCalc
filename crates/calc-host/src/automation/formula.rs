//! `Range.Formula` (get and put) and `Range.Text` (dc-piw0.6). The put enters the text through
//! the session's typed-entry path (`SkinIntent::EnterEntries`: `interpret_entry_in`, the formula
//! parser, relocation across the range, one recalculation), the get reads what the session holds.
use super::*;
use crate::{CellInput, SkinIntent};

/// Why a Range.Formula answer is the simplest Excel-like one: ~/codex-runs/eng4/excel-questions.md.
impl WorkbookAutomation {
    pub(super) fn register_formula(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Formula", InvokeKind::Get, Self::range_formula);
        handlers.on("Range", "Formula", InvokeKind::Put, Self::put_formula);
        handlers.on("Range", "FormulaR1C1", InvokeKind::Get, Self::range_formula_r1c1);
        handlers.on("Range", "FormulaR1C1", InvokeKind::Put, Self::put_formula_r1c1);
        handlers.on("Range", "Text", InvokeKind::Get, Self::cells_text);
    }

    /// The projected cells of the first area, row-major (session reads only).
    fn session_cells(
        access: &mut Access<'_>,
        area: GridArea,
        what: &str,
    ) -> Result<Vec<crate::ProjectedCell>, AutoError> {
        let Access::Session(session, _) = access else {
            return Err(AutoError::not_modelled(
                format!("{what} inside a worksheet invocation"),
                OWNER,
            ));
        };
        if area.cell_count() > 1_048_576 {
            return Err(AutoError::not_modelled(
                format!("{what} materialization limit"),
                OWNER,
            ));
        }
        let mut cells = Vec::with_capacity(area.cell_count() as usize);
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                cells.push(CellKey::new(area.sheet(), row, col).expect("cell in valid area"));
            }
        }
        crate::project(session.host(), &ProjectionRequest { cells })
            .map(|page| page.cells().to_vec())
            .map_err(|e| AutoError::not_modelled(e.to_string(), OWNER))
    }

    fn shaped(area: GridArea, mut values: Vec<AutoValue>) -> Reply {
        if values.len() == 1 {
            return Ok(values.remove(0));
        }
        Ok(AutoValue::Array(Box::new(AutoArray {
            rows: area.row_count(),
            columns: area.column_count(),
            lower: (1, 1),
            values,
        })))
    }

    /// What Excel's `Range.Formula` answers (dc-piw0.10 Q16): always a String. A formula is its text in
    /// canonical case with the spaces around `:` removed (`=SUM( A1:A2 )`); a constant is its text: a number
    /// to 15 significant digits (a date-formatted number is its serial), TRUE/FALSE, an error constant
    /// `#N/A`, text as it is (a quote prefix is dropped), an empty cell `""`. A range answers a 2-D array.
    fn range_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.formula_get(target, call, false)
    }
    /// `Range.FormulaR1C1`: the same answer with every reference of a formula spelled relative to its own
    /// cell (`=RC[-1]*2`); constants are as `Range.Formula` gives them.
    fn range_formula_r1c1(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.formula_get(target, call, true)
    }
    fn formula_get(&self, target: &ObjectRef, call: &Call<'_>, r1c1: bool) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            let area = range.areas()[0];
            let cells = Self::session_cells(access, area, "Range.Formula")?;
            let width = area.column_count() as usize;
            let values = cells
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let at = CellKey::new(
                        area.sheet(),
                        area.first().row() + (i / width) as u32,
                        area.first().column() + (i % width) as u32,
                    )
                    .expect("cell in valid area");
                    Ok(AutoValue::text(&match cell.input() {
                        Some(CellInput::Formula(_)) => {
                            let text = normalise_formula(&crate::search::entry_text(cell.input()));
                            match (r1c1, access_model(access)) {
                                (true, Some(root)) => convert_text(root, &text, at, false)?,
                                _ => text,
                            }
                        }
                        other => constant_entry_text(other),
                    }))
                })
                .collect::<Result<Vec<_>, AutoError>>()?;
            Self::shaped(area, values)
        })
    }

    /// The text the cells show under their number formats, the way the grid draws it, fitted to the
    /// column width (dc-piw0.10 Q16): General shrinks the decimals (`0.33333333` in 10 characters,
    /// `0.3` in 3), a number that cannot fit shows as many `#` as fit. A range answers the text its cells
    /// share, Null when they differ.
    fn cells_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            let area = range.areas()[0];
            let cells = Self::session_cells(access, area, "Range.Text")?;
            let Access::Session(session, strict) = access else {
                unreachable!("session_cells refused");
            };
            let host = session.host();
            let mut shared: Option<String> = None;
            let width = area.column_count() as usize;
            for (i, cell) in cells.iter().enumerate() {
                if *strict
                    && let Some(provenance) = observed_provenance(cell.value())
                {
                    session.record_macro_observation(crate::MacroObservation {
                        at: cell.at(),
                        provenance,
                    });
                }
                let at = CellKey::new(
                    area.sheet(),
                    area.first().row() + (i / width) as u32,
                    area.first().column() + (i % width) as u32,
                )
                .expect("cell in valid area");
                let text = Self::text_of_cell(host, cell, at)?;
                match &shared {
                    None => shared = Some(text),
                    Some(first) if *first == text => {}
                    Some(_) => return Ok(AutoValue::Null),
                }
            }
            Ok(AutoValue::text(&shared.unwrap_or_default()))
        })
    }

    pub(super) fn text_of_cell(
        host: &crate::HostState,
        cell: &crate::ProjectedCell,
        at: CellKey,
    ) -> Result<String, AutoError> {
        let value = match cell.value() {
            ProjectedValue::Empty => return Ok(String::new()),
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => v.clone(),
            ProjectedValue::FileCached(cache) => cache.value().to_value(),
            ProjectedValue::Stale(stale) => stale
                .last_value()
                .cloned()
                .or_else(|| stale.file_cache().map(|c| c.value().to_value()))
                .unwrap_or_else(CalcValue::empty),
            ProjectedValue::NotModeled { .. } => {
                return Err(AutoError::not_modelled(
                    "Range.Text of an unmodelled formula",
                    OWNER,
                ));
            }
        };
        let environment = host.entry_environment();
        // A session without a regional profile shows numbers as en-US does.
        let fallback;
        let profile = match environment.profile.as_ref() {
            Some(profile) => profile,
            None => {
                fallback = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs);
                &fallback
            }
        };
        let fits = host.column_characters(at.sheet(), at.column()).floor().max(0.0) as usize;
        let general = |value: &CalcValue| -> Result<String, AutoError> {
            Ok(match value.core_ref() {
                CoreRef::Number(n) => {
                    let text = calc_func::general_format(profile, n);
                    fit_general(profile.decimal_separator, n, text, fits)
                }
                CoreRef::Text(t) => t.to_string_lossy(),
                CoreRef::Logical(true) => "TRUE".to_owned(),
                CoreRef::Logical(false) => "FALSE".to_owned(),
                CoreRef::Error(_) => return Self::error_text(&value),
                CoreRef::Empty => String::new(),
                _ => return Err(AutoError::not_modelled("Range.Text cell kind", OWNER)),
            })
        };
        let code = host.number_format(at);
        Ok(match code {
            Some(code)
                if !code.eq_ignore_ascii_case("general")
                    && matches!(value.core_ref(), CoreRef::Number(_) | CoreRef::Text(_)) =>
            {
                let system = if host.declared_date1904() {
                    calc_func::locale::DateSystem::System1904
                } else {
                    calc_func::locale::DateSystem::System1900
                };
                match calc_func::cell_text(profile, system, &value, code) {
                    // A number that does not fit shows as many `#` as the column has characters.
                    Ok(text)
                        if matches!(value.core_ref(), CoreRef::Number(_))
                            && text.chars().count() > fits =>
                    {
                        "#".repeat(fits.max(1))
                    }
                    Ok(text) => text,
                    Err(calc_func::CellFormatError::Unshowable) => "#".repeat(fits.max(1)),
                    Err(calc_func::CellFormatError::NotModelled) => general(&value)?,
                }
            }
            _ => general(&value)?,
        })
    }

    fn error_text(value: &CalcValue) -> Result<String, AutoError> {
        match auto_value(value)? {
            AutoValue::Error(code) => Ok(match code {
                2000 => "#NULL!",
                2007 => "#DIV/0!",
                2015 => "#VALUE!",
                2023 => "#REF!",
                2029 => "#NAME?",
                2036 => "#NUM!",
                _ => "#N/A",
            }
            .to_owned()),
            _ => Err(AutoError::not_modelled("Range.Text error", OWNER)),
        }
    }

    /// An argument as the text a user would type: text as it is, numbers by their shortest
    /// round-trip decimal, logicals as TRUE/FALSE, Empty as nothing (a clear).
    pub(super) fn entry_of(value: &AutoValue) -> Result<String, AutoError> {
        Ok(match value {
            AutoValue::Empty | AutoValue::Missing => String::new(),
            AutoValue::Text(t) => t.to_string_lossy(),
            AutoValue::R8(n) => n.to_string(),
            AutoValue::I4(n) => n.to_string(),
            AutoValue::I2(n) => n.to_string(),
            AutoValue::Bool(true) => "TRUE".to_owned(),
            AutoValue::Bool(false) => "FALSE".to_owned(),
            _ => {
                return Err(AutoError::not_modelled(
                    "Range.Formula put of this value type",
                    OWNER,
                ));
            }
        })
    }

    fn put_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.formula_put(target, call, false)
    }
    /// `Range.FormulaR1C1` put: a formula's references are read relative to the cell they are entered in
    /// (`=RC[-1]*2`), so one text over a range is the same relative formula in every cell.
    fn put_formula_r1c1(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.formula_put(target, call, true)
    }
    fn formula_put(&self, target: &ObjectRef, call: &Call<'_>, r1c1: bool) -> Reply {
        // An array of arrays writes nothing and raises no error (Q16).
        if matches!(call.args.last(), Some(AutoValue::Array(a))
            if a.values.iter().any(|v| matches!(v, AutoValue::Array(_))))
        {
            return Ok(AutoValue::Empty);
        }
        self.mutate_range(target, call, |session, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let area = range.areas()[0];
            let mut entries = match call.arg(0) {
                AutoValue::Array(a) => {
                    if u64::from(a.rows) * u64::from(a.columns) != a.values.len() as u64 {
                        return Err(AutoError::not_modelled(
                            "Range.Formula array of unusual shape",
                            OWNER,
                        ));
                    }
                    // A one-row array is a 1-D VBA array: it goes along each row of the range (a column
                    // range gets its first element in every row). A 2-D array goes element by element and
                    // the cells it does not reach get #N/A, as an array entered into a larger range does.
                    let one_dimensional = a.rows == 1;
                    let mut entries = Vec::with_capacity(area.cell_count() as usize);
                    for row in 0..area.row_count() {
                        for column in 0..area.column_count() {
                            let at = CellKey::new(
                                area.sheet(),
                                area.first().row() + row,
                                area.first().column() + column,
                            )
                            .expect("cell in valid area");
                            let source = if one_dimensional { 0 } else { row };
                            let entry = if source < a.rows && column < a.columns {
                                Self::entry_of(&a.values[(source * a.columns + column) as usize])?
                            } else {
                                "#N/A".to_owned()
                            };
                            entries.push((GridArea::cell(at), entry));
                        }
                    }
                    entries
                }
                other => vec![(area, Self::entry_of(other)?)],
            };
            if r1c1 {
                for (cells, entry) in &mut entries {
                    if entry.starts_with('=') {
                        *entry = convert_text(session.host().model(), entry, cells.first(), true)?;
                    }
                }
            }
            Ok(SkinIntent::EnterEntries { entries })
        })
    }
}

/// The model of a session read, when the access is one.
/// What `Range.Formula` answers for a cell that holds no formula (see `range_formula`).
pub(super) fn constant_entry_text(input: Option<&CellInput>) -> String {
    match input {
        None => String::new(),
        Some(CellInput::Number(n)) => number_text_15(n.get()),
        Some(CellInput::Text(t)) => t.text().to_string_lossy(),
        Some(CellInput::Logical(b)) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        Some(CellInput::Error(code)) => code.excel_spelling().to_owned(),
        Some(CellInput::Formula(_)) => unreachable!("a formula is spelled by the caller"),
    }
}

/// The text of a cell's formula as `Range.Formula` gives it (canonical case, no spaces around `:`).
pub(super) fn formula_entry_text(input: Option<&CellInput>) -> String {
    match input {
        Some(CellInput::Formula(_)) => normalise_formula(&crate::search::entry_text(input)),
        other => constant_entry_text(other),
    }
}

fn access_model<'a>(access: &'a Access<'_>) -> Option<&'a calc_doc_model::ModelRoot> {
    match access {
        Access::Session(session, _) => Some(session.host().model()),
        Access::Invocation(_) => None,
    }
}

/// A formula entry (leading `=`) spelled in the other notation for the cell `at`: `to_a1` reads R1C1 and
/// writes A1 (the document holds A1), otherwise it reads A1 and writes R1C1. A text that does not parse is
/// Excel's 1004.
fn convert_text(
    root: &calc_doc_model::ModelRoot,
    entry: &str,
    at: CellKey,
    to_a1: bool,
) -> Result<String, AutoError> {
    use calc_fml::reference::ReferenceNotation::{A1, R1C1};
    let (from, to) = if to_a1 { (R1C1, A1) } else { (A1, R1C1) };
    let body = entry.strip_prefix('=').unwrap_or(entry);
    calc_engine::convert_formula_notation(root, body, at, from, to)
        .map(|text| format!("={text}"))
        .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))
}

/// A number as Excel's `Range.Formula` writes it: 15 significant digits, trailing zeros dropped, scientific
/// (`1.5E-10`, `1E+15`) from 1E+15 and below 1E-9.
fn number_text_15(n: f64) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    let scientific = format!("{:.14e}", n.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("a rust exponent");
    let exponent: i32 = exponent.parse().expect("an exponent");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if n < 0.0 { "-" } else { "" };
    if !(-9..15).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        return format!(
            "{sign}{first}{fraction}E{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
    }
    if exponent >= 0 {
        let integer_digits = exponent as usize + 1;
        let padded = format!("{digits:0<integer_digits$}");
        let (integer, fraction) = padded.split_at(integer_digits);
        if fraction.is_empty() {
            format!("{sign}{integer}")
        } else {
            format!("{sign}{integer}.{fraction}")
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    }
}

/// A formula as Excel hands it back (dc-piw0.10 Q16): function names and references in capitals, no spaces
/// around `:`, everything else (spaces inside parentheses, text, quoted names) as typed.
fn normalise_formula(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut quote) = (0, None::<char>);
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            ':' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(':');
                i += 1;
                while chars.get(i) == Some(&' ') {
                    i += 1;
                }
            }
            c if c.is_ascii_alphabetic() || c == '$' || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '$' | '_' | '.'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let reference = word.trim_start_matches('$');
                let letters = reference.chars().take_while(char::is_ascii_alphabetic).count();
                let after = reference[letters..].trim_start_matches('$');
                let cell_like = (1..=3).contains(&letters)
                    && !after.is_empty()
                    && after.chars().all(|c| c.is_ascii_digit());
                let function = chars.get(i) == Some(&'(');
                out.push_str(&if cell_like || function { word.to_ascii_uppercase() } else { word });
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// General fitted to a column of `fits` characters: the decimals shrink until the number fits, then it goes
/// scientific, then it is as many `#` as fit.
fn fit_general(decimal: &str, n: f64, general: String, fits: usize) -> String {
    let count = |s: &str| s.chars().count();
    if count(&general) <= fits || fits == 0 {
        return general;
    }
    if n.abs() < 1e11 {
        for decimals in (0..=15usize).rev() {
            let mut text = format!("{n:.decimals$}");
            if text.contains('.') {
                text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
            }
            let text = text.replace('.', decimal);
            if count(&text) <= fits && !(text == "0" && n != 0.0) {
                return text;
            }
        }
    }
    for digits in (0..=14usize).rev() {
        let raw = format!("{:.digits$e}", n);
        let (mantissa, exponent) = raw.split_once('e').expect("a rust exponent");
        let exponent: i32 = exponent.parse().expect("an exponent");
        let mantissa = if mantissa.contains('.') {
            mantissa.trim_end_matches('0').trim_end_matches('.')
        } else {
            mantissa
        };
        let text = format!(
            "{}E{}{:02}",
            mantissa.replace('.', decimal),
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
        if count(&text) <= fits {
            return text;
        }
    }
    "#".repeat(fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbers_read_back_with_fifteen_significant_digits() {
        for (n, want) in [
            (10.0, "10"),
            (0.1, "0.1"),
            (1.0 / 3.0, "0.333333333333333"),
            (44927.0, "44927"),
            (1234567.891, "1234567.891"),
            (-2.5, "-2.5"),
            (1e15, "1E+15"),
            (123456789012345678.0, "1.23456789012346E+17"),
            (1.5e-10, "1.5E-10"),
            (0.00001, "0.00001"),
            (100000000000000.0, "100000000000000"),
        ] {
            assert_eq!(number_text_15(n), want, "{n}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn formulas_read_back_in_capitals_without_spaces_around_the_range_operator() {
        for (typed, want) in [
            ("=sum( a1 : a2 )", "=SUM( A1:A2 )"),
            ("=A1*2", "=A1*2"),
            ("=sum($a$1:b2)+x", "=SUM($A$1:B2)+x"),
            ("=\"a b : c\"&a1", "=\"a b : c\"&A1"),
            ("='My Sheet'!a1", "='My Sheet'!A1"),
        ] {
            assert_eq!(normalise_formula(typed), want, "{typed}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn general_shrinks_to_the_column_then_goes_scientific_then_hashes() {
        let third = 1.0 / 3.0;
        assert_eq!(fit_general(".", third, "0.333333333".into(), 10), "0.33333333");
        assert_eq!(fit_general(".", third, "0.333333333".into(), 3), "0.3");
        assert_eq!(fit_general(".", 10.0, "10".into(), 8), "10");
        assert_eq!(fit_general(".", 123456789012.0, "123456789012".into(), 8), "1.23E+11");
        assert_eq!(fit_general(".", 123456789012.0, "123456789012".into(), 2), "##");
    }
}
