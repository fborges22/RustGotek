use crate::{Result, walk};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::Path,
};

// Fixed-size reads handle files larger than memory and short-read boundaries.
pub(crate) fn equal_files(left: &Path, right: &Path) -> Result<bool> {
    let mut left = fs::File::open(left)?;
    let mut right = fs::File::open(right)?;
    let size = left.metadata()?.len();
    if size != right.metadata()?.len() {
        return Ok(false);
    }
    let mut a = [0u8; 65536];
    let mut b = [0u8; 65536];
    let mut remaining = size;
    while remaining > 0 {
        let count = remaining.min(a.len() as u64) as usize;
        left.read_exact(&mut a[..count])?;
        right.read_exact(&mut b[..count])?;
        if a[..count] != b[..count] {
            return Ok(false);
        }
        remaining -= count as u64;
    }
    // Detect growth during verification rather than silently ignoring it.
    if left.read(&mut a[..1])? != 0 || right.read(&mut b[..1])? != 0 {
        return Err(io::Error::other("file changed while verifying").into());
    }
    Ok(true)
}

pub fn verify(source: &Path, backup: &Path) -> Result<()> {
    if source == backup {
        return Err("source and backup must be different directories".into());
    }
    let mut left = vec![];
    let mut right = vec![];
    walk(source, source, &mut left)?;
    walk(backup, backup, &mut right)?;
    let left: BTreeSet<_> = left.into_iter().collect();
    let right: BTreeSet<_> = right.into_iter().collect();
    let mut differences = 0;
    let mut files = 0;
    let mut bytes = 0u64;
    for path in left.union(&right) {
        let display = path.to_string_lossy().escape_default().to_string();
        let issue = if !left.contains(path) {
            Some("EXTRA in backup")
        } else if !right.contains(path) {
            Some("MISSING from backup")
        } else {
            let from = source.join(path);
            let to = backup.join(path);
            let a = fs::symlink_metadata(&from)?;
            let b = fs::symlink_metadata(&to)?;
            if a.file_type() != b.file_type() {
                Some("TYPE differs")
            } else if a.is_file() {
                if equal_files(&from, &to)? {
                    files += 1;
                    bytes += a.len();
                    None
                } else {
                    Some("CONTENT differs")
                }
            } else {
                None
            }
        };
        if let Some(issue) = issue {
            println!("{issue}: {display}");
            differences += 1;
        }
    }
    if differences != 0 {
        return Err(format!("backup verification failed: {differences} differing paths").into());
    }
    println!("Verified {files} files ({bytes} bytes); directory trees and file contents match.");
    Ok(())
}
