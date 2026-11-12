use crate::{Result, config_path, image_extension, slots, walk, within};
use std::{fs, io::Read, path::Path};

pub fn info(base: &Path) -> Result<()> {
    let mut entries = vec![];
    walk(base, base, &mut entries)?;
    let mut images = 0;
    let mut image_bytes = 0u64;
    let mut updates = 0;
    for entry in &entries {
        let metadata = fs::metadata(base.join(entry))?;
        if metadata.is_file() && image_extension(entry).is_ok() {
            images += 1;
            image_bytes += metadata.len();
        }
        if entry.components().count() == 1
            && entry
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("upd"))
        {
            updates += 1;
        }
    }
    println!("Media directory: {}", base.display());
    println!(
        "Recognized image filenames: {images} ({image_bytes} bytes, including hidden/trash directories)"
    );
    println!("Root .upd entries: {updates}");
    let active = config_path(base)?;
    match fs::symlink_metadata(&active) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err("active FF.CFG path is not a regular file".into());
            }
            println!(
                "Active FlashFloppy configuration path: {}",
                active.strip_prefix(base)?.display()
            );
            let mut bytes = vec![];
            fs::File::open(&active)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err("FF.CFG exceeds 1 MiB diagnostic limit".into());
            }
            let config = std::str::from_utf8(&bytes)?;
            for key in [
                "nav-mode",
                "interface",
                "host",
                "indexed-prefix",
                "display-type",
            ] {
                let values: Vec<_> = config
                    .lines()
                    .filter_map(|line| {
                        let (name, value) = line.split_once('=')?;
                        name.trim()
                            .eq_ignore_ascii_case(key)
                            .then_some(value.trim())
                    })
                    .collect();
                if values.is_empty() {
                    println!("  {key}: not specified in file");
                }
                for value in &values {
                    println!("  {key}: {}", value.escape_default());
                }
                if values.len() > 1 {
                    println!("  WARNING: {key} appears more than once");
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Active FlashFloppy configuration: absent")
        }
        Err(e) => return Err(e.into()),
    }
    if active.parent() != Some(base) && within(base, "FF.CFG")?.exists() {
        println!("NOTE: root FF.CFG is ignored because the FF directory exists.");
    }
    for name in ["HXCSDFE.CFG", "AUTOBOOT.HFE", "IMG.CFG", "FF/IMG.CFG"] {
        let path = within(base, name)?;
        if path.is_file() {
            println!("Present: {}", path.strip_prefix(base)?.display());
        }
    }
    let prefix = slots::prefix(base)?;
    let occupied = slots::scan(base, &prefix)?;
    println!("Indexed filename prefix: {prefix:?}");
    println!(
        "Occupied indexed names: {} slots; {} conflicts",
        occupied.len(),
        occupied.values().filter(|names| names.len() > 1).count()
    );
    println!(
        "This report describes files only. It does not identify installed firmware, MCU, bootloader, jumpers, or settings saved in the Gotek's flash memory. Image contents and filesystem compatibility have not been checked."
    );
    Ok(())
}
