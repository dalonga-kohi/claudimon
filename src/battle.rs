//! Turn-based Claudimon battles: friendly spars with wild creatures and Benchmark cups.
//!
//! Game logic resolves a whole turn instantly; the result is played back as a
//! script of steps (messages, HP drains, flashes, switches) so the screen tells
//! the story in order.

use crate::audio::Sfx;
use crate::creatures::{self, Elem, Stats, SPECIES};
use crate::encounter::draw_backdrop;
use crate::gfx::{Rgb, Screen};
use crate::ui;
use crate::world::MapId;
use crossterm::event::KeyCode;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    None,
    Drain,
    LowerAtk,
    LowerDef,
    LowerSpd,
    RaiseDef,
    RaiseAtkSpd,
    Priority,
    Recoil,
    Wild,
}

pub struct Move {
    pub name: &'static str,
    /// None = neutral (no type, no bonus).
    pub elem: Option<Elem>,
    pub power: u32,
    pub acc: u32,
    pub effect: Effect,
    /// Level at which a creature of this type learns it.
    pub learn: u32,
}

const fn mv(name: &'static str, elem: Option<Elem>, power: u32, acc: u32, effect: Effect, learn: u32) -> Move {
    Move { name, elem, power, acc, effect, learn }
}

use Effect as E;
pub static MOVES: &[Move] = &[
    mv("Nudge", None, 35, 100, E::None, 1),
    mv("Chain of Thought", None, 0, 100, E::RaiseAtkSpd, 6),
    mv("Leaf Flick", Some(Elem::Leaf), 40, 100, E::None, 1),
    mv("Vine Refactor", Some(Elem::Leaf), 60, 95, E::Drain, 9),
    mv("Photosynth Burst", Some(Elem::Leaf), 90, 90, E::None, 18),
    mv("Warm Up", Some(Elem::Ember), 40, 100, E::None, 1),
    mv("Flame Commit", Some(Elem::Ember), 65, 95, E::LowerDef, 9),
    mv("Hotfix Inferno", Some(Elem::Ember), 100, 90, E::Recoil, 18),
    mv("Splash Ping", Some(Elem::Tide), 40, 100, E::None, 1),
    mv("Undertow", Some(Elem::Tide), 60, 95, E::LowerSpd, 9),
    mv("Tsunami Merge", Some(Elem::Tide), 90, 90, E::None, 18),
    mv("Static Shock", Some(Elem::Spark), 40, 100, E::None, 1),
    mv("Quick Fetch", Some(Elem::Spark), 45, 100, E::Priority, 9),
    mv("Overclock Bolt", Some(Elem::Spark), 100, 90, E::Recoil, 18),
    mv("Pebble Toss", Some(Elem::Stone), 40, 100, E::None, 1),
    mv("Rock Solid", Some(Elem::Stone), 0, 100, E::RaiseDef, 9),
    mv("Boulder Push", Some(Elem::Stone), 95, 85, E::None, 18),
    mv("Bit Flip", Some(Elem::Byte), 40, 100, E::None, 1),
    mv("Cache Hit", Some(Elem::Byte), 50, 100, E::Priority, 9),
    mv("Buffer Overflow", Some(Elem::Byte), 105, 80, E::None, 18),
    mv("Daydream", Some(Elem::Dream), 40, 100, E::None, 1),
    mv("Lullaby", Some(Elem::Dream), 0, 95, E::LowerAtk, 9),
    mv("Haiku Burst", Some(Elem::Dream), 90, 95, E::None, 18),
    mv("Corrupt", Some(Elem::Glitch), 40, 100, E::Wild, 1),
    mv("Stack Smash", Some(Elem::Glitch), 65, 95, E::LowerDef, 9),
    mv("Segfault Strike", Some(Elem::Glitch), 110, 75, E::None, 18),
];

/// Damage multiplier of an attacking type against one defending type.
fn eff1(atk: Elem, def: Elem) -> f32 {
    use Elem::*;
    let (strong, weak): (&[Elem], &[Elem]) = match atk {
        Leaf => (&[Tide, Stone], &[Ember, Leaf, Glitch]),
        Ember => (&[Leaf, Byte], &[Tide, Stone, Ember]),
        Tide => (&[Ember, Stone], &[Leaf, Tide]),
        Spark => (&[Tide, Byte], &[Stone, Spark]),
        Stone => (&[Ember, Spark], &[Leaf, Tide]),
        Byte => (&[Glitch, Dream], &[Byte, Ember]),
        Dream => (&[Spark, Leaf], &[Dream, Glitch]),
        Glitch => (&[Dream, Leaf], &[Byte, Stone]),
    };
    if strong.contains(&def) {
        2.0
    } else if weak.contains(&def) {
        0.5
    } else {
        1.0
    }
}

pub fn effectiveness(atk: Option<Elem>, def: &[Elem]) -> f32 {
    match atk {
        Some(a) => def.iter().map(|d| eff1(a, *d)).product(),
        None => 1.0,
    }
}

/// The (up to) four moves a species knows at a level: the most advanced ones it can learn.
pub fn moveset(species: usize, level: u32) -> Vec<usize> {
    let types = SPECIES[species].types;
    let mut learnable: Vec<usize> = (0..MOVES.len())
        .filter(|&i| MOVES[i].learn <= level && MOVES[i].elem.is_none_or(|e| types.contains(&e)))
        .collect();
    learnable.sort_by_key(|&i| (std::cmp::Reverse(MOVES[i].learn), std::cmp::Reverse(MOVES[i].power)));
    learnable.truncate(4);
    if !learnable.iter().any(|&i| MOVES[i].power > 0) {
        let last = 3.min(learnable.len() - 1);
        learnable[last] = 0; // always keep something that does damage
    }
    learnable
}

/// XP for defeating a creature.
pub fn xp_for(species: usize, level: u32, trainer: bool) -> u32 {
    let base = (20 + 10 * SPECIES[species].rarity as u32) * level * 2 / 5;
    if trainer { base * 3 / 2 } else { base }
}

#[derive(Clone)]
pub struct Fighter {
    pub species: usize,
    pub shiny: bool,
    pub level: u32,
    pub hp: u32,
    pub stats: Stats,
    /// Stat stages for atk, def, spd (-3..=3).
    pub stages: [i32; 3],
    pub moves: Vec<usize>,
    /// Index into `SaveData::mons` for your own creatures.
    pub owned: Option<usize>,
    shown_hp: f32,
    target_hp: u32,
}

impl Fighter {
    pub fn new(species: usize, level: u32, shiny: bool, owned: Option<usize>) -> Fighter {
        let stats = creatures::stats_at(species, level);
        Fighter { species, shiny, level, hp: stats.hp, stats, stages: [0; 3], moves: moveset(species, level), owned, shown_hp: stats.hp as f32, target_hp: stats.hp }
    }
    pub fn name(&self) -> &'static str {
        SPECIES[self.species].name
    }
    fn stat(&self, i: usize) -> f32 {
        let base = [self.stats.atk, self.stats.def, self.stats.spd][i] as f32;
        base * (1.0 + 0.25 * self.stages[i] as f32).max(0.4)
    }
    pub fn fainted(&self) -> bool {
        self.hp == 0
    }
}

#[derive(Clone)]
pub enum BattleKind {
    /// A friendly spar with a wild creature during an encounter.
    Spar,
    Trainer { name: String },
}

enum Step {
    Text(String),
    Hp { ally: bool, idx: usize, hp: u32 },
    Flash { ally: bool },
    SwitchIn { ally: bool, idx: usize },
    Faint { ally: bool },
}

#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Moves,
    Switch,
}

pub struct Battle {
    pub kind: BattleKind,
    pub allies: Vec<Fighter>,
    pub foes: Vec<Fighter>,
    a: usize,
    f: usize,
    view_a: usize,
    view_f: usize,
    script: VecDeque<Step>,
    step_t: f32,
    text: String,
    menu: Menu,
    sel: usize,
    /// Some(true) = you won.
    pub result: Option<bool>,
    /// XP earned per ally (index into `allies`).
    pub xp: Vec<u32>,
    biome: MapId,
    time: f32,
    flash: [f32; 2],
    faint_anim: [f32; 2],
    rng: StdRng,
    /// Sounds to play, drained by the game.
    pub sfx: Vec<Sfx>,
}

const STEP_SECS: f32 = 1.0;

impl Battle {
    pub fn new(kind: BattleKind, allies: Vec<Fighter>, foes: Vec<Fighter>, biome: MapId, seed: u64) -> Battle {
        let n = allies.len();
        let intro = match &kind {
            BattleKind::Spar => format!("Friendly spar! {} squares up, eyes bright.", foes[0].name()),
            BattleKind::Trainer { name } if name == foes[0].name() => format!("{name} rises to meet you!"),
            BattleKind::Trainer { name } => format!("{name} wants to battle! {name} sends out {}!", foes[0].name()),
        };
        let mut b = Battle {
            kind,
            allies,
            foes,
            a: 0,
            f: 0,
            view_a: 0,
            view_f: 0,
            script: VecDeque::new(),
            step_t: 0.0,
            text: String::new(),
            menu: Menu::Moves,
            sel: 0,
            result: None,
            xp: vec![0; n],
            biome,
            time: 0.0,
            flash: [0.0; 2],
            faint_anim: [0.0; 2],
            rng: StdRng::seed_from_u64(seed),
            sfx: Vec::new(),
        };
        b.script.push_back(Step::Text(intro));
        let first = b.allies[0].name();
        b.script.push_back(Step::Text(format!("Go, {first}!")));
        b.next_step();
        b
    }

    /// Show a line before everything else that's queued.
    pub fn prepend(&mut self, text: String) {
        let current = std::mem::take(&mut self.text);
        self.script.push_front(Step::Text(current));
        self.text = text;
        self.step_t = STEP_SECS;
    }

    pub fn finished(&self) -> bool {
        self.result.is_some() && self.script.is_empty() && self.step_t <= 0.0
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        for side in [&mut self.allies, &mut self.foes] {
            for f in side.iter_mut() {
                let d = f.target_hp as f32 - f.shown_hp;
                f.shown_hp += d.signum() * (f.stats.hp as f32 * dt * 1.2).min(d.abs());
            }
        }
        self.flash = self.flash.map(|v| (v - dt).max(0.0));
        if self.step_t > 0.0 {
            self.step_t -= dt;
            if self.step_t <= 0.0 {
                self.next_step();
            }
        }
    }

    fn busy(&self) -> bool {
        self.step_t > 0.0
    }

    /// Start the next scripted step (or return control to the player).
    fn next_step(&mut self) {
        let Some(step) = self.script.pop_front() else {
            self.step_t = 0.0;
            return;
        };
        self.step_t = match step {
            Step::Text(t) => {
                if t.starts_with("It's super effective") || t.starts_with("A critical hit") {
                    self.sfx.push(Sfx::Super);
                } else if t.starts_with("You won the spar") || t.starts_with("You defeated") {
                    self.sfx.push(Sfx::Befriend);
                }
                self.text = t;
                STEP_SECS
            }
            Step::Hp { ally, idx, hp } => {
                let f = if ally { &mut self.allies[idx] } else { &mut self.foes[idx] };
                f.target_hp = hp;
                0.45
            }
            Step::Flash { ally } => {
                self.sfx.push(Sfx::Hit);
                self.flash[ally as usize] = 0.35;
                0.3
            }
            Step::SwitchIn { ally, idx } => {
                if ally {
                    self.view_a = idx;
                } else {
                    self.view_f = idx;
                }
                self.faint_anim[ally as usize] = 0.0;
                0.35
            }
            Step::Faint { ally } => {
                self.sfx.push(Sfx::Faint);
                self.faint_anim[ally as usize] = 1.0;
                0.6
            }
        };
    }

    pub fn key(&mut self, code: KeyCode) {
        if self.busy() {
            // Any action key hurries the current message along.
            if matches!(code, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e')) {
                self.step_t = 0.0;
                self.next_step();
            }
            return;
        }
        if self.result.is_some() {
            return;
        }
        let n_moves = self.allies[self.a].moves.len();
        let options = match self.menu {
            Menu::Moves => n_moves + 2, // moves, Switch, Yield
            Menu::Switch => self.allies.len() + 1,
        };
        match code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => self.sel = (self.sel + options - 1) % options,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Tab => self.sel = (self.sel + 1) % options,
            KeyCode::Char(c @ '1'..='6') => {
                let i = c as usize - '1' as usize;
                if i < options {
                    self.sel = i;
                    self.choose();
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => self.choose(),
            KeyCode::Esc | KeyCode::Backspace if self.menu == Menu::Switch => {
                self.menu = Menu::Moves;
                self.sel = 0;
            }
            _ => {}
        }
    }

    fn choose(&mut self) {
        match self.menu {
            Menu::Moves => {
                let n = self.allies[self.a].moves.len();
                if self.sel < n {
                    let m = self.allies[self.a].moves[self.sel];
                    self.turn(Some(m), None);
                } else if self.sel == n {
                    self.menu = Menu::Switch;
                    self.sel = 0;
                } else {
                    let msg = match self.kind {
                        BattleKind::Spar => "You call it a day. No hard feelings!",
                        BattleKind::Trainer { .. } => "You forfeit the match.",
                    };
                    self.script.push_back(Step::Text(msg.into()));
                    self.result = Some(false);
                    self.next_step();
                }
            }
            Menu::Switch => {
                let i = self.sel;
                self.menu = Menu::Moves;
                self.sel = 0;
                if i >= self.allies.len() || i == self.a || self.allies[i].fainted() {
                    return;
                }
                self.turn(None, Some(i));
            }
        }
    }

    /// Opponent move choice: best expected damage, with a dash of unpredictability.
    fn ai_pick(&mut self) -> usize {
        let me = &self.foes[self.f];
        let them = &self.allies[self.a];
        let mut best = (me.moves[0], f32::MIN);
        for &m in &me.moves {
            let mv = &MOVES[m];
            let mut score = if mv.power == 0 {
                let stage = match mv.effect {
                    E::RaiseDef => me.stages[1],
                    E::RaiseAtkSpd => me.stages[0],
                    E::LowerAtk => -them.stages[0],
                    _ => 3,
                };
                if stage < 2 { 25.0 } else { 0.0 }
            } else {
                let stab = if mv.elem.is_some_and(|e| SPECIES[me.species].types.contains(&e)) { 1.25 } else { 1.0 };
                mv.power as f32 * effectiveness(mv.elem, SPECIES[them.species].types) * stab * mv.acc as f32 / 100.0
            };
            score *= self.rng.gen_range(0.75..1.25);
            if score > best.1 {
                best = (m, score);
            }
        }
        best.0
    }

    /// Resolve one full turn: your action (a move or a switch) and the opponent's move.
    fn turn(&mut self, ally_move: Option<usize>, switch_to: Option<usize>) {
        let foe_move = self.ai_pick();
        if let Some(i) = switch_to {
            let back = self.allies[self.a].name();
            self.script.push_back(Step::Text(format!("{back}, come back! Go, {}!", self.allies[i].name())));
            self.a = i;
            self.script.push_back(Step::SwitchIn { ally: true, idx: i });
            self.attack(false, foe_move);
        } else if let Some(m) = ally_move {
            let prio = |mv: usize| MOVES[mv].effect == E::Priority;
            let ally_first = match (prio(m), prio(foe_move)) {
                (true, false) => true,
                (false, true) => false,
                _ => {
                    let (sa, sf) = (self.allies[self.a].stat(2), self.foes[self.f].stat(2));
                    sa > sf || (sa == sf && self.rng.gen_bool(0.5))
                }
            };
            let order = if ally_first { [(true, m), (false, foe_move)] } else { [(false, foe_move), (true, m)] };
            let (fa, ff) = (self.a, self.f);
            for (ally, mv) in order {
                // A creature knocked out (or swapped in) this turn doesn't get to act.
                let still_there = if ally { self.a == fa && !self.allies[fa].fainted() } else { self.f == ff && !self.foes[ff].fainted() };
                if !still_there || self.result.is_some() {
                    continue;
                }
                self.attack(ally, mv);
            }
        }
        self.next_step();
    }

    fn attack(&mut self, ally: bool, m: usize) {
        let mv = &MOVES[m];
        let (ai, di) = if ally { (self.a, self.f) } else { (self.f, self.a) };
        let (atk_name, atk_species, atk_level, atk_power) = {
            let a = if ally { &self.allies[ai] } else { &self.foes[ai] };
            (a.name(), a.species, a.level, a.stat(0))
        };
        let (def_species, def_guard) = {
            let d = if ally { &self.foes[di] } else { &self.allies[di] };
            (d.species, d.stat(1))
        };
        let who = if ally { atk_name.to_string() } else { format!("The opposing {atk_name}") };
        self.script.push_back(Step::Text(format!("{who} used {}!", mv.name)));
        if self.rng.gen_range(0..100) >= mv.acc {
            self.script.push_back(Step::Text("...but it missed!".into()));
            return;
        }
        if mv.power == 0 {
            let (target_self, stat, delta) = match mv.effect {
                E::RaiseDef => (true, 1, 1),
                E::RaiseAtkSpd => (true, 0, 1),
                E::LowerAtk => (false, 0, -1),
                _ => (true, 0, 0),
            };
            let applies_to_ally = ally == target_self;
            self.change_stage(applies_to_ally, stat, delta);
            if mv.effect == E::RaiseAtkSpd {
                self.change_stage(applies_to_ally, 2, 1);
            }
            return;
        }
        let power = if mv.effect == E::Wild { self.rng.gen_range(20..=90) } else { mv.power } as f32;
        let eff = effectiveness(mv.elem, SPECIES[def_species].types);
        let stab = if mv.elem.is_some_and(|e| SPECIES[atk_species].types.contains(&e)) { 1.25 } else { 1.0 };
        let crit = self.rng.gen_range(0..16) == 0;
        let base = (2.0 * atk_level as f32 / 5.0 + 2.0) * power * atk_power / def_guard.max(1.0) / 50.0 + 2.0;
        let roll = self.rng.gen_range(0.85..=1.0);
        let dmg = ((base * stab * eff * roll * if crit { 1.5 } else { 1.0 }).round() as u32).max(1);
        let target = if ally { &mut self.foes[di] } else { &mut self.allies[di] };
        let dealt = dmg.min(target.hp);
        target.hp -= dealt;
        let (thp, tname) = (target.hp, target.name());
        self.script.push_back(Step::Flash { ally: !ally });
        self.script.push_back(Step::Hp { ally: !ally, idx: di, hp: thp });
        if crit {
            self.script.push_back(Step::Text("A critical hit!".into()));
        }
        if eff > 1.0 {
            self.script.push_back(Step::Text("It's super effective!".into()));
        } else if eff < 1.0 {
            self.script.push_back(Step::Text("It's not very effective...".into()));
        }
        match mv.effect {
            E::Drain if dealt > 0 => {
                let me = if ally { &mut self.allies[ai] } else { &mut self.foes[ai] };
                me.hp = (me.hp + dealt / 2).min(me.stats.hp);
                let (hp, n) = (me.hp, me.name());
                self.script.push_back(Step::Hp { ally, idx: ai, hp });
                self.script.push_back(Step::Text(format!("{n} drew energy from {tname}!")));
            }
            E::Recoil => {
                let me = if ally { &mut self.allies[ai] } else { &mut self.foes[ai] };
                let r = (dealt / 4).max(1).min(me.hp.saturating_sub(1));
                me.hp -= r;
                let (hp, n) = (me.hp, me.name());
                self.script.push_back(Step::Hp { ally, idx: ai, hp });
                self.script.push_back(Step::Text(format!("{n} is strained by the effort!")));
            }
            E::LowerDef | E::LowerSpd | E::LowerAtk if thp > 0 && self.rng.gen_bool(0.3) => {
                let stat = match mv.effect {
                    E::LowerAtk => 0,
                    E::LowerDef => 1,
                    _ => 2,
                };
                self.change_stage(!ally, stat, -1);
            }
            _ => {}
        }
        if thp == 0 {
            self.knocked_out(!ally);
        }
    }

    fn change_stage(&mut self, ally: bool, stat: usize, delta: i32) {
        let f = if ally { &mut self.allies[self.a] } else { &mut self.foes[self.f] };
        let name = f.name();
        let label = ["attack", "defence", "speed"][stat];
        let before = f.stages[stat];
        f.stages[stat] = (before + delta).clamp(-3, 3);
        let msg = if f.stages[stat] == before {
            format!("{name}'s {label} won't go any {}!", if delta > 0 { "higher" } else { "lower" })
        } else if delta > 0 {
            format!("{name}'s {label} rose!")
        } else {
            format!("{name}'s {label} fell!")
        };
        self.script.push_back(Step::Text(msg));
    }

    fn knocked_out(&mut self, ally: bool) {
        if ally {
            let name = self.allies[self.a].name();
            self.script.push_back(Step::Faint { ally: true });
            self.script.push_back(Step::Text(format!("{name} is too tired to continue!")));
            match (0..self.allies.len()).find(|&i| !self.allies[i].fainted()) {
                Some(next) => {
                    self.a = next;
                    self.script.push_back(Step::Text(format!("Go, {}!", self.allies[next].name())));
                    self.script.push_back(Step::SwitchIn { ally: true, idx: next });
                }
                None => {
                    self.script.push_back(Step::Text("Your team is out of energy...".into()));
                    self.result = Some(false);
                }
            }
        } else {
            let foe = &self.foes[self.f];
            let name = foe.name();
            let trainer = matches!(self.kind, BattleKind::Trainer { .. });
            let xp = xp_for(foe.species, foe.level, trainer);
            self.script.push_back(Step::Faint { ally: false });
            let who = if trainer { format!("The opposing {name}") } else { name.to_string() };
            self.script.push_back(Step::Text(format!("{who} is worn out!")));
            for (i, a) in self.allies.iter().enumerate() {
                if !a.fainted() {
                    self.xp[i] += xp;
                }
            }
            self.script.push_back(Step::Text(format!("Your team gained {xp} XP!")));
            match (0..self.foes.len()).find(|&i| !self.foes[i].fainted()) {
                Some(next) => {
                    self.f = next;
                    if let BattleKind::Trainer { name } = &self.kind {
                        self.script.push_back(Step::Text(format!("{name} sends out {}!", self.foes[next].name())));
                    }
                    self.script.push_back(Step::SwitchIn { ally: false, idx: next });
                }
                None => {
                    let msg = match &self.kind {
                        BattleKind::Spar => "You won the spar!".to_string(),
                        BattleKind::Trainer { name } => format!("You defeated {name}!"),
                    };
                    self.script.push_back(Step::Text(msg));
                    self.result = Some(true);
                }
            }
        }
    }

    // ------------------------------------------------------------ drawing

    pub fn draw(&self, s: &mut Screen) {
        let w = s.w as i32;
        let rows = s.rows() as i32;
        let panel_h = 8;
        let scene_rows = (rows - panel_h).max(6);
        let scene_h = scene_rows * 2;
        let horizon = scene_h * 50 / 100;
        draw_backdrop(s, self.biome, crate::clock::Season::now(), w, scene_h, horizon, self.time);

        let scale = ((scene_h - 4) / 12).clamp(1, 5).min(w / 24).max(1);
        let size = 8 * scale;
        // Foe: upper right. Ally: lower left, facing right.
        let foe_pos = (w * 68 / 100 - size / 2, horizon - size + scale * 2);
        let ally_pos = (w * 26 / 100 - size / 2, scene_h - size - 2);
        for (ally, (x, y), f, idx) in [(false, foe_pos, &self.foes, self.view_f), (true, ally_pos, &self.allies, self.view_a)] {
            let fighter = &f[idx];
            // Platform.
            let pr = size / 2 + scale * 3;
            for yy in -scale..=scale {
                for xx in -pr..=pr {
                    let k = (xx * xx) as f32 / (pr * pr) as f32 + (yy * yy) as f32 / ((scale * scale).max(1)) as f32;
                    if k <= 1.0 {
                        s.blend(x + size / 2 + xx, y + size + yy, Rgb(20, 30, 20), 0.35);
                    }
                }
            }
            let faint = self.faint_anim[ally as usize] > 0.0 && fighter.fainted();
            if faint {
                continue;
            }
            let shake = if self.flash[ally as usize] > 0.0 { ((self.time * 50.0).sin() * scale as f32 * 1.5) as i32 } else { 0 };
            let bob = if (self.time * 2.0 + ally as i32 as f32).sin() > 0.6 { scale / 2 } else { 0 };
            creatures::draw(s, fighter.species, x + shake, y - bob, scale, ally, fighter.shiny, self.time);
            if self.flash[ally as usize] > 0.15 {
                for yy in 0..size {
                    for xx in 0..size {
                        s.blend(x + shake + xx, y - bob + yy, Rgb(255, 255, 255), 0.35);
                    }
                }
            }
        }

        // HP boxes (text layer).
        let foe = &self.foes[self.view_f];
        let ally = &self.allies[self.view_a];
        self.hp_box(s, 1, 1, foe, false);
        let ally_row = (scene_rows - 4).max(4);
        self.hp_box(s, w - 32, ally_row, ally, true);
        // Remaining team pips.
        let pips = |team: &Vec<Fighter>| team.iter().map(|f| if f.fainted() { '○' } else { '●' }).collect::<String>();
        s.text(1, 4, &pips(&self.foes), Rgb(240, 200, 120), None);
        s.text(w - 32, ally_row + 3, &pips(&self.allies), Rgb(240, 200, 120), None);

        self.draw_panel(s, scene_rows, panel_h);
    }

    fn hp_box(&self, s: &mut Screen, col: i32, row: i32, f: &Fighter, ally: bool) {
        let bg = Rgb(28, 24, 38);
        let title = format!(" {}{}  Lv{} ", if f.shiny { "✦" } else { "" }, f.name(), f.level);
        s.text(col, row, &format!("{title:<30}"), ui::TEXT, Some(bg));
        let frac = (f.shown_hp / f.stats.hp as f32).clamp(0.0, 1.0);
        let c = if frac > 0.5 { Rgb(110, 220, 120) } else if frac > 0.2 { Rgb(240, 200, 80) } else { Rgb(240, 90, 80) };
        s.text(col, row + 1, " HP ", ui::DIM, Some(bg));
        let bw = 18;
        let filled = (frac * bw as f32).ceil() as i32;
        for i in 0..bw {
            s.text(col + 4 + i, row + 1, if i < filled { "█" } else { "░" }, if i < filled { c } else { Rgb(70, 64, 84) }, Some(bg));
        }
        let hp = if ally { format!(" {:>3}/{:<3}", f.shown_hp.round() as u32, f.stats.hp) } else { " ".repeat(8) };
        s.text(col + 4 + bw, row + 1, &hp, ui::TEXT, Some(bg));
        let types: String = SPECIES[f.species].types.iter().map(|e| e.name()).collect::<Vec<_>>().join("/");
        s.text(col, row + 2, &format!(" {types:<29}"), ui::DIM, Some(bg));
    }

    fn draw_panel(&self, s: &mut Screen, top: i32, h: i32) {
        let w = s.w as i32;
        let bg = ui::BG;
        s.panel(0, top, w, h, bg, ui::BORDER);
        let fit = |t: &str| -> String { t.chars().take((w - 4) as usize).collect() };
        if self.busy() || self.result.is_some() || !self.script.is_empty() {
            s.text(2, top + 2, &fit(&self.text), ui::TEXT, Some(bg));
            if self.busy() {
                s.text(w - 10, top + h - 1, " ▼ next ", ui::DIM, Some(bg));
            }
            return;
        }
        let ally = &self.allies[self.a];
        s.text(2, top + 1, &fit(&format!("What will {} do?", ally.name())), ui::TEXT, Some(bg));
        match self.menu {
            Menu::Moves => {
                let col_w = ((w - 4) / 2).max(20);
                for (i, &m) in ally.moves.iter().enumerate() {
                    let mv = &MOVES[m];
                    let (cx, cy) = (2 + (i as i32 % 2) * col_w, top + 2 + i as i32 / 2);
                    let sel = i == self.sel;
                    let tc = mv.elem.map(|e| e.color()).unwrap_or(Rgb(200, 200, 210));
                    let eff = effectiveness(mv.elem, SPECIES[self.foes[self.f].species].types);
                    let hint = if mv.power == 0 { "status" } else if eff > 1.0 { "▲ strong" } else if eff < 1.0 { "▼ weak" } else { "" };
                    let label = format!(" {} {:<16} {:>3} {:<8}", i + 1, mv.name, if mv.power > 0 { mv.power.to_string() } else { "—".into() }, hint);
                    let label: String = label.chars().take(col_w as usize - 1).collect();
                    let (fg, b) = if sel { (Rgb(24, 18, 28), tc) } else { (tc, bg) };
                    s.text(cx, cy, &label, fg, Some(b));
                }
                let n = ally.moves.len();
                let yield_label = match self.kind {
                    BattleKind::Spar => "Call it a day",
                    BattleKind::Trainer { .. } => "Forfeit",
                };
                for (j, label) in ["Switch", yield_label].iter().enumerate() {
                    let sel = self.sel == n + j;
                    let text = format!(" {} {label} ", n + j + 1);
                    let (fg, b) = if sel { (Rgb(24, 18, 28), ui::GOLD) } else { (ui::DIM, bg) };
                    s.text(2 + j as i32 * 22, top + 5, &text, fg, Some(b));
                }
            }
            Menu::Switch => {
                for (i, f) in self.allies.iter().enumerate() {
                    let sel = i == self.sel;
                    let status = if f.fainted() {
                        "tired".to_string()
                    } else if i == self.a {
                        "in battle".to_string()
                    } else {
                        format!("{}/{} HP", f.hp, f.stats.hp)
                    };
                    let text = format!(" {} {:<14} Lv{:<3} {status} ", i + 1, f.name(), f.level);
                    let (fg, b) = if sel { (Rgb(24, 18, 28), ui::GOLD) } else { (if f.fainted() { ui::DIM } else { ui::TEXT }, bg) };
                    s.text(2, top + 2 + i as i32, &text, fg, Some(b));
                }
                s.text(2, top + h - 2, "esc: back", ui::DIM, Some(bg));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(name: &str) -> usize {
        creatures::by_name(name).unwrap()
    }

    #[test]
    fn type_chart_basics() {
        assert_eq!(effectiveness(Some(Elem::Tide), &[Elem::Ember]), 2.0);
        assert_eq!(effectiveness(Some(Elem::Ember), &[Elem::Tide]), 0.5);
        assert_eq!(effectiveness(Some(Elem::Tide), &[Elem::Ember, Elem::Stone]), 4.0);
        assert_eq!(effectiveness(None, &[Elem::Glitch]), 1.0);
        // Every type is strong against something and resisted by something.
        for e in [Elem::Leaf, Elem::Ember, Elem::Tide, Elem::Spark, Elem::Stone, Elem::Byte, Elem::Dream, Elem::Glitch] {
            let all = [Elem::Leaf, Elem::Ember, Elem::Tide, Elem::Spark, Elem::Stone, Elem::Byte, Elem::Dream, Elem::Glitch];
            assert!(all.iter().any(|d| eff1(e, *d) > 1.0) && all.iter().any(|d| eff1(e, *d) < 1.0), "{e:?}");
        }
    }

    #[test]
    fn movesets_grow_with_level() {
        let kit = sp("Haikitten");
        let low = moveset(kit, 1);
        assert!(low.iter().any(|&m| MOVES[m].power > 0));
        let high = moveset(kit, 30);
        assert_eq!(high.len(), 4);
        assert!(high.iter().any(|&m| MOVES[m].name == "Haiku Burst"));
        for i in 0..SPECIES.len() {
            for lvl in [1, 5, 12, 25, 50] {
                let ms = moveset(i, lvl);
                assert!(!ms.is_empty() && ms.len() <= 4);
                assert!(ms.iter().any(|&m| MOVES[m].power > 0), "{} lv{lvl} has no attack", SPECIES[i].name);
            }
        }
    }

    #[test]
    fn evolution_makes_you_stronger() {
        let (a, b, c) = (sp("Haikitten"), sp("Sonnetiger"), sp("Opuslion"));
        assert_eq!((creatures::stage(a), creatures::stage(b), creatures::stage(c)), (0, 1, 2));
        let total = |i: usize| {
            let s = creatures::stats_at(i, 30);
            s.hp + s.atk + s.def + s.spd
        };
        assert!(total(a) < total(b) && total(b) < total(c));
    }

    /// Mash the first move until the battle ends.
    fn auto_play(b: &mut Battle) -> bool {
        for _ in 0..5000 {
            if b.finished() {
                return b.result.unwrap();
            }
            if b.busy() || !b.script.is_empty() {
                b.update(0.5);
            } else {
                b.key(KeyCode::Char('1'));
            }
        }
        panic!("battle never ended");
    }

    #[test]
    fn stronger_team_wins_and_earns_xp() {
        let ally = vec![Fighter::new(sp("Opuslion"), 40, false, Some(0))];
        let foe = vec![Fighter::new(sp("Snailoop"), 5, false, None)];
        let mut b = Battle::new(BattleKind::Spar, ally, foe, MapId::Meadow, 1);
        assert!(auto_play(&mut b));
        assert!(b.xp[0] > 0);
    }

    #[test]
    fn weaker_team_loses_and_trainers_send_out_their_whole_team() {
        let ally = vec![Fighter::new(sp("Snailoop"), 3, false, Some(0))];
        let foes = vec![Fighter::new(sp("Mutox"), 30, false, None), Fighter::new(sp("Mutox"), 30, false, None)];
        let mut b = Battle::new(BattleKind::Trainer { name: "Test".into() }, ally, foes, MapId::Town, 2);
        assert!(!auto_play(&mut b));

        let allies = vec![Fighter::new(sp("Opuslion"), 50, false, Some(0))];
        let foes = vec![Fighter::new(sp("Snailoop"), 3, false, None), Fighter::new(sp("Cachew"), 3, false, None), Fighter::new(sp("Tokenling"), 3, false, None)];
        let mut b = Battle::new(BattleKind::Trainer { name: "Test".into() }, allies, foes, MapId::Town, 3);
        assert!(auto_play(&mut b));
        assert!(b.foes.iter().all(|f| f.fainted()), "had to beat all three");
    }

    #[test]
    fn switching_and_forfeit() {
        let allies = vec![Fighter::new(sp("Haikitten"), 10, false, Some(0)), Fighter::new(sp("Puddlefin"), 10, false, Some(1))];
        let foes = vec![Fighter::new(sp("Ferrilet"), 10, false, None)];
        let mut b = Battle::new(BattleKind::Spar, allies, foes, MapId::Shore, 4);
        while b.busy() || !b.script.is_empty() {
            b.update(0.5);
        }
        let n = b.allies[0].moves.len();
        b.key(KeyCode::Char(char::from(b'1' + n as u8))); // Switch
        b.key(KeyCode::Char('2')); // Puddlefin
        assert_eq!(b.a, 1);
        while b.busy() || !b.script.is_empty() {
            b.update(0.5);
        }
        let n = b.allies[1].moves.len();
        b.key(KeyCode::Char(char::from(b'2' + n as u8))); // Call it a day
        while b.busy() || !b.script.is_empty() {
            b.update(0.5);
        }
        assert_eq!(b.result, Some(false));
    }
}

#[cfg(test)]
mod balance {
    use super::*;
    use crate::arena::CUPS;

    /// A sensible player: picks the move with the best expected damage.
    fn smart(b: &mut Battle) -> usize {
        let me = &b.allies[b.a];
        let them = &b.foes[b.f];
        let score = |m: usize| {
            let mv = &MOVES[m];
            let stab = if mv.elem.is_some_and(|e| SPECIES[me.species].types.contains(&e)) { 1.25 } else { 1.0 };
            mv.power as f32 * effectiveness(mv.elem, SPECIES[them.species].types) * stab * mv.acc as f32
        };
        (0..me.moves.len()).max_by(|&x, &y| score(me.moves[x]).total_cmp(&score(me.moves[y]))).unwrap()
    }

    fn run_cup(cup: usize, team: &[(&str, u32)], seed: u64) -> usize {
        for (round, t) in CUPS[cup].trainers.iter().enumerate() {
            let allies = team.iter().map(|(n, l)| Fighter::new(crate::creatures::by_name(n).unwrap(), *l, false, Some(0))).collect();
            let foes = t.team.iter().map(|(n, l)| Fighter::new(crate::creatures::by_name(n).unwrap(), *l, false, None)).collect();
            let mut b = Battle::new(BattleKind::Trainer { name: t.name.into() }, allies, foes, MapId::Town, seed * 7 + round as u64);
            loop {
                if b.finished() {
                    break;
                }
                if b.busy() || !b.script.is_empty() || b.result.is_some() {
                    b.update(0.5);
                } else {
                    let i = smart(&mut b);
                    b.key(KeyCode::Char(char::from(b'1' + i as u8)));
                }
            }
            if b.result != Some(true) {
                return round;
            }
        }
        3
    }

    /// `cargo test cup_balance -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cup_balance() {
        let teams: Vec<(String, [(&str, u32); 3])> = [9, 10, 11]
            .iter()
            .map(|&l| (format!("meadow Lv {l}"), [("Haikitten", l), ("Promptpup", l), ("Cachew", l)]))
            .chain([18, 20, 22].iter().map(|&l| (format!("evolved Lv {l}"), [("Sonnetiger", l), ("Mutox", l), ("Torrentuna", l)])))
            .chain([28, 30, 32].iter().map(|&l| (format!("evolved Lv {l}"), [("Sonnetiger", l), ("Ferrisaur", l), ("Torrentuna", l)])))
            .collect();
        for (cup, c) in CUPS.iter().enumerate() {
            for (label, team) in teams.iter().skip(cup * 3).take(3) {
                let runs = 40;
                let wins = (0..runs).filter(|&s| run_cup(cup, team, s) == 3).count();
                println!("{:<11} {label:<20} wins {:>3}%", c.name, wins * 100 / runs as usize);
            }
        }
    }

    /// `cargo test mythos_balance -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mythos_balance() {
        let mythos = crate::creatures::by_name("Mythos").unwrap();
        for lvl in [34, 36, 38, 40, 42] {
            let team = [("Opuslion", lvl), ("Ferrisaur", lvl), ("Torrentuna", lvl)];
            let runs = 40;
            let wins = (0..runs)
                .filter(|&seed| {
                    let allies = team.iter().map(|(n, l)| Fighter::new(crate::creatures::by_name(n).unwrap(), *l, false, Some(0))).collect();
                    let mut b = Battle::new(BattleKind::Trainer { name: "Mythos".into() }, allies, vec![Fighter::new(mythos, 42, false, None)], MapId::Island, seed);
                    loop {
                        if b.finished() {
                            break b.result == Some(true);
                        }
                        if b.busy() || !b.script.is_empty() || b.result.is_some() {
                            b.update(0.5);
                        } else {
                            let i = smart(&mut b);
                            b.key(KeyCode::Char(char::from(b'1' + i as u8)));
                        }
                    }
                })
                .count();
            println!("team Lv {lvl} vs Mythos Lv 42: wins {:>3}%", wins * 100 / runs as usize);
        }
    }
}
