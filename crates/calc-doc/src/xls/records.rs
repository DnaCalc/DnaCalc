//! The BIFF record stream (MS-XLS 2.1.4): each record a 16-bit type, a 16-bit size and that many
//! bytes. A record longer than 8224 bytes continues in CONTINUE records (0x003C), which follow it;
//! the pieces are kept apart because strings and drawing data restart their own framing at a piece
//! boundary, and the rest read the pieces as one body.

pub(crate) const CONTINUE: u16 = 0x003C;

/// One logical record: a type, its body and the bodies of the CONTINUE records after it.
#[derive(Clone, Debug)]
pub(crate) struct Record<'a> {
    pub(crate) id: u16,
    pub(crate) body: &'a [u8],
    pub(crate) continues: Vec<&'a [u8]>,
    /// The byte offset of the record's header in its stream.
    pub(crate) offset: usize,
}

impl<'a> Record<'a> {
    /// The body and the continuations as one byte string.
    pub(crate) fn joined(&self) -> std::borrow::Cow<'a, [u8]> {
        if self.continues.is_empty() {
            std::borrow::Cow::Borrowed(self.body)
        } else {
            let mut all = self.body.to_vec();
            for piece in &self.continues {
                all.extend_from_slice(piece);
            }
            std::borrow::Cow::Owned(all)
        }
    }
}

/// Why a record stream cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Malformed(pub(crate) String);

/// The records of `data` in order, each CONTINUE folded into the record before it. A header or body
/// cut off by the end of the stream ends the list when it is at the very end (a stream padded
/// with zeros or truncated mid-record, as damaged files are) and is otherwise not possible.
pub(crate) fn read_all(data: &[u8]) -> Result<Vec<Record<'_>>, Malformed> {
    let mut out: Vec<Record<'_>> = Vec::new();
    let mut at = 0usize;
    while at + 4 <= data.len() {
        let id = u16::from_le_bytes([data[at], data[at + 1]]);
        let size = usize::from(u16::from_le_bytes([data[at + 2], data[at + 3]]));
        let start = at + 4;
        if start + size > data.len() {
            // A truncated last record is dropped; the caller finds the missing EOF if it matters.
            break;
        }
        let body = &data[start..start + size];
        if id == CONTINUE {
            match out.last_mut() {
                Some(previous) => previous.continues.push(body),
                None => return Err(Malformed("a CONTINUE record opens the stream".into())),
            }
        } else {
            out.push(Record {
                id,
                body,
                continues: Vec::new(),
                offset: at,
            });
        }
        at = start + size;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn continue_records_fold_into_the_record_before() {
        let mut data = Vec::new();
        for (id, body) in [(0x00FCu16, &[1u8, 2][..]), (CONTINUE, &[3, 4][..]), (0x000A, &[][..])] {
            data.extend_from_slice(&id.to_le_bytes());
            data.extend_from_slice(&(body.len() as u16).to_le_bytes());
            data.extend_from_slice(body);
        }
        let records = read_all(&data).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, 0x00FC);
        assert_eq!(records[0].continues, vec![&[3u8, 4][..]]);
        assert_eq!(&*records[0].joined(), &[1, 2, 3, 4]);
        assert_eq!((records[1].id, records[1].offset), (0x000A, 12));
        // A truncated last record is dropped, a stream opening with CONTINUE is malformed.
        assert_eq!(read_all(&data[..data.len() - 2]).unwrap().len(), 1);
        assert_eq!(read_all(&data[..10]).unwrap().len(), 1);
        assert!(read_all(&[0x3C, 0x00, 0x00, 0x00]).is_err());
    }
}
