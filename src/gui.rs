mod appearance;
mod chrome;
mod visual;
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};
use visual::Icon;

pub fn launch(bank_source: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_resizable(true)
            .with_app_id("org.rustgotek.RustGotek")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 640.0])
            .with_icon(Arc::new(visual::app_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "RustGotek — Disk workspace",
        options,
        Box::new(move |cc| {
            visual::apply(&cc.egui_ctx);
            appearance::watch_system_theme(&cc.egui_ctx);
            let mut app = App::new(bank_source, &cc.egui_ctx);
            app.appearance = cc
                .storage
                .and_then(|s| s.get_string("appearance"))
                .map(|s| visual::Appearance::parse(&s))
                .unwrap_or_default();
            cc.egui_ctx.set_theme(app.appearance.preference());
            Ok(Box::new(app))
        }),
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Media,
    Banks,
    Slots,
    Images,
    Backups,
    Config,
    Firmware,
}
#[derive(Debug, Clone, Copy)]
enum Kind {
    Text,
    File,
    Folder,
    Save,
}
struct Field {
    label: &'static str,
    kind: Kind,
    default: &'static str,
}
struct Operation {
    page: Page,
    title: &'static str,
    command: &'static str,
    description: &'static str,
    media: bool,
    writes_media: bool,
    confirm: bool,
    fields: &'static [Field],
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
static OPS: &[Operation] = &[
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
        "Inspect bank headers",
        "bank-list",
        "Choose a raw device (for example /dev/sda) or a whole-device backup. gotek1440 uses 1536 KiB spacing; gotek720 uses 1024 KiB. Confirm the layout for your model. Linux may request read authorization in a system dialog. Header matches are not filesystem validation.",
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
        "Replace one bank on an unmounted Linux device or image file. First export the selected bank as the expected original. A new recovery image is saved before writing. Confirm the target, profile and bank below; Linux may also request authorization. Power loss can interrupt recovery.",
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
        "Replace every byte of an unmounted Linux device or existing image file. The replacement and expected original backups must exactly match its capacity. Save a new full recovery copy before writing; allow space for the entire device. Confirm all paths before proceeding.",
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
        true,
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
        true,
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
            field!("Save edited copy as", Save, ""),
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

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    relative: String,
    directory: bool,
    size: u64,
}
#[derive(Debug, Clone)]
struct Listing {
    root: PathBuf,
    directory: PathBuf,
    entries: Vec<Entry>,
    read_only: bool,
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum Workspace {
    Images,
    Banks,
}
#[derive(Debug, Clone)]
struct DiskRow {
    key: String,
    name: String,
    kind: String,
    size: u64,
    path: PathBuf,
    bank: Option<u16>,
    directory: bool,
    recognized: bool,
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum Sort {
    Bank,
    Name,
    Kind,
    Size,
}
#[derive(Clone)]
struct Preview {
    target: PathBuf,
    bank: Option<(String, u16)>,
    image: PathBuf,
    directory: String,
    entries: Vec<crate::fat12::DiskEntry>,
    temp: Option<Arc<tempfile::TempDir>>,
    selected: Option<String>,
    checked: BTreeSet<String>,
    title: String,
    label: String,
}
#[derive(Clone)]
struct Form {
    title: String,
    description: String,
    command: String,
    labels: Vec<String>,
    kinds: Vec<Kind>,
    values: Vec<String>,
    media: bool,
    writes_media: bool,
    confirm: bool,
    review: bool,
    keepalive: Option<Arc<tempfile::TempDir>>,
    editor: Option<Preview>,
    batch: Option<Vec<DiskEdit>>,
    eject: Option<crate::usb::Plan>,
    /// Plain-language review lines shown instead of the raw field list.
    summary: Vec<String>,
}
impl Form {
    fn operation(op: &Operation) -> Self {
        Self {
            title: op.title.into(),
            description: op.description.into(),
            command: op.command.into(),
            labels: op.fields.iter().map(|f| f.label.into()).collect(),
            kinds: op.fields.iter().map(|f| f.kind).collect(),
            values: op.fields.iter().map(|f| f.default.into()).collect(),
            media: op.media,
            writes_media: op.writes_media,
            confirm: op.confirm,
            review: false,
            keepalive: None,
            editor: None,
            batch: None,
            eject: None,
            summary: vec![],
        }
    }
    fn bind_editor(&mut self, preview: &Preview) {
        self.values[0] = preview.image.to_string_lossy().into_owned();
        *self.values.last_mut().unwrap() = "direct-save".into();
        self.description = format!(
            "Save changes directly to {}{}. No backup image is created.",
            preview.target.display(),
            preview
                .bank
                .as_ref()
                .map(|(_, n)| format!(" • bank {n:03}"))
                .unwrap_or_default()
        );
        self.keepalive = preview.temp.clone();
        self.editor = Some(preview.clone());
    }
    fn args(&self, root: &str) -> Result<Vec<String>, String> {
        if self.media && root.is_empty() {
            return Err("Open a media folder first".into());
        }
        if self.values.len() != self.labels.len() || self.values.iter().any(|s| s.trim().is_empty())
        {
            return Err("Complete all fields before continuing".into());
        }
        match self.command.as_str() {
            "image-label" => {
                crate::fat12::encode_label(&self.values[1]).map_err(|e| e.to_string())?;
            }
            "image-import" | "image-mkdir" | "image-rename" => {
                let index = if self.command == "image-mkdir" { 1 } else { 2 };
                let name = self.values[index].rsplit('/').next().unwrap_or("");
                if self.command == "image-rename" && self.values[index].contains('/') {
                    return Err("Enter a name, without a directory path".into());
                }
                crate::fat12::validate_short_name(name).map_err(|_| "Use a DOS name: up to 8 characters plus a 3-character extension, for example NOTES.TXT".to_string())?;
            }
            "bank-format" | "image-format" => {
                let label = &self.values[if self.command == "bank-format" { 3 } else { 1 }];
                if label != "-" {
                    crate::fat12::encode_label(label).map_err(|e| e.to_string())?;
                }
            }
            "bank-swap" | "slot-swap" => {
                let (a, b) = if self.command == "bank-swap" {
                    (&self.values[2], &self.values[3])
                } else {
                    (&self.values[0], &self.values[1])
                };
                let parse = |v: &String| v.trim().parse::<u16>().ok().filter(|n| *n <= 999);
                match (parse(a), parse(b)) {
                    (Some(a), Some(b)) if a != b => {}
                    (Some(_), Some(_)) => return Err("Choose two different numbers".into()),
                    _ => return Err("Enter numbers from 0 to 999".into()),
                }
            }
            _ => {}
        }
        let mut args = vec![self.command.clone()];
        if self.media {
            args.push(root.into());
        }
        args.extend(self.values.clone());
        if matches!(
            self.command.as_str(),
            "bank-write" | "device-restore" | "bank-swap" | "bank-format" | "image-format"
        ) {
            args.push("--confirm-write".into());
        }
        Ok(args)
    }
}
enum Event {
    Listed(Listing),
    Banks(crate::banks::Catalog, String, String),
    Preview(Preview),
    Prepared(Form),
    FormFailed(Form, String),
    Done(String),
    Edited(Preview, String),
    Picked(usize, Option<PathBuf>),
    Folder(Option<PathBuf>),
    Source(Option<PathBuf>),
    Ejected,
    Cancelled(String),
}
struct App {
    appearance: visual::Appearance,
    workspace: Workspace,
    root: String,
    source: String,
    profile: String,
    /// USB disks found by the last detection: `(path, description)`.
    detected: Vec<(String, String)>,
    listing: Option<Listing>,
    bank_catalog: Option<crate::banks::Catalog>,
    bank_identity: Option<(String, String)>,
    rows: Vec<DiskRow>,
    selected: BTreeSet<String>,
    /// Bank row under the pointer, used as the target for dropped images.
    drop_row: Option<String>,
    filter: String,
    show_all: bool,
    sort: Sort,
    descending: bool,
    preview: Option<Preview>,
    form: Option<Form>,
    busy: bool,
    status: String,
    error: bool,
    log: String,
    show_log: bool,
    sender: mpsc::Sender<Result<Event, String>>,
    receiver: mpsc::Receiver<Result<Event, String>>,
}
impl App {
    fn new(bank_source: Option<String>, ctx: &egui::Context) -> Self {
        let (sender, receiver) = mpsc::channel();
        let root = discover_media()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut app = Self {
            appearance: visual::Appearance::System,
            workspace: if bank_source.is_some() {
                Workspace::Banks
            } else {
                Workspace::Images
            },
            root,
            source: bank_source.clone().unwrap_or_else(|| {
                if cfg!(target_os = "linux") {
                    "/dev/sda".into()
                } else if cfg!(windows) {
                    crate::usb::detect()
                        .into_iter()
                        .next()
                        .map(|(path, _)| path)
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            }),
            profile: "gotek1440".into(),
            detected: vec![],
            listing: None,
            bank_catalog: None,
            bank_identity: None,
            rows: vec![],
            selected: BTreeSet::new(),
            drop_row: None,
            filter: String::new(),
            show_all: false,
            sort: if bank_source.is_some() {
                Sort::Bank
            } else {
                Sort::Name
            },
            descending: false,
            preview: None,
            form: None,
            busy: false,
            status: "Open an image folder or scan your Gotek banks".into(),
            error: false,
            log: String::new(),
            show_log: false,
            sender,
            receiver,
        };
        if app.workspace == Workspace::Images && !app.root.is_empty() {
            app.refresh(ctx);
        }
        app
    }
    fn worker(
        &mut self,
        ctx: &egui::Context,
        status: &str,
        f: impl FnOnce() -> Result<Event, String> + Send + 'static,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = false;
        self.status = status.into();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| {
                    Err("Operation failed unexpectedly; check the result before retrying".into())
                });
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.receiver.try_recv() {
            self.busy = false;
            match result {
                Err(error) => {
                    self.error = true;
                    self.status = "Operation failed".into();
                    self.log = error;
                    self.show_log = true;
                }
                Ok(Event::FormFailed(mut form, error)) => {
                    form.review = false;
                    self.form = Some(form);
                    self.error = true;
                    self.status = "Operation failed — correct the fields and try again".into();
                    self.log = error;
                    self.show_log = true;
                }
                Ok(Event::Cancelled(status)) => {
                    self.error = false;
                    self.status = status;
                }
                Ok(Event::Ejected) => {
                    self.clear();
                    self.root.clear();
                    self.source.clear();
                    self.form = None;
                    self.error = false;
                    self.status = "USB powered off — safe to unplug".into();
                    self.log = self.status.clone();
                    self.show_log = true;
                }
                Ok(Event::Listed(listing)) => {
                    self.root = listing.root.to_string_lossy().into_owned();
                    self.rows = listing
                        .entries
                        .iter()
                        .map(|e| DiskRow {
                            key: e.relative.clone(),
                            name: e.name.clone(),
                            kind: if e.directory {
                                "Folder".into()
                            } else {
                                file_kind(&e.name)
                            },
                            size: e.size,
                            path: listing.root.join(&e.relative),
                            bank: None,
                            directory: e.directory,
                            recognized: is_image(&e.name),
                        })
                        .collect();
                    self.listing = Some(listing);
                    self.selected
                        .retain(|k| self.rows.iter().any(|r| &r.key == k));
                    self.status = format!("{} items loaded", self.rows.len());
                }
                Ok(Event::Banks(catalog, source, profile)) => {
                    self.rows = catalog
                        .rows
                        .iter()
                        .map(|b| DiskRow {
                            key: format!("bank:{:03}", b.number),
                            name: if b.label.is_empty() {
                                format!("Bank {:03}", b.number)
                            } else {
                                b.label.clone()
                            },
                            kind: b.status.clone(),
                            size: b.size,
                            path: PathBuf::from(&source),
                            bank: Some(b.number),
                            directory: false,
                            recognized: b.recognized,
                        })
                        .collect();
                    self.status = format!(
                        "{} recognizable disks · {} possible positions · {} device",
                        self.rows.iter().filter(|r| r.recognized).count(),
                        self.rows.len(),
                        format_size(catalog.size)
                    );
                    self.bank_catalog = Some(catalog);
                    self.bank_identity = Some((source, profile));
                    self.selected
                        .retain(|k| self.rows.iter().any(|r| &r.key == k));
                }
                Ok(Event::Prepared(form)) => {
                    self.form = Some(form);
                    self.status = "Ready".into();
                }
                Ok(Event::Preview(preview)) => {
                    self.status = format!("{} entries in {}", preview.entries.len(), preview.title);
                    self.preview = Some(preview);
                }
                Ok(Event::Edited(preview, output)) => {
                    self.preview = Some(preview);
                    self.log = output;
                    self.status = "Disk saved".into();
                    self.show_log = true;
                    self.refresh(ctx);
                }
                Ok(Event::Done(output)) => {
                    self.log = output;
                    self.status = "Operation completed".into();
                    self.show_log = true;
                    self.refresh(ctx);
                }
                Ok(Event::Picked(i, path)) => {
                    if let (Some(form), Some(path)) = (&mut self.form, path) {
                        if form.command == "image-import"
                            && i == 1
                            && (form.values[2].is_empty() || form.values[2].ends_with('/'))
                            && let Some(name) = path.file_name()
                        {
                            form.values[2].push_str(&suggest_dos_name(&name.to_string_lossy()));
                        }
                        form.values[i] = path.to_string_lossy().into_owned();
                        form.review = false;
                    }
                    self.status = "Ready".into();
                }
                Ok(Event::Folder(path)) => {
                    if let Some(path) = path {
                        self.root = path.to_string_lossy().into_owned();
                        self.workspace = Workspace::Images;
                        self.clear();
                        self.refresh(ctx);
                    } else {
                        self.status = "Ready".into();
                    }
                }
                Ok(Event::Source(path)) => {
                    if let Some(path) = path {
                        self.source = path.to_string_lossy().into_owned();
                        self.clear();
                    }
                    self.status = "Ready to scan".into();
                }
            }
        }
    }
    fn clear(&mut self) {
        self.rows.clear();
        self.selected.clear();
        self.preview = None;
        self.listing = None;
        self.bank_catalog = None;
        self.bank_identity = None;
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        if self.workspace == Workspace::Images {
            if self.root.is_empty() {
                return;
            }
            let root = PathBuf::from(&self.root);
            let directory = self
                .listing
                .as_ref()
                .filter(|l| l.root == root)
                .map(|l| l.directory.clone())
                .unwrap_or_else(|| root.clone());
            self.worker(ctx, "Reading folder…", move || {
                read_listing(&root, &directory).map(Event::Listed)
            });
        } else if self.bank_identity.as_ref() == Some(&(self.source.clone(), self.profile.clone()))
        {
            self.scan(ctx);
        }
    }
    fn scan(&mut self, ctx: &egui::Context) {
        let source = self.source.clone();
        let profile = self.profile.clone();
        self.worker(
            ctx,
            "Scanning bank headers… Linux may request authorization",
            move || {
                crate::banks::catalog(Path::new(&source), &profile)
                    .map(|c| Event::Banks(c, source, profile))
                    .map_err(|e| e.to_string())
            },
        );
    }
    fn visible(&self) -> Vec<usize> {
        let filter = self.filter.to_lowercase();
        let mut indices: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                (self.show_all || r.directory || r.recognized)
                    && (r.name.to_lowercase().contains(&filter)
                        || r.key.to_lowercase().contains(&filter)
                        || r.kind.to_lowercase().contains(&filter))
            })
            .map(|(i, _)| i)
            .collect();
        indices.sort_by(|a, b| {
            let a = &self.rows[*a];
            let b = &self.rows[*b];
            let order = match self.sort {
                Sort::Bank => a.bank.cmp(&b.bank),
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::Size => a.size.cmp(&b.size),
                Sort::Kind => a.kind.cmp(&b.kind),
            };
            let order = if self.descending {
                order.reverse()
            } else {
                order
            };
            b.directory.cmp(&a.directory).then(order)
        });
        indices
    }
    fn selection(&self) -> Vec<DiskRow> {
        self.rows
            .iter()
            .filter(|r| self.selected.contains(&r.key))
            .cloned()
            .collect()
    }
    fn open_form(&mut self, command: &str) {
        if let Some(op) = OPS.iter().find(|o| o.command == command) {
            self.error = false;
            let mut form = Form::operation(op);
            let selected = self.selection();
            if command == "device-restore" {
                form.values[0] = self.source.clone();
            }
            if command == "slot"
                && let Some(row) = selected
                    .first()
                    .filter(|r| !r.directory && r.bank.is_none())
            {
                form.values[0] = row.path.to_string_lossy().into_owned();
            }
            if command == "bank-create" {
                form.values[1] = self.profile.clone();
            }
            if command == "create-fat12"
                && self.workspace == Workspace::Banks
                && self.profile == "gotek720"
            {
                form.values[1] = "720".into();
            }
            if command.starts_with("bank-") && !matches!(command, "bank-create") {
                if !form.values.is_empty() {
                    form.values[0] = self.source.clone();
                }
                if form.values.len() > 1 && form.labels[1].contains("profile") {
                    form.values[1] = self.profile.clone();
                }
                if form.values.len() > 2
                    && form.labels[2].contains("Bank number")
                    && let Some(bank) = selected.first().and_then(|r| r.bank)
                {
                    form.values[2] = bank.to_string();
                }
            } else if !op.media
                && !matches!(
                    command,
                    "create-fat12" | "create" | "restore" | "device-restore"
                )
                && let Some(row) = selected.first()
                && row.bank.is_none()
                && !row.directory
                && !form.values.is_empty()
            {
                form.values[0] = row.path.to_string_lossy().into_owned();
            }
            self.form = Some(form);
        }
    }
    fn open_disk(&mut self, ctx: &egui::Context, row: DiskRow) {
        if row.is_hfe() {
            let image = row.path.to_string_lossy().into_owned();
            self.worker(ctx, "Inspecting HFE tracks…", move || {
                backend(vec!["inspect-hfe".into(), image]).map(Event::Done)
            });
            return;
        }
        if row.directory {
            let root = PathBuf::from(&self.root);
            self.selected.clear();
            self.worker(ctx, "Opening folder…", move || {
                read_listing(&root, &row.path).map(Event::Listed)
            });
            return;
        }
        let profile = self.profile.clone();
        self.worker(ctx, "Opening disk contents…", move || {
            open_editor(&row, &profile).map(Event::Preview)
        });
    }
    fn export_selected(&mut self, ctx: &egui::Context) {
        let rows = self.selection();
        let profile = self.profile.clone();
        self.worker(ctx, "Choose where to export the selected disks…", move || {
            let names: Vec<_> = rows.iter().map(|r| r.bank.map(|n| format!("bank{n:03}.img")).unwrap_or_else(|| r.name.clone())).collect();
            if rows.len() == 1 {
                let row = &rows[0];
                let Some(dest) = rfd::FileDialog::new().set_title("Export disk — choose a new file")
                    .set_file_name(&names[0]).save_file() else { return Ok(Event::Done("Export cancelled".into())); };
                export_row(row, &profile, &dest)?;
                return Ok(Event::Done(format!("Exported {} to {}", row.name, dest.display())));
            }
            let Some(dest) = rfd::FileDialog::new().set_title("Export selected disks to folder").pick_folder()
                else { return Ok(Event::Done("Export cancelled".into())); };
            if names.iter().any(|n| dest.join(n).exists()) {
                return Err("An export destination already exists; choose another folder. Nothing exported.".into());
            }
            for (i, (row, name)) in rows.iter().zip(&names).enumerate() {
                export_row(row, &profile, &dest.join(name)).map_err(|e| format!("Export stopped after {i} disks: {e}"))?;
            }
            Ok(Event::Done(format!("Exported {} disks to {}", rows.len(), dest.display())))
        });
    }
    fn import_images(&mut self, ctx: &egui::Context, dropped: Option<Vec<PathBuf>>) {
        let Some(listing) = &self.listing else {
            return;
        };
        let dest = listing.directory.clone();
        self.worker(ctx, "Choose images to add…", move || {
            let files = match dropped {
                Some(files) => files,
                None => match rfd::FileDialog::new()
                    .add_filter(
                        "Disk images",
                        &["img", "ima", "hfe", "adf", "dsk", "st", "ssd", "dsd", "trd"],
                    )
                    .pick_files()
                {
                    Some(files) => files,
                    None => return Ok(Event::Cancelled("Import cancelled".into())),
                },
            };
            let rejected: Vec<_> = files
                .iter()
                .filter(|f| !f.is_file() || !is_image(&f.to_string_lossy()))
                .map(|f| f.display().to_string())
                .collect();
            if !rejected.is_empty() {
                return Err(format!(
                    "Only disk image files can be added to the library. Nothing imported. Not accepted:\n{}",
                    rejected.join("\n")
                ));
            }
            for (i, file) in files.iter().enumerate() {
                let name = file
                    .file_name()
                    .ok_or("Missing image filename")?
                    .to_string_lossy();
                backend(vec![
                    "import".into(),
                    dest.to_string_lossy().into_owned(),
                    file.to_string_lossy().into_owned(),
                    name.into_owned(),
                ])
                .map_err(|e| format!("Imported {i} images before stopping: {e}"))?;
            }
            Ok(Event::Done(format!("Imported {} images", files.len())))
        });
    }
    /// One-step "put this floppy on that bank": pick (or drop) an image, then
    /// confirm a plain-language summary. Recovery is automatic.
    fn load_into_bank(&mut self, ctx: &egui::Context, row: &DiskRow, image: Option<PathBuf>) {
        let Some(bank) = row.bank else {
            return;
        };
        self.selected = BTreeSet::from([row.key.clone()]);
        self.replace_bank_form(row);
        let mut form = self.form.take().unwrap();
        let current = row.clone();
        self.worker(
            ctx,
            &format!("Choose an image for bank {bank:03}…"),
            move || {
                let image = match image {
                    Some(image) => image,
                    None => match rfd::FileDialog::new()
                        .set_title(format!("Load a floppy image into bank {bank:03}"))
                        .add_filter("Floppy images", &["img", "ima", "hfe", "dsk", "bin"])
                        .add_filter("All files", &["*"])
                        .pick_file()
                    {
                        Some(image) => image,
                        None => return Ok(Event::Cancelled("Load cancelled".into())),
                    },
                };
                form.values[3] = image.to_string_lossy().into_owned();
                Ok(prepare_bank_load(form, &current))
            },
        );
    }
    fn reprepare_bank_load(&mut self, ctx: &egui::Context, form: Form) {
        let bank = form.values[2].parse::<u16>().ok();
        let current = self
            .rows
            .iter()
            .find(|r| r.bank.is_some() && r.bank == bank)
            .cloned()
            .unwrap_or(DiskRow {
                key: String::new(),
                name: format!("Bank {}", form.values[2]),
                kind: "Unknown".into(),
                size: 0,
                path: PathBuf::from(&form.values[0]),
                bank,
                directory: false,
                recognized: false,
            });
        self.worker(ctx, "Checking image…", move || {
            Ok(prepare_bank_load(form, &current))
        });
    }
    fn replace_bank_form(&mut self, row: &DiskRow) {
        let bank = row.bank.unwrap();
        self.form = Some(Form {
            title: format!("Load image into bank {bank:03}"),
            description: concat!(
                "Choose a floppy image to put in this bank. The current contents are saved as a ",
                "recovery image first, then the new image is written and verified. ",
                "The USB must be unmounted. Other banks are not touched."
            )
            .into(),
            command: "gui-replace-bank".into(),
            labels: [
                "Target",
                "Profile",
                "Bank",
                "Floppy image to load",
                "Recovery copy of the current bank",
            ]
            .map(str::to_owned)
            .to_vec(),
            kinds: vec![Kind::Text, Kind::Text, Kind::Text, Kind::File, Kind::Save],
            values: vec![
                self.source.clone(),
                self.profile.clone(),
                bank.to_string(),
                String::new(),
                default_recovery_path(bank).to_string_lossy().into_owned(),
            ],
            media: false,
            writes_media: false,
            confirm: true,
            review: false,
            keepalive: None,
            editor: None,
            batch: None,
            eject: None,
            summary: vec![],
        });
    }
    fn execute_form(&mut self, ctx: &egui::Context) {
        let Some(form) = self.form.take() else {
            return;
        };
        if form.confirm && !form.review {
            self.form = Some(form);
            return;
        }
        let args = match form.args(&self.root) {
            Ok(args) => args,
            Err(e) => {
                self.status = e;
                self.error = true;
                self.form = Some(form);
                return;
            }
        };
        if let Some(plan) = form.eject {
            self.worker(ctx, "Unmounting USB and powering it off…", move || {
                crate::usb::power_off(&plan).map_err(|e| e.to_string())?;
                Ok(Event::Ejected)
            });
            return;
        }
        if form.editor.is_some() {
            self.worker(
                ctx,
                "Saving changes to disk…",
                move || match save_editor(&form) {
                    Ok(preview) => Ok(Event::Edited(
                        preview,
                        "Changes saved directly to disk and verified".into(),
                    )),
                    Err(error) => Ok(Event::FormFailed(form, error)),
                },
            );
            return;
        }
        let edited = match form.command.as_str() {
            "image-import" | "image-mkdir" | "image-remove" | "image-rename" | "image-label" => {
                form.values.last().map(PathBuf::from)
            }
            "create-fat12" => Some(PathBuf::from(&form.values[0])),
            _ => None,
        };
        let directory = self
            .preview
            .as_ref()
            .filter(|p| form.values.first().is_some_and(|v| Path::new(v) == p.image))
            .map(|p| p.directory.clone())
            .unwrap_or_else(|| "/".into());
        let retry_form = form.clone();
        let keepalive = form.keepalive;
        self.worker(ctx, &format!("{}…", form.title), move || {
            let _keepalive = keepalive;
            let result = (|| {
                let output = if args[0] == "gui-replace-bank" {
                    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
                    let image = raw_for_bank(Path::new(&args[4]), &args[2], temp.path())?;
                    if let Some(parent) = Path::new(&args[5]).parent()
                        && !parent.as_os_str().is_empty()
                    {
                        fs::create_dir_all(parent).map_err(|e| {
                            format!("Cannot create recovery folder {}: {e}", parent.display())
                        })?;
                    }
                    let expected = temp
                        .path()
                        .join("original.img")
                        .to_string_lossy()
                        .into_owned();
                    backend(vec![
                        "bank-export".into(),
                        args[1].clone(),
                        args[2].clone(),
                        args[3].clone(),
                        expected.clone(),
                    ])?;
                    backend(vec![
                        "bank-write".into(),
                        args[1].clone(),
                        args[2].clone(),
                        args[3].clone(),
                        image.to_string_lossy().into_owned(),
                        expected,
                        args[5].clone(),
                        "--confirm-write".into(),
                    ])?
                } else {
                    // Default recovery copies live in the per-user recovery folder.
                    let recovery: &[usize] = match args[0].as_str() {
                        "bank-swap" => &[5, 6],
                        "bank-format" => &[5],
                        "image-format" => &[3],
                        _ => &[],
                    };
                    for parent in recovery
                        .iter()
                        .filter_map(|&i| Path::new(&args[i]).parent())
                    {
                        if !parent.as_os_str().is_empty() {
                            fs::create_dir_all(parent).map_err(|e| {
                                format!("Cannot create recovery folder {}: {e}", parent.display())
                            })?;
                        }
                    }
                    backend(args)?
                };
                if let Some(image) = edited {
                    let title = image
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let row = DiskRow {
                        key: image.to_string_lossy().into_owned(),
                        name: title,
                        kind: "FAT12".into(),
                        size: 0,
                        path: image,
                        bank: None,
                        directory: false,
                        recognized: true,
                    };
                    let mut preview = open_editor(&row, "gotek1440")
                        .map_err(|e| format!("Disk saved, but could not open it: {e}"))?;
                    preview.entries = crate::fat12::directory_entries(&preview.image, &directory)
                        .map_err(|e| e.to_string())?;
                    preview.directory = directory;
                    return Ok(Event::Edited(preview, output));
                }
                Ok(Event::Done(output))
            })();
            match result {
                Ok(event) => Ok(event),
                Err(error) => Ok(Event::FormFailed(retry_form, error)),
            }
        });
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("appearance", self.appearance.name().into());
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}
impl App {
    fn ui(&mut self, ctx: &egui::Context) {
        self.receive(ctx);
        self.shortcuts(ctx);
        if self.busy && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        chrome::title_bar(
            ctx,
            &mut self.appearance,
            self.busy,
            if self.workspace == Workspace::Banks {
                "Gotek USB banks"
            } else {
                "Image library"
            },
        );
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(visual::colors(ctx).surface)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.add(
                            egui::Spinner::new()
                                .size(16.)
                                .color(visual::colors(ctx).accent),
                        );
                    } else {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(16., 16.), egui::Sense::hover());
                        visual::icon(
                            ui.painter(),
                            r,
                            if self.error { Icon::Close } else { Icon::Check },
                            if self.error {
                                visual::colors(ctx).red
                            } else {
                                visual::colors(ctx).green
                            },
                        );
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&self.status).size(12.).color(
                            if self.error {
                                visual::colors(ctx).red
                            } else {
                                visual::colors(ctx).muted
                            },
                        ))
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if visual::small_button(ui, Icon::Log, "Activity").clicked() {
                            self.show_log = !self.show_log;
                        }
                        if self.show_log
                            && visual::small_button(ui, Icon::Copy, "Copy log").clicked()
                        {
                            ctx.copy_text(self.log.clone());
                        }
                    });
                });
                if self.show_log {
                    egui::ScrollArea::vertical()
                        .max_height(115.)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.log)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                }
            });
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(86.)
            .frame(
                egui::Frame::new()
                    .fill(visual::colors(ctx).nav)
                    .inner_margin(egui::Margin::symmetric(10, 18)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    if visual::nav_button(
                        ui,
                        Icon::Library,
                        "Library",
                        self.workspace == Workspace::Images,
                    )
                    .on_hover_text("Image library")
                    .clicked()
                    {
                        self.switch_workspace(Workspace::Images, ctx);
                    }
                    if visual::nav_button(
                        ui,
                        Icon::Drive,
                        "USB banks",
                        self.workspace == Workspace::Banks,
                    )
                    .on_hover_text("Gotek USB banks")
                    .clicked()
                    {
                        self.switch_workspace(Workspace::Banks, ctx);
                    }
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                            .size(11.)
                            .color(Color32::from_rgb(145, 164, 190)),
                    );
                });
            });
        egui::TopBottomPanel::top("action-toolbar")
            .frame(
                egui::Frame::new()
                    .fill(visual::colors(ctx).surface)
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    self.actions_ui(ui, ctx)
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(visual::colors(ctx).paper)
                    .inner_margin(egui::Margin::symmetric(24, 20)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    self.workspace_ui(ui, ctx)
                });
            });
        self.preview_window(ctx);
        self.form_window(ctx);
        self.dropped_files(ctx);
        self.drop_overlay(ctx);
        chrome::resize_borders(ctx);
    }
    /// The bank a dropped image goes to: the row under the pointer, otherwise
    /// the single selected bank. The review step always names it explicitly.
    fn drop_bank(&self) -> Option<DiskRow> {
        if self.workspace != Workspace::Banks {
            return None;
        }
        let key = self
            .drop_row
            .clone()
            .or_else(|| match self.selection().as_slice() {
                [row] if row.bank.is_some() => Some(row.key.clone()),
                _ => None,
            })?;
        self.rows
            .iter()
            .find(|r| r.key == key && r.bank.is_some())
            .cloned()
    }
    /// What dropping files now would do, or why it cannot.
    fn drop_intent(&self) -> Result<String, String> {
        if self.busy || self.form.is_some() {
            return Err("Finish the current operation before dropping files".into());
        }
        if let Some(preview) = &self.preview {
            return Ok(format!("Add to {} · {}", preview.title, preview.directory));
        }
        match self.workspace {
            Workspace::Banks => self
                .drop_bank()
                .map(|r| format!("Load into bank {:03} · {}", r.bank.unwrap(), r.name))
                .ok_or_else(|| {
                    if self.rows.is_empty() {
                        "Scan banks first, then drop the image onto a bank".into()
                    } else {
                        "Drop onto a bank row, or select a bank first".into()
                    }
                }),
            Workspace::Images => {
                if let Some(listing) = &self.listing {
                    if listing.read_only {
                        Err("This folder is read-only".into())
                    } else {
                        Ok(format!("Add images to {}", listing.directory.display()))
                    }
                } else {
                    Err("Open an image folder first".into())
                }
            }
        }
    }
    fn dropped_files(&mut self, ctx: &egui::Context) {
        let files: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if files.is_empty() {
            return;
        }
        if let Err(reason) = self.drop_intent() {
            self.status = reason;
            self.error = true;
            return;
        }
        if let Some(preview) = self.preview.clone() {
            self.worker(ctx, "Preparing dropped items…", move || {
                let edits = plan_imports(&preview, &files)?;
                Ok(Event::Prepared(batch_form(&preview, edits, false)))
            });
            return;
        }
        match self.workspace {
            Workspace::Banks => {
                let row = self.drop_bank().unwrap();
                if let [file] = files.as_slice() {
                    self.load_into_bank(ctx, &row, Some(file.clone()));
                } else {
                    self.status = "Drop one image at a time onto a bank".into();
                    self.error = true;
                }
            }
            Workspace::Images => self.import_images(ctx, Some(files)),
        }
    }
    fn drop_overlay(&self, ctx: &egui::Context) {
        let count = ctx.input(|i| i.raw.hovered_files.len());
        if count == 0 {
            return;
        }
        let colors = visual::colors(ctx);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("drop-overlay"),
        ));
        let screen = ctx.content_rect();
        painter.rect_filled(screen, 0., Color32::from_black_alpha(90));
        let (text, color) = match self.drop_intent() {
            Ok(text) => (text, colors.accent),
            Err(text) => (text, colors.red),
        };
        let galley = painter.layout(
            text,
            egui::FontId::proportional(18.),
            color,
            (screen.width() - 120.).max(200.),
        );
        let card =
            egui::Rect::from_center_size(screen.center(), galley.size() + egui::vec2(48., 36.));
        painter.rect(
            card,
            12.,
            colors.surface,
            egui::Stroke::new(2_f32, color),
            egui::StrokeKind::Inside,
        );
        painter.galley(card.min + egui::vec2(24., 18.), galley, color);
    }
    fn switch_workspace(&mut self, mode: Workspace, ctx: &egui::Context) {
        if self.workspace == mode {
            return;
        }
        self.workspace = mode;
        self.sort = if mode == Workspace::Banks {
            Sort::Bank
        } else {
            Sort::Name
        };
        self.descending = false;
        self.clear();
        self.filter.clear();
        self.show_all = false;
        if mode == Workspace::Images {
            self.refresh(ctx);
        }
    }
    fn create_disk_form(&mut self) {
        self.open_form("create-fat12");
        if let (Some(l), Some(f)) = (&self.listing, &mut self.form)
            && !l.read_only
        {
            f.values[0] = new_image_path(&l.directory, "New disk")
                .to_string_lossy()
                .into_owned();
        }
    }
    fn choose_source(&mut self, ctx: &egui::Context) {
        if self.workspace == Workspace::Images {
            self.worker(ctx, "Choose an image folder…", || {
                Ok(Event::Folder(rfd::FileDialog::new().pick_folder()))
            });
        } else {
            self.worker(ctx, "Choose a whole-device image…", || {
                Ok(Event::Source(rfd::FileDialog::new().pick_file()))
            });
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.busy
            || self.form.is_some()
            || self.preview.is_some()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.create_disk_form();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.choose_source(ctx);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            if self.workspace == Workspace::Banks && !self.source.trim().is_empty() {
                self.scan(ctx);
            } else {
                self.refresh(ctx);
            }
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("disk-search")));
        }
    }
}

impl App {
    fn workspace_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let banks = self.workspace == Workspace::Banks;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(if banks {
                    "Gotek USB banks"
                } else {
                    "Image library"
                });
                ui.label(
                    RichText::new(if banks {
                        "Every floppy. One workspace."
                    } else {
                        "A home for your floppy collection."
                    })
                    .color(visual::colors(ui.ctx()).muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if banks {
                    visual::badge(
                        ui,
                        if self.profile == "gotek1440" {
                            "1.44 MB / BANK"
                        } else {
                            "720 KB / BANK"
                        },
                        visual::colors(ui.ctx()).accent,
                        visual::colors(ui.ctx()).pale,
                    );
                } else {
                    visual::badge(
                        ui,
                        "LOCAL IMAGES",
                        visual::colors(ui.ctx()).accent,
                        visual::colors(ui.ctx()).pale,
                    );
                }
            });
        });
        ui.add_space(16.);
        egui::Frame::new()
            .fill(visual::colors(ui.ctx()).surface)
            .stroke(egui::Stroke::new(1.0f32, visual::colors(ui.ctx()).border))
            .corner_radius(10)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.source_ui(ui, ctx);
            });
        ui.add_space(16.);
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::hover());
            visual::icon(
                ui.painter(),
                r,
                Icon::Search,
                visual::colors(ui.ctx()).muted,
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .id(egui::Id::new("disk-search"))
                    .hint_text("Search disks…")
                    .desired_width(210.),
            );
            ui.checkbox(
                &mut self.show_all,
                if banks {
                    "Include unrecognized"
                } else {
                    "Show all files"
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.selected.is_empty() {
                    if visual::small_button(ui, Icon::Close, "")
                        .on_hover_text("Clear selection")
                        .clicked()
                    {
                        self.selected.clear();
                    }
                    visual::badge(
                        ui,
                        &format!("{} selected", self.selected.len()),
                        visual::colors(ui.ctx()).accent,
                        visual::colors(ui.ctx()).pale,
                    );
                }
                ui.label(
                    RichText::new(format!("{} shown", self.visible().len()))
                        .size(12.)
                        .color(visual::colors(ui.ctx()).muted),
                );
            });
        });
        ui.add_space(10.);
        egui::Frame::new()
            .fill(visual::colors(ui.ctx()).surface)
            .stroke(egui::Stroke::new(1.0f32, visual::colors(ui.ctx()).border))
            .corner_radius(10)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.table_ui(ui, ctx);
            });
    }
    fn source_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let banks = self.workspace == Workspace::Banks;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::hover());
            visual::icon(
                ui.painter(),
                r,
                if banks { Icon::Drive } else { Icon::Folder },
                visual::colors(ui.ctx()).accent,
            );
            ui.label(
                RichText::new(if banks {
                    "DEVICE OR BACKUP IMAGE"
                } else {
                    "IMAGE FOLDER"
                })
                .size(11.)
                .strong()
                .color(visual::colors(ui.ctx()).muted),
            );
            if let Some(catalog) = &self.bank_catalog {
                ui.label(
                    RichText::new(format!("{} capacity", format_size(catalog.size)))
                        .size(12.)
                        .color(visual::colors(ui.ctx()).muted),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            let width = (ui.available_width() - if banks { 380. } else { 220. }).max(130.);
            let response = if banks {
                ui.add(
                    egui::TextEdit::singleline(&mut self.source)
                        .hint_text(if cfg!(windows) {
                            r"\\.\PhysicalDrive1, E: or a whole-device image"
                        } else {
                            "/dev/sda or a whole-device image"
                        })
                        .desired_width(width),
                )
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut self.root)
                        .hint_text("Choose a folder of images")
                        .desired_width(width),
                )
            };
            if response.changed() {
                self.clear();
            }
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if visual::small_button(ui, Icon::Folder, "Browse")
                .on_hover_text("Choose a source · Ctrl/Cmd+O")
                .clicked()
            {
                self.choose_source(ctx);
            }
            if banks {
                let detect = visual::small_button(ui, Icon::Search, "Detect USB ▾");
                if detect.clicked() {
                    self.detected = crate::usb::detect();
                }
                egui::Popup::menu(&detect).show(|ui| {
                    if self.detected.is_empty() {
                        ui.label("No USB disks detected. Check that the drive is plugged in.");
                    }
                    let mut chosen = None;
                    for (path, label) in &self.detected {
                        if ui.button(label).clicked() {
                            chosen = Some(path.clone());
                        }
                    }
                    if let Some(path) = chosen {
                        self.source = path;
                        self.clear();
                        self.status = "Ready to scan".into();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Search again").clicked() {
                        self.detected = crate::usb::detect();
                    }
                });
                detect.on_hover_text("List connected USB disks to use as the bank source");
                let old = self.profile.clone();
                egui::ComboBox::from_id_salt("bank-profile")
                    .width(120.)
                    .selected_text(if self.profile == "gotek1440" {
                        "1.44 MB banks"
                    } else {
                        "720 KB banks"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.profile, "gotek1440".into(), "1.44 MB banks");
                        ui.selectable_value(&mut self.profile, "gotek720".into(), "720 KB banks");
                    });
                if old != self.profile {
                    self.clear();
                }
            }
            let valid = if banks {
                !self.source.trim().is_empty()
            } else {
                !self.root.trim().is_empty()
            };
            let clicked = ui
                .add_enabled_ui(valid, |ui| {
                    visual::small_button(
                        ui,
                        Icon::Refresh,
                        if banks { "Scan banks" } else { "Refresh" },
                    )
                    .on_hover_text("Read the current source · F5")
                })
                .inner
                .clicked();
            if valid && (clicked || enter) {
                if banks {
                    self.scan(ctx);
                } else {
                    self.refresh(ctx);
                }
            }
        });
        if let Some(listing) = &self.listing {
            let root = listing.root.clone();
            let dir = listing.directory.clone();
            let readonly = listing.read_only;
            if root != dir || readonly {
                ui.horizontal_wrapped(|ui| {
                    if root != dir && visual::small_button(ui, Icon::Up, "Parent folder").clicked()
                    {
                        let parent = dir.parent().unwrap_or(&root).to_owned();
                        self.worker(ctx, "Reading folder…", move || {
                            read_listing(&root, &parent).map(Event::Listed)
                        });
                    }
                    ui.label(
                        RichText::new(dir.display().to_string())
                            .size(12.)
                            .color(visual::colors(ui.ctx()).muted),
                    );
                    if readonly {
                        visual::badge(
                            ui,
                            "READ ONLY",
                            visual::colors(ui.ctx()).amber,
                            visual::colors(ui.ctx()).amber_fill,
                        );
                    }
                });
            }
        }
    }
    fn actions_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let selected = self.selection();
        let single = selected.len() == 1 && !selected[0].directory;
        let can_export = !selected.is_empty() && selected.iter().all(|r| !r.directory);
        let readonly = self.listing.as_ref().is_some_and(|l| l.read_only);
        let banks = self.workspace == Workspace::Banks;
        ui.spacing_mut().item_spacing.x = 4.;
        ui.horizontal_wrapped(|ui| {
            if visual::toolbar_button(
                ui,
                Icon::Add,
                "New disk",
                "Create a formatted floppy image · Ctrl/Cmd+N",
                true,
                !banks,
            )
            .clicked()
            {
                self.create_disk_form();
            }
            if banks {
                if visual::toolbar_button(
                    ui,
                    Icon::Library,
                    "New layout",
                    "Create a whole-device image with formatted floppy banks",
                    true,
                    false,
                )
                .clicked()
                {
                    self.open_form("bank-create");
                    if let (Some(c), Some(f)) = (&self.bank_catalog, &mut self.form) {
                        f.values[3] = c.size.to_string();
                    }
                }
            } else if visual::toolbar_button(
                ui,
                Icon::Import,
                "Import",
                "Add disk images to the open folder",
                self.listing.is_some() && !readonly,
                false,
            )
            .clicked()
            {
                self.import_images(ctx, None);
            }
            toolbar_divider(ui);
            let bank_target = selected.first().filter(|r| single && r.bank.is_some());
            if banks
                && visual::toolbar_button(
                    ui,
                    Icon::Import,
                    "Load image",
                    "Put a floppy image into the selected bank — or drop an image file onto a bank row",
                    bank_target.is_some(),
                    true,
                )
                .clicked()
                && let Some(row) = bank_target
            {
                self.load_into_bank(ctx, row, None);
            }
            if visual::toolbar_button(
                ui,
                Icon::Folder,
                "Open",
                "Browse a disk or folder; inspect an HFE image",
                selected.len() == 1,
                false,
            )
            .clicked()
            {
                self.open_disk(ctx, selected[0].clone());
            }
            if visual::toolbar_button(
                ui,
                Icon::Export,
                "Export",
                "Export selected disks to new files",
                can_export,
                false,
            )
            .clicked()
            {
                self.export_selected(ctx);
            }
            let files: Vec<_> = selected.iter().filter(|r| !r.directory).collect();
            let (can_swap, swap_hint) = if banks {
                (
                    !self.rows.is_empty()
                        && selected.len() <= 2
                        && selected.iter().all(|r| r.bank.is_some()),
                    "Exchange the disks in two banks — select one or two banks",
                )
            } else {
                (
                    self.listing.as_ref().is_some_and(|l| l.root == l.directory)
                        && !readonly
                        && files.len() == selected.len()
                        && selected.len() <= 2,
                    "Exchange two indexed slots (for example DSKA0001 and DSKA0003)",
                )
            };
            if visual::toolbar_button(ui, Icon::Swap, "Swap", swap_hint, can_swap, false).clicked() {
                self.swap_form();
            }
            let format_target = selected.first().filter(|r| {
                single
                    && if banks {
                        r.bank.is_some()
                    } else {
                        !readonly && !r.is_hfe() && crate::fat12::capacity_for(r.size).is_some()
                    }
            });
            if visual::toolbar_button(
                ui,
                Icon::Format,
                "Format",
                if banks {
                    "Erase the selected bank to a blank FAT12 floppy"
                } else {
                    "Erase the selected raw floppy image to a blank FAT12 disk"
                },
                format_target.is_some(),
                false,
            )
            .clicked()
                && let Some(row) = format_target.cloned()
            {
                self.format_form(&row);
            }
            if banks {
                let replace = visual::toolbar_menu(
                    ui,
                    Icon::Replace,
                    "Replace",
                    "Advanced bank replacement options",
                    single,
                );
                egui::Popup::menu(&replace).show(|ui| {
                    if menu_item(ui, "Load image into bank…", true) {
                        self.load_into_bank(ctx, &selected[0], None);
                    }
                    ui.separator();
                    if menu_item(ui, "Use an expected-original image…", true) {
                        self.open_form("bank-write");
                    }
                    if menu_item(ui, "Replace in a new backup copy…", self.source_is_file()) {
                        self.open_form("bank-replace");
                    }
                });
            } else if visual::toolbar_button(
                ui,
                Icon::Trash,
                "Trash",
                "Move the selected image into recoverable trash",
                single && !readonly,
                false,
            )
            .clicked()
            {
                self.trash_form(&selected[0]);
            }
            toolbar_divider(ui);
            let check = visual::toolbar_menu(
                ui,
                Icon::Check,
                "Check",
                "Check a disk or inspect its format and geometry",
                selected.is_empty() || single,
            );
            egui::Popup::menu(&check).show(|ui| {
                let hfe = selected.first().is_some_and(DiskRow::is_hfe);
                if menu_item(ui, "Check FAT12 filesystem", !hfe) {
                    self.disk_command(ctx, "image-check");
                }
                if menu_item(ui, "Inspect FAT12 geometry", !hfe) {
                    self.disk_command(ctx, "inspect-fat12");
                }
                if menu_item(ui, "Inspect HFE tracks", selected.is_empty() || hfe) {
                    self.disk_command(ctx, "inspect-hfe");
                }
            });
            let backup = visual::toolbar_menu(
                ui,
                Icon::Backup,
                "Backup",
                "Create, verify or restore a backup of the current workspace",
                true,
            );
            egui::Popup::menu(&backup).show(|ui| self.backup_menu(ui));
            let convert = visual::toolbar_menu(
                ui,
                Icon::Convert,
                "Convert",
                "Convert the selected disk, or choose an image",
                selected.is_empty() || single,
            );
            egui::Popup::menu(&convert).show(|ui| {
                let hfe = selected.first().is_some_and(DiskRow::is_hfe);
                if menu_item(ui, "Raw image to HFE…", !hfe) {
                    self.disk_command(ctx, "convert-hfe");
                }
                if menu_item(ui, "HFE to raw image…", selected.is_empty() || hfe) {
                    self.disk_command(ctx, "convert-raw");
                }
            });
            let device = visual::toolbar_menu(
                ui,
                Icon::Tools,
                "Device",
                "Source reports, FlashFloppy slots, configuration and firmware",
                true,
            );
            egui::Popup::menu(&device).show(|ui| self.device_menu(ui, ctx));
        });
    }
    fn source_is_file(&self) -> bool {
        self.workspace == Workspace::Banks && Path::new(&self.source).is_file()
    }
    fn backup_menu(&mut self, ui: &mut egui::Ui) {
        let banks = self.workspace == Workspace::Banks;
        let available = if banks {
            !self.source.trim().is_empty()
        } else {
            self.listing.is_some()
        };
        ui.label(
            RichText::new(if banks { "WHOLE DEVICE" } else { "OPEN FOLDER" })
                .small()
                .color(visual::colors(ui.ctx()).muted),
        );
        if menu_item(ui, "Create backup…", available) {
            self.open_form(if banks { "bank-backup" } else { "backup" });
        }
        if menu_item(ui, "Verify against a backup…", available) {
            self.open_form(if banks { "bank-verify" } else { "verify" });
        }
        ui.separator();
        if menu_item(
            ui,
            if banks {
                "Restore whole-device image…"
            } else {
                "Restore to a new folder…"
            },
            true,
        ) {
            self.open_form(if banks { "device-restore" } else { "restore" });
        }
    }
    fn device_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let source = if self.workspace == Workspace::Banks {
            &self.source
        } else {
            &self.root
        };
        if menu_item(
            ui,
            "Safely eject USB…",
            cfg!(target_os = "linux") && !source.trim().is_empty(),
        ) {
            let source = PathBuf::from(source);
            self.worker(ctx, "Identifying USB drive…", move || {
                let plan = crate::usb::prepare(&source).map_err(|e| e.to_string())?;
                let mut form =
                    Form::operation(OPS.iter().find(|op| op.command == "bank-list").unwrap());
                form.title = "Safely eject USB".into();
                form.command = "usb-eject".into();
                form.description = plan.description();
                form.labels = vec!["Selected USB source".into()];
                form.values = vec![source.to_string_lossy().into_owned()];
                form.kinds = vec![Kind::Text];
                form.confirm = true;
                form.review = true;
                form.eject = Some(plan);
                Ok(Event::Prepared(form))
            });
        }
        ui.separator();
        if self.workspace == Workspace::Banks {
            if menu_item(ui, "Refresh bank table", !self.source.trim().is_empty()) {
                self.scan(ctx);
            }
            if menu_item(ui, "Bank header report", !self.source.trim().is_empty()) {
                self.open_form("bank-list");
            }
            ui.separator();
            ui.label(
                RichText::new("FlashFloppy file tools are in Image library.")
                    .small()
                    .color(visual::colors(ui.ctx()).muted),
            );
            if menu_item(ui, "Open mounted media folder…", true) {
                self.worker(ctx, "Choose a mounted media folder…", || {
                    Ok(Event::Folder(rfd::FileDialog::new().pick_folder()))
                });
            }
        } else {
            if menu_item(ui, "Media report", self.listing.is_some()) {
                self.open_form("media-info");
            }
            ui.separator();
            ui.menu_button("Indexed slots", |ui| self.category_menu(ui, Page::Slots));
            ui.menu_button("FlashFloppy settings", |ui| {
                self.category_menu(ui, Page::Config)
            });
        }
        ui.separator();
        ui.menu_button("Firmware", |ui| self.category_menu(ui, Page::Firmware));
    }
    fn category_menu(&mut self, ui: &mut egui::Ui, page: Page) {
        for op in OPS.iter().filter(|o| o.page == page) {
            let available = (!op.media
                || self.workspace == Workspace::Images && self.listing.is_some())
                && (!op.writes_media || !self.listing.as_ref().is_some_and(|l| l.read_only));
            if menu_item(ui, op.title, available) {
                self.open_form(op.command);
            }
        }
        if self.workspace != Workspace::Images || self.listing.is_none() {
            ui.label(
                RichText::new("Open a mounted folder in Image library for media actions.")
                    .small()
                    .color(visual::colors(ui.ctx()).muted),
            );
        }
    }
    fn trash_form(&mut self, row: &DiskRow) {
        self.form = Some(Form {
            title: format!("Move {} to trash", row.name),
            description: "Move this image into the media's recoverable .rustgotek-trash folder."
                .into(),
            command: "remove".into(),
            labels: vec!["Selected file".into()],
            kinds: vec![Kind::Text],
            values: vec![row.key.clone()],
            media: true,
            writes_media: true,
            confirm: true,
            review: false,
            keepalive: None,
            editor: None,
            batch: None,
            eject: None,
            summary: vec![],
        });
    }
    fn custom_form(
        &self,
        title: String,
        description: &str,
        command: &str,
        fields: &[(&str, Kind, String)],
        media: bool,
    ) -> Form {
        Form {
            title,
            description: description.into(),
            command: command.into(),
            labels: fields.iter().map(|f| f.0.to_string()).collect(),
            kinds: fields.iter().map(|f| f.1).collect(),
            values: fields.iter().map(|f| f.2.clone()).collect(),
            media,
            writes_media: media,
            confirm: true,
            review: false,
            keepalive: None,
            editor: None,
            batch: None,
            eject: None,
            summary: vec![],
        }
    }
    /// Swap two banks (Gotek USB) or two indexed slots (image library).
    /// The current selection prefills the two numbers.
    fn swap_form(&mut self) {
        self.error = false;
        let selected = self.selection();
        if self.workspace == Workspace::Banks {
            let banks: Vec<u16> = selected.iter().filter_map(|r| r.bank).collect();
            let a = banks.first().copied().unwrap_or(0);
            let b = banks
                .get(1)
                .copied()
                .unwrap_or(if a == 0 { 1 } else { a - 1 });
            self.form = Some(self.custom_form(
                "Swap two banks".into(),
                "Exchange the floppies stored in two banks. Both current disks are saved as recovery images first, then each bank is written and verified. Other banks are not touched; the USB must be unmounted.",
                "bank-swap",
                &[
                    ("Target", Kind::Text, self.source.clone()),
                    ("Profile", Kind::Text, self.profile.clone()),
                    ("First bank", Kind::Text, a.to_string()),
                    ("Second bank", Kind::Text, b.to_string()),
                    ("Recovery copy of the first bank", Kind::Save, default_recovery_path(a).to_string_lossy().into_owned()),
                    ("Recovery copy of the second bank", Kind::Save, default_recovery_path(b).to_string_lossy().into_owned()),
                ],
                false,
            ));
        } else {
            self.open_form("slot-swap");
            let prefix =
                crate::slots::prefix(Path::new(&self.root)).unwrap_or_else(|_| "DSKA".into());
            let numbers: Vec<u16> = selected
                .iter()
                .filter_map(|r| crate::slots::slot_of(&r.name, &prefix))
                .collect();
            if let Some(form) = &mut self.form {
                for (value, n) in form.values.iter_mut().zip(&numbers) {
                    *value = n.to_string();
                }
            }
        }
    }
    /// Erase a bank or a standalone raw image to a blank FAT12 floppy.
    fn format_form(&mut self, row: &DiskRow) {
        self.error = false;
        if let Some(bank) = row.bank {
            self.form = Some(self.custom_form(
                format!("Format bank {bank:03}"),
                "Erase this bank and replace it with a blank FAT12 floppy. The current disk is saved as a recovery image first. Other banks are not touched; the USB must be unmounted.",
                "bank-format",
                &[
                    ("Target", Kind::Text, self.source.clone()),
                    ("Profile", Kind::Text, self.profile.clone()),
                    ("Bank", Kind::Text, bank.to_string()),
                    ("Disk label (up to 11 characters)", Kind::Text, format!("FDISK{bank:03}")),
                    ("Recovery copy of the current bank", Kind::Save, default_recovery_path(bank).to_string_lossy().into_owned()),
                ],
                false,
            ));
            return;
        }
        let stem = row
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let label: String = stem
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '_' || *c == '-')
            .take(11)
            .collect::<String>()
            .trim()
            .to_ascii_uppercase();
        self.form = Some(self.custom_form(
            format!("Format {}", row.name),
            "Erase this image and replace it with a blank FAT12 floppy of the same capacity. The current contents are saved as a recovery image first.",
            "image-format",
            &[
                ("Disk image", Kind::Text, row.path.to_string_lossy().into_owned()),
                ("Disk label (up to 11 characters)", Kind::Text, if label.is_empty() { "DISK".into() } else { label }),
                (
                    "Recovery copy of the current contents",
                    Kind::Save,
                    new_image_path(&recovery_dir(), &format!("{stem}-before-format-{}", utc_timestamp()))
                        .to_string_lossy()
                        .into_owned(),
                ),
            ],
            false,
        ));
    }
    fn bank_name(&self, bank: &str) -> String {
        let number = bank.trim().parse::<u16>().ok();
        self.rows
            .iter()
            .find(|r| r.bank.is_some() && r.bank == number)
            .map(|r| format!("{} · {}", r.name, r.kind))
            .unwrap_or_else(|| "not scanned".into())
    }
    /// Plain-language review lines for swap and format confirmations.
    fn review_summary(&self, form: &Form) -> Vec<String> {
        let v = &form.values;
        match form.command.as_str() {
            "bank-swap" => vec![
                format!("Swap banks {:0>3} and {:0>3} on {}", v[2].trim(), v[3].trim(), v[0]),
                format!("Bank {:0>3} now: {}", v[2].trim(), self.bank_name(&v[2])),
                format!("Bank {:0>3} now: {}", v[3].trim(), self.bank_name(&v[3])),
                format!("Recovery copies: {} and {}", v[4], v[5]),
                "Both banks are saved first, then written and verified. If a write fails, both banks are restored. Other banks are not touched.".into(),
            ],
            "bank-format" => vec![
                format!("Erase bank {:0>3} on {}", v[2].trim(), v[0]),
                format!("Currently: {}", self.bank_name(&v[2])),
                format!(
                    "New disk: blank FAT12 {} · label {}",
                    if v[1] == "gotek720" { "720 KB" } else { "1.44 MB" },
                    v[3].trim().to_ascii_uppercase()
                ),
                format!("Recovery copy: {}", v[4]),
                "Everything on this bank is erased. The recovery copy lets you load the old disk back with Load image.".into(),
            ],
            "image-format" => {
                let size = fs::metadata(&v[0]).map(|m| m.len()).unwrap_or(0);
                vec![
                    format!("Erase {}", v[0]),
                    match crate::fat12::capacity_for(size) {
                        Some(kib) => format!(
                            "New disk: blank FAT12 {kib} KiB · label {}",
                            v[1].trim().to_ascii_uppercase()
                        ),
                        None => format!("Warning: {} is not a standard floppy size; formatting will be refused", format_size(size)),
                    },
                    format!("Recovery copy: {}", v[2]),
                    "Every file on this disk is erased. The image keeps its name and capacity.".into(),
                ]
            }
            "slot-swap" => vec![
                format!("Swap slots {:0>4} and {:0>4} in {}", v[0].trim(), v[1].trim(), self.root),
                "Images are renamed in place; their contents are not copied or changed. The Gotek shows each image under its new slot number.".into(),
            ],
            _ => vec![],
        }
    }
    fn disk_command(&mut self, ctx: &egui::Context, command: &'static str) {
        let selected = self.selection();
        let [row] = selected.as_slice() else {
            self.open_form(command);
            return;
        };
        if row.directory {
            return;
        }
        let row = row.clone();
        let profile = self.profile.clone();
        self.worker(ctx, "Preparing selected disk…", move || {
            let op = OPS
                .iter()
                .find(|o| o.command == command)
                .ok_or("Unknown disk action")?;
            let mut form = Form::operation(op);
            if is_disk_edit(command) {
                let preview = open_editor(&row, &profile)?;
                form.bind_editor(&preview);
                if command == "image-label" {
                    form.values[1] = preview.label.clone();
                }
                return Ok(Event::Prepared(form));
            }
            let (image, temp) = disk_snapshot(&row, &profile)?;
            if matches!(command, "image-check" | "inspect-fat12" | "inspect-hfe") {
                return backend(vec![command.into(), image.to_string_lossy().into_owned()])
                    .map(Event::Done);
            }
            form.values[0] = image.to_string_lossy().into_owned();
            form.keepalive = temp;
            if command == "convert-hfe" {
                form.values[2] = match row.size / 1024 {
                    360 => "pc360",
                    720 => "pc720",
                    1200 => "pc1200",
                    _ => "pc1440",
                }
                .into();
            }
            Ok(Event::Prepared(form))
        });
    }
    fn apply_disk_action(&mut self, ctx: &egui::Context, action: DiskAction) {
        let selected = self.selection();
        match action {
            DiskAction::Export => self.export_selected(ctx),
            DiskAction::Open => {
                if let [row] = selected.as_slice() {
                    self.open_disk(ctx, row.clone());
                }
            }
            DiskAction::Replace => {
                if let [row] = selected.as_slice() {
                    self.load_into_bank(ctx, row, None);
                }
            }
            DiskAction::Trash => {
                if let [row] = selected.as_slice() {
                    self.trash_form(row);
                }
            }
            DiskAction::Swap => self.swap_form(),
            DiskAction::Format => {
                if let [row] = selected.as_slice() {
                    self.format_form(row);
                }
            }
            DiskAction::Command(cmd) if cmd.starts_with("bank-") || cmd == "slot" => {
                self.open_form(cmd)
            }
            DiskAction::Command(cmd) => self.disk_command(ctx, cmd),
        }
    }
    fn sort_header(&mut self, ui: &mut egui::Ui, label: &str, sort: Sort) {
        let active = self.sort == sort;
        let response = ui.selectable_label(active, RichText::new(label).strong());
        if active {
            visual::sort_arrow(ui, self.descending);
        }
        if response.on_hover_text("Sort by this column").clicked() {
            if self.sort == sort {
                self.descending = !self.descending;
            } else {
                self.sort = sort;
                self.descending = false;
            }
        }
    }
    fn table_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let visible = self.visible();
        self.drop_row = None;
        if self.rows.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                visual::empty_art(
                    ui,
                    if self.workspace == Workspace::Banks {
                        Icon::Drive
                    } else {
                        Icon::Disk
                    },
                );
                ui.add_space(12.0);
                ui.heading(if self.workspace == Workspace::Images {
                    "Bring your disks into one place"
                } else {
                    "Your Gotek's disks will appear here"
                });
                ui.label(if self.workspace == Workspace::Images {
                    "Choose an image folder above, or create your first floppy image. Drop image files here to add them."
                } else {
                    "Enter the USB device path or select a whole-device backup, then Scan banks. To load a floppy image, drop it onto a bank or use Load image."
                });
            });
            return;
        }
        if visible.is_empty() {
            ui.add_space(30.0);
            ui.label(
                "No matching disks. Clear the search or enable the option to show all entries.",
            );
            return;
        }
        let mut open = None;
        let mut action = None;
        let readonly = self.listing.as_ref().is_some_and(|l| l.read_only);
        let backup_file = self.source_is_file();
        let banks = self.workspace == Workspace::Banks;
        let height = (ui.available_height() - 20.0).max(100.0);
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let dragging = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let drop_key = self.drop_bank().filter(|_| dragging).map(|r| r.key);
        let mut hovered_row = None;
        let mut table = TableBuilder::new(ui)
            .id_salt(("disk-table", banks))
            .sense(egui::Sense::click())
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .min_scrolled_height(height)
            .max_scroll_height(height)
            .column(Column::exact(30.0));
        if banks {
            table = table.column(Column::initial(70.0));
        }
        table
            .column(Column::remainder().at_least(160.0))
            .column(Column::initial(170.0))
            .column(Column::initial(100.0))
            .header(36.0, |mut header| {
                header.col(|ui| {
                    let mut all = visible
                        .iter()
                        .all(|i| self.selected.contains(&self.rows[*i].key));
                    if ui
                        .checkbox(&mut all, "")
                        .on_hover_text("Select all visible rows")
                        .changed()
                    {
                        for i in &visible {
                            if all {
                                self.selected.insert(self.rows[*i].key.clone());
                            } else {
                                self.selected.remove(&self.rows[*i].key);
                            }
                        }
                    }
                });
                if banks {
                    header.col(|ui| self.sort_header(ui, "Bank", Sort::Bank));
                }
                for (label, sort) in [
                    ("Disk / filename", Sort::Name),
                    ("Format / status", Sort::Kind),
                    ("Size", Sort::Size),
                ] {
                    header.col(|ui| self.sort_header(ui, label, sort));
                }
            })
            .body(|body| {
                body.rows(42.0, visible.len(), |mut tr| {
                    let row = &self.rows[visible[tr.index()]];
                    let mut chosen = self.selected.contains(&row.key);
                    let mut widget_response: Option<egui::Response> = None;
                    tr.set_selected(chosen || drop_key.as_ref() == Some(&row.key));
                    tr.col(|ui| {
                        let response = ui.checkbox(&mut chosen, "");
                        widget_response = Some(response.clone());
                        if response.changed() {
                            if chosen {
                                self.selected.insert(row.key.clone());
                            } else {
                                self.selected.remove(&row.key);
                            }
                        }
                    });
                    if banks {
                        tr.col(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        row.bank.map(|n| format!("{n:03}")).unwrap_or_default(),
                                    )
                                    .monospace(),
                                )
                                .selectable(false),
                            );
                        });
                    }
                    tr.col(|ui| {
                        let response = ui
                            .horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(20., 20.),
                                    egui::Sense::hover(),
                                );
                                visual::icon(
                                    ui.painter(),
                                    r,
                                    if row.directory {
                                        Icon::Folder
                                    } else {
                                        Icon::Disk
                                    },
                                    visual::colors(ui.ctx()).accent,
                                );
                                ui.selectable_label(chosen, &row.name)
                            })
                            .inner;
                        widget_response =
                            Some(widget_response.take().unwrap().union(response.clone()));
                        if response.clicked() {
                            if !ui.input(|i| i.modifiers.command) {
                                self.selected.clear();
                            }
                            self.selected.insert(row.key.clone());
                        }
                        if response.double_clicked() {
                            open = Some(row.clone());
                        }
                    });
                    tr.col(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&row.kind).color(if row.recognized {
                                visual::colors(ui.ctx()).green
                            } else {
                                visual::colors(ui.ctx()).muted
                            }))
                            .selectable(false),
                        );
                    });
                    tr.col(|ui| {
                        ui.add(
                            egui::Label::new(if row.directory {
                                String::new()
                            } else {
                                format_size(row.size)
                            })
                            .selectable(false),
                        );
                    });
                    let response = tr.response().union(widget_response.unwrap());
                    if banks && pointer.is_some_and(|p| response.rect.contains(p)) {
                        hovered_row = Some(row.key.clone());
                    }
                    if response.secondary_clicked() {
                        select_context_row(&mut self.selected, row);
                    }
                    if response.double_clicked() {
                        open = Some(row.clone());
                    }
                    response.context_menu(|ui| {
                        let selected: Vec<_> = self
                            .rows
                            .iter()
                            .filter(|r| self.selected.contains(&r.key))
                            .cloned()
                            .collect();
                        if let Some(chosen) = disk_menu(ui, &selected, readonly, backup_file) {
                            action = Some(chosen);
                        }
                    });
                })
            });
        self.drop_row = hovered_row;
        if let Some(row) = open {
            self.open_disk(ctx, row);
        }
        if let Some(action) = action {
            self.apply_disk_action(ctx, action);
        }
    }
    fn form_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.form.take() else {
            return;
        };
        let mut open = true;
        let mut execute = false;
        let mut browse = None;
        let mut prepare_load = false;
        egui::Window::new(&form.title)
            .id(egui::Id::new("operation-form"))
            .open(&mut open)
            .collapsible(false)
            .default_width(650.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.label(&form.description);
                    if self.error {
                        ui.colored_label(visual::colors(ui.ctx()).red, &self.log);
                    }
                    if form.media {
                        ui.label(format!("Media: {}", self.root));
                    }
                    ui.separator();
                    if form.review {
                        ui.colored_label(
                            visual::colors(ui.ctx()).amber,
                            "Review before changing data",
                        );
                        if form.summary.is_empty() {
                            for (label, value) in form.labels.iter().zip(&form.values) {
                                ui.label(RichText::new(label).strong());
                                ui.label(value);
                            }
                        } else {
                            let (last, lines) = form.summary.split_last().unwrap();
                            for (i, line) in lines.iter().enumerate() {
                                ui.label(if i == 0 {
                                    RichText::new(line).strong()
                                } else {
                                    RichText::new(line)
                                });
                            }
                            ui.add_space(4.);
                            ui.label(RichText::new(last).color(visual::colors(ui.ctx()).muted));
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if visual::small_button(ui, Icon::Back, "Back").clicked() {
                                form.review = false;
                            }
                            let v = &form.values;
                            let label = if form.eject.is_some() {
                                "Eject and power off".to_string()
                            } else {
                                match form.command.as_str() {
                                    "gui-replace-bank" => format!("Write to bank {:0>3}", v[2]),
                                    "bank-swap" => format!(
                                        "Swap banks {:0>3} and {:0>3}",
                                        v[2].trim(),
                                        v[3].trim()
                                    ),
                                    "slot-swap" => format!(
                                        "Swap slots {:0>4} and {:0>4}",
                                        v[0].trim(),
                                        v[1].trim()
                                    ),
                                    "bank-format" => format!("Erase bank {:0>3}", v[2].trim()),
                                    "image-format" => "Erase and format disk".to_string(),
                                    _ => "Confirm and run".to_string(),
                                }
                            };
                            let destructive = matches!(
                                form.command.as_str(),
                                "bank-format" | "image-format" | "remove" | "device-restore"
                            );
                            let clicked = if destructive {
                                visual::danger_button(ui, &label).clicked()
                            } else {
                                visual::primary_button(ui, &label, true).clicked()
                            };
                            if clicked {
                                execute = true;
                            }
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(440.0)
                            .show(ui, |ui| {
                                if let Some(edits) = &form.batch {
                                    for edit in edits {
                                        ui.label(edit.description());
                                    }
                                }
                                for i in 0..form.values.len() {
                                    if form.batch.is_some() {
                                        continue;
                                    }
                                    if form.editor.is_some()
                                        && (i == 0 || i == form.values.len() - 1)
                                    {
                                        continue;
                                    }
                                    ui.label(&form.labels[i]);
                                    if form.eject.is_some() {
                                        ui.label(&form.values[i]);
                                        continue;
                                    }
                                    if i == 0
                                        && form.command.starts_with("image-")
                                        && self
                                            .preview
                                            .as_ref()
                                            .is_some_and(|p| Path::new(&form.values[0]) == p.image)
                                    {
                                        ui.label(RichText::new(&form.values[i]).weak());
                                        continue;
                                    }
                                    ui.horizontal(|ui| {
                                        if form.labels[i].to_lowercase().contains("profile")
                                            && (form.values[i].starts_with("gotek")
                                                || form.command.starts_with("bank")
                                                || form.command == "gui-replace-bank")
                                        {
                                            egui::ComboBox::from_id_salt(("profile", i))
                                                .selected_text(&form.values[i])
                                                .show_ui(ui, |ui| {
                                                    for p in ["gotek1440", "gotek720"] {
                                                        ui.selectable_value(
                                                            &mut form.values[i],
                                                            p.into(),
                                                            p,
                                                        );
                                                    }
                                                });
                                        } else if form.command == "create-fat12" && i == 1 {
                                            egui::ComboBox::from_id_salt("capacity")
                                                .selected_text(format!("{} KiB", form.values[i]))
                                                .show_ui(ui, |ui| {
                                                    for size in
                                                        ["360", "720", "1200", "1440", "2880"]
                                                    {
                                                        ui.selectable_value(
                                                            &mut form.values[i],
                                                            size.into(),
                                                            format!("{size} KiB"),
                                                        );
                                                    }
                                                });
                                        } else {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut form.values[i])
                                                    .desired_width(510.0),
                                            );
                                        }
                                        if !matches!(form.kinds[i], Kind::Text)
                                            && visual::small_button(ui, Icon::Folder, "Browse…")
                                                .clicked()
                                        {
                                            browse = Some((i, form.kinds[i]));
                                        }
                                    });
                                }
                            });
                        let readonly = (form.writes_media
                            && self.listing.as_ref().is_some_and(|l| l.read_only))
                            || form
                                .editor
                                .as_ref()
                                .is_some_and(|p| p.bank.is_none() && mount_read_only(&p.target));
                        if readonly {
                            ui.colored_label(
                                visual::colors(ui.ctx()).amber,
                                "This media is read-only.",
                            );
                        }
                        let validation = form.args(&self.root);
                        if let Err(message) = &validation {
                            ui.colored_label(visual::colors(ui.ctx()).amber, message);
                        }
                        let valid = validation.is_ok() && !readonly;
                        ui.add_space(4.);
                        if visual::primary_button(
                            ui,
                            if form.editor.is_some() {
                                "Save changes"
                            } else if form.command == "gui-replace-bank" {
                                "Continue…"
                            } else if form.confirm {
                                "Review changes…"
                            } else if form.command == "create-fat12" {
                                "Create disk"
                            } else if matches!(
                                form.command.as_str(),
                                "image-import"
                                    | "image-mkdir"
                                    | "image-remove"
                                    | "image-rename"
                                    | "image-label"
                            ) {
                                "Save edited copy"
                            } else {
                                "Run operation"
                            },
                            valid,
                        )
                        .clicked()
                        {
                            if form.command == "gui-replace-bank" {
                                prepare_load = true;
                            } else if form.confirm {
                                form.summary = self.review_summary(&form);
                                form.review = true;
                            } else {
                                execute = true;
                            }
                        }
                    }
                });
            });
        if open && prepare_load {
            self.reprepare_bank_load(ctx, form);
        } else if open {
            self.form = Some(form);
        }
        if let Some((i, kind)) = browse {
            self.worker(ctx, "Choose a path…", move || {
                let d = rfd::FileDialog::new();
                Ok(Event::Picked(
                    i,
                    match kind {
                        Kind::File => d.pick_file(),
                        Kind::Folder => d.pick_folder(),
                        Kind::Save => d.save_file(),
                        Kind::Text => None,
                    },
                ))
            });
        }
        if execute && open {
            self.execute_form(ctx);
        }
    }
    fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(mut preview) = self.preview.take() else {
            return;
        };
        let mut open = true;
        let mut directory = None;
        let mut action = None;
        let mut extract = false;
        let mut import_batch = None;
        let mut remove_batch = false;
        let mut format = false;
        egui::Window::new(format!("Disk contents — {}", preview.title))
            .id(egui::Id::new("preview")).open(&mut open).default_size([720.0, 510.0])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    ui.label(preview.target.display().to_string());
                    if let Some((_, bank)) = &preview.bank { ui.strong(format!("Editing bank {bank:03}")); }
                    ui.horizontal_wrapped(|ui| {
                        ui.heading(if preview.label.is_empty() { "Unlabelled disk" } else { &preview.label });
                        if visual::small_button(ui, Icon::Edit, "Change label").clicked() { action = Some("image-label"); }
                        if visual::small_button(ui, Icon::Format, "Format disk…").on_hover_text("Erase every file and start with a blank disk").clicked() { format = true; }
                    });
                    ui.label(RichText::new("Changes save directly to this disk. Use Export first if you want a backup.").small().weak());
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled_ui(preview.directory != "/", |ui| visual::small_button(ui, Icon::Up, "Up")).inner.clicked() {
                            directory = Some(parent_directory(&preview.directory));
                        }
                        if visual::text_button(ui, "Disk root").clicked() { directory = Some("/".into()); }
                        let mut path = String::new();
                        for part in preview.directory.split('/').filter(|p| !p.is_empty()) {
                            path = inner_path(&path, part);
                            ui.label(RichText::new("/").color(visual::colors(ui.ctx()).muted));
                            if visual::text_button(ui, part).clicked() { directory = Some(path.clone()); }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if visual::small_button(ui, Icon::Check, "Check disk").clicked() { action = Some("image-check"); }
                        if visual::small_button(ui, Icon::Import, "Add files").clicked() { import_batch = Some(false); }
                        if visual::small_button(ui, Icon::Folder, "Add folders").clicked() { import_batch = Some(true); }
                        if visual::small_button(ui, Icon::Folder, "New folder").clicked() { action = Some("image-mkdir"); }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let selected = preview.selected.as_ref().filter(|_| preview.checked.len() <= 1).and_then(|s| preview.entries.iter().find(|e| &e.name == s));
                        if ui.add_enabled_ui(selected.is_some_and(|e| e.directory), |ui| visual::small_button(ui, Icon::Folder, "Open folder")).inner.clicked() {
                            directory = selected.map(|e| inner_path(&preview.directory, &e.name));
                        }
                        if ui.add_enabled_ui(selected.is_some_and(|e| !e.directory), |ui| visual::small_button(ui, Icon::Export, "Export file")).inner.clicked() { extract = true; }
                        if ui.add_enabled_ui(selected.is_some(), |ui| visual::small_button(ui, Icon::Edit, "Rename")).inner.clicked() { action = Some("image-rename"); }
                        if ui.add_enabled_ui(!preview.checked.is_empty(), |ui| visual::small_button(ui, Icon::Trash, "Remove selected")).inner.clicked() { remove_batch = true; }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if visual::text_button(ui, "Select all").clicked() {
                            preview.checked = preview.entries.iter().map(|e| e.name.clone()).collect();
                            preview.selected = preview.checked.iter().next().cloned();
                        }
                        if visual::text_button(ui, "Clear selection").clicked() { preview.checked.clear(); preview.selected = None; }
                        ui.label(RichText::new(format!("{} selected", preview.checked.len())).size(12.).color(visual::colors(ui.ctx()).muted));
                    });
                    TableBuilder::new(ui).id_salt("contents-table").striped(true)
                        .column(Column::remainder()).column(Column::initial(80.0)).column(Column::initial(100.0))
                        .max_scroll_height(280.0).header(28.0, |mut h| {
                            for name in ["Name", "Type", "Size"] { h.col(|ui| { ui.strong(name); }); }
                        }).body(|body| body.rows(30.0, preview.entries.len(), |mut tr| {
                            let e = &preview.entries[tr.index()];
                            tr.col(|ui| { ui.horizontal(|ui| {
                                let mut checked = preview.checked.contains(&e.name);
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked { preview.checked.insert(e.name.clone()); } else { preview.checked.remove(&e.name); }
                                    preview.selected = preview.checked.iter().next().cloned();
                                }
                                let response = ui.selectable_label(checked, &e.name);
                                if response.clicked() {
                                    if ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
                                        if !preview.checked.remove(&e.name) { preview.checked.insert(e.name.clone()); }
                                    } else { preview.checked = BTreeSet::from([e.name.clone()]); }
                                    preview.selected = preview.checked.iter().next().cloned();
                                }
                                if response.secondary_clicked() {
                                    if !preview.checked.contains(&e.name) { preview.checked = BTreeSet::from([e.name.clone()]); }
                                    preview.selected = Some(e.name.clone());
                                }
                                if response.double_clicked() && e.directory { directory = Some(inner_path(&preview.directory, &e.name)); }
                                response.context_menu(|ui| {
                                    if e.directory && menu_item(ui, "Open folder", preview.checked.len() == 1) { directory = Some(inner_path(&preview.directory, &e.name)); }
                                    if !e.directory && menu_item(ui, "Export file…", preview.checked.len() == 1) { extract = true; }
                                    if menu_item(ui, "Rename…", preview.checked.len() == 1) { action = Some("image-rename"); }
                                    if menu_item(ui, "Remove selected…", true) { remove_batch = true; }
                                    ui.separator();
                                    if menu_item(ui, "Add files here…", true) { import_batch = Some(false); }
                                    if menu_item(ui, "Add folders here…", true) { import_batch = Some(true); }
                                    if menu_item(ui, "New folder here…", true) { action = Some("image-mkdir"); }
                                });
                            }); });
                            tr.col(|ui| { ui.label(if e.directory { "Folder" } else { "File" }); });
                            tr.col(|ui| { ui.label(format_size(e.size)); });
                        }));
                    if preview.entries.is_empty() { ui.label("This folder is empty. Add a file or create a folder to get started."); }
                });
            });
        if format {
            // The open view would be stale after formatting, so close it.
            let row = DiskRow {
                key: String::new(),
                name: preview.title.clone(),
                kind: "FAT12".into(),
                size: fs::metadata(&preview.image).map(|m| m.len()).unwrap_or(0),
                path: preview.target.clone(),
                bank: preview.bank.as_ref().map(|(_, n)| *n),
                directory: false,
                recognized: true,
            };
            self.format_form(&row);
            return;
        }
        if let Some(folders) = import_batch {
            let preview = preview.clone();
            self.worker(ctx, "Choose items to add…", move || {
                let dialog = rfd::FileDialog::new();
                let sources = if folders {
                    dialog.pick_folders()
                } else {
                    dialog.pick_files()
                };
                let Some(sources) = sources else {
                    return Ok(Event::Preview(preview));
                };
                let edits = plan_imports(&preview, &sources)?;
                Ok(Event::Prepared(batch_form(&preview, edits, false)))
            });
        }
        if remove_batch {
            let preview = preview.clone();
            self.worker(ctx, "Preparing selected items…", move || {
                let edits = plan_removals(&preview)?;
                Ok(Event::Prepared(batch_form(&preview, edits, true)))
            });
        }
        if let Some(dir) = directory {
            let image = preview.image.clone();
            let temp = preview.temp.clone();
            let title = preview.title.clone();
            let target = preview.target.clone();
            let bank = preview.bank.clone();
            self.worker(ctx, "Reading disk directory…", move || {
                let entries =
                    crate::fat12::directory_entries(&image, &dir).map_err(|e| e.to_string())?;
                Ok(Event::Preview(Preview {
                    target,
                    bank,
                    label: crate::fat12::volume_label(&image).map_err(|e| e.to_string())?,
                    image,
                    directory: dir,
                    entries,
                    temp,
                    selected: None,
                    checked: BTreeSet::new(),
                    title,
                }))
            });
        }
        if let Some(command) = action {
            self.open_form(command);
            if let Some(form) = &mut self.form {
                form.values[0] = preview.image.to_string_lossy().into_owned();
                form.keepalive = preview.temp.clone();
                if command == "image-label" {
                    form.values[1] = preview.label.clone();
                }
                if command == "image-rename" {
                    form.values[2] = preview.selected.clone().unwrap_or_default();
                }

                if matches!(command, "image-remove" | "image-rename") {
                    form.values[1] = inner_path(
                        &preview.directory,
                        preview.selected.as_deref().unwrap_or(""),
                    );
                }
                if matches!(command, "image-mkdir" | "image-import") && preview.directory != "/" {
                    let index = if command == "image-import" { 2 } else { 1 };
                    form.values[index] = format!("{}/", preview.directory);
                }
                if is_disk_edit(command) {
                    form.bind_editor(&preview);
                }
            }
        }
        if extract {
            let image = preview.image.clone();
            let name = preview.selected.clone().unwrap();
            let path = inner_path(&preview.directory, &name);
            let temp = preview.temp.clone();
            self.worker(ctx, "Choose where to export the file…", move || {
                let _keepalive = temp;
                let Some(dest) = rfd::FileDialog::new().set_file_name(name).save_file() else {
                    return Ok(Event::Done("Export cancelled".into()));
                };
                backend(vec![
                    "image-extract".into(),
                    image.to_string_lossy().into_owned(),
                    path,
                    dest.to_string_lossy().into_owned(),
                ])
                .map(Event::Done)
            });
        }
        if open {
            self.preview = Some(preview);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiskAction {
    Open,
    Export,
    Replace,
    Trash,
    Swap,
    Format,
    Command(&'static str),
}
impl DiskRow {
    fn is_hfe(&self) -> bool {
        self.bank.is_none()
            && self
                .path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("hfe"))
    }
}
fn menu_item(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    let clicked = ui
        .add_enabled(
            enabled,
            egui::Button::new(label)
                .frame(false)
                .right_text("")
                .min_size(egui::vec2(220., 28.)),
        )
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}
fn select_context_row(selected: &mut BTreeSet<String>, row: &DiskRow) {
    if !selected.contains(&row.key) {
        selected.clear();
        selected.insert(row.key.clone());
    }
}
fn disk_menu(
    ui: &mut egui::Ui,
    rows: &[DiskRow],
    readonly: bool,
    backup_file: bool,
) -> Option<DiskAction> {
    let mut action = None;
    if let [row] = rows {
        ui.label(
            RichText::new(&row.name)
                .small()
                .color(visual::colors(ui.ctx()).muted),
        );
        ui.separator();
        if menu_item(
            ui,
            if row.directory {
                "Open folder"
            } else if row.is_hfe() {
                "Inspect HFE tracks"
            } else {
                "Open contents"
            },
            row.directory || row.recognized,
        ) {
            action = Some(DiskAction::Open);
        }
    } else {
        ui.label(
            RichText::new(format!("{} disks selected", rows.len()))
                .small()
                .color(visual::colors(ui.ctx()).muted),
        );
    }
    if menu_item(
        ui,
        if rows.len() > 1 {
            "Export selected disks…"
        } else {
            "Export disk…"
        },
        !rows.is_empty() && rows.iter().all(|r| !r.directory),
    ) {
        action = Some(DiskAction::Export);
    }
    if let [a, b] = rows {
        if a.bank.is_some() && b.bank.is_some() {
            ui.separator();
            if menu_item(ui, "Swap these two banks…", true) {
                action = Some(DiskAction::Swap);
            }
        } else if !a.directory && !b.directory && a.bank.is_none() && b.bank.is_none() {
            ui.separator();
            if menu_item(ui, "Swap their indexed slots…", !readonly) {
                action = Some(DiskAction::Swap);
            }
        }
    }
    if let [row] = rows
        && !row.directory
    {
        if row.recognized {
            ui.separator();
            if row.is_hfe() {
                if menu_item(ui, "Convert to raw image…", true) {
                    action = Some(DiskAction::Command("convert-raw"));
                }
            } else {
                if menu_item(ui, "Check FAT12 filesystem", true) {
                    action = Some(DiskAction::Command("image-check"));
                }
                if menu_item(ui, "Inspect FAT12 geometry", true) {
                    action = Some(DiskAction::Command("inspect-fat12"));
                }
                if menu_item(ui, "Convert to HFE…", true) {
                    action = Some(DiskAction::Command("convert-hfe"));
                }
                ui.menu_button("Edit disk contents", |ui| {
                    for (label, cmd) in [
                        ("Change disk label…", "image-label"),
                        ("Add a file…", "image-import"),
                        ("Create a folder…", "image-mkdir"),
                        ("Extract a file…", "image-extract"),
                        ("Rename an entry…", "image-rename"),
                        ("Remove a file or empty folder…", "image-remove"),
                    ] {
                        if menu_item(ui, label, true) {
                            action = Some(DiskAction::Command(cmd));
                        }
                    }
                });
            }
        }
        if row.bank.is_some() {
            ui.separator();
            if menu_item(ui, "Load image into bank…", true) {
                action = Some(DiskAction::Replace);
            }
            if menu_item(ui, "Swap with another bank…", true) {
                action = Some(DiskAction::Swap);
            }
            if menu_item(ui, "Format bank…", true) {
                action = Some(DiskAction::Format);
            }
            if menu_item(ui, "Replace using expected original…", true) {
                action = Some(DiskAction::Command("bank-write"));
            }
            if menu_item(ui, "Replace in a new backup copy…", backup_file) {
                action = Some(DiskAction::Command("bank-replace"));
            }
        } else {
            ui.separator();
            if row.recognized && menu_item(ui, "Assign to indexed slot…", !readonly) {
                action = Some(DiskAction::Command("slot"));
            }
            if !row.is_hfe()
                && crate::fat12::capacity_for(row.size).is_some()
                && menu_item(ui, "Format disk…", !readonly)
            {
                action = Some(DiskAction::Format);
            }
            if menu_item(ui, "Move to trash…", !readonly) {
                action = Some(DiskAction::Trash);
            }
        }
    }
    action
}
#[derive(Clone)]
enum DiskEdit {
    Import(PathBuf, String),
    Mkdir(String),
    Remove(String),
}
impl DiskEdit {
    fn description(&self) -> String {
        match self {
            Self::Import(source, name) => format!("Add {} → {name}", source.display()),
            Self::Mkdir(name) => format!("Create folder {name}"),
            Self::Remove(name) => format!("Remove {name}"),
        }
    }
}

fn batch_form(preview: &Preview, edits: Vec<DiskEdit>, removing: bool) -> Form {
    let mut form = Form::operation(OPS.iter().find(|o| o.command == "image-remove").unwrap());
    form.bind_editor(preview);
    form.values[1] = "batch".into();
    form.title = if removing {
        "Remove selected items"
    } else {
        "Add files and folders"
    }
    .into();
    form.description = format!(
        "{} {}. {} Review the list, then save this batch directly to disk.",
        edits.len(),
        if removing {
            "entries will be removed"
        } else {
            "entries will be added"
        },
        if removing {
            "Selected folders include all their contents."
        } else {
            "DOS names are shown below; existing entries are preserved."
        }
    );
    form.description.push_str(&format!(
        " Target: {}{}",
        preview.target.display(),
        preview
            .bank
            .as_ref()
            .map(|(_, n)| format!(" (bank {n:03})"))
            .unwrap_or_default()
    ));
    form.batch = Some(edits);
    form
}

fn unique_dos_name(source: &str, occupied: &mut BTreeSet<String>) -> String {
    let suggested = suggest_dos_name(source);
    if occupied.insert(suggested.clone()) {
        return suggested;
    }
    let (base, extension) = suggested.rsplit_once('.').unwrap_or((&suggested, ""));
    for n in 1.. {
        let suffix = format!("~{n}");
        let base = &base[..base.len().min(8usize.saturating_sub(suffix.len()))];
        let name = format!(
            "{base}{suffix}{}",
            if extension.is_empty() {
                String::new()
            } else {
                format!(".{extension}")
            }
        );
        if occupied.insert(name.clone()) {
            return name;
        }
    }
    unreachable!()
}

fn plan_imports(preview: &Preview, sources: &[PathBuf]) -> Result<Vec<DiskEdit>, String> {
    let plan = || -> crate::Result<Vec<DiskEdit>> {
        let mut edits = vec![];
        let mut pending = vec![(preview.directory.clone(), sources.to_vec(), true)];
        while let Some((directory, mut sources, existing)) = pending.pop() {
            sources.sort();
            sources.dedup();
            let mut occupied: BTreeSet<String> = if existing {
                crate::fat12::directory_entries(&preview.image, &directory)?
                    .into_iter()
                    .map(|e| e.name.to_ascii_uppercase())
                    .collect()
            } else {
                BTreeSet::new()
            };
            for source in sources {
                let metadata = fs::symlink_metadata(&source)?;
                if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
                    return Err(format!(
                        "Cannot import link or special file: {}",
                        source.display()
                    )
                    .into());
                }
                let name = source
                    .file_name()
                    .ok_or("Missing source name")?
                    .to_string_lossy();
                let name = inner_path(&directory, &unique_dos_name(&name, &mut occupied));
                if metadata.is_dir() {
                    edits.push(DiskEdit::Mkdir(name.clone()));
                    let children = fs::read_dir(&source)?
                        .map(|e| e.map(|e| e.path()))
                        .collect::<std::io::Result<Vec<_>>>()?;
                    pending.push((name, children, false));
                } else {
                    edits.push(DiskEdit::Import(source, name));
                }
            }
        }
        Ok(edits)
    };
    plan().map_err(|e| e.to_string())
}

fn plan_removals(preview: &Preview) -> Result<Vec<DiskEdit>, String> {
    let plan = || -> crate::Result<Vec<DiskEdit>> {
        crate::fat12::check(&preview.image)?;
        let mut edits = vec![];
        let mut pending = vec![];
        for name in &preview.checked {
            let entry = preview
                .entries
                .iter()
                .find(|e| &e.name == name)
                .ok_or("Selected entry no longer exists")?;
            pending.push((inner_path(&preview.directory, name), entry.directory, false));
        }
        while let Some((path, directory, visited)) = pending.pop() {
            if directory && !visited {
                pending.push((path.clone(), true, true));
                for entry in crate::fat12::directory_entries(&preview.image, &path)? {
                    pending.push((inner_path(&path, &entry.name), entry.directory, false));
                }
            } else {
                edits.push(DiskEdit::Remove(path));
            }
        }
        Ok(edits)
    };
    plan().map_err(|e| e.to_string())
}

fn stage_batch(input: &Path, edits: &[DiskEdit], temp: &Path, output: &Path) -> crate::Result<()> {
    if edits.is_empty() {
        return Err("Select at least one item".into());
    }
    let mut current = input.to_path_buf();
    for (index, edit) in edits.iter().enumerate() {
        let next = if index + 1 == edits.len() {
            output.to_path_buf()
        } else {
            temp.join(format!("step{index}.img"))
        };
        let result = match edit {
            DiskEdit::Import(source, name) => {
                if !fs::symlink_metadata(source)?.is_file() {
                    return Err("Import source changed or is not a regular file".into());
                }
                crate::fat12::import(&current, source, name, &next)
            }
            DiskEdit::Mkdir(name) => crate::fat12::mkdir(&current, name, &next),
            DiskEdit::Remove(name) => crate::fat12::remove(&current, name, &next),
        };
        result.map_err(|e| {
            format!(
                "{}: {e}. Batch not saved; disk unchanged.",
                edit.description()
            )
        })?;
        if current != input {
            fs::remove_file(&current)?;
        }
        current = next;
    }
    Ok(())
}

fn is_disk_edit(command: &str) -> bool {
    matches!(
        command,
        "image-import" | "image-mkdir" | "image-remove" | "image-rename" | "image-label"
    )
}

fn open_editor(row: &DiskRow, profile: &str) -> Result<Preview, String> {
    let (source, bank_temp) = disk_snapshot(row, profile)?;
    let (image, temp) = if let Some(temp) = bank_temp {
        (source, temp)
    } else {
        let temp = Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?);
        let image = temp.path().join("disk.img");
        crate::copy_new(&source, &image).map_err(|e| e.to_string())?;
        (image, temp)
    };
    Ok(Preview {
        target: row.path.clone(),
        bank: row.bank.map(|n| (profile.into(), n)),
        entries: crate::fat12::directory_entries(&image, "/")
            .map_err(|e| format!("Cannot edit this disk as FAT12: {e}"))?,
        label: crate::fat12::volume_label(&image).map_err(|e| e.to_string())?,
        image,
        temp: Some(temp),
        directory: "/".into(),
        selected: None,
        checked: BTreeSet::new(),
        title: row.name.clone(),
    })
}

fn save_editor(form: &Form) -> Result<Preview, String> {
    let preview = form.editor.as_ref().ok_or("No disk is open")?;
    let edit = || -> crate::Result<Preview> {
        // Staging stays in a temporary directory; it is never a user-managed
        // backup or an image the user must apply later.
        let temp = Arc::new(tempfile::tempdir()?);
        let image = temp.path().join("disk.img");
        let input = &preview.image;
        let v = &form.values;
        if let Some(edits) = &form.batch {
            stage_batch(input, edits, temp.path(), &image)?;
        } else {
            match form.command.as_str() {
                "image-import" => crate::fat12::import(input, Path::new(&v[1]), &v[2], &image)?,
                "image-mkdir" => crate::fat12::mkdir(input, &v[1], &image)?,
                "image-remove" => crate::fat12::remove(input, &v[1], &image)?,
                "image-rename" => crate::fat12::rename(input, &v[1], &v[2], &image)?,
                "image-label" => crate::fat12::set_label(input, &v[1], &image)?,
                _ => return Err("Unsupported disk edit".into()),
            }
        }
        let entries = crate::fat12::directory_entries(&image, &preview.directory)?;
        let label = crate::fat12::volume_label(&image)?;
        crate::banks::edit_disk(
            &preview.target,
            preview.bank.as_ref().map(|(p, n)| (p.as_str(), *n)),
            &fs::read(input)?,
            &fs::read(&image)?,
        )?;
        Ok(Preview {
            image,
            temp: Some(temp),
            entries,
            label,
            selected: None,
            checked: BTreeSet::new(),
            ..preview.clone()
        })
    };
    edit().map_err(|e| e.to_string())
}

fn disk_snapshot(
    row: &DiskRow,
    profile: &str,
) -> Result<(PathBuf, Option<Arc<tempfile::TempDir>>), String> {
    if let Some(bank) = row.bank {
        let temp = Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?);
        let image = temp.path().join("disk.img");
        backend(vec![
            "bank-export".into(),
            row.path.to_string_lossy().into_owned(),
            profile.into(),
            bank.to_string(),
            image.to_string_lossy().into_owned(),
        ])?;
        Ok((image, Some(temp)))
    } else {
        Ok((row.path.clone(), None))
    }
}
/// Validate a "load image into bank" request and turn it into a short review.
/// Problems are returned to the editable form with an actionable message.
fn prepare_bank_load(mut form: Form, current: &DiskRow) -> Event {
    match describe_bank_load(&form, current) {
        Ok(summary) => {
            form.summary = summary;
            form.review = true;
            Event::Prepared(form)
        }
        Err(error) => Event::FormFailed(form, error),
    }
}
fn describe_bank_load(form: &Form, current: &DiskRow) -> Result<Vec<String>, String> {
    let [target, profile, bank, image, recovery] = form.values.as_slice() else {
        return Err("Incomplete bank form".into());
    };
    let bank: u16 = bank.parse().map_err(|_| "Invalid bank number")?;
    let payload = bank_payload(profile)?;
    let image = Path::new(image);
    let name = image
        .file_name()
        .unwrap_or(image.as_os_str())
        .to_string_lossy();
    if !fs::metadata(image).is_ok_and(|m| m.is_file()) {
        return Err(format!("{} is not a readable file", image.display()));
    }
    if recovery.trim().is_empty() || Path::new(recovery).exists() {
        return Err(
            "Choose a new file for the recovery copy; existing files are never overwritten".into(),
        );
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let raw = raw_for_bank(image, profile, temp.path())?;
    let size = fs::metadata(&raw).map_err(|e| e.to_string())?.len();
    if size != payload {
        let hint = ["gotek1440", "gotek720"]
            .into_iter()
            .find(|p| p != profile && bank_payload(p) == Ok(size))
            .map(|p| {
                format!(" It fits the {p} layout; switch the bank profile if your USB uses it.")
            })
            .unwrap_or_else(|| " Choose a raw floppy image of exactly that size.".into());
        return Err(format!(
            "{name} is {} ({size} bytes), but a {profile} bank holds exactly {} ({payload} bytes).{hint}",
            format_size(size),
            format_size(payload),
        ));
    }
    let label = crate::fat12::volume_label(&raw)
        .ok()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    let converted = raw != image;
    Ok(vec![
        format!("Bank {bank:03} on {target}"),
        format!("Currently: {} · {}", current.name, current.kind),
        format!(
            "New image: {name}{}{} · {}",
            label.map(|l| format!(" (label {l})")).unwrap_or_default(),
            if converted { " · converted from HFE" } else { "" },
            format_size(size)
        ),
        format!("Recovery copy: {recovery}"),
        "The current bank is saved first, then the new image is written and verified. Other banks are not touched; the USB must be unmounted.".into(),
    ])
}
fn bank_payload(profile: &str) -> Result<u64, String> {
    match profile {
        "gotek1440" => Ok(1440 * 1024),
        "gotek720" => Ok(720 * 1024),
        _ => Err(format!("Unknown bank profile {profile}")),
    }
}
/// Banks hold raw sectors; HFE images are decoded into `dir` first.
fn raw_for_bank(image: &Path, profile: &str, dir: &Path) -> Result<PathBuf, String> {
    if !image
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("hfe"))
    {
        return Ok(image.to_path_buf());
    }
    let geometry = if profile == "gotek720" {
        "pc720"
    } else {
        "pc1440"
    };
    let raw = dir.join("converted-from-hfe.img");
    crate::convert::to_raw(image, &raw, geometry)
        .map_err(|e| format!("Could not convert the HFE image for this bank: {e}"))?;
    Ok(raw)
}
/// A durable per-user folder, so recovery copies never need to be chosen by hand.
fn recovery_dir() -> PathBuf {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|h| h.join(".local/share")))
    };
    base.unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("RustGotek")
        .join("recovery")
}
fn default_recovery_path(bank: u16) -> PathBuf {
    new_image_path(
        &recovery_dir(),
        &format!("bank{bank:03}-before-{}", utc_timestamp()),
    )
}
fn utc_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}
fn toolbar_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14., 64.), egui::Sense::hover());
    ui.painter().line_segment(
        [
            rect.center_top() + egui::vec2(0., 12.),
            rect.center_bottom() - egui::vec2(0., 12.),
        ],
        egui::Stroke::new(1.0f32, visual::colors(ui.ctx()).border),
    );
}
fn new_image_path(directory: &Path, stem: &str) -> PathBuf {
    let mut candidate = directory.join(format!("{stem}.img"));
    let mut n = 2;
    while candidate.exists() {
        candidate = directory.join(format!("{stem} {n}.img"));
        n += 1;
    }
    candidate
}
fn suggest_dos_name(name: &str) -> String {
    if crate::fat12::validate_short_name(name).is_ok() {
        return name.to_ascii_uppercase();
    }
    let (base, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let clean = |value: &str, limit: usize| {
        value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(limit)
            .collect::<String>()
            .to_ascii_uppercase()
    };
    let mut base = clean(base, 8);
    if base.is_empty() {
        base = "FILE".into();
    }
    let extension = clean(extension, 3);
    if extension.is_empty() {
        base
    } else {
        format!("{base}.{extension}")
    }
}

fn inner_path(directory: &str, name: &str) -> String {
    if directory == "/" {
        name.into()
    } else {
        format!("{directory}/{name}")
    }
}
fn parent_directory(directory: &str) -> String {
    directory
        .rsplit_once('/')
        .map(|(parent, _)| {
            if parent.is_empty() {
                "/".into()
            } else {
                parent.into()
            }
        })
        .unwrap_or_else(|| "/".into())
}
fn is_image(name: &str) -> bool {
    crate::image_extension(Path::new(name)).is_ok()
}
fn file_kind(name: &str) -> String {
    if is_image(name) {
        format!(
            "{} image",
            Path::new(name)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_uppercase()
        )
    } else {
        "File".into()
    }
}
fn export_row(row: &DiskRow, profile: &str, dest: &Path) -> Result<(), String> {
    if let Some(bank) = row.bank {
        backend(vec![
            "bank-export".into(),
            row.path.to_string_lossy().into_owned(),
            profile.into(),
            bank.to_string(),
            dest.to_string_lossy().into_owned(),
        ])
        .map(|_| ())
    } else {
        crate::copy_new(&row.path, dest).map_err(|e| e.to_string())
    }
}
fn backend(args: Vec<String>) -> Result<String, String> {
    // Raw-device operations share the GUI's authentication identity. Running
    // them as separate CLI processes causes needless polkit prompts.
    let words: Vec<_> = args.iter().map(String::as_str).collect();
    let device_operation = || -> crate::Result<Option<String>> {
        let message = match words.as_slice() {
            ["bank-export", source, profile, bank, dest] => {
                crate::banks::export(Path::new(source), profile, bank.parse()?, Path::new(dest))?;
                format!("Exported bank {bank} to {dest}")
            }
            ["bank-backup", source, dest] => {
                crate::banks::backup(Path::new(source), Path::new(dest))?;
                format!(
                    "Saved whole-device backup to {dest}. Use Verify backup to compare it with the source."
                )
            }
            ["bank-verify", source, backup] => {
                crate::banks::verify(Path::new(source), Path::new(backup))?;
                "Backup verified byte for byte".into()
            }
            ["bank-list", source, profile] => {
                let catalog = crate::banks::catalog(Path::new(source), profile)?;
                format!(
                    "Source: {source}\nProfile: {profile}\nCapacity: {} bytes; stride: {}; payload: {}\nBoot headers only; not a filesystem check.\n{}",
                    catalog.size,
                    catalog.stride,
                    catalog.payload,
                    catalog
                        .rows
                        .iter()
                        .map(|e| format!("{:03}  {}  {}", e.number, e.status, e.label))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            [
                "bank-write",
                target,
                profile,
                bank,
                image,
                expected,
                recovery,
                "--confirm-write",
            ] => {
                crate::banks::write_bank(
                    Path::new(target),
                    profile,
                    bank.parse()?,
                    Path::new(image),
                    Path::new(expected),
                    Path::new(recovery),
                )?;
                format!("Bank {bank} written and verified. Recovery image: {recovery}")
            }
            [
                "bank-swap",
                target,
                profile,
                a,
                b,
                recovery_a,
                recovery_b,
                "--confirm-write",
            ] => {
                crate::banks::swap_banks(
                    Path::new(target),
                    profile,
                    a.parse()?,
                    b.parse()?,
                    Path::new(recovery_a),
                    Path::new(recovery_b),
                )?;
                format!(
                    "Banks {a:0>3} and {b:0>3} swapped and verified. Recovery images: {recovery_a} and {recovery_b}"
                )
            }
            [
                "bank-format",
                target,
                profile,
                bank,
                label,
                recovery,
                "--confirm-write",
            ] => {
                crate::banks::format_bank(
                    Path::new(target),
                    profile,
                    bank.parse()?,
                    if *label == "-" { "" } else { label },
                    Path::new(recovery),
                )?;
                format!("Bank {bank:0>3} formatted and verified. Recovery image: {recovery}")
            }
            ["image-format", image, label, recovery, "--confirm-write"] => {
                crate::banks::format_image(
                    Path::new(image),
                    if *label == "-" { "" } else { label },
                    Path::new(recovery),
                )?;
                format!("{image} formatted and verified. Recovery image: {recovery}")
            }
            [
                "device-restore",
                target,
                image,
                expected,
                recovery,
                "--confirm-write",
            ] => {
                crate::banks::restore_device(
                    Path::new(target),
                    Path::new(image),
                    Path::new(expected),
                    Path::new(recovery),
                )?;
                format!("Device restored and verified. Recovery image: {recovery}")
            }
            _ => return Ok(None),
        };
        Ok(Some(message))
    };
    if let Some(message) = device_operation().map_err(|e| e.to_string())? {
        return Ok(message);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(exe);
    command.args(args).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if text.len() > 65536 {
        let mut end = 65536;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[Log limited to 64 KiB]");
    }
    if output.status.success() {
        Ok(if text.trim().is_empty() {
            "Done".into()
        } else {
            text
        })
    } else {
        Err(if text.trim().is_empty() {
            format!("Operation failed: {}", output.status)
        } else {
            text
        })
    }
}
fn read_listing(root: &Path, directory: &Path) -> Result<Listing, String> {
    let read = || -> crate::Result<Listing> {
        let root = fs::canonicalize(root)?;
        let directory = fs::canonicalize(directory)?;
        if !root.is_dir() || !directory.starts_with(&root) {
            return Err("Choose a directory inside the selected mounted media".into());
        }
        let mut entries = vec![];
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() || (!kind.is_file() && !kind.is_dir()) {
                continue;
            }
            let path = entry.path();
            entries.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                relative: path.strip_prefix(&root)?.to_string_lossy().into_owned(),
                directory: kind.is_dir(),
                size: entry.metadata()?.len(),
            });
        }
        entries.sort_by(|a, b| {
            b.directory
                .cmp(&a.directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let read_only = mount_read_only(&directory);
        Ok(Listing {
            root,
            directory,
            entries,
            read_only,
        })
    };
    read().map_err(|e| e.to_string())
}

fn mount_read_only(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    if let Ok(mounts) = fs::read_to_string("/proc/self/mountinfo") {
        return linux_mount_read_only(path, &mounts);
    }
    fs::metadata(path).is_ok_and(|m| m.permissions().readonly())
}

#[cfg(any(target_os = "linux", test))]
fn linux_mount_read_only(path: &Path, mounts: &str) -> bool {
    mounts
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 6 {
                return None;
            }
            let mount = fields[4]
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\");
            let mount = PathBuf::from(mount);
            path.starts_with(&mount).then_some((
                mount.as_os_str().len(),
                fields[5].split(',').any(|v| v == "ro"),
            ))
        })
        .max_by_key(|(length, _)| *length)
        .is_some_and(|(_, ro)| ro)
}

fn discover_media() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RUSTGOTEK_MEDIA")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
    {
        return Some(path);
    }
    #[cfg(windows)]
    if let Some(path) = crate::windev::media_roots().into_iter().next() {
        return Some(path);
    }
    let user = std::env::var("USER").unwrap_or_default();
    for base in [
        PathBuf::from("/run/media").join(&user),
        PathBuf::from("/media").join(&user),
        PathBuf::from("/Volumes"),
    ] {
        if let Ok(entries) = fs::read_dir(base) {
            let mut paths: Vec<_> = entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect();
            paths.sort();
            if let Some(path) = paths.into_iter().next() {
                return Some(path);
            }
        }
    }
    None
}
fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / 1048576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        // Bank mode starts idle and never probes a real device.
        App::new(Some("unused-test-device".into()), &egui::Context::default())
    }

    #[test]
    fn contents_editor_exposes_label_and_prefills_rename_in_current_folder() {
        let temp = tempfile::tempdir().unwrap();
        let blank = temp.path().join("blank.img");
        let image = temp.path().join("disk.img");
        crate::fat12::create(&blank, 1440).unwrap();
        crate::fat12::mkdir(&blank, "SAMPLES", &image).unwrap();
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = app();
        app.preview = Some(Preview {
            target: image.clone(),
            bank: None,
            label: "MY DISK".into(),
            image: image.clone(),
            directory: "/".into(),
            entries: crate::fat12::directory_entries(&image, "/").unwrap(),
            temp: None,
            selected: Some("SAMPLES".into()),
            checked: BTreeSet::from(["SAMPLES".into()]),
            title: "disk.img".into(),
        });
        render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        for label in [
            "Change label",
            "Add files",
            "Add folders",
            "New folder",
            "Open folder",
            "Rename",
            "Remove selected",
        ] {
            assert!(
                text_position(&output.shapes, label).is_some(),
                "{label} must be visible"
            );
        }
        let label = text_position(&output.shapes, "Change label").unwrap();
        click(&mut app, &ctx, label, egui::PointerButton::Primary);
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.command, "image-label");
        assert_eq!(form.values[1], "MY DISK");
        assert!(form.args("").is_ok());
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_position(&output.shapes, "Save changes").is_some());
        assert!(text_position(&output.shapes, "Save edited copy as").is_none());
        app.form = None;
        render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        let rename = text_position(&output.shapes, "Rename").unwrap();
        click(&mut app, &ctx, rename, egui::PointerButton::Primary);
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.values[1], "SAMPLES");
        assert_eq!(form.values[2], "SAMPLES");
        assert_eq!(form.editor.as_ref().unwrap().target, image);
    }

    #[test]
    fn device_worker_exports_and_verifies_without_a_cli_subprocess() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("banks.bin");
        let exported = temp.path().join("disk.img");
        let backup = temp.path().join("backup.bin");
        crate::banks::create_layout(&source, "gotek720", 2, "minimum").unwrap();
        let source = source.to_str().unwrap();
        let run = |args: &[&str]| backend(args.iter().map(|s| s.to_string()).collect());
        run(&[
            "bank-export",
            source,
            "gotek720",
            "1",
            exported.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(crate::fat12::volume_label(&exported).unwrap(), "FDISK001");
        run(&["bank-backup", source, backup.to_str().unwrap()]).unwrap();
        run(&["bank-verify", source, backup.to_str().unwrap()]).unwrap();
        assert!(
            run(&[
                "bank-export",
                source,
                "gotek720",
                "1",
                exported.to_str().unwrap()
            ])
            .is_err()
        );
    }

    #[test]
    fn import_suggestions_and_inline_validation_handle_long_names() {
        assert_eq!(suggest_dos_name("My long sample.wav"), "MYLONGSA.WAV");
        assert_eq!(suggest_dos_name("a-b.txt"), "A-B.TXT");
        assert!(crate::fat12::validate_short_name(&suggest_dos_name("🎵.wav")).is_ok());
        let op = OPS.iter().find(|o| o.command == "image-import").unwrap();
        let mut form = Form::operation(op);
        form.values = vec![
            "disk.img".into(),
            "source.txt".into(),
            "DIR/TOOLONGNAME.TXT".into(),
            "copy.img".into(),
        ];
        assert!(form.args("").is_err());
        form.values[2] = "DIR/NOTES.TXT".into();
        assert!(form.args("").is_ok());
    }

    fn edit_form(preview: &Preview, command: &str, fields: &[&str]) -> Form {
        let mut form = Form::operation(OPS.iter().find(|op| op.command == command).unwrap());
        form.bind_editor(preview);
        for (value, field) in form.values[1..].iter_mut().zip(fields) {
            *value = (*field).into();
        }
        form
    }

    fn editor_row(path: &Path, bank: Option<u16>) -> DiskRow {
        DiskRow {
            key: "test".into(),
            name: "Test disk".into(),
            kind: "FAT12".into(),
            size: 0,
            path: path.into(),
            bank,
            directory: false,
            recognized: true,
        }
    }

    #[test]
    fn direct_editor_changes_the_same_image_and_keeps_current_directory() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("disk.img");
        let file = temp.path().join("sample.txt");
        crate::fat12::create(&disk, 1440).unwrap();
        fs::write(&file, b"sample content").unwrap();
        let mut preview = open_editor(&editor_row(&disk, None), "gotek1440").unwrap();
        preview = save_editor(&edit_form(&preview, "image-label", &["MY SAMPLES"])).unwrap();
        assert_eq!(crate::fat12::volume_label(&disk).unwrap(), "MY SAMPLES");
        preview = save_editor(&edit_form(&preview, "image-mkdir", &["SAMPLES"])).unwrap();
        preview.directory = "SAMPLES".into();
        preview = save_editor(&edit_form(
            &preview,
            "image-import",
            &[file.to_str().unwrap(), "SAMPLES/NOTE.TXT"],
        ))
        .unwrap();
        assert_eq!(preview.directory, "SAMPLES");
        assert_eq!(
            crate::fat12::directory_entries(&disk, "SAMPLES").unwrap()[0].name,
            "NOTE.TXT"
        );
        preview = save_editor(&edit_form(
            &preview,
            "image-rename",
            &["SAMPLES/NOTE.TXT", "NEW.TXT"],
        ))
        .unwrap();
        assert_eq!(preview.entries[0].name, "NEW.TXT");
        preview = save_editor(&edit_form(&preview, "image-remove", &["SAMPLES/NEW.TXT"])).unwrap();
        assert!(
            crate::fat12::directory_entries(&disk, "SAMPLES")
                .unwrap()
                .is_empty()
        );
        preview.directory = "/".into();
        let preview = save_editor(&edit_form(&preview, "image-remove", &["SAMPLES"])).unwrap();
        assert_eq!(preview.target, disk);
        assert!(
            crate::fat12::directory_entries(&disk, "/")
                .unwrap()
                .is_empty()
        );
        crate::fat12::check(&disk).unwrap();
        assert_eq!(
            fs::read_dir(temp.path()).unwrap().count(),
            2,
            "no backup or edited copies alongside the disk"
        );
    }

    #[test]
    fn direct_bank_edits_preserve_other_banks_and_reject_stale_snapshots() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("banks.bin");
        crate::banks::create_layout(&disk, "gotek1440", 3, "minimum").unwrap();
        let original = fs::read(&disk).unwrap();
        let row = editor_row(&disk, Some(1));
        let preview = open_editor(&row, "gotek1440").unwrap();
        let stale = preview.clone();
        let preview = save_editor(&edit_form(&preview, "image-label", &["DIRECT"])).unwrap();
        let preview = save_editor(&edit_form(&preview, "image-mkdir", &["SAMPLES"])).unwrap();
        let result = fs::read(&disk).unwrap();
        let offset = 0x180000;
        let end = offset + 1440 * 1024;
        assert_eq!(&result[..offset], &original[..offset]);
        assert_eq!(&result[end..], &original[end..]);
        assert_eq!(&result[offset..end], fs::read(&preview.image).unwrap());
        assert_eq!(
            crate::banks::catalog(&disk, "gotek1440").unwrap().rows[1].label,
            "DIRECT"
        );
        let error = save_editor(&edit_form(&stale, "image-label", &["STALE"]))
            .err()
            .unwrap();
        assert!(error.contains("Disk changed"), "{error}");
        assert_eq!(fs::read(&disk).unwrap(), result);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn direct_image_edits_refuse_conflicts_locks_and_invalid_edits() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("disk.img");
        crate::fat12::create(&disk, 720).unwrap();
        let preview = open_editor(&editor_row(&disk, None), "gotek720").unwrap();
        let original = fs::read(&disk).unwrap();
        assert!(save_editor(&edit_form(&preview, "image-mkdir", &["INVALID LONG NAME"])).is_err());
        assert_eq!(fs::read(&disk).unwrap(), original);
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&disk)
            .unwrap();
        lock.try_lock().unwrap();
        assert!(save_editor(&edit_form(&preview, "image-label", &["LOCKED"])).is_err());
        assert_eq!(fs::read(&disk).unwrap(), original);
        drop(lock);
        let mut changed = original;
        changed[39] ^= 1;
        fs::write(&disk, &changed).unwrap();
        assert!(
            save_editor(&edit_form(&preview, "image-label", &["STALE"]))
                .err()
                .unwrap()
                .contains("Disk changed")
        );
        assert_eq!(fs::read(&disk).unwrap(), changed);
    }

    #[test]
    fn batch_imports_multiple_files_and_folder_trees_then_removes_them() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("disk.img");
        crate::fat12::create(&disk, 1440).unwrap();
        let sources = temp.path().join("sources");
        fs::create_dir_all(sources.join("Sounds/Nested/Empty")).unwrap();
        fs::create_dir_all(sources.join("Other")).unwrap();
        fs::write(sources.join("Sounds/Nested/tone.wav"), b"sound").unwrap();
        fs::write(sources.join("a very long name.txt"), b"one").unwrap();
        fs::write(sources.join("a very long note.txt"), b"two").unwrap();
        let preview = open_editor(&editor_row(&disk, None), "gotek1440").unwrap();
        let paths = fs::read_dir(&sources)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        let edits = plan_imports(&preview, &paths).unwrap();
        let mut preview = save_editor(&batch_form(&preview, edits, false)).unwrap();
        let names: BTreeSet<_> = preview.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            BTreeSet::from(["AVERYLON.TXT", "AVERYL~1.TXT", "SOUNDS", "OTHER"])
        );
        assert!(
            crate::fat12::directory_entries(&disk, "SOUNDS/NESTED")
                .unwrap()
                .iter()
                .any(|e| e.name == "EMPTY" && e.directory)
        );
        let extracted = temp.path().join("tone.wav");
        crate::fat12::extract(&disk, "SOUNDS/NESTED/TONE.WAV", &extracted).unwrap();
        assert_eq!(fs::read(extracted).unwrap(), b"sound");
        preview.checked = preview.entries.iter().map(|e| e.name.clone()).collect();
        let edits = plan_removals(&preview).unwrap();
        assert_eq!(edits.len(), 7);
        let preview = save_editor(&batch_form(&preview, edits, true)).unwrap();
        assert!(preview.entries.is_empty());
        crate::fat12::check(&disk).unwrap();
    }

    #[test]
    fn failed_batches_leave_disk_unchanged_and_imports_preserve_existing_names() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("disk.img");
        crate::fat12::create(&disk, 360).unwrap();
        let file = temp.path().join("note.txt");
        fs::write(&file, b"keep").unwrap();
        let preview = open_editor(&editor_row(&disk, None), "gotek1440").unwrap();
        let edits = plan_imports(&preview, std::slice::from_ref(&file)).unwrap();
        let preview = save_editor(&batch_form(&preview, edits, false)).unwrap();
        let edits = plan_imports(&preview, std::slice::from_ref(&file)).unwrap();
        assert!(matches!(&edits[0], DiskEdit::Import(_, name) if name == "NOTE~1.TXT"));
        let original = fs::read(&disk).unwrap();
        let huge = temp.path().join("huge.bin");
        fs::write(&huge, vec![1; 400 * 1024]).unwrap();
        let edits = vec![
            DiskEdit::Mkdir("NEW".into()),
            DiskEdit::Import(huge, "NEW/HUGE.BIN".into()),
        ];
        assert!(save_editor(&batch_form(&preview, edits, false)).is_err());
        assert_eq!(fs::read(&disk).unwrap(), original);
        let edits = vec![
            DiskEdit::Remove("NOTE.TXT".into()),
            DiskEdit::Remove("MISSING".into()),
        ];
        assert!(save_editor(&batch_form(&preview, edits, true)).is_err());
        assert_eq!(fs::read(&disk).unwrap(), original);
    }

    #[test]
    fn bank_batches_commit_only_the_selected_payload() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("banks.bin");
        crate::banks::create_layout(&disk, "gotek720", 3, "minimum").unwrap();
        let original = fs::read(&disk).unwrap();
        let preview = open_editor(&editor_row(&disk, Some(1)), "gotek720").unwrap();
        let edits = vec![DiskEdit::Mkdir("ONE".into()), DiskEdit::Mkdir("TWO".into())];
        let preview = save_editor(&batch_form(&preview, edits, false)).unwrap();
        assert_eq!(preview.entries.len(), 2);
        let updated = fs::read(&disk).unwrap();
        assert_eq!(&updated[..0x100000], &original[..0x100000]);
        assert_eq!(
            &updated[0x100000 + 720 * 1024..],
            &original[0x100000 + 720 * 1024..]
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn folder_import_refuses_symbolic_links() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("disk.img");
        crate::fat12::create(&disk, 720).unwrap();
        let folder = temp.path().join("folder");
        fs::create_dir(&folder).unwrap();
        std::os::unix::fs::symlink(&folder, folder.join("loop")).unwrap();
        let preview = open_editor(&editor_row(&disk, None), "gotek720").unwrap();
        assert!(
            plan_imports(&preview, &[folder])
                .err()
                .unwrap()
                .contains("link")
        );
    }

    #[test]
    fn successful_ejection_clears_sources_without_scanning_the_removed_drive() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.root = "/media/USB".into();
        app.source = "/dev/test-usb".into();
        app.selected.insert("bank:051".into());
        app.busy = true;
        app.sender.send(Ok(Event::Ejected)).unwrap();
        app.receive(&ctx);
        assert!(!app.busy);
        assert!(!app.error);
        assert!(app.root.is_empty() && app.source.is_empty());
        assert!(app.rows.is_empty() && app.selected.is_empty());
        assert!(app.preview.is_none());
        assert!(app.status.contains("safe to unplug"));
    }

    #[test]
    fn writes_wait_for_review_and_cancel_discards_them() {
        let ctx = egui::Context::default();
        for command in ["bank-write", "device-restore"] {
            let mut app = app();
            app.open_form(command);
            let form = app.form.as_mut().unwrap();
            form.values.iter_mut().for_each(|v| *v = "example".into());
            assert!(form.confirm);
            assert_eq!(form.args("").unwrap().last().unwrap(), "--confirm-write");
            app.execute_form(&ctx);
            assert!(!app.busy, "unreviewed writes must not start a worker");
            assert!(app.form.is_some());
            app.form = None;
            app.execute_form(&ctx);
            assert!(!app.busy, "cancelled writes must not start a worker");
        }
        let mut app = app();
        app.rows = vec![DiskRow {
            key: "bank:099".into(),
            name: "Test".into(),
            kind: "FAT12".into(),
            size: 1_474_560,
            path: "unused-test-device".into(),
            bank: Some(99),
            directory: false,
            recognized: true,
        }];
        app.replace_bank_form(&app.rows[0].clone());
        app.form.as_mut().unwrap().values[3] = "replacement.img".into();
        app.form.as_mut().unwrap().values[4] = "recovery.img".into();
        app.execute_form(&ctx);
        assert!(!app.busy);
        assert_eq!(app.form.as_ref().unwrap().values[2], "99");
    }

    /// Drive background work to completion, including follow-up refreshes.
    fn settle(app: &mut App, ctx: &egui::Context) {
        for _ in 0..1000 {
            app.receive(ctx);
            if !app.busy {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("worker did not finish: {}", app.status);
    }
    fn labelled_disk(dir: &Path, kib: usize, label: &str) -> PathBuf {
        let blank = dir.join(format!("blank{kib}.img"));
        let disk = dir.join(format!("{label}.img"));
        crate::fat12::create(&blank, kib).unwrap();
        crate::fat12::set_label(&blank, label, &disk).unwrap();
        disk
    }

    #[test]
    fn dropped_image_loads_into_bank_after_one_review_with_automatic_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let device = temp.path().join("usb.bin");
        crate::banks::create_layout(&device, "gotek720", 3, "minimum").unwrap();
        let image = labelled_disk(temp.path(), 720, "GAME DISK");
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = App::new(Some(device.to_string_lossy().into()), &ctx);
        app.profile = "gotek720".into();
        app.scan(&ctx);
        settle(&mut app, &ctx);
        assert_eq!(app.rows.len(), 3);
        app.selected = BTreeSet::from(["bank:001".into()]);
        let hovering = ctx.run(
            egui::RawInput {
                hovered_files: vec![egui::HoveredFile {
                    path: Some(image.clone()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            |ctx| app.ui(ctx),
        );
        assert!(text_position(&hovering.shapes, "Load into bank 001 · FDISK001").is_some());
        let _ = ctx.run(
            egui::RawInput {
                dropped_files: vec![egui::DroppedFile {
                    path: Some(image.clone()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            |ctx| app.ui(ctx),
        );
        settle(&mut app, &ctx);
        let form = app.form.as_mut().expect("drop should open the review");
        assert!(
            form.review,
            "a valid drop goes straight to review: {}",
            app.log
        );
        assert!(form.summary[0].starts_with("Bank 001"));
        assert!(form.summary.iter().any(|l| l.contains("GAME DISK")));
        assert!(form.values[4].contains("bank001-before-"));
        let recovery = temp.path().join("recovery").join("bank001.img");
        form.values[4] = recovery.to_string_lossy().into();
        app.execute_form(&ctx);
        settle(&mut app, &ctx);
        assert!(!app.error, "{}", app.log);
        let exported = |bank: u16| {
            let out = temp
                .path()
                .join(format!("check{bank}-{}.img", app.log.len()));
            crate::banks::export(&device, "gotek720", bank, &out).unwrap();
            crate::fat12::volume_label(&out).unwrap()
        };
        assert_eq!(exported(1), "GAME DISK");
        assert_eq!(exported(0), "FDISK000");
        assert_eq!(exported(2), "FDISK002");
        assert_eq!(crate::fat12::volume_label(&recovery).unwrap(), "FDISK001");
    }

    #[test]
    fn bank_load_review_explains_size_mismatch_and_converts_hfe() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app();
        app.profile = "gotek720".into();
        let row = editor_row(Path::new("unused-test-device"), Some(4));
        app.replace_bank_form(&row);
        let mut form = app.form.take().unwrap();
        assert_eq!(form.title, "Load image into bank 004");
        form.values[4] = temp.path().join("recovery.img").to_string_lossy().into();

        form.values[3] = labelled_disk(temp.path(), 1440, "BIG")
            .to_string_lossy()
            .into();
        let error = describe_bank_load(&form, &row).unwrap_err();
        assert!(error.contains("fits the gotek1440 layout"), "{error}");

        let raw = labelled_disk(temp.path(), 720, "FROM HFE");
        let hfe = temp.path().join("disk.hfe");
        crate::convert::to_hfe(&raw, &hfe, "pc720").unwrap();
        form.values[3] = hfe.to_string_lossy().into();
        let summary = describe_bank_load(&form, &row).unwrap();
        assert!(
            summary
                .iter()
                .any(|l| l.contains("converted from HFE") && l.contains("FROM HFE"))
        );

        fs::write(&form.values[4], b"keep").unwrap();
        assert!(
            describe_bank_load(&form, &row)
                .unwrap_err()
                .contains("never overwritten")
        );
    }

    #[test]
    fn drops_name_their_target_and_refuse_ambiguous_banks() {
        let mut app = app();
        assert!(app.drop_intent().unwrap_err().contains("Scan banks first"));
        app.rows = (0..2)
            .map(|n| DiskRow {
                key: format!("bank:{n:03}"),
                name: format!("FDISK{n:03}"),
                kind: "FAT12".into(),
                size: 737_280,
                path: "unused-test-device".into(),
                bank: Some(n),
                directory: false,
                recognized: true,
            })
            .collect();
        assert!(app.drop_intent().unwrap_err().contains("select a bank"));
        app.selected = BTreeSet::from(["bank:000".into()]);
        assert_eq!(app.drop_intent().unwrap(), "Load into bank 000 · FDISK000");
        app.drop_row = Some("bank:001".into());
        assert_eq!(app.drop_intent().unwrap(), "Load into bank 001 · FDISK001");
    }

    #[test]
    fn forms_preserve_literal_paths_and_reject_incomplete_values() {
        let op = OPS.iter().find(|o| o.command == "slot").unwrap();
        let mut form = Form::operation(op);
        form.values = vec!["/tmp/a file;$(echo nope).img".into(), "auto".into()];
        assert_eq!(
            form.args("/media/USB drive").unwrap(),
            vec![
                "slot",
                "/media/USB drive",
                "/tmp/a file;$(echo nope).img",
                "auto"
            ]
        );
        assert!(form.args("").is_err());
        form.values.clear();
        assert!(form.args("/media/USB").is_err());
    }

    #[test]
    fn read_only_mount_detection_uses_longest_matching_mount() {
        let mounts = "1 0 0:1 / / rw - ext4 root rw\n2 1 8:0 / /media/DISK\\0401 ro,nosuid - vfat /dev/sda ro\n3 2 0:2 / /media/DISK\\0401/nested rw - tmpfs tmp rw\n";
        assert!(linux_mount_read_only(
            Path::new("/media/DISK 1/files"),
            mounts
        ));
        assert!(!linux_mount_read_only(
            Path::new("/media/DISK 1/nested"),
            mounts
        ));
        assert!(!linux_mount_read_only(Path::new("/media/DISK 10"), mounts));
    }

    #[test]
    fn bank_table_filters_sorts_and_retains_only_current_selections() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("banks.bin");
        crate::banks::create_layout(&source, "gotek1440", 3, "minimum").unwrap();
        let mut bytes = fs::read(&source).unwrap();
        bytes[0x180000..0x180000 + 512].fill(0);
        fs::write(&source, bytes).unwrap();
        let catalog = crate::banks::catalog(&source, "gotek1440").unwrap();
        let mut app = app();
        app.selected.insert("bank:999".into());
        app.sender
            .send(Ok(Event::Banks(
                catalog,
                source.to_string_lossy().into_owned(),
                "gotek1440".into(),
            )))
            .unwrap();
        app.receive(&egui::Context::default());
        assert!(app.selected.is_empty());
        assert_eq!(app.visible(), vec![0, 2]);
        app.descending = true;
        assert_eq!(app.visible(), vec![2, 0]);
        app.filter = "002".into();
        assert_eq!(app.visible(), vec![2]);
        app.filter = "001".into();
        assert!(app.visible().is_empty());
        app.show_all = true;
        assert_eq!(app.visible(), vec![1]);
        app.clear();
        assert!(app.bank_identity.is_none());
        assert!(app.rows.is_empty());
    }

    #[test]
    fn image_listing_and_preview_read_real_fat12_data() {
        let temp = tempfile::tempdir().unwrap();
        let image = temp.path().join("disk.img");
        let edited = temp.path().join("edited.img");
        let source = temp.path().join("hello.txt");
        crate::fat12::create(&image, 1440).unwrap();
        fs::write(&source, "GOTEK WRITE OK\r\n").unwrap();
        crate::fat12::import(&image, &source, "HELLO.TXT", &edited).unwrap();
        let entries = crate::fat12::directory_entries(&edited, "/").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "HELLO.TXT");
        assert_eq!(entries[0].size, 16);
        assert!(!entries[0].directory);
        assert!(
            crate::fat12::directory_entries(&image, "/")
                .unwrap()
                .is_empty()
        );
        let listing = read_listing(temp.path(), temp.path()).unwrap();
        let mut app = app();
        app.workspace = Workspace::Images;
        app.sender.send(Ok(Event::Listed(listing))).unwrap();
        app.receive(&egui::Context::default());
        assert_eq!(app.visible().len(), 2);
        app.show_all = true;
        assert_eq!(app.visible().len(), 3);
        assert_ne!(new_image_path(temp.path(), "disk"), image);
        assert!(read_listing(temp.path(), temp.path().parent().unwrap()).is_err());
    }

    #[test]
    fn egui_renders_both_workspaces_dialogs_and_long_tables() {
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = app();
        app.rows = (0..1000)
            .map(|n| DiskRow {
                key: format!("bank:{n:03}"),
                name: format!("FDISK{n:03}"),
                kind: "Matching FAT BPB".into(),
                size: 1_474_560,
                path: "unused-test-device".into(),
                bank: Some(n),
                directory: false,
                recognized: true,
            })
            .collect();
        for width in [900.0, 1280.0] {
            for workspace in [Workspace::Banks, Workspace::Images] {
                app.workspace = workspace;
                for dialog in [None, Some("create-fat12"), Some("bank-write")] {
                    app.form = None;
                    if let Some(command) = dialog {
                        app.open_form(command);
                    }
                    for _ in 0..2 {
                        let output = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 700.0),
                                )),
                                ..Default::default()
                            },
                            |ctx| app.ui(ctx),
                        );
                        assert!(!output.shapes.is_empty());
                    }
                }
            }
        }
        assert!(!app.busy);
    }
    fn text_position(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Pos2> {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, label))
    }
    fn render(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.ui(ctx),
        )
    }
    fn click(
        app: &mut App,
        ctx: &egui::Context,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> egui::FullOutput {
        render(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        render(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        render(app, ctx, vec![])
    }
    #[test]
    fn row_context_menu_targets_bank_and_keeps_write_confirmation() {
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = app();
        app.rows = vec![DiskRow {
            key: "bank:099".into(),
            name: "FDISK099".into(),
            kind: "Matching FAT BPB".into(),
            size: 1_474_560,
            path: "unused-test-device".into(),
            bank: Some(99),
            directory: false,
            recognized: true,
        }];
        render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        let name = text_position(&output.shapes, "FDISK099").unwrap();
        // Click in the status column rather than on the filename widget.
        let output = click(
            &mut app,
            &ctx,
            egui::pos2(1030., name.y),
            egui::PointerButton::Secondary,
        );
        assert_eq!(app.selected, BTreeSet::from(["bank:099".into()]));
        let position = text_position(&output.shapes, "Replace using expected original…")
            .expect("bank context menu must be open");
        click(&mut app, &ctx, position, egui::PointerButton::Primary);
        let form = app
            .form
            .as_ref()
            .expect("context action should open its prefilled form");
        assert_eq!(form.command, "bank-write");
        assert_eq!(form.values[2], "99");
        assert!(form.confirm);
        assert!(!form.review);
        assert!(
            !app.busy,
            "opening a write dialog must never execute the write"
        );
    }
    #[test]
    fn right_click_preserves_group_and_retargets_unselected_rows() {
        let row = |n| DiskRow {
            key: format!("bank:{n:03}"),
            name: format!("FDISK{n:03}"),
            kind: "Matching FAT BPB".into(),
            size: 1_474_560,
            path: "unused-test-device".into(),
            bank: Some(n),
            directory: false,
            recognized: true,
        };
        let mut selected = BTreeSet::from([row(0).key, row(1).key]);
        select_context_row(&mut selected, &row(0));
        assert_eq!(selected.len(), 2);
        select_context_row(&mut selected, &row(2));
        assert_eq!(selected, BTreeSet::from([row(2).key]));
        let ctx = egui::Context::default();
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(disk_menu(ui, &[row(0), row(1)], false, false).is_none());
            });
        });
        assert!(text_position(&output.shapes, "Export selected disks…").is_some());
        assert!(text_position(&output.shapes, "Load image into bank…").is_none());
    }
    #[test]
    fn swap_and_format_prefill_from_selection_and_wait_for_review() {
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = app();
        app.workspace = Workspace::Banks;
        app.source = "unused-test-device".into();
        app.rows = [4u16, 9]
            .map(|n| DiskRow {
                key: format!("bank:{n:03}"),
                name: format!("FDISK{n:03}"),
                kind: "Matching FAT BPB".into(),
                size: 1_474_560,
                path: "unused-test-device".into(),
                bank: Some(n),
                directory: false,
                recognized: true,
            })
            .to_vec();
        app.selected = app.rows.iter().map(|r| r.key.clone()).collect();
        app.swap_form();
        let form = app.form.as_mut().unwrap();
        assert_eq!(form.command, "bank-swap");
        assert_eq!(
            (form.values[2].as_str(), form.values[3].as_str()),
            ("4", "9")
        );
        assert!(
            form.args("")
                .unwrap()
                .contains(&"--confirm-write".to_string())
        );
        let mut same = form.clone();
        same.values[3] = "4".into();
        assert!(same.args("").is_err());
        app.execute_form(&ctx);
        assert!(!app.busy, "a swap must not run before review");
        let summary = app.review_summary(app.form.as_ref().unwrap());
        assert!(summary[1].contains("FDISK004") && summary[2].contains("FDISK009"));
        app.form.as_mut().unwrap().review = true;
        app.form.as_mut().unwrap().summary = summary;
        render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_position(&output.shapes, "Swap banks 004 and 009").is_some());

        let row = app.rows[0].clone();
        app.format_form(&row);
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.command, "bank-format");
        assert_eq!(form.values[3], "FDISK004");
        assert!(form.confirm && !form.review);
        let mut bad = form.clone();
        bad.values[3] = "THIS LABEL IS TOO LONG".into();
        assert!(bad.args("").is_err());

        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("game disk.img");
        crate::fat12::create(&disk, 720).unwrap();
        app.workspace = Workspace::Images;
        app.format_form(&editor_row(&disk, None));
        let form = app.form.as_ref().unwrap();
        assert_eq!(form.command, "image-format");
        assert_eq!(form.values[1], "GAME DISK");
        assert!(app.review_summary(form)[1].contains("720 KiB"));
    }
    #[test]
    fn operating_system_close_waits_for_active_operation() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.busy = true;
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run(input, |ctx| app.ui(ctx));
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
    }
    #[test]
    fn appearance_menu_changes_theme_and_saves_preference() {
        #[derive(Default)]
        struct Storage(std::collections::HashMap<String, String>);
        impl eframe::Storage for Storage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.into(), value);
            }
            fn flush(&mut self) {}
        }
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut app = app();
        render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        let theme = text_position(&output.shapes, "System").unwrap();
        let output = click(&mut app, &ctx, theme, egui::PointerButton::Primary);
        let dark = text_position(&output.shapes, "Dark").expect("appearance menu must open");
        click(&mut app, &ctx, dark, egui::PointerButton::Primary);
        assert_eq!(app.appearance, visual::Appearance::Dark);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        let mut storage = Storage::default();
        eframe::App::save(&mut app, &mut storage);
        assert_eq!(
            storage.0.get("appearance").map(String::as_str),
            Some("Dark")
        );
    }
}
