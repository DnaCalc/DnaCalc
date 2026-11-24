//! Catalogue-driven per-function domain specs and strategic region generators (`dc-ztvv.21`).
//!
//! Every function in `calc-catalogue` gets automatic domain specs (arity, parameter classes,
//! name-inferred semantic kinds, and 6 universal strategic regions). Functions with richer
//! mathematical structure (`RATE`, `IRR`, `YIELD`, and any future plug-in) add targeted strategic
//! regions through [`function_hints`].

use super::rng::{Rng, det_pow, hex_f64, is_value2_exact, pow_u64, pow2_f64, step_ulps};
use calc_catalogue::{DeclarationRef, ParameterClass};
use serde_json::{Value, json};

/// Semantic kind of a parameter, inferred from catalogue metadata or overridden by a plug-in hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    RealGeneral,
    Periods,
    PaymentOrAmount,
    Rate,
    Guess,
    Timing,
    Frequency,
    Basis,
    DateSerial,
    Probability,
    IntegerSmall,
    ArrayNumeric,
    CellRange,
}

/// One parameter specification derived from `calc-catalogue`.
#[derive(Clone, Debug)]
pub struct ParamSpec {
    pub name: String,
    #[allow(dead_code)]
    pub class: ParameterClass,
    pub kind: ParamKind,
    pub optional: bool,
}

/// One strategic region in a function's input space.
#[derive(Clone, Debug)]
pub struct RegionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub base_weight: f64,
    pub kind: RegionKind,
}

/// Generator selector for a region (universal generic regions + plug-in function regions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    // Universal regions (apply to EVERY catalogue function)
    GenericSpecialValues,
    GenericSignPatterns,
    GenericLogUniform,
    GenericNearIntegers,
    GenericArityAndOptional,
    GenericTypeCoercionEdges,
    // Catalogue-generated & hint-file regions (dc-ztvv.26.1)
    SpecGenericDomain,
    SpecLiftingAndShapes,
    SpecHintAnchorsAndEdges,
    SpecHintRangesAndRelations,
    SpecHintErrorEdges,
    // RATE pilot regions
    RateOrdinaryLoans,
    RateFractionalAndNegNper,
    RateFvNegPvAndZeroTerms,
    RateNearFlatAndRegimes,
    RateUnderflowAndOverflow,
    RateConvergenceBudget,
    RateConstructedRoots,
    // IRR regions (generalization)
    IrrTwoFlowExact,
    IrrMultiFlowOrdinary,
    IrrMultiSignAndRetries,
    IrrScaleAndBudget,
    IrrFlatAndEdges,
    // YIELD regions (generalization)
    YieldSinglePeriod,
    YieldMultiPeriodOrdinary,
    YieldExtremePriceAndCoupon,
    YieldDateAndDomainEdges,
    YieldNearParAndSeed,
}

/// Complete domain specification for fuzzing one worksheet function.
#[derive(Clone, Debug)]
pub struct DomainSpec {
    pub function: String,
    pub min_arity: usize,
    pub max_arity: usize,
    pub params: Vec<ParamSpec>,
    pub regions: Vec<RegionSpec>,
    pub has_custom_hints: bool,
    pub has_hint_file: bool,
    pub merged: super::spec_gen::MergedSpec,
}

/// Special numeric anchors shared across all functions (all Value2-exact: finite, non-subnormal,
/// no `-0.0` so Touchstone echoes them cleanly without spurious unread rejections).
pub const SPECIAL_NUMBERS: &[f64] = &[
    0.0,
    1.0,
    -1.0,
    2.0,
    -2.0,
    0.5,
    -0.5,
    0.1,
    -0.1,
    0.01,
    -0.01,
    0.001,
    -0.001,
    10.0,
    -10.0,
    100.0,
    -100.0,
    360.0,
    -360.0,
    1000.0,
    -1000.0,
    1e-7,
    -1e-7,
    1e-4,
    -1e-4,
    1e-10,
    -1e-10,
    1e-12,
    -1e-12,
    1e-15,
    -1e-15,
    1e-100,
    -1e-100,
    1e-307,
    -1e-307,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    1e10,
    -1e10,
    1e100,
    -1e100,
    1e300,
    -1e300,
    f64::MAX,
    -f64::MAX,
    9_007_199_254_740_992.0,  // 2^53
    -9_007_199_254_740_992.0, // -2^53
    9_007_199_254_740_991.0,  // 2^53 - 1
    4_503_599_627_370_496.0,  // 2^52
    536_870_912.0,            // 2^29
    549_755_813_888.0,        // 2^39
];

const DECIMAL_LITERALS: &[f64] = &[
    0.0005, 0.005, 0.025, 0.05, 0.075, 0.1, 0.125, 0.25, 0.5, 0.75, 0.99, 1.0, 1.5, 2.0, 3.0, 4.0,
    5.0, 6.0, 10.0, 12.0, 24.0, 30.0, 60.0, 99.5, 100.0, 101.25, 120.0, 360.0, 599.55, 1000.0,
    10000.0, 100000.0,
];

const COERCION_TEXTS: &[&str] = &[
    "0", "1", "-1", "0.1", "0.05", "-120", "1000", "1e-7", "1e10", "hello", "", "TRUE", "FALSE",
];

const ERROR_VARIANTS: &[&str] = &["Num", "Div0", "Value", "NA", "Ref", "Name", "Null"];

fn infer_param_kind(
    name: &str,
    class: ParameterClass,
    merged_arg: Option<&super::spec_gen::GenericArgSpec>,
) -> ParamKind {
    if merged_arg.is_some_and(|a| a.is_reference_only()) || class == ParameterClass::Reference {
        return ParamKind::CellRange;
    }
    if let Some(a) = merged_arg {
        match a.kind {
            super::hints::HintArgKind::Reference => return ParamKind::CellRange,
            super::hints::HintArgKind::Array => return ParamKind::ArrayNumeric,
            super::hints::HintArgKind::Criteria
            | super::hints::HintArgKind::Logical
            | super::hints::HintArgKind::Text => return ParamKind::RealGeneral,
            _ => {}
        }
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("values")
        || lower.contains("array")
        || lower.contains("schedule")
        || (lower.contains("range") && !lower.contains("lookup"))
    {
        return ParamKind::ArrayNumeric;
    }
    if lower == "guess" {
        return ParamKind::Guess;
    }
    if lower == "type" {
        return ParamKind::Timing;
    }
    if lower == "frequency" {
        return ParamKind::Frequency;
    }
    if lower == "basis" {
        return ParamKind::Basis;
    }
    if lower.contains("date")
        || lower.contains("settlement")
        || lower.contains("maturity")
        || lower.contains("issue")
        || lower.contains("first_interest")
        || lower.contains("last_interest")
        || lower.contains("first_coupon")
    {
        return ParamKind::DateSerial;
    }
    if lower == "nper" || lower == "per" || lower.contains("period") || lower == "life" || lower == "npery" {
        return ParamKind::Periods;
    }
    if lower.contains("rate") || lower == "yld" || lower == "discount" || lower == "coupon" {
        return ParamKind::Rate;
    }
    if lower.contains("prob") || lower == "alpha" || lower == "significance" {
        return ParamKind::Probability;
    }
    if matches!(
        lower.as_str(),
        "pmt" | "pv" | "fv" | "pr" | "redemption" | "par" | "cost" | "salvage" | "principal" | "investment"
    ) {
        return ParamKind::PaymentOrAmount;
    }
    if lower.contains("num") || lower.contains("count") || lower.contains("digits") || lower.contains("tails") || lower == "month" {
        return ParamKind::IntegerSmall;
    }
    ParamKind::RealGeneral
}

fn universal_regions(has_custom: bool) -> Vec<RegionSpec> {
    // Keep universal coverage active even for hinted functions, scaled appropriately.
    let scale = if has_custom { 0.45 } else { 1.0 };
    vec![
        RegionSpec {
            name: "special_values",
            description: "Special anchors: 0, +-1, +-2^k, tiny/huge normals, +-DBL_MAX, +-2^53, typed decimals",
            base_weight: 1.0 * scale,
            kind: RegionKind::GenericSpecialValues,
        },
        RegionSpec {
            name: "sign_patterns",
            description: "Systematic sign/zero combinations across numeric parameters",
            base_weight: 0.9 * scale,
            kind: RegionKind::GenericSignPatterns,
        },
        RegionSpec {
            name: "log_uniform",
            description: "Log-uniform magnitudes spanning 2^-1020..2^1020",
            base_weight: 1.0 * scale,
            kind: RegionKind::GenericLogUniform,
        },
        RegionSpec {
            name: "near_integers",
            description: "Values within +-2^-m and +-1..4 ULP of integers",
            base_weight: 0.7 * scale,
            kind: RegionKind::GenericNearIntegers,
        },
        RegionSpec {
            name: "arity_and_optional",
            description: "All valid arities min..=max with omitted and explicit optional parameters",
            base_weight: 0.6 * scale,
            kind: RegionKind::GenericArityAndOptional,
        },
        RegionSpec {
            name: "type_coercion_edges",
            description: "Numeric/non-numeric text, booleans, blanks, and error propagation",
            base_weight: 0.35 * scale,
            kind: RegionKind::GenericTypeCoercionEdges,
        },
    ]
}

fn custom_regions(function: &str) -> Vec<RegionSpec> {
    match function {
        "RATE" => vec![
            RegionSpec {
                name: "rate_ordinary_loans",
                description: "Standard positive integer nper loans/annuities, type 0/1, missing and shifted guesses",
                base_weight: 1.2,
                kind: RegionKind::RateOrdinaryLoans,
            },
            RegionSpec {
                name: "rate_fractional_and_neg_nper",
                description: "Fractional positive nper and negative integer/fractional/extreme nper, type 0/1",
                base_weight: 1.8,
                kind: RegionKind::RateFractionalAndNegNper,
            },
            RegionSpec {
                name: "rate_fv_neg_pv_and_zero_terms",
                description: "fv = -pv (+-ULPs), pmt = 0, pv = 0, fv = 0, and near-zero balance cancellation",
                base_weight: 1.5,
                kind: RegionKind::RateFvNegPvAndZeroTerms,
            },
            RegionSpec {
                name: "rate_near_flat_and_regimes",
                description: "Near-flat balances, current==before ties, Regime II/III/IV huge roots (2^29..2^46), near -1",
                base_weight: 2.2,
                kind: RegionKind::RateNearFlatAndRegimes,
            },
            RegionSpec {
                name: "rate_underflow_and_overflow",
                description: "Tiny normal cashflows (1e-308..1e-290), huge cashflows, negative-period beginning-payment overflow",
                base_weight: 2.0,
                kind: RegionKind::RateUnderflowAndOverflow,
            },
            RegionSpec {
                name: "rate_convergence_budget",
                description: "High-iteration secant trajectories near 200-step cap and 1e-7 vs 1e-4 acceptance frontiers",
                base_weight: 1.8,
                kind: RegionKind::RateConvergenceBudget,
            },
            RegionSpec {
                name: "rate_constructed_roots",
                description: "Inverse-constructed exact roots across (-1+1e-8..1e14) with +-ULP perturbations and varied guesses",
                base_weight: 2.0,
                kind: RegionKind::RateConstructedRoots,
            },
        ],
        "IRR" => vec![
            RegionSpec {
                name: "irr_two_flow_exact",
                description: "Two-flow [-a, b] exact analytical roots across magnitudes and guess near -1 and 2^53",
                base_weight: 1.6,
                kind: RegionKind::IrrTwoFlowExact,
            },
            RegionSpec {
                name: "irr_multi_flow_ordinary",
                description: "3..12 cashflows with single sign change, missing and explicit guesses",
                base_weight: 1.4,
                kind: RegionKind::IrrMultiFlowOrdinary,
            },
            RegionSpec {
                name: "irr_multi_sign_and_retries",
                description: "Multiple sign changes triggering retry ladder (0.1, -0.1, 2/3) and negative 1-u crossings",
                base_weight: 2.0,
                kind: RegionKind::IrrMultiSignAndRetries,
            },
            RegionSpec {
                name: "irr_scale_and_budget",
                description: "Scaled cashflows (2^-1000..2^1000, 2^15..2^30 post-budget 1e-4, tiny DAZ/FTZ)",
                base_weight: 1.8,
                kind: RegionKind::IrrScaleAndBudget,
            },
            RegionSpec {
                name: "irr_flat_and_edges",
                description: "Huge roots u~1, flat secant stagnation, leading/trailing zeros, all-same-sign #NUM!",
                base_weight: 1.6,
                kind: RegionKind::IrrFlatAndEdges,
            },
        ],
        "YIELD" => vec![
            RegionSpec {
                name: "yield_single_period",
                description: "Single coupon period (n==1) across bases 0..4, freq 1/2/4, 30/360 month-end dsc==0",
                base_weight: 1.6,
                kind: RegionKind::YieldSinglePeriod,
            },
            RegionSpec {
                name: "yield_multi_period_ordinary",
                description: "Multi-period bonds (n=2..60) across bases 0..4, freq 1/2/4, par/premium/discount, on/off coupon",
                base_weight: 1.6,
                kind: RegionKind::YieldMultiPeriodOrdinary,
            },
            RegionSpec {
                name: "yield_extreme_price_and_coupon",
                description: "Deep discount/extreme premium, zero/high coupon, negative-base crossings and 100-iter cap",
                base_weight: 2.0,
                kind: RegionKind::YieldExtremePriceAndCoupon,
            },
            RegionSpec {
                name: "yield_date_and_domain_edges",
                description: "Leap-year/month-end/1900 dates, fractional serials, invalid dates/frequencies/bases",
                base_weight: 1.5,
                kind: RegionKind::YieldDateAndDomainEdges,
            },
            RegionSpec {
                name: "yield_near_par_and_seed",
                description: "Prices within +-ULPs of par/redemption targeting seed cancellation and 1e-10 Newton stop",
                base_weight: 1.6,
                kind: RegionKind::YieldNearParAndSeed,
            },
        ],
        _ => Vec::new(),
    }
}

impl DomainSpec {
    /// Build a domain spec for `function` from `calc-catalogue` metadata plus default hint files.
    pub fn for_function(function: &str) -> Result<Self, String> {
        let hints = super::hints::load_default_hints()?;
        Self::for_function_with_hints(function, &hints)
    }

    /// Build a domain spec for `function` merged with an explicit [`super::hints::HintBank`].
    pub fn for_function_with_hints(
        function: &str,
        hints: &super::hints::HintBank,
    ) -> Result<Self, String> {
        let name = function.to_ascii_uppercase();
        let decl: DeclarationRef = calc_catalogue::named(&name)
            .ok_or_else(|| format!("function {name} is not in calc-catalogue"))?;
        let merged = super::spec_gen::build_merged_spec(&name, hints)?;
        let min_arity = merged.min_arity;
        let max_arity = merged.max_arity;
        let mut params = Vec::with_capacity(max_arity);
        for i in 0..max_arity {
            let (pname, pclass) = match decl.parameter(i) {
                Some(param) => (param.name.to_owned(), param.class),
                None => (format!("arg{i}"), ParameterClass::Value),
            };
            params.push(ParamSpec {
                kind: infer_param_kind(&pname, pclass, merged.args.get(i)),
                name: pname,
                class: pclass,
                optional: i >= min_arity,
            });
        }
        let custom = custom_regions(&name);
        let has_custom_hints = !custom.is_empty();
        let has_hint_file = merged.has_hint_file;
        let scale = if has_custom_hints { 0.45 } else { 1.0 };
        let mut regions = universal_regions(has_custom_hints);
        regions.push(RegionSpec {
            name: "spec_generic_domain",
            description: "Catalogue-derived domain kinds, valid ranges, repeating groups, and lifting",
            base_weight: 1.1 * scale,
            kind: RegionKind::SpecGenericDomain,
        });
        if merged.has_whole_array_params() {
            regions.push(RegionSpec {
                name: "spec_lifting_and_shapes",
                description: "dc-dfl.2.6 per-parameter lifting and whole-array shape variations",
                base_weight: 0.65 * scale,
                kind: RegionKind::SpecLiftingAndShapes,
            });
        }
        if merged.has_hint_anchors() {
            regions.push(RegionSpec {
                name: "hint_anchors_and_edges",
                description: "Hint-file specials and documented edges with +-ULP perturbations",
                base_weight: 1.5,
                kind: RegionKind::SpecHintAnchorsAndEdges,
            });
        }
        if merged.has_hint_ranges_or_relations() {
            regions.push(RegionSpec {
                name: "hint_ranges_and_relations",
                description: "Hint-file intervals (inside + just outside) and relations (honoured + broken)",
                base_weight: 1.6,
                kind: RegionKind::SpecHintRangesAndRelations,
            });
        }
        if merged.has_hint_error_edges() {
            regions.push(RegionSpec {
                name: "hint_error_edges",
                description: "Hint-file error_edges triggers targeting Excel error boundaries",
                base_weight: 1.0,
                kind: RegionKind::SpecHintErrorEdges,
            });
        }
        regions.extend(custom);
        Ok(Self {
            function: name,
            min_arity,
            max_arity,
            params,
            regions,
            has_custom_hints,
            has_hint_file,
            merged,
        })
    }

    /// Generate one deterministic argument vector for `region`.
    pub fn generate(&self, region: RegionKind, rng: &mut Rng) -> Vec<Value> {
        use super::spec_gen::{SpecRegionKind, generate_spec_region};
        match region {
            RegionKind::GenericSpecialValues => self.gen_special_values(rng),
            RegionKind::GenericSignPatterns => self.gen_sign_patterns(rng),
            RegionKind::GenericLogUniform => self.gen_log_uniform(rng),
            RegionKind::GenericNearIntegers => self.gen_near_integers(rng),
            RegionKind::GenericArityAndOptional => self.gen_arity_and_optional(rng),
            RegionKind::GenericTypeCoercionEdges => self.gen_type_coercion_edges(rng),
            RegionKind::SpecGenericDomain => {
                generate_spec_region(&self.merged, SpecRegionKind::GenericDomain, rng)
            }
            RegionKind::SpecLiftingAndShapes => {
                generate_spec_region(&self.merged, SpecRegionKind::LiftingAndShapes, rng)
            }
            RegionKind::SpecHintAnchorsAndEdges => {
                generate_spec_region(&self.merged, SpecRegionKind::HintAnchorsAndEdges, rng)
            }
            RegionKind::SpecHintRangesAndRelations => {
                generate_spec_region(&self.merged, SpecRegionKind::HintRangesAndRelations, rng)
            }
            RegionKind::SpecHintErrorEdges => {
                generate_spec_region(&self.merged, SpecRegionKind::HintErrorEdges, rng)
            }
            RegionKind::RateOrdinaryLoans => gen_rate_ordinary_loans(rng),
            RegionKind::RateFractionalAndNegNper => gen_rate_fractional_and_neg_nper(rng),
            RegionKind::RateFvNegPvAndZeroTerms => gen_rate_fv_neg_pv_and_zero_terms(rng),
            RegionKind::RateNearFlatAndRegimes => gen_rate_near_flat_and_regimes(rng),
            RegionKind::RateUnderflowAndOverflow => gen_rate_underflow_and_overflow(rng),
            RegionKind::RateConvergenceBudget => gen_rate_convergence_budget(rng),
            RegionKind::RateConstructedRoots => gen_rate_constructed_roots(rng),
            RegionKind::IrrTwoFlowExact => gen_irr_two_flow_exact(rng),
            RegionKind::IrrMultiFlowOrdinary => gen_irr_multi_flow_ordinary(rng),
            RegionKind::IrrMultiSignAndRetries => gen_irr_multi_sign_and_retries(rng),
            RegionKind::IrrScaleAndBudget => gen_irr_scale_and_budget(rng),
            RegionKind::IrrFlatAndEdges => gen_irr_flat_and_edges(rng),
            RegionKind::YieldSinglePeriod => gen_yield_single_period(rng),
            RegionKind::YieldMultiPeriodOrdinary => gen_yield_multi_period_ordinary(rng),
            RegionKind::YieldExtremePriceAndCoupon => gen_yield_extreme_price_and_coupon(rng),
            RegionKind::YieldDateAndDomainEdges => gen_yield_date_and_domain_edges(rng),
            RegionKind::YieldNearParAndSeed => gen_yield_near_par_and_seed(rng),
        }
    }

    fn pick_arity(&self, rng: &mut Rng) -> usize {
        if self.max_arity <= self.min_arity {
            self.min_arity
        } else {
            self.min_arity + rng.gen_range(self.max_arity - self.min_arity + 1)
        }
    }

    fn gen_param_value(&self, param: &ParamSpec, rng: &mut Rng, mode: u8) -> Value {
        if matches!(param.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
            let len = 2 + rng.gen_range(6);
            let mut rows = Vec::with_capacity(len);
            for k in 0..len {
                let mut v = match mode {
                    0 => *rng.pick(SPECIAL_NUMBERS),
                    1 => rng.log_uniform(-30, 30, true),
                    _ => rng.uniform(-1000.0, 1000.0),
                };
                if k == 0 && v > 0.0 && rng.chance(0.7) {
                    v = -v;
                } else if k > 0 && v < 0.0 && rng.chance(0.6) {
                    v = -v;
                }
                rows.push(Value::Array(vec![hex_f64(v)]));
            }
            return if param.kind == ParamKind::CellRange {
                json!({ "range": rows })
            } else {
                Value::Array(rows)
            };
        }
        let val = match param.kind {
            ParamKind::Timing => *rng.pick(&[0.0, 1.0, -1.0, 2.0, 0.5, 1e-12]),
            ParamKind::Frequency => *rng.pick(&[1.0, 2.0, 4.0, 0.0, 3.0, -2.0, 1.9, 2.5]),
            ParamKind::Basis => *rng.pick(&[0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 5.0, 0.9, 4.5]),
            ParamKind::DateSerial => match mode {
                0 => *rng.pick(&[0.0, 1.0, 59.0, 60.0, 61.0, 36526.0, 44927.0, 46753.0, 2958465.0, -1.0]),
                _ => rng.gen_i64_inclusive(1, 60000) as f64,
            },
            ParamKind::Probability => match mode {
                0 => *rng.pick(&[0.0, 1.0, 0.5, 1e-12, 1.0 - 1e-12, -0.1, 1.1]),
                _ => rng.uniform(0.0, 1.0),
            },
            ParamKind::Guess => match mode {
                0 => *rng.pick(&[0.1, 0.0, -0.5, -0.9999999, -1.0, -1.0000001, 1.0, 10.0, 1e6]),
                1 => rng.log_uniform(-20, 20, true),
                _ => rng.uniform(-0.99, 2.0),
            },
            ParamKind::Periods => match mode {
                0 => *rng.pick(&[0.0, 1.0, -1.0, 0.5, -0.5, 10.0, 12.0, 360.0, -10.0, 1e6, -1e6]),
                1 => rng.log_uniform(-15, 20, true),
                _ => rng.uniform(-50.0, 400.0),
            },
            ParamKind::Rate => match mode {
                0 => *rng.pick(&[0.0, 0.05, 0.1, -0.1, 1.0, -1.0, 1e-7, 1e-15, 10.0]),
                1 => rng.log_uniform(-30, 10, true),
                _ => rng.uniform(-0.5, 1.0),
            },
            ParamKind::PaymentOrAmount => match mode {
                0 => *rng.pick(SPECIAL_NUMBERS),
                1 => rng.log_uniform(-40, 40, true),
                _ => rng.uniform(-10000.0, 10000.0),
            },
            ParamKind::IntegerSmall => match mode {
                0 => *rng.pick(&[0.0, 1.0, -1.0, 2.0, 10.0, 100.0]),
                _ => rng.gen_i64_inclusive(-10, 200) as f64,
            },
            ParamKind::RealGeneral => match mode {
                0 => *rng.pick(SPECIAL_NUMBERS),
                1 => rng.log_uniform(-1020, 1020, true),
                _ => *rng.pick(DECIMAL_LITERALS) * if rng.chance(0.5) { -1.0 } else { 1.0 },
            },
            ParamKind::ArrayNumeric | ParamKind::CellRange => unreachable!(),
        };
        hex_f64(val)
    }

    fn gen_special_values(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        (0..arity)
            .map(|i| {
                let p = &self.params[i];
                if matches!(p.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
                    self.gen_param_value(p, rng, 0)
                } else if rng.chance(0.25) {
                    let base = *rng.pick(DECIMAL_LITERALS);
                    let sign = if rng.chance(0.5) { -1.0 } else { 1.0 };
                    hex_f64(base * sign)
                } else if rng.chance(0.25) {
                    let exp = rng.gen_i64_inclusive(-1022, 1023) as i32;
                    let sign = if rng.chance(0.5) { -1.0 } else { 1.0 };
                    hex_f64(sign * pow2_f64(exp))
                } else {
                    let v = *rng.pick(SPECIAL_NUMBERS);
                    if rng.chance(0.25) {
                        let delta = *rng.pick(&[-2i64, -1, 1, 2]);
                        hex_f64(step_ulps(v, delta, true).unwrap_or(v))
                    } else {
                        hex_f64(v)
                    }
                }
            })
            .collect()
    }

    fn gen_sign_patterns(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        let pattern = rng.next_u64();
        (0..arity)
            .map(|i| {
                let p = &self.params[i];
                if matches!(p.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
                    return self.gen_param_value(p, rng, 1);
                }
                let code = (pattern >> (i * 2)) & 3;
                let mag = match p.kind {
                    ParamKind::Timing => return hex_f64(if (code & 1) == 0 { 0.0 } else { 1.0 }),
                    ParamKind::Frequency => return hex_f64(*rng.pick(&[1.0, 2.0, 4.0])),
                    ParamKind::Basis => return hex_f64(rng.gen_range(5) as f64),
                    ParamKind::DateSerial => rng.gen_i64_inclusive(1, 50000) as f64,
                    _ => rng.log_uniform(-20, 20, false),
                };
                let v = match code {
                    0 => 0.0,
                    1 => mag,
                    2 => -mag,
                    _ => if rng.chance(0.5) { mag } else { -mag },
                };
                hex_f64(v)
            })
            .collect()
    }

    fn gen_log_uniform(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        (0..arity)
            .map(|i| {
                let p = &self.params[i];
                if matches!(p.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
                    return self.gen_param_value(p, rng, 1);
                }
                if p.kind == ParamKind::Timing && rng.chance(0.8) {
                    return hex_f64(if rng.chance(0.5) { 0.0 } else { 1.0 });
                }
                if p.kind == ParamKind::Frequency && rng.chance(0.8) {
                    return hex_f64(*rng.pick(&[1.0, 2.0, 4.0]));
                }
                if p.kind == ParamKind::Basis && rng.chance(0.8) {
                    return hex_f64(rng.gen_range(5) as f64);
                }
                let (lo, hi) = if rng.chance(0.5) { (-1020, 1020) } else { (-40, 40) };
                hex_f64(rng.log_uniform(lo, hi, true))
            })
            .collect()
    }

    fn gen_near_integers(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        (0..arity)
            .map(|i| {
                let p = &self.params[i];
                if matches!(p.kind, ParamKind::ArrayNumeric | ParamKind::CellRange) {
                    return self.gen_param_value(p, rng, 2);
                }
                let base = if p.kind == ParamKind::DateSerial {
                    rng.gen_i64_inclusive(1, 55000) as f64
                } else if p.kind == ParamKind::Timing {
                    *rng.pick(&[0.0, 1.0])
                } else if p.kind == ParamKind::Basis {
                    rng.gen_range(6) as f64
                } else if p.kind == ParamKind::Frequency {
                    *rng.pick(&[1.0, 2.0, 4.0])
                } else if rng.chance(0.2) {
                    *rng.pick(&[
                        1_048_576.0,
                        4_503_599_627_370_496.0,
                        9_007_199_254_740_992.0,
                    ]) * if rng.chance(0.5) { -1.0 } else { 1.0 }
                } else {
                    rng.gen_i64_inclusive(-50, 400) as f64
                };
                let perturbed = if rng.chance(0.5) {
                    let delta = *rng.pick(&[-4i64, -2, -1, 0, 1, 2, 4]);
                    step_ulps(base, delta, true).unwrap_or(base)
                } else {
                    let shift = pow2_f64(-(rng.gen_i64_inclusive(1, 50) as i32));
                    let v = base + if rng.chance(0.5) { shift } else { -shift };
                    if is_value2_exact(v) { v } else { base }
                };
                hex_f64(perturbed)
            })
            .collect()
    }

    fn gen_arity_and_optional(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        (0..arity)
            .map(|i| {
                let p = &self.params[i];
                if p.optional && p.kind != ParamKind::CellRange && rng.chance(0.35) {
                    Value::Null
                } else {
                    self.gen_param_value(p, rng, 2)
                }
            })
            .collect()
    }

    fn gen_type_coercion_edges(&self, rng: &mut Rng) -> Vec<Value> {
        let arity = self.pick_arity(rng);
        let mut row: Vec<Value> = (0..arity)
            .map(|i| self.gen_param_value(&self.params[i], rng, 2))
            .collect();
        if arity == 0 {
            return row;
        }
        let pos = rng.gen_range(arity);
        let coerced = match rng.gen_range(4) {
            0 => Value::String((*rng.pick(COERCION_TEXTS)).to_owned()),
            1 => Value::Bool(rng.chance(0.5)),
            2 => Value::Null,
            _ => json!({"error": *rng.pick(ERROR_VARIANTS)}),
        };
        if self.params[pos].kind == ParamKind::CellRange {
            // Place the non-numeric edge cell inside the real cell range so both Excel and DNA Calc
            // evaluate range-cell coercion rather than hitting a non-reference fault.
            if let Some(rows) = row[pos].get_mut("range").and_then(Value::as_array_mut)
                && !rows.is_empty()
            {
                let r = rng.gen_range(rows.len());
                if let Some(cells) = rows[r].as_array_mut()
                    && !cells.is_empty()
                {
                    let c = rng.gen_range(cells.len());
                    cells[c] = coerced;
                }
            }
            return row;
        }
        if self.params[pos].kind == ParamKind::ArrayNumeric {
            return row;
        }
        row[pos] = coerced;
        row
    }
}

// ============================================================================
// RATE-specific strategic region generators
// ============================================================================

fn pack_rate(
    nper: f64,
    pmt: f64,
    pv: f64,
    fv: Option<f64>,
    timing: Option<f64>,
    guess: Option<f64>,
) -> Vec<Value> {
    let mut out = vec![hex_f64(nper), hex_f64(pmt), hex_f64(pv)];
    if let Some(g) = guess {
        out.push(hex_f64(fv.unwrap_or(0.0)));
        out.push(hex_f64(timing.unwrap_or(0.0)));
        out.push(hex_f64(g));
    } else if let Some(t) = timing {
        out.push(hex_f64(fv.unwrap_or(0.0)));
        out.push(hex_f64(t));
    } else if let Some(f) = fv {
        out.push(hex_f64(f));
    }
    out
}

fn eval_fv_approx(rate: f64, nper: f64, pmt: f64, pv: f64, timing: f64) -> Option<f64> {
    if !rate.is_finite() || !nper.is_finite() {
        return None;
    }
    let fv = if rate == 0.0 {
        -(pv + pmt * nper)
    } else {
        let base = 1.0 + rate;
        if base <= 0.0 && nper.fract() != 0.0 {
            return None;
        }
        let factor = det_pow(base, nper);
        if !factor.is_finite() {
            return None;
        }
        let tf = if timing != 0.0 { 1.0 + rate } else { 1.0 };
        -(pv * factor + pmt * tf * ((factor - 1.0) / rate))
    };
    is_value2_exact(fv).then_some(fv)
}

fn gen_rate_ordinary_loans(rng: &mut Rng) -> Vec<Value> {
    let nper = *rng.pick(&[1.0, 2.0, 3.0, 4.0, 6.0, 10.0, 12.0, 24.0, 36.0, 48.0, 60.0, 120.0, 180.0, 240.0, 360.0, 600.0])
        + if rng.chance(0.15) { rng.gen_i64_inclusive(1, 50) as f64 } else { 0.0 };
    let pv = rng.log_uniform(0, 24, true);
    let r_target = rng.uniform(-0.08, 0.25);
    let timing = if rng.chance(0.45) { 1.0 } else { 0.0 };
    let fv = if rng.chance(0.55) { 0.0 } else { -pv * rng.uniform(-0.5, 1.5) };
    let pmt = if r_target.abs() < 1e-9 {
        -(pv + fv) / nper
    } else {
        let f = pow_u64(1.0 + r_target, nper as u64);
        let tf = if timing != 0.0 { 1.0 + r_target } else { 1.0 };
        if f.is_finite() && (f - 1.0).abs() > 1e-12 {
            -(pv * f + fv) * r_target / (tf * (f - 1.0))
        } else {
            -pv / nper
        }
    };
    let pmt = if is_value2_exact(pmt) { pmt } else { -pv / nper };
    let fv_opt = if fv == 0.0 && timing == 0.0 && rng.chance(0.3) { None } else { Some(fv) };
    let t_opt = if fv_opt.is_none() { None } else if timing == 0.0 && rng.chance(0.25) { None } else { Some(timing) };
    let guess = match rng.gen_range(5) {
        0 => None,
        1 => Some(0.1),
        2 => Some(0.0),
        3 => Some(r_target + rng.uniform(-0.2, 0.2)),
        _ => Some(rng.uniform(-0.95, 2.0)),
    };
    pack_rate(nper, pmt, pv, fv_opt, t_opt, guess)
}

fn gen_rate_fractional_and_neg_nper(rng: &mut Rng) -> Vec<Value> {
    let nper = match rng.gen_range(6) {
        0 => rng.uniform(0.001, 20.0),                      // fractional positive
        1 => -rng.uniform(0.001, 20.0),                     // fractional negative
        2 => -(rng.gen_i64_inclusive(1, 500) as f64),       // negative integer
        3 => -rng.log_uniform(-30, 12, false),              // tiny-to-moderate negative
        4 => -rng.log_uniform(20, 300, false),              // huge negative nper
        _ => rng.log_uniform(-20, 10, false),               // tiny fractional positive
    };
    let timing = if rng.chance(0.5) { 1.0 } else { 0.0 };
    let pv = rng.log_uniform(-15, 20, true);
    let pmt = rng.log_uniform(-15, 20, true);
    let fv = if rng.chance(0.3) { 0.0 } else { rng.log_uniform(-15, 20, true) };
    let guess = match rng.gen_range(5) {
        0 => None,
        1 => Some(0.0),
        2 => Some(rng.uniform(-0.99, 1.5)),
        3 => Some(rng.log_uniform(-10, 15, false)),
        _ => Some(rng.uniform(-0.9999, -0.01)),
    };
    pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
}

fn gen_rate_fv_neg_pv_and_zero_terms(rng: &mut Rng) -> Vec<Value> {
    let nper = match rng.gen_range(4) {
        0 => rng.gen_i64_inclusive(1, 360) as f64,
        1 => rng.uniform(-10.0, 20.0),
        2 => *rng.pick(&[0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0]),
        _ => rng.log_uniform(-10, 10, true),
    };
    let pv = rng.log_uniform(-20, 25, true);
    let timing = if rng.chance(0.5) { 1.0 } else { 0.0 };
    let (pmt, pv, fv) = match rng.gen_range(6) {
        0 => {
            // Exact fv = -pv with nonzero pmt
            let pmt = if rng.chance(0.5) { 0.0 } else { rng.log_uniform(-20, 20, true) };
            (pmt, pv, -pv)
        }
        1 => {
            // fv within +-1..16 ULP of -pv
            let delta = rng.gen_i64_inclusive(-16, 16);
            let fv = step_ulps(-pv, delta, true).unwrap_or(-pv);
            let pmt = if rng.chance(0.4) { 0.0 } else { rng.log_uniform(-15, 10, true) };
            (pmt, pv, fv)
        }
        2 => {
            // Zero payment (pure compound growth: pv*(1+r)^n + fv = 0)
            let fv = -pv * rng.log_uniform(-10, 10, false);
            let fv = if is_value2_exact(fv) { fv } else { -pv };
            (0.0, pv, fv)
        }
        3 => {
            // Zero PV
            let pmt = rng.log_uniform(-15, 20, true);
            let fv = -pmt * nper * rng.uniform(0.5, 2.0);
            let fv = if is_value2_exact(fv) { fv } else { -pmt };
            (pmt, 0.0, fv)
        }
        4 => {
            // Near-zero root: pv + fv + pmt * nper ~= 0
            let pmt = rng.log_uniform(-10, 15, true);
            let exact_fv = -(pv + pmt * nper);
            let fv = if is_value2_exact(exact_fv) {
                let d = rng.gen_i64_inclusive(-32, 32);
                step_ulps(exact_fv, d, true).unwrap_or(exact_fv)
            } else {
                -pv
            };
            (pmt, pv, fv)
        }
        _ => (0.0, 0.0, 0.0), // Flat zero objective -> #NUM!
    };
    let guess = match rng.gen_range(4) {
        0 => None,
        1 => Some(0.0),
        2 => Some(*rng.pick(&[-0.001, 0.001, 1e-7, -1e-7, 1e-4, -1e-4])),
        _ => Some(rng.uniform(-0.99, 1.0)),
    };
    pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
}

fn gen_rate_near_flat_and_regimes(rng: &mut Rng) -> Vec<Value> {
    let timing = if rng.chance(0.5) { 1.0 } else { 0.0 };
    match rng.gen_range(7) {
        0 => {
            // Regime II / III / IV huge roots (2^28 .. 2^46) via inverse construction
            let r_star = rng.log_uniform(28, 46, false);
            let nper = if rng.chance(0.5) {
                -rng.uniform(0.01, 1.5)
            } else {
                rng.uniform(0.01, 0.5)
            };
            let pv = rng.log_uniform(-5, 12, true);
            let pmt = rng.log_uniform(-5, 12, true);
            let fv_base = eval_fv_approx(r_star, nper, pmt, pv, timing).unwrap_or(-pmt);
            let fv = step_ulps(fv_base, rng.gen_i64_inclusive(-16, 16), true).unwrap_or(fv_base);
            let guess = if rng.chance(0.4) {
                Some(rng.uniform(-0.5, 2.0))
            } else {
                Some(r_star * rng.uniform(0.5, 1.5))
            };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        1 => {
            // General small fractional negative-nper flat-secant regime:
            // same-sign (pmt, pv) with small opposite-sign fv and varied guess.
            let nper = -rng.log_uniform(-8, 0, false);
            let sign = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let pmt = sign * rng.log_uniform(0, 12, false);
            let pv = sign * rng.log_uniform(2, 14, false);
            let fv = -sign * rng.log_uniform(-8, 4, false);
            let guess = if rng.chance(0.25) {
                Some(rng.uniform(-0.8, 0.0))
            } else {
                Some(rng.uniform(0.01, 2.0))
            };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        2 => {
            // Flat-change tie probes (huge negative nper, guess=0 or small, both balance signs!)
            let nper = -rng.log_uniform(20, 300, false);
            let pv = rng.log_uniform(-10, 15, true);
            let pmt = rng.log_uniform(-10, 15, true);
            let fv = if rng.chance(0.5) {
                // Positive initial balance -> offset = -0.001 (cb22 Block A discriminator!)
                rng.log_uniform(0, 15, false)
            } else {
                -rng.log_uniform(0, 15, false)
            };
            let guess = Some(*rng.pick(&[0.0, 0.001, -0.001, 0.002, 0.1, -0.5]));
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        3 => {
            // Near -1.0 root and near -1.0 guess boundary
            let eps = rng.log_uniform(-30, -5, false);
            let r_star = (-1.0 + eps).max(-1.0 + 1e-12);
            let nper = *rng.pick(&[1.0, 2.0, 3.0, 5.0, 10.0, 0.5, 1.5, -1.0, -2.0]);
            let pv = rng.log_uniform(0, 12, false);
            let pmt = -rng.log_uniform(-2, 10, false);
            let fv = eval_fv_approx(r_star, nper, pmt, pv, timing).unwrap_or(0.0);
            let guess = if rng.chance(0.4) {
                let d = rng.gen_i64_inclusive(-16, 16);
                Some(step_ulps(-1.0, d, true).unwrap_or(-1.0))
            } else {
                Some((-1.0 + eps * rng.uniform(0.5, 3.0)).max(-1.0 + 1e-12))
            };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        4 => {
            // cb22 Block A family: positive nper (40..150), huge pv (1e150..1e295), guess near -0.997
            // probing current==before ties with positive initial balance and underflowed (1+r)^nper.
            let nper = rng.uniform(40.0, 150.0);
            let pv = rng.log_uniform(500, 980, true);
            let pmt = if rng.chance(0.5) { 0.0 } else { rng.log_uniform(-10, 20, true) };
            let fv = rng.log_uniform(-10, 15, true);
            let guess = Some(-1.0 + rng.log_uniform(-14, -4, false));
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        5 => {
            // cb22 Block B family: large negative nper (-5000..-200), huge pv (~1e250..1e305),
            // tiny fv (~1e-7..1e-3), positive guess -> double-flat inside 1e-4 acceptance.
            let nper = -rng.uniform(200.0, 5000.0);
            let pv = rng.log_uniform(800, 1015, true);
            let pmt = if rng.chance(0.6) { 0.0 } else { rng.log_uniform(-25, -8, true) };
            let fv = rng.log_uniform(-26, -10, true);
            let guess = Some(rng.uniform(0.05, 1.5));
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        _ => {
            // Double-flat / tiny-slope near-zero-nper region
            let nper = rng.log_uniform(-1020, -40, true);
            let pv = rng.log_uniform(-10, 15, true);
            let pmt = rng.log_uniform(-10, 15, true);
            let fv = -pv + rng.uniform(-1e-3, 1e-3);
            let fv = if is_value2_exact(fv) { fv } else { -pv };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), Some(rng.uniform(-0.5, 1.0)))
        }
    }
}

fn gen_rate_underflow_and_overflow(rng: &mut Rng) -> Vec<Value> {
    let timing = if rng.chance(0.5) { 1.0 } else { 0.0 };
    match rng.gen_range(5) {
        0 => {
            // Tiny normal cashflows (1e-308 .. 1e-290): dc-chz5m subnormal-slope / DAZ region
            let nper = match rng.gen_range(3) {
                0 => rng.uniform(0.1, 20.0),
                1 => -rng.uniform(0.1, 20.0),
                _ => rng.gen_i64_inclusive(1, 100) as f64,
            };
            let pv = rng.log_uniform(-1022, -960, true);
            let pmt = rng.log_uniform(-1022, -960, true);
            let fv = if rng.chance(0.4) { 0.0 } else { rng.log_uniform(-1022, -960, true) };
            let guess = Some(rng.uniform(-0.9, 2.0));
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        1 => {
            // Extreme negative-period beginning-payment overflow (dc-iuwct.64): |fv - pmt| around 1e-7
            let nper = -rng.log_uniform(-12, 5, false);
            let pmt = rng.log_uniform(-10, 15, true);
            let pv = rng.log_uniform(-10, 15, true);
            let offset = rng.log_uniform(-28, -20, true); // ~3e-9 .. 1e-6 around 1e-7
            let fv = if rng.chance(0.6) { pmt + offset } else { offset };
            let fv = if is_value2_exact(fv) { fv } else { pmt };
            let guess = Some(rng.uniform(-0.95, 1.5));
            pack_rate(nper, pmt, pv, Some(fv), Some(1.0), guess)
        }
        2 => {
            // Huge cashflows (1e280 .. 1e308) near binary64 overflow
            let nper = rng.uniform(-20.0, 20.0);
            let pv = rng.log_uniform(930, 1022, true);
            let pmt = rng.log_uniform(930, 1022, true);
            let fv = if rng.chance(0.4) { -pv } else { rng.log_uniform(930, 1022, true) };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), Some(rng.uniform(-0.5, 1.0)))
        }
        3 => {
            // Huge positive guess near 2^1022 (1 / MIN_POSITIVE) with negative periods
            let nper = -rng.uniform(0.01, 5.0);
            let pmt = rng.log_uniform(-5, 10, true);
            let pv = rng.log_uniform(-5, 10, true);
            let fv = if rng.chance(0.5) { 0.0 } else { rng.log_uniform(-26, -22, true) };
            let guess = Some(rng.log_uniform(980, 1023, false));
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
        }
        _ => {
            // Mixed extreme exponents: tiny pmt with ordinary pv/fv, or large nper causing factor overflow/underflow
            let nper = rng.log_uniform(6, 20, true);
            let pv = rng.log_uniform(-20, 20, true);
            let pmt = rng.log_uniform(-1022, -980, true);
            let fv = -pv * rng.uniform(0.5, 2.0);
            let fv = if is_value2_exact(fv) { fv } else { -pv };
            pack_rate(nper, pmt, pv, Some(fv), Some(timing), Some(rng.uniform(-0.5, 1.0)))
        }
    }
}

fn gen_rate_convergence_budget(rng: &mut Rng) -> Vec<Value> {
    let timing = if rng.chance(0.4) { 1.0 } else { 0.0 };
    if rng.chance(0.35) {
        // Classic 200-step mortgage cutoff family: RATE(360, -599.55, 100000, 0, 0, g)
        let nper = *rng.pick(&[120.0, 240.0, 360.0, 480.0, 600.0]);
        let pv = *rng.pick(&[100000.0, 200000.0, 1_000_000.0, 1000.0]);
        let r_true = rng.uniform(0.002, 0.02);
        let f = pow_u64(1.0 + r_true, nper as u64);
        let pmt = if f.is_finite() && f > 1.0 {
            -pv * r_true * f / (f - 1.0)
        } else {
            -pv * 0.006
        };
        let guess = rng.uniform(0.25, 0.85);
        pack_rate(nper, pmt, pv, Some(0.0), Some(timing), Some(guess))
    } else {
        // Large-scale annuities whose 1-ULP balance residual is near 1e-7..1e-4 (triggers 200-iter 1e-4 check)
        let nper = rng.gen_i64_inclusive(5, 360) as f64;
        let pv = rng.log_uniform(24, 38, false);
        let r_true = rng.uniform(0.001, 0.15);
        let f = pow_u64(1.0 + r_true, nper as u64);
        let tf = if timing != 0.0 { 1.0 + r_true } else { 1.0 };
        let pmt = if f.is_finite() && (f - 1.0).abs() > 1e-12 {
            -pv * f * r_true / (tf * (f - 1.0))
        } else {
            -pv / nper
        };
        let pmt = if is_value2_exact(pmt) { pmt } else { -pv / nper };
        let guess = r_true + rng.uniform(-0.05, 0.5);
        pack_rate(nper, pmt, pv, Some(0.0), Some(timing), Some(guess))
    }
}

fn gen_rate_constructed_roots(rng: &mut Rng) -> Vec<Value> {
    let timing = if rng.chance(0.5) { 1.0 } else { 0.0 };
    // Draw target root across all interesting regimes
    let r_star = match rng.gen_range(6) {
        0 => -1.0 + rng.log_uniform(-24, -1, false),       // negative near -1..0
        1 => rng.log_uniform(-40, -10, true),              // tiny around 0
        2 => rng.uniform(0.001, 2.0),                      // ordinary positive
        3 => rng.log_uniform(2, 28, false),                // large Regime I
        4 => rng.log_uniform(29, 39, false),               // Regime II (1e-7 < ulp <= 1e-4)
        _ => rng.log_uniform(39, 46, false),               // Regime III/IV (ulp > 1e-4)
    };
    let r_star = if r_star <= -1.0 { -0.999999 } else { r_star };
    let nper = match rng.gen_range(4) {
        0 => rng.gen_i64_inclusive(1, 100) as f64,
        1 => rng.uniform(0.05, 15.0),
        2 => -rng.uniform(0.05, 15.0),
        _ => -(rng.gen_i64_inclusive(1, 50) as f64),
    };
    let pv = rng.log_uniform(-10, 18, true);
    let pmt = if rng.chance(0.15) { 0.0 } else { rng.log_uniform(-10, 18, true) };
    let Some(fv_exact) = eval_fv_approx(r_star, nper, pmt, pv, timing) else {
        return gen_rate_ordinary_loans(rng);
    };
    let fv = if rng.chance(0.6) {
        fv_exact
    } else {
        let d = rng.gen_i64_inclusive(-32, 32);
        step_ulps(fv_exact, d, true).unwrap_or(fv_exact)
    };
    let guess = match rng.gen_range(5) {
        0 => None,
        1 => Some(r_star),
        2 => Some((r_star + *rng.pick(&[-1e-6, 1e-6, -1e-4, 1e-4, -0.01, 0.01])).max(-0.999999)),
        3 => Some(step_ulps(r_star, rng.gen_i64_inclusive(-8, 8), true).unwrap_or(r_star)),
        _ => Some(rng.uniform(-0.9, 1.5)),
    };
    pack_rate(nper, pmt, pv, Some(fv), Some(timing), guess)
}

// ============================================================================
// IRR-specific strategic region generators
// ============================================================================

fn pack_irr(flows: &[f64], guess: Option<f64>) -> Vec<Value> {
    let col: Vec<Value> = flows.iter().map(|&x| Value::Array(vec![hex_f64(x)])).collect();
    match guess {
        Some(g) => vec![Value::Array(col), hex_f64(g)],
        None => vec![Value::Array(col)],
    }
}

fn gen_irr_two_flow_exact(rng: &mut Rng) -> Vec<Value> {
    let a = rng.log_uniform(-40, 40, false);
    let r_star = match rng.gen_range(4) {
        0 => -1.0 + rng.log_uniform(-24, -1, false),
        1 => rng.uniform(-0.5, 5.0),
        2 => rng.log_uniform(2, 40, false),
        _ => rng.log_uniform(-40, -5, true),
    };
    let b = a * (1.0 + r_star);
    let b = if is_value2_exact(b) && b > 0.0 { b } else { a * 1.1 };
    let flows = if rng.chance(0.5) { [-a, b] } else { [a, -b] };
    let guess = match rng.gen_range(5) {
        0 => None,
        1 => Some(step_ulps(-1.0, rng.gen_i64_inclusive(-8, 16), true).unwrap_or(-0.999)),
        2 => {
            // 2^53 stored-addition boundary (1 + guess == guess -> #VALUE! vs #NUM!)
            let g = step_ulps(9_007_199_254_740_992.0, rng.gen_i64_inclusive(-8, 8), true)
                .unwrap_or(9_007_199_254_740_992.0);
            Some(g)
        }
        3 => Some(r_star + rng.uniform(-0.5, 0.5)),
        _ => Some(rng.uniform(-0.99, 10.0)),
    };
    pack_irr(&flows, guess)
}

fn gen_irr_multi_flow_ordinary(rng: &mut Rng) -> Vec<Value> {
    let len = 3 + rng.gen_range(10);
    let scale = rng.log_uniform(0, 20, false);
    let mut flows = Vec::with_capacity(len);
    flows.push(-scale);
    let mut sum = 0.0;
    for _ in 1..len {
        let f = scale * rng.uniform(0.05, 0.8);
        sum += f;
        flows.push(f);
    }
    if sum <= scale * 0.2 {
        flows[len - 1] += scale;
    }
    let guess = if rng.chance(0.35) {
        None
    } else {
        Some(rng.uniform(-0.9, 2.0))
    };
    pack_irr(&flows, guess)
}

fn gen_irr_multi_sign_and_retries(rng: &mut Rng) -> Vec<Value> {
    let len = 3 + rng.gen_range(10);
    let mut flows = Vec::with_capacity(len);
    for _ in 0..len {
        flows.push(rng.log_uniform(-10, 18, true));
    }
    // Ensure at least one positive and one negative flow
    if flows.iter().all(|&x| x >= 0.0) {
        flows[0] = -flows[0].abs().max(1.0);
    } else if flows.iter().all(|&x| x <= 0.0) {
        flows[len - 1] = flows[len - 1].abs().max(1.0);
    }
    let guess = match rng.gen_range(5) {
        0 => None,
        1 => Some(-0.99),
        2 => Some(100.0),
        3 => Some(rng.log_uniform(5, 40, false)),
        _ => Some(rng.uniform(-0.999, 5.0)),
    };
    pack_irr(&flows, guess)
}

fn gen_irr_scale_and_budget(rng: &mut Rng) -> Vec<Value> {
    let base = [-70000.0, 12000.0, 15000.0, 18000.0, 21000.0, 26000.0];
    match rng.gen_range(6) {
        0 => {
            // 2^15..2^32 scaling: post-budget 1e-4 frontier!
            let factor = pow2_f64(rng.gen_i64_inclusive(15, 32) as i32);
            let flows: Vec<f64> = base
                .iter()
                .map(|&x| x * factor * rng.uniform(0.8, 1.2))
                .collect();
            let guess = if rng.chance(0.4) { None } else { Some(rng.uniform(-0.8, 2.0)) };
            pack_irr(&flows, guess)
        }
        1 => {
            // Ultra-tiny normal cashflows (2^-1022 .. 2^-1005) triggering secant divisor DAZ abort!
            let len = 2 + rng.gen_range(5);
            let mut flows = Vec::with_capacity(len);
            flows.push(-rng.log_uniform(-1022, -1005, false));
            for _ in 1..len {
                flows.push(rng.log_uniform(-1022, -1005, false));
            }
            let guess = Some(rng.uniform(-0.5, 1.5));
            pack_irr(&flows, guess)
        }
        2 => {
            // Long chain (25..50 flows) with extreme guess so discount power underflows/overflows!
            let len = 25 + rng.gen_range(26);
            let mut flows = Vec::with_capacity(len);
            flows.push(-100.0);
            for _ in 1..len {
                flows.push(rng.uniform(1.0, 20.0));
            }
            let guess = if rng.chance(0.5) {
                Some(rng.log_uniform(20, 40, false))
            } else {
                Some(-1.0 + rng.log_uniform(-40, -15, false))
            };
            pack_irr(&flows, guess)
        }
        3 => {
            // Near-overflow cashflows
            let factor = pow2_f64(rng.gen_i64_inclusive(950, 1005) as i32);
            let flows: Vec<f64> = base
                .iter()
                .map(|&x| {
                    let v = x * factor * rng.uniform(0.8, 1.2);
                    if is_value2_exact(v) { v } else { x }
                })
                .collect();
            let guess = if rng.chance(0.4) { None } else { Some(rng.uniform(-0.8, 2.0)) };
            pack_irr(&flows, guess)
        }
        4 => {
            // Small-exponent cashflows near DAZ/FTZ
            let factor = pow2_f64(rng.gen_i64_inclusive(-1020, -960) as i32);
            let flows: Vec<f64> = base
                .iter()
                .map(|&x| {
                    let v = x * factor * rng.uniform(0.8, 1.2);
                    if is_value2_exact(v) { v } else { f64::MIN_POSITIVE }
                })
                .collect();
            pack_irr(&flows, Some(rng.uniform(-0.8, 2.0)))
        }
        _ => {
            let factor = pow2_f64(rng.gen_i64_inclusive(-30, 30) as i32);
            let flows: Vec<f64> = base
                .iter()
                .map(|&x| x * factor * rng.uniform(0.8, 1.2))
                .collect();
            let guess = if rng.chance(0.4) { None } else { Some(rng.uniform(-0.8, 2.0)) };
            pack_irr(&flows, guess)
        }
    }
}

fn gen_irr_flat_and_edges(rng: &mut Rng) -> Vec<Value> {
    match rng.gen_range(5) {
        0 => {
            // Leading and trailing zeros
            let mut flows = vec![0.0; 1 + rng.gen_range(3)];
            flows.push(-rng.log_uniform(0, 12, false));
            flows.push(rng.log_uniform(0, 12, false));
            flows.extend(std::iter::repeat_n(0.0, 1 + rng.gen_range(3)));
            pack_irr(&flows, Some(rng.uniform(-0.5, 1.0)))
        }
        1 => {
            // Huge root near u = 1 - 2^-k
            let m = rng.log_uniform(20, 50, false);
            pack_irr(&[-1.0, m], Some(rng.uniform(-0.5, 10.0)))
        }
        2 => {
            // Near -1 root (u = r/(1+r) very negative -> flat u0 + 0.001 == u0)
            let eps = pow2_f64(-(rng.gen_i64_inclusive(20, 52) as i32));
            pack_irr(&[-1.0, eps], Some(-1.0 + eps))
        }
        3 => {
            // All-same-sign -> #NUM!
            let sign = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let flows = vec![sign * 10.0, sign * 20.0, sign * 30.0];
            pack_irr(&flows, None)
        }
        _ => {
            // Long cashflow chain (15..25 periods) stressing power overflow/underflow
            let len = 15 + rng.gen_range(11);
            let mut flows = Vec::with_capacity(len);
            flows.push(-1000.0);
            for _ in 1..len {
                flows.push(rng.uniform(-50.0, 200.0));
            }
            pack_irr(&flows, Some(rng.uniform(-0.9, 5.0)))
        }
    }
}

// ============================================================================
// YIELD-specific strategic region generators
// ============================================================================

fn pack_yield(
    settlement: f64,
    maturity: f64,
    rate: f64,
    pr: f64,
    redemption: f64,
    frequency: f64,
    basis: Option<f64>,
) -> Vec<Value> {
    let mut out = vec![
        hex_f64(settlement),
        hex_f64(maturity),
        hex_f64(rate),
        hex_f64(pr),
        hex_f64(redemption),
        hex_f64(frequency),
    ];
    if let Some(b) = basis {
        out.push(hex_f64(b));
    }
    out
}

fn gen_yield_single_period(rng: &mut Rng) -> Vec<Value> {
    let freq = *rng.pick(&[1.0, 2.0, 4.0]);
    let basis = if rng.chance(0.15) { None } else { Some(rng.gen_range(5) as f64) };
    let max_days = match freq as u32 {
        1 => 360,
        2 => 180,
        _ => 88,
    };
    let (settlement, maturity) = if rng.chance(0.25) {
        // Month-end 30/360 dsc == 0 probes: e.g. 2024-03-30 -> 2024-03-31 (serials 45381 -> 45382)
        let m = *rng.pick(&[45382.0, 45443.0, 45504.0, 45535.0, 45596.0, 45657.0, 44985.0]);
        let s = m - *rng.pick(&[1.0, 2.0, 3.0, 30.0]);
        (s, m)
    } else {
        let s = rng.gen_i64_inclusive(36526, 55000) as f64;
        let span = rng.gen_i64_inclusive(1, max_days) as f64;
        (s, s + span)
    };
    let rate = if rng.chance(0.2) { 0.0 } else { rng.uniform(0.001, 0.25) };
    let red = if rng.chance(0.6) { 100.0 } else { rng.uniform(50.0, 150.0) };
    let pr = rng.uniform(10.0, 150.0);
    pack_yield(settlement, maturity, rate, pr, red, freq, basis)
}

fn gen_yield_multi_period_ordinary(rng: &mut Rng) -> Vec<Value> {
    let freq = *rng.pick(&[1.0, 2.0, 4.0]);
    let basis = if rng.chance(0.15) { None } else { Some(rng.gen_range(5) as f64) };
    let s = rng.gen_i64_inclusive(30000, 55000) as f64;
    let years = rng.gen_i64_inclusive(1, 30) as f64;
    let offset = if rng.chance(0.3) { 0.0 } else { rng.gen_i64_inclusive(1, 360) as f64 };
    let m = s + years * 365.0 + offset;
    let rate = if rng.chance(0.15) { 0.0 } else { rng.uniform(0.001, 0.2) };
    let red = if rng.chance(0.65) { 100.0 } else { rng.uniform(50.0, 200.0) };
    let pr = rng.uniform(40.0, 160.0);
    pack_yield(s, m, rate, pr, red, freq, basis)
}

fn gen_yield_extreme_price_and_coupon(rng: &mut Rng) -> Vec<Value> {
    let freq = *rng.pick(&[1.0, 2.0, 4.0]);
    let basis = Some(rng.gen_range(5) as f64);
    let s = rng.gen_i64_inclusive(36526, 48000) as f64;
    // Include on-coupon exact year multiples to trigger negative-base integer-offset curves!
    let m = if rng.chance(0.45) {
        // Same month/day k years later (approx via 365/366) -> use known exact coupon dates
        let anchor = *rng.pick(&[44927.0, 45292.0, 45658.0, 46023.0]); // Jan 1 2023..2026
        let years = rng.gen_i64_inclusive(2, 20);
        let s_anchor = anchor;
        let m_anchor = anchor + (years * 365 + (years + 1) / 4) as f64;
        (s_anchor, m_anchor)
    } else {
        (s, s + rng.gen_i64_inclusive(400, 15000) as f64)
    };
    let (rate, pr, red) = match rng.gen_range(6) {
        0 => (0.0, rng.log_uniform(-15, 25, false), 100.0), // zero-coupon extreme prices
        1 => (rng.uniform(0.01, 0.2), rng.log_uniform(-12, 2, false), 100.0), // deep discount tiny pr
        2 => (rng.uniform(0.01, 0.2), rng.log_uniform(8, 25, false), 100.0), // huge premium pr
        3 => (
            // Extreme scale triggering multi_period_seed_nonfinite / newton_final_nonfinite_num
            rng.log_uniform(900, 1020, false),
            rng.log_uniform(-1020, -900, false),
            rng.log_uniform(900, 1020, false),
        ),
        4 => (
            // Zero-coupon on-coupon deep premium -> Newton crosses base < 0 and hits subnormal slope!
            0.0,
            rng.log_uniform(7, 35, false),
            if rng.chance(0.5) { 100.0 } else { rng.log_uniform(-30, 10, false) },
        ),
        _ => (
            rng.log_uniform(0, 12, false),
            rng.uniform(1.0, 500.0),
            rng.log_uniform(0, 15, false),
        ),
    };
    pack_yield(m.0, m.1, rate, pr, red, freq, basis)
}

fn gen_yield_date_and_domain_edges(rng: &mut Rng) -> Vec<Value> {
    let freq = *rng.pick(&[1.0, 2.0, 4.0, 0.0, 3.0, -1.0, 1.5, 2.9]);
    let basis = *rng.pick(&[Some(0.0), Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(-1.0), Some(5.0), Some(4.9), Some(-0.1), None]);
    let (s, m) = match rng.gen_range(5) {
        0 => {
            // Leap days & 1900 serial boundary
            let dates = [0.0, 1.0, 59.0, 60.0, 61.0, 36585.0, 45351.0, 45352.0, 2958464.0, 2958465.0];
            let a = *rng.pick(&dates);
            let b = *rng.pick(&dates);
            (a, b)
        }
        1 => {
            // Fractional serials (truncation boundary)
            let s = rng.gen_i64_inclusive(40000, 46000) as f64 + rng.uniform(0.0, 0.999999);
            let m = s + rng.gen_i64_inclusive(100, 2000) as f64 + rng.uniform(0.0, 0.999999);
            (s, m)
        }
        2 => {
            // Invalid date domains (negative or > 2958465 or s >= m)
            let s = *rng.pick(&[-10.0, -0.5, 0.0, 45000.0, 2958466.0]);
            let m = *rng.pick(&[-5.0, 45000.0, 44999.0, 2958465.0, 3000000.0]);
            (s, m)
        }
        3 => {
            // Non-positive price/redemption or negative rate
            let s = 44927.0;
            let m = 46753.0;
            let r = *rng.pick(&[-0.01, -1e-12, 0.0, 0.05]);
            let pr = *rng.pick(&[-10.0, 0.0, 100.0]);
            let red = *rng.pick(&[-100.0, 0.0, 100.0]);
            return pack_yield(s, m, r, pr, red, freq, basis);
        }
        _ => {
            // Very long maturities (50..200 years)
            let s = 44927.0;
            let m = s + rng.gen_i64_inclusive(18000, 73000) as f64;
            (s, m)
        }
    };
    pack_yield(s, m, rng.uniform(0.0, 0.15), rng.uniform(50.0, 150.0), 100.0, freq, basis)
}

fn gen_yield_near_par_and_seed(rng: &mut Rng) -> Vec<Value> {
    let freq = *rng.pick(&[1.0, 2.0, 4.0]);
    let basis = Some(rng.gen_range(5) as f64);
    let s = rng.gen_i64_inclusive(40000, 48000) as f64;
    let m = s + rng.gen_i64_inclusive(400, 10000) as f64;
    let rate = *rng.pick(&[0.0, 0.01, 0.05, 0.08, 0.1, 0.125]);
    let red = if rng.chance(0.7) { 100.0 } else { *rng.pick(&[95.0, 105.0, 110.0]) };
    let delta_exp = rng.gen_i64_inclusive(0, 28) as u32;
    let delta = (1i64 << delta_exp) * if rng.chance(0.5) { 1 } else { -1 };
    let pr = step_ulps(100.0, delta, true).unwrap_or(100.0);
    pack_yield(s, m, rate, pr, red, freq, basis)
}

/// Apply a deterministic local mutation around a parent case (used by code-steered search and
/// adaptive densification around discrepancies).
pub fn mutate_case(spec: &DomainSpec, parent: &[Value], rng: &mut Rng) -> Vec<Value> {
    let mut out = parent.to_vec();
    if out.is_empty() {
        return out;
    }
    let idx = rng.gen_range(out.len());
    // Mutate a numeric element inside a real cell range (`{"range": [[...]]}`) or numeric array
    let is_range = out[idx].get("range").is_some();
    if let Some(rows) = out[idx].get("range").unwrap_or(&out[idx]).as_array().cloned() {
        if !rows.is_empty() {
            let r = rng.gen_range(rows.len());
            if let Some(cells) = rows[r].as_array().cloned()
                && !cells.is_empty()
            {
                let c = rng.gen_range(cells.len());
                if let Some(bits) = super::rng::parse_hex_bits(&cells[c]) {
                    let v = f64::from_bits(bits);
                    let mutated = mutate_f64(v, rng);
                    let mut new_rows = rows;
                    let mut new_cells = cells;
                    new_cells[c] = hex_f64(mutated);
                    new_rows[r] = Value::Array(new_cells);
                    out[idx] = if is_range {
                        json!({ "range": new_rows })
                    } else {
                        Value::Array(new_rows)
                    };
                    return out;
                }
            }
        }
    }
    // Mutate a scalar numeric argument
    if let Some(bits) = super::rng::parse_hex_bits(&out[idx]) {
        let v = f64::from_bits(bits);
        let pkind = spec.params.get(idx).map(|p| p.kind).unwrap_or(ParamKind::RealGeneral);
        let mutated = match pkind {
            ParamKind::Timing => if v == 0.0 { 1.0 } else { 0.0 },
            ParamKind::Frequency => *rng.pick(&[1.0, 2.0, 4.0]),
            ParamKind::Basis => rng.gen_range(5) as f64,
            ParamKind::DateSerial => (v + *rng.pick(&[-365.0, -30.0, -1.0, 1.0, 30.0, 365.0])).max(1.0),
            _ => mutate_f64(v, rng),
        };
        out[idx] = hex_f64(mutated);
    }
    out
}

fn mutate_f64(v: f64, rng: &mut Rng) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    let candidate = match rng.gen_range(6) {
        0 => step_ulps(v, *rng.pick(&[-16, -4, -2, -1, 1, 2, 4, 16]), true).unwrap_or(v),
        1 => step_ulps(v, rng.gen_i64_inclusive(-1024, 1024), true).unwrap_or(v),
        2 => v * *rng.pick(&[0.5, 2.0, 0.999, 1.001, 0.99, 1.01, -1.0]),
        3 => v + *rng.pick(&[-1e-7, 1e-7, -1e-4, 1e-4, -0.001, 0.001, -0.01, 0.01]),
        4 => {
            // Bit-flip one exponent or mantissa bit safely
            let bit = rng.gen_range(62);
            f64::from_bits(v.to_bits() ^ (1u64 << bit))
        }
        _ => v * pow2_f64(rng.gen_i64_inclusive(-8, 8) as i32),
    };
    if is_value2_exact(candidate) { candidate } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_introspection_covers_hinted_and_arbitrary_functions() {
        let rate = DomainSpec::for_function("RATE").unwrap();
        assert_eq!(rate.min_arity, 3);
        assert_eq!(rate.max_arity, 6);
        assert!(rate.has_custom_hints);
        assert!(rate.regions.len() >= 12);

        let irr = DomainSpec::for_function("IRR").unwrap();
        assert_eq!(irr.min_arity, 1);
        assert_eq!(irr.max_arity, 2);
        assert_eq!(irr.params[0].kind, ParamKind::ArrayNumeric);

        let yld = DomainSpec::for_function("YIELD").unwrap();
        assert_eq!(yld.min_arity, 6);
        assert_eq!(yld.max_arity, 7);

        // Unhinted arbitrary function works purely from catalogue metadata.
        let pmt = DomainSpec::for_function("PMT").unwrap();
        assert!(!pmt.has_custom_hints);
        assert_eq!(pmt.min_arity, 3);
        assert_eq!(pmt.max_arity, 5);
        let mut rng = Rng::from_seed(7);
        for region in &pmt.regions {
            let args = pmt.generate(region.kind, &mut rng);
            assert!((pmt.min_arity..=pmt.max_arity).contains(&args.len()));
        }
    }

    #[test]
    fn rate_near_flat_and_regimes_samples_generally_without_fixture_constants() {
        let rate = DomainSpec::for_function("RATE").unwrap();
        let mut rng = Rng::from_seed(20261124);
        let forbidden: [u64; 5] = [
            0xbfb6_dc00_4793_792d,
            0x4049_c31d_f3fc_025c,
            0x4081_7662_41ff_7680,
            0xbfe9_01d6_d18d_4f4b,
            0x3fdd_a8bb_c02b_4f20,
        ];
        let mut small_neg_npers = std::collections::HashSet::new();
        for _ in 0..200 {
            let args = rate.generate(RegionKind::RateNearFlatAndRegimes, &mut rng);
            for arg in &args {
                if let Some(bits) = super::super::rng::parse_hex_bits(arg) {
                    assert!(
                        !forbidden.contains(&bits),
                        "hardcoded fixture constant 0x{bits:016x} appeared in RateNearFlatAndRegimes"
                    );
                }
            }
            if let Some(bits) = super::super::rng::parse_hex_bits(&args[0]) {
                let nper = f64::from_bits(bits);
                if nper > -1.0 && nper < 0.0 {
                    small_neg_npers.insert(bits);
                }
            }
        }
        assert!(
            small_neg_npers.len() >= 15,
            "expected general sampling of small negative fractional nper in (-1, 0), got {} distinct values",
            small_neg_npers.len()
        );
    }
}
