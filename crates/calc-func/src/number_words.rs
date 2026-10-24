//! The words Excel's number, date and time recognition reads inside number text, as measured, and
//! the verdict they give: text holding any other word is not a number.
//!
//! A word here is a maximal run of ASCII letters. The rule takes these to be the only words Excel
//! reads inside number text, ASCII case ignored:
//!
//! - a month name or any prefix of one of at least three letters (`jan`, `sept`, `january`), in a
//!   date such as `1 jan 2020`;
//! - `A`, `P`, `AM` or `PM` after a time (`10:30 a`, `10:30 pm`);
//! - `E` between a mantissa and its exponent (`1e5`);
//! - `R`, the currency symbol of the Windows locale en-ZA, before the amount (`R1`), which Excel
//!   reads in upper case only (`r1` is #VALUE!); a lower-case r is left undecided all the same.
//!
//! So the rule finds that text holding any other word, such as the labels `2016-17 Winter` and
//! `Q1 2016`, is not a number: read as a number it is #VALUE!, it meets no number criterion under
//! =, and as a criterion it asks for text. Text holding only these words, or no word, is not
//! decided here: whether Excel reads it as a number (a date, a time, a fraction, a currency
//! amount, grouped digits) is dc-uar's.
//!
//! The verdict is given only for text of printable ASCII (U+0020 to U+007E). Any other character
//! leaves the text undecided: Excel may read a full-width letter or digit as its ASCII form, or
//! pass over a character such as a zero-width space, and so join two runs into a word it reads.
//!
//! The rule was measured in Excel 16.0 (build 20430) under the Windows locale en-ZA (Excel's
//! country code 1, its country setting 27, date order year-month-day); en-US itself was not
//! measured, and calc_func's context carries no locale. Its committed evidence is
//! dc_e00t__excel_comparison, which compares calc_func's results with Excel's capture of the
//! committed oracle workbook through calc_verdict: the hand-written rows (labels as operands, in
//! comparisons, as criteria and as range cells; text written like a date or a time that holds such
//! a word, such as `2020-01-02T10:30`, `10:30 a.m.`, `1st January 2020` and `10:30Z`; and forms
//! left undecided), and 1,056 generated texts, 88 words in 12 number, date and time forms, as
//! text+0, of which Excel gives #VALUE! for every one the rule decides; and Excel's VALUE of 36 of
//! them, three words in each form, equal to their text+0.

/// The month names, any prefix of which of at least [`MONTH_PREFIX`] letters Excel reads in a date.
const MONTHS: [&[u8]; 12] = [
    b"JANUARY",
    b"FEBRUARY",
    b"MARCH",
    b"APRIL",
    b"MAY",
    b"JUNE",
    b"JULY",
    b"AUGUST",
    b"SEPTEMBER",
    b"OCTOBER",
    b"NOVEMBER",
    b"DECEMBER",
];

/// The shortest prefix of a month name that Excel reads.
const MONTH_PREFIX: usize = 3;

/// The other words Excel reads inside number text: the time markers, the exponent mark and the
/// currency symbol R.
const MARKERS: [&[u8]; 6] = [b"A", b"P", b"AM", b"PM", b"E", b"R"];

/// Whether `units`, as UTF-16 code units, is text that Excel's number, date and time recognition
/// cannot read as a number: printable ASCII holding a word that none of its forms reads (see the
/// module documentation). False when the text is undecided, not when it is a number.
pub(crate) fn cannot_be_number(units: &[u16]) -> bool {
    if !units.iter().all(|&unit| (0x20..=0x7E).contains(&unit)) {
        return false;
    }
    let mut at = 0;
    while at < units.len() {
        if !is_letter(units[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < units.len() && is_letter(units[at]) {
            at += 1;
        }
        if !read_inside_number(&units[start..at]) {
            return true;
        }
    }
    false
}

fn is_letter(unit: u16) -> bool {
    u8::try_from(unit).is_ok_and(|byte| byte.is_ascii_alphabetic())
}

/// Whether the ASCII letters `word` are one of the words of the module documentation, ASCII case
/// ignored, which Excel may read inside some number text.
fn read_inside_number(word: &[u16]) -> bool {
    let spells = |upper: &[u8]| {
        word.len() == upper.len()
            && word.iter().zip(upper).all(|(&unit, &letter)| {
                u8::try_from(unit).is_ok_and(|byte| byte.to_ascii_uppercase() == letter)
            })
    };
    MARKERS.iter().any(|marker| spells(marker))
        || (word.len() >= MONTH_PREFIX
            && MONTHS
                .iter()
                .any(|month| word.len() <= month.len() && spells(&month[..word.len()])))
}

#[cfg(test)]
mod tests {
    use super::cannot_be_number;

    fn decided(text: &str) -> bool {
        cannot_be_number(&text.encode_utf16().collect::<Vec<u16>>())
    }

    #[test]
    fn a_word_no_number_text_holds_decides_the_text() {
        for text in [
            "2016-17 Winter",
            "Q1 2016",
            "2016-17 winter",
            "1st January 2020",
            "2020-01-02T10:30",
            "10:30 a.m.",
            "10:30Z",
            "1 Janx 2020",
            "Monday, 1 January 2020",
            "Ja 1",
            "r1 x",
        ] {
            assert!(decided(text), "{text:?} is decided");
        }
    }

    #[test]
    fn text_holding_only_words_excel_reads_is_undecided() {
        for text in [
            "",
            "2016",
            "2016-17",
            "1/2",
            "1 1/2",
            "1 Jan 2020",
            "1 JANUARY 2020",
            "1 sept 2020",
            "10:30 am",
            "10:30 P",
            "1e5",
            "R1",
            "r1",
            "(1)",
            "1 000",
        ] {
            assert!(!decided(text), "{text:?} is undecided");
        }
    }

    #[test]
    fn text_beyond_printable_ascii_is_undecided() {
        for text in [
            "2016\u{2013}17 Winter",
            "2016-17 Winter\u{a0}",
            "\u{ff11} Winter",
            "Q1\u{200b} 2016",
            "2016-17 Winter\t",
        ] {
            assert!(!decided(text), "{text:?} is undecided");
        }
    }
}
