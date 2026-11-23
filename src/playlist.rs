//! Playlist helpers: media file names, hashes, file types and a bounded,
//! hashing file copy. Storage and sync build on these.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sync_snapshot::{LocKey, loc_key};
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

/// A recording of a publication to add to a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRef {
    pub key_symbol: String,
    pub track: i64,
    pub issue_tag: i64,
    pub meps_language: i64,
    /// Audio or Video.
    pub kind: ItemKind,
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

/// One item of a playlist as the content hash sees it (spec "Content hash
/// of a playlist"): no database ids, and media by `FilePath` and stored
/// `Hash`, never by the file on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ItemContent {
    pub label: String,
    pub start_trim: Option<i64>,
    pub end_trim: Option<i64>,
    pub end_action: i64,
    pub source: SourceContent,
    /// `ThumbnailFilePath`.
    pub thumbnail: Option<String>,
    /// By `StartTimeTicks`.
    pub markers: Vec<MarkerContent>,
}

/// Where an item's media comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) enum SourceContent {
    /// Location key, `MajorMultimediaType` and `BaseDurationTicks`.
    Location(LocKey, i64, i64),
    /// `FilePath`, `Hash` and `DurationTicks`.
    Media(String, String, i64),
}

/// A `PlaylistItemMarker` with its verse and paragraph maps (sorted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MarkerContent {
    pub label: String,
    pub start: i64,
    pub duration: i64,
    pub end_transition: i64,
    pub verses: Vec<i64>,
    /// `(MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph)`.
    pub paragraphs: Vec<(i64, i64, i64)>,
}

/// The ordered items of playlist `tag` in database `schema` ("main" or
/// "inc"). An item with a location comes as that location, like in the
/// playlist view; an item without a location or media is left out. The
/// caller makes sure `schema` has the playlist tables (`inc` may lack them).
// Read by the merge and the sync of playlists; until then only by tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn content(conn: &Connection, schema: &str, tag: i64) -> Result<Vec<ItemContent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT i.PlaylistItemId, i.Label, i.StartTrimOffsetTicks, i.EndTrimOffsetTicks,
                i.EndAction, i.ThumbnailFilePath
         FROM {schema}.TagMap t JOIN {schema}.PlaylistItem i ON i.PlaylistItemId = t.PlaylistItemId
         WHERE t.TagId = ?1 ORDER BY t.Position, t.TagMapId"
    ))?;
    let rows = stmt
        .query_map([tag], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items = Vec::with_capacity(rows.len());
    for (id, label, start_trim, end_trim, end_action, thumbnail) in rows {
        let Some(source) = source_content(conn, schema, id)? else {
            continue;
        };
        items.push(ItemContent {
            label,
            start_trim,
            end_trim,
            end_action,
            source,
            thumbnail,
            markers: marker_content(conn, schema, id)?,
        });
    }
    Ok(items)
}

fn source_content(conn: &Connection, schema: &str, item: i64) -> Result<Option<SourceContent>> {
    let location: Option<(i64, i64, i64)> = conn
        .query_row(
            &format!(
                "SELECT LocationId, MajorMultimediaType, IFNULL(BaseDurationTicks, 0)
                 FROM {schema}.PlaylistItemLocationMap WHERE PlaylistItemId = ?1
                 ORDER BY LocationId LIMIT 1"
            ),
            [item],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((location, kind, ticks)) = location {
        return Ok(
            loc_key(conn, schema, location)?.map(|key| SourceContent::Location(key, kind, ticks))
        );
    }
    Ok(conn
        .query_row(
            &format!(
                "SELECT m.FilePath, m.Hash, im.DurationTicks
                 FROM {schema}.PlaylistItemIndependentMediaMap im
                 JOIN {schema}.IndependentMedia m ON m.IndependentMediaId = im.IndependentMediaId
                 WHERE im.PlaylistItemId = ?1 ORDER BY im.IndependentMediaId LIMIT 1"
            ),
            [item],
            |r| Ok(SourceContent::Media(r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

fn marker_content(conn: &Connection, schema: &str, item: i64) -> Result<Vec<MarkerContent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT PlaylistItemMarkerId, Label, StartTimeTicks, DurationTicks,
                EndTransitionDurationTicks
         FROM {schema}.PlaylistItemMarker WHERE PlaylistItemId = ?1
         ORDER BY StartTimeTicks, PlaylistItemMarkerId"
    ))?;
    let markers = stmt
        .query_map([item], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                MarkerContent {
                    label: r.get(1)?,
                    start: r.get(2)?,
                    duration: r.get(3)?,
                    end_transition: r.get(4)?,
                    verses: Vec::new(),
                    paragraphs: Vec::new(),
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut verses = conn.prepare(&format!(
        "SELECT VerseId FROM {schema}.PlaylistItemMarkerBibleVerseMap
         WHERE PlaylistItemMarkerId = ?1 ORDER BY VerseId"
    ))?;
    let mut paragraphs = conn.prepare(&format!(
        "SELECT MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph
         FROM {schema}.PlaylistItemMarkerParagraphMap WHERE PlaylistItemMarkerId = ?1
         ORDER BY MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph"
    ))?;
    markers
        .into_iter()
        .map(|(id, mut marker)| {
            marker.verses = verses
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            marker.paragraphs = paragraphs
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(marker)
        })
        .collect()
}

/// Fixed entries of a `.jwlplaylist` / `.jwlibrary` zip; a media file of
/// the same name would collide with them.
const RESERVED_NAMES: [&str; 3] = ["manifest.json", "userData.db", "default_thumbnail.png"];

/// A plain file name: 1..=128 chars, none of `/ \ :` or NUL, not starting
/// with `.`, and none of the [`RESERVED_NAMES`] (ignoring case).
pub fn safe_media_name(name: &str) -> bool {
    let len = name.chars().count();
    (1..=128).contains(&len)
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && !RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(name))
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
        // The fixed entries of a .jwlplaylist / .jwlibrary zip.
        for reserved in [
            "manifest.json",
            "userData.db",
            "default_thumbnail.png",
            "MANIFEST.JSON",
            "userdata.db",
            "Default_Thumbnail.PNG",
        ] {
            assert!(!safe_media_name(reserved), "{reserved}");
        }
        assert!(safe_media_name("manifest.json.png"));
        assert!(safe_media_name("userData.db.jpeg"));
        assert!(safe_media_name("06D2E671-01A0-49C8-9642-DCFA28BF20E7.png"));
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
