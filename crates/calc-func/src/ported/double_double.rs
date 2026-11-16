//! Double-double arithmetic (a value `hi + lo`, `|lo| <= ulp(hi) / 2`) for the few kernels whose
//! Excel answer is the correctly rounded value of a quantity binary64 alone cannot reach: the
//! central inverse-erf ratio of NORM.S.INV and the polished roots of the gamma inverses.
//! Error-free transformations after Knuth (two-sum) and Dekker (the product through `mul_add`).

#[derive(Clone, Copy, Debug)]
pub(crate) struct Dd(pub f64, pub f64);

impl Dd {
    pub(crate) fn from(x: f64) -> Self {
        Dd(x, 0.0)
    }

    pub(crate) fn to_f64(self) -> f64 {
        self.0 + self.1
    }

    pub(crate) fn neg(self) -> Dd {
        Dd(-self.0, -self.1)
    }

    pub(crate) fn add(self, b: Dd) -> Dd {
        let s = self.0 + b.0;
        let bb = s - self.0;
        let err = (self.0 - (s - bb)) + (b.0 - bb);
        let lo = err + self.1 + b.1;
        let hi = s + lo;
        Dd(hi, lo - (hi - s))
    }

    pub(crate) fn sub(self, b: Dd) -> Dd {
        self.add(b.neg())
    }

    pub(crate) fn mul(self, b: Dd) -> Dd {
        let p = self.0 * b.0;
        let err = self.0.mul_add(b.0, -p);
        let lo = err + (self.0 * b.1 + self.1 * b.0);
        let hi = p + lo;
        Dd(hi, lo - (hi - p))
    }

    pub(crate) fn mul_f64(self, b: f64) -> Dd {
        self.mul(Dd::from(b))
    }

    pub(crate) fn div_f64(self, d: f64) -> Dd {
        let q = self.0 / d;
        let p = q * d;
        let perr = q.mul_add(d, -p);
        let r = ((self.0 - p) - perr) + self.1;
        let lo = r / d;
        let hi = q + lo;
        Dd(hi, lo - (hi - q))
    }

    pub(crate) fn div(self, d: Dd) -> Dd {
        let q1 = self.0 / d.0;
        let r = self.sub(d.mul_f64(q1));
        let q2 = r.0 / d.0;
        let r = r.sub(d.mul_f64(q2));
        let q3 = r.0 / d.0;
        Dd::from(q1).add(Dd::from(q2)).add(Dd::from(q3))
    }
}

const LN2: Dd = Dd(0.6931471805599453, 2.3190468138462996e-17);

/// `exp(x)` to about 1e-31 relative for `|x| < 700`.
pub(crate) fn exp(x: Dd) -> Dd {
    let k = (x.0 / LN2.0).round();
    let r = x.sub(LN2.mul_f64(k)).div_f64(1024.0);
    // Taylor series of exp(r) - 1, |r| < 3.4e-4.
    let mut term = r;
    let mut sum = r;
    for n in 2..16 {
        term = term.mul(r).div_f64(n as f64);
        sum = sum.add(term);
        if term.0.abs() < 1e-34 {
            break;
        }
    }
    // (1 + sum)^1024 by ten squarings of the form (1 + e)^2 = 1 + (2e + e^2).
    for _ in 0..10 {
        sum = sum.add(sum).add(sum.mul(sum));
    }
    let y = sum.add(Dd::from(1.0));
    let scale = 2.0_f64.powi(k as i32);
    Dd(y.0 * scale, y.1 * scale)
}

/// `ln(x)` for positive `x`: one Newton step from the binary64 logarithm.
pub(crate) fn ln(x: Dd) -> Dd {
    let y0 = libm::log(x.0);
    let e = x.mul(exp(Dd::from(-y0)));
    Dd::from(y0).add(e.sub(Dd::from(1.0)))
}
