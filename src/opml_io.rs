//! OPML import/export and JSON backup export.

use crate::model::Feed;
use crate::store::Store;

/// Import an OPML file chosen with a native dialog. Returns number of new feeds.
pub fn import_opml_file(store: &Store) -> Option<usize> {
    let path = rfd::FileDialog::new()
        .add_filter("OPML / backup", &["opml", "xml", "json"])
        .pick_file()?;
    let bytes = std::fs::read(&path).ok()?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let trimmed = text.trim_start();
    let mut added = 0usize;
    if trimmed.starts_with('<') {
        // OPML
        match opml::OPML::from_str(&text) {
            Ok(doc) => {
                for outline in walk(&doc.body.outlines, None) {
                    if store
                        .add_feed(&outline.0, &outline.0, outline.1.as_deref())
                        .is_some()
                    {
                        added += 1;
                    }
                }
            }
            Err(_) => return None,
        }
    } else {
        // JSON backup: {"feeds": [{"url": ..., "title": ..., "category": ...}]}
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(v) => {
                if let Some(feeds) = v["feeds"].as_array() {
                    for f in feeds {
                        let url = f["url"].as_str().unwrap_or_default();
                        if url.is_empty() || !url.starts_with("https://") {
                            continue;
                        }
                        let title = f["title"].as_str().unwrap_or(url);
                        let category = f["category"].as_str().filter(|c| !c.is_empty());
                        if store.add_feed(url, title, category).is_some() {
                            added += 1;
                        }
                    }
                }
            }
            Err(_) => return None,
        }
    }
    Some(added)
}

fn walk(outlines: &[opml::Outline], category: Option<String>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for o in outlines {
        match o {
            opml::Outline {
                text: _,
                xml_url: Some(url),
                ..
            } => {
                out.push((url.clone(), category.clone()));
            }
            opml::Outline {
                text: _,
                xml_url: None,
                ..
            } => {
                // Folder: recurse with the folder name as the category.
                let child_category = Some(o.text.clone()).filter(|t| !t.is_empty());
                out.extend(walk(
                    &o.outlines,
                    child_category.or_else(|| category.clone()),
                ));
            }
        }
    }
    out
}

pub fn export_opml(store: &Store) -> Result<std::path::PathBuf, String> {
    let path = rfd::FileDialog::new()
        .set_file_name("folio-feeds.opml")
        .add_filter("OPML", &["opml"])
        .save_file()
        .ok_or_else(|| "cancelled".to_string())?;
    let doc = build_opml(store.feeds());
    std::fs::write(&path, doc).map_err(|e| format!("write failed: {e}"))?;
    Ok(path)
}

pub fn export_backup(store: &Store) -> Result<std::path::PathBuf, String> {
    let path = rfd::FileDialog::new()
        .set_file_name("folio-backup.json")
        .add_filter("JSON", &["json"])
        .save_file()
        .ok_or_else(|| "cancelled".to_string())?;
    let feeds: Vec<serde_json::Value> = store
        .feeds()
        .iter()
        .map(|f| {
            serde_json::json!({
                "url": f.url,
                "title": f.title,
                "category": f.category,
            })
        })
        .collect();
    let json = serde_json::json!({ "app": "Folio", "version": 1, "feeds": feeds });
    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap())
        .map_err(|e| format!("write failed: {e}"))?;
    Ok(path)
}

fn build_opml(feeds: Vec<Feed>) -> String {
    use std::collections::BTreeMap;
    let mut by_category: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for f in feeds {
        let key = f.category.clone().unwrap_or_else(|| "Uncategorized".into());
        by_category
            .entry(key)
            .or_default()
            .push((f.title.clone(), f.url.clone()));
    }
    let mut doc = opml::OPML::default();
    for (category, items) in by_category {
        let mut folder = opml::Outline {
            text: category.clone(),
            ..opml::Outline::default()
        };
        for (title, url) in items {
            let o = opml::Outline {
                text: title.clone(),
                xml_url: Some(url.clone()),
                r#type: Some("rss".into()),
                ..opml::Outline::default()
            };
            folder.outlines.push(o);
        }
        doc.body.outlines.push(folder);
    }
    doc.to_string().unwrap_or_default()
}
