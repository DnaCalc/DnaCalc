//! A compound file (MS-CFB, version 3, 512-byte sectors) written from a tree of storages and
//! streams: the VBA project of a legacy workbook (the `_VBA_PROJECT_CUR` storage of an .xls) is
//! stored as a compound file of its own, the `vbaProject.bin` of an .xlsm. Siblings are written
//! in the order MS-CFB sorts them (shorter names first, then by upper-case name), so the tree can
//! be searched by name, and form a balanced binary tree with its deepest level red.

use std::cmp::Ordering;

/// A node of the tree to write.
pub(crate) enum Node {
    Storage(Vec<(String, Node)>),
    Stream(Vec<u8>),
}

const SECTOR: usize = 512;
const END: u32 = 0xFFFF_FFFE;
const FAT_MARK: u32 = 0xFFFF_FFFD;
const DIFAT_MARK: u32 = 0xFFFF_FFFC;
const NONE: u32 = 0xFFFF_FFFF;

struct Entry {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    red: bool,
    start: u32,
    size: u64,
    data: Vec<u8>,
}

fn compare(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<u16>, Vec<u16>) = (
        a.to_uppercase().encode_utf16().collect(),
        b.to_uppercase().encode_utf16().collect(),
    );
    a.len().cmp(&b.len()).then_with(|| a.cmp(&b))
}

/// Flattens `children` into `entries`, returning the id of the root of their balanced tree.
fn add(entries: &mut Vec<Entry>, mut children: Vec<(String, Node)>) -> u32 {
    if children.is_empty() {
        return NONE;
    }
    children.sort_by(|a, b| compare(&a.0, &b.0));
    // The siblings take consecutive ids; their own children follow once all have one.
    let mut ids = Vec::with_capacity(children.len());
    let mut storages: Vec<(u32, Vec<(String, Node)>)> = Vec::new();
    for (name, node) in children {
        let id = entries.len() as u32;
        ids.push(id);
        let (kind, data) = match node {
            Node::Stream(data) => (2u8, data),
            Node::Storage(kids) => {
                storages.push((id, kids));
                (1u8, Vec::new())
            }
        };
        entries.push(Entry {
            name,
            kind,
            left: NONE,
            right: NONE,
            child: NONE,
            red: false,
            start: END,
            size: 0,
            data,
        });
    }
    let count = ids.len();
    let mut depth_max = 0usize;
    while (1usize << depth_max) - 1 < count {
        depth_max += 1;
    }
    let complete = (1usize << depth_max) - 1 == count;
    fn build(entries: &mut [Entry], ids: &[u32], depth: usize, depth_max: usize, complete: bool) -> u32 {
        if ids.is_empty() {
            return NONE;
        }
        let mid = ids.len() / 2;
        let id = ids[mid];
        let left = build(entries, &ids[..mid], depth + 1, depth_max, complete);
        let right = build(entries, &ids[mid + 1..], depth + 1, depth_max, complete);
        entries[id as usize].left = left;
        entries[id as usize].right = right;
        entries[id as usize].red = !complete && depth + 1 == depth_max;
        id
    }
    let root = build(entries, &ids, 0, depth_max, complete);
    for (id, kids) in storages {
        let child = add(entries, kids);
        entries[id as usize].child = child;
    }
    root
}

/// The compound file holding `root`'s children, or `None` if its sector IDs cannot be represented.
pub(crate) fn write(root: Vec<(String, Node)>) -> Option<Vec<u8>> {
    let mut entries = vec![Entry {
        name: "Root Entry".to_owned(),
        kind: 5,
        left: NONE,
        right: NONE,
        child: NONE,
        red: false,
        start: END,
        size: 0,
        data: Vec::new(),
    }];
    let top = add(&mut entries, root);
    entries[0].child = top;
    // Streams under 4096 bytes go in the mini stream.
    let mut mini = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    for entry in entries.iter_mut().filter(|e| e.kind == 2) {
        entry.size = entry.data.len() as u64;
        if entry.data.is_empty() || entry.data.len() >= 4096 {
            continue;
        }
        entry.start = mini_fat.len() as u32;
        let units = entry.data.len().div_ceil(64);
        for u in 0..units {
            mini_fat.push(if u + 1 == units { END } else { mini_fat.len() as u32 + 1 });
        }
        mini.extend_from_slice(&entry.data);
        mini.resize(mini_fat.len() * 64, 0);
    }
    entries[0].size = mini.len() as u64;
    let directory_sectors = (entries.len() * 128).div_ceil(SECTOR);
    let mini_fat_sectors = (mini_fat.len() * 4).div_ceil(SECTOR);
    let mini_sectors = mini.len().div_ceil(SECTOR);
    let big: Vec<usize> = (0..entries.len())
        .filter(|k| entries[*k].kind == 2 && entries[*k].data.len() >= 4096)
        .collect();
    let big_sectors: usize = big.iter().map(|k| entries[*k].data.len().div_ceil(SECTOR)).sum();
    let data_sectors = directory_sectors + mini_fat_sectors + mini_sectors + big_sectors;
    // Each DIFAT sector holds 127 FAT sector IDs and the next DIFAT sector ID.
    // Both FAT and DIFAT sectors themselves consume FAT entries, so size them together.
    let mut fat_sectors = 1usize;
    let mut difat_sectors = 0usize;
    loop {
        let needed_fat = data_sectors.checked_add(fat_sectors)?.checked_add(difat_sectors)?.div_ceil(128);
        let needed_difat = needed_fat.saturating_sub(109).div_ceil(127);
        if needed_fat == fat_sectors && needed_difat == difat_sectors { break; }
        fat_sectors = needed_fat;
        difat_sectors = needed_difat;
    }
    let total_sectors = data_sectors.checked_add(fat_sectors)?.checked_add(difat_sectors)?;
    if total_sectors >= DIFAT_MARK as usize { return None; }
    let mut fat = vec![NONE; fat_sectors.checked_mul(128)?];
    let mut next = 0usize;
    for _ in 0..fat_sectors {
        fat[next] = FAT_MARK;
        next += 1;
    }
    for _ in 0..difat_sectors {
        fat[next] = DIFAT_MARK;
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
    let directory_start = chain(&mut fat, directory_sectors, &mut next);
    let mini_fat_start = chain(&mut fat, mini_fat_sectors, &mut next);
    let mini_start = chain(&mut fat, mini_sectors, &mut next);
    for k in &big {
        let sectors = entries[*k].data.len().div_ceil(SECTOR);
        entries[*k].start = chain(&mut fat, sectors, &mut next);
    }
    entries[0].start = if mini.is_empty() { END } else { mini_start };

    let mut out = vec![0u8; SECTOR];
    out[..8].copy_from_slice(&super::cfb::SIGNATURE);
    out[0x18..0x1A].copy_from_slice(&0x3Eu16.to_le_bytes());
    out[0x1A..0x1C].copy_from_slice(&3u16.to_le_bytes());
    out[0x1C..0x1E].copy_from_slice(&0xFFFEu16.to_le_bytes());
    out[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
    out[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
    out[0x2C..0x30].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
    out[0x30..0x34].copy_from_slice(&directory_start.to_le_bytes());
    out[0x38..0x3C].copy_from_slice(&4096u32.to_le_bytes());
    out[0x3C..0x40].copy_from_slice(&mini_fat_start.to_le_bytes());
    out[0x40..0x44].copy_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
    out[0x44..0x48].copy_from_slice(&(if difat_sectors == 0 { END } else { fat_sectors as u32 }).to_le_bytes());
    out[0x48..0x4C].copy_from_slice(&(difat_sectors as u32).to_le_bytes());
    for k in 0..109 {
        let value = if k < fat_sectors { k as u32 } else { NONE };
        out[0x4C + 4 * k..0x50 + 4 * k].copy_from_slice(&value.to_le_bytes());
    }
    for value in &fat {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for sector in 0..difat_sectors {
        for slot in 0..127 {
            let index = 109 + sector * 127 + slot;
            let value = if index < fat_sectors { index as u32 } else { NONE };
            out.extend_from_slice(&value.to_le_bytes());
        }
        let next_difat = if sector + 1 < difat_sectors { (fat_sectors + sector + 1) as u32 } else { END };
        out.extend_from_slice(&next_difat.to_le_bytes());
    }
    let mut directory = Vec::with_capacity(entries.len() * 128);
    for entry in &entries {
        let mut e = vec![0u8; 128];
        let units: Vec<u16> = entry.name.encode_utf16().collect();
        for (k, u) in units.iter().take(31).enumerate() {
            e[2 * k..2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
        e[0x40..0x42].copy_from_slice(&((units.len().min(31) as u16 + 1) * 2).to_le_bytes());
        e[0x42] = entry.kind;
        e[0x43] = u8::from(!entry.red);
        e[0x44..0x48].copy_from_slice(&entry.left.to_le_bytes());
        e[0x48..0x4C].copy_from_slice(&entry.right.to_le_bytes());
        e[0x4C..0x50].copy_from_slice(&entry.child.to_le_bytes());
        e[0x74..0x78].copy_from_slice(&entry.start.to_le_bytes());
        e[0x78..0x80].copy_from_slice(&entry.size.to_le_bytes());
        directory.extend(e);
    }
    directory.resize(directory_sectors * SECTOR, 0);
    for k in entries.len()..directory_sectors * 4 {
        for b in &mut directory[k * 128 + 0x44..k * 128 + 0x50] {
            *b = 0xFF;
        }
    }
    out.extend(directory);
    for value in &mini_fat {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.resize(SECTOR * (1 + fat_sectors + difat_sectors + directory_sectors + mini_fat_sectors), 0xFF);
    out.extend_from_slice(&mini);
    out.resize(SECTOR * (1 + fat_sectors + difat_sectors + directory_sectors + mini_fat_sectors + mini_sectors), 0);
    for k in &big {
        out.extend_from_slice(&entries[*k].data);
        out.resize(out.len().div_ceil(SECTOR) * SECTOR, 0);
    }
    Some(out)
}


#[cfg(test)]
mod tests {
    use super::{write, Node};
    use crate::xls::cfb::Compound;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn difat_chain_retains_streams() {
        // Exercise zero, one, and two DIFAT sectors, with both mini and regular streams.
        for size in [4096, 7_200_000, 16_000_000] {
            let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let bytes = write(vec![("large".into(), Node::Stream(data.clone())),
                ("small".into(), Node::Stream(vec![1, 2, 3]))]).unwrap();
            let compound = Compound::open(&bytes).unwrap();
            assert_eq!(compound.read("large").unwrap().unwrap(), data);
            assert_eq!(compound.read("small").unwrap().unwrap(), [1, 2, 3]);
        }
    }
}
