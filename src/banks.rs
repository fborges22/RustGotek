//! Original-firmware fixed-stride banks. Profiles are explicit, never guessed
//! from the first FAT volume. Writes require an explicit confirmation, an expected original and recovery file.
use crate::Result;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

#[derive(Clone, Copy)]
struct Profile {
    stride: u64,
    payload: u64,
}
impl Profile {
    fn parse(name: &str) -> Result<Self> {
        // Layout facts: libdsk's drvgotek.c; sebmate/GotekTool's loop offsets.
        match name {
            "gotek1440" => Ok(Self { stride: 0x180000, payload: 1440 * 1024 }),
            "gotek720" => Ok(Self { stride: 0x100000, payload: 720 * 1024 }),
            _ => Err("bank profile must be gotek1440 or gotek720; confirm your device layout before choosing".into()),
        }
    }
    fn offset(self, bank: u16, size: u64) -> Result<u64> {
        if bank > 999 {
            return Err("bank number must be 0–999 (zero-based)".into());
        }
        let offset = u64::from(bank) * self.stride;
        if offset + self.payload > size {
            return Err("selected bank extends beyond the source".into());
        }
        Ok(offset)
    }
}

fn supported_source(meta: &fs::Metadata) -> bool {
    if meta.is_file() {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if meta.file_type().is_block_device() {
            return true;
        }
    }
    false
}
fn open_source(path: &Path) -> Result<(File, u64)> {
    #[cfg(windows)]
    if crate::windev::is_device(path) {
        return crate::windev::open_read(path);
    }
    if !supported_source(&fs::metadata(path)?) {
        return Err("source must be a regular whole-device image or readable block device".into());
    }
    let mut file = open_read_only(path).map_err(|e| {
        io::Error::other(format!("cannot read {}: {e}. Raw devices need OS read authorization; a mounted folder cannot expose other banks", path.display()))
    })?;
    if !supported_source(&file.metadata()?) {
        return Err("source type changed while opening".into());
    }
    let size = file.seek(SeekFrom::End(0))?;
    file.rewind()?;
    if size == 0 {
        return Err("source is empty or has no seekable size".into());
    }
    Ok((file, size))
}

fn open_read_only(path: &Path) -> Result<File> {
    match File::open(path) {
        Ok(file) => Ok(file),
        Err(error) => {
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::FileTypeExt;
                if error.kind() == io::ErrorKind::PermissionDenied
                    && fs::metadata(path)?.file_type().is_block_device()
                {
                    return linux_authorized_open(path, false);
                }
            }
            Err(error.into())
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_system_connection() -> Result<zbus::blocking::Connection> {
    use std::time::Duration;
    use zbus::blocking::connection::Builder;
    // Keep the same D-Bus caller for the application's lifetime so polkit can
    // reuse its retained authorization. Failed/cancelled authorization is not
    // cached, and device descriptors remain scoped to each operation.
    static CONNECTION: std::sync::Mutex<Option<zbus::blocking::Connection>> =
        std::sync::Mutex::new(None);
    let connection = {
        let mut cached = CONNECTION
            .lock()
            .map_err(|_| "Authorization connection lock failed")?;
        if cached.is_none() {
            *cached = Some(
                Builder::system()?
                    .method_timeout(Duration::from_secs(120))
                    .build()?,
            );
        }
        cached.as_ref().unwrap().clone()
    };
    Ok(connection)
}

#[cfg(target_os = "linux")]
fn linux_authorized_open(path: &Path, write: bool) -> Result<File> {
    use std::{collections::HashMap, os::unix::fs::MetadataExt};
    use zbus::{
        blocking::Proxy,
        proxy::MethodFlags,
        zvariant::{OwnedFd, Value},
    };
    let expected_device = fs::metadata(path)?.rdev();
    let connection = linux_system_connection()?;
    let manager = Proxy::new(
        &connection,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )?;
    let objects: zbus::fdo::ManagedObjects = manager.call("GetManagedObjects", &())?;
    let object = objects
        .into_iter()
        .find_map(|(object, interfaces)| {
            interfaces
                .get("org.freedesktop.UDisks2.Block")
                .and_then(|properties| properties.get("DeviceNumber"))
                .and_then(|value| u64::try_from(value).ok())
                .filter(|device| *device == expected_device)
                .map(|_| object)
        })
        .ok_or("UDisks does not recognize this block device")?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.UDisks2",
        object,
        "org.freedesktop.UDisks2.Block",
    )?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    if write {
        options.insert("flags", Value::from(libc::O_EXCL | libc::O_CLOEXEC));
    }
    // Authorization grants only this descriptor, never privileges to the GUI.
    let mode = if write { "rw" } else { "r" };
    let fd: OwnedFd = proxy
        .call_with_flags(
            "OpenDevice",
            MethodFlags::AllowInteractiveAuth.into(),
            &(mode, options),
        )?
        .ok_or("UDisks did not return a device handle")?;
    let file = File::from(std::os::fd::OwnedFd::from(fd));
    if file.metadata()?.rdev() != expected_device {
        return Err("device changed while authorizing access; select it again".into());
    }
    Ok(file)
}

fn new_output(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    let result = write(&mut output).and_then(|()| Ok(output.sync_all()?));
    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(path);
    }
    result
}
fn copy_exact(source: &mut File, output: &mut File, length: u64) -> Result<()> {
    let copied = io::copy(&mut source.take(length), output)?;
    if copied != length {
        return Err("source became shorter during the copy; incomplete output removed".into());
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BankEntry {
    pub number: u16,
    pub offset: u64,
    pub size: u64,
    pub label: String,
    pub status: String,
    pub recognized: bool,
}
#[derive(Debug, Clone)]
pub struct Catalog {
    pub size: u64,
    pub stride: u64,
    pub payload: u64,
    pub rows: Vec<BankEntry>,
}
pub fn catalog(path: &Path, profile: &str) -> Result<Catalog> {
    let layout = Profile::parse(profile)?;
    let (mut source, size) = open_source(path)?;
    layout.offset(0, size)?;
    let count = ((size - layout.payload) / layout.stride + 1).min(1000);
    let mut rows = Vec::new();
    for bank in 0..count {
        let offset = bank * layout.stride;
        source.seek(SeekFrom::Start(offset))?;
        let mut boot = [0; 512];
        source.read_exact(&mut boot)?;
        let word = |at| u16::from_le_bytes([boot[at], boot[at + 1]]) as u64;
        let matching = boot[510..] == [0x55, 0xaa]
            && word(11) == 512
            && word(19) * 512 == layout.payload
            && boot[13].is_power_of_two()
            && boot[13] <= 128
            && word(14) > 0
            && (1..=2).contains(&boot[16])
            && word(17) > 0
            && word(22) > 0;
        let status = if matching {
            "Matching FAT BPB"
        } else if boot.iter().all(|b| *b == 0) {
            "Zero boot sector"
        } else {
            "Unrecognized / mismatch"
        };
        let label: String = if matching && boot[38] == 0x29 {
            boot[43..54]
                .iter()
                .map(|b| {
                    if b.is_ascii_graphic() || *b == b' ' {
                        *b as char
                    } else {
                        '?'
                    }
                })
                .collect()
        } else {
            String::new()
        };
        rows.push(BankEntry {
            number: bank as u16,
            offset,
            size: layout.payload,
            label: label.trim_end().into(),
            status: status.into(),
            recognized: matching,
        });
    }
    Ok(Catalog {
        size,
        stride: layout.stride,
        payload: layout.payload,
        rows,
    })
}
pub fn list(path: &Path, profile: &str) -> Result<()> {
    let catalog = catalog(path, profile)?;
    println!("Profile: {profile} (explicit, not auto-detected)");
    println!(
        "Source: {} bytes; stride: {}; payload: {}",
        catalog.size, catalog.stride, catalog.payload
    );
    println!(
        "Scanning {} possible banks. Boot headers only; not a filesystem check or proof of device compatibility.",
        catalog.rows.len()
    );
    println!("BANK  OFFSET       HEADER                  BOOT LABEL");
    for entry in catalog.rows {
        println!(
            "{:03}   {:010}   {:23} {}",
            entry.number, entry.offset, entry.status, entry.label
        );
    }
    Ok(())
}

pub fn export(path: &Path, profile: &str, bank: u16, dest: &Path) -> Result<()> {
    let layout = Profile::parse(profile)?;
    let (mut source, size) = open_source(path)?;
    let offset = layout.offset(bank, size)?;
    source.seek(SeekFrom::Start(offset))?;
    new_output(dest, |output| {
        copy_exact(&mut source, output, layout.payload)
    })?;
    println!(
        "Exported bank {bank:03}: {} bytes at offset {offset}. Source unchanged; inspect the image before editing.",
        layout.payload
    );
    Ok(())
}

pub fn backup(path: &Path, dest: &Path) -> Result<()> {
    let (mut source, size) = open_source(path)?;
    new_output(dest, |output| copy_exact(&mut source, output, size))?;
    println!(
        "Copied all {size} bytes, including bank padding and trailing data. Keep the source unchanged while backing up; use bank-verify to compare."
    );
    Ok(())
}

pub fn verify(path: &Path, backup: &Path) -> Result<()> {
    let (mut source, size) = open_source(path)?;
    if !fs::metadata(backup)?.is_file() {
        return Err("backup must be a regular file".into());
    }
    let (mut saved, saved_size) = open_source(backup)?;
    if size != saved_size {
        return Err(format!("size differs: source {size} bytes; backup {saved_size} bytes").into());
    }
    let mut a = vec![0; 1024 * 1024];
    let mut b = vec![0; a.len()];
    let mut offset = 0;
    while offset < size {
        let count = (size - offset).min(a.len() as u64) as usize;
        source.read_exact(&mut a[..count])?;
        saved.read_exact(&mut b[..count])?;
        if let Some(index) = a[..count].iter().zip(&b[..count]).position(|(a, b)| a != b) {
            return Err(format!("backup differs at byte {}", offset + index as u64).into());
        }
        offset += count as u64;
    }
    println!("Verified all {size} bytes. Source and backup match.");
    Ok(())
}

pub fn replace(path: &Path, profile: &str, bank: u16, image: &Path, dest: &Path) -> Result<()> {
    let layout = Profile::parse(profile)?;
    if !fs::metadata(path)?.is_file() {
        return Err(
            "bank replacement requires a whole-device backup file; first back up the USB".into(),
        );
    }
    let (mut source, size) = open_source(path)?;
    if !source.metadata()?.is_file() {
        return Err("backup must remain a regular file".into());
    }
    let offset = layout.offset(bank, size)?;
    if !fs::metadata(image)?.is_file() {
        return Err("replacement floppy must be a regular file".into());
    }
    let (mut floppy, length) = open_source(image)?;
    if length != layout.payload {
        return Err(format!(
            "replacement must be exactly {} bytes for {profile}",
            layout.payload
        )
        .into());
    }
    let mut data = vec![0; layout.payload as usize];
    floppy.read_exact(&mut data)?;
    new_output(dest, |output| {
        copy_exact(&mut source, output, size)?;
        output.seek(SeekFrom::Start(offset))?;
        output.write_all(&data)?;
        output.flush()?;
        output.seek(SeekFrom::Start(offset))?;
        let mut check = vec![0; data.len()];
        output.read_exact(&mut check)?;
        if check != data {
            return Err("replacement read-back verification failed".into());
        }
        Ok(())
    })?;
    println!(
        "Replaced bank {bank:03} in a new backup copy; padding and other banks preserved. Original backup and USB unchanged."
    );
    Ok(())
}

fn read_floppy(path: &Path, length: u64) -> Result<Vec<u8>> {
    if !fs::metadata(path)?.is_file() {
        return Err("floppy input must be a regular file".into());
    }
    let mut file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() != length {
        return Err(format!("floppy input must be exactly {length} bytes").into());
    }
    let mut data = vec![0; length as usize];
    file.read_exact(&mut data)?;
    Ok(data)
}

fn open_write_target(path: &Path) -> Result<File> {
    #[cfg(windows)]
    if crate::windev::is_device(path) {
        return Err("writing raw USB devices is not supported on Windows yet; reading, exporting and backups work. Write banks from Linux or into a whole-device backup image".into());
    }
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if !file.metadata()?.is_file() {
            return Err("target type changed while opening".into());
        }
        file.try_lock()
            .map_err(|e| format!("target is already in use: {e}"))?;
        return Ok(file);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
        if metadata.file_type().is_block_device() {
            // O_EXCL on a block device rejects a mounted/in-use device. Never
            // retry without it, including when asking UDisks for authorization.
            let file = match OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_EXCL | libc::O_CLOEXEC)
                .open(path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    linux_authorized_open(path, true)?
                }
                Err(error) => {
                    return Err(format!(
                        "cannot claim device exclusively; unmount it before writing: {error}"
                    )
                    .into());
                }
            };
            if file.metadata()?.rdev() != metadata.rdev() {
                return Err("target device changed during authorization".into());
            }
            return Ok(file);
        }
    }
    Err("write target must be a regular image file or an unmounted Linux block device".into())
}

trait BankIo: Read + Write + Seek {
    fn sync_bank(&mut self) -> io::Result<()>;
}
impl BankIo for File {
    fn sync_bank(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}
fn read_bank(target: &mut (impl Read + Seek), offset: u64, length: usize) -> io::Result<Vec<u8>> {
    target.seek(SeekFrom::Start(offset))?;
    let mut data = vec![0; length];
    target.read_exact(&mut data)?;
    Ok(data)
}
fn write_verified(target: &mut impl BankIo, offset: u64, data: &[u8]) -> io::Result<()> {
    target.seek(SeekFrom::Start(offset))?;
    target.write_all(data)?;
    target.sync_bank()?;
    if read_bank(target, offset, data.len())? != data {
        return Err(io::Error::other(
            "bank read-back differs from written bytes",
        ));
    }
    Ok(())
}
fn replace_with_recovery(
    target: &mut impl BankIo,
    offset: u64,
    original: &[u8],
    replacement: &[u8],
) -> Result<()> {
    if let Err(error) = write_verified(target, offset, replacement) {
        return match write_verified(target, offset, original) {
            Ok(()) => Err(format!("write failed ({error}); original bank restored and verified; recovery file retained").into()),
            Err(restore) => Err(format!("write failed ({error}); automatic recovery also failed ({restore}). Target bank may be incomplete; retain the recovery file").into()),
        };
    }
    Ok(())
}

/// Commit a disk-editor change directly, with no persistent backup file.
/// A bank target changes only its payload; a standalone image must match in full.
pub fn edit_disk(
    target: &Path,
    bank: Option<(&str, u16)>,
    expected: &[u8],
    replacement: &[u8],
) -> Result<()> {
    if expected.len() != replacement.len() || expected.is_empty() {
        return Err("Edited disk has a different size; no changes made".into());
    }
    crate::fat12::inspect(replacement)?;
    let layout = bank
        .map(|(profile, number)| -> Result<(Profile, u16)> {
            let layout = Profile::parse(profile)?;
            layout.offset(number, u64::MAX)?;
            if expected.len() as u64 != layout.payload {
                return Err("Editor snapshot has the wrong bank capacity".into());
            }
            Ok((layout, number))
        })
        .transpose()?;
    if bank.is_none() && !fs::metadata(target)?.is_file() {
        return Err("Standalone disk edits require an image file".into());
    }
    let mut file = open_write_target(target)?;
    let size = file.seek(SeekFrom::End(0))?;
    let offset = if let Some((layout, number)) = layout {
        layout.offset(number, size)?
    } else {
        if size != expected.len() as u64 {
            return Err("Disk size changed; reopen it before editing. No changes made".into());
        }
        0
    };
    if read_bank(&mut file, offset, expected.len())? != expected {
        return Err(
            "Disk changed since it was opened; reopen it before editing. No changes made".into(),
        );
    }
    if expected == replacement {
        return Ok(());
    }
    save_editor_bytes(&mut file, offset, expected, replacement)
}

fn save_editor_bytes(
    target: &mut impl BankIo,
    offset: u64,
    expected: &[u8],
    replacement: &[u8],
) -> Result<()> {
    if let Err(error) = write_verified(target, offset, replacement) {
        return match write_verified(target, offset, expected) {
            Ok(()) => Err(format!("Save failed ({error}); original contents restored and verified").into()),
            Err(restore) => Err(format!("Save failed ({error}); restoring original contents also failed ({restore}). Disk may be incomplete").into()),
        };
    }
    Ok(())
}

/// Caller requires explicit user confirmation. No input path is truncated or
/// reformatted. The expected original prevents blindly replacing a changed bank.
pub fn write_bank(
    target: &Path,
    profile: &str,
    bank: u16,
    image: &Path,
    expected: &Path,
    recovery: &Path,
) -> Result<()> {
    let layout = Profile::parse(profile)?;
    // Reject invalid bank numbers and inputs before requesting device access.
    layout.offset(bank, u64::MAX)?;
    let replacement = read_floppy(image, layout.payload)?;
    let expected = read_floppy(expected, layout.payload)?;
    let mut target_file = open_write_target(target)?;
    let size = target_file.seek(SeekFrom::End(0))?;
    let offset = layout.offset(bank, size)?;
    let original = read_bank(&mut target_file, offset, layout.payload as usize)?;
    if original != expected {
        return Err("target bank differs from the expected original; export and review it again before writing. No changes made".into());
    }
    if original == replacement {
        println!(
            "Bank {bank:03} already matches the replacement. No write or recovery file needed."
        );
        return Ok(());
    }
    // Persist and re-read the recovery copy before the first target write.
    new_output(recovery, |file| {
        file.write_all(&original)?;
        Ok(())
    })?;
    #[cfg(unix)]
    File::open(
        recovery
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    if read_floppy(recovery, layout.payload)? != original {
        return Err("recovery verification failed; target unchanged".into());
    }
    // Recheck after saving recovery, in case a non-cooperating process wrote
    // to a regular image despite the advisory lock.
    if read_bank(&mut target_file, offset, original.len())? != original {
        return Err("target bank changed while saving recovery; write cancelled".into());
    }
    replace_with_recovery(&mut target_file, offset, &original, &replacement)?;
    println!(
        "Wrote and verified bank {bank:03} at byte {offset} ({} bytes). Recovery: {}. Other banks and padding were not written.",
        layout.payload,
        recovery.display()
    );
    Ok(())
}

/// Save `data` to a new recovery file, sync it and read it back.
fn save_recovery(recovery: &Path, data: &[u8]) -> Result<()> {
    new_output(recovery, |file| {
        file.write_all(data)?;
        Ok(())
    })?;
    #[cfg(unix)]
    File::open(
        recovery
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    if read_floppy(recovery, data.len() as u64)? != data {
        return Err("recovery verification failed; target unchanged".into());
    }
    Ok(())
}

/// Exchange the floppies in two banks. Both originals are saved to new,
/// verified recovery files before the first write; a failed write restores
/// both banks.
pub fn swap_banks(
    target: &Path,
    profile: &str,
    a: u16,
    b: u16,
    recovery_a: &Path,
    recovery_b: &Path,
) -> Result<()> {
    let layout = Profile::parse(profile)?;
    layout.offset(a, u64::MAX)?;
    layout.offset(b, u64::MAX)?;
    if a == b {
        return Err("choose two different banks to swap".into());
    }
    if recovery_a == recovery_b {
        return Err("each bank needs its own recovery file".into());
    }
    let mut file = open_write_target(target)?;
    let size = file.seek(SeekFrom::End(0))?;
    let (offset_a, offset_b) = (layout.offset(a, size)?, layout.offset(b, size)?);
    let length = layout.payload as usize;
    let first = read_bank(&mut file, offset_a, length)?;
    let second = read_bank(&mut file, offset_b, length)?;
    if first == second {
        println!("Banks {a:03} and {b:03} are identical. Nothing to swap.");
        return Ok(());
    }
    save_recovery(recovery_a, &first)?;
    save_recovery(recovery_b, &second)?;
    if read_bank(&mut file, offset_a, length)? != first
        || read_bank(&mut file, offset_b, length)? != second
    {
        return Err("banks changed while saving recovery copies; swap cancelled".into());
    }
    replace_with_recovery(&mut file, offset_a, &first, &second)?;
    if let Err(error) = write_verified(&mut file, offset_b, &first) {
        let undo_b = write_verified(&mut file, offset_b, &second);
        let undo_a = write_verified(&mut file, offset_a, &first);
        return Err(match (undo_a, undo_b) {
            (Ok(()), Ok(())) => format!("swap failed ({error}); both banks restored and verified; recovery files retained"),
            (a_result, b_result) => format!(
                "swap failed ({error}); automatic recovery also failed ({}). Retain both recovery files",
                [a_result.err(), b_result.err()].into_iter().flatten().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
            ),
        }
        .into());
    }
    println!(
        "Swapped and verified banks {a:03} and {b:03}. Recovery: {} and {}. Other banks and padding were not written.",
        recovery_a.display(),
        recovery_b.display()
    );
    Ok(())
}

/// Erase one bank to a blank FAT12 floppy. The original is saved first.
pub fn format_bank(
    target: &Path,
    profile: &str,
    bank: u16,
    label: &str,
    recovery: &Path,
) -> Result<()> {
    let layout = Profile::parse(profile)?;
    layout.offset(bank, u64::MAX)?;
    let blank = crate::fat12::formatted_labelled((layout.payload / 1024) as usize, label)?;
    let mut file = open_write_target(target)?;
    let size = file.seek(SeekFrom::End(0))?;
    let offset = layout.offset(bank, size)?;
    let original = read_bank(&mut file, offset, blank.len())?;
    save_recovery(recovery, &original)?;
    if read_bank(&mut file, offset, original.len())? != original {
        return Err("target bank changed while saving recovery; format cancelled".into());
    }
    replace_with_recovery(&mut file, offset, &original, &blank)?;
    println!(
        "Formatted bank {bank:03} as a blank FAT12 disk{}. Recovery: {}. Other banks were not written.",
        if label.trim().is_empty() {
            String::new()
        } else {
            format!(" labelled {}", label.trim().to_ascii_uppercase())
        },
        recovery.display()
    );
    Ok(())
}

/// Erase a standalone raw floppy image in place, keeping its capacity.
pub fn format_image(target: &Path, label: &str, recovery: &Path) -> Result<()> {
    if !fs::symlink_metadata(target)?.is_file() {
        return Err("only regular image files can be formatted".into());
    }
    let original = fs::read(target)?;
    let kib = crate::fat12::capacity_for(original.len() as u64).ok_or_else(|| {
        format!(
            "{} bytes is not a standard floppy size; formatting supports 360, 720, 1200, 1440 and 2880 KiB raw images",
            original.len()
        )
    })?;
    let blank = crate::fat12::formatted_labelled(kib, label)?;
    save_recovery(recovery, &original)?;
    edit_disk(target, None, &original, &blank)?;
    println!(
        "Formatted {} as a blank {kib} KiB FAT12 disk. Recovery: {}",
        target.display(),
        recovery.display()
    );
    Ok(())
}

fn first_difference(
    a: &mut (impl Read + Seek),
    b: &mut (impl Read + Seek),
    size: u64,
) -> io::Result<Option<u64>> {
    a.rewind()?;
    b.rewind()?;
    let mut left = vec![0; 1024 * 1024];
    let mut right = vec![0; left.len()];
    let mut offset = 0;
    while offset < size {
        let count = (size - offset).min(left.len() as u64) as usize;
        a.read_exact(&mut left[..count])?;
        b.read_exact(&mut right[..count])?;
        if let Some(index) = left[..count]
            .iter()
            .zip(&right[..count])
            .position(|(a, b)| a != b)
        {
            return Ok(Some(offset + index as u64));
        }
        offset += count as u64;
    }
    Ok(None)
}

fn compare_streams(
    a: &mut (impl Read + Seek),
    b: &mut (impl Read + Seek),
    size: u64,
) -> io::Result<()> {
    match first_difference(a, b, size)? {
        None => Ok(()),
        Some(offset) => Err(io::Error::other(format!(
            "contents differ at byte {offset}"
        ))),
    }
}

fn copy_stream_verified(
    target: &mut impl BankIo,
    source: &mut (impl Read + Seek),
    size: u64,
) -> io::Result<()> {
    target.rewind()?;
    source.rewind()?;
    let count = io::copy(&mut source.take(size), target)?;
    if count != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "restore source became shorter",
        ));
    }
    target.sync_bank()?;
    compare_streams(target, source, size)
}

fn restore_with_recovery(
    target: &mut impl BankIo,
    source: &mut (impl Read + Seek),
    recovery: &mut (impl Read + Seek),
    size: u64,
) -> Result<()> {
    if let Err(error) = copy_stream_verified(target, source, size) {
        return match copy_stream_verified(target, recovery, size) {
            Ok(()) => Err(format!("restore failed ({error}); original device contents restored and verified; recovery file retained").into()),
            Err(undo) => Err(format!("restore failed ({error}); automatic recovery also failed ({undo}). Target may be incomplete; retain the whole-device recovery file").into()),
        };
    }
    Ok(())
}

fn open_locked_image(path: &Path) -> Result<(File, u64)> {
    if !fs::metadata(path)?.is_file() {
        return Err("whole-device input must be a regular file".into());
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err("input type changed while opening".into());
    }
    file.try_lock_shared()
        .map_err(|e| format!("input image is in use: {e}"))?;
    let size = file.metadata()?.len();
    if size == 0 {
        return Err("whole-device input is empty".into());
    }
    Ok((file, size))
}

/// Restore all bytes, including bank gaps and trailing device data. Exact
/// capacity matching prevents silently retaining or truncating a device tail.
pub fn restore_device(target: &Path, image: &Path, expected: &Path, recovery: &Path) -> Result<()> {
    let (mut source, size) = open_locked_image(image)?;
    let (mut expected, expected_size) = open_locked_image(expected)?;
    if size != expected_size {
        return Err("replacement and expected whole-device images have different sizes".into());
    }
    let mut target = open_write_target(target)?;
    if target.seek(SeekFrom::End(0))? != size {
        return Err(
            "target capacity must exactly match both whole-device images; no changes made".into(),
        );
    }
    compare_streams(&mut target, &mut expected, size)
        .map_err(|e| format!("target differs from the expected backup ({e}); no changes made"))?;
    if first_difference(&mut target, &mut source, size)?.is_none() {
        println!(
            "Target already matches the whole-device image. No write or recovery file needed."
        );
        return Ok(());
    }
    target.rewind()?;
    new_output(recovery, |output| copy_exact(&mut target, output, size))?;
    #[cfg(unix)]
    File::open(
        recovery
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    let (mut saved, saved_size) = open_locked_image(recovery)?;
    if saved_size != size {
        return Err("recovery size differs; target unchanged".into());
    }
    compare_streams(&mut saved, &mut expected, size)
        .map_err(|e| format!("recovery verification failed ({e}); target unchanged"))?;
    compare_streams(&mut target, &mut saved, size)
        .map_err(|e| format!("target changed while saving recovery ({e}); write cancelled"))?;
    restore_with_recovery(&mut target, &mut source, &mut saved, size)?;
    println!(
        "Restored and verified all {size} bytes, including bank padding and trailing data. Original contents saved to {}",
        recovery.display()
    );
    Ok(())
}

/// Create a new superfloppy bank image. No MBR/partition table or firmware
/// selector metadata: each bank starts directly at its fixed-stride offset.
pub fn create_layout(dest: &Path, profile: &str, count: u16, capacity: &str) -> Result<()> {
    let layout = Profile::parse(profile)?;
    if !(1..=1000).contains(&count) {
        return Err("bank count must be 1–1000".into());
    }
    let minimum = layout.stride * u64::from(count);
    let size = if capacity == "minimum" {
        minimum
    } else {
        capacity.parse::<u64>()?
    };
    if size < minimum || size > i64::MAX as u64 || !size.is_multiple_of(512) {
        return Err(format!(
            "capacity must be a multiple of 512 bytes, at least {minimum}, and no larger than {}",
            i64::MAX
        )
        .into());
    }
    let mut floppy = crate::fat12::formatted_image((layout.payload / 1024) as usize)?;
    let word = |at| u16::from_le_bytes([floppy[at], floppy[at + 1]]) as usize;
    let root = (word(14) + floppy[16] as usize * word(22)) * 512;
    let serial = u32::from_le_bytes(floppy[39..43].try_into()?);
    new_output(dest, |output| {
        // Extending a newly-created file yields zero gaps and tail bytes.
        // Some filesystems store these regions sparsely.
        output.set_len(size)?;
        for bank in 0..count {
            let label = format!("FDISK{bank:03}   ");
            floppy[43..54].copy_from_slice(label.as_bytes());
            floppy[root..root + 11].copy_from_slice(label.as_bytes());
            floppy[root + 11] = 0x08; // root volume label matches the BPB
            floppy[39..43].copy_from_slice(&serial.wrapping_add(u32::from(bank)).to_le_bytes());
            let offset = u64::from(bank) * layout.stride;
            output.seek(SeekFrom::Start(offset))?;
            output.write_all(&floppy)?;
            if read_bank(output, offset, floppy.len())? != floppy {
                return Err(format!("bank {bank:03} creation read-back failed").into());
            }
        }
        Ok(())
    })?;
    println!(
        "Created {count} formatted {profile} banks in a new {size}-byte image. Labels FDISK000–FDISK{:03}; bank gaps and remaining capacity are zero. No bootable OS included.",
        count - 1
    );
    println!(
        "Review the image before restoring it to a matching-capacity device. Emulator compatibility requires hardware verification."
    );
    Ok(())
}

#[cfg(test)]
mod write_tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn streaming_restore_recovers_after_a_partial_chunk_write() {
        let size = 2 * 1024 * 1024 + 19;
        let original = vec![7; size];
        let mut target = Faulty {
            bytes: Cursor::new(original.clone()),
            remaining: 1024 * 1024 + 3,
            permanent: false,
        };
        let mut source = Cursor::new(vec![9; size]);
        let mut saved = Cursor::new(original.clone());
        let error =
            restore_with_recovery(&mut target, &mut source, &mut saved, size as u64).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("original device contents restored and verified")
        );
        assert_eq!(target.bytes.into_inner(), original);
    }
    #[test]
    fn short_restore_source_and_failed_rollback_are_reported() {
        let mut target = Faulty {
            bytes: Cursor::new(vec![7; 32]),
            remaining: usize::MAX,
            permanent: false,
        };
        let error = restore_with_recovery(
            &mut target,
            &mut Cursor::new(vec![9; 15]),
            &mut Cursor::new(vec![7; 32]),
            32,
        )
        .unwrap_err();
        assert!(error.to_string().contains("restore source became shorter"));
        assert_eq!(target.bytes.into_inner(), vec![7; 32]);
        let mut target = Faulty {
            bytes: Cursor::new(vec![7; 32]),
            remaining: 3,
            permanent: true,
        };
        let error = restore_with_recovery(
            &mut target,
            &mut Cursor::new(vec![9; 32]),
            &mut Cursor::new(vec![7; 32]),
            32,
        )
        .unwrap_err();
        assert!(error.to_string().contains("automatic recovery also failed"));
    }
    #[test]
    fn editor_save_restores_partial_writes_without_a_backup_file() {
        let original = vec![7; 32];
        let mut target = Faulty {
            bytes: Cursor::new(original.clone()),
            remaining: 3,
            permanent: false,
        };
        let error = save_editor_bytes(&mut target, 8, &[7; 16], &[9; 16]).unwrap_err();
        assert!(error.to_string().contains("restored and verified"));
        assert_eq!(target.bytes.into_inner(), original);
    }

    #[test]
    fn editor_save_reports_failed_rollback() {
        let mut target = Faulty {
            bytes: Cursor::new(vec![7; 32]),
            remaining: 3,
            permanent: true,
        };
        let error = save_editor_bytes(&mut target, 8, &[7; 16], &[9; 16]).unwrap_err();
        assert!(error.to_string().contains("Disk may be incomplete"));
        assert!(!error.to_string().contains("recovery file"));
    }

    struct Faulty {
        bytes: Cursor<Vec<u8>>,
        remaining: usize,
        permanent: bool,
    }
    impl Read for Faulty {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.bytes.read(buf)
        }
    }
    impl Seek for Faulty {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.bytes.seek(pos)
        }
    }
    impl Write for Faulty {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                if !self.permanent {
                    self.remaining = usize::MAX;
                }
                return Err(io::Error::other("injected write failure"));
            }
            let n = buf.len().min(self.remaining);
            self.remaining -= n;
            self.bytes.write(&buf[..n])
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl BankIo for Faulty {
        fn sync_bank(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn partial_write_restores_original_and_preserves_neighbors() {
        let mut target = Faulty {
            bytes: Cursor::new(vec![7; 32]),
            remaining: 3,
            permanent: false,
        };
        let error = replace_with_recovery(&mut target, 8, &[7; 16], &[9; 16]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("original bank restored and verified")
        );
        assert_eq!(target.bytes.into_inner(), vec![7; 32]);
    }
    #[test]
    fn permanent_failure_reports_incomplete_target() {
        let mut target = Faulty {
            bytes: Cursor::new(vec![7; 32]),
            remaining: 3,
            permanent: true,
        };
        let error = replace_with_recovery(&mut target, 8, &[7; 16], &[9; 16]).unwrap_err();
        assert!(error.to_string().contains("automatic recovery also failed"));
        assert_eq!(&target.bytes.get_ref()[..8], &[7; 8]);
        assert_eq!(&target.bytes.get_ref()[24..], &[7; 8]);
    }
}
