//! Hand-built legacy workbooks for the dc-4j0e tests: an OLE compound file writer (the FAT, the
//! directory, and a mini stream for the small streams, as Excel writes them) and BIFF8 record
//! builders. Every child of dc-4j0e adds its builders here.
#![allow(dead_code)]

/// A compound file (version 3, 512-byte sectors) holding `streams` at its root. Streams under
/// 4096 bytes go in the mini stream.
pub fn compound(streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
    const SECTOR: usize = 512;
    const END: u32 = 0xFFFF_FFFE;
    const FATS: u32 = 0xFFFF_FFFD;
    let small: Vec<usize> = (0..streams.len()).filter(|k| streams[*k].1.len() < 4096).collect();
    let big: Vec<usize> = (0..streams.len()).filter(|k| streams[*k].1.len() >= 4096).collect();
    // The mini stream and its FAT.
    let mut mini = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut mini_start = vec![END; streams.len()];
    for k in &small {
        let data = &streams[*k].1;
        if data.is_empty() {
            continue;
        }
        mini_start[*k] = mini_fat.len() as u32;
        let units = data.len().div_ceil(64);
        for u in 0..units {
            mini_fat.push(if u + 1 == units {
                END
            } else {
                mini_fat.len() as u32 + 1
            });
        }
        mini.extend_from_slice(data);
        mini.resize(mini_fat.len() * 64, 0);
    }
    let dir_sectors = ((1 + streams.len()) * 128).div_ceil(SECTOR);
    let mini_fat_sectors = (mini_fat.len() * 4).div_ceil(SECTOR);
    let mini_sectors = mini.len().div_ceil(SECTOR);
    let big_sectors: Vec<usize> = big.iter().map(|k| streams[*k].1.len().div_ceil(SECTOR)).collect();
    let data_sectors = dir_sectors + mini_fat_sectors + mini_sectors + big_sectors.iter().sum::<usize>();
    let mut fat_sectors = 1usize;
    while fat_sectors * 128 < data_sectors + fat_sectors {
        fat_sectors += 1;
    }
    assert!(fat_sectors <= 109, "the kit writes no DIFAT chain");
    let mut fat: Vec<u32> = vec![0xFFFF_FFFF; fat_sectors * 128];
    let mut next = 0usize;
    for _ in 0..fat_sectors {
        fat[next] = FATS;
        next += 1;
    }
    let chain = |fat: &mut Vec<u32>, count: usize, next: &mut usize| -> u32 {
        if count == 0 {
            return END;
        }
        let start = *next as u32;
        for k in 0..count {
            fat[*next] = if k + 1 == count { END } else { *next as u32 + 1 };
            *next += 1;
        }
        start
    };
    let dir_start = chain(&mut fat, dir_sectors, &mut next);
    let mini_fat_start = chain(&mut fat, mini_fat_sectors, &mut next);
    let mini_start_sector = chain(&mut fat, mini_sectors, &mut next);
    let big_starts: Vec<u32> = big_sectors.iter().map(|n| chain(&mut fat, *n, &mut next)).collect();

    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    out[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
    out[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
    out[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
    out[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
    out[0x2C..0x30].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
    out[0x30..0x34].copy_from_slice(&dir_start.to_le_bytes());
    out[0x38..0x3C].copy_from_slice(&4096u32.to_le_bytes());
    out[0x3C..0x40].copy_from_slice(&mini_fat_start.to_le_bytes());
    out[0x40..0x44].copy_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
    out[0x44..0x48].copy_from_slice(&END.to_le_bytes());
    for k in 0..109 {
        let value = if k < fat_sectors { k as u32 } else { 0xFFFF_FFFF };
        out[0x4C + 4 * k..0x50 + 4 * k].copy_from_slice(&value.to_le_bytes());
    }
    // The FAT sectors.
    for value in &fat {
        out.extend_from_slice(&value.to_le_bytes());
    }
    // The directory.
    let entry = |name: &str, kind: u8, left: u32, right: u32, child: u32, start: u32, size: u64| {
        let mut e = vec![0u8; 128];
        let units: Vec<u16> = name.encode_utf16().collect();
        for (k, u) in units.iter().enumerate() {
            e[2 * k..2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
        e[0x40..0x42].copy_from_slice(&((units.len() as u16 + 1) * 2).to_le_bytes());
        e[0x42] = kind;
        e[0x43] = 1;
        e[0x44..0x48].copy_from_slice(&left.to_le_bytes());
        e[0x48..0x4C].copy_from_slice(&right.to_le_bytes());
        e[0x4C..0x50].copy_from_slice(&child.to_le_bytes());
        e[0x74..0x78].copy_from_slice(&start.to_le_bytes());
        e[0x78..0x80].copy_from_slice(&size.to_le_bytes());
        e
    };
    let mut directory = entry(
        "Root Entry",
        5,
        0xFFFF_FFFF,
        0xFFFF_FFFF,
        if streams.is_empty() { 0xFFFF_FFFF } else { 1 },
        if mini.is_empty() { END } else { mini_start_sector },
        mini.len() as u64,
    );
    for (k, (name, data)) in streams.iter().enumerate() {
        let start = if let Some(p) = big.iter().position(|b| *b == k) {
            big_starts[p]
        } else {
            mini_start[k]
        };
        let right = if k + 1 < streams.len() { k as u32 + 2 } else { 0xFFFF_FFFF };
        directory.extend(entry(name, 2, 0xFFFF_FFFF, right, 0xFFFF_FFFF, start, data.len() as u64));
    }
    directory.resize(dir_sectors * SECTOR, 0);
    // Unused directory entries are free (type 0) with no sibling links.
    for k in 1 + streams.len()..dir_sectors * 4 {
        for b in &mut directory[k * 128 + 0x44..k * 128 + 0x50] {
            *b = 0xFF;
        }
    }
    out.extend(directory);
    for value in &mini_fat {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.resize(SECTOR * (1 + fat_sectors + dir_sectors) + mini_fat_sectors * SECTOR, 0xFF);
    out.extend_from_slice(&mini);
    out.resize(
        SECTOR * (1 + fat_sectors + dir_sectors + mini_fat_sectors + mini_sectors),
        0,
    );
    for (p, k) in big.iter().enumerate() {
        out.extend_from_slice(&streams[*k].1);
        out.resize(out.len().div_ceil(SECTOR) * SECTOR, 0);
        let _ = p;
    }
    out
}

/// One BIFF record; a body past 8224 bytes continues in CONTINUE records.
pub fn record(id: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pieces = body.chunks(8224);
    let first = pieces.next().unwrap_or(&[]);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(first.len() as u16).to_le_bytes());
    out.extend_from_slice(first);
    for piece in pieces {
        out.extend_from_slice(&0x003Cu16.to_le_bytes());
        out.extend_from_slice(&(piece.len() as u16).to_le_bytes());
        out.extend_from_slice(piece);
    }
    out
}

pub fn u16s(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// BOF: version, substream type, build, year, flags, lowest version.
pub fn bof(kind: u16) -> Vec<u8> {
    let mut body = u16s(&[0x0600, kind, 0x0DBB, 0x07CC]);
    body.extend_from_slice(&[0; 8]);
    record(0x0809, &body)
}

pub fn eof() -> Vec<u8> {
    record(0x000A, &[])
}

/// A ShortXLUnicodeString.
pub fn short_string(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        let mut out = vec![text.len() as u8, 0];
        out.extend_from_slice(text.as_bytes());
        out
    } else {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut out = vec![units.len() as u8, 1];
        out.extend(u16s(&units));
        out
    }
}

/// A sheet of a built workbook: its name, BOUNDSHEET kind and state, and its records between BOF
/// and EOF.
pub struct Sheet {
    pub name: String,
    pub kind: u8,
    pub state: u8,
    pub records: Vec<u8>,
}

impl Sheet {
    pub fn worksheet(name: &str, records: Vec<u8>) -> Sheet {
        Sheet {
            name: name.into(),
            kind: 0,
            state: 0,
            records,
        }
    }
}

/// The Workbook stream: globals (code page, date system, sheet list, `extra` records before the
/// first BOUNDSHEET) then each sheet's substream.
pub fn workbook_stream(date_1904: bool, extra: &[u8], sheets: &[Sheet]) -> Vec<u8> {
    let globals = |offsets: &[u32]| -> Vec<u8> {
        let mut g = bof(0x0005);
        g.extend(record(0x0042, &1252u16.to_le_bytes()));
        g.extend(record(0x0022, &u16::from(date_1904).to_le_bytes()));
        g.extend_from_slice(extra);
        for (k, s) in sheets.iter().enumerate() {
            let mut body = offsets[k].to_le_bytes().to_vec();
            body.push(s.state);
            body.push(s.kind);
            body.extend(short_string(&s.name));
            g.extend(record(0x0085, &body));
        }
        g.extend(record(0x003D, &u16s(&[0, 0, 0x4000, 0x2000, 0x0038, 0, 0, 1, 600])));
        g.extend(eof());
        g
    };
    let substream = |s: &Sheet| -> Vec<u8> {
        let kind = match s.kind {
            1 => 0x0040,
            2 => 0x0020,
            _ => 0x0010,
        };
        let mut v = bof(kind);
        v.extend_from_slice(&s.records);
        v.extend(eof());
        v
    };
    let length = globals(&vec![0; sheets.len()]).len() as u32;
    let mut offsets = Vec::new();
    let mut at = length;
    for s in sheets {
        offsets.push(at);
        at += substream(s).len() as u32;
    }
    let mut out = globals(&offsets);
    for s in sheets {
        out.extend(substream(s));
    }
    out
}

/// A complete .xls: the Workbook stream in a compound file.
pub fn xls(date_1904: bool, extra: &[u8], sheets: &[Sheet]) -> Vec<u8> {
    compound(&[("Workbook", workbook_stream(date_1904, extra, sheets))])
}

/// NUMBER: row, column, ixfe, IEEE double.
pub fn number(row: u16, column: u16, value: f64) -> Vec<u8> {
    let mut body = u16s(&[row, column, 15]);
    body.extend_from_slice(&value.to_le_bytes());
    record(0x0203, &body)
}

/// RK with the raw 32-bit RK value.
pub fn rk_cell(row: u16, column: u16, rk: u32) -> Vec<u8> {
    let mut body = u16s(&[row, column, 15]);
    body.extend_from_slice(&rk.to_le_bytes());
    record(0x027E, &body)
}

/// MULRK: consecutive RK cells from `column`.
pub fn mulrk(row: u16, column: u16, rks: &[u32]) -> Vec<u8> {
    let mut body = u16s(&[row, column]);
    for rk in rks {
        body.extend_from_slice(&15u16.to_le_bytes());
        body.extend_from_slice(&rk.to_le_bytes());
    }
    body.extend_from_slice(&(column + rks.len() as u16 - 1).to_le_bytes());
    record(0x00BD, &body)
}

/// LABELSST: row, column, ixfe, string index.
pub fn label_sst(row: u16, column: u16, index: u32) -> Vec<u8> {
    let mut body = u16s(&[row, column, 15]);
    body.extend_from_slice(&index.to_le_bytes());
    record(0x00FD, &body)
}

/// BOOLERR: a logical (`is_error` false) or an error code.
pub fn bool_err(row: u16, column: u16, value: u8, is_error: bool) -> Vec<u8> {
    let mut body = u16s(&[row, column, 15]);
    body.push(value);
    body.push(u8::from(is_error));
    record(0x0205, &body)
}

/// BLANK.
pub fn blank(row: u16, column: u16) -> Vec<u8> {
    record(0x0201, &u16s(&[row, column, 15]))
}

/// An SST holding `strings` as XLUnicodeRichExtendedString entries (`(text, runs)`, runs being
/// `(character index, font index)`), compressed when ASCII. The record splits into CONTINUE
/// records at 8224 bytes; a split inside characters repeats the flag byte as Excel writes it
/// only when `split_at` cuts a string: the kit cuts after `split_at` bytes of the body to test it.
pub fn sst(strings: &[(&str, &[(u16, u16)])], split_at: Option<usize>) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    body.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    // Entries with the byte offset where each one's characters begin, for a cut inside them.
    let mut cut_flag: Option<(usize, bool)> = None;
    for (text, runs) in strings {
        let ascii = text.is_ascii();
        let units: Vec<u16> = text.encode_utf16().collect();
        body.extend_from_slice(&(units.len() as u16).to_le_bytes());
        body.push(u8::from(!ascii) | if runs.is_empty() { 0 } else { 8 });
        if !runs.is_empty() {
            body.extend_from_slice(&(runs.len() as u16).to_le_bytes());
        }
        let chars_at = body.len();
        if ascii {
            body.extend_from_slice(text.as_bytes());
        } else {
            body.extend(u16s(&units));
        }
        if let Some(cut) = split_at
            && cut_flag.is_none()
            && cut >= chars_at
            && cut < body.len()
        {
            cut_flag = Some((cut, !ascii));
        }
        for (ich, font) in *runs {
            body.extend(u16s(&[*ich, *font]));
        }
    }
    match (split_at, cut_flag) {
        (Some(cut), Some((_, wide))) => {
            let mut out = record(0x00FC, &body[..cut]);
            let mut tail = vec![u8::from(wide)];
            tail.extend_from_slice(&body[cut..]);
            out.extend(record(0x003C, &tail));
            out
        }
        (Some(cut), None) => {
            let mut out = record(0x00FC, &body[..cut]);
            out.extend(record(0x003C, &body[cut..]));
            out
        }
        _ => record(0x00FC, &body),
    }
}

/// ROW: row index, height in twips, and the flag bits (outline level, hidden 0x20, custom
/// height 0x40).
pub fn row(row: u16, height: u16, flags: u8) -> Vec<u8> {
    let mut body = u16s(&[row, 0, 0, height, 0, 0]);
    body.extend_from_slice(&(u32::from(flags) | 0x100).to_le_bytes());
    record(0x0208, &body)
}

/// COLINFO: first and last column, width in 1/256 characters, flags.
pub fn colinfo(first: u16, last: u16, width: u16, flags: u16) -> Vec<u8> {
    record(0x007D, &u16s(&[first, last, width, 15, flags, 0]))
}
