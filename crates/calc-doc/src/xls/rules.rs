//! Rules a sheet applies to its cells (MS-XLS 2.4.42 CFHeader and CF, 2.4.88 DVal and 2.4.87 Dv,
//! 2.4.6 AutoFilter): conditional formats, data validations and the automatic filter, written as
//! the `conditionalFormatting`, `dataValidations` and `autoFilter` elements. A conditional
//! format's formatting (the CF record's font, border and pattern blocks) becomes a differential
//! format of the styles part.

use super::records::Record;
use super::sheet::column_name;
use super::strings::Pieces;
use super::styles::Styles;
use crate::xlsb::{Cursor, Xml};

const CF_HEADER: u16 = 0x01B0;
const CF: u16 = 0x01B1;
const DVAL: u16 = 0x01B2;
const DV: u16 = 0x01BE;
const AUTO_FILTER: u16 = 0x009E;
const AUTO_FILTER_INFO: u16 = 0x009D;

fn e(m: crate::xlsb::Malformed) -> String {
    m.0
}

fn cell(row: u16, column: u16) -> String {
    format!("{}{}", column_name(u32::from(column)), u32::from(row) + 1)
}

fn range(first: (u16, u16), last: (u16, u16)) -> String {
    if first == last {
        cell(first.0, first.1)
    } else {
        format!("{}:{}", cell(first.0, first.1), cell(last.0, last.1))
    }
}

/// A Ref8U list: a count, then (first row, last row, first column, last column) each.
fn ref_list(c: &mut Cursor<'_>) -> Result<(Vec<String>, (u16, u16)), String> {
    let count = c.u16("a range list").map_err(e)?;
    let mut ranges = Vec::new();
    let mut origin = None;
    for _ in 0..count {
        let (r1, r2) = (c.u16("a range").map_err(e)?, c.u16("a range").map_err(e)?);
        let (c1, c2) = (c.u16("a range").map_err(e)?, c.u16("a range").map_err(e)?);
        origin.get_or_insert((r1, c1));
        ranges.push(range((r1, c1), (r2, c2)));
    }
    Ok((ranges, origin.unwrap_or((0, 0))))
}

#[derive(Default)]
struct Format {
    sqref: String,
    origin: (u16, u16),
    rules: Vec<String>,
}

struct Validation {
    sqref: String,
    xml: Vec<u8>,
}

struct FilterColumn {
    column: u16,
    xml: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct Rules {
    formats: Vec<Format>,
    pending_rules: usize,
    priority: u32,
    validations: Vec<Validation>,
    filter_columns: Vec<FilterColumn>,
    filter_info: bool,
}

/// Prints formula tokens as text for a cell the formula is evaluated at.
pub(crate) type Print<'a> = &'a dyn Fn(&[u8], (u16, u16)) -> Option<String>;

impl Rules {
    pub(crate) fn read(
        &mut self,
        record: &Record<'_>,
        styles: &Styles,
        print: Print<'_>,
        sheet_name: &str,
        v5: Option<u16>,
    ) -> Result<bool, String> {
        match record.id {
            CF_HEADER => {
                let mut c = Cursor::new(record.body);
                let count = usize::from(c.u16("CFHeader").map_err(e)?);
                c.skip(2 + 8, "CFHeader").map_err(e)?;
                let (ranges, origin) = ref_list(&mut c)?;
                self.formats.push(Format {
                    sqref: ranges.join(" "),
                    origin,
                    rules: Vec::new(),
                });
                self.pending_rules = count;
            }
            CF => {
                let body = record.joined();
                let mut c = Cursor::new(&body);
                let kind = c.u8("CF").map_err(e)?;
                let operator = c.u8("CF").map_err(e)?;
                let size1 = usize::from(c.u16("CF").map_err(e)?);
                let size2 = usize::from(c.u16("CF").map_err(e)?);
                let flags = c.u32("CF").map_err(e)?;
                c.skip(2, "CF").map_err(e)?;
                let mut dxf = Dxf::default();
                if flags & 0x0400_0000 != 0 {
                    dxf.font(c.bytes(118, "CF").map_err(e)?);
                }
                if flags & 0x0800_0000 != 0 {
                    c.skip(8, "CF").map_err(e)?;
                }
                if flags & 0x1000_0000 != 0 {
                    dxf.border(c.bytes(8, "CF").map_err(e)?, flags);
                }
                if flags & 0x2000_0000 != 0 {
                    dxf.pattern(c.bytes(4, "CF").map_err(e)?);
                }
                if flags & 0x4000_0000 != 0 {
                    c.skip(2, "CF").map_err(e)?;
                }
                let first = c.bytes(size1, "CF").map_err(e)?.to_vec();
                let second = c.bytes(size2, "CF").map_err(e)?.to_vec();
                let Some(format) = self.formats.last_mut() else {
                    return Err(format!("a CF record on sheet {sheet_name:?} outside a conditional format"));
                };
                let origin = format.origin;
                let formula = |tokens: &[u8]| -> Result<String, String> {
                    print(tokens, origin).ok_or_else(|| {
                        format!("a conditional format formula on sheet {sheet_name:?} that could not be read")
                    })
                };
                self.priority += 1;
                let dxf_id = styles.add_dxf(dxf.xml(styles));
                let mut x = Xml { out: Vec::new() };
                let mut attributes = vec![("priority", self.priority.to_string())];
                attributes.insert(0, ("dxfId", dxf_id.to_string()));
                match kind {
                    1 => {
                        let name = match operator {
                            1 => "between",
                            2 => "notBetween",
                            3 => "equal",
                            4 => "notEqual",
                            5 => "greaterThan",
                            6 => "lessThan",
                            7 => "greaterThanOrEqual",
                            8 => "lessThanOrEqual",
                            other => return Err(format!("the conditional format comparison {other}")),
                        };
                        attributes.insert(0, ("type", "cellIs".to_owned()));
                        attributes.push(("operator", name.to_owned()));
                        x.open("cfRule", &attributes);
                        x.element("formula", &[], &formula(&first)?);
                        if size2 > 0 {
                            x.element("formula", &[], &formula(&second)?);
                        }
                        x.close("cfRule");
                    }
                    2 => {
                        attributes.insert(0, ("type", "expression".to_owned()));
                        x.open("cfRule", &attributes);
                        x.element("formula", &[], &formula(&first)?);
                        x.close("cfRule");
                    }
                    other => return Err(format!("the conditional format type {other}")),
                }
                format.rules.push(String::from_utf8(x.out).expect("UTF-8"));
                self.pending_rules = self.pending_rules.saturating_sub(1);
            }
            DVAL => {}
            DV => {
                let joined = record.joined();
                let flags = u32::from_le_bytes(
                    joined.get(..4).ok_or("a DV record that is cut off")?.try_into().expect("4 bytes"),
                );
                let (mut strings, after) = dv_strings(&joined)?;
                let error = strings.pop().unwrap_or_default();
                let prompt = strings.pop().unwrap_or_default();
                let error_title = strings.pop().unwrap_or_default();
                let prompt_title = strings.pop().unwrap_or_default();
                let mut c = Cursor::new(&joined[after..]);
                let size1 = usize::from(c.u16("DV").map_err(e)?);
                c.skip(2, "DV").map_err(e)?;
                let first = c.bytes(size1, "DV").map_err(e)?.to_vec();
                let size2 = usize::from(c.u16("DV").map_err(e)?);
                c.skip(2, "DV").map_err(e)?;
                let second = c.bytes(size2, "DV").map_err(e)?.to_vec();
                let (ranges, origin) = ref_list(&mut c)?;
                let kind = match flags & 0xF {
                    0 => None,
                    1 => Some("whole"),
                    2 => Some("decimal"),
                    3 => Some("list"),
                    4 => Some("date"),
                    5 => Some("time"),
                    6 => Some("textLength"),
                    7 => Some("custom"),
                    other => return Err(format!("the data validation type {other}")),
                };
                let mut a: Vec<(&str, String)> = Vec::new();
                if let Some(kind) = kind {
                    a.push(("type", kind.to_owned()));
                }
                match (flags >> 4) & 7 {
                    1 => a.push(("errorStyle", "warning".to_owned())),
                    2 => a.push(("errorStyle", "information".to_owned())),
                    _ => {}
                }
                let operator = match (flags >> 20) & 0xF {
                    1 => Some("notBetween"),
                    2 => Some("equal"),
                    3 => Some("notEqual"),
                    4 => Some("greaterThan"),
                    5 => Some("lessThan"),
                    6 => Some("greaterThanOrEqual"),
                    7 => Some("lessThanOrEqual"),
                    _ => None,
                };
                if let Some(operator) = operator {
                    a.push(("operator", operator.to_owned()));
                }
                if flags & 0x100 != 0 {
                    a.push(("allowBlank", "1".to_owned()));
                }
                if flags & 0x200 != 0 {
                    a.push(("showDropDown", "1".to_owned()));
                }
                if flags & 0x4_0000 != 0 {
                    a.push(("showInputMessage", "1".to_owned()));
                }
                if flags & 0x8_0000 != 0 {
                    a.push(("showErrorMessage", "1".to_owned()));
                }
                for (name, text) in [
                    ("errorTitle", &error_title),
                    ("error", &error),
                    ("promptTitle", &prompt_title),
                    ("prompt", &prompt),
                ] {
                    if !text.is_empty() {
                        a.push((name, text.clone()));
                    }
                }
                a.push(("sqref", ranges.join(" ")));
                let render = |tokens: &[u8]| -> Result<String, String> {
                    if flags & 0xF == 3 && flags & 0x80 != 0 {
                        // An explicit list: a string token whose items are separated by NULs.
                        let text = print(tokens, origin)
                            .ok_or_else(|| format!("a validation list on sheet {sheet_name:?} that could not be read"))?;
                        let items: Vec<String> = text
                            .trim_matches('"')
                            .split('\0')
                            .map(|s| s.to_owned())
                            .collect();
                        return Ok(format!("\"{}\"", items.join(",").replace('"', "\"\"")));
                    }
                    print(tokens, origin)
                        .ok_or_else(|| format!("a validation formula on sheet {sheet_name:?} that could not be read"))
                };
                let mut x = Xml { out: Vec::new() };
                x.open("dataValidation", &a);
                if size1 > 0 {
                    x.element("formula1", &[], &render(&first)?);
                }
                if size2 > 0 {
                    x.element("formula2", &[], &render(&second)?);
                }
                x.close("dataValidation");
                self.validations.push(Validation {
                    sqref: ranges.join(" "),
                    xml: x.out,
                });
            }
            AUTO_FILTER_INFO => self.filter_info = true,
            AUTO_FILTER => {
                let mut c = Cursor::new(record.body);
                let column = c.u16("AutoFilter").map_err(e)?;
                let flags = c.u16("AutoFilter").map_err(e)?;
                let first = c.bytes(10, "AutoFilter").map_err(e)?.to_vec();
                let second = c.bytes(10, "AutoFilter").map_err(e)?.to_vec();
                // Strings of the criteria follow, in order, as 8-bit-length byte or UTF-16 text.
                let mut p = Pieces::new(record);
                p.skip(24, "AutoFilter").map_err(|m| m.0)?;
                let mut x = Xml { out: Vec::new() };
                let join_or = flags & 3 == 1;
                let top10 = flags & 0x10 != 0;
                if top10 {
                    let items = (flags >> 7) & 0x1FF;
                    let mut a = vec![("val", items.to_string())];
                    if flags & 0x20 == 0 {
                        a.push(("top", "0".to_owned()));
                    }
                    if flags & 0x40 != 0 {
                        a.push(("percent", "1".to_owned()));
                    }
                    x.open("filterColumn", &[("colId", column.to_string())]);
                    x.empty("top10", &a);
                    x.close("filterColumn");
                } else {
                    let mut criteria = Vec::new();
                    for doper in [&first, &second] {
                        if let Some(criterion) = criterion(doper, &mut p, v5)? {
                            criteria.push(criterion);
                        }
                    }
                    if criteria.is_empty() {
                        return Ok(true);
                    }
                    x.open("filterColumn", &[("colId", column.to_string())]);
                    let mut a = Vec::new();
                    if criteria.len() > 1 && !join_or {
                        a.push(("and", "1".to_owned()));
                    }
                    x.open("customFilters", &a);
                    for (operator, value) in criteria {
                        let mut f = Vec::new();
                        if operator != "equal" {
                            f.push(("operator", operator.to_owned()));
                        }
                        f.push(("val", value));
                        x.empty("customFilter", &f);
                    }
                    x.close("customFilters");
                    x.close("filterColumn");
                }
                self.filter_columns.push(FilterColumn {
                    column,
                    xml: x.out,
                });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// `<autoFilter>` over `filter_range` (from the sheet's `_FilterDatabase` name), with the
    /// columns that carry criteria.
    pub(crate) fn write_auto_filter(&self, x: &mut Xml, filter_range: Option<&str>) {
        let Some(range) = filter_range else { return };
        if !self.filter_info && self.filter_columns.is_empty() {
            return;
        }
        if self.filter_columns.is_empty() {
            x.empty("autoFilter", &[("ref", range.to_owned())]);
            return;
        }
        x.open("autoFilter", &[("ref", range.to_owned())]);
        for column in &self.filter_columns {
            let _ = column.column;
            x.out.extend_from_slice(&column.xml);
        }
        x.close("autoFilter");
    }

    /// The `conditionalFormatting` and `dataValidations` elements.
    pub(crate) fn write_rules(&self, x: &mut Xml) {
        for format in &self.formats {
            if format.rules.is_empty() {
                continue;
            }
            x.open("conditionalFormatting", &[("sqref", format.sqref.clone())]);
            for rule in &format.rules {
                x.out.extend_from_slice(rule.as_bytes());
            }
            x.close("conditionalFormatting");
        }
        if !self.validations.is_empty() {
            x.open("dataValidations", &[("count", self.validations.len().to_string())]);
            for validation in &self.validations {
                let _ = &validation.sqref;
                x.out.extend_from_slice(&validation.xml);
            }
            x.close("dataValidations");
        }
    }
}

/// The four strings of a DV record (prompt title, error title, prompt, error) and where the
/// formulas begin: each a 16-bit length, a flag byte and the characters.
fn dv_strings(body: &[u8]) -> Result<(Vec<String>, usize), String> {
    let mut at = 4usize;
    let mut out = Vec::new();
    for _ in 0..4 {
        let count = usize::from(u16::from_le_bytes([
            *body.get(at).ok_or("a DV record that is cut off")?,
            *body.get(at + 1).ok_or("a DV record that is cut off")?,
        ]));
        at += 2;
        let flags = *body.get(at).ok_or("a DV record that is cut off")?;
        let width = if flags & 1 != 0 { 2 } else { 1 };
        let bytes = body.get(at + 1..at + 1 + count * width).ok_or("a DV string that is cut off")?;
        let text = if width == 2 {
                String::from_utf16_lossy(
                    &bytes.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect::<Vec<_>>(),
                )
            } else {
                bytes.iter().map(|b| char::from(*b)).collect()
            };
        at += 1 + count * width;
        out.push(text.trim_end_matches('\0').to_owned());
    }
    Ok((out, at))
}

/// One AutoFilter criterion (a DOPER, 10 bytes): its comparison and value, `None` when unused.
fn criterion(doper: &[u8], strings: &mut Pieces<'_>, v5: Option<u16>) -> Result<Option<(&'static str, String)>, String> {
    let kind = doper[0];
    let sign = doper[1];
    let operator = match sign {
        1 => "lessThan",
        2 => "equal",
        3 => "lessThanOrEqual",
        4 => "greaterThan",
        5 => "notEqual",
        6 => "greaterThanOrEqual",
        _ => "equal",
    };
    Ok(match kind {
        0 => None,
        2 => {
            let rk = u32::from_le_bytes([doper[2], doper[3], doper[4], doper[5]]);
            let bytes = rk.to_le_bytes();
            let mut c = Cursor::new(&bytes);
            Some((operator, crate::xlsb::formula::number_text(c.rk("RK").map_err(e)?)))
        }
        4 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(&doper[2..10]);
            Some((operator, crate::xlsb::formula::number_text(f64::from_le_bytes(b))))
        }
        6 => {
            // BIFF8 uses a Unicode option byte; BIFF5 stores code-page bytes directly.
            let count = usize::from(doper[6]);
            let text = if let Some(page) = v5 {
                let bytes = (0..count).map(|_| strings.u8("a filter text")).collect::<Result<Vec<_>, _>>().map_err(|m| m.0)?;
                super::codepage::decode(page, &bytes)?
            } else {
                let flag = strings.u8("AutoFilter").map_err(|m| m.0)?;
                let units = strings.characters(count, flag & 1 != 0, "a filter text").map_err(|m| m.0)?;
                String::from_utf16_lossy(&units)
            };
            Some((operator, text))
        }
        8 => Some((operator, if doper[3] != 0 { "#N/A".to_owned() } else if doper[2] != 0 { "TRUE".to_owned() } else { "FALSE".to_owned() })),
        0xC => Some(("equal", String::new())),
        0xE => Some(("notEqual", String::new())),
        other => return Err(format!("the filter value type {other}")),
    })
}

// ---- Differential formats ----

#[derive(Default)]
pub(crate) struct Dxf {
    font: Vec<u8>,
    font_color: Option<u16>,
    border_words: Option<(u32, u32, u32)>,
    pattern_words: Option<(u16, u16, u16)>,
}

impl Dxf {
    fn font(&mut self, block: &[u8]) {
        let word = |at: usize| u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        let options = word(68);
        let weight = u16::from_le_bytes([block[72], block[73]]);
        let style_modified = word(88) & 0x02 == 0;
        let strike_modified = word(88) & 0x80 == 0;
        let underline_modified = word(96) == 0;
        let color = word(80);
        let mut x = Xml { out: Vec::new() };
        if style_modified && weight >= 700 {
            x.empty("b", &[]);
        }
        if style_modified && options & 0x02 != 0 {
            x.empty("i", &[]);
        }
        if strike_modified && options & 0x80 != 0 {
            x.empty("strike", &[]);
        }
        if underline_modified {
            match block[76] {
                0 => {}
                0x02 => x.empty("u", &[("val", "double".to_owned())]),
                _ => x.empty("u", &[]),
            }
        }
        if color != 0xFFFF_FFFF {
            self.font_color = Some(color as u16);
        }
        self.font = x.out;
    }

    fn border(&mut self, block: &[u8], flags: u32) {
        let first = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
        let second = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
        self.border_words = Some((first, second, flags));
    }

    fn pattern(&mut self, block: &[u8]) {
        let style = u16::from_le_bytes([block[0], block[1]]) >> 10;
        let colors = u16::from_le_bytes([block[2], block[3]]);
        self.pattern_words = Some((style, colors & 0x7F, (colors >> 7) & 0x7F));
    }

    /// The `<dxf>` element.
    fn xml(&self, styles: &Styles) -> Vec<u8> {
        let mut x = Xml { out: Vec::new() };
        x.open("dxf", &[]);
        if !self.font.is_empty() || self.font_color.is_some() {
            x.open("font", &[]);
            x.out.extend_from_slice(&self.font);
            if let Some(color) = self.font_color {
                x.empty("color", &styles.color_attributes(color));
            }
            x.close("font");
        }
        if let Some((style, fore, back)) = self.pattern_words
            && style != 0
        {
            x.open("fill", &[]);
            x.open("patternFill", &[("patternType", styles.pattern_name(style).to_owned())]);
            x.empty("fgColor", &styles.color_attributes(fore));
            x.empty("bgColor", &styles.color_attributes(back));
            x.close("patternFill");
            x.close("fill");
        }
        if let Some((first, second, _flags)) = self.border_words {
            x.open("border", &[]);
            for (name, style, color) in [
                ("left", first & 0xF, (first >> 16) & 0x7F),
                ("right", (first >> 4) & 0xF, (first >> 23) & 0x7F),
                ("top", (first >> 8) & 0xF, second & 0x7F),
                ("bottom", (first >> 12) & 0xF, (second >> 7) & 0x7F),
            ] {
                if style != 0 {
                    x.open(name, &[("style", styles.border_style_name(style).to_owned())]);
                    x.empty("color", &styles.color_attributes(color as u16));
                    x.close(name);
                }
            }
            x.close("border");
        }
        x.close("dxf");
        x.out
    }
}
