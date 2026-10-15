//! "The Lost Chapters": the main questline toward Mythos, the dragon who dreamed the first story.

use crate::creatures::{Elem, MYTHICAL, SPECIES};
use crate::garden::{self, ITEMS};
use crate::gfx::Rgb;
use crate::save::SaveData;
use crate::ui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Story {
    /// 0 not started · 1 lighthouse · 2 pages · 3 eight wonders · 4 shrine · 5 lamps · 6 island · 7 complete
    pub chapter: u8,
    pub flags: BTreeSet<String>,
}

impl Story {
    pub fn has(&self, f: &str) -> bool {
        self.flags.contains(f)
    }
    pub fn set(&mut self, f: &str) {
        self.flags.insert(f.to_string());
    }
}

pub const CHAPTERS: [&str; 8] = [
    "Prologue",
    "Ch.1 A Flicker on the Shore",
    "Ch.2 The Scattered Verses",
    "Ch.3 Eight Kinds of Wonder",
    "Ch.4 The Thinking Shrine",
    "Ch.5 Light the Lighthouse",
    "Ch.6 Mythos Isle",
    "Epilogue",
];

/// The three Lost Pages: (flag, where, verse).
pub const PAGES: [(&str, &str, &str); 3] = [
    ("page_woods", "a quiet glade in the Latent Woods", "Eight kinds of wonder"),
    ("page_caves", "the deepest tunnel of the Kernel Caverns", "each one needs a home to dream —"),
    ("page_shore", "a tide pool on Token Shore", "think beyond, and I wake."),
];

pub fn page_for_map(map: crate::world::MapId) -> Option<usize> {
    use crate::world::MapId::*;
    match map {
        Woods => Some(0),
        Caves => Some(1),
        Shore => Some(2),
        _ => None,
    }
}

pub fn pages_found(data: &SaveData) -> usize {
    PAGES.iter().filter(|p| data.story.has(p.0)).count()
}

pub const ALL_TYPES: [Elem; 8] = [Elem::Leaf, Elem::Ember, Elem::Tide, Elem::Spark, Elem::Stone, Elem::Byte, Elem::Dream, Elem::Glitch];

/// Types you have no Claudimon of, and types with no habitat in your Sanctuary.
pub fn missing_wonders(data: &SaveData) -> (Vec<Elem>, Vec<Elem>) {
    let owned: BTreeSet<&str> = data
        .mons
        .iter()
        .filter_map(|m| crate::creatures::by_name(&m.species))
        .flat_map(|i| SPECIES[i].types.iter().map(|e| e.name()))
        .collect();
    let homes: BTreeSet<&str> = data.placed.iter().filter_map(|p| garden::item(&p.kind)).filter_map(|i| ITEMS[i].habitat).map(|e| e.name()).collect();
    let no_mon = ALL_TYPES.iter().copied().filter(|e| !owned.contains(e.name())).collect();
    let no_home = ALL_TYPES.iter().copied().filter(|e| !homes.contains(e.name())).collect();
    (no_mon, no_home)
}

/// Species that count toward a complete Claudidex (everything but the mythicals).
pub fn dex_targets() -> impl Iterator<Item = &'static str> {
    SPECIES.iter().filter(|s| s.rarity < MYTHICAL).map(|s| s.name)
}

pub fn dex_progress(data: &SaveData) -> (usize, usize) {
    let total = dex_targets().count();
    let have = dex_targets().filter(|n| data.befriended.contains(*n)).count();
    (have, total)
}

/// The lighthouse's five lamps: (name, lit).
pub fn lamps(data: &SaveData) -> [(&'static str, bool); 5] {
    let (have, total) = dex_progress(data);
    [
        ("Haiku Cup", data.badges.contains("haiku")),
        ("Sonnet Cup", data.badges.contains("sonnet")),
        ("Opus Cup", data.badges.contains("opus")),
        ("Complete Claudidex", have >= total),
        ("Thought Lantern", data.story.has("lantern")),
    ]
}

pub fn lamps_lit(data: &SaveData) -> usize {
    lamps(data).iter().filter(|l| l.1).count()
}

pub const TRIALS: [(&str, &str); 3] = [("trial_trust", "Trial of Trust"), ("trial_strength", "Trial of Strength"), ("trial_wisdom", "Trial of Wisdom")];

pub fn trials_done(data: &SaveData) -> bool {
    TRIALS.iter().all(|t| data.story.has(t.0))
}

/// The Trial of Wisdom: questions about the world you explored. The first answer is correct.
pub const QUIZ: [(&str, [&str; 4]); 8] = [
    ("Which type washes away Ember?", ["Tide", "Leaf", "Spark", "Dream"]),
    ("What flavour does a Haikitten love most?", ["Sweet", "Bitter", "Spicy", "Sour"]),
    ("Who keeps the lighthouse on Token Shore?", ["Ada", "Mira", "Old Cron", "Coach Bench"]),
    ("Where do Kernel Crystals grow?", ["Kernel Caverns", "Latent Woods", "Token Shore", "Promptville"]),
    ("Which creature becomes a Featurefly?", ["Bugbyte", "Gitcko", "Cachebat", "Lintworm"]),
    ("Who is champion of the Opus Cup?", ["Claude", "Senior Sam", "Principal Priya", "Mayor Sudo"]),
    ("What did you speak at the Thinking Shrine?", ["ultrathink", "hello world", "sudo", "rm -rf"]),
    ("How many syllables does a haiku's middle line have?", ["Seven", "Five", "Three", "Nine"]),
];
pub const QUIZ_LEN: usize = 5;

/// Mythos's last test: finish its haiku with a line of five syllables.
pub const HAIKU_START: [&str; 2] = ["Words became the stars,", "and the stars became stories —"];
pub const HAIKU_ENDS: [(&str, bool); 4] = [
    ("now you write with me.", true),
    ("and then everyone went home.", false),
    ("the end.", false),
    ("once upon a time, long ago.", false),
];

pub fn chapter_title(data: &SaveData) -> &'static str {
    CHAPTERS[(data.story.chapter as usize).min(CHAPTERS.len() - 1)]
}

/// What to do next, in a sentence.
pub fn objective(data: &SaveData) -> String {
    let st = &data.story;
    match st.chapter {
        0 => "Talk to Claude in Promptville — something is on their mind.".into(),
        1 => "Visit the old lighthouse on the far side of Token Shore.".into(),
        2 if pages_found(data) < 3 => format!("Find the Lost Pages ({}/3). Ada said: a woods glade, the deep caves, a tide pool.", pages_found(data)),
        2 => "Bring the three Lost Pages to Claude.".into(),
        3 => {
            let (no_mon, no_home) = missing_wonders(data);
            if no_mon.is_empty() && no_home.is_empty() {
                "Show Claude your Sanctuary: every kind of wonder has a home!".into()
            } else {
                let names = |v: &[Elem]| v.iter().map(|e| e.name()).collect::<Vec<_>>().join(", ");
                let mut parts = Vec::new();
                if !no_mon.is_empty() {
                    parts.push(format!("befriend: {}", names(&no_mon)));
                }
                if !no_home.is_empty() {
                    parts.push(format!("build habitats: {}", names(&no_home)));
                }
                format!("Eight kinds of wonder — {}.", parts.join("; "))
            }
        }
        4 => "Find the Thinking Shrine in the north-east of the Latent Woods. Visit at night, and think beyond.".into(),
        5 if lamps_lit(data) < 5 => format!("Light all five lighthouse lamps ({}/5), then see Ada at night.", lamps_lit(data)),
        5 => "All lamps are lit! Visit Ada at the lighthouse at night (20:00–06:00).".into(),
        6 if !trials_done(data) => "Complete the three trials at the altars of Mythos Isle.".into(),
        6 => "Climb to the peak of Mythos Isle.".into(),
        _ => "You befriended Mythos. The island remains — and on some nights, so does its dreamer.".into(),
    }
}

/// The journal: chapter, objective and the true-goal checklist.
pub fn journal(data: &SaveData) -> Vec<(String, Rgb)> {
    let mut lines = vec![(chapter_title(data).to_string(), ui::GOLD), (format!("→ {}", objective(data)), ui::TEXT), (String::new(), ui::TEXT)];
    if data.story.chapter >= 2 {
        for (flag, where_, verse) in PAGES {
            let line = if data.story.has(flag) { format!("  ✓ \"{verse}\"") } else { format!("  ○ a page in {where_}") };
            lines.push((line, if data.story.has(flag) { Rgb(250, 230, 170) } else { ui::DIM }));
        }
        lines.push((String::new(), ui::TEXT));
    }
    lines.push(("The lighthouse lamps".into(), ui::ACCENT));
    for (name, lit) in lamps(data) {
        let (have, total) = dex_progress(data);
        let detail = if name == "Complete Claudidex" { format!("  ({have}/{total})") } else { String::new() };
        lines.push((format!("  {} {name}{detail}", if lit { "✹" } else { "·" }), if lit { Rgb(255, 220, 130) } else { ui::DIM }));
    }
    if data.story.chapter >= 6 {
        lines.push((String::new(), ui::TEXT));
        for (flag, name) in TRIALS {
            lines.push((format!("  {} {name}", if data.story.has(flag) { "✓" } else { "○" }), if data.story.has(flag) { Rgb(150, 230, 140) } else { ui::DIM }));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiz_answers_are_true_to_the_game() {
        let sp = |n: &str| &SPECIES[crate::creatures::by_name(n).unwrap()];
        assert_eq!(crate::battle::effectiveness(Some(Elem::Tide), &[Elem::Ember]), 2.0);
        assert_eq!(sp("Haikitten").fav.name(), QUIZ[1].1[0]);
        assert_eq!(sp("Bugbyte").evolves.unwrap().1, "Featurefly");
        assert!(crate::arena::CUPS.iter().any(|c| c.trainers.iter().any(|t| t.name.contains(QUIZ[5].1[0]))));
        for (_, answers) in QUIZ {
            let set: BTreeSet<_> = answers.iter().collect();
            assert_eq!(set.len(), 4, "duplicate answers");
        }
    }

    #[test]
    fn exactly_one_haiku_ending_has_five_syllables() {
        assert_eq!(HAIKU_ENDS.iter().filter(|e| e.1).count(), 1);
    }

    #[test]
    fn eight_wonders_need_types_and_homes() {
        let mut d = SaveData::default();
        let (m, h) = missing_wonders(&d);
        assert_eq!((m.len(), h.len()), (8, 8));
        d.mons.push(crate::save::OwnedMon::new("Contextopus", false)); // Tide + Byte
        d.placed.push(crate::save::Placed { kind: "pond".into(), x: 0, y: 0 });
        let (m, h) = missing_wonders(&d);
        assert_eq!((m.len(), h.len()), (6, 7));
    }
}
