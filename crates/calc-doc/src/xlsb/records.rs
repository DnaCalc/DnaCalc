//! The record stream of an Excel binary workbook part (MS-XLSB 2.1.4): each record a record type
//! and a size, both variable-length integers of 7-bit groups, then that many bytes; and the
//! fields records are made of (2.5): little-endian integers, IEEE doubles, the RK number form,
//! wide strings (a 32-bit count of UTF-16 code units, then the units) and cell ranges.

/// Why a part's records cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Malformed(pub(crate) String);

pub(crate) type Read<T> = Result<T, Malformed>;

fn short(what: &str) -> Malformed {
    Malformed(format!("the record ends inside {what}"))
}

/// One record: its type and its body.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Record<'a> {
    pub(crate) kind: u16,
    pub(crate) body: &'a [u8],
}

/// The records of a part, in order.
pub(crate) fn records(data: &[u8]) -> Read<Vec<Record<'_>>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < data.len() {
        let mut kind: u32 = 0;
        for k in 0..2 {
            let byte = *data
                .get(at)
                .ok_or_else(|| Malformed(format!("a record type cut off at byte {at}")))?;
            at += 1;
            kind |= u32::from(byte & 0x7F) << (7 * k);
            if byte & 0x80 == 0 {
                break;
            }
        }
        let mut size: u64 = 0;
        for k in 0..4 {
            let byte = *data
                .get(at)
                .ok_or_else(|| Malformed(format!("a record size cut off at byte {at}")))?;
            at += 1;
            size |= u64::from(byte & 0x7F) << (7 * k);
            if byte & 0x80 == 0 {
                break;
            }
        }
        let end = at
            .checked_add(size as usize)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| {
                Malformed(format!("record {kind} of {size} bytes runs past the part"))
            })?;
        out.push(Record {
            kind: kind as u16,
            body: &data[at..end],
        });
        at = end;
    }
    Ok(out)
}

/// A reader over one record's body.
#[derive(Clone, Copy)]
pub(crate) struct Cursor<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) at: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Cursor { data, at: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.at)
    }

    pub(crate) fn bytes(&mut self, n: usize, what: &str) -> Read<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.data.len());
        let end = end.ok_or_else(|| short(what))?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub(crate) fn skip(&mut self, n: usize, what: &str) -> Read<()> {
        self.bytes(n, what).map(|_| ())
    }

    pub(crate) fn u8(&mut self, what: &str) -> Read<u8> {
        Ok(self.bytes(1, what)?[0])
    }

    pub(crate) fn u16(&mut self, what: &str) -> Read<u16> {
        let b = self.bytes(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self, what: &str) -> Read<u32> {
        let b = self.bytes(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn i32(&mut self, what: &str) -> Read<i32> {
        Ok(self.u32(what)? as i32)
    }

    pub(crate) fn f64(&mut self, what: &str) -> Read<f64> {
        let b = self.bytes(8, what)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    /// An RK number (2.5.122): a 30-bit integer or the high 30 bits of a double, divided by 100
    /// when its low bit says so.
    pub(crate) fn rk(&mut self, what: &str) -> Read<f64> {
        let raw = self.u32(what)?;
        let divided = raw & 1 != 0;
        let value = if raw & 2 != 0 {
            f64::from((raw as i32) >> 2)
        } else {
            f64::from_bits(u64::from(raw & 0xFFFF_FFFC) << 32)
        };
        Ok(if divided { value / 100.0 } else { value })
    }

    /// An XLWideString (2.5.166): a 32-bit count of UTF-16 code units, then the units.
    pub(crate) fn wide(&mut self, what: &str) -> Read<Vec<u16>> {
        let count = self.u32(what)? as usize;
        self.units(count, what)
    }

    /// A nullable wide string: a count of 0xFFFFFFFF is no string.
    pub(crate) fn nullable_wide(&mut self, what: &str) -> Read<Option<Vec<u16>>> {
        let count = self.u32(what)?;
        if count == 0xFFFF_FFFF {
            return Ok(None);
        }
        self.units(count as usize, what).map(Some)
    }

    pub(crate) fn units(&mut self, count: usize, what: &str) -> Read<Vec<u16>> {
        let bytes = self.bytes(count.checked_mul(2).ok_or_else(|| short(what))?, what)?;
        Ok(bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect())
    }

    /// An RfX (2.5.124): first row, last row, first column, last column, 0-based.
    pub(crate) fn rfx(&mut self, what: &str) -> Read<Area0> {
        Ok(Area0 {
            first_row: self.u32(what)?,
            last_row: self.u32(what)?,
            first_column: self.u32(what)?,
            last_column: self.u32(what)?,
        })
    }
}

/// A cell range with 0-based rows and columns, as records hold it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area0 {
    pub(crate) first_row: u32,
    pub(crate) last_row: u32,
    pub(crate) first_column: u32,
    pub(crate) last_column: u32,
}

/// The text of UTF-16 code units, unpaired surrogates replaced.
pub(crate) fn text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn record_headers_and_rk_numbers() {
        // A record of type 0x0817 (two bytes of the type) with a 2-byte body, then BrtRowHdr (0)
        // with an empty body.
        let data = [0x97, 0x10, 0x02, 0xAA, 0xBB, 0x00, 0x00];
        let read = records(&data).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!((read[0].kind, read[0].body), (0x0817, &[0xAA, 0xBB][..]));
        assert_eq!((read[1].kind, read[1].body.len()), (0, 0));
        // RK: 2 as an integer, 1.5 as a double's high bits, and 1.23 as 123 divided by 100.
        let rk = |raw: u32| Cursor::new(&raw.to_le_bytes()).rk("rk").unwrap();
        assert_eq!(rk(2 << 2 | 2), 2.0);
        assert_eq!(rk((1.5f64.to_bits() >> 32) as u32), 1.5);
        assert_eq!(rk(123 << 2 | 3), 1.23);
        assert!(records(&[0x05, 0x09, 0x00]).is_err());
    }
}
