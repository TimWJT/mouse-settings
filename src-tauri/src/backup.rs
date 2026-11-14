//! Copies of a profile taken right before it is overwritten, so any save can be undone.
//! Files live in the app's data folder as `profile-<address>-<unix seconds>.bin`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::error::Error;
use crate::profile::ProfileImage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub file: String,
    pub address: u16,
    pub saved_at: u64,
}

fn parse_name(name: &str) -> Option<(u16, u64)> {
    let rest = name.strip_prefix("profile-")?.strip_suffix(".bin")?;
    let (addr, secs) = rest.split_once('-')?;
    if !addr.bytes().all(|c| c.is_ascii_digit()) || !secs.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((addr.parse().ok()?, secs.parse().ok()?))
}

pub fn save(dir: &Path, address: u16, image: &ProfileImage) -> Result<String, Error> {
    fs::create_dir_all(dir).map_err(|e| Error::Refused(format!("couldn't create the backup folder ({e})")))?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Two saves in the same second get distinct names.
    let mut stamp = secs;
    let mut path: PathBuf;
    loop {
        path = dir.join(format!("profile-{address}-{stamp}.bin"));
        if !path.exists() {
            break;
        }
        stamp += 1;
    }
    fs::write(&path, image.bytes()).map_err(|e| Error::Refused(format!("couldn't save a backup first ({e})")))?;
    Ok(path.file_name().unwrap().to_string_lossy().into_owned())
}

pub fn list(dir: &Path) -> Vec<BackupEntry> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<BackupEntry> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let (address, saved_at) = parse_name(&file)?;
            Some(BackupEntry { file, address, saved_at })
        })
        .collect();
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    out
}

/// Loads a backup by file name (a bare name from `list`, never a path).
pub fn load(dir: &Path, file: &str) -> Result<(u16, ProfileImage), Error> {
    let (address, _) = parse_name(file).ok_or_else(|| Error::Invalid(format!("{file} isn't a backup file")))?;
    let bytes = fs::read(dir.join(file)).map_err(|e| Error::Invalid(format!("Couldn't read {file} ({e})")))?;
    let image = ProfileImage::from_bytes(&bytes)?;
    if !image.crc_ok() {
        return Err(Error::Invalid(format!("{file} is damaged (checksum mismatch)")));
    }
    Ok((address, image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::tests::sample;

    #[test]
    fn save_list_load_round_trip_and_rejects_paths() {
        let dir = std::env::temp_dir().join(format!("g502-backup-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = save(&dir, 1, &sample()).unwrap();
        let b = save(&dir, 1, &sample()).unwrap();
        assert_ne!(a, b);
        let listed = list(&dir);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].file, b, "newest first");
        assert_eq!(load(&dir, &a).unwrap(), (1, sample()));
        assert!(load(&dir, "../profile-1-1.bin").is_err());
        assert!(load(&dir, "notes.txt").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
