//! Raw BIFF3/4 worksheet records adapted to the existing workbook conversion route.
//! Format facts: OpenOffice.org Excel format documentation, rev. 1.42, sections 3,
//! 5.8, 5.33, 5.45, 5.49, 5.50, 5.74 and 5.115. No formula tokens are rewritten here.

use super::{LegacyReport, cfb_write::{self, Node}, records::{self, Record}};
use crate::{OpenError, xlsb::formula::Layout};

fn bad(detail: impl std::fmt::Display) -> OpenError {
    super::refuse("(raw BIFF stream)", format!("{detail} (dc-4j0e.14)"))
}

fn record(out: &mut Vec<u8>, id: u16, body: &[u8]) -> Result<(), OpenError> {
    let size = u16::try_from(body.len()).map_err(|_| bad("an oversized adapted record"))?;
    out.extend(id.to_le_bytes()); out.extend(size.to_le_bytes()); out.extend(body);
    Ok(())
}

fn original(out: &mut Vec<u8>, r: &Record<'_>) -> Result<(), OpenError> {
    record(out, r.id, r.body)?;
    for body in &r.continues { record(out, records::CONTINUE, body)?; }
    Ok(())
}

fn word(b: &[u8], at: usize) -> u16 { u16::from_le_bytes([b[at], b[at + 1]]) }

fn color(index: u32) -> Result<u32, OpenError> {
    match index { 0..=23 => Ok(index), 24 => Ok(64), 25 => Ok(65),
        _ => Err(bad(format!("the BIFF3/4 colour index {index}"))) }
}

fn xf(b: &[u8]) -> Result<Vec<u8>, OpenError> {
    if b.len() != 12 { return Err(bad("a short or extended BIFF3/4 XF")); }
    let area = u32::from(word(b, 6));
    let border = u32::from_le_bytes(b[8..12].try_into().expect("four bytes"));
    let pattern = area & 63;
    if pattern > 18 || (b[4] >> 4) & 3 == 3 { return Err(bad("an unsupported BIFF3/4 XF pattern or alignment")); }
    let top = border & 7;
    let left = (border >> 8) & 7;
    let bottom = (border >> 16) & 7;
    let right = (border >> 24) & 7;
    // Undefined colour bits on absent borders/patterns do not describe a visible colour.
    let line_color = |shift: u32, style: u32| if style == 0 { Ok(0) } else { color((border >> shift) & 31) };
    let fill = if pattern == 0 { 0 } else { color((area >> 6) & 31)? | color((area >> 11) & 31)? << 7 | pattern << 16 };
    let fill = fill | bottom << 22 | line_color(19, bottom)? << 25;
    let lines = top | left << 3 | right << 6 | line_color(3, top)? << 9
        | line_color(11, left)? << 16 | line_color(27, right)? << 23;
    let mut out = Vec::with_capacity(16);
    out.extend(u16::from(b[0]).to_le_bytes()); out.extend(u16::from(b[1]).to_le_bytes());
    out.extend(&b[2..4]); out.push(b[4] & 0x3f); out.push((b[5] & 0xfc) | (b[4] >> 6));
    out.extend(fill.to_le_bytes()); out.extend(lines.to_le_bytes());
    Ok(out)
}

fn font(b: &[u8]) -> Result<Vec<u8>, OpenError> {
    if b.len() < 7 || usize::from(b[6]) + 7 > b.len() { return Err(bad("a malformed BIFF3/4 FONT")); }
    let flags = word(b, 2);
    let mut out = b[..6].to_vec();
    out.extend((if flags & 1 != 0 { 700u16 } else { 400 }).to_le_bytes());
    out.extend(0u16.to_le_bytes()); // BIFF3/4 has no separate escapement field.
    out.extend([u8::from(flags & 4 != 0), 0, 0, 0]);
    // Some old writers append unused bytes after the length-prefixed name.
    out.extend(&b[6..7 + usize::from(b[6])]);
    let index = word(b, 4);
    if (26..=0x7ffe).contains(&index) { return Err(bad("an undefined old font colour index")); }
    out[4..6].copy_from_slice(&(match index { 24 => 64u16, 25 => 65u16, _ => index }).to_le_bytes());
    Ok(out)
}

/// The input remains the document's legacy source. The temporary compound stream only lets
/// the existing cell/style/package path consume the adapted record headers and fields.
pub(super) fn convert_biff34(bytes: &[u8], version: u8, password: Option<&str>) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    let records = records::read_all(bytes).map_err(|e| bad(e.0))?;
    let first = records.first().ok_or_else(|| bad("no BOF record"))?;
    if first.id != if version == 3 { 0x0209 } else { 0x0409 } || first.body.len() != 6 || word(first.body, 2) != 0x10 {
        return Err(bad(format!("a raw BIFF{version} substream other than a worksheet")));
    }
    if records.last().is_none_or(|r| r.id != 0x000a) { return Err(bad("the raw worksheet has no final EOF")); }
    if records.iter().take(records.len().saturating_sub(1)).any(|r| r.id == 0x000a) {
        return Err(bad("records after the raw worksheet's EOF"));
    }
    if records.iter().skip(1).any(|r| matches!(r.id, 0x0409 | 0x0209 | 0x0009 | 0x0809)) {
        return Err(bad("nested raw BIFF substreams"));
    }
    if records.iter().any(|r| r.id == 0x002f) { return Err(bad("an encrypted raw BIFF3/4 worksheet")); }
    let mut palette: Vec<[u8; 4]> = super::styles::DEFAULT_PALETTE.iter().map(|rgb|
        [(*rgb >> 16) as u8, (*rgb >> 8) as u8, *rgb as u8, 0]).collect();
    let saved_palettes: Vec<_> = records.iter().filter(|r| r.id == 0x0092).collect();
    if saved_palettes.len() > 1 { return Err(bad("multiple raw BIFF palettes")); }
    if let Some(r) = saved_palettes.first() {
        if !r.continues.is_empty() || r.body.len() != 66 || word(r.body, 0) != 16 {
            return Err(bad("a BIFF3/4 palette other than sixteen colours"));
        }
        for (target, source) in palette.iter_mut().zip(r.body[2..].chunks_exact(4)) {
            target.copy_from_slice(source);
        }
    }
    let mut grid_colors = std::collections::BTreeMap::new();
    let mut spare = 16;
    for r in records.iter().filter(|r| r.id == 0x023e) {
        let b = r.body;
        if b.len() != 10 || !r.continues.is_empty() { return Err(bad("a malformed old WINDOW2")); }
        if word(b, 0) & 0x20 != 0 { continue; }
        let rgb = [b[6], b[7], b[8]];
        if grid_colors.contains_key(&rgb) { continue; }
        let index = if let Some(index) = palette.iter().take(16).position(|p| p[..3] == rgb) { index } else {
            // Old XFs and fonts use palette indexes 0..23, plus system 24/25 (mapped
            // to 64/65). The adapted palette's extra entries are free for RGB grids.
            if spare >= palette.len() { return Err(bad("too many distinct raw grid colours")); }
            palette[spare] = [rgb[0], rgb[1], rgb[2], 0]; spare += 1; spare - 1
        };
        grid_colors.insert(rgb, (index + 8) as u16);
    }
    let mut globals = Vec::new();
    if !saved_palettes.is_empty() || !grid_colors.is_empty() {
        let mut b = 56u16.to_le_bytes().to_vec();
        for rgb in &palette { b.extend(rgb); }
        record(&mut globals, 0x0092, &b)?;
    }
    let mut sheet = Vec::new();
    let mut formats = 0u16;
    for r in records.iter().skip(1).take(records.len().saturating_sub(2)) {
        let b = r.body;
        if matches!(r.id, 0x0201 | 0x0203 | 0x0204 | 0x0205 | 0x027e | 0x0206 | 0x0406)
            && (b.len() < 6 || word(b, 0) >= 16384 || word(b, 2) >= 256) {
            return Err(bad("a BIFF3/4 cell outside its grid or with a short address"));
        }
        match r.id {
            0x0231 | 0x0243 | 0x0443 | 0x001e | 0x041e | 0x0218 if !r.continues.is_empty() => {
                return Err(bad(format!("a continued BIFF3/4 record {:#06x}", r.id)));
            }
            0x0231 => record(&mut globals, 0x0031, &font(b)?)?,
            0x0243 if version == 3 => {
                if b.len() != 12 { return Err(bad("a short or extended BIFF3 XF")); }
                let mut adapted = b.to_vec();
                let protection = (word(b, 4) & 0xfff0) | u16::from(b[2] & 7);
                adapted[2..4].copy_from_slice(&protection.to_le_bytes());
                // BIFF3 has no vertical alignment; the default is bottom alignment.
                adapted[4] = (b[4] & 15) | 0x20;
                adapted[5] = b[3] & 0xfc;
                record(&mut globals, 0x00e0, &xf(&adapted)?)?;
            }
            0x001e if version == 3 => {
                if b.is_empty() || usize::from(b[0]) + 1 != b.len() { return Err(bad("a malformed BIFF3 FORMAT")); }
                let mut out = formats.to_le_bytes().to_vec(); out.extend(b);
                record(&mut globals, 0x041e, &out)?;
                formats = formats.checked_add(1).ok_or_else(|| bad("too many BIFF3 number formats"))?;
            }
            0x0443 => record(&mut globals, 0x00e0, &xf(b)?)?,
            0x041e => {
                if b.len() < 3 || usize::from(b[2]) + 3 != b.len() { return Err(bad("a malformed BIFF3/4 FORMAT")); }
                let mut out = formats.to_le_bytes().to_vec(); out.extend(&b[2..]);
                record(&mut globals, 0x041e, &out)?;
                formats = formats.checked_add(1).ok_or_else(|| bad("too many BIFF3/4 number formats"))?;
            }
            0x0218 => {
                if b.len() < 6 || 6 + usize::from(b[3]) + usize::from(word(b, 4)) > b.len() {
                    return Err(bad("a malformed BIFF3/4 defined name"));
                }
                let mut out = b[..6].to_vec(); out.extend([0u8; 8]); out.extend(&b[6..]);
                record(&mut globals, 0x0018, &out)?;
            }
            0x0206 | 0x0406 | 0x0221 => {
                if !r.continues.is_empty() { return Err(bad("a continued BIFF3/4 formula record")); }
                let prefix = if r.id == 0x0221 { 8 } else { 16 };
                if b.len() < prefix + 2 { return Err(bad("a short BIFF3/4 formula record")); }
                let mut out = b[..prefix].to_vec(); out.extend([0u8; 4]); out.extend(&b[prefix..]);
                record(&mut sheet, if r.id == 0x0221 { 0x0221 } else { 0x0006 }, &out)?;
            }
            0x0092 => {} // The single palette above includes any grid RGB additions.
            0x023e if word(b, 0) & 0x20 == 0 => {
                let rgb = [b[6], b[7], b[8]];
                let index = grid_colors.get(&rgb).ok_or_else(|| bad("a missing grid colour mapping"))?;
                let mut window = b[..6].to_vec(); window.extend(index.to_le_bytes()); window.extend([0, 0]);
                record(&mut sheet, 0x023e, &window)?;
            }
            // These properties belong to the synthetic workbook globals; their record
            // bodies use the same layouts. Window1 stays unread rather than guessing units.
            0x000e | 0x0019 | 0x0022 | 0x0040 | 0x0042 | 0x005c | 0x008c | 0x0293 => original(&mut globals, r)?,
            _ => original(&mut sheet, r)?,
        }
    }
    let bof = |kind: u16| [0x0500u16.to_le_bytes(), kind.to_le_bytes(), 1u16.to_le_bytes(), 1995u16.to_le_bytes()].concat();
    let mut stream = Vec::new(); record(&mut stream, 0x0809, &bof(5))?;
    stream.extend(globals);
    // There is no stored sheet name in a single raw worksheet stream.
    let mut bound = vec![0u8; 4]; bound.extend([0, 0, 6]); bound.extend(b"Sheet1");
    let offset = u32::try_from(stream.len() + 4 + bound.len() + 4).map_err(|_| bad("a raw worksheet too large to adapt"))?;
    bound[..4].copy_from_slice(&offset.to_le_bytes());
    record(&mut stream, 0x0085, &bound)?; record(&mut stream, 0x000a, &[])?;
    record(&mut stream, 0x0809, &bof(0x10))?; stream.extend(sheet); record(&mut stream, 0x000a, &[])?;
    let compound = cfb_write::write(vec![("Workbook".into(), Node::Stream(stream))])
        .ok_or_else(|| bad("the adapted raw worksheet exceeds the compound writer limit"))?;
    let (package, mut report) = super::convert_compound(&compound, password, Some(if version == 3 { Layout::Biff3 } else { Layout::Biff4 })).map_err(|error| match error {
        OpenError::LegacyWorkbook { detail, .. } => bad(detail),
        other => other,
    })?;
    report.biff_version = version;
    Ok((package, report))
}

// BIFF2 cell attributes either reference the old XF list or carry the formatting inline.
// This initial subset declines borders/shading and formulas rather than guessing their
// older layouts. Format facts: the same specification, sections 2.5.13, 5.7, 5.10,
// 5.33, 5.45, 5.47, 5.60, 5.62, 5.63, 5.71 and 5.115.
fn xf2(font: u8, format: u8, protection: u8, alignment: u8) -> Result<Vec<u8>, OpenError> {
    if alignment & !7 != 0 || alignment & 7 > 4 {
        return Err(bad("BIFF2 borders, shading or unsupported alignment"));
    }
    Ok(vec![font, format, protection & 3, 0xfc, alignment, 0, 0, 0, 0, 0, 0, 0])
}

pub(super) fn convert_biff2(bytes: &[u8], password: Option<&str>) -> Result<(Vec<u8>, LegacyReport), OpenError> {
    let records = records::read_all(bytes).map_err(|e| bad(e.0))?;
    let first = records.first().ok_or_else(|| bad("no BIFF2 BOF"))?;
    if first.id != 9 || first.body.len() != 4 || word(first.body, 2) != 0x10 {
        return Err(bad("a raw BIFF2 substream other than a worksheet"));
    }
    let mut adapted = Vec::new();
    record(&mut adapted, 0x0209, &[0, 0, 0x10, 0, 0, 0])?;
    // A style XF precedes the old cell XFs, whose indexes are shifted by one.
    record(&mut adapted, 0x0243, &[0, 0, 5, 0xfc, 0xf0, 0xff, 0, 0, 0, 0, 0, 0])?;
    let mut count = 0u16;
    for r in records.iter().filter(|r| r.id == 0x0043) {
        let b = r.body;
        if b.len() != 4 || !r.continues.is_empty() { return Err(bad("a malformed BIFF2 XF")); }
        record(&mut adapted, 0x0243, &xf2(b[0], b[2] & 63, b[2] >> 6, b[3])?)?;
        count = count.checked_add(1).ok_or_else(|| bad("too many BIFF2 XFs"))?;
    }
    let source_xfs = count;
    let mut inline = std::collections::BTreeMap::new();
    let mut extended = None;
    let mut names = 0;
    for (i, r) in records.iter().enumerate().skip(1) {
        let b = r.body;
        if extended.is_some() && !matches!(r.id, 1..=5) {
            return Err(bad("a BIFF2 IXFE without its following cell"));
        }
        match r.id {
            0x0043 => {}
            0x0044 => {
                if b.len() != 2 || !r.continues.is_empty() { return Err(bad("a malformed BIFF2 IXFE")); }
                extended = Some(word(b, 0));
            }
            0x0031 => {
                if b.len() < 5 || usize::from(b[4]) + 5 > b.len() || !r.continues.is_empty() {
                    return Err(bad("a malformed BIFF2 FONT"));
                }
                let mut color = 0x7fff;
                if let Some(next) = records.get(i + 1).filter(|r| r.id == 0x0045) {
                    if next.body.len() != 2 || !next.continues.is_empty() { return Err(bad("a malformed BIFF2 FONTCOLOR")); }
                    color = word(next.body, 0);
                    if color > 7 && color != 0x7fff { return Err(bad("an unsupported BIFF2 font colour")); }
                }
                let mut font = b[..4].to_vec(); font.extend(color.to_le_bytes());
                font.extend(&b[4..5 + usize::from(b[4])]);
                record(&mut adapted, 0x0231, &font)?;
            }
            0x0045 => {
                if records.get(i - 1).is_none_or(|r| r.id != 0x0031) { return Err(bad("an unattached BIFF2 FONTCOLOR")); }
            }
            1..=5 => {
                let valid = match r.id { 1 => b.len() == 7, 2 | 5 => b.len() == 9,
                    3 => b.len() == 15, 4 => b.len() >= 8 && b.len() == 8 + usize::from(b[7]), _ => false };
                if !valid || !r.continues.is_empty() || word(b, 0) >= 16384 || word(b, 2) >= 256 {
                    return Err(bad("a malformed BIFF2 cell or an address outside its grid"));
                }
                let index = if source_xfs != 0 {
                    let index = if b[4] & 63 == 63 { extended.take().ok_or_else(|| bad("a BIFF2 cell missing IXFE"))? }
                        else { if extended.take().is_some() { return Err(bad("an unused BIFF2 IXFE")); } u16::from(b[4] & 63) };
                    if index >= source_xfs { return Err(bad("a BIFF2 cell referencing a missing XF")); }
                    index + 1
                } else {
                    if extended.take().is_some() { return Err(bad("BIFF2 IXFE without an XF table")); }
                    let key = [b[4] & 0xc0, b[5], b[6]];
                    if let Some(index) = inline.get(&key) { *index } else {
                        count = count.checked_add(1).ok_or_else(|| bad("too many inline BIFF2 styles"))?;
                        record(&mut adapted, 0x0243, &xf2(b[5] >> 6, b[5] & 63, b[4] >> 6, b[6])?)?;
                        inline.insert(key, count); count
                    }
                };
                let mut cell = b[..4].to_vec(); cell.extend(index.to_le_bytes());
                let id = match r.id {
                    1 => 0x0201,
                    2 => { cell.extend(f64::from(word(b, 7)).to_le_bytes()); 0x0203 }
                    3 => { cell.extend(&b[7..]); 0x0203 }
                    4 => { cell.extend(u16::from(b[7]).to_le_bytes()); cell.extend(&b[8..]); 0x0204 }
                    5 => { cell.extend(&b[7..]); 0x0205 }
                    _ => unreachable!(),
                };
                record(&mut adapted, id, &cell)?;
            }
            0x0018 => {
                if b.len() < 6 || !r.continues.is_empty() || b[0] & 6 != 0 {
                    return Err(bad("a malformed, macro or complex BIFF2 name"));
                }
                let length = usize::from(b[3]); let formula = usize::from(b[4]);
                if b.len() != 6 + length + formula || b[b.len() - 1] != b[4] {
                    return Err(bad("a malformed BIFF2 name length"));
                }
                let tokens = &b[5 + length..5 + length + formula];
                let simple = match tokens.first().map(|t| (t & 31) | 32) {
                    Some(0x24 | 0x2c) => tokens.len() == 4,
                    Some(0x25 | 0x2d) => tokens.len() == 7,
                    _ => false,
                };
                if !simple { return Err(bad("BIFF2 named expressions other than a single cell or range")); }
                let mut name = vec![0, 0, 0, b[3]]; name.extend(u16::from(b[4]).to_le_bytes());
                name.extend(&b[5..5 + length + formula]); record(&mut adapted, 0x0218, &name)?;
                names += 1;
            }
            0x003e if b.len() == 14 && r.continues.is_empty()
                && b[..5].iter().chain(&b[9..10]).all(|v| *v <= 1) => {
                if word(b, 5) >= 16384 || word(b, 7) >= 256 { return Err(bad("a BIFF2 view outside its grid")); }
                let mut flags = u16::from(b[9]) << 5;
                for (bit, value) in b[..5].iter().enumerate() { flags |= u16::from(*value) << bit; }
                let mut window = flags.to_le_bytes().to_vec(); window.extend(&b[5..9]); window.extend(&b[10..14]);
                record(&mut adapted, 0x023e, &window)?;
            }
            0x0006 | 0x0007 | 0x0021 => return Err(bad("raw BIFF2 formula and array layouts are not read")),
            _ => original(&mut adapted, r)?,
        }
    }
    let (package, mut report) = convert_biff34(&adapted, 3, password)?;
    report.biff_version = 2;
    // BIFF2 names have different flags and localised built-in names; only simple
    // user range definitions are projected. Keep the original record type incomplete.
    if names != 0 { *report.unknown_records.entry(0x0018).or_default() += names; }
    Ok((package, report))
}
