use std::f32::consts::{PI, SQRT_2};

use crate::config::Rain;
use crate::model::Column;
use crate::rng::hash;

const SQRT_5: f32 = 2.236_068;
// Rain clock units: the clock advances by fall_speed per second, so these match
// the old one-frame step and 30 s limit at the default speed of 0.2.
const SEARCH_STEP: f32 = 0.002;
const SEARCH_LIMIT: f32 = 10.0;
// The intro offsets were tuned in seconds at the default fall speed.
const INTRO_SCALE: f32 = 0.2;
const SETTLED: f32 = -1.0e6;

fn unit(seed: i64, salt: i64) -> f32 {
    (hash(seed, salt, 0x5eed) >> 40) as f32 / (1u64 << 24) as f32
}

fn raining(x: i32, density: f32) -> bool {
    unit(x as i64, 3) < density
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
                intro_offset: intro_offset * INTRO_SCALE,
                active: raining(x, density),
                switched: SETTLED,
            }
        })
        .collect()
}

// The same hash decides every column, so a small density change flips only the
// columns near the threshold; they start or stop from the top at `clock`.
pub fn set_density(columns: &mut [Column], density: f32, clock: f32) {
    for (x, column) in columns.iter_mut().enumerate() {
        let active = raining(x as i32, density);
        if active != column.active {
            column.active = active;
            column.switched = clock;
        }
    }
}

// Two texture rows: (offset, speed, intro offset, raining) and (switch clock, 0, 0, 0).
pub fn texels(columns: &[Column]) -> Vec<f32> {
    let first = columns.iter().flat_map(|c| [c.time_offset, c.speed, c.intro_offset, c.active as u8 as f32]);
    let second = columns.iter().flat_map(|c| [c.switched, 0.0, 0.0, 0.0]);
    first.chain(second).collect()
}

fn wobble(x: f32) -> f32 {
    x + 0.3 * (SQRT_2 * x).sin() + 0.2 * (SQRT_5 * x).sin()
}

fn rain_time(column: &Column, glyph_y: f32, clock: f32, rain: &Rain) -> f32 {
    wobble((glyph_y * 0.01 + column.time_offset + clock * column.speed) / rain.raindrop_length)
}

// First rain clock after `after` when a raindrop head reaches the cell. wobble() only
// grows, so the head passes when the rain time crosses a whole number.
pub fn next_drop(column: &Column, row: i32, rows: i32, after: f32, rain: &Rain) -> f32 {
    let glyph_y = (rows - 1 - row) as f32;
    let start = rain_time(column, glyph_y, after, rain).floor();
    let mut clock = after;
    while clock < after + SEARCH_LIMIT {
        clock += SEARCH_STEP;
        if rain_time(column, glyph_y, clock, rain).floor() > start {
            return clock;
        }
    }
    clock
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
    fn density_change_flips_only_crossing_columns() {
        let mut all = columns(1000, 0.3);
        set_density(&mut all, 0.35, 42.0);
        let flipped: Vec<&Column> = all.iter().filter(|c| c.switched == 42.0).collect();
        assert!(!flipped.is_empty() && flipped.len() < 100);
        assert!(flipped.iter().all(|c| c.active));
        assert_eq!(all.iter().filter(|c| c.active).count(), columns(1000, 0.35).iter().filter(|c| c.active).count());
    }

    #[test]
    fn drop_arrives_later_and_within_one_period() {
        let rain = Rain::default();
        let column = &columns(10, 1.0)[3];
        let clock = next_drop(column, 5, 54, 100.0, &rain);
        let period = rain.raindrop_length / column.speed;
        assert!(clock > 100.0 && clock < 100.0 + period / 0.129);
        assert!(next_drop(column, 5, 54, clock, &rain) > clock + 0.1);
    }
}
