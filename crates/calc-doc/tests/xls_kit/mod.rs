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
    const DIFATS: u32 = 0xFFFF_FFFC;
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
    while fat_sectors * 128 < data_sectors + fat_sectors + fat_sectors.saturating_sub(109).div_ceil(127) {
        fat_sectors += 1;
    }
    let difat_sectors = fat_sectors.saturating_sub(109).div_ceil(127);
    let mut fat: Vec<u32> = vec![0xFFFF_FFFF; fat_sectors * 128];
    let mut next = 0usize;
    for _ in 0..fat_sectors {
        fat[next] = FATS;
        next += 1;
    }
    for _ in 0..difat_sectors {
        fat[next] = DIFATS;
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
    let difat_start = if difat_sectors == 0 { END } else { fat_sectors as u32 };
    out[0x44..0x48].copy_from_slice(&difat_start.to_le_bytes());
    out[0x48..0x4C].copy_from_slice(&(difat_sectors as u32).to_le_bytes());
    for k in 0..109 {
        let value = if k < fat_sectors { k as u32 } else { 0xFFFF_FFFF };
        out[0x4C + 4 * k..0x50 + 4 * k].copy_from_slice(&value.to_le_bytes());
    }
    // The FAT sectors.
    for value in &fat {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for sector in 0..difat_sectors {
        for index in 0..127 {
            let fat_index = 109 + sector * 127 + index;
            let value = if fat_index < fat_sectors { fat_index as u32 } else { 0xFFFF_FFFF };
            out.extend_from_slice(&value.to_le_bytes());
        }
        let next = if sector + 1 < difat_sectors { (fat_sectors + sector + 1) as u32 } else { END };
        out.extend_from_slice(&next.to_le_bytes());
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
    out.resize(SECTOR * (1 + fat_sectors + difat_sectors + dir_sectors) + mini_fat_sectors * SECTOR, 0xFF);
    out.extend_from_slice(&mini);
    out.resize(
        SECTOR * (1 + fat_sectors + difat_sectors + dir_sectors + mini_fat_sectors + mini_sectors),
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

/// Formula tokens (ptg bytes).
pub mod ptg {
    use super::u16s;
    pub fn int(n: u16) -> Vec<u8> {
        let mut v = vec![0x1E];
        v.extend(n.to_le_bytes());
        v
    }
    pub fn num(x: f64) -> Vec<u8> {
        let mut v = vec![0x1F];
        v.extend(x.to_le_bytes());
        v
    }
    pub fn text(s: &str) -> Vec<u8> {
        let mut v = vec![0x17, s.chars().count() as u8];
        if s.is_ascii() {
            v.push(0);
            v.extend(s.bytes());
        } else {
            v.push(1);
            v.extend(u16s(&s.encode_utf16().collect::<Vec<_>>()));
        }
        v
    }
    pub const REL_ROW: u16 = 0x8000;
    pub const REL_COL: u16 = 0x4000;
    /// PtgRef: 0-based row and column; `flags` REL_ROW | REL_COL.
    pub fn cell(row: u16, column: u16, flags: u16) -> Vec<u8> {
        let mut v = vec![0x24];
        v.extend(u16s(&[row, column | flags]));
        v
    }
    /// PtgRefN: relative offsets (signed) for relative parts.
    pub fn cell_n(row: i16, column: i8, flags: u16) -> Vec<u8> {
        let mut v = vec![0x2C];
        v.extend(u16s(&[row as u16, u16::from(column as u8) | flags]));
        v
    }
    pub fn area(rows: (u16, u16), columns: (u16, u16), flags: u16) -> Vec<u8> {
        let mut v = vec![0x25];
        v.extend(u16s(&[rows.0, rows.1, columns.0 | flags, columns.1 | flags]));
        v
    }
    pub fn area3d(ixti: u16, rows: (u16, u16), columns: (u16, u16), flags: u16) -> Vec<u8> {
        let mut v = vec![0x3B];
        v.extend(u16s(&[ixti, rows.0, rows.1, columns.0 | flags, columns.1 | flags]));
        v
    }
    pub fn cell3d(ixti: u16, row: u16, column: u16, flags: u16) -> Vec<u8> {
        let mut v = vec![0x3A];
        v.extend(u16s(&[ixti, row, column | flags]));
        v
    }
    pub fn name(index: u32) -> Vec<u8> {
        let mut v = vec![0x23];
        v.extend(index.to_le_bytes());
        v
    }
    pub fn name_x(ixti: u16, index: u32) -> Vec<u8> {
        let mut v = vec![0x39];
        v.extend(ixti.to_le_bytes());
        v.extend(index.to_le_bytes());
        v
    }
    pub fn exp(row: u16, column: u16) -> Vec<u8> {
        let mut v = vec![0x01];
        v.extend(u16s(&[row, column]));
        v
    }
    pub fn table(row: u16, column: u16) -> Vec<u8> {
        let mut v = vec![0x02];
        v.extend(u16s(&[row, column]));
        v
    }
    pub const ADD: u8 = 0x03;
    pub const MUL: u8 = 0x05;
    pub const CONCAT: u8 = 0x08;
    pub const PAREN: u8 = 0x15;
    /// PtgFuncVar with `count` arguments of function `id`.
    pub fn func_var(count: u8, id: u16) -> Vec<u8> {
        let mut v = vec![0x42, count];
        v.extend(id.to_le_bytes());
        v
    }
    /// PtgFunc of a fixed-arity function.
    pub fn func(id: u16) -> Vec<u8> {
        let mut v = vec![0x41];
        v.extend(id.to_le_bytes());
        v
    }
    pub fn join(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }
    pub fn op(code: u8) -> Vec<u8> {
        vec![code]
    }
    /// PtgArray (7 reserved bytes); its data goes in the formula's extra bytes.
    pub fn array() -> Vec<u8> {
        let mut v = vec![0x20];
        v.extend([0u8; 7]);
        v
    }
}

/// FORMULA: cell, 8 value bytes, tokens, extra data.
pub fn formula(row: u16, column: u16, value: [u8; 8], rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    let mut body = u16s(&[row, column, 15]);
    body.extend(value);
    body.extend(u16s(&[0, 0, 0]));
    body.extend((rgce.len() as u16).to_le_bytes());
    body.extend_from_slice(rgce);
    body.extend_from_slice(rgcb);
    record(0x0006, &body)
}

pub fn num_value(x: f64) -> [u8; 8] {
    x.to_le_bytes()
}
pub const STR_VALUE: [u8; 8] = [0, 0, 0, 0, 0, 0, 0xFF, 0xFF];
pub fn bool_value(b: bool) -> [u8; 8] {
    [1, 0, u8::from(b), 0, 0, 0, 0xFF, 0xFF]
}
pub fn err_value(code: u8) -> [u8; 8] {
    [2, 0, code, 0, 0, 0, 0xFF, 0xFF]
}

/// STRING: the text result of the formula before it.
pub fn string_result(text: &str) -> Vec<u8> {
    let mut body = (text.chars().count() as u16).to_le_bytes().to_vec();
    body.push(0);
    body.extend(text.bytes());
    record(0x0207, &body)
}

/// SHRFMLA over rows/columns (inclusive) with the given tokens.
pub fn shrfmla(rows: (u16, u16), columns: (u8, u8), rgce: &[u8]) -> Vec<u8> {
    let mut body = u16s(&[rows.0, rows.1]);
    body.extend([columns.0, columns.1, 0, 1]);
    body.extend((rgce.len() as u16).to_le_bytes());
    body.extend_from_slice(rgce);
    record(0x04BC, &body)
}

/// ARRAY over rows/columns (inclusive).
pub fn array_record(rows: (u16, u16), columns: (u8, u8), rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    let mut body = u16s(&[rows.0, rows.1]);
    body.extend([columns.0, columns.1]);
    body.extend([0u8; 6]);
    body.extend((rgce.len() as u16).to_le_bytes());
    body.extend_from_slice(rgce);
    body.extend_from_slice(rgcb);
    record(0x0221, &body)
}

/// TABLE: range, flags (0x04 row input, 0x08 two inputs), row input cell, column input cell.
pub fn table_record(
    rows: (u16, u16),
    columns: (u8, u8),
    flags: u16,
    row_input: (u16, u16),
    column_input: (u16, u16),
) -> Vec<u8> {
    let mut body = u16s(&[rows.0, rows.1]);
    body.extend([columns.0, columns.1]);
    body.extend(u16s(&[flags, row_input.0, row_input.1, column_input.0, column_input.1]));
    record(0x0236, &body)
}

/// NAME: flags, scope (0 workbook, else 1-based sheet), name text (or the built-in code), tokens.
pub fn name_record(flags: u16, scope: u16, name: &str, rgce: &[u8]) -> Vec<u8> {
    name_record_extra(flags, scope, name, rgce, &[])
}

pub fn name_record_extra(flags: u16, scope: u16, name: &str, rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    let mut body = flags.to_le_bytes().to_vec();
    body.extend([0, name.len() as u8]);
    body.extend((rgce.len() as u16).to_le_bytes());
    body.extend(u16s(&[0, scope]));
    body.extend([0, 0, 0, 0]);
    body.push(0);
    body.extend(name.bytes());
    body.extend_from_slice(rgce);
    body.extend_from_slice(rgcb);
    record(0x0018, &body)
}

/// SUPBOOK for this workbook.
pub fn supbook_self(sheets: u16) -> Vec<u8> {
    record(0x01AE, &u16s(&[sheets, 0x0401]))
}

/// SUPBOOK for an add-in's functions.
pub fn supbook_addin() -> Vec<u8> {
    record(0x01AE, &u16s(&[1, 0x3A01]))
}

/// SUPBOOK for another workbook: encoded path and its sheet names.
pub fn supbook_external(path: &str, sheets: &[&str]) -> Vec<u8> {
    let mut body = u16s(&[sheets.len() as u16, path.len() as u16]);
    body.push(0);
    body.extend(path.bytes());
    for s in sheets {
        body.extend((s.len() as u16).to_le_bytes());
        body.push(0);
        body.extend(s.bytes());
    }
    record(0x01AE, &body)
}

/// EXTERNNAME of the SUPBOOK before it.
pub fn extern_name(name: &str) -> Vec<u8> {
    let mut body = u16s(&[0, 0, 0]);
    body.extend(short_string(name));
    body.extend(u16s(&[0]));
    record(0x0023, &body)
}

/// EXTERNSHEET: each XTI as (supbook, first sheet, last sheet).
pub fn extern_sheet(xti: &[(u16, i16, i16)]) -> Vec<u8> {
    let mut body = (xti.len() as u16).to_le_bytes().to_vec();
    for (b, f, l) in xti {
        body.extend(u16s(&[*b, *f as u16, *l as u16]));
    }
    record(0x0017, &body)
}

/// FONT: height in twips, grbit, palette colour, weight, script, underline, name.
pub fn font(height: u16, flags: u16, color: u16, weight: u16, name: &str) -> Vec<u8> {
    let mut body = u16s(&[height, flags, color, weight, 0]);
    body.extend([0, 0, 0, 0]);
    body.extend(short_string(name));
    record(0x0031, &body)
}

/// FORMAT: id and code.
pub fn format_record(id: u16, code: &str) -> Vec<u8> {
    let mut body = id.to_le_bytes().to_vec();
    body.extend((code.chars().count() as u16).to_le_bytes());
    body.push(0);
    body.extend(code.bytes());
    record(0x041E, &body)
}

/// The fields of an XF record.
pub struct Xf {
    pub font: u16,
    pub format: u16,
    /// Bit 2 style XF, bits 4-15 parent.
    pub flags: u16,
    pub alignment: u8,
    pub rotation: u8,
    pub indent: u8,
    pub used: u8,
    pub border1: u32,
    pub border2: u32,
    pub fill: u16,
}

impl Xf {
    pub fn plain() -> Xf {
        Xf {
            font: 0,
            format: 0,
            flags: 0x0001,
            alignment: 0x20,
            rotation: 0,
            indent: 0,
            used: 0,
            border1: 0,
            border2: 0,
            fill: 0x20C0,
        }
    }
    pub fn record(&self) -> Vec<u8> {
        let mut body = u16s(&[self.font, self.format, self.flags]);
        body.extend([self.alignment, self.rotation, self.indent, self.used]);
        body.extend(self.border1.to_le_bytes());
        body.extend(self.border2.to_le_bytes());
        body.extend(self.fill.to_le_bytes());
        record(0x00E0, &body)
    }
}

/// STYLE for a built-in style.
pub fn style_built_in(xf: u16, id: u8) -> Vec<u8> {
    let mut body = (xf | 0x8000).to_le_bytes().to_vec();
    body.extend([id, 0xFF]);
    record(0x0293, &body)
}

/// A globals prelude with `fonts`, `formats` and `xfs`: 15 style XFs then the XFs given.
pub fn formatting(fonts: &[Vec<u8>], formats: &[Vec<u8>], cell_xfs: &[Xf]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fonts {
        out.extend_from_slice(f);
    }
    for f in formats {
        out.extend_from_slice(f);
    }
    for _ in 0..15 {
        out.extend(Xf { flags: 0xFFF4, ..Xf::plain() }.record());
    }
    for xf in cell_xfs {
        out.extend(xf.record());
    }
    out.extend(style_built_in(0, 0));
    out
}

/// The BIFF8 SheetExt form without optional fields (MS-XLS 2.4.259).
pub fn sheet_ext(color: u8) -> Vec<u8> {
    let mut b = vec![0x62, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    b.extend(20u32.to_le_bytes());
    b.extend(u32::from(color).to_le_bytes());
    record(0x0862, &b)
}

pub fn window2(flags: u16, top: u16, left: u16) -> Vec<u8> {
    let mut body = u16s(&[flags, top, left]);
    body.extend([0u8; 14]);
    record(0x023E, &body)
}

pub fn pane(x: u16, y: u16, top: u16, left: u16, active: u8) -> Vec<u8> {
    let mut body = u16s(&[x, y, top, left]);
    body.push(active);
    record(0x0041, &body)
}

pub fn selection(pane: u8, active: (u16, u16), ranges: &[(u16, u16, u8, u8)]) -> Vec<u8> {
    let mut body = vec![pane];
    body.extend(u16s(&[active.0, active.1, 0, ranges.len() as u16]));
    for (r1, r2, c1, c2) in ranges {
        body.extend(u16s(&[*r1, *r2]));
        body.extend([*c1, *c2]);
    }
    record(0x001D, &body)
}

pub fn merged(areas: &[(u16, u16, u16, u16)]) -> Vec<u8> {
    let mut body = (areas.len() as u16).to_le_bytes().to_vec();
    for (r1, r2, c1, c2) in areas {
        body.extend(u16s(&[*r1, *r2, *c1, *c2]));
    }
    record(0x00E5, &body)
}

/// HLINK to a URL (with an optional display name and a location inside the document).
pub fn hlink_url(cell: (u16, u16), url: &str, display: Option<&str>, location: Option<&str>) -> Vec<u8> {
    let mut body = u16s(&[cell.0, cell.0, cell.1, cell.1]);
    body.extend([0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    body.extend(2u32.to_le_bytes());
    let mut flags = 0x01u32 | 0x02;
    if display.is_some() {
        flags |= 0x10;
    }
    if location.is_some() {
        flags |= 0x08;
    }
    body.extend(flags.to_le_bytes());
    let wide = |s: &str| -> Vec<u8> {
        let mut units: Vec<u16> = s.encode_utf16().collect();
        units.push(0);
        let mut v = (units.len() as u32).to_le_bytes().to_vec();
        v.extend(u16s(&units));
        v
    };
    if let Some(d) = display {
        body.extend(wide(d));
    }
    body.extend([0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    let mut units: Vec<u16> = url.encode_utf16().collect();
    units.push(0);
    body.extend(((units.len() * 2) as u32).to_le_bytes());
    body.extend(u16s(&units));
    if let Some(l) = location {
        body.extend(wide(l));
    }
    record(0x01B8, &body)
}

/// HLINK with a file moniker, optional Unicode path, and a location inside the target.
pub fn hlink_file(cell: (u16, u16), ansi: &str, unicode: Option<&str>, location: &str) -> Vec<u8> {
    let mut body = u16s(&[cell.0, cell.0, cell.1, cell.1]);
    body.extend([0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    body.extend(2u32.to_le_bytes());
    body.extend(9u32.to_le_bytes());
    body.extend([0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
    body.extend(0u16.to_le_bytes());
    body.extend(((ansi.len() + 1) as u32).to_le_bytes());
    body.extend(ansi.as_bytes());
    body.push(0);
    body.extend(u16s(&[0xFFFF, 0xDEAD]));
    body.extend([0; 20]);
    if let Some(path) = unicode {
        let units: Vec<u16> = path.encode_utf16().collect();
        body.extend(((units.len() * 2 + 6) as u32).to_le_bytes());
        body.extend(((units.len() * 2) as u32).to_le_bytes());
        body.extend(3u16.to_le_bytes());
        body.extend(u16s(&units));
    } else {
        body.extend(0u32.to_le_bytes());
    }
    let mut units: Vec<u16> = location.encode_utf16().collect();
    units.push(0);
    body.extend((units.len() as u32).to_le_bytes());
    body.extend(u16s(&units));
    record(0x01B8, &body)
}

/// OBJ of a comment (object type 0x19) with id `id`.
pub fn obj_note(id: u16) -> Vec<u8> {
    let mut body = u16s(&[0x0015, 18, 0x0019, id, 0x4011]);
    body.extend([0u8; 12]);
    body.extend(u16s(&[0, 0]));
    record(0x005D, &body)
}

/// TXO with its text and formatting-run CONTINUE records.
pub fn txo(text: &str) -> Vec<u8> {
    let mut body = u16s(&[0x0212, 0, 0, 0, 0, text.len() as u16, 16]);
    body.extend([0u8; 4]);
    let mut out = record(0x01B6, &body);
    let mut chars = vec![0u8];
    chars.extend(text.bytes());
    out.extend(record(0x003C, &chars));
    out.extend(record(0x003C, &[0u8; 16]));
    out
}

/// NOTE: cell, flags (2 shows it), object id, author.
pub fn note(row: u16, column: u16, flags: u16, object: u16, author: &str) -> Vec<u8> {
    let mut body = u16s(&[row, column, flags, object]);
    body.extend((author.len() as u16).to_le_bytes());
    body.push(0);
    body.extend(author.bytes());
    body.push(0);
    record(0x001C, &body)
}

/// Raw BIFF4 fixtures use the same cell payload helpers where the layouts agree.
pub mod v4 {
    use super::*;
    pub fn xls(records: &[u8]) -> Vec<u8> {
        [record(0x0409, &u16s(&[0, 0x10, 0])), record(0x0042, &u16s(&[1252])),
            records.to_vec(), record(0x000a, &[])].concat()
    }
    pub fn font(height: u16, flags: u16, color: u16, name: &str) -> Vec<u8> {
        let mut b = u16s(&[height, flags, color]); b.push(name.len() as u8); b.extend(name.bytes());
        record(0x0231, &b)
    }
    pub fn xf(font: u8, format: u8, flags: u16, alignment: u8, area: u16, border: u32) -> Vec<u8> {
        let mut b = vec![font, format]; b.extend(flags.to_le_bytes()); b.extend([alignment, 0xfc]);
        b.extend(area.to_le_bytes()); b.extend(border.to_le_bytes()); record(0x0443, &b)
    }
    pub fn formula(row: u16, col: u16, tokens: &[u8], extra: &[u8]) -> Vec<u8> {
        let mut b = u16s(&[row, col, 0]); b.extend(0f64.to_le_bytes()); b.extend(0u16.to_le_bytes());
        b.extend((tokens.len() as u16).to_le_bytes()); b.extend(tokens); b.extend(extra);
        record(0x0406, &b)
    }
    pub fn name(name: &str, tokens: &[u8]) -> Vec<u8> {
        let mut b = vec![0, 0, 0, name.len() as u8]; b.extend((tokens.len() as u16).to_le_bytes());
        b.extend(name.bytes()); b.extend(tokens); record(0x0218, &b)
    }
}

/// BIFF5/BIFF7 builders: byte strings in the workbook's code page, 14-bit rows, local link tables.
pub mod v5 {
    use super::{record, u16s};

    pub fn bof(kind: u16) -> Vec<u8> {
        let mut body = u16s(&[0x0500, kind, 0x1234, 0x07C9]);
        body.extend([0; 8]);
        record(0x0809, &body)
    }

    /// A byte string with an 8-bit length.
    pub fn string(bytes: &[u8]) -> Vec<u8> {
        let mut v = vec![bytes.len() as u8];
        v.extend_from_slice(bytes);
        v
    }

    pub struct Sheet {
        pub name: Vec<u8>,
        pub records: Vec<u8>,
    }

    /// The `Book` stream: CODEPAGE, `extra` globals records, BOUNDSHEETs, then each sheet.
    pub fn book(code_page: u16, extra: &[u8], sheets: &[Sheet]) -> Vec<u8> {
        let globals = |offsets: &[u32]| -> Vec<u8> {
            let mut g = bof(0x0005);
            g.extend(record(0x0042, &code_page.to_le_bytes()));
            g.extend(record(0x0022, &0u16.to_le_bytes()));
            g.extend_from_slice(extra);
            for (k, s) in sheets.iter().enumerate() {
                let mut body = offsets[k].to_le_bytes().to_vec();
                body.extend([0, 0]);
                body.extend(string(&s.name));
                g.extend(record(0x0085, &body));
            }
            g.extend(super::eof());
            g
        };
        let substream = |s: &Sheet| -> Vec<u8> {
            let mut v = bof(0x0010);
            v.extend_from_slice(&s.records);
            v.extend(super::eof());
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

    pub fn xls(code_page: u16, extra: &[u8], sheets: &[Sheet]) -> Vec<u8> {
        super::compound(&[("Book", book(code_page, extra, sheets))])
    }

    /// LABEL: row, column, ixfe, bytes.
    pub fn label(row: u16, column: u16, bytes: &[u8]) -> Vec<u8> {
        let mut body = u16s(&[row, column, 0, bytes.len() as u16]);
        body.extend_from_slice(bytes);
        record(0x0204, &body)
    }

    /// FORMULA with a numeric result and BIFF5 tokens.
    pub fn formula(row: u16, column: u16, value: f64, rgce: &[u8]) -> Vec<u8> {
        let mut body = u16s(&[row, column, 0]);
        body.extend(value.to_le_bytes());
        body.extend(u16s(&[0, 0, 0]));
        body.extend((rgce.len() as u16).to_le_bytes());
        body.extend_from_slice(rgce);
        record(0x0006, &body)
    }

    /// tRef: 14-bit row with the relative flags (bit 15 row, bit 14 column), a byte column.
    pub fn cell(row: u16, column: u8, relative_row: bool, relative_column: bool) -> Vec<u8> {
        let mut flags = row;
        if relative_row {
            flags |= 0x8000;
        }
        if relative_column {
            flags |= 0x4000;
        }
        let mut v = vec![0x24];
        v.extend(flags.to_le_bytes());
        v.push(column);
        v
    }

    /// tRef3d into this workbook: the (negative) index to the sheet's EXTERNSHEET record, then
    /// 8 unused bytes, the first and last sheet and the cell.
    pub fn cell3d(extern_index: i16, sheet: u16, row: u16, column: u8) -> Vec<u8> {
        let mut v = vec![0x3A];
        v.extend((-extern_index).to_le_bytes());
        v.extend([0u8; 8]);
        v.extend(u16s(&[sheet, sheet, row | 0x8000 | 0x4000]));
        v.push(column);
        v
    }

    /// EXTERNSHEET for a sheet of this workbook (the length field is one less than the bytes).
    pub fn extern_own(sheet: &[u8]) -> Vec<u8> {
        let mut body = vec![sheet.len() as u8, 3];
        body.extend_from_slice(sheet);
        record(0x0017, &body)
    }

    /// NAME: flags, scope (0 or a 1-based sheet), name, tokens.
    pub fn name(flags: u16, scope: u16, name: &[u8], rgce: &[u8]) -> Vec<u8> {
        let mut body = flags.to_le_bytes().to_vec();
        body.extend([0, name.len() as u8]);
        body.extend((rgce.len() as u16).to_le_bytes());
        body.extend(u16s(&[0, scope]));
        body.extend([0, 0, 0, 0]);
        body.extend_from_slice(name);
        body.extend_from_slice(rgce);
        record(0x0018, &body)
    }
}

impl v5::Sheet {
    pub fn new(name: &str, records: Vec<u8>) -> v5::Sheet {
        v5::Sheet {
            name: name.as_bytes().to_vec(),
            records,
        }
    }
}

pub mod v5_styles {
    use super::{record, u16s, v5};

    /// FONT of BIFF5: the name is a byte string.
    pub fn font(height: u16, flags: u16, color: u16, weight: u16, name: &str) -> Vec<u8> {
        let mut body = u16s(&[height, flags, color, weight, 0]);
        body.extend([0, 0, 0, 0]);
        body.extend(v5::string(name.as_bytes()));
        record(0x0031, &body)
    }

    pub fn format(id: u16, code: &str) -> Vec<u8> {
        let mut body = id.to_le_bytes().to_vec();
        body.extend(v5::string(code.as_bytes()));
        record(0x041E, &body)
    }

    /// XF of BIFF5 (16 bytes).
    pub fn xf(font: u16, format: u16, flags: u16, alignment: u8, orientation_used: u8, fill: u32, lines: u32) -> Vec<u8> {
        let mut body = u16s(&[font, format, flags]);
        body.extend([alignment, orientation_used]);
        body.extend(fill.to_le_bytes());
        body.extend(lines.to_le_bytes());
        record(0x00E0, &body)
    }

    /// NOTE of BIFF5: the text is in the record.
    pub fn note(row: u16, column: u16, text: &str) -> Vec<u8> {
        let mut body = u16s(&[row, column, text.len() as u16]);
        body.extend(text.bytes());
        record(0x001C, &body)
    }
}

/// OfficeArt and chart builders.
pub mod art {
    use super::{record, u16s};

    /// An OfficeArt record: version, instance, type, body.
    pub fn atom(version: u16, instance: u16, kind: u16, body: &[u8]) -> Vec<u8> {
        let mut v = ((instance << 4) | version).to_le_bytes().to_vec();
        v.extend(kind.to_le_bytes());
        v.extend((body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    /// MSODRAWINGGROUP with one PNG in the blip store.
    pub fn group_with_png(png: &[u8]) -> Vec<u8> {
        let mut blip = vec![0x11u8; 16];
        blip.push(0xFF);
        blip.extend_from_slice(png);
        let blip_atom = atom(0, 0x6E0, 0xF01E, &blip);
        group_with_blip(6, &blip_atom)
    }

    fn group_with_blip(kind: u8, blip_atom: &[u8]) -> Vec<u8> {
        let mut bse = vec![kind, kind];
        bse.extend([0x22u8; 16]);
        bse.extend([0, 0]);
        bse.extend((blip_atom.len() as u32).to_le_bytes());
        bse.extend(1u32.to_le_bytes());
        bse.extend(0u32.to_le_bytes());
        bse.extend([0, 0, 0, 0]);
        bse.extend_from_slice(blip_atom);
        let store = atom(0xF, 1, 0xF001, &atom(2, u16::from(kind), 0xF007, &bse));
        record(0x00EB, &atom(0xF, 0, 0xF000, &store))
    }

    /// An EMF blip with either a valid or deliberately damaged zlib checksum.
    pub fn group_with_emf(emf: &[u8], corrupt_checksum: bool) -> Vec<u8> {
        let mut compressed = miniz_oxide::deflate::compress_to_vec_zlib(emf, 6);
        if corrupt_checksum { *compressed.last_mut().unwrap() ^= 1; }
        let mut blip = vec![0x11u8; 16];
        let mut header = [0u8; 34];
        header[0..4].copy_from_slice(&(emf.len() as u32).to_le_bytes());
        header[20..24].copy_from_slice(&1_000_000u32.to_le_bytes());
        header[24..28].copy_from_slice(&1_000_000u32.to_le_bytes());
        header[28..32].copy_from_slice(&(compressed.len() as u32).to_le_bytes());
        header[33] = 0xfe;
        blip.extend(header); blip.extend(compressed);
        group_with_blip(2, &atom(0, 0x3d4, 0xf01a, &blip))
    }

    /// A shape container: type (instance of the Fsp), pib (0 for none), anchor, text box flag.
    pub fn shape(shape_type: u16, picture: u32, anchor: [u16; 8], text_box: bool) -> Vec<u8> {
        let mut children = atom(2, shape_type, 0xF00A, &[1, 4, 0, 0, 0, 0x0A, 0, 0]);
        if picture != 0 {
            let mut prop = 0x4104u16.to_le_bytes().to_vec();
            prop.extend(picture.to_le_bytes());
            children.extend(atom(3, 1, 0xF00B, &prop));
        }
        let mut a = vec![0u8, 0];
        a.extend(u16s(&anchor));
        children.extend(atom(0, 0, 0xF010, &a));
        if text_box {
            children.extend(atom(0, 0, 0xF00D, &[]));
        }
        children.extend(atom(0, 0, 0xF011, &[]));
        atom(0xF, 0, 0xF004, &children)
    }

    /// A standalone line with its explicit OfficeArt properties and OBJ.
    pub fn plain_line(properties: &[(u16, u32)], flags: u32, anchor: [u16; 8]) -> Vec<u8> {
        let mut shape = shape(20, 0, anchor, false);
        shape[20..24].copy_from_slice(&flags.to_le_bytes());
        let body: Vec<_> = properties.iter().flat_map(|(id, value)|
            id.to_le_bytes().into_iter().chain(value.to_le_bytes())).collect();
        shape.splice(8..8, atom(3, properties.len() as u16, 0xf00b, &body));
        let length = (shape.len() - 8) as u32;
        shape[4..8].copy_from_slice(&length.to_le_bytes());
        [drawing(&[shape]), obj(1, 1)].concat()
    }

    /// MSODRAWING holding the group shape and then `shapes`.
    pub fn drawing(shapes: &[Vec<u8>]) -> Vec<u8> {
        let group = atom(0xF, 0, 0xF004, &atom(2, 0, 0xF00A, &[0, 4, 0, 0, 5, 0, 0, 0]));
        let mut inner = group;
        for s in shapes {
            inner.extend_from_slice(s);
        }
        record(0x00EC, &atom(0xF, 0, 0xF002, &atom(0xF, 0, 0xF003, &inner)))
    }

    /// OBJ of type `kind` and id.
    pub fn obj(kind: u16, id: u16) -> Vec<u8> {
        let mut body = u16s(&[0x0015, 18, kind, id, 0x4011]);
        body.extend([0u8; 12]);
        body.extend(u16s(&[0, 0]));
        record(0x005D, &body)
    }

    // Chart records.
    pub fn begin() -> Vec<u8> {
        record(0x1033, &[])
    }
    pub fn end() -> Vec<u8> {
        record(0x1034, &[])
    }
    /// AI with a formula reference: id (0 title, 1 values, 2 categories) and tokens.
    pub fn ai(id: u8, rgce: &[u8]) -> Vec<u8> {
        let mut body = vec![id, 2];
        body.extend(u16s(&[0, 0, rgce.len() as u16]));
        body.extend_from_slice(rgce);
        record(0x1051, &body)
    }
    /// SERIESTEXT: id 0 and a short string.
    pub fn series_text(text: &str) -> Vec<u8> {
        let mut body = 0u16.to_le_bytes().to_vec();
        body.extend(super::short_string(text));
        record(0x100D, &body)
    }
    pub fn series() -> Vec<u8> {
        record(0x1003, &u16s(&[1, 1, 4, 4, 0, 0]))
    }
    /// A chart substream: BOF (chart), CHART, one chart format group of type `bar_flags`, `series`
    /// (each (title, categories, values) range tokens), a title and a legend, EOF.
    pub fn chart(series_list: &[(&str, Vec<u8>, Vec<u8>)], bar_flags: u16, title: &str) -> Vec<u8> {
        let mut out = super::bof(0x0020);
        out.extend(record(0x1002, &[0u8; 16]));
        out.extend(begin());
        for (name, cats, vals) in series_list {
            out.extend(series());
            out.extend(begin());
            out.extend(ai(2, cats));
            out.extend(ai(1, vals));
            out.extend(series_text(name));
            out.extend(record(0x1045, &u16s(&[0])));
            out.extend(end());
        }
        out.extend(record(0x1014, &[0u8; 20]));
        out.extend(begin());
        out.extend(record(0x1017, &u16s(&[0, 150, bar_flags])));
        out.extend(end());
        // A TEXT block for the chart title: linked as object 1.
        out.extend(record(0x1025, &[0u8; 32]));
        out.extend(begin());
        out.extend(record(0x104F, &[0u8; 20]));
        out.extend(record(0x1051, &[0, 1, 0, 0, 0, 0, 0, 0]));
        out.extend(series_text(title));
        out.extend(record(0x1027, &u16s(&[1, 0, 0, 0])));
        out.extend(end());
        out.extend(record(0x1015, &[0u8; 20]));
        out.extend(begin());
        out.extend(end());
        out.extend(end());
        out.extend(super::eof());
        out
    }
}

/// Rules: conditional formats, data validations, the automatic filter.
pub mod rules {
    use super::{record, u16s};

    pub fn cf_header(count: u16, ranges: &[(u16, u16, u16, u16)]) -> Vec<u8> {
        let mut body = u16s(&[count, 1]);
        let (r1, r2) = (ranges.iter().map(|r| r.0).min().unwrap(), ranges.iter().map(|r| r.1).max().unwrap());
        let (c1, c2) = (ranges.iter().map(|r| r.2).min().unwrap(), ranges.iter().map(|r| r.3).max().unwrap());
        body.extend(u16s(&[r1, r2, c1, c2, ranges.len() as u16]));
        for (a, b, c, d) in ranges {
            body.extend(u16s(&[*a, *b, *c, *d]));
        }
        record(0x01B0, &body)
    }

    /// CF: type, operator, tokens, and a bold font and a solid pattern (style, foreground, background).
    pub fn cf(kind: u8, operator: u8, first: &[u8], second: &[u8], bold: bool, pattern: Option<(u16, u16, u16)>) -> Vec<u8> {
        let mut flags = 0u32;
        if bold {
            flags |= 0x0400_0000;
        }
        if pattern.is_some() {
            flags |= 0x2000_0000;
        }
        let mut body = vec![kind, operator];
        body.extend(u16s(&[first.len() as u16, second.len() as u16]));
        body.extend(flags.to_le_bytes());
        body.extend(2u16.to_le_bytes());
        if bold {
            let mut font = vec![0u8; 118];
            font[64..68].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            font[72..74].copy_from_slice(&700u16.to_le_bytes());
            font[80..84].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            font[88..92].copy_from_slice(&0xFFFF_FFFDu32.to_le_bytes());
            font[92..96].copy_from_slice(&1u32.to_le_bytes());
            font[96..100].copy_from_slice(&1u32.to_le_bytes());
            body.extend(font);
        }
        if let Some((style, fore, back)) = pattern {
            body.extend((style << 10).to_le_bytes());
            body.extend((fore | back << 7).to_le_bytes());
        }
        body.extend_from_slice(first);
        body.extend_from_slice(second);
        record(0x01B1, &body)
    }

    fn string(text: &str) -> Vec<u8> {
        let mut v = (text.len() as u16).to_le_bytes().to_vec();
        v.push(0);
        v.extend(text.bytes());
        v
    }

    /// DV: type (3 list), operator, flags set by the caller, strings, tokens and one range.
    pub fn dv(
        flags: u32,
        strings: [&str; 4],
        first: &[u8],
        second: &[u8],
        range: (u16, u16, u16, u16),
    ) -> Vec<u8> {
        let mut body = flags.to_le_bytes().to_vec();
        for s in strings {
            body.extend(string(s));
        }
        body.extend(u16s(&[first.len() as u16, 0]));
        body.extend_from_slice(first);
        body.extend(u16s(&[second.len() as u16, 0]));
        body.extend_from_slice(second);
        body.extend(u16s(&[1, range.0, range.1, range.2, range.3]));
        record(0x01BE, &body)
    }

    pub fn autofilter_info(columns: u16) -> Vec<u8> {
        record(0x009D, &columns.to_le_bytes())
    }

    /// AUTOFILTER with one numeric "greater than" criterion on `column`.
    pub fn autofilter_greater_than(column: u16, value: f64) -> Vec<u8> {
        let mut body = u16s(&[column, 0]);
        let mut doper = vec![4u8, 4];
        doper.extend(value.to_le_bytes());
        body.extend(doper);
        body.extend([0u8; 10]);
        record(0x009E, &body)
    }
}
