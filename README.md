# RustGotek

Rust desktop application built with Iced for managing Gotek USB media and
floppy images, with a command-line interface for the same operations. It
supports original-firmware bank layouts, bank and whole-device backup/restore,
FAT12 editing, HFE conversion, and FlashFloppy configuration/update staging.
Linux device access and the SFR1M44-U100K workflow have been tested; format
and platform limitations are documented below.

Build with Rust 1.92 or newer:

```sh
cargo build --release
./target/release/rustgotek
```

Launching without arguments (or with `gui`) opens the desktop interface. Use
**Choose folder** to select the mounted USB directory. The sidebar provides
media browsing and import/export, original-firmware disk banks, indexed slots, FAT12 and HFE image tools,
backups, configuration, and firmware staging. Operations run asynchronously,
and their results appear in the log with a copy button. File and folder inputs
have native pickers; destructive media changes require confirmation.

On Linux, read-only mounts are detected and media write actions are disabled.
The USB media page takes a mount directory, not `/dev/sda`. The Disk banks
page takes a raw device or whole-device image instead. A tiny FAT volume on a
larger USB device exposes only one bank through the normal file browser.
Directory backups do not capture other banks or boot sectors; use the whole-device
backup operation on the Disk banks page for those.

The GUI uses Iced 0.14 with its software renderer, Tokio, and native dialogs
through rfd. Linux requires a desktop session; native dialogs use its XDG
portal service. CLI commands remain available without a display.

On Windows the executable is `target\release\rustgotek.exe`. To install the
command into your Cargo binary directory, use `cargo install --path . --locked`.
Run `rustgotek --help` for the complete command list. Errors return exit code 1;
successful operations return 0. Commands work on image files and mounted media
directories; no administrative access is normally needed.

Original-firmware bank workflow:

```sh
rustgotek bank-list /dev/sda gotek1440
rustgotek bank-export /dev/sda gotek1440 1 ./bank001.img
rustgotek image-check ./bank001.img
rustgotek image-list ./bank001.img
rustgotek bank-backup /dev/sda ./usb-full.bin
rustgotek bank-verify /dev/sda ./usb-full.bin
rustgotek bank-replace ./usb-full.bin gotek1440 1 ./replacement.img ./usb-edited.bin
```

The same operations are available on **Disk banks** in the GUI.
Open it with a source prefilled using `cargo run -- gui banks /dev/sda`.
Linux raw reads
use UDisks and the desktop authentication dialog if ordinary read permission is
missing. Read operations request a read-only device handle. The explicit bank-write
and device-restore operations request exclusive read/write access. The GUI does not run as root
or alter device permissions. Other systems can work with whole-device
image files; raw Windows device paths are not supported yet.

Profiles are explicit: `gotek1440` means 1440 KiB payloads at 1536 KiB intervals,
and `gotek720` means 720 KiB payloads at 1024 KiB intervals. These are layout
facts documented in [LibDsk's Gotek driver](https://fossies.org/linux/privat/libdsk-1.5.22/lib/drvgotek.c)
and [GotekTool](https://github.com/sebmate/GotekTool). Confirm your model's
layout: a matching first volume alone cannot identify the spacing. The report
checks up to 1000 possible positions and marks matching boot headers, zero
headers, and mismatches separately. It does not establish the number of disks
supported by your firmware, validate FAT chains, or detect every legacy format.

Export is byte-for-byte and supports non-FAT and damaged disks for recovery.
Run image checks separately when applicable. Backup copies the **entire source
capacity**, including gaps and trailing bytes; the destination needs that much
free space. Keep the source unmounted or read-only and unchanged throughout
backup and verification. Replacement requires a regular backup file and an
exactly sized raw floppy image. It creates a new file, preserves padding and
all other bytes, and reads back the changed bank. All output paths must be new.
**Write a replacement to a bank** is available in the GUI with confirmation.
Export the bank first, edit that image using Disk images, then choose the
replacement, the exported original, and a new recovery image path. The CLI
requires the explicit confirmation flag:

```sh
rustgotek bank-write /dev/sda gotek1440 1 ./replacement.img ./bank001.img ./bank001-recovery.img --confirm-write
```

The target must still match the exported original. Before writing, RustGotek
saves, syncs and verifies the recovery image. It writes only the selected
payload, syncs it and checks every byte on read-back. Detected write failures
trigger an attempt to restore and verify the original bank; the recovery file
is retained even if that attempt fails. Power loss or removal can prevent
recovery, so keep that file. To undo a successful write, use the saved recovery
image as the replacement and the currently installed image as the expected
original, with a different new recovery path.

Linux raw writes request `O_EXCL` and reject mounted/in-use devices, as defined
by [open(2)](https://man7.org/linux/man-pages/man2/open.2.html); RustGotek does
not unmount them automatically. Regular image targets are exclusively locked
against cooperating writers. All raw-device operations other than bank-write and device-restore remain read-only.
**Restore a whole-device image** restores every byte, including bank gaps
and trailing data. It requires the same explicit confirmation, plus matching
replacement and expected-original image sizes:

```sh
rustgotek device-restore /dev/sda ./usb-edited.bin ./usb-full.bin ./usb-recovery.bin --confirm-write
```

The destination capacity must exactly match the image; neither truncation nor
partial restoration is allowed. The target is compared against the expected
backup before writing. A new full recovery copy is saved, synced and checked,
then the replacement is streamed and compared with the target. Memory use is
bounded independently of device capacity. Input images have shared locks and
the target has exclusive access; do not modify these files with other tools.
A detected failure attempts a full restore from the recovery file. Keep enough
free space for that file and retain it if recovery fails or power is lost.
**Create a new bank layout** creates a new image containing 1–1000 blank
FAT12 disks. Choose a bank count supported by your firmware. Each bank has a
unique serial and matching boot/root labels `FDISK000`, `FDISK001`, and so on.
No operating system is included. The selected fixed-stride profile determines
disk capacity and spacing; all gaps and unused capacity are zero.

```sh
rustgotek bank-create ./new-layout.bin gotek1440 100 minimum
rustgotek bank-list ./new-layout.bin gotek1440
```

For restoration onto a USB, replace `minimum` with its exact byte capacity from
`bank-list`; the capacity must be a multiple of 512 and large enough for all
banks, including the final gap. The output may use sparse storage but reads as
the full specified capacity. Creating this image never opens a USB device.
Inspect/export its disks before deploying it with the confirmed restore action
and an expected-original backup. Existing output files are never overwritten.

The layout uses FAT12 volumes at fixed offsets, with no partition table or
extra selector metadata. This follows the formatting approach used by
[usbfd](https://github.com/dennisMe2/usbfd/blob/master/usbfd); other Gotek firmware
layouts are not inferred. Generated disks pass independent FAT checks, but
acceptance by your emulator still requires a hardware test.

Example workflow (replace `/media/USB` with the mounted USB directory):

```sh
rustgotek list /media/USB
rustgotek media-info /media/USB
rustgotek backup /media/USB ./usb-backup
rustgotek verify /media/USB ./usb-backup
rustgotek init /media/USB indexed
rustgotek slot /media/USB ./disk.img 0
rustgotek config-show /media/USB
rustgotek config-set /media/USB nav-mode indexed
rustgotek export /media/USB DSKA0000.img ./saved.img
rustgotek remove /media/USB DSKA0000.img
rustgotek restore ./usb-backup ./restored-usb
```

`media-info ROOT` produces a read-only diagnostic report: image filename
counts, root update entries, the active FlashFloppy configuration path and
selected option lines, known selector/configuration files, and indexed-slot
conflicts. It notes a root `FF.CFG` ignored by an `FF/` directory. Counts include
hidden and trash directories; image content is not validated. Configuration
lines are shown as written, including any inline comments.

Files do not prove which firmware is installed: both FlashFloppy and HxC may
use HxC-related files, and settings can persist in device flash without a
configuration file. For hardware-specific support, provide this report
together with the Gotek's startup display/firmware version, model or MCU,
host computer/instrument, and desktop OS.

`init` requires that no active configuration already exists. Configuration
commands use `FF/FF.CFG` when `FF/` exists, otherwise `FF.CFG` in the root.
`config-set` preserves unrelated lines and creates a numbered backup before
replacement. It accepts arbitrary option names and does not validate firmware
semantics. Consult the [FlashFloppy configuration reference](https://github.com/keirf/flashfloppy/wiki/FF.CFG-Configuration-File).

`slot` uses `indexed-prefix` from the active configuration, defaulting to
`DSKA` when absent. It supports slots 0–999 and `auto` to select the first free
slot. `slots ROOT` lists matching root-directory names, reports duplicate
assignments with a nonzero exit status, and shows the next available slot.
Matching is case-insensitive and reserves all names with the indexed prefix
and four digits, including directories and unknown image extensions. This is
a filename inventory, not proof that firmware can open each image. Set
`nav-mode = indexed` to use these names. See
[navigation modes](https://github.com/keirf/flashfloppy/wiki/Image-Navigation-Modes).

```sh
rustgotek config-set /media/USB indexed-prefix '"DISK"'
rustgotek slot /media/USB ./disk.img auto
rustgotek slots /media/USB
```

Prefixes must be quoted, may be empty, and are limited to seven ASCII filename
characters by this utility. A leading dot is rejected because FlashFloppy
hides such names. Duplicate prefix settings are rejected as ambiguous.
Changing the configuration does not rename previously assigned images.

`slot-move ROOT FROM TO` reassigns one indexed image to a free slot, using the
active prefix and preserving its suffix (for example, `DSKA0003_game.img`
becomes `DSKA0009_game.img`). It copies the bytes, preserves OS permissions,
verifies the copy, and then removes the old name. The operation needs enough
free space for another copy and is not atomic: interruption can leave both
slots present. Occupied destinations, ambiguous source slots, and non-image
sources are rejected. Other metadata, such as timestamps, is not preserved.

Copies never overwrite existing files. Backups and restores require a new
destination directory with an existing parent. Failed directory copies may
leave a partial destination; errors are reported and no success is printed.
Backups copy file contents and directory structure, not filesystem attributes
or boot sectors. Backup and restore reread both trees and compare every file
byte before reporting success. `verify ROOT BACKUP` runs the same comparison
on an existing backup; it reports missing/extra paths, file/directory type
differences, and changed contents, with a nonzero exit status on mismatch.
Empty directories and hidden files are included. File comparisons use bounded
memory; timestamps and filesystem attributes are not compared. Verification
requires exclusive access to both trees and is not a hardware write-cache or
power-loss durability test.

`remove` copies a file to `.rustgotek-trash` before removing
the original; recover it with `export` and the printed relative trash path.

Use media exclusively while running commands, then safely eject it using your
OS. Path traversal and symbolic links inside managed paths are rejected;
concurrent changes to the directory tree are not supported.
Managed relative paths use ASCII case-insensitive lookup, including when
preparing media in a case-sensitive staging directory. Thus `ff/ff.cfg` is
recognized, and importing `disk.img` cannot create a second file beside
`DISK.IMG`. Ambiguous case-only duplicates are rejected. Non-ASCII filename
case folding is not implemented; backup verification compares exact names.

`create disk.img 1440` creates a zero-filled, **unformatted** raw image. It does
not create a DOS/Amiga filesystem. File extensions are screened on import,
but image contents and target-machine compatibility are not yet validated.

For HFE containers, `rustgotek inspect-hfe disk.hfe` reports version, geometry,
header bitrate, encoding, write flag, and per-side track lengths. It checks
HFE v1/v2/v3 signatures and revisions, table bounds, padded track extents, and
overlapping regions. It reads only the header and track table; it does not
decode track bitstreams, validate opcodes or sector checksums, convert images,
or prove compatibility with a host machine. Structural checks follow the
[HxC HFE specification](https://hxc2001.com/floppy_drive_emulator/HFE-file-format.html).

Convert standard PC raw-sector images to HFE v1 with an explicit geometry:

```sh
rustgotek convert-hfe disk.img disk.hfe pc1440
```

Profiles are `pc360` (40 cylinders, 9 sectors/track), `pc720` (80, 9),
`pc1200` (80, 15), and `pc1440` (80, 18), all with two sides and 512-byte
sectors. Input must have exactly the profile's capacity and be ordered by
cylinder, head, then sector. The output contains IBM MFM tracks with address
marks, CRCs and standard gaps. No FAT filesystem is required in the input.
The input is unchanged and existing output files are refused.

Conversion preserves raw sector contents, not original physical track timing,
weak bits or copy protection (which raw images cannot describe). It does not
convert ADF or DSK containers. These profiles were
verified by decoding every generated sector with
[Greaseweazle](https://github.com/keirf/greaseweazle) and comparing the decoded
images byte for byte. Hardware compatibility still depends on your host and
firmware configuration. Greaseweazle is not needed to run the Rust utility.

`convert-raw INPUT OUTPUT PROFILE` performs the reverse conversion for HFE v1
IBM MFM images using the same four profiles. It checks cylinder/head/sector
IDs, 512-byte sector sizes, and ID/data CRCs; duplicate, missing, deleted-data,
overlapping or damaged sectors are rejected before creating an output file.
It handles bit-shifted and circular tracks but does not decode HFE v2/v3
opcodes, FM/GCR, or copy-protected formats. Raw output loses track metadata,
and CRC checks do not validate every MFM clock bit. Existing output files are
refused. This direction was also verified against Greaseweazle-generated HFE
images, recovering every source byte for each supported profile.

```sh
rustgotek convert-raw disk.hfe recovered.img pc1440
```

For PC-compatible FAT12 data disks, use:

```sh
rustgotek create-fat12 dos-disk.img 1440
rustgotek inspect-fat12 dos-disk.img
rustgotek image-check dos-disk.img
rustgotek image-list dos-disk.img
rustgotek image-list dos-disk.img DOCUMENT
rustgotek image-extract dos-disk.img DOCUMENT/NOTES.TXT ./notes.txt
rustgotek image-import dos-disk.img ./notes.txt NOTES.TXT updated-disk.img
rustgotek image-mkdir dos-disk.img DOCUMENT with-directory.img
```

Supported capacities are 360, 720, 1200, 1440, and 2880 KiB. These images
contain an empty FAT12 filesystem, with no operating system. Creation refuses
to overwrite existing files. Choose a capacity supported by your host machine;
these PC layouts do not provide Amiga or other machine-specific filesystems.

`inspect-fat12` checks a 512-byte-sector FAT12 superfloppy BPB, image length,
region bounds, allocation-table capacity, reserved FAT entries, and agreement
between FAT copies. It reports geometry and free space from the FAT. It does
not check directory chains, cross-linked files, or lost clusters, and is not a
filesystem repair tool. Images larger than 32 MiB are rejected. Layout follows
the [Microsoft FAT specification](https://www.scs.stanford.edu/~zyedidia/docs/_other/fat.pdf).

`image-check IMAGE` additionally traverses all file and directory chains,
detecting cycles, cross-links, incorrect file sizes, invalid `.`/`..` entries,
duplicate short names, and allocated clusters that no directory references.
It reports bad clusters separately and makes no changes. Errors produce a
nonzero exit status. This checks FAT12 allocation consistency, not every
filesystem field: long-name records, OEM naming rules, and boot code are not
validated, and no repair is attempted.

`image-list` reads the root directory or a specified subdirectory. `image-extract`
reads a single file to a new destination, refusing overwrites. Both commands
operate directly on the image without mounting it or modifying it. Use DOS
8.3 short names, separated by `/`; ASCII names match without regard to case.
Long filename records are skipped, so use the corresponding short alias shown
by `image-list`. OEM code-page name decoding is not yet supported.

Extraction follows fragmented cluster chains and checks for cycles, invalid
links, and a mismatch between the chain length and file size before creating
the destination. These checks apply to the accessed paths, not all files on
the volume; cross-links between unrelated files are not detected.

`image-import INPUT SOURCE IMAGE_PATH OUTPUT` adds a file and saves a new
image. The input remains unchanged, and an existing output is never
overwritten. The parent directory must already exist. New filenames use ASCII
DOS 8.3 notation; duplicate short names are rejected. Subdirectories grow as
needed; the root directory has a fixed capacity. New entries use the fixed
timestamp January 1, 1980, rather than copying host timestamps.

Before importing, the utility traverses existing files and directories to
reject invalid chains, file-size mismatches, cross-links, and directory cycles.
It checks directory capacity and free clusters before creating the output.
Unreferenced allocated clusters are retained; this operation does not repair
filesystems. Long names and OEM code-page naming remain unsupported for new
entries. The source file has a 32 MiB read limit.

`image-mkdir INPUT IMAGE_PATH OUTPUT` creates a DOS 8.3 directory in a new
image, preserving the original. Its parent must exist; use successive commands
to create nested paths. It initializes the directory's `.` and `..` entries,
allocates a cluster, and applies the same capacity and cross-link checks as
file import. Existing names and output files are never overwritten.

`image-remove INPUT IMAGE_PATH OUTPUT` removes a file or empty directory from
a new copy of the image. Select long-named files by their short alias from
`image-list`; associated valid long-name records are removed too. Nonempty
directories, read-only entries, malformed long-name associations, and existing
output files are rejected. The original image is retained. Freed data clusters
are available for reuse but are not securely erased.

`image-rename INPUT IMAGE_PATH NEW_NAME OUTPUT` renames a file or directory
within its existing parent, saving a new image. `NEW_NAME` must be a DOS 8.3
name without a path. Contents, allocation chains, attributes and timestamps
are preserved. A nonempty directory can be renamed. Existing destination
names and read-only entries are rejected. When renaming a long-named file by
its short alias, its long-name records are removed and the new short name
becomes its sole name; this command does not move entries between directories.

FlashFloppy update files can be inspected and staged for an explicit MCU:

```sh
rustgotek firmware-inspect ./flashfloppy-3.44.upd
rustgotek firmware-stage /media/USB ./flashfloppy-3.44.upd at32f435
```

These filenames illustrate the workflow; use a trusted release appropriate
for your device. `firmware-inspect` checks legacy CRC16 or universal file,
catalogue and payload checksums, bounds, unique MCU entries and nonoverlapping
payloads. The implementation was also checked against both official 3.44
release update files. Checksums detect corruption; they do not authenticate
firmware or establish that its code is safe to execute. The container does
not encode a version or distinguish normal firmware from bootloader,
QuickDisk, test or debug builds.

`firmware-stage` accepts `stm32f105`, `at32f415`, or `at32f435`, checks that a
payload exists for that MCU, copies the original filename to USB root and
verifies the copied bytes. It refuses any existing `.upd` entry in the root;
archive old updates first. It neither downloads releases nor flashes a device.
Universal updates on STM32F105/AT32F415 require a compatible recent bootloader;
legacy `FF_Gotek-*.upd` files serve older bootloaders on those MCUs.

For a Gotek already running FlashFloppy, safely eject the prepared USB stick,
insert it into the Gotek, and power on while holding both direction buttons.
Release them when the update display appears; the bootloader performs the
update and reboots. Initial firmware programming and bootloader replacement
are separate workflows. Consult the official
[update instructions](https://github.com/keirf/flashfloppy/wiki/Firmware-Update)
and [update format generator](https://github.com/keirf/flashfloppy/blob/master/scripts/mk_update.py).

Remaining scope includes broader image conversion and filesystem editing,
firmware support beyond FlashFloppy, and hardware identification/testing.
These depend on the Gotek model, firmware, host machine, and desktop OS.
The utility does not communicate directly with the Gotek or flash firmware.

Verification:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

The GitHub Actions workflow tests Linux, Windows and macOS, checks the minimum
Rust version, and runs FAT interoperability tests on Linux. Adding this
workflow does not establish those platforms as verified until its jobs pass.
Local verification has been performed on Linux, including all 38 standard tests
with Rust 1.92.0 and the locked dependencies. A distributable source
archive can be built and verified with `cargo package --allow-dirty --offline`;
this does not publish the crate.

An additional interoperability test uses `fsck.fat`, `mkfs.fat`, `mcopy`,
`mdir`, `mmd`, and `mtype` from dosfstools and mtools. It checks all generated
capacities, writes and reads a file, and inspects an independently formatted
image. Run it when those tools are installed:

```sh
cargo test fat12_external_interoperability -- --ignored
cargo test long_name_removal_is_compatible_with_mtools -- --ignored
cargo test created_banks_external_interoperability -- --ignored
```

The HFE round-trip test additionally requires Greaseweazle's `gw` executable
on PATH (or set `RUSTGOTEK_GW` to its path):

```sh
cargo test hfe_conversion_external_roundtrip -- --ignored
```

The opt-in Linux device-write integration test needs UDisks and a desktop
authorization agent. It creates a temporary loop device, checks mounted-write
rejection, writes one bank and restores the full device, verifies recovery and unchanged neighboring bytes,
and detaches the device afterward. It never selects a physical USB drive:

```sh
cargo build --locked
python3 tests/linux-loop-check.py
```

This loop-device test has passed locally. Physical bank read/write and recovery
were also verified on an SFR1M44-U100K USB. The user read a generated test file
through the Gotek on a P3B-F/Pentium II running MS-DOS, then wrote RGREPLY.TXT
to A:. RustGotek extracted the expected reply from the returned USB. Bank 99
was restored to its saved original and independently compared byte-for-byte
using the release build. No extra save-button procedure was needed.
