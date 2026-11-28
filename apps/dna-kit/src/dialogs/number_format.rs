//! The Number tab of Format Cells: Excel's categories and the format codes their options make.
//!
//! The codes are the ones Excel for Windows (en-US) writes for each choice. A code that no
//! choice composes belongs to Custom, which is also where every code can be read and edited.

pub const GENERAL: &str = "General";
pub const NUMBER: &str = "Number";
pub const CURRENCY: &str = "Currency";
pub const ACCOUNTING: &str = "Accounting";
pub const DATE: &str = "Date";
pub const TIME: &str = "Time";
pub const PERCENTAGE: &str = "Percentage";
pub const FRACTION: &str = "Fraction";
pub const SCIENTIFIC: &str = "Scientific";
pub const TEXT: &str = "Text";
pub const SPECIAL: &str = "Special";
pub const CUSTOM: &str = "Custom";

/// The Category list, in Excel's order.
pub const CATEGORIES: [&str; 12] = [
    GENERAL, NUMBER, CURRENCY, ACCOUNTING, DATE, TIME, PERCENTAGE, FRACTION, SCIENTIFIC, TEXT, SPECIAL, CUSTOM,
];

/// The Negative numbers list: a minus sign, red, parentheses, red parentheses.
pub const NEGATIVES: [&str; 4] = ["minus", "red", "parentheses", "red-parentheses"];

/// The Date category's Type list: the code and the entry Excel shows for 14 March 2012 13:30.
pub const DATE_TYPES: [(&str, &str); 17] = [
    ("m/d/yyyy", "*3/14/2012"),
    ("[$-F800]dddd, mmmm dd, yyyy", "*Wednesday, March 14, 2012"),
    ("m/d;@", "3/14"),
    ("m/d/yy;@", "3/14/12"),
    ("mm/dd/yy;@", "03/14/12"),
    ("[$-409]d-mmm;@", "14-Mar"),
    ("[$-409]d-mmm-yy;@", "14-Mar-12"),
    ("[$-409]dd-mmm-yy;@", "14-Mar-12 (two-digit day)"),
    ("[$-409]mmm-yy;@", "Mar-12"),
    ("[$-409]mmmm-yy;@", "March-12"),
    ("[$-409]mmmm d, yyyy;@", "March 14, 2012"),
    ("[$-409]m/d/yy h:mm AM/PM;@", "3/14/12 1:30 PM"),
    ("m/d/yy h:mm;@", "3/14/12 13:30"),
    ("[$-409]mmmmm;@", "M"),
    ("[$-409]mmmmm-yy;@", "M-12"),
    ("m/d/yyyy;@", "3/14/2012"),
    ("[$-409]d-mmm-yyyy;@", "14-Mar-2012"),
];

/// The Time category's Type list.
pub const TIME_TYPES: [(&str, &str); 9] = [
    ("[$-F400]h:mm:ss AM/PM", "*1:30:55 PM"),
    ("h:mm;@", "13:30"),
    ("[$-409]h:mm AM/PM;@", "1:30 PM"),
    ("h:mm:ss;@", "13:30:55"),
    ("[$-409]h:mm:ss AM/PM;@", "1:30:55 PM"),
    ("mm:ss.0;@", "30:55.2"),
    ("[h]:mm:ss;@", "37:30:55"),
    ("[$-409]m/d/yy h:mm AM/PM;@", "3/14/12 1:30 PM"),
    ("m/d/yy h:mm;@", "3/14/12 13:30"),
];

pub const FRACTION_TYPES: [(&str, &str); 9] = [
    ("# ?/?", "Up to one digit (1/4)"),
    ("# ??/??", "Up to two digits (21/25)"),
    ("# ???/???", "Up to three digits (312/943)"),
    ("# ?/2", "As halves (1/2)"),
    ("# ?/4", "As quarters (2/4)"),
    ("# ?/8", "As eighths (4/8)"),
    ("# ??/16", "As sixteenths (8/16)"),
    ("# ?/10", "As tenths (3/10)"),
    ("# ??/100", "As hundredths (30/100)"),
];

pub const SPECIAL_TYPES: [(&str, &str); 4] = [
    ("00000", "Zip Code"),
    ("00000-0000", "Zip Code + 4"),
    ("[<=9999999]###-####;(###) ###-####", "Phone Number"),
    ("000-00-0000", "Social Security Number"),
];

/// The codes the Custom category lists before the workbook's own.
pub const CUSTOM_CODES: [&str; 36] = [
    "General",
    "0",
    "0.00",
    "#,##0",
    "#,##0.00",
    "#,##0_);(#,##0)",
    "#,##0_);[Red](#,##0)",
    "#,##0.00_);(#,##0.00)",
    "#,##0.00_);[Red](#,##0.00)",
    "$#,##0_);($#,##0)",
    "$#,##0_);[Red]($#,##0)",
    "$#,##0.00_);($#,##0.00)",
    "$#,##0.00_);[Red]($#,##0.00)",
    "0%",
    "0.00%",
    "0.00E+00",
    "##0.0E+0",
    "# ?/?",
    "# ??/??",
    "m/d/yyyy",
    "d-mmm-yy",
    "d-mmm",
    "mmm-yy",
    "h:mm AM/PM",
    "h:mm:ss AM/PM",
    "h:mm",
    "h:mm:ss",
    "m/d/yyyy h:mm",
    "mm:ss",
    "mm:ss.0",
    "@",
    "[h]:mm:ss",
    "_($* #,##0_);_($* (#,##0);_($* \"-\"_);_(@_)",
    "_(* #,##0_);_(* (#,##0);_(* \"-\"_);_(@_)",
    "_($* #,##0.00_);_($* (#,##0.00);_($* \"-\"??_);_(@_)",
    "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)",
];

/// The types a category lists, or none where the category has no Type list.
pub fn types(category: &str) -> &'static [(&'static str, &'static str)] {
    match category {
        DATE => &DATE_TYPES,
        TIME => &TIME_TYPES,
        FRACTION => &FRACTION_TYPES,
        SPECIAL => &SPECIAL_TYPES,
        _ => &[],
    }
}

/// What the Number tab holds for one format code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choices {
    pub category: &'static str,
    /// Decimal places, 0 to 30.
    pub decimals: u8,
    /// Use 1000 Separator (,).
    pub separator: bool,
    /// The Symbol box holds `$`; otherwise None.
    pub dollar: bool,
    /// An entry of [`NEGATIVES`].
    pub negative: &'static str,
    /// The code selected in the category's Type list.
    pub kind: Option<&'static str>,
}

impl Choices {
    /// What Excel shows when a category is first chosen.
    pub fn defaults(category: &'static str) -> Self {
        Self {
            category,
            decimals: if category == GENERAL || category == TEXT { 0 } else { 2 },
            separator: false,
            dollar: matches!(category, CURRENCY | ACCOUNTING),
            negative: NEGATIVES[0],
            kind: types(category).first().map(|(code, _)| *code),
        }
    }

    /// The format code these choices make, or `None` in Custom, whose code is typed.
    pub fn code(&self) -> Option<String> {
        let places = |pad: &str| {
            if self.decimals == 0 { String::new() } else { format!(".{}", pad.repeat(usize::from(self.decimals))) }
        };
        let signed = |positive: String| match self.negative {
            "red" => format!("{positive};[Red]{positive}"),
            "parentheses" => format!("{positive}_);({positive})"),
            "red-parentheses" => format!("{positive}_);[Red]({positive})"),
            _ => positive,
        };
        Some(match self.category {
            GENERAL => "General".into(),
            NUMBER => signed(format!("{}{}", if self.separator { "#,##0" } else { "0" }, places("0"))),
            CURRENCY => signed(format!("{}#,##0{}", if self.dollar { "$" } else { "" }, places("0"))),
            ACCOUNTING => {
                let symbol = if self.dollar { "$" } else { "" };
                let digits = format!("#,##0{}", places("0"));
                let zero = if self.decimals == 0 { String::new() } else { "?".repeat(usize::from(self.decimals)) };
                format!("_({symbol}* {digits}_);_({symbol}* ({digits});_({symbol}* \"-\"{zero}_);_(@_)")
            }
            PERCENTAGE => format!("0{}%", places("0")),
            SCIENTIFIC => format!("0{}E+00", places("0")),
            TEXT => "@".into(),
            DATE | TIME | FRACTION | SPECIAL => self.kind?.to_owned(),
            _ => return None,
        })
    }

    /// The Number tab for an existing code: the category whose choices make it, else Custom.
    pub fn of(code: &str) -> Self {
        if code.eq_ignore_ascii_case("General") {
            return Self::defaults(GENERAL);
        }
        if code == "@" {
            return Self::defaults(TEXT);
        }
        for category in [NUMBER, CURRENCY, ACCOUNTING, PERCENTAGE, SCIENTIFIC] {
            let signs: &[&'static str] = if matches!(category, NUMBER | CURRENCY) { &NEGATIVES } else { &NEGATIVES[..1] };
            let separators: &[bool] = if category == NUMBER { &[false, true] } else { &[false] };
            // Currency without a symbol is the Number category with a separator.
            let symbols: &[bool] = match category {
                CURRENCY => &[true],
                ACCOUNTING => &[true, false],
                _ => &[false],
            };
            for decimals in 0..=30 {
                for &separator in separators {
                    for &dollar in symbols {
                        for &negative in signs {
                            let choices = Self { category, decimals, separator, dollar, negative, kind: None };
                            if choices.code().as_deref() == Some(code) {
                                return choices;
                            }
                        }
                    }
                }
            }
        }
        for category in [DATE, TIME, FRACTION, SPECIAL] {
            if let Some((kind, _)) = types(category).iter().find(|(kind, _)| *kind == code) {
                return Self { kind: Some(kind), ..Self::defaults(category) };
            }
        }
        Self { kind: None, ..Self::defaults(CUSTOM) }
    }

    /// The Negative numbers list for these choices, as Excel writes 1234.10 in each form.
    pub fn negative_labels(&self) -> [String; 4] {
        let symbol = if self.dollar && self.category == CURRENCY { "$" } else { "" };
        let whole = if self.separator || self.category == CURRENCY { "1,234" } else { "1234" };
        let fraction: String = "10".chars().chain(std::iter::repeat('0')).take(usize::from(self.decimals)).collect();
        let number = if fraction.is_empty() { format!("{symbol}{whole}") } else { format!("{symbol}{whole}.{fraction}") };
        [
            format!("-{number}"),
            format!("{number} (red)"),
            format!("({number})"),
            format!("({number}) (red)"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn each_category_opens_with_excels_defaults_and_writes_excels_code() {
        let code = |category| Choices::defaults(category).code();
        assert_eq!(code(GENERAL).as_deref(), Some("General"));
        assert_eq!(code(NUMBER).as_deref(), Some("0.00"));
        assert_eq!(code(CURRENCY).as_deref(), Some("$#,##0.00"));
        assert_eq!(code(ACCOUNTING).as_deref(), Some(r#"_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)"#));
        assert_eq!(code(DATE).as_deref(), Some("m/d/yyyy"));
        assert_eq!(code(TIME).as_deref(), Some("[$-F400]h:mm:ss AM/PM"));
        assert_eq!(code(PERCENTAGE).as_deref(), Some("0.00%"));
        assert_eq!(code(FRACTION).as_deref(), Some("# ?/?"));
        assert_eq!(code(SCIENTIFIC).as_deref(), Some("0.00E+00"));
        assert_eq!(code(TEXT).as_deref(), Some("@"));
        assert_eq!(code(SPECIAL).as_deref(), Some("00000"));
        assert_eq!(code(CUSTOM), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn number_and_currency_options_compose_the_four_negative_forms() {
        let number = |decimals, separator, negative| {
            Choices { decimals, separator, negative, ..Choices::defaults(NUMBER) }.code().unwrap()
        };
        assert_eq!(number(0, false, "minus"), "0");
        assert_eq!(number(2, true, "minus"), "#,##0.00");
        assert_eq!(number(2, false, "red"), "0.00;[Red]0.00");
        assert_eq!(number(1, true, "parentheses"), "#,##0.0_);(#,##0.0)");
        assert_eq!(number(2, true, "red-parentheses"), "#,##0.00_);[Red](#,##0.00)");
        let currency = |decimals, dollar, negative| {
            Choices { decimals, dollar, negative, ..Choices::defaults(CURRENCY) }.code().unwrap()
        };
        assert_eq!(currency(0, true, "parentheses"), "$#,##0_);($#,##0)");
        assert_eq!(currency(2, true, "red-parentheses"), "$#,##0.00_);[Red]($#,##0.00)");
        assert_eq!(currency(2, false, "red"), "#,##0.00;[Red]#,##0.00");
        let accounting = |decimals, dollar| Choices { decimals, dollar, ..Choices::defaults(ACCOUNTING) }.code().unwrap();
        assert_eq!(accounting(0, true), r#"_($* #,##0_);_($* (#,##0);_($* "-"_);_(@_)"#);
        assert_eq!(accounting(2, false), r#"_(* #,##0.00_);_(* (#,##0.00);_(* "-"??_);_(@_)"#);
        assert_eq!(Choices { decimals: 0, ..Choices::defaults(PERCENTAGE) }.code().unwrap(), "0%");
        assert_eq!(Choices { decimals: 30, ..Choices::defaults(SCIENTIFIC) }.code().unwrap(), format!("0.{}E+00", "0".repeat(30)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_code_reads_back_as_the_choices_that_make_it_and_anything_else_is_custom() {
        for code in CUSTOM_CODES {
            let choices = Choices::of(code);
            if choices.category != CUSTOM {
                assert_eq!(choices.code().as_deref(), Some(code), "{code}");
            }
        }
        let read = |code| {
            let choices = Choices::of(code);
            (choices.category, choices.decimals, choices.separator, choices.dollar, choices.negative)
        };
        assert_eq!(read("general"), (GENERAL, 0, false, false, "minus"));
        assert_eq!(read("#,##0.000"), (NUMBER, 3, true, false, "minus"));
        assert_eq!(read("0.00_);[Red](0.00)"), (NUMBER, 2, false, false, "red-parentheses"));
        assert_eq!(read("$#,##0.00"), (CURRENCY, 2, false, true, "minus"));
        assert_eq!(read(r#"_(* #,##0_);_(* (#,##0);_(* "-"_);_(@_)"#), (ACCOUNTING, 0, false, false, "minus"));
        assert_eq!(read("0.0%"), (PERCENTAGE, 1, false, false, "minus"));
        assert_eq!(Choices::of("[$-409]d-mmm-yy;@").category, DATE);
        assert_eq!(Choices::of("[h]:mm:ss;@").kind, Some("[h]:mm:ss;@"));
        assert_eq!(Choices::of("# ??/16").category, FRACTION);
        assert_eq!(Choices::of("000-00-0000").category, SPECIAL);
        for custom in ["0.00 \"kg\"", "d-mmm-yy", "##0.0E+0", "yyyy-mm-dd", "[Blue]0;[Red]-0"] {
            assert_eq!(Choices::of(custom).category, CUSTOM, "{custom}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_negative_list_follows_the_decimals_separator_and_symbol() {
        let labels = Choices { decimals: 2, separator: true, ..Choices::defaults(NUMBER) }.negative_labels();
        assert_eq!(labels, ["-1,234.10", "1,234.10 (red)", "(1,234.10)", "(1,234.10) (red)"]);
        assert_eq!(Choices { decimals: 0, ..Choices::defaults(NUMBER) }.negative_labels()[0], "-1234");
        assert_eq!(Choices { decimals: 3, ..Choices::defaults(CURRENCY) }.negative_labels()[2], "($1,234.100)");
    }
}
