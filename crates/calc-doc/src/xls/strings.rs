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

/// A string with its formatting runs (character index, font index), as the shared string table
/// holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RichString {
    pub(crate) units: Vec<u16>,
    pub(crate) runs: Vec<(u16, u16)>,
    /// The phonetic extension is not yet converted to SpreadsheetML.
    pub(crate) phonetic_unread: bool,
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
        self.skip(phonetic, what)?;
        Ok(RichString { units, runs, phonetic_unread: phonetic != 0 })
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
}
