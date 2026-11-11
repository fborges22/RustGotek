use crate::Result;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn word(data: &[u8], at: usize) -> u64 {
    u16::from_le_bytes([data[at], data[at + 1]]) as u64
}

pub fn inspect(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err("image must be a regular file".into());
    }
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut header = [0; 512];
    file.read_exact(&mut header)?;
    let version = match (&header[..8], header[8]) {
        (b"HXCPICFE", 0) => 1,
        (b"HXCPICFE", 1) => 2,
        (b"HXCHFEV3", 0) => 3,
        (b"HXCPICFE" | b"HXCHFEV3", _) => return Err("unsupported HFE revision".into()),
        _ => return Err("unrecognized HFE signature".into()),
    };
    let tracks = header[9] as usize;
    let sides = header[10];
    let bitrate = word(&header, 12);
    if tracks == 0 || !(1..=2).contains(&sides) || bitrate == 0 {
        return Err("invalid HFE track count, side count or bitrate".into());
    }
    let lut = word(&header, 18) * 512;
    let lut_end = lut + (tracks as u64 * 4).div_ceil(512) * 512;
    if lut < 512 || lut_end > size {
        return Err("HFE track table is outside image or overlaps header".into());
    }
    file.seek(SeekFrom::Start(lut))?;
    let mut table = vec![0; tracks * 4];
    file.read_exact(&mut table)?;
    let mut ranges = vec![];
    let mut minimum = u64::MAX;
    let mut maximum = 0;
    for n in 0..tracks {
        let start = word(&table, n * 4) * 512;
        let length = word(&table, n * 4 + 2);
        if length == 0 || !length.is_multiple_of(2) {
            return Err(format!(
                "track {n}: length must be nonzero and even for interleaved sides"
            )
            .into());
        }
        let end = start + length.div_ceil(512) * 512;
        if start < 512 || end > size {
            return Err(format!("track {n}: data outside image or overlaps header").into());
        }
        if start < lut_end && end > lut {
            return Err(format!("track {n}: data overlaps track table").into());
        }
        ranges.push((start, end, n));
        minimum = minimum.min(length / 2);
        maximum = maximum.max(length / 2);
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(format!(
                "track data overlaps: tracks {} and {}",
                pair[0].2, pair[1].2
            )
            .into());
        }
    }
    let encoding = match header[11] {
        0 => "ISO IBM MFM",
        1 => "Amiga MFM",
        2 => "ISO IBM FM",
        3 => "Emu FM",
        6 | 7 => "Apple II GCR",
        255 => "unspecified",
        _ => "unknown encoding ID",
    };
    let write = match header[20] {
        0 => "protected",
        255 => "allowed",
        _ => "unknown flag",
    };
    println!(
        "HFE v{version}: {size} bytes\nCylinders: {tracks}\nSides: {sides}\nHeader bitrate: {bitrate} kbit/s\nHeader RPM: {}\nEncoding: {encoding} (0x{:02x})\nInterface ID: 0x{:02x}\nWrite flag: {write}\nTrack bytes per side: {minimum}–{maximum}\nHeader and track extents checked. Bitstream opcodes, sector CRCs and host compatibility are not validated.",
        word(&header, 14),
        header[11],
        header[16]
    );
    Ok(())
}
