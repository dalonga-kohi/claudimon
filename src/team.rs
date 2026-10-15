//! The team screen: pick up to three Claudimon to battle with, and inspect their growth.

use crate::battle::{self, MOVES};
use crate::creatures::{self, SPECIES};
use crate::dex::wrap;
use crate::gfx::{Rgb, Screen};
use crate::save::SaveData;
use crate::ui;
use crossterm::event::KeyCode;

pub const PARTY_SIZE: usize = 3;

/// The creatures that fight: the chosen party, or (if none chosen) your three strongest.
pub fn party(data: &SaveData) -> Vec<usize> {
    let mut p: Vec<usize> = data.party.iter().copied().filter(|&i| i < data.mons.len()).take(PARTY_SIZE).collect();
    p.dedup();
    if p.is_empty() {
        let mut all: Vec<usize> = (0..data.mons.len()).collect();
        all.sort_by_key(|&i| std::cmp::Reverse((data.mons[i].level, data.mons[i].xp)));
        all.truncate(PARTY_SIZE);
        p = all;
    }
    p
}

pub struct TeamView {
    sel: usize,
    msg: Option<String>,
}

impl TeamView {
    pub fn new() -> TeamView {
        TeamView { sel: 0, msg: None }
    }

    /// Returns true to close.
    pub fn key(&mut self, code: KeyCode, data: &mut SaveData) -> bool {
        let n = data.mons.len().max(1);
        match code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => self.sel = (self.sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => self.sel = (self.sel + 1) % n,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') if !data.mons.is_empty() => {
                // Materialise the implicit party before editing it.
                if data.party.is_empty() {
                    data.party = party(data);
                }
                if let Some(pos) = data.party.iter().position(|&i| i == self.sel) {
                    data.party.remove(pos);
                    self.msg = Some(format!("{} rests at the Sanctuary.", data.mons[self.sel].species));
                } else if data.party.len() >= PARTY_SIZE {
                    self.msg = Some("Your party is full (3). Remove someone first.".into());
                } else {
                    data.party.push(self.sel);
                    self.msg = Some(format!("{} joins your party!", data.mons[self.sel].species));
                }
            }
            KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('q') | KeyCode::Backspace => return true,
            _ => {}
        }
        false
    }

    pub fn draw(&self, s: &mut Screen, data: &SaveData, t: f32) {
        let w = s.w as i32;
        let rows = s.rows() as i32;
        let bg = Rgb(26, 22, 36);
        s.clear(bg);
        s.text(0, 0, &" ".repeat(s.w), Rgb(0, 0, 0), Some(ui::ACCENT));
        s.text(1, 0, "✻ YOUR TEAM", Rgb(30, 20, 20), Some(ui::ACCENT));
        let party = party(data);
        let info = format!("party {}/{}  ·  {} Claudimon ", party.len(), PARTY_SIZE, data.mons.len());
        s.text(w - info.chars().count() as i32, 0, &info, Rgb(30, 20, 20), Some(ui::ACCENT));
        if data.mons.is_empty() {
            s.text(2, 3, "No Claudimon yet — befriend some in the wild first!", ui::DIM, Some(bg));
            return;
        }

        let list_w = 26;
        let visible = (rows - 4).max(1) as usize;
        let start = self.sel.saturating_sub(visible - 1);
        for (row, i) in (start..data.mons.len()).take(visible).enumerate() {
            let m = &data.mons[i];
            let mark = if party.contains(&i) { "●" } else { " " };
            let line = format!("{mark} {}{:<13} Lv{:<3}", if m.shiny { "✦" } else { "" }, m.species, m.level);
            let line: String = line.chars().take(list_w as usize - 1).collect();
            let (fg, b) = if i == self.sel { (Rgb(20, 16, 24), Rgb(240, 200, 160)) } else if party.contains(&i) { (ui::GOLD, bg) } else { (ui::TEXT, bg) };
            s.text(1, 2 + row as i32, &format!("{line:<w$}", w = list_w as usize - 1), fg, Some(b));
        }

        // Details of the selected creature.
        let Some(m) = data.mons.get(self.sel) else { return };
        let Some(sp) = creatures::by_name(&m.species) else { return };
        let dx = list_w + 2;
        let scale = ((rows * 2 - 20) / 12).clamp(1, 4).min((w - dx - 30) / 8).max(1);
        let pane = Rgb(40, 34, 54);
        s.fill_rect(dx, 4, 8 * scale + 4, 8 * scale + 4, pane);
        let bob = if (t * 2.0).sin() > 0.6 { 1 } else { 0 };
        creatures::draw(s, sp, dx + 2, 6 - bob, scale, false, m.shiny, t);

        let tx = dx + 8 * scale + 3;
        let mut row = 2;
        let put = |s: &mut Screen, row: &mut i32, text: &str, c: Rgb| {
            if tx < w - 2 {
                let text: String = text.chars().take((w - tx - 1) as usize).collect();
                s.text(tx, *row, &text, c, Some(bg));
            }
            *row += 1;
        };
        put(s, &mut row, &format!("{}{}", if m.shiny { "✦ " } else { "" }, m.species), ui::GOLD);
        let types: Vec<&str> = SPECIES[sp].types.iter().map(|e| e.name()).collect();
        put(s, &mut row, &format!("{}  ·  Lv {}", types.join("/"), m.level), ui::TEXT);
        // XP bar
        let need = creatures::xp_to_next(m.level);
        let frac = if m.level >= creatures::MAX_LEVEL { 1.0 } else { m.xp as f32 / need as f32 };
        let bar: String = (0..10).map(|i| if (i as f32) < frac * 10.0 { '█' } else { '░' }).collect();
        put(s, &mut row, &format!("XP {bar} {}/{}", m.xp, need), Rgb(130, 200, 255));
        let st = creatures::stats_at(sp, m.level);
        put(s, &mut row, &format!("HP {:<4} ATK {:<4}", st.hp, st.atk), ui::TEXT);
        put(s, &mut row, &format!("DEF {:<3} SPD {:<4}", st.def, st.spd), ui::TEXT);
        put(s, &mut row, &format!("♥ {:.0}%  ·  Friendship {}", m.happy, m.friendship), Rgb(255, 160, 190));
        if let Some((lvl, into)) = SPECIES[sp].evolves {
            let seen = data.seen.contains(into);
            put(s, &mut row, &format!("Evolves Lv {lvl} → {}", if seen { into } else { "???" }), Rgb(200, 160, 255));
        }
        row = row.max(4 + (8 * scale + 4) / 2 + 1);
        put(s, &mut row, "", ui::TEXT);
        let mrow = row;
        for (i, &mv) in battle::moveset(sp, m.level).iter().enumerate() {
            let mo = &MOVES[mv];
            let c = mo.elem.map(|e| e.color()).unwrap_or(Rgb(200, 200, 210));
            let p = if mo.power > 0 { format!("{:>3}", mo.power) } else { "  —".into() };
            s.text(dx, mrow + i as i32, &format!("• {:<17}{p}", mo.name), c, Some(bg));
        }
        if let Some(msg) = &self.msg {
            for (i, l) in wrap(msg, (w - dx - 2).max(10) as usize).iter().enumerate().take(2) {
                s.text(dx, rows - 3 + i as i32, l, ui::GOLD, Some(bg));
            }
        }
        s.text(1, rows - 1, "↑↓ choose   enter: add/remove from party   t/esc close", ui::DIM, Some(bg));
    }
}
