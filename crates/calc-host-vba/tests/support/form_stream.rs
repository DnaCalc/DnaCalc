//! Authors a UserForm designer `f` stream (MS-OFORMS 2.2.10 `FormControl` with `FormSiteData`)
//! for tests: a synthetic control, never an Excel observation. Only what the reader needs is
//! written: the form's font, and one site per name with just its `Name`.
#![allow(dead_code)]

/// How the form's font is stored (`FormStreamData.GuidAndFont`).
#[derive(Clone, Copy)]
pub enum Font {
    None,
    TextProps,
    StdFont,
}

pub fn form_stream(names: &[(&str, bool)], font: Font) -> Vec<u8> {
    let mut out = vec![0, 4];
    // FormControl: PropMask (fFont), DataBlock (Font marker 0xFFFF, padded to 4).
    let (mask, data): (u32, &[u8]) = match font {
        Font::None => (0, &[]),
        _ => (1 << 20, &[0xFF, 0xFF, 0, 0]),
    };
    let cb_form = (4 + data.len()) as u16;
    out.extend(cb_form.to_le_bytes());
    out.extend(mask.to_le_bytes());
    out.extend(data);
    match font {
        Font::None => {}
        Font::TextProps => {
            out.extend([0x20, 0x09, 0xC2, 0xAF, 0x4E, 0xDA, 0xCE, 0x11, 0xB9, 0x43, 0, 0xAA, 0, 0x68, 0x87, 0xB4]);
            // TextProps: minor 0, major 2, cb 8, PropMask (fFontHeight), DataBlock (FontHeight).
            out.extend([0, 2, 8, 0, 4, 0, 0, 0, 0xB8, 0x0B, 0, 0]);
        }
        Font::StdFont => {
            out.extend([0x03, 0x52, 0xE3, 0x0B, 0x91, 0x8F, 0xCE, 0x11, 0x9D, 0xE3, 0, 0xAA, 0, 0x4B, 0xB8, 0x51]);
            // StdFont: version 1, charset, flags, weight, height, face length, face.
            out.extend([1, 0, 0, 0, 0x90, 1, 0x10, 0x27, 0, 0, 5]);
            out.extend(b"Arial");
        }
    }
    // FormSiteData: no class table entries, then the site count and byte count.
    out.extend(0u16.to_le_bytes());
    out.extend((names.len() as u32).to_le_bytes());
    let mut sites = Vec::new();
    // SiteDepthsAndTypes: depth 0, a single control of site type 1 (ST_Ole) each.
    for _ in names {
        sites.extend([0u8, 1]);
    }
    while sites.len() % 4 != 0 {
        sites.push(0);
    }
    for (name, compressed) in names {
        let bytes: Vec<u8> = if *compressed {
            name.bytes().collect()
        } else {
            name.encode_utf16().flat_map(u16::to_le_bytes).collect()
        };
        let name_data = bytes.len() as u32 | if *compressed { 0x8000_0000 } else { 0 };
        sites.extend(0u16.to_le_bytes()); // Version
        sites.extend(((4 + 4 + bytes.len()) as u16).to_le_bytes()); // cbSite
        sites.extend(1u32.to_le_bytes()); // SitePropMask: fName
        sites.extend(name_data.to_le_bytes());
        sites.extend(bytes);
    }
    out.extend((sites.len() as u32).to_le_bytes());
    out.extend(sites);
    out
}
