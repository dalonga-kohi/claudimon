//! Befriending encounters. A wild Claudimon shows a mood (its "prompt");
//! respond with the right action to build trust before its patience runs out.

use crate::audio::Sfx;
use crate::clock::{self, Season};
use crate::creatures::{self, Flavor, Mood, FLAVORS, MOODS, SPECIES};
use crate::gfx::{Rgb, Screen};
use crate::save::SaveData;
use crate::world::{self, MapId};
use crossterm::event::KeyCode;
use rand::Rng;
use rand::rngs::StdRng;

pub enum Outcome {
    Befriended,
    Fled,
    Left,
    /// The player wants a friendly battle; the encounter resumes afterwards.
    Spar,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Menu,
    Treat,
    Play { t: f32, center: f32 },
    Done { won: bool, t: f32 },
}

const ACTIONS: [(&str, &str); 6] = [
    ("Treat", "offer a berry"),
    ("Play", "timing game"),
    ("Calm", "sit quietly"),
    ("Observe", "learn about it"),
    ("Spar", "friendly battle with your team — strong-willed creatures respect a good match"),
    ("Leave", "walk away"),
];

const GOOD: f32 = 0.11;
const PERFECT: f32 = 0.035;

struct Heart {
    x: f32,
    y: f32,
    vy: f32,
    life: f32,
}

pub struct Encounter {
    pub species: usize,
    pub level: u32,
    pub biome: MapId,
    pub shiny: bool,
    pub mood: Mood,
    trust: f32,
    shown_trust: f32,
    patience: u32,
    max_patience: u32,
    state: State,
    sel: usize,
    treat_sel: usize,
    log: String,
    log_color: Rgb,
    time: f32,
    jump: f32,
    shake: f32,
    hearts: Vec<Heart>,
    /// Sounds to play, drained by the game.
    pub sfx: Vec<Sfx>,
}

impl Encounter {
    pub fn new(species: usize, shiny: bool, mood: Mood, level: u32, biome: MapId, _rng: &mut StdRng) -> Encounter {
        let sp = &SPECIES[species];
        let max_patience = match sp.rarity {
            1 => 5,
            2 => 5,
            3 => 4,
            4 => 4,
            _ => 6, // mythical creatures are patient — and need it
        };
        let shiny_note = if shiny { "✦ A shimmering Constitutional variant! ✦ " } else { "" };
        Encounter {
            species,
            level,
            biome,
            shiny,
            mood,
            trust: 0.0,
            shown_trust: 0.0,
            patience: max_patience,
            max_patience,
            state: State::Menu,
            sel: 0,
            treat_sel: 0,
            log: format!("{shiny_note}A wild {} appears!", sp.name),
            log_color: if shiny { Rgb(255, 230, 140) } else { Rgb(230, 230, 240) },
            time: 0.0,
            jump: 0.0,
            shake: 0.0,
            hearts: Vec::new(),
            sfx: Vec::new(),
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.shown_trust += (self.trust - self.shown_trust) * (dt * 6.0).min(1.0);
        self.jump = (self.jump - dt * 3.0).max(0.0);
        self.shake = (self.shake - dt * 2.5).max(0.0);
        for h in self.hearts.iter_mut() {
            h.y += h.vy * dt;
            h.x += (h.life * 9.0).sin() * dt * 6.0;
            h.life -= dt;
        }
        self.hearts.retain(|h| h.life > 0.0);
        match &mut self.state {
            State::Play { t, .. } => *t += dt,
            State::Done { t, .. } => *t += dt,
            _ => {}
        }
    }

    fn say(&mut self, text: String, good: Option<bool>) {
        self.log = text;
        self.log_color = match good {
            Some(true) => Rgb(150, 240, 150),
            Some(false) => Rgb(250, 150, 140),
            None => Rgb(230, 230, 240),
        };
    }

    /// Apply a trust change, react visually, spend patience and maybe shift mood.
    fn resolve(&mut self, gain: f32, rng: &mut StdRng) {
        let rarity = SPECIES[self.species].rarity as f32;
        let gain = if gain > 0.0 { gain / (1.0 + (rarity - 1.0) * 0.25) } else { gain };
        self.trust = (self.trust + gain).clamp(0.0, 100.0);
        self.sfx.push(if gain >= 15.0 {
            Sfx::Heart
        } else if gain < 0.0 {
            Sfx::Bad
        } else {
            Sfx::Blip
        });
        if gain >= 15.0 {
            self.jump = 1.0;
            for _ in 0..(gain / 8.0) as usize {
                self.hearts.push(Heart { x: rng.gen_range(-6.0..6.0), y: 0.0, vy: -rng.gen_range(8.0..16.0), life: rng.gen_range(0.8..1.6) });
            }
        } else if gain < 0.0 {
            self.shake = 1.0;
        }
        self.patience = self.patience.saturating_sub(1);
        if self.trust >= 100.0 {
            self.state = State::Done { won: true, t: 0.0 };
            return;
        }
        if self.patience == 0 {
            self.state = State::Done { won: false, t: 0.0 };
            return;
        }
        if rng.gen_bool(0.25) {
            let new = MOODS[rng.gen_range(0..MOODS.len())];
            if new != self.mood {
                self.mood = new;
                let name = SPECIES[self.species].name;
                self.log.push_str(&format!(" Now it seems {}.", new.name().to_lowercase()));
                let _ = name;
            }
        }
        self.state = State::Menu;
    }

    fn mood_mult(&self, action: usize) -> f32 {
        use Mood::*;
        match (action, self.mood) {
            (0, Hungry) => 1.6,
            (0, Grumpy) => 0.7,
            (1, Playful) => 1.6,
            (1, Sleepy) => 0.4,
            (1, Shy) => 0.5,
            (2, Shy) => 2.5,
            (2, Sleepy) => 2.0,
            (2, Grumpy) => 1.6,
            (2, Playful) => 0.3,
            (3, Curious) => 4.0,
            _ => 1.0,
        }
    }

    pub fn key(&mut self, code: KeyCode, data: &mut SaveData, rng: &mut StdRng) -> Option<Outcome> {
        let name = SPECIES[self.species].name;
        match self.state {
            State::Done { won, t } => {
                if t > 0.6 && matches!(code, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') | KeyCode::Esc) {
                    return Some(if won { Outcome::Befriended } else { Outcome::Fled });
                }
            }
            State::Menu => match code {
                KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') | KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                    self.sel = (self.sel + ACTIONS.len() - 1) % ACTIONS.len()
                }
                KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') | KeyCode::Tab => {
                    self.sel = (self.sel + 1) % ACTIONS.len()
                }
                KeyCode::Char(c @ '1'..='6') => {
                    self.sel = c as usize - '1' as usize;
                    return self.choose(data, rng);
                }
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => return self.choose(data, rng),
                KeyCode::Esc | KeyCode::Char('q') => return Some(Outcome::Left),
                _ => {}
            },
            State::Treat => match code {
                KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => {
                    self.treat_sel = (self.treat_sel + FLAVORS.len() - 1) % FLAVORS.len()
                }
                KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') => {
                    self.treat_sel = (self.treat_sel + 1) % FLAVORS.len()
                }
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => self.state = State::Menu,
                KeyCode::Char(c @ '1'..='5') => {
                    self.treat_sel = c as usize - '1' as usize;
                    self.give_treat(data, rng);
                }
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => self.give_treat(data, rng),
                _ => {}
            },
            State::Play { t, center } => {
                if matches!(code, KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e')) {
                    let d = (marker(t, self.species) - center).abs();
                    let (gain, msg, good) = if d < PERFECT {
                        (38.0, format!("PERFECT! {name} is having the time of its life!"), true)
                    } else if d < GOOD {
                        (22.0, format!("Nice! {name} chases you around happily."), true)
                    } else {
                        (-6.0, format!("Whoops! {name} trips over and looks annoyed."), false)
                    };
                    let mult = if gain > 0.0 { self.mood_mult(1) } else { 1.0 };
                    self.say(msg, Some(good));
                    self.resolve(gain * mult, rng);
                }
            }
        }
        let _ = name;
        None
    }

    fn choose(&mut self, data: &mut SaveData, rng: &mut StdRng) -> Option<Outcome> {
        let sp = &SPECIES[self.species];
        match self.sel {
            0 => {
                self.state = State::Treat;
                if data.berries[self.treat_sel] == 0 {
                    self.treat_sel = data.berries.iter().position(|&n| n > 0).unwrap_or(0);
                }
            }
            1 => {
                let center = rng.gen_range(0.2..0.8);
                self.state = State::Play { t: 0.0, center };
                self.say("Press SPACE when the marker hits the glowing zone!".into(), None);
            }
            2 => {
                let mult = self.mood_mult(2);
                let msg = if mult >= 1.5 {
                    format!("You sit quietly. {} slowly creeps closer...", sp.name)
                } else if mult < 1.0 {
                    format!("You sit quietly. {} looks bored and restless.", sp.name)
                } else {
                    format!("You sit quietly. {} watches you calmly.", sp.name)
                };
                self.say(msg, Some(mult >= 1.0));
                self.resolve(12.0 * mult, rng);
            }
            3 => {
                let mult = self.mood_mult(3);
                let first = data.known_fav.insert(sp.name.to_string());
                let msg = format!(
                    "You study {} closely. It seems to love {} food{}",
                    sp.name,
                    sp.fav.name().to_lowercase(),
                    if mult > 1.0 { " — and it loves the attention!" } else if first { ". Noted in your Claudidex!" } else { "." }
                );
                self.say(msg, Some(true));
                self.resolve(6.0 * mult, rng);
            }
            4 => {
                if data.mons.is_empty() {
                    self.say("You need a Claudimon of your own to spar with!".into(), Some(false));
                } else {
                    return Some(Outcome::Spar);
                }
            }
            _ => return Some(Outcome::Left),
        }
        None
    }

    /// Back from a friendly battle: winning impresses, losing is a little awkward.
    pub fn apply_spar(&mut self, won: bool, rng: &mut StdRng) {
        let name = SPECIES[self.species].name;
        use Mood::*;
        let mult = match self.mood {
            Grumpy => 1.6,
            Playful => 1.3,
            Hungry => 0.8,
            Sleepy => 0.6,
            Shy => 0.5,
            Curious => 1.0,
        };
        if won {
            let msg = if mult > 1.0 { format!("{name} is thrilled — what a match! It looks at you with real respect.") } else { format!("{name} catches its breath. That was fun, if a bit much.") };
            self.say(msg, Some(true));
            self.resolve(45.0 * mult, rng);
        } else {
            self.say(format!("{name} won the spar and looks a little smug."), Some(false));
            self.resolve(-10.0, rng);
        }
    }

    fn give_treat(&mut self, data: &mut SaveData, rng: &mut StdRng) {
        let f: Flavor = FLAVORS[self.treat_sel];
        if data.berries[f.idx()] == 0 {
            self.say(format!("You're out of {}s! Pick some from berry bushes.", f.berry()), Some(false));
            return;
        }
        data.berries[f.idx()] -= 1;
        let sp = &SPECIES[self.species];
        let (base, msg, good) = if f == sp.fav {
            data.known_fav.insert(sp.name.to_string());
            (40.0, format!("{} devours the {}! Its favourite!", sp.name, f.berry()), true)
        } else if f == sp.hate {
            (-15.0, format!("{} spits out the {}. Yuck!", sp.name, f.berry()), false)
        } else {
            (15.0, format!("{} nibbles the {} politely.", sp.name, f.berry()), true)
        };
        // Grumpy creatures still can't resist their favourite.
        let mult = if f == sp.fav && self.mood == Mood::Grumpy { 1.0 } else { self.mood_mult(0) };
        self.say(msg, Some(good));
        self.resolve(if base > 0.0 { base * mult } else { base }, rng);
    }

    // ---------------------------------------------------------------- drawing

    pub fn draw(&self, s: &mut Screen, data: &SaveData, hour: f32) {
        let sp = &SPECIES[self.species];
        let rows = s.rows() as i32;
        let w = s.w as i32;
        let panel_h = 9;
        let scene_rows = (rows - panel_h).max(4);
        let scene_h = scene_rows * 2;
        let season = Season::now();

        let horizon = scene_h * 55 / 100;
        draw_backdrop(s, self.biome, season, w, scene_h, horizon, self.time);

        // Creature on a little platform.
        let scale = ((scene_h - 8) / 9).clamp(1, 6).min(w / 20);
        let size = 8 * scale;
        let cx = w / 2 - size / 2;
        let ground = scene_h - 3;
        let pr = size / 2 + scale * 2;
        for y in -scale..=scale {
            for x in -pr..=pr {
                let k = (x * x) as f32 / (pr * pr) as f32 + (y * y) as f32 / ((scale * scale).max(1)) as f32;
                if k <= 1.0 {
                    s.blend(w / 2 + x, ground + y, Rgb(30, 50, 20), 0.35);
                }
            }
        }
        let jump = ((1.0 - self.jump) * std::f32::consts::PI).sin() * self.jump.min(1.0) * scale as f32 * 3.0;
        let idle = if (self.time * 2.0).sin() > 0.6 { scale / 2 } else { 0 };
        let shake = ((self.time * 40.0).sin() * self.shake * scale as f32 * 1.5) as i32;
        let (cy, flee_dx) = match self.state {
            State::Done { won: false, t } => (ground - size, (t * 60.0) as i32),
            _ => (ground - size - jump as i32 - idle, 0),
        };
        creatures::draw(s, self.species, cx + shake + flee_dx, cy, scale, false, self.shiny, self.time);
        if self.biome == MapId::Caves {
            s.tint([0.75, 0.72, 0.85]);
        } else {
            s.tint(clock::light(hour));
        }

        for h in &self.hearts {
            let col = w / 2 + h.x as i32;
            let row = (cy + h.y as i32 * scale / 2).div_euclid(2);
            s.text(col, row, "♥", Rgb(255, 110, 150), None);
        }
        // Mood emote bubble beside the creature.
        if !matches!(self.state, State::Done { .. }) {
            let row = (cy / 2 - 1).max(1);
            s.text(cx + size + 1, row, &format!(" {} ", self.mood.emote()), Rgb(40, 30, 50), Some(Rgb(250, 248, 240)));
        }

        // Title bar.
        let bar = Rgb(30, 26, 36);
        s.text(0, 0, &" ".repeat(s.w), Rgb(255, 255, 255), Some(bar));
        let title = format!("{}{} Lv{}", if self.shiny { "✦ " } else { "" }, sp.name, self.level);
        s.text(1, 0, &title, if self.shiny { Rgb(255, 225, 140) } else { Rgb(250, 240, 230) }, Some(bar));
        let mut col = title.chars().count() as i32 + 3;
        for e in sp.types {
            let label = format!(" {} ", e.name());
            s.text(col, 0, &label, Rgb(20, 20, 30), Some(e.color()));
            col += label.len() as i32 + 1;
        }
        let stars = "★".repeat(sp.rarity as usize);
        s.text(w - stars.chars().count() as i32 - 1, 0, &stars, Rgb(250, 210, 90), Some(bar));

        self.draw_panel(s, data, scene_rows, panel_h);
    }

    fn draw_panel(&self, s: &mut Screen, data: &SaveData, top: i32, h: i32) {
        let sp = &SPECIES[self.species];
        let w = s.w as i32;
        let bg = Rgb(28, 24, 38);
        let dim = Rgb(150, 145, 170);
        s.panel(0, top, w, h, bg, Rgb(110, 90, 140));
        let inner = (w - 4) as usize;
        let fit = |t: &str| -> String { t.chars().take(inner).collect() };

        // Mood + hint
        s.text(2, top + 1, &fit(&format!("{} {}", sp.name, self.mood.hint())), Rgb(235, 225, 210), Some(bg));

        // Trust bar and patience
        let bar_w = ((w - 34).max(10)).min(40);
        s.text(2, top + 2, "Trust", dim, Some(bg));
        let filled = (self.shown_trust / 100.0 * bar_w as f32).round() as i32;
        for i in 0..bar_w {
            let c = if i < filled { Rgb(250, 120, 150).lerp(Rgb(255, 200, 120), i as f32 / bar_w as f32) } else { Rgb(60, 52, 72) };
            s.text(8 + i, top + 2, "█", c, Some(bg));
        }
        s.text(9 + bar_w, top + 2, &format!("{:>3}%", self.shown_trust.round() as i32), Rgb(240, 240, 240), Some(bg));
        let pcol = 15 + bar_w;
        s.text(pcol, top + 2, "Patience ", dim, Some(bg));
        for i in 0..self.max_patience {
            let (ch, c) = if i < self.patience { ("●", Rgb(130, 200, 255)) } else { ("○", Rgb(80, 80, 100)) };
            s.text(pcol + 9 + i as i32 * 2, top + 2, ch, c, Some(bg));
        }

        s.text(2, top + 4, &fit(&self.log), self.log_color, Some(bg));

        let row = top + 6;
        match self.state {
            State::Menu => {
                let mut col = 2;
                for (i, (name, desc)) in ACTIONS.iter().enumerate() {
                    let label = format!(" {} {} ", i + 1, name);
                    let (fg, b) = if i == self.sel { (Rgb(30, 20, 30), Rgb(218, 119, 86)) } else { (Rgb(230, 225, 240), Rgb(52, 44, 66)) };
                    s.text(col, row, &label, fg, Some(b));
                    if i == self.sel {
                        s.text(2, row + 1, &fit(&format!("› {desc}")), dim, Some(bg));
                    }
                    col += label.chars().count() as i32 + 1;
                }
            }
            State::Treat => {
                let mut col = 2;
                for (i, f) in FLAVORS.iter().enumerate() {
                    let n = data.berries[f.idx()];
                    let label = format!(" {} {} ", f.berry(), n);
                    let sel = i == self.treat_sel;
                    let fg = if n == 0 { Rgb(110, 105, 120) } else if sel { Rgb(20, 20, 20) } else { Rgb(235, 235, 240) };
                    let b = if sel { f.color() } else { Rgb(52, 44, 66) };
                    if col + label.chars().count() as i32 >= w - 1 {
                        break;
                    }
                    s.text(col, row, &label, fg, Some(b));
                    col += label.chars().count() as i32 + 1;
                }
                let f = FLAVORS[self.treat_sel];
                let known = data.known_fav.contains(sp.name);
                let tip = if known { format!("{} flavour · {} loves {} food", f.name(), sp.name, sp.fav.name().to_lowercase()) } else { format!("{} flavour · Esc: back", f.name()) };
                s.text(2, row + 1, &fit(&tip), dim, Some(bg));
            }
            State::Play { t, center } => {
                let bw = (w - 6).min(60);
                let bx = (w - bw) / 2;
                for i in 0..bw {
                    let p = (i as f32 + 0.5) / bw as f32;
                    let d = (p - center).abs();
                    let c = if d < PERFECT { Rgb(255, 220, 110) } else if d < GOOD { Rgb(120, 200, 120) } else { Rgb(60, 52, 72) };
                    s.text(bx + i, row, "▬", c, Some(bg));
                }
                let m = (marker(t, self.species) * bw as f32) as i32;
                s.text(bx + m.clamp(0, bw - 1), row, "▼", Rgb(255, 255, 255), Some(bg));
                s.text(bx + m.clamp(0, bw - 1), row + 1, "▲", Rgb(255, 255, 255), Some(bg));
            }
            State::Done { won, t } => {
                let msg = if won { format!("♥ {} trusts you! It joins your Sanctuary! ♥", sp.name) } else { format!("{} lost patience and wandered off...", sp.name) };
                let c = if won { Rgb(255, 200, 120) } else { Rgb(190, 190, 210) };
                s.text(2, row, &fit(&msg), c, Some(bg));
                if t > 0.6 {
                    s.text(2, row + 1, "press space to continue", dim, Some(bg));
                }
            }
        }
    }
}

/// The scenery behind an encounter or battle, matching where it happens.
pub fn draw_backdrop(s: &mut Screen, biome: MapId, season: Season, w: i32, scene_h: i32, horizon: i32, t: f32) {
    let (g0, g1, _) = world::grass_cols(season);
    let sky = |s: &mut Screen| {
        for y in 0..horizon {
            let k = y as f32 / horizon.max(1) as f32;
            for x in 0..w {
                s.set(x, y, Rgb(120, 180, 240).lerp(Rgb(215, 235, 250), k));
            }
        }
    };
    let ground = |s: &mut Screen, a: Rgb, b: Rgb| {
        for y in horizon..scene_h {
            for x in 0..w {
                s.set(x, y, if world::hash(x, y, 77) % 11 == 0 { b } else { a });
            }
        }
    };
    match biome {
        MapId::Woods => {
            sky(s);
            // Two layers of pine silhouettes.
            for (layer, col) in [(0, Rgb(70, 110, 100)), (1, Rgb(36, 80, 58))] {
                for x in 0..w {
                    let period = 9 + layer * 4;
                    let px = (x + layer * 5) % period;
                    let peak = (period / 2 - (px - period / 2).abs()) * 2;
                    let top = horizon - 6 - peak - layer * 2 - (world::hash(x / period, layer, 5) % 6) as i32;
                    for y in top.max(0)..horizon {
                        s.set(x, y, col);
                    }
                }
            }
            ground(s, g0.lerp(Rgb(30, 60, 30), 0.25), g1.lerp(Rgb(30, 60, 30), 0.25));
        }
        MapId::Shore => {
            sky(s);
            let sea_top = horizon - 6;
            for y in sea_top..horizon + 4 {
                for x in 0..w {
                    let wave = ((x as f32 * 0.3) + t * 2.0 + y as f32).sin();
                    let c = Rgb(50, 120, 200).lerp(Rgb(90, 170, 230), (y - sea_top) as f32 / 10.0);
                    s.set(x, y, if wave > 0.95 { Rgb(220, 240, 255) } else { c });
                }
            }
            for y in horizon + 4..scene_h {
                for x in 0..w {
                    let n = world::hash(x, y, 78) % 9;
                    s.set(x, y, if n == 0 { Rgb(220, 196, 140) } else { Rgb(236, 214, 160) });
                }
            }
        }
        MapId::Island => {
            // Mythos Isle: a violet dusk full of stars over a glowing sea.
            for y in 0..horizon {
                let k = y as f32 / horizon.max(1) as f32;
                for x in 0..w {
                    let mut c = Rgb(40, 30, 90).lerp(Rgb(230, 140, 160), k * k);
                    let st = world::hash(x, y, 86);
                    if st % 53 == 0 && k < 0.7 {
                        c = c.lerp(Rgb(255, 250, 230), 0.4 + ((t * 2.0 + st as f32).sin() * 0.5 + 0.5) * 0.6);
                    }
                    s.set(x, y, c);
                }
            }
            for y in horizon - 4..horizon + 3 {
                for x in 0..w {
                    let wave = (x as f32 * 0.25 + t * 1.5 + y as f32).sin();
                    s.set(x, y, Rgb(70, 60, 150).lerp(Rgb(150, 130, 220), (wave + 1.0) * 0.3));
                }
            }
            for y in horizon + 3..scene_h {
                for x in 0..w {
                    let n = world::hash(x, y, 87) % 23;
                    let c = match n {
                        0 => Rgb(255, 200, 230),
                        1 => Rgb(200, 220, 255),
                        2..=5 => Rgb(70, 130, 90),
                        _ => Rgb(84, 150, 100),
                    };
                    s.set(x, y, c);
                }
            }
        }
        MapId::Town => {
            // The Benchmark arena: stands full of cheering spectators, then a sandy floor.
            for y in 0..horizon {
                for x in 0..w {
                    let row = y / 3;
                    let seat = if row % 2 == 0 { Rgb(110, 90, 140) } else { Rgb(90, 74, 118) };
                    let fan = world::hash(x / 2, row, 83) % 5 == 0;
                    let cheer = ((t * 6.0 + x as f32 * 0.7).sin() > 0.6) as i32;
                    let c = if fan && y % 3 == 1 - cheer.min(1) {
                        [Rgb(240, 120, 90), Rgb(250, 210, 90), Rgb(120, 200, 240), Rgb(240, 240, 240)][(world::hash(x / 2, row, 84) % 4) as usize]
                    } else {
                        seat
                    };
                    s.set(x, y, c);
                }
            }
            for x in 0..w {
                s.set(x, horizon - 1, Rgb(218, 119, 86));
            }
            for y in horizon..scene_h {
                for x in 0..w {
                    let n = world::hash(x, y, 85) % 9;
                    s.set(x, y, if n == 0 { Rgb(205, 180, 130) } else { Rgb(225, 200, 150) });
                }
            }
        }
        MapId::Caves => {
            for y in 0..scene_h {
                for x in 0..w {
                    let n = world::hash(x, y, 79) % 13;
                    let base = if y < horizon { Rgb(40, 36, 50) } else { Rgb(70, 64, 80) };
                    s.set(x, y, if n == 0 { base.lerp(Rgb(110, 100, 120), 0.4) } else { base });
                }
            }
            // Stalactites and glowing crystals.
            for i in 0..w / 6 {
                let x = i * 6 + (world::hash(i, 0, 80) % 4) as i32;
                let len = 3 + (world::hash(i, 1, 80) % 7) as i32;
                for y in 0..len {
                    for dx in 0..(len - y).min(3) / 2 + 1 {
                        s.set(x + dx, y, Rgb(60, 54, 72));
                    }
                }
                if world::hash(i, 2, 80) % 3 == 0 {
                    let glow = 0.5 + (t * 1.5 + i as f32).sin() * 0.5;
                    for y in horizon - 5..horizon {
                        s.set(x, y, Rgb(90, 210, 240).lerp(Rgb(200, 255, 255), glow));
                    }
                }
            }
        }
        _ => {
            sky(s);
            for x in 0..w {
                let hill = horizon - 3 - ((x as f32 * 0.07).sin() * 3.0 + (x as f32 * 0.023 + 1.0).sin() * 4.0) as i32;
                for y in hill..horizon {
                    s.set(x, y, g1.lerp(Rgb(120, 150, 200), 0.45));
                }
            }
            ground(s, g0, g1);
        }
    }
}

/// Minigame marker position 0..1; rarer creatures move faster.
fn marker(t: f32, species: usize) -> f32 {
    let speed = 2.2 + SPECIES[species].rarity as f32 * 0.5;
    ((t * speed).sin() + 1.0) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn press(e: &mut Encounter, data: &mut SaveData, rng: &mut StdRng, code: KeyCode) -> Option<Outcome> {
        e.update(1.0);
        e.key(code, data, rng)
    }

    #[test]
    fn favourite_treats_befriend() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut data = SaveData::default();
        data.berries = [10; 5];
        let kitten = crate::creatures::by_name("Haikitten").unwrap();
        let mut e = Encounter::new(kitten, false, Mood::Hungry, 5, MapId::Meadow, &mut rng);
        let mut outcome = None;
        for _ in 0..10 {
            // 1 = Treat, then 1 = Honeyberry (Sweet, the Haikitten favourite).
            press(&mut e, &mut data, &mut rng, KeyCode::Char('1'));
            press(&mut e, &mut data, &mut rng, KeyCode::Char('1'));
            if let State::Done { .. } = e.state {
                outcome = press(&mut e, &mut data, &mut rng, KeyCode::Enter);
                break;
            }
        }
        assert!(matches!(outcome, Some(Outcome::Befriended)));
        assert!(data.berries[0] < 10, "treats are consumed");
        assert!(data.known_fav.contains("Haikitten"));
    }

    #[test]
    fn hated_treats_exhaust_patience() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut data = SaveData::default();
        data.berries = [10; 5];
        let kitten = crate::creatures::by_name("Haikitten").unwrap();
        let mut e = Encounter::new(kitten, false, Mood::Grumpy, 5, MapId::Meadow, &mut rng);
        let mut outcome = None;
        for _ in 0..10 {
            press(&mut e, &mut data, &mut rng, KeyCode::Char('1'));
            press(&mut e, &mut data, &mut rng, KeyCode::Char('4')); // Cocoabean: Bitter, hated
            if let State::Done { .. } = e.state {
                outcome = press(&mut e, &mut data, &mut rng, KeyCode::Enter);
                break;
            }
        }
        assert!(matches!(outcome, Some(Outcome::Fled)));
    }
}
