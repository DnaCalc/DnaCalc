//! Locale profiles and the date serial systems, ported from OxFunc
//! (crates/oxfunc_core/src/locale_format.rs at 6fca3518): the profile of each locale OxFunc names
//! (decimal and thousands separators, currency, the short date order and pattern), and the
//! conversion between a serial date number and a year, month and day in the 1900 and 1904 date
//! systems. The number-format engine that renders TEXT, FIXED and DOLLAR and reads VALUE is
//! [`crate::format`].
//!
//! A call reaches a profile and a date system through its services
//! ([`crate::EvalServices::with_environment`]); without one, a kernel that needs it is refused
//! as not modelled.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleId {
    EnUs,
    EnGb,
    EnIe,
    EnAu,
    EnNz,
    EnZa,
    EnIn,
    EnCa,
    EnPh,
    DeDe,
    RuRu,
    FiFi,
    EtEe,
    LvLv,
    LtLt,
    SkSk,
    CsCz,
    NbNo,
    NnNo,
    FrFr,
    EsEs,
    PtPt,
    ItIt,
    NlNl,
    PlPl,
    PtBr,
    JaJp,
    KoKr,
    ZhCn,
    HuHu,
    /// Host-regional-settings placeholder. This is not a stable locale identity;
    /// callers that need reproducible locale-keyed behavior should use an
    /// explicit locale identifier.
    CurrentExcelHost,
}

impl LocaleId {
    pub const fn stable_name(self) -> &'static str {
        match self {
            LocaleId::EnUs => "en-US",
            LocaleId::EnGb => "en-GB",
            LocaleId::EnIe => "en-IE",
            LocaleId::EnAu => "en-AU",
            LocaleId::EnNz => "en-NZ",
            LocaleId::EnZa => "en-ZA",
            LocaleId::EnIn => "en-IN",
            LocaleId::EnCa => "en-CA",
            LocaleId::EnPh => "en-PH",
            LocaleId::DeDe => "de-DE",
            LocaleId::RuRu => "ru-RU",
            LocaleId::FiFi => "fi-FI",
            LocaleId::EtEe => "et-EE",
            LocaleId::LvLv => "lv-LV",
            LocaleId::LtLt => "lt-LT",
            LocaleId::SkSk => "sk-SK",
            LocaleId::CsCz => "cs-CZ",
            LocaleId::NbNo => "nb-NO",
            LocaleId::NnNo => "nn-NO",
            LocaleId::FrFr => "fr-FR",
            LocaleId::EsEs => "es-ES",
            LocaleId::PtPt => "pt-PT",
            LocaleId::ItIt => "it-IT",
            LocaleId::NlNl => "nl-NL",
            LocaleId::PlPl => "pl-PL",
            LocaleId::PtBr => "pt-BR",
            LocaleId::JaJp => "ja-JP",
            LocaleId::KoKr => "ko-KR",
            LocaleId::ZhCn => "zh-CN",
            LocaleId::HuHu => "hu-HU",
            LocaleId::CurrentExcelHost => "current-excel-host",
        }
    }

    pub fn from_bcp47_language_tag(tag: &str) -> Option<Self> {
        let normalized = tag.trim().replace('_', "-").to_ascii_lowercase();
        let mut parts = normalized.split('-').filter(|part| !part.is_empty());
        let language = parts.next()?;
        let region = parts.next().unwrap_or("");

        match (language, region) {
            ("en", "") | ("en", "us") => Some(LocaleId::EnUs),
            ("en", "gb") => Some(LocaleId::EnGb),
            ("en", "ie") => Some(LocaleId::EnIe),
            ("en", "au") => Some(LocaleId::EnAu),
            ("en", "nz") => Some(LocaleId::EnNz),
            ("en", "za") => Some(LocaleId::EnZa),
            ("en", "in") => Some(LocaleId::EnIn),
            ("en", "ca") => Some(LocaleId::EnCa),
            ("en", "ph") => Some(LocaleId::EnPh),
            ("de", _) => Some(LocaleId::DeDe),
            ("ru", _) => Some(LocaleId::RuRu),
            ("fi", _) => Some(LocaleId::FiFi),
            ("et", _) => Some(LocaleId::EtEe),
            ("lv", _) => Some(LocaleId::LvLv),
            ("lt", _) => Some(LocaleId::LtLt),
            ("sk", _) => Some(LocaleId::SkSk),
            ("cs", _) => Some(LocaleId::CsCz),
            ("nb", _) => Some(LocaleId::NbNo),
            ("nn", _) => Some(LocaleId::NnNo),
            ("fr", _) => Some(LocaleId::FrFr),
            ("es", _) => Some(LocaleId::EsEs),
            ("pt", "") | ("pt", "pt") => Some(LocaleId::PtPt),
            ("pt", "br") => Some(LocaleId::PtBr),
            ("it", _) => Some(LocaleId::ItIt),
            ("nl", _) => Some(LocaleId::NlNl),
            ("pl", _) => Some(LocaleId::PlPl),
            ("zh", _) => Some(LocaleId::ZhCn),
            ("ja", _) => Some(LocaleId::JaJp),
            ("ko", _) => Some(LocaleId::KoKr),
            ("hu", _) => Some(LocaleId::HuHu),
            _ => None,
        }
    }

    pub const fn from_excel_lcid(lcid: u16) -> Option<Self> {
        match lcid {
            0x0409 => Some(LocaleId::EnUs),
            0x0809 => Some(LocaleId::EnGb),
            0x1809 => Some(LocaleId::EnIe),
            0x0C09 => Some(LocaleId::EnAu),
            0x1409 => Some(LocaleId::EnNz),
            0x1C09 => Some(LocaleId::EnZa),
            0x4009 => Some(LocaleId::EnIn),
            0x1009 => Some(LocaleId::EnCa),
            0x3409 => Some(LocaleId::EnPh),
            0x0407 | 0x0807 | 0x0C07 => Some(LocaleId::DeDe),
            0x0419 => Some(LocaleId::RuRu),
            0x040B => Some(LocaleId::FiFi),
            0x0425 => Some(LocaleId::EtEe),
            0x0426 => Some(LocaleId::LvLv),
            0x0427 => Some(LocaleId::LtLt),
            0x041B => Some(LocaleId::SkSk),
            0x0405 => Some(LocaleId::CsCz),
            0x0414 => Some(LocaleId::NbNo),
            0x0814 => Some(LocaleId::NnNo),
            0x040C | 0x080C | 0x0C0C | 0x100C => Some(LocaleId::FrFr),
            0x040A | 0x080A | 0x0C0A => Some(LocaleId::EsEs),
            0x0816 => Some(LocaleId::PtPt),
            0x0410 => Some(LocaleId::ItIt),
            0x0413 | 0x0813 => Some(LocaleId::NlNl),
            0x0415 => Some(LocaleId::PlPl),
            0x0416 => Some(LocaleId::PtBr),
            0x0411 => Some(LocaleId::JaJp),
            0x0412 => Some(LocaleId::KoKr),
            0x0804 | 0x1004 | 0x0404 | 0x0C04 => Some(LocaleId::ZhCn),
            0x040E => Some(LocaleId::HuHu),
            _ => None,
        }
    }
}

pub const CANONICAL_LOCALE_IDS: [LocaleId; 30] = [
    LocaleId::EnUs,
    LocaleId::EnGb,
    LocaleId::EnIe,
    LocaleId::EnAu,
    LocaleId::EnNz,
    LocaleId::EnZa,
    LocaleId::EnIn,
    LocaleId::EnCa,
    LocaleId::EnPh,
    LocaleId::DeDe,
    LocaleId::RuRu,
    LocaleId::FiFi,
    LocaleId::EtEe,
    LocaleId::LvLv,
    LocaleId::LtLt,
    LocaleId::SkSk,
    LocaleId::CsCz,
    LocaleId::NbNo,
    LocaleId::NnNo,
    LocaleId::FrFr,
    LocaleId::EsEs,
    LocaleId::PtPt,
    LocaleId::ItIt,
    LocaleId::NlNl,
    LocaleId::PlPl,
    LocaleId::PtBr,
    LocaleId::JaJp,
    LocaleId::KoKr,
    LocaleId::ZhCn,
    LocaleId::HuHu,
];

pub const ALL_LOCALE_IDS: [LocaleId; 31] = [
    LocaleId::EnUs,
    LocaleId::EnGb,
    LocaleId::EnIe,
    LocaleId::EnAu,
    LocaleId::EnNz,
    LocaleId::EnZa,
    LocaleId::EnIn,
    LocaleId::EnCa,
    LocaleId::EnPh,
    LocaleId::DeDe,
    LocaleId::RuRu,
    LocaleId::FiFi,
    LocaleId::EtEe,
    LocaleId::LvLv,
    LocaleId::LtLt,
    LocaleId::SkSk,
    LocaleId::CsCz,
    LocaleId::NbNo,
    LocaleId::NnNo,
    LocaleId::FrFr,
    LocaleId::EsEs,
    LocaleId::PtPt,
    LocaleId::ItIt,
    LocaleId::NlNl,
    LocaleId::PlPl,
    LocaleId::PtBr,
    LocaleId::JaJp,
    LocaleId::KoKr,
    LocaleId::ZhCn,
    LocaleId::HuHu,
    LocaleId::CurrentExcelHost,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateSystem {
    System1900,
    System1904,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateComponentOrder {
    Mdy,
    Dmy,
    Ymd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyPlacement {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencySpacing {
    None,
    Space,
    NarrowNoBreakSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyNegativePattern {
    LeadingMinus,
    TrailingMinus,
    Parentheses,
    MinusBeforeSymbol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatCodeTokenPolicy {
    InvariantExcel,
    LocalizedExcel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatProfile {
    pub id: LocaleId,
    pub decimal_separator: &'static str,
    pub thousands_separator: &'static str,
    pub list_separator: &'static str,
    pub currency_symbol: &'static str,
    pub date_separator: &'static str,
    pub time_separator: &'static str,
    pub currency_decimals: i32,
    pub short_date_order: DateComponentOrder,
    pub short_date_pattern: &'static str,
    pub two_digit_year_pivot: Option<u16>,
    pub currency_placement: CurrencyPlacement,
    pub currency_spacing: CurrencySpacing,
    pub currency_negative_pattern: CurrencyNegativePattern,
    pub format_code_decimal_token: &'static str,
    pub format_code_group_token: &'static str,
    pub format_code_token_policy: FormatCodeTokenPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFailure {
    UnsupportedText(String),
    UnsupportedByProfile(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatFailure {
    UnsupportedCode(String),
    InvalidDateSerial,
}
const fn short_date_order(id: LocaleId) -> DateComponentOrder {
    match id {
        LocaleId::EnUs | LocaleId::EnPh => DateComponentOrder::Mdy,
        LocaleId::EnZa
        | LocaleId::EnCa
        | LocaleId::LtLt
        | LocaleId::JaJp
        | LocaleId::KoKr
        | LocaleId::ZhCn
        | LocaleId::HuHu
        | LocaleId::CurrentExcelHost => DateComponentOrder::Ymd,
        _ => DateComponentOrder::Dmy,
    }
}

const fn short_date_pattern(id: LocaleId) -> &'static str {
    match id {
        LocaleId::EnUs | LocaleId::EnPh => "M/d/yyyy",
        LocaleId::EnGb | LocaleId::EnIe | LocaleId::EnIn => "dd/MM/yyyy",
        LocaleId::EnAu | LocaleId::EsEs => "d/M/yyyy",
        LocaleId::EnNz => "d/MM/yyyy",
        LocaleId::EnZa => "yyyy/MM/dd",
        LocaleId::EnCa | LocaleId::LtLt => "yyyy-MM-dd",
        LocaleId::DeDe
        | LocaleId::RuRu
        | LocaleId::EtEe
        | LocaleId::LvLv
        | LocaleId::CsCz
        | LocaleId::NbNo
        | LocaleId::NnNo => "dd.MM.yyyy",
        LocaleId::FiFi => "d.M.yyyy",
        LocaleId::PlPl => "d.MM.yyyy",
        LocaleId::SkSk => "d. M. yyyy",
        LocaleId::FrFr | LocaleId::PtPt | LocaleId::ItIt | LocaleId::PtBr => "dd/MM/yyyy",
        LocaleId::NlNl => "dd-MM-yyyy",
        LocaleId::JaJp => "yyyy/MM/dd",
        LocaleId::KoKr => "yyyy. M. d.",
        LocaleId::ZhCn => "yyyy/M/d",
        LocaleId::HuHu => "yyyy. MM. dd.",
        LocaleId::CurrentExcelHost => "yyyy/MM/dd",
    }
}

const fn currency_placement(id: LocaleId) -> CurrencyPlacement {
    match id {
        LocaleId::DeDe
        | LocaleId::RuRu
        | LocaleId::FiFi
        | LocaleId::EtEe
        | LocaleId::LvLv
        | LocaleId::LtLt
        | LocaleId::SkSk
        | LocaleId::CsCz
        | LocaleId::NnNo
        | LocaleId::FrFr
        | LocaleId::EsEs
        | LocaleId::PtPt
        | LocaleId::ItIt
        | LocaleId::PlPl
        | LocaleId::HuHu => CurrencyPlacement::After,
        _ => CurrencyPlacement::Before,
    }
}

const fn currency_spacing(id: LocaleId) -> CurrencySpacing {
    match id {
        LocaleId::FrFr => CurrencySpacing::NarrowNoBreakSpace,
        LocaleId::DeDe
        | LocaleId::RuRu
        | LocaleId::FiFi
        | LocaleId::EtEe
        | LocaleId::LvLv
        | LocaleId::LtLt
        | LocaleId::SkSk
        | LocaleId::CsCz
        | LocaleId::NbNo
        | LocaleId::NnNo
        | LocaleId::EsEs
        | LocaleId::PtPt
        | LocaleId::ItIt
        | LocaleId::NlNl
        | LocaleId::PlPl
        | LocaleId::PtBr
        | LocaleId::HuHu => CurrencySpacing::Space,
        _ => CurrencySpacing::None,
    }
}

const fn currency_negative_pattern(id: LocaleId) -> CurrencyNegativePattern {
    match id {
        LocaleId::EnUs
        | LocaleId::EnGb
        | LocaleId::EnIe
        | LocaleId::EnAu
        | LocaleId::EnNz
        | LocaleId::EnZa
        | LocaleId::EnIn
        | LocaleId::EnCa
        | LocaleId::EnPh
        | LocaleId::JaJp
        | LocaleId::KoKr
        | LocaleId::ZhCn
        | LocaleId::CurrentExcelHost => CurrencyNegativePattern::MinusBeforeSymbol,
        LocaleId::PtBr => CurrencyNegativePattern::MinusBeforeSymbol,
        _ => CurrencyNegativePattern::LeadingMinus,
    }
}

pub const fn format_profile(id: LocaleId) -> FormatProfile {
    match id {
        LocaleId::EnUs => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "$",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnGb => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{00A3}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnIe => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{20AC}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnAu => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "$",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnNz => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "$",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnZa => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ",",
            currency_symbol: "R",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnIn => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{20B9}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnCa => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "$",
            date_separator: "-",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EnPh => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{20B1}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::DeDe => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: ".",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::RuRu => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20BD}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::FiFi => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: ".",
            time_separator: ".",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EtEe => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::LvLv => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::LtLt => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "-",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::SkSk => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::CsCz => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "K\u{010D}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::NbNo | LocaleId::NnNo => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "kr",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::FrFr => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{202F}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::EsEs => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: ".",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::PtPt => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::ItIt => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: ".",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::NlNl => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: ".",
            list_separator: ";",
            currency_symbol: "\u{20AC}",
            date_separator: "-",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::PlPl => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "z\u{0142}",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::PtBr => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: ".",
            list_separator: ";",
            currency_symbol: "R$",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::JaJp => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{00A5}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 0,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::KoKr => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{20A9}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 0,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::ZhCn => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: ",",
            list_separator: ",",
            currency_symbol: "\u{00A5}",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::HuHu => FormatProfile {
            id,
            decimal_separator: ",",
            thousands_separator: "\u{00A0}",
            list_separator: ";",
            currency_symbol: "Ft",
            date_separator: ".",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
        LocaleId::CurrentExcelHost => FormatProfile {
            id,
            decimal_separator: ".",
            thousands_separator: " ",
            list_separator: ",",
            currency_symbol: "R",
            date_separator: "/",
            time_separator: ":",
            currency_decimals: 2,
            short_date_order: short_date_order(id),
            short_date_pattern: short_date_pattern(id),
            two_digit_year_pivot: None,
            currency_placement: currency_placement(id),
            currency_spacing: currency_spacing(id),
            currency_negative_pattern: currency_negative_pattern(id),
            format_code_decimal_token: ".",
            format_code_group_token: ",",
            format_code_token_policy: FormatCodeTokenPolicy::InvariantExcel,
        },
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(m <= 2);
    (year, m, d)
}

pub fn excel_serial_from_ymd(
    date_system: DateSystem,
    year: i64,
    month: i64,
    day: i64,
) -> Option<f64> {
    match date_system {
        DateSystem::System1900 => {
            if year == 1900 && month == 2 && day == 29 {
                return Some(60.0);
            }
            let base = days_from_civil(1899, 12, 31);
            let days = days_from_civil(year, month, 1) - base + (day - 1);
            if days < 0 {
                return None;
            }
            Some(if days >= 60 {
                (days + 1) as f64
            } else {
                days as f64
            })
        }
        DateSystem::System1904 => {
            let base = days_from_civil(1904, 1, 1);
            let days = days_from_civil(year, month, 1) - base + (day - 1);
            Some(days as f64)
        }
    }
}

pub fn ymd_from_excel_serial(date_system: DateSystem, serial: f64) -> Option<(i64, i64, i64)> {
    let whole = serial.trunc() as i64;
    match date_system {
        DateSystem::System1900 => {
            if whole == 0 {
                return Some((1900, 1, 0));
            }
            if whole == 60 {
                return Some((1900, 2, 29));
            }
            let adjusted = if whole >= 60 { whole - 1 } else { whole };
            let base = days_from_civil(1899, 12, 31);
            Some(civil_from_days(base + adjusted))
        }
        DateSystem::System1904 => {
            let base = days_from_civil(1904, 1, 1);
            Some(civil_from_days(base + whole))
        }
    }
}

impl FormatProfile {
    /// The profile of the culture named by a BCP 47 tag such as `en-ZA`, when OxFunc's table
    /// holds one.
    pub fn for_culture(tag: &str) -> Option<FormatProfile> {
        LocaleId::from_bcp47_language_tag(tag).map(format_profile)
    }

    /// This profile with the decimal and thousands separators a host's settings name (Windows
    /// lets a user change both), or `None` for a separator outside the ones a profile holds.
    pub fn with_separators(self, decimal: &str, thousands: &str) -> Option<FormatProfile> {
        fn known(text: &str) -> Option<&'static str> {
            Some(match text {
                "." => ".",
                "," => ",",
                " " => " ",
                "\u{00A0}" => "\u{00A0}",
                "\u{202F}" => "\u{202F}",
                "'" => "'",
                "\u{2019}" => "\u{2019}",
                _ => return None,
            })
        }
        // A format code reads the separators the host uses (Excel on this machine, en-ZA with a
        // space as the group separator, read `#,##0` with `,` as a literal).
        let (decimal, thousands) = (known(decimal)?, known(thousands)?);
        Some(FormatProfile {
            decimal_separator: decimal,
            thousands_separator: thousands,
            format_code_decimal_token: decimal,
            format_code_group_token: thousands,
            format_code_token_policy: FormatCodeTokenPolicy::LocalizedExcel,
            ..self
        })
    }
}

/// What a calculation runs in besides its numeric mode: the workbook's date system, the host's
/// locale profile (its separators, currency and date order), and the moment of the
/// recalculation as a serial date number, which NOW, TODAY and a date read without a year use.
/// A part the caller leaves out refuses the calls that need it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalcEnvironment {
    pub date_system: DateSystem,
    /// Workbook CompatibilityVersion, independent of locale and numeric mode. Workbooks
    /// without the setting use 1. Unsupported explicit versions remain observable to functions.
    pub compatibility_version: u32,
    pub profile: Option<FormatProfile>,
    pub now: Option<f64>,
}

impl CalcEnvironment {
    /// A Version 1 workbook in `date_system`, with no locale profile and no clock.
    pub const fn dates(date_system: DateSystem) -> Self {
        CalcEnvironment {
            date_system,
            compatibility_version: 1,
            profile: None,
            now: None,
        }
    }
}
