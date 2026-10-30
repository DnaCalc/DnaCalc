//! Function help: a one-line description of each named function, as signature help and a
//! function-help panel show it beside [`crate::FunctionCallTarget::signature_help`].
//!
//! **Microsoft's text.** Each function's `microsoft` description is Microsoft's official one-line
//! description of the function, copyright Microsoft Corporation, as published on Microsoft's
//! support site in the lists "Excel functions (by category)"
//! (<https://support.microsoft.com/en-us/office/excel-functions-by-category-5f91f4e9-7b42-46d2-9bd1-63f26a86c0eb>)
//! and "Excel functions (alphabetical)"
//! (<https://support.microsoft.com/en-us/office/excel-functions-alphabetical-b3944572-255d-4efb-bb96-c6d90033e188>),
//! kept as scraped (two lost a non-ASCII character in the scrape: BAHTTEXT's baht sign and the
//! gamma of GAMMALN and GAMMALN.PRECISE) and attributed: `source` is the function's own Microsoft
//! support article, from `migration/help-text-sources.tsv`. It is the function metadata
//! [`description`] gives. The `own` line is DNA Calc's own description of what the function
//! computes, written for this table (dc-96l.5). `OP_IMPLICIT_INTERSECTION` has no Microsoft text;
//! its description is our own.
//!
//! The table is keyed by the function's spelling and held in byte order, so a lookup is a binary
//! search. A function the table does not describe has no description; the table is help text only
//! and says nothing about whether calc-func implements a function, which is the catalogue's to say.

use crate::{FunctionCallTarget, Notation};

/// The help of one named function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionHelp {
    /// The function's spelling, upper case, as the catalogue spells it.
    pub spelling: &'static str,
    /// Microsoft's official one-line description (Microsoft's text, copyright Microsoft
    /// Corporation), or `None` when Microsoft has none.
    pub microsoft: Option<&'static str>,
    /// The Microsoft support article the description is published with.
    pub source: Option<&'static str>,
    /// DNA Calc's own one-line description.
    pub own: &'static str,
}

impl FunctionHelp {
    /// The description shown as the function's metadata: Microsoft's official text where there is
    /// one, otherwise our own.
    pub fn description(&self) -> &'static str {
        self.microsoft.unwrap_or(self.own)
    }
}

/// The description of a named function (see [`FunctionHelp::description`]), or `None` for an
/// operator and for a function the table does not describe.
pub fn description(target: FunctionCallTarget<'_>) -> Option<&'static str> {
    help(target).map(|h| h.description())
}

/// The help of a named function, or `None` for an operator and for a function the table does not
/// describe.
pub fn help(target: FunctionCallTarget<'_>) -> Option<&'static FunctionHelp> {
    if target.notation() != Notation::Call {
        return None;
    }
    help_of(target.spelling())
}

/// The description of the function spelled `spelling` (upper case, as the catalogue spells it).
pub fn description_of(spelling: &str) -> Option<&'static str> {
    help_of(spelling).map(|h| h.description())
}

/// The help of the function spelled `spelling` (upper case, as the catalogue spells it).
pub fn help_of(spelling: &str) -> Option<&'static FunctionHelp> {
    HELP.binary_search_by(|h| h.spelling.as_bytes().cmp(spelling.as_bytes()))
        .ok()
        .map(|at| &HELP[at])
}

/// Each function's help, in byte order of the spelling.
const HELP: &[FunctionHelp] = &[
    FunctionHelp {
        spelling: "ABS",
        microsoft: Some("Returns the absolute value of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/abs-function-3420200f-5628-4e8c-99da-c99d7c87713c",
        ),
        own: "Absolute value: the number without its sign, its distance from zero",
    },
    FunctionHelp {
        spelling: "ACCRINT",
        microsoft: Some("Returns the accrued interest for a security that pays periodic interest"),
        source: Some(
            "https://support.microsoft.com/en-us/office/accrint-function-fe45d089-6722-4fb3-9379-e1f911d8dc74",
        ),
        own: "Interest accrued on a security with periodic coupons, from issue to settlement",
    },
    FunctionHelp {
        spelling: "ACCRINTM",
        microsoft: Some(
            "Returns the accrued interest for a security that pays interest at maturity",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/accrintm-function-f62f01f9-5754-4cc4-805b-0e70199328a7",
        ),
        own: "Interest accrued on a security that pays all its interest when it matures",
    },
    FunctionHelp {
        spelling: "ACOS",
        microsoft: Some("Returns the arccosine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/acos-function-cb73173f-d089-4582-afa1-76e5524b5d5b",
        ),
        own: "Angle in radians, from 0 to pi, whose cosine is the given number",
    },
    FunctionHelp {
        spelling: "ACOSH",
        microsoft: Some("Returns the inverse hyperbolic cosine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/acosh-function-e3992cc1-103f-4e72-9f04-624b9ef5ebfe",
        ),
        own: "Inverse of the hyperbolic cosine",
    },
    FunctionHelp {
        spelling: "ACOT",
        microsoft: Some("Returns the arccotangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/acot-function-dc7e5008-fe6b-402e-bdd6-2eea8383d905",
        ),
        own: "Angle in radians, from 0 to pi, whose cotangent is the given number",
    },
    FunctionHelp {
        spelling: "ACOTH",
        microsoft: Some("Returns the hyperbolic arccotangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/acoth-function-cc49480f-f684-4171-9fc5-73e4e852300f",
        ),
        own: "Inverse of the hyperbolic cotangent",
    },
    FunctionHelp {
        spelling: "ADDRESS",
        microsoft: Some("Returns a reference as text to a single cell in a worksheet"),
        source: Some(
            "https://support.microsoft.com/en-us/office/address-function-d0c26c0d-3991-446b-8de4-ab46431d4f89",
        ),
        own: "Text of a cell reference built from a row and a column number",
    },
    FunctionHelp {
        spelling: "AGGREGATE",
        microsoft: Some("Returns an aggregate in a list or database"),
        source: Some(
            "https://support.microsoft.com/en-us/office/aggregate-function-43b9278e-6aa7-4f17-92b6-e19993fa26df",
        ),
        own: "One of nineteen summaries (sum, average, percentile and more) of a range, optionally skipping errors, hidden rows and nested subtotals",
    },
    FunctionHelp {
        spelling: "AMORDEGRC",
        microsoft: Some(
            "Returns the depreciation for each accounting period by using a depreciation coefficient",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/amordegrc-function-a14d0ca1-64a4-42eb-9b3d-b0dededf9e51",
        ),
        own: "French accounting depreciation for one period, with a coefficient that depends on the asset's life",
    },
    FunctionHelp {
        spelling: "AMORLINC",
        microsoft: Some("Returns the depreciation for each accounting period"),
        source: Some(
            "https://support.microsoft.com/en-us/office/amorlinc-function-7d417b45-f7f5-4dba-a0a5-3451a81079a8",
        ),
        own: "French accounting straight-line depreciation for one period, prorated in the first period",
    },
    FunctionHelp {
        spelling: "AND",
        microsoft: Some("Returns TRUE if all of its arguments are TRUE"),
        source: Some(
            "https://support.microsoft.com/en-us/office/and-function-5f19b2e8-e1df-4408-897a-ce285a19e9d9",
        ),
        own: "TRUE only when every argument is TRUE",
    },
    FunctionHelp {
        spelling: "ARABIC",
        microsoft: Some("Converts a Roman number to Arabic, as a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/arabic-function-9a8da418-c17b-4ef9-a657-9370a30a674f",
        ),
        own: "Number written by a Roman numeral",
    },
    FunctionHelp {
        spelling: "AREAS",
        microsoft: Some("Returns the number of areas in a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/areas-function-8392ba32-7a41-43b3-96b0-3695d2ec6152",
        ),
        own: "How many separate areas a reference covers",
    },
    FunctionHelp {
        spelling: "ARRAYTOTEXT",
        microsoft: Some("Returns an array of text values from any specified range"),
        source: Some(
            "https://support.microsoft.com/en-us/office/arraytotext-function-9cdcad46-2fa5-4c6b-ac92-14e7bc862b8b",
        ),
        own: "One text holding all the values of an array or range",
    },
    FunctionHelp {
        spelling: "ASC",
        microsoft: Some(
            "Changes full-width (double-byte) English letters or katakana within a character string to half-width (single-byte) characters",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/asc-function-0b6abf1c-c663-4004-a964-ebc00b723266",
        ),
        own: "Text with full-width (double-byte) characters turned into their half-width forms",
    },
    FunctionHelp {
        spelling: "ASIN",
        microsoft: Some("Returns the arcsine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/asin-function-81fb95e5-6d6f-48c4-bc45-58f955c6d347",
        ),
        own: "Angle in radians, from -pi/2 to pi/2, whose sine is the given number",
    },
    FunctionHelp {
        spelling: "ASINH",
        microsoft: Some("Returns the inverse hyperbolic sine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/asinh-function-4e00475a-067a-43cf-926a-765b0249717c",
        ),
        own: "Inverse of the hyperbolic sine",
    },
    FunctionHelp {
        spelling: "ATAN",
        microsoft: Some("Returns the arctangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/atan-function-50746fa8-630a-406b-81d0-4a2aed395543",
        ),
        own: "Angle in radians, between -pi/2 and pi/2, whose tangent is the given number",
    },
    FunctionHelp {
        spelling: "ATAN2",
        microsoft: Some("Returns the arctangent from x- and y-coordinates"),
        source: Some(
            "https://support.microsoft.com/en-us/office/atan2-function-c04592ab-b9e3-4908-b428-c96b3a565033",
        ),
        own: "Angle in radians from the positive x-axis to the point (x, y), between -pi and pi",
    },
    FunctionHelp {
        spelling: "ATANH",
        microsoft: Some("Returns the inverse hyperbolic tangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/atanh-function-3cd65768-0de7-4f1d-b312-d01c8c930d90",
        ),
        own: "Inverse of the hyperbolic tangent",
    },
    FunctionHelp {
        spelling: "AVEDEV",
        microsoft: Some(
            "Returns the average of the absolute deviations of data points from their mean",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/avedev-function-58fe8d65-2a84-4dc7-8052-f3f87b5c6639",
        ),
        own: "Mean of the absolute distances of the values from their mean",
    },
    FunctionHelp {
        spelling: "AVERAGE",
        microsoft: Some("Returns the average of its arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/average-function-047bac88-d466-426c-a32b-8f33eb960cf6",
        ),
        own: "Arithmetic mean of the numbers given",
    },
    FunctionHelp {
        spelling: "AVERAGEA",
        microsoft: Some(
            "Returns the average of its arguments, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/averagea-function-f5f84098-d453-4f4c-bbba-3d2c66356091",
        ),
        own: "Arithmetic mean that also counts text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "AVERAGEIF",
        microsoft: Some(
            "Returns the average (arithmetic mean) of all the cells in a range that meet a given criteria",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/averageif-function-faec8e2e-0dec-4308-af69-f5576d8ac642",
        ),
        own: "Mean of the cells that meet one condition",
    },
    FunctionHelp {
        spelling: "AVERAGEIFS",
        microsoft: Some(
            "Returns the average (arithmetic mean) of all cells that meet multiple criteria",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/averageifs-function-48910c45-1fc0-4389-a028-f7c5c3001690",
        ),
        own: "Mean of the cells that meet every one of several conditions",
    },
    FunctionHelp {
        spelling: "BAHTTEXT",
        microsoft: Some("Converts a number to text, using the ? (baht) currency format"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bahttext-function-5ba4d0b4-abd3-4325-8d22-7a92d59aab9c",
        ),
        own: "A number spelled out in Thai words as an amount in baht",
    },
    FunctionHelp {
        spelling: "BASE",
        microsoft: Some("Converts a number into a text representation with the given radix (base)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/base-function-2ef61411-aee9-4f29-a811-1c42456c6342",
        ),
        own: "A number written as text in another base (radix 2 to 36)",
    },
    FunctionHelp {
        spelling: "BESSELI",
        microsoft: Some("Returns the modified Bessel function In(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/besseli-function-8d33855c-9a8d-444b-98e0-852267b1c0df",
        ),
        own: "Modified Bessel function of the first kind, I_n(x)",
    },
    FunctionHelp {
        spelling: "BESSELJ",
        microsoft: Some("Returns the Bessel function Jn(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/besselj-function-839cb181-48de-408b-9d80-bd02982d94f7",
        ),
        own: "Bessel function of the first kind, J_n(x)",
    },
    FunctionHelp {
        spelling: "BESSELK",
        microsoft: Some("Returns the modified Bessel function Kn(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/besselk-function-606d11bc-06d3-4d53-9ecb-2803e2b90b70",
        ),
        own: "Modified Bessel function of the second kind, K_n(x)",
    },
    FunctionHelp {
        spelling: "BESSELY",
        microsoft: Some("Returns the Bessel function Yn(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bessely-function-f3a356b3-da89-42c3-8974-2da54d6353a2",
        ),
        own: "Bessel function of the second kind, Y_n(x)",
    },
    FunctionHelp {
        spelling: "BETA.DIST",
        microsoft: Some("Returns the beta cumulative distribution function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/beta-dist-function-11188c9c-780a-42c7-ba43-9ecb5a878d31",
        ),
        own: "Beta distribution: the density or the cumulative probability, on an optional interval",
    },
    FunctionHelp {
        spelling: "BETA.INV",
        microsoft: Some(
            "Returns the inverse of the cumulative distribution function for a specified beta distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/beta-inv-function-e84cb8aa-8df0-4cf6-9892-83a341d252eb",
        ),
        own: "The value at which the cumulative beta distribution reaches a given probability",
    },
    FunctionHelp {
        spelling: "BETADIST",
        microsoft: Some("Returns the beta cumulative distribution function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/betadist-function-49f1b9a9-a5da-470f-8077-5f1730b5fd47",
        ),
        own: "Cumulative beta probability (the older form of BETA.DIST)",
    },
    FunctionHelp {
        spelling: "BETAINV",
        microsoft: Some(
            "Returns the inverse of the cumulative distribution function for a specified beta distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/betainv-function-8b914ade-b902-43c1-ac9c-c05c54f10d6c",
        ),
        own: "Inverse of the cumulative beta distribution (the older form of BETA.INV)",
    },
    FunctionHelp {
        spelling: "BIN2DEC",
        microsoft: Some("Converts a binary number to decimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bin2dec-function-63905b57-b3a0-453d-99f4-647bb519cd6c",
        ),
        own: "Decimal value of a binary number given as text",
    },
    FunctionHelp {
        spelling: "BIN2HEX",
        microsoft: Some("Converts a binary number to hexadecimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bin2hex-function-0375e507-f5e5-4077-9af8-28d84f9f41cc",
        ),
        own: "Binary number rewritten in hexadecimal",
    },
    FunctionHelp {
        spelling: "BIN2OCT",
        microsoft: Some("Converts a binary number to octal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bin2oct-function-0a4e01ba-ac8d-4158-9b29-16c25c4c23fd",
        ),
        own: "Binary number rewritten in octal",
    },
    FunctionHelp {
        spelling: "BINOM.DIST",
        microsoft: Some("Returns the individual term binomial distribution probability"),
        source: Some(
            "https://support.microsoft.com/en-us/office/binom-dist-function-c5ae37b6-f39c-4be2-94c2-509a1480770c",
        ),
        own: "Binomial distribution: the chance of a number of successes in a number of trials, as a single value or cumulative",
    },
    FunctionHelp {
        spelling: "BINOM.DIST.RANGE",
        microsoft: Some("Returns the probability of a trial result using a binomial distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/binom-dist-range-function-17331329-74c7-4053-bb4c-6653a7421595",
        ),
        own: "Chance that the number of binomial successes falls between two bounds",
    },
    FunctionHelp {
        spelling: "BINOM.INV",
        microsoft: Some(
            "Returns the smallest value for which the cumulative binomial distribution is less than or equal to a criterion value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/binom-inv-function-80a0370c-ada6-49b4-83e7-05a91ba77ac9",
        ),
        own: "Fewest successes whose cumulative binomial probability reaches a target",
    },
    FunctionHelp {
        spelling: "BINOMDIST",
        microsoft: Some("Returns the individual term binomial distribution probability"),
        source: Some(
            "https://support.microsoft.com/en-us/office/binomdist-function-506a663e-c4ca-428d-b9a8-05583d68789c",
        ),
        own: "Binomial probability (the older form of BINOM.DIST)",
    },
    FunctionHelp {
        spelling: "BITAND",
        microsoft: Some("Returns a 'Bitwise And' of two numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bitand-function-8a2be3d7-91c3-4b48-9517-64548008563a",
        ),
        own: "Bits set in both of two whole numbers",
    },
    FunctionHelp {
        spelling: "BITLSHIFT",
        microsoft: Some("Returns a value number shifted left by shift_amount bits"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bitlshift-function-c55bb27e-cacd-4c7c-b258-d80861a03c9c",
        ),
        own: "A whole number with its bits moved left by a given count",
    },
    FunctionHelp {
        spelling: "BITOR",
        microsoft: Some("Returns a bitwise OR of 2 numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bitor-function-f6ead5c8-5b98-4c9e-9053-8ad5234919b2",
        ),
        own: "Bits set in either of two whole numbers",
    },
    FunctionHelp {
        spelling: "BITRSHIFT",
        microsoft: Some("Returns a value number shifted right by shift_amount bits"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bitrshift-function-274d6996-f42c-4743-abdb-4ff95351222c",
        ),
        own: "A whole number with its bits moved right by a given count",
    },
    FunctionHelp {
        spelling: "BITXOR",
        microsoft: Some("Returns a bitwise 'Exclusive Or' of two numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bitxor-function-c81306a1-03f9-4e89-85ac-b86c3cba10e4",
        ),
        own: "Bits set in exactly one of two whole numbers",
    },
    FunctionHelp {
        spelling: "BYCOL",
        microsoft: Some("Applies a LAMBDA to each column and returns an array of the results"),
        source: Some(
            "https://support.microsoft.com/en-us/office/bycol-function-58463999-7de5-49ce-8f38-b7f7a2192bfb",
        ),
        own: "One result per column, from a LAMBDA called on each column",
    },
    FunctionHelp {
        spelling: "BYROW",
        microsoft: Some("Applies a LAMBDA to each row and returns an array of the results"),
        source: Some(
            "https://support.microsoft.com/en-us/office/byrow-function-2e04c677-78c8-4e6b-8c10-a4602f2602bb",
        ),
        own: "One result per row, from a LAMBDA called on each row",
    },
    FunctionHelp {
        spelling: "CALL",
        microsoft: Some("Calls a procedure in a dynamic link library or code resource"),
        source: Some(
            "https://support.microsoft.com/en-us/office/call-function-32d58445-e646-4ffd-8d5e-b45077a5e995",
        ),
        own: "Runs a procedure from a code library (DLL) and gives back its result",
    },
    FunctionHelp {
        spelling: "CEILING",
        microsoft: Some(
            "Rounds a number to the nearest integer or to the nearest multiple of significance",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ceiling-function-0a5cd7c8-0720-4f0a-bd2c-c943e510899f",
        ),
        own: "Number rounded up, away from zero, to a multiple of the significance",
    },
    FunctionHelp {
        spelling: "CEILING.MATH",
        microsoft: Some(
            "Rounds a number up, to the nearest integer or to the nearest multiple of significance",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ceiling-math-function-80f95d2f-b499-4eee-9f16-f795a8e306c8",
        ),
        own: "Number rounded up to a multiple of the significance, with a choice of direction for negative numbers",
    },
    FunctionHelp {
        spelling: "CEILING.PRECISE",
        microsoft: Some(
            "Rounds a number the nearest integer or to the nearest multiple of significance. Regardless of the sign of the number, the number is rounded up.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ceiling-precise-function-f366a774-527a-4c92-ba49-af0a196e66cb",
        ),
        own: "Number rounded up toward plus infinity to a multiple of the significance, whatever its sign",
    },
    FunctionHelp {
        spelling: "CELL",
        microsoft: Some(
            "Returns information about the formatting, location, or contents of a cell",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cell-function-51bd39a5-f338-4dbe-a33f-955d67c2b2cf",
        ),
        own: "A fact about a cell, such as its address, format, contents or width",
    },
    FunctionHelp {
        spelling: "CHAR",
        microsoft: Some("Returns the character specified by the code number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/char-function-bbd249c8-b36e-4a91-8017-1c133f9b837a",
        ),
        own: "Character with a given code in the system character set",
    },
    FunctionHelp {
        spelling: "CHIDIST",
        microsoft: Some("Returns the one-tailed probability of the chi-squared distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chidist-function-c90d0fbc-5b56-4f5f-ab57-34af1bf6897e",
        ),
        own: "Right-tail probability of the chi-squared distribution (older form)",
    },
    FunctionHelp {
        spelling: "CHIINV",
        microsoft: Some(
            "Returns the inverse of the one-tailed probability of the chi-squared distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/chiinv-function-cfbea3f6-6e4f-40c9-a87f-20472e0512af",
        ),
        own: "Inverse of the right-tail chi-squared probability (older form)",
    },
    FunctionHelp {
        spelling: "CHISQ.DIST",
        microsoft: Some("Returns the cumulative beta probability density function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chisq-dist-function-8486b05e-5c05-4942-a9ea-f6b341518732",
        ),
        own: "Chi-squared distribution: the density or the left-tail probability",
    },
    FunctionHelp {
        spelling: "CHISQ.DIST.RT",
        microsoft: Some("Returns the one-tailed probability of the chi-squared distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chisq-dist-rt-function-dc4832e8-ed2b-49ae-8d7c-b28d5804c0f2",
        ),
        own: "Chi-squared upper-tail probability, P(X > x)",
    },
    FunctionHelp {
        spelling: "CHISQ.INV",
        microsoft: Some("Returns the cumulative beta probability density function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chisq-inv-function-400db556-62b3-472d-80b3-254723e7092f",
        ),
        own: "The value at which the left-tail chi-squared probability reaches a given level",
    },
    FunctionHelp {
        spelling: "CHISQ.INV.RT",
        microsoft: Some(
            "Returns the inverse of the one-tailed probability of the chi-squared distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/chisq-inv-rt-function-435b5ed8-98d5-4da6-823f-293e2cbc94fe",
        ),
        own: "The value at which the right-tail chi-squared probability falls to a given level",
    },
    FunctionHelp {
        spelling: "CHISQ.TEST",
        microsoft: Some("Returns the test for independence"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chisq-test-function-2e8a7861-b14a-4985-aa93-fb88de3f260f",
        ),
        own: "Chi-squared test of independence between observed and expected counts, as a probability",
    },
    FunctionHelp {
        spelling: "CHITEST",
        microsoft: Some("Returns the test for independence"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chitest-function-981ff871-b694-4134-848e-38ec704577ac",
        ),
        own: "Chi-squared test of independence (older form of CHISQ.TEST)",
    },
    FunctionHelp {
        spelling: "CHOOSE",
        microsoft: Some("Chooses a value from a list of values"),
        source: Some(
            "https://support.microsoft.com/en-us/office/choose-function-fc5c184f-cb62-4ec7-a46e-38653b98f5bc",
        ),
        own: "The value at a given position in a list of arguments",
    },
    FunctionHelp {
        spelling: "CHOOSECOLS",
        microsoft: Some("Returns the specified columns from an array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/choosecols-function-bf117976-2722-4466-9b9a-1c01ed9aebff",
        ),
        own: "Selected columns of an array, in the order asked for",
    },
    FunctionHelp {
        spelling: "CHOOSEROWS",
        microsoft: Some("Returns the specified rows from an array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/chooserows-function-51ace882-9bab-4a44-9625-7274ef7507a3",
        ),
        own: "Selected rows of an array, in the order asked for",
    },
    FunctionHelp {
        spelling: "CLEAN",
        microsoft: Some("Removes all nonprintable characters from text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/clean-function-26f3d7c5-475f-4a9c-90e5-4b8ba987ba41",
        ),
        own: "Text with the non-printing control characters taken out",
    },
    FunctionHelp {
        spelling: "CODE",
        microsoft: Some("Returns a numeric code for the first character in a text string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/code-function-c32b692b-2ed0-4a04-bdd9-75640144b928",
        ),
        own: "Code of the first character of a text in the system character set",
    },
    FunctionHelp {
        spelling: "COLUMN",
        microsoft: Some("Returns the column number of a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/column-function-44e8c754-711c-4df3-9da4-47a55042554b",
        ),
        own: "Column number of the first cell of a reference (of the formula's cell when left out)",
    },
    FunctionHelp {
        spelling: "COLUMNS",
        microsoft: Some("Returns the number of columns in a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/columns-function-4e8e7b4e-e603-43e8-b177-956088fa48ca",
        ),
        own: "How many columns an array or reference has",
    },
    FunctionHelp {
        spelling: "COMBIN",
        microsoft: Some("Returns the number of combinations for a given number of objects"),
        source: Some(
            "https://support.microsoft.com/en-us/office/combin-function-12a3f276-0a21-423a-8de6-06990aaf638a",
        ),
        own: "Number of ways to pick k items from n when order does not matter",
    },
    FunctionHelp {
        spelling: "COMBINA",
        microsoft: Some(
            "Returns the number of combinations with repetitions for a given number of items",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/combina-function-efb49eaa-4f4c-4cd2-8179-0ddfcf9d035d",
        ),
        own: "Number of ways to pick k items from n when items may repeat and order does not matter",
    },
    FunctionHelp {
        spelling: "COMPLEX",
        microsoft: Some("Converts real and imaginary coefficients into a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/complex-function-f0b8f3a9-51cc-4d6d-86fb-3a9362fa4128",
        ),
        own: "A complex number written as text from its real and imaginary parts",
    },
    FunctionHelp {
        spelling: "CONCAT",
        microsoft: Some(
            "Combines the text from multiple ranges and/or strings, but it doesn't provide the delimiter or IgnoreEmpty arguments.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/concat-function-9b1a9a3f-94ff-41af-9736-694cbd6b4ca2",
        ),
        own: "All the texts of the arguments and ranges joined end to end",
    },
    FunctionHelp {
        spelling: "CONCATENATE",
        microsoft: Some("Joins two or more text strings into one string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/concatenate-function-8f8ae884-2ca8-4f7a-b093-75d702bea31d",
        ),
        own: "Two or more texts joined end to end",
    },
    FunctionHelp {
        spelling: "CONFIDENCE",
        microsoft: Some("Returns the confidence interval for a population mean"),
        source: Some(
            "https://support.microsoft.com/en-us/office/confidence-function-75ccc007-f77c-4343-bc14-673642091ad6",
        ),
        own: "Half-width of a normal confidence interval for a mean (older form)",
    },
    FunctionHelp {
        spelling: "CONFIDENCE.NORM",
        microsoft: Some("Returns the confidence interval for a population mean"),
        source: Some(
            "https://support.microsoft.com/en-us/office/confidence-norm-function-7cec58a6-85bb-488d-91c3-63828d4fbfd4",
        ),
        own: "Half-width of a confidence interval for a mean, from the normal distribution",
    },
    FunctionHelp {
        spelling: "CONFIDENCE.T",
        microsoft: Some(
            "Returns the confidence interval for a population mean, using a Student's t distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/confidence-t-function-e8eca395-6c3a-4ba9-9003-79ccc61d3c53",
        ),
        own: "Half-width of a confidence interval for a mean, from Student's t distribution",
    },
    FunctionHelp {
        spelling: "CONVERT",
        microsoft: Some("Converts a number from one measurement system to another"),
        source: Some(
            "https://support.microsoft.com/en-us/office/convert-function-d785bef1-808e-4aac-bdcd-666c810f9af2",
        ),
        own: "A measurement converted from one unit to another",
    },
    FunctionHelp {
        spelling: "CORREL",
        microsoft: Some("Returns the correlation coefficient between two data sets"),
        source: Some(
            "https://support.microsoft.com/en-us/office/correl-function-995dcef7-0c0a-4bed-a3fb-239d7b68ca92",
        ),
        own: "Pearson correlation coefficient of two data sets",
    },
    FunctionHelp {
        spelling: "COS",
        microsoft: Some("Returns the cosine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/cos-function-0fb808a5-95d6-4553-8148-22aebdce5f05",
        ),
        own: "Cosine of an angle in radians",
    },
    FunctionHelp {
        spelling: "COSH",
        microsoft: Some("Returns the hyperbolic cosine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/cosh-function-e460d426-c471-43e8-9540-a57ff3b70555",
        ),
        own: "Hyperbolic cosine",
    },
    FunctionHelp {
        spelling: "COT",
        microsoft: Some("Returns the cotangent of an angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/cot-function-c446f34d-6fe4-40dc-84f8-cf59e5f5e31a",
        ),
        own: "Cotangent of an angle in radians",
    },
    FunctionHelp {
        spelling: "COTH",
        microsoft: Some("Returns the hyperbolic cotangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/coth-function-2e0b4cb6-0ba0-403e-aed4-deaa71b49df5",
        ),
        own: "Hyperbolic cotangent",
    },
    FunctionHelp {
        spelling: "COUNT",
        microsoft: Some(
            "Use this function to count how many numbers are in the list of arguments. You can use COUNTA to count how many values are in the list of arguments.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/count-function-a59cd7fc-b623-4d93-87a4-d23bf411294c",
        ),
        own: "How many of the values are numbers",
    },
    FunctionHelp {
        spelling: "COUNTA",
        microsoft: Some("Counts how many values are in the list of arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/counta-function-7dc98875-d5c1-46f1-9a82-53f3219e2509",
        ),
        own: "How many of the values are not empty",
    },
    FunctionHelp {
        spelling: "COUNTBLANK",
        microsoft: Some("Counts the number of blank cells within a range"),
        source: Some(
            "https://support.microsoft.com/en-us/office/countblank-function-6a92d772-675c-4bee-b346-24af6bd3ac22",
        ),
        own: "How many cells of a range are empty",
    },
    FunctionHelp {
        spelling: "COUNTIF",
        microsoft: Some("Counts the number of cells within a range that meet the given criteria"),
        source: Some(
            "https://support.microsoft.com/en-us/office/use-the-countif-function-in-microsoft-excel-e0de10c6-f885-4e71-abb4-1f464816df34",
        ),
        own: "How many cells meet one condition",
    },
    FunctionHelp {
        spelling: "COUNTIFS",
        microsoft: Some(
            "Use this function to count the number of cells within a range that meet multiple criteria.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/countifs-function-dda3dc6e-f74e-4aee-88bc-aa8c2a866842",
        ),
        own: "How many cells meet every one of several conditions",
    },
    FunctionHelp {
        spelling: "COUPDAYBS",
        microsoft: Some(
            "Returns the number of days from the beginning of the coupon period to the settlement date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/coupdaybs-function-eb9a8dfb-2fb2-4c61-8e5d-690b320cf872",
        ),
        own: "Days from the start of the coupon period to the settlement date",
    },
    FunctionHelp {
        spelling: "COUPDAYS",
        microsoft: Some(
            "Returns the number of days in the coupon period that contains the settlement date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/coupdays-function-cc64380b-315b-4e7b-950c-b30b0a76f671",
        ),
        own: "Days in the coupon period that holds the settlement date",
    },
    FunctionHelp {
        spelling: "COUPDAYSNC",
        microsoft: Some(
            "Returns the number of days from the settlement date to the next coupon date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/coupdaysnc-function-5ab3f0b2-029f-4a8b-bb65-47d525eea547",
        ),
        own: "Days left from settlement until the coupon date that follows it",
    },
    FunctionHelp {
        spelling: "COUPNCD",
        microsoft: Some("Returns the next coupon date after the settlement date"),
        source: Some(
            "https://support.microsoft.com/en-us/office/coupncd-function-fd962fef-506b-4d9d-8590-16df5393691f",
        ),
        own: "Coupon date that follows the settlement date",
    },
    FunctionHelp {
        spelling: "COUPNUM",
        microsoft: Some(
            "Returns the number of coupons payable between the settlement date and maturity date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/coupnum-function-a90af57b-de53-4969-9c99-dd6139db2522",
        ),
        own: "Number of coupons still to be paid between settlement and maturity",
    },
    FunctionHelp {
        spelling: "COUPPCD",
        microsoft: Some("Returns the previous coupon date before the settlement date"),
        source: Some(
            "https://support.microsoft.com/en-us/office/couppcd-function-2eb50473-6ee9-4052-a206-77a9a385d5b3",
        ),
        own: "Last coupon date on or before the settlement date",
    },
    FunctionHelp {
        spelling: "COVAR",
        microsoft: Some("Returns covariance, the average of the products of paired deviations"),
        source: Some(
            "https://support.microsoft.com/en-us/office/covar-function-50479552-2c03-4daf-bd71-a5ab88b2db03",
        ),
        own: "Population covariance of two data sets (older form)",
    },
    FunctionHelp {
        spelling: "COVARIANCE.P",
        microsoft: Some("Returns covariance, the average of the products of paired deviations"),
        source: Some(
            "https://support.microsoft.com/en-us/office/covariance-p-function-6f0e1e6d-956d-4e4b-9943-cfef0bf9edfc",
        ),
        own: "Covariance of two data sets taken as a whole population",
    },
    FunctionHelp {
        spelling: "COVARIANCE.S",
        microsoft: Some(
            "Returns the sample covariance, the average of the products deviations for each data point pair in two data sets",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/covariance-s-function-0a539b74-7371-42aa-a18f-1f5320314977",
        ),
        own: "Covariance of two data sets taken as a sample",
    },
    FunctionHelp {
        spelling: "CRITBINOM",
        microsoft: Some(
            "Returns the smallest value for which the cumulative binomial distribution is less than or equal to a criterion value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/critbinom-function-eb6b871d-796b-4d21-b69b-e4350d5f407b",
        ),
        own: "Fewest successes whose cumulative binomial probability reaches a target (older form)",
    },
    FunctionHelp {
        spelling: "CSC",
        microsoft: Some("Returns the cosecant of an angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/csc-function-07379361-219a-4398-8675-07ddc4f135c1",
        ),
        own: "Cosecant of an angle in radians",
    },
    FunctionHelp {
        spelling: "CSCH",
        microsoft: Some("Returns the hyperbolic cosecant of an angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/csch-function-f58f2c22-eb75-4dd6-84f4-a503527f8eeb",
        ),
        own: "Hyperbolic cosecant",
    },
    FunctionHelp {
        spelling: "CUBEKPIMEMBER",
        microsoft: Some(
            "Returns a key performance indicator (KPI) property and displays the KPI name in the cell. A KPI is a quantifiable measurement, such as monthly gross profit or quarterly employee turnover, that is used to monitor an organization's performance.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubekpimember-function-744608bf-2c62-42cd-b67a-a56109f4b03b",
        ),
        own: "A key performance indicator property of an OLAP cube",
    },
    FunctionHelp {
        spelling: "CUBEMEMBER",
        microsoft: Some(
            "Returns a member or tuple from the cube. Use to validate that the member or tuple exists in the cube.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubemember-function-0f6a15b9-2c18-4819-ae89-e1b5c8b398ad",
        ),
        own: "A member or tuple of an OLAP cube, if it exists",
    },
    FunctionHelp {
        spelling: "CUBEMEMBERPROPERTY",
        microsoft: Some(
            "Returns the value of a member property from the cube. Use to validate that a member name exists within the cube and to return the specified property for this member.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubememberproperty-function-001e57d6-b35a-49e5-abcd-05ff599e8951",
        ),
        own: "A property of a member of an OLAP cube",
    },
    FunctionHelp {
        spelling: "CUBERANKEDMEMBER",
        microsoft: Some(
            "Returns the nth, or ranked, member in a set. Use to return one or more elements in a set, such as the top sales performer or the top 10 students.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cuberankedmember-function-07efecde-e669-4075-b4bf-6b40df2dc4b3",
        ),
        own: "The member at a given rank in a set from an OLAP cube",
    },
    FunctionHelp {
        spelling: "CUBESET",
        microsoft: Some(
            "Defines a calculated set of members or tuples by sending a set expression to the cube on the server, which creates the set, and then returns that set to Microsoft Excel.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubeset-function-5b2146bd-62d6-4d04-9d8f-670e993ee1d9",
        ),
        own: "A set of members or tuples computed by an OLAP cube server",
    },
    FunctionHelp {
        spelling: "CUBESETCOUNT",
        microsoft: Some("Returns the number of items in a set."),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubesetcount-function-c4c2a438-c1ff-4061-80fe-982f2d705286",
        ),
        own: "How many items a cube set holds",
    },
    FunctionHelp {
        spelling: "CUBEVALUE",
        microsoft: Some("Returns an aggregated value from the cube."),
        source: Some(
            "https://support.microsoft.com/en-us/office/cubevalue-function-8733da24-26d1-4e34-9b3a-84a8f00dcbe0",
        ),
        own: "An aggregated value from an OLAP cube",
    },
    FunctionHelp {
        spelling: "CUMIPMT",
        microsoft: Some("Returns the cumulative interest paid between two periods"),
        source: Some(
            "https://support.microsoft.com/en-us/office/cumipmt-function-61067bb0-9016-427d-b95b-1a752af0e606",
        ),
        own: "Total interest paid on a loan between two periods",
    },
    FunctionHelp {
        spelling: "CUMPRINC",
        microsoft: Some("Returns the cumulative principal paid on a loan between two periods"),
        source: Some(
            "https://support.microsoft.com/en-us/office/cumprinc-function-94a4516d-bd65-41a1-bc16-053a6af4c04d",
        ),
        own: "Principal repaid on a loan from one period to another, added up",
    },
    FunctionHelp {
        spelling: "DATE",
        microsoft: Some("Returns the serial number of a particular date"),
        source: Some(
            "https://support.microsoft.com/en-us/office/date-function-e36c0c8c-4104-49da-ab83-82328b832349",
        ),
        own: "Serial date number for a year, month and day",
    },
    FunctionHelp {
        spelling: "DATEDIF",
        microsoft: Some(
            "Calculates the number of days, months, or years between two dates. This function is useful in formulas where you need to calculate an age.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/datedif-function-25dba1a4-2812-480b-84dd-8b32a451b35c",
        ),
        own: "Whole days, months or years between two dates",
    },
    FunctionHelp {
        spelling: "DATEVALUE",
        microsoft: Some("Converts a date in the form of text to a serial number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/datevalue-function-df8b07d4-7761-4a93-bc33-b7471bbff252",
        ),
        own: "Serial date number read from a date written as text",
    },
    FunctionHelp {
        spelling: "DAVERAGE",
        microsoft: Some("Returns the average of selected database entries"),
        source: Some(
            "https://support.microsoft.com/en-us/office/daverage-function-a6a2d5ac-4b4b-48cd-a1d8-7b37834e5aee",
        ),
        own: "Mean of a database column over the records that match the criteria",
    },
    FunctionHelp {
        spelling: "DAY",
        microsoft: Some("Converts a serial number to a day of the month"),
        source: Some(
            "https://support.microsoft.com/en-us/office/day-function-8a7d1cbb-6c7d-4ba1-8aea-25c134d03101",
        ),
        own: "Day of the month (1 to 31) of a date",
    },
    FunctionHelp {
        spelling: "DAYS",
        microsoft: Some("Returns the number of days between two dates"),
        source: Some(
            "https://support.microsoft.com/en-us/office/days-function-57740535-d549-4395-8728-0f07bff0b9df",
        ),
        own: "Number of days from one date to another",
    },
    FunctionHelp {
        spelling: "DAYS360",
        microsoft: Some("Calculates the number of days between two dates based on a 360-day year"),
        source: Some(
            "https://support.microsoft.com/en-us/office/days360-function-b9a509fd-49ef-407e-94df-0cbda5718c2a",
        ),
        own: "Days between two dates counted with twelve 30-day months",
    },
    FunctionHelp {
        spelling: "DB",
        microsoft: Some(
            "Returns the depreciation of an asset for a specified period by using the fixed-declining balance method",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/db-function-354e7d28-5f93-4ff1-8a52-eb4ee549d9d7",
        ),
        own: "Fixed-declining-balance depreciation of an asset for one period",
    },
    FunctionHelp {
        spelling: "DBCS",
        microsoft: Some(
            "Changes half-width (single-byte) English letters or katakana within a character string to full-width (double-byte) characters",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dbcs-function-a4025e73-63d2-4958-9423-21a24794c9e5",
        ),
        own: "Text with half-width (single-byte) characters turned into their full-width forms",
    },
    FunctionHelp {
        spelling: "DCOUNT",
        microsoft: Some("Counts the cells that contain numbers in a database"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dcount-function-c1fc7b93-fb0d-4d8d-97db-8d5f076eaeb1",
        ),
        own: "How many numbers a database column holds in the records that match the criteria",
    },
    FunctionHelp {
        spelling: "DCOUNTA",
        microsoft: Some("Counts nonblank cells in a database"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dcounta-function-00232a6d-5a66-4a01-a25b-c1653fda1244",
        ),
        own: "How many non-empty entries a database column holds in the matching records",
    },
    FunctionHelp {
        spelling: "DDB",
        microsoft: Some(
            "Returns the depreciation of an asset for a specified period by using the double-declining balance method or some other method that you specify",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ddb-function-519a7a37-8772-4c96-85c0-ed2c209717a5",
        ),
        own: "Double-declining-balance (or other factor) depreciation for one period",
    },
    FunctionHelp {
        spelling: "DEC2BIN",
        microsoft: Some("Converts a decimal number to binary"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dec2bin-function-0f63dd0e-5d1a-42d8-b511-5bf5c6d43838",
        ),
        own: "Decimal number written in binary",
    },
    FunctionHelp {
        spelling: "DEC2HEX",
        microsoft: Some("Converts a decimal number to hexadecimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dec2hex-function-6344ee8b-b6b5-4c6a-a672-f64666704619",
        ),
        own: "Decimal number written in hexadecimal",
    },
    FunctionHelp {
        spelling: "DEC2OCT",
        microsoft: Some("Converts a decimal number to octal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dec2oct-function-c9d835ca-20b7-40c4-8a9e-d3be351ce00f",
        ),
        own: "Decimal number written in octal",
    },
    FunctionHelp {
        spelling: "DECIMAL",
        microsoft: Some(
            "Converts a text representation of a number in a given base into a decimal number",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/decimal-function-ee554665-6176-46ef-82de-0a283658da2e",
        ),
        own: "Value of a number written as text in a given base",
    },
    FunctionHelp {
        spelling: "DEGREES",
        microsoft: Some("Converts radians to degrees"),
        source: Some(
            "https://support.microsoft.com/en-us/office/degrees-function-4d6ec4db-e694-4b94-ace0-1cc3f61f9ba1",
        ),
        own: "Angle in radians expressed in degrees",
    },
    FunctionHelp {
        spelling: "DELTA",
        microsoft: Some("Tests whether two values are equal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/delta-function-2f763672-c959-4e07-ac33-fe03220ba432",
        ),
        own: "1 when two numbers are equal, otherwise 0",
    },
    FunctionHelp {
        spelling: "DETECTLANGUAGE",
        microsoft: Some("Identifies the language of a specified text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/detectlanguage-function-0748e285-1912-4d24-b735-57d18142fa3b",
        ),
        own: "Code of the language a text is written in",
    },
    FunctionHelp {
        spelling: "DEVSQ",
        microsoft: Some("Returns the sum of squares of deviations"),
        source: Some(
            "https://support.microsoft.com/en-us/office/devsq-function-8b739616-8376-4df5-8bd0-cfe0a6caf444",
        ),
        own: "Sum of the squared distances of the values from their mean",
    },
    FunctionHelp {
        spelling: "DGET",
        microsoft: Some(
            "Extracts from a database a single record that matches the specified criteria",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dget-function-455568bf-4eef-45f7-90f0-ec250d00892e",
        ),
        own: "The one value of a database column that matches the criteria",
    },
    FunctionHelp {
        spelling: "DISC",
        microsoft: Some("Returns the discount rate for a security"),
        source: Some(
            "https://support.microsoft.com/en-us/office/disc-function-71fce9f3-3f05-4acf-a5a3-eac6ef4daa53",
        ),
        own: "Yearly discount rate implied by a security's price and redemption value",
    },
    FunctionHelp {
        spelling: "DMAX",
        microsoft: Some("Returns the maximum value from selected database entries"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dmax-function-f4e8209d-8958-4c3d-a1ee-6351665d41c2",
        ),
        own: "Largest value of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DMIN",
        microsoft: Some("Returns the minimum value from selected database entries"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dmin-function-4ae6f1d9-1f26-40f1-a783-6dc3680192a3",
        ),
        own: "Smallest value of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DOLLAR",
        microsoft: Some("Converts a number to text, using the $ (dollar) currency format"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dollar-function-a6cd05d9-9740-4ad3-a469-8109d18ff611",
        ),
        own: "A number as currency text, rounded to a number of decimals",
    },
    FunctionHelp {
        spelling: "DOLLARDE",
        microsoft: Some(
            "Converts a dollar price, expressed as a fraction, into a dollar price, expressed as a decimal number",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dollarde-function-db85aab0-1677-428a-9dfd-a38476693427",
        ),
        own: "A price quoted as a fraction (such as 1.02 for 1 2/16) as a decimal number",
    },
    FunctionHelp {
        spelling: "DOLLARFR",
        microsoft: Some(
            "Converts a dollar price, expressed as a decimal number, into a dollar price, expressed as a fraction",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dollarfr-function-0835d163-3023-4a33-9824-3042c5d4f495",
        ),
        own: "A decimal price written as a whole part and a fraction of a given denominator",
    },
    FunctionHelp {
        spelling: "DPRODUCT",
        microsoft: Some(
            "Multiplies the values in a particular field of records that match the criteria in a database",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dproduct-function-4f96b13e-d49c-47a7-b769-22f6d017cb31",
        ),
        own: "Product of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DROP",
        microsoft: Some(
            "Excludes a specified number of rows or columns from the start or end of an array",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/drop-function-1cb4e151-9e17-4838-abe5-9ba48d8c6a34",
        ),
        own: "An array with a number of rows or columns removed from its start or end",
    },
    FunctionHelp {
        spelling: "DSTDEV",
        microsoft: Some(
            "Estimates the standard deviation based on a sample of selected database entries",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dstdev-function-026b8c73-616d-4b5e-b072-241871c4ab96",
        ),
        own: "Sample standard deviation of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DSTDEVP",
        microsoft: Some(
            "Calculates the standard deviation based on the entire population of selected database entries",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dstdevp-function-04b78995-da03-4813-bbd9-d74fd0f5d94b",
        ),
        own: "Population standard deviation of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DSUM",
        microsoft: Some(
            "Adds the numbers in the field column of records in the database that match the criteria",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dsum-function-53181285-0c4b-4f5a-aaa3-529a322be41b",
        ),
        own: "Total of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DURATION",
        microsoft: Some(
            "Returns the annual duration of a security with periodic interest payments",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/duration-function-b254ea57-eadc-4602-a86a-c8e369334038",
        ),
        own: "Macaulay duration of a security with periodic coupons, in years",
    },
    FunctionHelp {
        spelling: "DVAR",
        microsoft: Some("Estimates variance based on a sample from selected database entries"),
        source: Some(
            "https://support.microsoft.com/en-us/office/dvar-function-d6747ca9-99c7-48bb-996e-9d7af00f3ed1",
        ),
        own: "Sample variance of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "DVARP",
        microsoft: Some(
            "Calculates variance based on the entire population of selected database entries",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/dvarp-function-eb0ba387-9cb7-45c8-81e9-0394912502fc",
        ),
        own: "Population variance of a database column over the matching records",
    },
    FunctionHelp {
        spelling: "EDATE",
        microsoft: Some(
            "Returns the serial number of the date that is the indicated number of months before or after the start date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/edate-function-3c920eb2-6e66-44e7-a1f5-753ae47ee4f5",
        ),
        own: "Date a number of months before or after a start date",
    },
    FunctionHelp {
        spelling: "EFFECT",
        microsoft: Some("Returns the effective annual interest rate"),
        source: Some(
            "https://support.microsoft.com/en-us/office/effect-function-910d4e4c-79e2-4009-95e6-507e04f11bc4",
        ),
        own: "Effective annual rate from a nominal rate and the compounding periods per year",
    },
    FunctionHelp {
        spelling: "ENCODEURL",
        microsoft: Some("Returns a URL-encoded string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/encodeurl-function-07c7fb90-7c60-4bff-8687-fac50fe33d0e",
        ),
        own: "Text made safe for a URL by percent-encoding",
    },
    FunctionHelp {
        spelling: "EOMONTH",
        microsoft: Some(
            "Returns the serial number of the last day of the month before or after a specified number of months",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/eomonth-function-7314ffa1-2bc9-4005-9d66-f49db127d628",
        ),
        own: "Last day of the month a number of months before or after a date",
    },
    FunctionHelp {
        spelling: "ERF",
        microsoft: Some("Returns the error function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/erf-function-c53c7e7b-5482-4b6c-883e-56df3c9af349",
        ),
        own: "Error function, integrated between two limits or from 0",
    },
    FunctionHelp {
        spelling: "ERF.PRECISE",
        microsoft: Some("Returns the error function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/erf-precise-function-9a349593-705c-4278-9a98-e4122831a8e0",
        ),
        own: "Error function integrated from 0 to x",
    },
    FunctionHelp {
        spelling: "ERFC",
        microsoft: Some("Returns the complementary error function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/erfc-function-736e0318-70ba-4e8b-8d08-461fe68b71b3",
        ),
        own: "Complementary error function, 1 - ERF(x)",
    },
    FunctionHelp {
        spelling: "ERFC.PRECISE",
        microsoft: Some("Returns the complementary ERF function integrated between x and infinity"),
        source: Some(
            "https://support.microsoft.com/en-us/office/erfc-precise-function-e90e6bab-f45e-45df-b2ac-cd2eb4d4a273",
        ),
        own: "Complementary error function integrated from x to infinity",
    },
    FunctionHelp {
        spelling: "ERROR.TYPE",
        microsoft: Some("Returns a number corresponding to an error type"),
        source: Some(
            "https://support.microsoft.com/en-us/office/error-type-function-10958677-7c8d-44f7-ae77-b9a9ee6eefaa",
        ),
        own: "Number identifying which error value a cell holds",
    },
    FunctionHelp {
        spelling: "EUROCONVERT",
        microsoft: Some(
            "Converts a number to euros, converts a number from euros to a euro member currency, or converts a number from one euro member currency to another by using the euro as an intermediary (triangulation)",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/euroconvert-function-79c8fd67-c665-450c-bb6c-15fc92f8345c",
        ),
        own: "An amount converted between euro member currencies through the euro",
    },
    FunctionHelp {
        spelling: "EVEN",
        microsoft: Some("Rounds a number up to the nearest even integer"),
        source: Some(
            "https://support.microsoft.com/en-us/office/even-function-197b5f06-c795-4c1e-8696-3c3b8a646cf9",
        ),
        own: "Number rounded away from zero to an even whole number",
    },
    FunctionHelp {
        spelling: "EXACT",
        microsoft: Some("Checks to see if two text values are identical"),
        source: Some(
            "https://support.microsoft.com/en-us/office/exact-function-d3087698-fc15-4a15-9631-12575cf29926",
        ),
        own: "TRUE when two texts are identical, letter case included",
    },
    FunctionHelp {
        spelling: "EXP",
        microsoft: Some("Returns e raised to the power of a given number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/exp-function-c578f034-2c45-4c37-bc8c-329660a63abe",
        ),
        own: "e raised to a power",
    },
    FunctionHelp {
        spelling: "EXPAND",
        microsoft: Some("Expands or pads an array to specified row and column dimensions"),
        source: Some(
            "https://support.microsoft.com/en-us/office/expand-function-7433fba5-4ad1-41da-a904-d5d95808bc38",
        ),
        own: "An array padded out to a given number of rows and columns",
    },
    FunctionHelp {
        spelling: "EXPON.DIST",
        microsoft: Some("Returns the exponential distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/expon-dist-function-4c12ae24-e563-4155-bf3e-8b78b6ae140e",
        ),
        own: "Exponential distribution: the density or the cumulative probability",
    },
    FunctionHelp {
        spelling: "EXPONDIST",
        microsoft: Some("Returns the exponential distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/expondist-function-68ab45fd-cd6d-4887-9770-9357eb8ee06a",
        ),
        own: "Exponential distribution (older form of EXPON.DIST)",
    },
    FunctionHelp {
        spelling: "F.DIST",
        microsoft: Some("Returns the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/f-dist-function-a887efdc-7c8e-46cb-a74a-f884cd29b25d",
        ),
        own: "F distribution: the density or the left-tail probability",
    },
    FunctionHelp {
        spelling: "F.DIST.RT",
        microsoft: Some("Returns the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/f-dist-rt-function-d74cbb00-6017-4ac9-b7d7-6049badc0520",
        ),
        own: "Right-tail probability of the F distribution",
    },
    FunctionHelp {
        spelling: "F.INV",
        microsoft: Some("Returns the inverse of the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/f-inv-function-0dda0cf9-4ea0-42fd-8c3c-417a1ff30dbe",
        ),
        own: "The value at which the left-tail F probability reaches a given level",
    },
    FunctionHelp {
        spelling: "F.INV.RT",
        microsoft: Some("Returns the inverse of the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/f-inv-rt-function-d371aa8f-b0b1-40ef-9cc2-496f0693ac00",
        ),
        own: "The value at which the right-tail F probability falls to a given level",
    },
    FunctionHelp {
        spelling: "F.TEST",
        microsoft: Some("Returns the result of an F-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/f-test-function-100a59e7-4108-46f8-8443-78ffacb6c0a7",
        ),
        own: "Two-tailed probability that two samples' variances do not differ",
    },
    FunctionHelp {
        spelling: "FACT",
        microsoft: Some("Returns the factorial of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fact-function-ca8588c2-15f2-41c0-8e8c-c11bd471a4f3",
        ),
        own: "Factorial: the product 1 x 2 x ... x n",
    },
    FunctionHelp {
        spelling: "FACTDOUBLE",
        microsoft: Some("Returns the double factorial of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/factdouble-function-e67697ac-d214-48eb-b7b7-cce2589ecac8",
        ),
        own: "Double factorial: the product of every second number down from n",
    },
    FunctionHelp {
        spelling: "FALSE",
        microsoft: Some("Returns the logical value FALSE"),
        source: Some(
            "https://support.microsoft.com/en-us/office/false-function-2d58dfa5-9c03-4259-bf8f-f0ae14346904",
        ),
        own: "The logical value FALSE",
    },
    FunctionHelp {
        spelling: "FDIST",
        microsoft: Some("Returns the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fdist-function-ecf76fba-b3f1-4e7d-a57e-6a5b7460b786",
        ),
        own: "Right-tail probability of the F distribution (older form)",
    },
    FunctionHelp {
        spelling: "FILTER",
        microsoft: Some(
            "Use this function to filter a range of data based on criteria you define.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/filter-function-f4f7cb66-82eb-4767-8f7c-4877ad80c759",
        ),
        own: "The rows or columns of an array for which a condition holds",
    },
    FunctionHelp {
        spelling: "FILTERXML",
        microsoft: Some("Returns specific data from the XML content by using the specified XPath"),
        source: Some(
            "https://support.microsoft.com/en-us/office/filterxml-function-4df72efc-11ec-4951-86f5-c1374812f5b7",
        ),
        own: "Values picked out of XML text by an XPath expression",
    },
    FunctionHelp {
        spelling: "FIND",
        microsoft: Some("Finds one text value within another (case-sensitive)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/find-function-c7912941-af2a-4bdf-a553-d0d89b0a0628",
        ),
        own: "Position of one text inside another, case-sensitive",
    },
    FunctionHelp {
        spelling: "FINDB",
        microsoft: Some("Finds one text value within another (case-sensitive)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/find-function-c7912941-af2a-4bdf-a553-d0d89b0a0628",
        ),
        own: "Position of one text inside another, case-sensitive, counted in bytes for double-byte languages",
    },
    FunctionHelp {
        spelling: "FINV",
        microsoft: Some("Returns the inverse of the F probability distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/finv-function-4d46c97c-c368-4852-bc15-41e8e31140b1",
        ),
        own: "Inverse of the right-tail F probability (older form)",
    },
    FunctionHelp {
        spelling: "FISHER",
        microsoft: Some("Returns the Fisher transformation"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fisher-function-d656523c-5076-4f95-b87b-7741bf236c69",
        ),
        own: "Fisher transformation of a correlation, artanh(x)",
    },
    FunctionHelp {
        spelling: "FISHERINV",
        microsoft: Some("Returns the inverse of the Fisher transformation"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fisherinv-function-62504b39-415a-4284-a285-19c8e82f86bb",
        ),
        own: "Inverse Fisher transformation, tanh(y)",
    },
    FunctionHelp {
        spelling: "FIXED",
        microsoft: Some("Formats a number as text with a fixed number of decimals"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fixed-function-ffd5723c-324c-45e9-8b96-e41be2a8274a",
        ),
        own: "A number as text with a fixed number of decimals, with or without thousands separators",
    },
    FunctionHelp {
        spelling: "FLOOR",
        microsoft: Some("Rounds a number down, toward zero"),
        source: Some(
            "https://support.microsoft.com/en-us/office/floor-function-14bb497c-24f2-4e04-b327-b0b4de5a8886",
        ),
        own: "Number rounded down, toward zero, to a multiple of the significance",
    },
    FunctionHelp {
        spelling: "FLOOR.MATH",
        microsoft: Some(
            "Rounds a number down, to the nearest integer or to the nearest multiple of significance",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/floor-math-function-c302b599-fbdb-4177-ba19-2c2b1249a2f5",
        ),
        own: "Number rounded down to a multiple of the significance, with a choice of direction for negative numbers",
    },
    FunctionHelp {
        spelling: "FLOOR.PRECISE",
        microsoft: Some(
            "Rounds a number down to the nearest integer or to the nearest multiple of significance. Regardless of the sign of the number, the number is rounded down.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/floor-precise-function-f769b468-1452-4617-8dc3-02f842a0702e",
        ),
        own: "Number rounded down toward minus infinity to a multiple of the significance, whatever its sign",
    },
    FunctionHelp {
        spelling: "FORECAST",
        microsoft: Some("Returns a value along a linear trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/forecast-and-forecast-linear-functions-50ca49c9-7b40-4892-94e4-7ad38bbeda99",
        ),
        own: "Value predicted for x on the least-squares line through the known points",
    },
    FunctionHelp {
        spelling: "FORECAST.LINEAR",
        microsoft: Some("Returns a value along a linear trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/forecast-and-forecast-linear-functions-50ca49c9-7b40-4892-94e4-7ad38bbeda99",
        ),
        own: "Value predicted for x on the least-squares line through the known points",
    },
    FunctionHelp {
        spelling: "FORMULATEXT",
        microsoft: Some("Returns the formula at the given reference as text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/formulatext-function-0a786771-54fd-4ae2-96ee-09cda35439c8",
        ),
        own: "The formula of a cell, as text",
    },
    FunctionHelp {
        spelling: "FREQUENCY",
        microsoft: Some("Returns a frequency distribution as a vertical array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/frequency-function-44e3be2b-eca0-42cd-a3f7-fd9ea898fdb9",
        ),
        own: "How many values fall into each bin, as a vertical array",
    },
    FunctionHelp {
        spelling: "FTEST",
        microsoft: Some("Returns the result of an F-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ftest-function-4c9e1202-53fe-428c-a737-976f6fc3f9fd",
        ),
        own: "Two-tailed F-test probability (older form of F.TEST)",
    },
    FunctionHelp {
        spelling: "FV",
        microsoft: Some("Returns the future value of an investment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/fv-function-2eef9f44-a084-4c61-bdd8-4fe4bb1b71b3",
        ),
        own: "Future value of an investment with fixed periodic payments and a fixed rate",
    },
    FunctionHelp {
        spelling: "FVSCHEDULE",
        microsoft: Some(
            "Returns the future value of an initial principal after applying a series of compound interest rates",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/fvschedule-function-bec29522-bd87-4082-bab9-a241f3fb251d",
        ),
        own: "Future value of a principal grown by a series of interest rates",
    },
    FunctionHelp {
        spelling: "GAMMA",
        microsoft: Some("Returns the Gamma function value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gamma-function-ce1702b1-cf55-471d-8307-f83be0fc5297",
        ),
        own: "Gamma function",
    },
    FunctionHelp {
        spelling: "GAMMA.DIST",
        microsoft: Some("Returns the gamma distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gamma-dist-function-9b6f1538-d11c-4d5f-8966-21f6a2201def",
        ),
        own: "Gamma distribution: the density or the cumulative probability",
    },
    FunctionHelp {
        spelling: "GAMMA.INV",
        microsoft: Some("Returns the inverse of the gamma cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gamma-inv-function-74991443-c2b0-4be5-aaab-1aa4d71fbb18",
        ),
        own: "The value at which the cumulative gamma distribution reaches a given probability",
    },
    FunctionHelp {
        spelling: "GAMMADIST",
        microsoft: Some("Returns the gamma distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gammadist-function-7327c94d-0f05-4511-83df-1dd7ed23e19e",
        ),
        own: "Gamma distribution (older form of GAMMA.DIST)",
    },
    FunctionHelp {
        spelling: "GAMMAINV",
        microsoft: Some("Returns the inverse of the gamma cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gammainv-function-06393558-37ab-47d0-aa63-432f99e7916d",
        ),
        own: "Inverse of the cumulative gamma distribution (older form)",
    },
    FunctionHelp {
        spelling: "GAMMALN",
        microsoft: Some("Returns the natural logarithm of the gamma function, ?(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gammaln-function-b838c48b-c65f-484f-9e1d-141c55470eb9",
        ),
        own: "Natural logarithm of the gamma function",
    },
    FunctionHelp {
        spelling: "GAMMALN.PRECISE",
        microsoft: Some("Returns the natural logarithm of the gamma function, ?(x)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gammaln-precise-function-5cdfe601-4e1e-4189-9d74-241ef1caa599",
        ),
        own: "Natural logarithm of the gamma function",
    },
    FunctionHelp {
        spelling: "GAUSS",
        microsoft: Some("Returns 0.5 less than the standard normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gauss-function-069f1b4e-7dee-4d6a-a71f-4b69044a6b33",
        ),
        own: "Probability that a standard normal value lies between 0 and z",
    },
    FunctionHelp {
        spelling: "GCD",
        microsoft: Some("Returns the greatest common divisor"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gcd-function-d5107a51-69e3-461f-8e4c-ddfc21b5073a",
        ),
        own: "Greatest common divisor of whole numbers",
    },
    FunctionHelp {
        spelling: "GEOMEAN",
        microsoft: Some("Returns the geometric mean"),
        source: Some(
            "https://support.microsoft.com/en-us/office/geomean-function-db1ac48d-25a5-40a0-ab83-0b38980e40d5",
        ),
        own: "Geometric mean: the n-th root of the product of n positive values",
    },
    FunctionHelp {
        spelling: "GESTEP",
        microsoft: Some("Tests whether a number is greater than a threshold value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/gestep-function-f37e7d2a-41da-4129-be95-640883fca9df",
        ),
        own: "1 when a number is at least the step value, otherwise 0",
    },
    FunctionHelp {
        spelling: "GETPIVOTDATA",
        microsoft: Some("Returns data stored in a PivotTable report"),
        source: Some(
            "https://support.microsoft.com/en-us/office/getpivotdata-function-8c083b99-a922-4ca0-af5e-3af55960761f",
        ),
        own: "A value read out of a PivotTable report",
    },
    FunctionHelp {
        spelling: "GROUPBY",
        microsoft: Some(
            "Helps a user group, aggregate, sort, and filter data based on the fields you specify",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/groupby-function-5e08ae8c-6800-4b72-b623-c41773611505",
        ),
        own: "A summary table with values aggregated by the groups of one or more row fields",
    },
    FunctionHelp {
        spelling: "GROWTH",
        microsoft: Some("Returns values along an exponential trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/growth-function-541a91dc-3d5e-437d-b156-21324e68b80d",
        ),
        own: "Values predicted on the exponential curve fitted to known data",
    },
    FunctionHelp {
        spelling: "HARMEAN",
        microsoft: Some("Returns the harmonic mean"),
        source: Some(
            "https://support.microsoft.com/en-us/office/harmean-function-5efd9184-fab5-42f9-b1d3-57883a1d3bc6",
        ),
        own: "Harmonic mean: n divided by the sum of the reciprocals",
    },
    FunctionHelp {
        spelling: "HEX2BIN",
        microsoft: Some("Converts a hexadecimal number to binary"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hex2bin-function-a13aafaa-5737-4920-8424-643e581828c1",
        ),
        own: "Hexadecimal number written in binary",
    },
    FunctionHelp {
        spelling: "HEX2DEC",
        microsoft: Some("Converts a hexadecimal number to decimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hex2dec-function-8c8c3155-9f37-45a5-a3ee-ee5379ef106e",
        ),
        own: "Decimal value of a hexadecimal number given as text",
    },
    FunctionHelp {
        spelling: "HEX2OCT",
        microsoft: Some("Converts a hexadecimal number to octal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hex2oct-function-54d52808-5d19-4bd0-8a63-1096a5d11912",
        ),
        own: "Hexadecimal number written in octal",
    },
    FunctionHelp {
        spelling: "HLOOKUP",
        microsoft: Some(
            "Looks in the top row of an array and returns the value of the indicated cell",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/hlookup-function-a3034eec-b719-4ba3-bb65-e1ad662ed95f",
        ),
        own: "A value from a given row of a table, in the column found by searching the top row",
    },
    FunctionHelp {
        spelling: "HOUR",
        microsoft: Some("Converts a serial number to an hour"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hour-function-a3afa879-86cb-4339-b1b5-2dd2d7310ac7",
        ),
        own: "Hour (0 to 23) of a time",
    },
    FunctionHelp {
        spelling: "HSTACK",
        microsoft: Some("Appends arrays horizontally and in sequence to return a larger array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hstack-function-98c4ab76-10fe-4b4f-8d5f-af1c125fe8c2",
        ),
        own: "Arrays placed side by side into one array",
    },
    FunctionHelp {
        spelling: "HYPERLINK",
        microsoft: Some(
            "Creates a shortcut or jump that opens a document stored on a network server, an intranet, or the Internet",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/hyperlink-function-333c7ce6-c5ae-4164-9c47-7de9b76f577f",
        ),
        own: "A clickable link to a document or web address, shown with a label",
    },
    FunctionHelp {
        spelling: "HYPGEOM.DIST",
        microsoft: Some("Returns the hypergeometric distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hypgeom-dist-function-6dbd547f-1d12-4b1f-8ae5-b0d9e3d22fbf",
        ),
        own: "Hypergeometric distribution: the chance of k successes in a sample drawn without replacement",
    },
    FunctionHelp {
        spelling: "HYPGEOMDIST",
        microsoft: Some("Returns the hypergeometric distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/hypgeomdist-function-23e37961-2871-4195-9629-d0b2c108a12e",
        ),
        own: "Hypergeometric probability (older form)",
    },
    FunctionHelp {
        spelling: "IF",
        microsoft: Some(
            "Use this function to return one value if a condition is true and another value if it's false.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/if-function-69aed7c9-4e8a-4755-a9bc-aa8bbff73be2",
        ),
        own: "One of two values, chosen by whether a condition is TRUE",
    },
    FunctionHelp {
        spelling: "IFERROR",
        microsoft: Some(
            "Returns a value you specify if a formula evaluates to an error; otherwise, returns the result of the formula",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/iferror-function-c526fd07-caeb-47b8-8bb6-63f3e417f611",
        ),
        own: "The value of an expression, or a fallback when it is an error",
    },
    FunctionHelp {
        spelling: "IFNA",
        microsoft: Some(
            "Returns the value you specify if the expression resolves to #N/A, otherwise returns the result of the expression",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ifna-function-6626c961-a569-42fc-a49d-79b4951fd461",
        ),
        own: "The value of an expression, or a fallback when it is #N/A",
    },
    FunctionHelp {
        spelling: "IFS",
        microsoft: Some(
            "Checks whether one or more conditions are met and returns a value that corresponds to the first TRUE condition.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ifs-function-36329a26-37b2-467c-972b-4a39bd951d45",
        ),
        own: "The value paired with the first condition that is TRUE",
    },
    FunctionHelp {
        spelling: "IMABS",
        microsoft: Some("Returns the absolute value (modulus) of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imabs-function-b31e73c6-d90c-4062-90bc-8eb351d765a1",
        ),
        own: "Modulus (absolute value) of a complex number",
    },
    FunctionHelp {
        spelling: "IMAGE",
        microsoft: Some("Returns an image from a given source"),
        source: Some(
            "https://support.microsoft.com/en-us/office/image-function-7e112975-5e52-4f2a-b9da-1d913d51f5d5",
        ),
        own: "A picture from a web address shown inside the cell",
    },
    FunctionHelp {
        spelling: "IMAGINARY",
        microsoft: Some("Returns the imaginary coefficient of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imaginary-function-dd5952fd-473d-44d9-95a1-9a17b23e428a",
        ),
        own: "Imaginary part of a complex number",
    },
    FunctionHelp {
        spelling: "IMARGUMENT",
        microsoft: Some("Returns the argument theta, an angle expressed in radians"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imargument-function-eed37ec1-23b3-4f59-b9f3-d340358a034a",
        ),
        own: "Argument (angle in radians) of a complex number",
    },
    FunctionHelp {
        spelling: "IMCONJUGATE",
        microsoft: Some("Returns the complex conjugate of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imconjugate-function-2e2fc1ea-f32b-4f9b-9de6-233853bafd42",
        ),
        own: "Complex conjugate of a complex number",
    },
    FunctionHelp {
        spelling: "IMCOS",
        microsoft: Some("Returns the cosine of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imcos-function-dad75277-f592-4a6b-ad6c-be93a808a53c",
        ),
        own: "Cosine of a complex number",
    },
    FunctionHelp {
        spelling: "IMCOSH",
        microsoft: Some("Returns the hyperbolic cosine of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imcosh-function-053e4ddb-4122-458b-be9a-457c405e90ff",
        ),
        own: "Hyperbolic cosine of a complex number",
    },
    FunctionHelp {
        spelling: "IMCOT",
        microsoft: Some("Returns the cotangent of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imcot-function-dc6a3607-d26a-4d06-8b41-8931da36442c",
        ),
        own: "Cotangent of a complex number",
    },
    FunctionHelp {
        spelling: "IMCSC",
        microsoft: Some("Returns the cosecant of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imcsc-function-9e158d8f-2ddf-46cd-9b1d-98e29904a323",
        ),
        own: "Cosecant of a complex number",
    },
    FunctionHelp {
        spelling: "IMCSCH",
        microsoft: Some("Returns the hyperbolic cosecant of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imcsch-function-c0ae4f54-5f09-4fef-8da0-dc33ea2c5ca9",
        ),
        own: "Hyperbolic cosecant of a complex number",
    },
    FunctionHelp {
        spelling: "IMDIV",
        microsoft: Some("Returns the quotient of two complex numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imdiv-function-a505aff7-af8a-4451-8142-77ec3d74d83f",
        ),
        own: "Quotient of two complex numbers",
    },
    FunctionHelp {
        spelling: "IMEXP",
        microsoft: Some("Returns the exponential of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imexp-function-c6f8da1f-e024-4c0c-b802-a60e7147a95f",
        ),
        own: "e raised to a complex power",
    },
    FunctionHelp {
        spelling: "IMLN",
        microsoft: Some("Returns the natural logarithm of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imln-function-32b98bcf-8b81-437c-a636-6fb3aad509d8",
        ),
        own: "Natural logarithm of a complex number",
    },
    FunctionHelp {
        spelling: "IMLOG10",
        microsoft: Some("Returns the base-10 logarithm of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imlog10-function-58200fca-e2a2-4271-8a98-ccd4360213a5",
        ),
        own: "Base-10 logarithm of a complex number",
    },
    FunctionHelp {
        spelling: "IMLOG2",
        microsoft: Some("Returns the base-2 logarithm of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imlog2-function-152e13b4-bc79-486c-a243-e6a676878c51",
        ),
        own: "Base-2 logarithm of a complex number",
    },
    FunctionHelp {
        spelling: "IMPOWER",
        microsoft: Some("Returns a complex number raised to an integer power"),
        source: Some(
            "https://support.microsoft.com/en-us/office/impower-function-210fd2f5-f8ff-4c6a-9d60-30e34fbdef39",
        ),
        own: "A complex number raised to a real power",
    },
    FunctionHelp {
        spelling: "IMPRODUCT",
        microsoft: Some("Returns the product of from 2 to 255 complex numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/improduct-function-2fb8651a-a4f2-444f-975e-8ba7aab3a5ba",
        ),
        own: "Product of complex numbers",
    },
    FunctionHelp {
        spelling: "IMREAL",
        microsoft: Some("Returns the real coefficient of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imreal-function-d12bc4c0-25d0-4bb3-a25f-ece1938bf366",
        ),
        own: "Real part of a complex number",
    },
    FunctionHelp {
        spelling: "IMSEC",
        microsoft: Some("Returns the secant of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsec-function-6df11132-4411-4df4-a3dc-1f17372459e0",
        ),
        own: "Secant of a complex number",
    },
    FunctionHelp {
        spelling: "IMSECH",
        microsoft: Some("Returns the hyperbolic secant of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsech-function-f250304f-788b-4505-954e-eb01fa50903b",
        ),
        own: "Hyperbolic secant of a complex number",
    },
    FunctionHelp {
        spelling: "IMSIN",
        microsoft: Some("Returns the sine of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsin-function-1ab02a39-a721-48de-82ef-f52bf37859f6",
        ),
        own: "Sine of a complex number",
    },
    FunctionHelp {
        spelling: "IMSINH",
        microsoft: Some("Returns the hyperbolic sine of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsinh-function-dfb9ec9e-8783-4985-8c42-b028e9e8da3d",
        ),
        own: "Hyperbolic sine of a complex number",
    },
    FunctionHelp {
        spelling: "IMSQRT",
        microsoft: Some("Returns the square root of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsqrt-function-e1753f80-ba11-4664-a10e-e17368396b70",
        ),
        own: "Principal square root of a complex number",
    },
    FunctionHelp {
        spelling: "IMSUB",
        microsoft: Some("Returns the difference between two complex numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsub-function-2e404b4d-4935-4e85-9f52-cb08b9a45054",
        ),
        own: "One complex number minus another",
    },
    FunctionHelp {
        spelling: "IMSUM",
        microsoft: Some("Returns the sum of complex numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imsum-function-81542999-5f1c-4da6-9ffe-f1d7aaa9457f",
        ),
        own: "Sum of complex numbers",
    },
    FunctionHelp {
        spelling: "IMTAN",
        microsoft: Some("Returns the tangent of a complex number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/imtan-function-8478f45d-610a-43cf-8544-9fc0b553a132",
        ),
        own: "Tangent of a complex number",
    },
    FunctionHelp {
        spelling: "INDEX",
        microsoft: Some("Uses an index to choose a value from a reference or array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/index-function-a5dcf0dd-996d-40a4-a822-b56b061328bd",
        ),
        own: "The value or reference at a given row and column of an array or range",
    },
    FunctionHelp {
        spelling: "INDIRECT",
        microsoft: Some("Returns a reference indicated by a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/indirect-function-474b3a3a-8a26-4f44-b491-92b6306fa261",
        ),
        own: "The reference that a text names",
    },
    FunctionHelp {
        spelling: "INFO",
        microsoft: Some("Returns information about the current operating environment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/info-function-725f259a-0e4b-49b3-8b52-58815c69acae",
        ),
        own: "A fact about the environment Excel runs in, such as the version or the system",
    },
    FunctionHelp {
        spelling: "INT",
        microsoft: Some("Rounds a number down to the nearest integer"),
        source: Some(
            "https://support.microsoft.com/en-us/office/int-function-a6c4af9e-356d-4369-ab6a-cb1fd9d343ef",
        ),
        own: "Number rounded down to a whole number",
    },
    FunctionHelp {
        spelling: "INTERCEPT",
        microsoft: Some("Returns the intercept of the linear regression line"),
        source: Some(
            "https://support.microsoft.com/en-us/office/intercept-function-2a9b74e2-9d47-4772-b663-3bca70bf63ef",
        ),
        own: "Where the least-squares line through the known points crosses the y-axis",
    },
    FunctionHelp {
        spelling: "INTRATE",
        microsoft: Some("Returns the interest rate for a fully invested security"),
        source: Some(
            "https://support.microsoft.com/en-us/office/intrate-function-5cb34dde-a221-4cb6-b3eb-0b9e55e1316f",
        ),
        own: "Interest rate of a security bought and redeemed at given amounts",
    },
    FunctionHelp {
        spelling: "IPMT",
        microsoft: Some("Returns the interest payment for an investment for a given period"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ipmt-function-5cce0ad6-8402-4a41-8d29-61a0b054cb6f",
        ),
        own: "Interest part of one payment on a loan or investment",
    },
    FunctionHelp {
        spelling: "IRR",
        microsoft: Some("Returns the internal rate of return for a series of cash flows"),
        source: Some(
            "https://support.microsoft.com/en-us/office/irr-function-64925eaa-9988-495b-b290-3ad0c163c1bc",
        ),
        own: "Rate at which a series of periodic cash flows has a net present value of zero",
    },
    FunctionHelp {
        spelling: "ISBLANK",
        microsoft: Some("Returns TRUE if the value is blank"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the cell is empty",
    },
    FunctionHelp {
        spelling: "ISERR",
        microsoft: Some("Returns TRUE if the value is any error value except #N/A"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE for any error value other than #N/A",
    },
    FunctionHelp {
        spelling: "ISERROR",
        microsoft: Some("Returns TRUE if the value is any error value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE for any error value",
    },
    FunctionHelp {
        spelling: "ISEVEN",
        microsoft: Some("Returns TRUE if the number is even"),
        source: Some(
            "https://support.microsoft.com/en-us/office/iseven-function-aa15929a-d77b-4fbb-92f4-2f479af55356",
        ),
        own: "TRUE when the whole part of a number is even",
    },
    FunctionHelp {
        spelling: "ISFORMULA",
        microsoft: Some("Returns TRUE if there is a reference to a cell that contains a formula"),
        source: Some(
            "https://support.microsoft.com/en-us/office/isformula-function-e4d1355f-7121-4ef2-801e-3839bfd6b1e5",
        ),
        own: "TRUE when the cell holds a formula",
    },
    FunctionHelp {
        spelling: "ISLOGICAL",
        microsoft: Some("Returns TRUE if the value is a logical value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is TRUE or FALSE",
    },
    FunctionHelp {
        spelling: "ISNA",
        microsoft: Some("Returns TRUE if the value is the #N/A error value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is #N/A",
    },
    FunctionHelp {
        spelling: "ISNONTEXT",
        microsoft: Some("Returns TRUE if the value is not text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is anything but text",
    },
    FunctionHelp {
        spelling: "ISNUMBER",
        microsoft: Some("Returns TRUE if the value is a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is a number",
    },
    FunctionHelp {
        spelling: "ISO.CEILING",
        microsoft: Some(
            "Returns a number that is rounded up to the nearest integer or to the nearest multiple of significance",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/iso-ceiling-function-e587bb73-6cc2-4113-b664-ff5b09859a83",
        ),
        own: "Number rounded up toward plus infinity to a multiple of the significance (ISO form)",
    },
    FunctionHelp {
        spelling: "ISODD",
        microsoft: Some("Returns TRUE if the number is odd"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the whole part of a number is odd",
    },
    FunctionHelp {
        spelling: "ISOMITTED",
        microsoft: Some(
            "Checks whether the value in a LAMBDA is missing and returns TRUE or FALSE",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/isomitted-function-831d6fbc-0f07-40c4-9c5b-9c73fd1d60c1",
        ),
        own: "TRUE when a LAMBDA parameter was left out of the call",
    },
    FunctionHelp {
        spelling: "ISOWEEKNUM",
        microsoft: Some("Returns the number of the ISO week number of the year for a given date"),
        source: Some(
            "https://support.microsoft.com/en-us/office/isoweeknum-function-1c2d0afe-d25b-4ab1-8894-8d0520e90e0e",
        ),
        own: "ISO 8601 week number of a date",
    },
    FunctionHelp {
        spelling: "ISPMT",
        microsoft: Some("Calculates the interest paid during a specific period of an investment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ispmt-function-fa58adb6-9d39-4ce0-8f43-75399cea56cc",
        ),
        own: "Interest paid in one period of a loan repaid in equal principal parts",
    },
    FunctionHelp {
        spelling: "ISREF",
        microsoft: Some("Returns TRUE if the value is a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is a reference",
    },
    FunctionHelp {
        spelling: "ISTEXT",
        microsoft: Some("Returns TRUE if the value is text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/is-functions-0f2d7971-6019-40a0-a171-f2d869135665",
        ),
        own: "TRUE when the value is text",
    },
    FunctionHelp {
        spelling: "KURT",
        microsoft: Some("Returns the kurtosis of a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/kurt-function-bc3a265c-5da4-4dcb-b7fd-c237789095ab",
        ),
        own: "Sample excess kurtosis: how heavy the tails of the data are",
    },
    FunctionHelp {
        spelling: "LAMBDA",
        microsoft: Some("Create custom, reusable functions and call them by a friendly name"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lambda-function-bd212d27-1cd1-4321-a34a-ccbf254b8b67",
        ),
        own: "A function defined in the formula from parameters and a calculation",
    },
    FunctionHelp {
        spelling: "LARGE",
        microsoft: Some("Returns the k-th largest value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/large-function-3af0af19-1190-42bb-bb8b-01672ec00a64",
        ),
        own: "The k-th largest value",
    },
    FunctionHelp {
        spelling: "LCM",
        microsoft: Some("Returns the least common multiple"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lcm-function-7152b67a-8bb5-4075-ae5c-06ede5563c94",
        ),
        own: "Least common multiple of whole numbers",
    },
    FunctionHelp {
        spelling: "LEFT",
        microsoft: Some("Returns the leftmost characters from a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/left-function-9203d2d2-7960-479b-84c6-1ea52b99640c",
        ),
        own: "The first characters of a text",
    },
    FunctionHelp {
        spelling: "LEFTB",
        microsoft: Some("Returns the leftmost characters from a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/left-function-9203d2d2-7960-479b-84c6-1ea52b99640c",
        ),
        own: "The first characters of a text, counted in bytes for double-byte languages",
    },
    FunctionHelp {
        spelling: "LEN",
        microsoft: Some("Returns the number of characters in a text string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/len-function-29236f94-cedc-429d-affd-b5e33d2c67cb",
        ),
        own: "Text length in characters",
    },
    FunctionHelp {
        spelling: "LENB",
        microsoft: Some("Returns the number of characters in a text string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/len-function-29236f94-cedc-429d-affd-b5e33d2c67cb",
        ),
        own: "Number of bytes a text takes in double-byte languages",
    },
    FunctionHelp {
        spelling: "LET",
        microsoft: Some("Use this function to assign names to calculation results."),
        source: Some(
            "https://support.microsoft.com/en-us/office/let-function-34842dd8-b92b-4d3f-b325-b8b8f9908999",
        ),
        own: "A calculation that names intermediate results and uses them",
    },
    FunctionHelp {
        spelling: "LINEST",
        microsoft: Some("Returns the parameters of a linear trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/linest-function-84d7d0d9-6e50-4101-977a-fa7abf772b6d",
        ),
        own: "Coefficients and statistics of the least-squares linear fit",
    },
    FunctionHelp {
        spelling: "LN",
        microsoft: Some("Returns the natural logarithm of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ln-function-81fe1ed7-dac9-4acd-ba1d-07a142c6118f",
        ),
        own: "Natural logarithm",
    },
    FunctionHelp {
        spelling: "LOG",
        microsoft: Some("Returns the logarithm of a number to a specified base"),
        source: Some(
            "https://support.microsoft.com/en-us/office/log-function-4e82f196-1ca9-4747-8fb0-6c4a3abb3280",
        ),
        own: "Logarithm in a given base (10 when the base is left out)",
    },
    FunctionHelp {
        spelling: "LOG10",
        microsoft: Some("Returns the base-10 logarithm of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/log10-function-c75b881b-49dd-44fb-b6f4-37e3486a0211",
        ),
        own: "Base-10 logarithm",
    },
    FunctionHelp {
        spelling: "LOGEST",
        microsoft: Some("Returns the parameters of an exponential trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/logest-function-f27462d8-3657-4030-866b-a272c1d18b4b",
        ),
        own: "Coefficients and statistics of the exponential curve fitted to the data",
    },
    FunctionHelp {
        spelling: "LOGINV",
        microsoft: Some("Returns the inverse of the lognormal cumulative distribution function"),
        source: Some(
            "https://support.microsoft.com/en-us/office/loginv-function-0bd7631a-2725-482b-afb4-de23df77acfe",
        ),
        own: "Inverse of the cumulative lognormal distribution (older form)",
    },
    FunctionHelp {
        spelling: "LOGNORM.DIST",
        microsoft: Some("Returns the cumulative lognormal distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lognorm-dist-function-eb60d00b-48a9-4217-be2b-6074aee6b070",
        ),
        own: "Lognormal distribution: the density or the cumulative probability",
    },
    FunctionHelp {
        spelling: "LOGNORM.INV",
        microsoft: Some("Returns the inverse of the lognormal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lognorm-inv-function-fe79751a-f1f2-4af8-a0a1-e151b2d4f600",
        ),
        own: "The value at which the cumulative lognormal distribution reaches a given probability",
    },
    FunctionHelp {
        spelling: "LOGNORMDIST",
        microsoft: Some("Returns the cumulative lognormal distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lognormdist-function-f8d194cb-9ee3-4034-8c75-1bdb3884100b",
        ),
        own: "Cumulative lognormal probability (older form)",
    },
    FunctionHelp {
        spelling: "LOOKUP",
        microsoft: Some("Looks up values in a vector or array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lookup-function-446d94af-663b-451d-8251-369d5e3864cb",
        ),
        own: "A value found by searching one row or column, taken from the matching place in another",
    },
    FunctionHelp {
        spelling: "LOWER",
        microsoft: Some("Converts text to lowercase"),
        source: Some(
            "https://support.microsoft.com/en-us/office/lower-function-3f21df02-a80c-44b2-afaf-81358f9fdeb4",
        ),
        own: "Text in lower case",
    },
    FunctionHelp {
        spelling: "MAKEARRAY",
        microsoft: Some(
            "Returns a calculated array of a specified row and column size, by applying a LAMBDA",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/makearray-function-b80da5ad-b338-4149-a523-5b221da09097",
        ),
        own: "An array of a given size whose items a LAMBDA computes from row and column",
    },
    FunctionHelp {
        spelling: "MAP",
        microsoft: Some(
            "Returns an array formed by mapping each value in the array(s) to a new value by applying a LAMBDA to create a new value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/map-function-48006093-f97c-47c1-bfcc-749263bb1f01",
        ),
        own: "An array of the results of a LAMBDA applied to each item of the given arrays",
    },
    FunctionHelp {
        spelling: "MATCH",
        microsoft: Some("Looks up values in a reference or array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/match-function-e8dffd45-c762-47d6-bf89-533f4a37673a",
        ),
        own: "Position of a value within a row or column",
    },
    FunctionHelp {
        spelling: "MAX",
        microsoft: Some("Returns the maximum value in a list of arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/max-function-e0012414-9ac8-4b34-9a47-73e662c08098",
        ),
        own: "Largest of the numbers",
    },
    FunctionHelp {
        spelling: "MAXA",
        microsoft: Some(
            "Returns the maximum value in a list of arguments, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/maxa-function-814bda1e-3840-4bff-9365-2f59ac2ee62d",
        ),
        own: "Largest value, counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "MAXIFS",
        microsoft: Some(
            "Returns the maximum value among cells specified by a given set of conditions or criteria",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/maxifs-function-dfd611e6-da2c-488a-919b-9b6376b28883",
        ),
        own: "Largest value among the cells that meet every condition",
    },
    FunctionHelp {
        spelling: "MDETERM",
        microsoft: Some("Returns the matrix determinant of an array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mdeterm-function-e7bfa857-3834-422b-b871-0ffd03717020",
        ),
        own: "Determinant of a square matrix",
    },
    FunctionHelp {
        spelling: "MDURATION",
        microsoft: Some(
            "Returns the Macauley modified duration for a security with an assumed par value of $100",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mduration-function-b3786a69-4f20-469a-94ad-33e5b90a763c",
        ),
        own: "Modified duration of a security with periodic coupons",
    },
    FunctionHelp {
        spelling: "MEDIAN",
        microsoft: Some("Returns the median of the given numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/median-function-d0916313-4753-414c-8537-ce85bdd967d2",
        ),
        own: "Middle value of the numbers once sorted",
    },
    FunctionHelp {
        spelling: "MID",
        microsoft: Some(
            "Returns a specific number of characters from a text string starting at the position you specify",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mid-function-d5f9e25c-d7d6-472e-b568-4ecb12433028",
        ),
        own: "Characters from the middle of a text, from a start position for a given length",
    },
    FunctionHelp {
        spelling: "MIDB",
        microsoft: Some(
            "Returns a specific number of characters from a text string starting at the position you specify",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mid-function-d5f9e25c-d7d6-472e-b568-4ecb12433028",
        ),
        own: "Characters from the middle of a text, counted in bytes for double-byte languages",
    },
    FunctionHelp {
        spelling: "MIN",
        microsoft: Some("Returns the minimum value in a list of arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/min-function-61635d12-920f-4ce2-a70f-96f202dcc152",
        ),
        own: "Smallest of the numbers",
    },
    FunctionHelp {
        spelling: "MINA",
        microsoft: Some(
            "Returns the smallest value in a list of arguments, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mina-function-245a6f46-7ca5-4dc7-ab49-805341bc31d3",
        ),
        own: "Smallest value, counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "MINIFS",
        microsoft: Some(
            "Returns the minimum value among cells specified by a given set of conditions or criteria.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/minifs-function-6ca1ddaa-079b-4e74-80cc-72eef32e6599",
        ),
        own: "Smallest value among the cells that meet every condition",
    },
    FunctionHelp {
        spelling: "MINUTE",
        microsoft: Some("Converts a serial number to a minute"),
        source: Some(
            "https://support.microsoft.com/en-us/office/minute-function-af728df0-05c4-4b07-9eed-a84801a60589",
        ),
        own: "Minute (0 to 59) of a time",
    },
    FunctionHelp {
        spelling: "MINVERSE",
        microsoft: Some("Returns the matrix inverse of an array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/minverse-function-11f55086-adde-4c9f-8eb9-59da2d72efc6",
        ),
        own: "Inverse of a square matrix",
    },
    FunctionHelp {
        spelling: "MIRR",
        microsoft: Some(
            "Returns the internal rate of return where positive and negative cash flows are financed at different rates",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mirr-function-b020f038-7492-4fb4-93c1-35c345b53524",
        ),
        own: "Internal rate of return with separate finance and reinvestment rates",
    },
    FunctionHelp {
        spelling: "MMULT",
        microsoft: Some("Returns the matrix product of two arrays"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mmult-function-40593ed7-a3cd-4b6b-b9a3-e4ad3c7245eb",
        ),
        own: "Matrix multiplication: rows of the first array times columns of the second",
    },
    FunctionHelp {
        spelling: "MOD",
        microsoft: Some("Returns the remainder from division"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mod-function-9b6cd169-b6ee-406a-a97b-edf2a9dc24f3",
        ),
        own: "Remainder after division, with the sign of the divisor",
    },
    FunctionHelp {
        spelling: "MODE",
        microsoft: Some("Returns the most common value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mode-function-e45192ce-9122-4980-82ed-4bdc34973120",
        ),
        own: "Most frequent number (older form)",
    },
    FunctionHelp {
        spelling: "MODE.MULT",
        microsoft: Some(
            "Returns a vertical array of the most frequently occurring, or repetitive values in an array or range of data",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/mode-mult-function-50fd9464-b2ba-4191-b57a-39446689ae8c",
        ),
        own: "All the most frequent numbers, as a vertical array",
    },
    FunctionHelp {
        spelling: "MODE.SNGL",
        microsoft: Some("Returns the most common value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mode-sngl-function-f1267c16-66c6-4386-959f-8fba5f8bb7f8",
        ),
        own: "Most frequent number",
    },
    FunctionHelp {
        spelling: "MONTH",
        microsoft: Some("Converts a serial number to a month"),
        source: Some(
            "https://support.microsoft.com/en-us/office/month-function-579a2881-199b-48b2-ab90-ddba0eba86e8",
        ),
        own: "Month (1 to 12) of a date",
    },
    FunctionHelp {
        spelling: "MROUND",
        microsoft: Some("Returns a number rounded to the desired multiple"),
        source: Some(
            "https://support.microsoft.com/en-us/office/mround-function-c299c3b0-15a5-426d-aa4b-d2d5b3baf427",
        ),
        own: "Number rounded to the nearest multiple of another",
    },
    FunctionHelp {
        spelling: "MULTINOMIAL",
        microsoft: Some("Returns the multinomial of a set of numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/multinomial-function-6fa6373c-6533-41a2-a45e-a56db1db1bf6",
        ),
        own: "Multinomial coefficient: (a + b + ...)! / (a! b! ...)",
    },
    FunctionHelp {
        spelling: "MUNIT",
        microsoft: Some("Returns the unit matrix or the specified dimension"),
        source: Some(
            "https://support.microsoft.com/en-us/office/munit-function-c9fe916a-dc26-4105-997d-ba22799853a3",
        ),
        own: "Identity matrix of a given size",
    },
    FunctionHelp {
        spelling: "N",
        microsoft: Some("Returns a value converted to a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/n-function-a624cad1-3635-4208-b54a-29733d1278c9",
        ),
        own: "A value as a number: numbers as they are, dates as serials, TRUE as 1, everything else 0",
    },
    FunctionHelp {
        spelling: "NA",
        microsoft: Some("Returns the error value #N/A"),
        source: Some(
            "https://support.microsoft.com/en-us/office/na-function-5469c2d1-a90c-4fb5-9bbc-64bd9bb6b47c",
        ),
        own: "The error value #N/A",
    },
    FunctionHelp {
        spelling: "NEGBINOM.DIST",
        microsoft: Some("Returns the negative binomial distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/negbinom-dist-function-c8239f89-c2d0-45bd-b6af-172e570f8599",
        ),
        own: "Negative binomial distribution: the chance of f failures before the s-th success",
    },
    FunctionHelp {
        spelling: "NEGBINOMDIST",
        microsoft: Some("Returns the negative binomial distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/negbinomdist-function-f59b0a37-bae2-408d-b115-a315609ba714",
        ),
        own: "Negative binomial probability (older form)",
    },
    FunctionHelp {
        spelling: "NETWORKDAYS",
        microsoft: Some("Returns the number of whole workdays between two dates"),
        source: Some(
            "https://support.microsoft.com/en-us/office/networkdays-function-48e717bf-a7a3-495f-969e-5005e3eb18e7",
        ),
        own: "Working days between two dates, leaving out weekends and listed holidays",
    },
    FunctionHelp {
        spelling: "NETWORKDAYS.INTL",
        microsoft: Some(
            "Returns the number of whole workdays between two dates using parameters to indicate which and how many days are weekend days",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/networkdays-intl-function-a9b26239-4f20-46a1-9ab8-4e925bfd5e28",
        ),
        own: "Working days between two dates with a chosen weekend and listed holidays",
    },
    FunctionHelp {
        spelling: "NOMINAL",
        microsoft: Some("Returns the annual nominal interest rate"),
        source: Some(
            "https://support.microsoft.com/en-us/office/nominal-function-7f1ae29b-6b92-435e-b950-ad8b190ddd2b",
        ),
        own: "Nominal annual rate from an effective rate and the compounding periods per year",
    },
    FunctionHelp {
        spelling: "NORM.DIST",
        microsoft: Some("Returns the normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/norm-dist-function-edb1cc14-a21c-4e53-839d-8082074c9f8d",
        ),
        own: "Normal distribution with a given mean and standard deviation: density or cumulative",
    },
    FunctionHelp {
        spelling: "NORM.INV",
        microsoft: Some("Returns the inverse of the normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/norm-inv-function-54b30935-fee7-493c-bedb-2278a9db7e13",
        ),
        own: "The value at which the cumulative normal distribution reaches a given probability",
    },
    FunctionHelp {
        spelling: "NORM.S.DIST",
        microsoft: Some("Returns the standard normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/norm-s-dist-function-1e787282-3832-4520-a9ae-bd2a8d99ba88",
        ),
        own: "Standard normal distribution: density or cumulative",
    },
    FunctionHelp {
        spelling: "NORM.S.INV",
        microsoft: Some("Returns the inverse of the standard normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/norm-s-inv-function-d6d556b4-ab7f-49cd-b526-5a20918452b1",
        ),
        own: "The z at which the cumulative standard normal distribution reaches a given probability",
    },
    FunctionHelp {
        spelling: "NORMDIST",
        microsoft: Some("Returns the normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/normdist-function-126db625-c53e-4591-9a22-c9ff422d6d58",
        ),
        own: "Normal distribution (older form of NORM.DIST)",
    },
    FunctionHelp {
        spelling: "NORMINV",
        microsoft: Some("Returns the inverse of the normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/norminv-function-87981ab8-2de0-4cb0-b1aa-e21d4cb879b8",
        ),
        own: "Inverse of the cumulative normal distribution (older form)",
    },
    FunctionHelp {
        spelling: "NORMSDIST",
        microsoft: Some("Returns the standard normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/normsdist-function-463369ea-0345-445d-802a-4ff0d6ce7cac",
        ),
        own: "Cumulative standard normal probability (older form)",
    },
    FunctionHelp {
        spelling: "NORMSINV",
        microsoft: Some("Returns the inverse of the standard normal cumulative distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/normsinv-function-8d1bce66-8e4d-4f3b-967c-30eed61f019d",
        ),
        own: "Inverse of the cumulative standard normal distribution (older form)",
    },
    FunctionHelp {
        spelling: "NOT",
        microsoft: Some("Reverses the logic of its argument"),
        source: Some(
            "https://support.microsoft.com/en-us/office/not-function-9cfc6011-a054-40c7-a140-cd4ba2d87d77",
        ),
        own: "The opposite logical value",
    },
    FunctionHelp {
        spelling: "NOW",
        microsoft: Some("Returns the serial number of the current date and time"),
        source: Some(
            "https://support.microsoft.com/en-us/office/now-function-3337fd29-145a-4347-b2e6-20c904739c46",
        ),
        own: "Current date and time as a serial number",
    },
    FunctionHelp {
        spelling: "NPER",
        microsoft: Some("Returns the number of periods for an investment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/nper-function-240535b5-6653-4d2d-bfcf-b6a38151d815",
        ),
        own: "Number of periods needed to pay off a loan or reach a target value",
    },
    FunctionHelp {
        spelling: "NPV",
        microsoft: Some(
            "Returns the net present value of an investment based on a series of periodic cash flows and a discount rate",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/npv-function-8672cb67-2576-4d07-b67b-ac28acf2a568",
        ),
        own: "Net present value of periodic cash flows at a discount rate",
    },
    FunctionHelp {
        spelling: "NUMBERVALUE",
        microsoft: Some("Converts text to number in a locale-independent manner"),
        source: Some(
            "https://support.microsoft.com/en-us/office/numbervalue-function-1b05c8cf-2bfa-4437-af70-596c7ea7d879",
        ),
        own: "Number read from text with the given decimal and group separators",
    },
    FunctionHelp {
        spelling: "OCT2BIN",
        microsoft: Some("Converts an octal number to binary"),
        source: Some(
            "https://support.microsoft.com/en-us/office/oct2bin-function-55383471-3c56-4d27-9522-1a8ec646c589",
        ),
        own: "Octal number written in binary",
    },
    FunctionHelp {
        spelling: "OCT2DEC",
        microsoft: Some("Converts an octal number to decimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/oct2dec-function-87606014-cb98-44b2-8dbb-e48f8ced1554",
        ),
        own: "Decimal value of an octal number given as text",
    },
    FunctionHelp {
        spelling: "OCT2HEX",
        microsoft: Some("Converts an octal number to hexadecimal"),
        source: Some(
            "https://support.microsoft.com/en-us/office/oct2hex-function-912175b4-d497-41b4-a029-221f051b858f",
        ),
        own: "Octal number written in hexadecimal",
    },
    FunctionHelp {
        spelling: "ODD",
        microsoft: Some("Rounds a number up to the nearest odd integer"),
        source: Some(
            "https://support.microsoft.com/en-us/office/odd-function-deae64eb-e08a-4c88-8b40-6d0b42575c98",
        ),
        own: "Number rounded away from zero to an odd whole number",
    },
    FunctionHelp {
        spelling: "ODDFPRICE",
        microsoft: Some(
            "Returns the price per $100 face value of a security with an odd first period",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/oddfprice-function-d7d664a8-34df-4233-8d2b-922bcf6a69e1",
        ),
        own: "Price per 100 face value of a security whose first period is irregular",
    },
    FunctionHelp {
        spelling: "ODDFYIELD",
        microsoft: Some("Returns the yield of a security with an odd first period"),
        source: Some(
            "https://support.microsoft.com/en-us/office/oddfyield-function-66bc8b7b-6501-4c93-9ce3-2fd16220fe37",
        ),
        own: "Yield of a security whose first period is irregular",
    },
    FunctionHelp {
        spelling: "ODDLPRICE",
        microsoft: Some(
            "Returns the price per $100 face value of a security with an odd last period",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/oddlprice-function-fb657749-d200-4902-afaf-ed5445027fc4",
        ),
        own: "Price per 100 face value of a security whose last period is irregular",
    },
    FunctionHelp {
        spelling: "ODDLYIELD",
        microsoft: Some("Returns the yield of a security with an odd last period"),
        source: Some(
            "https://support.microsoft.com/en-us/office/oddlyield-function-c873d088-cf40-435f-8d41-c8232fee9238",
        ),
        own: "Yield of a security whose last period is irregular",
    },
    FunctionHelp {
        spelling: "OFFSET",
        microsoft: Some("Returns a reference offset from a given reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/offset-function-c8de19ae-dd79-4b9b-a14e-b4d906d11b66",
        ),
        own: "A reference shifted by rows and columns from a start, optionally resized",
    },
    FunctionHelp {
        spelling: "OP_IMPLICIT_INTERSECTION",
        microsoft: None,
        source: None,
        own: "Projects the implicit-intersection result for a reference or array in the current formula context.",
    },
    FunctionHelp {
        spelling: "OR",
        microsoft: Some("Returns TRUE if any argument is TRUE"),
        source: Some(
            "https://support.microsoft.com/en-us/office/or-function-7d17ad14-8700-4281-b308-00b131e22af0",
        ),
        own: "TRUE when at least one argument is TRUE",
    },
    FunctionHelp {
        spelling: "PDURATION",
        microsoft: Some(
            "Returns the number of periods required by an investment to reach a specified value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/pduration-function-44f33460-5be5-4c90-b857-22308892adaf",
        ),
        own: "Number of periods an investment needs to grow to a target value at a fixed rate",
    },
    FunctionHelp {
        spelling: "PEARSON",
        microsoft: Some("Returns the Pearson product moment correlation coefficient"),
        source: Some(
            "https://support.microsoft.com/en-us/office/pearson-function-0c3e30fc-e5af-49c4-808a-3ef66e034c18",
        ),
        own: "Pearson correlation coefficient of two data sets",
    },
    FunctionHelp {
        spelling: "PERCENTILE",
        microsoft: Some("Returns the k-th percentile of values in a range"),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentile-function-91b43a53-543c-4708-93de-d626debdddca",
        ),
        own: "Value at a given percentile, inclusive of 0 and 1 (older form)",
    },
    FunctionHelp {
        spelling: "PERCENTILE.EXC",
        microsoft: Some(
            "Returns the k-th percentile of values in a range, where k is in the range 0..1, exclusive",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentile-exc-function-bbaa7204-e9e1-4010-85bf-c31dc5dce4ba",
        ),
        own: "Value at a given percentile, with 0 and 1 excluded",
    },
    FunctionHelp {
        spelling: "PERCENTILE.INC",
        microsoft: Some("Returns the k-th percentile of values in a range"),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentile-inc-function-680f9539-45eb-410b-9a5e-c1355e5fe2ed",
        ),
        own: "Value at a given percentile, with 0 and 1 included",
    },
    FunctionHelp {
        spelling: "PERCENTOF",
        microsoft: Some("Sums the values in the subset and divides it by all the values"),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentof-function-7c66da0a-ac30-45d0-bfc7-834a8bd7c962",
        ),
        own: "Share of a subset in a total, as a fraction",
    },
    FunctionHelp {
        spelling: "PERCENTRANK",
        microsoft: Some("Returns the percentage rank of a value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentrank-function-f1b5836c-9619-4847-9fc9-080ec9024442",
        ),
        own: "Relative standing of a value in a data set, as a fraction (older form)",
    },
    FunctionHelp {
        spelling: "PERCENTRANK.EXC",
        microsoft: Some(
            "Returns the rank of a value in a data set as a percentage (0..1, exclusive) of the data set",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentrank-exc-function-d8afee96-b7e2-4a2f-8c01-8fcdedaa6314",
        ),
        own: "Relative standing of a value in a data set, with 0 and 1 excluded",
    },
    FunctionHelp {
        spelling: "PERCENTRANK.INC",
        microsoft: Some("Returns the percentage rank of a value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/percentrank-inc-function-149592c9-00c0-49ba-86c1-c1f45b80463a",
        ),
        own: "Relative standing of a value in a data set, with 0 and 1 included",
    },
    FunctionHelp {
        spelling: "PERMUT",
        microsoft: Some("Returns the number of permutations for a given number of objects"),
        source: Some(
            "https://support.microsoft.com/en-us/office/permut-function-3bd1cb9a-2880-41ab-a197-f246a7a602d3",
        ),
        own: "Number of ordered arrangements of k items chosen from n",
    },
    FunctionHelp {
        spelling: "PERMUTATIONA",
        microsoft: Some(
            "Returns the number of permutations for a given number of objects (with repetitions) that can be selected from the total objects",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/permutationa-function-6c7d7fdc-d657-44e6-aa19-2857b25cae4e",
        ),
        own: "Number of ordered arrangements of k items from n when items may repeat: n to the power k",
    },
    FunctionHelp {
        spelling: "PHI",
        microsoft: Some(
            "Returns the value of the density function for a standard normal distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/phi-function-23e49bc6-a8e8-402d-98d3-9ded87f6295c",
        ),
        own: "Density of the standard normal distribution at z",
    },
    FunctionHelp {
        spelling: "PHONETIC",
        microsoft: Some("Extracts the phonetic (furigana) characters from a text string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/phonetic-function-9a329dac-0c0f-42f8-9a55-639086988554",
        ),
        own: "The phonetic (furigana) reading stored with Japanese text",
    },
    FunctionHelp {
        spelling: "PI",
        microsoft: Some("Returns the value of pi"),
        source: Some(
            "https://support.microsoft.com/en-us/office/pi-function-264199d0-a3ba-46b8-975a-c4a04608989b",
        ),
        own: "The constant pi, 3.14159265358979",
    },
    FunctionHelp {
        spelling: "PIVOTBY",
        microsoft: Some(
            "Helps a user group, aggregate, sort, and filter data based on the row and column fields that you specify",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/pivotby-function-de86516a-90ad-4ced-8522-3a25fac389cf",
        ),
        own: "A cross-tab of values aggregated by row groups and column groups",
    },
    FunctionHelp {
        spelling: "PMT",
        microsoft: Some("Returns the periodic payment for an annuity"),
        source: Some(
            "https://support.microsoft.com/en-us/office/pmt-function-0214da64-9a63-4996-bc20-214433fa6441",
        ),
        own: "Fixed payment per period that pays off a loan at a fixed rate",
    },
    FunctionHelp {
        spelling: "POISSON",
        microsoft: Some("Returns the Poisson distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/poisson-function-d81f7294-9d7c-4f75-bc23-80aa8624173a",
        ),
        own: "Poisson probability (older form of POISSON.DIST)",
    },
    FunctionHelp {
        spelling: "POISSON.DIST",
        microsoft: Some("Returns the Poisson distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/poisson-dist-function-8fe148ff-39a2-46cb-abf3-7772695d9636",
        ),
        own: "Poisson distribution: the chance of a number of events, as a single value or cumulative",
    },
    FunctionHelp {
        spelling: "POWER",
        microsoft: Some("Returns the result of a number raised to a power"),
        source: Some(
            "https://support.microsoft.com/en-us/office/power-function-d3f2908b-56f4-4c3f-895a-07fb519c362a",
        ),
        own: "A number raised to a power",
    },
    FunctionHelp {
        spelling: "PPMT",
        microsoft: Some(
            "Returns the payment on the principal for an investment for a given period",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/ppmt-function-c370d9e3-7749-4ca4-beea-b06c6ac95e1b",
        ),
        own: "Principal part of one payment on a loan or investment",
    },
    FunctionHelp {
        spelling: "PRICE",
        microsoft: Some(
            "Returns the price per $100 face value of a security that pays periodic interest",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/price-function-3ea9deac-8dfa-436f-a7c8-17ea02c21b0a",
        ),
        own: "Price per 100 face value of a security with periodic coupons",
    },
    FunctionHelp {
        spelling: "PRICEDISC",
        microsoft: Some("Returns the price per $100 face value of a discounted security"),
        source: Some(
            "https://support.microsoft.com/en-us/office/pricedisc-function-d06ad7c1-380e-4be7-9fd9-75e3079acfd3",
        ),
        own: "Price, per 100 of face value, of a security sold at a discount",
    },
    FunctionHelp {
        spelling: "PRICEMAT",
        microsoft: Some(
            "Returns the price per $100 face value of a security that pays interest at maturity",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/pricemat-function-52c3b4da-bc7e-476a-989f-a95f675cae77",
        ),
        own: "Price, per 100 of face value, of a security whose interest is all paid at maturity",
    },
    FunctionHelp {
        spelling: "PROB",
        microsoft: Some("Returns the probability that values in a range are between two limits"),
        source: Some(
            "https://support.microsoft.com/en-us/office/prob-function-9ac30561-c81c-4259-8253-34f0a238fc49",
        ),
        own: "Probability that values in a range fall between two limits, from their probabilities",
    },
    FunctionHelp {
        spelling: "PRODUCT",
        microsoft: Some("Multiplies its arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/product-function-8e6b5b24-90ee-4650-aeec-80982a0512ce",
        ),
        own: "Product of the numbers",
    },
    FunctionHelp {
        spelling: "PROPER",
        microsoft: Some("Capitalizes the first letter in each word of a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/proper-function-52a5a283-e8b2-49be-8506-b2887b889f94",
        ),
        own: "Text with the first letter of each word in upper case and the rest in lower case",
    },
    FunctionHelp {
        spelling: "PV",
        microsoft: Some("Returns the present value of an investment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/pv-function-23879d31-0e02-4321-be01-da16e8168cbd",
        ),
        own: "Present value of an investment with fixed periodic payments and a fixed rate",
    },
    FunctionHelp {
        spelling: "QUARTILE",
        microsoft: Some("Returns the quartile of a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/quartile-function-93cf8f62-60cd-4fdb-8a92-8451041e1a2a",
        ),
        own: "A quartile of a data set, inclusive (older form)",
    },
    FunctionHelp {
        spelling: "QUARTILE.EXC",
        microsoft: Some(
            "Returns the quartile of the data set, based on percentile values from 0..1, exclusive",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/quartile-exc-function-5a355b7a-840b-4a01-b0f1-f538c2864cad",
        ),
        own: "A quartile of a data set, from percentiles with 0 and 1 excluded",
    },
    FunctionHelp {
        spelling: "QUARTILE.INC",
        microsoft: Some("Returns the quartile of a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/quartile-inc-function-1bbacc80-5075-42f1-aed6-47d735c4819d",
        ),
        own: "A quartile of a data set, from percentiles with 0 and 1 included",
    },
    FunctionHelp {
        spelling: "QUOTIENT",
        microsoft: Some("Returns the integer portion of a division"),
        source: Some(
            "https://support.microsoft.com/en-us/office/quotient-function-9f7bf099-2a18-4282-8fa4-65290cc99dee",
        ),
        own: "Whole-number part of a division",
    },
    FunctionHelp {
        spelling: "RADIANS",
        microsoft: Some("Converts degrees to radians"),
        source: Some(
            "https://support.microsoft.com/en-us/office/radians-function-ac409508-3d48-45f5-ac02-1497c92de5bf",
        ),
        own: "Angle in degrees expressed in radians",
    },
    FunctionHelp {
        spelling: "RAND",
        microsoft: Some("Returns a random number between 0 and 1"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rand-function-4cbfa695-8869-4788-8d90-021ea9f5be73",
        ),
        own: "A random number from 0 up to but not including 1, new at each recalculation",
    },
    FunctionHelp {
        spelling: "RANDARRAY",
        microsoft: Some(
            "Returns an array of random numbers between 0 and 1. However, you can specify the number of rows and columns to fill, minimum and maximum values, and whether to return whole numbers or decimal values.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/randarray-function-21261e55-3bec-4885-86a6-8b0a47fd4d33",
        ),
        own: "An array of random numbers of a given size, within limits, whole or decimal",
    },
    FunctionHelp {
        spelling: "RANDBETWEEN",
        microsoft: Some("Returns a random number between the numbers you specify"),
        source: Some(
            "https://support.microsoft.com/en-us/office/randbetween-function-4cc7f0d1-87dc-4eb7-987f-a469ab381685",
        ),
        own: "A random whole number between two limits, new at each recalculation",
    },
    FunctionHelp {
        spelling: "RANK",
        microsoft: Some("Returns the rank of a number in a list of numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rank-function-6a2fc49d-1831-4a03-9d8c-c279cf99f723",
        ),
        own: "Rank of a number in a list (older form of RANK.EQ)",
    },
    FunctionHelp {
        spelling: "RANK.AVG",
        microsoft: Some("Returns the rank of a number in a list of numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rank-avg-function-bd406a6f-eb38-4d73-aa8e-6d1c3c72e83a",
        ),
        own: "Rank of a number in a list, with tied values sharing their average rank",
    },
    FunctionHelp {
        spelling: "RANK.EQ",
        microsoft: Some("Returns the rank of a number in a list of numbers"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rank-eq-function-284858ce-8ef6-450e-b662-26245be04a40",
        ),
        own: "Rank of a number in a list, with tied values sharing the top rank",
    },
    FunctionHelp {
        spelling: "RATE",
        microsoft: Some("Returns the interest rate per period of an annuity"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rate-function-9f665657-4a7e-4bb7-a030-83fc59e748ce",
        ),
        own: "Interest rate per period that makes the payments match the present and future values",
    },
    FunctionHelp {
        spelling: "RECEIVED",
        microsoft: Some("Returns the amount received at maturity for a fully invested security"),
        source: Some(
            "https://support.microsoft.com/en-us/office/received-function-7a3f8b93-6611-4f81-8576-828312c9b5e5",
        ),
        own: "Redemption amount of a security bought at a price and held to maturity at a discount rate",
    },
    FunctionHelp {
        spelling: "REDUCE",
        microsoft: Some(
            "Reduces an array to an accumulated value by applying a LAMBDA to each value and returning the total value in the accumulator",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/reduce-function-42e39910-b345-45f3-84b8-0642b568b7cb",
        ),
        own: "A running total built by a LAMBDA over each item of an array, giving the final value",
    },
    FunctionHelp {
        spelling: "REGEXEXTRACT",
        microsoft: Some("Extracts strings within the provided text that matches the pattern"),
        source: Some(
            "https://support.microsoft.com/en-us/office/regexextract-function-4b96c140-9205-4b6e-9fbe-6aa9e783ff57",
        ),
        own: "Text matched by a regular expression",
    },
    FunctionHelp {
        spelling: "REGEXREPLACE",
        microsoft: Some(
            "Replaces strings within the provided text that matches the pattern with replacement",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/regexreplace-function-9c030bb2-5e47-4efc-bad5-4582d7100897",
        ),
        own: "Text with the matches of a regular expression replaced",
    },
    FunctionHelp {
        spelling: "REGEXTEST",
        microsoft: Some("Determines whether any part of text matches the pattern"),
        source: Some(
            "https://support.microsoft.com/en-us/office/regextest-function-7d38200b-5e5c-4196-b4e6-9bff73afbd31",
        ),
        own: "TRUE when a text matches a regular expression",
    },
    FunctionHelp {
        spelling: "REGISTER.ID",
        microsoft: Some(
            "Returns the register ID of the specified dynamic link library (DLL) or code resource that has been previously registered",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/register-id-function-f8f0af0f-fd66-4704-a0f2-87b27b175b50",
        ),
        own: "Identifier of a registered procedure in a code library (DLL)",
    },
    FunctionHelp {
        spelling: "REPLACE",
        microsoft: Some("Replaces characters within text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/replace-function-8d799074-2425-4a8a-84bc-82472868878a",
        ),
        own: "Text with a span of characters replaced by other text",
    },
    FunctionHelp {
        spelling: "REPLACEB",
        microsoft: Some("Replaces characters within text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/replace-function-8d799074-2425-4a8a-84bc-82472868878a",
        ),
        own: "Text with a span of bytes replaced by other text, for double-byte languages",
    },
    FunctionHelp {
        spelling: "REPT",
        microsoft: Some("Repeats text a given number of times"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rept-function-04c4d778-e712-43b4-9c15-d656582bb061",
        ),
        own: "A text repeated a number of times",
    },
    FunctionHelp {
        spelling: "RIGHT",
        microsoft: Some("Returns the rightmost characters from a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/right-function-240267ee-9afa-4639-a02b-f19e1786cf2f",
        ),
        own: "The last characters of a text",
    },
    FunctionHelp {
        spelling: "RIGHTB",
        microsoft: Some("Returns the rightmost characters from a text value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/right-function-240267ee-9afa-4639-a02b-f19e1786cf2f",
        ),
        own: "The last characters of a text, counted in bytes for double-byte languages",
    },
    FunctionHelp {
        spelling: "ROMAN",
        microsoft: Some("Converts an Arabic numeral to Roman, as text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/roman-function-d6b0b99e-de46-4704-a518-b45a0f8b56f5",
        ),
        own: "A number written as a Roman numeral, in one of several styles",
    },
    FunctionHelp {
        spelling: "ROUND",
        microsoft: Some("Rounds a number to a specified number of digits"),
        source: Some(
            "https://support.microsoft.com/en-us/office/round-function-c018c5d8-40fb-4053-90b1-b3e7f61a213c",
        ),
        own: "Number rounded to a number of digits",
    },
    FunctionHelp {
        spelling: "ROUNDDOWN",
        microsoft: Some("Rounds a number down, toward zero"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rounddown-function-2ec94c73-241f-4b01-8c6f-17e6d7968f53",
        ),
        own: "Number rounded toward zero to a number of digits",
    },
    FunctionHelp {
        spelling: "ROUNDUP",
        microsoft: Some("Rounds a number up, away from zero"),
        source: Some(
            "https://support.microsoft.com/en-us/office/roundup-function-f8bc9b23-e795-47db-8703-db171d0c42a7",
        ),
        own: "Number rounded away from zero to a number of digits",
    },
    FunctionHelp {
        spelling: "ROW",
        microsoft: Some("Returns the row number of a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/row-function-3a63b74a-c4d0-4093-b49a-e76eb49a6d8d",
        ),
        own: "Row number of the first cell of a reference (of the formula's cell when left out)",
    },
    FunctionHelp {
        spelling: "ROWS",
        microsoft: Some("Returns the number of rows in a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rows-function-b592593e-3fc2-47f2-bec1-bda493811597",
        ),
        own: "How many rows an array or reference has",
    },
    FunctionHelp {
        spelling: "RRI",
        microsoft: Some("Returns an equivalent interest rate for the growth of an investment"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rri-function-6f5822d8-7ef1-4233-944c-79e8172930f4",
        ),
        own: "Equivalent rate per period for an investment to grow from a present to a future value",
    },
    FunctionHelp {
        spelling: "RSQ",
        microsoft: Some("Returns the square of the Pearson product moment correlation coefficient"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rsq-function-d7161715-250d-4a01-b80d-a8364f2be08f",
        ),
        own: "Square of the Pearson correlation coefficient of two data sets",
    },
    FunctionHelp {
        spelling: "RTD",
        microsoft: Some("Retrieves real-time data from a program that supports COM automation"),
        source: Some(
            "https://support.microsoft.com/en-us/office/rtd-function-e0cc001a-56f0-470a-9b19-9455dc0eb593",
        ),
        own: "Live data from a program that supports real-time data (COM automation)",
    },
    FunctionHelp {
        spelling: "SCAN",
        microsoft: Some(
            "Scans an array by applying a LAMBDA to each value and returns an array that has each intermediate value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/scan-function-d58dfd11-9969-4439-b2dc-e7062724de29",
        ),
        own: "Every intermediate value of a running calculation by a LAMBDA over an array",
    },
    FunctionHelp {
        spelling: "SEARCH",
        microsoft: Some("Finds one text value within another (not case-sensitive)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/search-function-9ab04538-0e55-4719-a72e-b6f54513b495",
        ),
        own: "Position of one text inside another, ignoring letter case and allowing wildcards",
    },
    FunctionHelp {
        spelling: "SEARCHB",
        microsoft: Some("Finds one text value within another (not case-sensitive)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/search-function-9ab04538-0e55-4719-a72e-b6f54513b495",
        ),
        own: "Position of one text inside another, ignoring case, counted in bytes for double-byte languages",
    },
    FunctionHelp {
        spelling: "SEC",
        microsoft: Some("Returns the secant of an angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sec-function-ff224717-9c87-4170-9b58-d069ced6d5f7",
        ),
        own: "Secant of an angle in radians",
    },
    FunctionHelp {
        spelling: "SECH",
        microsoft: Some("Returns the hyperbolic secant of an angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sech-function-e05a789f-5ff7-4d7f-984a-5edb9b09556f",
        ),
        own: "Hyperbolic secant",
    },
    FunctionHelp {
        spelling: "SECOND",
        microsoft: Some("Converts a serial number to a second"),
        source: Some(
            "https://support.microsoft.com/en-us/office/second-function-740d1cfc-553c-4099-b668-80eaa24e8af1",
        ),
        own: "Second (0 to 59) of a time",
    },
    FunctionHelp {
        spelling: "SEQUENCE",
        microsoft: Some("Generates a list of sequential numbers in an array, such as 1, 2, 3, 4"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sequence-function-57467a98-57e0-4817-9f14-2eb78519ca90",
        ),
        own: "An array of evenly spaced numbers of a given size",
    },
    FunctionHelp {
        spelling: "SERIESSUM",
        microsoft: Some("Returns the sum of a power series based on the formula"),
        source: Some(
            "https://support.microsoft.com/en-us/office/seriessum-function-a3ab25b5-1093-4f5b-b084-96c49087f637",
        ),
        own: "Sum of a power series with given coefficients",
    },
    FunctionHelp {
        spelling: "SHEET",
        microsoft: Some("Returns the sheet number of the referenced sheet"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sheet-function-44718b6f-8b87-47a1-a9d6-b701c06cff24",
        ),
        own: "Number of the sheet a reference or name points to",
    },
    FunctionHelp {
        spelling: "SHEETS",
        microsoft: Some("Returns the number of sheets in a reference"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sheets-function-770515eb-e1e8-45ce-8066-b557e5e4b80b",
        ),
        own: "Number of sheets in a reference or in the workbook",
    },
    FunctionHelp {
        spelling: "SIGN",
        microsoft: Some("Returns the sign of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sign-function-109c932d-fcdc-4023-91f1-2dd0e916a1d8",
        ),
        own: "1 for a positive number, 0 for zero, -1 for a negative number",
    },
    FunctionHelp {
        spelling: "SIN",
        microsoft: Some("Returns the sine of the given angle"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sin-function-cf0e3432-8b9e-483c-bc55-a76651c95602",
        ),
        own: "Sine of an angle in radians",
    },
    FunctionHelp {
        spelling: "SINH",
        microsoft: Some("Returns the hyperbolic sine of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sinh-function-1e4e8b9f-2b65-43fc-ab8a-0a37f4081fa7",
        ),
        own: "Hyperbolic sine",
    },
    FunctionHelp {
        spelling: "SKEW",
        microsoft: Some("Returns the skewness of a distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/skew-function-bdf49d86-b1ef-4804-a046-28eaea69c9fa",
        ),
        own: "Sample skewness: how lopsided the data are around their mean",
    },
    FunctionHelp {
        spelling: "SKEW.P",
        microsoft: Some(
            "Returns the skewness of a distribution based on a population: a characterization of the degree of asymmetry of a distribution around its mean",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/skew-p-function-76530a5c-99b9-48a1-8392-26632d542fcb",
        ),
        own: "Population skewness",
    },
    FunctionHelp {
        spelling: "SLN",
        microsoft: Some("Returns the straight-line depreciation of an asset for one period"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sln-function-cdb666e5-c1c6-40a7-806a-e695edc2f1c8",
        ),
        own: "Depreciation per period by the straight-line method",
    },
    FunctionHelp {
        spelling: "SLOPE",
        microsoft: Some("Returns the slope of the linear regression line"),
        source: Some(
            "https://support.microsoft.com/en-us/office/slope-function-11fb8f97-3117-4813-98aa-61d7e01276b9",
        ),
        own: "Slope of the least-squares line through the known points",
    },
    FunctionHelp {
        spelling: "SMALL",
        microsoft: Some("Returns the k-th smallest value in a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/small-function-17da8222-7c82-42b2-961b-14c45384df07",
        ),
        own: "The k-th smallest value",
    },
    FunctionHelp {
        spelling: "SORT",
        microsoft: Some("Sorts the contents of a range or array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sort-function-22f63bd0-ccc8-492f-953d-c20e8e44b86c",
        ),
        own: "The contents of an array in sorted order",
    },
    FunctionHelp {
        spelling: "SORTBY",
        microsoft: Some(
            "Sorts the contents of a range or array based on the values in a corresponding range or array",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/sortby-function-cd2d7a62-1b93-435c-b561-d6a35134f28f",
        ),
        own: "An array sorted by the values of other arrays",
    },
    FunctionHelp {
        spelling: "SQRT",
        microsoft: Some("Returns a positive square root"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sqrt-function-654975c2-05c4-4831-9a24-2c65e4040fdf",
        ),
        own: "Square root of a number that is zero or more",
    },
    FunctionHelp {
        spelling: "SQRTPI",
        microsoft: Some("Returns the square root of (number * pi)"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sqrtpi-function-1fb4e63f-9b51-46d6-ad68-b3e7a8b519b4",
        ),
        own: "Square root of a number times pi",
    },
    FunctionHelp {
        spelling: "STANDARDIZE",
        microsoft: Some("Returns a normalized value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/standardize-function-81d66554-2d54-40ec-ba83-6437108ee775",
        ),
        own: "z-score: distance from the mean in standard deviations",
    },
    FunctionHelp {
        spelling: "STDEV",
        microsoft: Some("Estimates standard deviation based on a sample"),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdev-function-51fecaaa-231e-4bbb-9230-33650a72c9b0",
        ),
        own: "Sample standard deviation (older form)",
    },
    FunctionHelp {
        spelling: "STDEV.P",
        microsoft: Some("Calculates standard deviation based on the entire population"),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdev-p-function-6e917c05-31a0-496f-ade7-4f4e7462f285",
        ),
        own: "Standard deviation of a whole population",
    },
    FunctionHelp {
        spelling: "STDEV.S",
        microsoft: Some("Estimates standard deviation based on a sample"),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdev-s-function-7d69cf97-0c1f-4acf-be27-f3e83904cc23",
        ),
        own: "Standard deviation of a sample",
    },
    FunctionHelp {
        spelling: "STDEVA",
        microsoft: Some(
            "Estimates standard deviation based on a sample, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdeva-function-5ff38888-7ea5-48de-9a6d-11ed73b29e9d",
        ),
        own: "Sample standard deviation counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "STDEVP",
        microsoft: Some("Calculates standard deviation based on the entire population"),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdevp-function-1f7c1c88-1bec-4422-8242-e9f7dc8bb195",
        ),
        own: "Population standard deviation (older form)",
    },
    FunctionHelp {
        spelling: "STDEVPA",
        microsoft: Some(
            "Calculates standard deviation based on the entire population, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/stdevpa-function-5578d4d6-455a-4308-9991-d405afe2c28c",
        ),
        own: "Population standard deviation counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "STEYX",
        microsoft: Some(
            "Returns the standard error of the predicted y-value for each x in the regression",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/steyx-function-6ce74b2c-449d-4a6e-b9ac-f9cef5ba48ab",
        ),
        own: "Standard error of the y values predicted by the least-squares line",
    },
    FunctionHelp {
        spelling: "STOCKHISTORY",
        microsoft: Some("Retrieves historical data about a financial instrument"),
        source: Some(
            "https://support.microsoft.com/en-us/office/stockhistory-function-1ac8b5b3-5f62-4d94-8ab8-7504ec7239a8",
        ),
        own: "Historical prices of a financial instrument over a date range",
    },
    FunctionHelp {
        spelling: "SUBSTITUTE",
        microsoft: Some("Substitutes new text for old text in a text string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/substitute-function-6434944e-a904-4336-a9b0-1e58df3bc332",
        ),
        own: "Text with occurrences of one text replaced by another",
    },
    FunctionHelp {
        spelling: "SUBTOTAL",
        microsoft: Some("Returns a subtotal in a list or database"),
        source: Some(
            "https://support.microsoft.com/en-us/office/subtotal-function-7b027003-f060-4ade-9040-e478765b9939",
        ),
        own: "A summary (sum, count, average and more) of a range that skips nested subtotals",
    },
    FunctionHelp {
        spelling: "SUM",
        microsoft: Some("Use this function to add the values in cells."),
        source: Some(
            "https://support.microsoft.com/en-us/office/sum-function-043e1c7d-7726-4e80-8f32-07b23e057f89",
        ),
        own: "Total of the numbers",
    },
    FunctionHelp {
        spelling: "SUMIF",
        microsoft: Some("Adds the cells specified by a given criteria"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumif-function-169b8c99-c05c-4483-a712-1697a653039b",
        ),
        own: "Total of the cells that meet one condition",
    },
    FunctionHelp {
        spelling: "SUMIFS",
        microsoft: Some(
            "Use this function when you need to add the cells in a range that meet multiple criteria.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumifs-function-c9e748f5-7ea7-455d-9406-611cebce642b",
        ),
        own: "Total of the cells that meet every one of several conditions",
    },
    FunctionHelp {
        spelling: "SUMPRODUCT",
        microsoft: Some("Returns the sum of the products of corresponding array components"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumproduct-function-16753e75-9f68-4874-94ac-4d2145a2fd2e",
        ),
        own: "Sum of the products of matching items of arrays",
    },
    FunctionHelp {
        spelling: "SUMSQ",
        microsoft: Some("Returns the sum of the squares of the arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumsq-function-e3313c02-51cc-4963-aae6-31442d9ec307",
        ),
        own: "Squares of the numbers, added up",
    },
    FunctionHelp {
        spelling: "SUMX2MY2",
        microsoft: Some(
            "Returns the sum of the difference of squares of corresponding values in two arrays",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumx2my2-function-9e599cc5-5399-48e9-a5e0-e37812dfa3e9",
        ),
        own: "Sum of x squared minus y squared over matching items",
    },
    FunctionHelp {
        spelling: "SUMX2PY2",
        microsoft: Some(
            "Returns the sum of the sum of squares of corresponding values in two arrays",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumx2py2-function-826b60b4-0aa2-4e5e-81d2-be704d3d786f",
        ),
        own: "Sum of x squared plus y squared over matching items",
    },
    FunctionHelp {
        spelling: "SUMXMY2",
        microsoft: Some(
            "Returns the sum of squares of differences of corresponding values in two arrays",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/sumxmy2-function-9d144ac1-4d79-43de-b524-e2ecee23b299",
        ),
        own: "Sum of the squared differences of matching items",
    },
    FunctionHelp {
        spelling: "SWITCH",
        microsoft: Some(
            "Evaluates an expression against a list of values and returns the result corresponding to the first matching value. If there is no match, an optional default value may be returned.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/switch-function-47ab33c0-28ce-4530-8a45-d532ec4aa25e",
        ),
        own: "The result paired with the first value that equals the expression, or a default",
    },
    FunctionHelp {
        spelling: "SYD",
        microsoft: Some(
            "Returns the sum-of-years' digits depreciation of an asset for a specified period",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/syd-function-069f8106-b60b-4ca2-98e0-2a0f206bdb27",
        ),
        own: "Sum-of-years'-digits depreciation for one period",
    },
    FunctionHelp {
        spelling: "T",
        microsoft: Some("Converts its arguments to text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-function-fb83aeec-45e7-4924-af95-53e073541228",
        ),
        own: "The value when it is text, otherwise empty text",
    },
    FunctionHelp {
        spelling: "T.DIST",
        microsoft: Some(
            "Returns the Percentage Points (probability) for the Student t-distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-dist-function-4329459f-ae91-48c2-bba8-1ead1c6c21b2",
        ),
        own: "Student's t distribution: the density or the left-tail probability",
    },
    FunctionHelp {
        spelling: "T.DIST.2T",
        microsoft: Some(
            "Returns the Percentage Points (probability) for the Student t-distribution",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-dist-2t-function-198e9340-e360-4230-bd21-f52f22ff5c28",
        ),
        own: "Two-tailed probability of Student's t distribution",
    },
    FunctionHelp {
        spelling: "T.DIST.RT",
        microsoft: Some("Returns the Student's t-distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-dist-rt-function-20a30020-86f9-4b35-af1f-7ef6ae683eda",
        ),
        own: "Right-tail probability of Student's t distribution",
    },
    FunctionHelp {
        spelling: "T.INV",
        microsoft: Some(
            "Returns the t-value of the Student's t-distribution as a function of the probability and the degrees of freedom",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-inv-function-2908272b-4e61-4942-9df9-a25fec9b0e2e",
        ),
        own: "The t at which the left-tail probability reaches a given level",
    },
    FunctionHelp {
        spelling: "T.INV.2T",
        microsoft: Some("Returns the inverse of the Student's t-distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-inv-2t-function-ce72ea19-ec6c-4be7-bed2-b9baf2264f17",
        ),
        own: "The t at which the two-tailed probability falls to a given level",
    },
    FunctionHelp {
        spelling: "T.TEST",
        microsoft: Some("Returns the probability associated with a Student's t-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/t-test-function-d4e08ec3-c545-485f-962e-276f7cbed055",
        ),
        own: "Probability from Student's t-test that two samples have the same mean",
    },
    FunctionHelp {
        spelling: "TAKE",
        microsoft: Some(
            "Returns a specified number of contiguous rows or columns from the start or end of an array",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/take-function-25382ff1-5da1-4f78-ab43-f33bd2e4e003",
        ),
        own: "Rows or columns kept from the start or end of an array",
    },
    FunctionHelp {
        spelling: "TAN",
        microsoft: Some("Returns the tangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tan-function-08851a40-179f-4052-b789-d7f699447401",
        ),
        own: "Tangent of an angle in radians",
    },
    FunctionHelp {
        spelling: "TANH",
        microsoft: Some("Returns the hyperbolic tangent of a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tanh-function-017222f0-a0c3-4f69-9787-b3202295dc6c",
        ),
        own: "Hyperbolic tangent",
    },
    FunctionHelp {
        spelling: "TBILLEQ",
        microsoft: Some("Returns the bond-equivalent yield for a Treasury bill"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tbilleq-function-2ab72d90-9b4d-4efe-9fc2-0f81f2c19c8c",
        ),
        own: "Treasury bill yield restated as a bond-equivalent yield",
    },
    FunctionHelp {
        spelling: "TBILLPRICE",
        microsoft: Some("Returns the price per $100 face value for a Treasury bill"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tbillprice-function-eacca992-c29d-425a-9eb8-0513fe6035a2",
        ),
        own: "Treasury bill price per 100 of face value, from its discount rate",
    },
    FunctionHelp {
        spelling: "TBILLYIELD",
        microsoft: Some("Returns the yield for a Treasury bill"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tbillyield-function-6d381232-f4b0-4cd5-8e97-45b9c03468ba",
        ),
        own: "Treasury bill yield, from its price",
    },
    FunctionHelp {
        spelling: "TDIST",
        microsoft: Some("Returns the Student's t-distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tdist-function-630a7695-4021-4853-9468-4a1f9dcdd192",
        ),
        own: "Student's t probability, one- or two-tailed (older form)",
    },
    FunctionHelp {
        spelling: "TEXT",
        microsoft: Some("Formats a number and converts it to text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/text-function-20d5ac4d-7b94-49fd-bb38-93d29371225c",
        ),
        own: "A value as text, in a number format",
    },
    FunctionHelp {
        spelling: "TEXTAFTER",
        microsoft: Some("Returns text that occurs after given character or string"),
        source: Some(
            "https://support.microsoft.com/en-us/office/textafter-function-c8db2546-5b51-416a-9690-c7e6722e90b4",
        ),
        own: "The part of a text after a delimiter",
    },
    FunctionHelp {
        spelling: "TEXTBEFORE",
        microsoft: Some(
            "Use this function to return text that occurs before a given character or string. You can use TEXTAFTER to return text that occurs after a given character or string.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/textbefore-function-d099c28a-dba8-448e-ac6c-f086d0fa1b29",
        ),
        own: "The part of a text before a delimiter",
    },
    FunctionHelp {
        spelling: "TEXTJOIN",
        microsoft: Some("Text: Combines the text from multiple ranges and/or strings"),
        source: Some(
            "https://support.microsoft.com/en-us/office/textjoin-function-357b449a-ec91-49d0-80c3-0e8fc845691c",
        ),
        own: "Texts joined with a delimiter, optionally skipping empty ones",
    },
    FunctionHelp {
        spelling: "TEXTSPLIT",
        microsoft: Some("Splits text strings by using column and row delimiters"),
        source: Some(
            "https://support.microsoft.com/en-us/office/textsplit-function-b1ca414e-4c21-4ca0-b1b7-bdecace8a6e7",
        ),
        own: "A text split into rows and columns at delimiters",
    },
    FunctionHelp {
        spelling: "TIME",
        microsoft: Some("Returns the serial number of a particular time"),
        source: Some(
            "https://support.microsoft.com/en-us/office/time-function-9a5aff99-8f7d-4611-845e-747d0b8d5457",
        ),
        own: "Time serial (fraction of a day) for an hour, minute and second",
    },
    FunctionHelp {
        spelling: "TIMEVALUE",
        microsoft: Some("Converts a time in the form of text to a serial number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/timevalue-function-0b615c12-33d8-4431-bf3d-f3eb6d186645",
        ),
        own: "Time serial read from a time written as text",
    },
    FunctionHelp {
        spelling: "TINV",
        microsoft: Some("Returns the inverse of the Student's t-distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tinv-function-a7c85b9d-90f5-41fe-9ca5-1cd2f3e1ed7c",
        ),
        own: "Two-tailed inverse of Student's t distribution (older form)",
    },
    FunctionHelp {
        spelling: "TOCOL",
        microsoft: Some("Returns the array in a single column"),
        source: Some(
            "https://support.microsoft.com/en-us/office/tocol-function-22839d9b-0b55-4fc1-b4e6-2761f8f122ed",
        ),
        own: "The items of an array in one column",
    },
    FunctionHelp {
        spelling: "TODAY",
        microsoft: Some("Returns the serial number of today's date"),
        source: Some(
            "https://support.microsoft.com/en-us/office/today-function-5eb3078d-a82c-4736-8930-2f51a028fdd9",
        ),
        own: "Current date as a serial number",
    },
    FunctionHelp {
        spelling: "TOROW",
        microsoft: Some("Returns the array in a single row"),
        source: Some(
            "https://support.microsoft.com/en-us/office/torow-function-b90d0964-a7d9-44b7-816b-ffa5c2fe2289",
        ),
        own: "The items of an array in one row",
    },
    FunctionHelp {
        spelling: "TRANSLATE",
        microsoft: Some("Translates a text from one language to another"),
        source: Some(
            "https://support.microsoft.com/en-us/office/translate-function-d34f71c7-2ffe-409a-9a63-5eb5e91aa3dd",
        ),
        own: "A text translated from one language to another",
    },
    FunctionHelp {
        spelling: "TRANSPOSE",
        microsoft: Some("Returns the transpose of an array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/transpose-function-ed039415-ed8a-4a81-93e9-4b6dfac76027",
        ),
        own: "An array with rows and columns swapped",
    },
    FunctionHelp {
        spelling: "TREND",
        microsoft: Some("Returns values along a linear trend"),
        source: Some(
            "https://support.microsoft.com/en-us/office/trend-function-e2f135f0-8827-4096-9873-9a7cf7b51ef1",
        ),
        own: "Values predicted on the least-squares line fitted to known data",
    },
    FunctionHelp {
        spelling: "TRIM",
        microsoft: Some("Removes spaces from text"),
        source: Some(
            "https://support.microsoft.com/en-us/office/trim-function-410388fa-c5df-49c6-b16c-9e5630b479f9",
        ),
        own: "Text with spaces removed at both ends and runs of spaces inside cut to one",
    },
    FunctionHelp {
        spelling: "TRIMMEAN",
        microsoft: Some("Returns the mean of the interior of a data set"),
        source: Some(
            "https://support.microsoft.com/en-us/office/trimmean-function-d90c9878-a119-4746-88fa-63d988f511d3",
        ),
        own: "Mean of the data after cutting a share of the values from both ends",
    },
    FunctionHelp {
        spelling: "TRIMRANGE",
        microsoft: Some(
            "Scans in from the edges of a range or array until it finds a non-blank cell (or value), it then excludes those blank rows or columns",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/trimrange-function-d7812248-3bc5-4c6b-901c-1afa9564f999",
        ),
        own: "A range with its empty outer rows and columns cut away",
    },
    FunctionHelp {
        spelling: "TRUE",
        microsoft: Some("Returns the logical value TRUE"),
        source: Some(
            "https://support.microsoft.com/en-us/office/true-function-7652c6e3-8987-48d0-97cd-ef223246b3fb",
        ),
        own: "The logical value TRUE",
    },
    FunctionHelp {
        spelling: "TRUNC",
        microsoft: Some("Truncates a number to an integer"),
        source: Some(
            "https://support.microsoft.com/en-us/office/trunc-function-8b86a64c-3127-43db-ba14-aa5ceb292721",
        ),
        own: "Number cut to a number of digits by dropping the rest",
    },
    FunctionHelp {
        spelling: "TTEST",
        microsoft: Some("Returns the probability associated with a Student's t-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ttest-function-1696ffc1-4811-40fd-9d13-a0eaad83c7ae",
        ),
        own: "Student's t-test probability (older form of T.TEST)",
    },
    FunctionHelp {
        spelling: "TYPE",
        microsoft: Some("Returns a number indicating the data type of a value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/type-function-45b4e688-4bc3-48b3-a105-ffa892995899",
        ),
        own: "Number saying what kind of value an argument is",
    },
    FunctionHelp {
        spelling: "UNICHAR",
        microsoft: Some(
            "Returns the Unicode character that is references by the given numeric value",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/unichar-function-ffeb64f5-f131-44c6-b332-5cd72f0659b8",
        ),
        own: "Character with a given Unicode code point",
    },
    FunctionHelp {
        spelling: "UNICODE",
        microsoft: Some(
            "Returns the number (code point) that corresponds to the first character of the text",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/unicode-function-adb74aaa-a2a5-4dde-aff6-966e4e81f16f",
        ),
        own: "Unicode code point of the first character of a text",
    },
    FunctionHelp {
        spelling: "UNIQUE",
        microsoft: Some("Use this function to return a list of unique values in a list or range."),
        source: Some(
            "https://support.microsoft.com/en-us/office/unique-function-c5ab87fd-30a3-4ce9-9d1a-40204fb85e1e",
        ),
        own: "The distinct rows or columns of an array, or those that occur only once",
    },
    FunctionHelp {
        spelling: "UPPER",
        microsoft: Some("Converts text to uppercase"),
        source: Some(
            "https://support.microsoft.com/en-us/office/upper-function-c11f29b3-d1a3-4537-8df6-04d0049963d6",
        ),
        own: "Text in upper case",
    },
    FunctionHelp {
        spelling: "VALUE",
        microsoft: Some("Converts a text argument to a number"),
        source: Some(
            "https://support.microsoft.com/en-us/office/value-function-257d0108-07dc-437d-ae1c-bc2d3953d8c2",
        ),
        own: "Number read from text",
    },
    FunctionHelp {
        spelling: "VALUETOTEXT",
        microsoft: Some("Returns text from any specified value"),
        source: Some(
            "https://support.microsoft.com/en-us/office/valuetotext-function-5fff61a2-301a-4ab2-9ffa-0a5242a08fea",
        ),
        own: "Text for any value, in a concise or a formula-ready form",
    },
    FunctionHelp {
        spelling: "VAR",
        microsoft: Some("Estimates variance based on a sample"),
        source: Some(
            "https://support.microsoft.com/en-us/office/var-function-1f2b7ab2-954d-4e17-ba2c-9e58b15a7da2",
        ),
        own: "Sample variance (older form)",
    },
    FunctionHelp {
        spelling: "VAR.P",
        microsoft: Some("Calculates variance based on the entire population"),
        source: Some(
            "https://support.microsoft.com/en-us/office/var-p-function-73d1285c-108c-4843-ba5d-a51f90656f3a",
        ),
        own: "Variance of a whole population",
    },
    FunctionHelp {
        spelling: "VAR.S",
        microsoft: Some("Estimates variance based on a sample"),
        source: Some(
            "https://support.microsoft.com/en-us/office/var-s-function-913633de-136b-449d-813e-65a00b2b990b",
        ),
        own: "Variance of a sample",
    },
    FunctionHelp {
        spelling: "VARA",
        microsoft: Some(
            "Estimates variance based on a sample, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/vara-function-3de77469-fa3a-47b4-85fd-81758a1e1d07",
        ),
        own: "Sample variance counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "VARP",
        microsoft: Some("Calculates variance based on the entire population"),
        source: Some(
            "https://support.microsoft.com/en-us/office/varp-function-26a541c4-ecee-464d-a731-bd4c575b1a6b",
        ),
        own: "Population variance (older form)",
    },
    FunctionHelp {
        spelling: "VARPA",
        microsoft: Some(
            "Calculates variance based on the entire population, including numbers, text, and logical values",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/varpa-function-59a62635-4e89-4fad-88ac-ce4dc0513b96",
        ),
        own: "Population variance counting text as 0 and logical values as 1 or 0",
    },
    FunctionHelp {
        spelling: "VDB",
        microsoft: Some(
            "Returns the depreciation of an asset for a specified or partial period by using a declining balance method",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/vdb-function-dde4e207-f3fa-488d-91d2-66d55e861d73",
        ),
        own: "Declining-balance depreciation over any span of periods, with an optional switch to straight line",
    },
    FunctionHelp {
        spelling: "VLOOKUP",
        microsoft: Some(
            "Looks in the first column of an array and moves across the row to return the value of a cell",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/vlookup-function-0bbc8083-26fe-4963-8ab8-93a18ad188a1",
        ),
        own: "A value from a given column of a table, in the row found by searching the first column",
    },
    FunctionHelp {
        spelling: "VSTACK",
        microsoft: Some("Appends arrays vertically and in sequence to return a larger array"),
        source: Some(
            "https://support.microsoft.com/en-us/office/vstack-function-a4b86897-be0f-48fc-adca-fcc10d795a9c",
        ),
        own: "Arrays placed one above another into one array",
    },
    FunctionHelp {
        spelling: "WEBSERVICE",
        microsoft: Some("Returns data from a web service"),
        source: Some(
            "https://support.microsoft.com/en-us/office/webservice-function-0546a35a-ecc6-4739-aed7-c0b7ce1562c4",
        ),
        own: "Text fetched from a web service at a URL",
    },
    FunctionHelp {
        spelling: "WEEKDAY",
        microsoft: Some("Converts a serial number to a day of the week"),
        source: Some(
            "https://support.microsoft.com/en-us/office/weekday-function-60e44483-2ed1-439f-8bd0-e404c190949a",
        ),
        own: "Day of the week of a date, as a number",
    },
    FunctionHelp {
        spelling: "WEEKNUM",
        microsoft: Some(
            "Converts a serial number to a number representing where the week falls numerically with a year",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/weeknum-function-e5c43a03-b4ab-426c-b411-b18c13c75340",
        ),
        own: "Week of the year a date falls in",
    },
    FunctionHelp {
        spelling: "WEIBULL",
        microsoft: Some("Returns the Weibull distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/weibull-function-b83dc2c6-260b-4754-bef2-633196f6fdcc",
        ),
        own: "Weibull probability (older form of WEIBULL.DIST)",
    },
    FunctionHelp {
        spelling: "WEIBULL.DIST",
        microsoft: Some("Returns the Weibull distribution"),
        source: Some(
            "https://support.microsoft.com/en-us/office/weibull-dist-function-4e783c39-9325-49be-bbc9-a83ef82b45db",
        ),
        own: "Weibull distribution: the density or the cumulative probability",
    },
    FunctionHelp {
        spelling: "WORKDAY",
        microsoft: Some(
            "Returns the serial number of the date before or after a specified number of workdays",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/workday-function-f764a5b7-05fc-4494-9486-60d494efbf33",
        ),
        own: "Date a number of working days before or after a start date, skipping weekends and holidays",
    },
    FunctionHelp {
        spelling: "WORKDAY.INTL",
        microsoft: Some(
            "Returns the serial number of the date before or after a specified number of workdays using parameters to indicate which and how many days are weekend days",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/workday-intl-function-a378391c-9ba7-4678-8a39-39611a9bf81d",
        ),
        own: "Date a number of working days away, with a chosen weekend and listed holidays",
    },
    FunctionHelp {
        spelling: "WRAPCOLS",
        microsoft: Some(
            "Wraps the provided row or column of values by columns after a specified number of elements",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/wrapcols-function-d038b05a-57b7-4ee0-be94-ded0792511e2",
        ),
        own: "A row or column of values folded into columns of a given length",
    },
    FunctionHelp {
        spelling: "WRAPROWS",
        microsoft: Some(
            "Wraps the provided row or column of values by rows after a specified number of elements",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/wraprows-function-796825f3-975a-4cee-9c84-1bbddf60ade0",
        ),
        own: "A row or column of values folded into rows of a given length",
    },
    FunctionHelp {
        spelling: "XIRR",
        microsoft: Some(
            "Returns the internal rate of return for a schedule of cash flows that is not necessarily periodic",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/xirr-function-de1242ec-6477-445b-b11b-a303ad9adc9d",
        ),
        own: "Rate at which cash flows on given dates have a net present value of zero",
    },
    FunctionHelp {
        spelling: "XLOOKUP",
        microsoft: Some(
            "Use this function when you need to search a range or an array, and return an item corresponding to the first match it finds. If a match doesn't exist, then XLOOKUP can return the closest (approximate) match.",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/xlookup-function-b7fd680e-6d10-43e6-84f9-88eae8bf5929",
        ),
        own: "The item matching a search in one array, taken from another, with a fallback and match options",
    },
    FunctionHelp {
        spelling: "XMATCH",
        microsoft: Some("Returns the relative position of an item in an array or range of cells."),
        source: Some(
            "https://support.microsoft.com/en-us/office/xmatch-function-d966da31-7a6b-4a13-a1c6-5a33ed6a0312",
        ),
        own: "Position of a value in an array, with match and search options",
    },
    FunctionHelp {
        spelling: "XNPV",
        microsoft: Some(
            "Returns the net present value for a schedule of cash flows that is not necessarily periodic",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/xnpv-function-1b42bbf6-370f-4532-a0eb-d67c16b664b7",
        ),
        own: "Net present value of cash flows on given dates",
    },
    FunctionHelp {
        spelling: "XOR",
        microsoft: Some("Returns a logical exclusive OR of all arguments"),
        source: Some(
            "https://support.microsoft.com/en-us/office/xor-function-1548d4c2-5e47-4f77-9a92-0533bba14f37",
        ),
        own: "TRUE when an odd number of the arguments are TRUE",
    },
    FunctionHelp {
        spelling: "YEAR",
        microsoft: Some("Converts a serial number to a year"),
        source: Some(
            "https://support.microsoft.com/en-us/office/year-function-c64f017a-1354-490d-981f-578e8ec8d3b9",
        ),
        own: "Year of a date",
    },
    FunctionHelp {
        spelling: "YEARFRAC",
        microsoft: Some(
            "Returns the year fraction representing the number of whole days between start_date and end_date",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/yearfrac-function-3844141e-c76d-4143-82b6-208454ddc6a8",
        ),
        own: "Fraction of a year between two dates under a day-count basis",
    },
    FunctionHelp {
        spelling: "YIELD",
        microsoft: Some("Returns the yield on a security that pays periodic interest"),
        source: Some(
            "https://support.microsoft.com/en-us/office/yield-function-f5f5ca43-c4bd-434f-8bd2-ed3c9727a4fe",
        ),
        own: "Yield of a security with periodic coupons",
    },
    FunctionHelp {
        spelling: "YIELDDISC",
        microsoft: Some(
            "Returns the annual yield for a discounted security; for example, a Treasury bill",
        ),
        source: Some(
            "https://support.microsoft.com/en-us/office/yielddisc-function-a9dbdbae-7dae-46de-b995-615faffaaed7",
        ),
        own: "Annual yield of a discounted security",
    },
    FunctionHelp {
        spelling: "YIELDMAT",
        microsoft: Some("Returns the annual yield of a security that pays interest at maturity"),
        source: Some(
            "https://support.microsoft.com/en-us/office/yieldmat-function-ba7d1809-0d33-4bcb-96c7-6c56ec62ef6f",
        ),
        own: "Yearly yield of a security whose interest is all paid at maturity",
    },
    FunctionHelp {
        spelling: "Z.TEST",
        microsoft: Some("Returns the one-tailed probability-value of a z-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/z-test-function-d633d5a3-2031-4614-a016-92180ad82bee",
        ),
        own: "One-tailed probability of a z-test for a sample mean",
    },
    FunctionHelp {
        spelling: "ZTEST",
        microsoft: Some("Returns the one-tailed probability-value of a z-test"),
        source: Some(
            "https://support.microsoft.com/en-us/office/ztest-function-8f33be8a-6bd6-4ecc-8e3a-d9a4420c4a6a",
        ),
        own: "One-tailed z-test probability (older form of Z.TEST)",
    },
];

#[cfg(test)]
mod tests {
    use super::{HELP, description, description_of, help_of};
    use crate::{Notation, catalogue};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_table_is_in_byte_order_without_duplicates() {
        for pair in HELP.windows(2) {
            assert!(
                pair[0].spelling.as_bytes() < pair[1].spelling.as_bytes(),
                "{:?}",
                pair
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn named_functions_are_described_and_operators_are_not() {
        let sum = catalogue()
            .resolve_surface(Notation::Call, "SUM")
            .expect("SUM is in the catalogue");
        assert_eq!(
            description(sum),
            Some("Use this function to add the values in cells.")
        );
        let plus = catalogue()
            .resolve_surface(Notation::Infix, "+")
            .expect("+ is in the catalogue");
        assert_eq!(description(plus), None);
        assert_eq!(description_of("NOSUCHFUNCTION"), None);
    }

    /// Microsoft's text is attributed: every Microsoft description has its support article, and
    /// every function has our own line too.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn microsoft_descriptions_carry_their_source() {
        for h in HELP {
            assert!(!h.own.is_empty(), "{}", h.spelling);
            if h.microsoft.is_some() {
                let source = h.source.unwrap_or_default();
                assert!(
                    source.starts_with("https://support.microsoft.com/"),
                    "{}",
                    h.spelling
                );
            }
        }
        let sum = help_of("SUM").unwrap();
        assert_eq!(sum.own, "Total of the numbers");
        assert_eq!(help_of("OP_IMPLICIT_INTERSECTION").unwrap().microsoft, None);
    }
}
