//! Worksheet print metadata. Attribute spellings retain absent/default distinctions and decimals.
//! Print areas and titles remain ordinary sheet-scoped defined names in the one name table.
use calc_value::ExcelText;
use std::collections::BTreeMap;

pub const PAGE_SECTIONS: &[(&str, &[&str])] = &[
    ("pageSetUpPr", &["autoPageBreaks", "fitToPage"]),
    (
        "printOptions",
        &[
            "horizontalCentered",
            "verticalCentered",
            "headings",
            "gridLines",
            "gridLinesSet",
        ],
    ),
    (
        "pageMargins",
        &["left", "right", "top", "bottom", "header", "footer"],
    ),
    (
        "pageSetup",
        &[
            "paperSize",
            "paperHeight",
            "paperWidth",
            "scale",
            "firstPageNumber",
            "fitToWidth",
            "fitToHeight",
            "pageOrder",
            "orientation",
            "usePrinterDefaults",
            "blackAndWhite",
            "draft",
            "cellComments",
            "useFirstPageNumber",
            "errors",
            "horizontalDpi",
            "verticalDpi",
            "copies",
        ],
    ),
    (
        "headerFooter",
        &[
            "differentOddEven",
            "differentFirst",
            "scaleWithDoc",
            "alignWithMargins",
        ],
    ),
];
pub const HEADER_FIELDS: &[&str] = &[
    "oddHeader",
    "oddFooter",
    "evenHeader",
    "evenFooter",
    "firstHeader",
    "firstFooter",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PageSetup {
    pub sections: BTreeMap<String, BTreeMap<String, String>>,
    pub headers: BTreeMap<String, ExcelText>,
    pub row_breaks: Vec<PageBreak>,
    pub column_breaks: Vec<PageBreak>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageBreak {
    /// One-based number of the row/column before the break; zero is before the first band.
    pub id: u32,
    /// Zero-based inclusive span on the opposite axis.
    pub min: u32,
    pub max: u32,
    pub manual: bool,
    pub pivot: bool,
}
impl PageSetup {
    pub fn get(&self, section: &str, name: &str) -> Option<&str> {
        self.sections.get(section)?.get(name).map(String::as_str)
    }
    pub fn set(&mut self, section: &str, name: &str, value: Option<String>) -> Result<(), String> {
        if !PAGE_SECTIONS
            .iter()
            .any(|(s, fields)| *s == section && fields.contains(&name))
        {
            return Err("Unknown page setup field (dc-971p.56)".into());
        }
        if section == "pageMargins" {
            let fields = self.sections.entry(section.into()).or_default();
            for (k, v) in [
                ("left", "0.7"),
                ("right", "0.7"),
                ("top", "0.75"),
                ("bottom", "0.75"),
                ("header", "0.3"),
                ("footer", "0.3"),
            ] {
                fields.entry(k.into()).or_insert_with(|| v.into());
            }
        }
        if let Some(value) = value {
            validate_attribute(section, name, &value)?;
            self.sections
                .entry(section.into())
                .or_default()
                .insert(name.into(), value);
        } else if let Some(fields) = self.sections.get_mut(section) {
            fields.remove(name);
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        for (section, fields) in &self.sections {
            for (name, value) in fields {
                if !PAGE_SECTIONS
                    .iter()
                    .any(|(s, names)| s == section && names.contains(&name.as_str()))
                {
                    return Err("Unknown page setup field (dc-971p.56)".into());
                }
                validate_attribute(section, name, value)?;
            }
        }
        if self.sections.get("pageMargins").is_some_and(|m| {
            ["left", "right", "top", "bottom", "header", "footer"]
                .iter()
                .any(|k| !m.contains_key(*k))
        }) {
            return Err("Page margins need all six dimensions (dc-971p.56)".into());
        }
        if self
            .headers
            .iter()
            .any(|(k, v)| !HEADER_FIELDS.contains(&k.as_str()) || v.utf16_code_units().len() > 255)
        {
            return Err("A header or footer must use a supported field and at most 255 characters (dc-971p.56)".into());
        }
        for (breaks, size, opposite) in [
            (&self.row_breaks, 1_048_576, 16_384),
            (&self.column_breaks, 16_384, 1_048_576),
        ] {
            if breaks.len() > 1026
                || breaks
                    .iter()
                    .any(|b| b.id >= size || b.min > b.max || b.max >= opposite)
            {
                return Err("A page break is outside the worksheet or there are more than 1026 breaks (dc-971p.56)".into());
            }
        }
        Ok(())
    }
}
fn validate_attribute(section: &str, name: &str, value: &str) -> Result<(), String> {
    let unsigned = || value.parse::<u32>().ok();
    let valid = if section == "pageMargins" {
        value.parse::<f64>().is_ok_and(|v| v.is_finite() && v >= 0.)
    } else {
        match name {
            "orientation" => ["default", "portrait", "landscape"].contains(&value),
            "pageOrder" => ["downThenOver", "overThenDown"].contains(&value),
            "cellComments" => ["none", "asDisplayed", "atEnd"].contains(&value),
            "errors" => ["displayed", "blank", "dash", "NA"].contains(&value),
            "scale" => unsigned().is_some_and(|v| (10..=400).contains(&v)),
            "paperSize" | "copies" | "horizontalDpi" | "verticalDpi" => {
                unsigned().is_some_and(|v| v > 0)
            }
            "firstPageNumber" | "fitToWidth" | "fitToHeight" => unsigned().is_some(),
            "paperWidth" | "paperHeight" => {
                ["mm", "cm", "in", "pt", "pc", "pi"].iter().any(|unit| {
                    value
                        .strip_suffix(unit)
                        .is_some_and(|n| n.parse::<f64>().is_ok_and(|v| v.is_finite() && v > 0.))
                })
            }
            _ => ["true", "false", "0", "1"].contains(&value),
        }
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid page setup {name}: {value} (dc-971p.56)"))
    }
}
