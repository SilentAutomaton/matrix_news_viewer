use std::collections::HashMap;

use khronos_egl as egl;
use wayland_egl::WlEglSurface;

use crate::config::Config;

pub type Rgb = [f32; 3];

pub const LUT_SIZE: usize = 256;
pub const TEXT_COLORS: usize = 16;

// Logical pixels in the compositor's global space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellRect {
    pub col: i32,
    pub row: i32,
    pub cols: i32,
    pub rows: i32,
}

impl CellRect {
    pub fn grow(self, cols: i32, rows: i32) -> Self {
        Self { col: self.col - cols, row: self.row - rows, cols: self.cols + 2 * cols, rows: self.rows + 2 * rows }
    }

    pub fn overlaps(&self, other: &CellRect) -> bool {
        self.col < other.col + other.cols && other.col < self.col + self.cols
            && self.row < other.row + other.rows && other.row < self.row + self.rows
    }

    pub fn contains(&self, col: i32, row: i32) -> bool {
        col >= self.col && col < self.col + self.cols && row >= self.row && row < self.row + self.rows
    }
}

// One RGBA texel per world cell: glyph, kind, colour index, intensity.
pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    pub cells: Vec<u8>,
}

impl Grid {
    pub fn new(cols: i32, rows: i32) -> Self {
        Self { cols, rows, cells: vec![0; (cols * rows * 4).max(4) as usize] }
    }

    pub fn set(&mut self, col: i32, row: i32, cell: [u8; 4]) {
        if col >= 0 && row >= 0 && col < self.cols && row < self.rows {
            let at = ((row * self.cols + col) * 4) as usize;
            self.cells[at..at + 4].copy_from_slice(&cell);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Headline {
    pub title: String,
    pub url: Option<String>,
    pub source: String,
    pub published: f64,
}

pub struct Pick {
    pub title: String,
    pub url: Option<String>,
    pub color: u8,
}

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

#[derive(Clone)]
pub struct Palette {
    pub lut: Vec<u8>,
    pub cursor: Rgb,
    pub cursor_intensity: f32,
    pub text: Rgb,
    pub background: Rgb,
    pub dither: f32,
}

// Per-column constants shared with the shader through a texture, so the CPU knows
// exactly when a raindrop head passes any cell.
pub struct Column {
    pub time_offset: f32,
    pub speed: f32,
    pub intro_offset: f32,
    pub active: bool,
    pub switched: f32,
}

pub struct Target {
    pub window: WlEglSurface,
    pub surface: egl::Surface,
}

pub struct Frame<'a> {
    pub time: f32,
    pub rain_clock: f32,
    pub symbol_clock: f32,
    pub world: Rect,
    pub grid: (i32, i32),
    pub config: &'a Config,
    pub palette: &'a Palette,
    pub text_colors: &'a [Rgb],
}
