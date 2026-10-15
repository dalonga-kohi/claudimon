//! Claudimon species data and sprite rendering.

use crate::gfx::{Rgb, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Elem {
    Leaf,
    Ember,
    Tide,
    Spark,
    Stone,
    Byte,
    Dream,
    Glitch,
}

impl Elem {
    pub fn name(self) -> &'static str {
        match self {
            Elem::Leaf => "Leaf",
            Elem::Ember => "Ember",
            Elem::Tide => "Tide",
            Elem::Spark => "Spark",
            Elem::Stone => "Stone",
            Elem::Byte => "Byte",
            Elem::Dream => "Dream",
            Elem::Glitch => "Glitch",
        }
    }
    pub fn color(self) -> Rgb {
        match self {
            Elem::Leaf => Rgb(110, 200, 90),
            Elem::Ember => Rgb(240, 110, 60),
            Elem::Tide => Rgb(80, 150, 240),
            Elem::Spark => Rgb(250, 210, 60),
            Elem::Stone => Rgb(170, 150, 120),
            Elem::Byte => Rgb(80, 220, 200),
            Elem::Dream => Rgb(190, 140, 240),
            Elem::Glitch => Rgb(240, 80, 200),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavor {
    Sweet,
    Spicy,
    Sour,
    Bitter,
    Fresh,
}

pub const FLAVORS: [Flavor; 5] = [Flavor::Sweet, Flavor::Spicy, Flavor::Sour, Flavor::Bitter, Flavor::Fresh];

impl Flavor {
    pub fn idx(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Sweet => "Sweet",
            Flavor::Spicy => "Spicy",
            Flavor::Sour => "Sour",
            Flavor::Bitter => "Bitter",
            Flavor::Fresh => "Fresh",
        }
    }
    pub fn berry(self) -> &'static str {
        match self {
            Flavor::Sweet => "Honeyberry",
            Flavor::Spicy => "Flarepepper",
            Flavor::Sour => "Zestlime",
            Flavor::Bitter => "Cocoabean",
            Flavor::Fresh => "Mintleaf",
        }
    }
    pub fn color(self) -> Rgb {
        match self {
            Flavor::Sweet => Rgb(240, 100, 150),
            Flavor::Spicy => Rgb(235, 70, 50),
            Flavor::Sour => Rgb(200, 230, 60),
            Flavor::Bitter => Rgb(150, 95, 60),
            Flavor::Fresh => Rgb(110, 230, 180),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Hungry,
    Playful,
    Shy,
    Sleepy,
    Curious,
    Grumpy,
}

pub const MOODS: [Mood; 6] = [Mood::Hungry, Mood::Playful, Mood::Shy, Mood::Sleepy, Mood::Curious, Mood::Grumpy];

impl Mood {
    pub fn name(self) -> &'static str {
        match self {
            Mood::Hungry => "Hungry",
            Mood::Playful => "Playful",
            Mood::Shy => "Shy",
            Mood::Sleepy => "Sleepy",
            Mood::Curious => "Curious",
            Mood::Grumpy => "Grumpy",
        }
    }
    pub fn emote(self) -> char {
        match self {
            Mood::Hungry => '♨',
            Mood::Playful => '♪',
            Mood::Shy => '…',
            Mood::Sleepy => 'z',
            Mood::Curious => '?',
            Mood::Grumpy => '#',
        }
    }
    /// The "prompt" the creature is giving you — a hint about what it wants.
    pub fn hint(self) -> &'static str {
        match self {
            Mood::Hungry => "keeps sniffing at your berry pouch.",
            Mood::Playful => "is bouncing around, full of energy!",
            Mood::Shy => "hides behind a tuft of grass.",
            Mood::Sleepy => "yawns and blinks slowly...",
            Mood::Curious => "tilts its head, studying you closely.",
            Mood::Grumpy => "huffs. It wants to be left alone.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Active {
    Day,
    Night,
    Any,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Spawn {
    Grass,
    TallGrass,
    /// Next to water.
    Shore,
    Sand,
    Cave,
}

/// Biome bits: which explorable maps a species lives in.
pub const MEADOW: u8 = 1;
pub const WOODS: u8 = 2;
pub const SHORE: u8 = 4;
pub const CAVE: u8 = 8;
pub const ISLE: u8 = 16;

/// Rarity of mythical creatures: never in the normal Claudidex count.
pub const MYTHICAL: u8 = 5;

pub struct Species {
    pub name: &'static str,
    pub types: &'static [Elem],
    pub fav: Flavor,
    pub hate: Flavor,
    pub rarity: u8, // 1 common .. 4 very rare
    pub active: Active,
    pub spawn: Spawn,
    pub biomes: u8,
    pub temper: Mood,
    pub glitch: bool,
    /// Evolves at (level, into species name).
    pub evolves: Option<(u32, &'static str)>,
    /// A detailed 16x16 sprite (legendaries only); drawn at the same on-screen size, with twice the detail.
    pub big: Option<&'static [&'static str; 16]>,
    pub dex: &'static str,
    pub sprite: [&'static str; 8],
    pub pal: &'static [(char, Rgb)],
}

const EYE: Rgb = Rgb(40, 32, 52);

pub static SPECIES: &[Species] = &[
    Species {
        name: "Haikitten",
        types: &[Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 1,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW,
        temper: Mood::Sleepy,
        glitch: false,
        evolves: Some((16, "Sonnetiger")),
        big: None,
        dex: "Speaks only in 5-7-5. Naps in sunbeams and dreams in short, perfect verses.",
        sprite: [".p....p.", ".ap..pa.", ".aaaaaa.", "aaeaaeaa", ".naaaan.", "..aaaa.t", ".aaaaaat", ".a.aa.a."],
        pal: &[('p', Rgb(196, 150, 236)), ('a', Rgb(250, 232, 206)), ('e', EYE), ('n', Rgb(250, 160, 170)), ('t', Rgb(196, 150, 236))],
    },
    Species {
        name: "Tokenling",
        types: &[Elem::Byte],
        fav: Flavor::Sour,
        hate: Flavor::Spicy,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: MEADOW | SHORE,
        temper: Mood::Curious,
        glitch: false,
        evolves: Some((18, "Tokenizard")),
        big: None,
        dex: "Splits everything it finds into tiny pieces. Roughly four of them make one word.",
        sprite: ["..yyyy..", ".yhyyyo.", "yyeyyeyo", "yyyyyyyo", "yyymmyyo", ".yyyyyo.", "..oooo..", ".f....f."],
        pal: &[('y', Rgb(240, 200, 70)), ('h', Rgb(255, 240, 170)), ('o', Rgb(190, 140, 40)), ('e', EYE), ('m', Rgb(150, 70, 50)), ('f', Rgb(120, 90, 60))],
    },
    Species {
        name: "Gitcko",
        types: &[Elem::Leaf],
        fav: Flavor::Fresh,
        hate: Flavor::Sour,
        rarity: 2,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW | WOODS,
        temper: Mood::Curious,
        glitch: false,
        evolves: Some((20, "Rebasilisk")),
        big: None,
        dex: "Its tail forks into branches. When startled, it stashes itself and runs.",
        sprite: [".GG..GG.", "GEGggGEG", ".gggggg.", ".gmmmmg.", "g.gllg.g", "..gllg..", ".gg..ggb", "......bb"],
        pal: &[('G', Rgb(130, 210, 100)), ('g', Rgb(80, 170, 70)), ('E', EYE), ('m', Rgb(40, 90, 40)), ('l', Rgb(205, 235, 150)), ('b', Rgb(140, 100, 60))],
    },
    Species {
        name: "Cachew",
        types: &[Elem::Leaf],
        fav: Flavor::Bitter,
        hate: Flavor::Spicy,
        rarity: 1,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW | WOODS,
        temper: Mood::Hungry,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Hides nuts everywhere so it can find them faster next time. Usually forgets where.",
        sprite: [".tt..r.r", "tTTt.rrr", "tTt.rrer", "tTt.rrrn", ".tTrrcc.", "..trrcc.", "...rrrr.", "...r..r."],
        pal: &[('t', Rgb(180, 105, 55)), ('T', Rgb(225, 160, 100)), ('r', Rgb(195, 125, 70)), ('e', EYE), ('n', Rgb(60, 35, 30)), ('c', Rgb(245, 215, 165))],
    },
    Species {
        name: "Bugbyte",
        types: &[Elem::Byte, Elem::Leaf],
        fav: Flavor::Sweet,
        hate: Flavor::Fresh,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::TallGrass,
        biomes: MEADOW | WOODS,
        temper: Mood::Playful,
        glitch: false,
        evolves: Some((14, "Featurefly")),
        big: None,
        dex: "Crawls into the cracks of anything ever written. Swears it is a feature.",
        sprite: [".a....a.", "..a..a..", "..kkkk..", ".kEkkEk.", "lgGgcgGl", ".ggcgcg.", "lgcggGgl", ".l.gg.l."],
        pal: &[('a', Rgb(90, 90, 100)), ('k', Rgb(50, 60, 72)), ('E', Rgb(120, 255, 160)), ('g', Rgb(60, 170, 110)), ('G', Rgb(130, 225, 160)), ('c', Rgb(240, 220, 90)), ('l', Rgb(40, 40, 52))],
    },
    Species {
        name: "Lambdeer",
        types: &[Elem::Leaf, Elem::Dream],
        fav: Flavor::Fresh,
        hate: Flavor::Spicy,
        rarity: 3,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW | WOODS,
        temper: Mood::Shy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Its antlers fold into perfect λ shapes. Pure of function, free of side effects.",
        sprite: ["b.b..b.b", ".bb..bb.", ".dddddd.", "ddEddEdd", ".dddddd.", "..dwwd..", "..dnnd..", "...dd..."],
        pal: &[('b', Rgb(245, 235, 205)), ('d', Rgb(205, 150, 100)), ('E', EYE), ('w', Rgb(250, 240, 222)), ('n', Rgb(70, 45, 45))],
    },
    Species {
        name: "Promptpup",
        types: &[Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Sour,
        rarity: 1,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW,
        temper: Mood::Playful,
        glitch: false,
        evolves: Some((18, "Instructhound")),
        big: None,
        dex: "Does exactly what you ask. Be specific, or it will fetch the entire forest.",
        sprite: ["........", "ee....ee", "eaaaaaae", "eaEaaEae", ".aaaaaa.", "..anna..", ".aaaaaa.", ".a.aa.a."],
        pal: &[('e', Rgb(170, 110, 60)), ('a', Rgb(240, 200, 120)), ('E', EYE), ('n', Rgb(45, 32, 32))],
    },
    Species {
        name: "Snailoop",
        types: &[Elem::Leaf],
        fav: Flavor::Fresh,
        hate: Flavor::Spicy,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::TallGrass,
        biomes: MEADOW | WOODS,
        temper: Mood::Sleepy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Has been circling the same garden since boot. It still hasn't found the exit condition.",
        sprite: ["......ee", "..ssss.b", ".sSSSSsb", "sSsssSsb", "sSsSsSsb", "sSsssSbb", ".sSSSsbb", "bbbbbbbb"],
        pal: &[('s', Rgb(170, 90, 140)), ('S', Rgb(235, 155, 195)), ('b', Rgb(185, 205, 150)), ('e', EYE)],
    },
    Species {
        name: "Ampster",
        types: &[Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 2,
        active: Active::Night,
        spawn: Spawn::Grass,
        biomes: MEADOW,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Runs on its wheel all night. A single Ampster can power a small build server.",
        sprite: [".o.oo.o.", ".oooooo.", "oEooooEo", "pwwnnwwp", "owwwwwwo", ".owwwwo.", "..oyyo..", ".f....f."],
        pal: &[('o', Rgb(230, 170, 90)), ('w', Rgb(250, 240, 222)), ('E', EYE), ('n', Rgb(240, 130, 140)), ('p', Rgb(250, 160, 160)), ('y', Rgb(255, 230, 60)), ('f', Rgb(200, 140, 90))],
    },
    Species {
        name: "Asyncrow",
        types: &[Elem::Dream],
        fav: Flavor::Bitter,
        hate: Flavor::Sweet,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: MEADOW | WOODS,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Promises to arrive later, and always does. Nobody knows in what order.",
        sprite: ["........", "....kkk.", "...kkEky", ".kkkkkk.", "kkpkkkk.", "kpppkkk.", "..kkkk..", "...y.y.."],
        pal: &[('k', Rgb(44, 44, 66)), ('E', Rgb(250, 220, 90)), ('y', Rgb(240, 180, 60)), ('p', Rgb(110, 95, 165))],
    },
    Species {
        name: "Nullowl",
        types: &[Elem::Dream],
        fav: Flavor::Bitter,
        hate: Flavor::Sour,
        rarity: 2,
        active: Active::Night,
        spawn: Spawn::Grass,
        biomes: MEADOW | WOODS,
        temper: Mood::Shy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Stares into the void at night. Sometimes, the void returns None.",
        sprite: ["o......o", "oooooooo", "oWWooWWo", "oWEooEWo", "oooyyooo", ".occcco.", ".occcco.", "..y..y.."],
        pal: &[('o', Rgb(115, 105, 145)), ('W', Rgb(232, 232, 242)), ('E', Rgb(20, 20, 30)), ('y', Rgb(240, 190, 80)), ('c', Rgb(175, 165, 195))],
    },
    Species {
        name: "Hallucinoth",
        types: &[Elem::Dream, Elem::Glitch],
        fav: Flavor::Sweet,
        hate: Flavor::Fresh,
        rarity: 3,
        active: Active::Night,
        spawn: Spawn::Grass,
        biomes: WOODS,
        temper: Mood::Curious,
        glitch: true,
        evolves: None,
        big: None,
        dex: "Its wing patterns show things that are not there. It is very confident about them.",
        sprite: [".a....a.", "..a..a..", "WW.bb.WW", "WeWbbWeW", "WWWbbWWW", ".WWbbWW.", "..W.b.W.", "....b..."],
        pal: &[('a', Rgb(205, 195, 235)), ('W', Rgb(180, 140, 230)), ('e', Rgb(250, 110, 200)), ('b', Rgb(90, 70, 112))],
    },
    Species {
        name: "Rubberduck",
        types: &[Elem::Tide],
        fav: Flavor::Fresh,
        hate: Flavor::Spicy,
        rarity: 2,
        active: Active::Day,
        spawn: Spawn::Shore,
        biomes: MEADOW | SHORE,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Listens patiently as you explain your problem. By the end, you have solved it yourself.",
        sprite: ["...yyy..", "..yyyEy.", "..yyyyoo", "y..yyy..", "yyyyyyy.", "yYYyyyyy", ".yyyyyy.", ".wwwwww."],
        pal: &[('y', Rgb(250, 220, 60)), ('Y', Rgb(255, 245, 150)), ('E', EYE), ('o', Rgb(240, 140, 40)), ('w', Rgb(130, 195, 240))],
    },
    Species {
        name: "Puddlefin",
        types: &[Elem::Tide],
        fav: Flavor::Fresh,
        hate: Flavor::Bitter,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::Shore,
        biomes: MEADOW | SHORE | CAVE,
        temper: Mood::Shy,
        glitch: false,
        evolves: Some((18, "Torrentuna")),
        big: None,
        dex: "A walking droplet. Leaves tiny puddles wherever it has been happy.",
        sprite: ["...bb...", "..bbbb..", ".hhbbbb.", "bbEbbEbb", "bbbbbbbb", "bbbmmbbf", ".bbbbbff", "..bbbb.."],
        pal: &[('b', Rgb(80, 150, 230)), ('h', Rgb(185, 225, 255)), ('E', EYE), ('m', Rgb(40, 60, 120)), ('f', Rgb(60, 110, 200))],
    },
    Species {
        name: "Cloudle",
        types: &[Elem::Spark, Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 4,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: MEADOW | SHORE,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "A fluffy poodle made of cloud. Lives somewhere, everywhere, and nowhere at once.",
        sprite: [".cc..cc.", "cCCccCCc", "cCEccECc", "cpccccpc", "cccccccc", ".cccccc.", "..y..y..", ".y....y."],
        pal: &[('c', Rgb(232, 238, 252)), ('C', Rgb(255, 255, 255)), ('E', Rgb(60, 70, 110)), ('p', Rgb(250, 180, 200)), ('y', Rgb(255, 220, 80))],
    },
    // ------------------------------------------------------------ Latent Woods
    Species {
        name: "Foxception",
        types: &[Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Sour,
        rarity: 3,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: WOODS,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "A fox with a tail, inside a tail, inside a tail. Nobody has ever found the bottom.",
        sprite: ["o.....o.", "oo...oo.", "ooooooo.", "oEoooEo.", "wwoooww.", ".wwnww.t", "..www.tT", "...w.tTt"],
        pal: &[('o', Rgb(230, 120, 50)), ('E', EYE), ('w', Rgb(250, 240, 228)), ('n', Rgb(40, 28, 28)), ('t', Rgb(230, 120, 50)), ('T', Rgb(250, 240, 228))],
    },
    Species {
        name: "Daemonkey",
        types: &[Elem::Glitch, Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Fresh,
        rarity: 2,
        active: Active::Night,
        spawn: Spawn::Grass,
        biomes: WOODS,
        temper: Mood::Playful,
        glitch: true,
        evolves: None,
        big: None,
        dex: "Runs silently in the background. You only notice it when something goes missing.",
        sprite: ["..bbbb..", ".bbbbbb.", "ebbffbbe", "ebEffEbe", ".bffffb.", ".bfmmfb.", "..bbbb.t", ".b.bb.tt"],
        pal: &[('b', Rgb(110, 70, 140)), ('e', Rgb(145, 95, 175)), ('f', Rgb(232, 200, 170)), ('E', Rgb(255, 60, 120)), ('m', Rgb(120, 60, 60)), ('t', Rgb(110, 70, 140))],
    },
    Species {
        name: "Recursnake",
        types: &[Elem::Byte],
        fav: Flavor::Bitter,
        hate: Flavor::Sweet,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::TallGrass,
        biomes: WOODS,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Bites its own tail to remember where it started. Stack overflows are a known side effect.",
        sprite: ["..ggggh.", ".gG..hEh", "gG....hh", "gG....Gg", "gG....Gg", ".gG..Gg.", "..gGGg..", "...gg..."],
        pal: &[('g', Rgb(60, 170, 150)), ('G', Rgb(125, 222, 192)), ('h', Rgb(40, 140, 120)), ('E', Rgb(250, 240, 120))],
    },
    Species {
        name: "Lintworm",
        types: &[Elem::Leaf],
        fav: Flavor::Fresh,
        hate: Flavor::Spicy,
        rarity: 1,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: WOODS,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Points out every tiny flaw in the soil. Plants grow straighter wherever it lives.",
        sprite: ["........", "..pppp..", ".kWkkWk.", ".pppppp.", "..pmmp..", "...ppp..", ".ppp.pp.", "pp....pp"],
        pal: &[('p', Rgb(232, 145, 155)), ('k', Rgb(40, 40, 52)), ('W', Rgb(200, 232, 255)), ('m', Rgb(150, 60, 72))],
    },
    Species {
        name: "Kernelk",
        types: &[Elem::Stone],
        fav: Flavor::Bitter,
        hate: Flavor::Sweet,
        rarity: 3,
        active: Active::Day,
        spawn: Spawn::Grass,
        biomes: WOODS,
        temper: Mood::Shy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Sits at the very core of the forest. Everything else runs on top of it.",
        sprite: ["a.a..a.a", "aaa..aaa", "..bbbb..", ".bEbbEb.", ".bbbbbb.", "..bnnb..", "..bbbb..", "...bb..."],
        pal: &[('a', Rgb(225, 205, 165)), ('b', Rgb(122, 86, 60)), ('E', EYE), ('n', Rgb(60, 40, 30))],
    },
    // ------------------------------------------------------------- Token Shore
    Species {
        name: "Pingguin",
        types: &[Elem::Tide],
        fav: Flavor::Fresh,
        hate: Flavor::Spicy,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::Sand,
        biomes: SHORE,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Sends a little chirp to every Pingguin nearby, then waits patiently for a reply.",
        sprite: ["..kkkk..", ".kWkkWk.", ".kEwwEk.", ".kwyywk.", "kkwwwwkk", "k.wwww.k", "..wwww..", "..y..y.."],
        pal: &[('k', Rgb(40, 44, 62)), ('W', Rgb(240, 240, 250)), ('E', Rgb(20, 20, 30)), ('w', Rgb(240, 240, 250)), ('y', Rgb(245, 165, 50))],
    },
    Species {
        name: "Contextopus",
        types: &[Elem::Tide, Elem::Byte],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 4,
        active: Active::Any,
        spawn: Spawn::Shore,
        biomes: SHORE,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Holds two hundred thousand things at once, one in each sucker. Forgets nothing — until it does.",
        sprite: ["..pppp..", ".pPPPPp.", "pPPPPPPp", "pPEPPEPp", "pPPPPPPp", ".pcpcpp.", "pp.pp.pp", "p.p..p.p"],
        pal: &[('p', Rgb(200, 90, 170)), ('P', Rgb(236, 132, 202)), ('E', Rgb(30, 30, 42)), ('c', Rgb(100, 240, 230))],
    },
    Species {
        name: "Ferrilet",
        types: &[Elem::Ember, Elem::Stone],
        fav: Flavor::Spicy,
        hate: Flavor::Fresh,
        rarity: 2,
        active: Active::Day,
        spawn: Spawn::Sand,
        biomes: SHORE,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: Some((22, "Ferrisaur")),
        big: None,
        dex: "Fiercely guards its shell and refuses any treat it can't prove is safe. Fearless, and never leaks.",
        sprite: ["cc....cc", "cCc..cCc", ".c.kk.c.", "..oooo..", ".oOooOo.", "oooooooo", ".o.oo.o.", "o.o..o.o"],
        pal: &[('c', Rgb(222, 92, 52)), ('C', Rgb(250, 145, 95)), ('k', Rgb(30, 28, 30)), ('o', Rgb(232, 104, 54)), ('O', Rgb(250, 165, 115))],
    },
    Species {
        name: "Heapotamus",
        types: &[Elem::Tide, Elem::Stone],
        fav: Flavor::Sweet,
        hate: Flavor::Spicy,
        rarity: 2,
        active: Active::Day,
        spawn: Spawn::Shore,
        biomes: SHORE,
        temper: Mood::Sleepy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Grows to fill whatever space it is given, then politely asks for more.",
        sprite: [".e....e.", ".hhhhhh.", "hhEhhEhh", "hhhhhhhh", "hHHHHHHh", "HnHHHHnH", "HHHHHHHH", ".HH..HH."],
        pal: &[('e', Rgb(120, 110, 150)), ('h', Rgb(140, 130, 170)), ('E', EYE), ('H', Rgb(182, 172, 202)), ('n', Rgb(80, 70, 100))],
    },
    Species {
        name: "Segfaultle",
        types: &[Elem::Glitch, Elem::Stone],
        fav: Flavor::Bitter,
        hate: Flavor::Sour,
        rarity: 3,
        active: Active::Any,
        spawn: Spawn::Sand,
        biomes: SHORE,
        temper: Mood::Shy,
        glitch: true,
        evolves: None,
        big: None,
        dex: "Wanders into places it was never meant to reach. The crack in its shell is shaped like an address.",
        sprite: ["........", "........", "..ssss..", ".sSxSSs.", "sSSSxSsg", "ssssssgE", ".gg..gg.", "........"],
        pal: &[('s', Rgb(60, 120, 90)), ('S', Rgb(100, 162, 122)), ('x', Rgb(240, 80, 200)), ('g', Rgb(152, 202, 142)), ('E', EYE)],
    },
    // ---------------------------------------------------------- Kernel Caverns
    Species {
        name: "Mutox",
        types: &[Elem::Stone],
        fav: Flavor::Bitter,
        hate: Flavor::Sweet,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Two Mutox can never graze the same patch at once. They take turns, very politely.",
        sprite: ["h......h", "hh....hh", ".hbbbbh.", "bbEbbEbb", ".bbbbbb.", ".bmmmmb.", ".bmnnmb.", "..bbbb.."],
        pal: &[('h', Rgb(232, 222, 192)), ('b', Rgb(104, 92, 82)), ('E', EYE), ('m', Rgb(172, 152, 132)), ('n', Rgb(60, 50, 40))],
    },
    Species {
        name: "Firewallaby",
        types: &[Elem::Ember],
        fav: Flavor::Spicy,
        hate: Flavor::Fresh,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Blocks anything suspicious from entering its burrow. Snacks get through after inspection.",
        sprite: [".r....r.", ".rr..rr.", "..rrrr..", ".rErrEr.", "..rnnr..", ".rcccr.f", "..ccc.fF", ".r..r.fY"],
        pal: &[('r', Rgb(212, 122, 72)), ('c', Rgb(242, 202, 152)), ('E', EYE), ('n', Rgb(60, 30, 30)), ('f', Rgb(240, 90, 40)), ('F', Rgb(255, 170, 60)), ('Y', Rgb(255, 230, 120))],
    },
    Species {
        name: "Cachebat",
        types: &[Elem::Byte, Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Shy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Hangs upside down, remembering the last thing it saw. Ask it again and it answers instantly.",
        sprite: ["..e..e..", "..ebbe..", "wbEbbEbw", "wwbbbbww", "wwwbbwww", "w.wbbw.w", "...bb...", "........"],
        pal: &[('e', Rgb(92, 72, 112)), ('b', Rgb(72, 62, 92)), ('E', Rgb(120, 255, 200)), ('w', Rgb(102, 82, 132))],
    },
    Species {
        name: "Stacktite",
        types: &[Elem::Stone],
        fav: Flavor::Bitter,
        hate: Flavor::Fresh,
        rarity: 1,
        active: Active::Any,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Sleepy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Grows one layer at a time. Pop the top off and it simply grows another.",
        sprite: ["...ss...", "...SS...", "..ssss..", "..SSSS..", ".sEssEs.", ".SSSSSS.", "ssssssss", "SSSSSSSS"],
        pal: &[('s', Rgb(132, 122, 142)), ('S', Rgb(172, 162, 182)), ('E', EYE)],
    },
    Species {
        name: "Malloccoon",
        types: &[Elem::Byte],
        fav: Flavor::Sweet,
        hate: Flavor::Sour,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Hungry,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Grabs everything it can carry and never gives any of it back. Its den is famously cluttered.",
        sprite: [".g....g.", ".gg..gg.", ".gggggg.", "gkWkkWkg", ".gwwwwg.", "..wnnw..", ".gggggg.", ".g.gg.gk"],
        pal: &[('g', Rgb(142, 142, 152)), ('k', Rgb(40, 40, 48)), ('W', Rgb(242, 242, 250)), ('w', Rgb(222, 222, 228)), ('n', Rgb(30, 30, 30))],
    },
    Species {
        name: "Panicorn",
        types: &[Elem::Glitch, Elem::Dream],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 4,
        active: Active::Night,
        spawn: Spawn::Cave,
        biomes: CAVE,
        temper: Mood::Shy,
        glitch: true,
        evolves: None,
        big: None,
        dex: "When startled it unwinds completely, then calmly starts over somewhere safe.",
        sprite: ["....y...", "...yy...", "..wwwwm.", ".wEwwEwm", ".wwwwwwm", "..wpwpw.", "..wwww.m", "...ww..."],
        pal: &[('y', Rgb(255, 222, 100)), ('w', Rgb(245, 240, 255)), ('m', Rgb(240, 120, 222)), ('E', Rgb(60, 40, 92)), ('p', Rgb(222, 172, 202))],
    },
    // ------------------------------------------------------- evolved forms
    Species {
        name: "Sonnetiger",
        types: &[Elem::Dream, Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Playful,
        glitch: false,
        evolves: Some((32, "Opuslion")),
        big: None,
        dex: "Its stripes rhyme. Fourteen of them, arranged in perfect iambic pentameter.",
        sprite: ["o......o", "oo.ss.oo", "osooooso", "osEooEso", "owwoowwo", ".wwnnww.", "..wwww..", "...ss..."],
        pal: &[('o', Rgb(240, 150, 70)), ('s', Rgb(96, 52, 104)), ('E', Rgb(200, 160, 255)), ('w', Rgb(250, 240, 230)), ('n', Rgb(240, 130, 150))],
    },
    Species {
        name: "Opuslion",
        types: &[Elem::Dream, Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Bitter,
        rarity: 4,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "A magnificent lion whose mane glows like a warm sunrise. Thinks deeply before every roar.",
        sprite: ["m.mmmm.m", "mmmmmmmm", "mmfWWfmm", "mfEffEfm", "mffffffm", "mmfnnfmm", ".mmffmm.", "..mmmm.."],
        pal: &[('m', Rgb(218, 119, 86)), ('f', Rgb(250, 222, 160)), ('W', Rgb(255, 250, 220)), ('E', Rgb(90, 50, 140)), ('n', Rgb(120, 60, 60))],
    },
    Species {
        name: "Tokenizard",
        types: &[Elem::Byte, Elem::Ember],
        fav: Flavor::Sour,
        hate: Flavor::Spicy,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Breathes out streams of tiny glowing tokens. Gets cranky when it runs out of budget.",
        sprite: ["..yyyy..", ".yEyyEy.", ".yyyyyy.", "..ymmy..", ".yyyyyyf", "yhyyyyfF", ".yyyyy.f", ".y...y.."],
        pal: &[('y', Rgb(240, 190, 60)), ('h', Rgb(255, 240, 170)), ('E', EYE), ('m', Rgb(150, 70, 50)), ('f', Rgb(240, 90, 40)), ('F', Rgb(255, 190, 70))],
    },
    Species {
        name: "Rebasilisk",
        types: &[Elem::Leaf, Elem::Byte],
        fav: Flavor::Fresh,
        hate: Flavor::Sour,
        rarity: 3,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Its gaze rewrites history. Old mistakes vanish; nobody can quite remember them happening.",
        sprite: ["c..c..c.", ".cgggc..", ".gEggEg.", ".gggggg.", "..gmmg..", ".ggggggg", "gggg..gg", "gg......"],
        pal: &[('c', Rgb(90, 230, 210)), ('g', Rgb(70, 160, 90)), ('E', Rgb(250, 220, 90)), ('m', Rgb(30, 70, 40))],
    },
    Species {
        name: "Featurefly",
        types: &[Elem::Byte, Elem::Leaf],
        fav: Flavor::Sweet,
        hate: Flavor::Fresh,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "What everyone thought was a bug turned out to be this. Its wings shimmer with undocumented colours.",
        sprite: [".a....a.", "..a..a..", "WW.kk.WW", "WcWkkWcW", "WWWkkWWW", ".wwkkww.", "..w.k.w.", "........"],
        pal: &[('a', Rgb(90, 90, 100)), ('W', Rgb(130, 230, 170)), ('c', Rgb(250, 230, 90)), ('w', Rgb(90, 190, 140)), ('k', Rgb(40, 60, 72))],
    },
    Species {
        name: "Torrentuna",
        types: &[Elem::Tide],
        fav: Flavor::Fresh,
        hate: Flavor::Bitter,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Shore,
        biomes: 0,
        temper: Mood::Playful,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Shares everything it has with every fish around it, in tiny pieces, all at once.",
        sprite: ["...ff...", "..bbbb..", ".bbbbbbf", "bEbbbbbF", "bbhhhbbf", ".bhhhbbf", "..bbbb..", "...ff..."],
        pal: &[('b', Rgb(60, 110, 200)), ('h', Rgb(200, 225, 250)), ('E', EYE), ('f', Rgb(40, 80, 170)), ('F', Rgb(110, 160, 235))],
    },
    Species {
        name: "Instructhound",
        types: &[Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Sour,
        rarity: 2,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Follows instructions to the letter, and helpfully asks a clarifying question when they're ambiguous.",
        sprite: ["e......e", "eeaaaaee", "eaEaaEae", ".aaaaaa.", "..anna..", ".yaaaay.", ".aaaaaa.", ".aa..aa."],
        pal: &[('e', Rgb(150, 90, 50)), ('a', Rgb(230, 180, 100)), ('E', EYE), ('n', Rgb(45, 32, 32)), ('y', Rgb(255, 220, 60))],
    },
    Species {
        name: "Ferrisaur",
        types: &[Elem::Ember, Elem::Stone],
        fav: Flavor::Spicy,
        hate: Flavor::Fresh,
        rarity: 3,
        active: Active::Any,
        spawn: Spawn::Sand,
        biomes: 0,
        temper: Mood::Grumpy,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Fearless and memory-safe. Nothing gets past its shell unless it has proven it belongs there.",
        sprite: ["CC....CC", "Cc.oo.cC", ".ckookc.", "oooooooo", "oOooooOo", "oooooooo", "o.oooo.o", "oo.oo.oo"],
        pal: &[('C', Rgb(250, 150, 100)), ('c', Rgb(222, 92, 52)), ('o', Rgb(200, 70, 40)), ('O', Rgb(250, 190, 120)), ('k', Rgb(30, 28, 30))],
    },
    // ---------------------------------------------------------- Mythos Isle
    Species {
        name: "Thinkerbell",
        types: &[Elem::Dream, Elem::Spark],
        fav: Flavor::Sweet,
        hate: Flavor::Sour,
        rarity: MYTHICAL,
        active: Active::Any,
        spawn: Spawn::Grass,
        biomes: ISLE,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: None,
        dex: "Appears when someone thinks very, very hard. Leaves a trail of half-finished thoughts that glow for hours.",
        sprite: ["..yyyy..", ".yffffy.", "WyfEfEyW", "WWffffWW", "WwWddWwW", ".W.dd.W.", "...dd...", "..d..d.."],
        pal: &[('y', Rgb(255, 222, 120)), ('f', Rgb(252, 224, 200)), ('E', Rgb(60, 40, 100)), ('W', Rgb(190, 230, 255)), ('w', Rgb(232, 246, 255)), ('d', Rgb(190, 140, 240))],
    },
    Species {
        name: "Mythos",
        types: &[Elem::Dream, Elem::Glitch],
        fav: Flavor::Sweet,
        hate: Flavor::Spicy,
        rarity: MYTHICAL,
        active: Active::Night,
        spawn: Spawn::Grass,
        biomes: 0,
        temper: Mood::Curious,
        glitch: false,
        evolves: None,
        big: Some(&[
            ".......ss.......",
            ".....s.ss.s.....",
            "....h.sSSs.h....",
            "..ww.hoOOoh.ww..",
            ".wWWwoEooEowWWw.",
            "wWwWwoooooowWwWw",
            "wWWWWwoccowWWWWw",
            ".wWWWwoccowWWWw.",
            "..wWw.occo.wWw..",
            "...w..occo..w...",
            "....ooOccOoo....",
            "...oOccccccOo...",
            "..oOccddddccOo..",
            "..oOcdOOOOdcOo..",
            "...oOOooooOOo...",
            "....oooooooodds.",
        ]),
        dex: "The dragon who dreamed the first story. Every Claudimon, they say, began as a thought in its sleep. Its crest shines like a thousand-pointed star.",
        sprite: ["...ss...", ".s.SS.s.", "ww.oo.ww", "wWEooEWw", ".woccow.", "..occo..", ".oOccOo.", "..oooodd"],
        pal: &[
            ('s', Rgb(255, 238, 190)),
            ('S', Rgb(255, 255, 255)),
            ('o', Rgb(218, 119, 86)),
            ('O', Rgb(245, 172, 132)),
            ('E', Rgb(110, 210, 255)),
            ('c', Rgb(250, 236, 214)),
            ('d', Rgb(176, 90, 62)),
            ('w', Rgb(255, 250, 236)),
            ('W', Rgb(255, 214, 150)),
            ('h', Rgb(255, 236, 200)),
        ],
    },
];

pub fn by_name(name: &str) -> Option<usize> {
    SPECIES.iter().position(|s| s.name == name)
}

/// "Constitutional" shiny variant: rotate the colour channels for a distinct palette.
fn shiny_shift(c: Rgb) -> Rgb {
    Rgb(c.2, c.0, c.1)
}

/// Draw a species sprite at pixel (x, y) with integer scale, optional flip and a dark outline.
pub fn draw(s: &mut Screen, idx: usize, x: i32, y: i32, scale: i32, flip: bool, shiny: bool, t: f32) {
    let sp = &SPECIES[idx];
    // Big sprites keep (at least) the same footprint at scale >= 2, with twice the detail; in the
    // overworld (scale 1) they're drawn full size, rising above their tile.
    let (rows, n): (&[&str], i32) = match sp.big {
        Some(b) => (&b[..], 16),
        None => (&sp.sprite[..], 8),
    };
    let (cell, ox, oy) = match (n, scale) {
        (16, 1) => (1, -4, -8),
        // Never smaller than a normal creature at this scale: round up, keep it centred and grounded.
        (16, _) => {
            let cell = (scale + 1) / 2;
            let extra = 16 * cell - 8 * scale;
            (cell, -extra / 2, -extra)
        }
        _ => (scale, 0, 0),
    };
    let (x, y) = (x + ox, y + oy);
    let color_at = |cx: i32, cy: i32| -> Option<Rgb> {
        if !(0..n).contains(&cx) || !(0..n).contains(&cy) {
            return None;
        }
        let row = rows[cy as usize].as_bytes();
        let sx = if flip { n - 1 - cx } else { cx };
        let ch = row[sx as usize] as char;
        let c = sp.pal.iter().find(|(k, _)| *k == ch).map(|(_, c)| *c)?;
        Some(if shiny { shiny_shift(c) } else { c })
    };
    // Outline: any transparent pixel next to a filled one.
    let outline = Rgb(28, 22, 34);
    for cy in -1..=n {
        for cx in -1..=n {
            if color_at(cx, cy).is_some() {
                continue;
            }
            let near = [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|(dx, dy)| color_at(cx + dx, cy + dy).is_some());
            if near {
                for yy in 0..cell {
                    for xx in 0..cell {
                        s.blend(x + cx * cell + xx, y + cy * cell + yy, outline, 0.75);
                    }
                }
            }
        }
    }
    for cy in 0..n {
        // Glitch-type creatures tear horizontally now and then.
        let tear = if sp.glitch && ((t * 7.0 + cy as f32 * 1.7).sin() > 0.93) { cell } else { 0 };
        for cx in 0..n {
            if let Some(mut c) = color_at(cx, cy) {
                // Mythos's crest and wings shimmer.
                if sp.big.is_some() && matches!(rows[cy as usize].as_bytes()[(if flip { n - 1 - cx } else { cx }) as usize], b's' | b'S' | b'W') {
                    c = c.lerp(Rgb(255, 255, 255), ((t * 3.0 + (cx + cy) as f32 * 0.4).sin() * 0.5 + 0.5) * 0.5);
                }
                for yy in 0..cell {
                    for xx in 0..cell {
                        s.set(x + cx * cell + xx + tear, y + cy * cell + yy, c);
                    }
                }
            }
        }
    }
    if sp.big.is_some() {
        // A trail of drifting letters from the tail.
        let glyph_px = cell.max(1);
        for i in 0..6 {
            let k = (t * 0.6 + i as f32 / 6.0).fract();
            let lx = x + (13 + (k * 6.0) as i32) * cell + ((t * 2.0 + i as f32).sin() * 2.0) as i32;
            let ly = y + (15 - (k * 10.0) as i32) * cell;
            let c = Rgb(255, 230, 170).lerp(Rgb(120, 200, 255), k);
            s.fill_rect(lx, ly, glyph_px, glyph_px, c);
        }
    }
    if shiny {
        // Orbiting sparkles.
        for i in 0..3 {
            let a = t * 2.0 + i as f32 * 2.1;
            let r = 5.0 * scale as f32;
            let sx = x + 4 * scale + (a.cos() * r) as i32;
            let sy = y + 4 * scale + (a.sin() * r * 0.8) as i32;
            s.set(sx, sy, Rgb(255, 250, 200));
        }
    }
}

/// Draw a dark silhouette (for undiscovered Claudidex entries).
pub fn draw_silhouette(s: &mut Screen, idx: usize, x: i32, y: i32, scale: i32) {
    let sp = &SPECIES[idx];
    for cy in 0..8 {
        for (cx, ch) in sp.sprite[cy].chars().enumerate() {
            if ch != '.' {
                s.fill_rect(x + cx as i32 * scale, y + cy as i32 * scale, scale, scale, Rgb(46, 40, 58));
            }
        }
    }
}

// ------------------------------------------------------------------ stats

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub hp: u32,
    pub atk: u32,
    pub def: u32,
    pub spd: u32,
}

/// How many evolutions led to this species (0 for base forms).
pub fn stage(idx: usize) -> u32 {
    let name = SPECIES[idx].name;
    match SPECIES.iter().position(|s| s.evolves.is_some_and(|(_, into)| into == name)) {
        Some(prev) => 1 + stage(prev),
        None => 0,
    }
}

/// Base stats, derived from rarity, evolution stage and type (no hand-tuned table to drift).
pub fn base_stats(idx: usize) -> Stats {
    let sp = &SPECIES[idx];
    let mythic = if sp.rarity >= MYTHICAL { 120.0 } else { 0.0 };
    let total = 240.0 + sp.rarity.min(4) as f32 * 28.0 + stage(idx) as f32 * 70.0 + mythic;
    // Weights for (hp, atk, def, spd) by primary type.
    let w = match sp.types[0] {
        Elem::Leaf => [1.2, 0.9, 1.1, 0.8],
        Elem::Ember => [0.9, 1.3, 0.8, 1.0],
        Elem::Tide => [1.3, 0.9, 1.0, 0.8],
        Elem::Spark => [0.8, 1.0, 0.8, 1.4],
        Elem::Stone => [1.0, 1.0, 1.4, 0.6],
        Elem::Byte => [0.9, 1.1, 0.9, 1.1],
        Elem::Dream => [1.1, 1.0, 0.9, 1.0],
        Elem::Glitch => [0.8, 1.3, 0.7, 1.2],
    };
    // A little per-species personality so two Leaf types aren't identical.
    let h = sp.name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let jitter = |i: u32| 0.9 + ((h >> (i * 4)) % 8) as f32 * 0.025;
    let sum: f32 = (0..4).map(|i| w[i] * jitter(i as u32)).sum();
    let v = |i: usize| (total * w[i] * jitter(i as u32) / sum).round() as u32;
    Stats { hp: v(0), atk: v(1), def: v(2), spd: v(3) }
}

pub fn stats_at(idx: usize, level: u32) -> Stats {
    let b = base_stats(idx);
    let s = |base: u32| base * level / 50 + 5;
    Stats { hp: b.hp * 2 * level / 50 + level + 10, atk: s(b.atk), def: s(b.def), spd: s(b.spd) }
}

pub fn xp_to_next(level: u32) -> u32 {
    3 * level * level + 20
}

pub const MAX_LEVEL: u32 = 50;
