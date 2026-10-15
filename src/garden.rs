//! Sanctuary management: catalog items (seeds, habitats, decor), crops,
//! creature happiness, appeal, visitor income and offline catch-up.

use crate::creatures::{self, Elem, FLAVORS, SPECIES};
use crate::gfx::{Rgb, Screen};
use crate::save::{Crop, SaveData};
use rand::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Seed,
    Habitat,
    Decor,
}

pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    pub cat: Category,
    pub price: u64,
    pub desc: &'static str,
    /// Footprint in tiles (art is 8 px per tile).
    pub size: (i32, i32),
    pub appeal: f32,
    pub habitat: Option<Elem>,
    /// Night light: radius in pixels and colour.
    pub light: Option<(f32, Rgb)>,
    /// Sold only at the Curio Shop in Promptville.
    pub curio: bool,
    pub art: &'static [&'static str],
    pub pal: &'static [(char, Rgb)],
}

const NO_ART: &[&str] = &[];

const fn seed(id: &'static str, name: &'static str, price: u64, desc: &'static str) -> Item {
    Item { id, name, cat: Category::Seed, price, desc, size: (1, 1), appeal: 0.0, habitat: None, light: None, curio: false, art: NO_ART, pal: &[] }
}

pub static ITEMS: &[Item] = &[
    seed("seed_sweet", "Honeyberry Seeds", 8, "Sweet berries. Ripe in 20 min. Haikittens go wild for them."),
    seed("seed_spicy", "Flarepepper Seeds", 10, "Spicy peppers. Ripe in 40 min. Handle with oven mitts."),
    seed("seed_sour", "Zestlime Seeds", 10, "Sour limes. Ripe in 30 min. Tokenlings love the zing."),
    seed("seed_bitter", "Cocoabean Seeds", 12, "Bitter beans. Ripe in 45 min. An acquired taste."),
    seed("seed_fresh", "Mintleaf Seeds", 8, "Fresh mint. Ripe in 20 min. Smells like a clean build."),
    Item {
        id: "pond",
        name: "Lily Pond",
        cat: Category::Habitat,
        price: 180,
        desc: "A cosy pond with lily pads. Home for Tide types.",
        size: (2, 2),
        appeal: 6.0,
        habitat: Some(Elem::Tide),
        light: None,
        curio: false,
        art: &[
            "....wwwwwwww....",
            "..wwbbbbbbbbww..",
            ".wbbbBBbbbbbbbw.",
            "wbbllbbbbbbbbbbw",
            "wblllpbbbbBBbbbw",
            "wbbllbbbbbbbbbbw",
            "wbbbbbbbbbbllbbw",
            "wbbBBbbbbbllllbw",
            "wbbbbbbbbbblplbw",
            "wbbbbbllbbbbbbbw",
            "wbbbbllllbbBBbbw",
            ".wbbbbllbbbbbbw.",
            ".wbbbbbbbbbbbbw.",
            "..wwbbbbbbbbww..",
            "....wwwwwwww....",
            "................",
        ],
        pal: &[('w', Rgb(150, 150, 162)), ('b', Rgb(70, 140, 220)), ('B', Rgb(150, 200, 245)), ('l', Rgb(70, 165, 75)), ('p', Rgb(250, 150, 190))],
    },
    Item {
        id: "stump",
        name: "Mossy Stump",
        cat: Category::Habitat,
        price: 150,
        desc: "An old stump overgrown with moss and mushrooms. Home for Leaf types.",
        size: (2, 2),
        appeal: 5.0,
        habitat: Some(Elem::Leaf),
        light: None,
        curio: false,
        art: &[
            "................",
            "................",
            "....tttttttt....",
            "...tTTTTTTTTt...",
            "..tTTrrrrrrTTt..",
            "..tTrrTTTTrrTt..",
            "..tTTrrrrrrTTt..",
            "..btTTTTTTTTtb..",
            "..bbtttttttbbb..",
            "..bbbBbbbBbbbb.m",
            "..bbBbbbbbBbbbmM",
            "..bbbbbBbbbbbb.s",
            ".gbbbbbbbbbbbbg.",
            "ggbbbgbbbbgbbbgg",
            ".gggggggggggggg.",
            "................",
        ],
        pal: &[
            ('t', Rgb(120, 170, 70)),
            ('T', Rgb(200, 160, 110)),
            ('r', Rgb(160, 115, 70)),
            ('b', Rgb(120, 80, 50)),
            ('B', Rgb(95, 62, 40)),
            ('g', Rgb(90, 160, 70)),
            ('m', Rgb(220, 60, 60)),
            ('M', Rgb(250, 230, 230)),
            ('s', Rgb(240, 225, 200)),
        ],
    },
    Item {
        id: "rack",
        name: "Server Rack",
        cat: Category::Habitat,
        price: 220,
        desc: "Warm, humming and full of blinking lights. Home for Byte types.",
        size: (2, 2),
        appeal: 6.0,
        habitat: Some(Elem::Byte),
        light: Some((10.0, Rgb(120, 255, 170))),
        curio: false,
        art: &[
            "...kkkkkkkkkk...",
            "...kSSSSSSSSk...",
            "...kSLsLsLssk...",
            "...kSSSSSSSSk...",
            "...kssssssssk...",
            "...kSLsLsGssk...",
            "...kSSSSSSSSk...",
            "...kssssssssk...",
            "...kSLsGsLssk...",
            "...kSSSSSSSSk...",
            "...kssssssssk...",
            "...kSGsLsLssk...",
            "...kSSSSSSSSk...",
            "...kkkkkkkkkk...",
            "...cc......cc...",
            "................",
        ],
        pal: &[('k', Rgb(40, 44, 56)), ('S', Rgb(74, 80, 98)), ('s', Rgb(56, 60, 76)), ('L', Rgb(110, 255, 150)), ('G', Rgb(218, 119, 86)), ('c', Rgb(30, 30, 36))],
    },
    Item {
        id: "coil",
        name: "Tesla Coil",
        cat: Category::Habitat,
        price: 220,
        desc: "Crackles pleasantly. Do not lick. Home for Spark types.",
        size: (2, 2),
        appeal: 6.0,
        habitat: Some(Elem::Spark),
        light: Some((9.0, Rgb(200, 220, 255))),
        curio: false,
        art: &[
            ".......yy.......",
            "......yYYy......",
            ".....yYYYYy.....",
            "......yYYy......",
            ".......cc.......",
            "......cCCc......",
            ".......cc.......",
            "......cCCc......",
            ".......cc.......",
            "......cCCc......",
            ".......cc.......",
            ".....kkkkkk.....",
            "....kKKKKKKk....",
            "...kKKKKKKKKk...",
            "...kkkkkkkkkk...",
            "................",
        ],
        pal: &[('y', Rgb(180, 190, 210)), ('Y', Rgb(235, 240, 250)), ('c', Rgb(190, 120, 60)), ('C', Rgb(235, 165, 95)), ('k', Rgb(60, 60, 72)), ('K', Rgb(92, 92, 104))],
    },
    Item {
        id: "lantern",
        name: "Moon Lantern",
        cat: Category::Habitat,
        price: 200,
        desc: "A stone lantern that glows softly all night. Home for Dream types.",
        size: (2, 2),
        appeal: 7.0,
        habitat: Some(Elem::Dream),
        light: Some((16.0, Rgb(200, 170, 255))),
        curio: false,
        art: &[
            "................",
            "................",
            "......kkkk......",
            "....kkkkkkkk....",
            "...kKKKKKKKKk...",
            "......ssss......",
            ".....sLLLLs.....",
            ".....sLMMLs.....",
            ".....sLLLLs.....",
            "......ssss......",
            ".......ss.......",
            ".......ss.......",
            "......ssss......",
            ".....sSSSSs.....",
            "....ssssssss....",
            "................",
        ],
        pal: &[('k', Rgb(80, 70, 110)), ('K', Rgb(112, 100, 152)), ('s', Rgb(150, 150, 165)), ('S', Rgb(185, 185, 198)), ('L', Rgb(210, 190, 255)), ('M', Rgb(255, 248, 255))],
    },
    Item {
        id: "campfire",
        name: "Campfire",
        cat: Category::Habitat,
        price: 160,
        desc: "Crackling logs ringed with stones. Home for Ember types.",
        size: (2, 2),
        appeal: 6.0,
        habitat: Some(Elem::Ember),
        light: Some((22.0, Rgb(255, 170, 90))),
        curio: false,
        art: &[
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            ".......f........",
            "......fFf.......",
            ".....fFYFf......",
            ".....fFYYFf.....",
            "....fFYYYFf.....",
            "..rr.fFYFf.rr...",
            "..rrrlllllrrr...",
            ".rrlllLLLlllrr..",
            "..rr.lllll.rr...",
            "................",
        ],
        pal: &[('r', Rgb(140, 140, 150)), ('l', Rgb(120, 76, 44)), ('L', Rgb(170, 115, 70)), ('f', Rgb(230, 90, 40)), ('F', Rgb(250, 160, 50)), ('Y', Rgb(255, 230, 120))],
    },
    Item {
        id: "rocks",
        name: "Rock Garden",
        cat: Category::Habitat,
        price: 150,
        desc: "Raked sand and sturdy boulders. Home for Stone types.",
        size: (2, 2),
        appeal: 5.0,
        habitat: Some(Elem::Stone),
        light: None,
        curio: false,
        art: &[
            "................",
            "................",
            "................",
            "......oooo......",
            ".....oOOOOo.....",
            "....oOOooood....",
            "....oOoooddd....",
            ".sssooooddddss..",
            "ssSSSoddddSSSSss",
            "ssssssssssssssss",
            "sSSSSSSSSSSSSSSs",
            "ssssssssssoossss",
            "sSSSSSSSSoOOoSSs",
            "ssssssssssddssss",
            ".ssssssssssssss.",
            "................",
        ],
        pal: &[('o', Rgb(140, 140, 152)), ('O', Rgb(190, 190, 200)), ('d', Rgb(100, 100, 112)), ('s', Rgb(225, 210, 170)), ('S', Rgb(205, 188, 150))],
    },
    Item {
        id: "crt",
        name: "CRT Monolith",
        cat: Category::Habitat,
        price: 260,
        desc: "An ancient monitor that refuses to turn off. Home for Glitch types.",
        size: (2, 2),
        appeal: 8.0,
        habitat: Some(Elem::Glitch),
        light: Some((12.0, Rgb(240, 120, 230))),
        curio: false,
        art: &[
            "................",
            "..kkkkkkkkkkkk..",
            "..kCCCCCCCCCCk..",
            "..kCggggggggCk..",
            "..kCgGgggGggCk..",
            "..kCggggggggCk..",
            "..kCgg>_ggggCk..",
            "..kCggggggggCk..",
            "..kCCCCCCCCCCk..",
            "..kkkkkkkkkkkk..",
            "......kkkk......",
            ".....kKKKKk.....",
            "....kkkkkkkk....",
            "................",
            "................",
            "................",
        ],
        pal: &[('k', Rgb(46, 42, 52)), ('K', Rgb(80, 74, 90)), ('C', Rgb(170, 165, 150)), ('g', Rgb(40, 30, 70)), ('G', Rgb(240, 80, 200)), ('>', Rgb(120, 255, 200)), ('_', Rgb(120, 255, 200))],
    },
    Item {
        id: "lamp",
        name: "Garden Lamp",
        cat: Category::Decor,
        price: 40,
        desc: "Lights up the night. Visitors feel safer, creatures feel cosier.",
        size: (1, 1),
        appeal: 1.5,
        habitat: None,
        light: Some((20.0, Rgb(255, 220, 150))),
        curio: false,
        art: &["..kkkk..", ".kYYYYk.", ".kYWWYk.", "..kkkk..", "...kk...", "...kk...", "...kk...", "..kkkk.."],
        pal: &[('k', Rgb(50, 50, 60)), ('Y', Rgb(255, 220, 130)), ('W', Rgb(255, 250, 230))],
    },
    Item {
        id: "bench",
        name: "Bench",
        cat: Category::Decor,
        price: 30,
        desc: "A place for visitors to sit and watch your Claudimon.",
        size: (1, 1),
        appeal: 1.0,
        habitat: None,
        light: None,
        curio: false,
        art: &["........", "bbbbbbbb", "BBBBBBBB", "........", "bbbbbbbb", "BBBBBBBB", "k.k..k.k", "k......k"],
        pal: &[('b', Rgb(190, 135, 80)), ('B', Rgb(150, 100, 60)), ('k', Rgb(60, 50, 50))],
    },
    Item {
        id: "pot",
        name: "Flower Pot",
        cat: Category::Decor,
        price: 20,
        desc: "A cheerful little pot of flowers.",
        size: (1, 1),
        appeal: 0.8,
        habitat: None,
        light: None,
        curio: false,
        art: &[".p.r.p..", "prpgrpr.", ".gpgpg..", "..ggg...", ".cccccc.", ".cCCCCc.", "..cccc..", "..cccc.."],
        pal: &[('p', Rgb(240, 120, 170)), ('r', Rgb(250, 210, 80)), ('g', Rgb(80, 160, 70)), ('c', Rgb(200, 110, 70)), ('C', Rgb(230, 150, 100))],
    },
    Item {
        id: "pumpkin",
        name: "Jack-o'-Lantern",
        cat: Category::Decor,
        price: 25,
        desc: "A grinning pumpkin with a candle inside. Spooky, but friendly.",
        size: (1, 1),
        appeal: 1.0,
        habitat: None,
        light: Some((10.0, Rgb(255, 170, 60))),
        curio: false,
        art: &["....g...", "...g....", ".oOooOo.", "oOooooOo", "oOeooeOo", "oOoeeeOo", ".oOooOo.", "........"],
        pal: &[('g', Rgb(80, 130, 50)), ('o', Rgb(235, 130, 40)), ('O', Rgb(200, 100, 30)), ('e', Rgb(255, 230, 120))],
    },
    Item {
        id: "hedge",
        name: "Hedge",
        cat: Category::Decor,
        price: 15,
        desc: "Neatly trimmed. Great for building little paddocks.",
        size: (1, 1),
        appeal: 0.4,
        habitat: None,
        light: None,
        curio: false,
        art: &[".gGgGgg.", "gGggGgGg", "ggGgggGg", "gGgGgggg", "ggggGgGg", "gGggggGg", "gggGgggg", ".dddddd."],
        pal: &[('g', Rgb(60, 130, 60)), ('G', Rgb(100, 170, 80)), ('d', Rgb(40, 90, 40))],
    },
    Item {
        id: "fountain",
        name: "Fountain",
        cat: Category::Decor,
        price: 300,
        desc: "A sparkling centrepiece. Visitors throw tokens in it for luck.",
        size: (2, 2),
        appeal: 8.0,
        habitat: None,
        light: None,
        curio: false,
        art: &[
            "................",
            ".......ww.......",
            "......wWWw......",
            ".......ww.......",
            ".......ss.......",
            "....w..ss..w....",
            "...w.ssSSss.w...",
            "..w..ssssss..w..",
            ".ssssssssssssss.",
            "sSbbbbbbbbbbbbSs",
            "sSbBbbbbbbBbbbSs",
            "sSbbbbBbbbbbbbSs",
            ".ssssssssssssss.",
            "..ssssssssssss..",
            "................",
            "................",
        ],
        pal: &[('w', Rgb(170, 215, 250)), ('W', Rgb(240, 250, 255)), ('s', Rgb(185, 185, 195)), ('S', Rgb(215, 215, 225)), ('b', Rgb(70, 140, 220)), ('B', Rgb(150, 200, 245))],
    },
    Item {
        id: "statue",
        name: "Claude Statue",
        cat: Category::Decor,
        price: 400,
        desc: "A monument to a helpful friend. Glows faintly. Very good for appeal.",
        size: (2, 2),
        appeal: 14.0,
        habitat: None,
        light: Some((18.0, Rgb(255, 170, 120))),
        curio: false,
        art: &[
            "................",
            "...o....o....o..",
            "....o...o...o...",
            ".....o..o..o....",
            "......oOOOo.....",
            "..oooooOWOooooo.",
            "......oOOOo.....",
            ".....o..o..o....",
            "....o...o...o...",
            "...o....o....o..",
            "......ssss......",
            "......sSSs......",
            ".....ssssss.....",
            "....sSSSSSSs....",
            "...ssssssssss...",
            "................",
        ],
        pal: &[('o', Rgb(218, 119, 86)), ('O', Rgb(240, 160, 120)), ('W', Rgb(255, 232, 214)), ('s', Rgb(160, 156, 168)), ('S', Rgb(196, 192, 204))],
    },
    Item {
        id: "crystal_lamp",
        name: "Crystal Lamp",
        cat: Category::Decor,
        price: 85,
        desc: "A Kernel Crystal on a stand. Casts a cool, calm glow.",
        size: (1, 1),
        appeal: 3.0,
        habitat: None,
        light: Some((24.0, Rgb(120, 240, 255))),
        curio: true,
        art: &["...cc...", "..cCCc..", "..cWWc..", ".cCWWCc.", "..cCCc..", "...kk...", "..kkkk..", ".kkkkkk."],
        pal: &[('c', Rgb(90, 200, 230)), ('C', Rgb(160, 240, 255)), ('W', Rgb(240, 255, 255)), ('k', Rgb(60, 56, 70))],
    },
    Item {
        id: "glowcap_lamp",
        name: "Glowcap Lamp",
        cat: Category::Decor,
        price: 55,
        desc: "A giant glowing mushroom from the Latent Woods. Softly bioluminescent.",
        size: (1, 1),
        appeal: 2.0,
        habitat: None,
        light: Some((16.0, Rgb(120, 255, 200))),
        curio: true,
        art: &["........", "..gggg..", ".gGggGg.", "gggggggg", "...ss...", "...ss...", "..ssss..", "........"],
        pal: &[('g', Rgb(70, 200, 170)), ('G', Rgb(180, 255, 230)), ('s', Rgb(230, 220, 200))],
    },
    Item {
        id: "potted_palm",
        name: "Potted Palm",
        cat: Category::Decor,
        price: 40,
        desc: "A little piece of Token Shore for your Sanctuary.",
        size: (1, 1),
        appeal: 1.5,
        habitat: None,
        light: None,
        curio: true,
        art: &["gg.G.gg.", ".gGGGg..", "g.gtg.g.", "...t....", "...t....", "..cccc..", "..cCCc..", "...cc..."],
        pal: &[('g', Rgb(60, 150, 70)), ('G', Rgb(100, 190, 90)), ('t', Rgb(150, 105, 60)), ('c', Rgb(200, 110, 70)), ('C', Rgb(230, 150, 100))],
    },
    Item {
        id: "duck_monument",
        name: "Rubber Duck Monument",
        cat: Category::Decor,
        price: 300,
        desc: "Dedicated to every bug ever solved by explaining it out loud.",
        size: (2, 2),
        appeal: 10.0,
        habitat: None,
        light: None,
        curio: true,
        art: &[
            "................",
            "......yyyy......",
            ".....yyyyyy.....",
            ".....yyyEyyoo...",
            ".....yyyyyyoo...",
            "..y...yyyy......",
            "..yy.yyyyyyy....",
            "..yyyyyyyyyyy...",
            "..yYYyyyyyyyy...",
            "...yYYYyyyyy....",
            "....yyyyyyy.....",
            "......ssss......",
            ".....sSSSSs.....",
            "....ssssssss....",
            "...sSSSSSSSSs...",
            "................",
        ],
        pal: &[('y', Rgb(250, 220, 60)), ('Y', Rgb(255, 245, 150)), ('E', Rgb(40, 32, 52)), ('o', Rgb(240, 140, 40)), ('s', Rgb(160, 156, 168)), ('S', Rgb(196, 192, 204))],
    },
    Item {
        id: "trophy_haiku",
        name: "Haiku Trophy",
        cat: Category::Decor,
        price: 0,
        desc: "Won at The Benchmark. Visitors stop and stare.",
        size: (1, 1),
        appeal: 3.0,
        habitat: None,
        light: None,
        curio: false,
        art: &[".yyyyyy.", "yyYyyyyy", ".yYyyyy.", "..yyyy..", "...yy...", "...yy...", "..kkkk..", ".kkkkkk."],
        pal: &[('y', Rgb(205, 130, 60)), ('Y', Rgb(250, 200, 150)), ('k', Rgb(90, 62, 44))],
    },
    Item {
        id: "trophy_sonnet",
        name: "Sonnet Trophy",
        cat: Category::Decor,
        price: 0,
        desc: "Won at The Benchmark. Visitors stop and stare.",
        size: (1, 1),
        appeal: 5.0,
        habitat: None,
        light: None,
        curio: false,
        art: &[".yyyyyy.", "yyYyyyyy", ".yYyyyy.", "..yyyy..", "...yy...", "...yy...", "..kkkk..", ".kkkkkk."],
        pal: &[('y', Rgb(200, 205, 218)), ('Y', Rgb(250, 252, 255)), ('k', Rgb(90, 62, 44))],
    },
    Item {
        id: "trophy_opus",
        name: "Opus Trophy",
        cat: Category::Decor,
        price: 0,
        desc: "Won at The Benchmark. Visitors stop and stare.",
        size: (1, 1),
        appeal: 8.0,
        habitat: None,
        light: Some((16.0, Rgb(255, 220, 120))),
        curio: false,
        art: &[".yyyyyy.", "yyYyyyyy", ".yYyyyy.", "..yyyy..", "...yy...", "...yy...", "..kkkk..", ".kkkkkk."],
        pal: &[('y', Rgb(250, 205, 70)), ('Y', Rgb(255, 245, 190)), ('k', Rgb(90, 62, 44))],
    },
    Item {
        id: "trophy_guardian",
        name: "Guardian Star",
        cat: Category::Decor,
        price: 0,
        desc: "Given by the Guardians of Mythos Isle. It hums with old stories.",
        size: (1, 1),
        appeal: 10.0,
        habitat: None,
        light: Some((20.0, Rgb(200, 180, 255))),
        curio: false,
        art: &["...ss...", "...SS...", "ssSWWSss", ".sSWWSs.", "..sSSs..", ".ss..ss.", "..kkkk..", ".kkkkkk."],
        pal: &[('s', Rgb(190, 160, 255)), ('S', Rgb(225, 210, 255)), ('W', Rgb(255, 255, 255)), ('k', Rgb(90, 80, 110))],
    },
];

pub fn item(id: &str) -> Option<usize> {
    ITEMS.iter().position(|i| i.id == id)
}

/// Seed item id for a flavour index.
pub fn seed_id(flavor: usize) -> &'static str {
    ["seed_sweet", "seed_spicy", "seed_sour", "seed_bitter", "seed_fresh"][flavor]
}

/// Land expansion prices; the index is the level you are buying.
pub const LAND_PRICES: [u64; 3] = [1500, 5000, 12000];
pub const MAX_LAND: u8 = 3;

/// Curios found while exploring: (item id, name, sell price at the Curio Shop).
pub const LOOT: [(&str, &str, u64); 3] = [("loot_shell", "Seashell", 6), ("loot_glowcap", "Glowcap", 5), ("loot_crystal", "Kernel Crystal", 14)];

/// What Claude pays per berry, by flavour index (slower crops sell for more).
pub const BERRY_PRICES: [u64; 5] = [3, 5, 4, 6, 3];

/// Draw a catalog item's art with small animations (flames, LEDs, sparks, glitches, water).
pub fn draw_item(s: &mut Screen, idx: usize, x: i32, y: i32, t: f32) {
    let it = &ITEMS[idx];
    for (ry, row) in it.art.iter().enumerate() {
        for (rx, ch) in row.chars().enumerate() {
            let Some(mut c) = it.pal.iter().find(|(k, _)| *k == ch).map(|p| p.1) else { continue };
            let (px, py) = (x + rx as i32, y + ry as i32);
            let n = crate::world::hash(rx as i32, ry as i32, (t * 4.0) as u32);
            match (it.id, ch) {
                ("campfire", 'f' | 'F' | 'Y') => {
                    let k = (t * 9.0 + rx as f32 * 1.3 + ry as f32).sin() * 0.5 + 0.5;
                    c = c.lerp(Rgb(255, 240, 160), k * 0.5);
                    if ry < 9 && n % 5 == 0 {
                        continue; // flickering flame tips
                    }
                }
                ("rack", 'L') if n % 3 == 0 => c = Rgb(30, 80, 50),
                ("crt", 'g') => {
                    let scan = ((t * 6.0) as i32 + ry as i32) % 6 == 0;
                    c = if scan { Rgb(90, 220, 220) } else if n % 7 == 0 { Rgb(140, 50, 160) } else { c };
                }
                ("fountain", 'w' | 'W') if n % 3 == 0 => c = Rgb(255, 255, 255),
                ("pond", 'b') if (t * 2.0 + rx as f32 * 0.5 + ry as f32 * 0.8).sin() > 0.9 => c = Rgb(170, 210, 250),
                _ => {}
            }
            s.set(px, py, c);
        }
    }
    if it.id == "coil" && (t * 3.0).sin() > 0.3 {
        // Little arcs of electricity around the sphere.
        for i in 0..3 {
            let a = t * 13.0 + i as f32 * 2.1;
            s.set(x + 8 + (a.cos() * 5.0) as i32, y + 2 + (a.sin() * 3.0) as i32, Rgb(255, 255, 160));
        }
    }
}

// ------------------------------------------------------------------ crops

pub fn grow_secs(flavor: usize) -> i64 {
    60 * [20, 40, 30, 45, 20][flavor.min(4)]
}

pub const WATER_COOLDOWN: i64 = 30 * 60;

impl Crop {
    /// Growth progress 0..1 (1 = ripe).
    pub fn progress(&self, now: i64) -> f32 {
        ((now - self.planted + self.bonus) as f32 / grow_secs(self.flavor as usize) as f32).clamp(0.0, 1.0)
    }
    pub fn can_water(&self, now: i64) -> bool {
        now - self.watered_at > WATER_COOLDOWN && self.progress(now) < 1.0
    }
    pub fn water(&mut self, now: i64) {
        self.watered_at = now;
        self.bonus += grow_secs(self.flavor as usize) / 4;
    }
    pub fn watered_recently(&self, now: i64) -> bool {
        now - self.watered_at < WATER_COOLDOWN
    }
}

/// Draw a crop growing on a soil tile at pixel (x, y).
pub fn draw_crop(s: &mut Screen, crop: &Crop, x: i32, y: i32, now: i64, t: f32) {
    let p = crop.progress(now);
    let fruit = FLAVORS[crop.flavor as usize].color();
    let leaf = Rgb(80, 170, 70);
    let dark = Rgb(50, 120, 50);
    if crop.watered_recently(now) {
        for yy in 0..8 {
            for xx in 0..8 {
                let c = s.get(x + xx, y + yy);
                s.set(x + xx, y + yy, c.lerp(Rgb(50, 32, 24), 0.45));
            }
        }
    }
    if p < 0.25 {
        s.set(x + 3, y + 5, Rgb(200, 180, 120));
        s.set(x + 5, y + 4, Rgb(200, 180, 120));
    } else if p < 0.6 {
        s.set(x + 4, y + 6, dark);
        s.set(x + 4, y + 5, leaf);
        s.set(x + 3, y + 4, leaf);
        s.set(x + 5, y + 4, leaf);
    } else {
        for (dx, dy) in [(4, 7), (4, 6), (4, 5), (3, 4), (5, 4), (2, 3), (6, 3), (3, 2), (5, 2), (4, 3), (4, 2)] {
            s.set(x + dx, y + dy, if dy > 4 { dark } else { leaf });
        }
        if p >= 1.0 {
            let bob = if (t * 3.0 + x as f32).sin() > 0.7 { -1 } else { 0 };
            for (dx, dy) in [(2, 2), (6, 2), (4, 1), (3, 4), (5, 4)] {
                s.set(x + dx, y + dy + bob, fruit);
            }
            s.set(x + 4, y + bob, fruit.lerp(Rgb(255, 255, 255), 0.6));
        } else {
            s.set(x + 4, y + 1, leaf);
        }
    }
}

// ------------------------------------------------------- happiness & appeal

/// Assign each creature to a matching habitat (max 3 residents per habitat).
/// Returns, per creature, the index into `data.placed` of its home.
pub fn assign_homes(data: &SaveData) -> Vec<Option<usize>> {
    let mut used = vec![0u32; data.placed.len()];
    data.mons
        .iter()
        .map(|m| {
            let sp = creatures::by_name(&m.species)?;
            let types = SPECIES[sp].types;
            let home = data.placed.iter().enumerate().position(|(i, p)| {
                let hab = item(&p.kind).and_then(|k| ITEMS[k].habitat);
                used[i] < 3 && hab.is_some_and(|e| types.contains(&e))
            })?;
            used[home] += 1;
            Some(home)
        })
        .collect()
}

pub fn decor_appeal(data: &SaveData) -> f32 {
    data.placed.iter().filter_map(|p| item(&p.kind)).map(|i| ITEMS[i].appeal).sum()
}

pub const HAPPY_WINDOW: i64 = 12 * 3600;

/// The happiness a creature drifts toward, given its current care.
pub fn happiness_target(data: &SaveData, i: usize, has_home: bool, now: i64) -> f32 {
    let m = &data.mons[i];
    let mut t = 35.0;
    if has_home {
        t += 25.0;
    }
    if now - m.last_fed < HAPPY_WINDOW {
        t += 15.0;
        if m.fed_fav {
            t += 10.0;
        }
    }
    if now - m.last_pet < HAPPY_WINDOW {
        t += 10.0;
    }
    t += (decor_appeal(data) / 4.0).min(15.0);
    t.min(100.0)
}

fn rarity_value(r: u8) -> f32 {
    [2.0, 4.0, 8.0, 15.0][(r.clamp(1, 4) - 1) as usize]
}

/// Appeal from the creatures themselves: rarer, happier, friendlier and shiny ones draw bigger crowds.
pub fn creature_appeal(data: &SaveData) -> f32 {
    data.mons
        .iter()
        .filter_map(|m| {
            let sp = creatures::by_name(&m.species)?;
            let base = rarity_value(SPECIES[sp].rarity) * if m.shiny { 2.0 } else { 1.0 };
            Some(base * (0.25 + 0.75 * m.happy / 100.0) * (1.0 + m.friendship as f32 / 50.0))
        })
        .sum()
}

/// Decorations multiply creature appeal with diminishing returns (×1 up to ×2.5).
/// Visitors come for the creatures; a pretty Sanctuary just makes them stay longer.
pub fn decor_multiplier(data: &SaveData) -> f32 {
    1.0 + DECOR_MAX_BONUS * (1.0 - (-decor_appeal(data) / DECOR_SCALE).exp())
}

const DECOR_MAX_BONUS: f32 = 1.5;
const DECOR_SCALE: f32 = 60.0;

/// How attractive the Sanctuary is to visitors. Zero without creatures, however fancy the decor.
pub fn appeal(data: &SaveData) -> f32 {
    creature_appeal(data) * decor_multiplier(data)
}

pub fn tokens_per_hour(data: &SaveData) -> f64 {
    if data.mons.is_empty() { 0.0 } else { appeal(data) as f64 * TOKENS_PER_APPEAL_HOUR }
}

pub const TOKENS_PER_APPEAL_HOUR: f64 = 25.0;
/// Offline income stops accumulating after this long, so it rewards check-ins.
pub const OFFLINE_CAP_SECS: i64 = 12 * 3600;

#[derive(Default, Debug)]
pub struct Report {
    pub levels: u32,
    pub secs: i64,
    pub tokens: u64,
    pub visitors: u32,
    pub gifts: u32,
    pub ripe: u32,
}

/// Advance the sanctuary simulation from `data.last_tick` to `now`.
pub fn simulate(data: &mut SaveData, now: i64, rng: &mut impl Rng) -> Report {
    let mut r = Report::default();
    let from = data.last_tick.min(now);
    data.last_tick = now;
    if from <= 0 {
        return r; // first run
    }
    let dt = now - from;
    r.secs = dt;
    if dt <= 0 {
        return r;
    }
    let capped = dt.min(OFFLINE_CAP_SECS) as f64 / 3600.0;
    let hours = dt as f32 / 3600.0;

    // Crops that ripened during this span.
    r.ripe = data.crops.values().filter(|c| c.progress(from) < 1.0 && c.progress(now) >= 1.0).count() as u32;

    // Happiness drifts toward its target (time constant ~3h); happy creatures may leave gifts.
    let homes = assign_homes(data);
    for i in 0..data.mons.len() {
        let target = happiness_target(data, i, homes[i].is_some(), now);
        let m = &mut data.mons[i];
        m.happy += (target - m.happy) * (1.0 - (-hours / 3.0).exp());
        // Resting at home slowly trains your Claudimon; happier ones learn faster.
        let xp = (capped as f32 * (30.0 + m.happy / 2.0) + m.xp_frac).max(0.0);
        m.xp_frac = xp.fract();
        let before = m.level;
        add_xp(m, xp as u32);
        r.levels += m.level - before;
        if m.happy >= 75.0 && !m.gift {
            let chance = 1.0 - (-(capped as f32) / 4.0).exp();
            if rng.gen_bool(chance.clamp(0.0, 1.0) as f64) {
                m.gift = true;
                r.gifts += 1;
            }
        }
    }

    if !data.mons.is_empty() {
        let earned = tokens_per_hour(data) * capped + data.token_frac;
        r.tokens = earned.floor() as u64;
        data.token_frac = earned - earned.floor();
        data.tokens += r.tokens;
        r.visitors = (appeal(data) as f64 / 3.0 * capped).round() as u32;
        data.total_visitors += r.visitors as u64;
    }
    r
}

/// Add XP and level up (no evolution here; the game shows that with a cutscene).
/// Returns the levels reached.
pub fn add_xp(m: &mut crate::save::OwnedMon, xp: u32) -> Vec<u32> {
    let mut reached = Vec::new();
    m.xp += xp;
    while m.level < creatures::MAX_LEVEL && m.xp >= creatures::xp_to_next(m.level) {
        m.xp -= creatures::xp_to_next(m.level);
        m.level += 1;
        reached.push(m.level);
    }
    if m.level >= creatures::MAX_LEVEL {
        m.xp = 0;
    }
    reached
}

/// The species this creature is ready to evolve into, if any.
pub fn ready_to_evolve(m: &crate::save::OwnedMon) -> Option<usize> {
    let sp = creatures::by_name(&m.species)?;
    let (lvl, into) = SPECIES[sp].evolves?;
    if m.level >= lvl { creatures::by_name(into) } else { None }
}

// ---------------------------------------------------------------- visitors

pub const VISITOR_NAMES: &[&str] = &[
    "Ada", "Linus", "Grace", "Alan", "Margaret", "Ken", "Dennis", "Barbara", "Guido", "Bjarne", "Yukihiro", "Hedy", "Edsger", "Frances", "Donald", "Radia",
];

pub const VISITOR_LINES: &[&str] = &[
    "My build takes 20 minutes, so here I am!",
    "I asked Claude for directions. It gave me three options and a caveat.",
    "Lovely Sanctuary! Very... well-structured.",
    "Ten out of ten. Would visit again after my next deploy.",
    "Have you tried turning the pond off and on again?",
    "Excellent vibes here. Very low latency.",
    "I'm just here while my tests run. All 4,000 of them.",
    "I heard this place has a 200k-token context window of fun.",
    "Is it just me, or is this garden thinking step by step?",
    "I left my laptop compiling at home. It'll be fine. Probably.",
    "Your berries smell amazing. Are those organic, free-range Honeyberries?",
    "I came for the creatures, I stayed for the cozy.",
    "rm -rf my worries. This place is great.",
    "Do you think the Claudimon dream in tokens?",
];

/// A line about one of your creatures, if you have any.
pub fn visitor_line(data: &SaveData, rng: &mut impl Rng) -> String {
    if !data.mons.is_empty() && rng.gen_bool(0.45) {
        let m = &data.mons[rng.gen_range(0..data.mons.len())];
        let mood = if m.happy >= 70.0 { "looks so happy here" } else if m.happy >= 40.0 { "is adorable" } else { "seems a bit lonely... maybe it needs a home?" };
        return format!("Your {} {}!", m.species, mood);
    }
    VISITOR_LINES[rng.gen_range(0..VISITOR_LINES.len())].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::{OwnedMon, Placed};
    use rand::SeedableRng;

    #[test]
    fn art_is_well_formed() {
        for it in ITEMS {
            if it.art.is_empty() {
                continue;
            }
            let (w, h) = ((it.size.0 * 8) as usize, (it.size.1 * 8) as usize);
            assert_eq!(it.art.len(), h, "{} art height", it.id);
            for row in it.art {
                assert_eq!(row.chars().count(), w, "{} art row width: {row:?}", it.id);
                for ch in row.chars().filter(|c| *c != '.') {
                    assert!(it.pal.iter().any(|(k, _)| *k == ch), "{} missing palette entry {ch:?}", it.id);
                }
            }
        }
    }

    fn mon(species: &str) -> OwnedMon {
        OwnedMon { met: 0, ..OwnedMon::new(species, false) }
    }

    #[test]
    fn habitats_house_matching_types_up_to_capacity() {
        let mut d = SaveData::default();
        for _ in 0..4 {
            d.mons.push(mon("Puddlefin")); // Tide
        }
        d.mons.push(mon("Haikitten")); // Dream
        d.placed.push(Placed { kind: "pond".into(), x: 5, y: 5 });
        let homes = assign_homes(&d);
        assert_eq!(homes.iter().take(4).filter(|h| h.is_some()).count(), 3, "pond holds three");
        assert_eq!(homes[4], None, "no Dream habitat yet");
    }

    #[test]
    fn offline_income_is_capped_and_crops_ripen() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let mut d = SaveData::default();
        d.mons.push(mon("Haikitten"));
        d.crops.insert("10:11".into(), Crop { flavor: 0, planted: 1000, bonus: 0, watered_at: 0 });
        d.last_tick = 1000;
        let start = d.tokens;
        let a = simulate(&mut d, 1000 + 48 * 3600, &mut rng);
        assert_eq!(a.ripe, 1);
        assert!(a.tokens > 0);
        let per_hour = tokens_per_hour(&d);
        assert!((a.tokens as f64) <= per_hour * 12.0 + 1.0, "income capped at 12h");
        assert_eq!(d.tokens, start + a.tokens);
    }

    /// A typical mid-game Sanctuary: four common and two uncommon creatures, fairly happy.
    fn mid_game() -> SaveData {
        let mut d = SaveData::default();
        for sp in ["Haikitten", "Tokenling", "Cachew", "Puddlefin", "Gitcko", "Asyncrow"] {
            d.mons.push(OwnedMon { happy: 70.0, friendship: 3, ..mon(sp) });
        }
        d
    }

    #[test]
    fn decor_alone_earns_nothing() {
        let mut d = SaveData::default();
        d.placed.push(Placed { kind: "statue".into(), x: 1, y: 1 });
        assert_eq!(appeal(&d), 0.0);
        assert_eq!(tokens_per_hour(&d), 0.0);
    }

    #[test]
    fn decor_multiplier_has_diminishing_returns() {
        let mut d = mid_game();
        assert_eq!(decor_multiplier(&d), 1.0);
        let mut last_gain = f32::INFINITY;
        for _ in 0..12 {
            let before = decor_multiplier(&d);
            d.placed.push(Placed { kind: "fountain".into(), x: 1, y: 1 });
            let gain = decor_multiplier(&d) - before;
            assert!(gain > 0.0 && gain < last_gain, "each extra fountain helps a bit less");
            last_gain = gain;
        }
        assert!(decor_multiplier(&d) < 1.0 + DECOR_MAX_BONUS);
    }

    #[test]
    fn every_purchase_pays_back_in_a_few_hours() {
        // In a mid-game Sanctuary with no decor yet, the first of any decoration
        // should pay for itself through extra income in roughly 2-8 hours.
        let base = mid_game();
        let income = tokens_per_hour(&base);
        assert!((200.0..600.0).contains(&income), "mid-game income {income:.0}/h");
        for it in ITEMS.iter().filter(|i| i.cat != Category::Seed && i.price > 0) {
            let mut d = base.clone();
            d.placed.push(Placed { kind: it.id.into(), x: 1, y: 1 });
            let gain = tokens_per_hour(&d) - income;
            let hours = it.price as f64 / gain;
            assert!((2.0..8.0).contains(&hours), "{} pays back in {hours:.1}h", it.id);
        }
    }

    #[test]
    fn early_game_rates_are_sensible() {
        let mut d = SaveData::default();
        d.mons.push(mon("Haikitten"));
        let one = tokens_per_hour(&d);
        assert!((15.0..40.0).contains(&one), "one common creature earns {one:.0}/h");
        // The cheapest habitat should be affordable within a few hours of play with three creatures.
        d.mons.push(mon("Tokenling"));
        d.mons.push(mon("Cachew"));
        let hours = 150.0 / tokens_per_hour(&d);
        assert!((1.0..4.0).contains(&hours), "first habitat takes {hours:.1}h of passive income");
    }

    /// `cargo test balance_report -- --ignored --nocapture` prints the economy at a glance.
    #[test]
    #[ignore]
    fn balance_report() {
        let base = mid_game();
        let income = tokens_per_hour(&base);
        println!("mid-game income: {income:.0}/h (creature appeal {:.1})", creature_appeal(&base));
        for it in ITEMS.iter().filter(|i| i.cat != Category::Seed) {
            let mut d = base.clone();
            d.placed.push(Placed { kind: it.id.into(), x: 1, y: 1 });
            let gain = tokens_per_hour(&d) - income;
            println!("{:<16} ✦{:>4}  +{:>5.1}/h  payback {:>4.1}h", it.name, it.price, gain, it.price as f64 / gain);
        }
        let mut d = base.clone();
        for decor in [0, 10, 20, 40, 60, 100, 200] {
            d.placed.clear();
            for _ in 0..decor {
                d.placed.push(Placed { kind: "bench".into(), x: 1, y: 1 });
            }
            println!("decor {decor:>3}: ×{:.2}  {:.0}/h", decor_multiplier(&d), tokens_per_hour(&d));
        }
        for n in [1, 3, 6] {
            let mut d = SaveData::default();
            for _ in 0..n {
                d.mons.push(mon("Haikitten"));
            }
            println!("{n} common creature(s) @50% happy, no decor: {:.0}/h", tokens_per_hour(&d));
        }
    }
}
