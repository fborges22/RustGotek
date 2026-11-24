//! Windows USB disk discovery, raw read/write access and safe ejection.
//!
//! Many USB flash drives report themselves as fixed disks, so detection asks
//! the storage stack for the bus type instead of trusting `GetDriveType`.
//! Gotek media written by the original firmware has no partition table and
//! may have no drive letter at all, so physical disks are enumerated directly.
use crate::Result;
use std::{
    collections::BTreeMap,
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{
        fs::{FileExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Storage::FileSystem::{
        BusTypeMmc, BusTypeSd, BusTypeUsb, FILE_SHARE_READ, FILE_SHARE_WRITE, FindFirstVolumeW,
        FindNextVolumeW, FindVolumeClose, GetDriveTypeW, GetLogicalDrives,
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            DISK_EXTENT, DISK_GEOMETRY_EX, FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME,
            FSCTL_UNLOCK_VOLUME, GET_LENGTH_INFORMATION, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            IOCTL_DISK_GET_LENGTH_INFO, IOCTL_DISK_UPDATE_PROPERTIES, IOCTL_STORAGE_EJECT_MEDIA,
            IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_MEDIA_REMOVAL,
            IOCTL_STORAGE_QUERY_PROPERTY, PREVENT_MEDIA_REMOVAL, PropertyStandardQuery,
            STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY,
            StorageDeviceProperty,
        },
        WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
    },
    UI::{
        Shell::{IsUserAnAdmin, ShellExecuteW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

const FILE_DEVICE_DISK: u32 = 7;
const MAX_PHYSICAL_DRIVES: u32 = 64;

#[derive(Clone, Debug)]
pub struct Disk {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub usb: bool,
    pub letters: Vec<char>,
}
impl Disk {
    pub fn label(&self) -> String {
        let letters = if self.letters.is_empty() {
            "no drive letter".to_string()
        } else {
            self.letters
                .iter()
                .map(|l| format!("{l}:"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        format!(
            "{} — {} · {:.1} MiB · {letters}",
            self.path,
            if self.name.is_empty() {
                "Unknown device"
            } else {
                &self.name
            },
            self.size as f64 / 1_048_576.0
        )
    }
}

/// Opens a device for IOCTL queries only; this needs no administrator rights.
fn open_query(path: &str) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(path)
}

fn ioctl<I>(file: &File, code: u32, input: Option<&I>, output: &mut [u8]) -> io::Result<usize> {
    let mut returned = 0u32;
    // SAFETY: the handle is open for the duration of the call, the input
    // pointer covers `size_of::<I>()` bytes, and the output slice is writable.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as _,
            code,
            input.map_or(null(), |i| (i as *const I).cast::<c_void>()),
            input.map_or(0, |_| size_of::<I>() as u32),
            output.as_mut_ptr().cast(),
            output.len() as u32,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(returned as usize)
    }
}

fn read_struct<T: Copy>(buffer: &[u8], returned: usize) -> Option<T> {
    // SAFETY: the length is checked and `read_unaligned` tolerates any alignment.
    (returned >= size_of::<T>() && buffer.len() >= size_of::<T>())
        .then(|| unsafe { buffer.as_ptr().cast::<T>().read_unaligned() })
}

struct Descriptor {
    name: String,
    usb: bool,
}
fn descriptor(file: &File) -> Option<Descriptor> {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut buffer = vec![0u8; 1024];
    let returned = ioctl(
        file,
        IOCTL_STORAGE_QUERY_PROPERTY,
        Some(&query),
        &mut buffer,
    )
    .ok()?;
    let d: STORAGE_DEVICE_DESCRIPTOR = read_struct(&buffer, returned)?;
    let text = |offset: u32| -> String {
        let offset = offset as usize;
        if offset == 0 || offset >= returned {
            return String::new();
        }
        let bytes = &buffer[offset..returned];
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).trim().to_string()
    };
    let name = [text(d.VendorIdOffset), text(d.ProductIdOffset)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(Descriptor {
        name,
        // Card readers and some USB bridges report SD/MMC or only the
        // removable-media flag; Gotek media can be any of these.
        usb: d.BusType == BusTypeUsb
            || d.BusType == BusTypeSd
            || d.BusType == BusTypeMmc
            || d.RemovableMedia,
    })
}

fn disk_size(file: &File) -> Option<u64> {
    let mut buffer = vec![0u8; 256];
    let returned = ioctl::<()>(file, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, None, &mut buffer).ok()?;
    let geometry: DISK_GEOMETRY_EX = read_struct(&buffer, returned)?;
    u64::try_from(geometry.DiskSize).ok()
}

fn device_number(file: &File) -> Option<u32> {
    let mut buffer = vec![0u8; size_of::<STORAGE_DEVICE_NUMBER>()];
    let returned = ioctl::<()>(file, IOCTL_STORAGE_GET_DEVICE_NUMBER, None, &mut buffer).ok()?;
    let number: STORAGE_DEVICE_NUMBER = read_struct(&buffer, returned)?;
    (number.DeviceType == FILE_DEVICE_DISK).then_some(number.DeviceNumber)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn logical_letters() -> Vec<char> {
    // SAFETY: no arguments; returns a bitmask of assigned drive letters.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| (b'A' + bit) as char)
        .collect()
}

fn drive_type(letter: char) -> u32 {
    let root = wide(&format!("{letter}:\\"));
    // SAFETY: `root` is a NUL-terminated UTF-16 string.
    unsafe { GetDriveTypeW(root.as_ptr()) }
}

/// Physical disk number behind each drive letter.
fn letter_disks() -> BTreeMap<char, u32> {
    logical_letters()
        .into_iter()
        .filter(|l| matches!(drive_type(*l), DRIVE_REMOVABLE | DRIVE_FIXED))
        .filter_map(|l| {
            let file = open_query(&format!(r"\\.\{l}:")).ok()?;
            Some((l, device_number(&file)?))
        })
        .collect()
}

/// All physical disks Windows exposes, USB or not.
pub fn disks() -> Vec<Disk> {
    let letters = letter_disks();
    (0..MAX_PHYSICAL_DRIVES)
        .filter_map(|number| {
            let path = format!(r"\\.\PhysicalDrive{number}");
            let file = open_query(&path).ok()?;
            let descriptor = descriptor(&file);
            Some(Disk {
                size: disk_size(&file).unwrap_or(0),
                name: descriptor
                    .as_ref()
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                usb: descriptor.is_some_and(|d| d.usb),
                letters: letters
                    .iter()
                    .filter(|(_, n)| **n == number)
                    .map(|(l, _)| *l)
                    .collect(),
                path,
            })
        })
        .collect()
}

fn system_letter() -> Option<char> {
    std::env::var("SystemDrive")
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| c.to_ascii_uppercase())
}

/// USB disks with media inserted, excluding the Windows system disk.
pub fn usb_disks() -> Vec<Disk> {
    let system = system_letter();
    disks()
        .into_iter()
        .filter(|d| d.usb && d.size > 0 && !d.letters.iter().any(|l| Some(*l) == system))
        .collect()
}

/// Mounted folders on USB drives, including flash drives reported as fixed.
pub fn media_roots() -> Vec<PathBuf> {
    let usb: Vec<_> = usb_disks().into_iter().flat_map(|d| d.letters).collect();
    let system = system_letter();
    logical_letters()
        .into_iter()
        .filter(|l| Some(*l) != system)
        .filter(|l| usb.contains(l) || drive_type(*l) == DRIVE_REMOVABLE)
        .map(|l| PathBuf::from(format!("{l}:\\")))
        .filter(|p| p.is_dir())
        .collect()
}

fn drive_letter(text: &str) -> Option<char> {
    let text = text
        .strip_prefix(r"\\.\")
        .or_else(|| text.strip_prefix(r"\\?\"))
        .unwrap_or(text);
    let text = text.strip_suffix(['\\', '/']).unwrap_or(text);
    let mut chars = text.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(l), Some(':'), None) if l.is_ascii_alphabetic() => Some(l.to_ascii_uppercase()),
        _ => None,
    }
}

/// True for raw-device paths, and for bare drive letters such as `E:`,
/// which select the whole USB disk behind that letter.
pub fn is_device(path: &Path) -> bool {
    let text = path.to_string_lossy();
    let lower = text.to_ascii_lowercase();
    lower.starts_with(r"\\.\")
        || lower.starts_with(r"\\?\physicaldrive")
        || lower.starts_with(r"\\?\globalroot")
        || drive_letter(&text).is_some()
}

/// Maps a drive letter to its physical disk; a Gotek bank layout spans the
/// whole disk, while the drive letter exposes only the first bank.
fn resolve(path: &Path) -> Result<String> {
    let text = path.to_string_lossy();
    match drive_letter(&text) {
        Some(letter) => {
            let file = open_query(&format!(r"\\.\{letter}:"))
                .map_err(|e| format!("cannot open drive {letter}: ({e})"))?;
            let number = device_number(&file).ok_or_else(|| {
                format!("drive {letter}: is not backed by a single physical disk")
            })?;
            Ok(format!(r"\\.\PhysicalDrive{number}"))
        }
        None => Ok(text.into_owned()),
    }
}

pub fn open_read(path: &Path) -> Result<(File, u64)> {
    let device = resolve(path)?;
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&device)
        .map_err(|e| -> Box<dyn std::error::Error> {
            if e.kind() == io::ErrorKind::PermissionDenied {
                format!("cannot read {device}: {e}. Reading raw USB devices on Windows requires running RustGotek as administrator").into()
            } else {
                format!("cannot open {device}: {e}. Use Detect USB to list connected drives").into()
            }
        })?;
    let mut buffer = vec![0u8; size_of::<GET_LENGTH_INFORMATION>()];
    let size = ioctl::<()>(&file, IOCTL_DISK_GET_LENGTH_INFO, None, &mut buffer)
        .ok()
        .and_then(|n| read_struct::<GET_LENGTH_INFORMATION>(&buffer, n))
        .and_then(|l| u64::try_from(l.Length).ok())
        .or_else(|| disk_size(&file))
        .filter(|s| *s > 0)
        .ok_or_else(|| format!("{device} has no readable capacity; is media inserted?"))?;
    Ok((file, size))
}

/// The physical disk number behind a device path or drive letter.
fn disk_number(device: &str) -> Result<u32> {
    let file = open_query(device).map_err(|e| format!("cannot open {device}: {e}"))?;
    device_number(&file).ok_or_else(|| format!("{device} is not a physical disk").into())
}

/// Volume device paths (`\\?\Volume{…}`) with an extent on disk `number`,
/// with any drive letters they are mounted at. Gotek media usually has a
/// single FAT volume at offset 0, but every volume on the disk must be locked.
fn volumes_on(number: u32) -> Vec<(String, Vec<char>)> {
    let letters = letter_disks();
    let mut found = vec![];
    let mut name = [0u16; 260];
    // SAFETY: the buffer is writable and its length is passed.
    let handle = unsafe { FindFirstVolumeW(name.as_mut_ptr(), name.len() as u32) };
    if handle == INVALID_HANDLE_VALUE {
        return found;
    }
    loop {
        let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
        let volume = String::from_utf16_lossy(&name[..end]);
        let device = volume.trim_end_matches('\\').to_string();
        if let Ok(file) = open_query(&device) {
            let mut buffer = vec![0u8; 1024];
            if let Ok(returned) = ioctl::<()>(
                &file,
                IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                None,
                &mut buffer,
            ) {
                let count = read_struct::<u32>(&buffer, returned).unwrap_or(0) as usize;
                let extent = size_of::<DISK_EXTENT>();
                let on_disk = (0..count).any(|i| {
                    let at = 8 + i * extent;
                    buffer
                        .get(at..at + extent)
                        .and_then(|b| read_struct::<DISK_EXTENT>(b, extent))
                        .is_some_and(|e| e.DiskNumber == number)
                });
                if on_disk {
                    let id = device_number_of_volume(&file);
                    let mounted: Vec<char> = letters
                        .iter()
                        .filter(|(_, n)| **n == number)
                        .map(|(l, _)| *l)
                        .filter(|l| {
                            id.is_some()
                                && open_query(&format!(r"\\.\{l}:"))
                                    .ok()
                                    .and_then(|f| device_number_of_volume(&f))
                                    == id
                        })
                        .collect();
                    found.push((device, mounted));
                }
            }
        }
        // SAFETY: `handle` came from FindFirstVolumeW; the buffer is writable.
        if unsafe { FindNextVolumeW(handle, name.as_mut_ptr(), name.len() as u32) } == 0 {
            break;
        }
    }
    // SAFETY: `handle` is a valid find-volume handle and is closed once.
    unsafe { FindVolumeClose(handle) };
    found
}

/// Identifies a volume by its partition number, so a drive letter can be
/// matched to its `\\?\Volume{…}` path on multi-volume disks.
fn device_number_of_volume(file: &File) -> Option<(u32, u32)> {
    let mut buffer = vec![0u8; size_of::<STORAGE_DEVICE_NUMBER>()];
    let returned = ioctl::<()>(file, IOCTL_STORAGE_GET_DEVICE_NUMBER, None, &mut buffer).ok()?;
    let number: STORAGE_DEVICE_NUMBER = read_struct(&buffer, returned)?;
    Some((number.DeviceNumber, number.PartitionNumber))
}

/// The writable disks: USB or removable, never the Windows system disk.
fn check_writable(number: u32, device: &str) -> Result<()> {
    let disk = disks()
        .into_iter()
        .find(|d| {
            d.path
                .eq_ignore_ascii_case(&format!(r"\\.\PhysicalDrive{number}"))
        })
        .ok_or_else(|| format!("{device} is not connected"))?;
    if disk.letters.iter().any(|l| Some(*l) == system_letter()) {
        return Err("refusing to write to the Windows system disk".into());
    }
    if !disk.usb {
        return Err(format!(
            "{device} is not a USB or removable disk; RustGotek only writes to USB media"
        )
        .into());
    }
    Ok(())
}

/// A volume kept locked and dismounted while its disk is written, so Windows
/// neither blocks the raw writes nor serves stale cached data afterwards.
struct LockedVolume(File);
impl Drop for LockedVolume {
    fn drop(&mut self) {
        let _ = ioctl::<()>(&self.0, FSCTL_UNLOCK_VOLUME, None, &mut []);
    }
}

fn lock_volume(device: &str, letters: &[char]) -> Result<LockedVolume> {
    let name = if letters.is_empty() {
        "the USB volume".to_string()
    } else {
        letters
            .iter()
            .map(|l| format!("{l}:"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(device)
        .map_err(|e| access_error(&name, e))?;
    // Explorer and antivirus scanners briefly hold volumes open; retry.
    let mut attempt = 0;
    while let Err(error) = ioctl::<()>(&file, FSCTL_LOCK_VOLUME, None, &mut []) {
        attempt += 1;
        if attempt == 20 {
            return Err(format!(
                "cannot lock {name} for writing ({error}). Close File Explorer windows and programs using the drive, then try again"
            )
            .into());
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    ioctl::<()>(&file, FSCTL_DISMOUNT_VOLUME, None, &mut [])
        .map_err(|e| format!("cannot dismount {name}: {e}"))?;
    Ok(LockedVolume(file))
}

fn access_error(what: &str, e: io::Error) -> Box<dyn std::error::Error> {
    if e.kind() == io::ErrorKind::PermissionDenied {
        format!("cannot open {what} for writing: {e}. Writing to USB media on Windows requires running RustGotek as administrator (Device → Restart as administrator)").into()
    } else {
        format!("cannot open {what} for writing: {e}").into()
    }
}

const ALIGN: usize = 4096;
const CHUNK: u64 = 1024 * 1024;

/// A heap buffer whose usable slice starts at a 4 KiB boundary, as raw disk
/// I/O may require.
struct Aligned {
    raw: Vec<u8>,
    start: usize,
    len: usize,
}
impl Aligned {
    fn new(len: usize) -> Self {
        let raw = vec![0u8; len + ALIGN];
        let start = raw.as_ptr().align_offset(ALIGN);
        Self { raw, start, len }
    }
    fn get(&mut self) -> &mut [u8] {
        &mut self.raw[self.start..self.start + self.len]
    }
}

/// Exclusive read/write access to a whole USB disk. Offsets and lengths of
/// any size are accepted; I/O is widened to whole sectors internally.
pub struct Sectored<D: Positioned> {
    file: D,
    size: u64,
    sector: u64,
    position: u64,
    _volumes: Vec<LockedVolume>,
}
pub type DeviceWriter = Sectored<File>;

/// Positional I/O on a device that only accepts whole sectors.
pub trait Positioned {
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize>;
    fn write_at(&self, buffer: &[u8], offset: u64) -> io::Result<usize>;
    fn sync(&mut self) -> io::Result<()>;
    /// Called once when the writer is dropped.
    fn finish(&mut self) {}
}
impl Positioned for File {
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        self.seek_read(buffer, offset)
    }
    fn write_at(&self, buffer: &[u8], offset: u64) -> io::Result<usize> {
        self.seek_write(buffer, offset)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
    fn finish(&mut self) {
        // Let Windows re-read the disk layout before the volumes unlock.
        let _ = self.sync_all();
        let _ = ioctl::<()>(self, IOCTL_DISK_UPDATE_PROPERTIES, None, &mut []);
    }
}

pub fn open_write(path: &Path) -> Result<DeviceWriter> {
    let device = resolve(path)?;
    let number = disk_number(&device)?;
    check_writable(number, &device)?;
    let mut volumes = vec![];
    for (volume, letters) in volumes_on(number) {
        volumes.push(lock_volume(&volume, &letters)?);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&device)
        .map_err(|e| access_error(&device, e))?;
    let mut buffer = vec![0u8; 256];
    let geometry = ioctl::<()>(&file, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, None, &mut buffer)
        .ok()
        .and_then(|n| read_struct::<DISK_GEOMETRY_EX>(&buffer, n));
    let sector = geometry
        .map(|g| u64::from(g.Geometry.BytesPerSector))
        .filter(|s| s.is_power_of_two() && *s >= 512)
        .unwrap_or(512);
    let mut length = vec![0u8; size_of::<GET_LENGTH_INFORMATION>()];
    let size = ioctl::<()>(&file, IOCTL_DISK_GET_LENGTH_INFO, None, &mut length)
        .ok()
        .and_then(|n| read_struct::<GET_LENGTH_INFORMATION>(&length, n))
        .and_then(|l| u64::try_from(l.Length).ok())
        .or_else(|| geometry.and_then(|g| u64::try_from(g.DiskSize).ok()))
        .filter(|s| *s > 0)
        .ok_or_else(|| format!("{device} has no capacity; is media inserted?"))?;
    Ok(Sectored {
        file,
        size,
        sector,
        position: 0,
        _volumes: volumes,
    })
}

impl<D: Positioned> Sectored<D> {
    /// Sector-aligned span covering `[position, position + len)`.
    fn span(&self, len: u64) -> (u64, u64) {
        let start = self.position - self.position % self.sector;
        let end = (self.position + len).div_ceil(self.sector) * self.sector;
        (start, end.min(self.size))
    }
    fn read_exact_at(&self, buffer: &mut [u8], mut offset: u64) -> io::Result<()> {
        let mut done = 0;
        while done < buffer.len() {
            let n = self.file.read_at(&mut buffer[done..], offset)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            done += n;
            offset += n as u64;
        }
        Ok(())
    }
    fn write_all_at(&self, buffer: &[u8], mut offset: u64) -> io::Result<()> {
        let mut done = 0;
        while done < buffer.len() {
            let n = self.file.write_at(&buffer[done..], offset)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            done += n;
            offset += n as u64;
        }
        Ok(())
    }
    pub fn sync(&mut self) -> io::Result<()> {
        self.file.sync()
    }
}

impl<D: Positioned> Read for Sectored<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let len = (buf.len() as u64).min(self.size - self.position).min(CHUNK);
        let (start, end) = self.span(len);
        let mut bounce = Aligned::new((end - start) as usize);
        self.read_exact_at(bounce.get(), start)?;
        let skip = (self.position - start) as usize;
        buf[..len as usize].copy_from_slice(&bounce.get()[skip..skip + len as usize]);
        self.position += len;
        Ok(len as usize)
    }
}

impl<D: Positioned> Write for Sectored<D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.position >= self.size {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "write beyond the end of the device",
            ));
        }
        let len = (buf.len() as u64).min(self.size - self.position).min(CHUNK);
        let (start, end) = self.span(len);
        let mut bounce = Aligned::new((end - start) as usize);
        if start != self.position || end != self.position + len {
            // Partial sectors keep their surrounding bytes.
            self.read_exact_at(bounce.get(), start)?;
        }
        let skip = (self.position - start) as usize;
        bounce.get()[skip..skip + len as usize].copy_from_slice(&buf[..len as usize]);
        self.write_all_at(bounce.get(), start)?;
        self.position += len;
        Ok(len as usize)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<D: Positioned> Seek for Sectored<D> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(n) => self.size.checked_add_signed(n),
            SeekFrom::Current(n) => self.position.checked_add_signed(n),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
        self.position = target;
        Ok(target)
    }
}

impl<D: Positioned> Drop for Sectored<D> {
    fn drop(&mut self) {
        self.file.finish();
    }
}

/// Lock and dismount every volume on the USB disk, then eject its media so
/// it is safe to unplug. Returns the drive letters that were removed.
pub fn eject(path: &Path) -> Result<Vec<char>> {
    let device = resolve(path)?;
    let number = disk_number(&device)?;
    check_writable(number, &device)?;
    let volumes = volumes_on(number);
    let letters: Vec<char> = volumes.iter().flat_map(|(_, l)| l.clone()).collect();
    let locked = volumes
        .iter()
        .map(|(volume, letters)| lock_volume(volume, letters))
        .collect::<Result<Vec<_>>>()?;
    let disk = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&device)
        .map_err(|e| access_error(&device, e))?;
    let allow = PREVENT_MEDIA_REMOVAL {
        PreventMediaRemoval: false,
    };
    let _ = ioctl(&disk, IOCTL_STORAGE_MEDIA_REMOVAL, Some(&allow), &mut []);
    ioctl::<()>(&disk, IOCTL_STORAGE_EJECT_MEDIA, None, &mut [])
        .map_err(|e| format!("volumes are dismounted, but ejecting {device} failed: {e}. Use Windows' Safely Remove Hardware before unplugging"))?;
    // Volumes stay locked until the media is gone.
    drop(locked);
    Ok(letters)
}

/// A short description of what `eject` will affect, for the review dialog.
pub fn eject_plan(path: &Path) -> Result<(String, String, Vec<char>)> {
    let device = resolve(path)?;
    let number = disk_number(&device)?;
    check_writable(number, &device)?;
    let disk = disks()
        .into_iter()
        .find(|d| d.path.eq_ignore_ascii_case(&device))
        .ok_or_else(|| format!("{device} is not connected"))?;
    let letters = volumes_on(number)
        .into_iter()
        .flat_map(|(_, l)| l)
        .collect();
    Ok((device, disk.label(), letters))
}

/// Whether this process runs with administrator rights.
pub fn is_elevated() -> bool {
    // SAFETY: no arguments.
    unsafe { IsUserAnAdmin() != 0 }
}

/// Start a new elevated copy of RustGotek (Windows shows the UAC prompt).
/// Returns false if the user declined or it could not start.
pub fn relaunch_elevated(args: &[String]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let quoted = args
        .iter()
        .map(|a| format!("\"{}\"", a.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ");
    let (verb, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&quoted));
    // SAFETY: every string is NUL-terminated UTF-16 and outlives the call.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    result as usize > 32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// An in-memory disk that, like a raw Windows disk handle, rejects any
    /// transfer that does not start and end on a sector boundary.
    struct Disk512(RefCell<Vec<u8>>);
    impl Positioned for Disk512 {
        fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
            assert!(
                offset.is_multiple_of(512) && buffer.len().is_multiple_of(512),
                "unaligned read"
            );
            let data = self.0.borrow();
            let start = offset as usize;
            buffer.copy_from_slice(&data[start..start + buffer.len()]);
            Ok(buffer.len())
        }
        fn write_at(&self, buffer: &[u8], offset: u64) -> io::Result<usize> {
            assert!(
                offset.is_multiple_of(512) && buffer.len().is_multiple_of(512),
                "unaligned write"
            );
            let start = offset as usize;
            self.0.borrow_mut()[start..start + buffer.len()].copy_from_slice(buffer);
            Ok(buffer.len())
        }
        fn sync(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn sector_writer_handles_any_offset_and_length() {
        let size = 3 * 1024 * 1024 + 512;
        let mut expected: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let mut device = Sectored {
            file: Disk512(RefCell::new(expected.clone())),
            size: size as u64,
            sector: 512,
            position: 0,
            _volumes: vec![],
        };
        // Unaligned, sector-aligned and multi-chunk writes, then reads.
        for (offset, len, value) in [
            (3usize, 10usize, 1u8),
            (512, 1024, 2),
            (1000, 1_500_000, 3),
            (size - 7, 7, 4),
        ] {
            device.seek(SeekFrom::Start(offset as u64)).unwrap();
            device.write_all(&vec![value; len]).unwrap();
            expected[offset..offset + len].fill(value);
        }
        let mut actual = vec![0; size];
        device.seek(SeekFrom::Start(0)).unwrap();
        device.read_exact(&mut actual).unwrap();
        assert!(actual == expected, "device contents differ");
        let mut middle = vec![0; 777];
        device.seek(SeekFrom::Start(1001)).unwrap();
        device.read_exact(&mut middle).unwrap();
        assert_eq!(middle, expected[1001..1778]);
        assert_eq!(device.seek(SeekFrom::End(0)).unwrap(), size as u64);
        assert_eq!(device.read(&mut [0; 4]).unwrap(), 0);
        assert!(device.write(&[1]).is_err(), "writes past the end must fail");
    }
    #[test]
    fn recognizes_device_paths_and_drive_letters() {
        for path in [
            r"\\.\PhysicalDrive1",
            r"E:",
            r"e:\",
            r"\\.\F:",
            r"\\?\PhysicalDrive2",
        ] {
            assert!(is_device(Path::new(path)), "{path}");
        }
        for path in [r"C:\images\banks.img", r"backup.img", r"\\?\C:\x.img"] {
            assert!(!is_device(Path::new(path)), "{path}");
        }
    }
}
