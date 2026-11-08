use super::*;

fn plain_text(s: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
}
pub(super) fn axis_item(kind: &str, repeat: u32, indexes: Vec<u32>) -> PivotAxisItem {
    PivotAxisItem {
        kind: kind.into(),
        repeat,
        indexes,
        ..Default::default()
    }
}
fn group_label(group: &Group) -> Result<CalcValue, PivotIssue> {
    if group.key == PivotItem::Missing {
        Ok(plain_text("(blank)"))
    } else {
        item_value(&group.key)
    }
}
/// The rows of a pivot with several row fields, written depth first: a group's own row (compact
/// and outline) or its first leaf row (tabular), its children, then its total row if it has one.
pub(super) struct Walk<'a> {
    pub(super) levels: &'a [Vec<Group>],
    /// `totals[k]` holds the total of each path of `k + 1` groups.
    pub(super) totals: &'a [BTreeMap<Vec<usize>, Total>],
    pub(super) position: &'a [Vec<Option<u32>>],
    pub(super) form: Form,
    pub(super) subtotal_top: bool,
    pub(super) subtotal_on: &'a [bool],
    pub(super) fill_down: bool,
    pub(super) kind: &'a str,
    pub(super) value_column: u32,
    pub(super) r: u32,
    /// Tabular: how many leading levels of the current path already show their labels.
    pub(super) printed: usize,
    pub(super) cells: BTreeMap<(u32, u32), CalcValue>,
    pub(super) axis: Vec<PivotAxisItem>,
}
impl Walk<'_> {
    /// The groups present under `prefix`, in the order of the next field's items.
    fn children(&self, prefix: &[usize]) -> Vec<usize> {
        let level = prefix.len();
        let mut low = prefix.to_vec();
        low.push(0);
        let mut found: Vec<usize> = self.totals[level]
            .range(low..)
            .take_while(|(k, _)| k[..level] == *prefix)
            .map(|(k, _)| k[level])
            .collect();
        found.dedup();
        found
    }
    pub(super) fn visit(&mut self, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
        for g in self.children(path) {
            path.push(g);
            self.group(path)?;
            path.pop();
        }
        Ok(())
    }
    fn group(&mut self, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
        let level = path.len() - 1;
        let depth = self.levels.len();
        let g = path[level];
        let group = &self.levels[level][g];
        let x = self.position[level][g].expect("visible group");
        let total = self.totals[level][&*path].result(self.kind);
        let label_column = if self.form == Form::Compact { 0 } else { level as u32 };
        if level + 1 == depth {
            match self.form {
                Form::Compact | Form::Outline => {
                    self.cells.insert((self.r, label_column), group_label(group)?);
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("data", level as u32, vec![x]));
                }
                Form::Tabular => {
                    // Labels print from the first level not yet shown (or every level when the
                    // labels repeat); the row item names the same levels.
                    let from = self.printed.min(level);
                    let mut indexes = Vec::new();
                    for (l, &gl) in path.iter().enumerate() {
                        if l >= from {
                            indexes.push(self.position[l][gl].expect("visible group"));
                        }
                        if l >= from || self.fill_down {
                            self.cells
                                .insert((self.r, l as u32), group_label(&self.levels[l][gl])?);
                        }
                    }
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("data", from as u32, indexes));
                    self.printed = depth;
                }
            }
            self.r += 1;
            return Ok(());
        }
        match self.form {
            Form::Compact | Form::Outline => {
                self.cells.insert((self.r, label_column), group_label(group)?);
                if self.subtotal_top && self.subtotal_on[level] {
                    self.cells.insert((self.r, self.value_column), total);
                }
                self.axis.push(axis_item("data", level as u32, vec![x]));
                self.r += 1;
            }
            Form::Tabular => self.printed = self.printed.min(level),
        }
        self.visit(path)?;
        if !self.subtotal_on[level] {
            return Ok(());
        }
        let group = &self.levels[level][g];
        let total = self.totals[level][&*path].result(self.kind);
        let total_label = plain_text(&format!("{} Total", label_text(group)?));
        match self.form {
            Form::Compact | Form::Outline if !self.subtotal_top => {
                self.cells.insert((self.r, label_column), total_label);
                self.cells.insert((self.r, self.value_column), total);
                self.axis.push(axis_item("default", level as u32, vec![x]));
                self.r += 1;
            }
            Form::Tabular => {
                self.cells.insert((self.r, level as u32), total_label);
                self.cells.insert((self.r, self.value_column), total);
                self.axis.push(axis_item("default", level as u32, vec![x]));
                self.r += 1;
                self.printed = level;
            }
            _ => {}
        }
        Ok(())
    }
}
