use std::collections::HashMap;

use crate::config::{Config, Text};
use crate::model::{CellRect, Grid, Pick};
use crate::rng::Rng;

pub const KIND_TEXT: u8 = 1;
pub const KIND_CATCH: u8 = 2;

const SINGLE_LINE_MAX: usize = 36;
const PARAGRAPH_LINES: usize = 8;
// Seconds a raindrop head holds a letter in the cursor colour while it catches or drops it.
const CATCH_SECONDS: f32 = 0.35;

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

fn glyphs(text: &str, index: &HashMap<char, u8>) -> Vec<char> {
    let mut chars: Vec<char> = Vec::new();
    for c in text.chars().flat_map(char::to_uppercase).map(normalize) {
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
    chars
}

fn to_glyphs(lines: &[Vec<char>], index: &HashMap<char, u8>) -> Vec<Vec<u8>> {
    lines.iter().map(|line| line.iter().map(|c| index[c]).collect()).collect()
}

pub fn layout(title: &str, max_width: usize, index: &HashMap<char, u8>) -> Vec<Vec<u8>> {
    let chars = glyphs(title, index);
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
    let lines: Vec<Vec<char>> = lines.iter().map(|line| truncate(line, max_width)).collect();
    to_glyphs(&lines, index)
}

fn wrap(chars: &[char], width: usize, max_lines: usize) -> Vec<Vec<char>> {
    let mut lines: Vec<Vec<char>> = Vec::new();
    let mut rest = chars;
    while !rest.is_empty() && lines.len() < max_lines {
        let split = if rest.len() <= width {
            rest.len()
        } else {
            rest[..=width].iter().rposition(|&c| c == ' ').filter(|&s| s > 0).unwrap_or(width)
        };
        lines.push(rest[..split].to_vec());
        rest = &rest[split..];
        while rest.first() == Some(&' ') {
            rest = &rest[1..];
        }
    }
    if !rest.is_empty()
        && let Some(last) = lines.last_mut() {
            last.truncate(width - 1);
            last.push('…');
        }
    lines
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
    region: CellRect,
    pub url: Option<String>,
    body: Option<String>,
    color: u8,
    catch: f32,
    end: f32,
}

impl Message {
    fn paint(&self, clock: f32, text: &Text, grid: &mut Grid) {
        for letter in &self.letters {
            let cell = if clock < letter.caught || clock >= letter.released {
                continue;
            } else if clock < letter.caught + self.catch || clock >= letter.released - self.catch {
                [letter.glyph, KIND_CATCH, 0, 255]
            } else {
                [letter.glyph, KIND_TEXT, self.color, (text.brightness * 255.0) as u8]
            };
            grid.set(letter.col, letter.row, cell);
        }
    }

    // Letter times are rain clock values, so a letter stays with its raindrop when the
    // fall speed changes later; seconds are converted once, at the current speed.
    // Letters that already fell are caught again, and everything stays for the new dwell.
    fn release(&mut self, clock: f32, config: &Config, drop: &dyn Fn(i32, i32, f32) -> f32) {
        for letter in &mut self.letters {
            if clock >= letter.released - self.catch {
                letter.caught = drop(letter.col, letter.row, clock);
            }
        }
        let shown = self.letters.iter().map(|l| l.caught).fold(clock, f32::max) + self.catch;
        let dwell_end = shown + (config.text.dwell + 0.055 * self.letters.len() as f32) * config.rain.fall_speed;
        for letter in &mut self.letters {
            letter.released = drop(letter.col, letter.row, dwell_end) + self.catch;
        }
        self.end = self.letters.iter().map(|l| l.released).fold(clock, f32::max);
    }
}

fn letters(
    lines: &[Vec<u8>], bounds: CellRect, centered: bool, space: Option<u8>, clock: f32,
    drop: &dyn Fn(i32, i32, f32) -> f32,
) -> Vec<Letter> {
    let mut letters = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let offset = if centered { (bounds.cols - line.len() as i32) / 2 } else { 0 };
        let row = bounds.row + line_index as i32;
        for (i, &glyph) in line.iter().enumerate() {
            if Some(glyph) == space {
                continue;
            }
            let col = bounds.col + offset + i as i32;
            letters.push(Letter { col, row, glyph, caught: drop(col, row, clock), released: f32::INFINITY });
        }
    }
    letters
}

// The paragraph goes on the side of the headline with more room, and shrinks
// until it clears the other messages.
fn paragraph_place(
    chars: &[char], headline: CellRect, region: CellRect, width: usize, others: &[CellRect],
) -> Option<(Vec<Vec<char>>, CellRect)> {
    let margin = (region.rows / 12).max(1);
    let below = region.row + region.rows - margin - (headline.row + headline.rows + 1);
    let above = headline.row - 1 - (region.row + margin);
    let room = below.max(above).min(PARAGRAPH_LINES as i32);
    (1..=room).rev().find_map(|max_lines| {
        let lines = wrap(chars, width, max_lines as usize);
        let cols = lines.iter().map(Vec::len).max()? as i32;
        let rows = lines.len() as i32;
        let centre = headline.col + headline.cols / 2 - cols / 2;
        let col = centre.min(region.col + region.cols - 2 - cols).max(region.col + 2);
        let row = if below >= above { headline.row + headline.rows + 1 } else { headline.row - 1 - rows };
        let bounds = CellRect { col, row, cols, rows };
        others.iter().all(|o| !o.grow(2, 2).overlaps(&bounds)).then_some((lines, bounds))
    })
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

    pub fn update(&mut self, clock: f32) {
        self.messages.retain(|m| clock < m.end);
    }

    // Headlines are paced in seconds, whatever the rain speed.
    pub fn schedule_next(&mut self, t: f32, text: &Text) {
        let span = (text.interval_max - text.interval_min).max(0.0);
        self.next_spawn = t + text.interval_min + span * self.rng.unit() as f32;
    }

    // `drop(col, row, after)` returns the rain clock when the next raindrop head reaches that cell.
    pub fn spawn(
        &mut self, clock: f32, config: &Config, pick: Pick, regions: &[CellRect],
        index: &HashMap<char, u8>, drop: &dyn Fn(i32, i32, f32) -> f32,
    ) {
        let text = &config.text;
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
                let letters = letters(&lines, bounds, true, index.get(&' ').copied(), clock, drop);
                let catch = CATCH_SECONDS * config.rain.fall_speed;
                let mut message = Message { letters, bounds, region, url: pick.url, body: pick.body, color: pick.color, catch, end: clock };
                message.release(clock, config, drop);
                self.messages.push(message);
                return;
            }
        }
    }

    pub fn paint(&self, clock: f32, text: &Text, grid: &mut Grid) {
        grid.cells.fill(0);
        for message in &self.messages {
            message.paint(clock, text, grid);
        }
    }

    fn at(&self, col: i32, row: i32) -> Option<usize> {
        self.messages.iter().position(|m| m.bounds.grow(1, 1).contains(col, row))
    }

    pub fn link_at(&self, col: i32, row: i32) -> Option<&str> {
        self.messages[self.at(col, row)?].url.as_deref()
    }

    pub fn same_message(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        self.messages.iter().any(|m| m.bounds.grow(1, 1).contains(a.0, a.1) && m.bounds.grow(1, 1).contains(b.0, b.1))
    }

    // Rains in the first paragraph of the item under the click, once per message.
    pub fn expand(
        &mut self, col: i32, row: i32, clock: f32, config: &Config, index: &HashMap<char, u8>,
        drop: &dyn Fn(i32, i32, f32) -> f32,
    ) {
        let Some(at) = self.at(col, row) else { return };
        let Some(body) = self.messages[at].body.take() else { return };
        let others: Vec<CellRect> = self.messages.iter().enumerate().filter(|&(i, _)| i != at).map(|(_, m)| m.bounds).collect();
        let message = &mut self.messages[at];
        let width = config.text.max_length.min(message.region.cols.saturating_sub(4).max(0) as usize);
        let chars = glyphs(&body, index);
        if width < 4 || chars.is_empty() {
            return;
        }
        let Some((lines, bounds)) = paragraph_place(&chars, message.bounds, message.region, width, &others) else { return };
        let added = letters(&to_glyphs(&lines, index), bounds, false, index.get(&' ').copied(), clock, drop);
        message.letters.extend(added);
        message.bounds = message.bounds.union(&bounds);
        message.release(clock, config, drop);
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
        let pick = Pick { title: "A fairly long headline that needs two lines here".into(), url: Some("https://x.example".into()), body: None, color: 0 };
        let drop = |col: i32, _row: i32, after: f32| after + 0.5 + (col % 5) as f32 * 0.3;
        let config = Config { rain: crate::config::Rain { fall_speed: 1.0, ..Default::default() }, ..Default::default() };
        headlines.spawn(0.0, &config, pick, &[region], &index(), &drop);
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

    #[test]
    fn wrap_breaks_at_spaces_and_marks_the_cut() {
        let chars: Vec<char> = "one two three four five six".chars().collect();
        let lines: Vec<String> = wrap(&chars, 9, 2).iter().map(|l| l.iter().collect()).collect();
        assert_eq!(lines, vec!["one two", "three…"]);
    }

    #[test]
    fn click_rains_in_the_paragraph_below_the_headline() {
        let config = Config { rain: crate::config::Rain { fall_speed: 1.0, ..Default::default() }, ..Default::default() };
        let region = CellRect { col: 0, row: 0, cols: 96, rows: 54 };
        let mut headlines = Headlines::new(0.0);
        let drop = |col: i32, _row: i32, after: f32| after + 0.5 + (col % 5) as f32 * 0.3;
        let body = "The first paragraph of the article, long enough to wrap over a few lines of the rain.";
        let pick = Pick { title: "Short".into(), url: None, body: Some(body.into()), color: 0 };
        headlines.spawn(0.0, &config, pick, &[region], &index(), &drop);
        let (title, end) = (headlines.messages[0].bounds, headlines.messages[0].end);
        headlines.expand(title.col, title.row, 1.0, &config, &index(), &drop);
        let message = &headlines.messages[0];
        assert!(message.bounds.rows > title.rows + 1);
        assert!(message.end > end);
        let mut grid = Grid::new(96, 54);
        headlines.paint(4.0, &config.text, &mut grid);
        let below = if message.bounds.row == title.row { title.row + title.rows + 1 } else { message.bounds.row };
        let at = (below * 96 * 4) as usize;
        assert!(grid.cells[at..at + 96 * 4].chunks(4).any(|c| c[1] == KIND_TEXT));
    }
}
