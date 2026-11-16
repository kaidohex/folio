use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Feed, Story};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(dir: &Path) -> rusqlite::Result<Store> {
        std::fs::create_dir_all(dir).ok();
        let conn = Connection::open(dir.join("folio.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS feeds (
                id INTEGER PRIMARY KEY,
                url TEXT UNIQUE NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                category TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_success INTEGER,
                last_error TEXT
            );
            CREATE TABLE IF NOT EXISTS stories (
                id INTEGER PRIMARY KEY,
                feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                guid TEXT NOT NULL,
                url TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                excerpt TEXT NOT NULL DEFAULT '',
                published INTEGER,
                fetched INTEGER NOT NULL DEFAULT 0,
                read INTEGER NOT NULL DEFAULT 0,
                saved INTEGER NOT NULL DEFAULT 0,
                UNIQUE(feed_id, guid)
            );
            CREATE INDEX IF NOT EXISTS idx_stories_fetched ON stories(fetched DESC);
            CREATE INDEX IF NOT EXISTS idx_stories_saved ON stories(saved) WHERE saved = 1;
            ",
        )?;
        Ok(Store { conn })
    }

    pub fn feeds(&self) -> Vec<Feed> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, title, category, enabled, last_success, last_error FROM feeds ORDER BY category, title")
            .expect("feeds query");
        stmt.query_map([], |r| {
            Ok(Feed {
                id: r.get(0)?,
                url: r.get(1)?,
                title: r.get(2)?,
                category: r.get(3)?,
                enabled: r.get::<_, i64>(4)? != 0,
                last_success: r.get(5)?,
                last_error: r.get(6)?,
            })
        })
        .expect("feeds map")
        .filter_map(Result::ok)
        .collect()
    }

    pub fn add_feed(&self, url: &str, title: &str, category: Option<&str>) -> Option<i64> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO feeds (url, title, category) VALUES (?1, ?2, ?3)",
                params![url, title, category],
            )
            .ok()
            .filter(|&n| n > 0)
            .map(|_| self.conn.last_insert_rowid())
    }

    pub fn remove_feed(&self, id: i64) {
        self.conn
            .execute("DELETE FROM stories WHERE feed_id = ?1", params![id])
            .ok();
        self.conn
            .execute("DELETE FROM feeds WHERE id = ?1", params![id])
            .ok();
    }

    pub fn set_feed_enabled(&self, id: i64, enabled: bool) {
        self.conn
            .execute(
                "UPDATE feeds SET enabled = ?2 WHERE id = ?1",
                params![id, enabled as i64],
            )
            .ok();
    }

    pub fn rename_feed(&self, id: i64, title: &str) {
        self.conn
            .execute(
                "UPDATE feeds SET title = ?2 WHERE id = ?1",
                params![id, title],
            )
            .ok();
    }

    pub fn record_health(&self, id: i64, ok: bool, message: Option<&str>, now_ms: i64) {
        if ok {
            self.conn
                .execute(
                    "UPDATE feeds SET last_success = ?2, last_error = NULL WHERE id = ?1",
                    params![id, now_ms],
                )
                .ok();
        } else {
            self.conn
                .execute(
                    "UPDATE feeds SET last_error = ?2 WHERE id = ?1",
                    params![id, message.unwrap_or("unknown error")],
                )
                .ok();
        }
    }

    /// Insert a story; returns true when it is new.
    pub fn insert_story(&self, s: &NewStory, now_ms: i64) -> bool {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO stories (feed_id, guid, url, title, excerpt, published, fetched)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![s.feed_id, s.guid, s.url, s.title, s.excerpt, s.published, now_ms],
            )
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    pub fn stories(&self, feed_id: Option<i64>, saved_only: bool, limit: u32) -> Vec<Story> {
        let (sql, bind_feed): (String, bool) = match (feed_id, saved_only) {
            (Some(_), true) => ("WHERE saved = 1 AND feed_id = ?1".into(), true),
            (Some(_), false) => ("WHERE feed_id = ?1".into(), true),
            (None, true) => ("WHERE saved = 1".into(), false),
            (None, false) => (String::new(), false),
        };
        let sql = format!(
            "SELECT s.id, s.feed_id, f.title, s.guid, s.url, s.title, s.excerpt, s.published, s.read, s.saved
             FROM stories s JOIN feeds f ON f.id = s.feed_id
             {sql} ORDER BY COALESCE(s.published, s.fetched) DESC LIMIT {limit}"
        );
        let mut stmt = self.conn.prepare(&sql).expect("stories query");
        let rows = if bind_feed {
            stmt.query_map(params![feed_id], story_row)
                .expect("stories map")
        } else {
            stmt.query_map([], story_row).expect("stories map")
        };
        let out: Vec<_> = rows.flatten().collect();
        out
    }

    pub fn search(&self, q: &str, limit: u32) -> Vec<Story> {
        let like = format!("%{}%", q.replace('%', "\\%"));
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.feed_id, f.title, s.guid, s.url, s.title, s.excerpt, s.published, s.read, s.saved
                 FROM stories s JOIN feeds f ON f.id = s.feed_id
                 WHERE s.title LIKE ?1 ESCAPE '\\' OR s.excerpt LIKE ?1 ESCAPE '\\' OR f.title LIKE ?1 ESCAPE '\\'
                 ORDER BY COALESCE(s.published, s.fetched) DESC LIMIT ?2",
            )
            .expect("search query");
        stmt.query_map(params![like, limit], story_row)
            .expect("search map")
            .filter_map(Result::ok)
            .collect()
    }

    pub fn saved_count(&self) -> i64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM stories WHERE saved = 1", [], |r| {
                r.get(0)
            })
            .unwrap_or(0)
    }

    pub fn set_read(&self, id: i64, read: bool) {
        self.conn
            .execute(
                "UPDATE stories SET read = ?2 WHERE id = ?1",
                params![id, read as i64],
            )
            .ok();
    }

    pub fn set_saved(&self, id: i64, saved: bool) {
        self.conn
            .execute(
                "UPDATE stories SET saved = ?2 WHERE id = ?1",
                params![id, saved as i64],
            )
            .ok();
    }

    pub fn story(&self, id: i64) -> Option<Story> {
        self.conn
            .query_row(
                "SELECT s.id, s.feed_id, f.title, s.guid, s.url, s.title, s.excerpt, s.published, s.read, s.saved
                 FROM stories s JOIN feeds f ON f.id = s.feed_id WHERE s.id = ?1",
                params![id],
                story_row,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn prune_cache(&self, keep: u32) {
        self.conn
            .execute(
                "DELETE FROM stories
                 WHERE saved = 0 AND id NOT IN (
                    SELECT id FROM stories ORDER BY COALESCE(published, fetched) DESC LIMIT ?1)",
                params![keep],
            )
            .ok();
    }
}

fn story_row(r: &rusqlite::Row) -> rusqlite::Result<Story> {
    Ok(Story {
        id: r.get(0)?,
        feed_id: r.get(1)?,
        feed_title: r.get(2)?,
        guid: r.get(3)?,
        url: r.get(4)?,
        title: r.get(5)?,
        excerpt: r.get(6)?,
        published: r.get(7)?,
        read: r.get::<_, i64>(8)? != 0,
        saved: r.get::<_, i64>(9)? != 0,
    })
}

pub struct NewStory {
    pub feed_id: i64,
    pub guid: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    pub published: Option<i64>,
}
