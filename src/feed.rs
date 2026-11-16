//! Fetching and parsing RSS/Atom feeds. Runs on background threads; the UI
//! polls results with egui's `request_repaint`.

use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use feed_rs::model::Feed as ParsedFeed;

use crate::store::{NewStory, Store};

pub struct FetchOutcome {
    pub feed_id: i64,
    pub ok: bool,
    pub new_stories: usize,
    pub error: Option<String>,
}

pub struct FetchResult {
    pub outcomes: Vec<FetchOutcome>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Spawn all enabled feeds on a thread pool; deliver one aggregated result.
pub fn spawn_fetch_all(
    data_dir: std::path::PathBuf,
    feeds: Vec<crate::model::Feed>,
    tx: std::sync::mpsc::Sender<FetchResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let client = build_client();
        let mut outcomes = Vec::new();
        let max_parallel = 6;
        for chunk in feeds.chunks(max_parallel) {
            let handles: Vec<_> = chunk
                .iter()
                .cloned()
                .map(|feed| {
                    let client = client.clone();
                    let data_dir = data_dir.clone();
                    std::thread::spawn(move || {
                        let store = Store::open(&data_dir).ok();
                        match store {
                            Some(store) => fetch_one(&store, &client, feed),
                            None => FetchOutcome {
                                feed_id: feed.id,
                                ok: false,
                                new_stories: 0,
                                error: Some("Could not open store".into()),
                            },
                        }
                    })
                })
                .collect();
            for h in handles {
                if let Ok(outcome) = h.join() {
                    outcomes.push(outcome);
                }
            }
        }
        let _ = tx.send(FetchResult { outcomes });
    })
}

fn build_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Folio/0.1; RSS reader)")
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .expect("http client")
}

fn fetch_one(
    store: &Store,
    client: &reqwest::blocking::Client,
    feed: crate::model::Feed,
) -> FetchOutcome {
    let started = now_ms();
    match fetch_and_parse(client, &feed.url) {
        Ok(parsed) => {
            let mut new_stories = 0usize;
            for entry in &parsed.entries {
                let story = new_story_from_entry(&feed, entry);
                if store.insert_story(&story, started) {
                    new_stories += 1;
                }
            }
            let title = feed_title(&feed, &parsed);
            if !title.is_empty() {
                rename_feed(store, feed.id, &title);
            }
            store.record_health(feed.id, true, None, started);
            store.prune_cache(600);
            FetchOutcome {
                feed_id: feed.id,
                ok: true,
                new_stories,
                error: None,
            }
        }
        Err(err) => {
            let message = friendly_error(&err);
            store.record_health(feed.id, false, Some(&message), started);
            FetchOutcome {
                feed_id: feed.id,
                ok: false,
                new_stories: 0,
                error: Some(message),
            }
        }
    }
}

pub fn fetch_and_parse_debug(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<ParsedFeed, String> {
    fetch_and_parse(client, url)
}

fn fetch_and_parse(client: &reqwest::blocking::Client, url: &str) -> Result<ParsedFeed, String> {
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    let text = std::string::String::from_utf8_lossy(&bytes);
    feed_rs::parser::parse(text.as_bytes()).map_err(|e| {
        if content_type.contains("html") {
            format!("got HTML, not a feed ({e})")
        } else {
            format!("not a valid feed ({e})")
        }
    })
}

fn friendly_error(err: &str) -> String {
    if err.contains("timed out") || err.contains("timeout") {
        "Timed out".into()
    } else if err.contains("dns") || err.contains("resolve") {
        "Could not resolve host".into()
    } else if err.contains("connection") {
        "Connection failed".into()
    } else {
        err.chars().take(120).collect()
    }
}

fn feed_title(feed: &crate::model::Feed, parsed: &ParsedFeed) -> String {
    parsed
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| feed.title.clone())
}

fn rename_feed(store: &Store, id: i64, title: &str) {
    store.rename_feed(id, title);
}

fn new_story_from_entry(feed: &crate::model::Feed, entry: &feed_rs::model::Entry) -> NewStory {
    let url: String = entry
        .links
        .first()
        .map(|l| l.href.clone())
        .unwrap_or_else(|| entry.id.clone());
    let title = entry
        .title
        .as_ref()
        .map(|t| strip_html(&t.content))
        .unwrap_or_else(|| "(untitled)".into());
    let excerpt = entry
        .summary
        .as_ref()
        .map(|t| strip_html(&t.content))
        .or_else(|| {
            entry
                .content
                .as_ref()
                .map(|c| strip_html(&c.body.clone().unwrap_or_default()))
        })
        .unwrap_or_default();
    let excerpt = excerpt
        .lines()
        .filter(|l| !l.trim().starts_with("http"))
        .collect::<Vec<_>>()
        .join(" ");
    let excerpt = truncate(&clean_whitespace(&excerpt), 280);
    let published = entry
        .published
        .or_else(|| entry.updated)
        .map(|d| d.timestamp_millis());
    NewStory {
        feed_id: feed.id,
        guid: entry.id.clone(),
        url,
        title,
        excerpt,
        published,
    }
}

fn clean_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Very small HTML tag stripper for titles/excerpts; no entity decoding beyond
/// the few that matter.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            c if !inside_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Join on a receiver that arrives from spawned threads, giving the UI a way to
/// wait without blocking the frame loop (call each frame; never blocks long).
pub fn poll_result(rx: &Receiver<FetchResult>) -> Option<FetchResult> {
    rx.try_recv().ok()
}
