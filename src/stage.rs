use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::atlas::{self, TextAtlas};
use crate::config::Config;
use crate::headline::{CellRect, Grid, Headlines};
use crate::news::{self, Headline, Picker};
use crate::notices;
use crate::palette::{Rgb, parse_hex};
use crate::rain::{self, Column};
use crate::render::TEXT_COLORS;

const REQUERY: Duration = Duration::from_secs(600);

pub struct Stage {
    pub grid: Grid,
    pub columns: Vec<Column>,
    headlines: Headlines,
    picker: Picker,
    news: Vec<Headline>,
    index: HashMap<char, u8>,
    db: PathBuf,
    db_modified: Option<SystemTime>,
    queried: Option<Instant>,
    reported: bool,
    last_t: f32,
}

fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

// The intro rain reaches the bottom row after this many seconds; text waits for it.
fn intro_seconds(config: &Config, rows: i32) -> f32 {
    if config.rain.skip_intro { 1.0 } else { 9.5 + rows as f32 / (100.0 * config.rain.fall_speed.max(0.01)) }
}

impl Stage {
    pub fn new(config: &Config) -> Self {
        Self {
            grid: Grid::new(1, 1),
            columns: rain::columns(1),
            headlines: Headlines::new(0.0),
            picker: Picker::new(),
            news: Vec::new(),
            index: HashMap::new(),
            db: crate::config::expand(&config.feed.db),
            db_modified: None,
            queried: None,
            reported: false,
            last_t: 0.0,
        }
    }

    pub fn reset(&mut self, config: &Config, cols: i32, rows: i32, now: f32) {
        self.grid = Grid::new(cols, rows);
        self.columns = rain::columns(cols);
        self.headlines = Headlines::new(now + intro_seconds(config, rows));
        self.db = crate::config::expand(&config.feed.db);
        self.queried = None;
    }

    pub fn text_atlas(&mut self, config: &Config, slot: (u32, u32)) -> Option<TextAtlas> {
        let Some(file) = atlas::font_file(&config.text.font) else {
            eprintln!("{}", notices::no_font(&config.text.font));
            return None;
        };
        let bytes = std::fs::read(&file).map_err(|e| eprintln!("{}", notices::bad_font(&config.text.font, &e.to_string()))).ok()?;
        match atlas::text(&bytes, slot) {
            Ok(built) => {
                self.index = built.index.clone();
                Some(built)
            }
            Err(reason) => {
                eprintln!("{}", notices::bad_font(&config.text.font, reason));
                None
            }
        }
    }

    pub fn poll(&mut self, config: &Config) {
        let modified = std::fs::metadata(&self.db).and_then(|m| m.modified()).ok();
        let stale = self.queried.is_none_or(|at| at.elapsed() > REQUERY);
        if modified == self.db_modified && !stale {
            return;
        }
        self.db_modified = modified;
        self.queried = Some(Instant::now());
        match news::read(&self.db, &config.feed, unix_now()) {
            Ok(news) => {
                self.news = news;
                self.reported = false;
            }
            Err(reason) => {
                if !self.reported {
                    eprintln!("{}", notices::no_news(&self.db, &reason));
                    self.reported = true;
                }
                self.news.clear();
            }
        }
    }

    pub fn frame(&mut self, config: &Config, t: f32, regions: &[CellRect]) {
        let text = &config.text;
        if t < self.last_t {
            self.headlines = Headlines::new(t);
        }
        self.last_t = t;
        self.headlines.update(t);
        if self.headlines.wants_new(t, text) && !self.index.is_empty() {
            let pick = match self.news.is_empty() {
                true => self.picker.fallback(&text.fallback),
                false => self.picker.next(&self.news, &config.feed, unix_now()),
            };
            let (columns, rows) = (&self.columns, self.grid.rows);
            let drop = |col: i32, row: i32, after: f32| rain::next_drop(&columns[col as usize], row, rows, after, &config.rain);
            if let Some(pick) = pick {
                self.headlines.spawn(t, text, pick, regions, &self.index, &drop);
            }
        }
        self.headlines.paint(t, text, &mut self.grid);
    }

    pub fn click(&self, col: i32, row: i32) {
        let Some(url) = self.headlines.link_at(col, row) else { return };
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return;
        }
        match Command::new("xdg-open").arg(url).spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
            }
            Err(_) => eprintln!("{}", notices::link_failed(url)),
        }
    }
}

pub fn text_colors(config: &Config, default: Rgb) -> Vec<Rgb> {
    let sources = config.feed.sources.iter().take(TEXT_COLORS - 1);
    std::iter::once(default)
        .chain(sources.map(|s| s.color.as_deref().and_then(parse_hex).unwrap_or(default)))
        .collect()
}
