use crate::{Result, config_path};
use std::{collections::BTreeMap, fs, io, path::Path};

pub fn prefix(base: &Path) -> Result<String> {
    let text = match fs::read_to_string(config_path(base)?) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok("DSKA".into()),
        Err(e) => return Err(e.into()),
    };
    let mut result = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with(['#', ';']) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("indexed-prefix") {
            continue;
        }
        if result.is_some() {
            return Err(
                "duplicate indexed-prefix settings; resolve them before assigning slots".into(),
            );
        }
        result = Some(parse_prefix(value)?);
    }
    Ok(result.unwrap_or_else(|| "DSKA".into()))
}

pub fn parse_prefix(value: &str) -> Result<String> {
    let value = value.trim();
    let tail = value
        .strip_prefix('"')
        .ok_or("indexed-prefix must be quoted")?;
    let (prefix, rest) = tail.split_once('"').ok_or("unterminated indexed-prefix")?;
    let rest = rest.trim();
    if !rest.is_empty() && !rest.starts_with(['#', ';']) {
        return Err("unexpected text after indexed-prefix".into());
    }
    if prefix.len() > 7
        || !prefix.is_ascii()
        || prefix
            .bytes()
            .any(|b| b < 32 || b == 127 || b"<>:\"/\\|?*".contains(&b))
    {
        return Err("indexed-prefix must be at most seven ASCII filename characters".into());
    }
    if prefix.starts_with('.') {
        return Err("indexed-prefix cannot start with a dot (hidden from navigation)".into());
    }
    Ok(prefix.to_owned())
}

// Reserve any matching name, including directories and unknown extensions, to
// avoid ambiguous slot assignments. Never follow links while scanning.
pub fn scan(base: &Path, prefix: &str) -> Result<BTreeMap<u16, Vec<String>>> {
    let mut slots: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    let prefix = prefix.to_ascii_uppercase();
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let upper = name.to_ascii_uppercase();
        let Some(tail) = upper.strip_prefix(&prefix) else {
            continue;
        };
        let digits = tail.as_bytes().get(..4).unwrap_or_default();
        if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
            continue;
        }
        let n: u16 = std::str::from_utf8(digits)?.parse()?;
        slots.entry(n).or_default().push(name);
    }
    for names in slots.values_mut() {
        names.sort();
    }
    Ok(slots)
}

pub fn list(base: &Path) -> Result<()> {
    let prefix = prefix(base)?;
    let slots = scan(base, &prefix)?;
    println!("Indexed prefix: {prefix:?}");
    for (n, names) in &slots {
        for name in names {
            println!(
                "{n:04} {}{}",
                name.escape_default(),
                if names.len() > 1 { " [CONFLICT]" } else { "" }
            );
        }
    }
    match (0..=999).find(|n| !slots.contains_key(n)) {
        Some(n) => println!("Next available slot (0–999): {n:04}"),
        None => println!("No free slots in range 0–999"),
    }
    if slots.values().any(|names| names.len() > 1) {
        return Err("duplicate slot assignments found".into());
    }
    Ok(())
}

pub fn move_slot(base: &Path, from: u16, to: u16) -> Result<()> {
    if from > 999 || to > 999 {
        return Err("slot must be 0–999".into());
    }
    if from == to {
        return Err("source and destination slots must differ".into());
    }
    let prefix = prefix(base)?;
    let occupied = scan(base, &prefix)?;
    let names = occupied.get(&from).ok_or("source slot is empty")?;
    if names.len() != 1 {
        return Err("source slot has conflicting assignments".into());
    }
    if occupied.contains_key(&to) {
        return Err("destination slot is occupied".into());
    }
    let source = crate::within(base, &names[0])?;
    crate::image_extension(&source)?;
    let suffix = names[0]
        .get(prefix.len() + 4..)
        .ok_or("invalid indexed filename")?;
    let name = format!("{prefix}{to:04}{suffix}");
    let destination = crate::within(base, &name)?;
    crate::copy_new(&source, &destination)?;
    fs::set_permissions(&destination, fs::metadata(&source)?.permissions())?;
    if !crate::backup::equal_files(&source, &destination)? {
        return Err(
            "slot copy verification failed; source retained, inspect destination before retrying"
                .into(),
        );
    }
    if let Err(error) = fs::remove_file(&source) {
        return Err(format!(
            "new slot verified, but old slot could not be removed: {error}; both copies retained"
        )
        .into());
    }
    println!("Moved slot {from:04} to {to:04}: {}", name.escape_default());
    Ok(())
}
