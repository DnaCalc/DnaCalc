//! Instruments for the cdoc-m1.1 cases, all independent of calc-doc's own code: a ZIP reader and a
//! ZIP builder written here from the ZIP format's description, SHA-256 and CRC-32, a per-thread
//! allocation counter, and package sources that record or change what they return.

use calc_doc::{PackageRead, PackageWrite, SinkError, SourceError};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Mutex;

// ---------------------------------------------------------------------------------------------
// SHA-256 (FIPS 180-4) and CRC-32 (ISO-HDLC).

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The SHA-256 of `data`, as lowercase hex.
pub fn sha256(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

/// The CRC-32 of `data`, bit by bit.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

// ---------------------------------------------------------------------------------------------
// A ZIP reader.

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// One entry as its central record and local header give it.
#[derive(Clone, Debug)]
pub struct ZipEntry {
    pub name: String,
    pub version_made_by: u16,
    pub version_needed: u16,
    pub flags: u16,
    pub method: u16,
    pub time: u16,
    pub date: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub size: u32,
    pub internal_attributes: u16,
    pub external_attributes: u32,
    pub local_offset: u32,
    pub central_extra: Vec<u8>,
    pub comment: Vec<u8>,
    /// The local header's 30 fixed bytes.
    pub local_header: Vec<u8>,
    pub local_extra: Vec<u8>,
    pub payload_start: usize,
    pub payload_end: usize,
    /// The data descriptor's bytes; empty when the entry has none.
    pub descriptor: Vec<u8>,
}

impl ZipEntry {
    pub fn payload<'a>(&self, archive: &'a [u8]) -> &'a [u8] {
        &archive[self.payload_start..self.payload_end]
    }

    /// The end of the entry's stored bytes.
    pub fn end(&self) -> usize {
        self.payload_end + self.descriptor.len()
    }
}

/// An archive's entries in central directory order, its central directory's place and its
/// comment.
#[derive(Debug)]
pub struct Zip {
    pub entries: Vec<ZipEntry>,
    pub central_offset: usize,
    pub central_size: usize,
    pub comment: Vec<u8>,
}

impl Zip {
    pub fn entry(&self, name: &str) -> &ZipEntry {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("the archive has an entry {name:?}"))
    }
}

/// Reads an archive, asserting it is well formed.
pub fn parse_zip(bytes: &[u8]) -> Zip {
    assert!(bytes.len() >= 22, "an archive has an end record");
    let end = (0..=bytes.len() - 22)
        .rev()
        .find(|&at| {
            u32_at(bytes, at) == 0x0605_4b50
                && at + 22 + usize::from(u16_at(bytes, at + 20)) == bytes.len()
        })
        .expect("an end record whose comment ends the archive");
    let count = usize::from(u16_at(bytes, end + 10));
    assert_eq!(count, usize::from(u16_at(bytes, end + 8)), "one disk");
    let central_size = u32_at(bytes, end + 12) as usize;
    let central_offset = u32_at(bytes, end + 16) as usize;
    assert_eq!(
        central_offset + central_size,
        end,
        "the central directory ends at the end record"
    );
    let mut entries = Vec::new();
    let mut at = central_offset;
    for _ in 0..count {
        assert_eq!(u32_at(bytes, at), 0x0201_4b50, "a central record at {at}");
        let name_len = usize::from(u16_at(bytes, at + 28));
        let extra_len = usize::from(u16_at(bytes, at + 30));
        let comment_len = usize::from(u16_at(bytes, at + 32));
        let name_start = at + 46;
        let name = String::from_utf8(bytes[name_start..name_start + name_len].to_vec())
            .expect("a UTF-8 entry name");
        let local_offset = u32_at(bytes, at + 42);
        let flags = u16_at(bytes, at + 8);
        let compressed_size = u32_at(bytes, at + 20);
        let local = local_offset as usize;
        assert_eq!(
            u32_at(bytes, local),
            0x0403_4b50,
            "a local header at {local}"
        );
        let local_name_len = usize::from(u16_at(bytes, local + 26));
        let local_extra_len = usize::from(u16_at(bytes, local + 28));
        assert_eq!(
            &bytes[local + 30..local + 30 + local_name_len],
            name.as_bytes(),
            "the local header names its entry"
        );
        let payload_start = local + 30 + local_name_len + local_extra_len;
        let payload_end = payload_start + compressed_size as usize;
        let descriptor_len = if flags & 8 == 0 {
            0
        } else if u32_at(bytes, payload_end) == 0x0807_4b50 {
            16
        } else {
            12
        };
        entries.push(ZipEntry {
            name,
            version_made_by: u16_at(bytes, at + 4),
            version_needed: u16_at(bytes, at + 6),
            flags,
            method: u16_at(bytes, at + 10),
            time: u16_at(bytes, at + 12),
            date: u16_at(bytes, at + 14),
            crc32: u32_at(bytes, at + 16),
            compressed_size,
            size: u32_at(bytes, at + 24),
            internal_attributes: u16_at(bytes, at + 36),
            external_attributes: u32_at(bytes, at + 38),
            local_offset,
            central_extra: bytes[name_start + name_len..name_start + name_len + extra_len].to_vec(),
            comment: bytes[name_start + name_len + extra_len
                ..name_start + name_len + extra_len + comment_len]
                .to_vec(),
            local_header: bytes[local..local + 30].to_vec(),
            local_extra: bytes[local + 30 + local_name_len..payload_start].to_vec(),
            payload_start,
            payload_end,
            descriptor: bytes[payload_end..payload_end + descriptor_len].to_vec(),
        });
        at = name_start + name_len + extra_len + comment_len;
    }
    assert_eq!(at, end, "the central records fill the central directory");
    Zip {
        entries,
        central_offset,
        central_size,
        comment: bytes[end + 22..].to_vec(),
    }
}

// ---------------------------------------------------------------------------------------------
// A ZIP builder.

/// An entry to build, every field as it is written.
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub name: Vec<u8>,
    pub flags: u16,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub size: u32,
    pub payload: Vec<u8>,
}

impl RawEntry {
    pub fn stored(name: &str, content: &[u8]) -> Self {
        RawEntry {
            name: name.as_bytes().to_vec(),
            flags: 0,
            method: 0,
            crc32: crc32(content),
            compressed_size: content.len() as u32,
            size: content.len() as u32,
            payload: content.to_vec(),
        }
    }

    /// A deflated entry whose payload, declared size and CRC are given as they are.
    pub fn deflated(name: &str, payload: &[u8], size: u32, crc32: u32) -> Self {
        RawEntry {
            name: name.as_bytes().to_vec(),
            flags: 0,
            method: 8,
            crc32,
            compressed_size: payload.len() as u32,
            size,
            payload: payload.to_vec(),
        }
    }

    /// The entry's local header and payload.
    pub fn local(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for field in [20u16, self.flags, self.method, 0, 0x21] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [self.crc32, self.compressed_size, self.size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&self.payload);
        out
    }

    /// The entry's central record, for a local header at `offset`.
    pub fn central(&self, offset: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for field in [20u16, 20, self.flags, self.method, 0, 0x21] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [self.crc32, self.compressed_size, self.size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [self.name.len() as u16, 0, 0, 0, 0] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&self.name);
        out
    }
}

/// An end record for `count` entries whose central directory of `size` bytes starts at `offset`.
pub fn end_record(count: u16, size: u32, offset: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for field in [0u16, 0, count, count] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// An archive of the entries, stored one after another in the order given.
pub fn build_zip(entries: &[RawEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    for entry in entries {
        offsets.push(out.len() as u32);
        out.extend_from_slice(&entry.local());
    }
    let start = out.len();
    for (entry, offset) in entries.iter().zip(offsets) {
        out.extend_from_slice(&entry.central(offset));
    }
    let size = (out.len() - start) as u32;
    out.extend_from_slice(&end_record(entries.len() as u16, size, start as u32));
    out
}

// ---------------------------------------------------------------------------------------------
// DEFLATE streams (RFC 1951), written bit by bit.

/// Bits packed into bytes from each byte's least significant bit up, as DEFLATE packs them.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    used: u32,
}

impl BitWriter {
    fn bit(&mut self, bit: u32) {
        if self.used == 0 {
            self.bytes.push(0);
        }
        *self.bytes.last_mut().expect("a byte") |= ((bit & 1) as u8) << self.used;
        self.used = (self.used + 1) % 8;
    }

    /// A header field: `count` bits of `value`, least significant first.
    fn field(&mut self, value: u32, count: u32) {
        for index in 0..count {
            self.bit(value >> index);
        }
    }

    /// A Huffman code of `count` bits, most significant first.
    fn code(&mut self, code: u32, count: u32) {
        for index in (0..count).rev() {
            self.bit(code >> index);
        }
    }

    /// Pads with zero bits to the next byte boundary.
    fn align(&mut self) {
        while self.used != 0 {
            self.bit(0);
        }
    }
}

/// An empty stored block that is not the final block: BFINAL 0 and BTYPE 00 in the first byte,
/// the rest of it padding, then LEN 0 and NLEN 0xFFFF.
pub const EMPTY_STORED_BLOCK: [u8; 5] = [0x00, 0x00, 0x00, 0xFF, 0xFF];

/// One final stored block holding `content` (at most 65 535 bytes): BFINAL 1 and BTYPE 00, then
/// LEN and NLEN, then the bytes.
pub fn final_stored_block(content: &[u8]) -> Vec<u8> {
    let len = u16::try_from(content.len()).expect("a stored block holds at most 65 535 bytes");
    let mut stream = vec![0x01];
    stream.extend_from_slice(&len.to_le_bytes());
    stream.extend_from_slice(&(!len).to_le_bytes());
    stream.extend_from_slice(content);
    stream
}

/// `content` as a streaming encoder ends it with a sync flush and then finishes: one block that is
/// not the final block, with the fixed Huffman codes, holding `content` as literals (none when
/// `content` is empty, and then no such block), then a sync-flush marker (an empty stored block,
/// its header padded to a byte boundary, then LEN 0 and NLEN 0xFFFF), then an empty final block
/// with the fixed Huffman codes. 7 bytes for no content, 9 for one byte below 144.
pub fn flushed(content: &[u8]) -> Vec<u8> {
    let mut bits = BitWriter::default();
    if !content.is_empty() {
        bits.field(0, 1); // BFINAL 0
        bits.field(1, 2); // BTYPE 01: the fixed Huffman codes
        for &byte in content {
            match byte {
                0..=143 => bits.code(0x30 + u32::from(byte), 8),
                _ => bits.code(0x190 + u32::from(byte - 144), 9),
            }
        }
        bits.code(0, 7); // end of block
    }
    bits.field(0, 1); // BFINAL 0
    bits.field(0, 2); // BTYPE 00: stored
    bits.align();
    bits.field(0, 16); // LEN 0
    bits.field(0xFFFF, 16); // NLEN
    bits.field(1, 1); // BFINAL 1
    bits.field(1, 2); // BTYPE 01
    bits.code(0, 7); // end of block
    bits.bytes
}

/// One final block with the fixed Huffman codes: the literal 0, then `matches` copies of length
/// 258 at distance 1, then the end of the block, so it decodes to 1 + 258 * `matches` zero bytes
/// in 18 + 13 * `matches` bits.
pub fn fixed_zeros(matches: usize) -> Vec<u8> {
    let mut bits = BitWriter::default();
    bits.field(1, 1); // BFINAL
    bits.field(1, 2); // BTYPE 01: the fixed Huffman codes
    bits.code(0x30, 8); // literal 0: symbols 0..=143 have the 8-bit codes 0x30..=0xBF
    for _ in 0..matches {
        bits.code(0xC5, 8); // length 258: symbol 285; symbols 280..=287 have 0xC0..=0xC7
        bits.code(0, 5); // distance 1: distance code 0, 5 bits, no extra bits
    }
    bits.code(0, 7); // end of block: symbol 256; symbols 256..=279 have the 7-bit codes 0..=23
    bits.bytes
}

// ---------------------------------------------------------------------------------------------
// The allocation counter.

thread_local! {
    static ALLOCATED: Cell<u64> = const { Cell::new(0) };
    static LARGEST: Cell<u64> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    let _ = ALLOCATED.try_with(|total| total.set(total.get() + bytes as u64));
    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(bytes as u64)));
}

/// The bytes this thread has asked the allocator for so far (frees are not subtracted).
pub fn allocated() -> u64 {
    ALLOCATED.with(Cell::get)
}

/// The largest single allocation or reallocation, in bytes, this thread has asked for since the
/// last [`reset_largest`].
pub fn largest() -> u64 {
    LARGEST.with(Cell::get)
}

/// Starts [`largest`] again from 0.
pub fn reset_largest() {
    LARGEST.with(|largest| largest.set(0));
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

// ---------------------------------------------------------------------------------------------
// Sources and sinks.

/// A source that records every range it is asked for.
pub struct RecordingSource {
    bytes: Vec<u8>,
    reads: Mutex<Vec<(u64, u64)>>,
}

impl RecordingSource {
    pub fn new(bytes: Vec<u8>) -> Self {
        RecordingSource {
            bytes,
            reads: Mutex::new(Vec::new()),
        }
    }

    /// Every range read so far, as (start, end).
    pub fn reads(&self) -> Vec<(u64, u64)> {
        self.reads.lock().expect("the read log").clone()
    }
}

impl PackageRead for RecordingSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let end = offset + buf.len() as u64;
        self.reads.lock().expect("the read log").push((offset, end));
        let range = offset as usize..end as usize;
        let bytes = self
            .bytes
            .get(range)
            .ok_or_else(|| SourceError::new("a read past the end"))?;
        buf.copy_from_slice(bytes);
        Ok(())
    }
}

/// A source whose bytes the test can change after the package is opened.
pub struct ChangingSource {
    bytes: Mutex<Vec<u8>>,
}

impl ChangingSource {
    pub fn new(bytes: Vec<u8>) -> Self {
        ChangingSource {
            bytes: Mutex::new(bytes),
        }
    }

    pub fn change(&self, edit: impl FnOnce(&mut Vec<u8>)) {
        edit(&mut self.bytes.lock().expect("the source bytes"));
    }
}

impl PackageRead for ChangingSource {
    fn size(&self) -> u64 {
        self.bytes.lock().expect("the source bytes").len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let bytes = self.bytes.lock().expect("the source bytes");
        let range = offset as usize..offset as usize + buf.len();
        let found = bytes.get(range).ok_or_else(|| {
            SourceError::new(format!(
                "{} bytes at {offset} are past the end of {} bytes",
                buf.len(),
                bytes.len()
            ))
        })?;
        buf.copy_from_slice(found);
        Ok(())
    }
}

/// A source whose every read fails.
pub struct FailingSource {
    pub size: u64,
}

impl PackageRead for FailingSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<(), SourceError> {
        Err(SourceError::new("the medium is unreadable"))
    }
}

/// A source with a range of unreadable bytes, which the test can move at any time: a read that
/// touches the range fails. Every read is recorded as (start, end, failed).
pub struct UnreadableRangeSource {
    bytes: Vec<u8>,
    unreadable: Mutex<std::ops::Range<u64>>,
    reads: Mutex<Vec<(u64, u64, bool)>>,
}

impl UnreadableRangeSource {
    pub fn new(bytes: Vec<u8>, unreadable: std::ops::Range<u64>) -> Self {
        UnreadableRangeSource {
            bytes,
            unreadable: Mutex::new(unreadable),
            reads: Mutex::new(Vec::new()),
        }
    }

    /// Makes `range`, and only it, unreadable from now on.
    pub fn set_unreadable(&self, range: std::ops::Range<u64>) {
        *self.unreadable.lock().expect("the unreadable range") = range;
    }

    /// Every read so far, as (start, end, failed).
    pub fn reads(&self) -> Vec<(u64, u64, bool)> {
        self.reads.lock().expect("the read log").clone()
    }

    /// The error a read touching `range` fails with.
    pub fn error(range: &std::ops::Range<u64>) -> SourceError {
        SourceError::new(format!(
            "bytes {}..{} are unreadable",
            range.start, range.end
        ))
    }
}

impl PackageRead for UnreadableRangeSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let end = offset + buf.len() as u64;
        let unreadable = self
            .unreadable
            .lock()
            .expect("the unreadable range")
            .clone();
        let fails = offset < unreadable.end && end > unreadable.start;
        self.reads
            .lock()
            .expect("the read log")
            .push((offset, end, fails));
        if fails {
            return Err(Self::error(&unreadable));
        }
        let bytes = self
            .bytes
            .get(offset as usize..end as usize)
            .ok_or_else(|| SourceError::new("a read past the end"))?;
        buf.copy_from_slice(bytes);
        Ok(())
    }
}

/// A sink that accepts `capacity` bytes and refuses the write that would pass it.
pub struct FullSink {
    pub accepted: Vec<u8>,
    pub capacity: usize,
}

impl PackageWrite for FullSink {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SinkError> {
        if self.accepted.len() + bytes.len() > self.capacity {
            return Err(SinkError::new("the medium is full"));
        }
        self.accepted.extend_from_slice(bytes);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// dc-ndi (D1): packages built from a directory of parts, and calc-doc's own open, load, edit and
// save route with the saved bytes read back, for the d1_* test files. Unlike the instruments
// above, these go through calc-doc's code: they drive the route under test, they do not judge it.

#[allow(dead_code)]
pub mod d1 {
    use super::{RawEntry, build_zip};
    use calc_doc::model::value::{CellKey, ExcelText, IdAllocator, SheetId};
    use calc_doc::model::{CellInput, FiniteNumber, FormulaText, ModelRoot, OpBatch, WorkbookOp};
    use calc_doc::{
        LoadedWorkbook, MemorySource, OpenPolicy, OpenedDocument, PackageRead, SavePolicy,
        SaveReceipt, prepare_edit, prepare_save, write,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// The directory of the D1 fixtures.
    pub fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/d1")
    }

    /// The bytes of a fixture file under `tests/fixtures/d1`.
    pub fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(fixtures().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    /// A package of every file under `dir`, each stored under its path relative to `dir`, in path
    /// order with `[Content_Types].xml` first.
    pub fn package_from_dir(dir: &Path) -> Vec<u8> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            for entry in std::fs::read_dir(dir).expect("a fixture directory") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let name = path
                        .strip_prefix(root)
                        .expect("under the root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((name, std::fs::read(&path).expect("a part")));
                }
            }
        }
        let mut parts = Vec::new();
        walk(dir, dir, &mut parts);
        parts.sort_by(|a, b| {
            (a.0 != "[Content_Types].xml", &a.0).cmp(&(b.0 != "[Content_Types].xml", &b.0))
        });
        let entries: Vec<RawEntry> = parts
            .iter()
            .map(|(name, bytes)| RawEntry::stored(name, bytes))
            .collect();
        build_zip(&entries)
    }

    pub fn open(bytes: &[u8]) -> OpenedDocument {
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
        calc_doc::open(source, &OpenPolicy::default()).expect("the package opens")
    }

    pub fn load(bytes: &[u8]) -> LoadedWorkbook {
        let document = open(bytes);
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        calc_doc::load(&document, &mut ids).expect("the workbook loads")
    }

    /// The package saved as opened.
    pub fn save_untouched(bytes: &[u8]) -> Vec<u8> {
        let document = open(bytes);
        let plan = prepare_save(&document.snapshot(), &SavePolicy::default()).expect("a plan");
        let mut out = Vec::new();
        write(plan, &mut out).expect("written");
        out
    }

    /// The revision `operations` make of the loaded revision.
    pub fn edited(loaded: &LoadedWorkbook, operations: Vec<WorkbookOp>) -> Arc<ModelRoot> {
        let model = loaded.model();
        prepare_edit(
            model,
            &OpBatch {
                base: model.revision(),
                operations,
            },
        )
        .expect("the edit is admitted")
        .into_model()
    }

    /// The package saved at `model`, a revision of `loaded`'s model.
    pub fn save_model(loaded: &LoadedWorkbook, model: &Arc<ModelRoot>) -> (Vec<u8>, SaveReceipt) {
        let plan = prepare_save(&loaded.snapshot(model), &SavePolicy::default()).expect("a plan");
        let mut out = Vec::new();
        let receipt = write(plan, &mut out).expect("written");
        (out, receipt)
    }

    /// The decompressed bytes of the part `name` of `package`, read through calc-doc; `None` when the
    /// package has no such part.
    pub fn part(package: &[u8], name: &str) -> Option<Vec<u8>> {
        let document = open(package);
        document.package().part(name)?;
        Some(document.package().read_part(name).expect("the part reads"))
    }

    pub fn part_text(package: &[u8], name: &str) -> String {
        String::from_utf8(part(package, name).unwrap_or_else(|| panic!("no part {name}")))
            .expect("UTF-8")
    }

    /// The ID of the sheet named `name`.
    pub fn sheet(loaded: &LoadedWorkbook, name: &str) -> SheetId {
        loaded
            .model()
            .sheets()
            .iter()
            .find(|sheet| String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == name)
            .unwrap_or_else(|| panic!("no sheet {name}"))
            .id()
    }

    /// The cell at an A1 address (`B3`) of `sheet`.
    pub fn at(sheet: SheetId, address: &str) -> CellKey {
        let split = address
            .find(|c: char| c.is_ascii_digit())
            .expect("a row number");
        let (letters, digits) = address.split_at(split);
        let column = letters.bytes().fold(0u32, |n, b| {
            n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
        });
        CellKey::new(sheet, digits.parse().expect("a row"), column).expect("in the grid")
    }

    pub fn number(value: f64) -> CellInput {
        CellInput::Number(FiniteNumber::new(value).expect("finite"))
    }

    pub fn formula(text: &str) -> CellInput {
        CellInput::Formula(
            FormulaText::new(ExcelText::from_utf16_code_units(
                text.encode_utf16().collect(),
            ))
            .expect("a formula"),
        )
    }

    pub fn formula_text(input: Option<&CellInput>) -> Option<String> {
        match input {
            Some(CellInput::Formula(text)) => {
                Some(String::from_utf16_lossy(text.text().utf16_code_units()))
            }
            _ => None,
        }
    }

    /// A copy of `package` whose worksheet part `sheet_part` gains a cell with a style index past
    /// any styles part's cell formats, which Excel repairs: the positive control of the Excel
    /// reopen check (a reopen route that does not report this copy cannot report a repair).
    pub fn repair_control(package: &[u8], sheet_part: &str) -> Vec<u8> {
        let archive = super::parse_zip(package);
        let entries: Vec<RawEntry> = archive
            .entries
            .iter()
            .map(|entry| {
                let payload = entry.payload(package);
                let mut bytes = match entry.method {
                    0 => payload.to_vec(),
                    _ => miniz_oxide::inflate::decompress_to_vec(payload).expect("a deflated part"),
                };
                if entry.name == sheet_part {
                    let text = String::from_utf8(bytes).expect("UTF-8");
                    bytes = text
                        .replacen(
                            "<sheetData>",
                            r#"<sheetData><row r="1000"><c r="A1000" s="9999"><v>1</v></c></row>"#,
                            1,
                        )
                        .into_bytes();
                }
                RawEntry::stored(&entry.name, &bytes)
            })
            .collect();
        build_zip(&entries)
    }

    /// Writes `bytes` to `name` under the directory `DNACALC_D1_SAVE_DIR` names, when it is set, for
    /// the Excel reopen script (`tests/fixtures/d1/reopen-in-excel.ps1`) to open.
    pub fn keep_for_excel(name: &str, bytes: &[u8]) {
        if let Ok(dir) = std::env::var("DNACALC_D1_SAVE_DIR") {
            std::fs::create_dir_all(&dir).expect("the save directory");
            std::fs::write(Path::new(&dir).join(name), bytes).expect("written");
        }
    }
}
