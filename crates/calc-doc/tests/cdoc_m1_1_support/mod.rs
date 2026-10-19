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
// The allocation counter.

thread_local! {
    static ALLOCATED: Cell<u64> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    let _ = ALLOCATED.try_with(|total| total.set(total.get() + bytes as u64));
}

/// The bytes this thread has asked the allocator for so far (frees are not subtracted).
pub fn allocated() -> u64 {
    ALLOCATED.with(Cell::get)
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
