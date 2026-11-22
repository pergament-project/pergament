mod common;

use common::Fixture;
use pergament::note_markdown::{resolve, Anchor};
use pergament::userdata::{Loc, BLOCK_PARAGRAPH, BLOCK_VERSE};
use pergament::{Library, Publication};

#[test]
fn finds_a_publication_by_user_data_key() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("in.jwpub");
    Fixture {
        bible_chapters: vec![(1, 1, "<p>In the beginning</p>".into())],
        ..Fixture::default()
    }
    .write_to(&file);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    let entry = lib.import(&file).unwrap();

    let found = Publication::find(&lib, "tst", 0, 0).unwrap().unwrap();
    assert_eq!(found.entry().dir_name, entry.dir_name);
    assert!(Publication::find(&lib, "tst", 1, 0).unwrap().is_none());
    assert!(Publication::find(&lib, "xyz", 0, 0).unwrap().is_none());
}

fn library(fixture: Fixture) -> (tempfile::TempDir, Library) {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("in.jwpub");
    fixture.write_to(&file);
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    lib.import(&file).unwrap();
    (tmp, lib)
}

fn bible_library() -> (tempfile::TempDir, Library) {
    library(Fixture {
        bible_chapters: vec![(
            1,
            1,
            r#"<p><span id="v1-1-1-1" class="v">In the beginning</span> <span id="v1-1-2-1" class="v">Second</span></p>"#.into(),
        )],
        ..Fixture::default()
    })
}

fn document_library() -> (tempfile::TempDir, Library) {
    library(Fixture {
        documents: vec![(5, r#"<p data-pid="3">Para three</p>"#.into())],
        ..Fixture::default()
    })
}

fn loc(key: &str, document_id: Option<i64>, book: Option<i64>, chapter: Option<i64>) -> Loc {
    Loc {
        key_symbol: key.into(),
        meps_language: 0,
        issue_tag: 0,
        document_id,
        book,
        chapter,
    }
}

fn anchor(reference: Option<&str>, quote: Option<&str>) -> Anchor {
    Anchor {
        reference: reference.map(str::to_string),
        quote: quote.map(str::to_string),
    }
}

#[test]
fn resolves_a_verse() {
    let (_tmp, lib) = bible_library();
    let l = loc("tst", None, Some(1), Some(1));
    assert_eq!(
        resolve(&lib, Some(&l), None, BLOCK_VERSE, Some(2)).unwrap(),
        anchor(Some("Genesis 1:2"), Some("Second"))
    );
}

#[test]
fn resolves_a_whole_chapter() {
    let (_tmp, lib) = bible_library();
    let l = loc("tst", None, Some(1), Some(1));
    assert_eq!(
        resolve(&lib, Some(&l), None, 0, None).unwrap(),
        anchor(Some("Genesis 1"), None)
    );
}

#[test]
fn resolves_a_paragraph() {
    let (_tmp, lib) = document_library();
    let l = loc("tst", Some(5), None, None);
    assert_eq!(
        resolve(&lib, Some(&l), None, BLOCK_PARAGRAPH, Some(3)).unwrap(),
        anchor(Some("Doc — Test Publication"), Some("Para three"))
    );
}

#[test]
fn missing_paragraph_keeps_the_reference() {
    let (_tmp, lib) = document_library();
    let l = loc("tst", Some(5), None, None);
    assert_eq!(
        resolve(&lib, Some(&l), None, BLOCK_PARAGRAPH, Some(9)).unwrap(),
        anchor(Some("Doc — Test Publication"), None)
    );
}

#[test]
fn not_downloaded_falls_back_to_the_stored_title() {
    let (_tmp, lib) = document_library();
    let l = loc("zzz", Some(5), None, None);
    assert_eq!(
        resolve(
            &lib,
            Some(&l),
            Some("Stored title"),
            BLOCK_PARAGRAPH,
            Some(3)
        )
        .unwrap(),
        anchor(Some("Stored title"), None)
    );
    assert_eq!(
        resolve(&lib, Some(&l), None, BLOCK_PARAGRAPH, Some(3)).unwrap(),
        anchor(Some("zzz"), None)
    );
    let bible = loc("zzz", None, Some(43), Some(3));
    assert_eq!(
        resolve(&lib, Some(&bible), Some(""), BLOCK_VERSE, Some(16)).unwrap(),
        anchor(Some("zzz 43:3"), None)
    );
}

#[test]
fn missing_document_falls_back() {
    let (_tmp, lib) = document_library();
    let l = loc("tst", Some(77), None, None);
    assert_eq!(
        resolve(
            &lib,
            Some(&l),
            Some("Stored title"),
            BLOCK_PARAGRAPH,
            Some(3)
        )
        .unwrap(),
        anchor(Some("Stored title"), None)
    );
}

#[test]
fn no_location_resolves_to_nothing() {
    let (_tmp, lib) = document_library();
    assert_eq!(
        resolve(&lib, None, Some("x"), BLOCK_PARAGRAPH, Some(3)).unwrap(),
        Anchor::default()
    );
}

#[test]
fn verse_quote_leaves_out_markers_and_number() {
    let (_tmp, lib) = library(Fixture {
        bible_chapters: vec![(
            1,
            1,
            r#"<p><span id="v1-1-16-1" class="v"><span data-vlid="16" class="vl">16 <span class="tt vl"></span></span>His Son<span data-mid="97" class="m">t<span class="tt m"></span></span> given<span data-fnid="9" class="fn">a<span class="tt fn"></span></span>, life.<span data-mid="98" class="m">u</span> </span></p>"#.into(),
        )],
        ..Fixture::default()
    });
    let l = loc("tst", None, Some(1), Some(1));
    assert_eq!(
        resolve(&lib, Some(&l), None, BLOCK_VERSE, Some(16)).unwrap(),
        anchor(Some("Genesis 1:16"), Some("His Son given, life."))
    );
}

#[test]
fn note_body_quotes_the_anchor() {
    let (_tmp, lib) = bible_library();
    let note = pergament::userdata::Note {
        guid: "g".into(),
        title: "T".into(),
        content: "text".into(),
        block_type: BLOCK_VERSE,
        block_identifier: Some(2),
        mark_guid: None,
        color: None,
        tags: vec![],
        last_modified: String::new(),
        created: String::new(),
        location: Some(pergament::userdata::NoteLocation {
            loc: loc("tst", None, Some(1), Some(1)),
            title: None,
        }),
    };
    let (text, anchor) = pergament::note_markdown::note_body(&lib, &note).unwrap();
    assert_eq!(anchor.quote.as_deref(), Some("Second"));
    assert_eq!(text, "**T**\n\n> Second\n> — Genesis 1:2\n\ntext\n");
}
