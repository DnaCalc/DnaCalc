//! The control names of a UserForm, read from the form's designer storage (`f` stream, MS-OFORMS
//! 2.2.10 `FormControl`, `FormSiteData`, `OleSiteConcreteControl`). The names are what the form's
//! code and every module that shows the form refer to (`TextBox1.Text`, `UserForm1.ListBox1`).
//! Nothing else of the form is read: no property, position or picture; a control's class is not
//! needed to compile code against it (dc-1jia7).
//!
//! Layout facts: OxForms `ms_oforms_persistence_facts.v1.json` (MS-OFORMS revision 9.1): field
//! order, the `FormPropMask`, `FormFlags`, `SitePropMask` and `FormObjectDepthTypeCount` bit
//! layouts, `CountOfBytesWithCompressionFlag`.

use std::io::{Cursor, Read};

/// A form whose designer storage cannot be read says why; the caller declares no controls.
#[derive(Debug, PartialEq, Eq)]
pub struct FormReadError(pub String);

fn bad<T>(what: &str) -> Result<T, FormReadError> {
    Err(FormReadError(format!("form designer stream: {what}")))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormReadError> {
        match self.at.checked_add(n).filter(|end| *end <= self.bytes.len()) {
            Some(end) => {
                let out = &self.bytes[self.at..end];
                self.at = end;
                Ok(out)
            }
            None => bad("ends inside a record"),
        }
    }
    fn u8(&mut self) -> Result<u8, FormReadError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, FormReadError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, FormReadError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn align(&mut self, to: usize, origin: usize) -> Result<(), FormReadError> {
        let rem = (self.at - origin) % to;
        if rem != 0 {
            self.take(to - rem)?;
        }
        Ok(())
    }
}

/// `FormPropMask` bits that say a stream follows the form's data blocks.
const FORM_MASK_MOUSE_ICON: u32 = 1 << 15;
const FORM_MASK_FONT: u32 = 1 << 20;
const FORM_MASK_PICTURE: u32 = 1 << 21;
/// `FormFlags` (the form's `BooleanProperties`): no class table is stored.
const FORM_FLAG_DONTSAVECLASSTABLE: u32 = 1 << 15;

/// `{AFC20920-DA4E-11CE-B943-00AA006887B4}`, the FormFont type `TextProps`, in file byte order.
const TEXT_PROPS_GUID: [u8; 16] =
    [0x20, 0x09, 0xC2, 0xAF, 0x4E, 0xDA, 0xCE, 0x11, 0xB9, 0x43, 0x00, 0xAA, 0x00, 0x68, 0x87, 0xB4];

/// Reads the names of the embedded controls from the form's `f` stream, in site order.
/// `code_page` decodes names stored compressed (one byte a character).
pub fn control_names(f: &[u8], code_page: u16) -> Result<Vec<String>, FormReadError> {
    let mut r = Reader { bytes: f, at: 0 };
    let (minor, major) = (r.u8()?, r.u8()?);
    if (minor, major) != (0, 4) {
        return bad("is not a FormControl (version 4.0)");
    }
    let cb_form = r.u16()? as usize;
    let form = r.take(cb_form)?;
    if form.len() < 4 {
        return bad("has no property mask");
    }
    let mask = u32::from_le_bytes([form[0], form[1], form[2], form[3]]);
    // The DataBlock follows the mask: BackColor, ForeColor and NextAvailableID (4 bytes each, if
    // present) come before BooleanProperties.
    let before_flags = [1u32 << 1, 1 << 2, 1 << 3].iter().filter(|bit| mask & **bit != 0).count();
    let flags = if mask & (1 << 6) != 0 {
        let at = 4 + 4 * before_flags;
        match form.get(at..at + 4) {
            Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            None => return bad("ends inside the form's data block"),
        }
    } else {
        0
    };
    // StreamData: MouseIcon, Font and Picture, each present when its mask bit is set.
    if mask & FORM_MASK_MOUSE_ICON != 0 {
        skip_picture(&mut r)?;
    }
    if mask & FORM_MASK_FONT != 0 {
        let guid = r.take(16)?; // FontGUID: the type of FormFont that follows
        if guid == TEXT_PROPS_GUID {
            // TextProps: MinorVersion, MajorVersion, cbTextProps, then that many bytes.
            r.take(2)?;
            let cb = r.u16()? as usize;
            r.take(cb)?;
        } else {
            r.take(1 + 2 + 1 + 2 + 4)?; // StdFont: Version, Charset, Flags, Weight, Height
            let face = r.u8()? as usize;
            r.take(face)?;
        }
    }
    if mask & FORM_MASK_PICTURE != 0 {
        skip_picture(&mut r)?;
    }
    // SiteData.
    if flags & FORM_FLAG_DONTSAVECLASSTABLE == 0 {
        let classes = r.u16()?;
        for _ in 0..classes {
            r.u16()?; // Version
            let cb = r.u16()? as usize;
            r.take(cb)?;
        }
    }
    let sites = r.u32()? as usize;
    let cb_sites = r.u32()? as usize;
    let start = r.at;
    // SiteDepthsAndTypes: Depth, then TypeOrCount (7 bits; bit 7 says it is a count of consecutive
    // controls sharing the type, which follows).
    let mut counted = 0usize;
    while counted < sites {
        r.u8()?;
        let type_or_count = r.u8()?;
        if type_or_count & 0x80 != 0 {
            r.u8()?;
            counted += (type_or_count & 0x7F) as usize;
        } else {
            counted += 1;
        }
    }
    if counted != sites {
        return bad("site depths do not add up to the site count");
    }
    r.align(4, start)?;
    let mut names = Vec::with_capacity(sites);
    for _ in 0..sites {
        names.push(site_name(&mut r, code_page)?);
    }
    if r.at - start != cb_sites {
        return bad("sites do not fill the byte count the site data states");
    }
    Ok(names)
}

fn skip_picture(r: &mut Reader<'_>) -> Result<(), FormReadError> {
    r.take(16)?; // CLSID_StdPicture
    if r.u32()? != 0x0000_746C {
        return bad("has a picture without its preamble");
    }
    let size = r.u32()? as usize;
    r.take(size)?;
    Ok(())
}

/// One `OleSiteConcreteControl`: the name of the control (empty when the site has none).
fn site_name(r: &mut Reader<'_>, code_page: u16) -> Result<String, FormReadError> {
    r.u16()?; // Version
    let cb = r.u16()? as usize;
    let site = r.take(cb)?;
    let mut s = Reader { bytes: site, at: 0 };
    let mask = s.u32()?;
    // DataBlock, in order, each only when its mask bit is set, aligned to its own size from the
    // start of the block.
    let origin = s.at;
    let mut name_data = None;
    let present = |bit: u32| mask & (1 << bit) != 0;
    if present(0) {
        name_data = Some(s.u32()?);
    }
    for bit in [1, 2, 3, 4, 5] {
        if present(bit) {
            s.u32()?; // TagData, ID, HelpContextID, BitFlags, ObjectStreamSize
        }
    }
    for bit in [6, 7, 9] {
        if present(bit) {
            s.u16()?; // TabIndex, ClsidCacheIndex, GroupID
        }
    }
    for bit in [11, 12, 13, 14] {
        if present(bit) {
            s.align(4, origin)?;
            s.u32()?;
        }
    }
    s.align(4, origin)?;
    let Some(name_data) = name_data else {
        return Ok(String::new());
    };
    let (length, compressed) = ((name_data & 0x7FFF_FFFF) as usize, name_data & 0x8000_0000 != 0);
    let bytes = s.take(length)?;
    Ok(decode(bytes, compressed, code_page))
}

fn decode(bytes: &[u8], compressed: bool, _code_page: u16) -> String {
    if compressed {
        // One byte a character. Control names are identifiers, so ASCII in practice; any other
        // byte is read as Latin-1 rather than guessed at.
        bytes.iter().map(|b| char::from(*b)).collect()
    } else {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    }
}

/// `source` of a form module with one `Public <name> As Object` line per control, after the
/// module's leading `Option` statements. The controls are members of the form class, which is
/// what `TextBox1.Text` inside the form and `UserForm1.TextBox1` outside it resolve to; their
/// classes are late bound, so a call on one compiles whatever its control type. A name that is
/// not a VBA identifier, or repeats an earlier control's, is left out.
pub fn declare_controls(source: &str, controls: &[String]) -> String {
    let mut seen = std::collections::BTreeSet::new();
    let declarations: String = controls
        .iter()
        .filter(|name| is_identifier(name) && seen.insert(name.to_lowercase()))
        .map(|name| format!("Public {name} As Object\r\n"))
        .collect();
    if declarations.is_empty() {
        return source.to_owned();
    }
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let leading = trimmed.is_empty()
            || trimmed.starts_with('\'')
            || trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("option ");
        if !leading {
            break;
        }
        at += line.len();
    }
    let mut out = String::with_capacity(source.len() + declarations.len());
    out.push_str(&source[..at]);
    if at > 0 && !source[..at].ends_with('\n') {
        out.push_str("\r\n");
    }
    out.push_str(&declarations);
    out.push_str(&source[at..]);
    out
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(char::is_alphabetic)
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().count() <= 255
}

/// The `f` stream of the form `module` in the project blob, when the blob has a storage for it.
pub fn designer_stream(project: &[u8], module: &str) -> Result<Option<Vec<u8>>, FormReadError> {
    let mut compound = cfb::CompoundFile::open(Cursor::new(project))
        .map_err(|e| FormReadError(format!("project storage: {e}")))?;
    let path = format!("/{module}/f");
    if !compound.exists(&path) {
        return Ok(None);
    }
    let mut stream = compound
        .open_stream(&path)
        .map_err(|e| FormReadError(format!("{path}: {e}")))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|e| FormReadError(format!("{path}: {e}")))?;
    Ok(Some(bytes))
}

/// The names of the controls of the form `module` in the project blob: none when the blob has no
/// designer storage for it or the storage is not a readable form stream.
pub fn controls_of(project: &[u8], module: &str, code_page: u16) -> Vec<String> {
    designer_stream(project, module)
        .ok()
        .flatten()
        .and_then(|f| control_names(&f, code_page).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/support/form_stream.rs"]
mod form_stream;

#[cfg(test)]
mod tests {
    use super::*;
    use super::form_stream::{Font, form_stream as author};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn control_names_are_read_in_site_order_whatever_the_font_storage_or_string_encoding() {
        let names = [("TextBox1", true), ("Ünicode", false), ("lstInfo", true)];
        for font in [Font::None, Font::TextProps, Font::StdFont] {
            let got = control_names(&author(&names, font), 1252).unwrap();
            assert_eq!(got, ["TextBox1", "Ünicode", "lstInfo"]);
        }
        assert_eq!(control_names(&author(&[], Font::TextProps), 1252).unwrap(), Vec::<String>::new());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_stream_that_is_not_a_form_or_is_cut_short_is_refused_not_guessed() {
        let good = author(&[("TextBox1", true)], Font::TextProps);
        let mut wrong_version = good.clone();
        wrong_version[1] = 3;
        assert!(control_names(&wrong_version, 1252).is_err());
        for cut in [0, 3, 10, good.len() - 1] {
            assert!(control_names(&good[..cut], 1252).is_err(), "cut at {cut}");
        }
        // Sites that do not fill the byte count the site data states.
        let mut padded = good.clone();
        // The byte count precedes the depths (2 bytes + 2 padding) and the site (2 + 2 + 4 + 4 + name).
        let at = padded.len() - (4 + 12 + "TextBox1".len()) - 4;
        padded[at] = padded[at].wrapping_add(1);
        assert!(control_names(&padded, 1252).is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn controls_are_declared_after_the_leading_option_statements_once_each() {
        let names: Vec<String> =
            ["TextBox1", "textbox1", "not valid", "1st", "lstInfo"].map(String::from).to_vec();
        assert_eq!(
            declare_controls("Option Explicit\r\n' note\r\nPrivate Sub A()\r\nEnd Sub\r\n", &names),
            "Option Explicit\r\n' note\r\nPublic TextBox1 As Object\r\nPublic lstInfo As Object\r\nPrivate Sub A()\r\nEnd Sub\r\n"
        );
        assert_eq!(declare_controls("Sub A()\nEnd Sub", &names[4..]), "Public lstInfo As Object\r\nSub A()\nEnd Sub");
        assert_eq!(declare_controls("Option Explicit", &names[4..]), "Option Explicit\r\nPublic lstInfo As Object\r\n");
        assert_eq!(declare_controls("Sub A()\nEnd Sub", &[]), "Sub A()\nEnd Sub");
    }
}
