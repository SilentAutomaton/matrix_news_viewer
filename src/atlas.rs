use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct TextAtlas {
    pub image: Image,
    pub columns: u32,
    pub rows: u32,
    pub index: HashMap<char, u8>,
}

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

pub fn text(font_bytes: &[u8], slot: u32) -> Result<TextAtlas, &'static str> {
    let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())?;
    let chars = charset();
    let rows = (chars.len() as u32).div_ceil(COLUMNS);
    let (width, height) = (COLUMNS * slot, rows * slot);
    let mut pixels = vec![0u8; (width * height) as usize];
    let px = slot as f32 * 0.8;
    let line = font.horizontal_line_metrics(px).ok_or("font has no horizontal metrics")?;
    let baseline = (slot as f32 - (line.ascent - line.descent)) / 2.0 + line.ascent;
    let mut index = HashMap::new();
    for (i, &c) in chars.iter().enumerate() {
        index.insert(c, i as u8);
        let (metrics, bitmap) = font.rasterize(c, px);
        let left = (slot as i32 - metrics.width as i32) / 2;
        let top = (baseline - (metrics.ymin as f32 + metrics.height as f32)).round() as i32;
        let (sx, sy) = ((i as u32 % COLUMNS) * slot, (i as u32 / COLUMNS) * slot);
        for y in 0..metrics.height as i32 {
            for x in 0..metrics.width as i32 {
                let (cx, cy) = (left + x, top + y);
                if cx < 0 || cy < 0 || cx >= slot as i32 || cy >= slot as i32 {
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
