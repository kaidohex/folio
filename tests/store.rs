use folio::store::{NewStory, Store};

fn temp_store(name: &str) -> Store {
    let dir = std::env::temp_dir().join(format!("folio-unit-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    Store::open(&dir).unwrap()
}

#[test]
fn add_feed_is_idempotent() {
    let s = temp_store("add");
    assert!(
        s.add_feed("https://a.example/feed", "A", Some("T"))
            .is_some()
    );
    assert!(
        s.add_feed("https://a.example/feed", "A", Some("T"))
            .is_none()
    );
    assert_eq!(s.feeds().len(), 1);
}

#[test]
fn stories_round_trip_and_saved_filter() {
    let s = temp_store("stories");
    let id = s.add_feed("https://a.example/feed", "A", None).unwrap();
    s.insert_story(
        &NewStory {
            feed_id: id,
            guid: "g1".into(),
            url: "https://a.example/1".into(),
            title: "Apollo story".into(),
            excerpt: "about space".into(),
            published: Some(1_700_000_000_000),
        },
        1,
    );
    assert_eq!(s.stories(None, false, 100).len(), 1);
    assert_eq!(s.search("apollo", 10).len(), 1);
    assert_eq!(s.search("nope", 10).len(), 0);
    assert_eq!(s.saved_count(), 0);
    let story = s.stories(None, false, 1).remove(0);
    s.set_saved(story.id, true);
    assert_eq!(s.saved_count(), 1);
    assert_eq!(s.stories(None, true, 100).len(), 1);
    assert_eq!(s.stories(None, false, 100).len(), 1);
}

#[test]
fn duplicate_guid_is_ignored() {
    let s = temp_store("dup");
    let id = s.add_feed("https://a.example/feed", "A", None).unwrap();
    let mk = || NewStory {
        feed_id: id,
        guid: "g1".into(),
        url: "https://a.example/1".into(),
        title: "t".into(),
        excerpt: String::new(),
        published: None,
    };
    assert!(s.insert_story(&mk(), 1));
    assert!(!s.insert_story(&mk(), 2));
    assert_eq!(s.stories(None, false, 100).len(), 1);
}

#[test]
fn remove_feed_cascades() {
    let s = temp_store("remove");
    let id = s.add_feed("https://a.example/feed", "A", None).unwrap();
    s.insert_story(
        &NewStory {
            feed_id: id,
            guid: "g".into(),
            url: "u".into(),
            title: "t".into(),
            excerpt: String::new(),
            published: None,
        },
        1,
    );
    s.remove_feed(id);
    assert!(s.feeds().is_empty());
    assert!(s.stories(None, false, 100).is_empty());
}

#[test]
fn prune_keeps_saved() {
    let s = temp_store("prune");
    let id = s.add_feed("https://a.example/feed", "A", None).unwrap();
    for g in 0..10 {
        s.insert_story(
            &NewStory {
                feed_id: id,
                guid: format!("g{g}"),
                url: format!("u{g}"),
                title: format!("t{g}"),
                excerpt: String::new(),
                published: None,
            },
            g,
        );
    }
    let oldest = s.stories(None, false, 100).pop().unwrap();
    s.set_saved(oldest.id, true);
    s.prune_cache(3);
    let remaining = s.stories(None, false, 100);
    assert_eq!(
        remaining.len(),
        4,
        "10 stories - 6 pruned + 1 saved survivor"
    );
    let saved = s.stories(None, true, 100);
    assert_eq!(saved.len(), 1, "saved story survives the prune");
    assert_eq!(saved[0].id, oldest.id);
}
