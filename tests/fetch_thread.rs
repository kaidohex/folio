use folio::feed::spawn_fetch_all;
use folio::store::Store;

#[test]
fn spawn_fetch_updates_health() {
    let dir = std::env::temp_dir().join("folio-thread-test");
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::open(&dir).unwrap();
    store.add_feed("https://hnrss.org/frontpage", "hn", Some("Tech"));
    let feeds = store.feeds();
    // The network can be flaky; retry up to three rounds.
    for _ in 0..3 {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = spawn_fetch_all(dir.clone(), feeds.clone(), tx);
        handle.join().unwrap();
        let _ = rx.recv().unwrap();
        if store.feeds()[0].last_success.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    let feeds = store.feeds();
    assert!(
        feeds[0].last_success.is_some(),
        "last_success should be set after retries, error={:?}",
        feeds[0].last_error
    );
    let stories = store.stories(None, false, 300);
    assert!(!stories.is_empty(), "should have stories");
}
