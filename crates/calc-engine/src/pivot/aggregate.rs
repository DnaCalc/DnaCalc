use super::*;

#[derive(Default)]
pub(super) struct Total {
    sum: f64,
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
    pub(super) fn result(&self, kind: &str) -> CalcValue {
        // UI28 P6: a sum over source cells that are all empty is a blank pivot cell, not 0.
        if kind == "sum" && self.nonempty == 0 {
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
            _ => self.sum,
        };
        if n.is_finite() {
            CalcValue::number(n)
        } else {
            CalcValue::error(WorksheetErrorCode::Num)
        }
    }
}
