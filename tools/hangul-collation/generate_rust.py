#!/usr/bin/env python3
"""Writes crates/calc-func/src/catalogue/comparison/hangul_collation.rs from the Windows sort keys
that dump-keys.ps1 measured.

    python3 generate_rust.py <directory holding single.txt, syllables.txt and iteration.txt> > hangul_collation.rs

Each input line is a code unit in hexadecimal and the key Windows gave it (hexadecimal bytes).
The conjoining jamo keys are a primary and three extension bytes (FF l FF v FF t); a syllable's is
its primary alone; a compatibility jamo's is its primary, then the sections of the key (its case
weight is the byte after the second 01).
"""
import sys


def read(path):
    table = {}
    with open(path) as handle:
        for line in handle:
            code, key = line.split()
            table[int(code, 16)] = bytes.fromhex(key)
    return table


def is_conjoining(code):
    return (0x1100 <= code <= 0x11FF or 0xA960 <= code <= 0xA97C
            or 0xD7B0 <= code <= 0xD7C6 or 0xD7CB <= code <= 0xD7FB)


def main(directory):
    single = read(directory + "/single.txt")
    syllables = read(directory + "/syllables.txt")
    iteration = read(directory + "/iteration.txt")
    jamo = {}
    for code, key in single.items():
        if is_conjoining(code):
            assert key[2] == 0xFF and key[4] == 0xFF and key[6] == 0xFF, hex(code)
            jamo[code] = (int.from_bytes(key[0:2], "big"), key[3], key[5], key[7])
    primaries = [int.from_bytes(syllables[c][:2], "big") for c in range(0xAC00, 0xD7A4)]
    runs = [(0, primaries[0])]
    runs += [(i, primaries[i]) for i in range(1, len(primaries)) if primaries[i] - primaries[i - 1] != 1]
    out = ["""// Measured Hangul sort-key weights (bead dc-8wp, Hangul jamo collation): what Windows' sort keys
// (LCMapStringEx with LCMAP_SORTKEY and NORM_IGNORECASE under en-ZA, NLS version 0x00060502) give the
// Hangul syllables, the compatibility jamo and the conjoining jamo, as measured on the machine
// Excel was measured on. No Windows table is copied: each entry is one measured fact.
// Written by tools/hangul-collation/generate_rust.py from the keys dump-keys.ps1 dumps.
//
// SYLLABLE_RUNS: the primary of the first syllable of each run of syllables whose primaries
// are consecutive: (index from U+AC00, primary).
// JAMO: each conjoining jamo: (code unit, primary, leading, vowel, trailing extension weights).
// COMPAT: each compatibility jamo: (code unit, primary, case weight).
"""]
    out.append("#[rustfmt::skip]")
    out.append("pub(super) const SYLLABLE_RUNS: [(u16, u16); %d] = [" % len(runs))
    out.append("    " + ", ".join("(%d, 0x%04X)" % run for run in runs) + ",")
    out.append("];\n")
    ordered = sorted(jamo.items())
    out.append("#[rustfmt::skip]")
    out.append("pub(super) const JAMO: [(u16, u16, u8, u8, u8); %d] = [" % len(ordered))
    for code, (p, l, v, t) in ordered:
        out.append("    (0x%04X, 0x%04X, 0x%02X, 0x%02X, 0x%02X)," % (code, p, l, v, t))
    out.append("];\n")
    compat = [(c, int.from_bytes(k[:2], "big"), k[4]) for c, k in sorted(single.items())
              if (0x3131 <= c <= 0x318E or 0xFFA0 <= c <= 0xFFDC) and k[:2] != b"\x01\x01"]
    out.append("#[rustfmt::skip]")
    out.append("pub(super) const COMPAT: [(u16, u16, u8); %d] = [" % len(compat))
    for entry in compat:
        out.append("    (0x%04X, 0x%04X, 0x%02X)," % entry)
    out.append("];")
    out.append("")
    out.append("// ITERATION: the second byte of the primary (04xx) an iteration or prolonged-sound mark takes after")
    out.append("// each conjoining jamo (the same for every such mark).")
    out.append("#[rustfmt::skip]")
    out.append("pub(super) const ITERATION: [(u16, u8); %d] = [" % len(ordered))
    for code, _ in ordered:
        key = iteration[code]
        assert key[8] == 0x04, hex(code)
        out.append("    (0x%04X, 0x%02X)," % (code, key[9]))
    out.append("];")
    sys.stdout.write("\n".join(out) + "\n")


main(sys.argv[1])
