use std::collections::HashMap;

use crate::config::Text;
use crate::model::{CellRect, Grid, Pick};
use crate::rng::Rng;

pub const KIND_TEXT: u8 = 1;
pub const KIND_CATCH: u8 = 2;

const SINGLE_LINE_MAX: usize = 36;
// How long a raindrop head holds a letter in the cursor colour while it catches or drops it.
const CATCH: f32 = 0.35;

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

struct Letter {
    col: i32,
    row: i32,
    glyph: u8,
    caught: f32,
    released: f32,
}

pub struct Message {
    letters: Vec<Letter>,
    pub bounds: CellRect,
    pub url: Option<String>,
    color: u8,
    end: f32,
}

impl Message {
    fn paint(&self, t: f32, text: &Text, grid: &mut Grid) {
        for letter in &self.letters {
            let cell = if t < letter.caught || t >= letter.released {
                continue;
            } else if t < letter.caught + CATCH || t >= letter.released - CATCH {
                [letter.glyph, KIND_CATCH, 0, 255]
            } else {
                [letter.glyph, KIND_TEXT, self.color, (text.brightness * 255.0) as u8]
            };
            grid.set(letter.col, letter.row, cell);
        }
    }
}

fn message(
    lines: &[Vec<u8>], bounds: CellRect, pick: Pick, text: &Text, space: Option<u8>, t: f32,
    drop: &dyn Fn(i32, i32, f32) -> f32,
) -> Message {
    let mut letters = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let offset = (bounds.cols - line.len() as i32) / 2;
        let row = bounds.row + line_index as i32;
        for (i, &glyph) in line.iter().enumerate() {
            if Some(glyph) == space {
                continue;
            }
            let col = bounds.col + offset + i as i32;
            letters.push(Letter { col, row, glyph, caught: drop(col, row, t), released: 0.0 });
        }
    }
    let shown = letters.iter().map(|l| l.caught).fold(t, f32::max) + CATCH;
    let dwell_end = shown + text.dwell + 0.055 * letters.len() as f32;
    for letter in &mut letters {
        letter.released = drop(letter.col, letter.row, dwell_end) + CATCH;
    }
    let end = letters.iter().map(|l| l.released).fold(t, f32::max);
    Message { letters, bounds, url: pick.url, color: pick.color, end }
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

    pub fn update(&mut self, t: f32) {
        self.messages.retain(|m| t < m.end);
    }

    // `drop(col, row, after)` is when the next raindrop head reaches that cell.
    pub fn spawn(
        &mut self, t: f32, text: &Text, pick: Pick, regions: &[CellRect], index: &HashMap<char, u8>,
        drop: &dyn Fn(i32, i32, f32) -> f32,
    ) {
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
                self.messages.push(message(&lines, bounds, pick, text, index.get(&' ').copied(), t, drop));
                return;
            }
        }
    }

    pub fn paint(&self, t: f32, text: &Text, grid: &mut Grid) {
        grid.cells.fill(0);
        for message in &self.messages {
            message.paint(t, text, grid);
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
        let drop = |col: i32, _row: i32, after: f32| after + 0.5 + (col % 5) as f32 * 0.3;
        headlines.spawn(0.0, &text, pick, &[region], &index(), &drop);
        let message = &headlines.messages[0];
        assert!(message.bounds.col >= region.col && message.bounds.col + message.bounds.cols <= region.col + region.cols);
        assert!(message.bounds.row >= region.row && message.bounds.row + message.bounds.rows <= region.row + region.rows);
        assert_eq!(headlines.link_at(message.bounds.col, message.bounds.row), Some("https://x.example"));
        let mut grid = Grid::new(96, 54);
        headlines.paint(0.1, &text, &mut grid);
        assert!(grid.cells.chunks(4).all(|c| c[1] == 0));
        headlines.paint(0.6, &text, &mut grid);
        assert!(grid.cells.chunks(4).any(|c| c[1] == KIND_CATCH));
        headlines.paint(3.0, &text, &mut grid);
        assert!(grid.cells.chunks(4).any(|c| c[1] == KIND_TEXT));
        headlines.update(1000.0);
        assert!(headlines.messages.is_empty());
    }
}
