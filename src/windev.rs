//! Windows USB disk discovery and raw read access.
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
    io,
    mem::size_of,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Storage::FileSystem::{
        BusTypeMmc, BusTypeSd, BusTypeUsb, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
        GetLogicalDrives,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            DISK_GEOMETRY_EX, GET_LENGTH_INFORMATION, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_GET_DEVICE_NUMBER,
            IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
            STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
        },
        WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
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

#[cfg(test)]
mod tests {
    use super::*;
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
