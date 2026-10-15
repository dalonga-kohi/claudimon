//! Persistent game state. Stored as JSON in the platform data dir
//! (~/.local/share/claudimon/save.json on Linux), or at $CLAUDIMON_SAVE.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::PathBuf;

pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OwnedMon {
    pub species: String,
    pub shiny: bool,
    pub friendship: u32,
    pub met: i64,
    #[serde(default)]
    pub last_pet: i64,
    #[serde(default)]
    pub last_fed: i64,
    #[serde(default)]
    pub fed_fav: bool,
    #[serde(default = "default_happy")]
    pub happy: f32,
    /// A gift is waiting to be collected.
    #[serde(default)]
    pub gift: bool,
    #[serde(default = "default_level")]
    pub level: u32,
    #[serde(default)]
    pub xp: u32,
    /// Fractional passive XP carried between simulation ticks.
    #[serde(default)]
    pub xp_frac: f32,
}

fn default_level() -> u32 {
    5
}

fn default_happy() -> f32 {
    50.0
}

impl OwnedMon {
    pub fn new(species: &str, shiny: bool) -> OwnedMon {
        OwnedMon { species: species.into(), shiny, friendship: 1, met: now(), last_pet: 0, last_fed: 0, fed_fav: false, happy: 50.0, gift: false, level: 5, xp: 0, xp_frac: 0.0 }
    }
}

/// A habitat or decoration placed in the Sanctuary (top-left tile).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Placed {
    pub kind: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Crop {
    pub flavor: u8,
    pub planted: i64,
    /// Seconds of growth gained from watering.
    pub bonus: i64,
    pub watered_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SaveData {
    pub version: u32,
    pub map: String,
    pub x: i32,
    pub y: i32,
    pub berries: [u32; 5],
    pub mons: Vec<OwnedMon>,
    pub seen: BTreeSet<String>,
    pub known_fav: BTreeSet<String>,
    /// "map:x:y" -> unix time the bush was picked
    pub bushes: BTreeMap<String, i64>,
    pub steps: u64,
    pub created: i64,
    pub last_seen: i64,
    pub tokens: u64,
    pub token_frac: f64,
    /// Owned but unplaced catalog items (seeds, habitats, decor) by id.
    pub items: BTreeMap<String, u32>,
    pub placed: Vec<Placed>,
    /// "x:y" soil tile in the garden -> crop
    pub crops: BTreeMap<String, Crop>,
    pub land: u8,
    /// Last time the Sanctuary simulation ran (0 = never).
    pub last_tick: i64,
    pub total_visitors: u64,
    pub quests: crate::quests::QuestLog,
    pub visited: BTreeSet<String>,
    /// Up to three indices into `mons` that fight in battles.
    pub party: Vec<usize>,
    /// Benchmark cups won, by cup id.
    pub badges: BTreeSet<String>,
    /// Every species you have ever befriended (or evolved into): the Claudidex.
    pub befriended: BTreeSet<String>,
    pub story: crate::story::Story,
    /// Once-only easter eggs that have been found.
    pub secrets: BTreeSet<String>,
    pub settings: Settings,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub music: bool,
    pub sfx: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { music: true, sfx: true }
    }
}

impl SaveData {
    pub fn item_count(&self, id: &str) -> u32 {
        self.items.get(id).copied().unwrap_or(0)
    }
    pub fn add_item(&mut self, id: &str, n: u32) {
        *self.items.entry(id.to_string()).or_default() += n;
    }
    /// Remove one; returns false if none were owned.
    pub fn take_item(&mut self, id: &str) -> bool {
        match self.items.get_mut(id) {
            Some(n) if *n > 0 => {
                *n -= 1;
                if *n == 0 {
                    self.items.remove(id);
                }
                true
            }
            _ => false,
        }
    }
}

impl Default for SaveData {
    fn default() -> Self {
        SaveData {
            version: VERSION,
            map: "garden".into(),
            x: 6,
            y: 7,
            berries: [3, 1, 1, 1, 2],
            mons: Vec::new(),
            seen: BTreeSet::new(),
            known_fav: BTreeSet::new(),
            bushes: BTreeMap::new(),
            steps: 0,
            created: now(),
            last_seen: now(),
            tokens: 120,
            token_frac: 0.0,
            items: BTreeMap::from([("seed_sweet".to_string(), 3), ("seed_fresh".to_string(), 2)]),
            placed: Vec::new(),
            crops: BTreeMap::new(),
            land: 0,
            last_tick: 0,
            total_visitors: 0,
            quests: Default::default(),
            visited: BTreeSet::new(),
            party: Vec::new(),
            badges: BTreeSet::new(),
            befriended: BTreeSet::new(),
            story: Default::default(),
            secrets: BTreeSet::new(),
            settings: Settings::default(),
        }
    }
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn path() -> PathBuf {
    if let Ok(p) = std::env::var("CLAUDIMON_SAVE") {
        return PathBuf::from(p);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("claudimon").join("save.json")
}

pub enum Loaded {
    Fresh,
    Existing(SaveData),
    /// The file existed but could not be parsed; it was moved aside to this path.
    Corrupt(PathBuf),
}

pub fn load() -> Loaded {
    let p = path();
    let Ok(text) = fs::read_to_string(&p) else {
        return Loaded::Fresh;
    };
    match serde_json::from_str::<SaveData>(&text) {
        Ok(d) => Loaded::Existing(d),
        Err(_) => {
            // Never silently overwrite a save we failed to read: keep a backup.
            let bak = p.with_extension(format!("json.bak-{}", now()));
            let _ = fs::rename(&p, &bak);
            Loaded::Corrupt(bak)
        }
    }
}

/// Read the save without any side effects (no backups, no moves). None if missing or unreadable.
pub fn load_readonly() -> Option<SaveData> {
    serde_json::from_str(&fs::read_to_string(path()).ok()?).ok()
}

/// Atomic write: serialize to a temp file next to the save, then rename over it.
pub fn store(d: &SaveData) -> io::Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(d).map_err(io::Error::other)?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_corrupt_backup() {
        let dir = std::env::temp_dir().join(format!("claudimon-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("save.json");
        // SAFETY: single-threaded test that owns this env var.
        unsafe { std::env::set_var("CLAUDIMON_SAVE", &file) };

        assert!(matches!(load(), Loaded::Fresh));

        let mut d = SaveData::default();
        d.x = 42;
        d.map = "meadow".into();
        d.mons.push(OwnedMon { shiny: true, friendship: 7, ..OwnedMon::new("Haikitten", true) });
        d.seen.insert("Haikitten".into());
        store(&d).unwrap();
        match load() {
            Loaded::Existing(l) => assert_eq!(l, d),
            _ => panic!("expected existing save"),
        }

        // Old saves missing new fields still load thanks to serde(default).
        fs::write(&file, r#"{"x": 3, "y": 4}"#).unwrap();
        match load() {
            Loaded::Existing(l) => assert_eq!((l.x, l.y, l.map.as_str()), (3, 4, "garden")),
            _ => panic!("expected partial save to load"),
        }

        fs::write(&file, "not json {").unwrap();
        assert!(matches!(load(), Loaded::Corrupt(_)));
        assert!(!file.exists(), "corrupt save should be moved aside");
        fs::remove_dir_all(&dir).unwrap();
    }
}
