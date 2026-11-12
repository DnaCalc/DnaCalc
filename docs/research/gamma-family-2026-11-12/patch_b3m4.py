# Write a B3 table (json list of 18 hex bits, P then Q) into gammaln.rs under graph G4:
# every Horner product and sum stored, P's last sum stored, Q's last sum kept in the register.
import json, sys, re
tab = [int(h, 16) for h in json.load(open(sys.argv[1]))]
p = "crates/calc-func/src/ported/excel_numeric/gammaln.rs"
s = open(p).read()
def block(name, vals):
    body = "".join("    0x%04X_%04X_%04X_%04X,\n" % ((v >> 48) & 0xffff, (v >> 32) & 0xffff, (v >> 16) & 0xffff, v & 0xffff) for v in vals)
    return "const %s: [u64; 9] = [\n%s];" % (name, body)
s, n1 = re.subn(r"const B3_P: \[u64; 9\] = \[\n.*?\];", block("B3_P", tab[:9]), s, flags=re.S)
s, n2 = re.subn(r"const B3_Q: \[u64; 9\] = \[\n.*?\];", block("B3_Q", tab[9:]), s, flags=re.S)
assert n1 == 1 and n2 == 1
old_chain = '''    let (num, den) = (chain(&B3_P), chain(&B3_Q));
    let r = F::store(&F::div(&num, &den, CW), CW);'''
new_chain = '''    // The numerator's last sum is stored, the denominator's stays in the register (E1, 2026-11-12).
    let (num, den) = (ext(F::store(&chain(&B3_P), CW)), chain(&B3_Q));
    let r = F::store(&F::div(&num, &den, CW), CW);'''
assert old_chain in s
s = s.replace(old_chain, new_chain, 1)
old_pin = '''    // The top of the band, as Excel gives it: one ulp above stored ln 2. It is read, because the
    // table below is two ulps above it there, and GAMMA.DIST's shape 2 takes it in every prefactor.
    if x == 3.0 {
        return f64::from_bits(0x3FE6_2E42_FEFA_39F0);
    }
'''
assert old_pin in s
s = s.replace(old_pin, "", 1)
open(p, "w").write(s)
print("written")
