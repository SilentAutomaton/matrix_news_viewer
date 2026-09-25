use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::model::{Image, TextAtlas};

const RAIN_ATLAS: &[u8] = include_bytes!("../assets/matrixcode_msdf.png");
const COLUMNS: u32 = 16;

pub fn charset() -> Vec<char> {
    let mut chars: Vec<char> = (' '..='~').filter(|c| !c.is_ascii_lowercase()).collect();
    chars.extend('А'..='Я');
    chars.extend("Ё«»№…—".chars());
    chars
}

pub fn rain() -> Image {
    let decoder = png::Decoder::new(std::io::Cursor::new(RAIN_ATLAS));
    let mut reader = decoder.read_info().expect("bundled atlas is a valid png");
    let mut pixels = vec![0; reader.output_buffer_size().expect("atlas size fits in memory")];
    let info = reader.next_frame(&mut pixels).expect("bundled atlas is a valid png");
    pixels.truncate(info.buffer_size());
    Image { width: info.width, height: info.height, pixels }
}

pub fn font_file(font: &str) -> Option<PathBuf> {
    let path = crate::config::expand(font);
    if path.is_file() {
        return Some(path);
    }
    let output = Command::new("fc-match").args(["-f", "%{file}", font]).output().ok()?;
    let file = PathBuf::from(String::from_utf8(output.stdout).ok()?);
    file.is_file().then_some(file)
}

// A terminal font fills about 0.73 of its line height, so a slot of one terminal cell
// gets text of the terminal's size.
pub fn text(font_bytes: &[u8], slot: (u32, u32)) -> Result<TextAtlas, &'static str> {
    let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())?;
    let chars = charset();
    let rows = (chars.len() as u32).div_ceil(COLUMNS);
    let (slot_w, slot_h) = slot;
    let (width, height) = (COLUMNS * slot_w, rows * slot_h);
    let mut pixels = vec![0u8; (width * height) as usize];
    let px = slot_h as f32 * 0.73;
    let line = font.horizontal_line_metrics(px).ok_or("font has no horizontal metrics")?;
    let baseline = (slot_h as f32 - (line.ascent - line.descent)) / 2.0 + line.ascent;
    let mut index = HashMap::new();
    for (i, &c) in chars.iter().enumerate() {
        index.insert(c, i as u8);
        let (metrics, bitmap) = font.rasterize(c, px);
        let left = (slot_w as i32 - metrics.width as i32) / 2;
        let top = (baseline - (metrics.ymin as f32 + metrics.height as f32)).round() as i32;
        let (sx, sy) = ((i as u32 % COLUMNS) * slot_w, (i as u32 / COLUMNS) * slot_h);
        for y in 0..metrics.height as i32 {
            for x in 0..metrics.width as i32 {
                let (cx, cy) = (left + x, top + y);
                if cx < 0 || cy < 0 || cx >= slot_w as i32 || cy >= slot_h as i32 {
                    continue;
                }
                let target = (sy + cy as u32) * width + sx + cx as u32;
                pixels[target as usize] = bitmap[(y * metrics.width as i32 + x) as usize];
            }
        }
    }
    Ok(TextAtlas { image: Image { width, height, pixels }, columns: COLUMNS, rows, index })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_fits_in_a_byte_index() {
        assert!(charset().len() <= 256);
        assert!(charset().contains(&'Ж'));
        assert!(!charset().contains(&'a'));
    }

    #[test]
    fn bundled_rain_atlas_decodes() {
        let image = rain();
        assert_eq!((image.width, image.height), (512, 512));
        assert_eq!(image.pixels.len(), 512 * 512 * 3);
    }
}
