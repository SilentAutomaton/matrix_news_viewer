use std::collections::HashMap;

use crate::config::Text;
use crate::news::Pick;
use crate::rng::{Rng, hash};

pub const KIND_POCKET: u8 = 1;
pub const KIND_TEXT: u8 = 2;
pub const KIND_SCRAMBLE: u8 = 3;
pub const KIND_FADE: u8 = 4;

const SINGLE_LINE_MAX: usize = 36;
const SCRAMBLE_HZ: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellRect {
    pub col: i32,
    pub row: i32,
    pub cols: i32,
    pub rows: i32,
}

impl CellRect {
    fn grow(self, cols: i32, rows: i32) -> Self {
        Self { col: self.col - cols, row: self.row - rows, cols: self.cols + 2 * cols, rows: self.rows + 2 * rows }
    }

    fn overlaps(&self, other: &CellRect) -> bool {
        self.col < other.col + other.cols && other.col < self.col + self.cols
            && self.row < other.row + other.rows && other.row < self.row + self.rows
    }

    fn contains(&self, col: i32, row: i32) -> bool {
        col >= self.col && col < self.col + self.cols && row >= self.row && row < self.row + self.rows
    }
}

fn normalize(c: char) -> char {
    match c {
        '“' | '”' | '„' => '"',
        '‘' | '’' | '‚' => '\'',
        '–' | '‒' | '―' => '—',
        c if c.is_whitespace() => ' ',
        c => c,
    }
}

fn truncate(line: &[char], max: usize) -> Vec<char> {
    if line.len() <= max {
        return line.to_vec();
    }
    let mut cut: Vec<char> = line[..max - 1].to_vec();
    if let Some(space) = cut.iter().rposition(|&c| c == ' ').filter(|&s| s > max / 2) {
        cut.truncate(space);
    }
    cut.push('…');
    cut
}

pub fn layout(title: &str, max_width: usize, index: &HashMap<char, u8>) -> Vec<Vec<u8>> {
    let mut chars: Vec<char> = Vec::new();
    for c in title.chars().flat_map(char::to_uppercase).map(normalize) {
        if c == ' ' && chars.last().is_none_or(|&last| last == ' ') {
            continue;
        }
        if c == ' ' || index.contains_key(&c) {
            chars.push(c);
        }
    }
    while chars.last() == Some(&' ') {
        chars.pop();
    }
    if chars.is_empty() || max_width < 4 {
        return Vec::new();
    }
    let lines: Vec<Vec<char>> = if chars.len() <= SINGLE_LINE_MAX.min(max_width) {
        vec![chars]
    } else {
        let middle = chars.len() / 2;
        let split = (0..chars.len())
            .filter(|&i| chars[i] == ' ')
            .min_by_key(|&i| i.abs_diff(middle))
            .unwrap_or(middle);
        let (first, second) = chars.split_at(split);
        vec![first.to_vec(), second.iter().skip_while(|&&c| c == ' ').copied().collect()]
    };
    lines.iter().map(|line| truncate(line, max_width).iter().map(|c| index[c]).collect()).collect()
}

pub struct Message {
    lines: Vec<Vec<u8>>,
    pub bounds: CellRect,
    pub url: Option<String>,
    color: u8,
    start: f32,
}

enum Phase {
    Reveal(f32),
    Dwell,
    Erase(f32),
    Done,
}

impl Message {
    fn width(&self) -> f32 {
        self.bounds.cols as f32
    }

    fn phase(&self, t: f32, text: &Text) -> Phase {
        let elapsed = t - self.start;
        let scramble = text.scramble_width;
        let reveal = (self.width() + scramble) / text.reveal_rate;
        let letters: usize = self.lines.iter().map(Vec::len).sum();
        let dwell = reveal + text.dwell + 0.055 * letters as f32;
        let erase = dwell + (self.width() + scramble) / text.erase_rate;
        if elapsed < reveal {
            Phase::Reveal(elapsed * text.reveal_rate)
        } else if elapsed < dwell {
            Phase::Dwell
        } else if elapsed < erase {
            Phase::Erase((elapsed - dwell) * text.erase_rate)
        } else {
            Phase::Done
        }
    }

    pub fn done(&self, t: f32, text: &Text) -> bool {
        matches!(self.phase(t, text), Phase::Done)
    }

    // One RGBA texel per cell: glyph, kind, colour index, intensity.
    fn paint(&self, t: f32, text: &Text, glyphs: u32, grid: &mut Grid) {
        let phase = self.phase(t, text);
        let tick = (t * SCRAMBLE_HZ) as i64;
        let scramble = text.scramble_width;
        let (lo, hi) = match phase {
            Phase::Reveal(head) => (0.0, head),
            Phase::Dwell => (0.0, f32::MAX),
            Phase::Erase(head) => (head, f32::MAX),
            Phase::Done => return,
        };
        let pocket = self.bounds.grow(1, 1);
        for row in pocket.row..pocket.row + pocket.rows {
            for col in pocket.col..pocket.col + pocket.cols {
                let x = (col - self.bounds.col) as f32;
                if x + 1.0 >= lo - scramble && x - 1.0 < hi {
                    grid.set(col, row, [0, KIND_POCKET, 0, 0]);
                }
            }
        }
        for (line_index, line) in self.lines.iter().enumerate() {
            let offset = (self.bounds.cols - line.len() as i32) / 2;
            let row = self.bounds.row + line_index as i32;
            for (i, &glyph) in line.iter().enumerate() {
                let col = self.bounds.col + offset + i as i32;
                let x = (offset + i as i32) as f32;
                let random = (hash(col as i64, row as i64, tick) % glyphs.max(1) as u64) as u8;
                let cell = match phase {
                    Phase::Reveal(head) if x < head - scramble => [glyph, KIND_TEXT, self.color, (text.brightness * 255.0) as u8],
                    Phase::Reveal(head) if x < head => [random, KIND_SCRAMBLE, 0, 255],
                    Phase::Erase(head) if x < head => continue,
                    Phase::Erase(head) if x < head + scramble => [random, KIND_FADE, 0, 128],
                    Phase::Reveal(_) => continue,
                    _ => [glyph, KIND_TEXT, self.color, (text.brightness * 255.0) as u8],
                };
                grid.set(col, row, cell);
            }
        }
    }
}

pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    pub cells: Vec<u8>,
}

impl Grid {
    pub fn new(cols: i32, rows: i32) -> Self {
        Self { cols, rows, cells: vec![0; (cols * rows * 4).max(4) as usize] }
    }

    fn set(&mut self, col: i32, row: i32, cell: [u8; 4]) {
        if col >= 0 && row >= 0 && col < self.cols && row < self.rows {
            let at = ((row * self.cols + col) * 4) as usize;
            self.cells[at..at + 4].copy_from_slice(&cell);
        }
    }
}

pub struct Headlines {
    pub messages: Vec<Message>,
    next_spawn: f32,
    rng: Rng,
}

impl Headlines {
    pub fn new(first_spawn: f32) -> Self {
        Self { messages: Vec::new(), next_spawn: first_spawn, rng: Rng::seeded() }
    }

    pub fn wants_new(&self, t: f32, text: &Text) -> bool {
        t >= self.next_spawn && self.messages.len() < text.max_concurrent
    }

    pub fn update(&mut self, t: f32, text: &Text) {
        self.messages.retain(|m| !m.done(t, text));
    }

    pub fn spawn(&mut self, t: f32, text: &Text, pick: Pick, regions: &[CellRect], index: &HashMap<char, u8>) {
        let span = (text.interval_max - text.interval_min).max(0.0);
        self.next_spawn = t + text.interval_min + span * self.rng.unit() as f32;
        if regions.is_empty() {
            return;
        }
        let region = regions[self.rng.below(regions.len())];
        let lines = layout(&pick.title, text.max_length.min(region.cols.saturating_sub(4).max(0) as usize), index);
        if lines.is_empty() {
            return;
        }
        let cols = lines.iter().map(Vec::len).max().unwrap_or(0) as i32;
        let rows = lines.len() as i32;
        let margin_rows = (region.rows / 12).max(1);
        let free_cols = region.cols - cols - 4;
        let free_rows = region.rows - rows - 2 * margin_rows;
        if free_cols < 0 || free_rows < 0 {
            return;
        }
        for _ in 0..20 {
            let bounds = CellRect {
                col: region.col + 2 + self.rng.below(free_cols as usize + 1) as i32,
                row: region.row + margin_rows + self.rng.below(free_rows as usize + 1) as i32,
                cols,
                rows,
            };
            if self.messages.iter().all(|m| !m.bounds.grow(2, 2).overlaps(&bounds)) {
                self.messages.push(Message { lines, bounds, url: pick.url, color: pick.color, start: t });
                return;
            }
        }
    }

    pub fn paint(&self, t: f32, text: &Text, glyphs: u32, grid: &mut Grid) {
        grid.cells.fill(0);
        for message in &self.messages {
            message.paint(t, text, glyphs, grid);
        }
    }

    pub fn link_at(&self, col: i32, row: i32) -> Option<&str> {
        self.messages.iter().find(|m| m.bounds.grow(1, 1).contains(col, row))?.url.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> HashMap<char, u8> {
        crate::atlas::charset().into_iter().enumerate().map(|(i, c)| (c, i as u8)).collect()
    }

    fn text_of(lines: &[Vec<u8>]) -> Vec<String> {
        let chars = crate::atlas::charset();
        lines.iter().map(|l| l.iter().map(|&g| chars[g as usize]).collect()).collect()
    }

    #[test]
    fn short_title_is_one_uppercase_line() {
        assert_eq!(text_of(&layout("Привет, мир — hello", 72, &index())), vec!["ПРИВЕТ, МИР — HELLO"]);
    }

    #[test]
    fn long_title_splits_into_two_balanced_lines() {
        let lines = text_of(&layout("Rust 2.0 released with a brand new borrow checker and faster builds", 72, &index()));
        assert_eq!(lines.len(), 2);
        assert!(lines[0].len().abs_diff(lines[1].len()) < 12);
    }

    #[test]
    fn too_long_lines_end_with_ellipsis() {
        let lines = text_of(&layout(&"word ".repeat(40), 20, &index()));
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
        assert!(lines[0].ends_with('…'));
    }

    #[test]
    fn unknown_characters_are_dropped() {
        assert_eq!(text_of(&layout("日本 Linux 🚀", 72, &index())), vec!["LINUX"]);
    }

    #[test]
    fn message_stays_inside_its_region_and_finishes() {
        let text = Text::default();
        let region = CellRect { col: 48, row: 0, cols: 48, rows: 54 };
        let mut headlines = Headlines::new(0.0);
        let pick = Pick { title: "A fairly long headline that needs two lines here".into(), url: Some("https://x.example".into()), color: 0 };
        headlines.spawn(0.0, &text, pick, &[region], &index());
        let message = &headlines.messages[0];
        assert!(message.bounds.col >= region.col && message.bounds.col + message.bounds.cols <= region.col + region.cols);
        assert!(message.bounds.row >= region.row && message.bounds.row + message.bounds.rows <= region.row + region.rows);
        assert_eq!(headlines.link_at(message.bounds.col, message.bounds.row), Some("https://x.example"));
        let mut grid = Grid::new(96, 54);
        headlines.paint(3.0, &text, 57, &mut grid);
        assert!(grid.cells.chunks(4).any(|c| c[1] == KIND_TEXT));
        headlines.update(1000.0, &text);
        assert!(headlines.messages.is_empty());
    }
}
