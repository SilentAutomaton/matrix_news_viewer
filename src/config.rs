use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub display: Display,
    pub palette: Palette,
    pub rain: Rain,
    pub bloom: Bloom,
    pub text: Text,
    pub feed: Feed,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    pub fps: f32,
    pub fps_on_battery: f32,
    pub outputs: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Palette {
    pub preset: String,
    pub stops: Vec<Stop>,
    pub cursor: String,
    pub cursor_intensity: f32,
    pub text: String,
    pub background: String,
    pub dither: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub at: f32,
    pub color: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Rain {
    pub cell_size: f32,
    pub animation_speed: f32,
    pub fall_speed: f32,
    pub raindrop_length: f32,
    pub cycle_speed: f32,
    pub glyphs: u32,
    pub base_brightness: f32,
    pub base_contrast: f32,
    pub skip_intro: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Bloom {
    pub strength: f32,
    pub size: f32,
    pub threshold: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Text {
    pub font: String,
    pub max_length: usize,
    pub interval_min: f32,
    pub interval_max: f32,
    pub reveal_rate: f32,
    pub dwell: f32,
    pub erase_rate: f32,
    pub scramble_width: f32,
    pub max_concurrent: usize,
    pub brightness: f32,
    pub fallback: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Feed {
    pub db: String,
    pub table: String,
    pub title: String,
    pub url: String,
    pub source: String,
    pub time: String,
    pub time_kind: TimeKind,
    pub window_hours: f64,
    pub half_life_hours: f64,
    pub sources: Vec<Source>,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TimeKind {
    Unix,
    Iso,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub matches: String,
    #[serde(default = "one")]
    pub weight: f64,
    pub color: Option<String>,
}

fn one() -> f64 {
    1.0
}

impl Default for Display {
    fn default() -> Self {
        Self { fps: 30.0, fps_on_battery: 15.0, outputs: Vec::new() }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            preset: "green".into(),
            stops: Vec::new(),
            cursor: String::new(),
            cursor_intensity: 2.0,
            text: String::new(),
            background: String::new(),
            dither: 0.05,
        }
    }
}

impl Default for Rain {
    fn default() -> Self {
        Self {
            cell_size: 20.0,
            animation_speed: 1.0,
            fall_speed: 0.3,
            raindrop_length: 0.75,
            cycle_speed: 1.8,
            glyphs: 57,
            base_brightness: -0.5,
            base_contrast: 1.1,
            skip_intro: false,
        }
    }
}

impl Default for Bloom {
    fn default() -> Self {
        Self { strength: 0.7, size: 0.4, threshold: 0.1 }
    }
}

impl Default for Text {
    fn default() -> Self {
        Self {
            font: "monospace:bold".into(),
            max_length: 72,
            interval_min: 4.0,
            interval_max: 10.0,
            reveal_rate: 18.0,
            dwell: 4.0,
            erase_rate: 30.0,
            scramble_width: 3.0,
            max_concurrent: 2,
            brightness: 1.0,
            fallback: vec!["WAKE UP".into(), "FOLLOW THE WHITE RABBIT".into(), "THERE IS NO SPOON".into()],
        }
    }
}

impl Default for Feed {
    fn default() -> Self {
        Self {
            db: "~/.local/share/newsboat/cache.db".into(),
            table: "rss_item".into(),
            title: "title".into(),
            url: "url".into(),
            source: "feedurl".into(),
            time: "pubDate".into(),
            time_kind: TimeKind::Unix,
            window_hours: 48.0,
            half_life_hours: 12.0,
            sources: Vec::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    base.join("matrix-news-viewer")
}

pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let config = parse("").unwrap();
        assert_eq!(config.display.fps, 30.0);
        assert_eq!(config.feed.time_kind, TimeKind::Unix);
    }

    #[test]
    fn partial_sections_keep_other_defaults() {
        let config = parse("[rain]\nfall_speed = 0.5\n[[feed.sources]]\nmatches = \"habr\"\ncolor = \"#ff0000\"").unwrap();
        assert_eq!(config.rain.fall_speed, 0.5);
        assert_eq!(config.rain.cell_size, 20.0);
        assert_eq!(config.feed.sources[0].weight, 1.0);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(parse("[rain]\nfal_speed = 0.5").is_err());
    }
}
