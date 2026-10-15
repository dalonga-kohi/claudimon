//! Music and sound effects, synthesised in code (no audio files): a tiny chiptune
//! synth with square, triangle and bell voices, drums and an echo.
//!
//! The game only talks about `Track`s and `Sfx`; `Player` turns them into sound.
//! If there's no audio device (SSH, CI, a server), everything is silently a no-op.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const RATE: u32 = 32_000;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Track {
    Sanctuary,
    Night,
    Town,
    Caves,
    Battle,
    Mythos,
}

pub const TRACKS: [Track; 6] = [Track::Sanctuary, Track::Night, Track::Town, Track::Caves, Track::Battle, Track::Mythos];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    Blip,
    Confirm,
    Cancel,
    Pick,
    Coin,
    Heart,
    Bad,
    Befriend,
    Fled,
    Hit,
    Super,
    Faint,
    LevelUp,
    Evolve,
    Warp,
    Bump,
    Quest,
}

pub const SFX: [Sfx; 17] = [
    Sfx::Blip,
    Sfx::Confirm,
    Sfx::Cancel,
    Sfx::Pick,
    Sfx::Coin,
    Sfx::Heart,
    Sfx::Bad,
    Sfx::Befriend,
    Sfx::Fled,
    Sfx::Hit,
    Sfx::Super,
    Sfx::Faint,
    Sfx::LevelUp,
    Sfx::Evolve,
    Sfx::Warp,
    Sfx::Bump,
    Sfx::Quest,
];

// ------------------------------------------------------------------ synth

#[derive(Clone, Copy)]
enum Inst {
    /// Pulse wave with a duty cycle, softened by a low-pass filter.
    Square(f32),
    Triangle,
    /// Sine with a gentle overtone and a long decay.
    Bell,
    Kick,
    Snare,
    Hat,
}

struct Voice {
    inst: Inst,
    gain: f32,
    /// (midi note or None for a rest, length in beats)
    notes: Vec<(Option<f32>, f32)>,
}

/// Parse "C#5", "Bb3", "R" into a midi note number.
fn midi(tok: &str) -> Option<f32> {
    if tok == "R" || tok == "x" {
        return if tok == "x" { Some(60.0) } else { None };
    }
    let mut chars = tok.chars();
    let base = match chars.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (acc, oct) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let oct: i32 = oct.parse().ok()?;
    Some((12 * (oct + 1) + base + acc) as f32)
}

/// A line of notes: "E5 G5 A5:2 R:0.5 | C5". Tokens default to `dflt` beats; ":n" overrides.
fn line(s: &str, dflt: f32) -> Vec<(Option<f32>, f32)> {
    s.split_whitespace()
        .filter(|t| *t != "|")
        .map(|t| {
            let (n, len) = t.split_once(':').map(|(n, l)| (n, l.parse().unwrap_or(dflt))).unwrap_or((t, dflt));
            (midi(n), len)
        })
        .collect()
}

/// Chord tones (as midi notes) for names like "C", "Am", "G7", "Fmaj7", "Dsus4", in a given octave.
fn chord(name: &str, oct: i32) -> Vec<f32> {
    let (root, kind) = match name.chars().nth(1) {
        Some('#') | Some('b') => name.split_at(2),
        _ => name.split_at(1),
    };
    let r = midi(&format!("{root}{oct}")).unwrap_or(60.0);
    let iv: &[f32] = match kind {
        "m" => &[0.0, 3.0, 7.0],
        "7" => &[0.0, 4.0, 7.0, 10.0],
        "m7" => &[0.0, 3.0, 7.0, 10.0],
        "maj7" => &[0.0, 4.0, 7.0, 11.0],
        "sus4" => &[0.0, 5.0, 7.0],
        _ => &[0.0, 4.0, 7.0],
    };
    iv.iter().map(|i| r + i).collect()
}

/// Bass line from chords: one bar (4 beats) per chord, using a pattern of chord-tone indices.
fn bass(chords: &[&str], oct: i32, pattern: &[(usize, f32)]) -> Vec<(Option<f32>, f32)> {
    chords.iter().flat_map(|c| {
        let tones = chord(c, oct);
        pattern.iter().map(move |&(i, len)| (Some(tones[i % tones.len()] + if i >= tones.len() { 12.0 } else { 0.0 }), len)).collect::<Vec<_>>()
    }).collect()
}

/// Arpeggio: chord tones cycled in steps of `step` beats, one bar per chord.
fn arp(chords: &[&str], oct: i32, order: &[usize], step: f32) -> Vec<(Option<f32>, f32)> {
    let per_bar = (4.0 / step) as usize;
    chords
        .iter()
        .flat_map(|c| {
            let tones = chord(c, oct);
            (0..per_bar).map(move |i| {
                let k = order[i % order.len()];
                (Some(tones[k % tones.len()] + 12.0 * (k / tones.len()) as f32), step)
            }).collect::<Vec<_>>()
        })
        .collect()
}

/// A repeated drum pattern for `bars` bars ("k" kick, "s" snare, "h" hat, "." rest; one char per step).
fn drums(pattern: &str, step: f32, bars: usize) -> [Voice; 3] {
    let mk = |which: char, inst: Inst, gain: f32| Voice {
        inst,
        gain,
        notes: (0..bars).flat_map(|_| pattern.chars().map(|c| (if c == which { Some(60.0) } else { None }, step)).collect::<Vec<_>>()).collect(),
    };
    [mk('k', Inst::Kick, 0.5), mk('s', Inst::Snare, 0.22), mk('h', Inst::Hat, 0.07)]
}

fn freq(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

/// Deterministic noise in -1..1.
fn noise(i: usize) -> f32 {
    let mut x = (i as u32).wrapping_mul(2654435761) ^ 0x9E37_79B9;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x % 20001) as f32 / 10000.0 - 1.0
}

/// Render one voice into `out`, starting at sample 0, at `bpm`.
fn render_voice(out: &mut [f32], v: &Voice, bpm: f32) {
    let spb = RATE as f32 * 60.0 / bpm;
    let mut t0 = 0.0f32;
    let mut lp = 0.0f32;
    for &(note, beats) in &v.notes {
        let start = t0 as usize;
        let len = (beats * spb) as usize;
        t0 += beats * spb;
        let Some(m) = note else { continue };
        let f = freq(m);
        // Gate: notes ring for most of their length, then release.
        let (attack, gate, release): (f32, f32, f32) = match v.inst {
            Inst::Bell => (0.002, len as f32, RATE as f32 * 0.6),
            Inst::Kick | Inst::Snare | Inst::Hat => (0.0, 0.0, 0.0),
            _ => (0.006, len as f32 * 0.86, RATE as f32 * 0.05),
        };
        let total = match v.inst {
            Inst::Kick => (RATE as f32 * 0.16) as usize,
            Inst::Snare => (RATE as f32 * 0.14) as usize,
            Inst::Hat => (RATE as f32 * 0.04) as usize,
            _ => (gate + release) as usize,
        };
        for i in 0..total {
            let idx = start + i;
            if idx >= out.len() {
                break;
            }
            let t = i as f32 / RATE as f32;
            let env = match v.inst {
                Inst::Kick | Inst::Snare | Inst::Hat => 1.0 - i as f32 / total as f32,
                _ => {
                    let a = (t / attack.max(1e-4)).min(1.0);
                    let r = if (i as f32) < gate { 1.0 } else { 1.0 - (i as f32 - gate) / release.max(1.0) };
                    a * r.max(0.0)
                }
            };
            let ph = (f * t).fract();
            let s = match v.inst {
                Inst::Square(duty) => {
                    let raw = if ph < duty { 1.0 } else { -1.0 };
                    lp += (raw - lp) * 0.35; // soften the edges
                    lp * 0.8
                }
                Inst::Triangle => 4.0 * (ph - 0.5).abs() - 1.0,
                Inst::Bell => {
                    let d = (-t * 3.5).exp();
                    ((std::f32::consts::TAU * f * t).sin() + 0.3 * (std::f32::consts::TAU * f * 2.0 * t).sin() * (-t * 6.0).exp()) * d
                }
                Inst::Kick => {
                    let fk = 45.0 + 110.0 * (-t * 30.0).exp();
                    (std::f32::consts::TAU * fk * t).sin() * env
                }
                Inst::Snare => (noise(idx) * 0.8 + (std::f32::consts::TAU * 190.0 * t).sin() * 0.3) * env * env,
                Inst::Hat => (noise(idx) - noise(idx + 1)) * 0.5 * env * env,
            };
            out[idx] += s * env * v.gain;
        }
    }
}

fn beats_of(v: &Voice) -> f32 {
    v.notes.iter().map(|n| n.1).sum()
}

/// Mix voices, add an optional echo (delay seconds, feedback), and soft-clip.
fn mix(voices: &[Voice], bpm: f32, echo: Option<(f32, f32)>) -> Vec<f32> {
    let beats = voices.iter().map(beats_of).fold(0.0, f32::max);
    let n = (beats * 60.0 / bpm * RATE as f32) as usize;
    let mut out = vec![0.0f32; n];
    for v in voices {
        render_voice(&mut out, v, bpm);
    }
    if let Some((secs, fb)) = echo {
        let d = (secs * RATE as f32) as usize;
        for i in d..n {
            out[i] += out[i - d] * fb;
        }
        // Wrap the tail so the loop seam echoes too.
        for i in 0..d.min(n) {
            out[i] += out[n - d + i] * fb * 0.5;
        }
    }
    for s in out.iter_mut() {
        *s = (*s * 0.9).tanh();
    }
    out
}

fn voice(inst: Inst, gain: f32, notes: Vec<(Option<f32>, f32)>) -> Voice {
    Voice { inst, gain, notes }
}

/// Compose and render a music loop.
pub fn render_track(t: Track) -> Vec<f32> {
    match t {
        Track::Sanctuary => {
            let ch = ["C", "Am", "F", "G", "C", "Am", "F", "G"];
            let mel = "E5 G5 A5 G5 | E5:2 C5 D5 | F5 A5 G5 F5 | E5:2 D5:2 | E5 G5 C6:1.5 B5:0.5 | A5:2 G5 E5 | F5 E5 D5 C5 | D5:2 R G4";
            mix(
                &[
                    voice(Inst::Square(0.25), 0.2, line(mel, 1.0)),
                    voice(Inst::Bell, 0.1, arp(&ch, 4, &[0, 1, 2, 1], 0.5)),
                    voice(Inst::Triangle, 0.3, bass(&ch, 2, &[(0, 1.0), (2, 1.0), (0, 1.0), (2, 1.0)])),
                    voice(Inst::Hat, 0.03, (0..64).map(|i| (if i % 2 == 1 { Some(60.0) } else { None }, 0.5)).collect()),
                ],
                96.0,
                None,
            )
        }
        Track::Night => {
            let ch = ["Am", "F", "C", "G", "Am", "F", "C", "E"];
            let mel = "A4:2 C5 E5 | F5:3 E5 | E5:2 G5 E5 | D5:4 | A4:2 C5 E5 | F5:2 A5:2 | G5:2 E5 C5 | B4:4";
            mix(
                &[
                    voice(Inst::Bell, 0.26, line(mel, 1.0)),
                    voice(Inst::Bell, 0.07, arp(&ch, 4, &[0, 2, 1, 2], 0.5)),
                    voice(Inst::Triangle, 0.24, bass(&ch, 2, &[(0, 4.0)])),
                ],
                68.0,
                Some((0.42, 0.35)),
            )
        }
        Track::Town => {
            let ch = ["F", "Bb", "C", "F", "Dm", "Bb", "C", "C"];
            let mel = "F5:0.5 A5:0.5 C6 A5:0.5 F5:0.5 G5 | Bb5 A5:0.5 G5:0.5 F5 D5 | C5:0.5 E5:0.5 G5 E5:0.5 C5:0.5 E5 | F5:3 R | D5:0.5 F5:0.5 A5 F5:0.5 D5:0.5 F5 | Bb5 D6 C6 Bb5 | A5 G5 E5 C5 | G5:2 R C5";
            let [k, s, h] = drums("k.h.s.h.k.h.s.hh", 0.25, 8);
            mix(
                &[
                    voice(Inst::Square(0.5), 0.16, line(mel, 1.0)),
                    voice(Inst::Triangle, 0.3, bass(&ch, 2, &[(0, 0.5), (2, 0.5), (0, 0.5), (2, 0.5), (0, 0.5), (2, 0.5), (1, 0.5), (2, 0.5)])),
                    voice(Inst::Bell, 0.06, arp(&ch, 4, &[0, 1, 2, 3], 1.0)),
                    k,
                    s,
                    h,
                ],
                116.0,
                None,
            )
        }
        Track::Battle => {
            let ch = ["Em", "Em", "D", "B", "Em", "C", "D", "B"];
            let mel = "E5:0.5 E5:0.5 G5:0.5 E5:0.5 B5 A5 | G5:0.5 E5:0.5 G5:0.5 A5:0.5 G5 E5 | F#5:0.5 D5:0.5 F#5:0.5 A5:0.5 D6 C6 | B5:2 D#5 F#5 | E5:0.5 G5:0.5 B5:0.5 E6:0.5 D6 B5 | C6:0.5 B5:0.5 A5:0.5 G5:0.5 E5 G5 | A5:0.5 G5:0.5 F#5:0.5 E5:0.5 D5 F#5 | B4 D#5 F#5 B5";
            let [k, s, h] = drums("k.h.s.h.k.k.s.h.", 0.25, 8);
            mix(
                &[
                    voice(Inst::Square(0.25), 0.18, line(mel, 1.0)),
                    voice(Inst::Square(0.5), 0.12, bass(&ch, 2, &[(0, 0.5), (0, 0.5), (2, 0.5), (0, 0.5), (0, 0.5), (0, 0.5), (2, 0.5), (1, 0.5)])),
                    voice(Inst::Square(0.125), 0.05, arp(&ch, 4, &[0, 1, 2, 1], 0.25)),
                    k,
                    s,
                    h,
                ],
                152.0,
                None,
            )
        }
        Track::Caves => {
            let ch = ["Dm", "Dm", "Bb", "A", "Dm", "Gm", "Bb", "A"];
            let mel = "D5:3 F5 | E5:4 | D5:2 A4:2 | C#5:4 | F5:3 E5 | D5:2 Bb4:2 | A4:2 D5:2 | E5:4";
            mix(
                &[
                    voice(Inst::Bell, 0.22, line(mel, 1.0)),
                    voice(Inst::Triangle, 0.22, bass(&ch, 2, &[(0, 4.0)])),
                    voice(Inst::Bell, 0.05, arp(&ch, 5, &[2, 0, 1, 0], 1.0)),
                ],
                60.0,
                Some((0.55, 0.45)),
            )
        }
        Track::Mythos => {
            let ch = ["D", "G", "Em7", "A", "D", "G", "Em7", "A"];
            let mel = "A5:2 F#5 A5 | B5:3 A5 | G5:2 E5:2 | A5:4 | D6:2 C#6 B5 | G5:3 B5 | A5:2 F#5 E5 | D5:4";
            mix(
                &[
                    voice(Inst::Bell, 0.28, line(mel, 1.0)),
                    voice(Inst::Bell, 0.08, arp(&ch, 4, &[0, 1, 2, 3, 2, 1], 0.25)),
                    voice(Inst::Triangle, 0.24, bass(&ch, 2, &[(0, 2.0), (2, 2.0)])),
                ],
                72.0,
                Some((0.5, 0.3)),
            )
        }
    }
}

/// Render a short sound effect.
pub fn render_sfx(s: Sfx) -> Vec<f32> {
    let one = |inst: Inst, gain: f32, notes: &str, bpm: f32, echo: Option<(f32, f32)>| mix(&[voice(inst, gain, line(notes, 0.25))], bpm, echo);
    let mut out = match s {
        Sfx::Blip => one(Inst::Square(0.25), 0.25, "C6:0.15", 120.0, None),
        Sfx::Confirm => one(Inst::Square(0.25), 0.3, "E6 A6", 240.0, None),
        Sfx::Cancel => one(Inst::Square(0.25), 0.3, "A5 E5", 240.0, None),
        Sfx::Pick => one(Inst::Square(0.5), 0.3, "C6:0.5 G6", 300.0, None),
        Sfx::Coin => one(Inst::Square(0.5), 0.3, "B5:0.5 E6:1.5", 300.0, None),
        Sfx::Heart => one(Inst::Bell, 0.5, "C6 E6 G6", 360.0, None),
        Sfx::Bad => one(Inst::Square(0.5), 0.3, "D3:0.5 C#3:1", 200.0, None),
        Sfx::Befriend => one(Inst::Square(0.25), 0.3, "C5 E5 G5 C6:0.5 R:0.25 G5 C6:1.5", 220.0, None),
        Sfx::Fled => one(Inst::Triangle, 0.4, "G4 E4 C4:1", 200.0, None),
        Sfx::Hit => mix(&[voice(Inst::Snare, 0.6, line("x:0.5", 0.5)), voice(Inst::Square(0.5), 0.2, line("C3:0.25", 0.25))], 200.0, None),
        Sfx::Super => mix(&[voice(Inst::Snare, 0.7, line("x x", 0.25)), voice(Inst::Square(0.25), 0.25, line("G5 C6", 0.25))], 240.0, None),
        Sfx::Faint => one(Inst::Triangle, 0.45, "C5 A4 F4 D4 C4:2", 360.0, None),
        Sfx::LevelUp => one(Inst::Square(0.25), 0.3, "C5 E5 G5 C6 E6 G6:2", 420.0, None),
        Sfx::Evolve => one(Inst::Bell, 0.4, "C5 E5 G5 B5 D6 F#6 A6 C7:3", 300.0, Some((0.12, 0.4))),
        Sfx::Warp => mix(&[voice(Inst::Hat, 0.5, line("x x x x x x x x", 0.12))], 120.0, Some((0.05, 0.6))),
        Sfx::Bump => one(Inst::Triangle, 0.35, "C3:0.3", 240.0, None),
        Sfx::Quest => one(Inst::Square(0.25), 0.3, "G5 C6 E6 G6:0.5 E6:0.5 G6:2", 300.0, None),
    };
    // Fade the very end to avoid clicks.
    let n = out.len();
    for i in 0..n.min(400) {
        out[n - 1 - i] *= i as f32 / 400.0;
    }
    out
}

// ----------------------------------------------------------------- player

const MUSIC_VOLUME: f32 = 0.35;
const SFX_VOLUME: f32 = 0.55;
const FADE_SECS: f32 = 0.6;

struct Device {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    music: Option<rodio::Sink>,
    sfx: Vec<rodio::Sink>,
}

/// Plays music and sound effects; harmless when no audio device exists.
pub struct Player {
    device: Option<Device>,
    sfx_bufs: HashMap<Sfx, Arc<Vec<f32>>>,
    tracks: Arc<Mutex<HashMap<Track, Arc<Vec<f32>>>>>,
    playing: Option<Track>,
    wanted: Option<Track>,
    fade: f32,
}

impl Player {
    pub fn new(enabled: bool) -> Player {
        let device = if enabled {
            rodio::OutputStream::try_default().ok().map(|(stream, handle)| Device { _stream: stream, handle, music: None, sfx: Vec::new() })
        } else {
            None
        };
        let tracks: Arc<Mutex<HashMap<Track, Arc<Vec<f32>>>>> = Arc::default();
        if device.is_some() {
            // Compose the music in the background so startup stays instant.
            let t = tracks.clone();
            std::thread::spawn(move || {
                for track in TRACKS {
                    let buf = Arc::new(render_track(track));
                    if let Ok(mut m) = t.lock() {
                        m.insert(track, buf);
                    }
                }
            });
        }
        let sfx_bufs = if device.is_some() { SFX.iter().map(|&s| (s, Arc::new(render_sfx(s)))).collect() } else { HashMap::new() };
        Player { device, sfx_bufs, tracks, playing: None, wanted: None, fade: 1.0 }
    }

    pub fn available(&self) -> bool {
        self.device.is_some()
    }

    pub fn play(&mut self, s: Sfx) {
        let (Some(dev), Some(buf)) = (&mut self.device, self.sfx_bufs.get(&s)) else { return };
        dev.sfx.retain(|k| !k.empty());
        if dev.sfx.len() >= 6 {
            return; // don't pile up a wall of noise
        }
        if let Ok(sink) = rodio::Sink::try_new(&dev.handle) {
            sink.set_volume(SFX_VOLUME);
            sink.append(rodio::buffer::SamplesBuffer::new(1, RATE, buf.as_ref().clone()));
            dev.sfx.push(sink);
        }
    }

    /// Ask for a track (None = silence). Switches happen with a short fade.
    pub fn set_track(&mut self, t: Option<Track>) {
        self.wanted = t;
    }

    pub fn update(&mut self, dt: f32) {
        let Some(dev) = &mut self.device else { return };
        if self.wanted != self.playing {
            // Fade out what's playing, then start the new track.
            self.fade -= dt / FADE_SECS;
            if let Some(m) = &dev.music {
                m.set_volume(MUSIC_VOLUME * self.fade.max(0.0));
            }
            if self.fade > 0.0 && dev.music.is_some() {
                return;
            }
            if let Some(m) = dev.music.take() {
                m.stop();
            }
            self.playing = None;
            let Some(want) = self.wanted else {
                self.fade = 1.0;
                return;
            };
            let buf = self.tracks.lock().ok().and_then(|m| m.get(&want).cloned());
            let Some(buf) = buf else { return }; // still composing; try next frame
            if let Ok(sink) = rodio::Sink::try_new(&dev.handle) {
                use rodio::Source;
                sink.set_volume(0.0);
                sink.append(rodio::buffer::SamplesBuffer::new(1, RATE, buf.as_ref().clone()).repeat_infinite());
                dev.music = Some(sink);
                self.playing = Some(want);
                self.fade = 0.0;
            }
        } else if self.fade < 1.0 {
            self.fade = (self.fade + dt / FADE_SECS).min(1.0);
            if let Some(m) = &dev.music {
                m.set_volume(MUSIC_VOLUME * self.fade);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_parsing() {
        assert_eq!(midi("A4"), Some(69.0));
        assert_eq!(midi("C4"), Some(60.0));
        assert_eq!(midi("C#4"), Some(61.0));
        assert_eq!(midi("Bb3"), Some(58.0));
        assert_eq!(midi("R"), None);
        assert_eq!(chord("Am", 4), vec![69.0, 72.0, 76.0]);
        assert_eq!(chord("F#m", 3), vec![54.0, 57.0, 61.0]);
        assert_eq!(line("E5 G5:2 | R:0.5", 1.0).len(), 3);
    }

    #[test]
    fn every_track_renders_clean_loops() {
        for t in TRACKS {
            let buf = render_track(t);
            let secs = buf.len() as f32 / RATE as f32;
            assert!((8.0..60.0).contains(&secs), "{t:?} is {secs:.1}s");
            assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{t:?} clips or NaNs");
            let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
            assert!(rms > 0.02, "{t:?} is too quiet ({rms})");
            // All voices of a track line up to the same number of bars.
        }
    }

    #[test]
    fn every_sfx_is_short_and_audible() {
        for s in SFX {
            let buf = render_sfx(s);
            let secs = buf.len() as f32 / RATE as f32;
            assert!(secs > 0.02 && secs < 2.5, "{s:?} is {secs:.2}s");
            assert!(buf.iter().any(|x| x.abs() > 0.05), "{s:?} is silent");
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }

    #[test]
    fn a_silent_player_is_harmless() {
        let mut p = Player::new(false);
        assert!(!p.available());
        p.play(Sfx::Coin);
        p.set_track(Some(Track::Town));
        p.update(1.0);
    }
}

#[cfg(test)]
mod device {
    use super::*;

    /// Opens the real audio device and starts a track at zero volume (no audible sound).
    /// `cargo test device_smoke -- --ignored`
    #[test]
    #[ignore]
    fn device_smoke() {
        let mut p = Player::new(true);
        if !p.available() {
            eprintln!("no audio device; skipping");
            return;
        }
        p.set_track(Some(Track::Sanctuary));
        for _ in 0..400 {
            p.update(0.0); // dt 0: the fade-in never rises above zero
            if p.playing.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(p.playing, Some(Track::Sanctuary), "music should start once composed");
        p.set_track(None);
        for _ in 0..10 {
            p.update(1.0);
        }
        assert_eq!(p.playing, None);
    }
}
