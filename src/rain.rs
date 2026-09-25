use std::f32::consts::{PI, SQRT_2};

use crate::config::Rain;
use crate::rng::hash;

const SQRT_5: f32 = 2.236_068;
const SEARCH_STEP: f32 = 1.0 / 60.0;
const SEARCH_LIMIT: f32 = 30.0;

// Per-column constants shared with the shader through a texture, so the CPU knows
// exactly when a raindrop head passes any cell.
pub struct Column {
    pub time_offset: f32,
    pub speed: f32,
    pub intro_offset: f32,
    pub active: bool,
}

fn unit(seed: i64, salt: i64) -> f32 {
    (hash(seed, salt, 0x5eed) >> 40) as f32 / (1u64 << 24) as f32
}

pub fn columns(count: i32, density: f32) -> Vec<Column> {
    (0..count)
        .map(|x| {
            let intro_offset = if x == count / 2 {
                -1.0
            } else if x == (count as f32 * 0.75) as i32 {
                -2.0
            } else {
                unit(x as i64, 2) * -4.0 + ((x as f32 / count as f32 * PI).sin() - 1.0) * 2.0 - 2.5
            };
            Column {
                time_offset: unit(x as i64, 0) * 1000.0,
                speed: unit(x as i64, 1) * 0.5 + 0.5,
                intro_offset,
                active: unit(x as i64, 3) < density,
            }
        })
        .collect()
}

pub fn texels(columns: &[Column]) -> Vec<f32> {
    columns.iter().flat_map(|c| [c.time_offset, c.speed, c.intro_offset, c.active as u8 as f32]).collect()
}

fn wobble(x: f32) -> f32 {
    x + 0.3 * (SQRT_2 * x).sin() + 0.2 * (SQRT_5 * x).sin()
}

fn rain_time(column: &Column, glyph_y: f32, t: f32, rain: &Rain) -> f32 {
    wobble((glyph_y * 0.01 + column.time_offset + t * rain.fall_speed * column.speed) / rain.raindrop_length)
}

// First moment after `after` when a raindrop head reaches the cell. wobble() only grows,
// so the head passes when the rain time crosses a whole number.
pub fn next_drop(column: &Column, row: i32, rows: i32, after: f32, rain: &Rain) -> f32 {
    let glyph_y = (rows - 1 - row) as f32;
    let start = rain_time(column, glyph_y, after, rain).floor();
    let mut t = after;
    while t < after + SEARCH_LIMIT {
        t += SEARCH_STEP;
        if rain_time(column, glyph_y, t, rain).floor() > start {
            return t;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_stay_in_shader_ranges() {
        for column in columns(96, 1.0) {
            assert!((0.0..1000.0).contains(&column.time_offset));
            assert!((0.5..1.0).contains(&column.speed));
        }
    }

    #[test]
    fn density_sets_the_share_of_raining_columns() {
        let active = columns(1000, 0.3).iter().filter(|c| c.active).count();
        assert!((250..350).contains(&active));
    }

    #[test]
    fn drop_arrives_later_and_within_one_period() {
        let rain = Rain::default();
        let column = &columns(10, 1.0)[3];
        let t = next_drop(column, 5, 54, 100.0, &rain);
        let period = rain.raindrop_length / (rain.fall_speed * column.speed) * 1.2;
        assert!(t > 100.0 && t < 100.0 + period / 0.129);
        assert!(next_drop(column, 5, 54, t, &rain) > t + 0.5);
    }
}
