//! Reusable UI: popup menus, dialogue boxes, the catalog and the "while you were away" report.

use crate::creatures::FLAVORS;
use crate::dex::wrap;
use crate::garden::{self, Category, ITEMS, LAND_PRICES, LOOT, MAX_LAND};
use crate::gfx::{Rgb, Screen};
use crate::save::SaveData;
use crate::world::Map;
use crossterm::event::KeyCode;

pub const BG: Rgb = Rgb(28, 24, 38);
pub const BORDER: Rgb = Rgb(110, 90, 140);
pub const TEXT: Rgb = Rgb(232, 226, 240);
pub const DIM: Rgb = Rgb(150, 145, 170);
pub const ACCENT: Rgb = Rgb(218, 119, 86);
pub const GOLD: Rgb = Rgb(255, 210, 110);

pub enum MenuResult {
    None,
    Chosen(usize),
    Cancel,
}

/// A small vertical popup menu.
pub struct Menu {
    pub title: String,
    pub options: Vec<(String, Rgb)>,
    pub sel: usize,
}

impl Menu {
    pub fn new(title: impl Into<String>, options: Vec<(String, Rgb)>) -> Menu {
        Menu { title: title.into(), options, sel: 0 }
    }

    pub fn key(&mut self, code: KeyCode) -> MenuResult {
        let n = self.options.len().max(1);
        match code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => self.sel = (self.sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') | KeyCode::Tab => self.sel = (self.sel + 1) % n,
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i < self.options.len() {
                    return MenuResult::Chosen(i);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') if !self.options.is_empty() => return MenuResult::Chosen(self.sel),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => return MenuResult::Cancel,
            _ => {}
        }
        MenuResult::None
    }

    pub fn draw(&self, s: &mut Screen) {
        let rows = s.rows() as i32;
        let width = self
            .options
            .iter()
            .map(|(o, _)| o.chars().count() + 6)
            .chain([self.title.chars().count() + 4, 22])
            .max()
            .unwrap_or(22)
            .min(s.w.saturating_sub(2)) as i32;
        let h = self.options.len() as i32 + 4;
        let (x, y) = ((s.w as i32 - width) / 2, (rows - h) / 2);
        s.panel(x, y, width, h, BG, BORDER);
        s.text(x + 2, y, &format!(" {} ", self.title), ACCENT, Some(BG));
        for (i, (o, c)) in self.options.iter().enumerate() {
            let sel = i == self.sel;
            let line: String = format!("{} {}", if sel { '›' } else { ' ' }, o).chars().take(width as usize - 4).collect();
            let pad = format!("{line:<w$}", w = width as usize - 4);
            let (fg, bg) = if sel { (Rgb(24, 18, 28), *c) } else { (*c, BG) };
            s.text(x + 2, y + 2 + i as i32, &pad, fg, Some(bg));
        }
        s.text(x + 2, y + h - 1, " enter ok · esc back ", DIM, Some(BG));
    }
}

/// A speech box at the bottom of the screen.
pub fn draw_talk(s: &mut Screen, speaker: &str, text: &str, color: Rgb) {
    let rows = s.rows() as i32;
    let w = s.w as i32;
    let lines = wrap(text, (w - 6).max(10) as usize);
    let h = lines.len() as i32 + 3;
    let y = rows - h - 1;
    s.panel(1, y, w - 2, h, BG, BORDER);
    s.text(3, y, &format!(" {speaker} "), color, Some(BG));
    for (i, l) in lines.iter().enumerate() {
        s.text(3, y + 1 + i as i32, l, TEXT, Some(BG));
    }
    s.text(w - 10, y + h - 1, " ▼ next ", DIM, Some(BG));
}

/// A centred panel with a title and lines of text.
pub fn draw_report(s: &mut Screen, title: &str, lines: &[(String, Rgb)]) {
    let rows = s.rows() as i32;
    let width = lines.iter().map(|(l, _)| l.chars().count() as i32 + 6).chain([title.chars().count() as i32 + 8, 36]).max().unwrap_or(36).min(s.w as i32 - 2);
    let h = lines.len() as i32 + 4;
    let (x, y) = ((s.w as i32 - width) / 2, (rows - h) / 2);
    s.panel(x, y, width, h, BG, ACCENT);
    s.text(x + 2, y, &format!(" {title} "), GOLD, Some(BG));
    for (i, (l, c)) in lines.iter().enumerate() {
        let l: String = l.chars().take(width as usize - 4).collect();
        s.text(x + 3, y + 2 + i as i32, &l, *c, Some(BG));
    }
    s.text(x + width - 18, y + h - 1, " press any key ", DIM, Some(BG));
}

// ------------------------------------------------------------------ catalog

const TABS: [&str; 5] = ["Seeds", "Habitats", "Decor", "Land", "Sell"];
const CURIO_TABS: [&str; 2] = ["Curios", "Sell finds"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShopKind {
    /// Claude's mail-order catalog, available anywhere.
    Claude,
    /// Mira's Curio Shop in Promptville.
    Curio,
}

pub struct Catalog {
    pub kind: ShopKind,
    tab: usize,
    sel: usize,
    msg: Option<(String, Rgb)>,
}

pub enum Purchase {
    Item(usize),
    Land,
    /// Sell berries of a flavour index: one, or all of them.
    Sell(usize, bool),
    /// Sell curios by index into `LOOT`: one, or all of them.
    SellLoot(usize, bool),
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog { kind: ShopKind::Claude, tab: 0, sel: 0, msg: None }
    }

    pub fn curio() -> Catalog {
        Catalog { kind: ShopKind::Curio, tab: 0, sel: 0, msg: None }
    }

    fn tabs(&self) -> &'static [&'static str] {
        match self.kind {
            ShopKind::Claude => &TABS,
            ShopKind::Curio => &CURIO_TABS,
        }
    }

    fn entries(&self) -> Vec<usize> {
        if self.kind == ShopKind::Curio {
            return (0..ITEMS.len()).filter(|&i| ITEMS[i].curio).collect();
        }
        let cat = match self.tab {
            0 => Category::Seed,
            1 => Category::Habitat,
            _ => Category::Decor,
        };
        (0..ITEMS.len()).filter(|&i| ITEMS[i].cat == cat && !ITEMS[i].curio && ITEMS[i].price > 0).collect()
    }

    fn len(&self) -> usize {
        match (self.kind, self.tab) {
            (ShopKind::Claude, 3) => 1,
            (ShopKind::Claude, 4) => FLAVORS.len(),
            (ShopKind::Curio, 1) => LOOT.len(),
            _ => self.entries().len(),
        }
    }

    pub fn say(&mut self, text: impl Into<String>, c: Rgb) {
        self.msg = Some((text.into(), c));
    }

    /// Returns Some(None) to close, Some(Some(p)) to try a purchase.
    pub fn key(&mut self, code: KeyCode) -> Option<Option<Purchase>> {
        match code {
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => {
                self.tab = (self.tab + self.tabs().len() - 1) % self.tabs().len();
                self.sel = 0;
            }
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Tab => {
                self.tab = (self.tab + 1) % self.tabs().len();
                self.sel = 0;
            }
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => self.sel = (self.sel + self.len() - 1) % self.len(),
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => self.sel = (self.sel + 1) % self.len(),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => {
                let all = code == KeyCode::Char(' ');
                return Some(Some(match (self.kind, self.tab) {
                    (ShopKind::Claude, 3) => Purchase::Land,
                    (ShopKind::Claude, 4) => Purchase::Sell(self.sel, all),
                    (ShopKind::Curio, 1) => Purchase::SellLoot(self.sel, all),
                    _ => Purchase::Item(self.entries()[self.sel]),
                }));
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') => return Some(None),
            _ => {}
        }
        None
    }

    pub fn draw(&self, s: &mut Screen, data: &SaveData, t: f32) {
        let w = s.w as i32;
        let rows = s.rows() as i32;
        let bg = Rgb(30, 26, 40);
        s.clear(bg);
        s.text(0, 0, &" ".repeat(s.w), Rgb(0, 0, 0), Some(ACCENT));
        let (title, quote) = match self.kind {
            ShopKind::Claude => ("✻ CLAUDE'S CATALOG", "\"I can have that delivered right away. Well — almost right away.\""),
            ShopKind::Curio => ("⚱ MIRA'S CURIO SHOP", "\"Shells, glowcaps, crystals — if you found it out there, I'll buy it!\""),
        };
        s.text(1, 0, title, Rgb(30, 20, 20), Some(ACCENT));
        let money = format!("✦ {} ", data.tokens);
        s.text(w - money.chars().count() as i32, 0, &money, Rgb(30, 20, 20), Some(ACCENT));
        s.text(1, 1, quote, DIM, Some(bg));

        let mut col = 1;
        for (i, name) in self.tabs().iter().enumerate() {
            let label = format!(" {name} ");
            let (fg, b) = if i == self.tab { (Rgb(24, 18, 28), GOLD) } else { (TEXT, Rgb(52, 44, 66)) };
            s.text(col, 3, &label, fg, Some(b));
            col += label.chars().count() as i32 + 1;
        }

        let stats = format!("income ~{:.0} ✦/h · decor ×{:.2} ", garden::tokens_per_hour(data), garden::decor_multiplier(data));
        if col + stats.chars().count() as i32 + 2 < w {
            s.text(w - stats.chars().count() as i32, 3, &stats, DIM, Some(bg));
        }

        let list_w = 34.min(w / 2);
        let detail_x = list_w + 3;
        if self.kind == ShopKind::Curio && self.tab == 1 {
            for (i, (id, name, price)) in LOOT.iter().enumerate() {
                let sel = i == self.sel;
                let have = data.item_count(id);
                let line = format!("{}{:<16}×{:<4}✦{} each", if sel { '›' } else { ' ' }, name, have, price);
                let fg = if sel { Rgb(24, 18, 28) } else if have > 0 { TEXT } else { Rgb(110, 104, 122) };
                s.text(1, 5 + i as i32, &line, fg, Some(if sel { GOLD } else { bg }));
            }
            let tip = "Seashells wash up on Token Shore, Glowcaps sprout in the Latent Woods and Kernel Crystals grow in the caverns. They regrow over time. enter: sell one · space: sell all";
            for (i, l) in wrap(tip, (w - 4).max(10) as usize).iter().enumerate() {
                s.text(1, 10 + i as i32, l, DIM, Some(bg));
            }
        } else if self.kind == ShopKind::Claude && self.tab == 4 {
            for (i, f) in FLAVORS.iter().enumerate() {
                let sel = i == self.sel;
                let have = data.berries[f.idx()];
                let line = format!("{}{:<14}×{:<4}✦{} each", if sel { '›' } else { ' ' }, f.berry(), have, garden::BERRY_PRICES[i]);
                let fg = if sel { Rgb(24, 18, 28) } else if have > 0 { TEXT } else { Rgb(110, 104, 122) };
                s.text(1, 5 + i as i32, &line, fg, Some(if sel { f.color() } else { bg }));
            }
            let tip = "Claude buys your berries — bushes and crops turn into tokens. enter: sell one · space: sell all";
            for (i, l) in wrap(tip, (w - 4).max(10) as usize).iter().enumerate() {
                s.text(1, 12 + i as i32, l, DIM, Some(bg));
            }
        } else if self.kind == ShopKind::Claude && self.tab == 3 {
            let lvl = data.land;
            let line = if lvl >= MAX_LAND { "Sanctuary fully expanded ✓".to_string() } else { format!("Clear more forest  ✦ {}", LAND_PRICES[lvl as usize]) };
            s.text(1, 5, &format!("› {line:<w$}", w = list_w as usize - 2), Rgb(24, 18, 28), Some(GOLD));
            let (cw, ch) = Map::garden_size(lvl);
            let next = Map::garden_size((lvl + 1).min(MAX_LAND));
            let desc = format!(
                "Your Sanctuary is {cw}×{ch} tiles. Each expansion clears forest for more habitats, crops and roaming room{}",
                if lvl < MAX_LAND { format!(" — next size {}×{}.", next.0, next.1) } else { ".".to_string() }
            );
            for (i, l) in wrap(&desc, (w - detail_x - 2).max(10) as usize).iter().enumerate() {
                s.text(detail_x, 5 + i as i32, l, TEXT, Some(bg));
            }
        } else {
            let entries = self.entries();
            for (row, &i) in entries.iter().enumerate() {
                let it = &ITEMS[i];
                let afford = data.tokens >= it.price;
                let owned = data.item_count(it.id);
                let name: String = it.name.chars().take(list_w as usize - 12).collect();
                let line = format!("{name:<n$}✦{:>4}", it.price, n = list_w as usize - 9);
                let sel = row == self.sel;
                let fg = if sel { Rgb(24, 18, 28) } else if afford { TEXT } else { Rgb(110, 104, 122) };
                let b = if sel { GOLD } else { bg };
                s.text(1, 5 + row as i32, &format!("{}{line}", if sel { '›' } else { ' ' }), fg, Some(b));
                if owned > 0 {
                    s.text(list_w + 0, 5 + row as i32, &format!("×{owned}"), DIM, Some(bg));
                }
            }
            if let Some(&i) = entries.get(self.sel) {
                let it = &ITEMS[i];
                let mut y = 5;
                if !it.art.is_empty() {
                    // Preview the item at 1:1 pixel scale on a grass swatch.
                    let (pw, ph) = (it.size.0 * 8 + 4, it.size.1 * 8 + 4);
                    s.fill_rect(detail_x, y * 2, pw, ph, Rgb(86, 150, 70));
                    garden::draw_item(s, i, detail_x + 2, y * 2 + 2, t);
                    y += ph / 2 + 1;
                } else {
                    let f = crate::creatures::FLAVORS[i.min(4)];
                    s.text(detail_x, y, "●●●", f.color(), Some(bg));
                    y += 2;
                }
                s.text(detail_x, y, it.name, GOLD, Some(bg));
                y += 1;
                if let Some(e) = it.habitat {
                    s.text(detail_x, y, &format!(" {} habitat ", e.name()), Rgb(20, 20, 30), Some(e.color()));
                    y += 1;
                }
                if it.appeal > 0.0 {
                    s.text(detail_x, y, &format!("Decor +{} (boosts creature appeal)", it.appeal), DIM, Some(bg));
                    y += 1;
                }
                for l in wrap(it.desc, (w - detail_x - 2).max(10) as usize) {
                    s.text(detail_x, y, &l, TEXT, Some(bg));
                    y += 1;
                }
            }
        }
        if let Some((m, c)) = &self.msg {
            s.text(1, rows - 2, m, *c, Some(bg));
        }
        s.text(1, rows - 1, "←→ tabs  ↑↓ choose  enter buy  esc close", DIM, Some(bg));
    }
}
