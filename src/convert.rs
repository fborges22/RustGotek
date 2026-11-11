use crate::Result;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

struct Mfm {
    bytes: Vec<u8>,
    previous: u8,
}
impl Mfm {
    fn byte(&mut self, value: u8) {
        let mut word = 0u16;
        for shift in (0..8).rev() {
            let bit = (value >> shift) & 1;
            let clock = u8::from(self.previous == 0 && bit == 0);
            word = (word << 2) | ((clock as u16) << 1) | bit as u16;
            self.previous = bit;
        }
        self.bytes.extend_from_slice(&word.to_be_bytes());
    }
    fn repeat(&mut self, value: u8, count: usize) {
        for _ in 0..count {
            self.byte(value);
        }
    }
    fn sync(&mut self, word: u16) {
        for _ in 0..3 {
            self.bytes.extend_from_slice(&word.to_be_bytes());
        }
        self.previous = (word & 1) as u8;
    }
    fn field(&mut self, mark: u8, data: &[u8]) {
        self.repeat(0, 12);
        self.sync(0x4489);
        let mut crc = 0xffffu16;
        for b in [0xa1, 0xa1, 0xa1, mark].iter().chain(data) {
            crc ^= (*b as u16) << 8;
            for _ in 0..8 {
                crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
            }
        }
        self.byte(mark);
        for &b in data {
            self.byte(b);
        }
        for b in crc.to_be_bytes() {
            self.byte(b);
        }
    }
}

fn geometry(profile: &str) -> Result<(usize, usize, usize, usize)> {
    Ok(match profile {
        "pc360" => (40, 9, 250, 300),
        "pc720" => (80, 9, 250, 300),
        "pc1200" => (80, 15, 500, 360),
        "pc1440" => (80, 18, 500, 300),
        _ => return Err("profile must be pc360, pc720, pc1200 or pc1440".into()),
    })
}

pub fn to_hfe(input: &Path, output: &Path, profile: &str) -> Result<()> {
    let (cylinders, sectors, bitrate, rpm) = geometry(profile)?;
    let expected = cylinders * 2 * sectors * 512;
    if !fs::symlink_metadata(input)?.is_file() {
        return Err("input must be a regular raw-sector image".into());
    }
    let mut raw = vec![];
    fs::File::open(input)?
        .take(expected as u64 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() != expected {
        return Err(format!(
            "{profile} requires exactly {expected} raw bytes in cylinder/head/sector order"
        )
        .into());
    }
    let track_bytes = (bitrate * 1000 * 60 / rpm / 8) * 2;
    let mut hfe = vec![0xff; 1024];
    hfe[..8].copy_from_slice(b"HXCPICFE");
    hfe[8] = 0;
    hfe[9] = cylinders as u8;
    hfe[10] = 2;
    hfe[11] = 0;
    hfe[12..14].copy_from_slice(&(bitrate as u16).to_le_bytes());
    hfe[14..16].copy_from_slice(&(rpm as u16).to_le_bytes());
    hfe[16] = u8::from(bitrate == 500);
    hfe[18..20].copy_from_slice(&1u16.to_le_bytes());
    for cylinder in 0..cylinders {
        let mut sides = vec![];
        for head in 0..2 {
            let mut mfm = Mfm {
                bytes: vec![],
                previous: 0,
            };
            mfm.repeat(0x4e, 80);
            mfm.repeat(0, 12);
            mfm.sync(0x5224);
            mfm.byte(0xfc);
            mfm.repeat(0x4e, 50);
            for sector in 0..sectors {
                mfm.field(0xfe, &[cylinder as u8, head as u8, sector as u8 + 1, 2]);
                mfm.repeat(0x4e, 22);
                let offset = ((cylinder * 2 + head) * sectors + sector) * 512;
                mfm.field(0xfb, &raw[offset..offset + 512]);
                mfm.repeat(0x4e, 84);
            }
            if mfm.bytes.len() > track_bytes {
                return Err("encoded sectors exceed track capacity".into());
            }
            mfm.repeat(0x4e, (track_bytes - mfm.bytes.len()) / 2);
            for b in &mut mfm.bytes {
                *b = b.reverse_bits();
            }
            sides.push(mfm.bytes);
        }
        let lut = 512 + cylinder * 4;
        let block = (hfe.len() / 512) as u16;
        hfe[lut..lut + 2].copy_from_slice(&block.to_le_bytes());
        hfe[lut + 2..lut + 4].copy_from_slice(&((track_bytes * 2) as u16).to_le_bytes());
        for chunk in 0..track_bytes.div_ceil(256) {
            for side in &sides {
                let start = chunk * 256;
                let end = (start + 256).min(track_bytes);
                hfe.extend_from_slice(&side[start..end]);
                hfe.resize(hfe.len() + 256 - (end - start), 0xff);
            }
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = file.write_all(&hfe).and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(output);
    }
    result?;
    println!(
        "Converted {profile}: {cylinders} cylinders, 2 sides, {sectors} sectors/track to HFE v1. Raw sector contents preserved; original track timing and copy-protection metadata cannot be recovered from a raw image."
    );
    Ok(())
}

fn bit(track: &[u8], at: usize) -> u8 {
    let at = at % (track.len() * 8);
    (track[at / 8] >> (7 - at % 8)) & 1
}

fn decode_field(track: &[u8], at: usize, count: usize) -> Result<Vec<u8>> {
    if 48 + count * 16 > track.len() * 8 {
        return Err("field is longer than track".into());
    }
    let mut bytes = vec![];
    for n in 0..count {
        let mut byte = 0;
        for i in 0..8 {
            byte = (byte << 1) | bit(track, at + 48 + n * 16 + i * 2 + 1);
        }
        bytes.push(byte);
    }
    let mut crc = 0xffffu16;
    for &b in [0xa1, 0xa1, 0xa1].iter().chain(&bytes) {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
        }
    }
    if crc != 0 {
        return Err("sector ID/data CRC mismatch".into());
    }
    Ok(bytes)
}

fn decode_track(track: &[u8], cylinder: usize, head: usize, sectors: usize) -> Result<Vec<u8>> {
    if track.is_empty() {
        return Err("empty track".into());
    }
    let bits = track.len() * 8;
    let mut syncs = vec![];
    let mut window = 0u64;
    for i in 0..bits + 47 {
        window = ((window << 1) | bit(track, i) as u64) & 0xffffffffffff;
        if i >= 47 && window == 0x448944894489 {
            syncs.push(i - 47);
        }
    }
    let mut raw = vec![0; sectors * 512];
    let mut found = vec![false; sectors];
    if syncs.len() != sectors * 2 {
        return Err(format!(
            "expected {} sector ID/data marks, found {}",
            sectors * 2,
            syncs.len()
        )
        .into());
    }
    for (index, &at) in syncs.iter().enumerate() {
        let mut mark = 0;
        for i in 0..8 {
            mark = (mark << 1) | bit(track, at + 48 + i * 2 + 1);
        }
        if mark == 0xfb {
            continue;
        }
        if mark != 0xfe {
            return Err("unsupported or deleted sector address mark".into());
        }
        let id = decode_field(track, at, 7)?;
        let sector = id[3] as usize;
        if id[1] as usize != cylinder
            || id[2] as usize != head
            || id[4] != 2
            || sector == 0
            || sector > sectors
        {
            return Err("sector ID does not match requested PC geometry".into());
        }
        if found[sector - 1] {
            return Err("duplicate sector ID".into());
        }
        let next = syncs[(index + 1) % syncs.len()];
        let data = decode_field(track, next, 515)?;
        if data[0] != 0xfb {
            return Err("sector ID has no following normal data field".into());
        }
        let following = syncs[(index + 2) % syncs.len()];
        if (next + bits - at) % bits < 48 + 7 * 16
            || (following + bits - next) % bits < 48 + 515 * 16
        {
            return Err("overlapping sector fields".into());
        }
        raw[(sector - 1) * 512..sector * 512].copy_from_slice(&data[1..513]);
        found[sector - 1] = true;
    }
    if found.contains(&false) {
        return Err("missing sector data".into());
    }
    Ok(raw)
}

pub fn to_raw(input: &Path, output: &Path, profile: &str) -> Result<()> {
    let (cylinders, sectors, _, _) = geometry(profile)?;
    crate::hfe::inspect(input)?;
    let mut hfe = vec![];
    fs::File::open(input)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut hfe)?;
    if hfe.len() > 32 * 1024 * 1024 || hfe.len() < 512 {
        return Err("unsupported HFE size".into());
    }
    if &hfe[..8] != b"HXCPICFE" || hfe[8] != 0 || ![0, 255].contains(&hfe[11]) {
        return Err("raw conversion requires HFE v1 with IBM MFM or unspecified encoding".into());
    }
    if hfe[9] as usize != cylinders || hfe[10] != 2 {
        return Err("HFE geometry differs from selected profile".into());
    }
    let lut = u16::from_le_bytes([hfe[18], hfe[19]]) as usize * 512;
    let mut raw = vec![];
    for cylinder in 0..cylinders {
        let entry = hfe
            .get(lut + cylinder * 4..lut + cylinder * 4 + 4)
            .ok_or("truncated HFE table")?;
        let start = u16::from_le_bytes([entry[0], entry[1]]) as usize * 512;
        let length = u16::from_le_bytes([entry[2], entry[3]]) as usize / 2;
        for head in 0..2 {
            let mut track = Vec::with_capacity(length);
            for n in 0..length {
                let at = start + (n / 256) * 512 + head * 256 + n % 256;
                track.push(hfe.get(at).ok_or("truncated HFE track")?.reverse_bits());
            }
            let data = decode_track(&track, cylinder, head, sectors)
                .map_err(|e| format!("cylinder {cylinder}, head {head}: {e}"))?;
            raw.extend_from_slice(&data);
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = file.write_all(&raw).and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(output);
    }
    result?;
    println!(
        "Decoded {profile}: all {} sectors passed ID and data CRC checks. Raw output retains sector data only.",
        cylinders * 2 * sectors
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(ids: &[u8]) -> Vec<u8> {
        let mut mfm = Mfm {
            bytes: vec![],
            previous: 0,
        };
        for &id in ids {
            mfm.repeat(0x4e, 50);
            mfm.field(0xfe, &[0, 0, id, 2]);
            mfm.repeat(0x4e, 22);
            mfm.field(0xfb, &vec![id; 512]);
            mfm.repeat(0x4e, 84);
        }
        mfm.bytes
    }
    #[test]
    fn circular_and_unaligned_tracks_decode() {
        let original = track(&[1, 2]);
        for shift in [0, 1, 17, 149, original.len() * 8 - 50] {
            let mut rotated = vec![0; original.len()];
            for (n, byte) in rotated.iter_mut().enumerate() {
                for i in 0..8 {
                    *byte = (*byte << 1) | bit(&original, n * 8 + i + shift);
                }
            }
            assert_eq!(
                decode_track(&rotated, 0, 0, 2).unwrap(),
                [vec![1; 512], vec![2; 512]].concat()
            );
        }
    }
    #[test]
    fn damaged_and_duplicate_sectors_are_rejected() {
        assert!(decode_track(&track(&[1, 1]), 0, 0, 2).is_err());
        assert!(decode_track(&track(&[1]), 0, 0, 2).is_err());
        let mut corrupt = track(&[1]);
        // Within the data field, flip a data bit without updating the CRC.
        corrupt[300] ^= 1;
        assert!(decode_track(&corrupt, 0, 0, 1).is_err());
        assert!(decode_track(&track(&[1]), 1, 0, 1).is_err());
    }
}
