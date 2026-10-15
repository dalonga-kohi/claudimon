//! Half-block pixel renderer: every terminal cell shows two vertically stacked
//! pixels using '▀' (fg = top pixel, bg = bottom pixel). Text can be overlaid.

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
};
use std::io::{self, Write};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }
    pub fn mul(self, m: [f32; 3]) -> Rgb {
        let f = |a: u8, k: f32| (a as f32 * k).clamp(0.0, 255.0) as u8;
        Rgb(f(self.0, m[0]), f(self.1, m[1]), f(self.2, m[2]))
    }
    fn to_color(self) -> Color {
        Color::Rgb { r: self.0, g: self.1, b: self.2 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    ch: char,
    fg: Rgb,
    bg: Rgb,
}

/// A pixel canvas (w x h pixels, h = 2 * rows) plus a text overlay layer.
pub struct Screen {
    pub w: usize,
    pub h: usize,
    px: Vec<Rgb>,
    text: Vec<Option<(char, Rgb, Option<Rgb>)>>,
    prev: Vec<Cell>,
    force: bool,
}

impl Screen {
    pub fn new(cols: u16, rows: u16) -> Self {
        let (w, r) = (cols as usize, rows as usize);
        Screen {
            w,
            h: r * 2,
            px: vec![Rgb::default(); w * r * 2],
            text: vec![None; w * r],
            prev: vec![Cell { ch: ' ', fg: Rgb::default(), bg: Rgb::default() }; w * r],
            force: true,
        }
    }

    pub fn rows(&self) -> usize {
        self.h / 2
    }

    pub fn clear(&mut self, c: Rgb) {
        self.px.fill(c);
        self.text.fill(None);
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Rgb {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize]
        } else {
            Rgb::default()
        }
    }

    /// Blend a colour over an existing pixel (alpha 0..1).
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        let old = self.get(x, y);
        self.set(x, y, old.lerp(c, a));
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Fill this screen's pixels by averaging k x k blocks of a larger screen.
    pub fn downsample_from(&mut self, big: &Screen, k: usize) {
        let n = (k * k) as u32;
        for y in 0..self.h {
            for x in 0..self.w {
                let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
                for dy in 0..k {
                    for dx in 0..k {
                        let p = big.get((x * k + dx) as i32, (y * k + dy) as i32);
                        r += p.0 as u32;
                        g += p.1 as u32;
                        b += p.2 as u32;
                    }
                }
                self.px[y * self.w + x] = Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8);
            }
        }
    }

    /// Ambient lighting plus point lights (x, y, radius, colour) that only matter in the dark.
    pub fn light(&mut self, ambient: [f32; 3], lights: &[(f32, f32, f32, Rgb)]) {
        let dark = (1.0 - (ambient[0] + ambient[1] + ambient[2]) / 3.0).clamp(0.0, 1.0);
        if lights.is_empty() || dark < 0.05 {
            self.tint(ambient);
            return;
        }
        let mut m: Vec<[f32; 3]> = vec![ambient; self.px.len()];
        for &(lx, ly, r, c) in lights {
            let (x0, x1) = (((lx - r).floor() as i32).max(0), ((lx + r).ceil() as i32).min(self.w as i32 - 1));
            let (y0, y1) = (((ly - r).floor() as i32).max(0), ((ly + r).ceil() as i32).min(self.h as i32 - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let d = ((x as f32 - lx).powi(2) + (y as f32 - ly).powi(2)).sqrt();
                    if d >= r {
                        continue;
                    }
                    let k = (1.0 - d / r).powi(2) * dark * 1.5;
                    let e = &mut m[y as usize * self.w + x as usize];
                    e[0] += c.0 as f32 / 255.0 * k;
                    e[1] += c.1 as f32 / 255.0 * k;
                    e[2] += c.2 as f32 / 255.0 * k;
                }
            }
        }
        for (p, k) in self.px.iter_mut().zip(m) {
            *p = p.mul([k[0].min(1.2), k[1].min(1.2), k[2].min(1.2)]);
        }
    }

    /// Apply a per-pixel colour multiplier (used for day/night lighting).
    pub fn tint(&mut self, m: [f32; 3]) {
        for p in self.px.iter_mut() {
            *p = p.mul(m);
        }
    }

    /// Write text at cell coordinates. `bg: None` keeps the pixels behind (top pixel shown as bg).
    pub fn text(&mut self, col: i32, row: i32, s: &str, fg: Rgb, bg: Option<Rgb>) {
        let rows = self.rows() as i32;
        if row < 0 || row >= rows {
            return;
        }
        for (i, ch) in s.chars().enumerate() {
            let c = col + i as i32;
            if c >= 0 && (c as usize) < self.w {
                self.text[row as usize * self.w + c as usize] = Some((ch, fg, bg));
            }
        }
    }

    /// A solid panel in cell coordinates with a rounded border.
    pub fn panel(&mut self, col: i32, row: i32, w: i32, h: i32, bg: Rgb, border: Rgb) {
        for r in row..row + h {
            for c in col..col + w {
                let ch = match (r == row, r == row + h - 1, c == col, c == col + w - 1) {
                    (true, _, true, _) => '╭',
                    (true, _, _, true) => '╮',
                    (_, true, true, _) => '╰',
                    (_, true, _, true) => '╯',
                    (true, _, _, _) | (_, true, _, _) => '─',
                    (_, _, true, _) | (_, _, _, true) => '│',
                    _ => ' ',
                };
                self.text(c, r, &ch.to_string(), border, Some(bg));
            }
        }
    }

    /// The text layer as plain lines (pixels shown as '·'), for checking layouts.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for r in 0..self.rows() {
            for c in 0..self.w {
                out.push(self.text[r * self.w + c].map(|t| t.0).unwrap_or('·'));
            }
            out.push('\n');
        }
        out
    }

    /// Every cell as one line: top-pixel, bottom-pixel, then (if text) the char and its colours.
    /// Used to render screenshots with a real font.
    pub fn to_cells(&self) -> String {
        let mut out = format!("{} {}\n", self.w, self.rows());
        for r in 0..self.rows() {
            for c in 0..self.w {
                let top = self.px[(r * 2) * self.w + c];
                let bot = self.px[(r * 2 + 1) * self.w + c];
                out.push_str(&format!("{} {} {} {} {} {}", top.0, top.1, top.2, bot.0, bot.1, bot.2));
                if let Some((ch, fg, bg)) = self.text[r * self.w + c] {
                    let bg = bg.unwrap_or(top.lerp(bot, 0.5));
                    out.push_str(&format!(" {} {} {} {} {} {} {}", ch as u32, fg.0, fg.1, fg.2, bg.0, bg.1, bg.2));
                }
                out.push('\n');
            }
        }
        out
    }

    /// Export the frame as a binary PPM (text cells approximated by their colours).
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut px = self.px.clone();
        for r in 0..self.rows() {
            for c in 0..self.w {
                if let Some((ch, fg, bg)) = self.text[r * self.w + c] {
                    let top = (r * 2) * self.w + c;
                    let bot = top + self.w;
                    let b = bg.unwrap_or(px[top].lerp(px[bot], 0.5));
                    let ink = if ch == ' ' || "─│╭╮╰╯".contains(ch) { b } else { b.lerp(fg, 0.7) };
                    px[top] = ink;
                    px[bot] = if ch == ' ' { b } else { b.lerp(fg, 0.35) };
                }
            }
        }
        let mut out = format!("P6 {} {} 255\n", self.w, self.h).into_bytes();
        for p in px {
            out.extend_from_slice(&[p.0, p.1, p.2]);
        }
        out
    }

    pub fn invalidate(&mut self) {
        self.force = true;
    }

    /// Diff against the previous frame and emit only changed cells.
    pub fn flush(&mut self, out: &mut impl Write) -> io::Result<()> {
        let rows = self.rows();
        let mut last_fg = None;
        let mut last_bg = None;
        let mut cursor_ok = false;
        for r in 0..rows {
            cursor_ok = false;
            for c in 0..self.w {
                let i = r * self.w + c;
                let top = self.px[(r * 2) * self.w + c];
                let bot = self.px[(r * 2 + 1) * self.w + c];
                let cell = match self.text[i] {
                    Some((ch, fg, bg)) => Cell { ch, fg, bg: bg.unwrap_or(top.lerp(bot, 0.5)) },
                    None if top == bot => Cell { ch: ' ', fg: top, bg: top },
                    None => Cell { ch: '▀', fg: top, bg: bot },
                };
                if !self.force && self.prev[i] == cell {
                    cursor_ok = false;
                    continue;
                }
                self.prev[i] = cell;
                if !cursor_ok {
                    queue!(out, MoveTo(c as u16, r as u16))?;
                    cursor_ok = true;
                }
                if last_fg != Some(cell.fg) && cell.ch != ' ' {
                    queue!(out, SetForegroundColor(cell.fg.to_color()))?;
                    last_fg = Some(cell.fg);
                }
                if last_bg != Some(cell.bg) {
                    queue!(out, SetBackgroundColor(cell.bg.to_color()))?;
                    last_bg = Some(cell.bg);
                }
                queue!(out, Print(cell.ch))?;
            }
        }
        let _ = cursor_ok;
        self.force = false;
        out.flush()
    }
}
