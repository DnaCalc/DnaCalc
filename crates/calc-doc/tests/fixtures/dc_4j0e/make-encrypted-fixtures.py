"""Writes the encrypted Workbook streams of dc-4j0e.7's tests (run: python3 -I make-encrypted-fixtures.py).

Each output is the `Workbook` stream of a BIFF8 file with one sheet, "Secret", holding the number
42.5 in A1 and the text "classified" in B1, obfuscated or encrypted as the specifications say:
MS-OFFCRYPTO 2.3.7 (XOR), 2.3.6 (RC4) and 2.3.5 (RC4 CryptoAPI), MS-XLS 2.2.10 (which records and
bytes stay clear, the 1024-byte RC4 blocks). Written independently of the Rust reader, from the
specification text: only Python's hashlib is shared. The tests wrap each stream in a compound file.
"""
import hashlib
import struct
import sys

PASSWORD_XOR = "swordfish"
PASSWORD_RC4 = "pässwörd"
PASSWORD_VELVET = "VelvetSweatshop"


def record(rid, body):
    return struct.pack("<HH", rid, len(body)) + body


def plain_stream(filepass_body):
    """A workbook stream whose FILEPASS body is `filepass_body` (None for no FILEPASS)."""
    parts = []
    bof = struct.pack("<HHHH", 0x0600, 0x0005, 0x0DBB, 0x07CC) + bytes(8)
    parts.append(("bof", record(0x0809, bof)))
    parts.append(("interface", record(0x00E1, struct.pack("<H", 0x04B0))))
    if filepass_body is not None:
        parts.append(("filepass", record(0x002F, filepass_body)))
    parts.append(("codepage", record(0x0042, struct.pack("<H", 1252))))
    # BOUNDSHEET with a placeholder offset, patched below.
    name = b"Secret"
    boundsheet_body = struct.pack("<IBB", 0, 0, 0) + bytes([len(name), 0]) + name
    parts.append(("boundsheet", record(0x0085, boundsheet_body)))
    parts.append(("eof_globals", record(0x000A, b"")))
    sheet_offset = sum(len(b) for _, b in parts)
    patched = []
    for kind, data in parts:
        if kind == "boundsheet":
            data = record(0x0085, struct.pack("<IBB", sheet_offset, 0, 0) + bytes([len(name), 0]) + name)
        patched.append((kind, data))
    parts = patched
    parts.append(("bof_sheet", record(0x0809, struct.pack("<HHHH", 0x0600, 0x0010, 0x0DBB, 0x07CC) + bytes(8))))
    parts.append(("number", record(0x0203, struct.pack("<HHHd", 0, 0, 15, 42.5))))
    text = b"classified"
    parts.append(("label", record(0x0204, struct.pack("<HHHHB", 0, 1, 15, len(text), 0) + text)))
    # Records of a type nothing reads, enough to carry the stream past three 1024-byte blocks.
    for _ in range(250):
        parts.append(("filler", record(0x01C1, bytes(range(8)))))
    parts.append(("eof_sheet", record(0x000A, b"")))
    return parts


CLEAR = {0x0809, 0x002F, 0x0194, 0x0195, 0x00E1, 0x0196, 0x0138}  # BOF, FilePass, UsrExcl, FileLock, InterfaceHdr, RRDInfo, RRDHead (MS-XLS 2.2.10)


def crypt_records(parts, keystream_byte):
    """Encrypts every record body after the FILEPASS, given `keystream_byte(position, record)`
    returning the transformation of one byte (position counted from the stream start)."""
    out = bytearray()
    seen_filepass = False
    for kind, data in parts:
        rid, size = struct.unpack_from("<HH", data)
        body = bytearray(data[4:])
        if seen_filepass and rid not in CLEAR:
            start = len(out) + 4
            for k in range(len(body)):
                if rid == 0x0085 and k < 4:
                    continue
                body[k] = keystream_byte(start + k, k, size, body[k])
        if kind == "filepass":
            seen_filepass = True
        out += data[:4] + bytes(body)
    return bytes(out)


# --- XOR obfuscation (MS-OFFCRYPTO 2.3.7) ---

INITIAL_CODE = [0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9, 0x280C, 0xA96A, 0x4EC3]
XOR_MATRIX = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09, 0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF,
    0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0, 0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40,
    0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5, 0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A,
    0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9, 0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0,
    0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC, 0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10,
    0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168, 0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C,
    0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD, 0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC,
    0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
]
PAD = [0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00]


def ror(b):
    return ((b >> 1) | (b << 7)) & 0xFF


def xor_ror(a, b):
    return ror(a ^ b)


def xor_key(pw):
    key = INITIAL_CODE[len(pw) - 1]
    current = 0x68
    for ch in reversed(pw):
        c = ch
        for _ in range(7):
            if c & 0x40:
                key ^= XOR_MATRIX[current]
            c = (c * 2) & 0xFFFFFFFF
            current -= 1
    return key


def xor_array(pw):
    key = xor_key(pw)
    arr = [0] * 16
    index = len(pw)
    if index % 2 == 1:
        arr[index] = xor_ror(PAD[0], key >> 8)
        index -= 1
        arr[index] = xor_ror(pw[len(pw) - 1], key & 0xFF)
    while index > 0:
        index -= 1
        arr[index] = xor_ror(pw[index], key >> 8)
        index -= 1
        arr[index] = xor_ror(pw[index], key & 0xFF)
    index = 15
    pad_index = 15 - len(pw)
    while pad_index > 0:
        arr[index] = xor_ror(PAD[pad_index], key >> 8)
        index -= 1
        pad_index -= 1
        arr[index] = xor_ror(PAD[pad_index], key & 0xFF)
        index -= 1
        pad_index -= 1
    return arr


def password_verifier(pw):
    verifier = 0
    for byte in reversed([len(pw)] + list(pw)):
        inter1 = 1 if verifier & 0x4000 else 0
        inter2 = (verifier * 2) & 0x7FFF
        verifier = (inter1 | inter2) ^ byte
    return verifier ^ 0xCE4B


def xor_stream(pw_text):
    pw = pw_text.encode("latin-1")
    arr = xor_array(pw)
    filepass = struct.pack("<HHH", 0, xor_key(pw), password_verifier(pw))
    parts = plain_stream(filepass)

    # XorArrayIndex starts at (position of the record's data + record size) mod 16 and advances
    # one for each byte of the record.
    def transform_record(position, k, size, byte):
        index = (position + size) % 16
        value = ((byte << 5) | (byte >> 3)) & 0xFF
        return value ^ arr[index]

    return crypt_records(parts, transform_record)


# --- RC4 ---

def rc4(key):
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) & 0xFF
        s[i], s[j] = s[j], s[i]
    i = j = 0
    while True:
        i = (i + 1) & 0xFF
        j = (j + s[i]) & 0xFF
        s[i], s[j] = s[j], s[i]
        yield s[(s[i] + s[j]) & 0xFF]


def block_keystreams(key_for_block):
    cache = {}

    def at(position):
        block, offset = divmod(position, 1024)
        if block not in cache:
            gen = rc4(key_for_block(block))
            cache[block] = [next(gen) for _ in range(1024)]
        return cache[block][offset]

    return at


def rc4_97_key_for_block(password, salt):
    h0 = hashlib.md5(password.encode("utf-16-le")).digest()
    h1 = hashlib.md5((h0[:5] + salt) * 16).digest()
    return lambda block: hashlib.md5(h1[:5] + struct.pack("<I", block)).digest()


def rc4_stream(password, salt=bytes(range(16)), verifier=bytes(range(100, 116))):
    key = rc4_97_key_for_block(password, salt)
    gen = rc4(key(0))
    enc = bytes(b ^ next(gen) for b in verifier + hashlib.md5(verifier).digest())
    filepass = struct.pack("<HHH", 1, 1, 1) + salt + enc[:16] + enc[16:]
    parts = plain_stream(filepass)
    ks = block_keystreams(key)
    return crypt_records(parts, lambda position, k, size, byte: byte ^ ks(position))


def cryptoapi_stream(password, key_bits, salt=bytes(range(50, 66)), verifier=bytes(range(7, 23))):
    def key_for_block(block):
        h0 = hashlib.sha1(salt + password.encode("utf-16-le")).digest()
        final = hashlib.sha1(h0 + struct.pack("<I", block)).digest()
        key = final[: key_bits // 8]
        return key + bytes(16 - len(key)) if key_bits == 40 else key

    gen = rc4(key_for_block(0))
    enc = bytes(b ^ next(gen) for b in verifier + hashlib.sha1(verifier).digest())
    csp = "Microsoft Enhanced Cryptographic Provider v1.0".encode("utf-16-le") + b"\0\0"
    flags = 0x04
    header = struct.pack("<IIIIIIII", flags, 0, 0x6801, 0x8004, key_bits, 1, 0, 0) + csp
    info = struct.pack("<HH", 4, 2) + struct.pack("<II", flags, len(header)) + header
    info += struct.pack("<I", 16) + salt + enc[:16] + struct.pack("<I", 20) + enc[16:]
    filepass = struct.pack("<H", 1) + info
    parts = plain_stream(filepass)
    ks = block_keystreams(key_for_block)
    return crypt_records(parts, lambda position, k, size, byte: byte ^ ks(position))


def main(out):
    files = {
        "xor.bin": xor_stream(PASSWORD_XOR),
        "rc4.bin": rc4_stream(PASSWORD_RC4),
        "rc4-velvet.bin": rc4_stream(PASSWORD_VELVET),
        "cryptoapi-128.bin": cryptoapi_stream(PASSWORD_RC4, 128),
        "cryptoapi-40.bin": cryptoapi_stream(PASSWORD_RC4, 40),
        "plain.bin": b"".join(b for _, b in plain_stream(None)),
    }
    for name, data in files.items():
        with open(f"{out}/{name}", "wb") as handle:
            handle.write(data)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else ".")
