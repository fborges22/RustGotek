use crate::Result;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn word(bytes: &[u8], offset: usize) -> usize {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize
}

/// Create a data floppy, with no operating system boot loader.
pub fn create(path: &Path, kib: usize) -> Result<()> {
    let data = formatted_image(kib)?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let result = f.write_all(&data).and_then(|()| f.sync_all());
    if result.is_err() {
        drop(f);
        let _ = fs::remove_file(path);
    }
    result?;
    Ok(())
}

/// Shared formatter for standalone images and fixed-stride bank layouts.
pub fn formatted_image(kib: usize) -> Result<Vec<u8>> {
    let (cluster, roots, fatsz, sectors_per_track, media) = match kib {
        360 => (2, 112, 2, 9, 0xfd),
        720 => (2, 112, 3, 9, 0xf9),
        1200 => (1, 224, 7, 15, 0xf9),
        1440 => (1, 224, 9, 18, 0xf0),
        2880 => (2, 240, 9, 36, 0xf0),
        _ => return Err("FAT12 capacity must be 360, 720, 1200, 1440, or 2880 KiB".into()),
    };
    let mut data = vec![0; kib * 1024];
    data[..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    data[3..11].copy_from_slice(b"RUSTGOTK");
    for (offset, value) in [
        (11, 512),
        (14, 1),
        (17, roots),
        (19, kib * 2),
        (22, fatsz),
        (24, sectors_per_track),
        (26, 2),
    ] {
        data[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    data[13] = cluster;
    data[16] = 2;
    data[21] = media;
    data[38] = 0x29;
    let serial = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as u32;
    data[39..43].copy_from_slice(&serial.to_le_bytes());
    data[43..54].copy_from_slice(b"NO NAME    ");
    data[54..62].copy_from_slice(b"FAT12   ");
    // INT 18h invokes the BIOS fallback; halt if it returns.
    data[62..68].copy_from_slice(&[0xcd, 0x18, 0xfa, 0xf4, 0xeb, 0xfd]);
    data[510..512].copy_from_slice(&[0x55, 0xaa]);
    for sector in [1, 1 + fatsz] {
        data[sector * 512..sector * 512 + 3].copy_from_slice(&[media, 0xff, 0xff]);
    }
    inspect(&data)?;
    Ok(data)
}

#[derive(Debug)]
pub struct Layout {
    pub sectors: usize,
    pub cluster_bytes: usize,
    pub clusters: usize,
    pub free_clusters: usize,
    pub root_entries: usize,
    pub sectors_per_track: usize,
    pub heads: usize,
}

/// Validate a FAT12 superfloppy layout and FAT copies, not directory chains.
pub fn inspect(data: &[u8]) -> Result<Layout> {
    if data.len() < 512 || data[510..512] != [0x55, 0xaa] {
        return Err("missing FAT boot-sector signature".into());
    }
    let bytes = word(data, 11);
    let cluster = data[13] as usize;
    let reserved = word(data, 14);
    let copies = data[16] as usize;
    let root_entries = word(data, 17);
    let sectors = word(data, 19);
    let fatsz = word(data, 22);
    if bytes != 512
        || !cluster.is_power_of_two()
        || cluster > 128
        || reserved == 0
        || !(1..=2).contains(&copies)
        || root_entries == 0
        || !(root_entries * 32).is_multiple_of(bytes)
        || fatsz == 0
        || sectors == 0
    {
        return Err("unsupported or invalid FAT12 floppy BPB".into());
    }
    if sectors * bytes != data.len() {
        return Err("image size differs from BPB sector count".into());
    }
    let overhead = reserved + copies * fatsz + (root_entries * 32).div_ceil(bytes);
    if overhead >= sectors {
        return Err("FAT regions exceed image size".into());
    }
    let clusters = (sectors - overhead) / cluster;
    if clusters == 0 || clusters >= 4085 {
        return Err("cluster count is not FAT12".into());
    }
    let fat_bytes = fatsz * bytes;
    if ((clusters + 2) * 3).div_ceil(2) > fat_bytes {
        return Err("FAT is too small for data region".into());
    }
    let fat = &data[reserved * bytes..reserved * bytes + fat_bytes];
    if fat[..3] != [data[21], 0xff, 0xff] {
        return Err("invalid FAT reserved entries".into());
    }
    for copy in 1..copies {
        let start = (reserved + copy * fatsz) * bytes;
        if &data[start..start + fat_bytes] != fat {
            return Err("FAT copies differ".into());
        }
    }
    let mut free_clusters = 0;
    for n in 2..clusters + 2 {
        let offset = n + n / 2;
        let pair = word(fat, offset);
        let entry = if n % 2 == 0 { pair & 0xfff } else { pair >> 4 };
        if entry == 0 {
            free_clusters += 1;
        }
    }
    Ok(Layout {
        sectors,
        cluster_bytes: bytes * cluster,
        clusters,
        free_clusters,
        root_entries,
        sectors_per_track: word(data, 24),
        heads: word(data, 26),
    })
}

fn read_image(path: &Path) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err("image must be a regular file".into());
    }
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 32 * 1024 * 1024 {
        return Err("image exceeds 32 MiB inspection limit".into());
    }
    Ok(data)
}

pub fn inspect_file(path: &Path) -> Result<()> {
    let data = read_image(path)?;
    let layout = inspect(&data)?;
    println!(
        "FAT12 floppy: {} bytes\nSectors: {} (512 bytes each)\nGeometry: {} heads, {} sectors/track\nCluster size: {} bytes\nData clusters: {}\nFree space: {} bytes\nRoot directory capacity: {} entries\nBPB and FAT copies checked; directory chains have not been checked.",
        data.len(),
        layout.sectors,
        layout.heads,
        layout.sectors_per_track,
        layout.cluster_bytes,
        layout.clusters,
        layout.free_clusters * layout.cluster_bytes,
        layout.root_entries
    );
    Ok(())
}

struct Image {
    data: Vec<u8>,
    layout: Layout,
    root_start: usize,
    data_start: usize,
    fat_start: usize,
}

struct Entry {
    offset: usize,
    name: String,
    directory: bool,
    cluster: usize,
    size: usize,
}

impl Image {
    fn allocation(&self, n: usize) -> usize {
        let pair = word(&self.data, self.fat_start + n + n / 2);
        if n.is_multiple_of(2) {
            pair & 0xfff
        } else {
            pair >> 4
        }
    }

    fn set_allocation(&mut self, n: usize, value: usize) {
        let fat_size = word(&self.data, 22) * 512;
        for copy in 0..self.data[16] as usize {
            let at = self.fat_start + copy * fat_size + n + n / 2;
            let pair = word(&self.data, at);
            let updated = if n.is_multiple_of(2) {
                (pair & 0xf000) | value
            } else {
                (pair & 0x000f) | (value << 4)
            };
            self.data[at..at + 2].copy_from_slice(&(updated as u16).to_le_bytes());
        }
    }

    fn directory_offsets(&self, directory: Option<usize>) -> Result<Vec<usize>> {
        if let Some(first) = directory {
            let mut offsets = vec![];
            for n in self.chain(first)? {
                let start = self.data_start + (n - 2) * self.layout.cluster_bytes;
                offsets.extend((start..start + self.layout.cluster_bytes).step_by(32));
            }
            Ok(offsets)
        } else {
            Ok((self.root_start..self.data_start).step_by(32).collect())
        }
    }

    // Traverse iteratively so a corrupt directory tree cannot overflow the stack.
    fn check_references(&self) -> Result<std::collections::HashSet<usize>> {
        let mut pending = vec![None];
        let mut claimed = std::collections::HashSet::new();
        while let Some(directory) = pending.pop() {
            for entry in self.entries(directory)? {
                if entry.cluster == 0 && entry.size == 0 && !entry.directory {
                    continue;
                }
                let chain = self.chain(entry.cluster)?;
                if !entry.directory && chain.len() != entry.size.div_ceil(self.layout.cluster_bytes)
                {
                    return Err("file size does not match allocated cluster chain".into());
                }
                for n in chain {
                    if !claimed.insert(n) {
                        return Err("cross-linked file or directory cycle".into());
                    }
                }
                if entry.directory {
                    pending.push(Some(entry.cluster));
                }
            }
        }
        Ok(claimed)
    }

    fn open(path: &Path) -> Result<Self> {
        let data = read_image(path)?;
        let layout = inspect(&data)?;
        let fat_start = word(&data, 14) * 512;
        let root_start = fat_start + data[16] as usize * word(&data, 22) * 512;
        let data_start = root_start + layout.root_entries * 32;
        Ok(Self {
            data,
            layout,
            root_start,
            data_start,
            fat_start,
        })
    }

    fn chain(&self, first: usize) -> Result<Vec<usize>> {
        let mut chain = vec![];
        let mut seen = vec![false; self.layout.clusters + 2];
        let mut n = first;
        loop {
            if n < 2 || n >= seen.len() || n >= 0xff0 {
                return Err("invalid cluster in FAT chain".into());
            }
            if seen[n] {
                return Err("cycle in FAT chain".into());
            }
            seen[n] = true;
            chain.push(n);
            let pair = word(&self.data, self.fat_start + n + n / 2);
            n = if n.is_multiple_of(2) {
                pair & 0xfff
            } else {
                pair >> 4
            };
            if n >= 0xff8 {
                return Ok(chain);
            }
        }
    }

    fn cluster(&self, n: usize) -> &[u8] {
        let start = self.data_start + (n - 2) * self.layout.cluster_bytes;
        &self.data[start..start + self.layout.cluster_bytes]
    }

    fn entries(&self, directory: Option<usize>) -> Result<Vec<Entry>> {
        let mut entries = vec![];
        for offset in self.directory_offsets(directory)? {
            let e = &self.data[offset..offset + 32];
            if e[0] == 0 {
                break;
            }
            // Deleted entries, long names and volume labels do not name files.
            if e[0] == 0xe5 || e[11] & 8 != 0 {
                continue;
            }
            let base = String::from_utf8_lossy(&e[..8]).trim_end().to_owned();
            let extension = String::from_utf8_lossy(&e[8..11]).trim_end().to_owned();
            if base == "." || base == ".." {
                continue;
            }
            let name = if extension.is_empty() {
                base
            } else {
                format!("{base}.{extension}")
            };
            entries.push(Entry {
                offset,
                name,
                directory: e[11] & 0x10 != 0,
                cluster: word(e, 26),
                size: u32::from_le_bytes(e[28..32].try_into()?) as usize,
            });
        }
        Ok(entries)
    }

    fn resolve(&self, path: &str) -> Result<Entry> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\\'))
        {
            return Err("use an image path with DOS short names and no . or .. components".into());
        }
        let mut dir = None;
        let mut visited = std::collections::HashSet::new();
        for (index, part) in parts.iter().enumerate() {
            let mut matching = self
                .entries(dir)?
                .into_iter()
                .filter(|e| e.name.eq_ignore_ascii_case(part));
            let entry = matching
                .next()
                .ok_or_else(|| format!("image entry not found: {part}"))?;
            if matching.next().is_some() {
                return Err("duplicate directory name".into());
            }
            if index == parts.len() - 1 {
                return Ok(entry);
            }
            if !entry.directory {
                return Err("intermediate image path is not a directory".into());
            }
            if !visited.insert(entry.cluster) {
                return Err("directory cycle".into());
            }
            dir = Some(entry.cluster);
        }
        Err("empty image path".into())
    }
}

#[derive(Debug, Clone)]
pub struct DiskEntry {
    pub name: String,
    pub directory: bool,
    pub size: u64,
}
pub fn directory_entries(path: &Path, directory: &str) -> Result<Vec<DiskEntry>> {
    let image = Image::open(path)?;
    let cluster = if directory == "/" {
        None
    } else {
        let entry = image.resolve(directory)?;
        if !entry.directory {
            return Err("image path is not a directory".into());
        }
        Some(entry.cluster)
    };
    Ok(image
        .entries(cluster)?
        .into_iter()
        .map(|entry| DiskEntry {
            name: entry.name,
            directory: entry.directory,
            size: entry.size as u64,
        })
        .collect())
}
pub fn list(path: &Path, directory: &str) -> Result<()> {
    for entry in directory_entries(path, directory)? {
        println!(
            "{:>10} {}{}",
            entry.size,
            entry.name.escape_default(),
            if entry.directory { "/" } else { "" }
        );
    }
    Ok(())
}

pub fn extract(path: &Path, name: &str, destination: &Path) -> Result<()> {
    let image = Image::open(path)?;
    let entry = image.resolve(name)?;
    if entry.directory {
        return Err("cannot extract a directory as a file".into());
    }
    let chain = if entry.cluster == 0 && entry.size == 0 {
        vec![]
    } else {
        image.chain(entry.cluster)?
    };
    if chain.len() != entry.size.div_ceil(image.layout.cluster_bytes) {
        return Err("file size does not match allocated cluster chain".into());
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| -> std::io::Result<()> {
        let mut left = entry.size;
        for n in chain {
            let bytes = image.cluster(n);
            let count = left.min(bytes.len());
            output.write_all(&bytes[..count])?;
            left -= count;
        }
        output.sync_all()
    })();
    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(destination);
    }
    result?;
    Ok(())
}

fn short_name(name: &str) -> Result<[u8; 11]> {
    let parts: Vec<_> = name.split('.').collect();
    if parts.len() > 2
        || parts[0].is_empty()
        || parts[0].len() > 8
        || (parts.len() == 2 && (parts[1].is_empty() || parts[1].len() > 3))
    {
        return Err("new file needs an ASCII DOS 8.3 name".into());
    }
    let mut encoded = [b' '; 11];
    for (index, part) in parts.iter().enumerate() {
        if !part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'()-@^_`{}~".contains(&b))
        {
            return Err("unsupported character in DOS short name".into());
        }
        let start = if index == 0 { 0 } else { 8 };
        encoded[start..start + part.len()].copy_from_slice(part.to_ascii_uppercase().as_bytes());
    }
    Ok(encoded)
}

/// Save an edited image under a new name, retaining the original byte for byte.
pub fn import(path: &Path, source: &Path, name: &str, output: &Path) -> Result<()> {
    add_entry(path, Some(source), name, output)
}

pub fn mkdir(path: &Path, name: &str, output: &Path) -> Result<()> {
    add_entry(path, None, name, output)
}

fn add_entry(path: &Path, source: Option<&Path>, name: &str, output: &Path) -> Result<()> {
    let mut image = Image::open(path)?;
    image.check_references()?;
    let name = name.trim_start_matches('/');
    let (parent, leaf) = name.rsplit_once('/').unwrap_or(("", name));
    let encoded = short_name(leaf)?;
    let directory = if parent.is_empty() {
        None
    } else {
        let entry = image.resolve(parent)?;
        if !entry.directory {
            return Err("image parent is not a directory".into());
        }
        Some(entry.cluster)
    };
    if image
        .entries(directory)?
        .iter()
        .any(|e| e.name.eq_ignore_ascii_case(leaf))
    {
        return Err("image file already exists".into());
    }
    let payload = if let Some(source) = source {
        read_image(source)?
    } else {
        vec![0; image.layout.cluster_bytes]
    };
    let offsets = image.directory_offsets(directory)?;
    let existing_slot = offsets
        .iter()
        .position(|&at| [0, 0xe5].contains(&image.data[at]));
    if existing_slot.is_none() && directory.is_none() {
        return Err("root directory is full".into());
    }
    let needed = payload.len().div_ceil(image.layout.cluster_bytes);
    let extra = usize::from(existing_slot.is_none());
    let free: Vec<_> = (2..(image.layout.clusters + 2).min(0xff0))
        .filter(|&n| image.allocation(n) == 0)
        .take(needed + extra)
        .collect();
    if free.len() < needed + extra {
        return Err("not enough free space in image".into());
    }
    let slot = if let Some(index) = existing_slot {
        let at = offsets[index];
        // Preserve the end marker when replacing the old final directory entry.
        if image.data[at] == 0 && index + 1 < offsets.len() {
            image.data[offsets[index + 1]] = 0;
        }
        at
    } else {
        let first = directory.ok_or("root directory cannot grow")?;
        let last = *image.chain(first)?.last().ok_or("empty directory chain")?;
        let n = free[needed];
        image.set_allocation(last, n);
        image.set_allocation(n, 0xfff);
        let start = image.data_start + (n - 2) * image.layout.cluster_bytes;
        image.data[start..start + image.layout.cluster_bytes].fill(0);
        start
    };
    for (index, &n) in free[..needed].iter().enumerate() {
        image.set_allocation(n, free[..needed].get(index + 1).copied().unwrap_or(0xfff));
        let start = image.data_start + (n - 2) * image.layout.cluster_bytes;
        let from = index * image.layout.cluster_bytes;
        let count = (payload.len() - from).min(image.layout.cluster_bytes);
        image.data[start..start + image.layout.cluster_bytes].fill(0);
        image.data[start..start + count].copy_from_slice(&payload[from..from + count]);
    }
    if source.is_none() {
        let start = image.data_start + (free[0] - 2) * image.layout.cluster_bytes;
        for (at, name, cluster) in [
            (start, b".          ", free[0]),
            (start + 32, b"..         ", directory.unwrap_or(0)),
        ] {
            let entry = &mut image.data[at..at + 32];
            entry[..11].copy_from_slice(name);
            entry[11] = 0x10;
            for offset in [16, 18, 24] {
                entry[offset..offset + 2].copy_from_slice(&33u16.to_le_bytes());
            }
            entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
        }
    }
    let entry = &mut image.data[slot..slot + 32];
    entry.fill(0);
    entry[..11].copy_from_slice(&encoded);
    entry[11] = if source.is_some() { 0x20 } else { 0x10 };
    // Deterministic valid DOS timestamp: 1980-01-01, midnight.
    for offset in [16, 18, 24] {
        entry[offset..offset + 2].copy_from_slice(&33u16.to_le_bytes());
    }
    entry[26..28].copy_from_slice(
        &(free.first().filter(|_| needed > 0).copied().unwrap_or(0) as u16).to_le_bytes(),
    );
    let file_size = if source.is_some() {
        payload.len() as u32
    } else {
        0
    };
    entry[28..32].copy_from_slice(&file_size.to_le_bytes());
    image.check_references()?;
    save_new(&image.data, output)
}

fn save_new(data: &[u8], output: &Path) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = f.write_all(data).and_then(|()| f.sync_all());
    if result.is_err() {
        drop(f);
        let _ = fs::remove_file(output);
    }
    result?;
    Ok(())
}

/// Set both DOS volume-label records while preserving the source image.
pub fn set_label(path: &Path, label: &str, output: &Path) -> Result<()> {
    let encoded = encode_label(label)?;
    let mut image = Image::open(path)?;
    image.check_references()?;
    let offsets = image.directory_offsets(None)?;
    let active: Vec<_> = offsets
        .iter()
        .copied()
        .take_while(|&at| image.data[at] != 0)
        .filter(|&at| image.data[at] != 0xe5 && image.data[at + 11] == 8)
        .collect();
    let slot = active
        .first()
        .copied()
        .or_else(|| {
            offsets
                .iter()
                .copied()
                .find(|&at| matches!(image.data[at], 0 | 0xe5))
        })
        .ok_or("Root directory is full; no room for a disk label")?;
    if image.data[slot] == 0 && slot + 32 < image.data_start {
        image.data[slot + 32] = 0;
    }
    for &at in active.iter().skip(1) {
        image.data[at] = 0xe5;
    }
    image.data[slot..slot + 32].fill(0);
    image.data[slot..slot + 11].copy_from_slice(&encoded);
    image.data[slot + 11] = 8;
    if image.data[38] == 0x29 {
        image.data[43..54].copy_from_slice(&encoded);
    }
    save_new(&image.data, output)
}

pub fn encode_label(label: &str) -> Result<[u8; 11]> {
    let label = label.trim();
    if label.is_empty()
        || label.len() > 11
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" !#$%&'()-@^_`{}~".contains(&b))
    {
        return Err("Use 1–11 ASCII letters, numbers, spaces or DOS label punctuation".into());
    }
    let mut encoded = [b' '; 11];
    encoded[..label.len()].copy_from_slice(label.to_ascii_uppercase().as_bytes());
    Ok(encoded)
}

pub fn volume_label(path: &Path) -> Result<String> {
    let image = Image::open(path)?;
    for at in image.directory_offsets(None)? {
        if image.data[at] == 0 {
            break;
        }
        if image.data[at] != 0xe5 && image.data[at + 11] == 8 {
            return Ok(String::from_utf8_lossy(&image.data[at..at + 11])
                .trim_end()
                .into());
        }
    }
    Ok(if image.data[38] == 0x29 {
        String::from_utf8_lossy(&image.data[43..54])
            .trim_end()
            .into()
    } else {
        String::new()
    })
}

pub fn validate_short_name(name: &str) -> Result<()> {
    short_name(name).map(|_| ())
}

pub fn remove(path: &Path, name: &str, output: &Path) -> Result<()> {
    let mut image = Image::open(path)?;
    image.check_references()?;
    let entry = image.resolve(name)?;
    if image.data[entry.offset + 11] & 1 != 0 {
        return Err("image entry is read-only".into());
    }
    if entry.directory && !image.entries(Some(entry.cluster))?.is_empty() {
        return Err("directory is not empty".into());
    }
    let name = name.trim_start_matches('/');
    let parent = name.rsplit_once('/').map(|(parent, _)| parent);
    let directory = parent
        .map(|p| image.resolve(p).map(|e| e.cluster))
        .transpose()?;
    let deleted = associated_records(&image, directory, &entry)?;
    if entry.cluster != 0 {
        for n in image.chain(entry.cluster)? {
            image.set_allocation(n, 0);
        }
    }
    for at in deleted {
        image.data[at] = 0xe5;
    }
    image.check_references()?;
    save_new(&image.data, output)
}

fn associated_records(
    image: &Image,
    directory: Option<usize>,
    entry: &Entry,
) -> Result<Vec<usize>> {
    let offsets = image.directory_offsets(directory)?;
    let index = offsets
        .iter()
        .position(|&at| at == entry.offset)
        .ok_or("missing directory entry")?;
    let mut deleted = vec![entry.offset];
    let checksum = image.data[entry.offset..entry.offset + 11]
        .iter()
        .fold(0u8, |sum, &b| sum.rotate_right(1).wrapping_add(b));
    // Long-name records precede the short alias in reverse ordinal order.
    // Remove only a complete, matching sequence; refuse malformed associations.
    let mut ordinal = 1u8;
    for &at in offsets[..index].iter().rev() {
        let record = &image.data[at..at + 32];
        if record[0] == 0xe5 || record[11] != 0x0f {
            break;
        }
        if ordinal > 20
            || record[0] & 0xbf != ordinal
            || record[12] != 0
            || record[13] != checksum
            || word(record, 26) != 0
        {
            return Err("malformed long filename records".into());
        }
        deleted.push(at);
        if record[0] & 0x40 != 0 {
            ordinal = 0;
            break;
        }
        ordinal += 1;
    }
    if deleted.len() > 1 && ordinal != 0 {
        return Err("incomplete long filename records".into());
    }
    Ok(deleted)
}

pub fn rename(path: &Path, name: &str, new_name: &str, output: &Path) -> Result<()> {
    let encoded = short_name(new_name)?;
    let mut image = Image::open(path)?;
    image.check_references()?;
    let entry = image.resolve(name)?;
    if image.data[entry.offset + 11] & 1 != 0 {
        return Err("image entry is read-only".into());
    }
    let parent = name
        .trim_start_matches('/')
        .rsplit_once('/')
        .map(|(p, _)| p);
    let directory = parent
        .map(|p| image.resolve(p).map(|e| e.cluster))
        .transpose()?;
    if image
        .entries(directory)?
        .iter()
        .any(|e| e.offset != entry.offset && e.name.eq_ignore_ascii_case(new_name))
    {
        return Err("destination image name already exists".into());
    }
    let records = associated_records(&image, directory, &entry)?;
    // Replace an existing long name with the requested DOS short name, keeping
    // the short entry, timestamps, attributes and all allocation chains intact.
    for &at in &records[1..] {
        image.data[at] = 0xe5;
    }
    image.data[entry.offset..entry.offset + 11].copy_from_slice(&encoded);
    // DOS NT case flags describe the old alias, not the new uppercase name.
    image.data[entry.offset + 12] &= !0x18;
    image.check_references()?;
    save_new(&image.data, output)
}

pub fn check(path: &Path) -> Result<()> {
    let image = Image::open(path)?;
    let claimed = image.check_references()?;
    let mut pending = vec![(None, 0)];
    let mut files = 0;
    let mut directories = 0;
    while let Some((directory, parent)) = pending.pop() {
        if let Some(cluster) = directory {
            let bytes = image.cluster(cluster);
            for (at, name, expected) in [(0, b".          ", cluster), (32, b"..         ", parent)]
            {
                let e = &bytes[at..at + 32];
                if &e[..11] != name
                    || e[11] != 0x10
                    || word(e, 26) != expected
                    || e[28..32] != [0; 4]
                {
                    return Err(format!("invalid dot entry in directory cluster {cluster}").into());
                }
            }
        }
        let mut names = std::collections::HashSet::new();
        for entry in image.entries(directory)? {
            if !names.insert(entry.name.to_ascii_uppercase()) {
                return Err("duplicate short names in directory".into());
            }
            if entry.directory {
                if entry.size != 0 {
                    return Err("directory has nonzero file size".into());
                }
                directories += 1;
                pending.push((Some(entry.cluster), directory.unwrap_or(0)));
            } else {
                files += 1;
            }
        }
    }
    let mut lost = 0;
    let mut bad = 0;
    for n in 2..image.layout.clusters + 2 {
        let value = image.allocation(n);
        if value == 0xff7 {
            bad += 1;
        } else if value != 0 && !claimed.contains(&n) {
            lost += 1;
        }
    }
    if lost != 0 {
        return Err(format!(
            "{lost} allocated clusters are not referenced by files or directories"
        )
        .into());
    }
    println!(
        "FAT12 allocation checks passed: {files} files, {directories} directories, {} referenced clusters, {bad} bad clusters.\nChecked layout, FAT copies, chains, cross-links, dot entries, duplicate short names and lost allocations.\nLong-name records, OEM names and boot code are not validated. No changes made.",
        claimed.len()
    );
    Ok(())
}
