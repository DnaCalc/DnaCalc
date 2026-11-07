# Hangul collation measurements

How `crates/calc-func/src/catalogue/comparison/hangul_collation.rs` and `hangul_pairs.tsv` were
measured (bead dc-8wp). Windows only; nothing here runs in a build or a test.

1. `dump-keys.ps1 -OutDir <dir>` writes the sort keys Windows gives the Hangul code units
   (`LCMapStringEx`, `LCMAP_SORTKEY`, `NORM_IGNORECASE`, en-ZA): `single.txt` (conjoining jamo,
   compatibility jamo), `syllables.txt` and `iteration.txt` (each jamo followed by U+30FC).
2. `python3 generate_rust.py <dir> > ../../crates/calc-func/src/catalogue/comparison/hangul_collation.rs`
   turns them into the tables the collation reads.
3. `random-pairs.ps1 -Count N -Seed S -Out pairs.tsv` writes random pairs of texts with the order
   `CompareStringEx` gives them; `hangul_pairs.tsv` holds six thousand pairs (seed 11) of jamo,
   syllables, compatibility jamo and the units that end a run of jamo, and three thousand more
   (seed 31, with -Context set to the iteration and prolonged-sound marks).

The test `hangul_texts_order_as_windows_orders` compares the collation with those pairs.
`nls.ps1` holds the two Windows calls.

What the keys showed, as the collation uses it: a run of conjoining jamo is one primary and three
extension bytes (leading, vowel, trailing), some neighbours join (see `jamo_cluster`), and the
syllables, compatibility jamo and jamo share one scale of primaries. Windows orders two texts by
their weights in turn and compares the extension bytes of two weights that start at the same
offset. Pairs where an ignorable unit or a mark leads both texts and different jamo follow are
the ones not modelled (two in six thousand, about one in five hundred of a harder mix).
