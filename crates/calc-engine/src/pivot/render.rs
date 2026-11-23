use super::*;

/// Render the supported row and column layouts from an explicit source snapshot.
pub fn layout(table: &PivotTable, cache: &PivotCache) -> Result<PivotLayout, PivotIssue> {
    let bad = |why: &str| refusal(&table.part, why);
    let snapshot = cache
        .snapshot
        .as_ref()
        .ok_or_else(|| bad("No explicit source refresh has run"))?;
    if !table.unsupported.is_empty() || !cache.unsupported.is_empty() {
        return Err(bad(
            "Grouped, calculated, filtered or extended pivot layouts are not modelled",
        ));
    }
    if !table.filters.is_empty()
        && (table.filters.len() != 1
            || table.row_fields.len() != 1
            || !table.column_fields.is_empty()
            || table.data_fields.len() != 1
            || table.filters[0].field as i32 != table.row_fields[0]
            || table.filters[0].measure != 0
            || table
                .page_fields
                .iter()
                .any(|page| page.field as i32 == table.row_fields[0]))
    {
        return Err(bad("This pivot value filter layout is not modelled"));
    }
    // M7 captures a compact labels-only report with one field on each axis.
    if table.data_fields.is_empty()
        && (table.row_fields.len() != 1
            || table.row_fields[0] < 0
            || table.column_fields.len() != 1
            || table.column_fields[0] < 0
            || !truthy(table.attributes.get("compact"), true)
            || !table.page_fields.is_empty()
            || !table.formats.is_empty()
            || !cache.calculated_items.is_empty()
            || cache.fields.iter().any(|field| {
                field.group.is_some() || field.attributes.contains_key("formula")
            })
            || table.fields.iter().any(|field| {
                field.sort_measure.is_some() || field.items.iter().any(|item| item.hidden)
            }))
    {
        return Err(bad("This labels-only pivot layout is not modelled"));
    }
    let mut layout = render_axes(table, cache, snapshot)?;
    if truthy(table.attributes.get("showError"), false) {
        let caption = table
            .attributes
            .get("errorCaption")
            .ok_or_else(|| bad("An error caption is required for this display setting"))?;
        for value in layout.cells.values_mut() {
            if matches!(value.core_ref(), calc_value::CoreRef::Error(_)) {
                *value = CalcValue::text(ExcelText::from_utf16_code_units(
                    caption.encode_utf16().collect(),
                ));
            }
        }
    }
    Ok(layout)
}

pub(super) fn truthy(value: Option<&String>, default: bool) -> bool {
    value.map_or(default, |v| v != "0" && v != "false")
}
pub(super) fn label_text(group: &Group) -> Result<String, PivotIssue> {
    Ok(match &group.key {
        PivotItem::Missing => "(blank)".into(),
        PivotItem::Text(s) | PivotItem::Number(s) | PivotItem::Error(s) => s.clone(),
        PivotItem::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        _ => {
            return Err(refusal(
                "pivot",
                "Grouped, date and unknown cache items need measured refresh semantics",
            ));
        }
    })
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Form {
    Compact,
    Outline,
    Tabular,
}
