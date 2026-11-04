"""Live Linux proof of the f1bf1f02 ERF.PRECISE stored-denominator edge."""
import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import socket
import time

from client import Client, bits, from_bits, number


def round64(value):
    """Round a positive rational to the x87 register's 64 significant bits."""
    exponent = value.numerator.bit_length() - value.denominator.bit_length()
    if value < Fraction(2) ** exponent:
        exponent -= 1
    unit = Fraction(2) ** (exponent - 63)
    quotient, remainder = divmod(value / unit, 1)
    if remainder > Fraction(1, 2) or (remainder == Fraction(1, 2) and int(quotient) % 2):
        quotient += 1
    return quotient * unit


def constant_denominator(input_bits, denominator_bits):
    # This diagnostic hypothesis is used only to classify a live reading. It is
    # never sent as an Excel formula, nor substituted for any observed answer.
    x = Fraction.from_float(from_bits(input_bits))
    n0 = Fraction.from_float(from_bits("3ff1df5e70c7956b"))
    product = Fraction.from_float(float(round64(x * n0)))
    d0 = Fraction.from_float(from_bits(denominator_bits))
    return bits(float(round64(product / d0)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    client = Client()
    record = {"machine": socket.gethostname(), "reference_sha256": hashlib.sha256(options.reference.read_bytes()).hexdigest(),
              "hypothesis_source": "DnaCalc f1bf1f02; normal_dist_common.rs ERF_N0 and ERF_D0"}
    reference = json.loads(options.reference.read_text())["witnesses"][:32]
    start = time.monotonic()
    reply = client.ask("ERF.PRECISE", [[number(row["args"][0])] for row in reference])
    record["sample_seconds"] = time.monotonic() - start
    mismatches = [{"reference": old, "observed": new} for old, new in zip(reference, reply["rows"])
                  if old["expected_bits"].removeprefix("0x") != new.get("bits")]
    record["sample"] = {"count": len(reference), "matched": len(reference) - len(mismatches), "mismatches": mismatches,
                        "response": reply}
    start = time.monotonic()
    batch = client.consecutive_run("ERF.PRECISE", "3e47bc15e9263000", 1000)
    record["batch_seconds"] = time.monotonic() - start
    record["batch"] = batch

    def denominator_stepped(input_bits, answer):
        before = constant_denominator(input_bits, "3fefada15778ece6")
        after = constant_denominator(input_bits, "3fefada15778ece7")
        if answer.get("bits") not in (before, after):
            raise ValueError("reading does not distinguish the two denominator hypotheses")
        if before == after:
            return None
        return answer["bits"] == after

    start = time.monotonic()
    edge = client.bisect_edge("ERF.PRECISE", "3e47bc15e9263000", "3e47bc15e9263400", denominator_stepped)
    record["bisect_seconds"] = time.monotonic() - start
    record["edge"] = edge
    record["edge_matches_recorded"] = (edge["low"], edge["high"]) == ("3e47bc15e9263272", "3e47bc15e9263273")
    record["ok"] = not mismatches and record["edge_matches_recorded"] and record["batch_seconds"] < 60
    options.output.write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({k: v for k, v in record.items() if k not in ("sample", "batch")}, indent=2), flush=True)
    if not record["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
