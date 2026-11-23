//! Playlist helpers: media file names, hashes, file types and a bounded,
//! hashing file copy. Storage and sync build on these.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

pub const TICKS_PER_SECOND: i64 = 10_000_000;
/// How long a photo is shown (4 s).
pub const PHOTO_TICKS: i64 = 40_000_000;
/// Media directory next to the userdata database.
pub const MEDIA_DIR: &str = "playlist-media";
/// Media directory inside the sync folder.
pub const SYNC_MEDIA_DIR: &str = "Pergament-media";
/// Largest accepted media file, per file (4 GiB).
pub const MAX_MEDIA: u64 = 4 << 30;

#[cfg(not(test))]
const LIMIT: u64 = MAX_MEDIA;
#[cfg(test)]
const LIMIT: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Audio,
    Video,
    Image,
}

/// A playlist in the list of playlists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    /// `Tag.TagId`.
    pub id: i64,
    pub name: String,
    pub items: i64,
    pub duration_ticks: i64,
    /// The first item's thumbnail.
    pub thumbnail: Option<PathBuf>,
}

/// Where an item's media comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ItemSource {
    /// A recording of a publication (`PlaylistItemLocationMap`).
    #[serde(rename_all = "camelCase")]
    Library {
        key_symbol: String,
        track: i64,
        issue_tag: i64,
        meps_language: i64,
        kind: ItemKind,
    },
    /// A file in the media directory (`PlaylistItemIndependentMediaMap`).
    #[serde(rename_all = "camelCase")]
    File {
        path: PathBuf,
        mime: String,
        /// Whether the file exists.
        available: bool,
    },
}

/// One item of a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItemInfo {
    /// `PlaylistItem.PlaylistItemId`.
    pub id: i64,
    pub label: String,
    pub kind: ItemKind,
    pub duration_ticks: i64,
    pub start_trim_ticks: Option<i64>,
    pub end_trim_ticks: Option<i64>,
    pub end_action: i64,
    /// Only when the file exists.
    pub thumbnail: Option<PathBuf>,
    pub source: ItemSource,
}

/// A playlist with its items in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub items: Vec<PlaylistItemInfo>,
}

/// A plain file name: 1..=128 chars, none of `/ \ :` or NUL, not starting with `.`.
pub fn safe_media_name(name: &str) -> bool {
    let len = name.chars().count();
    (1..=128).contains(&len) && !name.starts_with('.') && !name.contains(['/', '\\', ':', '\0'])
}

/// SHA-256, each byte as hex without zero padding (iOS JW Library).
pub fn ios_hash(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:x}")).collect()
}

/// Kind and MIME type by file extension. Video: mp4 m4v mov. Photo: jpg jpeg
/// png gif webp heic. Audio: mp3 m4a aac wav. The match is on lowercase only;
/// the caller lowercases the extension first.
pub fn file_type(ext: &str) -> Option<(ItemKind, &'static str)> {
    Some(match ext {
        "mp4" | "m4v" => (ItemKind::Video, "video/mp4"),
        "mov" => (ItemKind::Video, "video/quicktime"),
        "jpg" | "jpeg" => (ItemKind::Image, "image/jpeg"),
        "png" => (ItemKind::Image, "image/png"),
        "gif" => (ItemKind::Image, "image/gif"),
        "webp" => (ItemKind::Image, "image/webp"),
        "heic" => (ItemKind::Image, "image/heic"),
        "mp3" => (ItemKind::Audio, "audio/mpeg"),
        "m4a" => (ItemKind::Audio, "audio/mp4"),
        "aac" => (ItemKind::Audio, "audio/aac"),
        "wav" => (ItemKind::Audio, "audio/wav"),
        _ => return None,
    })
}

/// From a stored MimeType: `image/*` → Image, `audio/*` → Audio, `video/*` → Video.
pub fn kind_of_mime(mime: &str) -> Option<ItemKind> {
    match mime.split_once('/')?.0 {
        "image" => Some(ItemKind::Image),
        "audio" => Some(ItemKind::Audio),
        "video" => Some(ItemKind::Video),
        _ => None,
    }
}

/// Copy `src` to `dest` through `dest.part` and a rename, at most `MAX_MEDIA`
/// bytes, hashing as it goes. Returns the SHA-256 digest. On any error the
/// `.part` file is removed and `dest` is left untouched.
pub fn copy_hashed(src: &mut impl Read, dest: &Path) -> Result<[u8; 32]> {
    let mut part_name: OsString = dest.as_os_str().to_owned();
    part_name.push(".part");
    let part = PathBuf::from(part_name);

    let result = write_part(src, &part).and_then(|digest| {
        fs::rename(&part, dest)?;
        Ok(digest)
    });
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn write_part(src: &mut impl Read, part: &Path) -> Result<[u8; 32]> {
    let mut out = File::create(part)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        total += n as u64;
        if total > LIMIT {
            return Err(Error::TooLarge(format!(
                "media file exceeds {} bytes",
                LIMIT
            )));
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    out.sync_all()?;
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn safe_names() {
        assert!(safe_media_name("06D2E671-01A0-49C8-9642-DCFA28BF20E7.jpeg"));
        assert!(!safe_media_name(""));
        assert!(!safe_media_name(".x"));
        assert!(!safe_media_name("a/b"));
        assert!(!safe_media_name("a\\b"));
        assert!(!safe_media_name("C:x"));
        assert!(!safe_media_name("../x"));
        assert!(!safe_media_name("a\0b"));
        assert!(!safe_media_name(&"a".repeat(129)));
        assert!(safe_media_name(&"a".repeat(128)));
    }

    #[test]
    fn ios_hash_drops_padding() {
        assert_eq!(ios_hash(&[0x08, 0xab, 0x00]), "8ab0");
        let mut digest = [0xffu8; 32];
        digest[0] = 0x01;
        digest[5] = 0x0f;
        digest[9] = 0x00;
        assert_eq!(ios_hash(&digest).len(), 61);
    }

    #[test]
    fn file_types() {
        assert_eq!(file_type("mov"), Some((ItemKind::Video, "video/quicktime")));
        assert_eq!(file_type("png"), Some((ItemKind::Image, "image/png")));
        assert_eq!(file_type("heic"), Some((ItemKind::Image, "image/heic")));
        assert_eq!(file_type("m4a"), Some((ItemKind::Audio, "audio/mp4")));
        assert_eq!(file_type("txt"), None);
        // Case is the caller's job.
        assert_eq!(file_type("MOV"), None);
    }

    #[test]
    fn mime_kinds() {
        assert_eq!(kind_of_mime("image/jpeg"), Some(ItemKind::Image));
        assert_eq!(kind_of_mime("audio/mp4"), Some(ItemKind::Audio));
        assert_eq!(kind_of_mime("video/quicktime"), Some(ItemKind::Video));
        assert_eq!(kind_of_mime("text/plain"), None);
        assert_eq!(kind_of_mime("image"), None);
    }

    #[test]
    fn copy_hashed_writes_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let digest = copy_hashed(&mut Cursor::new(&data), &dest).unwrap();
        assert_eq!(digest[..], Sha256::digest(&data)[..]);
        assert_eq!(fs::read(&dest).unwrap(), data);
        assert!(!dir.path().join("a.bin.part").exists());
    }

    #[test]
    fn copy_hashed_rejects_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("big.bin");
        let mut src = std::io::repeat(0).take(LIMIT + 1);
        assert!(matches!(
            copy_hashed(&mut src, &dest),
            Err(Error::TooLarge(_))
        ));
        assert!(!dest.exists());
        assert!(!dir.path().join("big.bin.part").exists());
        // Exactly at the limit is fine.
        let mut src = std::io::repeat(0).take(LIMIT);
        assert!(copy_hashed(&mut src, &dest).is_ok());
    }
}
