use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Feed {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub category: Option<String>,
    pub enabled: bool,
    /// Last successful fetch (ms since epoch), if any.
    pub last_success: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Story {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub guid: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    pub published: Option<i64>,
    pub read: bool,
    pub saved: bool,
}

impl Story {
    pub fn published_or_fetched(&self, fetched_ms: i64) -> i64 {
        self.published.unwrap_or(fetched_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    All,
    Saved,
    Feed(i64),
    Search,
}

#[derive(Debug, Clone)]
pub struct HealthSummary {
    pub feed_id: i64,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}
