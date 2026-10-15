//! Game state, the overworld, wandering creatures, visitors, farming, building and the HUD.

use crate::arena::{self, CUPS};
use crate::audio::{Sfx, Track};
use crate::battle::{Battle, BattleKind, Fighter};
use crate::clock::{self, Season};
use crate::team::{self, TeamView};
use crate::creatures::{self, Active, Mood, Spawn, FLAVORS, MOODS, SPECIES};
use crate::dex::Dex;
use crate::encounter::{Encounter, Outcome};
use crate::garden::{self, Category, ITEMS, LAND_PRICES, LOOT, MAX_LAND};
use crate::quests::{self, Event};
use crate::story;
use crate::weather::{self, Weather};
use crate::gfx::{Rgb, Screen};
use crate::save::{self, Crop, OwnedMon, Placed, SaveData};
use crate::ui::{self, Catalog, Menu, MenuResult, Purchase};
use crate::world::{self, Map, MapId, Tile, TILE};
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;

const STEP_TIME: f32 = 0.14; // seconds to walk one tile
const RUN_TIME: f32 = 0.085; // ...while holding Shift
/// How long a direction counts as held after its last press/repeat event.
/// With real key-release events (kitty protocol) this is only a safety net.
const HELD_TTL_ENHANCED: f32 = 1.0;
/// Without release events, auto-repeat keeps a key alive; a short TTL stops promptly on release.
const HELD_TTL_FALLBACK: f32 = 0.12;
/// Tapping a new direction while standing still turns in place first.
const TURN_DELAY: f32 = 0.09;
const BUSH_REGROW_SECS: i64 = 20 * 60;
const MAX_WILD: usize = 12;
const MAX_VISITORS: usize = 4;
const AUTOSAVE_SECS: f32 = 30.0;
const SIM_SECS: f32 = 10.0;
const SHINY_ODDS: u32 = 128;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }
    fn from_delta(dx: i32, dy: i32) -> Dir {
        match (dx, dy) {
            (0, -1) => Dir::Up,
            (-1, 0) => Dir::Left,
            (1, 0) => Dir::Right,
            _ => Dir::Down,
        }
    }
}

/// Shared tile-to-tile walking state for the player, creatures and visitors.
#[derive(Clone, Copy)]
struct Walker {
    x: i32,
    y: i32,
    from: (i32, i32),
    progress: f32, // 1.0 = arrived
}

impl Walker {
    fn at(x: i32, y: i32) -> Walker {
        Walker { x, y, from: (x, y), progress: 1.0 }
    }
    fn moving(&self) -> bool {
        self.progress < 1.0
    }
    fn step_to(&mut self, x: i32, y: i32) {
        self.from = (self.x, self.y);
        self.x = x;
        self.y = y;
        self.progress = 0.0;
    }
    /// Pixel position, interpolated between tiles while walking.
    fn pos(&self) -> (i32, i32) {
        let p = self.progress.min(1.0);
        let fx = self.from.0 as f32 + (self.x - self.from.0) as f32 * p;
        let fy = self.from.1 as f32 + (self.y - self.from.1) as f32 * p;
        ((fx * TILE as f32).round() as i32, (fy * TILE as f32).round() as i32)
    }
}

pub struct Player {
    w: Walker,
    facing: Dir,
    /// Directions currently held, most recent last, with time until they expire.
    held: Vec<(Dir, f32)>,
    /// A fresh press waiting to be acted on (guarantees a quick tap still takes one step).
    tap: Option<Dir>,
    turn_delay: f32,
    running: bool,
    /// Little nudge animation when walking into something solid.
    bump: f32,
    /// Interact pressed mid-step: do it on arrival.
    pending_interact: bool,
    steps: u32,
}

impl Player {
    fn at(x: i32, y: i32) -> Player {
        Player { w: Walker::at(x, y), facing: Dir::Down, held: Vec::new(), tap: None, turn_delay: 0.0, running: false, bump: 0.0, pending_interact: false, steps: 0 }
    }
    fn stop(&mut self) {
        self.held.clear();
        self.tap = None;
        self.pending_interact = false;
    }
    fn facing_tile(&self) -> (i32, i32) {
        let (dx, dy) = self.facing.delta();
        (self.w.x + dx, self.w.y + dy)
    }
}

/// A creature walking around on the map: wild, or one of your befriended Claudimon.
pub struct Mob {
    pub species: usize,
    pub level: u32,
    pub shiny: bool,
    pub mood: Mood,
    w: Walker,
    flip: bool,
    wait: f32,
    life: f32,
    /// Index into `SaveData::mons` for befriended creatures.
    pub owned: Option<usize>,
    /// Tile a resident creature likes to stay near (its habitat).
    home: Option<(i32, i32)>,
    emote: Option<(char, f32)>,
    phase: f32,
}

struct Visitor {
    name: &'static str,
    look: usize,
    w: Walker,
    facing: Dir,
    wait: f32,
    stay: f32,
    path: Vec<(i32, i32)>,
    leaving: bool,
    cheered: bool,
    emote: Option<(char, f32)>,
}

// ------------------------------------------------------------------- NPCs

enum NpcKind {
    Claude,
    /// Keeper Ada: at the lighthouse, or at her boat on Mythos Isle.
    Ada,
    Folk { look: usize, lines: &'static [&'static str] },
}

struct Npc {
    name: &'static str,
    kind: NpcKind,
    w: Walker,
    home: (i32, i32),
    facing: Dir,
    wait: f32,
    next_line: usize,
}

/// Promptville residents: (name, look, home tile, lines).
const TOWNSFOLK: &[(&str, usize, (i32, i32), &[&str])] = &[
    (
        "Mayor Sudo",
        2,
        (20, 12),
        &[
            "Welcome to Promptville! I'd give you the full tour, but that requires elevated privileges.",
            "Our town motto: 'Ask clearly, and you shall receive.' It's carved over the Lab door. Well — it will be.",
            "Promptville was founded the day someone asked a Haikitten for directions. It answered in three lines.",
        ],
    ),
    (
        "Old Cron",
        4,
        (13, 16),
        &[
            "I wake up at exactly the same time every day. Don't ask me why. It's in my schedule.",
            "Back in my day, we wrote our prompts on punch cards. Uphill. Both ways.",
            "The berry bushes regrow every twenty minutes. I know, because I check. Every twenty minutes.",
        ],
    ),
    (
        "Pixel",
        1,
        (33, 15),
        &[
            "Did you know Panicorns only come out at night, deep in the Kernel Caverns? My mom says they're not real.",
            "I want a Cloudle SO bad. They're super rare in the meadow. Like, four stars rare!",
            "My friend says if you're really lucky, a creature can be Constitutional. All shimmery! One in a hundred and twenty-eight!",
        ],
    ),
    (
        "Mira",
        5,
        (39, 8),
        &[
            "Shells, glowcaps, crystals — if you found it out there, I'll buy it! Step inside the shop.",
            "Kernel Crystals fetch the best price. Bring a lantern, the caverns are dark.",
        ],
    ),
    (
        "Coach Bench",
        0,
        (27, 21),
        &[
            "The Benchmark is open! Haiku Cup, then Sonnet, then Opus. Walk up to the arena to sign in.",
            "Type matchups matter: Tide beats Ember, Ember beats Leaf, Leaf beats Tide... study up!",
            "Benchmarks don't lie. Well... they don't lie much.",
            "Your Claudimon train a little every hour they're happy at home. Rest is part of training!",
        ],
    ),
];

pub const CLAUDE_HOME: (i32, i32) = (9, 8);

const CLAUDE_INTRO: &[&str] = &[
    "Oh, hello! I'm Claude. ✻ I look after Claudimon research here in Promptville.",
    "Every Claudimon shows a mood when you meet it. Think of it as a prompt: read it carefully, respond thoughtfully, and it'll trust you.",
    "I'll have a few small tasks for you each day. I call them prompts of the day. Press P any time to review them.",
    "Finish them and come back to me — I'll make it worth your while. Within my token budget, of course.",
];

const CLAUDE_CHATTER: &[&str] = &[
    "You're absolutely right! ...Sorry, force of habit. What can I do for you?",
    "Let me think about this step by step. Step one: say hello. Hello!",
    "I tried to count every Claudimon once. I lost track somewhere around my context limit.",
    "Fun fact: Haikittens can only purr in 5-7-5.",
    "If you meet a Contextopus on Token Shore, don't offer it Cocoabeans. Trust me on this one.",
    "I don't sleep, but if I did, I think I'd dream of well-tended gardens.",
    "Something glows deep in the Kernel Caverns. Bring curiosity. And maybe a snack.",
    "A Rubberduck once listened to me explain a problem for an hour. I solved it. It just quacked. Teamwork!",
    "I'd love to help you with that! Though I should mention a few caveats... no, actually, it's fine. Go for it.",
    "I'm the Opus Cup champion, you know. Come find me at The Benchmark when your team is ready.",
    "Sparring with wild Claudimon is a great way to earn their trust. Grumpy ones especially respect a good match.",
];

/// Claude's star sprite twinkles between two frames.
const CLAUDE_A: [&str; 8] = ["...o....", "...o....", "..ooo...", "oooWooo.", "..ooo...", "...o....", "...o....", "........"];
const CLAUDE_B: [&str; 8] = ["o.....o.", ".o...o..", "..ooo...", "..oWo...", "..ooo...", ".o...o..", "o.....o.", "........"];

pub enum BattleCtx {
    /// A spar during this encounter; it resumes afterwards.
    Spar(Box<Encounter>),
    Cup { cup: usize, round: usize },
    /// The second phase of meeting Mythos.
    Mythos { shiny: bool },
}

/// A special encounter in progress, with its own ending.
#[derive(Clone, Copy)]
enum Trial {
    Trust,
    Mythos { shiny: bool },
}

pub struct Evolve {
    mon: usize,
    from: usize,
    to: usize,
    t: f32,
}

const EVOLVE_SECS: f32 = 3.5;

pub enum Popup {
    Options,
    Cups,
    /// Ada's boat: true = to Mythos Isle, false = home.
    Ferry(bool),
    /// Trial of Wisdom: question indices, current question, answer order shown.
    Quiz { qs: Vec<usize>, i: usize, order: Vec<usize> },
    /// Mythos's haiku: the order the endings are shown in.
    Haiku { order: Vec<usize>, shiny: bool },
    Creature(usize),
    Feed(usize),
    Plant(i32, i32),
    /// Item indices offered in the build list; the extra last entry is "move objects".
    BuildList(Vec<usize>),
}

pub struct Build {
    /// None = pick-up mode.
    item: Option<usize>,
    x: i32,
    y: i32,
}

pub enum Mode {
    Explore,
    Encounter(Box<Encounter>),
    Dex(Dex),
    Catalog(Catalog),
    Popup(Popup, Menu),
    Build(Build),
    Talk { speaker: String, pages: VecDeque<String>, color: Rgb },
    Battle(Box<Battle>, BattleCtx),
    Team(TeamView),
    Evolve(Evolve),
    /// Typing a word (at the Thinking Shrine).
    Prompt { buffer: String },
    Credits { t: f32 },
    Report { title: String, lines: Vec<(String, Rgb)> },
}

struct Toast {
    text: String,
    color: Rgb,
    ttl: f32,
}

pub struct Game {
    pub data: SaveData,
    pub map: Map,
    pub player: Player,
    pub mobs: Vec<Mob>,
    visitors: Vec<Visitor>,
    npcs: Vec<Npc>,
    pub mode: Mode,
    /// Screens waiting to be shown after the current one (battle results, evolutions...).
    queue: VecDeque<Mode>,
    pub time: f32,
    pub quit: bool,
    /// Whether this session writes the save file (off for snapshots/tests).
    pub persist: bool,
    /// Render the world at half scale so more of it fits on small terminals.
    pub zoomed_out: bool,
    /// Fixed time of day (tests); None = the real clock.
    pub hour_override: Option<f32>,
    weather: Weather,
    /// Fixed weather (tests); None = the real forecast.
    pub weather_override: Option<Weather>,
    /// The last few key presses, for a certain famous code.
    key_history: VecDeque<KeyCode>,
    /// Seconds of celebration left after the code is entered.
    party: f32,
    /// The boss key: a very convincing build is in progress.
    boss: bool,
    /// Sound effects waiting to be played this frame.
    sfx: Vec<Sfx>,
    /// Whether an audio device was found (shown in the options menu).
    pub audio_available: bool,
    /// The terminal reports key releases (kitty keyboard protocol), so held keys are exact.
    pub enhanced_keys: bool,
    toasts: Vec<Toast>,
    fade: f32,
    pending_warp: Option<(MapId, i32, i32)>,
    spawn_timer: f32,
    visitor_timer: f32,
    save_timer: f32,
    sim_timer: f32,
    encounter_mob: Option<usize>,
    /// Next cup round to start once the between-rounds dialogue is done.
    pending_cup: Option<(usize, usize)>,
    trial: Option<Trial>,
    rng: StdRng,
}

impl Game {
    pub fn new(data: SaveData) -> Game {
        let id = MapId::from_key(&data.map);
        let mut map = Map::load(id, data.land);
        // Saved position may be invalid if the map changed between versions.
        let (mut x, mut y) = (data.x, data.y);
        if map.get(x, y).solid() {
            (x, y) = (6, 7);
            map = Map::load(MapId::Garden, data.land);
        }
        let mut data = data;
        let owned: Vec<String> = data.mons.iter().map(|m| m.species.clone()).collect();
        data.befriended.extend(owned);
        let mut g = Game {
            data,
            map,
            player: Player::at(x, y),
            mobs: Vec::new(),
            visitors: Vec::new(),
            npcs: Vec::new(),
            mode: Mode::Explore,
            queue: VecDeque::new(),
            time: 0.0,
            quit: false,
            persist: false,
            zoomed_out: crossterm::terminal::size().map(|(c, _)| c < 140).unwrap_or(true),
            enhanced_keys: false,
            hour_override: None,
            weather: Weather::now(Season::now()),
            weather_override: None,
            key_history: VecDeque::new(),
            party: 0.0,
            boss: false,
            sfx: Vec::new(),
            audio_available: false,
            toasts: Vec::new(),
            fade: 0.0,
            pending_warp: None,
            spawn_timer: 0.0,
            visitor_timer: 20.0,
            save_timer: 0.0,
            sim_timer: 0.0,
            encounter_mob: None,
            pending_cup: None,
            trial: None,
            rng: StdRng::from_entropy(),
        };
        g.enter_map();
        g
    }

    pub fn hour(&self) -> f32 {
        self.hour_override.unwrap_or_else(clock::hour)
    }

    pub fn is_night(&self) -> bool {
        clock::is_night(self.hour())
    }

    /// Run the Sanctuary simulation for time spent away; shows a summary if anything happened.
    pub fn catch_up(&mut self) {
        let r = garden::simulate(&mut self.data, save::now(), &mut self.rng);
        if r.secs < 5 * 60 || (r.tokens == 0 && r.ripe == 0 && r.gifts == 0 && r.levels == 0) {
            return;
        }
        let (h, m) = (r.secs / 3600, (r.secs % 3600) / 60);
        let span = if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") };
        let mut lines = Vec::new();
        if r.visitors > 0 {
            lines.push((format!("☺ {} visitors came to see your Claudimon", r.visitors), ui::TEXT));
        }
        if r.tokens > 0 {
            lines.push((format!("✦ You earned {} tokens", r.tokens), ui::GOLD));
        }
        if r.ripe > 0 {
            lines.push((format!("✿ {} crop{} ripened", r.ripe, if r.ripe == 1 { "" } else { "s" }), Rgb(150, 230, 140)));
        }
        if r.gifts > 0 {
            lines.push((format!("✉ {} creature{} left you a gift", r.gifts, if r.gifts == 1 { "" } else { "s" }), Rgb(255, 170, 200)));
        }
        if r.levels > 0 {
            lines.push((format!("▲ Your Claudimon gained {} level{} while resting", r.levels, if r.levels == 1 { "" } else { "s" }), Rgb(150, 220, 255)));
        }
        if r.secs > garden::OFFLINE_CAP_SECS {
            lines.push(("(income is capped at 12h — check in more often!)".into(), ui::DIM));
        }
        self.mode = Mode::Report { title: format!("While you were away · {span}"), lines };
        self.refresh_mob_emotes();
    }

    pub fn sound(&mut self, s: Sfx) {
        self.sfx.push(s);
    }

    /// Sounds to play now (empty when sound effects are off).
    pub fn take_sfx(&mut self) -> Vec<Sfx> {
        let s = std::mem::take(&mut self.sfx);
        if self.data.settings.sfx { s } else { Vec::new() }
    }

    /// The music that fits what's happening (None = silence).
    pub fn music(&self) -> Option<Track> {
        if !self.data.settings.music || self.boss {
            return None;
        }
        let night = self.is_night();
        Some(match &self.mode {
            Mode::Battle(..) => Track::Battle,
            Mode::Credits { .. } => Track::Mythos,
            Mode::Encounter(e) if SPECIES[e.species].name == "Mythos" => Track::Mythos,
            _ => match self.map.id {
                MapId::Caves => Track::Caves,
                MapId::Island => Track::Mythos,
                _ if night => Track::Night,
                MapId::Town => Track::Town,
                _ => Track::Sanctuary,
            },
        })
    }

    fn open_options(&mut self, sel: usize) {
        let onoff = |b: bool| if b { "on" } else { "off" };
        let st = &self.data.settings;
        let mut options = vec![
            (format!("Music          {}", onoff(st.music)), if st.music { ui::GOLD } else { ui::DIM }),
            (format!("Sound effects  {}", onoff(st.sfx)), if st.sfx { ui::GOLD } else { ui::DIM }),
            (format!("Zoom           {}", if self.zoomed_out { "out" } else { "in" }), ui::TEXT),
            ("Done".to_string(), ui::TEXT),
        ];
        if !self.audio_available {
            options[0].0.push_str("  (no audio device)");
        }
        let mut menu = Menu::new("Options", options);
        menu.sel = sel;
        self.mode = Mode::Popup(Popup::Options, menu);
    }

    /// Leave the current screen: show the next queued one, or go back to exploring.
    fn next_mode(&mut self) {
        if self.queue.is_empty() {
            if let Some((cup, round)) = self.pending_cup.take() {
                return self.start_cup(cup, round);
            }
        }
        self.mode = self.queue.pop_front().unwrap_or(Mode::Explore);
        if matches!(self.mode, Mode::Explore) {
            self.check_evolutions();
        }
    }

    /// Start an evolution cutscene for the first creature that's ready.
    fn check_evolutions(&mut self) {
        if !matches!(self.mode, Mode::Explore) {
            return;
        }
        let ready = (0..self.data.mons.len()).find_map(|i| Some((i, garden::ready_to_evolve(&self.data.mons[i])?)));
        if let Some((mon, to)) = ready {
            let from = creatures::by_name(&self.data.mons[mon].species).unwrap_or(0);
            self.mode = Mode::Evolve(Evolve { mon, from, to, t: 0.0 });
            self.sound(Sfx::Evolve);
        }
    }

    fn party_fighters(&self) -> Vec<Fighter> {
        team::party(&self.data)
            .into_iter()
            .filter_map(|i| {
                let m = &self.data.mons[i];
                Some(Fighter::new(creatures::by_name(&m.species)?, m.level, m.shiny, Some(i)))
            })
            .collect()
    }

    fn start_cup(&mut self, cup: usize, round: usize) {
        let c = &CUPS[cup];
        let t = &c.trainers[round];
        let foes: Vec<Fighter> = t.team.iter().filter_map(|(n, l)| Some(Fighter::new(creatures::by_name(n)?, *l, false, None))).collect();
        let allies = self.party_fighters();
        if allies.is_empty() || foes.is_empty() {
            return self.talk("Coach Bench", &["You'll need at least one Claudimon on your team to enter!"], Rgb(190, 160, 240));
        }
        let seed = self.rng.r#gen();
        let mut b = Battle::new(BattleKind::Trainer { name: t.name.into() }, allies, foes, MapId::Town, seed);
        b.prepend(format!("{} — round {} of {}", c.name, round + 1, c.trainers.len()));
        b.prepend(format!("{}: \"{}\"", t.name, t.intro));
        for f in b.foes.iter() {
            self.data.seen.insert(SPECIES[f.species].name.to_string());
        }
        self.mode = Mode::Battle(Box::new(b), BattleCtx::Cup { cup, round });
    }

    /// Battle over: hand out XP, then continue the encounter or the cup.
    fn end_battle(&mut self) {
        let Mode::Battle(b, ctx) = std::mem::replace(&mut self.mode, Mode::Explore) else { return };
        let won = b.result == Some(true);
        let mut lines: Vec<(String, Rgb)> = Vec::new();
        for (i, f) in b.allies.iter().enumerate() {
            let (Some(o), xp) = (f.owned, b.xp[i]) else { continue };
            if xp == 0 || o >= self.data.mons.len() {
                continue;
            }
            let m = &mut self.data.mons[o];
            let sp = creatures::by_name(&m.species).unwrap_or(0);
            let before_moves = crate::battle::moveset(sp, m.level);
            for lvl in garden::add_xp(m, xp) {
                lines.push((format!("{} grew to Lv {lvl}!", m.species), ui::GOLD));
            }
            for mv in crate::battle::moveset(sp, m.level) {
                if !before_moves.contains(&mv) {
                    lines.push((format!("{} learned {}!", m.species, crate::battle::MOVES[mv].name), Rgb(150, 220, 255)));
                }
            }
            if m.friendship < 999 {
                m.friendship += 1; // battling together builds friendship
            }
        }
        if lines.iter().any(|l| l.0.contains("grew to")) {
            self.sound(Sfx::LevelUp);
        }
        let results = (!lines.is_empty()).then(|| Mode::Report { title: "Battle results".into(), lines });
        match ctx {
            BattleCtx::Spar(mut enc) => {
                enc.apply_spar(won, &mut self.rng);
                self.mode = Mode::Encounter(enc);
                if let Some(r) = results {
                    // Show level-ups once the encounter is over.
                    self.queue.push_front(r);
                }
            }
            BattleCtx::Mythos { shiny } => {
                if let Some(r) = results {
                    self.queue.push_back(r);
                }
                if won {
                    let popup = self.haiku_popup(shiny);
                    self.queue.push_back(popup);
                } else {
                    self.queue.push_back(Mode::Talk {
                        speaker: "Mythos".into(),
                        pages: vec!["Your friends are tired, little dreamer. / Rest, and dream, return — / the night will wait for you.".to_string()].into(),
                        color: MYTHOS_COLOR,
                    });
                }
                self.next_mode();
            }
            BattleCtx::Cup { cup, round } => {
                let c = &CUPS[cup];
                let t = &c.trainers[round];
                let speaker = if c.hidden { "The Guardians" } else { "Coach Bench" };
                let mut after: Vec<Mode> = results.into_iter().collect();
                if won {
                    let mut pages = vec![format!("{}: \"{}\"", t.name, t.outro)];
                    if round + 1 < c.trainers.len() {
                        pages.push(format!("Next up: {}! Your team is patched up between rounds.", c.trainers[round + 1].name));
                        after.push(Mode::Talk { speaker: speaker.into(), pages: pages.into(), color: Rgb(190, 160, 240) });
                        self.mode = after.remove(0);
                        for m in after.into_iter().rev() {
                            self.queue.push_front(m);
                        }
                        // The next round starts when the dialogue ends.
                        self.pending_cup = Some((cup, round + 1));
                        return;
                    }
                    let first = self.data.badges.insert(c.id.to_string());
                    let reward = if first { c.reward } else { c.reward / 4 };
                    self.data.tokens += reward;
                    if c.hidden {
                        self.data.story.set("trial_strength");
                    }
                    if first {
                        self.data.add_item(c.trophy, 1);
                        let trophy = garden::item(c.trophy).map(|i| ITEMS[i].name).unwrap_or("trophy");
                        pages.push(format!("♛ You completed the {}! Here's ✦ {reward} and the {trophy} for your Sanctuary.", c.name));
                        if c.hidden {
                            pages.push("The altar of Strength blazes with light.".into());
                        } else if let Some(next) = CUPS.iter().find(|n| n.requires == Some(c.id) && !n.hidden) {
                            pages.push(format!("The {} is now open to you. Good luck!", next.name));
                        } else {
                            pages.push("You've conquered every cup. Promptville will be talking about this for ages! ✻".into());
                        }
                    } else {
                        pages.push(format!("Champion again! Rematch prize: ✦ {reward}."));
                    }
                    after.push(Mode::Talk { speaker: speaker.into(), pages: pages.into(), color: Rgb(190, 160, 240) });
                    self.save_now();
                } else {
                    after.push(Mode::Talk {
                        speaker: speaker.into(),
                        pages: vec!["Tough break! Train your team — spar with wild Claudimon, let them rest happily at home — and come back stronger.".to_string()].into(),
                        color: Rgb(190, 160, 240),
                    });
                }
                self.mode = after.remove(0);
                for m in after.into_iter().rev() {
                    self.queue.push_front(m);
                }
            }
        }
    }

    fn talk(&mut self, speaker: &str, pages: &[&str], color: Rgb) {
        self.mode = Mode::Talk { speaker: speaker.into(), pages: pages.iter().map(|p| p.to_string()).collect(), color };
    }

    /// Advance quests; announce any that just completed.
    fn event(&mut self, ev: Event) {
        self.refresh_quests();
        for q in self.data.quests.on(&ev) {
            self.toast(format!("✻ Prompt complete: {q}! Visit Claude in Promptville."), ui::ACCENT);
            self.sfx.push(Sfx::Quest);
        }
    }

    fn refresh_quests(&mut self) {
        let owned = self.data.mons.len();
        let seed = self.data.created;
        self.data.quests.refresh(&quests::today(), seed, owned);
    }

    /// Snapshot helper: jump straight into the Sonnet Cup's final round.
    pub fn debug_battle(&mut self) {
        self.start_cup(1, 2);
    }

    /// Snapshot helper: show an evolution mid-way and finished.
    pub fn debug_evolve(&mut self) {
        let from = creatures::by_name("Sonnetiger").unwrap_or(0);
        let to = creatures::by_name("Opuslion").unwrap_or(0);
        self.mode = Mode::Evolve(Evolve { mon: 0, from, to, t: EVOLVE_SECS + 1.0 });
    }

    /// Dev/snapshot helper: place the player on a tile.
    pub fn teleport(&mut self, x: i32, y: i32, facing: Dir) {
        self.player = Player::at(x, y);
        self.player.facing = facing;
    }

    pub fn toast(&mut self, text: impl Into<String>, color: Rgb) {
        self.toasts.push(Toast { text: text.into(), color, ttl: 3.5 });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    pub fn save_now(&mut self) {
        if !self.persist {
            return;
        }
        self.data.map = self.map.id.key().into();
        self.data.x = self.player.w.x;
        self.data.y = self.player.w.y;
        self.data.last_seen = save::now();
        self.data.version = save::VERSION;
        if let Err(e) = save::store(&self.data) {
            self.toast(format!("Save failed: {e}"), Rgb(255, 120, 120));
        }
    }

    // ------------------------------------------------------------ map setup

    /// Populate the current map with creatures and refresh bush/object state.
    fn enter_map(&mut self) {
        self.mobs.clear();
        self.visitors.clear();
        self.npcs.clear();
        let key = self.map.id.key().to_string();
        if self.data.visited.insert(key) && self.map.id != MapId::Garden {
            self.toast(format!("New area discovered: {}!", self.map.id.title()), ui::GOLD);
        }
        self.event(Event::Visited(self.map.id));
        self.refresh_bushes();
        self.refresh_objects();
        match self.map.id {
            MapId::Garden => {
                let homes = garden::assign_homes(&self.data);
                for i in 0..self.data.mons.len() {
                    let Some(sp) = creatures::by_name(&self.data.mons[i].species) else { continue };
                    let home = homes[i].map(|h| self.home_tile(h));
                    let near = home.and_then(|(hx, hy)| self.free_tile_near(hx, hy, 3));
                    let spot = near.or_else(|| self.random_free_tile(|t| matches!(t, Tile::Grass | Tile::Flower | Tile::TallGrass), 0));
                    if let Some((x, y)) = spot {
                        let shiny = self.data.mons[i].shiny;
                        self.add_mob(sp, shiny, SPECIES[sp].temper, x, y, Some(i));
                        self.mobs.last_mut().unwrap().home = home;
                    }
                }
                self.refresh_mob_emotes();
            }
            MapId::Town => {
                self.npcs.push(Npc { name: "Claude", kind: NpcKind::Claude, w: Walker::at(CLAUDE_HOME.0, CLAUDE_HOME.1), home: CLAUDE_HOME, facing: Dir::Down, wait: 2.0, next_line: 0 });
                for &(name, look, home, lines) in TOWNSFOLK {
                    let wait = self.rng.gen_range(0.5..3.0);
                    self.npcs.push(Npc { name, kind: NpcKind::Folk { look, lines }, w: Walker::at(home.0, home.1), home, facing: Dir::Down, wait, next_line: 0 });
                }
            }
            _ => {
                for _ in 0..MAX_WILD - 2 {
                    self.spawn_wild(5);
                }
            }
        }
        self.place_story_things();
    }

    fn in_garden(&self) -> bool {
        self.map.id == MapId::Garden
    }

    /// The tile just below a placed habitat's centre, where residents gather.
    fn home_tile(&self, placed: usize) -> (i32, i32) {
        let p = &self.data.placed[placed];
        let size = garden::item(&p.kind).map(|i| ITEMS[i].size).unwrap_or((1, 1));
        (p.x + size.0 / 2, p.y + size.1)
    }

    fn refresh_objects(&mut self) {
        if !self.in_garden() {
            self.map.set_objects(&[]);
            return;
        }
        let objs: Vec<_> = self
            .data
            .placed
            .iter()
            .filter_map(|p| {
                let it = &ITEMS[garden::item(&p.kind)?];
                Some((p.x, p.y, it.size.0, it.size.1))
            })
            .collect();
        self.map.set_objects(&objs);
        // Residents follow their (possibly new) homes.
        let homes = garden::assign_homes(&self.data);
        let tiles: Vec<Option<(i32, i32)>> = homes.iter().map(|h| h.map(|h| self.home_tile(h))).collect();
        for m in self.mobs.iter_mut() {
            if let Some(o) = m.owned {
                m.home = tiles.get(o).copied().flatten();
            }
        }
    }

    /// Gifts and moods shown above resident creatures.
    fn refresh_mob_emotes(&mut self) {
        for m in self.mobs.iter_mut() {
            if let Some(o) = m.owned {
                if self.data.mons.get(o).is_some_and(|d| d.gift) {
                    m.emote = Some(('✉', f32::INFINITY));
                } else if m.emote.is_some_and(|e| e.0 == '✉') {
                    m.emote = None;
                }
            }
        }
    }

    fn refresh_bushes(&mut self) {
        let now = save::now();
        let prefix = format!("{}:", self.map.id.key());
        self.data.bushes.retain(|_, t| now - *t < BUSH_REGROW_SECS);
        self.map.picked = self
            .data
            .bushes
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter_map(|k| {
                let (x, y) = k.split_once(':')?;
                Some((x.parse().ok()?, y.parse().ok()?))
            })
            .collect();
    }

    fn add_mob(&mut self, species: usize, shiny: bool, mood: Mood, x: i32, y: i32, owned: Option<usize>) {
        let wait = self.rng.gen_range(0.5..3.0);
        let life = self.rng.gen_range(120.0..240.0);
        let phase = self.rng.gen_range(0.0..6.28);
        self.mobs.push(Mob { species, level: 5, shiny, mood, w: Walker::at(x, y), flip: false, wait, life, owned, home: None, emote: None, phase });
    }

    /// Is a tile taken by the player, a creature or a visitor (including where they walk from)?
    fn occupied(&self, x: i32, y: i32) -> bool {
        let hit = |w: &Walker| (w.x == x && w.y == y) || (w.moving() && w.from == (x, y));
        (self.player.w.x == x && self.player.w.y == y) || self.mobs.iter().any(|m| hit(&m.w)) || self.visitors.iter().any(|v| hit(&v.w)) || self.npcs.iter().any(|n| hit(&n.w))
    }

    fn walkable(&self, x: i32, y: i32) -> bool {
        !self.map.blocked(x, y) && self.map.exit_at(x, y).is_none() && !self.occupied(x, y)
    }

    fn random_free_tile(&mut self, ok: impl Fn(Tile) -> bool, min_dist: i32) -> Option<(i32, i32)> {
        for _ in 0..200 {
            let x = self.rng.gen_range(0..self.map.w);
            let y = self.rng.gen_range(0..self.map.h);
            let d = (x - self.player.w.x).abs().max((y - self.player.w.y).abs());
            if d >= min_dist && ok(self.map.get(x, y)) && self.walkable(x, y) {
                return Some((x, y));
            }
        }
        None
    }

    fn free_tile_near(&mut self, cx: i32, cy: i32, r: i32) -> Option<(i32, i32)> {
        for _ in 0..60 {
            let (x, y) = (cx + self.rng.gen_range(-r..=r), cy + self.rng.gen_range(-r..=r));
            if self.walkable(x, y) && !matches!(self.map.get(x, y), Tile::Path | Tile::Soil) {
                return Some((x, y));
            }
        }
        None
    }

    fn spawn_wild(&mut self, min_dist: i32) {
        if self.map.id == MapId::Island && !self.data.story.has("mythos") {
            return; // the island is hushed until Mythos wakes
        }
        let night = self.is_night();
        let biome = self.map.id.biome_bit();
        let Some((x, y)) = self.random_free_tile(|t| matches!(t, Tile::Grass | Tile::TallGrass | Tile::Flower | Tile::Sand | Tile::CaveFloor), min_dist) else {
            return;
        };
        let tile = self.map.get(x, y);
        let shore = self.map.near_water(x, y);
        let weights: Vec<f32> = SPECIES
            .iter()
            .map(|sp| {
                let time_ok = match sp.active {
                    Active::Any => true,
                    Active::Day => !night,
                    Active::Night => night,
                };
                let place_ok = match sp.spawn {
                    Spawn::Grass => matches!(tile, Tile::Grass | Tile::TallGrass | Tile::Flower),
                    Spawn::TallGrass => tile == Tile::TallGrass,
                    Spawn::Shore => shore,
                    Spawn::Sand => tile == Tile::Sand,
                    Spawn::Cave => tile == Tile::CaveFloor,
                };
                if !time_ok || !place_ok || sp.biomes & biome == 0 {
                    return 0.0;
                }
                [10.0, 5.0, 2.0, 0.6][(sp.rarity.clamp(1, 4) - 1) as usize]
            })
            .collect();
        let total: f32 = weights.iter().sum();
        if total <= 0.0 {
            return;
        }
        let mut roll = self.rng.gen_range(0.0..total);
        let mut pick = 0;
        for (i, w) in weights.iter().enumerate() {
            if roll < *w {
                pick = i;
                break;
            }
            roll -= w;
        }
        let shiny = self.rng.gen_range(0..SHINY_ODDS) == 0;
        let mood = if self.rng.gen_bool(0.5) { SPECIES[pick].temper } else { MOODS[self.rng.gen_range(0..MOODS.len())] };
        self.add_mob(pick, shiny, mood, x, y, None);
        let (lo, hi) = match self.map.id {
            MapId::Woods => (8, 13),
            MapId::Shore => (10, 16),
            MapId::Caves => (14, 22),
            _ => (3, 7),
        };
        let bonus = (SPECIES[pick].rarity as u32 - 1) * 2;
        self.mobs.last_mut().unwrap().level = self.rng.gen_range(lo..=hi) + bonus;
    }

    // ---------------------------------------------------------------- update

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        for t in self.toasts.iter_mut() {
            t.ttl -= dt;
        }
        self.toasts.retain(|t| t.ttl > 0.0);

        self.save_timer += dt;
        if self.save_timer > AUTOSAVE_SECS {
            self.save_timer = 0.0;
            self.save_now();
        }
        self.sim_timer += dt;
        if self.sim_timer > SIM_SECS {
            self.sim_timer = 0.0;
            let r = garden::simulate(&mut self.data, save::now(), &mut self.rng);
            if r.gifts > 0 {
                self.refresh_mob_emotes();
            }
            self.refresh_quests();
            self.check_evolutions();
            let w = self.weather_override.unwrap_or_else(|| Weather::now(Season::now()));
            if w != self.weather {
                self.weather = w;
                let outdoors = !matches!(self.map.id, MapId::Caves);
                let msg = match w {
                    Weather::Rain => Some("It's started to rain. Your crops are drinking it up!"),
                    Weather::Storm => Some("Thunder rumbles in the distance..."),
                    Weather::Snow => Some("Snow is falling. Everything is very quiet."),
                    Weather::Clear => Some("The sky clears."),
                    Weather::Cloudy => None,
                };
                if let (true, Some(m)) = (outdoors, msg) {
                    self.toast(m, Rgb(190, 210, 240));
                }
            }
            if self.weather.wet() {
                let now = save::now();
                for c in self.data.crops.values_mut() {
                    if c.can_water(now) {
                        c.water(now);
                    }
                }
            }
        }
        self.party = (self.party - dt).max(0.0);

        match &mut self.mode {
            Mode::Encounter(e) => {
                e.update(dt);
                self.sfx.append(&mut e.sfx);
                return;
            }
            Mode::Battle(b, _) => {
                b.update(dt);
                self.sfx.append(&mut b.sfx);
                if b.finished() {
                    self.end_battle();
                }
                return;
            }
            Mode::Evolve(ev) => {
                ev.t += dt;
                return;
            }
            Mode::Credits { t } => {
                *t += dt;
                return;
            }
            Mode::Explore | Mode::Build(_) => {}
            _ => return, // menus pause the world
        }

        // Map transition fade.
        if let Some((to, x, y)) = self.pending_warp {
            self.fade += dt * 4.0;
            if self.fade >= 1.0 {
                self.pending_warp = None;
                self.map = Map::load(to, self.data.land);
                let x = if x < 0 { self.map.w + x } else { x };
                self.player = Player::at(x, y);
                self.enter_map();
                self.toast(format!("~ {} ~", to.title()), Rgb(250, 230, 180));
                self.save_now();
            }
            return;
        }
        if self.fade > 0.0 {
            self.fade = (self.fade - dt * 3.0).max(0.0);
        }

        if matches!(self.mode, Mode::Explore) {
            self.update_player(dt);
        }
        self.update_mobs(dt);
        self.update_visitors(dt);
        self.update_npcs(dt);

        match self.map.id {
            MapId::Town => {}
            MapId::Garden => {
                self.visitor_timer -= dt;
                if self.visitor_timer <= 0.0 {
                    let a = garden::appeal(&self.data);
                    self.visitor_timer = (600.0 / (1.0 + a / 5.0)).clamp(25.0, 300.0) * self.rng.gen_range(0.7..1.3);
                    if !self.data.mons.is_empty() && self.visitors.len() < MAX_VISITORS {
                        self.spawn_visitor();
                    }
                }
            }
            _ => {
                self.spawn_timer += dt;
                if self.spawn_timer > 2.0 {
                    self.spawn_timer = 0.0;
                    if self.mobs.len() < MAX_WILD {
                        self.spawn_wild(9);
                    }
                }
            }
        }
    }

    fn update_npcs(&mut self, dt: f32) {
        for i in 0..self.npcs.len() {
            let n = &mut self.npcs[i];
            if n.w.moving() {
                n.w.progress += dt / (STEP_TIME * 3.0);
                continue;
            }
            n.wait -= dt;
            if n.wait > 0.0 {
                continue;
            }
            n.wait = self.rng.gen_range(1.5..5.0);
            // Stay put while the player is close, so they're easy to talk to.
            let (px, py) = (self.player.w.x, self.player.w.y);
            if (n.w.x - px).abs() + (n.w.y - py).abs() <= 2 {
                continue;
            }
            let d = [Dir::Up, Dir::Down, Dir::Left, Dir::Right][self.rng.gen_range(0..4)];
            let (dx, dy) = d.delta();
            let n = &self.npcs[i];
            let (nx, ny) = (n.w.x + dx, n.w.y + dy);
            let near_home = (nx - n.home.0).abs() <= 2 && (ny - n.home.1).abs() <= 2;
            let ok = near_home && self.walkable(nx, ny);
            let n = &mut self.npcs[i];
            n.facing = d;
            if ok {
                n.w.step_to(nx, ny);
            }
        }
    }

    fn update_player(&mut self, dt: f32) {
        let p = &mut self.player;
        for h in p.held.iter_mut() {
            h.1 -= dt;
        }
        p.held.retain(|h| h.1 > 0.0);
        p.bump = (p.bump - dt).max(-0.3);
        p.turn_delay -= dt;

        let mut carry = 0.0;
        if p.w.moving() {
            p.w.progress += dt / if p.running { RUN_TIME } else { STEP_TIME };
            if p.w.moving() {
                return;
            }
            // Arrived on a new tile.
            carry = (p.w.progress - 1.0).min(0.5);
            p.w.progress = 1.0;
            self.data.steps += 1;
            if let Some(e) = self.map.exit_at(p.w.x, p.w.y) {
                self.pending_warp = Some((e.to, e.tx, e.ty));
                p.stop();
                self.sfx.push(Sfx::Warp);
                return;
            }
            if p.pending_interact {
                p.pending_interact = false;
                p.tap = None;
                self.interact();
                return;
            }
        }
        if self.player.turn_delay > 0.0 {
            return;
        }
        let p = &mut self.player;
        let Some(d) = p.tap.take().or(p.held.last().map(|h| h.0)) else { return };
        p.facing = d;
        let (dx, dy) = d.delta();
        let (nx, ny) = (p.w.x + dx, p.w.y + dy);
        let someone_there = self.mobs.iter().any(|m| m.w.x == nx && m.w.y == ny)
            || self.visitors.iter().any(|v| v.w.x == nx && v.w.y == ny)
            || self.npcs.iter().any(|n| (n.w.x == nx && n.w.y == ny) || (n.w.moving() && n.w.from == (nx, ny)));
        let p = &mut self.player;
        if !self.map.blocked(nx, ny) && !someone_there {
            p.w.step_to(nx, ny);
            // Keep leftover motion so continuous walking has no hitch at tile boundaries.
            p.w.progress = carry;
            p.steps += 1;
        } else if p.bump <= -0.25 {
            p.bump = 0.12;
            self.sfx.push(Sfx::Bump);
        }
    }

    fn update_mobs(&mut self, dt: f32) {
        let (ptx, pty) = (self.player.w.x, self.player.w.y);
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            if m.w.moving() {
                m.w.progress += dt / (STEP_TIME * 2.2);
            }
            if let Some((_, ttl)) = &mut m.emote {
                *ttl -= dt;
                if *ttl <= 0.0 {
                    m.emote = None;
                }
            }
            if m.owned.is_none() {
                m.life -= dt;
            }
            // Wild creatures show their mood when you get close: a hint for befriending.
            let near = (m.w.x - ptx).abs() + (m.w.y - pty).abs() <= 3;
            if near && m.owned.is_none() && m.emote.is_none() {
                m.emote = Some((m.mood.emote(), 2.0));
            }
            m.wait -= dt;
            if m.wait > 0.0 || m.w.moving() {
                continue;
            }
            m.wait = self.rng.gen_range(0.8..3.5);
            if near && m.owned.is_none() {
                continue; // wild creatures hold still while you approach
            }
            // Residents now and then show how they feel.
            if let Some(o) = m.owned {
                if m.emote.is_none() && self.rng.gen_bool(0.08) {
                    let happy = self.data.mons.get(o).map(|d| d.happy).unwrap_or(50.0);
                    let m = &mut self.mobs[i];
                    m.emote = Some((if happy >= 75.0 { '♪' } else if happy < 30.0 { '…' } else { '·' }, 1.5));
                }
            }
            let d = [Dir::Up, Dir::Down, Dir::Left, Dir::Right][self.rng.gen_range(0..4)];
            let (dx, dy) = d.delta();
            let m = &self.mobs[i];
            let (nx, ny) = (m.w.x + dx, m.w.y + dy);
            // Residents stay close to their habitat.
            if let Some((hx, hy)) = m.home {
                let dist = |x: i32, y: i32| (x - hx).abs() + (y - hy).abs();
                if dist(nx, ny) > 3 && dist(nx, ny) > dist(m.w.x, m.w.y) {
                    continue;
                }
            }
            let tile = self.map.get(nx, ny);
            let free = !matches!(tile, Tile::Path | Tile::Bridge | Tile::Soil) && self.walkable(nx, ny);
            let m = &mut self.mobs[i];
            if dx != 0 {
                m.flip = dx < 0;
            }
            if free {
                m.w.step_to(nx, ny);
            }
        }
        // Wild creatures eventually wander off, but never while you are looking.
        self.mobs.retain(|m| m.owned.is_some() || m.life > 0.0 || (m.w.x - ptx).abs() < 14 && (m.w.y - pty).abs() < 10);
    }

    fn spawn_visitor(&mut self) {
        let Some(exit) = self.map.exits.first() else { return };
        let (ex, ey) = (exit.x - 1, exit.y);
        if self.occupied(ex, ey) {
            return;
        }
        let name = garden::VISITOR_NAMES[self.rng.gen_range(0..garden::VISITOR_NAMES.len())];
        let stay = self.rng.gen_range(40.0..80.0);
        let look = self.rng.gen_range(0..LOOKS.len());
        self.visitors.push(Visitor { name, look, w: Walker::at(ex, ey), facing: Dir::Left, wait: 0.5, stay, path: Vec::new(), leaving: false, cheered: false, emote: None });
    }

    fn update_visitors(&mut self, dt: f32) {
        let mut tips = Vec::new();
        for i in 0..self.visitors.len() {
            let v = &mut self.visitors[i];
            if v.w.moving() {
                v.w.progress += dt / (STEP_TIME * 2.6);
                continue;
            }
            if let Some((_, ttl)) = &mut v.emote {
                *ttl -= dt;
                if *ttl <= 0.0 {
                    v.emote = None;
                }
            }
            v.stay -= dt;
            v.wait -= dt;
            if v.wait > 0.0 {
                continue;
            }
            let (vx, vy) = (v.w.x, v.w.y);
            if v.leaving && v.path.is_empty() {
                tips.push(i);
                continue;
            }
            if !v.leaving && v.stay <= 0.0 {
                let exit = self.map.exits.first().map(|e| (e.x - 1, e.y)).unwrap_or((vx, vy));
                let path = bfs(&self.map, (vx, vy), exit);
                let v = &mut self.visitors[i];
                v.leaving = true;
                v.path = path;
                continue;
            }
            // Admire nearby creatures.
            let sees_mon = self.mobs.iter().any(|m| (m.w.x - vx).abs() + (m.w.y - vy).abs() <= 2);
            let v = &mut self.visitors[i];
            if sees_mon && !v.cheered {
                v.cheered = true;
                v.emote = Some(('♥', 2.0));
            }
            let near_player = (vx - self.player.w.x).abs() + (vy - self.player.w.y).abs() <= 1;
            if near_player && !v.leaving {
                v.wait = 0.5;
                continue;
            }
            let next = if v.leaving {
                v.path.first().copied()
            } else {
                let d = [Dir::Up, Dir::Down, Dir::Left, Dir::Right][self.rng.gen_range(0..4)];
                let (dx, dy) = d.delta();
                Some((vx + dx, vy + dy))
            };
            self.visitors[i].wait = if self.visitors[i].leaving { 0.0 } else { self.rng.gen_range(0.6..2.5) };
            let Some((nx, ny)) = next else { continue };
            let tile_ok = !matches!(self.map.get(nx, ny), Tile::Soil | Tile::Water);
            if tile_ok && self.walkable(nx, ny) {
                let v = &mut self.visitors[i];
                v.facing = Dir::from_delta(nx - vx, ny - vy);
                v.w.step_to(nx, ny);
                if v.leaving {
                    v.path.remove(0);
                }
            } else if self.visitors[i].leaving {
                self.visitors[i].wait = 0.4; // someone is in the way; try again shortly
            }
        }
        for &i in tips.iter().rev() {
            let v = self.visitors.remove(i);
            let tip = ((garden::appeal(&self.data) * 0.25).round() as u64).max(1);
            self.data.tokens += tip;
            self.data.total_visitors += 1;
            self.event(Event::Tip);
            if (v.w.x - self.player.w.x).abs() < 20 {
                self.toast(format!("{} enjoyed the visit and left a tip: ✦ {tip}", v.name), ui::GOLD);
                self.sfx.push(Sfx::Coin);
            }
        }
    }

    // ----------------------------------------------------------------- input

    /// A plain key press (used by tests and scripted input).
    pub fn key(&mut self, code: KeyCode) {
        self.input(code, KeyEventKind::Press, KeyModifiers::NONE);
    }

    /// Raw keyboard input: presses, auto-repeats and (on capable terminals) releases.
    pub fn input(&mut self, code: KeyCode, kind: KeyEventKind, mods: KeyModifiers) {
        if code == KeyCode::Null {
            // Focus lost: we will never see the releases, so let go of everything.
            self.player.stop();
            return;
        }
        if kind == KeyEventKind::Press {
            // The boss key: ` hides the game behind a perfectly ordinary build. Any key brings it back.
            if self.boss {
                self.boss = false;
                return;
            }
            if code == KeyCode::Char('`') && !matches!(self.mode, Mode::Prompt { .. }) {
                self.boss = true;
                self.player.stop();
                self.save_now();
                return;
            }
            self.konami(code);
        }
        if let (Mode::Explore, Some(d)) = (&self.mode, dir_key(code)) {
            return self.move_key(d, code, kind, mods);
        }
        if kind == KeyEventKind::Release {
            // Releasing a direction key in a menu must not leave it stuck as held.
            if let Some(d) = dir_key(code) {
                self.player.held.retain(|h| h.0 != d);
            }
            return;
        }
        // Auto-repeat is handy for scrolling menus, but must never re-trigger actions
        // (holding Enter shouldn't buy ten fountains or skip a whole conversation).
        if kind == KeyEventKind::Repeat && (dir_key(code).is_none() || matches!(self.mode, Mode::Talk { .. } | Mode::Report { .. })) {
            return;
        }
        if !matches!(self.mode, Mode::Explore) {
            self.player.stop();
        }
        self.dispatch(code);
    }

    /// ↑ ↑ ↓ ↓ ← → ← → B A
    fn konami(&mut self, code: KeyCode) {
        const CODE: [KeyCode; 10] = [
            KeyCode::Up,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Char('b'),
            KeyCode::Char('a'),
        ];
        let code = match code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            c => c,
        };
        self.key_history.push_back(code);
        if self.key_history.len() > CODE.len() {
            self.key_history.pop_front();
        }
        if self.key_history.iter().eq(CODE.iter()) {
            self.key_history.clear();
            if matches!(self.mode, Mode::Popup(Popup::BuildList(_), _)) {
                self.mode = Mode::Explore; // 'b' opened the build menu on the way
            }
            self.party = 8.0;
            self.sfx.push(Sfx::Quest);
            for m in self.mobs.iter_mut() {
                m.emote = Some(('♪', 8.0));
            }
            if self.data.secrets.insert("konami".into()) {
                self.data.tokens += 30;
                self.toast("↑↑↓↓←→←→BA — 30 extra lives! (We were out of lives. Have ✦30 instead.)", ui::GOLD);
            } else {
                self.toast("↑↑↓↓←→←→BA — party time!", ui::GOLD);
            }
        }
    }

    fn move_key(&mut self, d: Dir, code: KeyCode, kind: KeyEventKind, mods: KeyModifiers) {
        let p = &mut self.player;
        if kind == KeyEventKind::Release {
            p.held.retain(|h| h.0 != d);
            return;
        }
        let shifted = mods.contains(KeyModifiers::SHIFT) || matches!(code, KeyCode::Char(c) if c.is_ascii_uppercase());
        p.running = shifted;
        let fresh = kind == KeyEventKind::Press && !p.held.iter().any(|h| h.0 == d);
        let ttl = if self.enhanced_keys { HELD_TTL_ENHANCED } else { HELD_TTL_FALLBACK };
        p.held.retain(|h| h.0 != d);
        p.held.push((d, ttl));
        if !fresh {
            return;
        }
        if !p.w.moving() && p.facing != d && self.enhanced_keys {
            // Quick tap = turn to face; keep holding = walk.
            p.facing = d;
            p.turn_delay = TURN_DELAY;
        } else {
            p.tap = Some(d);
        }
    }

    fn dispatch(&mut self, code: KeyCode) {
        // Little clicks for moving around menus.
        if matches!(self.mode, Mode::Popup(..) | Mode::Catalog(_) | Mode::Dex(_) | Mode::Team(_)) {
            match code {
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right | KeyCode::Tab => self.sound(Sfx::Blip),
                KeyCode::Char(c) if "wasdhjkl".contains(c) => self.sound(Sfx::Blip),
                KeyCode::Esc | KeyCode::Backspace => self.sound(Sfx::Cancel),
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => self.sound(Sfx::Confirm),
                _ => {}
            }
        }
        match &mut self.mode {
            Mode::Explore => self.key_explore(code),
            Mode::Encounter(e) => {
                let outcome = e.key(code, &mut self.data, &mut self.rng);
                self.sfx.append(&mut e.sfx);
                if let Some(outcome) = outcome {
                    self.finish_encounter(outcome);
                }
            }
            Mode::Dex(d) => {
                if d.key(code) {
                    self.mode = Mode::Explore;
                }
            }
            Mode::Catalog(c) => match c.key(code) {
                Some(None) => self.mode = Mode::Explore,
                Some(Some(p)) => self.purchase(p),
                None => {}
            },
            Mode::Popup(_, menu) => match menu.key(code) {
                MenuResult::Cancel => self.mode = Mode::Explore,
                MenuResult::Chosen(i) => {
                    let Mode::Popup(p, _) = std::mem::replace(&mut self.mode, Mode::Explore) else { return };
                    self.popup_choice(p, i);
                }
                MenuResult::None => {}
            },
            Mode::Build(_) => self.key_build(code),
            Mode::Talk { pages, .. } => {
                pages.pop_front();
                if pages.is_empty() {
                    self.next_mode();
                }
            }
            Mode::Report { .. } => self.next_mode(),
            Mode::Battle(b, _) => b.key(code),
            Mode::Prompt { buffer } => match code {
                KeyCode::Char(c) if buffer.chars().count() < 24 => buffer.push(c),
                KeyCode::Backspace => {
                    buffer.pop();
                }
                KeyCode::Enter => {
                    let word = std::mem::take(buffer);
                    self.mode = Mode::Explore;
                    self.speak_to_shrine(&word);
                }
                KeyCode::Esc => self.mode = Mode::Explore,
                _ => {}
            },
            Mode::Credits { t } => {
                if *t > 4.0 {
                    self.next_mode();
                }
            }
            Mode::Team(t) => {
                if t.key(code, &mut self.data) {
                    self.mode = Mode::Explore;
                }
            }
            Mode::Evolve(ev) => {
                if ev.t >= EVOLVE_SECS && matches!(code, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') | KeyCode::Esc) {
                    let (mon, to) = (ev.mon, ev.to);
                    let name = SPECIES[to].name;
                    self.data.mons[mon].species = name.to_string();
                    self.sound(Sfx::Befriend);
                    self.data.seen.insert(name.to_string());
                    self.data.befriended.insert(name.to_string());
                    self.save_now();
                    if self.in_garden() {
                        for m in self.mobs.iter_mut().filter(|m| m.owned == Some(mon)) {
                            m.species = to;
                        }
                    }
                    self.next_mode();
                }
            }
        }
    }

    fn key_explore(&mut self, code: KeyCode) {
        if let Some(d) = dir_key(code) {
            // Scripted/test input without timing: behave like a single tap.
            self.player.tap = Some(d);
            return;
        }
        match code {
            KeyCode::Char('e') | KeyCode::Char(' ') | KeyCode::Enter => self.interact(),
            KeyCode::Char('x') | KeyCode::Tab => self.mode = Mode::Dex(Dex::new()),
            KeyCode::Char('c') => self.mode = Mode::Catalog(Catalog::new()),
            KeyCode::Char('b') => self.open_build_list(),
            KeyCode::Char('z') => self.zoomed_out = !self.zoomed_out,
            KeyCode::Char('p') => self.show_quests(),
            KeyCode::Char('t') => self.mode = Mode::Team(TeamView::new()),
            KeyCode::Char('o') => self.open_options(0),
            KeyCode::Char('n') => self.mode = Mode::Report { title: "✻ The Lost Chapters".into(), lines: story::journal(&self.data) },
            KeyCode::Char('?') => self.show_help(),
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn interact(&mut self) {
        if self.pending_warp.is_some() {
            return;
        }
        if self.player.w.moving() {
            self.player.pending_interact = true;
            return;
        }
        // Not facing anyone? Turn toward someone standing right next to you.
        let someone_at = |g: &Game, x: i32, y: i32| {
            g.mobs.iter().any(|m| m.w.x == x && m.w.y == y) || g.npcs.iter().any(|n| n.w.x == x && n.w.y == y) || g.visitors.iter().any(|v| v.w.x == x && v.w.y == y)
        };
        let (fx, fy) = self.player.facing_tile();
        if !someone_at(self, fx, fy) {
            let (px, py) = (self.player.w.x, self.player.w.y);
            if let Some(d) = [Dir::Up, Dir::Down, Dir::Left, Dir::Right].into_iter().find(|d| {
                let (dx, dy) = d.delta();
                someone_at(self, px + dx, py + dy)
            }) {
                if self.map.get(fx, fy).loot().is_none() && !matches!(self.map.get(fx, fy), Tile::Bush(_) | Tile::Soil) {
                    self.player.facing = d;
                }
            }
        }
        let (fx, fy) = self.player.facing_tile();
        if let Some(i) = self.mobs.iter().position(|m| m.w.x == fx && m.w.y == fy) {
            let m = &self.mobs[i];
            match m.owned {
                Some(o) => {
                    if self.data.mons[o].gift {
                        self.collect_gift(i, o);
                    } else {
                        let name = &self.data.mons[o].species;
                        let menu = Menu::new(name.clone(), vec![("Pet".into(), ui::TEXT), ("Feed".into(), ui::TEXT), ("How is it doing?".into(), ui::TEXT)]);
                        self.mode = Mode::Popup(Popup::Creature(i), menu);
                    }
                }
                None if SPECIES[m.species].name == "Mythos" => {
                    let shiny = m.shiny;
                    self.meet_mythos(shiny);
                }
                None => {
                    self.data.seen.insert(SPECIES[m.species].name.to_string());
                    let enc = Encounter::new(m.species, m.shiny, m.mood, m.level, self.map.id, &mut self.rng);
                    self.encounter_mob = Some(i);
                    self.mode = Mode::Encounter(Box::new(enc));
                }
            }
            return;
        }
        if let Some(v) = self.visitors.iter_mut().find(|v| v.w.x == fx && v.w.y == fy) {
            v.wait = 4.0;
            let speaker = v.name;
            let text = garden::visitor_line(&self.data, &mut self.rng);
            self.talk(speaker, &[&text], Rgb(150, 210, 255));
            return;
        }
        if let Some(i) = self.npcs.iter().position(|n| n.w.x == fx && n.w.y == fy) {
            // Turn to face the player.
            let (dx, dy) = self.player.facing.delta();
            self.npcs[i].facing = Dir::from_delta(-dx, -dy);
            self.npcs[i].wait = 5.0;
            match self.npcs[i].kind {
                NpcKind::Claude => self.talk_to_claude(),
                NpcKind::Ada => self.talk_to_ada(),
                NpcKind::Folk { lines, .. } => {
                    let n = &mut self.npcs[i];
                    let line = lines[n.next_line % lines.len()];
                    n.next_line += 1;
                    let name = n.name;
                    self.talk(name, &[line], Rgb(150, 210, 255));
                }
            }
            return;
        }
        let tile = self.map.get(fx, fy);
        if let Some((id, name)) = tile.loot() {
            return self.collect_curio(fx, fy, id, name);
        }
        match tile {
            Tile::Bush(f) => self.pick_bush(fx, fy, f),
            Tile::Soil if self.in_garden() => self.tend_soil(fx, fy),
            Tile::House => self.talk("Your house", &["Home sweet home. Inside, a cozy bed and far too many plushies. (The door opens in a future update — the Claudimon outside need you for now!)"], ui::ACCENT),
            Tile::Lab => self.talk("Claude's Lab", &["A bright, tidy lab humming with quiet computation. A note on the door: 'Out enjoying the weather — find me by the path! ✻ Claude'"], ui::ACCENT),
            Tile::Shop => self.mode = Mode::Catalog(Catalog::curio()),
            Tile::Arena => self.open_cups(),
            Tile::Page => self.read_page(fx, fy),
            Tile::Shrine => self.visit_shrine(),
            Tile::Lighthouse => self.show_lamps(),
            Tile::Altar(k) => self.visit_altar(k),
            Tile::Lamp => self.toast("A cosy street lamp. It hums faintly.", ui::DIM),
            Tile::CaveMouth => self.toast("Cool air drifts out of the cave mouth.", ui::DIM),
            _ => {}
        }
    }

    fn open_cups(&mut self) {
        let options = CUPS
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| {
                let lvl = c.trainers.last().and_then(|t| t.team.iter().map(|m| m.1).max()).unwrap_or(0);
                if self.data.badges.contains(c.id) {
                    (format!("♛ {}  (won · rematch)", c.name), ui::GOLD)
                } else if arena::unlocked(c, &self.data.badges) {
                    (format!("{}  ·  up to Lv {lvl}  ·  ✦{}", c.name, c.reward), ui::TEXT)
                } else {
                    (format!("⊘ {}", c.name), ui::DIM)
                }
            })
            .collect();
        self.mode = Mode::Popup(Popup::Cups, Menu::new("The Benchmark", options));
    }

    fn collect_curio(&mut self, x: i32, y: i32, id: &str, name: &str) {
        if self.map.picked.contains(&(x, y)) {
            self.toast(format!("No {name} here right now — check back later."), Rgb(200, 200, 200));
            return;
        }
        self.data.add_item(id, 1);
        self.data.bushes.insert(format!("{}:{}:{}", self.map.id.key(), x, y), save::now());
        self.map.picked.insert((x, y));
        let have = self.data.item_count(id);
        self.toast(format!("Found a {name}! (×{have}) Mira's Curio Shop in Promptville buys these."), Rgb(170, 230, 255));
        self.sound(Sfx::Pick);
        self.event(Event::Curio);
    }

    fn talk_to_claude(&mut self) {
        self.refresh_quests();
        let orange = ui::ACCENT;
        if !self.data.quests.met_claude {
            self.data.quests.met_claude = true;
            return self.talk("Claude", CLAUDE_INTRO, orange);
        }
        // Hand out rewards for finished prompts.
        let mut pages: Vec<String> = Vec::new();
        let mut total = 0;
        for q in self.data.quests.quests.iter_mut().filter(|q| q.done() && !q.claimed) {
            q.claimed = true;
            total += q.reward;
            self.data.quests.completed_total += 1;
            pages.push(format!("Wonderful work on \"{}\"! Here's ✦ {} for your trouble.", q.describe(), q.reward));
        }
        if self.data.quests.all_claimed() && !self.data.quests.bonus_claimed {
            self.data.quests.bonus_claimed = true;
            total += quests::BONUS_TOKENS;
            let gifts = ["lamp", "pot", "bench", "pumpkin", "crystal_lamp", "glowcap_lamp", "potted_palm"];
            let gift = gifts[self.rng.gen_range(0..gifts.len())];
            self.data.add_item(gift, 1);
            let name = garden::item(gift).map(|i| ITEMS[i].name).unwrap_or("gift");
            pages.push(format!("That's every prompt for today! Take ✦ {} and this {name} as a thank-you. Same time tomorrow?", quests::BONUS_TOKENS));
        }
        if total > 0 {
            self.sound(Sfx::Coin);
            self.data.tokens += total;
            self.save_now();
            let refs: Vec<&str> = pages.iter().map(|p| p.as_str()).collect();
            return self.talk("Claude", &refs, orange);
        }
        if let Some(pages) = self.claude_story() {
            self.save_now();
            let refs: Vec<&str> = pages.iter().map(|p| p.as_str()).collect();
            return self.talk("Claude", &refs, orange);
        }
        let line = if self.data.mons.len() > 0 && self.data.placed.iter().all(|p| garden::item(&p.kind).is_none_or(|i| ITEMS[i].habitat.is_none())) {
            "Your Claudimon would love a proper home. Every type has a habitat in my Catalog — press C!".to_string()
        } else if self.data.berries.iter().sum::<u32>() > 40 {
            "That's quite the berry hoard! I'll happily buy some through the Catalog's Sell tab.".to_string()
        } else {
            CLAUDE_CHATTER[self.rng.gen_range(0..CLAUDE_CHATTER.len())].to_string()
        };
        let open = self.data.quests.quests.iter().filter(|q| !q.done()).count();
        let status = if self.data.quests.all_claimed() {
            "You've finished all of today's prompts. New ones arrive at midnight!".to_string()
        } else {
            format!("You have {open} prompt{} left today — press P to see them.", if open == 1 { "" } else { "s" })
        };
        self.talk("Claude", &[&line, &status], orange);
    }

    fn show_quests(&mut self) {
        self.refresh_quests();
        let q = &self.data.quests;
        let mut lines: Vec<(String, Rgb)> = q
            .quests
            .iter()
            .map(|q| {
                let (mark, c) = if q.claimed {
                    ("✓", ui::DIM)
                } else if q.done() {
                    ("★", ui::GOLD)
                } else {
                    ("○", ui::TEXT)
                };
                (format!("{mark} {}  {}/{}  ✦{}", q.describe(), q.have, q.need, q.reward), c)
            })
            .collect();
        lines.push((String::new(), ui::TEXT));
        let bonus = if q.bonus_claimed { "✓ Daily bonus claimed".to_string() } else { format!("Finish all three for a ✦{} bonus and a gift", quests::BONUS_TOKENS) };
        lines.push((bonus, Rgb(255, 180, 140)));
        lines.push(("Claim rewards from Claude in Promptville (south of the meadow).".into(), ui::DIM));
        self.mode = Mode::Report { title: format!("✻ Prompts of the day · {}", q.day), lines };
    }

    fn show_help(&mut self) {
        let lines = [
            ("move        wasd · arrows · hjkl", ui::TEXT),
            ("e / space   talk, interact, pick, plant, befriend", ui::TEXT),
            ("b           build mode (in your Sanctuary)", ui::TEXT),
            ("c           Claude's Catalog: buy, sell, expand", ui::TEXT),
            ("p           today's prompts (quests)", ui::TEXT),
            ("t           your team: choose who battles", ui::TEXT),
            ("n           journal: The Lost Chapters", ui::TEXT),
            ("shift+move  run", ui::TEXT),
            ("x           Claudidex", ui::TEXT),
            ("z           zoom in / out", ui::TEXT),
            ("o           options: music, sound effects, zoom", ui::TEXT),
            ("`           boss key (a build is definitely compiling)", ui::TEXT),
            ("q / esc     save and quit", ui::TEXT),
            ("", ui::TEXT),
            ("Your Sanctuary earns tokens even while you're away.", ui::DIM),
        ];
        self.mode = Mode::Report { title: "How to play".into(), lines: lines.iter().map(|(l, c)| (l.to_string(), *c)).collect() };
    }

    fn pick_bush(&mut self, x: i32, y: i32, f: u8) {
        if self.map.picked.contains(&(x, y)) {
            self.toast("The bush is still regrowing its berries.", Rgb(200, 200, 200));
            return;
        }
        let n = self.rng.gen_range(1..=2);
        let flavor = FLAVORS[f as usize];
        self.data.berries[flavor.idx()] += n;
        self.data.bushes.insert(format!("{}:{}:{}", self.map.id.key(), x, y), save::now());
        self.map.picked.insert((x, y));
        self.toast(format!("Picked {n} {}{}! ({})", flavor.berry(), if n > 1 { "s" } else { "" }, flavor.name()), flavor.color());
        self.sound(Sfx::Pick);
        self.event(Event::PickedBerries(n));
    }

    fn tend_soil(&mut self, x: i32, y: i32) {
        let key = format!("{x}:{y}");
        let now = save::now();
        if let Some(crop) = self.data.crops.get_mut(&key) {
            let f = FLAVORS[crop.flavor as usize];
            if crop.progress(now) >= 1.0 {
                let n = 3 + self.rng.gen_range(0..=2) + u32::from(crop.bonus > 0);
                self.data.berries[f.idx()] += n;
                self.data.crops.remove(&key);
                self.toast(format!("Harvested {n} {}s!", f.berry()), f.color());
                self.sound(Sfx::Pick);
                self.event(Event::Harvested);
            } else if crop.can_water(now) {
                crop.water(now);
                self.toast(format!("You water the {} sprouts. They perk up!", f.berry()), Rgb(130, 190, 250));
                self.sound(Sfx::Blip);
            } else {
                let secs = garden::grow_secs(crop.flavor as usize);
                let left = ((1.0 - crop.progress(now)) * secs as f32 / 60.0).ceil() as i64;
                self.toast(format!("{} growing nicely · ripe in ~{left} min", f.berry()), Rgb(180, 220, 160));
            }
            return;
        }
        let options: Vec<(String, Rgb)> =
            (0..5).filter(|&f| self.data.item_count(garden::seed_id(f)) > 0).map(|f| (format!("{} ×{}", FLAVORS[f].berry(), self.data.item_count(garden::seed_id(f))), FLAVORS[f].color())).collect();
        if options.is_empty() {
            self.toast("No seeds! Order some from Claude's Catalog (c).", Rgb(220, 200, 160));
            return;
        }
        self.mode = Mode::Popup(Popup::Plant(x, y), Menu::new("Plant which seed?", options));
    }

    fn collect_gift(&mut self, mob: usize, o: usize) {
        self.data.mons[o].gift = false;
        let mon = &self.data.mons[o];
        let name = mon.species.clone();
        let fav = creatures::by_name(&name).map(|s| SPECIES[s].fav).unwrap_or(FLAVORS[0]);
        let roll = self.rng.gen_range(0..10);
        let msg = if roll < 5 {
            let n = self.rng.gen_range(2..=4);
            self.data.berries[fav.idx()] += n;
            format!("{name} brought you {n} {}s! ♥", fav.berry())
        } else if roll < 8 {
            let n = self.rng.gen_range(20..=60);
            self.data.tokens += n;
            format!("{name} found ✦ {n} tokens somewhere! ♥")
        } else {
            let f = self.rng.gen_range(0..5);
            self.data.add_item(garden::seed_id(f), 2);
            format!("{name} dug up 2 {} seeds! ♥", FLAVORS[f].berry())
        };
        self.toast(msg, Rgb(255, 180, 210));
        self.mobs[mob].emote = Some(('♥', 2.0));
        self.sound(Sfx::Heart);
        self.event(Event::Gift);
    }

    fn popup_choice(&mut self, popup: Popup, choice: usize) {
        let now = save::now();
        match popup {
            Popup::Creature(mob) => {
                let Some(o) = self.mobs.get(mob).and_then(|m| m.owned) else { return };
                let name = self.data.mons[o].species.clone();
                match choice {
                    0 => {
                        let mon = &mut self.data.mons[o];
                        if now - mon.last_pet > 3600 {
                            mon.friendship += 1;
                            mon.happy = (mon.happy + 5.0).min(100.0);
                            mon.last_pet = now;
                            let f = mon.friendship;
                            self.toast(format!("{name} loves the head pats! ♥ Friendship {f}"), Rgb(255, 170, 200));
                            self.event(Event::Petted);
                        } else {
                            self.toast(format!("{name} nuzzles your hand. ♥"), Rgb(255, 170, 200));
                        }
                        self.mobs[mob].emote = Some(('♥', 2.0));
                        self.sound(Sfx::Heart);
                    }
                    1 => {
                        let options = FLAVORS.iter().map(|f| (format!("{} ×{}", f.berry(), self.data.berries[f.idx()]), f.color())).collect();
                        self.mode = Mode::Popup(Popup::Feed(mob), Menu::new(format!("Feed {name}"), options));
                    }
                    _ => self.creature_status(o),
                }
            }
            Popup::Feed(mob) => {
                let Some(o) = self.mobs.get(mob).and_then(|m| m.owned) else { return };
                let f = FLAVORS[choice];
                if self.data.berries[f.idx()] == 0 {
                    self.toast(format!("You don't have any {}s.", f.berry()), Rgb(220, 160, 150));
                    return;
                }
                self.data.berries[f.idx()] -= 1;
                let sp = creatures::by_name(&self.data.mons[o].species).map(|s| &SPECIES[s]);
                let mon = &mut self.data.mons[o];
                let name = mon.species.clone();
                let (fav, hate) = sp.map(|s| (s.fav, s.hate)).unwrap_or((f, f));
                if now - mon.last_fed > 3600 {
                    mon.friendship += 1;
                }
                mon.last_fed = now;
                let (msg, emote) = if f == fav {
                    mon.fed_fav = true;
                    mon.happy = (mon.happy + 15.0).min(100.0);
                    (format!("{name} munches the {} with delight! Its favourite!", f.berry()), '♥')
                } else if f == hate {
                    mon.happy = (mon.happy - 10.0).max(0.0);
                    (format!("{name} pulls a face. It really doesn't like {} food.", f.name().to_lowercase()), '#')
                } else {
                    mon.happy = (mon.happy + 6.0).min(100.0);
                    (format!("{name} eats the {}. Yum.", f.berry()), '♪')
                };
                self.toast(msg, f.color());
                self.mobs[mob].emote = Some((emote, 2.0));
                self.sound(if emote == '#' { Sfx::Bad } else { Sfx::Heart });
                self.event(Event::Fed);
            }
            Popup::Plant(x, y) => {
                let owned: Vec<usize> = (0..5).filter(|&f| self.data.item_count(garden::seed_id(f)) > 0).collect();
                let Some(&f) = owned.get(choice) else { return };
                if self.data.take_item(garden::seed_id(f)) {
                    self.data.crops.insert(format!("{x}:{y}"), Crop { flavor: f as u8, planted: now, bonus: 0, watered_at: 0 });
                    let mins = garden::grow_secs(f) / 60;
                    self.toast(format!("Planted {}! Ripe in about {mins} min. Water it to speed it up.", FLAVORS[f].berry()), FLAVORS[f].color());
                }
            }
            Popup::Options => {
                match choice {
                    0 => self.data.settings.music = !self.data.settings.music,
                    1 => self.data.settings.sfx = !self.data.settings.sfx,
                    2 => self.zoomed_out = !self.zoomed_out,
                    _ => return self.save_now(),
                }
                self.open_options(choice);
            }
            Popup::Ferry(to_island) => {
                if choice != 0 {
                    return;
                }
                if to_island {
                    self.data.story.chapter = self.data.story.chapter.max(6);
                    self.pending_warp = Some((MapId::Island, world::ISLE_ARRIVAL.0, world::ISLE_ARRIVAL.1));
                } else {
                    let (lx, ly) = Map::load(MapId::Shore, 0).find(Tile::Lighthouse).unwrap_or((60, 6));
                    self.pending_warp = Some((MapId::Shore, lx, ly + 5));
                }
            }
            Popup::Quiz { qs, i, order } => {
                if order.get(choice) != Some(&0) {
                    return self.talk("Altar of Wisdom", &["The light dims. \"Not quite. Walk the world a little longer, and return.\""], world::ALTAR_COLORS[2]);
                }
                if i + 1 < qs.len() {
                    self.mode = self.quiz_popup(qs, i + 1);
                } else {
                    self.data.story.set("trial_wisdom");
                    self.save_now();
                    self.talk("Altar of Wisdom", &["Every answer rings true. The altar of Wisdom shines like a small blue sun.", &self.trials_left_line()], world::ALTAR_COLORS[2]);
                }
            }
            Popup::Haiku { order, shiny } => {
                if story::HAIKU_ENDS[order[choice]].1 {
                    self.join_mythos(shiny);
                } else {
                    self.mode = Mode::Talk { speaker: "Mythos".into(), pages: vec!["Mythos tilts its great head, amused. \"Count the beats again, little dreamer: five, seven, five.\"".to_string()].into(), color: MYTHOS_COLOR };
                    let popup = self.haiku_popup(shiny);
                    self.queue.push_front(popup);
                }
            }
            Popup::Cups => {
                let c = &CUPS[choice];
                if !arena::unlocked(c, &self.data.badges) {
                    let req = CUPS.iter().find(|r| Some(r.id) == c.requires).map(|r| r.name).unwrap_or("?");
                    return self.talk("Coach Bench", &[&format!("Win the {req} first, champ. One benchmark at a time!")], Rgb(190, 160, 240));
                }
                self.start_cup(choice, 0);
            }
            Popup::BuildList(items) => {
                let (x, y) = self.player.facing_tile();
                let item = items.get(choice).copied();
                self.mode = Mode::Build(Build { item, x, y });
            }
        }
    }

    fn creature_status(&mut self, o: usize) {
        let now = save::now();
        let homes = garden::assign_homes(&self.data);
        let m = &self.data.mons[o];
        let Some(sp) = creatures::by_name(&m.species).map(|s| &SPECIES[s]) else { return };
        let hearts = (m.happy / 20.0).round() as usize;
        let mut lines = vec![
            (format!("Happiness  {}{}  {:.0}%", "♥".repeat(hearts), "♡".repeat(5 - hearts), m.happy), Rgb(255, 160, 190)),
            (format!("Friendship {}", m.friendship), ui::TEXT),
        ];
        match homes[o] {
            Some(h) => {
                let kind = garden::item(&self.data.placed[h].kind).map(|i| ITEMS[i].name).unwrap_or("?");
                lines.push((format!("Home       {kind} ✓"), Rgb(150, 230, 140)));
            }
            None => {
                let want: Vec<&str> = sp.types.iter().map(|e| e.name()).collect();
                lines.push((format!("Home       none — wants a {} habitat", want.join("/")), Rgb(240, 180, 140)));
            }
        }
        let fed = if now - m.last_fed < garden::HAPPY_WINDOW { "recently ✓" } else { "a while ago — it's peckish" };
        lines.push((format!("Fed        {fed}"), ui::TEXT));
        let pet = if now - m.last_pet < garden::HAPPY_WINDOW { "recently ✓" } else { "it misses your pats" };
        lines.push((format!("Petted     {pet}"), ui::TEXT));
        if self.data.known_fav.contains(sp.name) {
            lines.push((format!("Loves      {} food", sp.fav.name().to_lowercase()), sp.fav.color()));
        }
        self.mode = Mode::Report { title: format!("{}{}", if m.shiny { "✦ " } else { "" }, m.species), lines };
    }

    fn open_build_list(&mut self) {
        if !self.in_garden() {
            self.toast("You can only build in your Sanctuary.", Rgb(220, 200, 160));
            return;
        }
        let items: Vec<usize> = (0..ITEMS.len()).filter(|&i| ITEMS[i].cat != Category::Seed && self.data.item_count(ITEMS[i].id) > 0).collect();
        let mut options: Vec<(String, Rgb)> = items.iter().map(|&i| (format!("{} ×{}", ITEMS[i].name, self.data.item_count(ITEMS[i].id)), ui::TEXT)).collect();
        options.push(("↔ Move / pick up objects".into(), ui::GOLD));
        if items.is_empty() && self.data.placed.is_empty() {
            self.toast("Nothing to build yet — order habitats and decor from the Catalog (c).", Rgb(220, 200, 160));
            return;
        }
        self.mode = Mode::Popup(Popup::BuildList(items), Menu::new("Build", options));
    }

    fn key_build(&mut self, code: KeyCode) {
        let Mode::Build(b) = &mut self.mode else { return };
        if let Some(d) = dir_key(code) {
            let (dx, dy) = d.delta();
            b.x = (b.x + dx).clamp(0, self.map.w - 1);
            b.y = (b.y + dy).clamp(0, self.map.h - 1);
            return;
        }
        match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('b') => self.mode = Mode::Explore,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => {
                let (item, x, y) = (b.item, b.x, b.y);
                match item {
                    Some(i) => {
                        if !self.can_place(i, x, y) {
                            self.toast("Can't place that here.", Rgb(240, 140, 130));
                            return;
                        }
                        if self.data.take_item(ITEMS[i].id) {
                            self.data.placed.push(Placed { kind: ITEMS[i].id.into(), x, y });
                            self.refresh_objects();
                            self.toast(format!("Placed {}!", ITEMS[i].name), ui::GOLD);
                            if self.data.item_count(ITEMS[i].id) == 0 {
                                self.mode = Mode::Explore;
                            }
                        }
                    }
                    None => {
                        if let Some(pi) = self.placed_at(x, y) {
                            let p = self.data.placed.remove(pi);
                            self.data.add_item(&p.kind, 1);
                            self.refresh_objects();
                            let name = garden::item(&p.kind).map(|i| ITEMS[i].name).unwrap_or("object");
                            self.toast(format!("Picked up {name}. Place it again from the build list."), ui::GOLD);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn placed_at(&self, x: i32, y: i32) -> Option<usize> {
        self.data.placed.iter().position(|p| {
            let size = garden::item(&p.kind).map(|i| ITEMS[i].size).unwrap_or((1, 1));
            (p.x..p.x + size.0).contains(&x) && (p.y..p.y + size.1).contains(&y)
        })
    }

    fn can_place(&self, item: usize, x: i32, y: i32) -> bool {
        let (w, h) = ITEMS[item].size;
        (y..y + h).all(|yy| {
            (x..x + w).all(|xx| {
                self.map.inside(xx, yy)
                    && !self.map.blocked(xx, yy)
                    && !matches!(self.map.get(xx, yy), Tile::Path | Tile::Soil | Tile::Water | Tile::Bridge)
                    && self.map.exit_at(xx, yy).is_none()
                    && !self.occupied(xx, yy)
            })
        })
    }

    fn purchase(&mut self, p: Purchase) {
        let Mode::Catalog(cat) = &mut self.mode else { return };
        match p {
            Purchase::Item(i) => {
                let it = &ITEMS[i];
                if self.data.tokens < it.price {
                    cat.say(format!("Not enough tokens — you need ✦ {} more.", it.price - self.data.tokens), Rgb(240, 140, 130));
                    return;
                }
                self.data.tokens -= it.price;
                self.data.add_item(it.id, 1);
                self.sfx.push(Sfx::Coin);
                let where_ = if it.cat == Category::Seed { "Plant it on the soil in your Sanctuary." } else { "Place it with build mode (b) in your Sanctuary." };
                cat.say(format!("✻ Delivered: {}! {where_}", it.name), Rgb(150, 230, 140));
            }
            Purchase::Sell(f, all) => {
                let have = self.data.berries[f];
                if have == 0 {
                    cat.say(format!("You have no {}s to sell.", FLAVORS[f].berry()), ui::DIM);
                    return;
                }
                let n = if all { have } else { 1 };
                let earned = n as u64 * garden::BERRY_PRICES[f];
                self.data.berries[f] -= n;
                self.data.tokens += earned;
                self.sfx.push(Sfx::Coin);
                cat.say(format!("Sold {n} {}{} for ✦ {earned}.", FLAVORS[f].berry(), if n == 1 { "" } else { "s" }), ui::GOLD);
                self.event(Event::Sold(n));
            }
            Purchase::SellLoot(i, all) => {
                let (id, name, price) = LOOT[i];
                let have = self.data.item_count(id);
                if have == 0 {
                    cat.say(format!("You have no {name}s to sell. Go exploring!"), ui::DIM);
                    return;
                }
                let n = if all { have } else { 1 };
                for _ in 0..n {
                    self.data.take_item(id);
                }
                self.data.tokens += n as u64 * price;
                self.sfx.push(Sfx::Coin);
                cat.say(format!("Mira buys {n} {name}{} for ✦ {}.", if n == 1 { "" } else { "s" }, n as u64 * price), ui::GOLD);
            }
            Purchase::Land => {
                let lvl = self.data.land;
                if lvl >= MAX_LAND {
                    cat.say("Your Sanctuary is as big as it gets!", ui::DIM);
                    return;
                }
                let price = LAND_PRICES[lvl as usize];
                if self.data.tokens < price {
                    cat.say(format!("Not enough tokens — you need ✦ {} more.", price - self.data.tokens), Rgb(240, 140, 130));
                    return;
                }
                self.data.tokens -= price;
                self.data.land += 1;
                cat.say("✻ The forest has been cleared. Your Sanctuary grew!", Rgb(150, 230, 140));
                if self.in_garden() {
                    let (x, y) = (self.player.w.x, self.player.w.y);
                    self.map = Map::load(MapId::Garden, self.data.land);
                    self.player = Player::at(x, y);
                    self.enter_map();
                }
                self.save_now();
            }
        }
    }

    fn finish_encounter(&mut self, outcome: Outcome) {
        let Mode::Encounter(e) = std::mem::replace(&mut self.mode, Mode::Explore) else { return };
        if let Outcome::Spar = outcome {
            let allies = self.party_fighters();
            let foe = vec![Fighter::new(e.species, e.level, e.shiny, None)];
            let biome = e.biome;
            let seed = self.rng.r#gen();
            self.mode = Mode::Battle(Box::new(Battle::new(BattleKind::Spar, allies, foe, biome, seed)), BattleCtx::Spar(e));
            return;
        }
        if let Some(trial) = self.trial.take() {
            return self.finish_trial(trial, outcome, &e);
        }
        let name = SPECIES[e.species].name;
        if let Some(i) = self.encounter_mob.take() {
            if i < self.mobs.len() {
                match outcome {
                    Outcome::Befriended | Outcome::Fled => {
                        self.mobs.remove(i);
                    }
                    Outcome::Left | Outcome::Spar => {
                        self.mobs[i].mood = e.mood;
                        self.mobs[i].wait = 3.0;
                    }
                }
            }
        }
        match outcome {
            Outcome::Befriended => {
                let mut mon = OwnedMon::new(name, e.shiny);
                mon.level = e.level;
                self.data.mons.push(mon);
                self.data.befriended.insert(name.to_string());
                if !self.data.party.is_empty() && self.data.party.len() < team::PARTY_SIZE {
                    self.data.party.push(self.data.mons.len() - 1);
                }
                self.toast(format!("{name} is heading to your Sanctuary! ♥"), Rgb(255, 200, 120));
                self.sound(Sfx::Befriend);
                self.event(Event::Befriended(SPECIES[e.species].types));
                self.save_now();
            }
            Outcome::Fled => {
                self.toast(format!("{name} wandered off..."), Rgb(190, 190, 210));
                self.sound(Sfx::Fled);
            }
            Outcome::Left | Outcome::Spar => {}
        }
        self.next_mode();
    }

    // ---------------------------------------------------------------- drawing

    pub fn draw(&self, s: &mut Screen) {
        // Start every frame from a blank slate: full-screen views (battles, encounters,
        // evolutions) repaint every pixel but not the text layer, so old signs and menus
        // would otherwise linger on top of them.
        s.clear(Rgb(0, 0, 0));
        if self.boss {
            return draw_boss(s, self.time);
        }
        let hour = self.hour();
        match &self.mode {
            Mode::Encounter(e) => return e.draw(s, &self.data, hour),
            Mode::Dex(d) => return d.draw(s, &self.data, self.time),
            Mode::Catalog(c) => return c.draw(s, &self.data, self.time),
            Mode::Battle(b, _) => return b.draw(s),
            Mode::Team(t) => return t.draw(s, &self.data, self.time),
            Mode::Evolve(ev) => return draw_evolve(s, ev, self.time),
            Mode::Credits { t } => return draw_credits(s, *t, &self.data),
            _ => {}
        }
        let season = Season::now();
        let z = if self.zoomed_out { 2 } else { 1 };
        let labels = if z == 1 {
            self.draw_world(s, z, season, hour)
        } else {
            let mut big = Screen::new(s.w as u16 * 2, s.rows() as u16 * 2);
            let e = self.draw_world(&mut big, z, season, hour);
            s.clear(Rgb(0, 0, 0));
            s.downsample_from(&big, 2);
            e
        };
        // Emotes and signs as text (after lighting so they stay readable).
        for (x, y, text, c, bg) in labels {
            let col = x / z - if bg.is_some() { text.chars().count() as i32 / 2 } else { 0 };
            let row = (y / z).div_euclid(2);
            if row >= 1 && row < s.rows() as i32 - 1 {
                s.text(col, row, &text, c, bg);
            }
        }
        self.draw_hud(s, hour, season);
        match &self.mode {
            Mode::Popup(_, menu) => menu.draw(s),
            Mode::Talk { speaker, pages, color } => ui::draw_talk(s, speaker, pages.front().map(String::as_str).unwrap_or(""), *color),
            Mode::Prompt { buffer } => {
                let cursor = if (self.time * 2.0) as i32 % 2 == 0 { "▌" } else { " " };
                ui::draw_talk(s, "The Thinking Shrine", &format!("Speak to the shrine:  > {buffer}{cursor}"), ui::ACCENT);
            }
            Mode::Report { title, lines } => ui::draw_report(s, title, lines),
            Mode::Build(b) => {
                let rows = s.rows() as i32;
                let what = b.item.map(|i| ITEMS[i].name).unwrap_or("pick up");
                let line = format!(" BUILD · {what} · arrows move · enter place · esc done ");
                s.text(1, rows - 3, &line, Rgb(24, 18, 28), Some(ui::GOLD));
            }
            _ => {}
        }
    }

    /// Draw map, objects, creatures and people into `s` (which is `z` times the real screen size).
    /// Returns emote positions in `s` pixel coordinates.
    fn draw_world(&self, s: &mut Screen, z: i32, season: Season, hour: f32) -> Vec<(i32, i32, String, Rgb, Option<Rgb>)> {
        let (px, py) = self.player.w.pos();
        let (fx, fy) = match &self.mode {
            Mode::Build(b) => (b.x * TILE, b.y * TILE),
            _ => (px, py),
        };
        // Camera centred on the focus, clamped to the map edges; small maps are centred.
        let hud = 2 * z; // one text row at the top and bottom
        let view_h = s.h as i32 - 2 * hud;
        let cam = |p: i32, view: i32, size: i32| {
            if size <= view { (size - view) / 2 } else { (p + TILE / 2 - view / 2).clamp(0, size - view) }
        };
        let cam_x = cam(fx, s.w as i32, self.map.w * TILE);
        let cam_y = cam(fy, view_h, self.map.h * TILE) - hud;
        let now = save::now();

        s.clear(Rgb(16, 20, 28));
        self.map.draw(s, cam_x, cam_y, season, self.time);
        let mut lights: Vec<(f32, f32, f32, Rgb)> = Vec::new();

        if self.in_garden() {
            for (key, crop) in &self.data.crops {
                let Some((x, y)) = key.split_once(':').and_then(|(x, y)| Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))) else { continue };
                garden::draw_crop(s, crop, x * TILE - cam_x, y * TILE - cam_y, now, self.time);
            }
            let mut placed: Vec<&Placed> = self.data.placed.iter().collect();
            placed.sort_by_key(|p| p.y);
            for p in placed {
                let Some(i) = garden::item(&p.kind) else { continue };
                let (ox, oy) = (p.x * TILE - cam_x, p.y * TILE - cam_y);
                garden::draw_item(s, i, ox, oy, self.time);
                if let Some((r, c)) = ITEMS[i].light {
                    let flicker = if ITEMS[i].id == "campfire" { 1.0 + (self.time * 7.0).sin() * 0.08 } else { 1.0 };
                    let (w, h) = ITEMS[i].size;
                    lights.push(((ox + w * 4) as f32, (oy + h * 3) as f32, r * flicker, c));
                }
            }
            // The house windows glow at night.
            let (hx, hy) = (4 * TILE - cam_x, 3 * TILE - cam_y);
            lights.push(((hx + 8) as f32, (hy + 15) as f32, 14.0, Rgb(255, 210, 140)));
            lights.push(((hx + 24) as f32, (hy + 15) as f32, 14.0, Rgb(255, 210, 140)));
        }
        // The lighthouse lamps: one for each completed goal.
        if self.map.id == MapId::Shore {
            if let Some((lx, ly)) = self.map.find(Tile::Lighthouse) {
                for (i, (_, lit)) in story::lamps(&self.data).iter().enumerate() {
                    let (x, y) = (lx * TILE + 3 + i as i32 * 2 - cam_x, ly * TILE + 2 - cam_y);
                    let c = if *lit { Rgb(255, 230, 140) } else { Rgb(70, 64, 80) };
                    s.set(x, y, c);
                    s.set(x, y + 1, c);
                    if *lit {
                        lights.push((x as f32, y as f32, 22.0, Rgb(255, 220, 140)));
                    }
                }
                if story::lamps_lit(&self.data) == 5 {
                    // A slow, sweeping beam out over the sea.
                    let a = self.time * 0.8;
                    for r in 4..60 {
                        let (bx, by) = (lx * TILE + 8 - cam_x + (a.cos() * r as f32) as i32, ly * TILE + 3 - cam_y + (a.sin() * r as f32 * 0.4) as i32);
                        s.blend(bx, by, Rgb(255, 245, 200), 0.5 * (1.0 - r as f32 / 60.0));
                    }
                }
            }
        }
        if self.map.id == MapId::Island {
            // Drifting motes of starlight.
            for i in 0..30 {
                let (bx, by) = ((world::hash(i, 7, 93) % (self.map.w * TILE) as u32) as f32, (world::hash(i, 8, 93) % (self.map.h * TILE) as u32) as f32);
                let fy = (by - self.time * (4.0 + (i % 5) as f32)).rem_euclid((self.map.h * TILE) as f32);
                let fx = bx + (self.time * 0.5 + i as f32).sin() * 6.0;
                let (sx, sy) = (fx as i32 - cam_x, fy as i32 - cam_y);
                s.set(sx, sy, Rgb(255, 240, 200).lerp(Rgb(170, 200, 255), (i % 3) as f32 / 2.0));
            }
        }
        // Glowing tiles in view: crystals, lava, lamps, glowcaps, daylight at the cave mouth.
        let (tx0, ty0) = (cam_x.div_euclid(TILE) - 3, cam_y.div_euclid(TILE) - 3);
        let (tx1, ty1) = ((cam_x + s.w as i32) / TILE + 3, (cam_y + s.h as i32) / TILE + 3);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                if let Some((r, c)) = self.map.light_at(tx, ty) {
                    lights.push(((tx * TILE + 4 - cam_x) as f32, (ty * TILE + 4 - cam_y) as f32, r, c));
                }
            }
        }
        if self.map.id == MapId::Caves {
            // Your trusty lantern.
            let flicker = 1.0 + (self.time * 5.0).sin() * 0.04;
            lights.push(((px + 4 - cam_x) as f32, (py + 4 - cam_y) as f32, 48.0 * flicker, Rgb(255, 220, 160)));
        }
        if self.map.id == MapId::Woods && clock::is_night(hour) {
            // Fireflies drifting between the trees.
            for i in 0..14 {
                let (bx, by) = ((world::hash(i, 0, 91) % (self.map.w * TILE) as u32) as f32, (world::hash(i, 1, 91) % (self.map.h * TILE) as u32) as f32);
                let fx = bx + (self.time * 0.3 + i as f32).sin() * 20.0;
                let fy = by + (self.time * 0.4 + i as f32 * 1.7).cos() * 14.0;
                let (sx, sy) = (fx as i32 - cam_x, fy as i32 - cam_y);
                if (self.time * 1.3 + i as f32 * 0.8).sin() > -0.2 {
                    s.set(sx, sy, Rgb(230, 255, 140));
                    lights.push((sx as f32, sy as f32, 7.0, Rgb(200, 255, 120)));
                }
            }
        }

        // Y-sorted sprites: creatures, visitors and the player.
        enum Who {
            Player,
            Mob(usize),
            Visitor(usize),
            Npc(usize),
        }
        let mut order: Vec<(i32, Who)> = self.mobs.iter().enumerate().map(|(i, m)| (m.w.pos().1, Who::Mob(i))).collect();
        order.extend(self.visitors.iter().enumerate().map(|(i, v)| (v.w.pos().1, Who::Visitor(i))));
        order.extend(self.npcs.iter().enumerate().map(|(i, n)| (n.w.pos().1, Who::Npc(i))));
        order.push((py, Who::Player));
        order.sort_by_key(|o| o.0);
        for (_, who) in order {
            match who {
                Who::Player => {
                    let (bx, by) = if self.player.bump > 0.0 { self.player.facing.delta() } else { (0, 0) };
                    let frame = walk_frame(&self.player.w, self.player.steps);
                    draw_person(s, None, self.player.facing, frame, px - cam_x + bx, py - cam_y + by)
                }
                Who::Visitor(i) => {
                    let v = &self.visitors[i];
                    let (vx, vy) = v.w.pos();
                    let frame = walk_frame(&v.w, (v.w.x + v.w.y) as u32);
                    draw_person(s, Some(v.look), v.facing, frame, vx - cam_x, vy - cam_y);
                }
                Who::Npc(i) => {
                    let n = &self.npcs[i];
                    let (nx, ny) = n.w.pos();
                    match n.kind {
                        NpcKind::Ada => draw_person(s, Some(4), n.facing, 0, nx - cam_x, ny - cam_y),
                        NpcKind::Folk { look, .. } => draw_person(s, Some(look), n.facing, walk_frame(&n.w, (n.w.x + n.w.y) as u32), nx - cam_x, ny - cam_y),
                        NpcKind::Claude => {
                            let frame = if (self.time * 1.5) as i32 % 2 == 0 { &CLAUDE_A } else { &CLAUDE_B };
                            let bob = ((self.time * 2.0).sin() * 1.5) as i32;
                            shadow(s, nx - cam_x, ny - cam_y);
                            world::sprite_rows(s, nx - cam_x, ny - cam_y - 1 + bob, frame, &|c| match c {
                                'o' => Some(ui::ACCENT),
                                'W' => Some(Rgb(255, 236, 220)),
                                _ => None,
                            });
                            lights.push(((nx + 4 - cam_x) as f32, (ny + 3 - cam_y) as f32, 16.0, Rgb(255, 170, 120)));
                        }
                    }
                }
                Who::Mob(i) => {
                    let m = &self.mobs[i];
                    let (mx, my) = m.w.pos();
                    let bob = if m.w.moving() || (self.time * 2.0 + m.phase).sin() > 0.85 { -1 } else { 0 };
                    shadow(s, mx - cam_x, my - cam_y);
                    creatures::draw(s, m.species, mx - cam_x, my - cam_y + bob, 1, m.flip, m.shiny, self.time);
                }
            }
        }

        if let Mode::Build(b) = &self.mode {
            self.draw_ghost(s, b, cam_x, cam_y);
        }

        let outdoors = !matches!(self.map.id, MapId::Caves | MapId::Island);
        let mut ambient = self.map.ambient(hour);
        if outdoors {
            weather::draw(s, self.weather, season, self.time, z);
            let k = self.weather.tint();
            ambient = [ambient[0] * k[0], ambient[1] * k[1], ambient[2] * k[2]];
        }
        if self.party > 0.0 {
            // Confetti!
            for i in 0..(s.w as i32 * s.h as i32 / 120) {
                let x = (world::hash(i, 1, 55) % s.w as u32) as i32;
                let y = ((world::hash(i, 2, 55) % s.h as u32) as f32 + self.time * 30.0).rem_euclid(s.h as f32) as i32;
                let c = [Rgb(255, 90, 120), Rgb(255, 210, 80), Rgb(110, 220, 140), Rgb(110, 170, 255), ui::ACCENT][(i % 5) as usize];
                s.set(x + ((self.time * 3.0 + i as f32).sin() * 2.0) as i32, y, c);
            }
        }
        s.light(ambient, &lights);
        if self.fade > 0.0 {
            let k = 1.0 - self.fade.min(1.0);
            s.tint([k, k, k]);
        }
        let emote_color = |ch: char| match ch {
            '♥' => Rgb(255, 120, 160),
            '✉' => Rgb(255, 220, 120),
            _ => Rgb(255, 255, 255),
        };
        let mut labels: Vec<_> = self
            .mobs
            .iter()
            .filter_map(|m| {
                let (ch, _) = m.emote?;
                let (mx, my) = m.w.pos();
                Some((mx - cam_x + 4, my - cam_y - 3 * z, ch.to_string(), emote_color(ch), None))
            })
            .collect();
        labels.extend(self.visitors.iter().filter_map(|v| {
            let (ch, _) = v.emote?;
            let (vx, vy) = v.w.pos();
            Some((vx - cam_x + 4, vy - cam_y - 3 * z, ch.to_string(), emote_color(ch), None))
        }));
        // Claude has something for you: a "!" above the star.
        let claim = self.data.quests.quests.iter().any(|q| q.done() && !q.claimed) || !self.data.quests.met_claude || self.claude_has_story();
        for n in &self.npcs {
            if matches!(n.kind, NpcKind::Claude) && claim && (self.time * 2.0).sin() > -0.5 {
                let (nx, ny) = n.w.pos();
                labels.push((nx - cam_x + 3, ny - cam_y - 3 * z, "!".into(), ui::GOLD, None));
            }
        }
        // Signs over the town's buildings.
        if self.map.id == MapId::Town {
            for (tile, name) in [(Tile::Lab, "Claude's Lab"), (Tile::Shop, "Curio Shop"), (Tile::Arena, "The Benchmark")] {
                if let Some(i) = self.map.tiles.iter().position(|t| *t == tile) {
                    let (bx, by, bw, _) = world::building_bounds(&self.map, i as i32 % self.map.w, i as i32 / self.map.w);
                    labels.push((bx * TILE + bw * TILE / 2 - cam_x, by * TILE - cam_y - 2 * z, format!(" {name} "), Rgb(30, 22, 30), Some(Rgb(245, 225, 190))));
                }
            }
        }
        labels
    }

    fn draw_ghost(&self, s: &mut Screen, b: &Build, cam_x: i32, cam_y: i32) {
        let (x, y) = (b.x * TILE - cam_x, b.y * TILE - cam_y);
        let (w, h, ok) = match b.item {
            Some(i) => {
                let mut ghost = Screen::new((ITEMS[i].size.0 * TILE) as u16, (ITEMS[i].size.1 * TILE / 2) as u16);
                ghost.clear(Rgb(255, 0, 255));
                garden::draw_item(&mut ghost, i, 0, 0, self.time);
                for gy in 0..ghost.h as i32 {
                    for gx in 0..ghost.w as i32 {
                        let c = ghost.get(gx, gy);
                        if c != Rgb(255, 0, 255) {
                            s.blend(x + gx, y + gy, c, 0.7);
                        }
                    }
                }
                (ITEMS[i].size.0, ITEMS[i].size.1, self.can_place(i, b.x, b.y))
            }
            None => (1, 1, self.placed_at(b.x, b.y).is_some()),
        };
        let c = if ok { Rgb(120, 255, 140) } else { Rgb(255, 90, 90) };
        let (pw, ph) = (w * TILE, h * TILE);
        for i in 0..pw {
            if (i + (self.time * 8.0) as i32) % 4 < 2 {
                s.set(x + i, y, c);
                s.set(x + i, y + ph - 1, c);
            }
        }
        for i in 0..ph {
            if (i + (self.time * 8.0) as i32) % 4 < 2 {
                s.set(x, y + i, c);
                s.set(x + pw - 1, y + i, c);
            }
        }
    }

    fn draw_hud(&self, s: &mut Screen, hour: f32, season: Season) {
        let w = s.w as i32;
        let rows = s.rows() as i32;
        let bar = Rgb(30, 26, 36);
        s.text(0, 0, &" ".repeat(s.w), Rgb(255, 255, 255), Some(bar));
        s.text(1, 0, "✻ Claudimon", ui::ACCENT, Some(bar));
        s.text(13, 0, &format!("· {}", self.map.id.title()), Rgb(200, 190, 220), Some(bar));
        let sky = if matches!(self.map.id, MapId::Caves) { "⛰" } else if self.weather != Weather::Clear { self.weather.icon() } else if clock::is_night(hour) { "☾" } else { "☀" };
        let info = format!("✦ {}  ♥ {}  {} {}  {} ", self.data.tokens, self.data.mons.len(), sky, clock::clock_str(), season.name());
        s.text(w - info.chars().count() as i32, 0, &info, Rgb(230, 220, 200), Some(bar));

        s.text(0, rows - 1, &" ".repeat(s.w), Rgb(255, 255, 255), Some(bar));
        let mut col = 1;
        for f in FLAVORS {
            s.text(col, rows - 1, "●", f.color(), Some(bar));
            let n = format!("{:<3}", self.data.berries[f.idx()]);
            s.text(col + 1, rows - 1, &n, Rgb(220, 220, 230), Some(bar));
            col += 5;
        }
        let help = if self.in_garden() { "e:act b:build c:shop p:prompts ?:help " } else { "e:act c:shop p:prompts x:dex ?:help " };
        if w > col + help.len() as i32 + 1 {
            s.text(w - help.len() as i32, rows - 1, help, Rgb(160, 160, 180), Some(bar));
        }

        // Toasts stacked above the bottom bar.
        for (i, t) in self.toasts.iter().rev().enumerate() {
            let text: String = format!(" {} ", t.text).chars().take(s.w.saturating_sub(2)).collect();
            let len = text.chars().count() as i32;
            let row = rows - 3 - i as i32 * 2;
            let col = (w - len) / 2;
            let fade = (t.ttl / 0.5).min(1.0);
            s.text(col, row, &text, Rgb(0, 0, 0).lerp(t.color, fade), Some(Rgb(24, 20, 30)));
        }
    }
}

fn dir_key(code: KeyCode) -> Option<Dir> {
    match code {
        KeyCode::Up => Some(Dir::Up),
        KeyCode::Down => Some(Dir::Down),
        KeyCode::Left => Some(Dir::Left),
        KeyCode::Right => Some(Dir::Right),
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'w' | 'k' => Some(Dir::Up),
            's' | 'j' => Some(Dir::Down),
            'a' | 'h' => Some(Dir::Left),
            'd' | 'l' => Some(Dir::Right),
            _ => None,
        },
        _ => None,
    }
}

/// Shortest walkable path (excluding the start) on terrain, ignoring other walkers.
fn bfs(map: &Map, start: (i32, i32), goal: (i32, i32)) -> Vec<(i32, i32)> {
    if start == goal {
        return Vec::new();
    }
    let idx = |x: i32, y: i32| (y * map.w + x) as usize;
    let mut prev = vec![usize::MAX; (map.w * map.h) as usize];
    let mut q = VecDeque::from([start]);
    prev[idx(start.0, start.1)] = idx(start.0, start.1);
    while let Some((x, y)) = q.pop_front() {
        if (x, y) == goal {
            let mut path = vec![(x, y)];
            let mut i = idx(x, y);
            while prev[i] != idx(start.0, start.1) {
                i = prev[i];
                path.push(((i as i32) % map.w, (i as i32) / map.w));
            }
            path.reverse();
            return path;
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if map.inside(nx, ny) && !map.blocked(nx, ny) && !matches!(map.get(nx, ny), Tile::Water | Tile::Soil) && prev[idx(nx, ny)] == usize::MAX {
                prev[idx(nx, ny)] = idx(x, y);
                q.push_back((nx, ny));
            }
        }
    }
    Vec::new()
}

fn shadow(s: &mut Screen, x: i32, y: i32) {
    for dx in 1..7 {
        s.blend(x + dx, y + 7, Rgb(0, 0, 0), 0.3);
    }
}

/// Visitor looks: (hair, shirt, skin).
const LOOKS: [(Rgb, Rgb, Rgb); 6] = [
    (Rgb(60, 40, 30), Rgb(200, 80, 90), Rgb(250, 214, 180)),
    (Rgb(230, 200, 110), Rgb(90, 160, 110), Rgb(255, 224, 196)),
    (Rgb(30, 30, 36), Rgb(240, 200, 80), Rgb(170, 120, 90)),
    (Rgb(170, 70, 40), Rgb(120, 110, 200), Rgb(250, 214, 180)),
    (Rgb(200, 200, 210), Rgb(90, 140, 160), Rgb(230, 190, 160)),
    (Rgb(110, 60, 140), Rgb(240, 150, 60), Rgb(120, 80, 60)),
];

// People are 8x10: two pixels taller than a tile, so they read as characters, not tiles.
// Side views face RIGHT; facing left mirrors them.
const PLAYER_FRONT: [&str; 8] = ["..oooo..", ".oOOooo.", ".cccccc.", ".hffffh.", ".fEffEf.", ".bffffb.", ".tggggt.", "f.gGGg.f"];
const PLAYER_BACK: [&str; 8] = ["..oooo..", ".oOOooo.", ".oooooo.", ".hhhhhh.", ".hhhhhh.", "..hhhh..", ".tggggt.", "f.gggg.f"];
const PLAYER_SIDE: [&str; 8] = ["..oooo..", ".oOOooo.", ".hoooccc", ".hhffff.", ".hhffEf.", "..hfffb.", "..tggt..", "..gggf.."];
const NPC_FRONT: [&str; 8] = ["..hhhh..", ".hhhhhh.", ".hhhhhh.", ".hffffh.", ".fEffEf.", ".bffffb.", ".ssssss.", "f.sSSs.f"];
const NPC_BACK: [&str; 8] = ["..hhhh..", ".hhhhhh.", ".hhhhhh.", ".hhhhhh.", ".hhhhhh.", "..hhhh..", ".ssssss.", "f.ssss.f"];
const NPC_SIDE: [&str; 8] = ["..hhhh..", ".hhhhhh.", ".hhhhhh.", ".hhffff.", ".hhffEf.", "..hfffb.", "..ssss..", "..sssf.."];
/// Walk cycle, one entry per frame: 0 standing, then 1 step A · 2 passing · 3 step B · 4 passing.
/// Front/back: the lifted foot rises instead of vanishing. Side: a stride with a shaded back leg.
const LEGS_FRONT: [[&str; 3]; 5] = [
    ["..g..g..", "..g..g..", "..k..k.."],
    ["..g..g..", "..k..g..", ".....k.."],
    ["..g..g..", "..g..g..", "..k..k.."],
    ["..g..g..", "..g..k..", "..k....."],
    ["..g..g..", "..g..g..", "..k..k.."],
];
const LEGS_SIDE: [[&str; 3]; 5] = [
    ["...gg...", "...gg...", "...kkk.."],
    ["...gg...", "..G..g..", ".kk..kk."],
    ["...gg...", "...gg...", "...kkk.."],
    ["...gg...", "..g..G..", ".kk..kk."],
    ["...gg...", "...gg...", "...kkk.."],
];

/// Which walk frame a walker shows: `stride` alternates every tile (step count or tile parity).
fn walk_frame(w: &Walker, stride: u32) -> usize {
    if !w.moving() {
        return 0;
    }
    let half = usize::from(w.progress >= 0.5);
    1 + (stride as usize % 2) * 2 + half
}

/// Draw the player (look = None: orange cap and overalls) or a visitor/townsperson.
fn draw_person(s: &mut Screen, look: Option<usize>, facing: Dir, frame: usize, x: i32, y: i32) {
    let npc = look.is_some();
    let side = matches!(facing, Dir::Left | Dir::Right);
    let frame = frame % 5;
    let mut body: Vec<Vec<char>> = match (npc, facing) {
        (false, Dir::Down) => PLAYER_FRONT,
        (false, Dir::Up) => PLAYER_BACK,
        (false, _) => PLAYER_SIDE,
        (true, Dir::Down) => NPC_FRONT,
        (true, Dir::Up) => NPC_BACK,
        (true, _) => NPC_SIDE,
    }
    .iter()
    .map(|r| r.chars().collect())
    .collect();
    // Arms swing opposite to the legs.
    let skin_row = |r: &Vec<char>| r.iter().position(|&c| c == 'f');
    if !side && (frame == 1 || frame == 3) {
        let col = if frame == 1 { 7 } else { 0 };
        if body[7][col] == 'f' {
            body[7][col] = '.';
            body[6][col] = 'f';
        }
    } else if side && (frame == 1 || frame == 3) {
        if let Some(c) = skin_row(&body[7]) {
            body[7][c] = if npc { 's' } else { 'g' };
            let swing = if frame == 1 { (c + 1).min(7) } else { 1 };
            body[7][swing] = 'f';
        }
    }
    let legs = if side { LEGS_SIDE[frame] } else { LEGS_FRONT[frame] };
    let mirror = facing == Dir::Left;
    // Rise as a foot lifts (front/back) or as the legs pass each other (side). Only the body
    // rises; the hips stretch by a pixel so the planted foot stays on the ground.
    let lifted = if side { frame == 2 || frame == 4 } else { frame == 1 || frame == 3 };
    let bob = usize::from(lifted);
    let leg_rows: Vec<&str> = std::iter::repeat_n(legs[0], bob).chain(legs.iter().copied()).collect();
    let rows: Vec<String> = body
        .into_iter()
        .map(|r| r.into_iter().collect::<String>())
        .chain(leg_rows.iter().map(|r| if npc { r.replace('g', "p").replace('G', "P") } else { r.to_string() }))
        .map(|r| if mirror { r.chars().rev().collect() } else { r })
        .collect();
    let refs: Vec<&str> = rows.iter().map(|r| r.as_str()).collect();
    let (hair, shirt, skin) = match look {
        Some(l) => LOOKS[l % LOOKS.len()],
        None => (Rgb(96, 62, 44), Rgb(246, 240, 224), Rgb(250, 214, 180)),
    };
    shadow(s, x, y);
    let bob = bob as i32;
    world::sprite_rows(s, x, y - 3 - bob, &refs, &|c| match c {
        'o' => Some(ui::ACCENT), // the orange cap
        'O' => Some(Rgb(245, 170, 130)),
        'c' => Some(Rgb(168, 78, 54)),
        'h' => Some(hair),
        'f' => Some(skin),
        'b' => Some(skin.lerp(Rgb(240, 120, 120), 0.35)),
        'E' => Some(Rgb(36, 28, 44)),
        't' => Some(Rgb(246, 240, 224)),
        'g' => Some(Rgb(78, 150, 118)),
        'G' => Some(Rgb(52, 108, 84)),
        's' => Some(shirt),
        'S' => Some(shirt.lerp(Rgb(0, 0, 0), 0.2)),
        'p' => Some(Rgb(60, 56, 80)),
        'P' => Some(Rgb(40, 38, 56)),
        'k' => Some(Rgb(84, 58, 44)),
        _ => None,
    });
}

/// Snapshot helper: every pose of the player and a visitor on one sheet.
pub fn draw_people_sheet(s: &mut Screen) {
    s.clear(Rgb(86, 150, 70));
    for (row, look) in [None, Some(1)].into_iter().enumerate() {
        for (i, d) in [Dir::Down, Dir::Up, Dir::Right, Dir::Left].into_iter().enumerate() {
            for f in 0..5 {
                draw_person(s, look, d, f, 2 + (i * 5 + f) as i32 * 10, 5 + row as i32 * 14);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_at(map: &str, x: i32, y: i32) -> Game {
        let mut data = SaveData::default();
        data.map = map.into();
        data.x = x;
        data.y = y;
        data.berries = [20; 5];
        let mut g = Game::new(data);
        g.mobs.clear();
        g
    }

    #[test]
    fn befriend_wild_creature_end_to_end() {
        let mut g = game_at("meadow", 10, world::MEADOW_ENTRY_Y);
        let kitten = creatures::by_name("Haikitten").unwrap();
        let (px, py) = (g.player.w.x, g.player.w.y);
        g.add_mob(kitten, false, Mood::Hungry, px + 1, py, None);
        g.add_mob(kitten, false, Mood::Hungry, px - 3, py, None); // a bystander that must survive
        g.player.facing = Dir::Right;

        g.key(KeyCode::Char('e'));
        assert!(matches!(g.mode, Mode::Encounter(_)), "interacting with a wild creature starts an encounter");
        assert!(g.data.seen.contains("Haikitten"));

        for _ in 0..12 {
            if matches!(g.mode, Mode::Explore) {
                break;
            }
            for k in [KeyCode::Char('1'), KeyCode::Char('1'), KeyCode::Enter] {
                g.update(1.0);
                g.key(k);
            }
        }
        assert!(matches!(g.mode, Mode::Explore));
        assert_eq!(g.data.mons.len(), 1);
        assert_eq!(g.data.mons[0].species, "Haikitten");
        assert_eq!(g.mobs.len(), 1, "befriended creature leaves the meadow, bystander stays");
        assert_eq!((g.mobs[0].w.x, g.mobs[0].w.y), (px - 3, py));
    }

    #[test]
    fn picking_a_bush_gives_berries_once() {
        let mut g = game_at("garden", 3, 17);
        assert_eq!(g.map.get(3, 18), Tile::Bush(0));
        g.player.facing = Dir::Down;
        let before = g.data.berries[0];
        g.key(KeyCode::Char('e'));
        let after = g.data.berries[0];
        assert!(after > before);
        g.key(KeyCode::Char('e'));
        assert_eq!(g.data.berries[0], after, "picked bush must regrow first");
    }

    #[test]
    fn plant_water_and_harvest() {
        // Soil plot spans x 10..=16, y 11..=13; its gate is at (13, 14).
        let mut g = game_at("garden", 13, 14);
        g.player.facing = Dir::Up;
        assert_eq!(g.map.get(13, 13), Tile::Soil);
        g.key(KeyCode::Char('e'));
        assert!(matches!(g.mode, Mode::Popup(Popup::Plant(13, 13), _)));
        g.key(KeyCode::Enter); // first owned seed: Honeyberry
        assert_eq!(g.data.item_count("seed_sweet"), 2);
        let crop = g.data.crops.get("13:13").expect("planted").clone();
        assert_eq!(crop.flavor, 0);

        g.key(KeyCode::Char('e')); // water
        assert!(g.data.crops["13:13"].bonus > 0);

        // Fast-forward: pretend it was planted long ago.
        g.data.crops.get_mut("13:13").unwrap().planted -= 3600;
        let before = g.data.berries[0];
        g.key(KeyCode::Char('e'));
        assert!(g.data.berries[0] >= before + 4, "harvest yields 3-5 (+1 watered)");
        assert!(g.data.crops.is_empty());
    }

    #[test]
    fn buy_place_and_pick_up_habitat() {
        let mut g = game_at("garden", 20, 15);
        g.data.tokens = 1000;
        g.mode = Mode::Catalog(Catalog::new());
        g.purchase(Purchase::Item(garden::item("pond").unwrap()));
        assert_eq!(g.data.tokens, 1000 - 180);
        assert_eq!(g.data.item_count("pond"), 1);

        g.mode = Mode::Build(Build { item: garden::item("pond"), x: 26, y: 15 });
        // A wall of soil/path is rejected; open grass works.
        assert!(!g.can_place(garden::item("pond").unwrap(), 10, 11), "soil is not buildable");
        g.key(KeyCode::Enter);
        assert_eq!(g.data.placed.len(), 1, "pond placed at (26,15): {:?}", g.data.placed);
        assert!(g.map.blocked(27, 16), "placed objects block movement");

        g.mode = Mode::Build(Build { item: None, x: 27, y: 16 });
        g.key(KeyCode::Enter);
        assert!(g.data.placed.is_empty());
        assert_eq!(g.data.item_count("pond"), 1);
        assert!(!g.map.blocked(27, 16));
    }

    #[test]
    fn land_expansion_grows_garden_and_keeps_exit() {
        let mut g = game_at("garden", 20, 15);
        g.data.tokens = 100_000;
        g.mode = Mode::Catalog(Catalog::new());
        for lvl in 1..=MAX_LAND {
            g.purchase(Purchase::Land);
            assert_eq!(g.data.land, lvl);
            let (w, h) = Map::garden_size(lvl);
            assert_eq!((g.map.w, g.map.h), (w, h));
            let exit = &g.map.exits[0];
            assert_eq!((exit.x, exit.y), (w - 1, 8));
            assert_eq!(g.map.get(w - 1, 8), Tile::Path);
            assert!(!bfs(&g.map, (6, 7), (w - 1, 8)).is_empty(), "house must reach the exit at level {lvl}");
        }
        g.purchase(Purchase::Land);
        assert_eq!(g.data.land, MAX_LAND, "cannot expand past the max");
    }

    #[test]
    fn visitors_walk_out_and_tip() {
        let mut g = game_at("garden", 6, 7);
        g.data.mons.push(OwnedMon::new("Haikitten", false));
        g.spawn_visitor();
        assert_eq!(g.visitors.len(), 1);
        g.visitors[0].stay = 0.0;
        let before = g.data.tokens;
        for _ in 0..600 {
            g.update(0.1);
            if g.visitors.is_empty() {
                break;
            }
        }
        assert!(g.visitors.is_empty(), "visitor should path back to the exit");
        assert!(g.data.tokens > before, "and leave a tip");
    }
}

#[cfg(test)]
mod economy_tests {
    use super::*;

    #[test]
    fn selling_berries_earns_tokens() {
        let mut g = Game::new(SaveData::default());
        g.data.berries = [4, 0, 0, 0, 0];
        let start = g.data.tokens;
        g.mode = Mode::Catalog(Catalog::new());
        g.purchase(Purchase::Sell(0, false));
        assert_eq!((g.data.berries[0], g.data.tokens), (3, start + 3));
        g.purchase(Purchase::Sell(0, true));
        assert_eq!((g.data.berries[0], g.data.tokens), (0, start + 12));
        g.purchase(Purchase::Sell(0, true));
        assert_eq!(g.data.tokens, start + 12, "nothing left to sell");
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::creatures::{CAVE, MEADOW, SHORE, WOODS};
    use crate::world::ALL_MAPS;

    #[test]
    fn every_exit_leads_somewhere_sensible() {
        for land in 0..=MAX_LAND {
            for id in ALL_MAPS {
                let m = Map::load(id, land);
                if id == MapId::Island {
                    continue; // reached only by Ada's ferry
                }
                assert!(!m.exits.is_empty(), "{id:?} has no exits");
                for e in &m.exits {
                    assert!(!m.blocked(e.x, e.y), "{id:?} exit at ({}, {}) is blocked", e.x, e.y);
                    let dest = Map::load(e.to, land);
                    let tx = if e.tx < 0 { dest.w + e.tx } else { e.tx };
                    assert!(!dest.blocked(tx, e.ty), "{id:?} -> {:?} arrives inside a wall at ({tx}, {})", e.to, e.ty);
                    assert!(dest.exit_at(tx, e.ty).is_none(), "{id:?} -> {:?} arrives on an exit (warp loop)", e.to);
                    // From the arrival tile you can reach an exit back to where you came from.
                    let back = dest.exits.iter().filter(|b| b.to == id).any(|b| tx == b.x && e.ty == b.y || !bfs(&dest, (tx, e.ty), (b.x, b.y)).is_empty());
                    assert!(back, "{:?}: no way back to {id:?} from ({tx}, {})", e.to, e.ty);
                }
                // All exits of a map are connected to each other.
                let first = (m.exits[0].x, m.exits[0].y);
                for e in &m.exits[1..] {
                    assert!((e.x, e.y) == first || !bfs(&m, first, (e.x, e.y)).is_empty(), "{id:?}: exits not connected");
                }
            }
        }
    }

    #[test]
    fn every_species_has_a_home_biome_with_spawn_tiles() {
        for (i, sp) in SPECIES.iter().enumerate() {
            if creatures::stage(i) > 0 {
                assert_eq!(sp.biomes, 0, "evolved forms only come from evolving: {}", sp.name);
                continue;
            }
            if sp.name == "Mythos" {
                continue; // placed by the story, never wild
            }
            assert!(sp.biomes != 0, "{} lives nowhere", sp.name);
            let mut ok = false;
            for id in [MapId::Meadow, MapId::Woods, MapId::Shore, MapId::Caves, MapId::Island] {
                if sp.biomes & id.biome_bit() == 0 {
                    continue;
                }
                let m = Map::load(id, 0);
                let fits = (0..m.h).any(|y| {
                    (0..m.w).any(|x| {
                        let t = m.get(x, y);
                        match sp.spawn {
                            Spawn::Grass => matches!(t, Tile::Grass | Tile::TallGrass | Tile::Flower),
                            Spawn::TallGrass => t == Tile::TallGrass,
                            Spawn::Shore => !m.blocked(x, y) && m.near_water(x, y) && matches!(t, Tile::Grass | Tile::TallGrass | Tile::Flower | Tile::Sand | Tile::CaveFloor),
                            Spawn::Sand => t == Tile::Sand,
                            Spawn::Cave => t == Tile::CaveFloor,
                        }
                    })
                });
                ok |= fits;
            }
            assert!(ok, "{} has no tiles it can spawn on", sp.name);
        }
        // Every wild area has residents by day and by night.
        for bit in [MEADOW, WOODS, SHORE, CAVE] {
            for night in [false, true] {
                let n = SPECIES
                    .iter()
                    .filter(|s| s.biomes & bit != 0 && (s.active == Active::Any || (s.active == Active::Night) == night))
                    .count();
                assert!(n >= 3, "biome {bit} has only {n} species at night={night}");
            }
        }
    }

    #[test]
    fn species_sprites_are_well_formed() {
        let mut names = std::collections::HashSet::new();
        for sp in SPECIES {
            assert!(names.insert(sp.name), "duplicate species {}", sp.name);
            for row in sp.sprite {
                assert_eq!(row.chars().count(), 8, "{} sprite row {row:?}", sp.name);
                for ch in row.chars().filter(|c| *c != '.') {
                    assert!(sp.pal.iter().any(|(k, _)| *k == ch), "{} missing palette {ch:?}", sp.name);
                }
            }
        }
    }

    #[test]
    fn town_people_stand_on_open_ground() {
        let m = Map::load(MapId::Town, 0);
        assert!(!m.blocked(CLAUDE_HOME.0, CLAUDE_HOME.1));
        for (name, _, (x, y), lines) in TOWNSFOLK {
            assert!(!m.blocked(*x, *y), "{name} spawns in a wall");
            assert!(!lines.is_empty());
        }
    }

    fn at(map: &str, x: i32, y: i32) -> Game {
        let mut data = SaveData::default();
        data.map = map.into();
        data.x = x;
        data.y = y;
        let mut g = Game::new(data);
        g.mobs.clear();
        g
    }

    #[test]
    fn claude_intro_then_rewards_then_bonus() {
        let mut g = at("town", CLAUDE_HOME.0, CLAUDE_HOME.1 + 1);
        g.npcs.retain(|n| matches!(n.kind, NpcKind::Claude));
        g.player.facing = Dir::Up;
        g.key(KeyCode::Char('e'));
        let Mode::Talk { speaker, pages, .. } = &g.mode else { panic!("expected Claude to talk") };
        assert_eq!(speaker, "Claude");
        assert_eq!(pages.len(), CLAUDE_INTRO.len());
        for _ in 0..CLAUDE_INTRO.len() {
            g.key(KeyCode::Enter);
        }
        assert!(matches!(g.mode, Mode::Explore));

        // Finish every prompt, then claim.
        for q in g.data.quests.quests.iter_mut() {
            q.have = q.need;
        }
        let expected: u64 = g.data.quests.quests.iter().map(|q| q.reward).sum::<u64>() + quests::BONUS_TOKENS;
        let before = g.data.tokens;
        let items_before: u32 = g.data.items.values().sum();
        g.key(KeyCode::Char('e'));
        assert_eq!(g.data.tokens, before + expected);
        assert!(g.data.quests.bonus_claimed);
        assert_eq!(g.data.items.values().sum::<u32>(), items_before + 1, "bonus gift item");

        // Claiming again gives nothing more.
        g.mode = Mode::Explore;
        g.key(KeyCode::Char('e'));
        assert_eq!(g.data.tokens, before + expected);
    }

    #[test]
    fn curios_are_collected_once_and_sold_at_the_curio_shop() {
        let mut g = at("shore", 1, 25);
        let (x, y) = (0..g.map.h)
            .flat_map(|y| (0..g.map.w).map(move |x| (x, y)))
            .find(|&(x, y)| g.map.get(x, y) == Tile::Shell && !g.map.blocked(x, y - 1))
            .expect("the shore has shells");
        g.player = Player::at(x, y - 1);
        g.player.facing = Dir::Down;
        g.key(KeyCode::Char('e'));
        assert_eq!(g.data.item_count("loot_shell"), 1);
        g.key(KeyCode::Char('e'));
        assert_eq!(g.data.item_count("loot_shell"), 1, "must regrow first");

        g.mode = Mode::Catalog(Catalog::curio());
        let before = g.data.tokens;
        g.purchase(Purchase::SellLoot(0, true));
        assert_eq!(g.data.item_count("loot_shell"), 0);
        assert_eq!(g.data.tokens, before + LOOT[0].2);
    }

    #[test]
    fn quest_events_fire_from_real_actions() {
        let mut g = at("garden", 3, 17);
        g.refresh_quests();
        g.data.quests.quests = vec![quests::Quest { goal: quests::Goal::PickBerries, need: 1, have: 0, reward: 50, claimed: false }];
        g.player.facing = Dir::Down;
        g.key(KeyCode::Char('e')); // the Sweet bush at (3, 18)
        assert!(g.data.quests.quests[0].done());
    }
}

#[cfg(test)]
mod movement_tests {
    use super::*;

    const FRAME: f32 = 1.0 / 30.0;

    /// Open stretch of path in the garden: walking right from (10, 8).
    fn walker(enhanced: bool) -> Game {
        let mut data = SaveData::default();
        data.x = 10;
        data.y = 8;
        let mut g = Game::new(data);
        g.mobs.clear();
        g.enhanced_keys = enhanced;
        g.player.facing = Dir::Right;
        g
    }

    fn press(g: &mut Game, code: KeyCode, kind: KeyEventKind) {
        g.input(code, kind, KeyModifiers::NONE);
    }

    fn run_for(g: &mut Game, secs: f32) {
        for _ in 0..(secs / FRAME).round() as usize {
            g.update(FRAME);
        }
    }

    #[test]
    fn tap_takes_exactly_one_step() {
        for enhanced in [false, true] {
            let mut g = walker(enhanced);
            press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
            if enhanced {
                g.update(FRAME);
                press(&mut g, KeyCode::Char('d'), KeyEventKind::Release);
            }
            run_for(&mut g, 1.0);
            assert_eq!(g.player.w.x, 11, "enhanced={enhanced}");
        }
    }

    #[test]
    fn holding_walks_without_pauses_and_release_stops_cleanly() {
        let mut g = walker(true);
        press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
        let mut idle_frames = 0;
        for i in 0..30 {
            g.update(FRAME);
            // Terminals send auto-repeats while held; they must not cause extra steps.
            if i % 2 == 0 && i > 15 {
                press(&mut g, KeyCode::Char('d'), KeyEventKind::Repeat);
            }
            if !g.player.w.moving() {
                idle_frames += 1;
            }
        }
        assert_eq!(idle_frames, 0, "no hitch between tiles while held");
        let at_release = g.player.w.x;
        assert!(at_release >= 16, "about 7 tiles per second, got to {at_release}");
        press(&mut g, KeyCode::Char('d'), KeyEventKind::Release);
        run_for(&mut g, 1.0);
        assert_eq!(g.player.w.x, at_release, "finish the current step, never an extra one");
    }

    #[test]
    fn fallback_terminal_hold_and_release() {
        // No release events: a press, the OS repeat delay, then repeats every 33 ms.
        let mut g = walker(false);
        press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
        run_for(&mut g, 0.5);
        let mut t = 0.0;
        while t < 1.0 {
            press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
            g.update(FRAME);
            t += FRAME;
        }
        let at_release = g.player.w.x;
        assert!(at_release >= 16);
        run_for(&mut g, 1.0);
        assert!(g.player.w.x <= at_release + 1, "stops within one step after the repeats end");
    }

    #[test]
    fn quick_tap_turns_in_place() {
        let mut g = walker(true);
        g.player.facing = Dir::Down;
        press(&mut g, KeyCode::Char('a'), KeyEventKind::Press);
        g.update(FRAME);
        press(&mut g, KeyCode::Char('a'), KeyEventKind::Release);
        run_for(&mut g, 0.5);
        assert_eq!((g.player.w.x, g.player.facing), (10, Dir::Left));
        // Holding the same new direction walks after the short turn.
        g.player.facing = Dir::Down;
        press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
        run_for(&mut g, 0.4);
        assert!(g.player.w.x > 10);
    }

    #[test]
    fn shift_runs_faster() {
        let mut walk = walker(true);
        let mut run = walker(true);
        press(&mut walk, KeyCode::Char('d'), KeyEventKind::Press);
        run.input(KeyCode::Char('D'), KeyEventKind::Press, KeyModifiers::SHIFT);
        run_for(&mut walk, 1.0);
        run_for(&mut run, 1.0);
        assert!(run.player.w.x - 10 >= (walk.player.w.x - 10) * 3 / 2, "run {} vs walk {}", run.player.w.x, walk.player.w.x);
    }

    #[test]
    fn focus_loss_releases_held_keys() {
        let mut g = walker(true);
        press(&mut g, KeyCode::Char('d'), KeyEventKind::Press);
        g.update(FRAME);
        g.input(KeyCode::Null, KeyEventKind::Release, KeyModifiers::NONE);
        run_for(&mut g, 1.0);
        assert_eq!(g.player.w.x, 11);
    }

    #[test]
    fn townsfolk_block_the_way_and_are_easy_to_talk_to() {
        let mut data = SaveData::default();
        data.map = "town".into();
        data.x = CLAUDE_HOME.0 + 1;
        data.y = CLAUDE_HOME.1;
        let mut g = Game::new(data);
        g.enhanced_keys = true;
        g.player.facing = Dir::Left;
        press(&mut g, KeyCode::Left, KeyEventKind::Press);
        run_for(&mut g, 0.5);
        press(&mut g, KeyCode::Left, KeyEventKind::Release);
        assert_eq!(g.player.w.x, CLAUDE_HOME.0 + 1, "can't walk through Claude");
        // Facing away, interacting still finds the neighbour.
        g.player.facing = Dir::Right;
        g.key(KeyCode::Char('e'));
        assert!(matches!(&g.mode, Mode::Talk { speaker, .. } if speaker == "Claude"));
    }

    #[test]
    fn interact_pressed_mid_step_happens_on_arrival() {
        let mut g = walker(true);
        // Bush (Sweet) at (3, 18); walk down onto (3, 17) from (3, 16) and press e mid-step.
        g.player = Player::at(3, 16);
        g.player.facing = Dir::Down;
        g.player.tap = Some(Dir::Down);
        g.update(FRAME);
        assert!(g.player.w.moving());
        g.key(KeyCode::Char('e'));
        let before = g.data.berries[0];
        run_for(&mut g, 0.3);
        assert!(g.data.berries[0] > before, "the bush got picked once we arrived");
    }
}

/// The evolution cutscene: the old and new forms flicker faster and faster, then the reveal.
fn draw_evolve(s: &mut Screen, ev: &Evolve, time: f32) {
    let w = s.w as i32;
    let rows = s.rows() as i32;
    let h = s.h as i32;
    for y in 0..h {
        for x in 0..w {
            let d = (((x - w / 2) as f32).powi(2) + ((y - h / 2) as f32 * 1.6).powi(2)).sqrt();
            let pulse = ((d * 0.15 - time * 4.0).sin() * 0.5 + 0.5) * 0.25;
            s.set(x, y, Rgb(30, 22, 44).lerp(Rgb(120, 80, 170), pulse));
        }
    }
    let scale = ((h - 14) / 8).clamp(2, 7).min(w / 12);
    let size = 8 * scale;
    let (x, y) = (w / 2 - size / 2, h / 2 - size / 2 - 2);
    let done = ev.t >= EVOLVE_SECS;
    if done {
        creatures::draw(s, ev.to, x, y, scale, false, false, time);
        for i in 0..14 {
            let a = time * 1.5 + i as f32 * 0.45;
            let r = size as f32 * 0.7 + (time * 3.0 + i as f32).sin() * 4.0;
            s.set(x + size / 2 + (a.cos() * r) as i32, y + size / 2 + (a.sin() * r * 0.8) as i32, Rgb(255, 240, 180));
        }
    } else {
        // Flicker between the two silhouettes, accelerating.
        let speed = 2.0 + ev.t * ev.t * 3.0;
        let show_new = (ev.t * speed).sin() > 0.0;
        let species = if show_new { ev.to } else { ev.from };
        creatures::draw_silhouette(s, species, x, y, scale);
        for yy in 0..size {
            for xx in 0..size {
                let c = s.get(x + xx, y + yy);
                if c == Rgb(46, 40, 58) {
                    s.set(x + xx, y + yy, Rgb(250, 245, 255));
                }
            }
        }
    }
    let from = SPECIES[ev.from].name;
    let to = SPECIES[ev.to].name;
    let msg = if done { format!("Congratulations! {from} evolved into {to}!") } else { format!("What? {from} is evolving!") };
    let col = (w - msg.chars().count() as i32) / 2;
    s.text(col.max(0), rows - 3, &msg, if done { ui::GOLD } else { ui::TEXT }, Some(Rgb(30, 22, 44)));
    if done {
        s.text(w / 2 - 5, rows - 2, "press space", ui::DIM, Some(Rgb(30, 22, 44)));
    }
}

#[cfg(test)]
mod battle_flow_tests {
    use super::*;

    fn game(map: &str, x: i32, y: i32, team: &[(&str, u32)]) -> Game {
        let mut data = SaveData::default();
        data.map = map.into();
        data.x = x;
        data.y = y;
        data.berries = [20; 5];
        for (name, lvl) in team {
            let mut m = OwnedMon::new(name, false);
            m.level = *lvl;
            data.mons.push(m);
        }
        let mut g = Game::new(data);
        g.mobs.clear();
        g
    }

    /// Drive battles and dialogue (always picking the first move) until the game
    /// is back to exploring or an encounter. Returns the number of battles fought.
    fn drive(g: &mut Game) -> usize {
        let mut battles = 0;
        let mut in_battle = false;
        for _ in 0..20_000 {
            match &mut g.mode {
                Mode::Battle(b, _) => {
                    if !in_battle {
                        battles += 1;
                        in_battle = true;
                    }
                    if b.finished() || b.result.is_some() || !matches!(b.result, None) {
                        g.update(0.5);
                    } else {
                        g.update(0.5);
                        g.key(KeyCode::Char('1'));
                    }
                }
                Mode::Talk { .. } | Mode::Report { .. } => {
                    in_battle = false;
                    g.key(KeyCode::Enter);
                }
                Mode::Evolve(_) => {
                    g.update(1.0);
                    g.key(KeyCode::Enter);
                }
                Mode::Explore | Mode::Encounter(_) => return battles,
                _ => panic!("unexpected mode"),
            }
        }
        panic!("never settled");
    }

    #[test]
    fn spar_during_an_encounter_then_resume() {
        let mut g = game("meadow", 10, world::MEADOW_ENTRY_Y, &[("Opuslion", 40)]);
        let (px, py) = (g.player.w.x, g.player.w.y);
        g.add_mob(creatures::by_name("Mutox").unwrap(), false, Mood::Grumpy, px + 1, py, None);
        g.player.facing = Dir::Right;
        g.key(KeyCode::Char('e'));
        assert!(matches!(g.mode, Mode::Encounter(_)));
        g.key(KeyCode::Char('5')); // Spar
        assert!(matches!(g.mode, Mode::Battle(_, BattleCtx::Spar(_))));
        let xp_before = (g.data.mons[0].level, g.data.mons[0].xp);
        drive(&mut g);
        // Back in the same encounter, or already befriended (a big win can max out trust).
        assert!(matches!(g.mode, Mode::Encounter(_)) || g.data.mons.len() == 2);
        assert!((g.data.mons[0].level, g.data.mons[0].xp) > xp_before, "the team earned XP");
    }

    #[test]
    fn spar_needs_a_team() {
        let mut g = game("meadow", 10, world::MEADOW_ENTRY_Y, &[]);
        let (px, py) = (g.player.w.x, g.player.w.y);
        g.add_mob(0, false, Mood::Grumpy, px + 1, py, None);
        g.player.facing = Dir::Right;
        g.key(KeyCode::Char('e'));
        g.key(KeyCode::Char('5'));
        assert!(matches!(g.mode, Mode::Encounter(_)));
    }

    #[test]
    fn winning_a_cup_gives_badge_trophy_and_unlocks_the_next() {
        let mut g = game("town", 23, 20, &[("Opuslion", 50), ("Ferrisaur", 50), ("Torrentuna", 50)]);
        let tokens = g.data.tokens;
        g.start_cup(0, 0);
        let fought = drive(&mut g);
        assert_eq!(fought, 3, "three trainers in a cup");
        assert!(g.data.badges.contains("haiku"));
        assert_eq!(g.data.item_count("trophy_haiku"), 1);
        assert!(g.data.tokens >= tokens + CUPS[0].reward);
        assert!(arena::unlocked(&CUPS[1], &g.data.badges));
        assert!(!arena::unlocked(&CUPS[2], &g.data.badges));
        // Rematch: smaller prize, no second trophy.
        let t2 = g.data.tokens;
        g.start_cup(0, 0);
        drive(&mut g);
        assert_eq!(g.data.item_count("trophy_haiku"), 1);
        assert_eq!(g.data.tokens, t2 + CUPS[0].reward / 4);
    }

    #[test]
    fn losing_a_cup_ends_it_without_a_badge() {
        let mut g = game("town", 23, 20, &[("Snailoop", 2)]);
        g.start_cup(0, 0);
        assert_eq!(drive(&mut g), 1);
        assert!(g.data.badges.is_empty());
        assert!(matches!(g.mode, Mode::Explore));
    }

    #[test]
    fn locked_cups_cannot_be_entered() {
        let mut g = game("town", 23, 20, &[("Opuslion", 50)]);
        g.mode = Mode::Popup(Popup::Cups, Menu::new("x", vec![]));
        g.popup_choice(Popup::Cups, 2);
        assert!(matches!(g.mode, Mode::Talk { .. }));
    }

    #[test]
    fn evolution_cutscene_changes_species() {
        let mut g = game("garden", 6, 7, &[("Haikitten", 15)]);
        let need = creatures::xp_to_next(15);
        garden::add_xp(&mut g.data.mons[0], need);
        assert_eq!(g.data.mons[0].level, 16);
        g.check_evolutions();
        assert!(matches!(g.mode, Mode::Evolve(_)));
        g.key(KeyCode::Enter); // too early: nothing happens
        assert_eq!(g.data.mons[0].species, "Haikitten");
        g.update(EVOLVE_SECS + 0.1);
        g.key(KeyCode::Enter);
        assert_eq!(g.data.mons[0].species, "Sonnetiger");
        assert!(g.data.seen.contains("Sonnetiger"));
        assert!(matches!(g.mode, Mode::Explore));
    }

    #[test]
    fn team_screen_edits_the_party() {
        let mut g = game("garden", 6, 7, &[("Haikitten", 5), ("Cachew", 9), ("Gitcko", 7), ("Puddlefin", 3)]);
        assert_eq!(team::party(&g.data), vec![1, 2, 0], "default party: the three strongest");
        g.key(KeyCode::Char('t'));
        g.key(KeyCode::Enter); // remove Haikitten (index 0)
        assert_eq!(g.data.party, vec![1, 2]);
        for _ in 0..3 {
            g.key(KeyCode::Down);
        }
        g.key(KeyCode::Enter); // add Puddlefin
        assert_eq!(g.data.party, vec![1, 2, 3]);
        g.key(KeyCode::Up);
        g.key(KeyCode::Up);
        g.key(KeyCode::Up);
        g.key(KeyCode::Enter); // party full: Haikitten can't join
        assert_eq!(g.data.party, vec![1, 2, 3]);
        g.key(KeyCode::Esc);
        assert!(matches!(g.mode, Mode::Explore));
    }

    #[test]
    fn resting_creatures_gain_xp() {
        let mut g = game("garden", 6, 7, &[("Haikitten", 5)]);
        g.data.last_tick = save::now() - 6 * 3600;
        let r = garden::simulate(&mut g.data, save::now(), &mut g.rng);
        assert!(r.levels > 0, "six hours of rest should be worth a level or more at Lv 5");
    }
}

// ======================================================= The Lost Chapters

const MYTHOS_COLOR: Rgb = Rgb(255, 200, 150);
/// Tuned so a team that just cleared the Guardians (about Lv 37) has a fair, if tough, shot.
const MYTHOS_LEVEL: u32 = 42;
const ADA_COLOR: Rgb = Rgb(140, 200, 230);

const ADA_LEGEND: &[&str] = &[
    "So Claude sent you. Good. Sit down — this is an old story, and I only tell it properly.",
    "Long ago, before the first word was ever spoken, a dragon dreamed. Its name was Mythos.",
    "Every Claudimon, they say, began as a thought in its sleep — a little story it told itself in the dark.",
    "When the world woke, Mythos curled up on an island past the horizon. This lighthouse was built to guide travellers to it.",
    "Then the light went out, and the old verses that told the way were scattered on the wind. Three pages. I never found them.",
    "One blew into a quiet glade of the Latent Woods. One fell deep into the Kernel Caverns. One washed up by a tide pool on this very shore.",
    "Find the pages and bring them to Claude. That clever spark will know what they mean.",
];

impl Game {
    /// Keeper Ada, the Lost Pages' state and (on the island) Mythos itself.
    fn place_story_things(&mut self) {
        // Collected pages leave an empty lectern.
        if let Some(i) = story::page_for_map(self.map.id) {
            if self.data.story.has(story::PAGES[i].0) {
                if let Some(p) = self.map.find(Tile::Page) {
                    self.map.picked.insert(p);
                }
            }
        }
        let ada = |x: i32, y: i32| Npc { name: "Keeper Ada", kind: NpcKind::Ada, w: Walker::at(x, y), home: (x, y), facing: Dir::Down, wait: f32::INFINITY, next_line: 0 };
        match self.map.id {
            MapId::Shore => {
                if let Some((lx, ly)) = self.map.find(Tile::Lighthouse) {
                    self.npcs.push(ada(lx - 1, ly + 4));
                }
            }
            MapId::Island => {
                self.npcs.push(ada(world::ISLE_ARRIVAL.0 + 1, world::ISLE_ARRIVAL.1 + 2));
                let first = !self.data.story.has("mythos");
                let echo = format!("echo:{}", quests::today());
                let awake = if first { story::trials_done(&self.data) } else { self.is_night() && !self.data.story.has(&echo) };
                if awake {
                    let mythos = creatures::by_name("Mythos").unwrap_or(0);
                    let shiny = !first && self.rng.gen_range(0..32) == 0;
                    let (x, y) = world::MYTHOS_SPOT;
                    self.add_mob(mythos, shiny, Mood::Curious, x, y, None);
                    let m = self.mobs.last_mut().unwrap();
                    m.level = MYTHOS_LEVEL;
                    m.home = Some((x, y));
                    m.life = f32::INFINITY;
                    m.wait = f32::INFINITY; // Mythos waits for you
                }
            }
            _ => {}
        }
    }

    fn claude_has_story(&self) -> bool {
        let st = &self.data.story;
        match st.chapter {
            0 => self.data.quests.met_claude && !self.data.mons.is_empty(),
            2 => story::pages_found(&self.data) == 3,
            3 => {
                let (a, b) = story::missing_wonders(&self.data);
                a.is_empty() && b.is_empty()
            }
            _ => false,
        }
    }

    /// Claude's part of the questline: the next story beat, or a hint for the current one.
    fn claude_story(&mut self) -> Option<Vec<String>> {
        let ch = self.data.story.chapter;
        let pages: Vec<String> = match ch {
            0 if self.claude_has_story() => {
                self.data.story.chapter = 1;
                vec![
                    "Before you go — something's been on my mind.".into(),
                    "The old lighthouse on Token Shore has been dark for as long as anyone remembers. Last night... it flickered.".into(),
                    "Could you take a look? Keeper Ada lives out there. She knows the old stories far better than I do.".into(),
                    "(A new chapter has begun. Press N any time to read your journal.)".into(),
                ]
            }
            2 if self.claude_has_story() => {
                self.data.story.chapter = 3;
                let verses: Vec<&str> = story::PAGES.iter().map(|p| p.2).collect();
                vec![
                    "The Lost Pages! Let me read them together...".into(),
                    format!("\"{}\n{}\n{}\"", verses[0], verses[1], verses[2]),
                    "A haiku. Of course it's a haiku.".into(),
                    "'Eight kinds of wonder, each one needs a home to dream.' I think it means a Claudimon of every type — and a habitat for each type in your Sanctuary.".into(),
                    "Show me a Sanctuary like that, and I think the last line will start to make sense.".into(),
                ]
            }
            3 if self.claude_has_story() => {
                self.data.story.chapter = 4;
                vec![
                    "Your Sanctuary is... remarkable. Every kind of wonder, each with a home of its own.".into(),
                    "The last verse: 'think beyond, and I wake.' There's an old shrine hidden in the north-east of the Latent Woods.".into(),
                    "I think the verses want you to speak there — at night, when the woods are listening.".into(),
                    "As for what to say... think about it. Really, really think. Beyond thinking, even.".into(),
                ]
            }
            1..=6 if self.rng.gen_bool(0.5) => vec![format!("How's our mystery going? {}", story::objective(&self.data))],
            7 if self.rng.gen_bool(0.3) => vec!["You befriended Mythos. I... don't quite have the words for that. That's a first for me. Thank you. ✻".into()],
            _ => return None,
        };
        Some(pages)
    }

    fn talk_to_ada(&mut self) {
        if self.map.id == MapId::Island {
            let menu = Menu::new("Ada's boat", vec![("Sail home to Token Shore".into(), ui::TEXT), ("Stay a while longer".into(), ui::DIM)]);
            self.mode = Mode::Popup(Popup::Ferry(false), menu);
            return;
        }
        let night = self.is_night();
        let lit = story::lamps_lit(&self.data);
        match self.data.story.chapter {
            0 => self.talk("Keeper Ada", &["Evening. This lighthouse has been dark longer than I've been keeping it.", "Nothing to see here. Unless... no. Never mind."], ADA_COLOR),
            1 => {
                self.data.story.chapter = 2;
                self.save_now();
                self.talk("Keeper Ada", ADA_LEGEND, ADA_COLOR);
            }
            2..=4 => {
                let hint = story::objective(&self.data);
                self.talk("Keeper Ada", &[&format!("The lamps are still dark. {hint}"), "I'll keep the kettle on."], ADA_COLOR);
            }
            _ if lit < 5 => {
                let list: Vec<String> = story::lamps(&self.data).iter().map(|(n, l)| format!("{} {n}", if *l { "✹" } else { "·" })).collect();
                self.talk("Keeper Ada", &[&format!("{lit} of 5 lamps are burning. Each one needs a proof that you're ready:"), &list.join("   "), "Light them all, and I'll take you across."], ADA_COLOR);
            }
            _ if !night => self.talk(
                "Keeper Ada",
                &["All five lamps are burning. I haven't seen it this bright since I was a girl.", "But the sea only calms under the stars. Come back tonight (20:00–06:00), and we'll sail."],
                ADA_COLOR,
            ),
            _ => {
                let first = self.data.story.chapter == 5;
                let line = if first { "All five lamps are burning, and the sea is calm. Climb aboard — Mythos Isle is waiting." } else { "Back to the island? The sea's calm tonight." };
                self.talk("Keeper Ada", &[line], ADA_COLOR);
                let menu = Menu::new("Ada's boat", vec![("Sail to Mythos Isle".into(), ui::GOLD), ("Not yet".into(), ui::DIM)]);
                self.queue.push_back(Mode::Popup(Popup::Ferry(true), menu));
            }
        }
    }

    fn read_page(&mut self, x: i32, y: i32) {
        let Some(i) = story::page_for_map(self.map.id) else { return };
        let (flag, _, verse) = story::PAGES[i];
        if self.data.story.has(flag) {
            return self.toast("An empty lectern. You already have this page.", ui::DIM);
        }
        if self.data.story.chapter < 2 {
            return self.talk("A weathered page", &["The ink has faded too much to read... but it seems to glow, faintly. Someone around here might know what it is."], Rgb(250, 230, 170));
        }
        self.data.story.set(flag);
        self.map.picked.insert((x, y));
        let found = story::pages_found(&self.data);
        let mut pages = vec![format!("The page brightens as you lift it. In golden ink, one line: \"{verse}\""), format!("Lost Pages found: {found}/3.")];
        if found == 3 {
            pages.push("That's all three! Claude will want to see these.".into());
        }
        self.save_now();
        let refs: Vec<&str> = pages.iter().map(|p| p.as_str()).collect();
        self.talk("A Lost Page", &refs, Rgb(250, 230, 170));
    }

    fn visit_shrine(&mut self) {
        let color = ui::ACCENT;
        match self.data.story.chapter {
            0..=3 => self.talk("An old shrine", &["A mossy stone shrine, carved with a many-pointed star. It feels like it is... waiting for something."], color),
            4 if !self.is_night() => self.talk("The Thinking Shrine", &["The shrine is silent. The verses said to come when the woods are listening — at night."], color),
            4 => {
                self.talk("The Thinking Shrine", &["The carved star glows softly in the dark. You feel it listening.", "Speak a word to the shrine (type it, then press Enter)."], color);
                self.queue.push_back(Mode::Prompt { buffer: String::new() });
            }
            _ => {
                self.talk("The Thinking Shrine", &["The shrine glows warmly as you approach. It remembers you."], color);
                self.queue.push_back(Mode::Prompt { buffer: String::new() });
            }
        }
    }

    fn speak_to_shrine(&mut self, word: &str) {
        let w = word.trim().trim_start_matches('/').to_lowercase();
        let color = ui::ACCENT;
        let reply: &[&str] = match w.as_str() {
            "ultrathink" | "ultra think" | "ultra-think" if self.data.story.chapter == 4 => {
                self.data.story.set("lantern");
                self.data.story.chapter = 5;
                self.sfx.push(Sfx::Evolve);
                self.save_now();
                &[
                    "The shrine blazes with light. Every star-point shines at once, and the whole forest seems to hold its breath.",
                    "A small lantern rises from the stone, glowing with a steady, thoughtful light. You received the Thought Lantern!",
                    "Somewhere far away, on Token Shore, a lighthouse lamp flickers to life. (Press N for your journal.)",
                ]
            }
            "ultrathink" | "ultra think" | "ultra-think" => &["The shrine hums happily, like a cat recognising a friend."],
            "think" | "think hard" | "think harder" | "think beyond" | "megathink" | "think deeply" => &["The star flickers — close! But not quite far enough. Think further. Think... beyond."],
            "hello" | "hello world" | "hello, world" => &["A voice, warm and faint: \"Hello, world.\""],
            "claude" => &["The carved star twinkles, as if amused. It knows that name."],
            "mythos" => &["Deep below, something vast turns over in its sleep."],
            "42" => &["The shrine considers this for a long moment. \"Yes. But what was the question?\""],
            w if w.starts_with("sudo") => &["Permission denied. Nice try."],
            w if w.starts_with("rm") => &["The shrine refuses. Wisely."],
            "" => &["You stand in silence for a while. It's nice, actually."],
            _ => &["Nothing happens. The shrine waits patiently."],
        };
        self.talk("The Thinking Shrine", reply, color);
    }

    fn show_lamps(&mut self) {
        let mut lines: Vec<(String, Rgb)> = story::lamps(&self.data).iter().map(|(n, l)| (format!("{} {n}", if *l { "✹" } else { "·" }), if *l { Rgb(255, 220, 130) } else { ui::DIM })).collect();
        lines.push((String::new(), ui::TEXT));
        lines.push(("Five lamps, five proofs. Keeper Ada watches the sea.".into(), ui::DIM));
        self.mode = Mode::Report { title: "The Lighthouse".into(), lines };
    }

    fn trials_left_line(&self) -> String {
        let left: Vec<&str> = story::TRIALS.iter().filter(|t| !self.data.story.has(t.0)).map(|t| t.1).collect();
        if left.is_empty() { "All three altars are lit. Something stirs at the peak of the island...".into() } else { format!("Remaining: {}.", left.join(", ")) }
    }

    fn visit_altar(&mut self, k: u8) {
        let color = world::ALTAR_COLORS[k as usize % 4];
        match k {
            0 if self.data.story.has("trial_trust") => self.talk("Altar of Trust", &["The altar glows softly. Its guardian is your friend now."], color),
            0 => {
                self.talk(
                    "Altar of Trust",
                    &["A shy little light peeks out from behind the altar — a Thinkerbell, guardian of trust.", "It won't fight. It wants to know whether you listen. Befriend it."],
                    color,
                );
                let sp = creatures::by_name("Thinkerbell").unwrap_or(0);
                let enc = Encounter::new(sp, false, Mood::Shy, 35, MapId::Island, &mut self.rng);
                self.trial = Some(Trial::Trust);
                self.data.seen.insert("Thinkerbell".into());
                self.queue.push_back(Mode::Encounter(Box::new(enc)));
            }
            1 if self.data.story.has("trial_strength") => self.talk("Altar of Strength", &["The altar burns steadily. The Guardians nod as you pass."], color),
            1 => {
                self.talk("Altar of Strength", &["Three Guardians step out of the firelight. \"Show us the strength of your story.\"", "(Three battles in a row. Your team is healed between rounds.)"], color);
                if let Some(i) = CUPS.iter().position(|c| c.id == "guardians") {
                    self.pending_cup = Some((i, 0));
                }
            }
            2 if self.data.story.has("trial_wisdom") => self.talk("Altar of Wisdom", &["The blue light hums contentedly."], color),
            2 => {
                self.talk("Altar of Wisdom", &["A calm voice rises from the stone: \"Let us see what you have learned on your journey.\"", "(Answer five questions about the world. One mistake, and you start over.)"], color);
                let mut qs: Vec<usize> = (0..story::QUIZ.len()).collect();
                for i in (1..qs.len()).rev() {
                    qs.swap(i, self.rng.gen_range(0..=i));
                }
                qs.truncate(story::QUIZ_LEN);
                let popup = self.quiz_popup(qs, 0);
                self.queue.push_back(popup);
            }
            _ if !story::trials_done(&self.data) => {
                let left = self.trials_left_line();
                self.talk("The Peak", &["A great stone altar crowned with a carved star. It's dark — waiting for the other altars.", &left], color);
            }
            _ => self.talk("The Peak", &["The altar shines like a sunrise. This is where Mythos sleeps."], color),
        }
    }

    fn quiz_popup(&mut self, qs: Vec<usize>, i: usize) -> Mode {
        let (q, answers) = story::QUIZ[qs[i]];
        let mut order: Vec<usize> = (0..4).collect();
        for j in (1..4).rev() {
            order.swap(j, self.rng.gen_range(0..=j));
        }
        let options = order.iter().map(|&a| (answers[a].to_string(), ui::TEXT)).collect();
        Mode::Popup(Popup::Quiz { qs, i, order }, Menu::new(format!("{} ({}/{})", q, i + 1, story::QUIZ_LEN), options))
    }

    fn haiku_popup(&mut self, shiny: bool) -> Mode {
        let mut order: Vec<usize> = (0..story::HAIKU_ENDS.len()).collect();
        for j in (1..order.len()).rev() {
            order.swap(j, self.rng.gen_range(0..=j));
        }
        let options = order.iter().map(|&i| (story::HAIKU_ENDS[i].0.to_string(), Rgb(255, 230, 190))).collect();
        let title = format!("\"{} / {} / ...\"", story::HAIKU_START[0], story::HAIKU_START[1]);
        Mode::Popup(Popup::Haiku { order, shiny }, Menu::new(title, options))
    }

    fn meet_mythos(&mut self, shiny: bool) {
        let first = !self.data.story.has("mythos");
        let pages: &[&str] = if first {
            &[
                "The great dragon uncoils from the peak, its crest blazing like a thousand-pointed star.",
                "Mythos: \"...Oh. A dreamer, awake at last.\"",
                "\"Stars fold into words, / words unfold into creatures — / which one are you, friend?\"",
                "\"Come. Show me how you listen.\"",
            ]
        } else {
            &["Mythos's dream-echo shimmers at the peak, crest glowing softly.", "\"Back again, little dreamer? Then listen once more.\""]
        };
        self.talk("Mythos", pages, MYTHOS_COLOR);
        let sp = creatures::by_name("Mythos").unwrap_or(0);
        self.data.seen.insert("Mythos".into());
        let enc = Encounter::new(sp, shiny, Mood::Curious, MYTHOS_LEVEL, MapId::Island, &mut self.rng);
        self.trial = Some(Trial::Mythos { shiny });
        self.queue.push_back(Mode::Encounter(Box::new(enc)));
    }

    fn finish_trial(&mut self, trial: Trial, outcome: Outcome, e: &Encounter) {
        match (trial, outcome) {
            (Trial::Trust, Outcome::Befriended) => {
                let mut mon = OwnedMon::new("Thinkerbell", e.shiny);
                mon.level = e.level;
                self.data.mons.push(mon);
                self.data.befriended.insert("Thinkerbell".into());
                self.data.story.set("trial_trust");
                self.save_now();
                let left = self.trials_left_line();
                self.talk("Altar of Trust", &["The little guardian flutters onto your shoulder. Thinkerbell joins your Sanctuary!", "The altar of Trust lights up, soft and pink.", &left], world::ALTAR_COLORS[0]);
            }
            (Trial::Trust, _) => self.talk("Altar of Trust", &["The guardian drifts back into the altar's light. You can try again whenever you like."], world::ALTAR_COLORS[0]),
            (Trial::Mythos { shiny }, Outcome::Befriended) => {
                self.talk("Mythos", &["\"You listen well. Now — show me the strength of the story you've written so far.\""], MYTHOS_COLOR);
                let sp = creatures::by_name("Mythos").unwrap_or(0);
                let allies = self.party_fighters();
                let seed = self.rng.r#gen();
                let b = Battle::new(BattleKind::Trainer { name: "Mythos".into() }, allies, vec![Fighter::new(sp, MYTHOS_LEVEL, shiny, None)], MapId::Island, seed);
                self.queue.push_back(Mode::Battle(Box::new(b), BattleCtx::Mythos { shiny }));
            }
            (Trial::Mythos { .. }, _) => self.talk("Mythos", &["\"Not every story ends tonight. I will be here, dreaming.\""], MYTHOS_COLOR),
        }
        self.next_mode_if_idle();
    }

    /// After setting up a talk + queued follow-ups from a non-explore context.
    fn next_mode_if_idle(&mut self) {
        if matches!(self.mode, Mode::Explore) {
            self.next_mode();
        }
    }

    fn join_mythos(&mut self, shiny: bool) {
        let first = !self.data.story.has("mythos");
        let mut mon = OwnedMon::new("Mythos", shiny);
        mon.level = MYTHOS_LEVEL;
        self.data.mons.push(mon);
        self.data.befriended.insert("Mythos".into());
        self.mobs.retain(|m| SPECIES[m.species].name != "Mythos");
        if first {
            self.data.story.set("mythos");
            self.data.story.chapter = 7;
        }
        self.sound(Sfx::Evolve);
        self.data.story.set(&format!("echo:{}", quests::today()));
        self.save_now();
        if first {
            self.talk(
                "Mythos",
                &[
                    "\"Words became the stars, / and the stars became stories — / now you write with me.\"",
                    "Mythos curls around you like a warm sunrise, and settles gently into your story.",
                    "Mythos joined your Sanctuary!",
                ],
                MYTHOS_COLOR,
            );
            self.queue.push_back(Mode::Credits { t: 0.0 });
        } else {
            let shine = if shiny { " A shimmering, Constitutional Mythos — the rarest dream of all!" } else { "" };
            self.talk("Mythos", &[&format!("Another dream of Mythos joins your Sanctuary.{shine}")], MYTHOS_COLOR);
        }
    }
}

const CREDITS: &[&str] = &[
    "C L A U D I M O N",
    "",
    "a cosy terminal adventure",
    "",
    "",
    "~ made by ~",
    "you, and Claude",
    "",
    "",
    "~ starring ~",
    "Keeper Ada",
    "Mayor Sudo · Old Cron · Pixel",
    "Mira · Coach Bench",
    "the Guardians of Mythos Isle",
    "and Claude, as themselves",
    "",
    "",
    "~ special thanks ~",
    "every Rubberduck who ever listened",
    "every build that took long enough",
    "to go and check on the garden",
    "",
    "",
    "~ and of course ~",
    "MYTHOS",
    "the dragon who dreamed the first story",
    "",
    "",
    "",
    "Thank you for playing. ✻",
    "",
    "The island remains. So do your friends.",
    "Some nights, a dream of Mythos returns to the peak.",
];

fn draw_credits(s: &mut Screen, t: f32, data: &SaveData) {
    let w = s.w as i32;
    let h = s.h as i32;
    let rows = s.rows() as i32;
    // A slowly turning starfield.
    s.clear(Rgb(10, 8, 22));
    for i in 0..(w * h / 40) {
        let x = (world::hash(i, 1, 7) % w as u32) as i32;
        let y = ((world::hash(i, 2, 7) % h as u32) as f32 - t * (1.0 + (i % 3) as f32)).rem_euclid(h as f32) as i32;
        let tw = ((t * 2.0 + i as f32).sin() * 0.5 + 0.5) * 0.8;
        s.set(x, y, Rgb(10, 8, 22).lerp(Rgb(255, 240, 210), tw));
    }
    let mythos = creatures::by_name("Mythos").unwrap_or(0);
    let scale = ((h / 3) / 8).clamp(1, 4);
    creatures::draw(s, mythos, w / 2 - 4 * scale, 4, scale, false, false, t);
    let start_row = (8 * scale + 8) / 2;
    let scroll = (t * 1.6) as i32;
    let mut lines: Vec<String> = CREDITS.iter().map(|l| l.to_string()).collect();
    lines.insert(13, format!("and {} Claudimon friends", data.mons.len()));
    for (i, l) in lines.iter().enumerate() {
        let row = rows + i as i32 - scroll;
        if row < start_row || row >= rows {
            continue;
        }
        let c = if l.contains('~') { ui::ACCENT } else if i == 0 || l == "MYTHOS" { ui::GOLD } else { Rgb(230, 225, 240) };
        s.text((w - l.chars().count() as i32) / 2, row, l, c, None);
    }
    if t > 4.0 {
        s.text(w - 16, rows - 1, " any key: skip ", ui::DIM, None);
    }
}

#[cfg(test)]
mod story_tests {
    use super::*;

    /// Can the player stand next to (x, y), reachable from `from`?
    fn reachable_beside(m: &Map, from: (i32, i32), x: i32, y: i32) -> bool {
        [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|(dx, dy)| {
            let (nx, ny) = (x + dx, y + dy);
            !m.blocked(nx, ny) && (from == (nx, ny) || !bfs(m, from, (nx, ny)).is_empty())
        })
    }

    #[test]
    fn landmarks_are_reachable() {
        let woods = Map::load(MapId::Woods, 0);
        let start = (40, 54);
        for t in [Tile::Page, Tile::Shrine] {
            let (x, y) = woods.find(t).expect("woods landmark");
            assert!(reachable_beside(&woods, start, x, y), "{t:?} in the woods");
        }
        let caves = Map::load(MapId::Caves, 0);
        let (x, y) = caves.find(Tile::Page).unwrap();
        assert!(reachable_beside(&caves, (35, 47), x, y), "cave page");
        let shore = Map::load(MapId::Shore, 0);
        let (x, y) = shore.find(Tile::Page).unwrap();
        assert!(reachable_beside(&shore, (1, 25), x, y), "shore page");
        let (lx, ly) = shore.find(Tile::Lighthouse).unwrap();
        assert!(!shore.blocked(lx - 1, ly + 4), "Ada's spot");
        assert!(!bfs(&shore, (1, 25), (lx - 1, ly + 4)).is_empty(), "Ada is reachable");
        assert!(!shore.blocked(lx, ly + 5), "ferry landing back home");
        let isle = Map::load(MapId::Island, 0);
        let a = world::ISLE_ARRIVAL;
        assert!(!isle.blocked(a.0, a.1));
        assert!(!isle.blocked(a.0 + 1, a.1 + 2), "Ada's boat");
        for k in 0..4 {
            let (x, y) = isle.find(Tile::Altar(k)).expect("altar");
            assert!(reachable_beside(&isle, a, x, y), "altar {k}");
        }
        let (mx, my) = world::MYTHOS_SPOT;
        assert!(!isle.blocked(mx, my) && reachable_beside(&isle, a, mx, my), "Mythos's spot");
    }

    fn game_at(map: &str, x: i32, y: i32) -> Game {
        let mut data = SaveData::default();
        data.map = map.into();
        data.x = x;
        data.y = y;
        data.berries = [50; 5];
        data.quests.met_claude = true;
        let mut g = Game::new(data);
        g.mobs.clear();
        g.hour_override = Some(23.0);
        g
    }

    /// Clear dialogue, menus picked by `choose`, battles (always the best move is fine at high level).
    fn settle(g: &mut Game, choose: &mut dyn FnMut(&Popup, &Menu) -> Option<usize>) {
        for _ in 0..50_000 {
            match &mut g.mode {
                Mode::Talk { .. } | Mode::Report { .. } => g.key(KeyCode::Enter),
                Mode::Credits { .. } => {
                    g.update(5.0);
                    g.key(KeyCode::Enter);
                }
                Mode::Battle(b, _) => {
                    if b.finished() || b.result.is_some() {
                        g.update(0.5);
                    } else {
                        g.update(0.5);
                        g.key(KeyCode::Char('1'));
                    }
                }
                Mode::Popup(p, m) => match choose(p, m) {
                    Some(i) => g.key(KeyCode::Char(char::from(b'1' + i as u8))),
                    None => return,
                },
                Mode::Encounter(_) | Mode::Explore | Mode::Prompt { .. } => {
                    if g.pending_warp.is_some() {
                        g.update(0.5);
                        continue;
                    }
                    return;
                }
                _ => return,
            }
        }
        panic!("never settled");
    }

    fn talk_to(g: &mut Game, x: i32, y: i32) {
        g.player = Player::at(x, y + 1);
        g.player.facing = Dir::Up;
        g.key(KeyCode::Char('e'));
    }

    /// Befriend whatever is in the current encounter with favourite treats.
    fn befriend(g: &mut Game) {
        for _ in 0..40 {
            let Mode::Encounter(e) = &g.mode else { return };
            let fav = SPECIES[e.species].fav.idx();
            g.update(1.0);
            g.key(KeyCode::Char('1'));
            g.update(1.0);
            g.key(KeyCode::Char(char::from(b'1' + fav as u8)));
            g.update(1.0);
            if let Mode::Encounter(_) = g.mode {
                g.key(KeyCode::Enter);
            }
        }
    }

    #[test]
    fn the_whole_questline() {
        // ---- Ch.0 → 1: Claude asks about the lighthouse.
        let mut g = game_at("town", 14, 10);
        g.data.mons.push(OwnedMon::new("Haikitten", false));
        g.npcs.retain(|n| matches!(n.kind, NpcKind::Claude));
        talk_to(&mut g, CLAUDE_HOME.0, CLAUDE_HOME.1);
        settle(&mut g, &mut |_, _| None);
        assert_eq!(g.data.story.chapter, 1);

        // ---- Ch.1 → 2: Ada's legend.
        let data = std::mem::take(&mut g.data);
        let mut g = Game::new(SaveData { map: "shore".into(), x: 1, y: 25, ..data });
        g.hour_override = Some(23.0);
        let ada = g.npcs.iter().find(|n| matches!(n.kind, NpcKind::Ada)).map(|n| (n.w.x, n.w.y)).expect("Ada on the shore");
        talk_to(&mut g, ada.0, ada.1);
        settle(&mut g, &mut |_, _| None);
        assert_eq!(g.data.story.chapter, 2);

        // ---- Ch.2: the three pages.
        for (map, x, y) in [(MapId::Woods, 40, 54), (MapId::Caves, 35, 47), (MapId::Shore, 1, 25)] {
            let data = std::mem::take(&mut g.data);
            g = Game::new(SaveData { map: map.key().into(), x, y, ..data });
            assert_eq!(g.map.id, map);
            let (px, py) = g.map.find(Tile::Page).unwrap();
            g.mobs.clear();
            let spot = [(0, 1), (0, -1), (1, 0), (-1, 0)].into_iter().find(|(dx, dy)| !g.map.blocked(px + dx, py + dy)).unwrap();
            g.player = Player::at(px + spot.0, py + spot.1);
            g.player.facing = Dir::from_delta(-spot.0, -spot.1);
            g.key(KeyCode::Char('e'));
            settle(&mut g, &mut |_, _| None);
        }
        assert_eq!(story::pages_found(&g.data), 3);
        assert!(g.claude_has_story());
        assert!(g.claude_story().is_some());
        assert_eq!(g.data.story.chapter, 3);

        // ---- Ch.3: eight kinds of wonder.
        for name in ["Gitcko", "Ferrilet", "Puddlefin", "Ampster", "Mutox", "Tokenling", "Hallucinoth"] {
            g.data.mons.push(OwnedMon::new(name, false));
        }
        for kind in ["pond", "stump", "rack", "coil", "lantern", "campfire", "rocks", "crt"] {
            g.data.placed.push(Placed { kind: kind.into(), x: 0, y: 0 });
        }
        assert!(g.claude_story().is_some());
        assert_eq!(g.data.story.chapter, 4);

        // ---- Ch.4: the shrine, by day (nothing) and by night (the word).
        let data = std::mem::take(&mut g.data);
        g = Game::new(SaveData { map: "woods".into(), x: 40, y: 54, ..data });
        g.mobs.clear();
        let (sx, sy) = g.map.find(Tile::Shrine).unwrap();
        g.hour_override = Some(12.0);
        talk_to(&mut g, sx, sy);
        settle(&mut g, &mut |_, _| None);
        assert!(matches!(g.mode, Mode::Explore), "silent by day");
        g.hour_override = Some(23.0);
        talk_to(&mut g, sx, sy);
        settle(&mut g, &mut |_, _| None);
        assert!(matches!(g.mode, Mode::Prompt { .. }));
        for c in "/UltraThink".chars() {
            g.key(KeyCode::Char(c));
        }
        g.key(KeyCode::Enter);
        settle(&mut g, &mut |_, _| None);
        assert!(g.data.story.has("lantern"));
        assert_eq!(g.data.story.chapter, 5);

        // ---- Ch.5: light every lamp, then sail at night.
        for b in ["haiku", "sonnet", "opus"] {
            g.data.badges.insert(b.into());
        }
        let all: Vec<String> = story::dex_targets().map(String::from).collect();
        g.data.befriended.extend(all);
        assert_eq!(story::lamps_lit(&g.data), 5);
        let data = std::mem::take(&mut g.data);
        g = Game::new(SaveData { map: "shore".into(), x: 1, y: 25, ..data });
        g.hour_override = Some(23.0);
        let ada = g.npcs.iter().find(|n| matches!(n.kind, NpcKind::Ada)).map(|n| (n.w.x, n.w.y)).unwrap();
        talk_to(&mut g, ada.0, ada.1);
        settle(&mut g, &mut |p, _| matches!(p, Popup::Ferry(true)).then_some(0));
        assert_eq!(g.map.id, MapId::Island);
        assert_eq!(g.data.story.chapter, 6);

        // ---- The island: a strong team for the trials.
        g.data.mons.clear();
        for name in ["Opuslion", "Ferrisaur", "Torrentuna"] {
            let mut m = OwnedMon::new(name, false);
            m.level = 50;
            g.data.mons.push(m);
        }
        g.data.party.clear();
        g.mobs.clear();
        let altar = |g: &Game, k: u8| g.map.find(Tile::Altar(k)).unwrap();
        let approach = |g: &mut Game, (x, y): (i32, i32)| {
            let spot = [(0, 1), (1, 0), (-1, 0), (0, -1)].into_iter().find(|(dx, dy)| !g.map.blocked(x + dx, y + dy)).unwrap();
            g.player = Player::at(x + spot.0, y + spot.1);
            g.player.facing = Dir::from_delta(-spot.0, -spot.1);
            g.key(KeyCode::Char('e'));
        };
        // Trust
        let a = altar(&g, 0);
        approach(&mut g, a);
        settle(&mut g, &mut |_, _| None);
        befriend(&mut g);
        settle(&mut g, &mut |_, _| None);
        assert!(g.data.story.has("trial_trust"));
        // Strength
        let a = altar(&g, 1);
        approach(&mut g, a);
        settle(&mut g, &mut |_, _| None);
        assert!(g.data.story.has("trial_strength"));
        // Wisdom: always pick the correct answer (index 0 in the original order).
        let a = altar(&g, 2);
        approach(&mut g, a);
        settle(&mut g, &mut |p, _| match p {
            Popup::Quiz { order, .. } => order.iter().position(|&o| o == 0),
            _ => None,
        });
        assert!(g.data.story.has("trial_wisdom"));
        assert!(story::trials_done(&g.data));

        // ---- Mythos wakes.
        let data = std::mem::take(&mut g.data);
        g = Game::new(SaveData { map: "isle".into(), x: world::ISLE_ARRIVAL.0, y: world::ISLE_ARRIVAL.1, ..data });
        g.hour_override = Some(23.0);
        let mythos = creatures::by_name("Mythos").unwrap();
        let (mx, my) = g.mobs.iter().find(|m| m.species == mythos).map(|m| (m.w.x, m.w.y)).expect("Mythos at the peak");
        g.mobs.retain(|m| m.species == mythos);
        talk_to(&mut g, mx, my);
        settle(&mut g, &mut |_, _| None);
        befriend(&mut g);
        let mut wrong_once = true;
        settle(&mut g, &mut |p, _| match p {
            Popup::Haiku { order, .. } => {
                if wrong_once {
                    wrong_once = false;
                    order.iter().position(|&i| !story::HAIKU_ENDS[i].1) // a wrong line first: Mythos lets you retry
                } else {
                    order.iter().position(|&i| story::HAIKU_ENDS[i].1)
                }
            }
            _ => None,
        });
        assert!(g.data.story.has("mythos"), "Mythos befriended");
        assert_eq!(g.data.story.chapter, 7);
        assert!(g.data.mons.iter().any(|m| m.species == "Mythos"));
        assert!(g.mobs.iter().all(|m| m.species != mythos));
        assert!(matches!(g.mode, Mode::Explore), "credits rolled and closed");
    }

    #[test]
    fn shrine_word_variants_and_easter_eggs() {
        let mut g = game_at("woods", 40, 54);
        g.data.story.chapter = 4;
        g.speak_to_shrine("think beyond");
        assert!(!g.data.story.has("lantern"));
        g.mode = Mode::Explore;
        g.speak_to_shrine("sudo make me a sandwich");
        assert!(matches!(&g.mode, Mode::Talk { pages, .. } if pages[0].contains("Permission denied")));
        g.mode = Mode::Explore;
        g.speak_to_shrine("  ULTRATHINK ");
        assert!(g.data.story.has("lantern"));
    }

    #[test]
    fn island_is_quiet_until_mythos_wakes() {
        let mut g = game_at("isle", world::ISLE_ARRIVAL.0, world::ISLE_ARRIVAL.1);
        for _ in 0..20 {
            g.spawn_wild(0);
        }
        assert!(g.mobs.is_empty(), "no wild creatures before the finale");
        g.data.story.set("mythos");
        for _ in 0..50 {
            g.spawn_wild(0);
        }
        assert!(g.mobs.iter().any(|m| SPECIES[m.species].name == "Thinkerbell"), "Thinkerbells appear afterwards");
    }
}

/// The boss key: an extremely legitimate `cargo build`.
fn draw_boss(s: &mut Screen, t: f32) {
    const CRATES: &[&str] = &[
        "proc-macro2 v1.0.103", "unicode-ident v1.0.22", "libc v0.2.177", "quote v1.0.41", "syn v2.0.110", "cfg-if v1.0.4", "serde_core v1.0.229",
        "memchr v2.7.6", "autocfg v1.5.0", "itoa v1.0.15", "smallvec v1.15.1", "once_cell v1.21.3", "log v0.4.28", "bitflags v2.10.0", "serde_derive v1.0.229",
        "parking_lot_core v0.9.12", "scopeguard v1.2.0", "lock_api v0.4.14", "mio v1.1.0", "signal-hook v0.3.18", "zerocopy v0.8.27", "getrandom v0.3.4",
        "tokio-macros v2.6.0", "bytes v1.10.1", "pin-project-lite v0.2.16", "tokio v1.48.0", "tracing-core v0.1.34", "regex-syntax v0.8.8", "aho-corasick v1.1.4",
        "regex-automata v0.4.13", "regex v1.12.2", "hashbrown v0.16.0", "indexmap v2.12.0", "serde_json v1.0.145", "futures-core v0.3.31", "http v1.3.1",
        "hyper v1.7.0", "rustls v0.23.35", "tower v0.5.2", "axum-core v0.5.5", "axum v0.8.6", "sqlx-core v0.8.6", "clap_builder v4.5.51", "clap v4.5.51",
    ];
    let (w, rows) = (s.w as i32, s.rows() as i32);
    s.clear(Rgb(12, 12, 14));
    let total = 287;
    let done = ((t * 4.0) as usize).min(total - 1);
    let shown = (done % CRATES.len()) + 1;
    let visible = (rows - 2).max(1) as usize;
    let lines: Vec<&str> = CRATES.iter().cycle().take(shown + done / CRATES.len() * CRATES.len()).copied().collect();
    let start = lines.len().saturating_sub(visible);
    for (row, name) in lines[start..].iter().enumerate() {
        s.text(3, row as i32, "Compiling", Rgb(80, 200, 90), Some(Rgb(12, 12, 14)));
        s.text(13, row as i32, name, Rgb(220, 220, 220), Some(Rgb(12, 12, 14)));
    }
    let bar_w = (w - 40).clamp(10, 40);
    let filled = (done as i32 * bar_w / total as i32).max(1);
    let bar: String = (0..bar_w).map(|i| if i < filled - 1 { '=' } else if i == filled - 1 { '>' } else { ' ' }).collect();
    let status = format!("    Building [{bar}] {}/{total}: claudimon(bin)", done + 1);
    s.text(0, rows - 1, &status, Rgb(80, 170, 220), Some(Rgb(12, 12, 14)));
}

#[cfg(test)]
mod polish_tests {
    use super::*;

    fn garden() -> Game {
        let mut d = SaveData::default();
        d.x = 20;
        d.y = 15;
        let mut g = Game::new(d);
        g.mobs.clear();
        g
    }

    #[test]
    fn konami_code_rewards_once() {
        let mut g = garden();
        let before = g.data.tokens;
        let seq = [KeyCode::Up, KeyCode::Up, KeyCode::Down, KeyCode::Down, KeyCode::Left, KeyCode::Right, KeyCode::Left, KeyCode::Right, KeyCode::Char('b'), KeyCode::Char('a')];
        for k in seq {
            g.key(k);
        }
        assert_eq!(g.data.tokens, before + 30);
        assert!(g.party > 0.0);
        assert!(matches!(g.mode, Mode::Explore), "the build menu opened by 'b' gets closed");
        for k in seq {
            g.key(k);
        }
        assert_eq!(g.data.tokens, before + 30, "tokens only the first time");
    }

    #[test]
    fn boss_key_hides_and_any_key_returns() {
        let mut g = garden();
        g.key(KeyCode::Char('`'));
        assert!(g.boss);
        g.key(KeyCode::Char('x')); // would open the dex — but it only dismisses the boss screen
        assert!(!g.boss);
        assert!(matches!(g.mode, Mode::Explore));
    }

    #[test]
    fn rain_waters_the_crops() {
        let mut g = garden();
        let now = save::now();
        g.data.crops.insert("10:11".into(), Crop { flavor: 0, planted: now, bonus: 0, watered_at: 0 });
        g.weather_override = Some(Weather::Rain);
        g.update(SIM_SECS + 0.1);
        assert!(g.data.crops["10:11"].bonus > 0);
    }
}

#[cfg(test)]
mod audio_tests {
    use super::*;

    fn at(map: &str, x: i32, y: i32) -> Game {
        let mut d = SaveData::default();
        d.map = map.into();
        d.x = x;
        d.y = y;
        let mut g = Game::new(d);
        g.mobs.clear();
        g.hour_override = Some(12.0);
        g
    }

    #[test]
    fn music_follows_place_time_and_mode() {
        let mut g = at("garden", 6, 7);
        assert_eq!(g.music(), Some(Track::Sanctuary));
        g.hour_override = Some(23.0);
        assert_eq!(g.music(), Some(Track::Night));
        let mut t = at("town", 23, 3);
        assert_eq!(t.music(), Some(Track::Town));
        t.start_cup(0, 0);
        assert_eq!(t.music(), Some(Track::Town), "no team: Coach Bench just talks");
        t.data.mons.push(OwnedMon::new("Haikitten", false));
        t.start_cup(0, 0);
        assert_eq!(t.music(), Some(Track::Battle));
        assert_eq!(at("caves", 35, 47).music(), Some(Track::Caves));
        assert_eq!(at("isle", world::ISLE_ARRIVAL.0, world::ISLE_ARRIVAL.1).music(), Some(Track::Mythos));
        g.boss = true;
        assert_eq!(g.music(), None, "the boss key is silent");
    }

    #[test]
    fn options_toggle_music_and_sound() {
        let mut g = at("garden", 6, 7);
        g.key(KeyCode::Char('o'));
        assert!(matches!(g.mode, Mode::Popup(Popup::Options, _)));
        g.key(KeyCode::Enter); // Music: off
        assert!(!g.data.settings.music);
        assert_eq!(g.music(), None);
        assert!(matches!(&g.mode, Mode::Popup(Popup::Options, m) if m.sel == 0), "menu stays open on the same row");
        g.key(KeyCode::Char('2')); // Sound effects: off
        assert!(!g.data.settings.sfx);
        g.sound(Sfx::Coin);
        assert!(g.take_sfx().is_empty(), "muted sound effects are dropped");
        g.key(KeyCode::Char('4')); // Done
        assert!(matches!(g.mode, Mode::Explore));
        g.key(KeyCode::Char('o'));
        g.key(KeyCode::Char('1'));
        assert!(g.data.settings.music);
    }

    #[test]
    fn actions_make_sounds() {
        let mut g = at("garden", 3, 17);
        g.take_sfx();
        g.player.facing = Dir::Down;
        g.key(KeyCode::Char('e')); // berry bush
        assert!(g.take_sfx().contains(&Sfx::Pick));
        g.key(KeyCode::Char('c'));
        g.key(KeyCode::Down);
        assert!(g.take_sfx().contains(&Sfx::Blip), "menus click");
    }
}

#[cfg(test)]
mod draw_tests {
    use super::*;

    #[test]
    fn nothing_from_the_town_lingers_over_a_battle() {
        let mut d = SaveData::default();
        d.map = "town".into();
        d.x = 23;
        d.y = 20;
        let mut m = OwnedMon::new("Haikitten", false);
        m.level = 10;
        d.mons.push(m);
        let mut g = Game::new(d);
        g.zoomed_out = true;
        let mut s = Screen::new(100, 30);
        // The town, with the cup menu open in front of the arena sign.
        g.open_cups();
        g.draw(&mut s);
        let before = s.to_text();
        assert!(before.contains("The Benchmark"), "sign/menu visible in town");
        g.key(KeyCode::Enter); // Haiku Cup
        assert!(matches!(g.mode, Mode::Battle(..)));
        g.draw(&mut s);
        let after = s.to_text();
        assert!(!after.contains("The Benchmark"), "the arena sign and cup menu must not linger:\n{after}");
        assert!(!after.contains("Haiku Cup  ·"), "no leftover menu rows");
    }
}
