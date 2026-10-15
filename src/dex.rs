//! The Claudidex: every species you have seen and befriended.

use crate::creatures::{self, SPECIES};
use crate::gfx::{Rgb, Screen};
use crate::save::SaveData;
use crossterm::event::KeyCode;

pub struct Dex {
    sel: usize,
}

impl Dex {
    pub fn new() -> Dex {
        Dex { sel: 0 }
    }

    /// Returns true when the dex should close.
    pub fn key(&mut self, code: KeyCode) -> bool {
        let n = SPECIES.len();
        match code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => self.sel = (self.sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => self.sel = (self.sel + 1) % n,
            KeyCode::Esc | KeyCode::Char('x') | KeyCode::Char('q') | KeyCode::Tab => return true,
            _ => {}
        }
        false
    }

    pub fn draw(&self, s: &mut Screen, data: &SaveData, t: f32) {
        let w = s.w as i32;
        let rows = s.rows() as i32;
        let bg = Rgb(26, 22, 36);
        s.clear(bg);
        let bar = Rgb(218, 119, 86);
        s.text(0, 0, &" ".repeat(s.w), Rgb(0, 0, 0), Some(bar));
        let owned_species = SPECIES.iter().filter(|sp| data.mons.iter().any(|m| m.species == sp.name)).count();
        s.text(1, 0, "✻ CLAUDIDEX", Rgb(30, 20, 20), Some(bar));
        let stats = format!("seen {}  befriended {}/{} ", data.seen.len(), owned_species, SPECIES.len());
        s.text(w - stats.len() as i32, 0, &stats, Rgb(30, 20, 20), Some(bar));

        // List on the left, scrolled to keep the selection visible.
        let list_w = 20;
        let visible = (rows - 3).max(1) as usize;
        let start = self.sel.saturating_sub(visible - 1).min(SPECIES.len().saturating_sub(visible));
        for (row, i) in (start..SPECIES.len()).take(visible).enumerate() {
            let sp = &SPECIES[i];
            let seen = data.seen.contains(sp.name);
            let owned = data.mons.iter().any(|m| m.species == sp.name);
            let name = if seen { sp.name } else { "???" };
            let mark = if owned { "★" } else if seen { "·" } else { " " };
            let line = format!("{mark}{:03} {:<14}", i + 1, name);
            let (fg, b) = if i == self.sel { (Rgb(20, 16, 24), Rgb(240, 200, 160)) } else if seen { (Rgb(225, 220, 235), bg) } else { (Rgb(100, 95, 115), bg) };
            s.text(1, 2 + row as i32, &line, fg, Some(b));
        }

        // Detail pane.
        let sp = &SPECIES[self.sel];
        let seen = data.seen.contains(sp.name);
        let owned: Vec<_> = data.mons.iter().filter(|m| m.species == sp.name).collect();
        let dx = list_w + 3;
        let avail_w = w - dx - 2;
        let scale = ((rows * 2 - 14) / 9).clamp(1, 5).min(avail_w / 10);
        let px = dx + 2;
        let py = 6;
        s.fill_rect(dx, 4, avail_w, 8 * scale + 6, Rgb(40, 34, 54));
        if seen {
            let bob = if (t * 2.0).sin() > 0.6 { 1 } else { 0 };
            let shiny = owned.iter().any(|m| m.shiny) && (t as i32 / 3) % 2 == 1;
            creatures::draw(s, self.sel, px, py - bob, scale, false, shiny, t);
        } else {
            creatures::draw_silhouette(s, self.sel, px, py, scale);
        }

        let tx = px + 8 * scale + 3;
        let mut row = 3;
        let title = if seen { sp.name } else { "???" };
        s.text(tx, row, title, Rgb(255, 240, 220), Some(Rgb(40, 34, 54)));
        row += 1;
        if seen {
            let mut col = tx;
            for e in sp.types {
                let label = format!(" {} ", e.name());
                s.text(col, row, &label, Rgb(20, 20, 30), Some(e.color()));
                col += label.len() as i32 + 1;
            }
            row += 1;
            s.text(tx, row, &"★".repeat(sp.rarity as usize), Rgb(250, 210, 90), Some(Rgb(40, 34, 54)));
            row += 1;
            let fav = if data.known_fav.contains(sp.name) { sp.fav.name() } else { "?" };
            s.text(tx, row, &format!("Loves: {fav}"), Rgb(200, 195, 215), Some(Rgb(40, 34, 54)));
            row += 1;
            let when = match sp.active {
                creatures::Active::Day => "Day",
                creatures::Active::Night => "Night",
                creatures::Active::Any => "Any time",
            };
            s.text(tx, row, &format!("Active: {when}"), Rgb(200, 195, 215), Some(Rgb(40, 34, 54)));
        }

        // Description under the portrait, word-wrapped.
        let mut drow = (py + 8 * scale + 6) / 2 + 1;
        let text = if owned.is_empty() {
            if seen { "Befriend one to learn more about it." } else { "Not yet discovered." }
        } else {
            sp.dex
        };
        for line in wrap(text, (avail_w - 2).max(10) as usize) {
            s.text(dx + 1, drow, &line, Rgb(220, 215, 230), Some(bg));
            drow += 1;
        }
        if !owned.is_empty() {
            let best = owned.iter().map(|m| m.friendship).max().unwrap_or(0);
            let shiny = owned.iter().filter(|m| m.shiny).count();
            let mut info = format!("In your Sanctuary: {}  ·  best friendship {}", owned.len(), best);
            if shiny > 0 {
                info.push_str(&format!("  ·  ✦ {shiny}"));
            }
            s.text(dx + 1, drow + 1, &info, Rgb(255, 200, 140), Some(bg));
        }
        s.text(1, rows - 1, "↑↓ browse   x/esc close", Rgb(140, 135, 160), Some(bg));
    }
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + word.chars().count() + 1 > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}
