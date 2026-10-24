//! Month and weekday names by locale, ported from OxFml (crates/oxfml_core/src/format/locale_tables.rs at 15daac5).

use crate::locale::{FormatProfile, LocaleId};

#[derive(Clone, Copy)]
struct LocaleNames {
    months_short: [&'static str; 12],
    months_long: [&'static str; 12],
    weekdays_short: [&'static str; 7],
    weekdays_long: [&'static str; 7],
}

pub(crate) fn month_name(profile: &FormatProfile, month: i64, abbreviated: bool) -> &'static str {
    let Ok(index) = usize::try_from(month - 1) else {
        return "";
    };
    let names = locale_names(profile.id);
    if abbreviated {
        names.months_short.get(index).copied().unwrap_or("")
    } else {
        names.months_long.get(index).copied().unwrap_or("")
    }
}

pub(crate) fn weekday_name(profile: &FormatProfile, index: usize, abbreviated: bool) -> &'static str {
    let names = locale_names(profile.id);
    if abbreviated {
        names.weekdays_short.get(index).copied().unwrap_or("")
    } else {
        names.weekdays_long.get(index).copied().unwrap_or("")
    }
}

fn locale_names(id: LocaleId) -> &'static LocaleNames {
    match id {
        LocaleId::EnUs => &EN_US,
        LocaleId::EnGb => &EN_GB,
        LocaleId::EnIe => &EN_IE,
        LocaleId::EnAu => &EN_AU,
        LocaleId::EnNz => &EN_NZ,
        LocaleId::EnZa => &EN_ZA,
        LocaleId::EnIn => &EN_IN,
        LocaleId::EnCa => &EN_CA,
        LocaleId::EnPh => &EN_PH,
        LocaleId::DeDe => &DE_DE,
        LocaleId::RuRu => &RU_RU,
        LocaleId::FiFi => &FI_FI,
        LocaleId::EtEe => &ET_EE,
        LocaleId::LvLv => &LV_LV,
        LocaleId::LtLt => &LT_LT,
        LocaleId::SkSk => &SK_SK,
        LocaleId::CsCz => &CS_CZ,
        LocaleId::NbNo => &NB_NO,
        LocaleId::NnNo => &NN_NO,
        LocaleId::FrFr => &FR_FR,
        LocaleId::EsEs => &ES_ES,
        LocaleId::PtPt => &PT_PT,
        LocaleId::ItIt => &IT_IT,
        LocaleId::NlNl => &NL_NL,
        LocaleId::PlPl => &PL_PL,
        LocaleId::PtBr => &PT_BR,
        LocaleId::JaJp => &JA_JP,
        LocaleId::KoKr => &KO_KR,
        LocaleId::ZhCn => &ZH_CN,
        LocaleId::HuHu => &HU_HU,
        LocaleId::CurrentExcelHost => &EN_US,
    }
}

const EN_US: LocaleNames = LocaleNames {
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    months_long: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    weekdays_short: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    weekdays_long: [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ],
};

const EN_GB: LocaleNames = LocaleNames {
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec",
    ],
    ..EN_US
};

const EN_IE: LocaleNames = LocaleNames { ..EN_GB };

const EN_AU: LocaleNames = LocaleNames {
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "June", "July", "Aug", "Sept", "Oct", "Nov", "Dec",
    ],
    ..EN_US
};

const EN_NZ: LocaleNames = LocaleNames { ..EN_GB };
const EN_ZA: LocaleNames = LocaleNames { ..EN_GB };
const EN_IN: LocaleNames = LocaleNames { ..EN_GB };

const EN_CA: LocaleNames = LocaleNames {
    months_short: EN_US.months_short,
    ..EN_US
};

const EN_PH: LocaleNames = LocaleNames { ..EN_US };

const DE_DE: LocaleNames = LocaleNames {
    months_short: [
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ],
    months_long: [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    weekdays_short: ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
    weekdays_long: [
        "Sonntag",
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
    ],
};

const RU_RU: LocaleNames = LocaleNames {
    months_short: [
        "янв.",
        "февр.",
        "март",
        "апр.",
        "май",
        "июнь",
        "июль",
        "авг.",
        "сент.",
        "окт.",
        "нояб.",
        "дек.",
    ],
    months_long: [
        "январь",
        "февраль",
        "март",
        "апрель",
        "май",
        "июнь",
        "июль",
        "август",
        "сентябрь",
        "октябрь",
        "ноябрь",
        "декабрь",
    ],
    weekdays_short: ["вс", "пн", "вт", "ср", "чт", "пт", "сб"],
    weekdays_long: [
        "воскресенье",
        "понедельник",
        "вторник",
        "среда",
        "четверг",
        "пятница",
        "суббота",
    ],
};

const FI_FI: LocaleNames = LocaleNames {
    months_short: [
        "tammi", "helmi", "maalis", "huhti", "touko", "kesä", "heinä", "elo", "syys", "loka",
        "marras", "joulu",
    ],
    months_long: [
        "tammikuu",
        "helmikuu",
        "maaliskuu",
        "huhtikuu",
        "toukokuu",
        "kesäkuu",
        "heinäkuu",
        "elokuu",
        "syyskuu",
        "lokakuu",
        "marraskuu",
        "joulukuu",
    ],
    weekdays_short: ["su", "ma", "ti", "ke", "to", "pe", "la"],
    weekdays_long: [
        "sunnuntai",
        "maanantai",
        "tiistai",
        "keskiviikko",
        "torstai",
        "perjantai",
        "lauantai",
    ],
};

const ET_EE: LocaleNames = LocaleNames {
    months_short: [
        "jaanuar",
        "veebruar",
        "märts",
        "aprill",
        "mai",
        "juuni",
        "juuli",
        "august",
        "september",
        "oktoober",
        "november",
        "detsember",
    ],
    months_long: [
        "jaanuar",
        "veebruar",
        "märts",
        "aprill",
        "mai",
        "juuni",
        "juuli",
        "august",
        "september",
        "oktoober",
        "november",
        "detsember",
    ],
    weekdays_short: ["P", "E", "T", "K", "N", "R", "L"],
    weekdays_long: [
        "pühapäev",
        "esmaspäev",
        "teisipäev",
        "kolmapäev",
        "neljapäev",
        "reede",
        "laupäev",
    ],
};

const LV_LV: LocaleNames = LocaleNames {
    months_short: [
        "janv.", "febr.", "marts", "apr.", "maijs", "jūn.", "jūl.", "aug.", "sept.", "okt.",
        "nov.", "dec.",
    ],
    months_long: [
        "janvāris",
        "februāris",
        "marts",
        "aprīlis",
        "maijs",
        "jūnijs",
        "jūlijs",
        "augusts",
        "septembris",
        "oktobris",
        "novembris",
        "decembris",
    ],
    weekdays_short: [
        "Svētd.", "Pirmd.", "Otrd.", "Trešd.", "Ceturtd.", "Piektd.", "Sestd.",
    ],
    weekdays_long: [
        "Svētdiena",
        "Pirmdiena",
        "Otrdiena",
        "Trešdiena",
        "Ceturtdiena",
        "Piektdiena",
        "Sestdiena",
    ],
};

const LT_LT: LocaleNames = LocaleNames {
    months_short: [
        "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
    ],
    months_long: [
        "sausis",
        "vasaris",
        "kovas",
        "balandis",
        "gegužė",
        "birželis",
        "liepa",
        "rugpjūtis",
        "rugsėjis",
        "spalis",
        "lapkritis",
        "gruodis",
    ],
    weekdays_short: ["sk", "pr", "an", "tr", "kt", "pn", "št"],
    weekdays_long: [
        "sekmadienis",
        "pirmadienis",
        "antradienis",
        "trečiadienis",
        "ketvirtadienis",
        "penktadienis",
        "šeštadienis",
    ],
};

const SK_SK: LocaleNames = LocaleNames {
    months_short: [
        "jan", "feb", "mar", "apr", "máj", "jún", "júl", "aug", "sep", "okt", "nov", "dec",
    ],
    months_long: [
        "január",
        "február",
        "marec",
        "apríl",
        "máj",
        "jún",
        "júl",
        "august",
        "september",
        "október",
        "november",
        "december",
    ],
    weekdays_short: ["ne", "po", "ut", "st", "št", "pi", "so"],
    weekdays_long: [
        "nedeľa", "pondelok", "utorok", "streda", "štvrtok", "piatok", "sobota",
    ],
};

const CS_CZ: LocaleNames = LocaleNames {
    months_short: [
        "led", "úno", "bře", "dub", "kvě", "čvn", "čvc", "srp", "zář", "říj", "lis", "pro",
    ],
    months_long: [
        "leden",
        "únor",
        "březen",
        "duben",
        "květen",
        "červen",
        "červenec",
        "srpen",
        "září",
        "říjen",
        "listopad",
        "prosinec",
    ],
    weekdays_short: ["ne", "po", "út", "st", "čt", "pá", "so"],
    weekdays_long: [
        "neděle",
        "pondělí",
        "úterý",
        "středa",
        "čtvrtek",
        "pátek",
        "sobota",
    ],
};

const NB_NO: LocaleNames = LocaleNames {
    months_short: [
        "jan", "feb", "mar", "apr", "mai", "jun", "jul", "aug", "sep", "okt", "nov", "des",
    ],
    months_long: [
        "januar",
        "februar",
        "mars",
        "april",
        "mai",
        "juni",
        "juli",
        "august",
        "september",
        "oktober",
        "november",
        "desember",
    ],
    weekdays_short: ["søn.", "man.", "tir.", "ons.", "tor.", "fre.", "lør."],
    weekdays_long: [
        "søndag", "mandag", "tirsdag", "onsdag", "torsdag", "fredag", "lørdag",
    ],
};

const NN_NO: LocaleNames = LocaleNames {
    months_short: NB_NO.months_short,
    months_long: NB_NO.months_long,
    weekdays_short: ["søn", "mån", "tys", "ons", "tor", "fre", "lau"],
    weekdays_long: [
        "søndag", "måndag", "tysdag", "onsdag", "torsdag", "fredag", "laurdag",
    ],
};

const FR_FR: LocaleNames = LocaleNames {
    months_short: [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ],
    months_long: [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    weekdays_short: ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
    weekdays_long: [
        "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
    ],
};

const ES_ES: LocaleNames = LocaleNames {
    months_short: [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
    ],
    months_long: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    weekdays_short: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    weekdays_long: [
        "domingo",
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
    ],
};

const PT_PT: LocaleNames = LocaleNames {
    months_short: [
        "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.",
        "dez.",
    ],
    months_long: [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ],
    weekdays_short: [
        "domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado",
    ],
    weekdays_long: [
        "domingo",
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
    ],
};

const IT_IT: LocaleNames = LocaleNames {
    months_short: [
        "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
    ],
    months_long: [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ],
    weekdays_short: ["dom", "lun", "mar", "mer", "gio", "ven", "sab"],
    weekdays_long: [
        "domenica",
        "lunedì",
        "martedì",
        "mercoledì",
        "giovedì",
        "venerdì",
        "sabato",
    ],
};

const NL_NL: LocaleNames = LocaleNames {
    months_short: [
        "jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
    ],
    months_long: [
        "januari",
        "februari",
        "maart",
        "april",
        "mei",
        "juni",
        "juli",
        "augustus",
        "september",
        "oktober",
        "november",
        "december",
    ],
    weekdays_short: ["zo", "ma", "di", "wo", "do", "vr", "za"],
    weekdays_long: [
        "zondag",
        "maandag",
        "dinsdag",
        "woensdag",
        "donderdag",
        "vrijdag",
        "zaterdag",
    ],
};

const PL_PL: LocaleNames = LocaleNames {
    months_short: [
        "sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru",
    ],
    months_long: [
        "styczeń",
        "luty",
        "marzec",
        "kwiecień",
        "maj",
        "czerwiec",
        "lipiec",
        "sierpień",
        "wrzesień",
        "październik",
        "listopad",
        "grudzień",
    ],
    weekdays_short: ["niedz.", "pon.", "wt.", "śr.", "czw.", "pt.", "sob."],
    weekdays_long: [
        "niedziela",
        "poniedziałek",
        "wtorek",
        "środa",
        "czwartek",
        "piątek",
        "sobota",
    ],
};

const PT_BR: LocaleNames = LocaleNames {
    months_short: PT_PT.months_short,
    months_long: PT_PT.months_long,
    weekdays_short: ["dom.", "seg.", "ter.", "qua.", "qui.", "sex.", "sáb."],
    weekdays_long: PT_PT.weekdays_long,
};

const JA_JP: LocaleNames = LocaleNames {
    months_short: [
        "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
    ],
    months_long: [
        "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
    ],
    weekdays_short: ["日", "月", "火", "水", "木", "金", "土"],
    weekdays_long: [
        "日曜日",
        "月曜日",
        "火曜日",
        "水曜日",
        "木曜日",
        "金曜日",
        "土曜日",
    ],
};

const KO_KR: LocaleNames = LocaleNames {
    months_short: [
        "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
    ],
    months_long: [
        "1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월",
    ],
    weekdays_short: ["일", "월", "화", "수", "목", "금", "토"],
    weekdays_long: [
        "일요일",
        "월요일",
        "화요일",
        "수요일",
        "목요일",
        "금요일",
        "토요일",
    ],
};

const ZH_CN: LocaleNames = LocaleNames {
    months_short: [
        "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月",
    ],
    months_long: [
        "一月",
        "二月",
        "三月",
        "四月",
        "五月",
        "六月",
        "七月",
        "八月",
        "九月",
        "十月",
        "十一月",
        "十二月",
    ],
    weekdays_short: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
    weekdays_long: [
        "星期日",
        "星期一",
        "星期二",
        "星期三",
        "星期四",
        "星期五",
        "星期六",
    ],
};

const HU_HU: LocaleNames = LocaleNames {
    months_short: [
        "jan.", "febr.", "márc.", "ápr.", "máj.", "jún.", "júl.", "aug.", "szept.", "okt.", "nov.",
        "dec.",
    ],
    months_long: [
        "január",
        "február",
        "március",
        "április",
        "május",
        "június",
        "július",
        "augusztus",
        "szeptember",
        "október",
        "november",
        "december",
    ],
    weekdays_short: ["V", "H", "K", "Sze", "Cs", "P", "Szo"],
    weekdays_long: [
        "vasárnap",
        "hétfő",
        "kedd",
        "szerda",
        "csütörtök",
        "péntek",
        "szombat",
    ],
};
