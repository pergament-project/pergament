//! Notes exported to a folder as Markdown files, mirrored on each export.
//!
//! The folder's manifest names the files an earlier export wrote; only those
//! are ever updated or removed. Every other file is left alone.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::note_markdown::{
    Anchor, Publications, note_body_in, strip_app_links, tag_slug, utc_timestamp,
};
use crate::userdata::{Note, UserData};
use crate::{Error, Library, Result};

/// The file in the export folder that lists the files an export wrote.
pub const MANIFEST: &str = ".pergament-export.json";

const MANIFEST_VERSION: u32 = 1;
const MAX_NAME_CHARS: usize = 80;
/// Leaves room for ` N.md` within the 255-byte name limit of common file systems.
const MAX_NAME_BYTES: usize = 200;
const FALLBACK_NAME: &str = "Note";

/// What an export did, or would do with `dry_run`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub notes: usize,
    pub created: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// A file of a note: its file name and its whole contents.
pub struct NoteFile {
    pub guid: String,
    /// File name without `.md`.
    pub base: String,
    pub contents: String,
}

/// The manifest: file name → guid of the note it holds.
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    files: BTreeMap<String, String>,
}

/// One exported file of `note`, `body`/`anchor` from `note_markdown::note_body`.
pub fn note_file(note: &Note, body: &str, anchor: &Anchor) -> NoteFile {
    let mut contents = String::from("---\n");
    contents.push_str(&format!("pergament-id: {}\n", json(&note.guid)));
    if let Some(reference) = &anchor.reference {
        contents.push_str(&format!("reference: {}\n", json(reference)));
    }
    let tags: Vec<String> = note
        .tags
        .iter()
        .map(|t| tag_slug(t))
        .filter(|s| !s.is_empty())
        .collect();
    contents.push_str(&format!("tags: {}\n", json(&tags)));
    contents.push_str(&format!("created: {}\n", timestamp(&note.created)));
    contents.push_str(&format!("modified: {}\n", timestamp(&note.last_modified)));
    contents.push_str("---\n\n");
    contents.push_str(body);
    NoteFile {
        guid: note.guid.clone(),
        base: base_name(note),
        contents,
    }
}

/// `value` as JSON, which is also a YAML flow value.
fn json<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("strings serialize")
}

/// A timestamp as UTC, or the raw value quoted when it can't be read.
fn timestamp(ts: &str) -> String {
    utc_timestamp(ts).unwrap_or_else(|| json(ts))
}

/// The file name of a note without `.md`: its title, its first words, or `Note`.
fn base_name(note: &Note) -> String {
    let title = sanitize(&note.title);
    if !title.is_empty() {
        return title;
    }
    let text: String = strip_app_links(&note.content)
        .chars()
        .filter(|c| !"#*>_[]".contains(*c))
        .collect();
    let words: Vec<&str> = text.split_whitespace().take(8).collect();
    let words = sanitize(&words.join(" "));
    if !words.is_empty() {
        return words;
    }
    FALLBACK_NAME.to_string()
}

/// `name` made safe as a file name on Windows, macOS and Linux; `""` if nothing is left.
fn sanitize(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| !"<>:\"/\\|?*".contains(*c) && !c.is_control())
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_start_matches(['.', ' ']);
    let mut cut: String = trimmed.chars().take(MAX_NAME_CHARS).collect();
    while cut.len() > MAX_NAME_BYTES {
        cut.pop();
    }
    let mut out = cut.trim_end_matches(['.', ' ']).to_string();
    let stem_len = out.find('.').unwrap_or(out.len());
    if is_reserved(&out[..stem_len]) {
        out.insert(stem_len, '_');
    }
    out
}

/// A device name Windows reserves, with or without an extension.
fn is_reserved(stem: &str) -> bool {
    let stem = stem.to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let b = stem.as_bytes();
            b.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && (b'1'..=b'9').contains(&b[3])
        }
    }
}

/// A plain `*.md` file name in the folder: no separators and no leading dot
/// (so never `..`). Inner dots stay, as a title like `Wait.. what` is one name.
fn is_plain_md(name: &str) -> bool {
    name.len() > ".md".len()
        && name.ends_with(".md")
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

/// The manifest's files that are plain `*.md` names; empty when there is no manifest.
fn read_manifest(folder: &Path) -> Result<BTreeMap<String, String>> {
    let text = match fs::read_to_string(folder.join(MANIFEST)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.into()),
    };
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("{MANIFEST}: {e}")))?;
    if manifest.version != MANIFEST_VERSION {
        return Err(Error::Invalid(format!(
            "{MANIFEST}: unknown version {}",
            manifest.version
        )));
    }
    Ok(manifest
        .files
        .into_iter()
        .filter(|(name, _)| is_plain_md(name))
        .collect())
}

/// The entries of `folder` by name, `true` for regular files (not links or
/// folders); empty when it is missing.
fn list_folder(folder: &Path) -> Result<HashMap<String, bool>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = HashMap::new();
    for entry in entries {
        let entry = entry?;
        // Names that aren't UTF-8 can't clash with the names an export writes.
        if let Ok(name) = entry.file_name().into_string() {
            out.insert(name, entry.file_type()?.is_file());
        }
    }
    Ok(out)
}

/// Whether the manifest's `name` is a file an earlier export wrote: the folder
/// lists exactly that name, byte for byte, as a regular file. A case or Unicode
/// normalization variant may be the user's own file, so it never counts.
fn is_owned(name: &str, listed: &HashMap<String, bool>) -> bool {
    listed.get(name) == Some(&true)
}

/// Whether `name` is `{base}.md` or `{base} N.md`, ignoring case.
fn is_name_of(name: &str, base: &str) -> bool {
    let (name, base) = (name.to_lowercase(), base.to_lowercase());
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let Some(rest) = stem.strip_prefix(&base) else {
        return false;
    };
    rest.is_empty()
        || rest.strip_prefix(' ').is_some_and(|n| {
            !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit())
        })
}

/// An error unless `folder` is an absolute path, so nothing lands in the working directory.
fn check_folder(folder: &Path) -> Result<()> {
    if folder.is_absolute() {
        Ok(())
    } else {
        Err(Error::Invalid(format!("not a folder: {folder:?}")))
    }
}

/// Writes `files` into `folder` and mirrors earlier exports there; with `dry_run` only counts.
pub fn export(folder: &Path, files: Vec<NoteFile>, dry_run: bool) -> Result<ExportReport> {
    check_folder(folder)?;
    let manifest = read_manifest(folder)?;
    let on_disk = list_folder(folder)?;
    // Only regular files the manifest names are ours; a link or folder never is.
    let owned: BTreeMap<String, String> = manifest
        .into_iter()
        .filter(|(name, _)| is_owned(name, &on_disk))
        .collect();
    let owned_lower: HashMap<String, &str> = owned
        .keys()
        .map(|n| (n.to_lowercase(), n.as_str()))
        .collect();
    let foreign_lower: HashSet<String> = on_disk
        .keys()
        .filter(|n| !owned.contains_key(*n))
        .map(|n| n.to_lowercase())
        .collect();

    let bases: Vec<String> = files
        .iter()
        .map(|f| match sanitize(&f.base) {
            b if b.is_empty() => FALLBACK_NAME.to_string(),
            b => b,
        })
        .collect();
    let mut names: Vec<Option<String>> = vec![None; files.len()];
    let mut taken: HashSet<String> = HashSet::new();
    for (i, file) in files.iter().enumerate() {
        let kept = owned
            .iter()
            .find(|(name, guid)| {
                **guid == file.guid
                    && is_name_of(name, &bases[i])
                    && !taken.contains(&name.to_lowercase())
            })
            .map(|(name, _)| name.clone());
        if let Some(name) = kept {
            taken.insert(name.to_lowercase());
            names[i] = Some(name);
        }
    }
    let guids: HashSet<&str> = files.iter().map(|f| f.guid.as_str()).collect();
    for (i, base) in bases.iter().enumerate() {
        if names[i].is_some() {
            continue;
        }
        let name = (1..)
            .map(|n| match n {
                1 => format!("{base}.md"),
                n => format!("{base} {n}.md"),
            })
            .find(|name| {
                let lower = name.to_lowercase();
                !taken.contains(&lower)
                    && !foreign_lower.contains(&lower)
                    // An owned file differing only in case would be removed over
                    // it, and a gone note's file is removed, not overwritten.
                    && owned_lower
                        .get(&lower)
                        .is_none_or(|o| *o == name && guids.contains(owned[*o].as_str()))
            })
            .expect("some numbered name is free");
        taken.insert(name.to_lowercase());
        names[i] = Some(name);
    }
    let names: Vec<String> = names.into_iter().map(|n| n.expect("assigned")).collect();

    let assigned: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut report = ExportReport {
        notes: files.len(),
        ..ExportReport::default()
    };
    let mut writes = Vec::new();
    for (file, name) in files.iter().zip(&names) {
        if !is_plain_md(name) {
            return Err(Error::Invalid(format!("unsafe export file name {name:?}")));
        }
        let renamed =
            owned.values().any(|g| *g == file.guid) && owned.get(name) != Some(&file.guid);
        // An assigned name is never a foreign file, so only an owned one can be there.
        let existing = match owned.contains_key(name) {
            true => Some(fs::read(folder.join(name))?),
            false => None,
        };
        match existing {
            None if renamed => report.updated += 1,
            None => report.created += 1,
            Some(old) if !renamed && old == file.contents.as_bytes() => {
                report.unchanged += 1;
                continue;
            }
            Some(_) => report.updated += 1,
        }
        writes.push((name, owned.contains_key(name), &file.contents));
    }
    let removals: Vec<&String> = owned
        .keys()
        .filter(|name| !assigned.contains(name.as_str()))
        .collect();
    report.removed = removals
        .iter()
        .filter(|name| !guids.contains(owned[**name].as_str()))
        .count();
    if dry_run {
        return Ok(report);
    }

    let manifest = Manifest {
        version: MANIFEST_VERSION,
        files: names
            .iter()
            .zip(&files)
            .map(|(name, file)| (name.clone(), file.guid.clone()))
            .collect(),
    };
    let mut created = Vec::new();
    let written = write_export(folder, &writes, &removals, &manifest, &mut created);
    if written.is_err() {
        // Files left without a manifest would count as foreign next time.
        for path in &created {
            let _ = fs::remove_file(path);
        }
    }
    written.map(|()| report)
}

/// The writing part of [`export`]; every file it creates is added to `created`.
fn write_export(
    folder: &Path,
    writes: &[(&String, bool, &String)],
    removals: &[&String],
    manifest: &Manifest,
    created: &mut Vec<PathBuf>,
) -> Result<()> {
    fs::create_dir_all(folder)?;
    for (name, exists, contents) in writes {
        write_file(&folder.join(name), !exists, contents.as_bytes(), created)?;
    }
    for name in removals {
        match fs::remove_file(folder.join(name)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    let tmp = folder.join(format!("{MANIFEST}.tmp"));
    match fs::remove_file(&tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    write_file(&tmp, true, json(manifest).as_bytes(), created)?;
    fs::rename(&tmp, folder.join(MANIFEST))?;
    Ok(())
}

/// Writes `contents` to `path`; with `new`, fails rather than follow or replace
/// anything there, and adds the file to `created` once it exists.
fn write_file(path: &Path, new: bool, contents: &[u8], created: &mut Vec<PathBuf>) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if new {
        options.create_new(true);
    } else {
        options.truncate(true);
    }
    let mut file = options.open(path)?;
    if new {
        created.push(path.to_owned());
    }
    file.write_all(contents)?;
    Ok(())
}

/// Every note of `user`, resolved against `library`, exported into `folder`.
pub fn export_notes(
    library: &Library,
    user: &UserData,
    folder: &Path,
    dry_run: bool,
) -> Result<ExportReport> {
    check_folder(folder)?;
    let notes = user.all_notes(None)?;
    let mut publications = Publications::open(
        library,
        notes
            .iter()
            .filter_map(|n| n.location.as_ref().map(|l| &l.loc)),
    )?;
    let mut files = Vec::with_capacity(notes.len());
    for note in &notes {
        let (body, anchor) = note_body_in(&mut publications, note)?;
        files.push(note_file(note, &body, &anchor));
    }
    export(folder, files, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing of regular files, plus `others` that are links or folders.
    fn listed(files: &[&str], others: &[&str]) -> HashMap<String, bool> {
        let files = files.iter().map(|n| (n.to_string(), true));
        let others = others.iter().map(|n| (n.to_string(), false));
        files.chain(others).collect()
    }

    #[test]
    fn exact_listed_regular_file_is_owned() {
        assert!(is_owned("Hello.md", &listed(&["Hello.md"], &[])));
        assert!(is_owned(
            "Hello.md",
            &listed(&["Hello.md", "hello.md"], &[])
        ));
        assert!(is_owned("Caf\u{e9}.md", &listed(&["Caf\u{e9}.md"], &[])));
    }

    #[test]
    fn case_variant_is_not_owned() {
        assert!(!is_owned("Hello.md", &listed(&["hello.md"], &[])));
        assert!(!is_owned("Hello.md", &listed(&["HELLO.MD", "x.md"], &[])));
    }

    #[test]
    fn normalization_variant_is_not_owned() {
        assert!(!is_owned("Caf\u{e9}.md", &listed(&["Cafe\u{301}.md"], &[])));
    }

    #[test]
    fn case_and_normalization_variant_is_not_owned() {
        assert!(!is_owned("Caf\u{e9}.md", &listed(&["cafe\u{301}.md"], &[])));
    }

    #[test]
    fn link_folder_or_missing_name_is_not_owned() {
        assert!(!is_owned("Hello.md", &listed(&[], &["Hello.md"])));
        assert!(!is_owned("Hello.md", &listed(&[], &[])));
    }
}
