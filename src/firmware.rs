use crate::Result;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &b in bytes {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
        }
    }
    crc
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffffffffu32;
    for &b in bytes {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x80000000 != 0 { 0x04c11db7 } else { 0 };
        }
    }
    crc
}

fn dword(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

struct Update {
    universal: bool,
    models: Vec<(u8, usize)>,
}

fn payload(data: &[u8]) -> Result<()> {
    if data.len() < 8
        || !data.len().is_multiple_of(4)
        || &data[data.len() - 4..data.len() - 2] != b"FY"
    {
        return Err("invalid firmware payload size or FY footer".into());
    }
    if crc16(data) != 0 {
        return Err("firmware payload CRC16 mismatch".into());
    }
    Ok(())
}

fn parse(data: &[u8]) -> Result<Update> {
    if !data.starts_with(b"FFUP") {
        payload(data)?;
        return Ok(Update {
            universal: false,
            models: vec![(1, data.len() - 4)],
        });
    }
    if data.len() < 36 || !data.ends_with(b"FFUP") {
        return Err("invalid universal update header/footer".into());
    }
    if crc32(&data[..data.len() - 4]) != 0 {
        return Err("update file CRC32 mismatch".into());
    }
    let offset = dword(data, 4);
    let count = dword(data, 8);
    if offset != 12 || count == 0 || count > 255 {
        return Err("invalid update catalogue offset or count".into());
    }
    let header_end = 12 + count * 12 + 4;
    let payload_end = data.len() - 8;
    if header_end > payload_end || &data[header_end - 4..header_end - 2] != b"FZ" {
        return Err("invalid update catalogue footer or bounds".into());
    }
    if crc16(&data[..header_end]) != 0 {
        return Err("update catalogue CRC16 mismatch".into());
    }
    let mut models = vec![];
    let mut ranges = vec![];
    for i in 0..count {
        let at = 12 + i * 12;
        let model = data[at];
        if data[at + 1..at + 4] != [0; 3] {
            return Err("nonzero reserved catalogue bytes".into());
        }
        if models.iter().any(|&(m, _)| m == model) {
            return Err("duplicate firmware model".into());
        }
        let start = dword(data, at + 4);
        let len = dword(data, at + 8);
        let end = start
            .checked_add(len)
            .ok_or("firmware payload range overflow")?;
        if start < header_end || end > payload_end {
            return Err("firmware payload outside data region".into());
        }
        payload(&data[start..end])?;
        ranges.push((start, end));
        models.push((model, len - 4));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err("overlapping firmware payloads".into());
    }
    Ok(Update {
        universal: true,
        models,
    })
}

fn read(path: &Path) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err("update must be a regular file".into());
    }
    let mut data = vec![];
    fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 4 * 1024 * 1024 {
        return Err("update exceeds 4 MiB limit".into());
    }
    Ok(data)
}

pub fn inspect(path: &Path) -> Result<()> {
    let update = parse(&read(path)?)?;
    println!(
        "FlashFloppy {} update; checksums valid",
        if update.universal {
            "universal"
        } else {
            "legacy"
        }
    );
    for (model, bytes) in update.models {
        let name = match model {
            1 => "STM32F105 / AT32F415",
            4 => "AT32F435",
            _ => "unknown MCU",
        };
        println!("Model {model}: {name}, {bytes} payload bytes");
    }
    println!(
        "Checksums detect corruption, not authenticity. Firmware version and application type are not encoded in this container."
    );
    Ok(())
}

pub fn stage(root: &Path, source: &Path, model: &str) -> Result<()> {
    let target = match model {
        "stm32f105" | "at32f415" => 1,
        "at32f435" => 4,
        _ => return Err("target must be stm32f105, at32f415 or at32f435".into()),
    };
    let data = read(source)?;
    let update = parse(&data)?;
    if !update.models.iter().any(|&(m, _)| m == target) {
        return Err("update contains no payload for target MCU".into());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("update filename must be UTF-8")?;
    let lower = name.to_ascii_lowercase();
    let prefix = if update.universal {
        "flashfloppy-"
    } else {
        "ff_gotek-"
    };
    if !lower.starts_with(prefix) || !lower.ends_with(".upd") {
        return Err(
            "update filename does not match FlashFloppy bootloader naming convention".into(),
        );
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("upd"))
        {
            return Err(
                "USB root already contains an .upd entry; archive it before staging".into(),
            );
        }
    }
    let destination = root.join(name);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    let result = output.write_all(&data).and_then(|()| output.sync_all());
    drop(output);
    if let Err(error) = result {
        let _ = fs::remove_file(&destination);
        return Err(error.into());
    }
    if read(&destination)? != data {
        return Err("staged update readback differs from source".into());
    }
    println!("Staged and verified {} for {model}.", destination.display());
    if update.universal && target == 1 {
        println!(
            "Universal updates require a compatible recent FlashFloppy bootloader on this MCU."
        );
    }
    println!(
        "No device was flashed. Use a trusted release intended for your Gotek; this container does not identify bootloader or alternative firmware builds. See the README for the hardware update procedure."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_crc_check_values() {
        assert_eq!(crc16(b"123456789"), 0x29b1);
        assert_eq!(crc32(b"123456789"), 0x0376e6e7);
    }

    fn fixture() -> Vec<u8> {
        let mut payload = vec![0; 16];
        payload.extend_from_slice(b"FY");
        payload.extend_from_slice(&crc16(&payload).to_be_bytes());
        let mut data = b"FFUP".to_vec();
        data.extend_from_slice(&12u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&[4, 0, 0, 0]);
        data.extend_from_slice(&28u32.to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(b"FZ");
        data.extend_from_slice(&crc16(&data).to_be_bytes());
        data.extend_from_slice(&payload);
        data.extend_from_slice(&crc32(&data).to_be_bytes());
        data.extend_from_slice(b"FFUP");
        data
    }

    fn fix_crcs(data: &mut [u8]) {
        let h = crc16(&data[..26]);
        data[26..28].copy_from_slice(&h.to_be_bytes());
        let end = data.len() - 8;
        let crc = crc32(&data[..end]);
        data[end..end + 4].copy_from_slice(&crc.to_be_bytes());
    }

    #[test]
    fn universal_validation_checks_structure_and_checksums() {
        let data = fixture();
        assert_eq!(parse(&data).unwrap().models, vec![(4, 16)]);
        for end in 0..data.len() {
            assert!(parse(&data[..end]).is_err());
        }
        for at in 0..data.len() {
            let mut bad = data.clone();
            bad[at] ^= 1;
            assert!(parse(&bad).is_err(), "mutation at {at}");
        }
        for (at, value) in [
            (4, 0),
            (8, 0),
            (13, 1),
            (16, 0),
            (16, 255),
            (20, 255),
            (24, 0),
        ] {
            let mut bad = data.clone();
            bad[at] = value;
            fix_crcs(&mut bad);
            assert!(parse(&bad).is_err(), "invalid field {at}");
        }
        let legacy = &data[28..48];
        assert!(!parse(legacy).unwrap().universal);
    }
}
