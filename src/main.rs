mod backup;
mod banks;
mod convert;
mod fat12;
mod firmware;
mod gui;
mod hfe;
mod media;
mod slots;
mod usb;
#[cfg(windows)]
mod windev;

use std::{
    env, fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    process,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const HELP: &str = "rustgotek — manage mounted Gotek USB media

Commands:
  gui                              Open the desktop interface (also the default)
  gui banks [SOURCE]               Open Disk banks, optionally prefill the source
  usb-list                         List connected USB disks usable as a bank source
  usb-eject SOURCE --confirm-eject Unmount and power off a USB drive (Linux)
  bank-list SOURCE PROFILE         Inspect fixed-stride bank boot headers
  bank-create OUT PROFILE COUNT CAPACITY_BYTES|minimum
                                   Create a new formatted bank-layout image
  bank-export SOURCE PROFILE N OUT Export a raw floppy from bank N (0–999)
  bank-backup SOURCE OUT            Copy the complete device/image to a new file
  bank-verify SOURCE BACKUP         Compare every byte with a whole-device backup
  bank-replace BACKUP PROFILE N IMG OUT  Replace a bank in a NEW backup file
  bank-write TARGET PROFILE N IMG EXPECTED RECOVERY --confirm-write
                                   Write one bank, save original and verify
  device-restore TARGET IMAGE EXPECTED RECOVERY --confirm-write
                                   Restore a whole device with recovery backup
  list ROOT                        List files recursively (links rejected)
  media-info ROOT                  Report media configuration and image inventory
  import ROOT SOURCE NAME           Copy an image into the media
  slot ROOT SOURCE NUMBER|auto      Import to an indexed slot (0–999)
  slots ROOT                       Show indexed names, conflicts and next slot
  slot-move ROOT FROM TO            Reassign an image to a free slot (0–999)
  export ROOT NAME DESTINATION      Copy a file out of the media
  remove ROOT NAME                  Move a file into .rustgotek-trash
  config-show ROOT                  Print FlashFloppy FF.CFG
  config-set ROOT KEY VALUE         Set a FlashFloppy option; save old config
  init ROOT native|indexed          Create FlashFloppy FF.CFG
  backup ROOT DESTINATION           Copy entire media to a new directory
  restore BACKUP DESTINATION        Restore to a new directory
  verify ROOT BACKUP                Compare directory trees and every file byte
  firmware-inspect FILE             Validate FlashFloppy update checksums
  firmware-stage ROOT FILE MCU      Stage an update for an explicit target MCU
  create PATH SIZE_KIB              Create a zero-filled raw image (unformatted)
  create-fat12 PATH SIZE_KIB        Create a formatted PC data floppy image
  inspect-fat12 PATH                Check FAT12 layout and report free space
  inspect-hfe PATH                  Inspect HFE header and track-data extents
  convert-hfe INPUT OUTPUT PROFILE  Convert PC raw sectors to HFE v1
  convert-raw INPUT OUTPUT PROFILE  Decode standard PC HFE v1 to raw sectors
  image-check PATH                  Check FAT12 chains and allocation consistency
  image-list PATH [DIRECTORY]       List FAT12 directory using DOS short names
  image-extract PATH NAME DEST      Extract a FAT12 file without mounting
  image-import PATH SOURCE NAME OUT Add a file and save a new FAT12 image
  image-mkdir PATH NAME OUT         Create a directory in a new FAT12 image
  image-remove PATH NAME OUT        Remove a file or empty directory to new image
  image-label PATH LABEL OUT       Set the disk label in a new FAT12 image
  image-rename PATH NAME NEW OUT    Rename within its FAT12 directory to new image

ROOT must be an existing mounted directory. Bank SOURCE can be a readable block
device or a whole-device image. Profiles: gotek1440, gotek720 (explicit layout).
bank-write and device-restore require an unmounted Linux device or image file,
an expected original image, a new recovery path and --confirm-write.
create-fat12 supports 360, 720, 1200, 1440, and 2880 KiB image files.
Copies refuse overwrites. remove is recoverable through export from the trash.
Configuration values are firmware-dependent; consult your firmware manual.
";

fn root(path: &str) -> Result<PathBuf> {
    let p = fs::canonicalize(path)?;
    if !p.is_dir() {
        return Err("root must be a directory".into());
    }
    Ok(p)
}

// Resolve ASCII case as FAT does, including on a case-sensitive staging tree.
// Ambiguous names and links are rejected rather than selecting arbitrarily.
fn within(base: &Path, name: &str) -> Result<PathBuf> {
    let rel = Path::new(name);
    if rel.as_os_str().is_empty() {
        return Err("empty relative path".into());
    }
    let mut p = base.to_path_buf();
    for c in rel.components() {
        let component = match c {
            Component::Normal(s) => s,
            _ => return Err("use a relative path without . or ..".into()),
        };
        let mut matched = None;
        match fs::read_dir(&p) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    if entry.file_name().eq_ignore_ascii_case(component) {
                        if matched.is_some() {
                            return Err(format!(
                                "ambiguous FAT filename in {}: {}",
                                p.display(),
                                component.to_string_lossy()
                            )
                            .into());
                        }
                        matched = Some(entry.file_name());
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        p.push(matched.as_deref().unwrap_or(component));
        match fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("symbolic link rejected: {}", p.display()).into());
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(p)
}

fn copy_new(src: &Path, dst: &Path) -> Result<()> {
    if !fs::symlink_metadata(src)?.file_type().is_file() {
        return Err("source must be a regular file".into());
    }
    let mut input = fs::File::open(src)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    let result = (|| -> io::Result<()> {
        io::copy(&mut input, &mut output)?;
        output.sync_all()
    })();
    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(dst);
    }
    result?;
    Ok(())
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let kind = e.file_type()?;
        if kind.is_symlink() {
            return Err(format!("symbolic link rejected: {}", e.path().display()).into());
        }
        if !kind.is_file() && !kind.is_dir() {
            return Err("special file rejected".into());
        }
        out.push(e.path().strip_prefix(base)?.to_path_buf());
        if kind.is_dir() {
            walk(base, &e.path(), out)?;
        }
    }
    Ok(())
}

fn snapshot(src: &Path, dst: &Path) -> Result<()> {
    let parent = fs::canonicalize(
        dst.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let dst = parent.join(dst.file_name().ok_or("destination needs a name")?);
    if dst.starts_with(src) {
        return Err("backup destination must be outside source".into());
    }
    let mut entries = vec![];
    walk(src, src, &mut entries)?;
    fs::create_dir(&dst)?;
    for rel in entries {
        let from = src.join(&rel);
        let to = dst.join(&rel);
        if from.is_dir() {
            fs::create_dir(&to)?;
        } else {
            copy_new(&from, &to)?;
        }
    }
    backup::verify(src, &dst)?;
    println!("Copied and verified: {}", dst.display());
    Ok(())
}

fn config_path(base: &Path) -> Result<PathBuf> {
    let primary = within(base, "FF/FF.CFG")?;
    if within(base, "FF")?.is_dir() {
        Ok(primary)
    } else {
        within(base, "FF.CFG")
    }
}

fn set_config(base: &Path, key: &str, value: &str) -> Result<()> {
    if key.eq_ignore_ascii_case("indexed-prefix") {
        slots::parse_prefix(value)?;
    }
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("invalid configuration key".into());
    }
    if value.is_empty() || value.contains(['\r', '\n', '#', ';']) {
        return Err("value must be a single nonempty value without comments".into());
    }
    let path = config_path(base)?;
    let old = fs::read_to_string(&path)?;
    let mut found = false;
    let mut lines = Vec::new();
    for line in old.lines() {
        if line
            .split_once('=')
            .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        {
            if !found {
                lines.push(format!("{key} = {value}"));
                found = true;
            }
        } else {
            lines.push(line.to_owned());
        }
    }
    if !found {
        lines.push(format!("{key} = {value}"));
    }
    let backup = unique(&path, "bak")?;
    copy_new(&path, &backup)?;
    let temp = unique(&path, "tmp")?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all((lines.join("\n") + "\n").as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)?;
    println!("Previous configuration: {}", backup.display());
    Ok(())
}

fn unique(path: &Path, suffix: &str) -> Result<PathBuf> {
    for n in 0..10000 {
        let mut name = path.as_os_str().to_os_string();
        name.push(format!(".{suffix}.{n}"));
        let p = PathBuf::from(name);
        match fs::symlink_metadata(&p) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(p),
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
    }
    Err("too many backup files".into())
}

fn image_extension(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "img", "ima", "adf", "hfe", "dsk", "st", "v9t9", "trd", "ssd", "dsd",
    ]
    .contains(&ext.as_str())
    {
        return Err("unrecognized disk-image extension".into());
    }
    Ok(ext)
}

fn run(a: &[String]) -> Result<()> {
    match a.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["help"] | ["--help"] | ["-h"] => print!("{HELP}"),
        ["--version"] => println!("rustgotek {}", env!("CARGO_PKG_VERSION")),
        ["usb-eject", source, "--confirm-eject"] => {
            let plan = usb::prepare(Path::new(source))?;
            usb::power_off(&plan)?;
            println!("USB powered off. Safe to unplug.");
        }
        ["usb-list"] => {
            let found = usb::detect();
            if found.is_empty() {
                println!("No USB disks detected.");
            }
            for (_, label) in found {
                println!("{label}");
            }
        }
        ["bank-list", source, profile] => banks::list(Path::new(source), profile)?,
        ["bank-create", dest, profile, count, capacity] => {
            banks::create_layout(Path::new(dest), profile, count.parse()?, capacity)?
        }
        [
            "device-restore",
            target,
            image,
            expected,
            recovery,
            "--confirm-write",
        ] => banks::restore_device(
            Path::new(target),
            Path::new(image),
            Path::new(expected),
            Path::new(recovery),
        )?,
        [
            "bank-write",
            target,
            profile,
            bank,
            image,
            expected,
            recovery,
            "--confirm-write",
        ] => banks::write_bank(
            Path::new(target),
            profile,
            bank.parse()?,
            Path::new(image),
            Path::new(expected),
            Path::new(recovery),
        )?,
        ["bank-export", source, profile, bank, out] => {
            banks::export(Path::new(source), profile, bank.parse()?, Path::new(out))?
        }
        ["bank-backup", source, out] => banks::backup(Path::new(source), Path::new(out))?,
        ["bank-verify", source, backup] => banks::verify(Path::new(source), Path::new(backup))?,
        ["bank-replace", source, profile, bank, image, out] => banks::replace(
            Path::new(source),
            profile,
            bank.parse()?,
            Path::new(image),
            Path::new(out),
        )?,
        ["media-info", r] => media::info(&root(r)?)?,
        ["list", r] => {
            let base = root(r)?;
            let mut entries = vec![];
            walk(&base, &base, &mut entries)?;
            for rel in entries {
                let m = fs::metadata(base.join(&rel))?;
                println!(
                    "{:>10} {}{}",
                    m.len(),
                    rel.display(),
                    if m.is_dir() { "/" } else { "" }
                );
            }
        }
        ["import", r, source, name] => {
            image_extension(Path::new(source))?;
            image_extension(Path::new(name))?;
            copy_new(Path::new(source), &within(&root(r)?, name)?)?;
        }
        ["slot", r, source, number] => {
            let base = root(r)?;
            let configured_prefix = slots::prefix(&base)?;
            let occupied = slots::scan(&base, &configured_prefix)?;
            let n: u16 = if *number == "auto" {
                (0..=999)
                    .find(|n| !occupied.contains_key(n))
                    .ok_or("no free slots in range 0–999")?
            } else {
                number.parse()?
            };
            if n > 999 {
                return Err("slot must be 0–999".into());
            }
            let prefix = format!("{configured_prefix}{n:04}");
            if occupied.contains_key(&n) {
                return Err("slot already occupied".into());
            }
            let ext = image_extension(Path::new(source))?;
            copy_new(
                Path::new(source),
                &within(&base, &format!("{prefix}.{ext}"))?,
            )?;
            println!("Assigned slot {n:04}: {prefix}.{ext}");
        }
        ["slots", r] => slots::list(&root(r)?)?,
        ["slot-move", r, from, to] => slots::move_slot(&root(r)?, from.parse()?, to.parse()?)?,
        ["export", r, name, destination] => {
            copy_new(&within(&root(r)?, name)?, Path::new(destination))?
        }
        ["remove", r, name] => {
            let base = root(r)?;
            let source = within(&base, name)?;
            if !fs::metadata(&source)?.is_file() {
                return Err("only regular files can be removed".into());
            }
            let trash = within(&base, ".rustgotek-trash")?;
            fs::create_dir_all(&trash)?;
            let dst = unique(
                &trash.join(source.file_name().ok_or("missing filename")?),
                "deleted",
            )?;
            copy_new(&source, &dst)?;
            fs::remove_file(source)?;
            println!("Recoverable copy: {}", dst.display());
        }
        ["init", r, mode] if ["native", "indexed"].contains(mode) => {
            let base = root(r)?;
            if config_path(&base)?.exists() {
                return Err("FF.CFG already exists".into());
            }
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(config_path(&base)?)?;
            writeln!(f, "# Created by rustgotek\nnav-mode = {mode}")?;
            f.sync_all()?;
        }
        ["config-show", r] => print!("{}", fs::read_to_string(config_path(&root(r)?)?)?),
        ["config-set", r, key, value] => set_config(&root(r)?, key, value)?,
        ["backup" | "restore", r, dest] => snapshot(&root(r)?, Path::new(dest))?,
        ["verify", r, dest] => backup::verify(&root(r)?, &root(dest)?)?,
        ["create-fat12", path, size] => {
            fat12::create(Path::new(path), size.parse()?)?;
            println!("Created FAT12 data floppy image (no operating system).");
        }
        ["inspect-fat12", path] => fat12::inspect_file(Path::new(path))?,
        ["inspect-hfe", path] => hfe::inspect(Path::new(path))?,
        ["convert-raw", input, output, profile] => {
            convert::to_raw(Path::new(input), Path::new(output), profile)?
        }
        ["convert-hfe", input, output, profile] => {
            convert::to_hfe(Path::new(input), Path::new(output), profile)?
        }
        ["firmware-inspect", path] => firmware::inspect(Path::new(path))?,
        ["firmware-stage", r, path, model] => firmware::stage(&root(r)?, Path::new(path), model)?,
        ["image-check", path] => fat12::check(Path::new(path))?,
        ["image-label", path, label, output] => {
            fat12::set_label(Path::new(path), label, Path::new(output))?
        }
        ["image-rename", path, name, new_name, output] => {
            fat12::rename(Path::new(path), name, new_name, Path::new(output))?
        }
        ["image-remove", path, name, output] => {
            fat12::remove(Path::new(path), name, Path::new(output))?
        }
        ["image-mkdir", path, name, output] => {
            fat12::mkdir(Path::new(path), name, Path::new(output))?
        }
        ["image-import", path, source, name, output] => {
            fat12::import(Path::new(path), Path::new(source), name, Path::new(output))?
        }
        ["image-list", path] => fat12::list(Path::new(path), "/")?,
        ["image-list", path, directory] => fat12::list(Path::new(path), directory)?,
        ["image-extract", path, name, destination] => {
            fat12::extract(Path::new(path), name, Path::new(destination))?
        }
        ["create", path, size] => {
            let kib: u64 = size.parse()?;
            if kib == 0 || kib > 32768 {
                return Err("size must be 1–32768 KiB".into());
            }
            let f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            f.set_len(kib * 1024)?;
            f.sync_all()?;
            println!(
                "Created unformatted raw image; format it with your target machine before use."
            );
        }
        _ => return Err("invalid command or arguments; run rustgotek --help".into()),
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let bank_gui = (2..=3).contains(&args.len()) && args[0] == "gui" && args[1] == "banks";
    if args.is_empty() || args == ["gui"] || bank_gui {
        let source = bank_gui.then(|| args.get(2).cloned().unwrap_or_default());
        if let Err(error) = gui::launch(source) {
            eprintln!("rustgotek: {error}");
            process::exit(1);
        }
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("rustgotek: {e}");
        process::exit(1);
    }
}
