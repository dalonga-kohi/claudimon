//! Claude's daily "prompts": three small quests per day, refreshed at local midnight.

use crate::creatures::Elem;
use crate::world::MapId;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Goal {
    Befriend,
    BefriendType(String),
    PickBerries,
    Harvest,
    Feed,
    Pet,
    SellBerries,
    Visit(String),
    Curios,
    Tips,
    Gifts,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Quest {
    pub goal: Goal,
    pub need: u32,
    pub have: u32,
    pub reward: u64,
    pub claimed: bool,
}

impl Quest {
    pub fn done(&self) -> bool {
        self.have >= self.need
    }

    pub fn describe(&self) -> String {
        let n = self.need;
        let s = |word: &str| if n == 1 { word.to_string() } else { format!("{word}s") };
        match &self.goal {
            Goal::Befriend => format!("Befriend {n} {}", s("Claudimon")),
            Goal::BefriendType(t) => format!("Befriend {n} {t}-type {}", s("Claudimon")),
            Goal::PickBerries => format!("Pick {n} berries from bushes"),
            Goal::Harvest => format!("Harvest {n} {}", s("crop")),
            Goal::Feed => format!("Feed your Claudimon {n} {}", s("time")),
            Goal::Pet => format!("Give {n} head {}", s("pat")),
            Goal::SellBerries => format!("Sell {n} berries"),
            Goal::Visit(m) => format!("Explore {}", MapId::from_key(m).title()),
            Goal::Curios => format!("Collect {n} {}", s("curio")),
            Goal::Tips => format!("Receive {n} visitor {}", s("tip")),
            Goal::Gifts => format!("Collect {n} {} from happy Claudimon", s("gift")),
        }
    }
}

/// Something the player did that may advance a quest.
pub enum Event<'a> {
    Befriended(&'a [Elem]),
    PickedBerries(u32),
    Harvested,
    Fed,
    Petted,
    Sold(u32),
    Visited(MapId),
    Curio,
    Tip,
    Gift,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct QuestLog {
    pub day: String,
    pub quests: Vec<Quest>,
    pub bonus_claimed: bool,
    pub met_claude: bool,
    pub completed_total: u32,
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl QuestLog {
    /// Make sure today's quests exist; returns true if a new day started.
    pub fn refresh(&mut self, day: &str, seed: i64, owned: usize) -> bool {
        if self.day == day && !self.quests.is_empty() {
            return false;
        }
        self.day = day.to_string();
        self.quests = generate(day, seed, owned);
        self.bonus_claimed = false;
        true
    }

    /// Record an event; returns descriptions of quests that just completed.
    pub fn on(&mut self, ev: &Event) -> Vec<String> {
        let mut done = Vec::new();
        for q in self.quests.iter_mut().filter(|q| !q.done()) {
            let add = match (&q.goal, ev) {
                (Goal::Befriend, Event::Befriended(_)) => 1,
                (Goal::BefriendType(t), Event::Befriended(types)) if types.iter().any(|e| e.name() == t) => 1,
                (Goal::PickBerries, Event::PickedBerries(n)) => *n,
                (Goal::Harvest, Event::Harvested) => 1,
                (Goal::Feed, Event::Fed) => 1,
                (Goal::Pet, Event::Petted) => 1,
                (Goal::SellBerries, Event::Sold(n)) => *n,
                (Goal::Visit(m), Event::Visited(id)) if id.key() == m => 1,
                (Goal::Curios, Event::Curio) => 1,
                (Goal::Tips, Event::Tip) => 1,
                (Goal::Gifts, Event::Gift) => 1,
                _ => 0,
            };
            if add > 0 {
                q.have = (q.have + add).min(q.need);
                if q.done() {
                    done.push(q.describe());
                }
            }
        }
        done
    }

    pub fn all_claimed(&self) -> bool {
        !self.quests.is_empty() && self.quests.iter().all(|q| q.claimed)
    }
}

/// Three distinct quests for a day, deterministic per player and date.
pub fn generate(day: &str, seed: i64, owned: usize) -> Vec<Quest> {
    let day_num: u64 = day.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211));
    let mut rng = StdRng::seed_from_u64(day_num ^ seed as u64);
    let types = ["Leaf", "Ember", "Tide", "Spark", "Stone", "Byte", "Dream", "Glitch"];
    let places = ["meadow", "woods", "shore", "caves"];
    // Care quests only make sense once you have creatures to care for.
    let has_mons = owned > 0;
    let mut pool: Vec<(Goal, u32, u64)> = vec![
        (Goal::Befriend, rng.gen_range(1..=3), 70),
        (Goal::BefriendType(types[rng.gen_range(0..types.len())].into()), 1, 110),
        (Goal::PickBerries, rng.gen_range(4..=10), 12),
        (Goal::Harvest, rng.gen_range(2..=5), 30),
        (Goal::SellBerries, rng.gen_range(8..=20), 6),
        (Goal::Visit(places[rng.gen_range(0..places.len())].into()), 1, 80),
        (Goal::Curios, rng.gen_range(3..=6), 25),
    ];
    if has_mons {
        pool.push((Goal::Feed, rng.gen_range(2..=4), 30));
        pool.push((Goal::Pet, rng.gen_range(2..=4), 25));
        pool.push((Goal::Tips, rng.gen_range(2..=5), 25));
        pool.push((Goal::Gifts, rng.gen_range(1..=2), 70));
    }
    let mut out = Vec::new();
    while out.len() < 3 && !pool.is_empty() {
        let (goal, need, per) = pool.swap_remove(rng.gen_range(0..pool.len()));
        out.push(Quest { goal, need, have: 0, reward: (per * need as u64).max(50).min(260), claimed: false });
    }
    out
}

/// Reward for finishing all three of the day's quests.
pub const BONUS_TOKENS: u64 = 150;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quests_are_daily_and_deterministic() {
        let a = generate("2026-10-14", 42, 3);
        let b = generate("2026-10-14", 42, 3);
        let c = generate("2026-10-15", 42, 3);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 3);
        for q in &a {
            assert!((50..=260).contains(&q.reward));
        }
        // Distinct goal kinds.
        let kinds: std::collections::HashSet<_> = a.iter().map(|q| std::mem::discriminant(&q.goal)).collect();
        assert_eq!(kinds.len(), 3);
    }

    #[test]
    fn new_players_get_no_care_quests() {
        for day in 1..=40 {
            for q in generate(&format!("2026-11-{day:02}"), 7, 0) {
                assert!(!matches!(q.goal, Goal::Feed | Goal::Pet | Goal::Tips | Goal::Gifts), "{:?}", q.goal);
            }
        }
    }

    #[test]
    fn events_advance_matching_quests() {
        let mut log = QuestLog::default();
        log.quests = vec![
            Quest { goal: Goal::BefriendType("Tide".into()), need: 1, have: 0, reward: 100, claimed: false },
            Quest { goal: Goal::PickBerries, need: 5, have: 0, reward: 60, claimed: false },
            Quest { goal: Goal::Visit("shore".into()), need: 1, have: 0, reward: 80, claimed: false },
        ];
        assert!(log.on(&Event::Befriended(&[Elem::Leaf])).is_empty());
        assert_eq!(log.on(&Event::Befriended(&[Elem::Tide, Elem::Byte])).len(), 1);
        assert!(log.on(&Event::PickedBerries(3)).is_empty());
        assert_eq!(log.on(&Event::PickedBerries(4)).len(), 1);
        assert_eq!(log.quests[1].have, 5, "progress is capped at the goal");
        assert!(log.on(&Event::Visited(MapId::Woods)).is_empty());
        assert_eq!(log.on(&Event::Visited(MapId::Shore)).len(), 1);
    }

    #[test]
    fn refresh_only_on_a_new_day() {
        let mut log = QuestLog::default();
        assert!(log.refresh("2026-10-14", 1, 2));
        log.quests[0].have = 1;
        assert!(!log.refresh("2026-10-14", 1, 2));
        assert_eq!(log.quests[0].have, 1);
        assert!(log.refresh("2026-10-15", 1, 2));
        assert_eq!(log.quests.iter().map(|q| q.have).sum::<u32>(), 0);
    }
}
