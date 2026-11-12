use iced::widget::{
    button, column, container, pick_list, row, rule, scrollable, space, text, text_input,
};
use iced::{Color, Element, Fill, Task, Theme};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn launch(bank_source: Option<String>) -> iced::Result {
    iced::application(
        move || {
            let (mut app, task) = App::new();
            if let Some(source) = &bank_source {
                app.page = Page::Banks;
                app.values = OPS[0].fields.iter().map(|f| f.default.into()).collect();
                app.values[0] = source.clone();
                app.busy = false;
                app.status = "Choose a disk bank operation".into();
                return (app, Task::none());
            }
            (app, task)
        },
        App::update,
        App::view,
    )
    .title("RustGotek · Floppy manager")
    .theme(|_: &App| {
        Theme::custom(
            "RustGotek",
            iced::theme::Palette {
                background: Color::from_rgb8(17, 24, 32),
                text: Color::from_rgb8(224, 233, 237),
                primary: Color::from_rgb8(74, 196, 175),
                success: Color::from_rgb8(74, 196, 175),
                warning: Color::from_rgb8(229, 180, 83),
                danger: Color::from_rgb8(235, 111, 111),
            },
        )
    })
    .window_size((1180, 820))
    .centered()
    .run()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choice(usize);
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(OPS[self.0].title)
    }
}

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
#[derive(Debug, Clone)]
struct JobResult {
    success: bool,
    output: String,
}
#[derive(Debug, Clone)]
enum Message {
    Page(Page),
    Root(String),
    PickRoot,
    RootPicked(Option<PathBuf>),
    Refresh,
    Listed(Result<Listing, String>),
    Open(String),
    Up,
    Select(String),
    Filter(String),
    Choose(Choice),
    Field(usize, String),
    Browse(usize),
    Picked(usize, Option<PathBuf>),
    Run,
    Report,
    Import,
    ImportPicked(Option<PathBuf>),
    Export,
    ExportPicked(Option<PathBuf>),
    Remove,
    Confirm,
    Cancel,
    Completed(JobResult),
    CopyResult,
}

struct App {
    page: Page,
    root: String,
    listing: Option<Listing>,
    selected: Option<String>,
    filter: String,
    choice: Choice,
    values: Vec<String>,
    busy: bool,
    status: String,
    output: String,
    pending: Option<(String, Vec<String>)>,
    failed: bool,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let root = discover_media()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut app = Self { page: Page::Media, root, listing: None, selected: None, filter: String::new(),
            choice: Choice(0), values: vec![], busy: false, status: "Choose a mounted USB directory to begin".into(),
            output: "Choose USB media for mounted files, Disk banks for raw devices and whole-device backups, or Disk images for local floppy images.\nBank reads leave the device unchanged. Writing a replacement requires confirmation and saves the original bank for recovery.".into(),
            pending: None, failed: false };
        let task = if !app.root.is_empty() {
            app.load(None)
        } else {
            Task::none()
        };
        (app, task)
    }
    fn read_only(&self) -> bool {
        self.listing.as_ref().is_some_and(|l| l.read_only)
    }
    fn load(&mut self, directory: Option<PathBuf>) -> Task<Message> {
        self.pending = None;
        self.selected = None;
        self.busy = true;
        self.status = "Reading media…".into();
        let root = PathBuf::from(&self.root);
        let directory = directory.unwrap_or_else(|| root.clone());
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || read_listing(&root, &directory))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            },
            Message::Listed,
        )
    }
    fn execute(&mut self, title: String, args: Vec<String>) -> Task<Message> {
        self.pending = None;
        self.busy = true;
        self.failed = false;
        self.status = format!("{title}…");
        self.output.clear();
        Task::perform(run_backend(args), Message::Completed)
    }
    fn prepare(&mut self, title: &str, args: Vec<String>, confirm: bool) -> Task<Message> {
        if confirm {
            self.pending = Some((title.into(), args));
            Task::none()
        } else {
            self.execute(title.into(), args)
        }
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        if self.busy
            && !matches!(
                message,
                Message::Listed(_)
                    | Message::Completed(_)
                    | Message::RootPicked(_)
                    | Message::Picked(_, _)
                    | Message::ImportPicked(_)
                    | Message::ExportPicked(_)
            )
        {
            return Task::none();
        }
        match message {
            Message::Page(page) => {
                self.page = page;
                self.pending = None;
                if let Some(index) = OPS.iter().position(|op| op.page == page) {
                    self.choice = Choice(index);
                    self.values = OPS[index].fields.iter().map(|f| f.default.into()).collect();
                }
            }
            Message::Root(root) => {
                self.root = root;
                self.listing = None;
                self.selected = None;
                self.pending = None;
            }
            Message::PickRoot => {
                self.busy = true;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose mounted Gotek USB directory")
                            .pick_folder()
                            .await
                            .map(|h| h.path().to_owned())
                    },
                    Message::RootPicked,
                );
            }
            Message::RootPicked(path) => {
                self.busy = false;
                if let Some(path) = path {
                    self.root = path.to_string_lossy().into_owned();
                    return self.load(None);
                }
            }
            Message::Refresh => return self.load(None),
            Message::Listed(result) => {
                self.busy = false;
                match result {
                    Ok(listing) => {
                        self.root = listing.root.to_string_lossy().into_owned();
                        self.status = format!(
                            "{} entries · {}",
                            listing.entries.len(),
                            if listing.read_only {
                                "read-only media"
                            } else {
                                "media opened"
                            }
                        );
                        self.listing = Some(listing);
                        self.selected = None;
                        self.failed = false;
                    }
                    Err(error) => {
                        self.listing = None;
                        self.failed = true;
                        self.status = "Could not open media".into();
                        self.output = error;
                    }
                }
            }
            Message::Open(relative) => {
                if let Some(listing) = &self.listing {
                    return self.load(Some(listing.root.join(relative)));
                }
            }
            Message::Up => {
                if let Some(l) = &self.listing
                    && l.directory != l.root
                {
                    return self.load(l.directory.parent().map(Path::to_path_buf));
                }
            }
            Message::Select(path) => self.selected = Some(path),
            Message::Filter(value) => self.filter = value,
            Message::Choose(choice) => {
                self.choice = choice;
                self.values = OPS[choice.0]
                    .fields
                    .iter()
                    .map(|f| f.default.into())
                    .collect();
                self.pending = None;
            }
            Message::Field(index, value) => {
                self.values[index] = value;
                self.pending = None;
            }
            Message::Browse(index) => {
                self.busy = true;
                let field = &OPS[self.choice.0].fields[index];
                let kind = field.kind;
                let title = field.label;
                return Task::perform(
                    async move {
                        let dialog = rfd::AsyncFileDialog::new().set_title(title);
                        let result = match kind {
                            Kind::Folder => dialog.pick_folder().await,
                            Kind::Save => dialog.save_file().await,
                            _ => dialog.pick_file().await,
                        };
                        (index, result.map(|h| h.path().to_owned()))
                    },
                    |(i, p)| Message::Picked(i, p),
                );
            }
            Message::Picked(index, path) => {
                self.busy = false;
                if let Some(path) = path {
                    self.values[index] = path.to_string_lossy().into_owned();
                }
            }
            Message::Run => {
                let op = &OPS[self.choice.0];
                if op.writes_media && self.read_only() {
                    return Task::none();
                }
                match operation_args(self.choice, &self.root, &self.values) {
                    Ok(args) => return self.prepare(op.title, args, op.confirm),
                    Err(error) => {
                        self.failed = true;
                        self.status = "Check the form".into();
                        self.output = error;
                    }
                }
            }
            Message::Report => {
                return self.execute(
                    "Inspect media".into(),
                    vec!["media-info".into(), self.root.clone()],
                );
            }
            Message::Import => {
                if self.read_only() {
                    return Task::none();
                }
                self.busy = true;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose disk image to import")
                            .add_filter(
                                "Disk images",
                                &[
                                    "img", "ima", "hfe", "adf", "dsk", "st", "ssd", "dsd", "trd",
                                    "v9t9",
                                ],
                            )
                            .pick_file()
                            .await
                            .map(|h| h.path().to_owned())
                    },
                    Message::ImportPicked,
                );
            }
            Message::ImportPicked(path) => {
                self.busy = false;
                if let (Some(path), Some(listing)) = (path, &self.listing) {
                    let name = path.file_name().unwrap_or_default();
                    let relative = listing
                        .directory
                        .strip_prefix(&listing.root)
                        .unwrap_or(Path::new(""))
                        .join(name);
                    return self.execute(
                        "Import image".into(),
                        vec![
                            "import".into(),
                            self.root.clone(),
                            path.to_string_lossy().into_owned(),
                            relative.to_string_lossy().into_owned(),
                        ],
                    );
                }
            }
            Message::Export => {
                if let Some(selected) = &self.selected {
                    self.busy = true;
                    let name = Path::new(selected)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    return Task::perform(
                        async move {
                            rfd::AsyncFileDialog::new()
                                .set_title("Export file to a new path")
                                .set_file_name(name)
                                .save_file()
                                .await
                                .map(|h| h.path().to_owned())
                        },
                        Message::ExportPicked,
                    );
                }
            }
            Message::ExportPicked(path) => {
                self.busy = false;
                if let (Some(path), Some(selected)) = (path, &self.selected) {
                    return self.execute(
                        "Export file".into(),
                        vec![
                            "export".into(),
                            self.root.clone(),
                            selected.clone(),
                            path.to_string_lossy().into_owned(),
                        ],
                    );
                }
            }
            Message::Remove => {
                if !self.read_only()
                    && let Some(selected) = &self.selected
                {
                    return self.prepare(
                        "Move selected file to recoverable trash",
                        vec!["remove".into(), self.root.clone(), selected.clone()],
                        true,
                    );
                }
            }
            Message::Confirm => {
                if let Some((title, args)) = self.pending.take() {
                    return self.execute(title, args);
                }
            }
            Message::Cancel => self.pending = None,
            Message::Completed(result) => {
                self.busy = false;
                self.failed = !result.success;
                self.status = if result.success {
                    "Operation completed"
                } else {
                    "Operation failed"
                }
                .into();
                self.output = result.output;
                // Refresh is explicit so the operation result remains visible and failures never look successful.
            }
            Message::CopyResult => return iced::clipboard::write(self.output.clone()),
        }
        Task::none()
    }
    fn action<'a>(
        &self,
        label: &'a str,
        message: Message,
        enabled: bool,
    ) -> iced::widget::Button<'a, Message> {
        button(text(label).size(14))
            .padding([9, 14])
            .on_press_maybe((enabled && !self.busy).then_some(message))
    }
    fn view(&self) -> Element<'_, Message> {
        let mut sidebar = column![
            text("RUSTGOTEK").size(23),
            text("Floppy workspace").size(13),
            space().height(26)
        ]
        .spacing(8);
        for page in Page::ALL {
            sidebar = sidebar.push(
                self.action(page.title(), Message::Page(page), true)
                    .width(Fill)
                    .style(if self.page == page {
                        button::primary
                    } else {
                        button::text
                    }),
            );
        }
        sidebar = sidebar
            .push(space().height(Fill))
            .push(text("Local files · No cloud").size(12))
            .push(text("Iced desktop edition").size(12));
        let sidebar = container(sidebar)
            .padding(22)
            .width(205)
            .height(Fill)
            .style(container::rounded_box);
        let header = row![
            column![
                text(self.page.title()).size(30),
                text(self.page.description()).size(14)
            ]
            .spacing(5),
            space().width(Fill),
            container(
                text(if self.busy {
                    "WORKING"
                } else if self.page != Page::Banks && self.read_only() {
                    "READ ONLY"
                } else {
                    "READY"
                })
                .size(12)
            )
            .padding([8, 12])
            .style(container::rounded_box)
        ]
        .spacing(12);
        let media_bar = row![
            text_input("Mounted USB directory (not /dev/sda)", &self.root)
                .on_input(Message::Root)
                .padding(10),
            self.action("Choose folder…", Message::PickRoot, true),
            self.action("Open / Refresh", Message::Refresh, !self.root.is_empty())
        ]
        .spacing(8);
        let content = if self.page == Page::Media {
            self.media_view()
        } else {
            self.form_view()
        };
        let mut main = column![header].spacing(18);
        if self.page != Page::Banks {
            main = main.push(media_bar);
        }
        main = main.push(content);
        if let Some((title, args)) = &self.pending {
            main = main.push(
                container(
                    column![
                        text(title).size(16),
                        text(
                            args.iter()
                                .skip(1)
                                .map(|s| format!("{s:?}"))
                                .collect::<Vec<_>>()
                                .join("  →  ")
                        )
                        .size(12),
                        row![
                            self.action("Confirm", Message::Confirm, true),
                            self.action("Cancel", Message::Cancel, true)
                                .style(button::secondary)
                        ]
                        .spacing(8)
                    ]
                    .spacing(9),
                )
                .padding(14)
                .style(container::rounded_box),
            );
        }
        main = main
            .push(rule::horizontal(1))
            .push(
                row![
                    text(&self.status).size(14).color(if self.failed {
                        Color::from_rgb8(235, 111, 111)
                    } else {
                        Color::from_rgb8(112, 213, 190)
                    }),
                    space().width(Fill),
                    self.action("Copy result", Message::CopyResult, !self.output.is_empty())
                        .style(button::text)
                ]
                .spacing(8),
            )
            .push(
                container(scrollable(text(&self.output).size(13)).height(140))
                    .padding(12)
                    .style(container::rounded_box),
            );
        row![
            sidebar,
            container(main).padding(24).width(Fill).height(Fill)
        ]
        .into()
    }
    fn media_view(&self) -> Element<'_, Message> {
        let mut content = column![
            row![
                self.action("Media report", Message::Report, self.listing.is_some()),
                self.action(
                    "Import image…",
                    Message::Import,
                    self.listing.is_some() && !self.read_only()
                ),
                self.action("Export selected…", Message::Export, self.selected.is_some())
                    .style(button::secondary),
                self.action(
                    "Move to trash",
                    Message::Remove,
                    self.selected.is_some() && !self.read_only()
                )
                .style(button::danger)
            ]
            .spacing(8)
        ]
        .spacing(12);
        if self.read_only() {
            content = content.push(
                text("This mount is read-only. Browsing, export and backups are available.")
                    .size(13),
            );
        }
        if let Some(listing) = &self.listing {
            let path = listing
                .directory
                .strip_prefix(&listing.root)
                .unwrap_or(Path::new(""));
            content = content.push(
                row![
                    self.action("Up", Message::Up, listing.directory != listing.root)
                        .style(button::secondary),
                    text(format!("/{}", path.display())).width(Fill),
                    text_input("Filter files…", &self.filter)
                        .on_input(Message::Filter)
                        .padding(8)
                        .width(200)
                ]
                .spacing(12)
                .align_y(iced::Center),
            );
            let mut files = column![
                row![
                    text("NAME").size(11).width(Fill),
                    text("TYPE").size(11).width(90),
                    text("SIZE").size(11).width(95)
                ]
                .padding(8),
                rule::horizontal(1)
            ]
            .spacing(3);
            let filter = self.filter.to_lowercase();
            let mut count = 0;
            for entry in listing
                .entries
                .iter()
                .filter(|e| e.name.to_lowercase().contains(&filter))
            {
                count += 1;
                let selected = self.selected.as_deref() == Some(&entry.relative);
                let message = if entry.directory {
                    Message::Open(entry.relative.clone())
                } else {
                    Message::Select(entry.relative.clone())
                };
                let label = if entry.directory {
                    format!("[+] {}", entry.name)
                } else {
                    entry.name.clone()
                };
                let size = if entry.directory {
                    String::new()
                } else {
                    format_size(entry.size)
                };
                files = files.push(
                    button(
                        row![
                            text(label).size(14).width(Fill),
                            text(if entry.directory { "Folder" } else { "File" })
                                .size(12)
                                .width(90),
                            text(size).size(12).width(95)
                        ]
                        .spacing(8),
                    )
                    .width(Fill)
                    .padding([8, 10])
                    .on_press_maybe((!self.busy).then_some(message))
                    .style(if selected {
                        button::primary
                    } else {
                        button::text
                    }),
                );
            }
            if count == 0 {
                files = files.push(text("No files to show.").size(15));
            }
            content = content.push(scrollable(files).height(Fill));
        } else {
            content = content.push(container(column![text("Your floppy collection starts here").size(22), text("Choose the mounted USB folder above, or open Disk images to work with local files.").size(15), text("Original Gotek disk banks may expose only one floppy at a time. Open Disk banks to read other banks from a raw device or whole-device backup.").size(13)].spacing(14)).padding(24).center_y(Fill).height(Fill));
        }
        container(content).height(Fill).into()
    }
    fn form_view(&self) -> Element<'_, Message> {
        let options: Vec<_> = OPS
            .iter()
            .enumerate()
            .filter(|(_, o)| o.page == self.page)
            .map(|(i, _)| Choice(i))
            .collect();
        let op = &OPS[self.choice.0];
        let mut form = column![
            pick_list(options, Some(self.choice), Message::Choose)
                .padding(10)
                .width(Fill),
            text(op.description).size(14)
        ]
        .spacing(14);
        if op.media {
            form = form.push(text("Uses the mounted media directory selected above.").size(12));
        }
        for (index, field) in op.fields.iter().enumerate() {
            let input = text_input(field.label, &self.values[index])
                .on_input(move |s| Message::Field(index, s))
                .padding(10);
            let mut input_row = row![input].spacing(8);
            if !matches!(field.kind, Kind::Text) {
                input_row = input_row.push(
                    self.action("Browse…", Message::Browse(index), true)
                        .style(button::secondary),
                );
            }
            form = form.push(column![text(field.label).size(12), input_row].spacing(5));
        }
        let enabled = (!op.media || self.listing.is_some())
            && (!op.writes_media || !self.read_only())
            && self.values.iter().all(|s| !s.trim().is_empty());
        if op.writes_media && self.read_only() {
            form = form
                .push(text("Writes are disabled while this media is mounted read-only.").size(13));
        }
        form = form.push(self.action(op.title, Message::Run, enabled));
        scrollable(container(form).padding(18).style(container::rounded_box))
            .height(Fill)
            .into()
    }
}

fn operation_args(choice: Choice, root: &str, values: &[String]) -> Result<Vec<String>, String> {
    let op = OPS.get(choice.0).ok_or("Unknown operation")?;
    if values.len() != op.fields.len() {
        return Err("Incomplete operation form".into());
    }
    if op.media && root.is_empty() {
        return Err("Choose a mounted media directory first".into());
    }
    if let Some((i, _)) = values.iter().enumerate().find(|(_, v)| v.trim().is_empty()) {
        return Err(format!("{} is required", op.fields[i].label));
    }
    let mut args = vec![op.command.into()];
    if op.media {
        args.push(root.into());
    }
    args.extend_from_slice(values);
    if matches!(op.command, "bank-write" | "device-restore") {
        args.push("--confirm-write".into());
    }
    Ok(args)
}

async fn run_backend(args: Vec<String>) -> JobResult {
    let result = async {
        let exe = std::env::current_exe()?;
        let mut command = tokio::process::Command::new(exe);
        command.args(args).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        command.output().await
    }
    .await;
    match result {
        Ok(output) => {
            let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
            if !output.stderr.is_empty() {
                message.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            if message.trim().is_empty() {
                message = if output.status.success() {
                    "Done. Refresh the media browser to see changes."
                } else {
                    "Operation failed without diagnostic output."
                }
                .into();
            }
            if message.len() > 65536 {
                let mut end = 65536;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
                message.push_str("\n[Display limited to 64 KiB]");
            }
            JobResult {
                success: output.status.success(),
                output: message,
            }
        }
        Err(error) => JobResult {
            success: false,
            output: error.to_string(),
        },
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
        let read_only = mount_read_only(&root);
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
    #[test]
    fn bank_write_waits_for_confirmation_and_cancel_discards_it() {
        let (mut app, _) = App::new();
        app.busy = false;
        app.page = Page::Banks;
        app.choice = Choice(OPS.iter().position(|o| o.command == "bank-write").unwrap());
        app.values = [
            "device.bin",
            "gotek1440",
            "1",
            "new.img",
            "old.img",
            "recovery.img",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let _ = app.update(Message::Run);
        assert!(!app.busy, "backend must not run before confirmation");
        let (_, args) = app.pending.as_ref().unwrap();
        assert_eq!(args.last().unwrap(), "--confirm-write");
        assert_eq!(args[1], "device.bin");
        let _ = app.update(Message::Cancel);
        assert!(app.pending.is_none());
        assert!(!app.busy);
        let _ = app.update(Message::Confirm);
        assert!(!app.busy, "cancelled operation must not execute");
    }
    #[test]
    fn whole_device_restore_requires_gui_confirmation() {
        let (mut app, _) = App::new();
        app.busy = false;
        app.choice = Choice(
            OPS.iter()
                .position(|o| o.command == "device-restore")
                .unwrap(),
        );
        app.values = ["device.bin", "new.bin", "old.bin", "recovery.bin"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let _ = app.update(Message::Run);
        assert!(!app.busy);
        assert_eq!(
            app.pending.as_ref().unwrap().1.last().unwrap(),
            "--confirm-write"
        );
        let _ = app.update(Message::Cancel);
        assert!(app.pending.is_none());
    }
    #[test]
    fn forms_build_literal_arguments_without_shell_interpretation() {
        let choice = Choice(OPS.iter().position(|o| o.command == "slot").unwrap());
        let values = vec!["/tmp/a file;$(echo nope).img".into(), "auto".into()];
        assert_eq!(
            operation_args(choice, "/media/USB drive", &values).unwrap(),
            vec![
                "slot",
                "/media/USB drive",
                "/tmp/a file;$(echo nope).img",
                "auto"
            ]
        );
        assert!(operation_args(choice, "", &values).is_err());
        assert!(operation_args(choice, "/media/USB", &[]).is_err());
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
}
