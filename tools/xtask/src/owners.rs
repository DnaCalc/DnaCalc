//! The work package that owns a worksheet function, for the triage of a refusal that names a
//! function but no owner (bead `dc-vcs`).
//!
//! CalcFml's `CompileError::Unsupported` names the bead that owns the construct it refuses, so the
//! triage takes that owner as it is. A kernel's `FunctionFault::NotImplemented`, a function the
//! catalogue does not hold (`CompileError::UnknownFunction`) and the other refusals of a catalogued
//! function name no owner. For those this table gives the work package whose scope holds the
//! function, by Excel's function category. The bulk-port packages F1, F2, F3, L1 and L2
//! (`docs/plan/BULK_PORT_PLAN.md` section 4) are closed, and their gaps passed to these owners:
//!
//! - F1's categories, exactness `dc-j9k`: math and trigonometry, statistics (with the
//!   compatibility functions), engineering, and the arithmetic operators.
//! - F2's categories, exactness `dc-uar`: text, date and time, information, logical, lookup and
//!   reference, database, web and cube functions, and the concatenation and comparison operators.
//! - F3's categories, exactness `dc-j9k`: financial functions, the dynamic-array functions and the
//!   LAMBDA helpers.
//! - L1's, L3 `dc-m3u`: LET and LAMBDA, which the plan puts in the formula language.
//! - L2's, L3 `dc-m3u`: INDIRECT and OFFSET, whose dynamic dependencies the plan puts in the
//!   engine.
//!
//! The table is this repository's reading of the plan, not a statement the refusal makes, and a
//! name in no list (a user-defined or add-in function, or a name Excel does not know) is owned by
//! none: [`UNASSIGNED`].

/// The owner of a function in none of the lists.
pub(crate) const UNASSIGNED: &str = "no package (a name in no function category)";

const F1: &str = "exactness dc-j9k";
const F2: &str = "exactness dc-uar";
const F3: &str = "exactness dc-j9k";
const L1: &str = "L3 dc-m3u";
const L2: &str = "L3 dc-m3u";

const MATH: &str = "ABS ACOS ACOSH ACOT ACOTH AGGREGATE ARABIC ASIN ASINH ATAN ATAN2 ATANH BASE \
    CEILING CEILING.MATH CEILING.PRECISE COMBIN COMBINA COS COSH COT COTH CSC CSCH DECIMAL DEGREES \
    EVEN EXP FACT FACTDOUBLE FLOOR FLOOR.MATH FLOOR.PRECISE GCD INT ISO.CEILING LCM LN LOG LOG10 \
    MDETERM MINVERSE MMULT MOD MROUND MULTINOMIAL MUNIT ODD PI POWER PRODUCT QUOTIENT RADIANS RAND \
    RANDARRAY RANDBETWEEN ROMAN ROUND ROUNDDOWN ROUNDUP SEC SECH SERIESSUM SIGN SIN SINH SQRT \
    SQRTPI SUBTOTAL SUM SUMIF SUMIFS SUMPRODUCT SUMSQ SUMX2MY2 SUMX2PY2 SUMXMY2 TAN TANH TRUNC \
    + - * / ^ %";

const STATISTICS: &str = "AVEDEV AVERAGE AVERAGEA AVERAGEIF AVERAGEIFS BETA.DIST BETA.INV \
    BINOM.DIST BINOM.DIST.RANGE BINOM.INV CHISQ.DIST CHISQ.DIST.RT CHISQ.INV CHISQ.INV.RT \
    CHISQ.TEST CONFIDENCE.NORM CONFIDENCE.T CORREL COUNT COUNTA COUNTBLANK COUNTIF COUNTIFS \
    COVARIANCE.P COVARIANCE.S DEVSQ EXPON.DIST F.DIST F.DIST.RT F.INV F.INV.RT F.TEST FISHER \
    FISHERINV FORECAST FORECAST.ETS FORECAST.ETS.CONFINT FORECAST.ETS.SEASONALITY \
    FORECAST.ETS.STAT FORECAST.LINEAR FREQUENCY GAMMA GAMMA.DIST GAMMA.INV GAMMALN \
    GAMMALN.PRECISE GAUSS GEOMEAN GROWTH HARMEAN HYPGEOM.DIST INTERCEPT KURT LARGE LINEST LOGEST \
    LOGNORM.DIST LOGNORM.INV MAX MAXA MAXIFS MEDIAN MIN MINA MINIFS MODE.MULT MODE.SNGL \
    NEGBINOM.DIST NORM.DIST NORM.INV NORM.S.DIST NORM.S.INV PEARSON PERCENTILE.EXC PERCENTILE.INC \
    PERCENTRANK.EXC PERCENTRANK.INC PERMUT PERMUTATIONA PHI POISSON.DIST PROB QUARTILE.EXC \
    QUARTILE.INC RANK.AVG RANK.EQ RSQ SKEW SKEW.P SLOPE SMALL STANDARDIZE STDEV.P STDEV.S STDEVA \
    STDEVPA STEYX T.DIST T.DIST.2T T.DIST.RT T.INV T.INV.2T T.TEST TREND TRIMMEAN VAR.P VAR.S \
    VARA VARPA WEIBULL.DIST Z.TEST BETADIST BETAINV BINOMDIST CHIDIST CHIINV CHITEST CONFIDENCE \
    COVAR CRITBINOM EXPONDIST FDIST FINV FTEST GAMMADIST GAMMAINV HYPGEOMDIST LOGINV LOGNORMDIST \
    MODE NEGBINOMDIST NORMDIST NORMINV NORMSDIST NORMSINV PERCENTILE PERCENTRANK POISSON QUARTILE \
    RANK STDEV STDEVP TDIST TINV TTEST VAR VARP WEIBULL ZTEST";

const ENGINEERING: &str = "BESSELI BESSELJ BESSELK BESSELY BIN2DEC BIN2HEX BIN2OCT BITAND \
    BITLSHIFT BITOR BITRSHIFT BITXOR COMPLEX CONVERT DEC2BIN DEC2HEX DEC2OCT DELTA ERF ERF.PRECISE \
    ERFC ERFC.PRECISE GESTEP HEX2BIN HEX2DEC HEX2OCT IMABS IMAGINARY IMARGUMENT IMCONJUGATE IMCOS \
    IMCOSH IMCOT IMCSC IMCSCH IMDIV IMEXP IMLN IMLOG10 IMLOG2 IMPOWER IMPRODUCT IMREAL IMSEC \
    IMSECH IMSIN IMSINH IMSQRT IMSUB IMSUM IMTAN OCT2BIN OCT2DEC OCT2HEX";

const TEXT: &str = "ARRAYTOTEXT ASC BAHTTEXT CHAR CLEAN CODE CONCAT CONCATENATE DBCS DOLLAR EXACT \
    FIND FINDB FIXED JIS LEFT LEFTB LEN LENB LOWER MID MIDB NUMBERVALUE PHONETIC PROPER \
    REGEXEXTRACT REGEXREPLACE REGEXTEST REPLACE REPLACEB REPT RIGHT RIGHTB SEARCH SEARCHB \
    SUBSTITUTE T TEXT TEXTAFTER TEXTBEFORE TEXTJOIN TEXTSPLIT TRIM UNICHAR UNICODE UPPER VALUE \
    VALUETOTEXT TRANSLATE DETECTLANGUAGE & = <> < > <= >=";

const DATE_TIME: &str = "DATE DATEDIF DATEVALUE DAY DAYS DAYS360 EDATE EOMONTH HOUR ISOWEEKNUM \
    MINUTE MONTH NETWORKDAYS NETWORKDAYS.INTL NOW SECOND TIME TIMEVALUE TODAY WEEKDAY WEEKNUM \
    WORKDAY WORKDAY.INTL YEAR YEARFRAC";

const INFORMATION_LOGICAL: &str = "CELL ERROR.TYPE INFO ISBLANK ISERR ISERROR ISEVEN ISFORMULA \
    ISLOGICAL ISNA ISNONTEXT ISNUMBER ISODD ISREF ISTEXT N NA SHEET SHEETS TYPE AND FALSE IF \
    IFERROR IFNA IFS NOT OR SWITCH TRUE XOR";

const LOOKUP_DATABASE_WEB: &str = "ADDRESS AREAS CHOOSE COLUMN COLUMNS FORMULATEXT GETPIVOTDATA \
    HLOOKUP HYPERLINK IMAGE INDEX LOOKUP MATCH ROW ROWS RTD TRANSPOSE VLOOKUP XLOOKUP XMATCH \
    DAVERAGE DCOUNT DCOUNTA DGET DMAX DMIN DPRODUCT DSTDEV DSTDEVP DSUM DVAR DVARP ENCODEURL \
    FILTERXML WEBSERVICE CUBEKPIMEMBER CUBEMEMBER CUBEMEMBERPROPERTY CUBERANKEDMEMBER CUBESET \
    CUBESETCOUNT CUBEVALUE";

const FINANCIAL: &str = "ACCRINT ACCRINTM AMORDEGRC AMORLINC COUPDAYBS COUPDAYS COUPDAYSNC \
    COUPNCD COUPNUM COUPPCD CUMIPMT CUMPRINC DB DDB DISC DOLLARDE DOLLARFR DURATION EFFECT FV \
    FVSCHEDULE INTRATE IPMT IRR ISPMT MDURATION MIRR NOMINAL NPER NPV ODDFPRICE ODDFYIELD \
    ODDLPRICE ODDLYIELD PDURATION PMT PPMT PRICE PRICEDISC PRICEMAT PV RATE RECEIVED RRI SLN SYD \
    TBILLEQ TBILLPRICE TBILLYIELD VDB XIRR XNPV YIELD YIELDDISC YIELDMAT";

const ARRAYS_HELPERS: &str = "SEQUENCE FILTER SORT SORTBY UNIQUE VSTACK HSTACK TOCOL TOROW \
    WRAPROWS WRAPCOLS TAKE DROP CHOOSEROWS CHOOSECOLS EXPAND GROUPBY PIVOTBY TRIMRANGE MAP REDUCE \
    SCAN BYROW BYCOL MAKEARRAY ISOMITTED";

const LANGUAGE: &str = "LET LAMBDA";

const ENGINE: &str = "INDIRECT OFFSET";

/// The lists in the order they are searched, with their owner.
const TABLE: [(&str, &str); 11] = [
    (ENGINE, L2),
    (LANGUAGE, L1),
    (MATH, F1),
    (STATISTICS, F1),
    (ENGINEERING, F1),
    (TEXT, F2),
    (DATE_TIME, F2),
    (INFORMATION_LOGICAL, F2),
    (LOOKUP_DATABASE_WEB, F2),
    (FINANCIAL, F3),
    (ARRAYS_HELPERS, F3),
];

/// The prefixes a file stores before a function's name (`_xlfn._xlws.SORT`); a user-defined
/// function's `_xludf.` is not one of them, so such a name stays unassigned.
const FILE_PREFIXES: [&str; 2] = ["_XLFN.", "_XLWS."];

/// The work package that owns the function or operator spelled `spelling` (any case, with or
/// without the file prefixes), or [`UNASSIGNED`].
pub(crate) fn function_owner(spelling: &str) -> &'static str {
    let mut name = spelling.trim().to_ascii_uppercase();
    while let Some(rest) = FILE_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
    {
        name = rest.to_owned();
    }
    TABLE
        .iter()
        .find(|(list, _)| list.split_whitespace().any(|entry| entry == name))
        .map_or(UNASSIGNED, |(_, owner)| owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn functions_map_to_the_package_of_their_category() {
        for (spelling, owner) in [
            ("SUM", F1),
            ("norm.s.dist", F1),
            ("IMAGINARY", F1),
            ("+", F1),
            ("NOT", F2),
            ("VLOOKUP", F2),
            ("_xlfn.XLOOKUP", F2),
            ("&", F2),
            ("PMT", F3),
            ("_xlfn._xlws.SORT", F3),
            ("LAMBDA", L1),
            ("INDIRECT", L2),
            ("OFFSET", L2),
            ("_xludf.MYFUNC", UNASSIGNED),
            ("NOSUCHFUNCTION", UNASSIGNED),
        ] {
            assert_eq!(function_owner(spelling), owner, "{spelling}");
        }
    }

    /// No name is listed twice, so the search order never decides an owner.
    #[test]
    fn no_function_has_two_owners() {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for (list, owner) in TABLE {
            for name in list.split_whitespace() {
                if let Some(first) = seen.insert(name, owner) {
                    panic!("{name} is listed under {first} and {owner}");
                }
            }
        }
        assert!(seen.len() > 450, "{} names", seen.len());
    }

    /// Every function the catalogue holds has an owner.
    #[test]
    fn every_catalogued_function_has_an_owner() {
        let unowned: Vec<String> = calc_func::catalogue()
            .targets()
            .map(|target| target.spelling().to_owned())
            .filter(|spelling| function_owner(spelling) == UNASSIGNED)
            .collect();
        assert!(unowned.is_empty(), "{unowned:?}");
    }
}
