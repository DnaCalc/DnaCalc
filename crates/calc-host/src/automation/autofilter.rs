//! `Range.AutoFilter`, `Worksheet.AutoFilterMode`, `Worksheet.FilterMode` and `Worksheet.ShowAllData`
//! (dc-oxh8w.43), one handler each, over the host's own AutoFilter command (`WorkbookCommand::AutoFilter`:
//! criteria evaluation, hidden rows and the undo step are the host's). `Range.AutoFilter` with no Field
//! switches the filter on over the range (a single cell: its current region) or off when the sheet has
//! one; with a Field (counted from the filter's first column) it sets that column's criterion, enabling
//! the filter first when there is none. Criteria1 is a number (equals), a text with an optional
//! comparison prefix (`>5`, `<>x`, `=` for blanks, `<>` for non-blanks, wildcards as typed), an array
//! with xlFilterValues, or the item count of xlTop10Items and its three siblings; Criteria2 joins a
//! second comparison by xlAnd or xlOr. Colour, dynamic and icon filters refuse typed.
use super::*;
use crate::{FilterChange, FilterCriteria, FilterOperator, FilterPredicate, SkinIntent, WorkbookCommand};

const OWNER: &str = "dc-oxh8w.43";
const XL_AND: i64 = 1;
const XL_OR: i64 = 2;
const XL_TOP10_ITEMS: i64 = 3;
const XL_BOTTOM10_ITEMS: i64 = 4;
const XL_TOP10_PERCENT: i64 = 5;
const XL_BOTTOM10_PERCENT: i64 = 6;
const XL_FILTER_VALUES: i64 = 7;
/// Highest valid XlAutoFilterOperator (12 xlFilterNoFill, 13 xlFilterAutomaticFontColor, 14 xlFilterNoIcon).
const XL_FILTER_NO_ICON: i64 = 14;

fn error_1004() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

/// One comparison from a Criteria argument: the operator prefix of a text, else equality.
fn predicate(value: &AutoValue) -> Result<FilterPredicate, AutoError> {
    let text = match value {
        AutoValue::Text(t) => t.to_string_lossy(),
        AutoValue::R8(n) | AutoValue::Date(n) => n.to_string(),
        AutoValue::R4(n) => n.to_string(),
        AutoValue::I4(n) => n.to_string(),
        AutoValue::I2(n) => n.to_string(),
        AutoValue::Currency(n) => (*n as f64 / 10_000.0).to_string(),
        AutoValue::Bool(true) => "TRUE".to_owned(),
        AutoValue::Bool(false) => "FALSE".to_owned(),
        _ => return Err(geometry::type_mismatch()),
    };
    if !matches!(value, AutoValue::Text(_)) {
        return Ok(FilterPredicate { operator: FilterOperator::Equal, value: text });
    }
    // Longest prefixes first.
    for (prefix, operator) in [
        ("<>", FilterOperator::NotEqual),
        ("<=", FilterOperator::LessEqual),
        (">=", FilterOperator::GreaterEqual),
        ("<", FilterOperator::Less),
        (">", FilterOperator::Greater),
        ("=", FilterOperator::Equal),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return Ok(FilterPredicate { operator, value: rest.to_owned() });
        }
    }
    Ok(FilterPredicate { operator: FilterOperator::Equal, value: text })
}

impl WorkbookAutomation {
    pub(super) fn register_autofilter(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "AutoFilter", InvokeKind::Method, Self::range_auto_filter);
        handlers.on("Worksheet", "AutoFilterMode", InvokeKind::Get, Self::worksheet_auto_filter_mode);
        handlers.on("Worksheet", "AutoFilterMode", InvokeKind::Put, Self::worksheet_set_auto_filter_mode);
        handlers.on("Worksheet", "FilterMode", InvokeKind::Get, Self::worksheet_filter_mode);
        handlers.on("Worksheet", "ShowAllData", InvokeKind::Method, Self::worksheet_show_all_data);
    }

    fn criterion(call: &Call<'_>) -> Result<Option<FilterCriteria>, AutoError> {
        let given = |index: usize| !call.arg(index).is_missing() && !matches!(call.arg(index), AutoValue::Empty);
        let operator = if given(2) { geometry::integer(call.arg(2))? } else { XL_AND };
        if !(XL_AND..=XL_FILTER_NO_ICON).contains(&operator) {
            return Err(error_1004());
        }
        if operator > XL_FILTER_VALUES {
            return Err(AutoError::not_modelled("Range.AutoFilter colour, icon and dynamic filters", OWNER));
        }
        if !given(1) {
            // A Field alone shows everything in the column.
            return Ok(None);
        }
        let first = call.arg(1);
        Ok(Some(match operator {
            XL_AND | XL_OR => {
                let mut predicates = vec![predicate(first)?];
                if given(3) {
                    predicates.push(predicate(call.arg(3))?);
                }
                FilterCriteria::Custom { and: operator == XL_AND || predicates.len() == 1, predicates }
            }
            XL_TOP10_ITEMS..=XL_BOTTOM10_PERCENT => {
                let count = match first {
                    AutoValue::Text(t) => t.to_string_lossy().trim().parse::<f64>().map_err(|_| error_1004())?,
                    AutoValue::R8(n) => *n,
                    other => geometry::integer(other)? as f64,
                };
                if !(1.0..=500.0).contains(&count) || count.fract() != 0.0 {
                    return Err(error_1004());
                }
                let percent = operator >= XL_TOP10_PERCENT;
                if percent && count > 100.0 {
                    return Err(error_1004());
                }
                FilterCriteria::Top {
                    top: operator == XL_TOP10_ITEMS || operator == XL_TOP10_PERCENT,
                    percent,
                    value: (count as u32).to_string(),
                }
            }
            _ => {
                let AutoValue::Array(array) = first else {
                    return Err(geometry::type_mismatch());
                };
                let mut values = Vec::new();
                let mut blank = false;
                for item in &array.values {
                    let text = Self::entry_of(item)?;
                    if text.is_empty() {
                        blank = true;
                    } else {
                        values.push(text);
                    }
                }
                FilterCriteria::Values { values, blank }
            }
        }))
    }

    fn range_auto_filter(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 6 {
            return Err(geometry::bad_count());
        }
        let field = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => None,
            other => Some(geometry::integer(other)?),
        };
        let criteria = if field.is_some() { Self::criterion(call)? } else { None };
        self.mutate_range_with_owner(target, call, OWNER, |session, range| {
            let mut area = range.areas()[0];
            let sheet = area.sheet();
            let existing = session.host().model().definitions().auto_filter(sheet).map(|f| f.area);
            if area.cell_count() == 1 && existing.is_none() {
                area = Self::current_region(session.host().model(), area.first());
            }
            let change = match field {
                None if existing.is_some() => FilterChange::Remove,
                None => FilterChange::Enable { area },
                Some(field) => {
                    let base = existing.unwrap_or(area);
                    if field < 1 || field > i64::from(base.column_count()) {
                        return Err(error_1004());
                    }
                    FilterChange::EnableColumn {
                        area,
                        column: base.first().column() + field as u32 - 1,
                        criteria: criteria.clone(),
                    }
                }
            };
            Ok(SkinIntent::Command(WorkbookCommand::AutoFilter {
                sheet,
                revision: session.host().revision().get(),
                change,
            }))
        })?;
        Ok(AutoValue::Bool(true))
    }

    fn worksheet_auto_filter_mode(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let sheet = Self::worksheet_sheet(target)?;
            Ok(AutoValue::Bool(meta.model.definitions().auto_filter(sheet).is_some()))
        })
    }

    fn worksheet_filter_mode(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let sheet = Self::worksheet_sheet(target)?;
            Ok(AutoValue::Bool(meta.model.definitions().auto_filter(sheet).is_some_and(|f| !f.columns.is_empty())))
        })
    }

    /// Setting AutoFilterMode False removes the filter; setting it True is an error in Excel.
    fn worksheet_set_auto_filter_mode(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let [AutoValue::Bool(on)] = call.args.as_slice() else {
            return Err(geometry::type_mismatch());
        };
        if *on {
            return Err(error_1004());
        }
        let sheet = Self::worksheet_sheet(target)?;
        self.mutate(target, call, |session, _| {
            Ok(SkinIntent::Command(WorkbookCommand::AutoFilter {
                sheet,
                revision: session.host().revision().get(),
                change: FilterChange::Remove,
            }))
        })?;
        Ok(AutoValue::Empty)
    }

    /// Shows every row; an error when the sheet has no criteria applied.
    fn worksheet_show_all_data(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let sheet = Self::worksheet_sheet(target)?;
        self.mutate(target, call, |session, _| {
            let filtering = session
                .host()
                .model()
                .definitions()
                .auto_filter(sheet)
                .is_some_and(|f| !f.columns.is_empty());
            if !filtering {
                return Err(error_1004());
            }
            Ok(SkinIntent::Command(WorkbookCommand::AutoFilter {
                sheet,
                revision: session.host().revision().get(),
                change: FilterChange::Clear,
            }))
        })?;
        Ok(AutoValue::Empty)
    }
}
