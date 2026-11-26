"""A strict reader for the packages the Excel worker builds (a test helper; the worker reads no package).

`read_package(bytes)` returns {part name: bytes} for a ZIP and `read_compound_file` the streams of a
vbaProject.bin; `decompress_vba_stream` is MS-OVBA 2.4.1.3.1, refusing a chunk that decompresses to more
than 4096 bytes and a compound file whose mini stream cutoff is not 4096 (the rw15 differentials).
"""
import io
import struct
import zipfile

MAX_PROJECT = 8 * 1024 * 1024
END, NONE = 0xFFFFFFFE, 0xFFFFFFFF


def _refused(detail):
    return ValueError("invalid VBA workbook: " + detail)


def read_package(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        return {info.filename: archive.read(info) for info in archive.infolist()}


def decompress_vba_stream(data):
    """MS-OVBA 2.4.1.3.1, refusing anything malformed."""
    if not data or data[0] != 1:
        raise _refused("bad compressed container")
    out, at = bytearray(), 1
    while at < len(data):
        if at + 2 > len(data):
            raise _refused("truncated compressed chunk")
        header = data[at] | data[at + 1] << 8
        end = at + (header & 0xFFF) + 3
        chunk_start = len(out)
        if header >> 12 & 7 != 3 or end > len(data):
            raise _refused("bad compressed chunk")
        if not header & 0x8000:
            if end - at != 4098:
                raise _refused("bad raw chunk")
            out += data[at + 2:end]
        else:
            start, pos = len(out), at + 2
            while pos < end:
                flags = data[pos]
                pos += 1
                for bit in range(8):
                    if pos >= end:
                        break
                    if not flags >> bit & 1:
                        out.append(data[pos])
                        pos += 1
                        continue
                    if pos + 2 > end:
                        raise _refused("truncated copy token")
                    token = data[pos] | data[pos + 1] << 8
                    pos += 2
                    distance = len(out) - start
                    bits = max((distance - 1).bit_length(), 4) if distance else 4
                    length = (token & (0xFFFF >> bits)) + 3
                    offset = (token >> (16 - bits)) + 1
                    if offset > distance:
                        raise _refused("bad copy token")
                    for _ in range(length):
                        out.append(out[-offset])
        if len(out) - chunk_start > 4096:
            raise _refused("a chunk decompresses to more than 4096 bytes")
        at = end
        if len(out) > MAX_PROJECT:
            raise _refused("oversized module stream")
    return bytes(out)


def read_compound_file(blob):
    if len(blob) < 1536 or len(blob) > MAX_PROJECT or blob[:8] != bytes.fromhex("d0cf11e0a1b11ae1"):
        raise _refused("vbaProject.bin is not a compound file")
    major, byte_order, shift, mini_shift = struct.unpack_from("<HHHH", blob, 26)
    if (major, byte_order, shift, mini_shift) != (3, 0xFFFE, 9, 6):
        raise _refused("unsupported compound file layout")
    fat_count, dir_start, _, cutoff, mini_start, mini_count, difat_start, difat_count = struct.unpack_from(
        "<IIIIIIII", blob, 44)
    if cutoff != 4096:
        raise _refused("the mini stream cutoff is not 4096")
    sector = 512
    sectors = (len(blob) - 512) // sector

    def sector_bytes(index):
        if index >= sectors:
            raise _refused("sector out of range")
        return blob[512 + index * sector:512 + (index + 1) * sector]

    difat = list(struct.unpack_from("<109I", blob, 76))
    chain, seen = difat_start, set()
    for _ in range(difat_count):
        if chain in seen or chain >= sectors:
            raise _refused("bad DIFAT")
        seen.add(chain)
        entries = struct.unpack("<128I", sector_bytes(chain))
        difat += entries[:127]
        chain = entries[127]
    fat = []
    for index in difat[:fat_count]:
        fat += struct.unpack("<128I", sector_bytes(index))

    def chain_of(start, table):
        out, seen = [], set()
        while start != END:
            if start >= len(table) or start in seen:
                raise _refused("bad sector chain")
            seen.add(start)
            out.append(start)
            start = table[start]
        return out

    def read_chain(start, size):
        body = b"".join(sector_bytes(i) for i in chain_of(start, fat))
        if len(body) < size:
            raise _refused("short stream")
        return body[:size]

    directory = read_chain(dir_start, len(chain_of(dir_start, fat)) * sector)
    entries = []
    for at in range(0, len(directory), 128):
        raw = directory[at:at + 128]
        length = struct.unpack_from("<H", raw, 64)[0]
        kind = raw[66]
        left, right, child = struct.unpack_from("<III", raw, 68)
        start = struct.unpack_from("<I", raw, 116)[0]
        size = struct.unpack_from("<Q", raw, 120)[0]
        if kind and (length < 2 or length > 64 or length % 2):
            raise _refused("bad directory entry name")
        name = raw[:max(length - 2, 0)].decode("utf-16-le", "strict") if kind else ""
        entries.append((name, kind, left, right, child, start, size))
    if not entries or entries[0][1] != 5:
        raise _refused("no root entry")
    mini_fat = []
    if mini_count:
        body = read_chain(mini_start, len(chain_of(mini_start, fat)) * sector)
        mini_fat = list(struct.unpack(f"<{len(body) // 4}I", body))
    mini_stream = read_chain(entries[0][5], entries[0][6]) if entries[0][6] else b""

    def stream(entry):
        size, start = entry[6], entry[5]
        if size >= cutoff:
            return read_chain(start, size)
        body = b"".join(mini_stream[i * 64:(i + 1) * 64] for i in chain_of(start, mini_fat))
        if len(body) < size:
            raise _refused("short mini stream")
        return body[:size]

    found, visited = {}, set()

    def walk(index, prefix):
        stack = [index]
        while stack:
            at = stack.pop()
            if at == NONE:
                continue
            if at >= len(entries) or at in visited:
                raise _refused("bad directory tree")
            visited.add(at)
            name, kind, left, right, child, _, _ = entries[at]
            path = prefix + name
            if kind == 2:
                if path in found:
                    raise _refused("duplicate stream")
                found[path] = stream(entries[at])
            elif kind == 1:
                walk(child, path + "/")
            else:
                raise _refused("unexpected directory entry")
            stack += [left, right]

    visited.add(0)
    walk(entries[0][4], "")
    if sum(1 for entry in entries if entry[1]) != len(visited):
        raise _refused("unreachable directory entries")
    return found
