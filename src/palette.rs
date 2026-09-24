use std::collections::HashMap;

use crate::config;

pub type Rgb = [f32; 3];

pub struct Palette {
    pub lut: Vec<u8>,
    pub cursor: Rgb,
    pub cursor_intensity: f32,
    pub text: Rgb,
    pub background: Rgb,
    pub dither: f32,
}

pub const LUT_SIZE: usize = 256;
const ORANGE: Rgb = [1.0, 0.55, 0.1];

pub fn parse_hex(text: &str) -> Option<Rgb> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn hsl(h: f32, s: f32, l: f32) -> Rgb {
    let a = s * l.min(1.0 - l);
    let f = |n: f32| {
        let k = (n + h * 12.0) % 12.0;
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

struct Base {
    stops: Vec<(f32, Rgb)>,
    cursor: Rgb,
    text: Rgb,
    background: Rgb,
}

fn preset(hue: f32, text: Rgb) -> Base {
    Base {
        stops: vec![
            (0.0, hsl(hue, 0.9, 0.0)),
            (0.2, hsl(hue, 0.9, 0.2)),
            (0.7, hsl(hue, 0.9, 0.7)),
            (0.8, hsl(hue, 0.9, 0.8)),
        ],
        cursor: hsl(hue - 0.058, 1.0, 0.73),
        text,
        background: [0.0; 3],
    }
}

// theme.toml is written by the desktop theme switcher: rain, cursor, text, background.
fn from_theme(theme: &HashMap<String, String>) -> Base {
    let color = |key: &str, fallback: Rgb| theme.get(key).and_then(|v| parse_hex(v)).unwrap_or(fallback);
    let background = color("background", [0.0; 3]);
    let rain = color("rain", hsl(0.3, 0.9, 0.5));
    Base {
        stops: vec![
            (0.0, background),
            (0.2, mix(background, rain, 0.3)),
            (0.7, rain),
            (0.8, mix(rain, [1.0; 3], 0.3)),
        ],
        cursor: color("cursor", mix(rain, [1.0; 3], 0.6)),
        text: color("text", ORANGE),
        background: [0.0; 3],
    }
}

fn base(config: &config::Palette, theme: &HashMap<String, String>) -> Base {
    match config.preset.as_str() {
        "amber" => preset(0.1, hsl(0.55, 1.0, 0.6)),
        "ice" => preset(0.55, ORANGE),
        "red" => preset(0.0, hsl(0.55, 1.0, 0.6)),
        "theme" => from_theme(theme),
        _ => preset(0.3, ORANGE),
    }
}

pub fn build(config: &config::Palette, theme: &HashMap<String, String>) -> Palette {
    let mut base = base(config, theme);
    let custom: Vec<(f32, Rgb)> = config.stops.iter().filter_map(|s| Some((s.at, parse_hex(&s.color)?))).collect();
    if !custom.is_empty() {
        base.stops = custom;
    }
    base.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let pick = |text: &str, fallback: Rgb| parse_hex(text).unwrap_or(fallback);
    Palette {
        lut: lut(&base.stops),
        cursor: pick(&config.cursor, base.cursor),
        cursor_intensity: config.cursor_intensity,
        text: pick(&config.text, base.text),
        background: pick(&config.background, base.background),
        dither: config.dither,
    }
}

fn lut(stops: &[(f32, Rgb)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(LUT_SIZE * 3);
    for i in 0..LUT_SIZE {
        let x = i as f32 / (LUT_SIZE - 1) as f32;
        let upper = stops.iter().position(|s| s.0 >= x);
        let color = match upper {
            None => stops.last().map_or([0.0; 3], |s| s.1),
            Some(0) => stops[0].1,
            Some(j) => {
                let (a, b) = (stops[j - 1], stops[j]);
                mix(a.1, b.1, (x - a.0) / (b.0 - a.0).max(1e-6))
            }
        };
        out.extend(color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    }
    out
}

pub fn parse_theme(text: &str) -> HashMap<String, String> {
    toml::from_str(text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parses_with_and_without_hash() {
        assert_eq!(parse_hex("#ff0000"), Some([1.0, 0.0, 0.0]));
        assert_eq!(parse_hex("00ff00"), Some([0.0, 1.0, 0.0]));
        assert_eq!(parse_hex("red"), None);
    }

    #[test]
    fn lut_interpolates_and_clamps_past_last_stop() {
        let table = lut(&[(0.0, [0.0; 3]), (0.5, [1.0; 3])]);
        assert_eq!(&table[0..3], &[0, 0, 0]);
        assert_eq!(table[64 * 3], 128);
        assert_eq!(&table[(LUT_SIZE - 1) * 3..], &[255, 255, 255]);
    }

    #[test]
    fn theme_preset_uses_theme_colors() {
        let theme = parse_theme("rain = \"#00ff00\"\ntext = \"#0000ff\"");
        let config = config::Palette { preset: "theme".into(), ..Default::default() };
        let palette = build(&config, &theme);
        assert_eq!(palette.text, [0.0, 0.0, 1.0]);
        let at_rain = &palette.lut[178 * 3..][..3];
        assert!(at_rain[0] < 5 && at_rain[1] > 250 && at_rain[2] < 5);
    }
}
