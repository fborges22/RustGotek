use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

pub fn launch(bank_source: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RustGotek — Disk workspace",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.visuals.selection.bg_fill = Color32::from_rgb(33, 116, 106);
            style.visuals.panel_fill = Color32::from_rgb(20, 27, 35);
            for text_style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                style
                    .text_styles
                    .insert(text_style, egui::FontId::proportional(14.0));
            }
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
            cc.egui_ctx.set_style(style);
            Ok(Box::new(App::new(bank_source, &cc.egui_ctx)))
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
impl Page {
    const ALL: [Self; 7] = [
        Self::Media,
        Self::Banks,
        Self::Slots,
        Self::Images,
        Self::Backups,
        Self::Config,
        Self::Firmware,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Media => "USB media",
            Self::Banks => "Disk banks",
            Self::Slots => "Indexed slots",
            Self::Images => "Disk images",
            Self::Backups => "Backups",
            Self::Config => "Configuration",
            Self::Firmware => "Firmware",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Media => "Browse the files on your mounted Gotek USB drive.",
            Self::Banks => "Inspect, export and replace original Gotek floppy banks.",
            Self::Slots => "Manage FlashFloppy indexed image files. Not raw disk banks.",
            Self::Images => "Create, inspect, edit and convert local floppy images.",
            Self::Backups => "Preserve your files and verify every copied byte.",
            Self::Config => "Inspect and edit FlashFloppy settings with automatic backups.",
            Self::Firmware => "Validate update files and prepare USB media for a specific MCU.",
        }
    }
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
        Images,
        "Create a PC floppy",
        "create-fat12",
        "Create a formatted FAT12 data image. No operating system is included.",
        false,
        false,
        false,
        [
            field!("New image", Save, ""),
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
            field!("New image", Save, "")
        ]
    ),
    op!(
        Images,
        "Create an image directory",
        "image-mkdir",
        "Create a directory in a new copy of the FAT12 image.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("DOS 8.3 directory path", Text, ""),
            field!("New image", Save, "")
        ]
    ),
    op!(
        Images,
        "Remove an image entry",
        "image-remove",
        "Remove a file or empty directory in a new copy. Original is preserved.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("Path inside image", Text, ""),
            field!("New image", Save, "")
        ]
    ),
    op!(
        Images,
        "Rename an image entry",
        "image-rename",
        "Rename a file or directory within its parent in a new copy.",
        false,
        false,
        false,
        [
            field!("Original image", File, ""),
            field!("Path inside image", Text, ""),
            field!("New DOS 8.3 name", Text, ""),
            field!("New image", Save, "")
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
struct Preview {
    image: PathBuf,
    directory: String,
    entries: Vec<crate::fat12::DiskEntry>,
    temp: Option<Arc<tempfile::TempDir>>,
    selected: Option<String>,
    title: String,
}
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
        }
    }
    fn args(&self, root: &str) -> Result<Vec<String>, String> {
        if self.media && root.is_empty() {
            return Err("Open a media folder first".into());
        }
        if self.values.len() != self.labels.len() || self.values.iter().any(|s| s.trim().is_empty())
        {
            return Err("Complete all fields before continuing".into());
        }
        let mut args = vec![self.command.clone()];
        if self.media {
            args.push(root.into());
        }
        args.extend(self.values.clone());
        if matches!(self.command.as_str(), "bank-write" | "device-restore") {
            args.push("--confirm-write".into());
        }
        Ok(args)
    }
}
enum Event {
    Listed(Listing),
    Banks(crate::banks::Catalog, String, String),
    Preview(Preview),
    Done(String),
    Edited(Preview, String),
    Picked(usize, Option<PathBuf>),
    Folder(Option<PathBuf>),
    Source(Option<PathBuf>),
}
struct App {
    workspace: Workspace,
    root: String,
    source: String,
    profile: String,
    listing: Option<Listing>,
    bank_catalog: Option<crate::banks::Catalog>,
    bank_identity: Option<(String, String)>,
    rows: Vec<DiskRow>,
    selected: BTreeSet<String>,
    filter: String,
    show_all: bool,
    sort: Sort,
    descending: bool,
    preview: Option<Preview>,
    form: Option<Form>,
    tools: bool,
    tool_page: Page,
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
            workspace: if bank_source.is_some() {
                Workspace::Banks
            } else {
                Workspace::Images
            },
            root,
            source: bank_source.clone().unwrap_or_else(|| {
                if cfg!(target_os = "linux") {
                    "/dev/sda".into()
                } else {
                    String::new()
                }
            }),
            profile: "gotek1440".into(),
            listing: None,
            bank_catalog: None,
            bank_identity: None,
            rows: vec![],
            selected: BTreeSet::new(),
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
            tools: false,
            tool_page: Page::Images,
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
                Ok(Event::Preview(preview)) => {
                    self.status = format!("{} entries in {}", preview.entries.len(), preview.title);
                    self.preview = Some(preview);
                }
                Ok(Event::Edited(preview, output)) => {
                    self.preview = Some(preview);
                    self.log = output;
                    self.status = "Saved and opened the new image".into();
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
                            form.values[2].push_str(&name.to_string_lossy().to_ascii_uppercase());
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
            let mut form = Form::operation(op);
            let selected = self.selection();
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
            self.tools = false;
        }
    }
    fn open_disk(&mut self, ctx: &egui::Context, row: DiskRow) {
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
            let (image, temp) = if let Some(bank) = row.bank {
                let temp = Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?);
                let image = temp.path().join("disk.img");
                backend(vec!["bank-export".into(), row.path.to_string_lossy().into_owned(), profile,
                    bank.to_string(), image.to_string_lossy().into_owned()])?;
                (image, Some(temp))
            } else { (row.path, None) };
            let entries = crate::fat12::directory_entries(&image, "/").map_err(|e| {
                format!("Cannot browse this disk as FAT12: {e}. Use Inspect / Convert under More tools for HFE or other formats.")
            })?;
            Ok(Event::Preview(Preview { image, directory: "/".into(), entries, temp, selected: None, title: row.name }))
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
    fn import_images(&mut self, ctx: &egui::Context) {
        let Some(listing) = &self.listing else {
            return;
        };
        let dest = listing.directory.clone();
        self.worker(ctx, "Choose images to add…", move || {
            let Some(files) = rfd::FileDialog::new()
                .add_filter(
                    "Disk images",
                    &["img", "ima", "hfe", "adf", "dsk", "st", "ssd", "dsd", "trd"],
                )
                .pick_files()
            else {
                return Ok(Event::Done("Import cancelled".into()));
            };
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
    fn replace_bank_form(&mut self, row: &DiskRow) {
        self.form = Some(Form {
            title: format!("Replace bank {:03}", row.bank.unwrap()),
            description: concat!(
                "Save the current bank to a recovery image, write the replacement and verify it. ",
                "The device must be unmounted. Other banks are preserved."
            )
            .into(),
            command: "gui-replace-bank".into(),
            labels: [
                "Target",
                "Profile",
                "Bank",
                "Replacement floppy image",
                "New recovery image",
            ]
            .map(str::to_owned)
            .to_vec(),
            kinds: vec![Kind::Text, Kind::Text, Kind::Text, Kind::File, Kind::Save],
            values: vec![
                self.source.clone(),
                self.profile.clone(),
                row.bank.unwrap().to_string(),
                String::new(),
                String::new(),
            ],
            media: false,
            writes_media: false,
            confirm: true,
            review: false,
            keepalive: None,
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
        let edited = match form.command.as_str() {
            "image-import" | "image-mkdir" | "image-remove" | "image-rename" => {
                form.values.last().map(PathBuf::from)
            }
            "create-fat12" => Some(PathBuf::from(&form.values[0])),
            _ => None,
        };
        let keepalive = form.keepalive;
        self.worker(ctx, &format!("{}…", form.title), move || {
            let _keepalive = keepalive;
            let output = if args[0] == "gui-replace-bank" {
                let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
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
                    args[4].clone(),
                    expected,
                    args[5].clone(),
                    "--confirm-write".into(),
                ])?
            } else {
                backend(args)?
            };
            if let Some(image) = edited {
                let entries = crate::fat12::directory_entries(&image, "/")
                    .map_err(|e| format!("Image saved, but could not open it: {e}"))?;
                let title = image
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                return Ok(Event::Edited(
                    Preview {
                        image,
                        directory: "/".into(),
                        entries,
                        temp: None,
                        selected: None,
                        title,
                    },
                    output,
                ));
            }
            Ok(Event::Done(output))
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}
impl App {
    fn ui(&mut self, ctx: &egui::Context) {
        self.receive(ctx);
        egui::TopBottomPanel::top("heading").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.heading("RustGotek");
                ui.label(
                    RichText::new("DISK WORKSPACE")
                        .small()
                        .color(Color32::from_rgb(104, 199, 182)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    ui.label(if self.busy { "Working" } else { "Ready" });
                });
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    if self.error {
                        Color32::LIGHT_RED
                    } else {
                        Color32::from_rgb(119, 213, 189)
                    },
                    &self.status,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.show_log, "Activity log");
                    if ui.button("Copy log").clicked() {
                        ctx.copy_text(self.log.clone());
                    }
                });
            });
            if self.show_log {
                egui::ScrollArea::vertical()
                    .max_height(115.0)
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
            .exact_width(190.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                ui.label(RichText::new("YOUR DISKS").small().weak());
                ui.add_space(8.0);
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    for (mode, label) in [
                        (Workspace::Images, "Image library"),
                        (Workspace::Banks, "Gotek USB banks"),
                    ] {
                        if ui
                            .selectable_label(
                                self.workspace == mode,
                                RichText::new(label).size(17.0),
                            )
                            .clicked()
                            && self.workspace != mode
                        {
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
                    }
                    ui.add_space(24.0);
                    ui.label(RichText::new("UTILITIES").small().weak());
                    if ui.button("Backups & restore").clicked() {
                        self.tool_page = if self.workspace == Workspace::Banks {
                            Page::Banks
                        } else {
                            Page::Backups
                        };
                        self.tools = true;
                    }
                    if ui.button("All tools").clicked() {
                        self.tools = true;
                    }
                    ui.separator();
                    let help = if self.workspace == Workspace::Banks {
                        "Bank numbers start at 000. Scanning reads headers only; use Check disk for allocation checks."
                    } else {
                        "Open a folder of disk images or mounted USB files. Double-click a disk to browse its files."
                    };
                    ui.label(RichText::new(help).small().weak());
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                self.workspace_ui(ui, ctx)
            });
        });
        self.tools_window(ctx);
        self.preview_window(ctx);
        self.form_window(ctx);
    }
}

impl App {
    fn workspace_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(8.0);
        ui.heading(if self.workspace == Workspace::Images {
            "Image library"
        } else {
            "Gotek USB banks"
        });
        ui.label(if self.workspace == Workspace::Images {
            "Your disk images, ready to browse, edit and export."
        } else {
            "Select disks below to browse, export or replace them."
        });
        ui.add_space(12.0);
        self.source_ui(ui, ctx);
        ui.add_space(12.0);
        self.actions_ui(ui, ctx);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Search names, bank numbers or formats…")
                    .desired_width(280.0),
            );
            ui.checkbox(
                &mut self.show_all,
                if self.workspace == Workspace::Banks {
                    "Include unrecognized banks"
                } else {
                    "Show all files"
                },
            );
            ui.label(format!("{} selected", self.selected.len()));
            if ui.button("Clear selection").clicked() {
                self.selected.clear();
            }
        });
        self.table_ui(ui, ctx);
    }
    fn source_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            if self.workspace == Workspace::Images {
                ui.label("Folder");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.root)
                            .desired_width((ui.available_width() * 0.55).clamp(160.0, 600.0)),
                    )
                    .changed()
                {
                    self.clear();
                }
                if ui.button("Choose folder…").clicked() {
                    self.worker(ctx, "Choose a folder…", || {
                        Ok(Event::Folder(rfd::FileDialog::new().pick_folder()))
                    });
                }
                if ui.button("Open / Refresh").clicked() {
                    self.refresh(ctx);
                }
            } else {
                ui.label("Source");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.source)
                            .hint_text("/dev/sda or a whole-device image")
                            .desired_width((ui.available_width() - 420.0).max(160.0)),
                    )
                    .changed()
                {
                    self.clear();
                }
                if ui.button("Browse…").clicked() {
                    self.worker(ctx, "Choose a whole-device image…", || {
                        Ok(Event::Source(rfd::FileDialog::new().pick_file()))
                    });
                }
                let old = self.profile.clone();
                egui::ComboBox::from_id_salt("bank-profile")
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
                if ui
                    .add_enabled(
                        !self.source.trim().is_empty(),
                        egui::Button::new("Scan banks"),
                    )
                    .clicked()
                {
                    self.scan(ctx);
                }
            }
        });
        if let Some(listing) = &self.listing {
            let root = listing.root.clone();
            let dir = listing.directory.clone();
            let readonly = listing.read_only;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(root != dir, egui::Button::new("< Up"))
                    .clicked()
                {
                    let parent = dir.parent().unwrap_or(&root).to_owned();
                    self.worker(ctx, "Reading folder…", move || {
                        read_listing(&root, &parent).map(Event::Listed)
                    });
                }
                ui.label(dir.display().to_string());
                if readonly {
                    ui.colored_label(Color32::LIGHT_YELLOW, "Read-only media");
                }
            });
        }
    }
    fn actions_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let selected = self.selection();
        let single = selected.len() == 1 && !selected[0].directory;
        let can_export = !selected.is_empty() && selected.iter().all(|r| !r.directory);
        let readonly = self.listing.as_ref().is_some_and(|l| l.read_only);
        ui.horizontal_wrapped(|ui| {
            if ui.button(RichText::new("+ Create disk").strong()).clicked() {
                self.open_form("create-fat12");
                if let (Some(l), Some(f)) = (&self.listing, &mut self.form)
                    && !readonly
                {
                    f.values[0] = new_image_path(&l.directory, "New disk")
                        .to_string_lossy()
                        .into_owned();
                }
            }
            if self.workspace == Workspace::Images
                && ui
                    .add_enabled(
                        self.listing.is_some() && !readonly,
                        egui::Button::new("Import images…"),
                    )
                    .clicked()
            {
                self.import_images(ctx);
            }
            if ui
                .add_enabled(single, egui::Button::new("Open contents"))
                .clicked()
            {
                self.open_disk(ctx, selected[0].clone());
            }
            let export_label = if selected.len() > 1 {
                format!("Export {} disks…", selected.len())
            } else {
                "Export…".into()
            };
            if ui
                .add_enabled(can_export, egui::Button::new(export_label))
                .clicked()
            {
                self.export_selected(ctx);
            }
            if self.workspace == Workspace::Banks {
                if ui
                    .add_enabled(single, egui::Button::new("Replace disk…"))
                    .clicked()
                {
                    self.replace_bank_form(&selected[0]);
                }
                if ui.button("New bank layout…").clicked() {
                    self.open_form("bank-create");
                    if let (Some(c), Some(f)) = (&self.bank_catalog, &mut self.form) {
                        f.values[3] = c.size.to_string();
                    }
                }
            } else if ui
                .add_enabled(single && !readonly, egui::Button::new("Move to trash…"))
                .clicked()
            {
                let r = &selected[0];
                self.form = Some(Form {
                    title: format!("Move {} to trash", r.name),
                    description:
                        "The file is moved into the media's recoverable .rustgotek-trash folder."
                            .into(),
                    command: "remove".into(),
                    labels: vec!["Selected file".into()],
                    kinds: vec![Kind::Text],
                    values: vec![r.key.clone()],
                    media: true,
                    writes_media: true,
                    confirm: true,
                    review: false,
                    keepalive: None,
                });
            }
            if ui.button("More tools…").clicked() {
                self.tool_page = if self.workspace == Workspace::Banks {
                    Page::Banks
                } else {
                    Page::Images
                };
                self.tools = true;
            }
        });
    }
    fn sort_header(&mut self, ui: &mut egui::Ui, label: &str, sort: Sort) {
        let label = if self.sort == sort {
            format!("{label} {}", if self.descending { "v" } else { "^" })
        } else {
            label.into()
        };
        if ui.selectable_label(self.sort == sort, label).clicked() {
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
        if self.rows.is_empty() {
            ui.add_space(55.0);
            ui.vertical_centered(|ui| {
                ui.heading(if self.workspace == Workspace::Images {
                    "Bring your disks into one place"
                } else {
                    "Your Gotek's disks will appear here"
                });
                ui.label(if self.workspace == Workspace::Images {
                    "Choose an image folder above, or create your first floppy image."
                } else {
                    "Enter the USB device path or select a whole-device backup, then Scan banks."
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
        let mut export = None;
        let mut replace = None;
        let banks = self.workspace == Workspace::Banks;
        let height = (ui.available_height() - 45.0).max(100.0);
        let mut table = TableBuilder::new(ui)
            .id_salt(("disk-table", banks))
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
            .header(32.0, |mut header| {
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
                body.rows(34.0, visible.len(), |mut tr| {
                    let row = &self.rows[visible[tr.index()]];
                    let mut chosen = self.selected.contains(&row.key);
                    tr.set_selected(chosen);
                    tr.col(|ui| {
                        if ui.checkbox(&mut chosen, "").changed() {
                            if chosen {
                                self.selected.insert(row.key.clone());
                            } else {
                                self.selected.remove(&row.key);
                            }
                        }
                    });
                    if banks {
                        tr.col(|ui| {
                            ui.monospace(row.bank.map(|n| format!("{n:03}")).unwrap_or_default());
                        });
                    }
                    tr.col(|ui| {
                        let response = ui.selectable_label(
                            chosen,
                            if row.directory {
                                format!("▸ {}", row.name)
                            } else {
                                row.name.clone()
                            },
                        );
                        if response.clicked() {
                            if !ui.input(|i| i.modifiers.command) {
                                self.selected.clear();
                            }
                            self.selected.insert(row.key.clone());
                        }
                        if response.double_clicked() {
                            open = Some(row.clone());
                        }
                        response.context_menu(|ui| {
                            if ui.button("Open contents").clicked() {
                                open = Some(row.clone());
                                ui.close();
                            }
                            if !row.directory && ui.button("Export…").clicked() {
                                export = Some(row.clone());
                                ui.close();
                            }
                            if row.bank.is_some() && ui.button("Replace disk…").clicked() {
                                replace = Some(row.clone());
                                ui.close();
                            }
                        });
                    });
                    tr.col(|ui| {
                        ui.label(RichText::new(&row.kind).color(if row.recognized {
                            Color32::from_rgb(137, 205, 187)
                        } else {
                            Color32::GRAY
                        }));
                    });
                    tr.col(|ui| {
                        ui.label(if row.directory {
                            String::new()
                        } else {
                            format_size(row.size)
                        });
                    });
                })
            });
        if let Some(row) = open {
            self.open_disk(ctx, row);
        }
        if let Some(row) = export {
            self.selected.clear();
            self.selected.insert(row.key);
            self.export_selected(ctx);
        }
        if let Some(row) = replace {
            self.replace_bank_form(&row);
        }
    }
    fn tools_window(&mut self, ctx: &egui::Context) {
        if !self.tools {
            return;
        }
        let mut open = true;
        let mut command = None;
        egui::Window::new("More tools")
            .open(&mut open)
            .default_width(680.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for page in Page::ALL {
                            ui.selectable_value(&mut self.tool_page, page, page.title());
                        }
                    });
                    ui.label(self.tool_page.description());
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(440.0)
                        .show(ui, |ui| {
                            for op in OPS.iter().filter(|o| o.page == self.tool_page) {
                                ui.horizontal(|ui| {
                                    if ui.button(op.title).clicked() {
                                        command = Some(op.command);
                                    }
                                    ui.label(RichText::new(op.description).small().weak());
                                });
                                ui.separator();
                            }
                        });
                });
            });
        self.tools = open;
        if let Some(command) = command {
            self.open_form(command);
        }
    }
    fn form_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.form.take() else {
            return;
        };
        let mut open = true;
        let mut execute = false;
        let mut browse = None;
        egui::Window::new(&form.title)
            .id(egui::Id::new("operation-form"))
            .open(&mut open)
            .collapsible(false)
            .default_width(650.0)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.label(&form.description);
                    if form.media {
                        ui.label(format!("Media: {}", self.root));
                    }
                    ui.separator();
                    if form.review {
                        ui.colored_label(Color32::LIGHT_YELLOW, "Review before changing data");
                        for (label, value) in form.labels.iter().zip(&form.values) {
                            ui.label(RichText::new(label).strong());
                            ui.label(value);
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Back").clicked() {
                                form.review = false;
                            }
                            if ui
                                .button(RichText::new("Confirm and run").strong())
                                .clicked()
                            {
                                execute = true;
                            }
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(440.0)
                            .show(ui, |ui| {
                                for i in 0..form.values.len() {
                                    ui.label(&form.labels[i]);
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
                                            && ui.button("Browse…").clicked()
                                        {
                                            browse = Some((i, form.kinds[i]));
                                        }
                                    });
                                }
                            });
                        let readonly =
                            form.writes_media && self.listing.as_ref().is_some_and(|l| l.read_only);
                        if readonly {
                            ui.colored_label(Color32::LIGHT_YELLOW, "This media is read-only.");
                        }
                        let valid = form.args(&self.root).is_ok() && !readonly;
                        if ui
                            .add_enabled(
                                valid,
                                egui::Button::new(if form.confirm {
                                    "Review changes…"
                                } else if form.command == "create-fat12" {
                                    "Create disk"
                                } else if matches!(
                                    form.command.as_str(),
                                    "image-import"
                                        | "image-mkdir"
                                        | "image-remove"
                                        | "image-rename"
                                ) {
                                    "Save new image"
                                } else {
                                    "Run operation"
                                }),
                            )
                            .clicked()
                        {
                            if form.confirm {
                                form.review = true;
                            } else {
                                execute = true;
                            }
                        }
                    }
                });
            });
        if open {
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
        egui::Window::new(format!("Disk contents — {}", preview.title))
            .id(egui::Id::new("preview")).open(&mut open).default_size([720.0, 510.0])
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.busy && self.form.is_none(), |ui| {
                    if preview.temp.is_none() { ui.label(preview.image.display().to_string()); }
                    if preview.temp.is_some() {
                        ui.label(RichText::new("Bank snapshot: save edits to an image, then use Replace disk to write it back.").small().weak());
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(preview.directory != "/", egui::Button::new("< Up")).clicked() {
                            directory = Some(parent_directory(&preview.directory));
                        }
                        ui.strong(&preview.directory);
                        if ui.button("Check disk").clicked() { action = Some("image-check"); }
                        if ui.button("Add file…").clicked() { action = Some("image-import"); }
                        if ui.button("New folder…").clicked() { action = Some("image-mkdir"); }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let selected = preview.selected.as_ref().and_then(|s| preview.entries.iter().find(|e| &e.name == s));
                        if ui.add_enabled(selected.is_some_and(|e| !e.directory), egui::Button::new("Export file…")).clicked() { extract = true; }
                        if ui.add_enabled(selected.is_some(), egui::Button::new("Rename…")).clicked() { action = Some("image-rename"); }
                        if ui.add_enabled(selected.is_some(), egui::Button::new("Remove from copy…")).clicked() { action = Some("image-remove"); }
                    });
                    ui.separator();
                    TableBuilder::new(ui).id_salt("contents-table").striped(true)
                        .column(Column::remainder()).column(Column::initial(80.0)).column(Column::initial(100.0))
                        .max_scroll_height(280.0).header(28.0, |mut h| {
                            for name in ["Name", "Type", "Size"] { h.col(|ui| { ui.strong(name); }); }
                        }).body(|body| body.rows(30.0, preview.entries.len(), |mut tr| {
                            let e = &preview.entries[tr.index()];
                            tr.col(|ui| {
                                let response = ui.selectable_label(preview.selected.as_ref() == Some(&e.name), &e.name);
                                if response.clicked() { preview.selected = Some(e.name.clone()); }
                                if response.double_clicked() && e.directory { directory = Some(inner_path(&preview.directory, &e.name)); }
                            });
                            tr.col(|ui| { ui.label(if e.directory { "Folder" } else { "File" }); });
                            tr.col(|ui| { ui.label(format_size(e.size)); });
                        }));
                    if preview.entries.is_empty() { ui.label("This directory is empty. Add a file to a new copy to get started."); }
                });
            });
        if let Some(dir) = directory {
            let image = preview.image.clone();
            let temp = preview.temp.clone();
            let title = preview.title.clone();
            self.worker(ctx, "Reading disk directory…", move || {
                let entries =
                    crate::fat12::directory_entries(&image, &dir).map_err(|e| e.to_string())?;
                Ok(Event::Preview(Preview {
                    image,
                    directory: dir,
                    entries,
                    temp,
                    selected: None,
                    title,
                }))
            });
        }
        if let Some(command) = action {
            self.open_form(command);
            if let Some(form) = &mut self.form {
                form.values[0] = preview.image.to_string_lossy().into_owned();
                form.keepalive = preview.temp.clone();
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
                if matches!(
                    command,
                    "image-import" | "image-mkdir" | "image-remove" | "image-rename"
                ) && preview.temp.is_none()
                    && let Some(parent) = preview.image.parent()
                    && !mount_read_only(parent)
                {
                    let stem = preview
                        .image
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy();
                    *form.values.last_mut().unwrap() =
                        new_image_path(parent, &format!("{stem} edited"))
                            .to_string_lossy()
                            .into_owned();
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

fn new_image_path(directory: &Path, stem: &str) -> PathBuf {
    let mut candidate = directory.join(format!("{stem}.img"));
    let mut n = 2;
    while candidate.exists() {
        candidate = directory.join(format!("{stem} {n}.img"));
        n += 1;
    }
    candidate
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
}
