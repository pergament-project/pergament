mod common;

use std::fs;
use std::path::Path;

use common::Fixture;
use pergament::note_export::{export, export_notes, note_file, ExportReport, NoteFile, MANIFEST};
use pergament::note_markdown::Anchor;
use pergament::userdata::{Loc, Note, NoteInput, UserData, BLOCK_VERSE};
use pergament::Library;

fn file(guid: &str, base: &str, contents: &str) -> NoteFile {
    NoteFile {
        guid: guid.into(),
        base: base.into(),
        contents: contents.into(),
    }
}

fn two() -> Vec<NoteFile> {
    vec![file("g1", "One", "one\n"), file("g2", "Two", "two\n")]
}

fn report(
    notes: usize,
    created: usize,
    updated: usize,
    removed: usize,
    unchanged: usize,
) -> ExportReport {
    ExportReport {
        notes,
        created,
        updated,
        removed,
        unchanged,
    }
}

fn manifest(folder: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(folder.join(MANIFEST)).unwrap()).unwrap()
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap()
}

/// The `.md` files in `folder`, sorted.
fn md_files(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|d| {
            d.map(|e| e.unwrap().file_name().into_string().unwrap())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn first_export_creates_files_and_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("notes");
    assert_eq!(
        export(&folder, two(), false).unwrap(),
        report(2, 2, 0, 0, 0)
    );
    assert_eq!(read(folder.join("One.md")), "one\n");
    assert_eq!(read(folder.join("Two.md")), "two\n");
    assert_eq!(
        manifest(&folder),
        serde_json::json!({"version": 1, "files": {"One.md": "g1", "Two.md": "g2"}})
    );
    assert!(!folder.join(format!("{MANIFEST}.tmp")).exists());
}

#[test]
fn same_input_again_is_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    assert_eq!(
        export(tmp.path(), two(), false).unwrap(),
        report(2, 0, 0, 0, 2)
    );
}

#[test]
fn changed_contents_update_and_dry_run_only_counts() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    let changed = || {
        vec![
            file("g1", "One", "one changed\n"),
            file("g2", "Two", "two\n"),
        ]
    };
    assert_eq!(
        export(tmp.path(), changed(), true).unwrap(),
        report(2, 0, 1, 0, 1)
    );
    assert_eq!(read(tmp.path().join("One.md")), "one\n");
    assert_eq!(
        export(tmp.path(), changed(), false).unwrap(),
        report(2, 0, 1, 0, 1)
    );
    assert_eq!(read(tmp.path().join("One.md")), "one changed\n");
}

#[test]
fn dry_run_of_a_missing_folder_creates_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("new");
    assert_eq!(export(&folder, two(), true).unwrap(), report(2, 2, 0, 0, 0));
    assert!(!folder.exists());
}

#[test]
fn title_change_renames_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    let renamed = vec![file("g1", "Uno", "one\n"), file("g2", "Two", "two\n")];
    assert_eq!(
        export(tmp.path(), renamed, false).unwrap(),
        report(2, 0, 1, 0, 1)
    );
    assert_eq!(md_files(tmp.path()), ["Two.md", "Uno.md"]);
    assert_eq!(
        manifest(tmp.path())["files"],
        serde_json::json!({"Two.md": "g2", "Uno.md": "g1"})
    );
}

#[test]
fn numbered_name_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let same = || vec![file("g1", "Same", "a\n"), file("g2", "Same", "b\n")];
    export(tmp.path(), same(), false).unwrap();
    assert_eq!(md_files(tmp.path()), ["Same 2.md", "Same.md"]);
    // In reverse order each note still keeps its name.
    let mut reversed = same();
    reversed.reverse();
    assert_eq!(
        export(tmp.path(), reversed, false).unwrap(),
        report(2, 0, 0, 0, 2)
    );
    assert_eq!(read(tmp.path().join("Same 2.md")), "b\n");
}

#[test]
fn dropped_note_is_removed() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    let one = vec![file("g1", "One", "one\n")];
    assert_eq!(
        export(tmp.path(), one, false).unwrap(),
        report(1, 0, 0, 1, 1)
    );
    assert_eq!(md_files(tmp.path()), ["One.md"]);
    assert_eq!(
        manifest(tmp.path())["files"],
        serde_json::json!({"One.md": "g1"})
    );
}

#[test]
fn owned_file_gone_from_disk_is_dropped_silently() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    fs::remove_file(tmp.path().join("Two.md")).unwrap();
    let one = vec![file("g1", "One", "one\n")];
    assert_eq!(
        export(tmp.path(), one, false).unwrap(),
        report(1, 0, 0, 0, 1)
    );
}

#[test]
fn foreign_files_are_left_alone() {
    for foreign in ["Hello.md", "hello.md"] {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(foreign), "mine").unwrap();
        let notes = vec![file("g1", "Hello", "note\n")];
        assert_eq!(
            export(tmp.path(), notes, false).unwrap(),
            report(1, 1, 0, 0, 0)
        );
        assert_eq!(read(tmp.path().join(foreign)), "mine");
        assert_eq!(read(tmp.path().join("Hello 2.md")), "note\n");
        assert_eq!(
            manifest(tmp.path())["files"],
            serde_json::json!({"Hello 2.md": "g1"})
        );
    }
}

#[test]
fn foreign_file_with_unknown_id_is_not_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let stray = "---\npergament-id: \"gone\"\n---\n\nx\n";
    fs::write(tmp.path().join("Stray.md"), stray).unwrap();
    export(tmp.path(), two(), false).unwrap();
    assert_eq!(
        export(tmp.path(), two(), false).unwrap(),
        report(2, 0, 0, 0, 2)
    );
    assert_eq!(read(tmp.path().join("Stray.md")), stray);
}

#[test]
fn unsafe_manifest_entries_are_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("notes");
    fs::create_dir_all(folder.join("sub")).unwrap();
    fs::write(tmp.path().join("x.md"), "outside").unwrap();
    fs::write(folder.join("sub/x.md"), "nested").unwrap();
    fs::write(folder.join(".hidden.md"), "hidden").unwrap();
    fs::write(folder.join("notes.txt"), "text").unwrap();
    fs::write(
        folder.join(MANIFEST),
        r#"{"version":1,"files":{"../x.md":"g1","sub/x.md":"g2","sub\\x.md":"g2",".hidden.md":"g3","notes.txt":"g4","..":"g5","":"g6",".md":"g7"}}"#,
    )
    .unwrap();
    // None of the notes is in the input, so each owned file would be removed.
    let notes = vec![file("g9", "Other", "o\n")];
    assert_eq!(
        export(&folder, notes, false).unwrap(),
        report(1, 1, 0, 0, 0)
    );
    assert_eq!(read(tmp.path().join("x.md")), "outside");
    assert_eq!(read(folder.join("sub/x.md")), "nested");
    assert_eq!(read(folder.join(".hidden.md")), "hidden");
    assert_eq!(read(folder.join("notes.txt")), "text");
    assert_eq!(
        manifest(&folder)["files"],
        serde_json::json!({"Other.md": "g9"})
    );
}

#[test]
fn unsafe_manifest_entries_are_not_reused_as_names() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("notes");
    fs::create_dir_all(&folder).unwrap();
    fs::write(tmp.path().join("x.md"), "outside").unwrap();
    fs::write(
        folder.join(MANIFEST),
        r#"{"version":1,"files":{"../x.md":"g1"}}"#,
    )
    .unwrap();
    let notes = vec![file("g1", "../x", "o\n")];
    export(&folder, notes, false).unwrap();
    assert_eq!(read(tmp.path().join("x.md")), "outside");
}

#[test]
fn failed_export_removes_the_files_it_created() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), two(), false).unwrap();
    let before = read(tmp.path().join(MANIFEST));
    // A folder where the manifest's temporary file goes makes the run fail at the end.
    let blocker = tmp.path().join(format!("{MANIFEST}.tmp"));
    fs::create_dir(&blocker).unwrap();
    fs::write(blocker.join("keep"), "x").unwrap();
    let three = || {
        let mut files = two();
        files.push(file("g3", "Three", "three\n"));
        files
    };
    for _ in 0..2 {
        assert!(export(tmp.path(), three(), false).is_err());
        assert_eq!(md_files(tmp.path()), ["One.md", "Two.md"]);
        assert_eq!(read(tmp.path().join(MANIFEST)), before);
        assert_eq!(read(blocker.join("keep")), "x");
    }
    fs::remove_dir_all(&blocker).unwrap();
    assert_eq!(
        export(tmp.path(), three(), false).unwrap(),
        report(3, 1, 0, 0, 2)
    );
    assert_eq!(md_files(tmp.path()), ["One.md", "Three.md", "Two.md"]);
}

#[test]
fn long_names_fit_in_255_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let title = "😀".repeat(80);
    let base = base_of(&title, "");
    assert!(base.len() <= 200, "{}", base.len());
    assert!(title.starts_with(&base));
    let notes = vec![
        file("g1", &title, "a\n"),
        file("g2", &title, "b\n"),
        file("g3", &"é".repeat(150), "c\n"),
    ];
    assert_eq!(
        export(tmp.path(), notes, false).unwrap(),
        report(3, 3, 0, 0, 0)
    );
    for name in md_files(tmp.path()) {
        assert!(name.len() <= 255, "{}", name.len());
    }
}

#[test]
fn name_of_a_gone_note_is_not_reused() {
    let tmp = tempfile::tempdir().unwrap();
    export(tmp.path(), vec![file("old", "Same", "old\n")], false).unwrap();
    let new = vec![file("new", "Same", "new\n")];
    assert_eq!(
        export(tmp.path(), new, true).unwrap(),
        report(1, 1, 0, 1, 0)
    );
    let new = vec![file("new", "Same", "new\n")];
    assert_eq!(
        export(tmp.path(), new, false).unwrap(),
        report(1, 1, 0, 1, 0)
    );
    assert_eq!(md_files(tmp.path()), ["Same 2.md"]);
    assert_eq!(read(tmp.path().join("Same 2.md")), "new\n");
}

#[test]
fn bad_manifest_is_an_error_and_writes_nothing() {
    for bad in [r#"{"version":2,"files":{}}"#, "not json", r#"{"files":{}}"#] {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(MANIFEST), bad).unwrap();
        assert!(export(tmp.path(), two(), false).is_err(), "{bad}");
        assert!(export(tmp.path(), two(), true).is_err(), "{bad}");
        assert!(md_files(tmp.path()).is_empty(), "{bad}");
        assert_eq!(read(tmp.path().join(MANIFEST)), bad);
    }
}

fn note(guid: &str, title: &str, content: &str) -> Note {
    Note {
        guid: guid.into(),
        title: title.into(),
        content: content.into(),
        block_type: 0,
        block_identifier: None,
        mark_guid: None,
        color: None,
        tags: vec![],
        last_modified: String::new(),
        created: String::new(),
        location: None,
    }
}

fn base_of(title: &str, content: &str) -> String {
    note_file(&note("g", title, content), "", &Anchor::default()).base
}

#[test]
fn file_names_are_sanitized() {
    assert_eq!(base_of("a/b:c?", ""), "abc");
    assert_eq!(base_of("CON", ""), "CON_");
    assert_eq!(base_of("lpt9", ""), "lpt9_");
    assert_eq!(base_of("COM10", ""), "COM10");
    assert_eq!(base_of(" ..a  \t b.. ", ""), "a b");
    assert_eq!(base_of("a\u{7}b", ""), "ab");
    assert_eq!(base_of("...", "From the content"), "From the content");
    let long = "ä".repeat(100);
    assert_eq!(base_of(&long, ""), "ä".repeat(80));
}

#[test]
fn file_name_falls_back_to_content_then_note() {
    assert_eq!(
        base_of(
            "",
            "# See [Gen 1:1](pergament://bible/1:1:1-1:1:1) *one* two three four five six seven"
        ),
        "See Gen 11 one two three four five"
    );
    assert_eq!(base_of(" ", "> _[]_"), "Note");
    assert_eq!(base_of("", ""), "Note");
}

#[test]
fn front_matter_is_exact() {
    let mut n = note("G-1", "T", "x");
    n.tags = vec!["tag one".into(), "two".into()];
    n.created = "2025-03-21T19:29:43+0100".into();
    n.last_modified = "2025-03-22T08:00:00+0100".into();
    let anchor = Anchor {
        reference: Some("Genesis 1:1 \"q\"".into()),
        quote: None,
    };
    let f = note_file(&n, "body\n", &anchor);
    assert_eq!(f.guid, "G-1");
    assert_eq!(f.base, "T");
    assert_eq!(
        f.contents,
        "---\npergament-id: \"G-1\"\nreference: \"Genesis 1:1 \\\"q\\\"\"\ntags: [\"tag-one\",\"two\"]\ncreated: 2025-03-21T18:29:43Z\nmodified: 2025-03-22T07:00:00Z\n---\n\nbody\n"
    );
}

#[test]
fn front_matter_without_reference_or_tags_keeps_raw_dates() {
    let mut n = note("g", "T", "x");
    n.created = "yesterday".into();
    let f = note_file(&n, "b\n", &Anchor::default());
    assert_eq!(
        f.contents,
        "---\npergament-id: \"g\"\ntags: []\ncreated: \"yesterday\"\nmodified: \"\"\n---\n\nb\n"
    );
}

#[test]
fn exports_the_notes_of_user_data() {
    let tmp = tempfile::tempdir().unwrap();
    let jwpub = tmp.path().join("in.jwpub");
    Fixture {
        bible_chapters: vec![(
            1,
            1,
            r#"<p><span id="v1-1-1-1" class="v">In the beginning</span></p>"#.into(),
        )],
        ..Fixture::default()
    }
    .write_to(&jwpub);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    lib.import(&jwpub).unwrap();
    let mut user = UserData::open(tmp.path().join("userData.db")).unwrap();
    let loc = Loc {
        key_symbol: "tst".into(),
        meps_language: 0,
        issue_tag: 0,
        document_id: None,
        book: Some(1),
        chapter: Some(1),
    };
    let on_verse = NoteInput {
        title: "Start".into(),
        content: "text".into(),
        block_type: BLOCK_VERSE,
        block_identifier: Some(1),
        ..NoteInput::default()
    };
    let guid = user.save_note(Some(&loc), None, &on_verse).unwrap();
    let free = NoteInput {
        content: "free note".into(),
        ..NoteInput::default()
    };
    user.save_note(None, None, &free).unwrap();

    let folder = tmp.path().join("out");
    assert_eq!(
        export_notes(&lib, &user, &folder, false).unwrap(),
        report(2, 2, 0, 0, 0)
    );
    let start = read(folder.join("Start.md"));
    assert!(
        start.starts_with(&format!(
            "---\npergament-id: \"{guid}\"\nreference: \"Genesis 1:1\"\n"
        )),
        "{start}"
    );
    assert!(
        start.ends_with("\n---\n\n**Start**\n\n> In the beginning\n> — Genesis 1:1\n\ntext\n"),
        "{start}"
    );
    assert!(read(folder.join("free note.md")).ends_with("---\n\nfree note\n"));
    assert_eq!(
        export_notes(&lib, &user, &folder, true).unwrap(),
        report(2, 0, 0, 0, 2)
    );
}
