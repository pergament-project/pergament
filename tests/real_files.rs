//! Tests against user-supplied publications. Skipped unless
//! `PERGAMENT_TEST_JWPUB` (study Bible) / `PERGAMENT_TEST_JWPUB_WP` (Watchtower)
//! / `PERGAMENT_TEST_JWPUB_LFF` (lff book) / `PERGAMENT_TEST_JWLPLAYLIST`
//! (JW Library playlist, `testdata/Songs.jwlplaylist`) are set.

use std::path::PathBuf;

use pergament::Library;
use pergament::crypto::ContentKey;
use pergament::jwpub;

fn fixture(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            eprintln!("skipping: {var} not set");
            None
        }
    }
}

/// Import, then decode every Document and compare with ContentLength.
fn import_and_decode_all(path: PathBuf) -> (pergament::Entry, rusqlite::Connection, ContentKey) {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let entry = lib.import(&path).unwrap();
    // Keep the DB outside the temp library so the connection outlives it.
    let db_copy = std::env::temp_dir().join(format!(
        "pergament-test-{}-{}",
        std::process::id(),
        entry.db_file
    ));
    std::fs::copy(lib.db_path(&entry), &db_copy).unwrap();
    let conn = jwpub::open_db(&db_copy).unwrap();
    std::fs::remove_file(&db_copy).ok();

    let key = ContentKey::new(
        entry.meps_language,
        &entry.symbol,
        entry.year,
        &entry.issue_tag,
    );
    let mut stmt = conn
        .prepare(
            "SELECT DocumentId, Content, ContentLength FROM Document WHERE Content IS NOT NULL",
        )
        .unwrap();
    let mut n = 0;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .unwrap();
    for row in rows {
        let (id, blob, len) = row.unwrap();
        let html = key
            .decode(&blob)
            .unwrap_or_else(|e| panic!("doc {id}: {e}"));
        assert_eq!(html.len() as i64, len, "doc {id} length");
        n += 1;
    }
    assert!(n > 0);
    drop(stmt);
    (entry, conn, key)
}

#[test]
fn study_bible() {
    let Some(path) = fixture("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let (entry, conn, key) = import_and_decode_all(path);
    assert_eq!(entry.symbol, "nwtsty");
    assert_eq!(entry.lang_code.as_deref(), Some("X"));
    assert_eq!(entry.issue_tag, "0");

    let verses: i64 = conn
        .query_row("SELECT count(*) FROM BibleVerse", [], |r| r.get(0))
        .unwrap();
    assert_eq!(verses, 31194);
    // Gen 1:2 is BibleVerseId 1; its decoded span carries the verse id.
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT Content FROM BibleVerse WHERE BibleVerseId = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(key.decode(&blob).unwrap().contains(r#"id="v1-1-2""#));
}

#[test]
fn watchtower() {
    let Some(path) = fixture("PERGAMENT_TEST_JWPUB_WP") else {
        return;
    };
    let (entry, _conn, _key) = import_and_decode_all(path);
    assert_eq!(entry.symbol, "wp26");
    assert_eq!(entry.issue_tag, "20260900");
}

fn open_library(var: &str) -> Option<(tempfile::TempDir, Library, pergament::Entry)> {
    let path = fixture(var)?;
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let entry = lib.import(&path).unwrap();
    Some((tmp, lib, entry))
}

/// `verse_ref` must agree with the `id="v{book}-{ch}-{verse}[-part]"` span in
/// every decoded verse.
#[test]
fn study_bible_verse_refs() {
    let Some((_tmp, lib, entry)) = open_library("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let p = pergament::Publication::open(&lib, &entry).unwrap();
    let conn = jwpub::open_db(&lib.db_path(&entry)).unwrap();
    let mut stmt = conn
        .prepare("SELECT BibleVerseId, Content FROM BibleVerse")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
        .unwrap();
    for row in rows {
        let (id, blob) = row.unwrap();
        let html = p.decode(&blob).unwrap();
        let r = p.verse_ref(id).unwrap();
        let expected = format!("id=\"v{}-{}-{}", r.book, r.chapter, r.verse);
        assert!(
            html.contains(&format!("{expected}\"")) || html.contains(&format!("{expected}-")),
            "verse {id}: {expected} not in {html}"
        );
    }
}

#[test]
fn study_bible_renders_chapters() {
    let Some((_tmp, lib, entry)) = open_library("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let p = pergament::Publication::open(&lib, &entry).unwrap();
    let r = pergament::Renderer::new(&p, pergament::RenderOptions::default());
    // Genesis 1 has footnotes and cross references, Matthew 1 study notes.
    let gen1 = r.chapter(1, 1).unwrap();
    assert!(gen1.contains("class=\"footnotes\""));
    assert!(gen1.contains("class=\"xrefs\""));
    assert!(r.chapter(40, 1).unwrap().contains("class=\"study-notes\""));
    for book in p.bible_books().unwrap() {
        for ch in 1..=book.chapters {
            let html = r.chapter(book.number, ch).unwrap();
            assert!(!html.contains("jwpub:"), "{} {ch}", book.number);
            assert!(!html.contains("data-"), "{} {ch}", book.number);
        }
    }
}

#[test]
fn all_documents_render() {
    for var in ["PERGAMENT_TEST_JWPUB", "PERGAMENT_TEST_JWPUB_WP"] {
        let Some((_tmp, lib, entry)) = open_library(var) else {
            continue;
        };
        let p = pergament::Publication::open(&lib, &entry).unwrap();
        let r = pergament::Renderer::new(
            &p,
            pergament::RenderOptions {
                media_base: Some("file:///m/".into()),
                standalone: true,
            },
        );
        for d in p.documents().unwrap().into_iter().filter(|d| d.has_content) {
            let html = r.document(d.id).unwrap();
            assert!(!html.contains("jwpub"), "{var} doc {}", d.id);
        }
    }
}

/// Live download from jw.org. Opt-in only: set PERGAMENT_TEST_NETWORK=1.
#[test]
fn live_download_matches_fixture() {
    if std::env::var_os("PERGAMENT_TEST_NETWORK").is_none() {
        eprintln!("skipping: PERGAMENT_TEST_NETWORK not set");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    let client = pergament::net::Client::new(pergament::net::HttpConfig::default());
    let req = pergament::remote::Request {
        key_symbol: "wp",
        lang_code: "X",
        issue_tag: Some(20260900),
    };
    let entry = pergament::remote::download(
        &client,
        &mut lib,
        &tmp.path().join("dl"),
        &req,
        None,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(entry.symbol, "wp26");
    assert_eq!(entry.issue_tag, "20260900");
    assert_eq!(entry.meps_language, 2);
}

/// Bible links from an article open the imported Bible; publication links
/// resolve by MEPS document id.
#[test]
fn navigation_across_publications() {
    use pergament::links::Link;
    use pergament::navigate::{self, TargetKind};
    let (Some(bible), Some(wp)) = (
        fixture("PERGAMENT_TEST_JWPUB"),
        fixture("PERGAMENT_TEST_JWPUB_WP"),
    ) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let nwtsty = lib.import(&bible).unwrap();
    let wp = lib.import(&wp).unwrap();

    let link = Link::parse("pergament://bible/19:23:1-19:23:3").unwrap();
    let t = navigate::resolve(&lib, Some(&wp), &link).unwrap().unwrap();
    assert_eq!(t.publication, nwtsty.dir_name);
    assert_eq!(
        t.kind,
        TargetKind::Chapter {
            book: 19,
            chapter: 23,
            verse: 1
        }
    );
    let page = navigate::page(&lib, &t).unwrap();
    assert!(page.html.contains("v19-23-1"));

    // MEPS document 2026003 is the article "Kann eine bessere Politik …".
    let link = Link::parse("pergament://pub/X:2026003/").unwrap();
    let t = navigate::resolve(&lib, Some(&nwtsty), &link)
        .unwrap()
        .unwrap();
    assert_eq!(t.publication, wp.dir_name);
    assert_eq!(t.kind, TargetKind::Document(3));

    let link = Link::parse("pergament://pub/X:1/").unwrap();
    assert!(navigate::resolve(&lib, Some(&wp), &link).unwrap().is_none());

    // Verse > 1 scrolls to the verse span.
    let t = navigate::Target {
        publication: nwtsty.dir_name.clone(),
        kind: TargetKind::Chapter {
            book: 19,
            chapter: 23,
            verse: 4,
        },
    };
    assert_eq!(
        navigate::page(&lib, &t).unwrap().fragment.as_deref(),
        Some("v19-23-4-1")
    );
}

#[test]
fn toc_and_covers() {
    let (Some(bible), Some(wp)) = (
        fixture("PERGAMENT_TEST_JWPUB"),
        fixture("PERGAMENT_TEST_JWPUB_WP"),
    ) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let nwtsty_entry = lib.import(&bible).unwrap();
    let nwtsty = pergament::Publication::open(&lib, &nwtsty_entry).unwrap();
    let toc = nwtsty.toc().unwrap();
    // The study Bible's navigation view: EINFÜHRUNG, BÜCHER, INDEX, ANHANG A-C.
    assert_eq!(toc.len(), 6);
    let books = &toc[1];
    assert_eq!(books.children.len(), 2);
    let numbers: Vec<i64> = books
        .children
        .iter()
        .flat_map(|t| &t.children)
        .map(|b| b.bible_book.unwrap())
        .collect();
    assert_eq!(numbers, (1..=66).collect::<Vec<_>>());
    assert_eq!(books.children[0].children.len(), 39);
    assert!(nwtsty.cover_image().unwrap().unwrap().ends_with("_cvr.jpg"));

    let wp_entry = lib.import(&wp).unwrap();
    let wp = pergament::Publication::open(&lib, &wp_entry).unwrap();
    let toc = wp.toc().unwrap();
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].children.len(), 8);
    assert_eq!(toc[0].children[3].document_id, Some(3));
    assert!(toc[0].children.iter().all(|c| c.bible_book.is_none()));
    assert_eq!(
        wp.cover_image().unwrap().as_deref(),
        Some("2026000_X_cvr.jpg")
    );
}

/// Study-note links (`jwpub://c/`) and links to Bible book documents.
#[test]
fn study_note_and_book_links() {
    use pergament::links::Link;
    use pergament::navigate::{self, TargetKind};
    let Some(bible) = fixture("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let nwtsty = lib.import(&bible).unwrap();

    // Rendered study notes link to other notes via pergament://note/.
    let html = navigate::page(
        &lib,
        &navigate::Target {
            publication: nwtsty.dir_name.clone(),
            kind: TargetKind::Chapter {
                book: 40,
                chapter: 16,
                verse: 1,
            },
        },
    )
    .unwrap()
    .html;
    assert!(html.contains("pergament://note/X:1001070145/8:38"));
    assert!(!html.contains("jwpub:"));

    // MEPS 1001070145 is the book document of Mark (41).
    let link = Link::parse("pergament://note/X:1001070145/8:38").unwrap();
    let t = navigate::resolve(&lib, Some(&nwtsty), &link)
        .unwrap()
        .unwrap();
    assert_eq!(
        t.kind,
        TargetKind::Chapter {
            book: 41,
            chapter: 8,
            verse: 38
        }
    );

    // MEPS 1001070144 is Matthew's book document: open chapter 1.
    let link = Link::parse("pergament://pub/X:1001070144/1-1").unwrap();
    let t = navigate::resolve(&lib, Some(&nwtsty), &link)
        .unwrap()
        .unwrap();
    assert_eq!(
        t.kind,
        TargetKind::Chapter {
            book: 40,
            chapter: 1,
            verse: 1
        }
    );
}

/// The study pane for Genesis 12, checked against the original app.
#[test]
fn chapter_study_genesis_12() {
    let Some(bible) = fixture("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let entry = lib.import(&bible).unwrap();
    let p = pergament::Publication::open(&lib, &entry).unwrap();

    let abbr = p.book_abbreviations().unwrap();
    assert_eq!(abbr.len(), 66);
    assert_eq!(abbr[&1], "1Mo");
    assert_eq!(abbr[&44], "Apg");
    assert_eq!(abbr[&66], "Off");

    let r = pergament::Renderer::new(&p, pergament::RenderOptions::default());
    let s = r.chapter_study(1, 12).unwrap();
    assert_eq!(s.outline_title.as_deref(), Some("1. Mose: Übersicht"));
    let lines: Vec<_> = s.outline.iter().map(|o| o.text.as_str()).collect();
    assert_eq!(
        lines,
        [
            "Abram zieht von Haran nach Kanaan",
            "Gott gibt Abram Versprechen",
            "Abram und Sarai in Ägypten"
        ]
    );
    let v1 = &s.verses[0];
    assert_eq!((v1.chapter, v1.verse), (12, 1));
    assert_eq!(v1.footnotes.len(), 1);
    assert!(v1.footnotes[0].html.contains("Familie"));
    let refs: Vec<_> = v1.xrefs[0].refs.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(refs, ["Jos 24:3", "Apg 7:3, 4"]);
    // Matthew has study notes.
    assert!(
        r.chapter_study(40, 1)
            .unwrap()
            .verses
            .iter()
            .any(|v| !v.notes.is_empty())
    );

    // The parallel accounts of Matthew 4:1 are marked, the other references are not.
    let m4 = r.chapter_study(40, 4).unwrap();
    let v1 = m4.verses.iter().find(|v| v.verse == 1).unwrap();
    let parallel: Vec<_> = v1.xrefs[0]
        .refs
        .iter()
        .filter(|r| r.parallel)
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(parallel, ["Mar 1:12, 13", "Luk 4:1-4"]);
    assert!(v1.xrefs[0].refs.iter().any(|r| !r.parallel));

    // A book's overview is a document of its own, and so is its intro video.
    let o = r.book_overview(40, Some("X")).unwrap();
    let video = o.video.as_ref().unwrap();
    assert_eq!((video.pub_symbol.as_str(), video.track), ("nwtsv", 400));
    let doc = r.document(o.document.unwrap()).unwrap();
    assert!(doc.contains("Übersicht über Matthäus"));
    assert!(doc.contains("<strong>A.</strong>"));
}

/// Every citation link in a real file resolves: each `Extract.Link` finds its
/// own excerpt (self-consistency), and the `jwpub://p/…` anchors of the
/// documents resolve to `Extract` rows more often than not.
#[test]
fn extracts_resolve_on_real_files() {
    let Some((_tmp, lib, entry)) = open_library("PERGAMENT_TEST_JWPUB_LFF") else {
        return;
    };
    let p = pergament::Publication::open(&lib, &entry).unwrap();
    let conn = jwpub::open_db(&lib.db_path(&entry)).unwrap();

    // Self-consistency: every Extract.Link with content matches exactly one
    // excerpt.
    let mut stmt = conn
        .prepare("SELECT DISTINCT Link FROM Extract WHERE Link IS NOT NULL AND Content IS NOT NULL")
        .unwrap();
    let links: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(stmt);
    assert!(!links.is_empty(), "no Extract rows with content");
    for link in &links {
        assert!(p.citation_extract(link).unwrap().is_some(), "{link}");
    }

    // The citation anchors of the documents correspond to Extract.Link rows
    // in pub form: "p/" + the tail of jwpub://p/….
    let marker = "href=\"jwpub://p/";
    let (mut some, mut none) = (0, 0);
    for d in p.documents().unwrap().into_iter().filter(|d| d.has_content) {
        let html = p.document(d.id).unwrap().1.unwrap();
        let mut rest = html.as_str();
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let end = rest.find('"').unwrap();
            let link = format!("p/{}", &rest[..end]);
            rest = &rest[end..];
            if p.citation_extract(&link).unwrap().is_some() {
                some += 1;
            } else {
                none += 1;
            }
        }
    }
    assert!(
        some > none,
        "of {} anchors, {some} resolved and {none} did not",
        some + none
    );
}

/// The crate's NWT verse table must match the installed study Bible.
#[test]
fn verse_table_matches_the_study_bible() {
    let Some((_tmp, lib, entry)) = open_library("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let p = pergament::Publication::open(&lib, &entry).unwrap();
    for (b, chapters) in pergament::reading_plans::VERSES.iter().enumerate() {
        let book = b as i64 + 1;
        assert_eq!(
            p.bible_book(book).unwrap().chapters,
            chapters.len() as i64,
            "book {book}"
        );
        for (c, &last) in chapters.iter().enumerate() {
            let chapter = c as i64 + 1;
            assert_eq!(
                p.last_verse(book, chapter).unwrap(),
                i64::from(last),
                "{book}:{chapter}"
            );
        }
    }
}

/// A note on John 3:16 quotes the verse without its number.
#[test]
fn note_anchor_resolves_john_3_16() {
    let Some((_tmp, lib, entry)) = open_library("PERGAMENT_TEST_JWPUB") else {
        return;
    };
    let p = pergament::Publication::open(&lib, &entry).unwrap();
    let loc = pergament::userdata::Loc {
        key_symbol: p.key_symbol().unwrap(),
        meps_language: entry.meps_language,
        issue_tag: 0,
        document_id: None,
        book: Some(43),
        chapter: Some(3),
    };
    let anchor = pergament::note_markdown::resolve(
        &lib,
        Some(&loc),
        None,
        pergament::userdata::BLOCK_VERSE,
        Some(16),
    )
    .unwrap();
    let quote = anchor.quote.unwrap();
    assert!(!quote.is_empty());
    assert!(!quote.starts_with("16"), "{quote}");
    // A footnote or cross-reference letter would trail the final punctuation.
    assert!(quote.ends_with(['.', '!', '?', '"', '”', '“', '»', '«']), "{quote}");
    let reference = anchor.reference.unwrap();
    assert!(reference.ends_with("3:16"), "{reference}");
}

/// The values of one imported playlist item, read from the database:
/// label, accuracy, end action, trims, location (key symbol, track, type,
/// language, issue), multimedia type, base duration, media (path, mime,
/// original name, hash, duration) and thumbnail path.
fn playlist_rows(ud: &pergament::userdata::UserData, tag: i64) -> Vec<Vec<rusqlite::types::Value>> {
    let conn = rusqlite::Connection::open(ud.path()).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT i.Label, i.Accuracy, i.EndAction, i.StartTrimOffsetTicks,
                    i.EndTrimOffsetTicks, l.KeySymbol, l.Track, l.Type, l.MepsLanguage,
                    l.IssueTagNumber, lm.MajorMultimediaType, lm.BaseDurationTicks,
                    m.FilePath, m.MimeType, m.OriginalFilename, m.Hash, im.DurationTicks,
                    i.ThumbnailFilePath
             FROM TagMap t JOIN PlaylistItem i ON i.PlaylistItemId = t.PlaylistItemId
             LEFT JOIN PlaylistItemLocationMap lm ON lm.PlaylistItemId = i.PlaylistItemId
             LEFT JOIN Location l ON l.LocationId = lm.LocationId
             LEFT JOIN PlaylistItemIndependentMediaMap im ON im.PlaylistItemId = i.PlaylistItemId
             LEFT JOIN IndependentMedia m ON m.IndependentMediaId = im.IndependentMediaId
             WHERE t.TagId = ?1 ORDER BY t.Position",
        )
        .unwrap();
    stmt.query_map([tag], |r| (0..18).map(|i| r.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// The JW Library sample playlist from an iPhone imports with every value,
/// file and thumbnail, and survives an export and a second import.
#[test]
fn imports_jw_library_playlist() {
    use pergament::playlist::{ItemKind, ItemSource};
    use rusqlite::types::Value::{Integer as I, Null, Text as T};

    let Some(path) = fixture("PERGAMENT_TEST_JWLPLAYLIST") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut ud =
        pergament::userdata::UserData::open(tmp.path().join("a").join("userData.db")).unwrap();
    let id = ud.import_playlist(&path).unwrap();
    let playlist = ud.playlist(id).unwrap();
    assert_eq!(playlist.name, "Songs");
    let labels: Vec<&str> = playlist.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "1. Jehovas großartige Eigenschaften",
            "JW Broadcasting November 2026",
            "IMG_3019.png",
            "IMG_2974.mov"
        ]
    );
    let kinds: Vec<ItemKind> = playlist.items.iter().map(|i| i.kind).collect();
    assert_eq!(
        kinds,
        [
            ItemKind::Audio,
            ItemKind::Video,
            ItemKind::Image,
            ItemKind::Video
        ]
    );
    let durations: Vec<i64> = playlist.items.iter().map(|i| i.duration_ticks).collect();
    assert_eq!(
        durations,
        [1_378_840_000, 36_009_720_000, 40_000_000, 139_000_000]
    );
    for item in &playlist.items {
        assert!(
            item.thumbnail.as_ref().is_some_and(|t| t.is_file()),
            "{}",
            item.label
        );
        if let ItemSource::File { available, .. } = &item.source {
            assert!(available, "{}", item.label);
        }
    }

    let s = |v: &str| T(v.into());
    let rows = playlist_rows(&ud, id);
    let common = |label: &str| vec![s(label), I(1), I(0), Null, Null];
    let expected: [Vec<rusqlite::types::Value>; 4] = [
        [
            &common("1. Jehovas großartige Eigenschaften")[..],
            &[s("sjjm"), I(1), I(2), I(2), I(0), I(0), I(1_378_840_000)],
            &[Null, Null, Null, Null, Null],
            &[s("06D2E671-01A0-49C8-9642-DCFA28BF20E7.jpeg")],
        ]
        .concat(),
        [
            &common("JW Broadcasting November 2026")[..],
            &[
                s("jwb-143"),
                I(1),
                I(3),
                I(2),
                I(0),
                I(2),
                I(36_009_720_000),
            ],
            &[Null, Null, Null, Null, Null],
            &[s("4782EE3E-0366-4379-AF89-F9AC49C5C6BE.jpeg")],
        ]
        .concat(),
        [
            &common("IMG_3019.png")[..],
            &[Null, Null, Null, Null, Null, Null, Null],
            &[
                s("D5D214AC-7584-4E32-BB72-D2AB75AFED6E.png"),
                s("image/png"),
                s("41B9A189-A440-47CB-865E-4B83A420D92B.png"),
                s("c924d6669f8afa33c7e9925944f1365f7de94785621fa6c67e4bc4c78a14437"),
                I(40_000_000),
            ],
            &[s("2C4646A7-18DA-4BC3-A186-33A129645B4E.jpeg")],
        ]
        .concat(),
        [
            &common("IMG_2974.mov")[..],
            &[Null, Null, Null, Null, Null, Null, Null],
            &[
                s("EDB6A882-2CF0-482C-A3E1-4771B71491FC.mov"),
                s("video/quicktime"),
                s("3646BE69-E18A-4F83-937A-7F7E6391EA9B.mov"),
                s("9aae6f6a697ccff8983b964d1f53d3275cd992a6f8fe30a24d702d5728877e"),
                I(139_000_000),
            ],
            &[s("A1B0B64C-20FA-4163-A2A5-3F9E41842C05.jpeg")],
        ]
        .concat(),
    ];
    assert_eq!(rows, expected);

    // The video is copied whole.
    let video = "EDB6A882-2CF0-482C-A3E1-4771B71491FC.mov";
    let copied = std::fs::read(ud.playlist_media_dir().join(video)).unwrap();
    assert!(copied.len() as u64 <= 78_602_157);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut entry = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name(video).unwrap(), &mut entry).unwrap();
    assert_eq!(copied, entry);

    // Export and import again: the same playlist.
    let exported = tmp.path().join("Songs.jwlplaylist");
    ud.export_playlist(id, &exported).unwrap();
    let mut other =
        pergament::userdata::UserData::open(tmp.path().join("b").join("userData.db")).unwrap();
    let again = other.import_playlist(&exported).unwrap();
    assert_eq!(playlist_rows(&other, again), rows);
    assert_eq!(
        std::fs::read(other.playlist_media_dir().join(video)).unwrap(),
        copied
    );
}
