//! Password-protected legacy workbooks (MS-XLS 2.2.10, MS-OFFCRYPTO 2.3.5 to 2.3.7): the FILEPASS
//! record names how the Workbook stream after it is obfuscated or encrypted, and the stream is
//! decrypted in place before its records are read.
//!
//! - XOR obfuscation (MS-OFFCRYPTO 2.3.7, Excel 5 to 2003): a 16-byte array from the password, a
//!   rotate and an XOR per byte.
//! - RC4 (2.3.6, "Excel 97-2003 encryption"): the key is MD5 based, block number by 1024-byte
//!   block of the stream.
//! - RC4 CryptoAPI (2.3.5, Excel 2002 to 2003 "strong"): the key is SHA-1 based, again by block.
//!
//! Record headers (type and size) stay clear, and so do the BOF, FilePass, UsrExcl, FileLock,
//! InterfaceHdr, RRDInfo and RRDHead records and the lbPlyPos field of BoundSheet8; for RC4 the
//! clear bytes still use up their place in the key stream.
//!
//! The hash functions (MD5, SHA-1) and RC4 are the standard algorithms, written here so the
//! package stays portable; their unit tests check the published test vectors.

use std::fmt;

/// Why a stream could not be decrypted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CryptoError {
    /// The FILEPASS record names a scheme or parameters this reader does not read.
    Unsupported(String),
    /// The password does not open the file.
    WrongPassword,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::Unsupported(what) => f.write_str(what),
            CryptoError::WrongPassword => f.write_str("the password does not open the workbook"),
        }
    }
}

fn unsupported<T>(what: impl Into<String>) -> Result<T, CryptoError> {
    Err(CryptoError::Unsupported(what.into()))
}

// ---- MD5 (RFC 1321) ----

pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
        14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
        21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let constants: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut state: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_le_bytes());
    for chunk in message.chunks_exact(64) {
        let words: Vec<u32> = chunk
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(constants[i])
                .wrapping_add(words[g])
                .rotate_left(SHIFTS[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (k, word) in state.iter().enumerate() {
        out[4 * k..4 * k + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

// ---- SHA-1 (FIPS 180-4) ----

pub(crate) fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (k, b) in chunk.chunks_exact(4).enumerate() {
            w[k] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for k in 16..80 {
            w[k] = (w[k - 3] ^ w[k - 8] ^ w[k - 14] ^ w[k - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (k, word) in w.iter().enumerate() {
            let (f, constant) = match k / 20 {
                0 => ((b & c) | (!b & d), 0x5A82_7999),
                1 => (b ^ c ^ d, 0x6ED9_EBA1),
                2 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (k, word) in h.iter().enumerate() {
        out[4 * k..4 * k + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ---- RC4 ----

pub(crate) struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    pub(crate) fn new(key: &[u8]) -> Rc4 {
        let mut s = [0u8; 256];
        for (k, item) in s.iter_mut().enumerate() {
            *item = k as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, usize::from(j));
        }
        Rc4 { s, i: 0, j: 0 }
    }

    pub(crate) fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.s[usize::from(self.i)]);
        self.s.swap(usize::from(self.i), usize::from(self.j));
        self.s[usize::from(self.s[usize::from(self.i)].wrapping_add(self.s[usize::from(self.j)]))]
    }

    fn apply(&mut self, data: &mut [u8]) {
        for byte in data {
            *byte ^= self.next_byte();
        }
    }
}

// ---- XOR obfuscation (MS-OFFCRYPTO 2.3.7) ----

const INITIAL_CODE: [u16; 15] = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9,
    0x280C, 0xA96A, 0x4EC3,
];
const XOR_MATRIX: [u16; 105] = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09, 0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7,
    0x9DCF, 0x2BBF, 0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0, 0x0375, 0x06EA, 0x0DD4,
    0x1BA8, 0x3750, 0x6EA0, 0xDD40, 0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5, 0x6F45,
    0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A, 0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC,
    0x5FD9, 0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0, 0xB861, 0x60E3, 0xC1C6, 0x93AD,
    0x377B, 0x6EF6, 0xDDEC, 0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10, 0xAA51, 0x4483,
    0x8906, 0x022D, 0x045A, 0x08B4, 0x1168, 0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C,
    0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD, 0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373,
    0x06E6, 0x0DCC, 0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
];
const PAD_ARRAY: [u8; 15] = [
    0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00,
];

fn xor_key(password: &[u8]) -> u16 {
    let mut key = INITIAL_CODE[password.len() - 1];
    let mut current = 0x68usize;
    for ch in password.iter().rev() {
        let mut c = u32::from(*ch);
        for _ in 0..7 {
            if c & 0x40 != 0 {
                key ^= XOR_MATRIX[current];
            }
            c *= 2;
            current = current.wrapping_sub(1);
        }
    }
    key
}

fn xor_ror(a: u8, b: u8) -> u8 {
    (a ^ b).rotate_right(1)
}

fn xor_array(password: &[u8]) -> [u8; 16] {
    let key = xor_key(password);
    let (high, low) = ((key >> 8) as u8, (key & 0xFF) as u8);
    let mut array = [0u8; 16];
    let mut index = password.len();
    if index % 2 == 1 {
        array[index] = xor_ror(PAD_ARRAY[0], high);
        index -= 1;
        array[index] = xor_ror(password[password.len() - 1], low);
    }
    while index > 0 {
        index -= 1;
        array[index] = xor_ror(password[index], high);
        index -= 1;
        array[index] = xor_ror(password[index], low);
    }
    let mut index = 15usize;
    let mut pad_index = 15 - password.len();
    while pad_index > 0 {
        array[index] = xor_ror(PAD_ARRAY[pad_index], high);
        index -= 1;
        pad_index -= 1;
        array[index] = xor_ror(PAD_ARRAY[pad_index], low);
        index = index.wrapping_sub(1);
        pad_index = pad_index.wrapping_sub(1);
    }
    array
}

fn password_verifier(password: &[u8]) -> u16 {
    let mut verifier = 0u16;
    let array: Vec<u8> = std::iter::once(password.len() as u8).chain(password.iter().copied()).collect();
    for byte in array.iter().rev() {
        let intermediate1 = u16::from(verifier & 0x4000 != 0);
        let intermediate2 = verifier.wrapping_mul(2) & 0x7FFF;
        verifier = (intermediate1 | intermediate2) ^ u16::from(*byte);
    }
    verifier ^ 0xCE4B
}

// ---- the stream ----

const BOF: u16 = 0x0809;
const EOF: u16 = 0x000A;
const FILEPASS: u16 = 0x002F;
const BOUNDSHEET: u16 = 0x0085;
/// BOF, FilePass, UsrExcl, FileLock, InterfaceHdr, RRDInfo, RRDHead.
const CLEAR_RECORDS: [u16; 7] = [BOF, FILEPASS, 0x0194, 0x0195, 0x00E1, 0x0196, 0x0138];

/// What a FILEPASS record asks for.
enum Scheme {
    Xor { key: u16, verifier: u16 },
    Rc4 { salt: [u8; 16], verifier: [u8; 16], hash: [u8; 16] },
    CryptoApi { key_bits: u32, salt: [u8; 16], verifier: [u8; 16], hash: [u8; 20] },
}

fn read_u16(body: &[u8], at: usize) -> Result<u16, CryptoError> {
    body.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| CryptoError::Unsupported("a FILEPASS record that is cut off".into()))
}

fn read_u32(body: &[u8], at: usize) -> Result<u32, CryptoError> {
    body.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| CryptoError::Unsupported("a FILEPASS record that is cut off".into()))
}

fn take<const N: usize>(body: &[u8], at: usize) -> Result<[u8; N], CryptoError> {
    let mut out = [0u8; N];
    out.copy_from_slice(
        body.get(at..at + N)
            .ok_or_else(|| CryptoError::Unsupported("a FILEPASS record that is cut off".into()))?,
    );
    Ok(out)
}

fn scheme(body: &[u8]) -> Result<Scheme, CryptoError> {
    // BIFF2 to BIFF5 write only the key and the verifier; BIFF8 puts the type first.
    if body.len() == 4 {
        return Ok(Scheme::Xor {
            key: read_u16(body, 0)?,
            verifier: read_u16(body, 2)?,
        });
    }
    match read_u16(body, 0)? {
        0 => Ok(Scheme::Xor {
            key: read_u16(body, 2)?,
            verifier: read_u16(body, 4)?,
        }),
        1 => {
            let (major, minor) = (read_u16(body, 2)?, read_u16(body, 4)?);
            if (major, minor) == (1, 1) {
                return Ok(Scheme::Rc4 {
                    salt: take(body, 6)?,
                    verifier: take(body, 22)?,
                    hash: take(body, 38)?,
                });
            }
            if matches!(major, 2..=4) && minor == 2 {
                let header_size = read_u32(body, 10)? as usize;
                let header = 14;
                let algorithm = read_u32(body, header + 8)?;
                let hash_algorithm = read_u32(body, header + 12)?;
                let key_bits = read_u32(body, header + 16)?;
                if !matches!(algorithm, 0 | 0x6801) || !matches!(hash_algorithm, 0 | 0x8004) {
                    return unsupported(format!(
                        "RC4 CryptoAPI encryption with algorithm {algorithm:#x} and hash {hash_algorithm:#x}"
                    ));
                }
                let key_bits = if key_bits == 0 { 40 } else { key_bits };
                if !(40..=128).contains(&key_bits) || key_bits % 8 != 0 {
                    return unsupported(format!("an RC4 key of {key_bits} bits"));
                }
                let at = header + header_size;
                if read_u32(body, at)? != 16 {
                    return unsupported("an RC4 CryptoAPI salt that is not 16 bytes");
                }
                return Ok(Scheme::CryptoApi {
                    key_bits,
                    salt: take(body, at + 4)?,
                    verifier: take(body, at + 20)?,
                    hash: take(body, at + 40)?,
                });
            }
            unsupported(format!("RC4 encryption of version {major}.{minor}"))
        }
        other => unsupported(format!("an encryption type {other} in FILEPASS")),
    }
}

/// The RC4 key of `block` for the scheme, or `None` for XOR.
fn rc4_block_key(scheme: &Scheme, password: &str) -> impl Fn(u32) -> Vec<u8> {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    match scheme {
        Scheme::Rc4 { salt, .. } => {
            let h0 = md5(&utf16);
            let mut buffer = Vec::with_capacity(336);
            for _ in 0..16 {
                buffer.extend_from_slice(&h0[..5]);
                buffer.extend_from_slice(salt);
            }
            let h1 = md5(&buffer);
            let truncated = h1[..5].to_vec();
            Box::new(move |block: u32| {
                let mut input = truncated.clone();
                input.extend_from_slice(&block.to_le_bytes());
                md5(&input).to_vec()
            }) as Box<dyn Fn(u32) -> Vec<u8>>
        }
        Scheme::CryptoApi { key_bits, salt, .. } => {
            let mut input = salt.to_vec();
            input.extend_from_slice(&utf16);
            let h0 = sha1(&input);
            let bits = *key_bits;
            Box::new(move |block: u32| {
                let mut input = h0.to_vec();
                input.extend_from_slice(&block.to_le_bytes());
                let hfinal = sha1(&input);
                let mut key = hfinal[..(bits / 8) as usize].to_vec();
                if bits == 40 {
                    key.resize(16, 0);
                }
                key
            })
        }
        Scheme::Xor { .. } => Box::new(|_| Vec::new()),
    }
}

/// The password as the ANSI bytes XOR obfuscation takes (15 characters at most).
fn ansi(password: &str) -> Result<Vec<u8>, CryptoError> {
    let mut bytes = Vec::new();
    for c in password.chars() {
        let code = u32::from(c);
        if code > 0xFF {
            return unsupported("a password with a character outside the ANSI code page");
        }
        bytes.push(code as u8);
    }
    if bytes.is_empty() || bytes.len() > 15 {
        return unsupported("a password of no characters or more than 15 for XOR obfuscation");
    }
    Ok(bytes)
}

/// Decrypts the Workbook stream `stream` in a copy whose FILEPASS record is left as it is;
/// `None` when the stream has no FILEPASS in its globals.
pub(crate) fn decrypt(stream: &[u8], password: &str) -> Result<Option<Vec<u8>>, CryptoError> {
    // Find FILEPASS among the physical records of the globals.
    let mut position = 0usize;
    let mut found: Option<(usize, usize)> = None;
    let mut depth = 0usize;
    while position + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[position], stream[position + 1]]);
        let size = usize::from(u16::from_le_bytes([stream[position + 2], stream[position + 3]]));
        if position + 4 + size > stream.len() {
            break;
        }
        match id {
            BOF => depth += 1,
            EOF => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            FILEPASS => {
                found = Some((position + 4, size));
                break;
            }
            _ => {}
        }
        position += 4 + size;
    }
    let Some((body_at, body_size)) = found else {
        return Ok(None);
    };
    decrypt_records(stream, &stream[body_at..body_at + body_size], password, body_at + body_size).map(Some)
}

/// A pivot-cache BIFF stream uses the Workbook FILEPASS scheme, starting its own
/// cipher position at zero (MS-XLS 2.2.10). Its record headers remain clear.
pub(crate) fn decrypt_pivot_cache(
    stream: &[u8], filepass: &[u8], password: &str,
) -> Result<Vec<u8>, CryptoError> {
    decrypt_records(stream, filepass, password, 0)
}

fn decrypt_records(
    stream: &[u8], filepass: &[u8], password: &str, start_at: usize,
) -> Result<Vec<u8>, CryptoError> {
    let scheme = scheme(filepass)?;
    // Check the password, and build the keystream source.
    let mut output = stream.to_vec();
    let mut keystream: Option<(u32, Vec<u8>)> = None;
    let key_of = rc4_block_key(&scheme, password);
    let xor = match &scheme {
        Scheme::Xor { key, verifier } => {
            let bytes = ansi(password)?;
            if xor_key(&bytes) != *key || password_verifier(&bytes) != *verifier {
                return Err(CryptoError::WrongPassword);
            }
            Some(xor_array(&bytes))
        }
        Scheme::Rc4 { verifier, hash, .. } => {
            let mut cipher = Rc4::new(&key_of(0));
            let mut data = [verifier.as_slice(), hash.as_slice()].concat();
            cipher.apply(&mut data);
            if md5(&data[..16]) != data[16..32] {
                return Err(CryptoError::WrongPassword);
            }
            None
        }
        Scheme::CryptoApi { verifier, hash, .. } => {
            let mut cipher = Rc4::new(&key_of(0));
            let mut data = [verifier.as_slice(), hash.as_slice()].concat();
            cipher.apply(&mut data);
            if sha1(&data[..16]) != data[16..36] {
                return Err(CryptoError::WrongPassword);
            }
            None
        }
    };
    // Workbook records after FILEPASS, or every record in a cache stream.
    let mut position = start_at;
    while position + 4 <= output.len() {
        let id = u16::from_le_bytes([output[position], output[position + 1]]);
        let size = usize::from(u16::from_le_bytes([output[position + 2], output[position + 3]]));
        let start = position + 4;
        if start + size > output.len() {
            break;
        }
        if !CLEAR_RECORDS.contains(&id) {
            let skip = if id == BOUNDSHEET { 4.min(size) } else { 0 };
            match &xor {
                Some(array) => {
                    for k in skip..size {
                        let index = (start + size + k) % 16;
                        let byte = output[start + k] ^ array[index];
                        output[start + k] = byte.rotate_right(5);
                    }
                }
                None => {
                    for p in start + skip..start + size {
                        let block = (p / 1024) as u32;
                        if keystream.as_ref().is_none_or(|(b, _)| *b != block) {
                            // The key stream of a block, from its first byte: every byte of the
                            // stream (headers and clear records too) has its place in it.
                            let mut cipher = Rc4::new(&key_of(block));
                            let bytes: Vec<u8> = (0..1024).map(|_| cipher.next_byte()).collect();
                            keystream = Some((block, bytes));
                        }
                        output[p] ^= keystream.as_ref().expect("set above").1[p % 1024];
                    }
                }
            }
        }
        position = start + size;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn published_test_vectors() {
        // RFC 1321, FIPS 180, and the well-known RC4 vector (key "Key", text "Plaintext").
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&md5(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        let mut data = b"Plaintext".to_vec();
        Rc4::new(b"Key").apply(&mut data);
        assert_eq!(hex(&data), "bbf316e8d940af0ad3");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xor_verifier_of_the_empty_password_and_a_known_one() {
        // The verifier of no characters is the constant 0xCE4B.
        assert_eq!(password_verifier(b""), 0xCE4B);
    }
}
