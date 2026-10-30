//! Function help: a one-line description of each named function, as signature help and a
//! function-help panel show it beside [`crate::FunctionCallTarget::signature_help`].
//!
//! The descriptions are DNA Calc's own (dc-96l.5, 2026-10-30): one line per function saying what
//! it computes in Excel, written for this table. They replace one-line summaries taken from
//! Microsoft's support site, which carry no licence to republish; what was replaced, and where it
//! came from, is recorded in `migration/help-text-sources.tsv`. `OP_IMPLICIT_INTERSECTION`'s
//! line was already our own. The table is keyed by
//! the function's spelling and held in byte order, so a lookup is a binary search. A function
//! the table does not describe has no description; the table is help text only and says nothing
//! about whether calc-func implements a function, which is the catalogue's to say.

use crate::{FunctionCallTarget, Notation};

/// The description of a named function, or `None` for an operator and for a function the table
/// does not describe.
pub fn description(target: FunctionCallTarget<'_>) -> Option<&'static str> {
    if target.notation() != Notation::Call {
        return None;
    }
    description_of(target.spelling())
}

/// The description of the function spelled `spelling` (upper case, as the catalogue spells it).
pub fn description_of(spelling: &str) -> Option<&'static str> {
    DESCRIPTIONS
        .binary_search_by(|(name, _)| name.as_bytes().cmp(spelling.as_bytes()))
        .ok()
        .map(|at| DESCRIPTIONS[at].1)
}

/// Each function's spelling and its description, in byte order of the spelling.
const DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "ABS",
        "Absolute value: the number without its sign, its distance from zero",
    ),
    (
        "ACCRINT",
        "Interest accrued on a security with periodic coupons, from issue to settlement",
    ),
    (
        "ACCRINTM",
        "Interest accrued on a security that pays all its interest when it matures",
    ),
    (
        "ACOS",
        "Angle in radians, from 0 to pi, whose cosine is the given number",
    ),
    ("ACOSH", "Inverse of the hyperbolic cosine"),
    (
        "ACOT",
        "Angle in radians, from 0 to pi, whose cotangent is the given number",
    ),
    ("ACOTH", "Inverse of the hyperbolic cotangent"),
    (
        "ADDRESS",
        "Text of a cell reference built from a row and a column number",
    ),
    (
        "AGGREGATE",
        "One of nineteen summaries (sum, average, percentile and more) of a range, optionally skipping errors, hidden rows and nested subtotals",
    ),
    (
        "AMORDEGRC",
        "French accounting depreciation for one period, with a coefficient that depends on the asset's life",
    ),
    (
        "AMORLINC",
        "French accounting straight-line depreciation for one period, prorated in the first period",
    ),
    ("AND", "TRUE only when every argument is TRUE"),
    ("ARABIC", "Number written by a Roman numeral"),
    ("AREAS", "How many separate areas a reference covers"),
    (
        "ARRAYTOTEXT",
        "One text holding all the values of an array or range",
    ),
    (
        "ASC",
        "Text with full-width (double-byte) characters turned into their half-width forms",
    ),
    (
        "ASIN",
        "Angle in radians, from -pi/2 to pi/2, whose sine is the given number",
    ),
    ("ASINH", "Inverse of the hyperbolic sine"),
    (
        "ATAN",
        "Angle in radians, between -pi/2 and pi/2, whose tangent is the given number",
    ),
    (
        "ATAN2",
        "Angle in radians from the positive x-axis to the point (x, y), between -pi and pi",
    ),
    ("ATANH", "Inverse of the hyperbolic tangent"),
    (
        "AVEDEV",
        "Mean of the absolute distances of the values from their mean",
    ),
    ("AVERAGE", "Arithmetic mean of the numbers given"),
    (
        "AVERAGEA",
        "Arithmetic mean that also counts text as 0 and logical values as 1 or 0",
    ),
    ("AVERAGEIF", "Mean of the cells that meet one condition"),
    (
        "AVERAGEIFS",
        "Mean of the cells that meet every one of several conditions",
    ),
    (
        "BAHTTEXT",
        "A number spelled out in Thai words as an amount in baht",
    ),
    (
        "BASE",
        "A number written as text in another base (radix 2 to 36)",
    ),
    (
        "BESSELI",
        "Modified Bessel function of the first kind, I_n(x)",
    ),
    ("BESSELJ", "Bessel function of the first kind, J_n(x)"),
    (
        "BESSELK",
        "Modified Bessel function of the second kind, K_n(x)",
    ),
    ("BESSELY", "Bessel function of the second kind, Y_n(x)"),
    (
        "BETA.DIST",
        "Beta distribution: the density or the cumulative probability, on an optional interval",
    ),
    (
        "BETA.INV",
        "The value at which the cumulative beta distribution reaches a given probability",
    ),
    (
        "BETADIST",
        "Cumulative beta probability (the older form of BETA.DIST)",
    ),
    (
        "BETAINV",
        "Inverse of the cumulative beta distribution (the older form of BETA.INV)",
    ),
    ("BIN2DEC", "Decimal value of a binary number given as text"),
    ("BIN2HEX", "Binary number rewritten in hexadecimal"),
    ("BIN2OCT", "Binary number rewritten in octal"),
    (
        "BINOM.DIST",
        "Binomial distribution: the chance of a number of successes in a number of trials, as a single value or cumulative",
    ),
    (
        "BINOM.DIST.RANGE",
        "Chance that the number of binomial successes falls between two bounds",
    ),
    (
        "BINOM.INV",
        "Fewest successes whose cumulative binomial probability reaches a target",
    ),
    (
        "BINOMDIST",
        "Binomial probability (the older form of BINOM.DIST)",
    ),
    ("BITAND", "Bits set in both of two whole numbers"),
    (
        "BITLSHIFT",
        "A whole number with its bits moved left by a given count",
    ),
    ("BITOR", "Bits set in either of two whole numbers"),
    (
        "BITRSHIFT",
        "A whole number with its bits moved right by a given count",
    ),
    ("BITXOR", "Bits set in exactly one of two whole numbers"),
    (
        "BYCOL",
        "One result per column, from a LAMBDA called on each column",
    ),
    (
        "BYROW",
        "One result per row, from a LAMBDA called on each row",
    ),
    (
        "CALL",
        "Runs a procedure from a code library (DLL) and gives back its result",
    ),
    (
        "CEILING",
        "Number rounded up, away from zero, to a multiple of the significance",
    ),
    (
        "CEILING.MATH",
        "Number rounded up to a multiple of the significance, with a choice of direction for negative numbers",
    ),
    (
        "CEILING.PRECISE",
        "Number rounded up toward plus infinity to a multiple of the significance, whatever its sign",
    ),
    (
        "CELL",
        "A fact about a cell, such as its address, format, contents or width",
    ),
    (
        "CHAR",
        "Character with a given code in the system character set",
    ),
    (
        "CHIDIST",
        "Right-tail probability of the chi-squared distribution (older form)",
    ),
    (
        "CHIINV",
        "Inverse of the right-tail chi-squared probability (older form)",
    ),
    (
        "CHISQ.DIST",
        "Chi-squared distribution: the density or the left-tail probability",
    ),
    (
        "CHISQ.DIST.RT",
        "Chi-squared upper-tail probability, P(X > x)",
    ),
    (
        "CHISQ.INV",
        "The value at which the left-tail chi-squared probability reaches a given level",
    ),
    (
        "CHISQ.INV.RT",
        "The value at which the right-tail chi-squared probability falls to a given level",
    ),
    (
        "CHISQ.TEST",
        "Chi-squared test of independence between observed and expected counts, as a probability",
    ),
    (
        "CHITEST",
        "Chi-squared test of independence (older form of CHISQ.TEST)",
    ),
    (
        "CHOOSE",
        "The value at a given position in a list of arguments",
    ),
    (
        "CHOOSECOLS",
        "Selected columns of an array, in the order asked for",
    ),
    (
        "CHOOSEROWS",
        "Selected rows of an array, in the order asked for",
    ),
    (
        "CLEAN",
        "Text with the non-printing control characters taken out",
    ),
    (
        "CODE",
        "Code of the first character of a text in the system character set",
    ),
    (
        "COLUMN",
        "Column number of the first cell of a reference (of the formula's cell when left out)",
    ),
    ("COLUMNS", "How many columns an array or reference has"),
    (
        "COMBIN",
        "Number of ways to pick k items from n when order does not matter",
    ),
    (
        "COMBINA",
        "Number of ways to pick k items from n when items may repeat and order does not matter",
    ),
    (
        "COMPLEX",
        "A complex number written as text from its real and imaginary parts",
    ),
    (
        "CONCAT",
        "All the texts of the arguments and ranges joined end to end",
    ),
    ("CONCATENATE", "Two or more texts joined end to end"),
    (
        "CONFIDENCE",
        "Half-width of a normal confidence interval for a mean (older form)",
    ),
    (
        "CONFIDENCE.NORM",
        "Half-width of a confidence interval for a mean, from the normal distribution",
    ),
    (
        "CONFIDENCE.T",
        "Half-width of a confidence interval for a mean, from Student's t distribution",
    ),
    (
        "CONVERT",
        "A measurement converted from one unit to another",
    ),
    ("CORREL", "Pearson correlation coefficient of two data sets"),
    ("COS", "Cosine of an angle in radians"),
    ("COSH", "Hyperbolic cosine"),
    ("COT", "Cotangent of an angle in radians"),
    ("COTH", "Hyperbolic cotangent"),
    ("COUNT", "How many of the values are numbers"),
    ("COUNTA", "How many of the values are not empty"),
    ("COUNTBLANK", "How many cells of a range are empty"),
    ("COUNTIF", "How many cells meet one condition"),
    (
        "COUNTIFS",
        "How many cells meet every one of several conditions",
    ),
    (
        "COUPDAYBS",
        "Days from the start of the coupon period to the settlement date",
    ),
    (
        "COUPDAYS",
        "Days in the coupon period that holds the settlement date",
    ),
    (
        "COUPDAYSNC",
        "Days left from settlement until the coupon date that follows it",
    ),
    ("COUPNCD", "Coupon date that follows the settlement date"),
    (
        "COUPNUM",
        "Number of coupons still to be paid between settlement and maturity",
    ),
    (
        "COUPPCD",
        "Last coupon date on or before the settlement date",
    ),
    (
        "COVAR",
        "Population covariance of two data sets (older form)",
    ),
    (
        "COVARIANCE.P",
        "Covariance of two data sets taken as a whole population",
    ),
    (
        "COVARIANCE.S",
        "Covariance of two data sets taken as a sample",
    ),
    (
        "CRITBINOM",
        "Fewest successes whose cumulative binomial probability reaches a target (older form)",
    ),
    ("CSC", "Cosecant of an angle in radians"),
    ("CSCH", "Hyperbolic cosecant"),
    (
        "CUBEKPIMEMBER",
        "A key performance indicator property of an OLAP cube",
    ),
    (
        "CUBEMEMBER",
        "A member or tuple of an OLAP cube, if it exists",
    ),
    (
        "CUBEMEMBERPROPERTY",
        "A property of a member of an OLAP cube",
    ),
    (
        "CUBERANKEDMEMBER",
        "The member at a given rank in a set from an OLAP cube",
    ),
    (
        "CUBESET",
        "A set of members or tuples computed by an OLAP cube server",
    ),
    ("CUBESETCOUNT", "How many items a cube set holds"),
    ("CUBEVALUE", "An aggregated value from an OLAP cube"),
    (
        "CUMIPMT",
        "Total interest paid on a loan between two periods",
    ),
    (
        "CUMPRINC",
        "Principal repaid on a loan from one period to another, added up",
    ),
    ("DATE", "Serial date number for a year, month and day"),
    ("DATEDIF", "Whole days, months or years between two dates"),
    (
        "DATEVALUE",
        "Serial date number read from a date written as text",
    ),
    (
        "DAVERAGE",
        "Mean of a database column over the records that match the criteria",
    ),
    ("DAY", "Day of the month (1 to 31) of a date"),
    ("DAYS", "Number of days from one date to another"),
    (
        "DAYS360",
        "Days between two dates counted with twelve 30-day months",
    ),
    (
        "DB",
        "Fixed-declining-balance depreciation of an asset for one period",
    ),
    (
        "DBCS",
        "Text with half-width (single-byte) characters turned into their full-width forms",
    ),
    (
        "DCOUNT",
        "How many numbers a database column holds in the records that match the criteria",
    ),
    (
        "DCOUNTA",
        "How many non-empty entries a database column holds in the matching records",
    ),
    (
        "DDB",
        "Double-declining-balance (or other factor) depreciation for one period",
    ),
    ("DEC2BIN", "Decimal number written in binary"),
    ("DEC2HEX", "Decimal number written in hexadecimal"),
    ("DEC2OCT", "Decimal number written in octal"),
    (
        "DECIMAL",
        "Value of a number written as text in a given base",
    ),
    ("DEGREES", "Angle in radians expressed in degrees"),
    ("DELTA", "1 when two numbers are equal, otherwise 0"),
    (
        "DETECTLANGUAGE",
        "Code of the language a text is written in",
    ),
    (
        "DEVSQ",
        "Sum of the squared distances of the values from their mean",
    ),
    (
        "DGET",
        "The one value of a database column that matches the criteria",
    ),
    (
        "DISC",
        "Yearly discount rate implied by a security's price and redemption value",
    ),
    (
        "DMAX",
        "Largest value of a database column over the matching records",
    ),
    (
        "DMIN",
        "Smallest value of a database column over the matching records",
    ),
    (
        "DOLLAR",
        "A number as currency text, rounded to a number of decimals",
    ),
    (
        "DOLLARDE",
        "A price quoted as a fraction (such as 1.02 for 1 2/16) as a decimal number",
    ),
    (
        "DOLLARFR",
        "A decimal price written as a whole part and a fraction of a given denominator",
    ),
    (
        "DPRODUCT",
        "Product of a database column over the matching records",
    ),
    (
        "DROP",
        "An array with a number of rows or columns removed from its start or end",
    ),
    (
        "DSTDEV",
        "Sample standard deviation of a database column over the matching records",
    ),
    (
        "DSTDEVP",
        "Population standard deviation of a database column over the matching records",
    ),
    (
        "DSUM",
        "Total of a database column over the matching records",
    ),
    (
        "DURATION",
        "Macaulay duration of a security with periodic coupons, in years",
    ),
    (
        "DVAR",
        "Sample variance of a database column over the matching records",
    ),
    (
        "DVARP",
        "Population variance of a database column over the matching records",
    ),
    (
        "EDATE",
        "Date a number of months before or after a start date",
    ),
    (
        "EFFECT",
        "Effective annual rate from a nominal rate and the compounding periods per year",
    ),
    ("ENCODEURL", "Text made safe for a URL by percent-encoding"),
    (
        "EOMONTH",
        "Last day of the month a number of months before or after a date",
    ),
    (
        "ERF",
        "Error function, integrated between two limits or from 0",
    ),
    ("ERF.PRECISE", "Error function integrated from 0 to x"),
    ("ERFC", "Complementary error function, 1 - ERF(x)"),
    (
        "ERFC.PRECISE",
        "Complementary error function integrated from x to infinity",
    ),
    (
        "ERROR.TYPE",
        "Number identifying which error value a cell holds",
    ),
    (
        "EUROCONVERT",
        "An amount converted between euro member currencies through the euro",
    ),
    (
        "EVEN",
        "Number rounded away from zero to an even whole number",
    ),
    (
        "EXACT",
        "TRUE when two texts are identical, letter case included",
    ),
    ("EXP", "e raised to a power"),
    (
        "EXPAND",
        "An array padded out to a given number of rows and columns",
    ),
    (
        "EXPON.DIST",
        "Exponential distribution: the density or the cumulative probability",
    ),
    (
        "EXPONDIST",
        "Exponential distribution (older form of EXPON.DIST)",
    ),
    (
        "F.DIST",
        "F distribution: the density or the left-tail probability",
    ),
    ("F.DIST.RT", "Right-tail probability of the F distribution"),
    (
        "F.INV",
        "The value at which the left-tail F probability reaches a given level",
    ),
    (
        "F.INV.RT",
        "The value at which the right-tail F probability falls to a given level",
    ),
    (
        "F.TEST",
        "Two-tailed probability that two samples' variances do not differ",
    ),
    ("FACT", "Factorial: the product 1 x 2 x ... x n"),
    (
        "FACTDOUBLE",
        "Double factorial: the product of every second number down from n",
    ),
    ("FALSE", "The logical value FALSE"),
    (
        "FDIST",
        "Right-tail probability of the F distribution (older form)",
    ),
    (
        "FILTER",
        "The rows or columns of an array for which a condition holds",
    ),
    (
        "FILTERXML",
        "Values picked out of XML text by an XPath expression",
    ),
    (
        "FIND",
        "Position of one text inside another, case-sensitive",
    ),
    (
        "FINDB",
        "Position of one text inside another, case-sensitive, counted in bytes for double-byte languages",
    ),
    (
        "FINV",
        "Inverse of the right-tail F probability (older form)",
    ),
    (
        "FISHER",
        "Fisher transformation of a correlation, artanh(x)",
    ),
    ("FISHERINV", "Inverse Fisher transformation, tanh(y)"),
    (
        "FIXED",
        "A number as text with a fixed number of decimals, with or without thousands separators",
    ),
    (
        "FLOOR",
        "Number rounded down, toward zero, to a multiple of the significance",
    ),
    (
        "FLOOR.MATH",
        "Number rounded down to a multiple of the significance, with a choice of direction for negative numbers",
    ),
    (
        "FLOOR.PRECISE",
        "Number rounded down toward minus infinity to a multiple of the significance, whatever its sign",
    ),
    (
        "FORECAST",
        "Value predicted for x on the least-squares line through the known points",
    ),
    (
        "FORECAST.LINEAR",
        "Value predicted for x on the least-squares line through the known points",
    ),
    ("FORMULATEXT", "The formula of a cell, as text"),
    (
        "FREQUENCY",
        "How many values fall into each bin, as a vertical array",
    ),
    (
        "FTEST",
        "Two-tailed F-test probability (older form of F.TEST)",
    ),
    (
        "FV",
        "Future value of an investment with fixed periodic payments and a fixed rate",
    ),
    (
        "FVSCHEDULE",
        "Future value of a principal grown by a series of interest rates",
    ),
    ("GAMMA", "Gamma function"),
    (
        "GAMMA.DIST",
        "Gamma distribution: the density or the cumulative probability",
    ),
    (
        "GAMMA.INV",
        "The value at which the cumulative gamma distribution reaches a given probability",
    ),
    ("GAMMADIST", "Gamma distribution (older form of GAMMA.DIST)"),
    (
        "GAMMAINV",
        "Inverse of the cumulative gamma distribution (older form)",
    ),
    ("GAMMALN", "Natural logarithm of the gamma function"),
    ("GAMMALN.PRECISE", "Natural logarithm of the gamma function"),
    (
        "GAUSS",
        "Probability that a standard normal value lies between 0 and z",
    ),
    ("GCD", "Greatest common divisor of whole numbers"),
    (
        "GEOMEAN",
        "Geometric mean: the n-th root of the product of n positive values",
    ),
    (
        "GESTEP",
        "1 when a number is at least the step value, otherwise 0",
    ),
    ("GETPIVOTDATA", "A value read out of a PivotTable report"),
    (
        "GROUPBY",
        "A summary table with values aggregated by the groups of one or more row fields",
    ),
    (
        "GROWTH",
        "Values predicted on the exponential curve fitted to known data",
    ),
    (
        "HARMEAN",
        "Harmonic mean: n divided by the sum of the reciprocals",
    ),
    ("HEX2BIN", "Hexadecimal number written in binary"),
    (
        "HEX2DEC",
        "Decimal value of a hexadecimal number given as text",
    ),
    ("HEX2OCT", "Hexadecimal number written in octal"),
    (
        "HLOOKUP",
        "A value from a given row of a table, in the column found by searching the top row",
    ),
    ("HOUR", "Hour (0 to 23) of a time"),
    ("HSTACK", "Arrays placed side by side into one array"),
    (
        "HYPERLINK",
        "A clickable link to a document or web address, shown with a label",
    ),
    (
        "HYPGEOM.DIST",
        "Hypergeometric distribution: the chance of k successes in a sample drawn without replacement",
    ),
    ("HYPGEOMDIST", "Hypergeometric probability (older form)"),
    (
        "IF",
        "One of two values, chosen by whether a condition is TRUE",
    ),
    (
        "IFERROR",
        "The value of an expression, or a fallback when it is an error",
    ),
    (
        "IFNA",
        "The value of an expression, or a fallback when it is #N/A",
    ),
    (
        "IFS",
        "The value paired with the first condition that is TRUE",
    ),
    ("IMABS", "Modulus (absolute value) of a complex number"),
    (
        "IMAGE",
        "A picture from a web address shown inside the cell",
    ),
    ("IMAGINARY", "Imaginary part of a complex number"),
    (
        "IMARGUMENT",
        "Argument (angle in radians) of a complex number",
    ),
    ("IMCONJUGATE", "Complex conjugate of a complex number"),
    ("IMCOS", "Cosine of a complex number"),
    ("IMCOSH", "Hyperbolic cosine of a complex number"),
    ("IMCOT", "Cotangent of a complex number"),
    ("IMCSC", "Cosecant of a complex number"),
    ("IMCSCH", "Hyperbolic cosecant of a complex number"),
    ("IMDIV", "Quotient of two complex numbers"),
    ("IMEXP", "e raised to a complex power"),
    ("IMLN", "Natural logarithm of a complex number"),
    ("IMLOG10", "Base-10 logarithm of a complex number"),
    ("IMLOG2", "Base-2 logarithm of a complex number"),
    ("IMPOWER", "A complex number raised to a real power"),
    ("IMPRODUCT", "Product of complex numbers"),
    ("IMREAL", "Real part of a complex number"),
    ("IMSEC", "Secant of a complex number"),
    ("IMSECH", "Hyperbolic secant of a complex number"),
    ("IMSIN", "Sine of a complex number"),
    ("IMSINH", "Hyperbolic sine of a complex number"),
    ("IMSQRT", "Principal square root of a complex number"),
    ("IMSUB", "One complex number minus another"),
    ("IMSUM", "Sum of complex numbers"),
    ("IMTAN", "Tangent of a complex number"),
    (
        "INDEX",
        "The value or reference at a given row and column of an array or range",
    ),
    ("INDIRECT", "The reference that a text names"),
    (
        "INFO",
        "A fact about the environment Excel runs in, such as the version or the system",
    ),
    ("INT", "Number rounded down to a whole number"),
    (
        "INTERCEPT",
        "Where the least-squares line through the known points crosses the y-axis",
    ),
    (
        "INTRATE",
        "Interest rate of a security bought and redeemed at given amounts",
    ),
    (
        "IPMT",
        "Interest part of one payment on a loan or investment",
    ),
    (
        "IRR",
        "Rate at which a series of periodic cash flows has a net present value of zero",
    ),
    ("ISBLANK", "TRUE when the cell is empty"),
    ("ISERR", "TRUE for any error value other than #N/A"),
    ("ISERROR", "TRUE for any error value"),
    ("ISEVEN", "TRUE when the whole part of a number is even"),
    ("ISFORMULA", "TRUE when the cell holds a formula"),
    ("ISLOGICAL", "TRUE when the value is TRUE or FALSE"),
    ("ISNA", "TRUE when the value is #N/A"),
    ("ISNONTEXT", "TRUE when the value is anything but text"),
    ("ISNUMBER", "TRUE when the value is a number"),
    (
        "ISO.CEILING",
        "Number rounded up toward plus infinity to a multiple of the significance (ISO form)",
    ),
    ("ISODD", "TRUE when the whole part of a number is odd"),
    (
        "ISOMITTED",
        "TRUE when a LAMBDA parameter was left out of the call",
    ),
    ("ISOWEEKNUM", "ISO 8601 week number of a date"),
    (
        "ISPMT",
        "Interest paid in one period of a loan repaid in equal principal parts",
    ),
    ("ISREF", "TRUE when the value is a reference"),
    ("ISTEXT", "TRUE when the value is text"),
    (
        "KURT",
        "Sample excess kurtosis: how heavy the tails of the data are",
    ),
    (
        "LAMBDA",
        "A function defined in the formula from parameters and a calculation",
    ),
    ("LARGE", "The k-th largest value"),
    ("LCM", "Least common multiple of whole numbers"),
    ("LEFT", "The first characters of a text"),
    (
        "LEFTB",
        "The first characters of a text, counted in bytes for double-byte languages",
    ),
    ("LEN", "Text length in characters"),
    (
        "LENB",
        "Number of bytes a text takes in double-byte languages",
    ),
    (
        "LET",
        "A calculation that names intermediate results and uses them",
    ),
    (
        "LINEST",
        "Coefficients and statistics of the least-squares linear fit",
    ),
    ("LN", "Natural logarithm"),
    (
        "LOG",
        "Logarithm in a given base (10 when the base is left out)",
    ),
    ("LOG10", "Base-10 logarithm"),
    (
        "LOGEST",
        "Coefficients and statistics of the exponential curve fitted to the data",
    ),
    (
        "LOGINV",
        "Inverse of the cumulative lognormal distribution (older form)",
    ),
    (
        "LOGNORM.DIST",
        "Lognormal distribution: the density or the cumulative probability",
    ),
    (
        "LOGNORM.INV",
        "The value at which the cumulative lognormal distribution reaches a given probability",
    ),
    (
        "LOGNORMDIST",
        "Cumulative lognormal probability (older form)",
    ),
    (
        "LOOKUP",
        "A value found by searching one row or column, taken from the matching place in another",
    ),
    ("LOWER", "Text in lower case"),
    (
        "MAKEARRAY",
        "An array of a given size whose items a LAMBDA computes from row and column",
    ),
    (
        "MAP",
        "An array of the results of a LAMBDA applied to each item of the given arrays",
    ),
    ("MATCH", "Position of a value within a row or column"),
    ("MAX", "Largest of the numbers"),
    (
        "MAXA",
        "Largest value, counting text as 0 and logical values as 1 or 0",
    ),
    (
        "MAXIFS",
        "Largest value among the cells that meet every condition",
    ),
    ("MDETERM", "Determinant of a square matrix"),
    (
        "MDURATION",
        "Modified duration of a security with periodic coupons",
    ),
    ("MEDIAN", "Middle value of the numbers once sorted"),
    (
        "MID",
        "Characters from the middle of a text, from a start position for a given length",
    ),
    (
        "MIDB",
        "Characters from the middle of a text, counted in bytes for double-byte languages",
    ),
    ("MIN", "Smallest of the numbers"),
    (
        "MINA",
        "Smallest value, counting text as 0 and logical values as 1 or 0",
    ),
    (
        "MINIFS",
        "Smallest value among the cells that meet every condition",
    ),
    ("MINUTE", "Minute (0 to 59) of a time"),
    ("MINVERSE", "Inverse of a square matrix"),
    (
        "MIRR",
        "Internal rate of return with separate finance and reinvestment rates",
    ),
    (
        "MMULT",
        "Matrix multiplication: rows of the first array times columns of the second",
    ),
    (
        "MOD",
        "Remainder after division, with the sign of the divisor",
    ),
    ("MODE", "Most frequent number (older form)"),
    (
        "MODE.MULT",
        "All the most frequent numbers, as a vertical array",
    ),
    ("MODE.SNGL", "Most frequent number"),
    ("MONTH", "Month (1 to 12) of a date"),
    (
        "MROUND",
        "Number rounded to the nearest multiple of another",
    ),
    (
        "MULTINOMIAL",
        "Multinomial coefficient: (a + b + ...)! / (a! b! ...)",
    ),
    ("MUNIT", "Identity matrix of a given size"),
    (
        "N",
        "A value as a number: numbers as they are, dates as serials, TRUE as 1, everything else 0",
    ),
    ("NA", "The error value #N/A"),
    (
        "NEGBINOM.DIST",
        "Negative binomial distribution: the chance of f failures before the s-th success",
    ),
    ("NEGBINOMDIST", "Negative binomial probability (older form)"),
    (
        "NETWORKDAYS",
        "Working days between two dates, leaving out weekends and listed holidays",
    ),
    (
        "NETWORKDAYS.INTL",
        "Working days between two dates with a chosen weekend and listed holidays",
    ),
    (
        "NOMINAL",
        "Nominal annual rate from an effective rate and the compounding periods per year",
    ),
    (
        "NORM.DIST",
        "Normal distribution with a given mean and standard deviation: density or cumulative",
    ),
    (
        "NORM.INV",
        "The value at which the cumulative normal distribution reaches a given probability",
    ),
    (
        "NORM.S.DIST",
        "Standard normal distribution: density or cumulative",
    ),
    (
        "NORM.S.INV",
        "The z at which the cumulative standard normal distribution reaches a given probability",
    ),
    ("NORMDIST", "Normal distribution (older form of NORM.DIST)"),
    (
        "NORMINV",
        "Inverse of the cumulative normal distribution (older form)",
    ),
    (
        "NORMSDIST",
        "Cumulative standard normal probability (older form)",
    ),
    (
        "NORMSINV",
        "Inverse of the cumulative standard normal distribution (older form)",
    ),
    ("NOT", "The opposite logical value"),
    ("NOW", "Current date and time as a serial number"),
    (
        "NPER",
        "Number of periods needed to pay off a loan or reach a target value",
    ),
    (
        "NPV",
        "Net present value of periodic cash flows at a discount rate",
    ),
    (
        "NUMBERVALUE",
        "Number read from text with the given decimal and group separators",
    ),
    ("OCT2BIN", "Octal number written in binary"),
    ("OCT2DEC", "Decimal value of an octal number given as text"),
    ("OCT2HEX", "Octal number written in hexadecimal"),
    (
        "ODD",
        "Number rounded away from zero to an odd whole number",
    ),
    (
        "ODDFPRICE",
        "Price per 100 face value of a security whose first period is irregular",
    ),
    (
        "ODDFYIELD",
        "Yield of a security whose first period is irregular",
    ),
    (
        "ODDLPRICE",
        "Price per 100 face value of a security whose last period is irregular",
    ),
    (
        "ODDLYIELD",
        "Yield of a security whose last period is irregular",
    ),
    (
        "OFFSET",
        "A reference shifted by rows and columns from a start, optionally resized",
    ),
    (
        "OP_IMPLICIT_INTERSECTION",
        "Projects the implicit-intersection result for a reference or array in the current formula context.",
    ),
    ("OR", "TRUE when at least one argument is TRUE"),
    (
        "PDURATION",
        "Number of periods an investment needs to grow to a target value at a fixed rate",
    ),
    (
        "PEARSON",
        "Pearson correlation coefficient of two data sets",
    ),
    (
        "PERCENTILE",
        "Value at a given percentile, inclusive of 0 and 1 (older form)",
    ),
    (
        "PERCENTILE.EXC",
        "Value at a given percentile, with 0 and 1 excluded",
    ),
    (
        "PERCENTILE.INC",
        "Value at a given percentile, with 0 and 1 included",
    ),
    ("PERCENTOF", "Share of a subset in a total, as a fraction"),
    (
        "PERCENTRANK",
        "Relative standing of a value in a data set, as a fraction (older form)",
    ),
    (
        "PERCENTRANK.EXC",
        "Relative standing of a value in a data set, with 0 and 1 excluded",
    ),
    (
        "PERCENTRANK.INC",
        "Relative standing of a value in a data set, with 0 and 1 included",
    ),
    (
        "PERMUT",
        "Number of ordered arrangements of k items chosen from n",
    ),
    (
        "PERMUTATIONA",
        "Number of ordered arrangements of k items from n when items may repeat: n to the power k",
    ),
    ("PHI", "Density of the standard normal distribution at z"),
    (
        "PHONETIC",
        "The phonetic (furigana) reading stored with Japanese text",
    ),
    ("PI", "The constant pi, 3.14159265358979"),
    (
        "PIVOTBY",
        "A cross-tab of values aggregated by row groups and column groups",
    ),
    (
        "PMT",
        "Fixed payment per period that pays off a loan at a fixed rate",
    ),
    (
        "POISSON",
        "Poisson probability (older form of POISSON.DIST)",
    ),
    (
        "POISSON.DIST",
        "Poisson distribution: the chance of a number of events, as a single value or cumulative",
    ),
    ("POWER", "A number raised to a power"),
    (
        "PPMT",
        "Principal part of one payment on a loan or investment",
    ),
    (
        "PRICE",
        "Price per 100 face value of a security with periodic coupons",
    ),
    (
        "PRICEDISC",
        "Price, per 100 of face value, of a security sold at a discount",
    ),
    (
        "PRICEMAT",
        "Price, per 100 of face value, of a security whose interest is all paid at maturity",
    ),
    (
        "PROB",
        "Probability that values in a range fall between two limits, from their probabilities",
    ),
    ("PRODUCT", "Product of the numbers"),
    (
        "PROPER",
        "Text with the first letter of each word in upper case and the rest in lower case",
    ),
    (
        "PV",
        "Present value of an investment with fixed periodic payments and a fixed rate",
    ),
    (
        "QUARTILE",
        "A quartile of a data set, inclusive (older form)",
    ),
    (
        "QUARTILE.EXC",
        "A quartile of a data set, from percentiles with 0 and 1 excluded",
    ),
    (
        "QUARTILE.INC",
        "A quartile of a data set, from percentiles with 0 and 1 included",
    ),
    ("QUOTIENT", "Whole-number part of a division"),
    ("RADIANS", "Angle in degrees expressed in radians"),
    (
        "RAND",
        "A random number from 0 up to but not including 1, new at each recalculation",
    ),
    (
        "RANDARRAY",
        "An array of random numbers of a given size, within limits, whole or decimal",
    ),
    (
        "RANDBETWEEN",
        "A random whole number between two limits, new at each recalculation",
    ),
    ("RANK", "Rank of a number in a list (older form of RANK.EQ)"),
    (
        "RANK.AVG",
        "Rank of a number in a list, with tied values sharing their average rank",
    ),
    (
        "RANK.EQ",
        "Rank of a number in a list, with tied values sharing the top rank",
    ),
    (
        "RATE",
        "Interest rate per period that makes the payments match the present and future values",
    ),
    (
        "RECEIVED",
        "Redemption amount of a security bought at a price and held to maturity at a discount rate",
    ),
    (
        "REDUCE",
        "A running total built by a LAMBDA over each item of an array, giving the final value",
    ),
    ("REGEXEXTRACT", "Text matched by a regular expression"),
    (
        "REGEXREPLACE",
        "Text with the matches of a regular expression replaced",
    ),
    ("REGEXTEST", "TRUE when a text matches a regular expression"),
    (
        "REGISTER.ID",
        "Identifier of a registered procedure in a code library (DLL)",
    ),
    (
        "REPLACE",
        "Text with a span of characters replaced by other text",
    ),
    (
        "REPLACEB",
        "Text with a span of bytes replaced by other text, for double-byte languages",
    ),
    ("REPT", "A text repeated a number of times"),
    ("RIGHT", "The last characters of a text"),
    (
        "RIGHTB",
        "The last characters of a text, counted in bytes for double-byte languages",
    ),
    (
        "ROMAN",
        "A number written as a Roman numeral, in one of several styles",
    ),
    ("ROUND", "Number rounded to a number of digits"),
    (
        "ROUNDDOWN",
        "Number rounded toward zero to a number of digits",
    ),
    (
        "ROUNDUP",
        "Number rounded away from zero to a number of digits",
    ),
    (
        "ROW",
        "Row number of the first cell of a reference (of the formula's cell when left out)",
    ),
    ("ROWS", "How many rows an array or reference has"),
    (
        "RRI",
        "Equivalent rate per period for an investment to grow from a present to a future value",
    ),
    (
        "RSQ",
        "Square of the Pearson correlation coefficient of two data sets",
    ),
    (
        "RTD",
        "Live data from a program that supports real-time data (COM automation)",
    ),
    (
        "SCAN",
        "Every intermediate value of a running calculation by a LAMBDA over an array",
    ),
    (
        "SEARCH",
        "Position of one text inside another, ignoring letter case and allowing wildcards",
    ),
    (
        "SEARCHB",
        "Position of one text inside another, ignoring case, counted in bytes for double-byte languages",
    ),
    ("SEC", "Secant of an angle in radians"),
    ("SECH", "Hyperbolic secant"),
    ("SECOND", "Second (0 to 59) of a time"),
    (
        "SEQUENCE",
        "An array of evenly spaced numbers of a given size",
    ),
    ("SERIESSUM", "Sum of a power series with given coefficients"),
    ("SHEET", "Number of the sheet a reference or name points to"),
    (
        "SHEETS",
        "Number of sheets in a reference or in the workbook",
    ),
    (
        "SIGN",
        "1 for a positive number, 0 for zero, -1 for a negative number",
    ),
    ("SIN", "Sine of an angle in radians"),
    ("SINH", "Hyperbolic sine"),
    (
        "SKEW",
        "Sample skewness: how lopsided the data are around their mean",
    ),
    ("SKEW.P", "Population skewness"),
    ("SLN", "Depreciation per period by the straight-line method"),
    (
        "SLOPE",
        "Slope of the least-squares line through the known points",
    ),
    ("SMALL", "The k-th smallest value"),
    ("SORT", "The contents of an array in sorted order"),
    ("SORTBY", "An array sorted by the values of other arrays"),
    ("SQRT", "Square root of a number that is zero or more"),
    ("SQRTPI", "Square root of a number times pi"),
    (
        "STANDARDIZE",
        "z-score: distance from the mean in standard deviations",
    ),
    ("STDEV", "Sample standard deviation (older form)"),
    ("STDEV.P", "Standard deviation of a whole population"),
    ("STDEV.S", "Standard deviation of a sample"),
    (
        "STDEVA",
        "Sample standard deviation counting text as 0 and logical values as 1 or 0",
    ),
    ("STDEVP", "Population standard deviation (older form)"),
    (
        "STDEVPA",
        "Population standard deviation counting text as 0 and logical values as 1 or 0",
    ),
    (
        "STEYX",
        "Standard error of the y values predicted by the least-squares line",
    ),
    (
        "STOCKHISTORY",
        "Historical prices of a financial instrument over a date range",
    ),
    (
        "SUBSTITUTE",
        "Text with occurrences of one text replaced by another",
    ),
    (
        "SUBTOTAL",
        "A summary (sum, count, average and more) of a range that skips nested subtotals",
    ),
    ("SUM", "Total of the numbers"),
    ("SUMIF", "Total of the cells that meet one condition"),
    (
        "SUMIFS",
        "Total of the cells that meet every one of several conditions",
    ),
    (
        "SUMPRODUCT",
        "Sum of the products of matching items of arrays",
    ),
    ("SUMSQ", "Squares of the numbers, added up"),
    (
        "SUMX2MY2",
        "Sum of x squared minus y squared over matching items",
    ),
    (
        "SUMX2PY2",
        "Sum of x squared plus y squared over matching items",
    ),
    (
        "SUMXMY2",
        "Sum of the squared differences of matching items",
    ),
    (
        "SWITCH",
        "The result paired with the first value that equals the expression, or a default",
    ),
    ("SYD", "Sum-of-years'-digits depreciation for one period"),
    ("T", "The value when it is text, otherwise empty text"),
    (
        "T.DIST",
        "Student's t distribution: the density or the left-tail probability",
    ),
    (
        "T.DIST.2T",
        "Two-tailed probability of Student's t distribution",
    ),
    (
        "T.DIST.RT",
        "Right-tail probability of Student's t distribution",
    ),
    (
        "T.INV",
        "The t at which the left-tail probability reaches a given level",
    ),
    (
        "T.INV.2T",
        "The t at which the two-tailed probability falls to a given level",
    ),
    (
        "T.TEST",
        "Probability from Student's t-test that two samples have the same mean",
    ),
    (
        "TAKE",
        "Rows or columns kept from the start or end of an array",
    ),
    ("TAN", "Tangent of an angle in radians"),
    ("TANH", "Hyperbolic tangent"),
    (
        "TBILLEQ",
        "Treasury bill yield restated as a bond-equivalent yield",
    ),
    (
        "TBILLPRICE",
        "Treasury bill price per 100 of face value, from its discount rate",
    ),
    ("TBILLYIELD", "Treasury bill yield, from its price"),
    (
        "TDIST",
        "Student's t probability, one- or two-tailed (older form)",
    ),
    ("TEXT", "A value as text, in a number format"),
    ("TEXTAFTER", "The part of a text after a delimiter"),
    ("TEXTBEFORE", "The part of a text before a delimiter"),
    (
        "TEXTJOIN",
        "Texts joined with a delimiter, optionally skipping empty ones",
    ),
    (
        "TEXTSPLIT",
        "A text split into rows and columns at delimiters",
    ),
    (
        "TIME",
        "Time serial (fraction of a day) for an hour, minute and second",
    ),
    ("TIMEVALUE", "Time serial read from a time written as text"),
    (
        "TINV",
        "Two-tailed inverse of Student's t distribution (older form)",
    ),
    ("TOCOL", "The items of an array in one column"),
    ("TODAY", "Current date as a serial number"),
    ("TOROW", "The items of an array in one row"),
    (
        "TRANSLATE",
        "A text translated from one language to another",
    ),
    ("TRANSPOSE", "An array with rows and columns swapped"),
    (
        "TREND",
        "Values predicted on the least-squares line fitted to known data",
    ),
    (
        "TRIM",
        "Text with spaces removed at both ends and runs of spaces inside cut to one",
    ),
    (
        "TRIMMEAN",
        "Mean of the data after cutting a share of the values from both ends",
    ),
    (
        "TRIMRANGE",
        "A range with its empty outer rows and columns cut away",
    ),
    ("TRUE", "The logical value TRUE"),
    (
        "TRUNC",
        "Number cut to a number of digits by dropping the rest",
    ),
    (
        "TTEST",
        "Student's t-test probability (older form of T.TEST)",
    ),
    ("TYPE", "Number saying what kind of value an argument is"),
    ("UNICHAR", "Character with a given Unicode code point"),
    (
        "UNICODE",
        "Unicode code point of the first character of a text",
    ),
    (
        "UNIQUE",
        "The distinct rows or columns of an array, or those that occur only once",
    ),
    ("UPPER", "Text in upper case"),
    ("VALUE", "Number read from text"),
    (
        "VALUETOTEXT",
        "Text for any value, in a concise or a formula-ready form",
    ),
    ("VAR", "Sample variance (older form)"),
    ("VAR.P", "Variance of a whole population"),
    ("VAR.S", "Variance of a sample"),
    (
        "VARA",
        "Sample variance counting text as 0 and logical values as 1 or 0",
    ),
    ("VARP", "Population variance (older form)"),
    (
        "VARPA",
        "Population variance counting text as 0 and logical values as 1 or 0",
    ),
    (
        "VDB",
        "Declining-balance depreciation over any span of periods, with an optional switch to straight line",
    ),
    (
        "VLOOKUP",
        "A value from a given column of a table, in the row found by searching the first column",
    ),
    ("VSTACK", "Arrays placed one above another into one array"),
    ("WEBSERVICE", "Text fetched from a web service at a URL"),
    ("WEEKDAY", "Day of the week of a date, as a number"),
    ("WEEKNUM", "Week of the year a date falls in"),
    (
        "WEIBULL",
        "Weibull probability (older form of WEIBULL.DIST)",
    ),
    (
        "WEIBULL.DIST",
        "Weibull distribution: the density or the cumulative probability",
    ),
    (
        "WORKDAY",
        "Date a number of working days before or after a start date, skipping weekends and holidays",
    ),
    (
        "WORKDAY.INTL",
        "Date a number of working days away, with a chosen weekend and listed holidays",
    ),
    (
        "WRAPCOLS",
        "A row or column of values folded into columns of a given length",
    ),
    (
        "WRAPROWS",
        "A row or column of values folded into rows of a given length",
    ),
    (
        "XIRR",
        "Rate at which cash flows on given dates have a net present value of zero",
    ),
    (
        "XLOOKUP",
        "The item matching a search in one array, taken from another, with a fallback and match options",
    ),
    (
        "XMATCH",
        "Position of a value in an array, with match and search options",
    ),
    ("XNPV", "Net present value of cash flows on given dates"),
    ("XOR", "TRUE when an odd number of the arguments are TRUE"),
    ("YEAR", "Year of a date"),
    (
        "YEARFRAC",
        "Fraction of a year between two dates under a day-count basis",
    ),
    ("YIELD", "Yield of a security with periodic coupons"),
    ("YIELDDISC", "Annual yield of a discounted security"),
    (
        "YIELDMAT",
        "Yearly yield of a security whose interest is all paid at maturity",
    ),
    (
        "Z.TEST",
        "One-tailed probability of a z-test for a sample mean",
    ),
    (
        "ZTEST",
        "One-tailed z-test probability (older form of Z.TEST)",
    ),
];

#[cfg(test)]
mod tests {
    use super::{DESCRIPTIONS, description, description_of};
    use crate::{Notation, catalogue};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_table_is_in_byte_order_without_duplicates() {
        for pair in DESCRIPTIONS.windows(2) {
            assert!(pair[0].0.as_bytes() < pair[1].0.as_bytes(), "{:?}", pair);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn named_functions_are_described_and_operators_are_not() {
        let sum = catalogue()
            .resolve_surface(Notation::Call, "SUM")
            .expect("SUM is in the catalogue");
        assert_eq!(description(sum), Some("Total of the numbers"));
        let plus = catalogue()
            .resolve_surface(Notation::Infix, "+")
            .expect("+ is in the catalogue");
        assert_eq!(description(plus), None);
        assert_eq!(description_of("NOSUCHFUNCTION"), None);
    }
}
