#!/usr/bin/env python3
"""dc-jnt test migration: legacy Excel readings as catalogue rows and their question capture.

The legacy suites of OxFunc (tests/w111_*_replay.rs) replay Excel readings (build 20430, Value2 bit
patterns) that are kept as JSON under docs/function-lane/evidence/. This tool turns them into

  * a section file of the catalogue oracle, `catalogue/sheet-jnt_<topic>.txt` (one row per line:
    the formula, then the tab-separated typed inputs of make-catalogue-fixtures.ps1), and
  * a version 3 function-question capture of the whole jnt workbook that holds the legacy
    readings as the expected answers of those rows, to be split into the section's capture with
    `cargo xtask capture --split --pending-only` (no Excel run).

Nothing here computes an expected value: every answer is a legacy reading, copied as it was read.

  jnt_import.py rows    <batch> --repo <DnaCalc clone> --legacy <OxFunc clone> [--work <dir>]
  jnt_import.py capture <batch> --repo <DnaCalc clone> --legacy <OxFunc clone> [--work <dir>]

Rows are chosen per family (one function and formula shape): at most QUOTA rows by a seeded stride
over the family's source classes (a class is one evidence file and one id tag), plus every boundary
row (an id tag ending in "boundary", or an argument that is +-1, the smallest normal or the largest
finite number). A reading whose inputs a cell cannot hold (a subnormal, an infinity, a NaN or -0)
is left out and counted.
"""
import argparse
import collections
import hashlib
import json
import math
import os
import random
import re
import struct
import sys

QUOTA = 250
# A batch that sets "skip_array_forms" leaves out typed cases whose formula holds an array constant:
# the legacy capture entered those as dynamic-array formulas, which a catalogue row (a legacy formula
# in the file) is not; live Excel read CHOOSE({1},x) under MROUND as #VALUE! there (dc-jnt.6).
SKIP_ARRAY_FORMS = False
SEED = "dc-jnt-1"
EVIDENCE = "docs/function-lane/evidence/w111-broad-20260929/"
LEGACY_TESTS = "crates/oxfunc_core/tests/"
EXCEL_BUILD = "16.0.20430 64-bit, Value2 bulk input (OxFunc w111-broad-20260929)"

ERRORS = {"Null": "null", "Div0": "div0", "Value": "value", "Ref": "ref", "Name": "name", "Num": "num", "NA": "na"}
ERROR_LITERAL = {"Null": "#NULL!", "Div0": "#DIV/0!", "Value": "#VALUE!", "Ref": "#REF!", "Name": "#NAME?",
                 "Num": "#NUM!", "NA": "#N/A"}

# --- batches ---------------------------------------------------------------------------------------
# A family: name, the function it calls, its sources (evidence path, shape) and an optional table of
# formula templates by argument count ({A}, {B}, {C} are the row's input cells).


def weeknum_supported(case):
    """WEEKNUM selectors outside 1, 2, 11 to 17 and 21 were withheld by the legacy replay: duplicate
    oracle inputs disagreed there."""
    if case.get("function") != "WEEKNUM" or len(case["args"]) < 2:
        return True
    number = struct.unpack(">d", struct.pack(">Q", int(case["args"][1][1], 16)))[0]
    ceiling = math.ceil(number)
    threshold = 2.0 ** -22 + 2.0 ** -33 if ceiling > 0 else 2.0 ** -23 + 2.0 ** -34
    selector = ceiling if ceiling - number <= threshold else math.floor(number)
    return selector in (1, 2, 21) or 11 <= selector <= 17


def fam(name, fn, sources, templates=None, quota=QUOTA, unit_edge=False, multi=False, keep=None):
    """A family. With `multi`, the sources hold several functions and each function is a family of its own."""
    return dict(name=name, fn=fn, sources=sources, templates=templates or {}, quota=quota, unit_edge=unit_edge,
                multi=multi, keep=keep)


def hexsrc(path):
    return (path, "hex")


def typedsrc(path):
    return (path, "typed")


def pairsrc(cases, excel):
    return (cases + "|" + excel, "pair")


def dirsrc(path):
    return (path, "dir")


BATCHES = {
    "J11": dict(
        bead="dc-jnt.15",
        sheet="jnt_operators",
        hand=True,
        title="Operators: arithmetic, unary plus and minus, comparison and concatenation over numbers, numeric text, logicals and blanks",
        tests=["op_add.rs", "operator_arithmetic_family.rs", "operator_compare_concat_family.rs"],
        families=[],
    ),
    "J10": dict(
        bead="dc-jnt.14",
        sheet="jnt_dates_random",
        hand=True,
        title="Dates and time: DATE, DAY, MONTH, YEAR, DAYS, HOUR, MINUTE, SECOND, TIME, DATEVALUE, TIMEVALUE, DAYS360, DATEDIF, EDATE, EOMONTH, WEEKDAY, WEEKNUM, ISOWEEKNUM, WORKDAY, NETWORKDAYS",
        tests=["date_fn.rs", "date_parts_family.rs", "date_value_family.rs", "date_week_family.rs",
               "workday_networkdays_family.rs"],
        families=[],
    ),
    "J1": dict(
        bead="dc-jnt.5",
        sheet="jnt_w111_elementary",
        title="W111 elementary functions: ATAN, ASIN, ACOS, ACOT, TANH, COTH, LOG, LOG10, POWER, FISHER, MEDIAN",
        tests=["w111_atan_live_replay.rs", "w111_atan_typed_replay.rs", "w111_asin_acos_live_replay.rs",
               "w111_acot_live_replay.rs", "w111_tanh_coth_live_replay.rs",
               "w111_elementary_prepared_live_replay.rs", "w111_elementary_publication_replay.rs",
               "w111_power_integer_threshold_live_replay.rs"],
        families=[
            fam("ATAN", "ATAN", [hexsrc("atan/discovery.json"), hexsrc("atan/heldout.json"),
                                 hexsrc("atan/refinement.json"), hexsrc("atan/heldout2.json")]),
            fam("ATAN typed", "ATAN", [typedsrc("atan/typed-heldout-bank.json"),
                                       typedsrc("atan/typed-repeat-controls-bank.json")]),
            fam("ASIN", "ASIN", [hexsrc("asin/discovery.json"), hexsrc("asin/isolation-asin.json"),
                                 hexsrc("asin/signed-discovery.json"), hexsrc("asin/heldout.json"),
                                 hexsrc("asin/heldout2-asin.json")], unit_edge=True),
            fam("ACOS", "ACOS", [hexsrc("asin/acos-caller.json"), hexsrc("asin/heldout2-acos.json")], unit_edge=True),
            fam("ACOT", "ACOT", [hexsrc("acot/discovery.json"), hexsrc("acot/heldout.json"),
                                 hexsrc("acot/heldout2.json")]),
            fam("TANH", "TANH", [hexsrc("hyperbolic/discovery/answers/answers-tanh.json"),
                                 hexsrc("hyperbolic/heldout/answers/answers-tanh.json"),
                                 hexsrc("tanh-coth/independent-tanh.json")]),
            fam("COTH", "COTH", [hexsrc("hyperbolic/discovery/answers/answers-coth.json"),
                                 hexsrc("hyperbolic/heldout/answers/answers-coth.json"),
                                 hexsrc("tanh-coth/independent-coth.json")]),
            fam("LOG", "LOG", [hexsrc("elementary-publication/discovery/answers-log.json"),
                               hexsrc("elementary-publication/heldout/answers-log.json"),
                               hexsrc("elementary-publication/omitted-base/answers-log.json"),
                               hexsrc("elementary-publication/prepared-first-heldout/answers-log.json")]),
            fam("LOG10", "LOG10", [hexsrc("elementary-publication/omitted-base/answers-log10.json"),
                                   hexsrc("elementary-publication/prepared-first-heldout/answers-log10.json")]),
            fam("POWER", "POWER", [hexsrc("elementary-publication/discovery/answers-power.json"),
                                   hexsrc("elementary-publication/heldout/answers-power.json"),
                                   hexsrc("power-integer-thresholds/discovery-answers.json"),
                                   hexsrc("power-integer-thresholds/decimal-wrap-answers.json"),
                                   hexsrc("power-integer-thresholds/heldout-answers.json")]),
            fam("FISHER", "FISHER", [hexsrc("elementary-publication/discovery/answers-fisher.json"),
                                     hexsrc("elementary-publication/heldout/answers-fisher.json")]),
            fam("MEDIAN", "MEDIAN", [hexsrc("elementary-publication/discovery/answers-median.json"),
                                     hexsrc("elementary-publication/heldout/answers-median.json")]),
            fam("prepared values", None, [typedsrc("elementary-publication/prepared-discovery.json"),
                                          typedsrc("elementary-publication/prepared-heldout.json"),
                                          typedsrc("elementary-publication/median-error-origin.json"),
                                          typedsrc("elementary-publication/aggregate-origin-heldout.json")]),
        ],
    ),
    "J2": dict(
        bead="dc-jnt.6",
        skip_array_forms=True,
        sheet="jnt_w111_rounding",
        title="W111 rounding and integers: ROUND family, MROUND, QUOTIENT, GCD, LCM, FACT, PERMUTATIONA, radix conversions, MOD, RADIANS, SECH",
        tests=["w111_rounding_boundary_live_replay.rs", "w111_rounding_prepared_live_replay.rs",
               "w111_mround_live_replay.rs", "w111_mround_prepared_live_replay.rs", "w111_integer_live_replay.rs",
               "w111_quotient_live_replay.rs", "w111_quotient_prepared_live_replay.rs", "w111_gcd_lcm_live_replay.rs",
               "w111_fact_live_replay.rs", "w111_radix_live_replay.rs", "w111_radix_typed_replay.rs",
               "w111_permutationa_typed_replay.rs", "w111_mod_publication.rs", "w111_numeric_publication_live_replay.rs"],
        families=[
            fam("ROUND", "ROUND", [hexsrc("rounding-boundaries/numeric-round.json"),
                                   hexsrc("rounding-boundaries/endpoint-round.json")]),
            fam("ROUNDUP", "ROUNDUP", [hexsrc("rounding-boundaries/numeric-roundup.json"),
                                       hexsrc("rounding-boundaries/endpoint-roundup.json"),
                                       hexsrc("rounding-boundaries/tiny-difference-roundup.json"),
                                       hexsrc("rounding-boundaries/tiny-threshold-roundup.json"),
                                       hexsrc("rounding-boundaries/refined-heldout-roundup.json"),
                                       hexsrc("rounding-boundaries/residual-heldout-roundup.json")]),
            fam("ROUNDDOWN", "ROUNDDOWN", [hexsrc("rounding-boundaries/numeric-rounddown.json"),
                                           hexsrc("rounding-boundaries/endpoint-rounddown.json"),
                                           hexsrc("rounding-boundaries/residual-heldout-rounddown.json")]),
            fam("TRUNC", "TRUNC", [hexsrc("rounding-boundaries/numeric-trunc.json"),
                                   hexsrc("rounding-boundaries/endpoint-trunc.json")]),
            fam("MROUND", "MROUND", [hexsrc("mround/broad-answers.json"), hexsrc("mround/half-boundary-answers.json"),
                                     hexsrc("mround/fraction-cutoff-answers.json"), hexsrc("mround/heldout-answers.json"),
                                     hexsrc("mround/staged-heldout-answers.json")]),
            fam("QUOTIENT", "QUOTIENT", [hexsrc("quotient/discovery.json"), hexsrc("quotient/heldout.json")]),
            fam("GCD", "GCD", [hexsrc("gcd-lcm/numeric-gcd.json"), hexsrc("gcd-lcm/numeric-discovery2-gcd.json"),
                               hexsrc("gcd-lcm/numeric-refinement-gcd.json"), hexsrc("gcd-lcm/numeric-heldout-gcd.json")]),
            fam("LCM", "LCM", [hexsrc("gcd-lcm/numeric-lcm.json"), hexsrc("gcd-lcm/numeric-discovery2-lcm.json"),
                               hexsrc("gcd-lcm/numeric-refinement-lcm.json"), hexsrc("gcd-lcm/numeric-heldout-lcm.json")]),
            fam("FACT", "FACT", [hexsrc("fact/discovery.json"), hexsrc("fact/heldout.json")]),
            fam("PERMUTATIONA", "PERMUTATIONA", [hexsrc("integer-publication/permutationa-heldout/answers-permutationa.json"),
                                                 hexsrc("integer-publication/permutationa-second/answers-permutationa.json")]),
            fam("MOD", "MOD", [hexsrc("@tests/fixtures/w111_mod_publication.json")]),
            fam("RADIANS", "RADIANS", [hexsrc("integer-publication/radians-sech-heldout2/answers/answers-radians.json")]),
            fam("SECH", "SECH", [hexsrc("integer-publication/radians-sech-heldout2/answers/answers-sech.json")]),
            fam("radix", None, [dirsrc("radix"), dirsrc("radix/heldout")], multi=True),
            fam("rounding prepared", None, [typedsrc("rounding-boundaries/prepared-discovery.json"),
                                            typedsrc("rounding-boundaries/prepared-heldout.json")]),
            fam("MROUND prepared", "MROUND", [typedsrc("mround/prepared-discovery.json"), typedsrc("mround/prepared-heldout.json"),
                                              typedsrc("mround/reference-discovery.json"), typedsrc("mround/reference-heldout.json")]),
            fam("QUOTIENT prepared", "QUOTIENT", [typedsrc("quotient/reference-discovery.json"),
                                                  typedsrc("quotient/reference-heldout.json"),
                                                  typedsrc("quotient/padding-heldout.json"), typedsrc("quotient/typed.json"),
                                                  typedsrc("quotient/typed-validation.json")]),
            fam("GCD and LCM typed", None, [typedsrc("gcd-lcm/typed-initial.json"), typedsrc("gcd-lcm/typed-discovery2.json"),
                                            typedsrc("gcd-lcm/typed-refinement.json"), typedsrc("gcd-lcm/typed-array-order.json"),
                                            typedsrc("gcd-lcm/typed-heldout.json")]),
            fam("PERMUTATIONA typed", "PERMUTATIONA", [typedsrc("integer-publication/permutationa-prepared/typed-discovery.json"),
                                                       typedsrc("integer-publication/permutationa-prepared/typed-heldout.json")]),
            fam("radix typed", None, [typedsrc("radix/typed-discovery.json"), typedsrc("radix/typed-heldout.json")]),
        ],
    ),
    "J3": dict(
        bead="dc-jnt.7",
        skip_array_forms=True,
        sheet="jnt_w111_text_dates",
        title="W111 text, dates and statistics: numeric text, numeric to text, text slices, dates, ADDRESS, conditional, NOT, distributions, aggregates",
        tests=["w111_numeric_text_live_replay.rs", "w111_numeric_to_text_live_replay.rs", "w111_text_slice_live_replay.rs",
               "w111_date_live_replay.rs", "w111_date_typed_replay.rs", "w111_address_live_replay.rs",
               "w111_conditional_live_replay.rs", "w111_not_live_replay.rs", "w111_distribution_surface_live_replay.rs",
               "w111_distribution_numeric_live_replay.rs", "w111_aggregate_publication_replay.rs"],
        families=[
            fam("numeric text", None, [typedsrc("numeric-text/core-discovery.json"), typedsrc("numeric-text/core-heldout.json"),
                                       typedsrc("numeric-text/percent-bounds.json"),
                                       typedsrc("numeric-text/precision-heldout-matched.json"),
                                       typedsrc("numeric-text/midpoints.json"), typedsrc("numeric-text/nibble-heldout.json"),
                                       typedsrc("count-family/discovery.json"), typedsrc("count-family/computed.json"),
                                       typedsrc("count-family/heldout.json"), typedsrc("count-family/origin-refinement.json")]),
            fam("numeric to text", None, [typedsrc("numeric-to-text/discovery.json"), typedsrc("numeric-to-text/heldout.json"),
                                          typedsrc("numeric-to-text/explicit-serializers.json"),
                                          typedsrc("numeric-to-text/precision.json"),
                                          typedsrc("numeric-to-text/explicit-serializers-heldout.json")]),
            fam("text slice", None, [typedsrc("text-slice/discovery.json"), typedsrc("text-slice/numeric-boundaries.json"),
                                     typedsrc("text-slice/heldout.json")]),
            fam("dates", None, [dirsrc("dates/boundaries"), dirsrc("dates/conversion"), dirsrc("dates/integer"),
                                dirsrc("dates/heldout"), dirsrc("dates/heldout2"), dirsrc("dates/refinement")],
                multi=True, keep=weeknum_supported),
            fam("dates typed", None, [typedsrc("dates/typed-first-bank.json"), typedsrc("dates/typed-refinement-bank.json"),
                                      typedsrc("dates/typed-heldout2-bank.json"), typedsrc("dates/typed-array-heldout-bank.json")]),
            fam("ADDRESS", "ADDRESS", [hexsrc("address/boundaries.json"), hexsrc("address/discovery.json"),
                                       hexsrc("address/rounding.json"), hexsrc("address/floor-threshold.json"),
                                       hexsrc("address/floor-bits.json"), hexsrc("address/floor-zoom.json"),
                                       hexsrc("address/grammar-validation.json"), hexsrc("address/numeric-sheet.json"),
                                       hexsrc("address/numeric-format-discriminators.json"), hexsrc("address/numeric-ties.json"),
                                       hexsrc("address/reference-prefix.json"), hexsrc("address/refined-heldout.json")]),
            fam("ADDRESS typed", "ADDRESS", [typedsrc("address/typed.json"), typedsrc("address/errors.json"),
                                             typedsrc("address/sheet.json"), typedsrc("address/refinement.json"),
                                             typedsrc("address/blank-omission.json"), typedsrc("address/tokens.json"),
                                             typedsrc("address/numeric-text-syntax.json"),
                                             typedsrc("address/blank-numeric-text.json")]),
            fam("conditional", None, [pairsrc("conditional/discovery-cases.json", "conditional/discovery-excel.jsonl"),
                                      pairsrc("conditional/shape-cases.json", "conditional/shape-excel.jsonl"),
                                      pairsrc("conditional/heldout-cases.json", "conditional/heldout-excel.jsonl"),
                                      pairsrc("conditional/heldout2-cases.json", "conditional/heldout2-excel.jsonl")]),
            fam("NOT", "NOT", [pairsrc("not/cases.json", "not/excel.jsonl"),
                               pairsrc("not/heldout-cases.json", "not/heldout-excel.jsonl")]),
            fam("distribution surface", None,
                [pairsrc("distribution-surface/discovery-cases.json", "distribution-surface/discovery-excel.jsonl"),
                 pairsrc("distribution-surface/surface-cases.json", "distribution-surface/surface-excel.jsonl"),
                 pairsrc("distribution-surface/heldout-cases.json", "distribution-surface/heldout-excel.jsonl"),
                 pairsrc("distribution-surface/padding-heldout-cases.json", "distribution-surface/padding-heldout-excel.jsonl"),
                 pairsrc("distribution-surface/adjacent-cases.json", "distribution-surface/adjacent-excel.jsonl"),
                 pairsrc("distribution-surface/prepared-heldout-cases.json", "distribution-surface/prepared-heldout-excel.jsonl"),
                 pairsrc("distribution-surface/unit-padding-cases.json", "distribution-surface/unit-padding-excel.jsonl"),
                 pairsrc("distribution-surface/unit-heldout-cases.json", "distribution-surface/unit-heldout-excel.jsonl")]),
            fam("distribution numeric", None,
                [hexsrc("distribution-surface/final-hypgeom.dist.json"), hexsrc("distribution-surface/poisson-branch-heldout.json"),
                 hexsrc("distribution-surface/poisson-publication/discovery-answers.json"),
                 hexsrc("distribution-surface/poisson-publication/heldout-answers.json"),
                 hexsrc("integer-publication/heldout/answers/answers-norm.s.dist.json"),
                 hexsrc("distribution-surface/cdf-research/z-heldout-answers-norm.s.dist.json"),
                 hexsrc("distribution-surface/cdf-research/z-heldout-answers-gauss.json")], multi=True),
            fam("aggregate publication", None, [hexsrc("aggregate-publication/discovery/answers-harmean.json"),
                                                hexsrc("aggregate-publication/discovery/answers-devsq.json"),
                                                hexsrc("aggregate-publication/heldout/answers-harmean.json"),
                                                hexsrc("aggregate-publication/heldout/answers-devsq.json")], multi=True),
        ],
    ),
}

# --- reading the evidence ----------------------------------------------------------------------------


def special_class(bits):
    """None for a number a cell holds exactly, else the reason it cannot."""
    exponent = (bits >> 52) & 0x7FF
    mantissa = bits & ((1 << 52) - 1)
    if exponent == 0x7FF:
        return "non-finite"
    if bits == 1 << 63:
        return "negative zero"
    if exponent == 0 and mantissa:
        return "subnormal"
    return None


def is_boundary_value(bits, family):
    """The smallest normal and the largest finite number, and 1 for a family whose domain ends there."""
    magnitude = bits & ~(1 << 63)
    edges = (0x0010000000000000, 0x7FEFFFFFFFFFFFFF) + ((0x3FF0000000000000,) if family.get("unit_edge") else ())
    return magnitude in edges


def tag_of(identifier):
    return re.sub(r"\d+", "#", identifier)


def expected_of_bits(text):
    """The typed expectation of a legacy `expected_bits`: ('number', hex16) or ('error', wire name)."""
    if text.startswith("0x"):
        return ("number", text[2:].lower().rjust(16, "0"))
    if text.startswith("error:"):
        return ("error", ERRORS[text[6:]])
    if text.startswith("text:"):
        return ("text", text[5:])
    if text in ("logical:true", "logical:false"):
        return ("logical", text == "logical:true")
    raise ValueError("unreadable expectation " + text)


def evidence_path(legacy, rel):
    if rel.startswith("@tests/"):
        return os.path.join(legacy, LEGACY_TESTS, rel[len("@tests/"):])
    return os.path.join(legacy, EVIDENCE, rel)


def read_hex_source(legacy, rel, family, skipped):
    path = evidence_path(legacy, rel)
    with open(path, "rb") as handle:
        raw = handle.read()
    data = json.loads(raw)
    cases = []
    for row in data["witnesses"]:
        args = row["args"]
        reasons = []
        bits = []
        tokens = []
        for arg in args:
            if re.fullmatch(r"0x[0-9a-fA-F]{16}", arg):
                value = int(arg, 16)
                bits.append(value)
                tokens.append(("n", "%016x" % value))
                why = special_class(value)
                if why:
                    reasons.append(why)
            elif CANNOT_HOLD.search(arg):
                reasons.append("text a cell cannot hold")
            elif CONTROL.search(arg):
                tokens.append(("f", "&".join(char_pieces(arg)), arg))
            else:
                tokens.append(("s", arg))
        if reasons:
            skipped[reasons[0]] += 1
            continue
        if len(args) > 6:
            skipped["more than six arguments"] += 1
            continue
        tag = tag_of(row["id"])
        boundary = any(is_boundary_value(b, family) for b in bits)
        named = bool(re.search(r"(^|-)boundary$", tag))
        cases.append(dict(id=row["id"], source=rel, tag=tag, boundary=boundary,
                          args=tokens, nargs=len(args), named_boundary=named,
                          expected=expected_of_bits(row["expected_bits"]), formula=None,
                          function=data.get("function")))
    return cases, hashlib.sha256(raw).hexdigest()


CELL = re.compile(r"(?<![A-Za-z0-9_.$!'])\$?([A-Z]{1,3})\$?([0-9]{1,7})(?![A-Za-z0-9_(.!:])")


def outside_quotes(text):
    """Splits a formula into (is_text, part) pieces: parts inside double quotes are text."""
    pieces, current, inside, i = [], "", False, 0
    while i < len(text):
        ch = text[i]
        if ch == '"':
            if inside and text[i + 1:i + 2] == '"':
                current += '""'
                i += 2
                continue
            if inside:
                current += ch
                pieces.append((True, current))
                current, inside = "", False
            else:
                if current:
                    pieces.append((False, current))
                current, inside = ch, True
        else:
            current += ch
        i += 1
    if current:
        pieces.append((inside, current))
    return pieces


CONTROL = re.compile("[\x01-\x1f]")
CANNOT_HOLD = re.compile("[\x00\ud800-\udfff\ufffe\uffff]")


def char_pieces(text):
    """Text holding control characters as formula pieces: quoted runs and CHAR(n) calls. XML 1.0 cannot
    carry these characters (and a line cannot carry a tab or a newline), so an input cell or a formula
    literal that holds one is written as ="ab"&CHAR(12)&"cd" and Excel builds the text."""
    out, buf = [], ""
    for ch in text:
        if ord(ch) < 32:
            if buf:
                out.append('"' + buf.replace('"', '""') + '"')
                buf = ""
            out.append("CHAR(%d)" % ord(ch))
        else:
            buf += ch
    if buf:
        out.append('"' + buf.replace('"', '""') + '"')
    return out


def typed_input(value):
    kind = value["kind"]
    if kind == "number":
        return ("n", "%016x" % struct.unpack(">Q", struct.pack(">d", float(value["value"])))[0])
    if kind == "text":
        text = value["value"]
        if CANNOT_HOLD.search(text):
            return None
        if CONTROL.search(text):
            return ("f", "&".join(char_pieces(text)), text)
        return ("s", text)
    if kind == "logical":
        return ("b", "TRUE" if value["value"] else "FALSE")
    if kind == "error":
        return ("e", ERROR_LITERAL[value["code"]])
    if kind == "empty_cell":
        return ("-", "")
    return None


def typed_expectation(outcome):
    kind = outcome["kind"]
    if kind == "number":
        bits = outcome["bits_hex"]
        return ("number", bits[2:].lower().rjust(16, "0"))
    if kind == "error":
        return ("error", ERRORS[outcome["code"]])
    if kind == "text":
        return ("text", outcome["value"])
    if kind in ("logical", "boolean"):
        return ("logical", bool(outcome["value"]))
    return None


def read_typed_source(legacy, rel, family, skipped):
    with open(evidence_path(legacy, rel), "rb") as handle:
        raw = handle.read()
    return typed_cases(json.loads(raw)["witnesses"], rel, skipped), hashlib.sha256(raw).hexdigest()


def read_pair_source(legacy, rel, family, skipped):
    """A case set (`{"cases": [...]}`) and the JSON lines of Excel's outcomes, named `cases|excel`."""
    cases_rel, excel_rel = rel.split("|")
    with open(evidence_path(legacy, cases_rel), "rb") as handle:
        raw_cases = handle.read()
    with open(evidence_path(legacy, excel_rel), "rb") as handle:
        raw_excel = handle.read()
    outcomes = {}
    for line in raw_excel.decode("utf-8").splitlines():
        if line.strip():
            outcome = json.loads(line)
            outcomes[outcome["case_id"]] = outcome
    rows = []
    for case in json.loads(raw_cases)["cases"]:
        if case["case_id"] in outcomes:
            rows.append(dict(case=case, excel=outcomes[case["case_id"]]))
        else:
            skipped["pair: no outcome"] += 1
    digest = hashlib.sha256(raw_cases + raw_excel).hexdigest()
    return typed_cases(rows, rel, skipped), digest


def typed_cases(rows, rel, skipped):
    cases = []
    for row in rows:
        case, excel = row["case"], row["excel"]
        if excel.get("execution_status") != "ok":
            skipped["typed: Excel did not answer"] += 1
            continue
        expected = typed_expectation(excel["outcome"])
        if expected is None:
            skipped["typed: " + excel["outcome"]["kind"] + " outcome"] += 1
            continue
        text = case["formula_text"]
        if not text.startswith("="):
            skipped["typed: not a formula"] += 1
            continue
        text = text[1:]
        if SKIP_ARRAY_FORMS and "{" in "".join(part for is_text, part in outside_quotes(text) if not is_text):
            # Excel's legacy file form reads CHOOSE({1},x) and the like through implicit intersection;
            # the legacy capture entered such formulas as dynamic-array formulas.
            skipped["typed: array constant in a formula (dynamic-array entry)"] += 1
            continue
        if CANNOT_HOLD.search(text):
            skipped["typed: text a cell cannot hold"] += 1
            continue
        rebuilt, usable = [], True
        for is_text, part in outside_quotes(text):
            if CONTROL.search(part):
                if is_text and len(part) >= 2 and part.endswith('"'):
                    part = "(" + "&".join(char_pieces(part[1:-1].replace('""', '"'))) + ")"
                else:
                    usable = False
            rebuilt.append(part)
        if not usable:
            skipped["typed: control character outside a text literal"] += 1
            continue
        text = "".join(rebuilt)
        fixtures = {f["target"]: f["value"] for f in case["cell_fixture"]}
        mapping, inputs, ok = {}, [], True
        pieces = []
        for is_text, part in outside_quotes(text):
            if is_text:
                pieces.append(part)
                continue
            if ":" in part and CELL.search(part) and re.search(r"[A-Z]+[0-9]+:[A-Z$]", part):
                ok = False
                skipped["typed: range reference"] += 1
                break

            def replace(match, mapping=mapping):
                address = match.group(1) + match.group(2)
                if address not in mapping:
                    mapping[address] = "ABCEFG"[len(mapping)] if len(mapping) < 6 else None
                letter = mapping[address]
                return "{" + letter + "}" if letter else match.group(0)

            pieces.append(CELL.sub(replace, part))
        if not ok:
            continue
        if any(v is None for v in mapping.values()):
            skipped["typed: more than six cells"] += 1
            continue
        formula = "".join(pieces)
        inputs = []
        for address, letter in sorted(mapping.items(), key=lambda kv: kv[1]):
            if address in fixtures:
                token = typed_input(fixtures[address])
                if token is None:
                    ok = False
                    skipped["typed: fixture " + fixtures[address]["kind"]] += 1
                    break
                if token[0] == "n" and special_class(int(token[1], 16)):
                    ok = False
                    skipped["typed: " + special_class(int(token[1], 16))] += 1
                    break
            else:
                token = ("-", "")
            inputs.append(token)
        if not ok:
            continue
        if any(f not in mapping for f in fixtures if re.match(r"^[A-Z]+[0-9]+$", f)) and fixtures:
            # a fixture cell the formula does not name: harmless decoy, left out
            pass
        cases.append(dict(id=case["case_id"], source=rel, tag=tag_of(case["case_tag"]) if "case_tag" in case else "",
                          boundary=False, args=inputs, nargs=len(inputs), expected=expected,
                          formula=formula, function=case.get("canonical_surface_name")))
    return cases


PREFIX_FILES = ["prefix-f1.txt", "prefix-l1.txt", "prefix-f2.txt", "prefix-f3.txt", "prefix-b1l.txt", "prefix-h9u.txt"]
_prefixed = None


def prefixed_functions(repo):
    """The functions Excel needs `_xlfn.` for in a file: their row of the sheet prefix_metadata reads #NAME?
    without the prefix and a value with it (make-catalogue-fixtures.ps1, the capture of that sheet)."""
    global _prefixed
    if _prefixed is not None:
        return _prefixed
    base = os.path.join(repo, "crates/calc-func/tests")
    calls = []
    for name in PREFIX_FILES:
        for line in open(os.path.join(base, "fixtures/catalogue", name), encoding="utf-8").read().splitlines():
            if line and not line.startswith("#"):
                calls.append(line)
    capture = json.load(open(os.path.join(base, "captures/catalogue/sections/prefix_metadata.capture.json")))
    seen = {(o["row"], o["column"]): o["excel_raw"] for o in capture["observations"]}
    found = set()
    for index, call in enumerate(calls, start=1):
        with_prefix, without = seen.get((index, 1)), seen.get((index, 2))
        if without == {"error": "name"} and with_prefix != {"error": "name"}:
            found.add(call.split("(")[0])
    # Functions the prefix files do not list but Excel stores with the prefix (live Excel read them as
    # #NAME? without it, dc-jnt.7).
    found |= {"IFNA", "IFS", "SWITCH", "XOR", "CONCAT", "TEXTJOIN", "MAXIFS", "MINIFS", "DAYS", "ISFORMULA",
              "FORMULATEXT", "SHEET", "SHEETS", "ISOMITTED"}
    _prefixed = found
    return found


def add_prefixes(repo, text):
    names = prefixed_functions(repo)
    pieces = []
    for is_text, part in outside_quotes(text):
        if not is_text:
            part = re.sub(r"(?<![A-Za-z0-9_.])([A-Z][A-Z0-9.]*)\(",
                          lambda m: ("_xlfn." if m.group(1) in names else "") + m.group(1) + "(", part)
        pieces.append(part)
    return "".join(pieces)


def template_for(family, nargs):
    if nargs in family["templates"]:
        return family["templates"][nargs]
    return "%s(%s)" % (family["fn"], ",".join("{%s}" % "ABCEFG"[k] for k in range(nargs)))


# --- choosing the rows -------------------------------------------------------------------------------


def stride_pick(items, count, seed):
    if count >= len(items):
        return list(items)
    rng = random.Random(seed)
    step = len(items) / count
    offset = rng.random() * step
    picks, last = [], -1
    for k in range(count):
        index = min(len(items) - 1, int(offset + k * step))
        if index <= last:
            index = last + 1
        if index >= len(items):
            break
        picks.append(items[index])
        last = index
    return picks


def choose(family, cases):
    """The rows of the family: a seeded stride over its classes up to the quota, plus boundary rows."""
    classes = collections.OrderedDict()
    for case in cases:
        classes.setdefault((case["source"], case["tag"]), []).append(case)
    quota = family["quota"]
    remaining = quota
    shares = {}
    ordered = sorted(classes.items(), key=lambda kv: len(kv[1]))
    left = len(ordered)
    for key, members in ordered:
        share = min(len(members), max(1, remaining // left))
        shares[key] = share
        remaining -= share
        left -= 1
    chosen, seen = [], set()
    for key, members in classes.items():
        for case in stride_pick(members, shares[key], "%s|%s|%s|%s" % (SEED, family["name"], key[0], key[1])):
            chosen.append(case)
            seen.add(id(case))
    # A class whose id tag ends in "boundary" is all boundary rows when it is small; a large one is
    # sampled like any other class.
    sizes = {key: len(members) for key, members in classes.items()}
    boundary = [c for c in cases if id(c) not in seen and (
        c["boundary"] or (c.get("named_boundary") and sizes[(c["source"], c["tag"])] <= family["quota"]))]
    chosen.extend(boundary)
    return chosen, len(boundary), len(classes)


def row_text(family, case, repo):
    formula = add_prefixes(repo, case["formula"] or template_for(family, case["nargs"]))
    fields = [formula]
    for token in case["args"]:
        kind, body = token[0], token[1]
        fields.append("-" if kind == "-" else "%s:%s" % (kind, body))
    return "\t".join(fields)


def stored_formula(text, row):
    cells = {"{%s}" % c: "%s%d" % (c, row) for c in "ABCEFG"}
    for key, value in cells.items():
        text = text.replace(key, value)
    return "=" + text.replace("_xlfn.", "")


READERS = {"hex": read_hex_source, "typed": read_typed_source, "pair": read_pair_source}


def expand_families(batch, legacy):
    """The batch's families with their usable cases: (family, cases, skipped, hashes)."""
    out = []
    for declared in batch["families"]:
        skipped = collections.Counter()
        cases, hashes = [], {}
        for rel, shape in declared["sources"]:
            if shape == "dir":
                names = sorted(n for n in os.listdir(evidence_path(legacy, rel))
                               if n.startswith("answers-") and n.endswith(".json"))
                found = [(rel + "/" + n, "hex") for n in names]
            else:
                found = [(rel, shape)]
            for one, kind in found:
                got, digest = READERS[kind](legacy, one, declared, skipped)
                hashes[one] = digest
                cases.extend(got)
        if declared["multi"]:
            groups = collections.OrderedDict()
            for case in cases:
                groups.setdefault(case["function"], []).append(case)
            for index, (function, members) in enumerate(groups.items()):
                family = dict(declared, name="%s %s" % (declared["name"], function), fn=function, multi=False)
                notes = skipped if index == 0 else collections.Counter()
                if declared["keep"]:
                    kept = [c for c in members if declared["keep"](c)]
                    if len(kept) != len(members):
                        notes["withheld by the legacy replay"] += len(members) - len(kept)
                    members = kept
                out.append((family, members, notes, hashes))
        else:
            if declared["keep"]:
                kept = [c for c in cases if declared["keep"](c)]
                if len(kept) != len(cases):
                    skipped["withheld by the legacy replay"] += len(cases) - len(kept)
                cases = kept
            out.append((declared, cases, skipped, hashes))
    return out


def build_rows(batch, legacy, repo):
    lines, cases_out, report, hashes = [], [], [], {}
    total_skipped = collections.Counter()
    for family, cases, skipped, family_hashes in expand_families(batch, legacy):
        hashes.update(family_hashes)
        chosen, boundary_extra, classes = choose(family, cases)
        # the same inputs read twice with different answers stay as two rows (a legacy contradiction)
        unique, seen_rows = [], {}
        for case in chosen:
            key = row_text(family, case, repo)
            if key in seen_rows:
                if seen_rows[key] != case["expected"]:
                    unique.append(case)
                    seen_rows[key + "#" + case["id"]] = case["expected"]
                continue
            seen_rows[key] = case["expected"]
            unique.append(case)
        lines.append("#")
        lines.append("# family %s: %d rows of %d usable readings in %d classes (%d boundary rows beyond the stride); "
                     "%s" % (family["name"], len(unique), len(cases), classes, boundary_extra,
                             "left out: " + ", ".join("%s %d" % kv for kv in sorted(skipped.items())) if skipped else "none left out"))
        lines.append("# sources: " + ", ".join(sorted({c["source"] for c in unique})))
        for case in unique:
            lines.append(row_text(family, case, repo))
            extras = {str(k): t[2] for k, t in enumerate(case["args"]) if len(t) == 3}
            cases_out.append(dict(family=family["name"], id=case["id"], source=case["source"], expected=case["expected"],
                                  line=row_text(family, case, repo), extras=extras))
        report.append((family["name"], len(unique), len(cases), classes, boundary_extra, dict(skipped)))
        total_skipped.update(skipped)
    return lines, cases_out, report, hashes


def hand_rows(batch, repo, work):
    """Cases written by hand from OxFunc unit tests (jnt_hand.py): ask Excel through the question service."""
    sys.path.insert(0, os.path.join(repo, "tools/excel-service"))
    import jnt_hand
    from client import Client, number, text, boolean, blank, error
    client = Client()
    wire_of_literal = {v: k for k, v in ERROR_LITERAL_WIRE.items()}
    lines, cases, contradicted, builds, envs = [], [], [], set(), set()
    for function, args, legacy, source in jnt_hand.CASES[batch["id"]]:
        if function.startswith("="):
            # An operator formula: the question service takes function calls only, so the answer is
            # live Excel's read-back of the sheet (capture mode, --live).
            line = function[1:]
            hypothesis = None
            if legacy is not None:
                hypothesis = ("number", "%016x" % struct.unpack(">Q", struct.pack(">d", legacy[1]))[0]) if legacy[0] == "number" else legacy
            lines.append(line)
            cases.append(dict(family="operators", id=source, source=source, expected=None, legacy=hypothesis, line=line, extras={}))
            continue
        tokens, parts, asked = [], [], []
        for arg in args:
            kind = arg[0]
            if kind == "s":
                parts.append('"' + arg[1].replace('"', '""') + '"')
                asked.append(text(arg[1], literal=True))
                continue
            parts.append("{%s}" % "ABCEFG"[len(tokens)])
            if kind == "n":
                tokens.append("n:%016x" % struct.unpack(">Q", struct.pack(">d", arg[1]))[0])
                asked.append(number(arg[1]))
            elif kind == "b":
                tokens.append("b:" + ("TRUE" if arg[1] else "FALSE"))
                asked.append(boolean(arg[1]))
            elif kind == "e":
                tokens.append("e:" + arg[1])
                asked.append(error(arg[1]))
            else:
                tokens.append("-")
                asked.append(blank())
        formula = add_prefixes(repo, "%s(%s)" % (function, ",".join(parts)))
        line = "\t".join([formula] + tokens)
        reply = client.ask(function, [asked], priority="interactive")
        answer = reply["rows"][0]
        builds.add(str(reply.get("excel_build")))
        envs.add(str(reply.get("env_id")))
        if answer["type"] == "number":
            expected = ("number", answer["bits"])
        elif answer["type"] == "error":
            expected = ("error", wire_of_literal[answer["code"]])
        elif answer["type"] == "boolean":
            expected = ("logical", bool(answer["value"]))
        else:
            sys.exit("the service did not answer %s: %r" % (line, answer))
        lines.append(line)
        cases.append(dict(family=function, id=source, source=source, expected=expected, line=line, extras={}))
        if legacy is not None:
            hypothesis = None
            if legacy[0] == "number":
                hypothesis = ("number", "%016x" % struct.unpack(">Q", struct.pack(">d", legacy[1]))[0])
            elif legacy[0] == "error":
                hypothesis = ("error", legacy[1])
            agrees = True
            if legacy[0] == "approx":
                got = struct.unpack(">d", bytes.fromhex(expected[1]))[0] if expected[0] == "number" else None
                agrees = got is not None and abs(got - legacy[1]) < 1e-12
            else:
                agrees = hypothesis == expected
            if not agrees:
                contradicted.append(dict(formula=line.split("\t")[0], inputs=line.split("\t")[1:], source=source,
                                         legacy=list(legacy), excel=list(expected), excel_build=sorted(builds)[0],
                                         reason="the legacy unit-test expectation differs from Excel's answer"))
    if builds:
        entry = "OxFunc@05196c39:crates/oxfunc_core/src/functions unit tests (hypotheses); answers: question service touchstone/1, Excel 16.0.%s, env %s;sha256:%s" % (
            "/".join(sorted(builds)), "/".join(sorted(envs)), hashlib.sha256("\n".join(lines).encode()).hexdigest())
    else:
        entry = "OxFunc@05196c39:crates/oxfunc_core/src/functions unit tests (hypotheses); answers: live Excel read-back of the sheet;sha256:%s" % (
            hashlib.sha256("\n".join(lines).encode()).hexdigest())
    return lines, cases, contradicted, entry


def hand_header(batch):
    if batch["id"] == "J11":
        return [
            "# %s %s: %s." % (batch["bead"], batch["id"], batch["title"]),
            "# Ported from OxFunc@05196c39:crates/oxfunc_core/src/functions/{%s} (unit tests)." % ",".join(batch["tests"]),
            "# Each row is an operator formula a legacy unit test asserts. The legacy expectation is a hypothesis: the",
            "# answer in the section capture is live Excel's read-back of the row (the question service takes function",
            "# calls only); a difference is listed in contradicted-%s.json." % batch["sheet"],
        ]
    return [
        "# %s %s: %s." % (batch["bead"], batch["id"], batch["title"]),
        "# Ported from OxFunc@05196c39:crates/oxfunc_core/src/functions/{%s} (unit tests)." % ",".join(batch["tests"]),
        "# Each row is a call a legacy unit test asserts. The legacy expectation is a hypothesis: the answer in the",
        "# section capture is Excel's, asked through the question service (tools/excel-service) with the build",
        "# recorded in the capture entry; a difference is listed in contradicted-%s.json." % batch["sheet"],
        "# Numbers are exact cells (typed inputs); text arguments stay literals in the formula.",
    ]


def header(batch):
    tests = ", ".join(batch["tests"])
    return [
        "# %s %s: %s." % (batch["bead"], batch["id"], batch["title"]),
        "# Ported from OxFunc@05196c39:%s (%s)." % (LEGACY_TESTS, tests),
        "# Each row is a legacy Excel reading: the formula reads its exact binary64 inputs from columns A to C of",
        "# its row (make-catalogue-fixtures.ps1, typed inputs) and the answer is the bit pattern the legacy suite",
        "# replayed. Excel %s. The answers are in the section capture, imported by" % EXCEL_BUILD,
        "# tools/excel-service/jnt_import.py; none is computed here. A row DNA Calc does not match stays failing.",
    ]


def utf16_hex(text):
    data = text.encode("utf-16-le", "surrogatepass")
    return "".join("%04x" % unit for unit in struct.unpack("<%dH" % (len(data) // 2), data))


def expected_of_token(token):
    """A live read-back token (n:, e:, s:, b:) as an expectation."""
    kind, body = token[0], token[2:]
    if kind == "n":
        return ("number", body)
    if kind == "e":
        return ("error", {v: k for k, v in ERROR_LITERAL_WIRE.items()}[body])
    if kind == "s":
        return ("text_hex", body)
    if kind == "b":
        return ("logical", body == "TRUE")
    raise ValueError(token)


ERROR_LITERAL_WIRE = {"null": "#NULL!", "div0": "#DIV/0!", "value": "#VALUE!", "ref": "#REF!", "name": "#NAME?",
                      "num": "#NUM!", "na": "#N/A"}


def token_of(expected):
    kind, value = expected
    if kind == "number":
        return "n:" + value
    if kind == "error":
        return "e:" + ERROR_LITERAL_WIRE[value]
    if kind == "text":
        return "s:" + utf16_hex(value)
    return "b:" + ("TRUE" if value else "FALSE")


def observation(sheet, row, expected):
    kind, value = expected
    if kind == "text_hex":
        raw = {"text": value}
    elif kind == "number":
        raw = {"number": value}
    elif kind == "error":
        raw = {"error": value}
    elif kind == "text":
        raw = {"text": utf16_hex(value)}
    elif kind == "logical":
        raw = {"logical": value}
    else:
        raise ValueError(kind)
    return {"sheet": sheet, "row": row, "column": 4, "excel_raw": raw}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["rows", "capture"])
    parser.add_argument("batch")
    parser.add_argument("--repo", required=True)
    parser.add_argument("--legacy", required=True)
    parser.add_argument("--work", default=None)
    parser.add_argument("--live", default=None, help="the live Excel read-back (columns r, A, B, C, D, E, F, G) of the sheet")
    parser.add_argument("--build", default=None, help="the Excel build of that read-back")
    arguments = parser.parse_args()
    batch = dict(BATCHES[arguments.batch])
    batch["id"] = arguments.batch
    global SKIP_ARRAY_FORMS
    SKIP_ARRAY_FORMS = batch.get("skip_array_forms", False)
    work = arguments.work or os.path.expanduser("~/codex-runs/jnt1/work")
    os.makedirs(work, exist_ok=True)
    fixtures = os.path.join(arguments.repo, "crates/calc-func/tests/fixtures/jnt")
    section = os.path.join(fixtures, "sheet-%s.txt" % batch["sheet"])
    hand = batch.get("hand", False)
    if hand and arguments.mode == "rows":
        lines, cases, contradicted, entry = hand_rows(batch, arguments.repo, work)
        with open(section, "w", encoding="utf-8", newline="\n") as handle:
            handle.write("\n".join(hand_header(batch) + lines) + "\n")
        with open(os.path.join(work, batch["sheet"] + ".cases.json"), "w") as handle:
            json.dump(cases, handle)
        with open(os.path.join(work, batch["sheet"] + ".meta.json"), "w") as handle:
            json.dump({"entry": entry}, handle)
        path_contra = os.path.join(fixtures, "contradicted-%s.json" % batch["sheet"])
        if contradicted:
            with open(path_contra, "w") as handle:
                json.dump(contradicted, handle, indent=0)
                handle.write("\n")
        elif os.path.exists(path_contra):
            os.remove(path_contra)
        print("%s: %d rows asked of Excel, %d legacy expectations Excel contradicted; %s" % (
            batch["sheet"], len(cases), len(contradicted), entry[:160]))
        return
    if hand:
        lines, cases, report, hashes = [], [], [], {}
    else:
        lines, cases, report, hashes = build_rows(batch, arguments.legacy, arguments.repo)
    if arguments.mode == "rows":
        with open(section, "w", encoding="utf-8", newline="\n") as handle:
            handle.write("\n".join(header(batch) + lines) + "\n")
        with open(os.path.join(work, batch["sheet"] + ".cases.json"), "w") as handle:
            json.dump(cases, handle)
        for name, rows, usable, classes, extra, skipped in report:
            print("%-16s rows %4d  usable %7d  classes %2d  boundary extra %3d  left out %s" % (
                name, rows, usable, classes, extra, skipped))
        print("section %s: %d rows" % (section, len(cases)))
        return
    # capture: the whole-workbook question capture of the catalogue, rows of this sheet only
    workbook_path = os.path.join(fixtures, "oracle.xlsx")
    with open(workbook_path, "rb") as handle:
        workbook = handle.read()
    digest = hashlib.sha256(workbook).hexdigest()
    saved = json.load(open(os.path.join(work, batch["sheet"] + ".cases.json")))
    actual = [line for line in open(section, encoding="utf-8").read().split("\n") if line and not line.startswith("#")]
    if [c["line"] for c in saved] != actual:
        sys.exit("the section file is not the rows this tool chose: run `rows` again")
    sheet = batch["sheet"]
    if hand:
        entry = json.load(open(os.path.join(work, batch["sheet"] + ".meta.json")))["entry"]
    else:
        entry = "OxFunc@05196c39:%s (%s);sha256:%s" % (
            EVIDENCE.rstrip("/"), "Excel " + EXCEL_BUILD,
            hashlib.sha256("".join(k + hashes[k] for k in sorted(hashes)).encode()).hexdigest())
    formulas, observations, contradicted = [], [], []
    live = None
    if arguments.live:
        if any(c["expected"] is None for c in saved):
            entry += "; operator rows answered by live Excel build " + (arguments.build or "unknown")
        live = [line.rstrip("\n").split("\t") for line in open(arguments.live, encoding="utf-8")]
        if any(c.get("extras") for c in saved):
            entry += "; input formulas read back by live Excel build " + (arguments.build or "unknown")
    for index, case in enumerate(saved, start=1):
        formula_text = case["line"].split("\t")[0]
        formulas.append({"sheet": sheet, "row": index, "column": 4, "kind": "ordinary",
                         "formula": stored_formula(formula_text, index)})
        if case["expected"] is None:
            if live is None:
                sys.exit("row %d is answered by live Excel: give --live and --build" % index)
            seen = live[index - 1][1 + 3]
            expected = expected_of_token(seen)
            hypothesis = case.get("legacy")
            if hypothesis is not None and token_of(tuple(hypothesis)) != seen:
                contradicted.append({"row": index, "formula": case["line"], "legacy": token_of(tuple(hypothesis)), "live": seen,
                                     "source": case["source"], "legacy_build": "none (unit-test hypothesis)",
                                     "live_build": arguments.build or "unknown",
                                     "reason": "the legacy unit-test expectation differs from Excel's answer"})
            observations.append(observation(sheet, index, expected))
            continue
        expected = tuple(case["expected"])
        if live is not None:
            # Excel is the oracle: where live Excel reads another answer than the legacy reading, the row
            # carries live Excel's answer and the disagreement is recorded (contradicted-<sheet>.json).
            seen = live[index - 1][1 + 3]
            if seen != token_of(expected):
                formula_head = case["line"].split("\t")[0]
                if formula_head.startswith("DATE(") and seen.startswith("e:") and expected[0] == "number":
                    reason = "version difference: a number under Excel 20430 and an error under 20527; recorded in crates/calc-func/witness-differences.tsv"
                elif formula_head.startswith("LEN(") or "text slice" in case["family"]:
                    reason = "legacy harness mangling: a surrogate pair counted as one unit; Excel counts UTF-16 units"
                else:
                    reason = "unclassified: live Excel reads another answer than the legacy reading"
                contradicted.append({"row": index, "family": case["family"], "id": case["id"], "formula": formula_head,
                                     "inputs": case["line"].split("\t")[1:], "legacy": token_of(expected), "live": seen,
                                     "legacy_build": "16.0.20430", "live_build": arguments.build or "unknown", "reason": reason})
                expected = expected_of_token(seen)
        observations.append(observation(sheet, index, expected))
        # Inputs written as formulas (text with control characters): the answer of those cells is what
        # live Excel read back, and a read-back that is not the intended text fails the import.
        for k, text in case.get("extras", {}).items():
            if live is None:
                sys.exit("row %d has input formulas: give --live and --build" % index)
            column = [1, 2, 3, 5, 6, 7][int(k)]
            body = case["line"].split("\t")[1 + int(k)][2:]
            got = live[index - 1][1 + [0, 1, 2, 4, 5, 6][int(k)]]
            if got != "s:" + utf16_hex(text):
                sys.exit("row %d column %d: Excel read back %s, not the intended text %r" % (index, column, got, text))
            formulas.append({"sheet": sheet, "row": index, "column": column, "kind": "ordinary", "formula": "=" + body})
            observations.append({"sheet": sheet, "row": index, "column": column,
                                 "excel_raw": {"text": got[2:]}})
    if contradicted:
        entry += "; %d rows carry live Excel's answer where the legacy reading differs (contradicted-%s.json)" % (len(contradicted), sheet)
    path_contra = os.path.join(fixtures, "contradicted-%s.json" % sheet)
    if contradicted:
        with open(path_contra, "w") as handle:
            json.dump(contradicted, handle, indent=0)
            handle.write("\n")
        print("wrote", path_contra, len(contradicted))
    elif os.path.exists(path_contra) and not hand:
        os.remove(path_contra)
    record = {"format": "dnacalc-capture-record", "version": 3, "case": "jnt__" + sheet,
              "source": "sha256:" + digest,
              "acquisition": {"kind": "function_questions", "entry": entry},
              "environment": None,
              "sheets": [{"name": sheet, "visibility": "visible"}],
              "formulas": formulas, "observations": observations}
    out = os.path.join(work, sheet + ".whole.capture.json")
    with open(out, "w") as handle:
        json.dump(record, handle, indent=1)
    print("wrote", out, len(formulas), "rows; workbook sha256", digest)


if __name__ == "__main__":
    main()
