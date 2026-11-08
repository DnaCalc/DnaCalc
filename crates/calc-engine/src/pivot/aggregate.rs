use super::*;

#[derive(Default)]
pub(super) struct Total {
    sum: f64,
    squares: f64,
    product: f64,
    unsafe_moment: bool,
    unsafe_integer_product: bool,
    has_zero: bool,
    numbers: u64,
    nonempty: u64,
    min: Option<f64>,
    max: Option<f64>,
    error: Option<WorksheetErrorCode>,
}
impl Total {
    pub(super) fn push(&mut self, value: &PivotItem) -> Result<(), PivotIssue> {
        let value = item_value(value)?;
        match value.core_ref() {
            calc_value::CoreRef::Number(n) => {
                self.sum += n;
                self.squares += n * n;
                self.product = if self.numbers == 0 { n } else { self.product * n };
                self.has_zero |= n == 0.0;
                self.unsafe_integer_product |= n.fract() != 0.0 || !self.product.is_finite()
                    || self.product.abs() > 9_007_199_254_740_992.0;
                self.unsafe_moment |= !n.is_finite() || n.abs() > 67_108_864.0 || (n * 4.0).fract() != 0.0;
                self.numbers += 1;
                self.nonempty += 1;
                self.min = Some(self.min.map_or(n, |a| a.min(n)));
                self.max = Some(self.max.map_or(n, |a| a.max(n)));
            }
            calc_value::CoreRef::Error(e) => {
                self.nonempty += 1;
                self.error.get_or_insert(e);
            }
            calc_value::CoreRef::Text(_) | calc_value::CoreRef::Logical(_) => self.nonempty += 1,
            _ => {}
        }
        Ok(())
    }
    /// The captured formulas below are admitted only where quarter-unit inputs and the raw
    /// moments fit exactly in binary64. Other numeric inputs need more Excel measurements.
    pub(super) fn supports(&self, kind: &str) -> bool {
        if !matches!(kind, "product" | "stdDev" | "stdDevp" | "var" | "varp") || self.error.is_some() {
            return true;
        }
        if self.numbers == 0 || self.unsafe_moment { return false; }
        if kind == "product" {
            return self.product.is_finite() && (self.product != 0.0 || self.has_zero);
        }
        let n = self.numbers as f64;
        if self.squares * 16.0 * n > 9_007_199_254_740_992.0 { return false; }
        let square_total = self.sum * self.sum;
        let numerator = if matches!(kind, "stdDev" | "var") {
            self.squares - square_total / n
        } else {
            n * self.squares - square_total
        };
        numerator.is_finite() && numerator >= 0.0
    }
    pub(super) fn source_error(&self) -> Option<WorksheetErrorCode> {
        self.error
    }
    pub(super) fn supports_grouped_product(&self, kind: &str) -> bool {
        kind != "product" || self.error.is_some() || !self.unsafe_integer_product
    }
    pub(super) fn result(&self, kind: &str) -> CalcValue {
        // UI28 P6 and M1 err-keys: an empty set has no displayed sum or count cell.
        if matches!(kind, "sum" | "count" | "countNums") && self.nonempty == 0 {
            return CalcValue::empty();
        }
        if !matches!(kind, "count" | "countNums")
            && let Some(e) = self.error
        {
            return CalcValue::error(e);
        }
        let n = match kind {
            "count" => self.nonempty as f64,
            "countNums" => self.numbers as f64,
            "min" => self.min.unwrap_or(0.),
            "max" => self.max.unwrap_or(0.),
            "average" if self.numbers == 0 => return CalcValue::error(WorksheetErrorCode::Div0),
            "average" => self.sum / self.numbers as f64,
            "product" => self.product,
            "stdDev" | "stdDevp" | "var" | "varp" => {
                let count = self.numbers as f64;
                let sample = matches!(kind, "stdDev" | "var");
                if count == 0.0 || (sample && count < 2.0) {
                    return CalcValue::error(WorksheetErrorCode::Div0);
                }
                let square_total = self.sum * self.sum;
                let variance = if sample {
                    (self.squares - square_total / count) / (count - 1.0)
                } else {
                    (count * self.squares - square_total) / (count * count)
                };
                if matches!(kind, "stdDev" | "stdDevp") { variance.sqrt() } else { variance }
            }
            _ => self.sum,
        };
        if n.is_finite() {
            CalcValue::number(n)
        } else {
            CalcValue::error(WorksheetErrorCode::Num)
        }
    }
}
