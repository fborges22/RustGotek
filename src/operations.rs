#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Media,
    Banks,
    Slots,
    Images,
    Backups,
    Config,
    Firmware,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    Text,
    File,
    Folder,
    Save,
}
pub(crate) struct Field {
    pub(crate) label: &'static str,
    pub(crate) kind: Kind,
    pub(crate) default: &'static str,
}
pub(crate) struct Operation {
    pub(crate) page: Page,
    pub(crate) title: &'static str,
    pub(crate) command: &'static str,
    pub(crate) description: &'static str,
    pub(crate) media: bool,
    pub(crate) writes_media: bool,
    pub(crate) confirm: bool,
    pub(crate) fields: &'static [Field],
}
macro_rules! field {
    ($label:literal, $kind:ident, $default:literal) => {
        Field {
            label: $label,
            kind: Kind::$kind,
            default: $default,
        }
    };
}
macro_rules! op {
    ($page:ident, $title:literal, $cmd:literal, $desc:literal, $media:literal, $write:literal, $confirm:literal, [$($field:expr),*]) => {

        Operation { page: Page::$page, title: $title, command: $cmd, description: $desc,
            media: $media, writes_media: $write, confirm: $confirm, fields: &[$($field),*] }
    };
}
pub(crate) static OPS: &[Operation] = &[
    op!(
        Media,
        "Media report",
        "media-info",
        "Inspect image inventory and FlashFloppy configuration in the open folder.",
        true,
        false,
        false,
        []
    ),
    op!(
        Banks,
        "List connected USB disks",
        "usb-list",
        "Show connected USB disks and their device paths.",
        false,
        false,
        false,
        []
    ),
    #[cfg(any(target_os = "linux", windows))]
    op!(
        Banks,
        "Initialize USB drive",
        "usb-init",
        "For original Gotek firmware. Erases the entire selected USB drive, including partitions and files, and creates numbered blank FAT12 disks. No recovery copy is made. Unmount its volumes on Linux or run as administrator on Windows. For FlashFloppy use Initialize FlashFloppy configuration on a mounted FAT32 drive.",
        false,
        false,
        true,
        [
            field!("Whole USB drive", Text, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440"),
            field!("Number of banks (1–1000)", Text, "100")
        ]
    ),
    op!(
        Banks,
        "Inspect bank headers",
        "bank-list",
        "Choose a raw device (for example /dev/sda) or a whole-device backup. gotek1440 uses 1536 KiB spacing; gotek720 uses 1024 KiB. Confirm the layout for your model. Linux may request read authorization in a system dialog; Windows needs administrator rights. Header matches are not filesystem validation.",
        false,
        false,
        false,
        [
            field!("Device or whole-device backup", File, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440")
        ]
    ),
    op!(
        Banks,
        "Export a floppy bank",
        "bank-export",
        "Copy the selected bank to a new image without changing the source. Bank numbers start at 0. Use Disk images to inspect or edit the exported image.",
        false,
        false,
        false,
        [
            field!("Device or whole-device backup", File, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440"),
            field!("Bank number (0–999)", Text, "0"),
            field!("New floppy image", Save, "")
        ]
    ),
    op!(
        Banks,
        "Back up the whole device",
        "bank-backup",
        "Copy every byte, including all banks, padding and trailing data. The destination needs space for the entire USB capacity. Keep the source unmounted or read-only during copying.",
        false,
        false,
        false,
        [
            field!("Device or whole-device backup", File, ""),
            field!("New whole-device backup", Save, "")
        ]
    ),
    op!(
        Banks,
        "Verify a whole-device backup",
        "bank-verify",
        "Compare every byte of the device or image with its backup. Keep the source unchanged during verification.",
        false,
        false,
        false,
        [
            field!("Device or whole-device image", File, ""),
            field!("Whole-device backup", File, "")
        ]
    ),
    op!(
        Banks,
        "Replace a bank in a backup copy",
        "bank-replace",
        "Create a new whole-device image containing the replacement floppy. Preserve other banks and padding. The USB and original backup remain unchanged. Use Restore a whole-device image to write the completed backup to a matching-capacity device.",
        false,
        false,
        false,
        [
            field!("Original whole-device backup", File, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440"),
            field!("Bank number (0–999)", Text, "0"),
            field!("Replacement floppy image", File, ""),
            field!("New whole-device image", Save, "")
        ]
    ),
    op!(
        Banks,
        "Write a replacement to a bank",
        "bank-write",
        "Replace one bank on a USB device (unmounted on Linux; locked automatically on Windows) or image file. First export the selected bank as the expected original. A new recovery image is saved before writing. Confirm the target, profile and bank below; Linux may request authorization and Windows needs administrator rights. Power loss can interrupt recovery.",
        false,
        false,
        true,
        [
            field!("Target device or whole-device image", File, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440"),
            field!("Bank number (0–999)", Text, "0"),
            field!("Replacement floppy image", File, ""),
            field!("Expected original (exported bank)", File, ""),
            field!("New recovery image", Save, "")
        ]
    ),
    op!(
        Banks,
        "Restore a whole-device image",
        "device-restore",
        "Replace every byte of a USB device (unmounted on Linux; locked automatically on Windows) or existing image file. The replacement and expected original backups must exactly match its capacity. Save a new full recovery copy before writing; allow space for the entire device. Confirm all paths before proceeding.",
        false,
        false,
        true,
        [
            field!("Target device or existing whole-device image", File, ""),
            field!("Whole-device image to restore", File, ""),
            field!("Expected original whole-device backup", File, ""),
            field!("New whole-device recovery file", Save, "")
        ]
    ),
    op!(
        Banks,
        "Create a new bank layout",
        "bank-create",
        "Create a new image containing blank FAT12 floppies labelled FDISK000 onwards. Gaps and unused capacity are zero. Choose the bank count supported by your emulator. For USB restoration, enter its exact capacity from Inspect bank headers. No USB is modified by this operation.",
        false,
        false,
        false,
        [
            field!("New whole-device image", Save, ""),
            field!("Bank profile: gotek1440 or gotek720", Text, "gotek1440"),
            field!("Number of banks (1–1000)", Text, "100"),
            field!("Total capacity in bytes, or minimum", Text, "minimum")
        ]
    ),
    op!(
        Slots,
        "List assigned slots",
        "slots",
        "Show the indexed prefix, occupied slots and conflicts.",
        true,
        false,
        false,
        []
    ),
    op!(
        Slots,
        "Assign a disk image",
        "slot",
        "Copy an image to a numbered slot. Existing slots are never replaced.",
        true,
        true,
        false,
        [
            field!("Disk image", File, ""),
            field!("Slot number (0–999) or auto", Text, "auto")
        ]
    ),
    op!(
        Slots,
        "Move a slot",
        "slot-move",
        "Verify the new copy before removing the old slot name.",
        true,
        true,
        false,
        [
            field!("Current slot", Text, "0"),
            field!("New slot", Text, "1")
        ]
    ),
    op!(
        Slots,
        "Swap two slots",
        "slot-swap",
        "Exchange two indexed images by renaming them in place; no data is copied. If one slot is empty, the image simply moves there.",
        true,
        true,
        false,
        [
            field!("First slot", Text, "0"),
            field!("Second slot", Text, "1")
        ]
    ),
    op!(
        Images,
        "Create a PC floppy",
        "create-fat12",
        "Create a formatted FAT12 data image. No operating system is included.",
        false,
        false,
        false,
        [
            field!("New disk image", Save, ""),
            field!("Capacity in KiB: 360, 720, 1200, 1440, 2880", Text, "1440")
        ]
    ),
    op!(
        Images,
        "Check a FAT12 image",
        "image-check",
        "Check allocations, sector layout, file chains and directory links.",
        false,
        false,
        false,
        [field!("Disk image", File, "")]
    ),
    op!(
        Images,
        "Inspect FAT12 layout",
        "inspect-fat12",
        "Show geometry and free space.",
        false,
        false,
        false,
        [field!("Disk image", File, "")]
    ),
    op!(
        Images,
        "Inspect an HFE image",
        "inspect-hfe",
        "Check the HFE container and track extents.",
        false,
        false,
        false,
        [field!("HFE image", File, "")]
    ),
    op!(
        Images,
        "Browse an image directory",
        "image-list",
        "List FAT12 contents using DOS short names.",
        false,
        false,
        false,
        [
            field!("Disk image", File, ""),
            field!("Directory inside image", Text, "/")
        ]
    ),
    op!(
        Images,
        "Extract a file",
        "image-extract",
        "Copy a FAT12 file out of its image without mounting it.",
        false,
        false,
        false,
        [
            field!("Disk image", File, ""),
            field!("File inside image", Text, ""),
            field!("Save extracted file", Save, "")
        ]
    ),
    op!(
        Images,
        "Change disk label",
        "image-label",
        "Give this disk a recognizable DOS label (up to 11 characters). The original image is preserved.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("Disk label", Text, ""),
            field!("Save edited copy as", Save, "")
        ]
    ),
    op!(
        Images,
        "Add a file",
        "image-import",
        "Save an edited copy of a FAT12 image; the original is preserved.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("File to add", File, ""),
            field!("DOS 8.3 path inside image", Text, ""),
            field!("Save edited copy as", Save, "")
        ]
    ),
    op!(
        Images,
        "New folder",
        "image-mkdir",
        "Create a directory in a new copy of the FAT12 image.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("DOS 8.3 directory path", Text, ""),
            field!("Save edited copy as", Save, "")
        ]
    ),
    op!(
        Images,
        "Remove file or folder",
        "image-remove",
        "Remove a file or empty directory in a new copy. Original is preserved.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("Path inside image", Text, ""),
            field!("Save edited copy as", Save, "")
        ]
    ),
    op!(
        Images,
        "Rename file or folder",
        "image-rename",
        "Rename a file or directory within its parent in a new copy.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("Path inside image", Text, ""),
            field!("New DOS 8.3 name", Text, ""),
            field!("Save edited copy as", Save, "")
        ]
    ),
    op!(
        Images,
        "Convert raw image to HFE",
        "convert-hfe",
        "Encode PC sectors as HFE v1 IBM MFM tracks.",
        false,
        false,
        false,
        [
            field!("Raw image", File, ""),
            field!("New HFE image", Save, ""),
            field!("Profile: pc360, pc720, pc1200, pc1440", Text, "pc1440")
        ]
    ),
    op!(
        Images,
        "Convert HFE to raw image",
        "convert-raw",
        "Decode standard PC HFE v1 sectors, checking IDs and CRCs.",
        false,
        false,
        false,
        [
            field!("HFE image", File, ""),
            field!("New raw image", Save, ""),
            field!("Profile: pc360, pc720, pc1200, pc1440", Text, "pc1440")
        ]
    ),
    op!(
        Backups,
        "Back up media files",
        "backup",
        "Copy and verify the mounted filesystem. This does not back up hidden raw disk banks or boot sectors.",
        true,
        false,
        false,
        [field!(
            "New backup directory (must not already exist)",
            Save,
            ""
        )]
    ),
    op!(
        Backups,
        "Verify a backup",
        "verify",
        "Compare directory trees and every file byte.",
        true,
        false,
        false,
        [field!("Existing backup directory", Folder, "")]
    ),
    op!(
        Backups,
        "Restore to a new directory",
        "restore",
        "Copy and verify a backup into a new directory. Existing directories are never merged or overwritten.",
        false,
        false,
        false,
        [
            field!("Backup directory", Folder, ""),
            field!("New destination directory", Save, "")
        ]
    ),
    op!(
        Config,
        "View FlashFloppy settings",
        "config-show",
        "Read the active FF.CFG, including the FF subdirectory when present.",
        true,
        false,
        false,
        []
    ),
    op!(
        Config,
        "Change a setting",
        "config-set",
        "Save a numbered backup before updating one option. Values depend on your firmware version.",
        true,
        true,
        false,
        [
            field!("Setting name", Text, "nav-mode"),
            field!("Value", Text, "indexed")
        ]
    ),
    op!(
        Config,
        "Initialize FlashFloppy configuration",
        "init",
        "Create FF.CFG only when no active configuration exists. This does not install firmware.",
        true,
        true,
        false,
        [field!("Navigation mode: native or indexed", Text, "native")]
    ),
    op!(
        Firmware,
        "Inspect an update",
        "firmware-inspect",
        "Validate FlashFloppy update checksums and show MCU payloads. CRCs do not prove authenticity.",
        false,
        false,
        false,
        [field!("Update file (.upd)", File, "")]
    ),
    op!(
        Firmware,
        "Stage a firmware update",
        "firmware-stage",
        "Copy a trusted FlashFloppy update to USB for an explicit MCU. The device is not flashed by this application.",
        true,
        true,
        true,
        [
            field!("Update file (.upd)", File, ""),
            field!("MCU: stm32f105, at32f415 or at32f435", Text, "")
        ]
    ),
];

pub(crate) fn requires_write_flag(command: &str) -> bool {
    matches!(
        command,
        "usb-init" | "bank-write" | "device-restore" | "bank-swap" | "bank-format" | "image-format"
    )
}
