//! SHA-256 (FIPS 180-4), for the content hashes a version 2 CaptureRecord carries: the input
//! package, the package Excel saved, and the capture worker script (part of the capture tool's
//! revision). It is here, in the portable verdict library, so that the capture adapter that writes
//! those hashes and a replay that checks them on Windows, Linux and wasm use one implementation.

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

/// Incremental SHA-256, retaining at most one incomplete 64-byte block.
/// The capture adapter and corpus inventory use the same compression path as
/// the one-shot helper. Length is recorded modulo 2^64 bits, as in that helper.
pub struct Sha256 {
    h: [u32; 8],
    buffer: [u8; 64],
    used: usize,
    bytes: u64,
}
impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}
impl Sha256 {
    pub fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            used: 0,
            bytes: 0,
        }
    }
    pub fn update(&mut self, mut data: &[u8]) {
        self.bytes = self.bytes.wrapping_add(data.len() as u64);
        if self.used != 0 {
            let take = (64 - self.used).min(data.len());
            self.buffer[self.used..self.used + take].copy_from_slice(&data[..take]);
            self.used += take;
            data = &data[take..];
            if self.used < 64 {
                return;
            }
            compress(&mut self.h, &self.buffer);
            self.used = 0;
        }
        let mut blocks = data.chunks_exact(64);
        for block in &mut blocks {
            compress(&mut self.h, block);
        }
        let rest = blocks.remainder();
        self.buffer[..rest.len()].copy_from_slice(rest);
        self.used = rest.len();
    }
    /// Consume the state and return 64 lower-case hexadecimal digits.
    pub fn finish_hex(mut self) -> String {
        self.buffer[self.used] = 0x80;
        self.used += 1;
        if self.used > 56 {
            self.buffer[self.used..].fill(0);
            compress(&mut self.h, &self.buffer);
            self.buffer.fill(0);
        } else {
            self.buffer[self.used..56].fill(0);
        }
        self.buffer[56..].copy_from_slice(&self.bytes.wrapping_mul(8).to_be_bytes());
        compress(&mut self.h, &self.buffer);
        self.h.iter().map(|word| format!("{word:08x}")).collect()
    }
}

/// The SHA-256 digest of `data` as 64 lower-case hex digits.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(data);
    hash.finish_hex()
}

fn compress(h: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(choice)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(majority);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (state, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *state = state.wrapping_add(add);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn known_digests() {
        // FIPS 180-4 example messages and the empty message.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 1_000]),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chunked_hashes_keep_padding_and_short_read_boundaries() {
        // Authored byte patterns, frozen against Python's independent hashlib.
        // These cover both padding blocks and the corpus reader's 64 KiB boundary.
        for (size, expected) in [
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                1,
                "e7cf46a078fed4fafd0b5e3aff144802b853f8ae459a4f0c14add3314b7cc3a6",
            ),
            (
                55,
                "6f05cee3f2eb9c3a705751c3e6128ca1058c979840185022562b37669f5a43e7",
            ),
            (
                56,
                "132b75588dc874c33a703f735d6757b1e16df7222691ace083d3e546e5ec8451",
            ),
            (
                57,
                "11fe2b452822baa01edd0ff8529ab032445670e5e2f8dcb666b33249cb163ee8",
            ),
            (
                63,
                "210493d74bd9543f6a71b25a0fdb121f925ac497c44783f6141204091f01cb15",
            ),
            (
                64,
                "7e0cba021b48aa7b07ecfea8ce6310217dc489ac5aabfc25ea7716a7d6b2c108",
            ),
            (
                65,
                "0cf5579c9065cf8833440fa1e2a86838e0b02afcec17f57eee3d91507fa4ebf8",
            ),
            (
                119,
                "28be33584276d451ee76557bb71b73c86c89f85ca63b2513faba5f7996c9c01c",
            ),
            (
                120,
                "181521e4abe319a3764155de22bf960782a277a97af707d2c630461820466138",
            ),
            (
                121,
                "48e35af9ef2886329b9ffc3d0f74c0b21793d5a7722708c18c446d89f5381b9b",
            ),
            (
                127,
                "5226a84f6e541c20f29be41d3eda33ad91e2899c527e1ed9c24ee84a0c832bac",
            ),
            (
                128,
                "4dbf70b7713c44bcd51c588f17a4c605a3427fa1fe866b77a957f5dbc5c06a94",
            ),
            (
                129,
                "419a0568153aaa3f88aa6047f1cb37ddc110be08edb393cffd6b123e162e4a15",
            ),
            (
                65535,
                "5bd69d2f0d3d309dab3bfcd6d6869ca786dd3ba4977a4ab6d193763a51bbb493",
            ),
            (
                65536,
                "4cd0cee236c992ac8bbfcaa88b64428a8fd9be2a07652c2bd63783ea42ddbe22",
            ),
            (
                65537,
                "b569695a8e253309bdf1ebe288e2506388c8915a1cadc6ea6af98ae0c602d4ce",
            ),
        ] {
            let bytes: Vec<_> = (0..size).map(|i| ((i * 17 + 11) % 251) as u8).collect();
            assert_eq!(sha256_hex(&bytes), expected, "one-shot {size}");
            for chunk in [1, 7, 63, 64, 65, 4093, 65536] {
                let mut hash = Sha256::new();
                hash.update(&[]);
                for data in bytes.chunks(chunk) {
                    hash.update(data);
                    hash.update(&[]);
                }
                assert_eq!(hash.finish_hex(), expected, "{size} bytes, chunk {chunk}");
            }
        }
        // FIPS's million-'a' vector without allocating a million-byte message.
        let mut hash = Sha256::new();
        for _ in 0..1000 {
            hash.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hash.finish_hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
