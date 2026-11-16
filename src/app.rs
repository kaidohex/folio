use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, RichText};

use crate::feed::{FetchResult, spawn_fetch_all};
use crate::model::{Feed, Story, View};
use crate::opml_io::{export_backup, export_opml, import_opml_file};
use crate::store::Store;

const ACCENT: Color32 = Color32::from_rgb(224, 90, 71); // coral
const SIDEBAR: Color32 = Color32::from_rgb(17, 24, 39); // dark navy
const SIDEBAR_TEXT: Color32 = Color32::from_rgb(228, 231, 238);
const SIDEBAR_MUTED: Color32 = Color32::from_rgb(150, 158, 172);

pub struct App {
    store: Store,
    data_dir: std::path::PathBuf,

    view: View,
    stories: Vec<Story>,
    saved_count: i64,
    reading: Option<Story>,

    search_query: String,
    fetch_rx: Option<Receiver<FetchResult>>,
    fetching: bool,

    manage_open: bool,
    add_url: String,
    add_category: String,
    toasts: Vec<Toast>,
    update_status: UpdateStatus,
    update_rx: Option<Receiver<Result<String, String>>>,
}

type Toast = (String, Instant);

#[derive(Debug, Clone, PartialEq)]
enum UpdateStatus {
    Unknown,
    UpToDate,
    New(String),
    Failed(String),
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let data_dir = crate::app::dirs();
        let store = Store::open(&data_dir).expect("open store");
        configure_style(&cc.egui_ctx);
        let mut app = App {
            store,
            data_dir,
            view: View::All,
            stories: Vec::new(),
            saved_count: 0,
            reading: None,
            search_query: String::new(),
            fetch_rx: None,
            fetching: false,
            manage_open: false,
            add_url: String::new(),
            add_category: String::new(),
            toasts: Vec::new(),
            update_status: UpdateStatus::Unknown,
            update_rx: None,
        };
        app.reload();
        app.start_refresh();
        app.start_update_check();
        app
    }

    fn reload(&mut self) {
        self.stories = match &self.view {
            View::All => self.store.stories(None, false, 300),
            View::Saved => self.store.stories(None, true, 300),
            View::Feed(id) => self.store.stories(Some(*id), false, 300),
            View::Search => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() {
                    Vec::new()
                } else {
                    self.store.search(&q, 200)
                }
            }
        };
        self.saved_count = self.store.saved_count();
    }

    fn set_view(&mut self, view: View) {
        self.reading = None;
        self.view = view;
        self.reload();
    }

    fn start_refresh(&mut self) {
        if self.fetching {
            return;
        }
        let feeds = self
            .store
            .feeds()
            .into_iter()
            .filter(|f| f.enabled)
            .collect();
        self.spawn_fetch(feeds, "Fetching all feeds…");
    }

    fn spawn_fetch(&mut self, feeds: Vec<Feed>, note: &str) {
        self.fetching = true;
        let (tx, rx) = channel();
        spawn_fetch_all(self.data_dir.clone(), feeds, tx);
        self.fetch_rx = Some(rx);
        self.toast(note.to_string());
    }

    fn start_update_check(&mut self) {
        let (tx, rx) = channel();
        self.update_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(update_check());
        });
    }

    fn toast(&mut self, msg: String) {
        self.toasts.push((msg, Instant::now()));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(ui.ctx());

        egui::Panel::left("sidebar")
            .exact_size(300.0)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(16.0))
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(250, 250, 248))
                    .inner_margin(24.0),
            )
            .show(ui, |ui| self.center(ui));

        let mut manage_open = self.manage_open;
        egui::Window::new("Manage feeds")
            .open(&mut manage_open)
            .resizable(true)
            .default_width(620.0)
            .show(ui.ctx(), |ui| self.manage(ui));
        self.manage_open = manage_open;

        self.draw_toasts(ui.ctx());
    }
}

impl App {
    fn poll(&mut self, ctx: &egui::Context) {
        if self.fetching {
            let mut result = None;
            if let Some(rx) = &self.fetch_rx
                && let Ok(r) = rx.try_recv()
            {
                result = Some(r);
            }
            if let Some(r) = result {
                self.fetching = false;
                self.fetch_rx = None;
                let failures = r.outcomes.iter().filter(|o| !o.ok).count();
                let new_items: usize = r.outcomes.iter().map(|o| o.new_stories).sum();
                if failures > 0 {
                    self.toast(format!("Refreshed — {failures} feed(s) failed"));
                } else if new_items > 0 {
                    self.toast(format!("Refreshed — {new_items} new stories"));
                } else {
                    self.toast("Refreshed — everything already read".into());
                }
                self.reload();
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
        if let Some(rx) = &self.update_rx
            && let Ok(res) = rx.try_recv()
        {
            self.update_status = match res {
                Ok(latest) => {
                    if latest == version() {
                        UpdateStatus::UpToDate
                    } else {
                        UpdateStatus::New(latest)
                    }
                }
                Err(e) => UpdateStatus::Failed(e),
            };
            self.update_rx = None;
            ctx.request_repaint();
        }
    }

    // ---------- Sidebar ----------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("📰 Folio")
                .size(21.0)
                .strong()
                .color(SIDEBAR_TEXT),
        );
        ui.add_space(14.0);

        // Search.
        let search = egui::TextEdit::singleline(&mut self.search_query)
            .hint_text("Search all stories…")
            .desired_width(ui.available_width());
        let search_resp = ui.add(search);
        if search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.set_view(View::Search);
        }
        ui.add_space(12.0);

        // View buttons.
        if sidebar_button(ui, "All stories", matches!(self.view, View::All)) {
            self.set_view(View::All);
        }
        if sidebar_button(
            ui,
            format!("Saved ({})", self.saved_count),
            matches!(self.view, View::Saved),
        ) {
            self.set_view(View::Saved);
        }
        ui.add_space(14.0);
        ui.label(RichText::new("Feeds").size(12.0).color(SIDEBAR_MUTED));
        ui.add_space(4.0);

        let feeds = self.store.feeds();
        egui::ScrollArea::vertical()
            .id_salt("sidebar_feeds")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut current_category: Option<String> = None;
                for feed in &feeds {
                    if feed.category != current_category {
                        current_category = feed.category.clone();
                        if let Some(cat) = &current_category {
                            ui.add_space(6.0);
                            ui.label(RichText::new(cat).size(11.0).color(SIDEBAR_MUTED));
                        }
                    }
                    let selected = matches!(self.view, View::Feed(id) if id == feed.id);
                    let dead = feed.enabled && feed.last_error.is_some();
                    let label = if dead {
                        format!("● {}", feed.title)
                    } else {
                        feed.title.clone()
                    };
                    let text = if selected {
                        RichText::new(label)
                            .size(13.5)
                            .strong()
                            .color(Color32::WHITE)
                    } else {
                        RichText::new(label).size(13.5).color(SIDEBAR_TEXT)
                    };
                    let resp = ui.add(egui::Button::new(text).frame(selected).fill(if selected {
                        ACCENT.gamma_multiply(0.35)
                    } else {
                        Color32::TRANSPARENT
                    }));
                    if resp.clicked() {
                        self.set_view(View::Feed(feed.id));
                    }
                    if dead {
                        resp.on_hover_text(format!(
                            "Not updating · {}",
                            feed.last_error.as_deref().unwrap_or("error")
                        ));
                    }
                }
            });

        ui.add_space(10.0);
        if ui
            .add(
                egui::Button::new(
                    RichText::new("⚙ Manage feeds")
                        .size(13.0)
                        .color(SIDEBAR_TEXT),
                )
                .fill(Color32::from_rgb(35, 43, 58)),
            )
            .clicked()
        {
            self.manage_open = true;
        }
    }

    // ---------- Center ----------

    fn center(&mut self, ui: &mut egui::Ui) {
        if let Some(story) = &self.reading {
            let story = story.clone();
            self.reading_view(ui, &story);
            return;
        }
        let heading = match &self.view {
            View::All => "All stories".to_string(),
            View::Saved => "Saved".to_string(),
            View::Feed(id) => self
                .store
                .feeds()
                .iter()
                .find(|f| f.id == *id)
                .map(|f| f.title.clone())
                .unwrap_or_else(|| "Feed".into()),
            View::Search => format!("Search results — “{}”", self.search_query.trim()),
        };
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(
                RichText::new(&heading).size(22.0).strong(),
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("↻ Refresh").clicked() {
                    self.start_refresh();
                }
            });
        });
        ui.add_space(12.0);

        if self.stories.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                let msg = if matches!(self.view, View::Search) {
                    "No matches."
                } else if self.fetching {
                    "Fetching feeds…"
                } else {
                    "No stories yet."
                };
                ui.label(RichText::new(msg).size(15.0).weak());
            });
            return;
        }

        let stories = self.stories.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for story in &stories {
                    self.story_card(ui, story);
                    ui.add_space(10.0);
                }
            });
    }

    fn story_card(&mut self, ui: &mut egui::Ui, story: &Story) {
        let title_text = if story.read {
            RichText::new(&story.title).size(15.0).weak()
        } else {
            RichText::new(&story.title).size(15.0).strong()
        };
        let source_text = RichText::new(format!(
            "{} · {}",
            story.feed_title,
            format_date(story.published)
        ))
        .size(11.5)
        .color(ACCENT);

        egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.vertical(|ui| {
                    if ui.add(egui::Button::new(title_text).frame(false)).clicked() {
                        self.store.set_read(story.id, true);
                        self.reading = Some(story.clone());
                        self.reload();
                    }
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(source_text));
                    if !story.excerpt.is_empty() {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(RichText::new(&story.excerpt).size(12.5).weak())
                                .wrap(),
                        );
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.small_button("Open in browser").clicked() {
                            open_url(story);
                        }
                        if ui
                            .small_button(if story.saved {
                                "★ Unsave"
                            } else {
                                "☆ Save"
                            })
                            .clicked()
                        {
                            self.store.set_saved(story.id, !story.saved);
                            self.toast(if story.saved {
                                "Removed from Saved".into()
                            } else {
                                "Saved for later".into()
                            });
                            self.reload();
                        }
                        if ui.small_button("⧉ Share").clicked() {
                            self.share(story);
                        }
                    });
                });
            });
    }

    fn share(&mut self, story: &Story) {
        let ok = copy_to_clipboard(&story.url);
        self.toast(if ok {
            "Link copied to clipboard".into()
        } else {
            "Could not copy link".into()
        });
    }

    fn reading_view(&mut self, ui: &mut egui::Ui, story: &Story) {
        if ui.button("← Back").clicked() {
            self.reading = None;
            self.reload();
            return;
        }
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(&story.title).size(24.0).strong()).wrap());
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "{} · {}",
                story.feed_title,
                format_date(story.published)
            ))
            .size(12.5)
            .color(ACCENT),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Read in browser ↗").clicked() {
                open_url(story);
            }
            if ui
                .button(if story.saved {
                    "★ Unsave"
                } else {
                    "☆ Save"
                })
                .clicked()
            {
                self.store.set_saved(story.id, !story.saved);
                self.reload();
            }
            if ui.button("⧉ Share").clicked() {
                self.share(story);
            }
        });
        ui.add_space(16.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if story.excerpt.is_empty() {
                ui.label("No preview text in this feed's content — open in browser to read the full article.");
            } else {
                ui.add(egui::Label::new(RichText::new(&story.excerpt).size(14.0)).wrap());
            }
        });
    }

    // ---------- Manage feeds ----------

    fn manage(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Add feed:");
            ui.add(
                egui::TextEdit::singleline(&mut self.add_url)
                    .hint_text("https://example.com/feed")
                    .desired_width(240.0),
            );
            ui.label("Category");
            ui.add(
                egui::TextEdit::singleline(&mut self.add_category)
                    .hint_text("(optional)")
                    .desired_width(120.0),
            );
            let can_add = !self.add_url.trim().is_empty();
            if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                let url = self.add_url.trim().to_string();
                let category = self.add_category.trim();
                let category = if category.is_empty() {
                    None
                } else {
                    Some(category.to_string())
                };
                if self
                    .store
                    .add_feed(&url, &url, category.as_deref())
                    .is_some()
                {
                    self.toast("Feed added — fetching…".into());
                    self.add_url.clear();
                    self.add_category.clear();
                    self.start_refresh();
                } else {
                    self.toast("That feed is already in your list".into());
                }
            }
        });
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("⭳ Import OPML / backup").clicked()
                && let Some(count) = import_opml_file(&self.store)
            {
                self.toast(format!("Imported — {count} new feed(s)"));
                self.start_refresh();
            }
            if ui.button("⭱ Export OPML").clicked() {
                match export_opml(&self.store) {
                    Ok(path) => self.toast(format!("Exported to {}", path.display())),
                    Err(e) if e != "cancelled" => self.toast(format!("Export failed: {e}")),
                    Err(_) => {}
                }
            }
            if ui.button("⭱ Export backup (JSON)").clicked() {
                match export_backup(&self.store) {
                    Ok(path) => self.toast(format!("Exported to {}", path.display())),
                    Err(e) if e != "cancelled" => self.toast(format!("Export failed: {e}")),
                    Err(_) => {}
                }
            }
        });
        ui.separator();
        ui.add_space(4.0);

        let feeds = self.store.feeds();
        egui::ScrollArea::vertical()
            .id_salt("feeds_list")
            .max_height(340.0)
            .show(ui, |ui| {
                for feed in feeds {
                    self.feed_row(ui, &feed);
                    ui.separator();
                }
            });

        ui.add_space(6.0);
        let footer = match &self.update_status {
            UpdateStatus::Unknown => format!("Folio v{} · checking for updates…", version()),
            UpdateStatus::UpToDate => format!("Folio v{} · up to date", version()),
            UpdateStatus::New(latest) => format!("Folio v{} · {} available", version(), latest),
            UpdateStatus::Failed(_) => {
                format!("Folio v{} · could not check for updates", version())
            }
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(footer).size(11.5).weak());
            if ui.small_button("Check for updates").clicked() {
                self.start_update_check();
            }
            if let UpdateStatus::Failed(e) = &self.update_status {
                ui.label(RichText::new(e).size(10.0).weak());
            }
        });
    }

    fn feed_row(&mut self, ui: &mut egui::Ui, feed: &Feed) {
        let healthy = feed.last_success.is_some() && feed.last_error.is_none();
        let dot = if feed.enabled && !healthy {
            RichText::new("● ").color(Color32::RED).size(13.0)
        } else if feed.enabled {
            RichText::new("● ")
                .color(Color32::from_rgb(76, 175, 120))
                .size(13.0)
        } else {
            RichText::new("● ").color(Color32::GRAY).size(13.0)
        };
        ui.horizontal(|ui| {
            ui.label(dot);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&feed.title).strong());
                    if let Some(cat) = &feed.category {
                        ui.label(RichText::new(cat).size(10.5).weak());
                    }
                });
                match (&feed.last_error, feed.last_success) {
                    (Some(err), last) => {
                        ui.label(
                            RichText::new(format!(
                                "Not updating · last success {}",
                                format_date(last)
                            ))
                            .size(10.5)
                            .color(Color32::RED),
                        );
                        ui.label(RichText::new(err).size(10.0).weak());
                    }
                    (None, last) => {
                        ui.label(
                            RichText::new(format!("Last success {}", format_date(last)))
                                .size(10.5)
                                .weak(),
                        );
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Remove").clicked() {
                    self.store.remove_feed(feed.id);
                    self.toast("Feed removed".into());
                    if self.view == View::Feed(feed.id) {
                        self.view = View::All;
                    }
                    self.reload();
                }
                let enabled_label = if feed.enabled {
                    "Enabled ✓"
                } else {
                    "Disabled"
                };
                if ui.small_button(enabled_label).clicked() {
                    self.store.set_feed_enabled(feed.id, !feed.enabled);
                    self.reload();
                }
                if ui.small_button("Retry").clicked() {
                    self.spawn_fetch(vec![feed.clone()], &format!("Retrying {}…", feed.title));
                }
            });
        });
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts
            .retain(|(_, at)| at.elapsed() < Duration::from_secs(3));
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(200));
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .show(ctx, |ui| {
                for (msg, _) in &self.toasts {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(30, 35, 45))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(msg).color(Color32::WHITE).size(13.0));
                        });
                    ui.add_space(6.0);
                }
            });
    }
}

fn sidebar_button(ui: &mut egui::Ui, label: impl Into<String>, selected: bool) -> bool {
    let text = if selected {
        RichText::new(label.into())
            .size(14.0)
            .strong()
            .color(Color32::WHITE)
    } else {
        RichText::new(label.into()).size(14.0).color(SIDEBAR_TEXT)
    };
    ui.add(
        egui::Button::new(text)
            .frame(selected)
            .fill(if selected {
                ACCENT.gamma_multiply(0.35)
            } else {
                Color32::TRANSPARENT
            })
            .min_size(egui::vec2(ui.available_width(), 28.0)),
    )
    .clicked()
}

fn open_url(story: &Story) {
    if !story.url.is_empty() {
        let _ = open::that_detached(&story.url);
    }
}

pub fn dirs() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("FOLIO_DATA_DIR") {
        return std::path::PathBuf::from(dir);
    }
    dirs_data().join("Folio")
}

#[cfg(target_os = "windows")]
fn dirs_data() -> std::path::PathBuf {
    std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(not(target_os = "windows"))]
fn dirs_data() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
}

fn copy_to_clipboard(text: &str) -> bool {
    (|| -> Result<(), arboard::Error> {
        let mut cb = arboard::Clipboard::new()?;
        cb.set_text(text.to_string())?;
        Ok(())
    })()
    .is_ok()
}

fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn format_date(ms: Option<i64>) -> String {
    use chrono::TimeZone;
    match ms {
        Some(ms) => chrono::Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|d| d.format("%b %d, %Y %H:%M").to_string())
            .unwrap_or_default(),
        None => "—".into(),
    }
}

fn update_check() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Folio")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://api.github.com/repos/kaidohex/folio/releases/latest")
        .send()
        .map_err(|e| format!("offline ({e})"))?;
    let text = resp.text().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = json["tag_name"].as_str().ok_or("unexpected response")?;
    Ok(tag.trim_start_matches('v').to_string())
}

fn configure_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = Color32::from_rgb(250, 250, 248);
        visuals.faint_bg_color = Color32::from_rgb(244, 244, 241);
        visuals.window_fill = Color32::from_rgb(252, 252, 250);
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke.color = Color32::WHITE;
        visuals.widgets.active.fg_stroke.color = ACCENT;
        visuals.widgets.hovered.fg_stroke.color = ACCENT;
        style.visuals = visuals;
    });
}
