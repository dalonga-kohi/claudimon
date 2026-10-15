//! Real-world time: day/night lighting and seasons follow the system clock.

use chrono::{Datelike, Local, Timelike};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn now() -> Season {
        match Local::now().month() {
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            9..=11 => Season::Autumn,
            _ => Season::Winter,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Season::Spring => "Spring",
            Season::Summer => "Summer",
            Season::Autumn => "Autumn",
            Season::Winter => "Winter",
        }
    }
}

/// Fractional hour of the day, 0.0..24.0.
pub fn hour() -> f32 {
    if let Some(h) = std::env::var("CLAUDIMON_HOUR").ok().and_then(|h| h.parse::<f32>().ok()) {
        return h.rem_euclid(24.0);
    }
    let t = Local::now();
    t.hour() as f32 + t.minute() as f32 / 60.0
}

pub fn clock_str() -> String {
    Local::now().format("%H:%M").to_string()
}

pub fn is_night(h: f32) -> bool {
    !(6.0..20.0).contains(&h)
}

/// RGB light multiplier for a given hour: warm dawn/dusk, cool blue night.
pub fn light(h: f32) -> [f32; 3] {
    const KEYS: [(f32, [f32; 3]); 8] = [
        (0.0, [0.35, 0.40, 0.65]),
        (5.0, [0.40, 0.42, 0.65]),
        (6.5, [1.00, 0.80, 0.70]),
        (9.0, [1.00, 1.00, 1.00]),
        (17.0, [1.00, 0.98, 0.95]),
        (19.0, [1.05, 0.75, 0.60]),
        (21.0, [0.40, 0.42, 0.68]),
        (24.0, [0.35, 0.40, 0.65]),
    ];
    for w in KEYS.windows(2) {
        let (h0, a) = w[0];
        let (h1, b) = w[1];
        if h >= h0 && h <= h1 {
            let t = (h - h0) / (h1 - h0);
            return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        }
    }
    [1.0, 1.0, 1.0]
}
