//! Strings of BIFF8 records (MS-XLS 2.5.293 and 2.5.240): a length, a flag byte, then characters
//! compressed to one byte each or as UTF-16. A record that continues in CONTINUE records may be cut
//! inside a string's characters, and the continuation then begins with a fresh flag byte saying
//! whether the characters after the cut are compressed; a cut anywhere else (before the run
//! data, between strings) has none. [`Pieces`] reads across those cuts.

use super::records::{Malformed, Record};

/// A record's body and its CONTINUE bodies, read in order.
pub(crate) struct Pieces<'a> {
    pieces: Vec<&'a [u8]>,
    piece: usize,
    at: usize,
}

fn short(what: &str) -> Malformed {
    Malformed(format!("the record ends inside {what}"))
}

/// The text continuations of a TxO precede separate formatting-run continuations.
pub(crate) fn txo_text(record: &Record<'_>) -> Result<Option<Vec<u16>>, Malformed> {
    let body = record.body;
    let Some(first) = record.continues.first() else { return Ok(None) };
    if body.len() < 14 || first.is_empty() { return Err(short("Txo text")); }
    let count = usize::from(u16::from_le_bytes([body[10], body[11]]));
    let first_count = (first.len() - 1) / if first[0] & 1 != 0 { 2 } else { 1 };
    let mut text_end = 1;
    if first_count < count {
        let mut remaining = count;
        for piece in &record.continues {
            if piece.is_empty() || piece[0] > 1 { return Err(short("Txo text")); }
            let width = if piece[0] == 1 { 2 } else { 1 };
            if (piece.len() - 1) % width != 0 { return Err(short("Txo text")); }
            let units = (piece.len() - 1) / width;
            if units > remaining || units == 0 { return Err(short("Txo text")); }
            remaining -= units;
            if remaining == 0 { break; }
            text_end += 1;
        }
        if remaining != 0 { return Err(short("Txo text")); }
        let runs = usize::from(u16::from_le_bytes([body[12], body[13]]));
        let formatting = &record.continues[text_end..];
        let total = formatting.iter().map(|piece| piece.len()).sum::<usize>();
        let followed_by_shape = formatting.last().is_some_and(|piece| {
            total.checked_sub(piece.len()) == Some(runs) && super::drawing::is_continued_shape(piece)
        });
        if total != runs && !followed_by_shape {
            return Err(short("Txo formatting runs"));
        }
    }
    let mut pieces = Pieces::new(&Record { id: 0, body: first,
        continues: record.continues[1..text_end].to_vec(), offset: 0 });
    let flag = pieces.u8("Txo")?;
    Ok(Some(pieces.characters(count, flag & 1 != 0, "Txo text")?))
}

/// The explicitly bounded formatting runs following a TxO's character continuations.
pub(crate) fn txo_runs(record: &Record<'_>, text: &[u16]) -> Option<Vec<(u16, u16)>> {
    let body = record.body;
    if body.len() < 14 || text.is_empty() { return None; }
    let count = usize::from(u16::from_le_bytes([body[10], body[11]]));
    let size = usize::from(u16::from_le_bytes([body[12], body[13]]));
    if count != text.len() || size < 16 || size % 8 != 0 { return None; }
    let mut remaining = count;
    let mut end = 0;
    for piece in &record.continues {
        if piece.is_empty() || piece[0] > 1 { return None; }
        let width = if piece[0] == 1 { 2 } else { 1 };
        if (piece.len() - 1) % width != 0 { return None; }
        let units = (piece.len() - 1) / width;
        if units == 0 || units > remaining { return None; }
        remaining -= units; end += 1;
        if remaining == 0 { break; }
    }
    if remaining != 0 { return None; }
    let mut bytes = Vec::with_capacity(size);
    for piece in &record.continues[end..] {
        if bytes.len().checked_add(piece.len())? > size { return None; }
        bytes.extend_from_slice(piece);
        if bytes.len() == size { break; }
    }
    if bytes.len() != size || usize::from(u16::from_le_bytes([bytes[size-8], bytes[size-7]])) != count { return None; }
    let mut runs = Vec::new();
    for chunk in bytes[..size-8].chunks_exact(8) {
        let at = u16::from_le_bytes([chunk[0], chunk[1]]);
        let font = u16::from_le_bytes([chunk[2], chunk[3]]);
        let index = usize::from(at);
        if index >= count || (index > 0 && (0xdc00..=0xdfff).contains(&text[index])
            && (0xd800..=0xdbff).contains(&text[index-1]))
            || runs.last().is_some_and(|&(previous, _)| previous >= at) { return None; }
        runs.push((at, font));
    }
    (runs.first().is_some_and(|(at, _)| *at == 0)).then_some(runs)
}

/// A string with its formatting runs (character index, font index), as the shared string table
/// holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RichString {
    pub(crate) units: Vec<u16>,
    pub(crate) runs: Vec<(u16, u16)>,
    /// The phonetic extension is not yet converted to SpreadsheetML.
    pub(crate) phonetic_unread: bool,
    pub(crate) phonetic: Option<Phonetic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Phonetic {
    pub(crate) font: u16,
    pub(crate) kind: u8,
    pub(crate) alignment: u8,
    pub(crate) runs: Vec<PhoneticRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhoneticRun {
    pub(crate) first: u16,
    pub(crate) end: u16,
    pub(crate) text: Vec<u16>,
}

/// Preserve formatting independently when the run encoding is not yet supported.
fn phonetic_format(bytes: &[u8]) -> Option<Phonetic> {
    let mut c = crate::xlsb::Cursor::new(bytes);
    let what = "phonetic string";
    if c.u16(what).ok()? != 1 || usize::from(c.u16(what).ok()?) + 4 != bytes.len() { return None; }
    let font = c.u16(what).ok()?;
    let flags = c.u16(what).ok()?;
    Some(Phonetic { font, kind: (flags & 3) as u8, alignment: ((flags >> 2) & 3) as u8, runs: Vec::new() })
}

/// Parse ExtRst data. Excel's conversion accepts one trailing word and maps
/// phonetic text without explicit runs to the complete base string.
fn phonetic_extension(bytes: &[u8], base_len: usize) -> Option<Phonetic> {
    let mut c = crate::xlsb::Cursor::new(bytes);
    let what = "phonetic string";
    if c.u16(what).ok()? != 1 || usize::from(c.u16(what).ok()?) + 4 != bytes.len() { return None; }
    let font = c.u16(what).ok()?;
    let flags = c.u16(what).ok()?;
    let count = usize::from(c.u16(what).ok()?);
    let units = usize::from(c.u16(what).ok()?);
    if usize::from(c.u16(what).ok()?) != units { return None; }
    let content_size = units * 2 + count * 6;
    if !matches!(c.remaining().checked_sub(content_size)?, 0 | 2) { return None; }
    let text = c.units(units, what).ok()?;
    if count == 0 {
        let runs = if text.is_empty() {
            Vec::new()
        } else {
            let end = u16::try_from(base_len).ok().filter(|&len| len != 0)?;
            vec![PhoneticRun { first: 0, end, text }]
        };
        return Some(Phonetic { font, kind: (flags & 3) as u8, alignment: ((flags >> 2) & 3) as u8,
            runs });
    }
    let mut positions = Vec::with_capacity(count);
    let mut base_units = 0usize;
    for _ in 0..count {
        let phonetic = c.u16(what).ok()?;
        let first = c.u16(what).ok()?;
        let length = c.u16(what).ok()?;
        let end = usize::from(first).checked_add(usize::from(length))?;
        base_units = base_units.checked_add(usize::from(length))?;
        if base_units > base_len { return None; }
        if usize::from(phonetic) >= units || end > base_len || end > usize::from(u16::MAX) { return None; }
        if positions.last().is_some_and(|&(p, b, _)| p >= phonetic || b >= first) { return None; }
        positions.push((phonetic, first, end as u16));
    }
    if positions.first()?.0 != 0 { return None; }
    let mut runs = Vec::with_capacity(count);
    for (i, &(start, first, end)) in positions.iter().enumerate() {
        let stop = positions.get(i + 1).map_or(units, |next| usize::from(next.0));
        runs.push(PhoneticRun { first, end, text: text[usize::from(start)..stop].to_vec() });
    }
    Some(Phonetic { font, kind: (flags & 3) as u8, alignment: ((flags >> 2) & 3) as u8, runs })
}

impl<'a> Pieces<'a> {
    pub(crate) fn new(record: &Record<'a>) -> Self {
        let mut pieces = vec![record.body];
        pieces.extend(record.continues.iter().copied());
        Pieces {
            pieces,
            piece: 0,
            at: 0,
        }
    }

    /// Whether every byte has been read.
    pub(crate) fn done(&self) -> bool {
        let mut at = self.at;
        for piece in &self.pieces[self.piece..] {
            if at < piece.len() {
                return false;
            }
            at = 0;
        }
        true
    }

    fn settle(&mut self) {
        while self.piece + 1 < self.pieces.len() && self.at >= self.pieces[self.piece].len() {
            self.piece += 1;
            self.at = 0;
        }
    }

    pub(crate) fn u8(&mut self, what: &str) -> Result<u8, Malformed> {
        self.settle();
        let byte = *self.pieces[self.piece].get(self.at).ok_or_else(|| short(what))?;
        self.at += 1;
        Ok(byte)
    }

    pub(crate) fn u16(&mut self, what: &str) -> Result<u16, Malformed> {
        let lo = self.u8(what)?;
        let hi = self.u8(what)?;
        Ok(u16::from_le_bytes([lo, hi]))
    }

    pub(crate) fn u32(&mut self, what: &str) -> Result<u32, Malformed> {
        let lo = self.u16(what)?;
        let hi = self.u16(what)?;
        Ok(u32::from(lo) | u32::from(hi) << 16)
    }

    pub(crate) fn skip(&mut self, mut count: usize, what: &str) -> Result<(), Malformed> {
        while count > 0 {
            self.settle();
            let left = self.pieces[self.piece].len().saturating_sub(self.at);
            if left == 0 {
                return Err(short(what));
            }
            let step = left.min(count);
            self.at += step;
            count -= step;
        }
        Ok(())
    }

    /// `count` characters whose first flag byte (bit 0: UTF-16) is `utf16`; at a cut, the
    /// continuation's own flag byte replaces it.
    pub(crate) fn characters(
        &mut self,
        count: usize,
        mut utf16: bool,
        what: &str,
    ) -> Result<Vec<u16>, Malformed> {
        let mut out = Vec::with_capacity(count);
        while out.len() < count {
            let width = if utf16 { 2 } else { 1 };
            let left = self.pieces[self.piece].len().saturating_sub(self.at) / width;
            if left == 0 {
                // BIFF requires a UTF-16 continuation to start between characters. Do not
                // discard a trailing half-character and silently return a different string.
                if utf16 && self.at < self.pieces[self.piece].len() {
                    return Err(short(what));
                }
                if self.piece + 1 >= self.pieces.len() {
                    return Err(short(what));
                }
                self.piece += 1;
                self.at = 0;
                utf16 = self.u8(what)? & 1 != 0;
                continue;
            }
            let take = left.min(count - out.len());
            let bytes = &self.pieces[self.piece][self.at..self.at + take * width];
            if utf16 {
                out.extend(bytes.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])));
            } else {
                out.extend(bytes.iter().map(|b| u16::from(*b)));
            }
            self.at += take * width;
        }
        Ok(out)
    }

    /// An XLUnicodeString (2.5.294): a 16-bit count, a flag byte, the characters.
    pub(crate) fn unicode_string(&mut self, what: &str) -> Result<Vec<u16>, Malformed> {
        let count = usize::from(self.u16(what)?);
        let flags = self.u8(what)?;
        self.characters(count, flags & 1 != 0, what)
    }

    /// An XLUnicodeRichExtendedString (2.5.293): count, flags (1 UTF-16, 4 phonetic data, 8
    /// formatting runs), run count, phonetic size, characters, runs, phonetic data.
    pub(crate) fn rich_string(&mut self) -> Result<RichString, Malformed> {
        let what = "a shared string";
        let count = usize::from(self.u16(what)?);
        let flags = self.u8(what)?;
        let run_count = if flags & 0x08 != 0 { usize::from(self.u16(what)?) } else { 0 };
        let phonetic = if flags & 0x04 != 0 { self.u32(what)? as usize } else { 0 };
        let units = self.characters(count, flags & 1 != 0, what)?;
        let mut runs = Vec::with_capacity(run_count);
        for _ in 0..run_count {
            runs.push((self.u16(what)?, self.u16(what)?));
        }
        let (parsed, phonetic_unread) = if phonetic == 0 {
            (None, false)
        } else if phonetic <= 65539 {
            let mut bytes = Vec::with_capacity(phonetic);
            for _ in 0..phonetic { bytes.push(self.u8(what)?); }
            // Excel preserves an ExtRst split across CONTINUE without a character flag.
            if let Some(parsed) = phonetic_extension(&bytes, units.len()) {
                (Some(parsed), false)
            } else {
                (phonetic_format(&bytes), true)
            }
        } else {
            self.skip(phonetic, what)?;
            (None, true)
        };
        Ok(RichString { units, runs, phonetic_unread, phonetic: parsed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record<'a>(body: &'a [u8], continues: Vec<&'a [u8]>) -> Record<'a> {
        Record {
            id: 0,
            body,
            continues,
            offset: 0,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_cut_inside_characters_restarts_the_flag_byte() {
        // "abcdef": compressed "abc", cut, then UTF-16 "de" and "f" would need flag 1.
        let body = [6u8, 0, 0, b'a', b'b', b'c'];
        let tail = [1u8, b'd', 0, b'e', 0, b'f', 0];
        let r = record(&body, vec![&tail]);
        let mut p = Pieces::new(&r);
        let units = p.unicode_string("test").unwrap();
        assert_eq!(String::from_utf16(&units).unwrap(), "abcdef");
        assert!(p.done());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rich_strings_keep_runs_and_skip_phonetic_data_across_a_cut() {
        // "hi" with two runs and 3 bytes of phonetic data, cut between the two runs.
        let body = [2u8, 0, 0x0C, 2, 0, 3, 0, 0, 0, b'h', b'i', 0, 0, 5, 0];
        let tail = [1u8, 0, 7, 0, 9, 9, 9];
        let r = record(&body, vec![&tail]);
        let mut p = Pieces::new(&r);
        let s = p.rich_string().unwrap();
        assert_eq!(String::from_utf16(&s.units).unwrap(), "hi");
        assert_eq!(s.runs, vec![(0, 5), (1, 7)]);
        assert!(s.phonetic_unread);
        assert!(p.done());
        // The same string without its phonetic bytes is a typed failure, not a panic.
        let short = record(&body, vec![&tail[..4]]);
        assert!(Pieces::new(&short).rich_string().is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phonetic_mapping_refuses_out_of_bounds_and_unaccounted_data() {
        let mut bytes = Vec::new();
        for value in [1u16, 18, 0, 7, 1, 1, 1, b'x' as u16, 0, 0, 1] {
            bytes.extend(value.to_le_bytes());
        }
        assert!(phonetic_extension(&bytes, 1).is_some());
        assert!(phonetic_extension(&bytes, 0).is_none());
        bytes.extend([0, 0]);
        assert!(phonetic_extension(&bytes, 1).is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phonetic_zero_runs_cover_the_base_string() {
        let mut bytes = Vec::new();
        for value in [1u16, 16, 0, 7, 0, 3, 3, b'x' as u16, b'y' as u16, b'z' as u16] {
            bytes.extend(value.to_le_bytes());
        }
        let phonetic = phonetic_extension(&bytes, 2).unwrap();
        assert_eq!(phonetic.runs, vec![PhoneticRun { first: 0, end: 2, text: vec![120, 121, 122] }]);
        assert!(phonetic_extension(&bytes, 0).is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phonetic_extension_crossing_continue_is_retained() {
        let mut ext = Vec::new();
        for value in [1u16, 18, 0, 7, 1, 1, 1, b'x' as u16, 0, 0, 1] { ext.extend(value.to_le_bytes()); }
        let mut body = vec![1, 0, 4];
        body.extend((ext.len() as u32).to_le_bytes()); body.push(b'A');
        body.extend(&ext[..10]);
        let r = record(&body, vec![&ext[10..]]);
        let item = Pieces::new(&r).rich_string().unwrap();
        let phonetic = item.phonetic.unwrap();
        assert_eq!(phonetic.runs, vec![PhoneticRun { first: 0, end: 1, text: vec![u16::from(b'x')] }]);
        assert!(!item.phonetic_unread);
        assert_eq!(item.units, vec![u16::from(b'A')]);
    }
}
