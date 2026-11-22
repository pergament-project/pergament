mod common;

use common::Fixture;
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
