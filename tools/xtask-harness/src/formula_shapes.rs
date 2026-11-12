//! Argument shapes of function calls, read lexically from a formula's text (bead dc-ztvv).
//!
//! A shape key is `NAME(tok,tok,...)`: one token per argument, in Touchstone's vocabulary
//! (`tools/excel-service/touchstone/PROTOCOL.md`) where the text itself says the type, and with
//! reference tokens where the argument is a cell or range, because a call whose argument is a
//! reference is a different Excel question from the same call with a literal (a reference keeps
//! blank and text cells apart, and position, hidden rows and implicit intersection count).
//!
//! | token | argument |
//! |---|---|
//! | `n` | number literal |
//! | `S` | text literal |
//! | `B` | `TRUE` or `FALSE` |
//! | `e` | error literal (`#N/A`) |
//! | `z` | omitted (an empty argument) |
//! | `aRxC` | array constant of R rows and C columns |
//! | `r` | one cell reference (also `Sheet!A1`, `[1]Sheet!A1`, `$A$1`) |
//! | `R` | a range, whole row or column, or a 3-D reference |
//! | `N` | a name (defined name or table name) |
//! | `T` | a structured reference (`Table[Column]`) |
//! | `f` | the result of another function call |
//! | `x` | any other expression (operators, parentheses, unions) |
//!
//! The Touchstone tokens `s` (text through a cell) and `b` (boolean through a cell) and the
//! type of a cell a reference reads are not known from the text: they need the cell's value, and
//! `r`/`R` stand for them. A function listed in [`POSITION_SENSITIVE`] gets a trailing `!`:
//! its answer depends on where the formula sits or on what a reference is (row, column, sheet,
//! hidden rows, formula text), so a capture of it has to run on the file route, never the call
//! route.

/// Functions whose answer depends on the formula's position or on the reference itself.
pub(crate) const POSITION_SENSITIVE: &[&str] = &[
    "ROW", "COLUMN", "ROWS", "COLUMNS", "CELL", "OFFSET", "INDIRECT", "ADDRESS", "AREAS",
    "FORMULATEXT", "ISFORMULA", "ISREF", "SHEET", "SHEETS", "SUBTOTAL", "AGGREGATE", "INFO",
    "GETPIVOTDATA", "HYPERLINK", "INDEX", "RTD",
];

/// At most this many argument tokens are written; more end in `+`.
const MAX_TOKENS: usize = 10;

fn is_cell_part(s: &str) -> bool {
    // A1, $A$1, AB12: letters then digits, with optional dollars.
    let s: String = s.chars().filter(|c| *c != '$').collect();
    let letters = s.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = s.len() - letters;
    letters >= 1
        && letters <= 3
        && digits >= 1
        && s[letters..].chars().all(|c| c.is_ascii_digit())
}

fn is_axis_part(s: &str) -> bool {
    // A whole column (`A`, `$A`) or whole row (`1`, `$1`).
    let s: String = s.chars().filter(|c| *c != '$').collect();
    !s.is_empty()
        && (s.chars().all(|c| c.is_ascii_digit())
            || (s.len() <= 3 && s.chars().all(|c| c.is_ascii_alphabetic())))
}

/// The address after a sheet prefix: `A1` is one cell, `A1:B2`, `A:A` and `1:1` ranges.
fn address_token(addr: &str) -> Option<&'static str> {
    match addr.split_once(':') {
        None if is_cell_part(addr) => Some("r"),
        Some((a, b)) if is_cell_part(a) && is_cell_part(b) => Some("R"),
        Some((a, b)) if is_axis_part(a) && is_axis_part(b) => Some("R"),
        _ => None,
    }
}

const ERRORS: &[&str] = &[
    "#N/A", "#REF!", "#DIV/0!", "#VALUE!", "#NAME?", "#NUM!", "#NULL!", "#SPILL!", "#CALC!",
    "#GETTING_DATA",
];

/// The token of one argument's text.
fn token(arg: &[char]) -> String {
    let text: String = arg.iter().collect();
    let text = text.trim();
    if text.is_empty() {
        return "z".into();
    }
    if ERRORS.contains(&text.to_ascii_uppercase().as_str()) {
        return "e".into();
    }
    let chars: Vec<char> = text.chars().collect();
    // Walk the text once at the top level: operators, whether it is one string, one array
    // constant or one call.
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut top_operator = false;
    let mut top_colon = false;
    let mut last_close_at_end = false;
    let mut first_group_closes_at_end = false;
    let mut seen_group = false;
    let mut brackets = 0i32;
    let mut only_leading_signs = true;
    let mut space_run = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                if depth == 0 && brackets == 0 && i + 1 < chars.len() {
                    top_operator = true; // text followed by more: "a"&B1
                }
            }
            '\'' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            '(' | '{' => {
                depth += 1;
            }
            ')' | '}' => {
                depth -= 1;
                if depth == 0 {
                    last_close_at_end = i + 1 == chars.len();
                    if !seen_group {
                        first_group_closes_at_end = last_close_at_end;
                        seen_group = true;
                    }
                }
            }
            '[' => brackets += 1,
            ']' => brackets -= 1,
            ':' if depth == 0 && brackets == 0 => top_colon = true,
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | '%'
                if depth == 0 && brackets == 0 =>
            {
                let leading_sign = matches!(c, '+' | '-') && only_leading_signs;
                let exponent_sign = matches!(c, '+' | '-')
                    && i >= 2
                    && matches!(chars[i - 1], 'e' | 'E')
                    && chars[i - 2].is_ascii_digit();
                // `Sheet-1!A1` is quoted, so an unquoted operator is a real one.
                if !leading_sign && !exponent_sign {
                    top_operator = true;
                }
            }
            ' ' if depth == 0 && brackets == 0 => {
                // The intersection operator: `A1:B2 B1:C3`; trailing spaces were trimmed.
                space_run = true;
            }
            ',' | ';' if depth == 0 && brackets == 0 => top_operator = true,
            _ => {}
        }
        if c != ' ' && space_run {
            if !matches!(c, '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | '%') {
                top_operator = true;
            }
            space_run = false;
        }
        only_leading_signs &= matches!(c, ' ' | '+' | '-');
        i += 1;
    }
    if top_operator {
        return "x".into();
    }
    if chars[0] == '"' && chars[chars.len() - 1] == '"' && chars.len() >= 2 {
        return "S".into();
    }
    if chars[0] == '{' && first_group_closes_at_end {
        let inner = &text[1..text.len() - 1];
        let rows = inner.split(';').count();
        let cols = inner
            .split(';')
            .next()
            .map_or(1, |row| row.split(',').count());
        return format!("a{rows}x{cols}");
    }
    if chars[0] == '(' && first_group_closes_at_end {
        return "x".into();
    }
    let upper = text.to_ascii_uppercase();
    if upper == "TRUE" || upper == "FALSE" {
        return "B".into();
    }
    if ERRORS.contains(&upper.as_str()) {
        return "e".into();
    }
    let unsigned = text.trim_start_matches(['+', '-']).trim();
    if !unsigned.is_empty()
        && unsigned.parse::<f64>().is_ok()
        && unsigned
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
    {
        return "n".into();
    }
    if let Some(open) = text.find('(') {
        // name(...) at the end of the text; possibly with a colon range of two calls.
        let head = &text[..open];
        if top_colon {
            return "R".into(); // INDEX(...):INDEX(...)
        }
        if last_close_at_end
            && !head.is_empty()
            && head
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\'))
        {
            return "f".into();
        }
        if top_colon {
            return "R".into(); // INDEX(...):INDEX(...)
        }
        return "x".into();
    }
    if brackets_balanced_structured(text) {
        return "T".into();
    }
    // A reference: optional sheet prefix, then the address.
    let (sheet, address) = match text.rfind('!') {
        Some(bang) => (Some(&text[..bang]), &text[bang + 1..]),
        None => (None, text),
    };
    if let Some(token) = address_token(address) {
        if sheet.is_some_and(|s| s.contains(':') && !s.starts_with('\''))
            || sheet.is_some_and(|s| s.trim_matches('\'').contains(':'))
        {
            return "R".into();
        }
        return token.into();
    }
    if top_colon {
        return "R".into();
    }
    if text
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '!' | '\'' | ' ' | '$'))
    {
        return "N".into();
    }
    "x".into()
}

fn brackets_balanced_structured(text: &str) -> bool {
    // `Table1[Col]`, `Table1[[#This Row],[Col]]`, `[@Col]`.
    text.contains('[') && text.ends_with(']') && !text.starts_with('[') || text.starts_with("[@")
}

/// One delimiter pass for all calls. Argument tokenisation is charged against a linear
/// work allowance before reading each slice, so deeply nested expressions cannot cause
/// quadratic rescans. The retained argument list is capped even for a million commas.
pub(crate) fn call_shapes(
    chars: &[char],
    calls: &[(usize, String)],
) -> Result<std::collections::BTreeMap<String, u64>, crate::classify::ScanRefusal> {
    use crate::classify::ScanRefusal;
    struct Frame<'a> {
        delimiter: char,
        name: Option<&'a str>,
        start: usize,
        args: Vec<(usize, usize)>,
        count: usize,
    }
    impl Frame<'_> {
        fn argument(&mut self, end: usize) {
            self.count += 1;
            if self.args.len() < MAX_TOKENS {
                self.args.push((self.start, end));
            }
            self.start = end + 1;
        }
    }
    let mut out = std::collections::BTreeMap::new();
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut calls = calls.iter().peekable();
    let mut work = chars.len().saturating_mul(16).saturating_add(1024);
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' | '"' => {
                let quote = chars[i];
                i += 1;
                loop {
                    if i >= chars.len() {
                        return Err(ScanRefusal::MalformedFormula);
                    }
                    if chars[i] == quote {
                        if chars.get(i + 1) == Some(&quote) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            '(' | '{' | '[' => {
                let name = if calls.peek().is_some_and(|(at, _)| *at == i) {
                    calls.next().map(|(_, name)| name.as_str())
                } else {
                    None
                };
                stack.push(Frame {
                    delimiter: chars[i],
                    name,
                    start: i + 1,
                    args: Vec::new(),
                    count: 0,
                });
            }
            ')' | '}' | ']' => {
                let mut frame = stack.pop().ok_or(ScanRefusal::MalformedFormula)?;
                let expected = match chars[i] {
                    ')' => '(',
                    '}' => '{',
                    _ => '[',
                };
                if frame.delimiter != expected {
                    return Err(ScanRefusal::MalformedFormula);
                }
                if let Some(name) = frame.name {
                    frame.argument(i);
                    // An empty call has no arguments; a comma still supplies omitted arguments.
                    if frame.count == 1
                        && chars[frame.args[0].0..i].iter().all(|c| c.is_whitespace())
                    {
                        frame.args.clear();
                        frame.count = 0;
                    }
                    let mut tokens = Vec::new();
                    for (start, end) in frame.args {
                        work = work
                            .checked_sub(end - start + 1)
                            .ok_or(ScanRefusal::WorkBudget)?;
                        tokens.push(token(&chars[start..end]));
                    }
                    if frame.count > MAX_TOKENS {
                        tokens.push("+".into());
                    }
                    let mark = if POSITION_SENSITIVE.contains(&name) {
                        "!"
                    } else {
                        ""
                    };
                    *out.entry(format!("{name}({}){mark}", tokens.join(",")))
                        .or_default() += 1;
                }
            }
            ',' if stack.last().is_some_and(|f| f.name.is_some()) => {
                stack.last_mut().expect("a call").argument(i);
            }
            _ => {}
        }
        i += 1;
    }
    if !stack.is_empty() {
        return Err(ScanRefusal::MalformedFormula);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    fn shape(formula: &str, name: &str) -> String {
        crate::classify::analyze_formula(formula)
            .unwrap()
            .shapes
            .keys()
            .find(|key| key.starts_with(&format!("{name}(")))
            .unwrap()
            .clone()
    }

    #[test]
    fn literals_references_and_calls_have_their_own_tokens() {
        assert_eq!(shape("SUM(A1:B2)", "SUM"), "SUM(R)");
        assert_eq!(shape("SUM(A1,2,-3.5,1E-3)", "SUM"), "SUM(r,n,n,n)");
        assert_eq!(shape(r#"IF(A1>0,"yes",B$2)"#, "IF"), "IF(x,S,r)");
        assert_eq!(shape("IF(TRUE,#N/A,)", "IF"), "IF(B,e,z)");
        assert_eq!(
            shape("MAX(Sheet1!A:A,'My Sheet'!$B$2,Total)", "MAX"),
            "MAX(R,r,N)"
        );
        assert_eq!(
            shape("SUM('Jan:Dec'!A1,Rate*2,ROUND(A1,2))", "SUM"),
            "SUM(R,x,f)"
        );
        assert_eq!(shape("SUM(Table1[Qty],[@Qty])", "SUM"), "SUM(T,T)");
        assert_eq!(shape("SUM({1,2,3;4,5,6})", "SUM"), "SUM(a2x3)");
        assert_eq!(
            shape(r#"VLOOKUP("a"&B1,A1:C9,2,FALSE)"#, "VLOOKUP"),
            "VLOOKUP(x,R,n,B)"
        );
        assert_eq!(shape("SUM([1]Data!A1:A9)", "SUM"), "SUM(R)");
        assert_eq!(shape("SUM(INDEX(A:A,1):INDEX(A:A,5))", "SUM"), "SUM(R)");
    }

    #[test]
    fn position_sensitive_functions_are_marked_and_empty_calls_have_no_token() {
        assert_eq!(shape("ROW()", "ROW"), "ROW()!");
        assert_eq!(shape("ROW(A5)", "ROW"), "ROW(r)!");
        assert_eq!(shape("NOW()", "NOW"), "NOW()");
        assert_eq!(shape("IF(,)", "IF"), "IF(z,z)");
    }

    #[test]
    fn long_argument_lists_are_cut() {
        let formula = "SUM(1,2,3,4,5,6,7,8,9,10,11,12)";
        assert_eq!(shape(formula, "SUM"), "SUM(n,n,n,n,n,n,n,n,n,n,+)");
    }
}
