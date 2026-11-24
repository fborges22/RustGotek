//! Safe USB removal: UDisks on Linux (sharing the GUI's authorization
//! identity); volume lock, dismount and media eject on Windows.
use crate::Result;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Drive {
    path: String,
    id: String,
    detected: u64,
    name: String,
    bus: String,
    sibling: String,
    can_power_off: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Volume {
    path: String,
    drive: String,
    number: u64,
    device: String,
    mounts: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    drive: String,
    number: u64,
    drives: Vec<Drive>,
    volumes: Vec<Volume>,
}
impl Plan {
    pub fn description(&self) -> String {
        let names = self
            .drives
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let volumes = self
            .volumes
            .iter()
            .map(|v| {
                format!(
                    "{}{}",
                    v.device,
                    if v.mounts.is_empty() {
                        " (unmounted)".into()
                    } else {
                        format!(" — {}", v.mounts.join(", "))
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if cfg!(windows) {
            format!(
                "Safely eject {names}\n{volumes}\nAll volumes on this USB disk will be locked and dismounted, then its media is ejected. Close File Explorer windows and files open on it first."
            )
        } else {
            format!(
                "Safely eject {names}\n{volumes}\nAll volumes on this USB device will be unmounted, then the device will be powered off. Close files open on it first."
            )
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn make_plan(number: u64, drives: &[Drive], volumes: &[Volume]) -> Result<Plan> {
    let volume = volumes
        .iter()
        .find(|v| v.number == number)
        .ok_or("The source is not a USB block device or mounted USB folder")?;
    let drive = drives
        .iter()
        .find(|d| d.path == volume.drive)
        .ok_or("Cannot identify the physical USB drive")?;
    if drive.bus != "usb" || !drive.can_power_off {
        return Err("The selected drive is not a USB device that supports power off".into());
    }
    let mut affected: Vec<_> = drives
        .iter()
        .filter(|d| {
            d.path == drive.path || (!drive.sibling.is_empty() && d.sibling == drive.sibling)
        })
        .cloned()
        .collect();
    if affected.iter().any(|d| d.bus != "usb") {
        return Err("Cannot safely identify all drives sharing this USB device".into());
    }
    affected.sort();
    let mut volumes: Vec<_> = volumes
        .iter()
        .filter(|v| affected.iter().any(|d| d.path == v.drive))
        .cloned()
        .collect();
    if volumes.iter().any(|v| v.mounts.iter().any(|m| m == "/")) {
        return Err("Cannot eject the running system drive".into());
    }
    volumes.sort_by(|a, b| {
        let depth = |v: &Volume| v.mounts.iter().map(|p| p.len()).max().unwrap_or(0);
        depth(b).cmp(&depth(a)).then_with(|| a.path.cmp(&b.path))
    });
    Ok(Plan {
        drive: drive.path.clone(),
        number,
        drives: affected,
        volumes,
    })
}

/// Connected USB disks usable as a bank source: `(path, description)`.
pub fn detect() -> Vec<(String, String)> {
    #[cfg(windows)]
    {
        crate::windev::usb_disks()
            .into_iter()
            .map(|d| (d.path.clone(), d.label()))
            .collect()
    }
    #[cfg(target_os = "linux")]
    {
        let mut found: Vec<(String, String)> = std::fs::read_dir("/dev/disk/by-id")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("usb-") || name.contains("-part") {
                    return None;
                }
                let device = std::fs::canonicalize(entry.path()).ok()?;
                let device = device.to_string_lossy().into_owned();
                Some((device.clone(), format!("{device} — {name}")))
            })
            .collect();
        found.sort();
        found.dedup_by(|a, b| a.0 == b.0);
        found
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

pub fn prepare(source: &Path) -> Result<Plan> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let metadata = std::fs::metadata(source)?;
        let number = if metadata.file_type().is_block_device() {
            metadata.rdev()
        } else if metadata.is_dir() {
            metadata.dev()
        } else {
            return Err("Select a raw USB device or mounted USB folder, not an image file".into());
        };
        let (drives, volumes) = linux::inventory()?;
        make_plan(number, &drives, &volumes)
    }
    #[cfg(windows)]
    {
        let (device, name, letters) = crate::windev::eject_plan(source)?;
        Ok(Plan {
            drive: device.clone(),
            number: 0,
            drives: vec![Drive {
                path: device.clone(),
                id: String::new(),
                detected: 0,
                name,
                bus: "usb".into(),
                sibling: String::new(),
                can_power_off: true,
            }],
            volumes: letters
                .into_iter()
                .map(|l| Volume {
                    path: format!("{l}:"),
                    drive: device.clone(),
                    number: 0,
                    device: format!("{l}:"),
                    mounts: vec![format!(r"{l}:\")],
                })
                .collect(),
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = source;
        Err("Safe ejection requires Linux or Windows; use your system's eject action".into())
    }
}

pub fn power_off(plan: &Plan) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let (drives, volumes) = linux::inventory()?;
        if &make_plan(plan.number, &drives, &volumes)? != plan {
            return Err(
                "USB device or mounts changed; review Safely eject USB again. Nothing unmounted"
                    .into(),
            );
        }
        run_steps(plan, |path, interface, method| {
            let (drives, volumes) = linux::inventory()?;
            let current = make_plan(plan.number, &drives, &volumes)?;
            if current.drives != plan.drives {
                return Err("USB identity changed; ejection stopped".into());
            }
            if method == "PowerOff" && current.volumes.iter().any(|v| !v.mounts.is_empty()) {
                return Err("USB still has mounted volumes; ejection stopped".into());
            }
            linux::call(path, interface, method)
        })
    }
    #[cfg(windows)]
    {
        // Re-identify the disk: a different USB may now use the same number.
        let current = prepare(Path::new(&plan.drive))?;
        if current.drives != plan.drives {
            return Err("USB device changed; review Safely eject USB again".into());
        }
        crate::windev::eject(Path::new(&plan.drive)).map(|_| ())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = plan;
        Err("Safe ejection requires Linux or Windows".into())
    }
}

#[cfg(any(target_os = "linux", test))]
fn run_steps(plan: &Plan, mut call: impl FnMut(&str, &str, &str) -> Result<()>) -> Result<()> {
    for volume in &plan.volumes {
        if !volume.mounts.is_empty() {
            call(&volume.path, "org.freedesktop.UDisks2.Filesystem", "Unmount")
                .map_err(|e| format!("Could not unmount {}: {e}. USB was not powered off; some earlier volumes may already be unmounted", volume.device))?;
        }
    }
    call(&plan.drive, "org.freedesktop.UDisks2.Drive", "PowerOff")
        .map_err(|e| format!("USB power off did not complete: {e}. Volumes may be unmounted, but safe removal has not been confirmed").into())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::{BTreeSet, HashMap};
    use zbus::{
        blocking::Proxy,
        proxy::MethodFlags,
        zvariant::{OwnedValue, Value},
    };
    type Props = HashMap<String, OwnedValue>;
    fn string(p: &Props, name: &str) -> String {
        p.get(name)
            .and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or("")
            .into()
    }
    fn number(p: &Props, name: &str) -> u64 {
        p.get(name).and_then(|v| u64::try_from(v).ok()).unwrap_or(0)
    }
    fn path(p: &Props, name: &str) -> String {
        p.get(name)
            .and_then(|v| <&zbus::zvariant::ObjectPath<'_>>::try_from(v).ok())
            .map(|v| v.as_str().into())
            .unwrap_or_default()
    }
    fn text(bytes: Vec<u8>) -> String {
        String::from_utf8_lossy(bytes.strip_suffix(&[0]).unwrap_or(&bytes)).into_owned()
    }
    pub fn inventory() -> Result<(Vec<Drive>, Vec<Volume>)> {
        let connection = crate::banks::linux_system_connection()?;
        let manager = Proxy::new(
            &connection,
            "org.freedesktop.UDisks2",
            "/org/freedesktop/UDisks2",
            "org.freedesktop.DBus.ObjectManager",
        )?;
        let objects: zbus::fdo::ManagedObjects = manager.call("GetManagedObjects", &())?;
        let mut drives = vec![];
        let mut volumes = vec![];
        for (object, interfaces) in &objects {
            if let Some(p) = interfaces.get("org.freedesktop.UDisks2.Drive") {
                drives.push(Drive {
                    path: object.to_string(),
                    id: string(p, "Id"),
                    detected: number(p, "TimeDetected"),
                    name: format!(
                        "{} {} ({})",
                        string(p, "Vendor"),
                        string(p, "Model"),
                        string(p, "Serial")
                    ),
                    bus: string(p, "ConnectionBus"),
                    sibling: string(p, "SiblingId"),
                    can_power_off: p
                        .get("CanPowerOff")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(false),
                });
            }
            if let Some(p) = interfaces.get("org.freedesktop.UDisks2.Block") {
                let mut drive = path(p, "Drive");
                let mut backing = path(p, "CryptoBackingDevice");
                let mut seen = BTreeSet::new();
                while drive == "/" && backing != "/" && seen.insert(backing.clone()) {
                    let Some(p) = objects
                        .iter()
                        .find(|(k, _)| k.as_str() == backing)
                        .and_then(|(_, i)| i.get("org.freedesktop.UDisks2.Block"))
                    else {
                        break;
                    };
                    drive = path(p, "Drive");
                    backing = path(p, "CryptoBackingDevice");
                }
                let mounts = interfaces
                    .get("org.freedesktop.UDisks2.Filesystem")
                    .and_then(|p| p.get("MountPoints"))
                    .map(|v| -> Result<Vec<String>> {
                        Ok(Vec::<Vec<u8>>::try_from(v.try_clone()?)?
                            .into_iter()
                            .map(text)
                            .collect())
                    })
                    .transpose()?
                    .unwrap_or_default();
                let device = p
                    .get("PreferredDevice")
                    .map(|v| -> Result<String> { Ok(text(Vec::<u8>::try_from(v.try_clone()?)?)) })
                    .transpose()?
                    .unwrap_or_else(|| object.to_string());
                volumes.push(Volume {
                    path: object.to_string(),
                    drive,
                    number: number(p, "DeviceNumber"),
                    device,
                    mounts,
                });
            }
        }
        Ok((drives, volumes))
    }
    pub fn call(path: &str, interface: &str, method: &str) -> Result<()> {
        let connection = crate::banks::linux_system_connection()?;
        let proxy = Proxy::new(&connection, "org.freedesktop.UDisks2", path, interface)?;
        let options: HashMap<&str, Value<'_>> = HashMap::new(); // Never force-unmount a busy filesystem.
        let reply: Option<()> = proxy.call_with_flags(
            method,
            MethodFlags::AllowInteractiveAuth.into(),
            &(options,),
        )?;
        reply.ok_or_else(|| "UDisks did not confirm the operation".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn drive(path: &str, bus: &str, sibling: &str) -> Drive {
        Drive {
            path: path.into(),
            id: path.into(),
            detected: 42,
            name: path.into(),
            bus: bus.into(),
            sibling: sibling.into(),
            can_power_off: true,
        }
    }
    fn volume(path: &str, drive: &str, number: u64, mounts: &[&str]) -> Volume {
        Volume {
            path: path.into(),
            drive: drive.into(),
            number,
            device: format!("/dev/{path}"),
            mounts: mounts.iter().map(|m| (*m).into()).collect(),
        }
    }
    #[test]
    fn unmounts_all_usb_volumes_and_siblings_before_power_off() {
        let drives = vec![
            drive("usb", "usb", "reader"),
            drive("slot2", "usb", "reader"),
            drive("internal", "ata", ""),
        ];
        let volumes = vec![
            volume("a", "usb", 1, &["/media/USB"]),
            volume("b", "usb", 2, &["/media/USB/nested"]),
            volume("c", "slot2", 3, &["/media/CARD"]),
            volume("system", "internal", 4, &["/"]),
        ];
        let plan = make_plan(1, &drives, &volumes).unwrap();
        assert_eq!(plan.drives.len(), 2);
        assert_eq!(plan.volumes.len(), 3);
        let mut calls = vec![];
        run_steps(&plan, |path, _, method| {
            calls.push((path.to_string(), method.to_string()));
            Ok(())
        })
        .unwrap();
        assert_eq!(calls[0], ("b".into(), "Unmount".into()));
        assert_eq!(calls.last().unwrap(), &("usb".into(), "PowerOff".into()));
        assert_eq!(calls.len(), 4);
        assert!(plan.description().contains("/media/CARD"));
    }
    #[test]
    fn busy_volume_stops_ejection_and_poweroff_failure_is_not_success() {
        let plan = make_plan(
            1,
            &[drive("usb", "usb", "")],
            &[volume("a", "usb", 1, &["/media/USB"])],
        )
        .unwrap();
        let mut count = 0;
        let result = run_steps(&plan, |_, _, _| {
            count += 1;
            Err("DeviceBusy".into())
        });
        assert_eq!(count, 1);
        assert!(result.unwrap_err().to_string().contains("not powered off"));
        let result = run_steps(&plan, |_, _, method| {
            if method == "PowerOff" {
                Err("NotAuthorized".into())
            } else {
                Ok(())
            }
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("safe removal has not been confirmed")
        );
    }
    #[test]
    fn rejects_non_usb_system_and_unsupported_drives() {
        let volume = volume("a", "drive", 1, &[]);
        assert!(
            make_plan(
                1,
                &[drive("drive", "ata", "")],
                std::slice::from_ref(&volume)
            )
            .is_err()
        );
        let mut usb = drive("drive", "usb", "");
        usb.can_power_off = false;
        assert!(make_plan(1, &[usb.clone()], std::slice::from_ref(&volume)).is_err());
        usb.can_power_off = true;
        let mut system = volume;
        system.mounts.push("/".into());
        assert!(make_plan(1, &[usb], &[system]).is_err());
    }
    #[test]
    fn unmounted_drive_goes_straight_to_power_off_and_identity_changes_invalidate_plan() {
        let drives = vec![drive("usb", "usb", "")];
        let volumes = vec![volume("a", "usb", 1, &[])];
        let plan = make_plan(1, &drives, &volumes).unwrap();
        let mut methods = vec![];
        run_steps(&plan, |_, _, method| {
            methods.push(method.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(methods, ["PowerOff"]);
        let mut replacement = drives;
        replacement[0].detected += 1;
        assert_ne!(plan, make_plan(1, &replacement, &volumes).unwrap());
    }
    #[test]
    fn image_file_is_rejected_without_contacting_or_ejecting_a_device() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(prepare(file.path()).is_err());
    }
}
