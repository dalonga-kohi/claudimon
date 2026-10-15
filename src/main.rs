mod arena;
mod audio;
mod battle;
mod clock;
mod creatures;
mod dex;
mod encounter;
mod game;
mod garden;
mod gfx;
mod quests;
mod save;
mod story;
mod team;
mod ui;
mod weather;
mod world;

use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableFocusChange, EnableFocusChange, Event, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use game::Game;
use gfx::{Rgb, Screen};
use std::io::{self, Write};
use std::time::{Duration, Instant};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--snapshot") => return snapshot(&args[2..]),
        Some("--peek") => {
            // Ignore write errors (e.g. `claudimon --peek | head`): a closed pipe is not a crash.
            let _ = writeln!(io::stdout(), "{}", peek());
            return Ok(());
        }
        Some("--reset") => return reset(args.iter().any(|a| a == "--yes" || a == "-y")),
        Some("--version") | Some("-V") => {
            let _ = writeln!(io::stdout(), "claudimon {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help") | Some("-h") => {
            let help = format!(
                "claudimon {} — a cosy terminal creature sanctuary\n\nUSAGE:\n  claudimon                play\n  claudimon --peek         one-line status of your Sanctuary (shell prompts, slow builds)\n  claudimon --reset        erase your save and start over (asks first)\n  claudimon --reset --yes  erase without asking\n  claudimon --mute         play without any audio\n  claudimon --version      print the version\n\nIn game, press ? for the controls.\nSave file: {}  (override with CLAUDIMON_SAVE=/path/to/save.json)",
                env!("CARGO_PKG_VERSION"),
                save::path().display()
            );
            let _ = writeln!(io::stdout(), "{help}");
            return Ok(());
        }
        _ => {}
    }

    // Open the audio device before taking over the terminal, so any driver chatter on stderr
    // doesn't land on top of the game.
    let mute = args.iter().any(|a| a == "--mute");
    let player = audio::Player::new(!mute);

    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(out, EnterAlternateScreen, Hide)?;
    // Ask for real key-release events where supported (kitty, foot, WezTerm, ...),
    // so holding a direction walks smoothly and letting go stops immediately.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(out, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES | KeyboardEnhancementFlags::REPORT_EVENT_TYPES))?;
    }
    execute!(out, EnableFocusChange)?;
    // Restore the terminal even if we panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if enhanced {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        default_hook(info);
    }));

    let res = run(&mut out, enhanced, player);

    if enhanced {
        execute!(out, PopKeyboardEnhancementFlags)?;
    }
    execute!(out, DisableFocusChange)?;
    terminal::disable_raw_mode()?;
    execute!(out, LeaveAlternateScreen, Show)?;
    res
}

/// Erase the save file, after showing what would be lost and asking for confirmation.
fn reset(yes: bool) -> io::Result<()> {
    let path = save::path();
    let mut out = io::stdout();
    if !path.exists() {
        writeln!(out, "Nothing to erase — there is no save at {}.", path.display())?;
        return Ok(());
    }
    if !yes {
        let summary = match save::load_readonly() {
            Some(d) => format!(
                "{} Claudimon, ✦ {} tokens, {} of the Claudidex, {} cup{}, {}",
                d.mons.len(),
                d.tokens,
                d.befriended.len(),
                d.badges.len(),
                if d.badges.len() == 1 { "" } else { "s" },
                story::chapter_title(&d)
            ),
            None => "an unreadable save file".into(),
        };
        writeln!(out, "This will permanently erase your Sanctuary:\n  {summary}\n  ({})\n", path.display())?;
        write!(out, "Type 'erase' to confirm: ")?;
        out.flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if answer.trim().to_lowercase() != "erase" {
            writeln!(out, "Cancelled — nothing was changed.")?;
            return Ok(());
        }
    }
    std::fs::remove_file(&path)?;
    let _ = std::fs::remove_file(path.with_extension("json.tmp"));
    writeln!(out, "Save erased. A brand-new Sanctuary is waiting — run `claudimon` to begin. ✻")?;
    Ok(())
}

/// A one-line status of your Sanctuary, computed without touching the save file.
fn peek() -> String {
    let save::Loaded::Existing(mut d) = save::load() else {
        return "✻ Claudimon │ no Sanctuary yet — run `claudimon` to begin".into();
    };
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(0);
    let now = save::now();
    garden::simulate(&mut d, now, &mut rng);
    let ripe = d.crops.values().filter(|c| c.progress(now) >= 1.0).count();
    let gifts = d.mons.iter().filter(|m| m.gift).count();
    let mut parts = vec![format!("✦ {} (+{:.0}/h)", d.tokens, garden::tokens_per_hour(&d)), format!("♥ {}", d.mons.len())];
    if ripe > 0 {
        parts.push(format!("✿ {ripe} ripe"));
    }
    if gifts > 0 {
        parts.push(format!("✉ {gifts} gift{}", if gifts == 1 { "" } else { "s" }));
    }
    if d.quests.day == quests::today() && !d.quests.quests.is_empty() {
        let done = d.quests.quests.iter().filter(|q| q.done()).count();
        parts.push(format!("prompts {done}/{}", d.quests.quests.len()));
    }
    if d.story.chapter > 0 {
        parts.push(story::chapter_title(&d).to_string());
    }
    format!("✻ Claudimon │ {}", parts.join(" │ "))
}

fn new_game() -> Game {
    let (data, note) = match save::load() {
        save::Loaded::Fresh => (save::SaveData::default(), None),
        save::Loaded::Existing(d) => (d, None),
        save::Loaded::Corrupt(bak) => (save::SaveData::default(), Some(format!("Save was unreadable; backed up to {}", bak.display()))),
    };
    let away = save::now() - data.last_seen;
    let first_time = data.steps == 0;
    let mut g = Game::new(data);
    g.persist = true;
    g.catch_up();
    if first_time {
        g.toast("Welcome to your Sanctuary! Head east → to meet wild Claudimon.", Rgb(255, 210, 160));
        g.toast("Press ? for controls, o for music & sound options.", Rgb(200, 200, 220));
    } else if away > 60 && matches!(g.mode, game::Mode::Explore) {
        let (h, m) = (away / 3600, (away % 3600) / 60);
        let span = if h > 0 { format!("{h}h {m}m") } else { format!("{m}m") };
        g.toast(format!("Welcome back! You were away for {span}."), Rgb(255, 210, 160));
    }
    if let Some(n) = note {
        g.toast(n, Rgb(255, 140, 120));
    }
    g
}

fn run(out: &mut impl Write, enhanced: bool, mut player: audio::Player) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let mut screen = Screen::new(cols, rows);
    let mut game = new_game();
    game.enhanced_keys = enhanced;
    game.audio_available = player.available();
    let frame = Duration::from_millis(33);
    let mut last = Instant::now();

    while !game.quit {
        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(k) => {
                    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                        game.quit = true;
                    }
                    game.input(k.code, k.kind, k.modifiers);
                }
                Event::FocusLost => game.input(KeyCode::Null, event::KeyEventKind::Release, KeyModifiers::NONE),
                Event::Resize(c, r) => {
                    screen = Screen::new(c, r);
                    screen.invalidate();
                }
                _ => {}
            }
        }
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;
        game.update(dt);
        player.set_track(game.music());
        for s in game.take_sfx() {
            player.play(s);
        }
        player.update(dt);
        game.draw(&mut screen);
        screen.flush(out)?;
        let spent = now.elapsed();
        if spent < frame {
            event::poll(frame - spent)?;
        }
    }
    game.save_now();
    Ok(())
}

/// Dev tool: render one frame of a scene to a PPM image without touching the save.
/// Usage: claudimon --snapshot <garden|meadow|encounter[:N]|dex|catalog|sheet> <cols> <rows> <out.ppm>
/// Set CLAUDIMON_HOUR to preview a time of day.
fn snapshot(args: &[String]) -> io::Result<()> {
    let scene = args.first().map(String::as_str).unwrap_or("garden");
    let cols: u16 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(80);
    let rows: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(24);
    let out = args.get(3).cloned().unwrap_or_else(|| "snapshot.ppm".into());
    let mut screen = Screen::new(cols, rows);
    let mut data = save::SaveData::default();
    data.steps = 1;
    for sp in creatures::SPECIES.iter() {
        data.seen.insert(sp.name.into());
    }
    // No "new area discovered" toasts in screenshots.
    data.visited.extend(world::ALL_MAPS.iter().map(|m| m.key().to_string()));
    let (scene, arg) = scene.split_once(':').unwrap_or((scene, ""));
    match scene {
        "people" => game::draw_people_sheet(&mut screen),
        "sheet" => {
            screen.clear(Rgb(60, 110, 60));
            for i in 0..creatures::SPECIES.len() {
                let (x, y) = ((i % 8) as i32 * 30 + 3, (i / 8) as i32 * 28 + 2);
                creatures::draw(&mut screen, i, x, y, 3, false, i == 0, 0.0);
            }
        }
        _ => {
            let spots = [("meadow", 3, world::MEADOW_ENTRY_Y), ("woods", 40, 50), ("shore", 4, 25), ("caves", 35, 44), ("town", 14, 10), ("claude", 14, 10), ("prompts", 14, 10), ("battle", 23, 20), ("team", 23, 20), ("evolve", 23, 20)];
            if scene == "lighthouse" {
                let (lx, ly) = world::Map::load(world::MapId::Shore, 0).find(world::Tile::Lighthouse).unwrap_or((60, 6));
                data.map = "shore".into();
                data.x = lx - 2;
                data.y = ly + 5;
                data.badges.insert("haiku".into());
                data.badges.insert("sonnet".into());
                data.story.set("lantern");
            }
            if scene == "isle" {
                data.map = "isle".into();
                data.x = 28;
                data.y = 12;
                for f in ["trial_trust", "trial_strength", "trial_wisdom"] {
                    data.story.set(f);
                }
            }
            if let Some(&(key, x, y)) = spots.iter().find(|(k, _, _)| *k == scene) {
                data.map = if matches!(key, "claude" | "prompts" | "battle" | "team" | "evolve") { "town".into() } else { key.into() };
                data.x = x;
                data.y = y;
            }
            if scene == "dex" {
                data.mons.push(save::OwnedMon { friendship: 3, ..save::OwnedMon::new("Haikitten", false) });
                data.known_fav.insert("Haikitten".into());
            }
            if scene == "garden" {
                for name in ["Haikitten", "Tokenling", "Rubberduck", "Cloudle", "Bugbyte", "Puddlefin"] {
                    data.mons.push(save::OwnedMon::new(name, false));
                }
                data.mons[1].gift = true;
                for (kind, x, y) in [("lantern", 12, 4), ("pond", 26, 10), ("rack", 30, 14), ("lamp", 8, 9), ("lamp", 20, 9), ("statue", 19, 15), ("campfire", 33, 16), ("pumpkin", 9, 7), ("bench", 21, 7), ("fountain", 14, 15)] {
                    data.placed.push(save::Placed { kind: kind.into(), x, y });
                }
                let now = save::now();
                for (i, x) in (10..17).enumerate() {
                    data.crops.insert(format!("{x}:12"), save::Crop { flavor: (i % 5) as u8, planted: now - i as i64 * 600, bonus: 0, watered_at: 0 });
                }
                data.x = 18;
                data.y = 10;
            }
            let mut g = Game::new(data);
            match scene {
                "dex" => {
                    g.mode = game::Mode::Dex(dex::Dex::new());
                }
                "prompts" => {
                    let _ = g.key(KeyCode::Char('p'));
                }
                "claude" => {
                    // Stand below Claude and say hello.
                    g.teleport(game::CLAUDE_HOME.0 + 1, game::CLAUDE_HOME.1, game::Dir::Left);
                    let _ = g.key(KeyCode::Char('e'));
                }
                "battle" | "team" | "evolve" => {
                    for (name, lvl) in [("Sonnetiger", 18), ("Ferrilet", 15), ("Puddlefin", 12)] {
                        let mut m = save::OwnedMon::new(name, false);
                        m.level = lvl;
                        m.xp = 400;
                        g.data.mons.push(m);
                    }
                    match scene {
                        "battle" => g.debug_battle(),
                        "team" => {
                            let _ = g.key(KeyCode::Char('t'));
                        }
                        _ => g.debug_evolve(),
                    }
                }
                "credits" => g.mode = game::Mode::Credits { t: 9.0 },
                "boss" => {
                    let _ = g.key(KeyCode::Char('`'));
                    for _ in 0..200 {
                        g.update(0.1);
                    }
                }
                "curio" => {
                    g.mode = game::Mode::Catalog(ui::Catalog::curio());
                }
                "catalog" => {
                    g.mode = game::Mode::Catalog(ui::Catalog::new());
                    let _ = g.key(KeyCode::Right);
                }
                "encounter" => {
                    let (sp, biome) = arg.split_once('@').unwrap_or((arg, "meadow"));
                    let sp = sp.parse().unwrap_or_else(|_| creatures::by_name(sp).unwrap_or(0));
                    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(1);
                    let e = encounter::Encounter::new(sp, false, creatures::SPECIES[sp].temper, 7, world::MapId::from_key(biome), &mut rng);
                    g.mode = game::Mode::Encounter(Box::new(e));
                }
                _ => {}
            }
            if std::env::var("CLAUDIMON_ZOOM").is_ok_and(|z| z == "in") {
                g.zoomed_out = false;
            }
            for _ in 0..30 {
                g.update(0.033);
            }
            g.draw(&mut screen);
        }
    }
    std::fs::write(format!("{out}.txt"), screen.to_text())?;
    std::fs::write(format!("{out}.cells"), screen.to_cells())?;
    std::fs::write(out, screen.to_ppm())
}
