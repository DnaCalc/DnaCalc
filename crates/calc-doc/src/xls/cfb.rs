//! The OLE compound file (MS-CFB) a legacy Excel workbook lives in: the header, the FAT, DIFAT and
//! mini FAT chains, and the directory, read from memory. The reader is tolerant where the format
//! allows it (a short last sector, a stream size past its chain) and refuses what it cannot
//! follow (a chain that loops or leaves the file), never panicking on a malformed file.

/// The signature every compound file starts with.
pub(crate) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

const FAT_SECTOR: u32 = 0xFFFF_FFFD;
const DIFAT_SECTOR: u32 = 0xFFFF_FFFC;
const MAX_REGULAR: u32 = 0xFFFF_FFFA;

/// Why a compound file cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CfbError(pub(crate) String);

fn bad<T>(what: impl Into<String>) -> Result<T, CfbError> {
    Err(CfbError(what.into()))
}

/// What a directory entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Storage,
    Stream,
    Root,
}

/// One directory entry with its path from the root.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) path: String,
    pub(crate) kind: EntryKind,
    pub(crate) start: u32,
    pub(crate) size: u64,
}

pub(crate) struct Compound<'a> {
    data: &'a [u8],
    sector_size: usize,
    mini_size: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_cutoff: u64,
    mini_stream: Vec<u8>,
    entries: Vec<Entry>,
}

fn u16_at(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn u64_at(data: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&data[at..at + 8]);
    u64::from_le_bytes(a)
}

/// Whether `data` starts with the compound file signature.
pub(crate) fn is_compound(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == SIGNATURE
}

impl<'a> Compound<'a> {
    pub(crate) fn open(data: &'a [u8]) -> Result<Compound<'a>, CfbError> {
        if data.len() < 512 || !is_compound(data) {
            return bad("the compound file header is missing");
        }
        let major = u16_at(data, 0x1A);
        let sector_shift = u16_at(data, 0x1E);
        let mini_shift = u16_at(data, 0x20);
        if !matches!((major, sector_shift), (3, 9) | (4, 12)) {
            return bad(format!(
                "version {major} with sector shift {sector_shift} is not read"
            ));
        }
        if mini_shift != 6 {
            return bad(format!("mini sector shift {mini_shift} is not read"));
        }
        let sector_size = 1usize << sector_shift;
        let mini_size = 1usize << mini_shift;
        let dir_start = u32_at(data, 0x30);
        let mini_cutoff = u64::from(u32_at(data, 0x38));
        let mini_fat_start = u32_at(data, 0x3C);
        let difat_start = u32_at(data, 0x44);
        let difat_count = u32_at(data, 0x48) as usize;
        let sector_count = data.len().saturating_sub(sector_size).div_ceil(sector_size);
        // Sector n starts at (n + 1) * sector size: the header takes the first sector's place.
        let sector = |n: u32| -> Option<&'a [u8]> {
            let start = (n as usize).checked_add(1)?.checked_mul(sector_size)?;
            if start >= data.len() {
                return None;
            }
            Some(&data[start..(start + sector_size).min(data.len())])
        };

        // The FAT sector list: the header's 109 entries, then the DIFAT chain.
        let mut fat_sectors: Vec<u32> = Vec::new();
        for k in 0..109 {
            let n = u32_at(data, 0x4C + 4 * k);
            if n <= MAX_REGULAR {
                fat_sectors.push(n);
            }
        }
        let mut difat = difat_start;
        let mut steps = 0usize;
        while difat <= MAX_REGULAR && steps < difat_count.min(sector_count + 1) {
            let Some(bytes) = sector(difat) else {
                return bad("a DIFAT sector lies outside the file");
            };
            if bytes.len() < sector_size {
                break;
            }
            for k in 0..sector_size / 4 - 1 {
                let n = u32_at(bytes, 4 * k);
                if n <= MAX_REGULAR {
                    fat_sectors.push(n);
                }
            }
            difat = u32_at(bytes, sector_size - 4);
            steps += 1;
        }
        let mut fat: Vec<u32> = Vec::with_capacity(fat_sectors.len() * sector_size / 4);
        for n in &fat_sectors {
            let Some(bytes) = sector(*n) else {
                return bad("a FAT sector lies outside the file");
            };
            fat.extend(bytes.chunks_exact(4).map(|p| u32_at(p, 0)));
        }
        let mut compound = Compound {
            data,
            sector_size,
            mini_size,
            fat,
            mini_fat: Vec::new(),
            mini_cutoff,
            mini_stream: Vec::new(),
            entries: Vec::new(),
        };
        let directory = compound.chain(dir_start, None)?;
        // The directory: 128-byte entries forming a tree by sibling ids.
        let count = directory.len() / 128;
        if count == 0 {
            return bad("the directory is empty");
        }
        struct Raw {
            name: String,
            kind: u8,
            left: u32,
            right: u32,
            child: u32,
            start: u32,
            size: u64,
        }
        let mut raw: Vec<Raw> = Vec::with_capacity(count);
        for k in 0..count {
            let e = &directory[k * 128..(k + 1) * 128];
            let name_len = (u16_at(e, 0x40) as usize).min(64);
            let units: Vec<u16> = e[..name_len.saturating_sub(2)]
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
            let mut size = u64_at(e, 0x78);
            if major == 3 {
                size &= 0xFFFF_FFFF;
            }
            raw.push(Raw {
                name: String::from_utf16_lossy(&units),
                kind: e[0x42],
                left: u32_at(e, 0x44),
                right: u32_at(e, 0x48),
                child: u32_at(e, 0x4C),
                start: u32_at(e, 0x74),
                size,
            });
        }
        if raw[0].kind != 5 {
            return bad("the directory has no root entry");
        }
        let mut entries = vec![Entry {
            path: String::new(),
            kind: EntryKind::Root,
            start: raw[0].start,
            size: raw[0].size,
        }];
        let mut seen = vec![false; count];
        seen[0] = true;
        let mut stack: Vec<(u32, String)> = vec![(raw[0].child, String::new())];
        while let Some((id, parent)) = stack.pop() {
            if id as usize >= count {
                continue;
            }
            let id = id as usize;
            if seen[id] {
                continue;
            }
            seen[id] = true;
            let r = &raw[id];
            let path = if parent.is_empty() {
                r.name.clone()
            } else {
                format!("{parent}/{}", r.name)
            };
            stack.push((r.left, parent.clone()));
            stack.push((r.right, parent));
            let kind = match r.kind {
                1 => EntryKind::Storage,
                2 => EntryKind::Stream,
                _ => continue,
            };
            if kind == EntryKind::Storage {
                stack.push((r.child, path.clone()));
            }
            entries.push(Entry {
                path,
                kind,
                start: r.start,
                size: r.size,
            });
        }
        entries[1..].sort_by(|a, b| a.path.cmp(&b.path));
        // The mini FAT and the mini stream (the root's chain).
        if mini_fat_start <= MAX_REGULAR {
            let bytes = compound.chain(mini_fat_start, None)?;
            compound.mini_fat = bytes.chunks_exact(4).map(|p| u32_at(p, 0)).collect();
        }
        let root = &entries[0];
        if root.start <= MAX_REGULAR {
            compound.mini_stream = compound.chain(root.start, Some(root.size))?;
        }
        compound.entries = entries;
        Ok(compound)
    }

    /// The bytes of the FAT chain starting at `start`, cut to `size` when given.
    fn chain(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::new();
        let mut at = start;
        let mut steps = 0usize;
        while at <= MAX_REGULAR {
            steps += 1;
            if steps > self.fat.len() + 1 {
                return bad("a sector chain loops");
            }
            let begin = (at as usize + 1) * self.sector_size;
            if begin >= self.data.len() {
                return bad("a sector chain leaves the file");
            }
            let end = (begin + self.sector_size).min(self.data.len());
            out.extend_from_slice(&self.data[begin..end]);
            if let Some(size) = size
                && out.len() as u64 >= size
            {
                break;
            }
            at = match self.fat.get(at as usize) {
                Some(next) => *next,
                None => return bad("a sector chain leaves the FAT"),
            };
            if at == FAT_SECTOR || at == DIFAT_SECTOR {
                return bad("a sector chain runs into a FAT sector");
            }
        }
        if let Some(size) = size {
            out.truncate(size as usize);
        }
        Ok(out)
    }

    fn mini_chain(&self, start: u32, size: u64) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::new();
        let mut at = start;
        let mut steps = 0usize;
        while at <= MAX_REGULAR && (out.len() as u64) < size {
            steps += 1;
            if steps > self.mini_fat.len() + 1 {
                return bad("a mini sector chain loops");
            }
            let begin = at as usize * self.mini_size;
            if begin >= self.mini_stream.len() {
                return bad("a mini sector chain leaves the mini stream");
            }
            let end = (begin + self.mini_size).min(self.mini_stream.len());
            out.extend_from_slice(&self.mini_stream[begin..end]);
            at = match self.mini_fat.get(at as usize) {
                Some(next) => *next,
                None => return bad("a mini sector chain leaves the mini FAT"),
            };
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// Every storage and stream with its path from the root (`/` separated, no leading slash).
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries[1..]
    }

    /// The storage at `path` and everything under it as a compound file of its own (the children
    /// of the storage become the root's), or `None` when there is no such storage.
    pub(crate) fn extract_storage(&self, path: &str) -> Result<Option<Vec<u8>>, CfbError> {
        use super::cfb_write::Node;
        let prefix = format!("{}/", path.to_ascii_uppercase());
        if !self
            .entries()
            .iter()
            .any(|e| e.kind == EntryKind::Storage && e.path.eq_ignore_ascii_case(path))
        {
            return Ok(None);
        }
        // Build the tree of what lies under the storage.
        fn insert(nodes: &mut Vec<(String, Node)>, parts: &[&str], node: Node) {
            let Some((first, rest)) = parts.split_first() else { return };
            if rest.is_empty() {
                nodes.push(((*first).to_owned(), node));
                return;
            }
            let at = match nodes.iter().position(|(n, _)| n == first) {
                Some(at) => at,
                None => {
                    nodes.push(((*first).to_owned(), Node::Storage(Vec::new())));
                    nodes.len() - 1
                }
            };
            if let Node::Storage(kids) = &mut nodes[at].1 {
                insert(kids, rest, node);
            }
        }
        let mut root: Vec<(String, Node)> = Vec::new();
        // Storages first (so a stream's parents exist), then streams.
        let under: Vec<&Entry> = self
            .entries()
            .iter()
            .filter(|e| e.path.to_ascii_uppercase().starts_with(&prefix))
            .collect();
        for entry in under.iter().filter(|e| e.kind == EntryKind::Storage) {
            let relative: Vec<&str> = entry.path[prefix.len()..].split('/').collect();
            let exists = {
                // A storage already created as a parent stays as it is.
                fn find(nodes: &[(String, Node)], parts: &[&str]) -> bool {
                    match parts.split_first() {
                        None => true,
                        Some((first, rest)) => nodes.iter().any(|(n, node)| {
                            n == first
                                && match node {
                                    Node::Storage(kids) => find(kids, rest),
                                    Node::Stream(_) => false,
                                }
                        }),
                    }
                }
                find(&root, &relative)
            };
            if !exists {
                insert(&mut root, &relative, Node::Storage(Vec::new()));
            }
        }
        for entry in under.iter().filter(|e| e.kind == EntryKind::Stream) {
            let relative: Vec<&str> = entry.path[prefix.len()..].split('/').collect();
            let data = self.read_entry(entry)?;
            insert(&mut root, &relative, Node::Stream(data));
        }
        super::cfb_write::write(root).map(Some).ok_or_else(|| {
            CfbError("the extracted compound storage exceeds the sector writer limit (dc-4j0e)".to_owned())
        })
    }

    /// The stream at `path`, compared without regard to ASCII case.
    pub(crate) fn find(&self, path: &str) -> Option<&Entry> {
        self.entries()
            .iter()
            .find(|e| e.kind == EntryKind::Stream && e.path.eq_ignore_ascii_case(path))
    }

    /// The bytes of the stream at `path`, `None` when there is none.
    pub(crate) fn read(&self, path: &str) -> Result<Option<Vec<u8>>, CfbError> {
        match self.find(path) {
            Some(entry) => self.read_entry(entry).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>, CfbError> {
        if entry.size < self.mini_cutoff {
            self.mini_chain(entry.start, entry.size)
        } else {
            self.chain(entry.start, Some(entry.size))
        }
    }
}
