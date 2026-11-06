//! Revision-bound print settings and the existing scoped Print_Area / Print_Titles names.
use crate::{CellEdit, HostState};
use calc_doc_model::{HEADER_FIELDS, PageBreak};
use calc_value::{CellKey, ExcelText, GridArea, NameScope, SheetId};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageSetupChange {
    pub attributes: Vec<(String, String, Option<String>)>,
    pub headers: Vec<(String, String)>,
    pub row_breaks: Option<Vec<PageBreak>>,
    pub column_breaks: Option<Vec<PageBreak>>,
    /// Empty removes the scoped name; None leaves it unchanged.
    pub print_area: Option<String>,
    pub print_titles: Option<String>,
}
/// Split a reference union without splitting a quoted sheet name containing commas.
pub fn print_references(
    host: &HostState,
    sheet: SheetId,
    text: &str,
) -> Result<Vec<GridArea>, String> {
    let mut quoted = false;
    let mut start = 0;
    let mut tokens = Vec::new();
    for (i, c) in text.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        } else if c == ',' && !quoted {
            tokens.push(&text[start..i]);
            start = i + 1;
        }
    }
    tokens.push(&text[start..]);
    if quoted || tokens.len() > 128 {
        return Err(
            "Print ranges need at most 128 references on this worksheet (dc-971p.56)".into(),
        );
    }
    let origin = CellKey::new(sheet, 1, 1).map_err(|e| e.to_string())?;
    tokens.into_iter().map(|t|match host.resolve_reference_text(origin,t.trim().trim_start_matches('=')) {
        crate::ReferenceTarget::Area(area) if area.sheet()==sheet=>Ok(area),
        _=>Err("Print ranges need cell ranges or whole rows/columns on this worksheet (dc-971p.56)".into()),
    }).collect()
}
pub fn print_name(host: &HostState, sheet: SheetId, name: &str) -> Option<String> {
    host.model()
        .definitions()
        .names()
        .iter()
        .find(|n| {
            n.scope == NameScope::Sheet(sheet) && calc_value::workbook_names_equal(&n.name, name)
        })
        .map(|n| n.formula.clone())
}
pub(crate) fn prepare(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    change: PageSetupChange,
) -> Result<Vec<CellEdit>, String> {
    if host.revision().get() != revision {
        return Err("The workbook changed; reopen Page Setup (dc-971p.56)".into());
    }
    crate::can_edit(host, CellKey::new(sheet, 1, 1).map_err(|e| e.to_string())?)?;
    let original = host.model().definitions().page_setup(sheet);
    let mut setup = original.clone();
    for (section, name, value) in change.attributes {
        setup.set(&section, &name, value)?;
    }
    for (name, value) in change.headers {
        if !HEADER_FIELDS.contains(&name.as_str()) {
            return Err("Unknown header/footer field (dc-971p.56)".into());
        }
        if value.is_empty() {
            setup.headers.remove(&name);
        } else {
            setup.headers.insert(
                name,
                ExcelText::from_utf16_code_units(value.encode_utf16().collect()),
            );
        }
    }
    if let Some(breaks) = change.row_breaks {
        setup.row_breaks = breaks;
    }
    if let Some(breaks) = change.column_breaks {
        setup.column_breaks = breaks;
    }
    setup.validate()?;
    let mut edits = Vec::new();
    if setup != original {
        edits.push(CellEdit::PageSetup { sheet, setup });
    }
    let sheet_name = host
        .model()
        .sheet_by_id(sheet)
        .ok_or("Worksheet unavailable")?
        .name()
        .text()
        .to_string_lossy()
        .replace('\'', "''");
    let col = |c| {
        let mut n = c;
        let mut out = String::new();
        while n > 0 {
            n -= 1;
            out.insert(0, (b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        out
    };
    for (name, text, titles) in [
        ("_xlnm.Print_Area", change.print_area, false),
        ("_xlnm.Print_Titles", change.print_titles, true),
    ] {
        let Some(text) = text else {
            continue;
        };
        let formula = if text.trim().is_empty() {
            None
        } else {
            let ranges = print_references(host, sheet, &text)?;
            let mut row_titles = 0;
            let mut col_titles = 0;
            let mut out = Vec::new();
            for area in ranges {
                let a = area.first();
                let b = area.last();
                let range = if titles {
                    if a.column() == 1 && b.column() == 16_384 {
                        row_titles += 1;
                        format!("${}:${}", a.row(), b.row())
                    } else if a.row() == 1 && b.row() == 1_048_576 {
                        col_titles += 1;
                        format!("${}:${}", col(a.column()), col(b.column()))
                    } else {
                        return Err(
                            "Print titles need whole rows and/or whole columns (dc-971p.56)".into(),
                        );
                    }
                } else {
                    format!(
                        "${}${}:${}${}",
                        col(a.column()),
                        a.row(),
                        col(b.column()),
                        b.row()
                    )
                };
                out.push(format!("'{sheet_name}'!{range}"));
            }
            if row_titles > 1 || col_titles > 1 {
                return Err(
                    "Choose one consecutive title row band and one title column band (dc-971p.56)"
                        .into(),
                );
            }
            Some(out.join(","))
        };
        let old = print_name(host, sheet, name);
        if old != formula && (old.is_some() || formula.is_some()) {
            edits.push(CellEdit::SetName {
                name: name.into(),
                scope: NameScope::Sheet(sheet),
                formula,
            });
        }
    }
    if edits.is_empty() {
        return Err("Page Setup has no changes".into());
    }
    Ok(edits)
}
