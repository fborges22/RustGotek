use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn usb_initialization_requires_confirmation_and_a_whole_usb_device() {
    let s = Sandbox::new();
    let image = s.0.join("existing.img");
    fs::write(&image, b"original data").unwrap();
    for args in [
        vec!["usb-init", "existing.img", "gotek720", "1"],
        vec![
            "usb-init",
            "existing.img",
            "gotek720",
            "1",
            "--confirm-write",
        ],
        vec![
            "usb-init",
            "existing.img",
            "gotek720",
            "0",
            "--confirm-write",
        ],
    ] {
        assert!(!s.run(&args).status.success());
        assert_eq!(fs::read(&image).unwrap(), b"original data");
    }
}

#[test]
fn media_report_distinguishes_files_from_installed_firmware() {
    let s = Sandbox::new();
    fs::create_dir_all(s.0.join("usb/ff")).unwrap();
    fs::write(s.0.join("usb/FF.CFG"), "nav-mode = native\n").unwrap();
    let config = "nav-mode = indexed\nhost = unspecified\nindexed-prefix = \"DISK\"\n";
    fs::write(s.0.join("usb/ff/ff.cfg"), config).unwrap();
    fs::write(s.0.join("usb/DISK0001.img"), b"image").unwrap();
    fs::write(s.0.join("usb/disk0001_other.hfe"), b"other").unwrap();
    fs::write(s.0.join("usb/HXCSDFE.CFG"), b"marker").unwrap();
    let result = s.run(&["media-info", "usb"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8_lossy(&result.stdout);
    for expected in [
        "Recognized image filenames: 2",
        "nav-mode: indexed",
        "root FF.CFG is ignored",
        "Present: HXCSDFE.CFG",
        "1 conflicts",
        "does not identify installed firmware",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    assert_eq!(
        fs::read_to_string(s.0.join("usb/ff/ff.cfg")).unwrap(),
        config
    );
    fs::create_dir(s.0.join("empty")).unwrap();
    let result = s.run(&["media-info", "empty"]);
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("configuration: absent"));
}

#[test]
fn raw_to_hfe_profiles_and_size_checks() {
    let s = Sandbox::new();
    for (profile, size) in [
        ("pc360", "360"),
        ("pc720", "720"),
        ("pc1200", "1200"),
        ("pc1440", "1440"),
    ] {
        let input = format!("{profile}.img");
        let output = format!("{profile}.hfe");
        s.ok(&["create-fat12", &input, size]);
        s.ok(&["convert-hfe", &input, &output, profile]);
        s.ok(&["inspect-hfe", &output]);
        let decoded = format!("{profile}-decoded.img");
        s.ok(&["convert-raw", &output, &decoded, profile]);
        assert_eq!(
            fs::read(s.0.join(&decoded)).unwrap(),
            fs::read(s.0.join(&input)).unwrap()
        );
        assert!(
            !s.run(&["convert-hfe", &input, &output, profile])
                .status
                .success()
        );
    }
    assert!(
        !s.run(&["convert-hfe", "pc360.img", "wrong.hfe", "pc1440"])
            .status
            .success()
    );
    assert!(!s.0.join("wrong.hfe").exists());
    assert!(
        !s.run(&["convert-hfe", "pc360.img", "wrong.hfe", "unknown"])
            .status
            .success()
    );
}

#[test]
#[ignore = "requires Greaseweazle gw; optionally set RUSTGOTEK_GW to executable"]
fn hfe_conversion_external_roundtrip() {
    let s = Sandbox::new();
    let gw = std::env::var_os("RUSTGOTEK_GW").unwrap_or_else(|| "gw".into());
    for (profile, kib, format) in [
        ("pc360", 360, "ibm.360"),
        ("pc720", 720, "ibm.720"),
        ("pc1200", 1200, "ibm.1200"),
        ("pc1440", 1440, "ibm.1440"),
    ] {
        let input = format!("{profile}.img");
        let hfe = format!("{profile}.hfe");
        let decoded = format!("{profile}-decoded.img");
        let bytes: Vec<u8> = (0..kib * 1024)
            .map(|i| ((i * 31 + i / 512) % 256) as u8)
            .collect();
        fs::write(s.0.join(&input), &bytes).unwrap();
        s.ok(&["convert-hfe", &input, &hfe, profile]);
        let out = Command::new(&gw)
            .current_dir(&s.0)
            .args(["convert", "--format", format, &hfe, &decoded])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            fs::read(s.0.join(&decoded)).unwrap(),
            bytes,
            "profile {profile}"
        );
        let independent = format!("{profile}-gw.hfe");
        let out = Command::new(&gw)
            .current_dir(&s.0)
            .args(["convert", "--format", format, &input, &independent])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let recovered = format!("{profile}-recovered.img");
        s.ok(&["convert-raw", &independent, &recovered, profile]);
        assert_eq!(fs::read(s.0.join(recovered)).unwrap(), bytes);
    }
}

#[test]
fn slot_moves_preserve_contents_suffix_and_reject_conflicts() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    fs::write(s.0.join("usb/ff.cfg"), "indexed-prefix = \"DISK\"\n").unwrap();
    let payload = vec![0x81; 150_123];
    fs::write(s.0.join("usb/disk0003_game.IMG"), &payload).unwrap();
    s.ok(&["slot-move", "usb", "3", "9"]);
    assert!(!s.0.join("usb/disk0003_game.IMG").exists());
    assert_eq!(
        fs::read(s.0.join("usb/DISK0009_game.IMG")).unwrap(),
        payload
    );
    fs::write(s.0.join("usb/DISK0005.img"), b"occupied").unwrap();
    for (from, to) in [("9", "5"), ("9", "9"), ("9", "1000"), ("2", "4")] {
        assert!(!s.run(&["slot-move", "usb", from, to]).status.success());
    }
    assert_eq!(
        fs::read(s.0.join("usb/DISK0009_game.IMG")).unwrap(),
        payload
    );
    fs::write(s.0.join("usb/disk0009_other.hfe"), b"conflict").unwrap();
    assert!(!s.run(&["slot-move", "usb", "9", "4"]).status.success());
    assert!(!s.0.join("usb/DISK0004_game.IMG").exists());
    fs::create_dir(s.0.join("usb/DISK0006.img")).unwrap();
    assert!(!s.run(&["slot-move", "usb", "6", "4"]).status.success());
}

#[test]
fn image_rename_preserves_allocations_and_directory_contents() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "base.img", "1440"]);
    s.ok(&["image-mkdir", "base.img", "DOCS", "dir.img"]);
    fs::write(s.0.join("source.txt"), b"rename preserves contents").unwrap();
    s.ok(&[
        "image-import",
        "dir.img",
        "source.txt",
        "DOCS/OLD.TXT",
        "input.img",
    ]);
    let original = fs::read(s.0.join("input.img")).unwrap();
    s.ok(&[
        "image-rename",
        "input.img",
        "DOCS/OLD.TXT",
        "NEW.TXT",
        "renamed.img",
    ]);
    let renamed = fs::read(s.0.join("renamed.img")).unwrap();
    // Renaming changes only the short name, leaving payload, FATs and metadata intact.
    let changes: Vec<_> = original
        .iter()
        .zip(&renamed)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(changes, vec![33 * 512 + 64, 33 * 512 + 65, 33 * 512 + 66]);
    assert_eq!(fs::read(s.0.join("input.img")).unwrap(), original);
    s.ok(&["image-rename", "renamed.img", "DOCS", "LETTERS", "tree.img"]);
    s.ok(&["image-check", "tree.img"]);
    s.ok(&["image-extract", "tree.img", "LETTERS/NEW.TXT", "out.txt"]);
    assert_eq!(
        fs::read(s.0.join("out.txt")).unwrap(),
        b"rename preserves contents"
    );
    s.ok(&[
        "image-import",
        "tree.img",
        "source.txt",
        "LETTERS/OTHER.TXT",
        "two.img",
    ]);
    for new_name in ["OTHER.TXT", "../BAD.TXT", "TOOLONGNAME.TXT"] {
        assert!(
            !s.run(&[
                "image-rename",
                "two.img",
                "LETTERS/NEW.TXT",
                new_name,
                "bad.img"
            ])
            .status
            .success()
        );
        assert!(!s.0.join("bad.img").exists());
    }
}

#[test]
fn managed_media_paths_follow_ascii_fat_case_rules() {
    let s = Sandbox::new();
    fs::create_dir_all(s.0.join("usb/ff")).unwrap();
    fs::create_dir_all(s.0.join("usb/Games")).unwrap();
    fs::write(
        s.0.join("usb/ff/ff.cfg"),
        "nav-mode = indexed\nindexed-prefix = \"TEST\"\n",
    )
    .unwrap();
    fs::write(s.0.join("usb/Games/Disk.IMG"), b"original").unwrap();
    fs::write(s.0.join("source.img"), b"new").unwrap();
    s.ok(&["config-show", "usb"]);
    s.ok(&["config-set", "usb", "nav-mode", "native"]);
    assert!(
        fs::read_to_string(s.0.join("usb/ff/ff.cfg"))
            .unwrap()
            .contains("nav-mode = native")
    );
    assert!(!s.run(&["init", "usb", "native"]).status.success());
    assert!(
        !s.run(&["import", "usb", "source.img", "games/disk.img"])
            .status
            .success()
    );
    s.ok(&["export", "usb", "games/disk.img", "out.img"]);
    assert_eq!(fs::read(s.0.join("out.img")).unwrap(), b"original");
    s.ok(&["slot", "usb", "source.img", "0"]);
    assert!(s.0.join("usb/TEST0000.img").exists());
    s.ok(&["remove", "usb", "GAMES/DISK.img"]);
    assert!(!s.0.join("usb/Games/Disk.IMG").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn ambiguous_case_names_are_rejected_on_staging_trees() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    fs::write(s.0.join("usb/ff.cfg"), "nav-mode = native\n").unwrap();
    fs::write(s.0.join("usb/FF.CFG"), "nav-mode = indexed\n").unwrap();
    let result = s.run(&["config-show", "usb"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ambiguous FAT filename"));
    assert!(
        !s.run(&["config-set", "usb", "nav-mode", "native"])
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(s.0.join("usb/FF.CFG")).unwrap(),
        "nav-mode = indexed\n"
    );
}

#[test]
fn firmware_staging_checks_target_and_existing_updates() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    // Synthetic legacy container, CRC independently calculated with Python binascii.
    // This is test data, not executable firmware.
    let bytes = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70, 89, 41, 29,
    ];
    fs::write(s.0.join("FF_Gotek-test.upd"), bytes).unwrap();
    s.ok(&["firmware-inspect", "FF_Gotek-test.upd"]);
    assert!(
        !s.run(&["firmware-stage", "usb", "FF_Gotek-test.upd", "at32f435"])
            .status
            .success()
    );
    assert_eq!(fs::read_dir(s.0.join("usb")).unwrap().count(), 0);
    s.ok(&["firmware-stage", "usb", "FF_Gotek-test.upd", "stm32f105"]);
    assert_eq!(fs::read(s.0.join("usb/FF_Gotek-test.upd")).unwrap(), bytes);
    assert!(
        !s.run(&["firmware-stage", "usb", "FF_Gotek-test.upd", "stm32f105"])
            .status
            .success()
    );
    fs::remove_file(s.0.join("usb/FF_Gotek-test.upd")).unwrap();
    fs::write(s.0.join("usb/OLD.UPD"), b"old").unwrap();
    assert!(
        !s.run(&["firmware-stage", "usb", "FF_Gotek-test.upd", "at32f415"])
            .status
            .success()
    );
    assert_eq!(fs::read(s.0.join("usb/OLD.UPD")).unwrap(), b"old");
    fs::remove_file(s.0.join("usb/OLD.UPD")).unwrap();
    let mut corrupt = bytes;
    corrupt[0] ^= 1;
    fs::write(s.0.join("FF_Gotek-test.upd"), corrupt).unwrap();
    assert!(
        !s.run(&["firmware-stage", "usb", "FF_Gotek-test.upd", "at32f415"])
            .status
            .success()
    );
    assert_eq!(fs::read_dir(s.0.join("usb")).unwrap().count(), 0);
}

#[test]
fn hfe_inspection_checks_variants_and_track_boundaries() {
    let s = Sandbox::new();
    // Two cylinders with 512 interleaved bytes each, separate from header/LUT.
    let mut bytes = vec![0xff; 2048];
    bytes[..8].copy_from_slice(b"HXCPICFE");
    bytes[8] = 0;
    bytes[9] = 2;
    bytes[10] = 2;
    bytes[11] = 0;
    bytes[12..14].copy_from_slice(&250u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&300u16.to_le_bytes());
    bytes[16] = 0;
    bytes[18..20].copy_from_slice(&1u16.to_le_bytes());
    bytes[512..520].copy_from_slice(&[2, 0, 0, 2, 3, 0, 0, 2]);
    for (signature, revision, version) in [
        (b"HXCPICFE", 0, 1),
        (b"HXCPICFE", 1, 2),
        (b"HXCHFEV3", 0, 3),
    ] {
        let mut variant = bytes.clone();
        variant[..8].copy_from_slice(signature);
        variant[8] = revision;
        fs::write(s.0.join("test.hfe"), &variant).unwrap();
        let output = s.run(&["inspect-hfe", "test.hfe"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("HFE v{version}")));
        assert_eq!(fs::read(s.0.join("test.hfe")).unwrap(), variant);
    }
    for (offset, value) in [
        (0, 0),
        (8, 2),
        (9, 0),
        (10, 3),
        (18, 0),
        (18, 255),
        (512, 0),
        (512, 1),
        (512, 255),
        (514, 1),
        (516, 2),
    ] {
        let mut corrupt = bytes.clone();
        corrupt[offset] = value;
        fs::write(s.0.join("bad.hfe"), corrupt).unwrap();
        assert!(
            !s.run(&["inspect-hfe", "bad.hfe"]).status.success(),
            "offset {offset}, value {value}"
        );
    }
    for length in [8, 511, 600, 1024, 2047] {
        fs::write(s.0.join("short.hfe"), &bytes[..length]).unwrap();
        assert!(!s.run(&["inspect-hfe", "short.hfe"]).status.success());
    }
}

#[test]
fn backup_verification_detects_content_and_tree_changes() {
    let s = Sandbox::new();
    fs::create_dir_all(s.0.join("usb/empty-directory")).unwrap();
    fs::write(s.0.join("usb/empty.img"), []).unwrap();
    let bytes: Vec<_> = (0..150_123).map(|i| (i % 251) as u8).collect();
    fs::write(s.0.join("usb/disk.img"), &bytes).unwrap();
    let backup = s.run(&["backup", "usb", "backup"]);
    assert!(backup.status.success());
    assert!(String::from_utf8_lossy(&backup.stdout).contains("Copied and verified"));
    s.ok(&["verify", "usb", "backup"]);
    assert!(!s.run(&["verify", "usb", "usb"]).status.success());
    let mut damaged = bytes.clone();
    damaged[100_000] ^= 0xff;
    fs::write(s.0.join("backup/disk.img"), damaged).unwrap();
    fs::remove_file(s.0.join("backup/empty.img")).unwrap();
    fs::write(s.0.join("backup/extra.img"), b"extra").unwrap();
    fs::remove_dir(s.0.join("backup/empty-directory")).unwrap();
    fs::write(s.0.join("backup/empty-directory"), b"wrong type").unwrap();
    let check = s.run(&["verify", "usb", "backup"]);
    assert!(!check.status.success());
    let report = String::from_utf8_lossy(&check.stdout);
    for expected in [
        "CONTENT differs: disk.img",
        "MISSING from backup: empty.img",
        "EXTRA in backup: extra.img",
        "TYPE differs: empty-directory",
    ] {
        assert!(report.contains(expected), "{report}");
    }
    assert_eq!(fs::read(s.0.join("usb/disk.img")).unwrap(), bytes);
}

#[test]
fn image_check_detects_lost_allocations_and_invalid_directories() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "base.img", "1440"]);
    s.ok(&["image-check", "base.img"]);
    let original = fs::read(s.0.join("base.img")).unwrap();
    let mut lost = original.clone();
    for start in [512, 5120] {
        lost[start + 3] = 0xff;
        lost[start + 4] = 0x0f;
    }
    fs::write(s.0.join("lost.img"), &lost).unwrap();
    let result = s.run(&["image-check", "lost.img"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("not referenced"));
    assert_eq!(fs::read(s.0.join("lost.img")).unwrap(), lost);
    s.ok(&["image-mkdir", "base.img", "DOCS", "directory.img"]);
    s.ok(&["image-check", "directory.img"]);
    let directory = fs::read(s.0.join("directory.img")).unwrap();
    let mut bad_dot = directory.clone();
    bad_dot[33 * 512 + 32 + 26] = 2;
    fs::write(s.0.join("bad-dot.img"), bad_dot).unwrap();
    let result = s.run(&["image-check", "bad-dot.img"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("dot entry"));
    let mut crosslinked = directory;
    crosslinked.copy_within(19 * 512..19 * 512 + 32, 19 * 512 + 32);
    crosslinked[19 * 512 + 32..19 * 512 + 43].copy_from_slice(b"OTHER      ");
    fs::write(s.0.join("crosslinked.img"), crosslinked).unwrap();
    let result = s.run(&["image-check", "crosslinked.img"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cross-linked"));
    let mut duplicates = original;
    for at in [19 * 512, 19 * 512 + 32] {
        duplicates[at..at + 11].copy_from_slice(b"EMPTY   TXT");
        duplicates[at + 11] = 0x20;
    }
    fs::write(s.0.join("duplicate.img"), duplicates).unwrap();
    let result = s.run(&["image-check", "duplicate.img"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("duplicate short names"));
}

#[test]
fn image_remove_reclaims_space_and_preserves_input() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "base.img", "720"]);
    s.ok(&["image-mkdir", "base.img", "DOCS", "dir.img"]);
    fs::write(s.0.join("data.bin"), vec![42; 5000]).unwrap();
    s.ok(&[
        "image-import",
        "dir.img",
        "data.bin",
        "DOCS/DATA.BIN",
        "full.img",
    ]);
    let original = fs::read(s.0.join("full.img")).unwrap();
    assert!(
        !s.run(&["image-remove", "full.img", "DOCS", "bad.img"])
            .status
            .success()
    );
    assert!(!s.0.join("bad.img").exists());
    s.ok(&["image-remove", "full.img", "DOCS/DATA.BIN", "empty-dir.img"]);
    s.ok(&["image-remove", "empty-dir.img", "DOCS", "empty.img"]);
    assert_eq!(fs::read(s.0.join("full.img")).unwrap(), original);
    assert_eq!(
        s.run(&["inspect-fat12", "base.img"]).stdout,
        s.run(&["inspect-fat12", "empty.img"]).stdout
    );
    s.ok(&[
        "image-import",
        "empty.img",
        "data.bin",
        "DATA.BIN",
        "reused.img",
    ]);
    s.ok(&["image-extract", "reused.img", "DATA.BIN", "out.bin"]);
    assert_eq!(fs::read(s.0.join("out.bin")).unwrap(), vec![42; 5000]);
    let mut readonly = fs::read(s.0.join("reused.img")).unwrap();
    readonly[7 * 512 + 11] |= 1;
    fs::write(s.0.join("readonly.img"), readonly).unwrap();
    assert!(
        !s.run(&["image-remove", "readonly.img", "DATA.BIN", "bad.img"])
            .status
            .success()
    );
    assert!(!s.0.join("bad.img").exists());
}

#[test]
#[ignore = "requires dosfstools and mtools on PATH"]
fn long_name_removal_is_compatible_with_mtools() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "base.img", "1440"]);
    fs::write(s.0.join("hello.txt"), b"long filename test").unwrap();
    let out = Command::new("mcopy")
        .current_dir(&s.0)
        .args(["-i", "base.img", "hello.txt", "::Long Filename.txt"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let listing = s.run(&["image-list", "base.img"]);
    assert!(listing.status.success());
    let text = String::from_utf8(listing.stdout).unwrap();
    let alias = text.split_whitespace().last().unwrap();
    s.ok(&[
        "image-rename",
        "base.img",
        alias,
        "SHORT.TXT",
        "renamed.img",
    ]);
    let out = Command::new("fsck.fat")
        .current_dir(&s.0)
        .args(["-n", "renamed.img"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = Command::new("mtype")
        .current_dir(&s.0)
        .args(["-i", "renamed.img", "::SHORT.TXT"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"long filename test");
    s.ok(&["image-remove", "base.img", alias, "removed.img"]);
    let out = Command::new("fsck.fat")
        .current_dir(&s.0)
        .args(["-n", "removed.img"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let mut corrupt = fs::read(s.0.join("base.img")).unwrap();
    corrupt[19 * 512 + 13] ^= 1;
    fs::write(s.0.join("corrupt.img"), corrupt).unwrap();
    assert!(
        !s.run(&["image-remove", "corrupt.img", alias, "bad.img"])
            .status
            .success()
    );
    assert!(!s.0.join("bad.img").exists());
}

#[test]
fn image_directories_can_be_created_nested_and_populated() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "original.img", "1440"]);
    let original = fs::read(s.0.join("original.img")).unwrap();
    s.ok(&["image-mkdir", "original.img", "DOCS", "docs.img"]);
    s.ok(&["image-mkdir", "docs.img", "DOCS/LETTERS", "nested.img"]);
    fs::write(s.0.join("letter.txt"), b"Hello from RustGotek\n").unwrap();
    s.ok(&[
        "image-import",
        "nested.img",
        "letter.txt",
        "DOCS/LETTERS/HELLO.TXT",
        "populated.img",
    ]);
    s.ok(&["image-list", "populated.img", "DOCS/LETTERS"]);
    s.ok(&[
        "image-extract",
        "populated.img",
        "DOCS/LETTERS/HELLO.TXT",
        "out.txt",
    ]);
    assert_eq!(
        fs::read(s.0.join("out.txt")).unwrap(),
        b"Hello from RustGotek\n"
    );
    assert_eq!(fs::read(s.0.join("original.img")).unwrap(), original);
    for path in [
        "docs",
        "DOCS/LETTERS",
        "MISSING/CHILD",
        "../BAD",
        "DOCS/LETTERS/HELLO.TXT/CHILD",
    ] {
        assert!(
            !s.run(&["image-mkdir", "populated.img", path, "bad.img"])
                .status
                .success()
        );
        assert!(!s.0.join("bad.img").exists());
    }
    assert!(
        !s.run(&["image-mkdir", "original.img", "NEW", "original.img"])
            .status
            .success()
    );
    assert_eq!(fs::read(s.0.join("original.img")).unwrap(), original);
}

#[test]
fn slots_honor_configuration_and_report_conflicts() {
    let s = Sandbox::new();
    fs::create_dir_all(s.0.join("usb/FF")).unwrap();
    fs::write(s.0.join("usb/FF.CFG"), "indexed-prefix = \"IGNORED\"\n").unwrap();
    fs::write(
        s.0.join("usb/FF/FF.CFG"),
        "# indexed-prefix = \"OLD\"\nindexed-prefix = \"Disk\" # custom\n",
    )
    .unwrap();
    fs::write(s.0.join("source.img"), b"test image").unwrap();
    fs::write(s.0.join("usb/disk0000_name.img"), b"occupied").unwrap();
    s.ok(&["slot", "usb", "source.img", "auto"]);
    assert_eq!(
        fs::read(s.0.join("usb/Disk0001.img")).unwrap(),
        b"test image"
    );
    assert!(!s.run(&["slot", "usb", "source.img", "0"]).status.success());
    let out = s.run(&["slots", "usb"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Next available slot (0–999): 0002"));
    fs::write(s.0.join("usb/DISK0001_other.hfe"), b"duplicate").unwrap();
    let out = s.run(&["slots", "usb"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("[CONFLICT]"));
    s.ok(&["config-set", "usb", "indexed-prefix", "\"\""]);
    s.ok(&["slot", "usb", "source.img", "auto"]);
    assert!(s.0.join("usb/0000.img").exists());
    let before = fs::read(s.0.join("usb/FF/FF.CFG")).unwrap();
    for invalid in [
        "UNQUOTED",
        "\"TOOLONG8\"",
        "\"../bad\"",
        "\"A/B\"",
        "\"A\" garbage",
        "\".\"",
    ] {
        assert!(
            !s.run(&["config-set", "usb", "indexed-prefix", invalid])
                .status
                .success()
        );
    }
    assert_eq!(fs::read(s.0.join("usb/FF/FF.CFG")).unwrap(), before);
    fs::write(
        s.0.join("usb/FF/FF.CFG"),
        "indexed-prefix = \"ONE\"\nindexed-prefix = \"TWO\"\n",
    )
    .unwrap();
    assert!(
        !s.run(&["slot", "usb", "source.img", "auto"])
            .status
            .success()
    );
}
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rustgotek-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rustgotek"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) {
        let result = self.run(args);
        assert!(
            result.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn formatted_images_and_corruption_detection() {
    let s = Sandbox::new();
    for size in ["360", "720", "1200", "1440", "2880"] {
        let name = format!("{size}.img");
        s.ok(&["create-fat12", &name, size]);
        s.ok(&["inspect-fat12", &name]);
        s.ok(&["image-list", &name]);
        assert_eq!(
            fs::metadata(s.0.join(&name)).unwrap().len(),
            size.parse::<u64>().unwrap() * 1024
        );
        assert!(!s.run(&["create-fat12", &name, size]).status.success());
    }
    assert!(
        !s.run(&["create-fat12", "invalid.img", "880"])
            .status
            .success()
    );
    assert!(!s.0.join("invalid.img").exists());
    let original = fs::read(s.0.join("1440.img")).unwrap();
    for offset in [11, 13, 14, 16, 17, 19, 22, 510, 512, 5120] {
        let mut damaged = original.clone();
        damaged[offset] ^= 0xff;
        fs::write(s.0.join("damaged.img"), damaged).unwrap();
        assert!(
            !s.run(&["inspect-fat12", "damaged.img"]).status.success(),
            "offset {offset}"
        );
    }
    fs::write(s.0.join("short.img"), &original[..500]).unwrap();
    assert!(!s.run(&["inspect-fat12", "short.img"]).status.success());
}

#[test]
#[ignore = "requires dosfstools and mtools on PATH"]
fn fat12_external_interoperability() {
    let s = Sandbox::new();
    fs::write(s.0.join("hello.txt"), b"Gotek interoperability\r\n").unwrap();
    for size in ["360", "720", "1200", "1440", "2880"] {
        let name = format!("{size}.img");
        s.ok(&["create-fat12", &name, size]);
        for (program, args) in [
            ("fsck.fat", vec!["-n", &name]),
            ("mcopy", vec!["-i", &name, "hello.txt", "::HELLO.TXT"]),
            ("mdir", vec!["-i", &name, "::"]),
            ("fsck.fat", vec!["-n", &name]),
        ] {
            let out = Command::new(program)
                .current_dir(&s.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{program}: {} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        s.ok(&["inspect-fat12", &name]);
        let out = Command::new("mtype")
            .current_dir(&s.0)
            .args(["-i", &name, "::HELLO.TXT"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"Gotek interoperability\r\n");
        let extracted = format!("{size}.txt");
        s.ok(&["image-extract", &name, "hello.txt", &extracted]);
        assert_eq!(fs::read(s.0.join(extracted)).unwrap(), out.stdout);
        let edited = format!("edited-{size}.img");
        s.ok(&["image-import", &name, "hello.txt", "NEW.TXT", &edited]);
        let directory_image = format!("directory-{size}.img");
        let nested_image = format!("nested-{size}.img");
        s.ok(&["image-mkdir", &edited, "DOCS", &directory_image]);
        s.ok(&[
            "image-mkdir",
            &directory_image,
            "DOCS/NESTED",
            &nested_image,
        ]);
        let check = Command::new("fsck.fat")
            .current_dir(&s.0)
            .args(["-n", &nested_image])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stdout)
        );
        let check = Command::new("mcopy")
            .current_dir(&s.0)
            .args(["-i", &nested_image, "hello.txt", "::DOCS/NESTED/HI.TXT"])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );
        let nested_output = format!("nested-{size}.txt");
        s.ok(&[
            "image-extract",
            &nested_image,
            "DOCS/NESTED/HI.TXT",
            &nested_output,
        ]);
        assert_eq!(
            fs::read(s.0.join(nested_output)).unwrap(),
            b"Gotek interoperability\r\n"
        );
        for (program, args) in [
            ("fsck.fat", vec!["-n", &edited]),
            ("mtype", vec!["-i", &edited, "::NEW.TXT"]),
        ] {
            let check = Command::new(program)
                .current_dir(&s.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                check.status.success(),
                "{}",
                String::from_utf8_lossy(&check.stdout)
            );
            if program == "mtype" {
                assert_eq!(check.stdout, out.stdout);
            }
        }
    }
    // Inspect a filesystem produced independently by dosfstools.
    s.ok(&["create", "external.img", "1440"]);
    let out = Command::new("mkfs.fat")
        .current_dir(&s.0)
        .args(["-F", "12", "external.img"])
        .output()
        .unwrap();
    assert!(out.status.success());
    s.ok(&["inspect-fat12", "external.img"]);
    s.ok(&["image-check", "external.img"]);
    let out = Command::new("mmd")
        .current_dir(&s.0)
        .args(["-i", "external.img", "::SUBDIR"])
        .output()
        .unwrap();
    assert!(out.status.success());
    // Grow beyond the first subdirectory cluster (which includes . and ..).
    let mut current = "external.img".to_owned();
    for n in 0..20 {
        let next = format!("growth-{n}.img");
        s.ok(&[
            "image-import",
            &current,
            "hello.txt",
            &format!("SUBDIR/F{n}.TXT"),
            &next,
        ]);
        current = next;
    }
    let out = Command::new("fsck.fat")
        .current_dir(&s.0)
        .args(["-n", &current])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = Command::new("mtype")
        .current_dir(&s.0)
        .args(["-i", &current, "::SUBDIR/F19.TXT"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"Gotek interoperability\r\n");
}

#[test]
fn image_import_preserves_original_and_rejects_failed_edits() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "disk.img", "360"]);
    let original = fs::read(s.0.join("disk.img")).unwrap();
    let payload: Vec<u8> = (0..3500).map(|n| (n % 251) as u8).collect();
    fs::write(s.0.join("source.bin"), &payload).unwrap();
    s.ok(&[
        "image-import",
        "disk.img",
        "source.bin",
        "FILE.BIN",
        "edited.img",
    ]);
    assert_eq!(fs::read(s.0.join("disk.img")).unwrap(), original);
    s.ok(&["image-extract", "edited.img", "file.bin", "out.bin"]);
    assert_eq!(fs::read(s.0.join("out.bin")).unwrap(), payload);
    let mut crosslinked = fs::read(s.0.join("edited.img")).unwrap();
    let root_offset = 5 * 512;
    crosslinked.copy_within(root_offset..root_offset + 32, root_offset + 32);
    crosslinked[root_offset + 32..root_offset + 43].copy_from_slice(b"OTHER   BIN");
    fs::write(s.0.join("crosslinked.img"), crosslinked).unwrap();
    assert!(
        !s.run(&[
            "image-import",
            "crosslinked.img",
            "source.bin",
            "NEW.BIN",
            "bad.img"
        ])
        .status
        .success()
    );
    assert!(!s.0.join("bad.img").exists());
    for name in [
        "file.bin",
        "TOOLONGNAME.TXT",
        "../BAD.TXT",
        "BAD?.TXT",
        "NO/DIR.TXT",
    ] {
        assert!(
            !s.run(&["image-import", "edited.img", "source.bin", name, "bad.img"])
                .status
                .success()
        );
        assert!(!s.0.join("bad.img").exists());
    }
    assert!(
        !s.run(&[
            "image-import",
            "disk.img",
            "source.bin",
            "FILE.BIN",
            "disk.img"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(s.0.join("disk.img")).unwrap(), original);
    fs::write(s.0.join("huge.bin"), vec![0; 400 * 1024]).unwrap();
    assert!(
        !s.run(&[
            "image-import",
            "disk.img",
            "huge.bin",
            "HUGE.BIN",
            "bad.img"
        ])
        .status
        .success()
    );
    assert!(!s.0.join("bad.img").exists());
    fs::write(s.0.join("empty.bin"), []).unwrap();
    s.ok(&[
        "image-import",
        "disk.img",
        "empty.bin",
        "EMPTY.BIN",
        "empty.img",
    ]);
    s.ok(&["image-extract", "empty.img", "EMPTY.BIN", "zero.bin"]);
    assert_eq!(fs::metadata(s.0.join("zero.bin")).unwrap().len(), 0);
    // Root capacity and allocation exhaustion fail without creating an output.
    let mut full = original.clone();
    let root = 5 * 512;
    for n in 0..112 {
        let at = root + n * 32;
        full[at..at + 11].copy_from_slice(format!("F{n:07}TXT").as_bytes());
        full[at + 11] = 0x20;
    }
    fs::write(s.0.join("full.img"), full).unwrap();
    assert!(
        !s.run(&[
            "image-import",
            "full.img",
            "empty.bin",
            "NEW.TXT",
            "bad.img"
        ])
        .status
        .success()
    );
    assert!(!s.0.join("bad.img").exists());
}

#[test]
fn fragmented_files_subdirectories_and_invalid_chains() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "disk.img", "1440"]);
    let mut data = fs::read(s.0.join("disk.img")).unwrap();
    fn fat(data: &mut [u8], n: usize, value: u16) {
        for start in [512, 5120] {
            let offset = start + n + n / 2;
            let pair = u16::from_le_bytes([data[offset], data[offset + 1]]);
            let updated = if n.is_multiple_of(2) {
                (pair & 0xf000) | value
            } else {
                (pair & 0x000f) | (value << 4)
            };
            data[offset..offset + 2].copy_from_slice(&updated.to_le_bytes());
        }
    }
    fn entry(data: &mut [u8], at: usize, name: &[u8; 11], cluster: u16, size: u32, attr: u8) {
        data[at..at + 11].copy_from_slice(name);
        data[at + 11] = attr;
        data[at + 26..at + 28].copy_from_slice(&cluster.to_le_bytes());
        data[at + 28..at + 32].copy_from_slice(&size.to_le_bytes());
    }
    let root = 19 * 512;
    let start = 33 * 512;
    entry(&mut data, root, b"SUBDIR     ", 2, 0, 0x10);
    entry(&mut data, start, b"FILE    BIN", 3, 600, 0x20);
    entry(&mut data, start + 32, b"EMPTY   TXT", 0, 0, 0x20);
    fat(&mut data, 2, 0xfff);
    fat(&mut data, 3, 5);
    fat(&mut data, 5, 0xfff);
    data[start + 512..start + 1024].fill(0x11);
    data[start + 1536..start + 1624].fill(0x22);
    fs::write(s.0.join("disk.img"), &data).unwrap();
    s.ok(&["image-list", "disk.img", "subdir"]);
    s.ok(&["image-extract", "disk.img", "/subdir/file.bin", "out.bin"]);
    let expected = [vec![0x11; 512], vec![0x22; 88]].concat();
    assert_eq!(fs::read(s.0.join("out.bin")).unwrap(), expected);
    assert!(
        !s.run(&["image-extract", "disk.img", "subdir/file.bin", "out.bin"])
            .status
            .success()
    );
    s.ok(&["image-extract", "disk.img", "subdir/empty.txt", "empty.txt"]);
    assert_eq!(fs::metadata(s.0.join("empty.txt")).unwrap().len(), 0);
    for bad in [0, 1, 3, 0xff0, 0xff7, 0xfff, 4000] {
        let mut corrupt = data.clone();
        fat(&mut corrupt, 3, bad);
        fs::write(s.0.join("bad.img"), corrupt).unwrap();
        assert!(
            !s.run(&["image-extract", "bad.img", "subdir/file.bin", "bad.bin"])
                .status
                .success(),
            "link {bad}"
        );
        assert!(!s.0.join("bad.bin").exists());
    }
    assert!(
        !s.run(&["image-extract", "disk.img", "subdir", "directory.bin"])
            .status
            .success()
    );
    assert!(
        !s.run(&["image-list", "disk.img", "../subdir"])
            .status
            .success()
    );
}

#[test]
fn media_lifecycle_preserves_bytes_and_refuses_overwrites() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    let bytes: Vec<u8> = (0..4096).map(|n| (n % 251) as u8).collect();
    fs::write(s.0.join("disk.img"), &bytes).unwrap();
    s.ok(&["slot", "usb", "disk.img", "7"]);
    assert!(!s.run(&["slot", "usb", "disk.img", "7"]).status.success());
    s.ok(&["export", "usb", "DSKA0007.img", "out.img"]);
    assert_eq!(fs::read(s.0.join("out.img")).unwrap(), bytes);
    assert!(
        !s.run(&["export", "usb", "DSKA0007.img", "out.img"])
            .status
            .success()
    );
    s.ok(&["backup", "usb", "backup"]);
    s.ok(&["restore", "backup", "restored"]);
    assert_eq!(fs::read(s.0.join("restored/DSKA0007.img")).unwrap(), bytes);
    s.ok(&["remove", "usb", "DSKA0007.img"]);
    assert!(!s.0.join("usb/DSKA0007.img").exists());
    assert_eq!(
        fs::read(s.0.join("usb/.rustgotek-trash/DSKA0007.img.deleted.0")).unwrap(),
        bytes
    );
}

#[test]
fn configuration_respects_ff_directory_and_backs_up() {
    let s = Sandbox::new();
    fs::create_dir_all(s.0.join("usb/FF")).unwrap();
    fs::write(s.0.join("usb/FF.CFG"), "ignored\n").unwrap();
    s.ok(&["init", "usb", "indexed"]);
    let original = fs::read(s.0.join("usb/FF/FF.CFG")).unwrap();
    s.ok(&["config-set", "usb", "nav-mode", "native"]);
    assert_eq!(fs::read(s.0.join("usb/FF/FF.CFG.bak.0")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(s.0.join("usb/FF.CFG")).unwrap(),
        "ignored\n"
    );
    let result = s.run(&["config-show", "usb"]);
    assert!(String::from_utf8_lossy(&result.stdout).contains("nav-mode = native"));
    assert!(
        !s.run(&["config-set", "usb", "host", "x\ninterface=ibmpc"])
            .status
            .success()
    );
}

#[test]
fn traversal_and_recursive_backup_are_rejected() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    fs::write(s.0.join("disk.img"), b"disk").unwrap();
    assert!(
        !s.run(&["import", "usb", "disk.img", "../escape.img"])
            .status
            .success()
    );
    assert!(!s.run(&["backup", "usb", "usb/nested"]).status.success());
    assert!(!s.0.join("escape.img").exists());
    assert!(!s.0.join("usb/nested").exists());
}

#[cfg(unix)]
#[test]
fn linked_paths_are_rejected() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    fs::create_dir(s.0.join("outside")).unwrap();
    std::os::unix::fs::symlink(s.0.join("outside"), s.0.join("usb/link")).unwrap();
    fs::write(s.0.join("disk.img"), b"disk").unwrap();
    assert!(
        !s.run(&["import", "usb", "disk.img", "link/escape.img"])
            .status
            .success()
    );
    assert!(!s.run(&["backup", "usb", "backup"]).status.success());
    assert!(!s.0.join("outside/escape.img").exists());
}

#[test]
fn legacy_banks_preserve_padding_and_unselected_data() {
    for (profile, kib, stride) in [("gotek1440", 1440, 0x180000), ("gotek720", 720, 0x100000)] {
        let s = Sandbox::new();
        s.ok(&["create-fat12", "floppy.img", &kib.to_string()]);
        let floppy = fs::read(s.0.join("floppy.img")).unwrap();
        // Nonzero gap and tail bytes detect accidentally copying contiguous
        // floppy lengths instead of the independent on-device bank stride.
        let mut original = vec![0xa5; 3 * stride + 79];
        for n in 0..3 {
            original[n * stride..n * stride + floppy.len()].copy_from_slice(&floppy);
        }
        fs::write(s.0.join("device.bin"), &original).unwrap();
        let report = s.run(&["bank-list", "device.bin", profile]);
        assert!(report.status.success());
        let report = String::from_utf8_lossy(&report.stdout);
        assert!(report.contains("Scanning 3 possible banks"), "{report}");
        assert_eq!(report.matches("Matching FAT BPB").count(), 3);
        s.ok(&["bank-export", "device.bin", profile, "1", "export.img"]);
        assert_eq!(fs::read(s.0.join("export.img")).unwrap(), floppy);
        s.ok(&["image-check", "export.img"]);
        s.ok(&["bank-backup", "device.bin", "backup.bin"]);
        assert_eq!(fs::read(s.0.join("backup.bin")).unwrap(), original);
        s.ok(&["bank-verify", "device.bin", "backup.bin"]);
        let replacement = vec![0x34; floppy.len()]; // Non-FAT raw floppy supported.
        fs::write(s.0.join("replacement.img"), &replacement).unwrap();
        s.ok(&[
            "bank-replace",
            "backup.bin",
            profile,
            "1",
            "replacement.img",
            "edited.bin",
        ]);
        let mut expected = original.clone();
        expected[stride..stride + replacement.len()].copy_from_slice(&replacement);
        assert_eq!(fs::read(s.0.join("edited.bin")).unwrap(), expected);
        assert_eq!(fs::read(s.0.join("backup.bin")).unwrap(), original);
        assert_eq!(fs::read(s.0.join("device.bin")).unwrap(), original);
        let mismatch = s.run(&["bank-verify", "device.bin", "edited.bin"]);
        assert!(!mismatch.status.success());
        assert!(String::from_utf8_lossy(&mismatch.stderr).contains(&format!("byte {stride}")));
        for args in [
            vec!["bank-export", "device.bin", profile, "3", "bad.img"],
            vec!["bank-export", "device.bin", profile, "1000", "bad.img"],
            vec!["bank-export", "device.bin", "unknown", "0", "bad.img"],
            vec!["bank-export", "device.bin", profile, "-1", "bad.img"],
            vec![
                "bank-replace",
                "device.bin",
                profile,
                "0",
                "device.bin",
                "bad.img",
            ],
            vec!["bank-backup", "device.bin", "device.bin"],
            vec!["bank-export", "device.bin", profile, "1", "export.img"],
            vec![
                "bank-replace",
                "backup.bin",
                profile,
                "0",
                "replacement.img",
                "backup.bin",
            ],
        ] {
            assert!(!s.run(&args).status.success(), "{args:?}");
            assert!(!s.0.join("bad.img").exists());
        }
        assert_eq!(fs::read(s.0.join("device.bin")).unwrap(), original);
        assert_eq!(fs::read(s.0.join("backup.bin")).unwrap(), original);
    }
}

#[test]
fn legacy_bank_scan_handles_zero_headers_and_truncated_payloads() {
    let s = Sandbox::new();
    let mut image = vec![0; 0x180000 + 1440 * 1024 - 1];
    fs::write(s.0.join("partial.bin"), &image).unwrap();
    let result = s.run(&["bank-list", "partial.bin", "gotek1440"]);
    assert!(result.status.success());
    let text = String::from_utf8_lossy(&result.stdout);
    assert!(text.contains("Scanning 1 possible banks"));
    assert!(text.contains("Zero boot sector"));
    assert!(
        !s.run(&["bank-export", "partial.bin", "gotek1440", "1", "bad.img"])
            .status
            .success()
    );
    assert!(!s.0.join("bad.img").exists());
    image.push(0);
    fs::write(s.0.join("complete.bin"), &image).unwrap();
    s.ok(&["bank-export", "complete.bin", "gotek1440", "1", "last.img"]);
    assert_eq!(
        fs::metadata(s.0.join("last.img")).unwrap().len(),
        1440 * 1024
    );
    fs::write(s.0.join("short.bin"), [0; 511]).unwrap();
    assert!(
        !s.run(&["bank-list", "short.bin", "gotek1440"])
            .status
            .success()
    );
    assert!(
        !s.run(&["bank-verify", "complete.bin", "short.bin"])
            .status
            .success()
    );
}

#[test]
fn bank_write_requires_confirmation_and_matching_original_and_saves_recovery() {
    for (profile, length, stride) in [
        ("gotek1440", 1440 * 1024, 0x180000),
        ("gotek720", 720 * 1024, 0x100000),
    ] {
        let s = Sandbox::new();
        let before = vec![0x32; stride * 3 + 47];
        fs::write(s.0.join("target.bin"), &before).unwrap();
        fs::write(s.0.join("replacement.img"), vec![0x65; length]).unwrap();
        s.ok(&["bank-export", "target.bin", profile, "1", "original.img"]);
        let args = [
            "bank-write",
            "target.bin",
            profile,
            "1",
            "replacement.img",
            "original.img",
            "recovery.img",
            "--confirm-write",
        ];
        assert!(!s.run(&args[..7]).status.success());
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
        assert!(!s.0.join("recovery.img").exists());
        // Existing recovery paths and stale expectations must fail before writing.
        fs::write(s.0.join("recovery.img"), b"keep").unwrap();
        assert!(!s.run(&args).status.success());
        assert_eq!(fs::read(s.0.join("recovery.img")).unwrap(), b"keep");
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
        fs::remove_file(s.0.join("recovery.img")).unwrap();
        fs::write(s.0.join("wrong.img"), vec![0x99; length]).unwrap();
        let mut wrong = args;
        wrong[5] = "wrong.img";
        assert!(!s.run(&wrong).status.success());
        assert!(!s.0.join("recovery.img").exists());
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
        wrong = args;
        wrong[3] = "1000";
        assert!(!s.run(&wrong).status.success());
        wrong[3] = "3";
        assert!(!s.run(&wrong).status.success());
        wrong = args;
        wrong[4] = "wrong-size.img";
        fs::write(s.0.join("wrong-size.img"), b"too small").unwrap();
        assert!(!s.run(&wrong).status.success());
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
        s.ok(&args);
        let after = fs::read(s.0.join("target.bin")).unwrap();
        assert_eq!(&after[..stride], &before[..stride]);
        assert_eq!(&after[stride..stride + length], vec![0x65; length]);
        assert_eq!(&after[stride + length..], &before[stride + length..]);
        assert_eq!(
            fs::read(s.0.join("recovery.img")).unwrap(),
            vec![0x32; length]
        );
        // Stale original blocks a repeated write even with a fresh recovery path.
        wrong = args;
        wrong[6] = "second-recovery.img";
        assert!(!s.run(&wrong).status.success());
        assert!(!s.0.join("second-recovery.img").exists());
        // The saved original is usable for a verified restore of this bank.
        s.ok(&[
            "bank-write",
            "target.bin",
            profile,
            "1",
            "recovery.img",
            "replacement.img",
            "undo-recovery.img",
            "--confirm-write",
        ]);
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
    }
}

#[test]
fn bank_write_refuses_locked_images() {
    let s = Sandbox::new();
    fs::write(s.0.join("target.bin"), vec![0; 1440 * 1024]).unwrap();
    fs::write(s.0.join("replacement.img"), vec![1; 1440 * 1024]).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(s.0.join("target.bin"))
        .unwrap();
    file.lock().unwrap();
    let result = s.run(&[
        "bank-write",
        "target.bin",
        "gotek1440",
        "0",
        "replacement.img",
        "target.bin",
        "recovery.img",
        "--confirm-write",
    ]);
    assert!(!result.status.success());
    assert!(!s.0.join("recovery.img").exists());
    drop(file);
    assert_eq!(
        fs::read(s.0.join("target.bin")).unwrap(),
        vec![0; 1440 * 1024]
    );
}

#[test]
fn full_device_restore_preserves_recovery_and_checks_every_byte() {
    let s = Sandbox::new();
    let size = 3 * 1024 * 1024 + 79; // crosses streaming chunks; non-aligned tail
    let before: Vec<u8> = (0..size).map(|n| (n % 251) as u8).collect();
    let mut replacement = before.clone();
    replacement[0] ^= 0xff;
    replacement[1024 * 1024 + 7] ^= 0xff;
    replacement[size - 1] ^= 0xff;
    fs::write(s.0.join("target.bin"), &before).unwrap();
    fs::write(s.0.join("expected.bin"), &before).unwrap();
    fs::write(s.0.join("replacement.bin"), &replacement).unwrap();
    let args = [
        "device-restore",
        "target.bin",
        "replacement.bin",
        "expected.bin",
        "recovery.bin",
        "--confirm-write",
    ];
    assert!(!s.run(&args[..5]).status.success());
    assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
    fs::write(s.0.join("recovery.bin"), b"keep").unwrap();
    assert!(!s.run(&args).status.success());
    assert_eq!(fs::read(s.0.join("recovery.bin")).unwrap(), b"keep");
    assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
    fs::remove_file(s.0.join("recovery.bin")).unwrap();
    // Wrong expectation, source sizes and aliases all fail before target writes.
    for (index, value) in [(3, "replacement.bin"), (2, "target.bin"), (3, "target.bin")] {
        let mut invalid = args;
        invalid[index] = value;
        assert!(!s.run(&invalid).status.success(), "{invalid:?}");
        assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
        assert!(!s.0.join("recovery.bin").exists());
    }
    fs::write(s.0.join("small.bin"), &before[..size - 1]).unwrap();
    let mut invalid = args;
    invalid[2] = "small.bin";
    assert!(!s.run(&invalid).status.success());
    invalid[3] = "small.bin";
    assert!(!s.run(&invalid).status.success());
    s.ok(&args);
    assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), replacement);
    assert_eq!(fs::read(s.0.join("recovery.bin")).unwrap(), before);
    assert_eq!(fs::read(s.0.join("expected.bin")).unwrap(), before);
    assert_eq!(fs::read(s.0.join("replacement.bin")).unwrap(), replacement);
    s.ok(&[
        "device-restore",
        "target.bin",
        "recovery.bin",
        "replacement.bin",
        "undo.bin",
        "--confirm-write",
    ]);
    assert_eq!(fs::read(s.0.join("target.bin")).unwrap(), before);
    assert_eq!(fs::read(s.0.join("undo.bin")).unwrap(), replacement);
}

#[test]
fn bank_layout_creation_formats_distinct_disks_and_zeroes_gaps() {
    for (profile, payload, stride) in [
        ("gotek1440", 1440 * 1024, 0x180000),
        ("gotek720", 720 * 1024, 0x100000),
    ] {
        let s = Sandbox::new();
        let capacity = 3 * stride + 1024;
        s.ok(&[
            "bank-create",
            "new.bin",
            profile,
            "3",
            &capacity.to_string(),
        ]);
        let data = fs::read(s.0.join("new.bin")).unwrap();
        assert_eq!(data.len(), capacity);
        let mut serials = std::collections::BTreeSet::new();
        for bank in 0..3 {
            let offset = bank * stride;
            let floppy = format!("bank{bank}.img");
            s.ok(&[
                "bank-export",
                "new.bin",
                profile,
                &bank.to_string(),
                &floppy,
            ]);
            s.ok(&["image-check", &floppy]);
            let bytes = fs::read(s.0.join(&floppy)).unwrap();
            assert_eq!(bytes.len(), payload);
            let label = format!("FDISK{bank:03}   ");
            assert_eq!(&bytes[43..54], label.as_bytes());
            let word = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
            let root = (word(14) + bytes[16] as usize * word(22)) * 512;
            assert_eq!(&bytes[root..root + 11], label.as_bytes());
            assert_eq!(bytes[root + 11], 8);
            assert!(serials.insert(bytes[39..43].to_vec()));
            assert!(
                data[offset + payload..offset + stride]
                    .iter()
                    .all(|b| *b == 0)
            );
        }
        assert!(data[3 * stride..].iter().all(|b| *b == 0));
        let report = s.run(&["bank-list", "new.bin", profile]);
        assert!(report.status.success());
        assert_eq!(
            String::from_utf8_lossy(&report.stdout)
                .matches("Matching FAT BPB")
                .count(),
            3
        );
        // Generated banks support the normal file-edit workflow.
        fs::write(s.0.join("hello.txt"), b"new bank test").unwrap();
        s.ok(&[
            "image-import",
            "bank1.img",
            "hello.txt",
            "HELLO.TXT",
            "edited.img",
        ]);
        s.ok(&["image-check", "edited.img"]);
        s.ok(&[
            "bank-replace",
            "new.bin",
            profile,
            "1",
            "edited.img",
            "ready.bin",
        ]);
        s.ok(&["bank-export", "ready.bin", profile, "1", "again.img"]);
        s.ok(&["image-extract", "again.img", "HELLO.TXT", "readback.txt"]);
        assert_eq!(
            fs::read(s.0.join("readback.txt")).unwrap(),
            b"new bank test"
        );
        s.ok(&["bank-create", "minimum.bin", profile, "1", "minimum"]);
        assert_eq!(
            fs::metadata(s.0.join("minimum.bin")).unwrap().len(),
            stride as u64
        );
        assert!(
            !s.run(&["bank-create", "new.bin", profile, "1", "minimum"])
                .status
                .success()
        );
        assert_eq!(fs::read(s.0.join("new.bin")).unwrap(), data);
        for (count, size) in [
            ("0", "minimum"),
            ("1001", "minimum"),
            ("-1", "minimum"),
            ("3", "512"),
            ("1", "2000001"),
            ("1", "18446744073709551615"),
        ] {
            assert!(
                !s.run(&["bank-create", "invalid.bin", profile, count, size])
                    .status
                    .success()
            );
            assert!(!s.0.join("invalid.bin").exists());
        }
    }
}

#[test]
#[ignore = "requires dosfstools and mtools on PATH"]
fn created_banks_external_interoperability() {
    let s = Sandbox::new();
    for profile in ["gotek1440", "gotek720"] {
        let whole = format!("{profile}.bin");
        s.ok(&["bank-create", &whole, profile, "3", "minimum"]);
        for bank in 0..3 {
            let name = format!("{profile}-{bank}.img");
            s.ok(&["bank-export", &whole, profile, &bank.to_string(), &name]);
            let path = s.0.join(name);
            let check = Command::new("fsck.fat")
                .arg("-n")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                check.status.success(),
                "{} {}",
                String::from_utf8_lossy(&check.stdout),
                String::from_utf8_lossy(&check.stderr)
            );
            let listing = Command::new("mdir")
                .arg("-i")
                .arg(path)
                .arg("::")
                .output()
                .unwrap();
            assert!(listing.status.success());
            assert!(String::from_utf8_lossy(&listing.stdout).contains(&format!("FDISK{bank:03}")));
        }
    }
}

#[test]
fn disk_labels_update_both_records_and_preserve_files_and_originals() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "blank.img", "1440"]);
    fs::write(s.0.join("notes.txt"), b"keep me").unwrap();
    s.ok(&[
        "image-import",
        "blank.img",
        "notes.txt",
        "NOTES.TXT",
        "disk.img",
    ]);
    let original = fs::read(s.0.join("disk.img")).unwrap();
    s.ok(&["image-label", "disk.img", "my samples", "labelled.img"]);
    s.ok(&["image-label", "labelled.img", "SYNTH 2", "renamed.img"]);
    s.ok(&["image-check", "renamed.img"]);
    s.ok(&["image-extract", "renamed.img", "NOTES.TXT", "export.txt"]);
    assert_eq!(fs::read(s.0.join("export.txt")).unwrap(), b"keep me");
    assert_eq!(fs::read(s.0.join("disk.img")).unwrap(), original);
    let labelled = fs::read(s.0.join("renamed.img")).unwrap();
    assert_eq!(&labelled[43..54], b"SYNTH 2    ");
    let root = 19 * 512;
    let labels: Vec<_> = labelled[root..root + 224 * 32]
        .as_chunks::<32>()
        .0
        .iter()
        .take_while(|e| e[0] != 0)
        .filter(|e| e[0] != 0xe5 && e[11] == 8)
        .collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(&labels[0][..11], b"SYNTH 2    ");
    for invalid in ["", "TOO LONG A LABEL", "BAD/NAME", "é", "BAD.NAME"] {
        assert!(
            !s.run(&["image-label", "disk.img", invalid, "invalid.img"])
                .status
                .success()
        );
        assert!(!s.0.join("invalid.img").exists());
    }
    assert!(
        !s.run(&["image-label", "disk.img", "LABEL", "renamed.img"])
            .status
            .success()
    );
    assert_eq!(fs::read(s.0.join("renamed.img")).unwrap(), labelled);
}

#[test]
fn usb_eject_requires_explicit_confirmation_and_refuses_image_files() {
    let s = Sandbox::new();
    fs::write(s.0.join("disk.img"), b"unchanged").unwrap();
    assert!(!s.run(&["usb-eject", "disk.img"]).status.success());
    assert!(
        !s.run(&["usb-eject", "disk.img", "--confirm-eject"])
            .status
            .success()
    );
    assert_eq!(fs::read(s.0.join("disk.img")).unwrap(), b"unchanged");
}

#[test]
fn bank_swap_and_format_save_recovery_and_leave_other_banks() {
    let s = Sandbox::new();
    s.ok(&["bank-create", "usb.bin", "gotek720", "3", "minimum"]);
    let stride = 1024 * 1024;
    let payload = 720 * 1024;
    let original = fs::read(s.0.join("usb.bin")).unwrap();
    let bank = |data: &[u8], n: usize| data[n * stride..n * stride + payload].to_vec();
    // Swapping requires confirmation and distinct banks.
    assert!(
        !s.run(&[
            "bank-swap",
            "usb.bin",
            "gotek720",
            "0",
            "2",
            "a.img",
            "b.img"
        ])
        .status
        .success()
    );
    assert!(
        !s.run(&[
            "bank-swap",
            "usb.bin",
            "gotek720",
            "1",
            "1",
            "a.img",
            "b.img",
            "--confirm-write"
        ])
        .status
        .success()
    );
    s.ok(&[
        "bank-swap",
        "usb.bin",
        "gotek720",
        "0",
        "2",
        "a.img",
        "b.img",
        "--confirm-write",
    ]);
    let swapped = fs::read(s.0.join("usb.bin")).unwrap();
    assert_eq!(bank(&swapped, 0), bank(&original, 2));
    assert_eq!(bank(&swapped, 2), bank(&original, 0));
    assert_eq!(bank(&swapped, 1), bank(&original, 1));
    assert_eq!(fs::read(s.0.join("a.img")).unwrap(), bank(&original, 0));
    assert_eq!(fs::read(s.0.join("b.img")).unwrap(), bank(&original, 2));
    // Existing recovery files are never overwritten.
    assert!(
        !s.run(&[
            "bank-swap",
            "usb.bin",
            "gotek720",
            "0",
            "2",
            "a.img",
            "c.img",
            "--confirm-write"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(s.0.join("usb.bin")).unwrap(), swapped);

    s.ok(&["bank-export", "usb.bin", "gotek720", "1", "before.img"]);
    fs::write(s.0.join("data.bin"), b"payload").unwrap();
    s.ok(&[
        "image-import",
        "before.img",
        "data.bin",
        "DATA.BIN",
        "full.img",
    ]);
    s.ok(&[
        "bank-write",
        "usb.bin",
        "gotek720",
        "1",
        "full.img",
        "before.img",
        "r1.img",
        "--confirm-write",
    ]);
    s.ok(&[
        "bank-format",
        "usb.bin",
        "gotek720",
        "1",
        "Blank Disk",
        "r2.img",
        "--confirm-write",
    ]);
    s.ok(&["bank-export", "usb.bin", "gotek720", "1", "after.img"]);
    let listing = s.run(&["image-list", "after.img"]);
    assert!(!String::from_utf8_lossy(&listing.stdout).contains("DATA.BIN"));
    s.ok(&["image-check", "after.img"]);
    let labelled = fs::read(s.0.join("after.img")).unwrap();
    assert_eq!(&labelled[43..54], b"BLANK DISK ");
    assert_eq!(
        fs::read(s.0.join("r2.img")).unwrap(),
        fs::read(s.0.join("full.img")).unwrap()
    );
    let formatted = fs::read(s.0.join("usb.bin")).unwrap();
    assert_eq!(bank(&formatted, 0), bank(&swapped, 0));
    assert_eq!(bank(&formatted, 2), bank(&swapped, 2));
}

#[test]
fn image_format_erases_in_place_and_refuses_odd_sizes() {
    let s = Sandbox::new();
    s.ok(&["create-fat12", "disk.img", "1440"]);
    fs::write(s.0.join("note.txt"), b"hello").unwrap();
    s.ok(&[
        "image-import",
        "disk.img",
        "note.txt",
        "NOTE.TXT",
        "full.img",
    ]);
    let before = fs::read(s.0.join("full.img")).unwrap();
    assert!(
        !s.run(&["image-format", "full.img", "-", "r.img"])
            .status
            .success()
    );
    s.ok(&["image-format", "full.img", "-", "r.img", "--confirm-write"]);
    assert_eq!(fs::read(s.0.join("r.img")).unwrap(), before);
    assert_eq!(
        fs::metadata(s.0.join("full.img")).unwrap().len(),
        before.len() as u64
    );
    s.ok(&["image-check", "full.img"]);
    let listing = s.run(&["image-list", "full.img"]);
    assert!(!String::from_utf8_lossy(&listing.stdout).contains("NOTE.TXT"));
    fs::write(s.0.join("odd.img"), vec![0; 1000]).unwrap();
    assert!(
        !s.run(&["image-format", "odd.img", "-", "r2.img", "--confirm-write"])
            .status
            .success()
    );
    assert!(!s.0.join("r2.img").exists());
}

#[test]
fn slot_swap_renames_both_images_and_moves_into_empty_slots() {
    let s = Sandbox::new();
    fs::create_dir(s.0.join("usb")).unwrap();
    fs::write(s.0.join("usb/DSKA0001_game.img"), b"one").unwrap();
    fs::write(s.0.join("usb/DSKA0003.hfe"), b"three").unwrap();
    s.ok(&["slot-swap", "usb", "1", "3"]);
    assert_eq!(fs::read(s.0.join("usb/DSKA0003_game.img")).unwrap(), b"one");
    assert_eq!(fs::read(s.0.join("usb/DSKA0001.hfe")).unwrap(), b"three");
    assert!(!s.0.join("usb/DSKA0001_game.img").exists());
    s.ok(&["slot-swap", "usb", "1", "7"]);
    assert_eq!(fs::read(s.0.join("usb/DSKA0007.hfe")).unwrap(), b"three");
    assert!(!s.run(&["slot-swap", "usb", "4", "5"]).status.success());
    assert!(!s.run(&["slot-swap", "usb", "3", "3"]).status.success());
    let names: Vec<_> = fs::read_dir(s.0.join("usb"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn recovery_copies_are_optional_for_bank_and_disk_writes() {
    let s = Sandbox::new();
    s.ok(&["bank-create", "usb.bin", "gotek720", "2", "minimum"]);
    let before = fs::read(s.0.join("usb.bin")).unwrap();
    s.ok(&[
        "bank-swap",
        "usb.bin",
        "gotek720",
        "0",
        "1",
        "-",
        "-",
        "--confirm-write",
    ]);
    assert_ne!(fs::read(s.0.join("usb.bin")).unwrap(), before);
    s.ok(&[
        "bank-format",
        "usb.bin",
        "gotek720",
        "0",
        "EMPTY",
        "-",
        "--confirm-write",
    ]);
    s.ok(&["bank-export", "usb.bin", "gotek720", "1", "one.img"]);
    s.ok(&["bank-export", "usb.bin", "gotek720", "0", "zero.img"]);
    s.ok(&[
        "bank-write",
        "usb.bin",
        "gotek720",
        "1",
        "zero.img",
        "one.img",
        "-",
        "--confirm-write",
    ]);
    s.ok(&["create-fat12", "disk.img", "720"]);
    let output = s.run(&["image-format", "disk.img", "-", "-", "--confirm-write"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("No recovery copy"));
    // Only the files created above exist: no recovery images were written.
    let mut names: Vec<_> = fs::read_dir(&s.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["disk.img", "one.img", "usb.bin", "zero.img"]);
}
