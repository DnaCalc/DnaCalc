//! Function declaration fragments moved from CalcFunc. The executable kernels stay there.
use crate::{ArgumentDemand, ArgumentGroups, Availability, EntryClass, EntryTraits, FunctionId, LegacyPlace, Notation, Parameter, ParameterClass, ResultForm, ThreeD, WorksheetDeclaration};

macro_rules! data_family {
    (code: $code:expr; $(
        $member:literal $spelling:literal ($min:literal, $max:literal)
        [$($class:ident $pname:literal $(@ $entry:ident)? $(~ $legacy:ident)?),*]
        $(groups ($gfrom:literal, $gsize:literal, $gtail:literal);)?
        $(result $result:ident;)? $(availability $availability:ident;)?
        $(traits $($traits:ident),+;)? $(volatile $volatility:literal;)?
        $(host_route ($provider:expr, $fetch_kind:expr);)?
        $(import_route ($iprovider:expr, $import_kind:expr);)?
        $(three_d ($take:expr, $value:expr, $reference:expr);)? ;
    )*) => {
        pub const ENTRIES: &[WorksheetDeclaration] = &[$({
            #[allow(unused_mut)]
            let mut item = WorksheetDeclaration {
                id: FunctionId::new($code, $member), kind: crate::EntryKind::Worksheet, spelling: $spelling, notation: Notation::Call,
                availability: Availability::Unspecified, arity: calc_value::CallableArityShape::range($min, $max),
                groups: None, parameters: &[$(Parameter::of($pname, data_family!(@class $class))$(.entry(EntryClass::$entry))?$(.legacy(LegacyPlace::$legacy))?),*],
                result: ResultForm::Value, demand: ArgumentDemand::Eager, traits: EntryTraits::NONE,
            };
            $(item.groups = Some(ArgumentGroups::new($gfrom, $gsize, $gtail));)?
            $(item.result = ResultForm::$result;)? $(item.availability = Availability::$availability;)?
            $(item.traits = EntryTraits::NONE$(.$traits())+;)?
            $(item.traits.volatile = $volatility;)?
            $(item.kind = crate::EntryKind::HostRoute { provider: $provider, route: crate::HostRouteKind::Fetch($fetch_kind) };)?
            $(item.kind = crate::EntryKind::HostRoute { provider: $iprovider, route: crate::HostRouteKind::Import($import_kind) };)?
            $(item.traits.three_d = ThreeD::new($take, $value, $reference);)?
            item
        }),*];
    };
    (@class V) => { ParameterClass::Value };
    (@class R) => { ParameterClass::ValueOrReference };
    (@class F) => { ParameterClass::Reference };
}

pub const fn find(entries: &'static [WorksheetDeclaration], id: FunctionId) -> WorksheetDeclaration {
    let mut i = 0;
    while i < entries.len() {
        if entries[i].id.raw() == id.raw() { return entries[i]; }
        i += 1;
    }
    panic!("kernel has no declaration")
}

pub mod distributions {
    use super::*;
    pub const CODE: u16 = 104;
    data_family! {
        code: CODE;
    1 "NORM.DIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] three_d (None, None, Some(&[])); ;
    2 "NORMDIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] three_d (None, None, Some(&[])); ;
    3 "NORM.INV" (3, 3) [V "probability", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    4 "NORMINV" (3, 3) [V "probability", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    5 "NORM.S.DIST" (2, 2) [V "z", V "cumulative"] three_d (None, None, Some(&[])); ;
    6 "NORMSDIST" (1, 1) [V "z"] three_d (None, None, Some(&[])); ;
    7 "NORM.S.INV" (1, 1) [V "probability"] three_d (None, None, Some(&[])); ;
    8 "NORMSINV" (1, 1) [V "probability"] three_d (None, None, Some(&[])); ;
    9 "LOGNORM.DIST" (4, 4) [V "x", V "mean", V "standard_dev", V "cumulative"] three_d (None, None, Some(&[])); ;
    10 "LOGNORMDIST" (3, 3) [V "x", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    11 "LOGNORM.INV" (3, 3) [V "probability", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    12 "LOGINV" (3, 3) [V "probability", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    13 "CONFIDENCE" (3, 3) [V "alpha", V "standard_dev", V "size"] three_d (None, None, Some(&[])); ;
    14 "CONFIDENCE.NORM" (3, 3) [V "alpha", V "standard_dev", V "size"] three_d (None, None, Some(&[])); ;
    15 "CONFIDENCE.T" (3, 3) [V "alpha", V "standard_dev", V "size"] three_d (None, None, Some(&[])); ;
    16 "GAUSS" (1, 1) [V "z"] three_d (None, None, Some(&[])); ;
    17 "PHI" (1, 1) [V "x"] three_d (None, None, Some(&[])); ;
    20 "CHISQ.DIST" (3, 3) [V "x", V "deg_freedom", V "cumulative"] three_d (None, None, Some(&[])); ;
    21 "CHISQ.DIST.RT" (2, 2) [V "x", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    22 "CHIDIST" (2, 2) [V "x", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    23 "CHISQ.INV" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    24 "CHISQ.INV.RT" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    25 "CHIINV" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    30 "F.DIST" (4, 4) [V "x", V "deg_freedom1", V "deg_freedom2", V "cumulative"] three_d (None, None, Some(&[])); ;
    31 "F.DIST.RT" (3, 3) [V "x", V "deg_freedom1", V "deg_freedom2"] three_d (None, None, Some(&[])); ;
    32 "FDIST" (3, 3) [V "x", V "deg_freedom1", V "deg_freedom2"] three_d (None, None, Some(&[])); ;
    33 "F.INV" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] three_d (None, None, Some(&[])); ;
    34 "F.INV.RT" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] three_d (None, None, Some(&[])); ;
    35 "FINV" (3, 3) [V "probability", V "deg_freedom1", V "deg_freedom2"] three_d (None, None, Some(&[])); ;
    40 "T.DIST" (3, 3) [V "x", V "deg_freedom", V "cumulative"] three_d (None, None, Some(&[])); ;
    41 "T.DIST.RT" (2, 2) [V "x", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    42 "T.DIST.2T" (2, 2) [V "x", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    43 "TDIST" (3, 3) [V "x", V "deg_freedom", V "tails"] three_d (None, None, Some(&[])); ;
    44 "T.INV" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    45 "T.INV.2T" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    46 "TINV" (2, 2) [V "probability", V "deg_freedom"] three_d (None, None, Some(&[])); ;
    50 "BETA.DIST" (4, 6) [V "x", V "alpha", V "beta", V "cumulative", V "A", V "B"] three_d (None, None, Some(&[])); ;
    51 "BETADIST" (3, 5) [V "x", V "alpha", V "beta", V "A", V "B"] three_d (None, None, Some(&[])); ;
    52 "BETA.INV" (3, 5) [V "probability", V "alpha", V "beta", V "A", V "B"] three_d (None, None, Some(&[])); ;
    53 "BETAINV" (3, 5) [V "probability", V "alpha", V "beta", V "A", V "B"] three_d (None, None, Some(&[])); ;
    54 "GAMMA.DIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] three_d (None, None, Some(&[])); ;
    55 "GAMMADIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] three_d (None, None, Some(&[])); ;
    56 "GAMMA.INV" (3, 3) [V "probability", V "alpha", V "beta"] three_d (None, None, Some(&[])); ;
    57 "GAMMAINV" (3, 3) [V "probability", V "alpha", V "beta"] three_d (None, None, Some(&[])); ;
    58 "GAMMA" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    59 "GAMMALN" (1, 1) [V "x"] three_d (None, None, Some(&[])); ;
    60 "GAMMALN.PRECISE" (1, 1) [V "x"] three_d (None, None, Some(&[])); ;
    61 "WEIBULL.DIST" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] three_d (None, None, Some(&[])); ;
    62 "WEIBULL" (4, 4) [V "x", V "alpha", V "beta", V "cumulative"] three_d (None, None, Some(&[])); ;
    63 "EXPON.DIST" (3, 3) [V "x", V "lambda", V "cumulative"] three_d (None, None, Some(&[])); ;
    64 "EXPONDIST" (3, 3) [V "x", V "lambda", V "cumulative"] three_d (None, None, Some(&[])); ;
    70 "BINOM.DIST" (4, 4) [V "number_s", V "trials", V "probability_s", V "cumulative"] three_d (None, None, Some(&[])); ;
    71 "BINOMDIST" (4, 4) [V "number_s", V "trials", V "probability_s", V "cumulative"] three_d (None, None, Some(&[])); ;
    72 "BINOM.DIST.RANGE" (3, 4) [V "trials", V "probability_s", V "number_s", V "number_s2"] three_d (None, None, Some(&[])); ;
    73 "BINOM.INV" (3, 3) [V "trials", V "probability_s", V "alpha"] three_d (None, None, Some(&[])); ;
    74 "CRITBINOM" (3, 3) [V "trials", V "probability_s", V "alpha"] three_d (None, None, Some(&[])); ;
    75 "POISSON.DIST" (3, 3) [V "x", V "mean", V "cumulative"] three_d (None, None, Some(&[])); ;
    76 "POISSON" (3, 3) [V "x", V "mean", V "cumulative"] three_d (None, None, Some(&[])); ;
    77 "HYPGEOM.DIST" (5, 5) [V "sample_s", V "number_sample", V "population_s", V "number_pop", V "cumulative"] three_d (None, None, Some(&[])); ;
    78 "HYPGEOMDIST" (4, 4) [V "sample_s", V "number_sample", V "population_s", V "number_pop"] three_d (None, None, Some(&[])); ;
    79 "NEGBINOM.DIST" (4, 4) [V "number_f", V "number_s", V "probability_s", V "cumulative"] three_d (None, None, Some(&[])); ;
    80 "NEGBINOMDIST" (3, 3) [V "number_f", V "number_s", V "probability_s"] three_d (None, None, Some(&[])); ;
    90 "ERF" (1, 2) [V "lower_limit", V "upper_limit"] three_d (None, Some(&[]), None); ;
    91 "ERF.PRECISE" (1, 1) [V "x"] three_d (None, Some(&[]), None); ;
    92 "ERFC" (1, 1) [V "x"] three_d (None, Some(&[]), None); ;
    93 "ERFC.PRECISE" (1, 1) [V "x"] three_d (None, Some(&[]), None); ;
    100 "CHISQ.TEST" (2, 2) [R "actual_range" ~ ArrayAtRoot, R "expected_range" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    101 "CHITEST" (2, 2) [R "actual_range" ~ ArrayAtRoot, R "expected_range" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    102 "F.TEST" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    103 "FTEST" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    104 "T.TEST" (4, 4) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot, V "tails", V "type"] three_d (None, None, Some(&[])); ;
    105 "TTEST" (4, 4) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot, V "tails", V "type"] three_d (None, None, Some(&[])); ;
    106 "Z.TEST" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "sigma"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    107 "ZTEST" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "sigma"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    }
}

pub mod bonds {
    use super::*;
    pub const CODE: u16 = 301;
    data_family! {
        code: CODE;
    1 "PRICE" (6, 7) [V "settlement", V "maturity", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    2 "YIELD" (6, 7) [V "settlement", V "maturity", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    3 "DURATION" (5, 6) [V "settlement", V "maturity", V "coupon", V "yld", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    4 "MDURATION" (5, 6) [V "settlement", V "maturity", V "coupon", V "yld", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    5 "PRICEMAT" (5, 6) [V "settlement", V "maturity", V "issue", V "rate", V "yld", V "basis"] three_d (None, Some(&[]), None); ;
    6 "YIELDMAT" (5, 6) [V "settlement", V "maturity", V "issue", V "rate", V "pr", V "basis"] three_d (None, Some(&[]), None); ;
    7 "YIELDDISC" (4, 5) [V "settlement", V "maturity", V "pr", V "redemption", V "basis"] three_d (None, Some(&[]), None); ;
    8 "ACCRINT" (6, 8) [V "issue", V "first_interest", V "settlement", V "rate", V "par", V "frequency", V "basis", V "calc_method"] three_d (None, Some(&[]), None); ;
    9 "ACCRINTM" (4, 5) [V "issue", V "settlement", V "rate", V "par", V "basis"] three_d (None, Some(&[3]), None); ;
    10 "COUPDAYBS" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    11 "COUPDAYS" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    12 "COUPDAYSNC" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    13 "COUPNCD" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    14 "COUPNUM" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    15 "COUPPCD" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); ;
    16 "DISC" (4, 5) [V "settlement", V "maturity", V "pr", V "redemption", V "basis"] three_d (None, Some(&[]), None); ;
    17 "INTRATE" (4, 5) [V "settlement", V "maturity", V "investment", V "redemption", V "basis"] three_d (None, Some(&[]), None); ;
    18 "RECEIVED" (4, 5) [V "settlement", V "maturity", V "investment", V "discount", V "basis"] three_d (None, Some(&[]), None); ;
    19 "PRICEDISC" (4, 5) [V "settlement", V "maturity", V "discount", V "redemption", V "basis"] three_d (None, Some(&[]), None); ;
    20 "TBILLEQ" (3, 3) [V "settlement", V "maturity", V "discount"] three_d (None, Some(&[]), None); ;
    21 "TBILLPRICE" (3, 3) [V "settlement", V "maturity", V "discount"] three_d (None, Some(&[]), None); ;
    22 "TBILLYIELD" (3, 3) [V "settlement", V "maturity", V "pr"] three_d (None, Some(&[]), None); ;
    23 "ODDFPRICE" (8, 9) [V "settlement", V "maturity", V "issue", V "first_coupon", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    24 "ODDFYIELD" (8, 9) [V "settlement", V "maturity", V "issue", V "first_coupon", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    25 "ODDLPRICE" (7, 8) [V "settlement", V "maturity", V "last_interest", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    26 "ODDLYIELD" (7, 8) [V "settlement", V "maturity", V "last_interest", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); ;
    }
}

pub mod math {
    use super::*;
    pub const CODE: u16 = 100;
    data_family! {
        code: CODE;
    1 "ACOS" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    2 "ACOSH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    3 "ACOT" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    4 "ACOTH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    5 "ASIN" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    6 "ASINH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    7 "ATAN" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    8 "ATAN2" (2, 2) [V "x_num", V "y_num"] three_d (None, None, Some(&[])); ;
    9 "ATANH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    10 "COS" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    11 "COSH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    12 "COT" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    13 "COTH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    14 "CSC" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    15 "CSCH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    16 "SEC" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    17 "SECH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    18 "SIN" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    19 "SINH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    20 "TAN" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    21 "TANH" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    22 "DEGREES" (1, 1) [V "angle"] three_d (None, None, Some(&[])); ;
    23 "RADIANS" (1, 1) [V "angle"] three_d (None, None, Some(&[])); ;
    24 "PI" (0, 0) [] ;
    25 "EXP" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    26 "LN" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    27 "LOG" (1, 2) [V "number", V "base"] three_d (None, None, Some(&[])); ;
    28 "LOG10" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    29 "SQRT" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    30 "SQRTPI" (1, 1) [V "number"] three_d (None, Some(&[]), None); ;
    31 "POWER" (2, 2) [V "number", V "power"] three_d (None, None, Some(&[])); ;
    32 "FACT" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    33 "FACTDOUBLE" (1, 1) [V "number"] three_d (None, Some(&[]), None); ;
    34 "COMBIN" (2, 2) [V "number", V "number_chosen"] three_d (None, None, Some(&[])); ;
    35 "COMBINA" (2, 2) [V "number", V "number_chosen"] three_d (None, None, Some(&[])); ;
    36 "PERMUT" (2, 2) [V "number", V "number_chosen"] three_d (None, None, Some(&[])); ;
    37 "PERMUTATIONA" (2, 2) [V "number", V "number_chosen"] three_d (None, None, Some(&[])); ;
    38 "GCD" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (None, Some(&[]), None); ;
    39 "LCM" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (None, Some(&[]), None); ;
    40 "MULTINOMIAL" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (None, Some(&[]), None); ;
    41 "ARABIC" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    42 "ROMAN" (1, 2) [V "number", V "form"] three_d (None, None, Some(&[])); ;
    43 "BASE" (2, 3) [V "number", V "radix", V "min_length"] three_d (None, None, Some(&[])); ;
    44 "DECIMAL" (2, 2) [V "text", V "radix"] three_d (None, None, Some(&[])); ;
    45 "SERIESSUM" (4, 4) [V "x", V "n", V "m", R "coefficients" ~ AggregateAtRoot] three_d (None, Some(&[]), None); ;
    46 "SUMX2MY2" (2, 2) [R "array_x" ~ ArrayAtRoot, R "array_y" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    47 "SUMX2PY2" (2, 2) [R "array_x" ~ ArrayAtRoot, R "array_y" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    48 "SUMXMY2" (2, 2) [R "array_x" ~ ArrayAtRoot, R "array_y" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    49 "MDETERM" (1, 1) [R "array" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    50 "MINVERSE" (1, 1) [R "array" ~ ArrayAlways] three_d (None, None, Some(&[])); ;
    51 "MMULT" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    52 "MUNIT" (1, 1) [V "dimension"] three_d (None, None, Some(&[])); ;
    }
}

pub mod lambda_helpers {
    use super::*;
    pub const CODE: u16 = 311;
    data_family! {
        code: CODE;
    1 "MAP" (2, 254) [R "array1" ~ ArrayAtRoot, R "lambda_or_array2" ~ AggregateAtRootTwice] three_d (None, Some(&[0]), Some(&[1, usize::MAX]));  ;
    2 "REDUCE" (3, 3) [R "initial_value" ~ AggregateAtRoot, R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] result MayBeEither; three_d (None, Some(&[1]), Some(&[0, 2]));  ;
    3 "SCAN" (3, 3) [R "initial_value" ~ AggregateAtRoot, R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] three_d (None, Some(&[1]), Some(&[0, 2]));  ;
    4 "BYROW" (1, 2) [R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] three_d (None, Some(&[0]), Some(&[1]));  ;
    5 "BYCOL" (1, 2) [R "array" ~ ArrayAtRoot, R "lambda" ~ AggregateAtRoot] three_d (None, Some(&[0]), Some(&[1]));  ;
    6 "MAKEARRAY" (3, 3) [V "rows", V "cols", R "lambda" ~ AggregateAtRoot] three_d (None, None, Some(&[]));  ;
    }
}

pub mod statistics {
    use super::*;
    pub const CODE: u16 = 103;
    data_family! {
        code: CODE;
    1 "AVEDEV" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    2 "DEVSQ" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    3 "GEOMEAN" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    4 "HARMEAN" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    5 "MEDIAN" (1, 255) [R "number1" ~ AggregateUnderArray, R "number2" ~ AggregateUnderArrayRepeated] three_d (Some(&[]), None, None); ;
    6 "MODE" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); ;
    7 "MODE.SNGL" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); ;
    8 "MODE.MULT" (1, 255) [R "number1" ~ ArrayAtRoot, R "number2" ~ ArrayAtRootRepeated] three_d (None, None, Some(&[])); ;
    9 "LARGE" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); ;
    10 "SMALL" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); ;
    11 "RANK" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); ;
    12 "RANK.EQ" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); ;
    13 "RANK.AVG" (2, 3) [V "number", F "ref" @Reference, V "order"] three_d (Some(&[1]), None, Some(&[0, 2])); ;
    14 "PERCENTILE" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); ;
    15 "PERCENTILE.INC" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); ;
    16 "PERCENTILE.EXC" (2, 2) [R "array" ~ AggregateAtRoot, V "k"] three_d (Some(&[0]), None, Some(&[1])); ;
    17 "QUARTILE" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); ;
    18 "QUARTILE.INC" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); ;
    19 "QUARTILE.EXC" (2, 2) [R "array" ~ AggregateAtRoot, V "quart"] three_d (Some(&[0]), None, Some(&[1])); ;
    20 "PERCENTRANK" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    21 "PERCENTRANK.INC" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    22 "PERCENTRANK.EXC" (2, 3) [R "array" ~ AggregateAtRoot, V "x", V "significance"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    23 "STDEV" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    24 "STDEV.S" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    25 "STDEV.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    26 "STDEVA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    27 "STDEVP" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    28 "STDEVPA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    29 "VAR" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    30 "VAR.S" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    31 "VAR.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    32 "VARA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    33 "VARP" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    34 "VARPA" (1, 255) [R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    35 "COVAR" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    36 "COVARIANCE.P" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    37 "COVARIANCE.S" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    38 "CORREL" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    39 "PEARSON" (2, 2) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    40 "RSQ" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    41 "SLOPE" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    42 "INTERCEPT" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    43 "STEYX" (2, 2) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    44 "FORECAST" (3, 3) [V "x", R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    45 "FORECAST.LINEAR" (3, 3) [V "x", R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot] three_d (None, None, Some(&[])); ;
    46 "KURT" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    47 "SKEW" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    48 "SKEW.P" (1, 255) [R "number1" ~ AggregateAtRoot, R "number2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    49 "TRIMMEAN" (2, 2) [R "array" ~ AggregateAtRoot, V "percent"] three_d (Some(&[0]), None, Some(&[1])); ;
    50 "STANDARDIZE" (3, 3) [V "x", V "mean", V "standard_dev"] three_d (None, None, Some(&[])); ;
    51 "FISHER" (1, 1) [V "x"] three_d (None, None, Some(&[])); ;
    52 "FISHERINV" (1, 1) [V "y"] three_d (None, None, Some(&[])); ;
    53 "PROB" (3, 4) [R "x_range" ~ ArrayAtRoot, R "prob_range" ~ ArrayAtRoot, V "lower_limit", V "upper_limit"] three_d (None, None, Some(&[])); ;
    54 "FREQUENCY" (2, 2) [R "data_array" ~ ArrayAtRoot, R "bins_array" ~ ArrayAtRoot] three_d (Some(&[0, 1]), None, None); ;
    55 "TREND" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot, R "new_xs", V "const"] three_d (None, Some(&[]), None); ;
    56 "GROWTH" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAtRoot, R "new_xs", V "const"] three_d (None, Some(&[]), None); ;
    57 "LINEST" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAlways, V "const", V "stats"] three_d (None, Some(&[]), None); ;
    58 "LOGEST" (1, 4) [R "known_ys" ~ ArrayAtRoot, R "known_xs" ~ ArrayAlways, V "const", V "stats"] three_d (None, Some(&[]), None); ;
    59 "FORECAST.ETS" (3, 6) [V "target_date", R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); ;
    60 "FORECAST.ETS.CONFINT" (3, 7) [V "target_date", R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "confidence_level", V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); ;
    61 "FORECAST.ETS.SEASONALITY" (2, 4) [R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); ;
    62 "FORECAST.ETS.STAT" (3, 6) [R "values" ~ ArrayAlways, R "timeline" ~ ArrayAlways, V "statistic_type", V "seasonality", V "data_completion", V "aggregation"] three_d (None, None, Some(&[])); ;
    }
}

pub mod grouping {
    use super::*;
    pub const CODE: u16 = 321;
    data_family! {
        code: CODE;
    1 "GROUPBY" (3, 8) [R "row_fields" ~ ArrayAlways, R "values" ~ ArrayAlways, R "function", V "field_headers", V "total_depth", R "sort_order", R "filter_array" ~ ArrayAlways, V "field_relationship"] ;
    2 "PIVOTBY" (4, 11) [R "row_fields" ~ ArrayAlways, R "col_fields" ~ ArrayAlways, R "values" ~ ArrayAlways, R "function", V "field_headers", V "row_total_depth", R "row_sort_order", V "col_total_depth", R "col_sort_order", R "filter_array" ~ ArrayAlways, V "relative_to"] ;
    }
}

pub mod host_facts {
    use super::*;
    pub const CODE: u16 = 204;
    data_family! {
        code: CODE;
    1 "CELL" (1, 2) [V "info_type", F "reference" @Reference] traits lift_dynamic_references; volatile true; ;
    2 "INFO" (1, 1) [V "type_text"] volatile true; ;
    3 "SHEET" (0, 1) [R "value" ~ AggregateAtRoot] three_d (Some(&[0]), None, None); ;
    4 "SHEETS" (0, 1) [R "reference" ~ ArrayAtRoot] three_d (Some(&[0]), None, None); ;
    5 "FORMULATEXT" (1, 1) [F "reference" @Reference] three_d (None, Some(&[]), None); ;
    6 "ADDRESS" (2, 5) [V "row_num", V "column_num", V "abs_num", V "a1", V "sheet_text"] three_d (None, None, Some(&[])); ;
    7 "GETPIVOTDATA" (2, 254) [R "data_field", R "pivot_table", V "field1", V "item1"] ;
    8 "TRANSPOSE" (1, 1) [R "array" ~ AggregateAlwaysIntersectReference] three_d (None, None, Some(&[])); ;
    9 "FIELDVALUE" (2, 2) [V "value" ~ SingleValue, V "field_name" ~ SingleValue] ;
    }
}

pub mod financial {
    use super::*;
    pub const CODE: u16 = 300;
    data_family! {
        code: CODE;
    1 "PV" (3, 5) [V "rate", V "nper", V "pmt", V "fv", V "type"] three_d (None, None, Some(&[])); ;
    2 "FV" (3, 5) [V "rate", V "nper", V "pmt", V "pv", V "type"] three_d (None, None, Some(&[])); ;
    3 "PMT" (3, 5) [V "rate", V "nper", V "pv", V "fv", V "type"] three_d (None, None, Some(&[])); ;
    4 "NPER" (3, 5) [V "rate", V "pmt", V "pv", V "fv", V "type"] three_d (None, None, Some(&[])); ;
    5 "RATE" (3, 6) [V "nper", V "pmt", V "pv", V "fv", V "type", V "guess"] three_d (None, None, Some(&[])); ;
    6 "NPV" (2, 255) [V "rate" ~ AggregateAtRoot, R "value1" ~ AggregateAtRoot, R "value2" ~ AggregateAtRootRepeated] three_d (Some(&[1, usize::MAX]), None, Some(&[0])); ;
    7 "IPMT" (4, 6) [V "rate", V "per", V "nper", V "pv", V "fv", V "type"] three_d (None, None, Some(&[])); ;
    8 "PPMT" (4, 6) [V "rate", V "per", V "nper", V "pv", V "fv", V "type"] three_d (None, None, Some(&[])); ;
    9 "ISPMT" (4, 4) [V "rate", V "per", V "nper", V "pv"] three_d (None, None, Some(&[])); ;
    10 "MIRR" (3, 3) [R "values" ~ ArrayAtRoot, V "finance_rate", V "reinvest_rate"] three_d (Some(&[0]), None, Some(&[1, 2])); ;
    11 "FVSCHEDULE" (2, 2) [V "principal", R "schedule" ~ AggregateAtRoot] three_d (None, Some(&[]), None); ;
    12 "PDURATION" (3, 3) [V "rate", V "pv", V "fv"] three_d (None, None, Some(&[])); ;
    13 "RRI" (3, 3) [V "nper", V "pv", V "fv"] three_d (None, None, Some(&[])); ;
    14 "NOMINAL" (2, 2) [V "effect_rate", V "npery"] three_d (None, Some(&[]), None); ;
    15 "EFFECT" (2, 2) [V "nominal_rate", V "npery"] three_d (None, Some(&[]), None); ;
    16 "CUMIPMT" (6, 6) [V "rate", V "nper", V "pv", V "start_period", V "end_period", V "type"] three_d (None, Some(&[]), None); ;
    17 "CUMPRINC" (6, 6) [V "rate", V "nper", V "pv", V "start_period", V "end_period", V "type"] three_d (None, Some(&[]), None); ;
    18 "IRR" (1, 2) [R "values" ~ ArrayAtRoot, V "guess"] three_d (None, Some(&[0]), Some(&[1])); ;
    19 "XNPV" (3, 3) [V "rate", R "values" ~ AggregateAtRoot, R "dates" ~ AggregateAtRoot] three_d (None, Some(&[]), None); ;
    20 "XIRR" (2, 3) [R "values" ~ AggregateAtRoot, R "dates" ~ AggregateAtRoot, V "guess"] three_d (None, Some(&[]), None); ;
    21 "SLN" (3, 3) [V "cost", V "salvage", V "life"] three_d (None, None, Some(&[])); ;
    22 "SYD" (4, 4) [V "cost", V "salvage", V "life", V "per"] three_d (None, None, Some(&[])); ;
    23 "DB" (4, 5) [V "cost", V "salvage", V "life", V "period", V "month"] three_d (None, None, Some(&[])); ;
    24 "DDB" (4, 5) [V "cost", V "salvage", V "life", V "period", V "factor"] three_d (None, None, Some(&[])); ;
    25 "VDB" (5, 7) [V "cost", V "salvage", V "life", V "start_period", V "end_period", V "factor", V "no_switch"] three_d (None, None, Some(&[])); ;
    26 "AMORLINC" (6, 7) [V "cost", V "date_purchased", V "first_period", V "salvage", V "period", V "rate", V "basis"] three_d (None, Some(&[]), None); ;
    27 "AMORDEGRC" (6, 7) [V "cost", V "date_purchased", V "first_period", V "salvage", V "period", V "rate", V "basis"] three_d (None, Some(&[]), None); ;
    28 "DOLLARDE" (2, 2) [V "fractional_dollar", V "fraction"] three_d (None, Some(&[]), None); ;
    29 "DOLLARFR" (2, 2) [V "decimal_dollar", V "fraction"] three_d (None, Some(&[]), None); ;
    }
}

pub mod dates {
    use super::*;
    pub const CODE: u16 = 203;
    data_family! {
        code: CODE;
    1 "DATE" (3, 3) [V "year", V "month", V "day"] three_d (None, None, Some(&[])); ;
    2 "TIME" (3, 3) [V "hour", V "minute", V "second"] three_d (None, None, Some(&[])); ;
    3 "YEAR" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    4 "MONTH" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    5 "DAY" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    6 "HOUR" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    7 "MINUTE" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    8 "SECOND" (1, 1) [V "serial_number"] three_d (None, None, Some(&[])); ;
    9 "DAYS" (2, 2) [V "end_date", V "start_date"] three_d (None, None, Some(&[])); ;
    10 "DAYS360" (2, 3) [V "start_date", V "end_date", V "method"] three_d (None, None, Some(&[])); ;
    11 "DATEDIF" (3, 3) [V "start_date", V "end_date", V "unit"] three_d (None, None, Some(&[])); ;
    12 "DATEVALUE" (1, 1) [V "date_text"] three_d (None, None, Some(&[])); ;
    13 "TIMEVALUE" (1, 1) [V "time_text"] three_d (None, None, Some(&[])); ;
    14 "EDATE" (2, 2) [V "start_date", V "months"] three_d (None, Some(&[]), None); ;
    15 "EOMONTH" (2, 2) [V "start_date", V "months"] three_d (None, Some(&[]), None); ;
    16 "WEEKDAY" (1, 2) [V "serial_number", V "return_type"] three_d (None, None, Some(&[])); ;
    17 "WEEKNUM" (1, 2) [V "serial_number", V "return_type"] three_d (None, Some(&[]), None); ;
    18 "ISOWEEKNUM" (1, 1) [V "date"] three_d (None, None, Some(&[])); ;
    19 "WORKDAY" (2, 3) [V "start_date", V "days", R "holidays" ~ AggregateAtRoot] three_d (None, Some(&[]), None); ;
    20 "WORKDAY.INTL" (2, 4) [V "start_date", V "days", V "weekend", R "holidays"] three_d (None, Some(&[0, 1]), Some(&[2])); ;
    21 "NETWORKDAYS" (2, 3) [V "start_date", V "end_date", R "holidays" ~ AggregateAtRoot] three_d (None, Some(&[]), None); ;
    22 "NETWORKDAYS.INTL" (2, 4) [V "start_date", V "end_date", V "weekend", R "holidays"] three_d (None, Some(&[0, 1]), Some(&[2])); ;
    23 "YEARFRAC" (2, 3) [V "start_date", V "end_date", V "basis"] three_d (None, Some(&[]), None); ;
    24 "NOW" (0, 0) [] volatile true; ;
    25 "TODAY" (0, 0) [] volatile true; ;
    }
}

pub mod rounding_more {
    use super::*;
    pub const CODE: u16 = 101;
    data_family! {
        code: CODE;
    1 "CEILING.MATH" (1, 3) [V "number", V "significance", V "mode"] three_d (None, None, Some(&[])); ;
    2 "CEILING.PRECISE" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); ;
    3 "FLOOR.MATH" (1, 3) [V "number", V "significance", V "mode"] three_d (None, None, Some(&[])); ;
    4 "FLOOR.PRECISE" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); ;
    5 "ISO.CEILING" (1, 2) [V "number", V "significance"] three_d (None, None, Some(&[])); ;
    }
}

pub mod engineering {
    use super::*;
    pub const CODE: u16 = 105;
    data_family! {
        code: CODE;
    1 "BIN2DEC" (1, 1) [V "number"] three_d (None, Some(&[]), None); ;
    2 "BIN2HEX" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    3 "BIN2OCT" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    4 "DEC2BIN" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    5 "DEC2HEX" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    6 "DEC2OCT" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    7 "HEX2BIN" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    8 "HEX2DEC" (1, 1) [V "number"] three_d (None, Some(&[]), None); ;
    9 "HEX2OCT" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    10 "OCT2BIN" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    11 "OCT2DEC" (1, 1) [V "number"] three_d (None, Some(&[]), None); ;
    12 "OCT2HEX" (1, 2) [V "number", V "places"] three_d (None, Some(&[]), None); ;
    20 "BITAND" (2, 2) [V "number1", V "number2"] three_d (None, None, Some(&[])); ;
    21 "BITOR" (2, 2) [V "number1", V "number2"] three_d (None, None, Some(&[])); ;
    22 "BITXOR" (2, 2) [V "number1", V "number2"] three_d (None, None, Some(&[])); ;
    23 "BITLSHIFT" (2, 2) [V "number", V "shift_amount"] three_d (None, None, Some(&[])); ;
    24 "BITRSHIFT" (2, 2) [V "number", V "shift_amount"] three_d (None, None, Some(&[])); ;
    30 "COMPLEX" (2, 3) [V "real_num", V "i_num", V "suffix"] three_d (None, Some(&[]), None); ;
    31 "IMABS" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    32 "IMAGINARY" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    33 "IMARGUMENT" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    34 "IMREAL" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    35 "IMCONJUGATE" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    36 "IMCOS" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    37 "IMCOSH" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    38 "IMCOT" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    39 "IMCSC" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    40 "IMCSCH" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    41 "IMEXP" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    42 "IMLN" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    43 "IMLOG10" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    44 "IMLOG2" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    45 "IMSEC" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    46 "IMSECH" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    47 "IMSIN" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    48 "IMSINH" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    49 "IMSQRT" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    50 "IMTAN" (1, 1) [V "inumber"] three_d (None, Some(&[]), None); ;
    51 "IMDIV" (2, 2) [V "inumber1", V "inumber2"] three_d (None, Some(&[]), None); ;
    52 "IMPOWER" (2, 2) [V "inumber", V "number"] three_d (None, Some(&[]), None); ;
    53 "IMSUB" (2, 2) [V "inumber1", V "inumber2"] three_d (None, Some(&[]), None); ;
    54 "IMSUM" (1, 255) [R "inumber1" ~ AggregateAtRoot, R "inumber2" ~ AggregateAtRootRepeated] three_d (None, Some(&[]), None); ;
    55 "IMPRODUCT" (1, 255) [R "inumber1" ~ AggregateAtRoot, R "inumber2" ~ AggregateAtRootRepeated] three_d (None, Some(&[]), None); ;
    60 "BESSELI" (2, 2) [V "x", V "n"] three_d (None, Some(&[]), None); ;
    61 "BESSELJ" (2, 2) [V "x", V "n"] three_d (None, Some(&[]), None); ;
    62 "BESSELK" (2, 2) [V "x", V "n"] three_d (None, Some(&[]), None); ;
    63 "BESSELY" (2, 2) [V "x", V "n"] three_d (None, Some(&[]), None); ;
    70 "DELTA" (1, 2) [V "number1", V "number2"] three_d (None, Some(&[]), None); ;
    71 "GESTEP" (1, 2) [V "number", V "step"] three_d (None, Some(&[]), None); ;
    72 "CONVERT" (3, 3) [V "number", V "from_unit", V "to_unit"] three_d (None, Some(&[]), None); ;
    }
}

pub mod database {
    use super::*;
    pub const CODE: u16 = 205;
    data_family! {
        code: CODE;
    1 "DAVERAGE" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    2 "DCOUNT" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    3 "DCOUNTA" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    4 "DGET" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    5 "DMAX" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    6 "DMIN" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    7 "DPRODUCT" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    8 "DSTDEV" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    9 "DSTDEVP" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    10 "DSUM" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    11 "DVAR" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    12 "DVARP" (3, 3) [R "database" @Reference, V "field", R "criteria"] three_d (None, Some(&[0]), None); ;
    }
}

pub mod arrays {
    use super::*;
    pub const CODE: u16 = 310;
    data_family! {
        code: CODE;
    1 "SEQUENCE" (1, 4) [V "rows", V "columns", V "start", V "step"] three_d (None, None, Some(&[]));  ;
    2 "FILTER" (2, 3) [R "array" ~ ArrayAtRoot, R "include" ~ ArrayAtRoot, R "if_empty" ~ AggregateAtRoot] three_d (None, Some(&[0, 1]), None); ;
    3 "SORT" (1, 4) [R "array" ~ ArrayAtRoot, R "sort_index" ~ ArrayAtRoot, R "sort_order", V "by_col"] three_d (None, None, Some(&[])); ;
    4 "SORTBY" (2, 255) [R "array" ~ ArrayAtRoot, R "by_array1" ~ ArrayAtRoot, R "sort_order1" ~ AggregateAtRoot] groups (1, 2, 0); three_d (None, None, Some(&[])); ;
    5 "UNIQUE" (1, 3) [R "array" ~ ArrayAtRoot, V "by_col", V "exactly_once"] three_d (None, None, Some(&[])); ;
    6 "TAKE" (2, 3) [R "array" ~ ArrayAtRoot, V "rows", V "columns"] result MayBeReference; three_d (None, Some(&[0]), Some(&[1, 2])); ;
    7 "DROP" (2, 3) [R "array" ~ ArrayAtRoot, V "rows", V "columns"] result MayBeReference; three_d (None, Some(&[0]), Some(&[1, 2])); ;
    8 "CHOOSEROWS" (2, 255) [R "array" ~ ArrayAtRoot, R "row_num1" ~ AggregateAtRoot, R "row_num2" ~ AggregateAtRoot] three_d (None, Some(&[0]), Some(&[1, usize::MAX])); ;
    9 "CHOOSECOLS" (2, 255) [R "array" ~ ArrayAtRoot, R "col_num1" ~ AggregateAtRoot, R "col_num2" ~ AggregateAtRoot] three_d (None, Some(&[0]), Some(&[1, usize::MAX])); ;
    10 "VSTACK" (1, 254) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRootRepeated] three_d (Some(&[]), None, None); ;
    11 "HSTACK" (1, 254) [R "array1" ~ ArrayAtRoot, R "array2" ~ ArrayAtRootRepeated] three_d (Some(&[]), None, None); ;
    12 "TOCOL" (1, 3) [R "array" ~ ArrayAtRoot, V "ignore", V "scan_by_column"] three_d (Some(&[0]), None, Some(&[1])); ;
    13 "TOROW" (1, 3) [R "array" ~ ArrayAtRoot, V "ignore", V "scan_by_column"] three_d (Some(&[0]), None, Some(&[1])); ;
    14 "WRAPROWS" (2, 3) [R "vector" ~ ArrayAtRoot, V "wrap_count", V "pad_with"] three_d (None, None, Some(&[0, 1])); ;
    15 "WRAPCOLS" (2, 3) [R "vector" ~ ArrayAtRoot, V "wrap_count", V "pad_with"] three_d (None, None, Some(&[0, 1])); ;
    16 "EXPAND" (2, 4) [R "array" ~ ArrayAtRoot, V "rows", V "columns", R "pad_with"] three_d (None, None, Some(&[])); ;
    }
}

pub mod subtotal {
    use super::*;
    pub const CODE: u16 = 102;
    data_family! {
        code: CODE;
    1 "SUBTOTAL" (2, 255) [V "function_num", F "ref1" @Reference ~ ArrayAlwaysLiftReferences, F "ref2" @Reference ~ ArrayAlwaysLiftReferencesRepeated] traits skip_nested_subtotals, lift_dynamic_references; three_d (None, Some(&[1, usize::MAX]), Some(&[0])); ;
    2 "AGGREGATE" (3, 253) [V "function_num", V "options", R "ref1" ~ ArrayAtRoot, R "ref2" ~ AggregateAtRoot, R "ref3" @Reference] traits skip_nested_subtotals; three_d (None, Some(&[2, usize::MAX]), Some(&[0, 1])); ;
    }
}

pub mod text_format {
    use super::*;
    pub const CODE: u16 = 202;
    data_family! {
        code: CODE;
    1 "VALUE" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    2 "TEXT" (2, 2) [V "value", V "format_text"] three_d (None, None, Some(&[])); ;
    3 "FIXED" (1, 3) [V "number", V "decimals", V "no_commas"] three_d (None, None, Some(&[])); ;
    4 "DOLLAR" (1, 2) [V "number", V "decimals"] three_d (None, None, Some(&[])); ;
    5 "NUMBERVALUE" (1, 3) [V "text", V "decimal_separator", V "group_separator"] three_d (None, None, Some(&[])); ;
    6 "VALUETOTEXT" (1, 2) [V "value", V "format"] three_d (None, None, Some(&[])); ;
    7 "ARRAYTOTEXT" (1, 2) [R "array" ~ ArrayAtRoot, V "format"] three_d (None, None, Some(&[])); ;
    8 "BAHTTEXT" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    9 "ASC" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    10 "DBCS" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    100 "JIS" (1, 1) [V "text"] availability AbsentOnWindows; ;
    12 "PHONETIC" (1, 1) [F "reference" @Reference] three_d (None, Some(&[]), None); ;
    13 "TEXTBEFORE" (2, 6) [V "text", R "delimiter" ~ AggregateAtRoot, V "instance_num", V "match_mode", V "match_end", V "if_not_found"] three_d (None, None, Some(&[])); ;
    14 "TEXTAFTER" (2, 6) [V "text", R "delimiter" ~ AggregateAtRoot, V "instance_num", V "match_mode", V "match_end", V "if_not_found"] three_d (None, None, Some(&[])); ;
    15 "TEXTSPLIT" (2, 6) [V "text", R "col_delimiter" ~ AggregateAtRoot, R "row_delimiter" ~ AggregateAtRoot, V "ignore_empty", V "match_mode", V "pad_with"] three_d (None, None, Some(&[])); ;
    16 "REGEXTEST" (2, 3) [V "text", V "pattern", V "case_sensitivity"] three_d (None, None, Some(&[])); ;
    17 "REGEXEXTRACT" (2, 4) [V "text", V "pattern", V "return_mode", V "case_sensitivity"] three_d (None, None, Some(&[])); ;
    18 "REGEXREPLACE" (3, 5) [V "text", V "pattern", V "replacement", V "occurrence", V "case_sensitivity"] three_d (None, None, Some(&[])); ;
    19 "ENCODEURL" (1, 1) [V "text"] ;
    20 "FILTERXML" (2, 2) [V "xml", V "xpath"] traits intersect_legacy_result; ;
    21 "HYPERLINK" (1, 2) [V "link_location", V "friendly_name"] ;
    }
}

pub mod text_functions {
    use super::*;
    pub const CODE: u16 = 200;
    data_family! {
        code: CODE;
    1 "CHAR" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    2 "CODE" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    3 "UNICHAR" (1, 1) [V "number"] three_d (None, None, Some(&[])); ;
    4 "UNICODE" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    5 "LEN" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    6 "LEFT" (1, 2) [V "text", V "num_chars"] three_d (None, None, Some(&[])); ;
    7 "RIGHT" (1, 2) [V "text", V "num_chars"] three_d (None, None, Some(&[])); ;
    8 "MID" (3, 3) [V "text", V "start_num", V "num_chars"] three_d (None, None, Some(&[])); ;
    9 "LENB" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    10 "LEFTB" (1, 2) [V "text", V "num_bytes"] three_d (None, None, Some(&[])); ;
    11 "RIGHTB" (1, 2) [V "text", V "num_bytes"] three_d (None, None, Some(&[])); ;
    12 "MIDB" (3, 3) [V "text", V "start_num", V "num_bytes"] three_d (None, None, Some(&[])); ;
    13 "UPPER" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    14 "LOWER" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    15 "PROPER" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    16 "TRIM" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    17 "CLEAN" (1, 1) [V "text"] three_d (None, None, Some(&[])); ;
    18 "REPT" (2, 2) [V "text", V "number_times"] three_d (None, None, Some(&[])); ;
    19 "EXACT" (2, 2) [V "text1", V "text2"] three_d (None, None, Some(&[])); ;
    20 "FIND" (2, 3) [V "find_text", V "within_text", V "start_num"] three_d (None, None, Some(&[])); ;
    21 "FINDB" (2, 3) [V "find_text", V "within_text", V "start_num"] three_d (None, None, Some(&[])); ;
    22 "SEARCH" (2, 3) [V "find_text", V "within_text", V "start_num"] three_d (None, None, Some(&[])); ;
    23 "SEARCHB" (2, 3) [V "find_text", V "within_text", V "start_num"] three_d (None, None, Some(&[])); ;
    24 "REPLACE" (4, 4) [V "old_text", V "start_num", V "num_chars", V "new_text"] three_d (None, None, Some(&[])); ;
    25 "REPLACEB" (4, 4) [V "old_text", V "start_num", V "num_bytes", V "new_text"] three_d (None, None, Some(&[])); ;
    26 "SUBSTITUTE" (3, 4) [V "text", V "old_text", V "new_text", V "instance_num"] three_d (None, None, Some(&[])); ;
    27 "T" (1, 1) [V "value" ~ LiftReferences] traits lift_dynamic_references; three_d (None, Some(&[]), None); ;
    28 "CONCATENATE" (1, 255) [V "text1", V "text2"] three_d (None, None, Some(&[])); ;
    29 "CONCAT" (1, 254) [R "text1" ~ AggregateAtRoot, R "text2" ~ AggregateAtRootRepeated] three_d (Some(&[]), None, None); ;
    30 "TEXTJOIN" (3, 252) [R "delimiter" ~ AggregateAtRoot, V "ignore_empty" ~ AggregateAtRoot, R "text1" ~ AggregateAtRoot, R "text2" ~ AggregateAtRootRepeated] three_d (Some(&[0, 2, usize::MAX]), None, Some(&[1])); ;
    }
}

pub mod random {
    use super::*;
    pub const CODE: u16 = 106;
    data_family! {
        code: CODE;
    1 "RAND" (0, 0) [] volatile true; ;
    2 "RANDBETWEEN" (2, 2) [V "bottom", V "top"] volatile true; ;
    3 "RANDARRAY" (0, 5) [V "rows", V "columns", V "min", V "max", V "whole_number"] volatile true; ;
    }
}

pub mod cube_functions {
    use super::*;
    pub const CODE: u16 = 324;
    data_family! {
        code: CODE;
    1 "CUBEMEMBER" (2, 3) [R "connection", R "member_expression", R "caption"]
    ;
    2 "CUBEVALUE" (1, 127) [R "connection", R "member_expression"]
    ;
    3 "CUBESET" (2, 5) [R "connection", R "set_expression", R "caption", R "sort_order", R "sort_by"]
    ;
    4 "CUBESETCOUNT" (1, 1) [R "set"] ;
    5 "CUBERANKEDMEMBER" (3, 4) [R "connection", R "set_expression", R "rank", R "caption"]
    ;
    6 "CUBEMEMBERPROPERTY" (3, 3) [R "connection", R "member_expression", R "property"]
    ;
    7 "CUBEKPIMEMBER" (3, 4) [R "connection", R "kpi_name", R "kpi_property", R "caption"]
    ;
    }
}

pub mod fetching {
    use super::*;
    pub const CODE: u16 = 323;
    data_family! {
        code: CODE;
    1 "WEBSERVICE" (1, 1) [V "url"] volatile true; host_route (crate::FETCH_PROVIDER, crate::FetchKind::Text); ;
    2 "IMAGE" (1, 5) [V "source", V "alt_text", V "sizing", V "height", V "width"] host_route (crate::FETCH_PROVIDER, crate::FetchKind::Picture);
    ;
    }
}

pub mod importing {
    use super::*;
    pub const CODE: u16 = 326;
    data_family! {
        code: CODE;
    1 "IMPORTCSV" (1, 5) [V "path", V "skip_rows", V "take_rows", V "text_only", V "locale"] import_route (crate::IMPORT_PROVIDER, crate::ImportKind::Csv);
    ;
    2 "IMPORTTEXT" (1, 7) [V "path", V "delimiter", V "skip_rows", V "take_rows", V "encoding", V "text_only", V "locale"] import_route (crate::IMPORT_PROVIDER, crate::ImportKind::Text);
    ;
    }
}
