//! Weather: changes every few hours (the same for everyone on a given day), tints the
//! world, fills the sky with particles, and rain waters your crops for you.

use crate::clock::Season;
use crate::gfx::{Rgb, Screen};
use crate::world::hash;
use chrono::{Datelike, Local, Timelike};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Weather {
    Clear,
    Cloudy,
    Rain,
    Storm,
    Snow,
}

impl Weather {
    pub fn now(season: Season) -> Weather {
        if let Ok(w) = std::env::var("CLAUDIMON_WEATHER") {
            return match w.as_str() {
                "cloudy" => Weather::Cloudy,
                "rain" => Weather::Rain,
                "storm" => Weather::Storm,
                "snow" => Weather::Snow,
                _ => Weather::Clear,
            };
        }
        let t = Local::now();
        Weather::roll(season, t.ordinal() as i32 + t.year() * 400, t.hour() as i32 / 3)
    }

    /// Deterministic weather for a day and three-hour block.
    pub fn roll(season: Season, day: i32, block: i32) -> Weather {
        let r = hash(day, block, 977) % 100;
        match season {
            Season::Winter => match r {
                0..=29 => Weather::Snow,
                30..=54 => Weather::Cloudy,
                _ => Weather::Clear,
            },
            Season::Spring => match r {
                0..=21 => Weather::Rain,
                22..=24 => Weather::Storm,
                25..=44 => Weather::Cloudy,
                _ => Weather::Clear,
            },
            Season::Summer => match r {
                0..=9 => Weather::Rain,
                10..=16 => Weather::Storm,
                17..=29 => Weather::Cloudy,
                _ => Weather::Clear,
            },
            Season::Autumn => match r {
                0..=24 => Weather::Rain,
                25..=27 => Weather::Storm,
                28..=52 => Weather::Cloudy,
                _ => Weather::Clear,
            },
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Weather::Clear => "☀",
            Weather::Cloudy => "☁",
            Weather::Rain => "☂",
            Weather::Storm => "↯",
            Weather::Snow => "❄",
        }
    }

    pub fn wet(self) -> bool {
        matches!(self, Weather::Rain | Weather::Storm)
    }

    /// Extra light multiplier on top of the time of day.
    pub fn tint(self) -> [f32; 3] {
        match self {
            Weather::Clear => [1.0, 1.0, 1.0],
            Weather::Cloudy => [0.9, 0.9, 0.94],
            Weather::Rain => [0.8, 0.84, 0.92],
            Weather::Storm => [0.66, 0.68, 0.8],
            Weather::Snow => [0.95, 0.97, 1.02],
        }
    }
}

/// Screen-space particles: rain streaks, snowflakes, falling leaves or petals, lightning.
/// `z` is the render scale (2 when zoomed out), so particles stay crisp after downsampling.
pub fn draw(s: &mut Screen, w: Weather, season: Season, t: f32, z: i32) {
    let (sw, sh) = (s.w as i32, s.h as i32);
    let area = (sw * sh / (z * z)).max(1);
    let dot = |s: &mut Screen, x: i32, y: i32, c: Rgb, a: f32| {
        for dy in 0..z {
            for dx in 0..z {
                s.blend(x + dx, y + dy, c, a);
            }
        }
    };
    let fall = |i: i32, speed: f32, drift: f32| -> (i32, i32) {
        let x0 = (hash(i, 1, 3) % sw as u32) as f32;
        let y0 = (hash(i, 2, 3) % sh as u32) as f32;
        let sp = speed * (0.7 + (hash(i, 3, 3) % 60) as f32 / 100.0);
        (((x0 + t * drift * sp).rem_euclid(sw as f32)) as i32, ((y0 + t * sp).rem_euclid(sh as f32)) as i32)
    };
    match w {
        Weather::Rain | Weather::Storm => {
            let n = area / if w == Weather::Storm { 45 } else { 70 };
            for i in 0..n {
                let (x, y) = fall(i, 120.0, -0.25);
                for k in 0..3 {
                    dot(s, x - k * z / 2, y - k * z, Rgb(190, 210, 250), 0.55 - k as f32 * 0.15);
                }
            }
            if w == Weather::Storm {
                // A lightning flash every so often.
                let phase = (t / 7.0).fract();
                if phase < 0.025 || (0.05..0.065).contains(&phase) {
                    for y in 0..sh {
                        for x in 0..sw {
                            s.blend(x, y, Rgb(240, 240, 255), 0.45);
                        }
                    }
                }
            }
        }
        Weather::Snow => {
            for i in 0..area / 90 {
                let (x, y) = fall(i, 9.0, 0.0);
                let sway = ((t * 1.3 + i as f32).sin() * 2.5) as i32;
                dot(s, x + sway * z, y, Rgb(255, 255, 255), 0.9);
            }
        }
        Weather::Clear | Weather::Cloudy => {
            // Gentle seasonal drift: petals in spring, leaves in autumn.
            let colors: &[Rgb] = match season {
                Season::Spring => &[Rgb(255, 190, 215), Rgb(255, 225, 235)],
                Season::Autumn => &[Rgb(230, 120, 40), Rgb(210, 70, 40), Rgb(240, 190, 60)],
                _ => return,
            };
            for i in 0..area / 500 {
                let (x, y) = fall(i, 7.0, 1.6);
                let sway = ((t * 1.1 + i as f32 * 0.7).sin() * 3.0) as i32;
                dot(s, x + sway * z, y, colors[(hash(i, 4, 3) as usize) % colors.len()], 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_is_stable_within_a_block_and_varied_across_days() {
        assert_eq!(Weather::roll(Season::Autumn, 100, 3), Weather::roll(Season::Autumn, 100, 3));
        let kinds: std::collections::HashSet<_> = (0..200).map(|d| Weather::roll(Season::Autumn, d, 2)).collect();
        assert!(kinds.len() >= 3);
        assert!((0..400).all(|d| Weather::roll(Season::Summer, d, 1) != Weather::Snow), "no summer snow");
        let clear = (0..1000).filter(|d| Weather::roll(Season::Summer, *d, 0) == Weather::Clear).count();
        assert!(clear > 550, "summer is mostly sunny ({clear}/1000)");
    }
}
