//! The number-format oracle: the text Excel displays for a value under a number format, by locale
//! and date system, replayed through the formatter the result panel uses
//! ([`dna_formula_ui::result::formatted_view`]'s [`calc_func::cell_text_localized`]).
//!
//! `tests/fixtures/formats/cases.tsv` holds the questions (`make-cases.py` writes it) and
//! `excel-formats.tsv` Excel's answers, captured by `tools/excel-session/formats-job.ps1` in an
//! owned Excel session: `Range.Text` of a cell holding the value under `NumberFormatLocal`, with
//! the culture's separators set on the Excel instance and the culture's `[$-LCID]` prefix for day
//! and month names. A row Excel itself refused (an error column) is not an answer and is counted
//! apart. Every other row is compared: a formatter that gives a different text, or refuses, fails
//! the row; the test reports matched against mismatched plus refused over all answered rows and
//! asserts the counts it was last measured at, so a regression and an improvement both show.

use calc_func::locale::DateSystem;
use calc_func::value::{CalcValue, ExcelText};
use calc_fml::FormatProfile;

const CASES: &str = include_str!("../tests/fixtures/formats/cases.tsv");
const ANSWERS: &str = include_str!("../tests/fixtures/formats/excel-formats.tsv");

/// `U+NNNN` in a separator column or a code is that character.
fn separator(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("U+") {
        let hex = rest.get(at + 2..at + 6).and_then(|hex| u32::from_str_radix(hex, 16).ok());
        match hex.and_then(char::from_u32) {
            Some(c) => {
                out.push_str(&rest[..at]);
                out.push(c);
                rest = &rest[at + 6..];
            }
            None => {
                out.push_str(&rest[..at + 2]);
                rest = &rest[at + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of the answer column: `\uXXXX` is that UTF-16 code unit.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 5 < chars.len() + 0 && chars[i + 1] == 'u' {
            let hex: String = chars[i + 2..i + 6].iter().collect();
            out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
            i += 6;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// One answered row the formatter did not reproduce.
#[derive(Debug)]
pub(crate) struct Difference {
    pub line: String,
    pub culture: String,
    pub code: String,
}

/// What the replay found: the answered rows by outcome.
#[derive(Debug, Default)]
pub(crate) struct Replay {
    pub matched: usize,
    pub mismatched: Vec<Difference>,
    pub refused: Vec<Difference>,
    /// Rows Excel did not answer (it refused the code or the value).
    pub unanswered: usize,
}

pub(crate) fn replay() -> Replay {
    let questions: Vec<&str> = CASES.lines().filter(|l| !l.starts_with('#')).collect();
    let answers: Vec<&str> = ANSWERS.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(
        questions.len(),
        answers.len(),
        "every question has exactly one answer"
    );
    let mut result = Replay::default();
    for (question, answer) in questions.iter().zip(&answers) {
        let q: Vec<&str> = question.split('\t').collect();
        let a: Vec<&str> = answer.split('\t').collect();
        assert_eq!(q[0], a[0], "the answer follows its question");
        if !a.get(11).copied().unwrap_or("").is_empty() {
            result.unanswered += 1;
            continue;
        }
        let (culture, decimal, thousands, lcid, system, kind, value) =
            (q[1], separator(q[2]), separator(q[3]), q[4], q[5], q[6], q[7]);
        let code = separator(q[8]);
        let profile = FormatProfile::for_culture(culture)
            .and_then(|p| p.with_separators(&decimal, &thousands))
            .unwrap_or_else(|| panic!("{culture}: a profile with separators {decimal:?} {thousands:?}"));
        let date_system = if system == "1904" {
            DateSystem::System1904
        } else {
            DateSystem::System1900
        };
        let value = if kind == "text" {
            CalcValue::text(ExcelText::from_utf16_code_units(value.encode_utf16().collect()))
        } else {
            CalcValue::number(value.parse::<f64>().expect("a number"))
        };
        let asked = if lcid.is_empty() {
            code.clone()
        } else {
            format!("[$-{lcid}]{code}")
        };
        let expected = unescape(a[9]);
        let differ = |how: String| Difference {
            line: format!("{} {culture} {system} {kind} {:?} {code:?}: Excel {expected:?}, {how}", q[0], q[7]),
            culture: culture.to_owned(),
            code: code.clone(),
        };
        match calc_func::cell_text_localized(&profile, date_system, &value, &asked) {
            Ok(text) if text == expected => result.matched += 1,
            Ok(text) => result.mismatched.push(differ(format!("here {text:?}"))),
            Err(error) => result.refused.push(differ(format!("here {error:?}"))),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the rows Excel answered and the formatter did not come to, by cause. A cause is a
    /// property of the question, so a row that differs for another reason fails the test.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_result_panels_formatter_replays_excels_displayed_text() {
        let replay = replay();
        let answered = replay.matched + replay.mismatched.len() + replay.refused.len();
        // The Excel instance takes only the separators and the language of day and month names
        // from the culture. A non-breaking space in a local code is a literal there, and a `/`
        // keeps the instance's own date separator; in a culture whose date separator differs
        // (de-DE) the page's formatter, which converts it, cannot be compared with that.
        let instance_limit = |d: &Difference| {
            d.code.contains('\u{a0}')
                || d.code.contains('\u{202f}')
                || (d.culture == "de-DE" && d.code.contains('/'))
        };
        let (limits, real): (Vec<_>, Vec<_>) = replay.mismatched.iter().partition(|d| instance_limit(d));
        let (fractions, other_refused): (Vec<_>, Vec<_>) =
            replay.refused.iter().partition(|d| d.code.contains("?/"));
        eprintln!(
            "number formats: {answered} answered by Excel ({} not answered); matched {}, mismatched {} \
             ({} from the Excel instance's limits), refused {} ({} fractions)",
            replay.unanswered,
            replay.matched,
            replay.mismatched.len(),
            limits.len(),
            replay.refused.len(),
            fractions.len(),
        );
        for d in real.iter().chain(&other_refused) {
            eprintln!("  {}", d.line);
        }
        assert!(real.is_empty(), "{} rows differ for no known cause", real.len());
        assert!(other_refused.is_empty(), "{} rows are refused for no known cause", other_refused.len());
        assert_eq!(
            (replay.matched, limits.len(), fractions.len(), replay.unanswered),
            EXPECTED,
            "the measured counts moved: update EXPECTED with the improvement or fix the regression"
        );
    }

    /// matched, mismatched because of the instance's limits, refused as fractions (not modelled in
    /// calc-func), and not answered by Excel: as last measured.
    const EXPECTED: (usize, usize, usize, usize) = (1669, 61, 48, 32);
}
