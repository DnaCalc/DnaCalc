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
pub enum LocaleProfileId {
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
    /// explicit locale profile id.
    CurrentExcelHost,
}

impl LocaleProfileId {
    pub const fn stable_name(self) -> &'static str {
        match self {
            LocaleProfileId::EnUs => "en-US",
            LocaleProfileId::EnGb => "en-GB",
            LocaleProfileId::EnIe => "en-IE",
            LocaleProfileId::EnAu => "en-AU",
            LocaleProfileId::EnNz => "en-NZ",
            LocaleProfileId::EnZa => "en-ZA",
            LocaleProfileId::EnIn => "en-IN",
            LocaleProfileId::EnCa => "en-CA",
            LocaleProfileId::EnPh => "en-PH",
            LocaleProfileId::DeDe => "de-DE",
            LocaleProfileId::RuRu => "ru-RU",
            LocaleProfileId::FiFi => "fi-FI",
            LocaleProfileId::EtEe => "et-EE",
            LocaleProfileId::LvLv => "lv-LV",
            LocaleProfileId::LtLt => "lt-LT",
            LocaleProfileId::SkSk => "sk-SK",
            LocaleProfileId::CsCz => "cs-CZ",
            LocaleProfileId::NbNo => "nb-NO",
            LocaleProfileId::NnNo => "nn-NO",
            LocaleProfileId::FrFr => "fr-FR",
            LocaleProfileId::EsEs => "es-ES",
            LocaleProfileId::PtPt => "pt-PT",
            LocaleProfileId::ItIt => "it-IT",
            LocaleProfileId::NlNl => "nl-NL",
            LocaleProfileId::PlPl => "pl-PL",
            LocaleProfileId::PtBr => "pt-BR",
            LocaleProfileId::JaJp => "ja-JP",
            LocaleProfileId::KoKr => "ko-KR",
            LocaleProfileId::ZhCn => "zh-CN",
            LocaleProfileId::HuHu => "hu-HU",
            LocaleProfileId::CurrentExcelHost => "current-excel-host",
        }
    }

    pub fn from_bcp47_language_tag(tag: &str) -> Option<Self> {
        let normalized = tag.trim().replace('_', "-").to_ascii_lowercase();
        let mut parts = normalized.split('-').filter(|part| !part.is_empty());
        let language = parts.next()?;
        let region = parts.next().unwrap_or("");

        match (language, region) {
            ("en", "") | ("en", "us") => Some(LocaleProfileId::EnUs),
            ("en", "gb") => Some(LocaleProfileId::EnGb),
            ("en", "ie") => Some(LocaleProfileId::EnIe),
            ("en", "au") => Some(LocaleProfileId::EnAu),
            ("en", "nz") => Some(LocaleProfileId::EnNz),
            ("en", "za") => Some(LocaleProfileId::EnZa),
            ("en", "in") => Some(LocaleProfileId::EnIn),
            ("en", "ca") => Some(LocaleProfileId::EnCa),
            ("en", "ph") => Some(LocaleProfileId::EnPh),
            ("de", _) => Some(LocaleProfileId::DeDe),
            ("ru", _) => Some(LocaleProfileId::RuRu),
            ("fi", _) => Some(LocaleProfileId::FiFi),
            ("et", _) => Some(LocaleProfileId::EtEe),
            ("lv", _) => Some(LocaleProfileId::LvLv),
            ("lt", _) => Some(LocaleProfileId::LtLt),
            ("sk", _) => Some(LocaleProfileId::SkSk),
            ("cs", _) => Some(LocaleProfileId::CsCz),
            ("nb", _) => Some(LocaleProfileId::NbNo),
            ("nn", _) => Some(LocaleProfileId::NnNo),
            ("fr", _) => Some(LocaleProfileId::FrFr),
            ("es", _) => Some(LocaleProfileId::EsEs),
            ("pt", "") | ("pt", "pt") => Some(LocaleProfileId::PtPt),
            ("pt", "br") => Some(LocaleProfileId::PtBr),
            ("it", _) => Some(LocaleProfileId::ItIt),
            ("nl", _) => Some(LocaleProfileId::NlNl),
            ("pl", _) => Some(LocaleProfileId::PlPl),
            ("zh", _) => Some(LocaleProfileId::ZhCn),
            ("ja", _) => Some(LocaleProfileId::JaJp),
            ("ko", _) => Some(LocaleProfileId::KoKr),
            ("hu", _) => Some(LocaleProfileId::HuHu),
            _ => None,
        }
    }

    pub const fn from_excel_lcid(lcid: u16) -> Option<Self> {
        match lcid {
            0x0409 => Some(LocaleProfileId::EnUs),
            0x0809 => Some(LocaleProfileId::EnGb),
            0x1809 => Some(LocaleProfileId::EnIe),
            0x0C09 => Some(LocaleProfileId::EnAu),
            0x1409 => Some(LocaleProfileId::EnNz),
            0x1C09 => Some(LocaleProfileId::EnZa),
            0x4009 => Some(LocaleProfileId::EnIn),
            0x1009 => Some(LocaleProfileId::EnCa),
            0x3409 => Some(LocaleProfileId::EnPh),
            0x0407 | 0x0807 | 0x0C07 => Some(LocaleProfileId::DeDe),
            0x0419 => Some(LocaleProfileId::RuRu),
            0x040B => Some(LocaleProfileId::FiFi),
            0x0425 => Some(LocaleProfileId::EtEe),
            0x0426 => Some(LocaleProfileId::LvLv),
            0x0427 => Some(LocaleProfileId::LtLt),
            0x041B => Some(LocaleProfileId::SkSk),
            0x0405 => Some(LocaleProfileId::CsCz),
            0x0414 => Some(LocaleProfileId::NbNo),
            0x0814 => Some(LocaleProfileId::NnNo),
            0x040C | 0x080C | 0x0C0C | 0x100C => Some(LocaleProfileId::FrFr),
            0x040A | 0x080A | 0x0C0A => Some(LocaleProfileId::EsEs),
            0x0816 => Some(LocaleProfileId::PtPt),
            0x0410 => Some(LocaleProfileId::ItIt),
            0x0413 | 0x0813 => Some(LocaleProfileId::NlNl),
            0x0415 => Some(LocaleProfileId::PlPl),
            0x0416 => Some(LocaleProfileId::PtBr),
            0x0411 => Some(LocaleProfileId::JaJp),
            0x0412 => Some(LocaleProfileId::KoKr),
            0x0804 | 0x1004 | 0x0404 | 0x0C04 => Some(LocaleProfileId::ZhCn),
            0x040E => Some(LocaleProfileId::HuHu),
            _ => None,
        }
    }
}

pub const CANONICAL_LOCALE_PROFILE_IDS: [LocaleProfileId; 30] = [
    LocaleProfileId::EnUs,
    LocaleProfileId::EnGb,
    LocaleProfileId::EnIe,
    LocaleProfileId::EnAu,
    LocaleProfileId::EnNz,
    LocaleProfileId::EnZa,
    LocaleProfileId::EnIn,
    LocaleProfileId::EnCa,
    LocaleProfileId::EnPh,
    LocaleProfileId::DeDe,
    LocaleProfileId::RuRu,
    LocaleProfileId::FiFi,
    LocaleProfileId::EtEe,
    LocaleProfileId::LvLv,
    LocaleProfileId::LtLt,
    LocaleProfileId::SkSk,
    LocaleProfileId::CsCz,
    LocaleProfileId::NbNo,
    LocaleProfileId::NnNo,
    LocaleProfileId::FrFr,
    LocaleProfileId::EsEs,
    LocaleProfileId::PtPt,
    LocaleProfileId::ItIt,
    LocaleProfileId::NlNl,
    LocaleProfileId::PlPl,
    LocaleProfileId::PtBr,
    LocaleProfileId::JaJp,
    LocaleProfileId::KoKr,
    LocaleProfileId::ZhCn,
    LocaleProfileId::HuHu,
];

pub const LOCALE_PROFILE_IDS: [LocaleProfileId; 31] = [
    LocaleProfileId::EnUs,
    LocaleProfileId::EnGb,
    LocaleProfileId::EnIe,
    LocaleProfileId::EnAu,
    LocaleProfileId::EnNz,
    LocaleProfileId::EnZa,
    LocaleProfileId::EnIn,
    LocaleProfileId::EnCa,
    LocaleProfileId::EnPh,
    LocaleProfileId::DeDe,
    LocaleProfileId::RuRu,
    LocaleProfileId::FiFi,
    LocaleProfileId::EtEe,
    LocaleProfileId::LvLv,
    LocaleProfileId::LtLt,
    LocaleProfileId::SkSk,
    LocaleProfileId::CsCz,
    LocaleProfileId::NbNo,
    LocaleProfileId::NnNo,
    LocaleProfileId::FrFr,
    LocaleProfileId::EsEs,
    LocaleProfileId::PtPt,
    LocaleProfileId::ItIt,
    LocaleProfileId::NlNl,
    LocaleProfileId::PlPl,
    LocaleProfileId::PtBr,
    LocaleProfileId::JaJp,
    LocaleProfileId::KoKr,
    LocaleProfileId::ZhCn,
    LocaleProfileId::HuHu,
    LocaleProfileId::CurrentExcelHost,
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
    pub id: LocaleProfileId,
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
const fn short_date_order(id: LocaleProfileId) -> DateComponentOrder {
    match id {
        LocaleProfileId::EnUs | LocaleProfileId::EnPh => DateComponentOrder::Mdy,
        LocaleProfileId::EnZa
        | LocaleProfileId::EnCa
        | LocaleProfileId::LtLt
        | LocaleProfileId::JaJp
        | LocaleProfileId::KoKr
        | LocaleProfileId::ZhCn
        | LocaleProfileId::HuHu
        | LocaleProfileId::CurrentExcelHost => DateComponentOrder::Ymd,
        _ => DateComponentOrder::Dmy,
    }
}

const fn short_date_pattern(id: LocaleProfileId) -> &'static str {
    match id {
        LocaleProfileId::EnUs | LocaleProfileId::EnPh => "M/d/yyyy",
        LocaleProfileId::EnGb | LocaleProfileId::EnIe | LocaleProfileId::EnIn => "dd/MM/yyyy",
        LocaleProfileId::EnAu | LocaleProfileId::EsEs => "d/M/yyyy",
        LocaleProfileId::EnNz => "d/MM/yyyy",
        LocaleProfileId::EnZa => "yyyy/MM/dd",
        LocaleProfileId::EnCa | LocaleProfileId::LtLt => "yyyy-MM-dd",
        LocaleProfileId::DeDe
        | LocaleProfileId::RuRu
        | LocaleProfileId::EtEe
        | LocaleProfileId::LvLv
        | LocaleProfileId::CsCz
        | LocaleProfileId::NbNo
        | LocaleProfileId::NnNo => "dd.MM.yyyy",
        LocaleProfileId::FiFi => "d.M.yyyy",
        LocaleProfileId::PlPl => "d.MM.yyyy",
        LocaleProfileId::SkSk => "d. M. yyyy",
        LocaleProfileId::FrFr
        | LocaleProfileId::PtPt
        | LocaleProfileId::ItIt
        | LocaleProfileId::PtBr => "dd/MM/yyyy",
        LocaleProfileId::NlNl => "dd-MM-yyyy",
        LocaleProfileId::JaJp => "yyyy/MM/dd",
        LocaleProfileId::KoKr => "yyyy. M. d.",
        LocaleProfileId::ZhCn => "yyyy/M/d",
        LocaleProfileId::HuHu => "yyyy. MM. dd.",
        LocaleProfileId::CurrentExcelHost => "yyyy/MM/dd",
    }
}

const fn currency_placement(id: LocaleProfileId) -> CurrencyPlacement {
    match id {
        LocaleProfileId::DeDe
        | LocaleProfileId::RuRu
        | LocaleProfileId::FiFi
        | LocaleProfileId::EtEe
        | LocaleProfileId::LvLv
        | LocaleProfileId::LtLt
        | LocaleProfileId::SkSk
        | LocaleProfileId::CsCz
        | LocaleProfileId::NnNo
        | LocaleProfileId::FrFr
        | LocaleProfileId::EsEs
        | LocaleProfileId::PtPt
        | LocaleProfileId::ItIt
        | LocaleProfileId::PlPl
        | LocaleProfileId::HuHu => CurrencyPlacement::After,
        _ => CurrencyPlacement::Before,
    }
}

const fn currency_spacing(id: LocaleProfileId) -> CurrencySpacing {
    match id {
        LocaleProfileId::FrFr => CurrencySpacing::NarrowNoBreakSpace,
        LocaleProfileId::DeDe
        | LocaleProfileId::RuRu
        | LocaleProfileId::FiFi
        | LocaleProfileId::EtEe
        | LocaleProfileId::LvLv
        | LocaleProfileId::LtLt
        | LocaleProfileId::SkSk
        | LocaleProfileId::CsCz
        | LocaleProfileId::NbNo
        | LocaleProfileId::NnNo
        | LocaleProfileId::EsEs
        | LocaleProfileId::PtPt
        | LocaleProfileId::ItIt
        | LocaleProfileId::NlNl
        | LocaleProfileId::PlPl
        | LocaleProfileId::PtBr
        | LocaleProfileId::HuHu => CurrencySpacing::Space,
        _ => CurrencySpacing::None,
    }
}

const fn currency_negative_pattern(id: LocaleProfileId) -> CurrencyNegativePattern {
    match id {
        LocaleProfileId::EnUs
        | LocaleProfileId::EnGb
        | LocaleProfileId::EnIe
        | LocaleProfileId::EnAu
        | LocaleProfileId::EnNz
        | LocaleProfileId::EnZa
        | LocaleProfileId::EnIn
        | LocaleProfileId::EnCa
        | LocaleProfileId::EnPh
        | LocaleProfileId::JaJp
        | LocaleProfileId::KoKr
        | LocaleProfileId::ZhCn
        | LocaleProfileId::CurrentExcelHost => CurrencyNegativePattern::MinusBeforeSymbol,
        LocaleProfileId::PtBr => CurrencyNegativePattern::MinusBeforeSymbol,
        _ => CurrencyNegativePattern::LeadingMinus,
    }
}

pub const fn format_profile(id: LocaleProfileId) -> FormatProfile {
    match id {
        LocaleProfileId::EnUs => FormatProfile {
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
        LocaleProfileId::EnGb => FormatProfile {
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
        LocaleProfileId::EnIe => FormatProfile {
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
        LocaleProfileId::EnAu => FormatProfile {
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
        LocaleProfileId::EnNz => FormatProfile {
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
        LocaleProfileId::EnZa => FormatProfile {
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
        LocaleProfileId::EnIn => FormatProfile {
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
        LocaleProfileId::EnCa => FormatProfile {
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
        LocaleProfileId::EnPh => FormatProfile {
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
        LocaleProfileId::DeDe => FormatProfile {
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
        LocaleProfileId::RuRu => FormatProfile {
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
        LocaleProfileId::FiFi => FormatProfile {
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
        LocaleProfileId::EtEe => FormatProfile {
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
        LocaleProfileId::LvLv => FormatProfile {
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
        LocaleProfileId::LtLt => FormatProfile {
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
        LocaleProfileId::SkSk => FormatProfile {
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
        LocaleProfileId::CsCz => FormatProfile {
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
        LocaleProfileId::NbNo | LocaleProfileId::NnNo => FormatProfile {
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
        LocaleProfileId::FrFr => FormatProfile {
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
        LocaleProfileId::EsEs => FormatProfile {
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
        LocaleProfileId::PtPt => FormatProfile {
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
        LocaleProfileId::ItIt => FormatProfile {
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
        LocaleProfileId::NlNl => FormatProfile {
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
        LocaleProfileId::PlPl => FormatProfile {
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
        LocaleProfileId::PtBr => FormatProfile {
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
        LocaleProfileId::JaJp => FormatProfile {
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
        LocaleProfileId::KoKr => FormatProfile {
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
        LocaleProfileId::ZhCn => FormatProfile {
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
        LocaleProfileId::HuHu => FormatProfile {
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
        LocaleProfileId::CurrentExcelHost => FormatProfile {
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

pub fn ymd_from_excel_serial(
    date_system: DateSystem,
    serial: f64,
) -> Option<(i64, i64, i64)> {
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
        LocaleProfileId::from_bcp47_language_tag(tag).map(format_profile)
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
        Some(FormatProfile {
            decimal_separator: known(decimal)?,
            thousands_separator: known(thousands)?,
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
    pub profile: Option<FormatProfile>,
    pub now: Option<f64>,
}

impl CalcEnvironment {
    /// The environment of a workbook in `date_system` with no locale profile and no clock.
    pub const fn dates(date_system: DateSystem) -> Self {
        CalcEnvironment {
            date_system,
            profile: None,
            now: None,
        }
    }
}
