//! Tile maps and procedural, season-aware tile art (8x8 pixels per tile).

use crate::clock::Season;
use crate::gfx::{Rgb, Screen};
use std::collections::HashSet;

pub const TILE: i32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Grass,
    TallGrass,
    Flower,
    Path,
    Tree,
    Water,
    Sand,
    Fence,
    Soil,
    House,
    Rock,
    Bridge,
    /// Berry bush; the value is the flavour index of its berries.
    Bush(u8),
    Palm,
    /// Collectible curios that regrow like berry bushes.
    Shell,
    Mushroom,
    Crystal,
    CaveFloor,
    CaveWall,
    Lava,
    CaveMouth,
    Lab,
    Shop,
    Arena,
    Lamp,
    Lighthouse,
    /// A Lost Page on a little lectern (collected pages leave it empty).
    Page,
    /// The Thinking Shrine in the Latent Woods.
    Shrine,
    /// Mythos Isle altars: 0 trust, 1 strength, 2 wisdom, 3 the peak.
    Altar(u8),
}

impl Tile {
    pub fn solid(self) -> bool {
        matches!(
            self,
            Tile::Tree
                | Tile::Water
                | Tile::Fence
                | Tile::House
                | Tile::Rock
                | Tile::Bush(_)
                | Tile::Palm
                | Tile::Crystal
                | Tile::CaveWall
                | Tile::Lava
                | Tile::Lab
                | Tile::Shop
                | Tile::Arena
                | Tile::Lamp
                | Tile::Lighthouse
                | Tile::Page
                | Tile::Shrine
                | Tile::Altar(_)
        )
    }

    /// Curio collected from this tile: (item id, display name).
    pub fn loot(self) -> Option<(&'static str, &'static str)> {
        match self {
            Tile::Shell => Some(("loot_shell", "Seashell")),
            Tile::Mushroom => Some(("loot_glowcap", "Glowcap")),
            Tile::Crystal => Some(("loot_crystal", "Kernel Crystal")),
            _ => None,
        }
    }
    fn from_char(c: char) -> Tile {
        match c {
            '"' => Tile::TallGrass,
            '*' => Tile::Flower,
            '=' => Tile::Path,
            'T' => Tile::Tree,
            '~' => Tile::Water,
            ':' => Tile::Sand,
            '#' => Tile::Fence,
            '%' => Tile::Soil,
            'H' => Tile::House,
            'o' => Tile::Rock,
            'L' => Tile::Lab,
            'S' => Tile::Shop,
            'A' => Tile::Arena,
            'i' => Tile::Lamp,
            'P' => Tile::Palm,
            '0'..='4' => Tile::Bush(c as u8 - b'0'),
            _ => Tile::Grass,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapId {
    Garden,
    Meadow,
    Woods,
    Shore,
    Caves,
    Town,
    Island,
}

pub const ALL_MAPS: [MapId; 7] = [MapId::Garden, MapId::Meadow, MapId::Woods, MapId::Shore, MapId::Caves, MapId::Town, MapId::Island];

impl MapId {
    pub fn key(self) -> &'static str {
        match self {
            MapId::Garden => "garden",
            MapId::Meadow => "meadow",
            MapId::Woods => "woods",
            MapId::Shore => "shore",
            MapId::Caves => "caves",
            MapId::Town => "town",
            MapId::Island => "isle",
        }
    }
    pub fn from_key(k: &str) -> MapId {
        ALL_MAPS.into_iter().find(|m| m.key() == k).unwrap_or(MapId::Garden)
    }
    pub fn title(self) -> &'static str {
        match self {
            MapId::Garden => "Your Sanctuary",
            MapId::Meadow => "Prompt Meadows",
            MapId::Woods => "Latent Woods",
            MapId::Shore => "Token Shore",
            MapId::Caves => "Kernel Caverns",
            MapId::Town => "Promptville",
            MapId::Island => "Mythos Isle",
        }
    }
    /// Which species live here (see `creatures::MEADOW` etc.); 0 = no wild creatures.
    pub fn biome_bit(self) -> u8 {
        match self {
            MapId::Meadow => crate::creatures::MEADOW,
            MapId::Woods => crate::creatures::WOODS,
            MapId::Shore => crate::creatures::SHORE,
            MapId::Caves => crate::creatures::CAVE,
            MapId::Island => crate::creatures::ISLE,
            _ => 0,
        }
    }
}

/// Stepping onto (x, y) moves the player to (tx, ty) on another map.
pub struct Exit {
    pub x: i32,
    pub y: i32,
    pub to: MapId,
    pub tx: i32,
    pub ty: i32,
}

pub struct Map {
    pub id: MapId,
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    pub exits: Vec<Exit>,
    /// Bushes whose berries have been picked (regrow over real time).
    pub picked: HashSet<(i32, i32)>,
    /// Tiles covered by placed habitats/decor.
    pub occupied: Vec<bool>,
}

const GARDEN: &[&str] = &[
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
    "TT....*.....\"\"\"\"......T......*.......TTT",
    "TT..HHHH....\"\"\"\"..............~~~~...TTT",
    "TT..HHHH..*..\"\"\"...*.....*...~~~~~~..TTT",
    "TT..HHHH.....................~~~~~~~..TT",
    "TT...==..........o............~~~~~...TT",
    "TT...==.....................:::.......TT",
    "TT...===================================",
    "TT...==......*.........==.............TT",
    "TT...==..#########.....==....*....T...TT",
    "TT...==..#%%%%%%%#.....==.............TT",
    "TT...==..#%%%%%%%#.....==...\"\"\"\"\".....TT",
    "TT.*.==..#%%%%%%%#.....==...\"\"\"\"\"..*..TT",
    "TT...==..####.####.....==.............TT",
    "TT...==..........*.....==......o......TT",
    "TT...==.......T........==.............TT",
    "TT...=====================............TT",
    "TT.0.....*..........*.......T.....*.4.TT",
    "TTT...........\"\"\"...................TTTT",
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
];

pub const MEADOW_ENTRY_Y: i32 = 32;
/// Where Ada's ferry lands on Mythos Isle, and where Mythos sleeps.
pub const ISLE_ARRIVAL: (i32, i32) = (28, 36);
pub const MYTHOS_SPOT: (i32, i32) = (28, 7);

impl Map {
    pub fn load(id: MapId, land: u8) -> Map {
        match id {
            MapId::Garden => Map::garden(land),
            MapId::Meadow => Map::meadow(),
            MapId::Woods => Map::woods(),
            MapId::Shore => Map::shore(),
            MapId::Caves => Map::caves(),
            MapId::Town => Map::town(),
            MapId::Island => Map::island(),
        }
    }

    /// First tile of a kind (for landmarks like the lighthouse or a page).
    pub fn find(&self, tile: Tile) -> Option<(i32, i32)> {
        self.tiles.iter().position(|t| *t == tile).map(|i| (i as i32 % self.w, i as i32 / self.w))
    }

    fn set(&mut self, x: i32, y: i32, t: Tile) {
        if self.inside(x, y) {
            self.tiles[(y * self.w + x) as usize] = t;
        }
    }

    /// Mythos Isle: a glowing island past the horizon, reached only by Ada's ferry.
    fn island() -> Map {
        let (w, h) = (56, 40);
        let mut tiles = vec![Tile::Water; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = ((x - 28) as f32 / 24.0, (y - 20) as f32 / 17.0);
                let d = (dx * dx + dy * dy).sqrt() + (fbm(x as f32 / 6.0, y as f32 / 6.0, 501) - 0.5) * 0.35;
                let r = hash(x, y, 511) % 100;
                tiles[(y * w + x) as usize] = if d > 1.0 {
                    Tile::Water
                } else if d > 0.88 {
                    if r < 6 { Tile::Shell } else { Tile::Sand }
                } else if fbm(x as f32 / 5.0, y as f32 / 5.0, 521) > 0.64 {
                    Tile::Tree
                } else {
                    match r {
                        0..=11 => Tile::Flower,
                        12..=19 => Tile::TallGrass,
                        20..=21 => Tile::Rock,
                        _ => Tile::Grass,
                    }
                };
            }
        }
        let c = (28, 20);
        carve(&mut tiles, w, h, (28, 39), c, Tile::Path, 531); // the pier, then the path inland
        carve(&mut tiles, w, h, c, (14, 20), Tile::Path, 541);
        carve(&mut tiles, w, h, c, (42, 20), Tile::Path, 551);
        carve(&mut tiles, w, h, c, (28, 9), Tile::Path, 561);
        let mut m = Map::new(MapId::Island, w, h, tiles, Vec::new());
        // The peak: a clearing where Mythos sleeps.
        for y in 4..=8 {
            for x in 25..=32 {
                m.set(x, y, if (y + x) % 5 == 0 { Tile::Flower } else { Tile::Grass });
            }
        }
        m.set(28, 4, Tile::Altar(3));
        m.set(29, 4, Tile::Altar(3));
        m.set(13, 20, Tile::Altar(0));
        m.set(44, 20, Tile::Altar(1));
        m.set(30, 28, Tile::Altar(2));
        m.seal_unreachable(ISLE_ARRIVAL, Tile::Tree);
        m
    }

    fn new(id: MapId, w: i32, h: i32, tiles: Vec<Tile>, exits: Vec<Exit>) -> Map {
        Map { id, w, h, tiles, exits, picked: HashSet::new(), occupied: vec![false; (w * h) as usize] }
    }

    /// Two side-by-side exit tiles leading to two side-by-side arrival tiles.
    fn exit_pair(a: (i32, i32), b: (i32, i32), to: MapId, ta: (i32, i32), tb: (i32, i32)) -> [Exit; 2] {
        [Exit { x: a.0, y: a.1, to, tx: ta.0, ty: ta.1 }, Exit { x: b.0, y: b.1, to, tx: tb.0, ty: tb.1 }]
    }

    /// Turn every walkable tile not reachable from `start` into `wall`.
    fn seal_unreachable(&mut self, start: (i32, i32), wall: Tile) {
        let reach = self.reachable(start.0, start.1);
        for (i, t) in self.tiles.iter_mut().enumerate() {
            if !t.solid() && !reach[i] {
                *t = wall;
            }
        }
    }

    fn woods() -> Map {
        let (w, h) = (80, 56);
        let mut tiles = vec![Tile::Grass; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let edge = x.min(y).min(w - 1 - x).min(h - 1 - y) as f32;
                let n = fbm(x as f32 / 10.0, y as f32 / 10.0, 201);
                let g = fbm(x as f32 / 6.0, y as f32 / 6.0, 211);
                let r = hash(x, y, 221) % 1000;
                tiles[(y * w + x) as usize] = if edge < 2.0 + vnoise(x as f32 / 3.0, y as f32 / 3.0, 231) * 3.0 || n > 0.56 {
                    Tile::Tree
                } else if n < 0.2 {
                    Tile::Water
                } else if g > 0.62 {
                    Tile::TallGrass
                } else if r < 22 {
                    Tile::Mushroom
                } else if r < 40 {
                    Tile::Flower
                } else if r < 48 {
                    Tile::Rock
                } else if r < 54 {
                    Tile::Bush((hash(x, y, 241) % 5) as u8)
                } else {
                    Tile::Grass
                };
            }
        }
        let hub = (40, 28);
        carve(&mut tiles, w, h, (40, 55), hub, Tile::Path, 251);
        carve(&mut tiles, w, h, (40, 8), hub, Tile::Path, 261);
        // A rocky outcrop with the mouth of the Kernel Caverns.
        for y in 4..=7 {
            for x in 37..=44 {
                tiles[(y * w + x) as usize] = if y == 7 && (40..=41).contains(&x) { Tile::CaveMouth } else { Tile::Rock };
            }
        }
        carve(&mut tiles, w, h, hub, (13, 40), Tile::Grass, 271);
        carve(&mut tiles, w, h, hub, (67, 11), Tile::Grass, 281);
        let mut exits = Vec::new();
        exits.extend(Map::exit_pair((40, 55), (41, 55), MapId::Meadow, (48, 1), (49, 1)));
        exits.extend(Map::exit_pair((40, 7), (41, 7), MapId::Caves, (35, 47), (36, 47)));
        let mut m = Map::new(MapId::Woods, w, h, tiles, exits);
        // A quiet glade with a Lost Page...
        for y in 37..=44 {
            for x in 8..=15 {
                m.set(x, y, if hash(x, y, 291) % 4 == 0 { Tile::Flower } else { Tile::Grass });
            }
        }
        m.set(11, 40, Tile::Page);
        // ...and the Thinking Shrine, ringed with flowers, hidden in the north-east.
        for y in 8..=15 {
            for x in 65..=74 {
                let ring = (x - 70i32).abs().max((y - 11i32).abs()) == 2;
                m.set(x, y, if ring { Tile::Flower } else { Tile::Grass });
            }
        }
        m.set(70, 11, Tile::Shrine);
        m.seal_unreachable((40, 54), Tile::Tree);
        m
    }

    fn shore() -> Map {
        let (w, h) = (90, 50);
        let coast = |y: i32| 54 + (fbm(0.5, y as f32 / 9.0, 301) * 16.0) as i32;
        let mut tiles = vec![Tile::Grass; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let c = coast(y);
                let edge = x.min(y).min(h - 1 - y) as f32;
                let r = hash(x, y, 311) % 1000;
                let pool = fbm(x as f32 / 5.0, y as f32 / 5.0, 321) > 0.8;
                tiles[(y * w + x) as usize] = if x >= c {
                    Tile::Water
                } else if edge < 2.0 + vnoise(x as f32 / 3.0, y as f32 / 3.0, 331) * 2.5 {
                    if x > c - 12 { Tile::Palm } else { Tile::Tree }
                } else if x >= c - 11 {
                    match r {
                        _ if pool && x < c - 2 => Tile::Water,
                        0..=24 => Tile::Shell,
                        25..=44 => Tile::Palm,
                        45..=54 => Tile::Rock,
                        _ => Tile::Sand,
                    }
                } else if x >= c - 14 {
                    if r < 500 { Tile::Sand } else if r < 540 { Tile::Palm } else { Tile::Grass }
                } else {
                    let g = fbm(x as f32 / 6.0, y as f32 / 6.0, 341);
                    match r {
                        _ if g > 0.64 => Tile::TallGrass,
                        0..=29 => Tile::Flower,
                        30..=44 => Tile::Palm,
                        45..=52 => Tile::Bush((hash(x, y, 351) % 5) as u8),
                        53..=58 => Tile::Rock,
                        _ => Tile::Grass,
                    }
                };
            }
        }
        carve(&mut tiles, w, h, (0, 25), (coast(25) - 6, 25), Tile::Path, 361);
        carve(&mut tiles, w, h, (coast(25) - 6, 25), (coast(8) - 6, 8), Tile::Sand, 371);
        carve(&mut tiles, w, h, (coast(25) - 6, 25), (coast(42) - 6, 42), Tile::Sand, 381);
        // The old lighthouse on a sandy point, with room for Keeper Ada beside it.
        let lx = coast(9) - 5;
        carve(&mut tiles, w, h, (coast(8) - 6, 8), (lx, 11), Tile::Sand, 391);
        let exits = Map::exit_pair((0, 25), (0, 26), MapId::Meadow, (94, 20), (94, 21)).into();
        let mut m = Map::new(MapId::Shore, w, h, tiles, exits);
        for y in 4..=12 {
            for x in lx - 2..=lx + 4 {
                m.set(x, y, Tile::Sand);
            }
        }
        for y in 6..=9 {
            for x in lx..=lx + 1 {
                m.set(x, y, Tile::Lighthouse);
            }
        }
        // A Lost Page by the southern tide pool.
        let px = coast(42) - 6;
        m.set(px + 2, 42, Tile::Page);
        m.set(px + 3, 42, Tile::Water);
        m.seal_unreachable((1, 25), Tile::Palm);
        m
    }

    fn caves() -> Map {
        let (w, h) = (70, 50);
        let mut tiles = vec![Tile::CaveWall; (w * h) as usize];
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let n = fbm(x as f32 / 7.0, y as f32 / 7.0, 401);
                tiles[(y * w + x) as usize] = if n <= 0.47 {
                    Tile::CaveWall
                } else if fbm(x as f32 / 9.0, y as f32 / 9.0, 411) > 0.72 {
                    Tile::Lava
                } else if fbm(x as f32 / 6.0, y as f32 / 6.0, 421) > 0.78 {
                    Tile::Water
                } else if hash(x, y, 431) % 70 == 0 {
                    Tile::Rock
                } else {
                    Tile::CaveFloor
                };
            }
        }
        let hub = (35, 24);
        carve(&mut tiles, w, h, (35, 48), hub, Tile::CaveFloor, 441);
        for (i, far) in [(10, 8), (58, 9), (9, 40), (60, 40)].into_iter().enumerate() {
            carve(&mut tiles, w, h, hub, far, Tile::CaveFloor, 451 + i as u32 * 10);
        }
        for x in 35..=36 {
            tiles[(49 * w + x) as usize] = Tile::CaveMouth;
        }
        let exits = Map::exit_pair((35, 49), (36, 49), MapId::Woods, (40, 8), (41, 8)).into();
        let mut m = Map::new(MapId::Caves, w, h, tiles, exits);
        m.seal_unreachable((35, 48), Tile::CaveWall);
        m.set(60, 9, Tile::Page);
        // Crystals grow on walls facing open floor.
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = (y * w + x) as usize;
                if m.tiles[i] == Tile::CaveWall && m.get(x, y + 1) == Tile::CaveFloor && hash(x, y, 461) % 9 == 0 {
                    m.tiles[i] = Tile::Crystal;
                }
            }
        }
        m
    }

    fn town() -> Map {
        let w = TOWN[0].chars().count() as i32;
        let h = TOWN.len() as i32;
        let tiles = TOWN.iter().flat_map(|r| r.chars().chain(std::iter::repeat('T')).take(w as usize).map(Tile::from_char)).collect();
        let exits = Map::exit_pair((23, 0), (24, 0), MapId::Meadow, (48, 62), (49, 62)).into();
        Map::new(MapId::Town, w, h, tiles, exits)
    }

    /// Ambient light multiplier for this map at a given hour.
    pub fn ambient(&self, hour: f32) -> [f32; 3] {
        let l = crate::clock::light(hour);
        match self.id {
            MapId::Woods => [l[0] * 0.82, l[1] * 0.9, l[2] * 0.84],
            MapId::Caves => [0.5, 0.48, 0.6],
            // Mythos Isle sits in a permanent, magical dusk.
            MapId::Island => [l[0] * 0.75 + 0.1, l[1] * 0.7 + 0.1, l[2] * 0.75 + 0.22],
            _ => l,
        }
    }

    /// Light emitted by the tile at (x, y): radius in pixels and colour.
    pub fn light_at(&self, x: i32, y: i32) -> Option<(f32, Rgb)> {
        match self.get(x, y) {
            Tile::Crystal if !self.picked.contains(&(x, y)) => Some((18.0, Rgb(110, 240, 255))),
            Tile::Crystal => Some((8.0, Rgb(80, 160, 190))),
            Tile::Lava => Some((24.0, Rgb(255, 130, 50))),
            Tile::Lamp => Some((22.0, Rgb(255, 215, 150))),
            Tile::Mushroom if !self.picked.contains(&(x, y)) => Some((7.0, Rgb(120, 255, 200))),
            Tile::CaveMouth if self.id == MapId::Caves => Some((34.0, Rgb(220, 240, 210))),
            Tile::Page if !self.picked.contains(&(x, y)) => Some((14.0, Rgb(255, 230, 160))),
            Tile::Shrine => Some((18.0, Rgb(255, 170, 120))),
            Tile::Altar(k) => Some((16.0, ALTAR_COLORS[k as usize % 4])),
            _ => None,
        }
    }

    /// Garden size for a land-expansion level: the forest is cleared east, then south, then east again.
    pub fn garden_size(land: u8) -> (i32, i32) {
        match land {
            0 => (40, 22),
            1 => (52, 22),
            2 => (52, 32),
            _ => (64, 32),
        }
    }

    fn garden(land: u8) -> Map {
        let (w, h) = Map::garden_size(land);
        let core_w = GARDEN.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32;
        let core_h = GARDEN.len() as i32;
        let mut tiles = vec![Tile::Tree; (w * h) as usize];
        // Cleared land beyond the original plot: grass with a sprinkle of nature.
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let r = hash(x, y, 101) % 100;
                let t = match r {
                    0..=3 => Tile::Flower,
                    4..=8 if vnoise(x as f32 / 5.0, y as f32 / 5.0, 7) > 0.6 => Tile::TallGrass,
                    9 => Tile::Rock,
                    _ => Tile::Grass,
                };
                tiles[(y * w + x) as usize] = t;
            }
        }
        // Stamp the hand-made core; its east/south borders vanish once that land is bought.
        for (y, row) in GARDEN.iter().enumerate() {
            let y = y as i32;
            for (x, ch) in row.chars().chain(std::iter::repeat('T')).take(core_w as usize).enumerate() {
                let x = x as i32;
                let open_east = w > core_w && x >= core_w - 3 && (2..h - 2).contains(&y);
                let open_south = h > core_h && y >= core_h - 3 && (2..w - 2).contains(&x);
                if ch == 'T' && (open_east || open_south) {
                    continue;
                }
                tiles[(y * w + x) as usize] = Tile::from_char(ch);
            }
        }
        // The main path always runs to the east edge, where the meadow exit is.
        for x in core_w - 3..w {
            tiles[(8 * w + x) as usize] = Tile::Path;
        }
        Map {
            id: MapId::Garden,
            w,
            h,
            tiles,
            exits: vec![Exit { x: w - 1, y: 8, to: MapId::Meadow, tx: 1, ty: MEADOW_ENTRY_Y }],
            picked: HashSet::new(),
            occupied: vec![false; (w * h) as usize],
        }
    }

    /// A large, deterministic, procedurally generated meadow.
    fn meadow() -> Map {
        let (w, h) = (96, 64);
        let mut tiles = vec![Tile::Grass; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let edge = x.min(y).min(w - 1 - x).min(h - 1 - y) as f32;
                let wobble = vnoise(x as f32 / 4.0, y as f32 / 4.0, 91) * 3.0;
                let n = fbm(x as f32 / 14.0, y as f32 / 14.0, 11);
                let g = fbm(x as f32 / 7.0, y as f32 / 7.0, 23);
                let r = hash(x, y, 37) % 1000;
                let t = if edge < 2.0 + wobble {
                    Tile::Tree
                } else if n < 0.30 {
                    Tile::Water
                } else if n < 0.34 {
                    Tile::Sand
                } else if n > 0.68 {
                    Tile::Tree
                } else if g > 0.60 {
                    Tile::TallGrass
                } else if r < 25 {
                    Tile::Flower
                } else if r < 33 {
                    Tile::Rock
                } else if r < 41 {
                    Tile::Bush((hash(x, y, 41) % 5) as u8)
                } else {
                    Tile::Grass
                };
                tiles[(y * w + x) as usize] = t;
            }
        }
        // Carve a meandering path from the west entrance, bridging any water.
        let mut py = MEADOW_ENTRY_Y;
        for x in 0..w - 6 {
            for yy in [py, py + 1] {
                let i = (yy * w + x) as usize;
                tiles[i] = if tiles[i] == Tile::Water { Tile::Bridge } else { Tile::Path };
            }
            if x > 4 && hash(x, 0, 53) % 5 == 0 {
                let dy = if hash(x, 1, 59) % 2 == 0 { 1 } else { -1 };
                py = (py + dy).clamp(8, h - 10);
                let i = (py * w + x) as usize;
                tiles[i] = if tiles[i] == Tile::Water { Tile::Bridge } else { Tile::Path };
            }
        }
        // Roads to the other areas, all meeting in the middle of the meadow.
        let hub = (48, 32);
        carve(&mut tiles, w, h, (48, 0), hub, Tile::Path, 61);
        carve(&mut tiles, w, h, (95, 20), hub, Tile::Path, 71);
        carve(&mut tiles, w, h, (48, 63), hub, Tile::Path, 81);
        let mut exits = Vec::new();
        // The garden end depends on its size; a negative x counts from the garden's east edge.
        exits.extend(Map::exit_pair((0, MEADOW_ENTRY_Y), (0, MEADOW_ENTRY_Y + 1), MapId::Garden, (-2, 8), (-2, 8)));
        exits.extend(Map::exit_pair((48, 0), (49, 0), MapId::Woods, (40, 54), (41, 54)));
        exits.extend(Map::exit_pair((95, 20), (95, 21), MapId::Shore, (1, 25), (1, 26)));
        exits.extend(Map::exit_pair((48, 63), (49, 63), MapId::Town, (23, 1), (24, 1)));
        let mut m = Map::new(MapId::Meadow, w, h, tiles, exits);
        // Anything unreachable from the entrance becomes forest, so creatures never spawn in sealed pockets.
        m.seal_unreachable((0, MEADOW_ENTRY_Y), Tile::Tree);
        m
    }

    fn reachable(&self, sx: i32, sy: i32) -> Vec<bool> {
        let mut seen = vec![false; self.tiles.len()];
        let mut stack = vec![(sx, sy)];
        while let Some((x, y)) = stack.pop() {
            if x < 0 || y < 0 || x >= self.w || y >= self.h {
                continue;
            }
            let i = (y * self.w + x) as usize;
            if seen[i] || self.tiles[i].solid() {
                continue;
            }
            seen[i] = true;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        seen
    }

    pub fn get(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            Tile::Tree
        } else {
            self.tiles[(y * self.w + x) as usize]
        }
    }

    /// Solid terrain or a placed object.
    pub fn blocked(&self, x: i32, y: i32) -> bool {
        self.get(x, y).solid() || (self.inside(x, y) && self.occupied[(y * self.w + x) as usize])
    }

    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    /// Mark the footprints of placed objects as blocked.
    pub fn set_objects(&mut self, objs: &[(i32, i32, i32, i32)]) {
        self.occupied.iter_mut().for_each(|o| *o = false);
        for &(x, y, w, h) in objs {
            for yy in y..y + h {
                for xx in x..x + w {
                    if self.inside(xx, yy) {
                        self.occupied[(yy * self.w + xx) as usize] = true;
                    }
                }
            }
        }
    }

    pub fn near_water(&self, x: i32, y: i32) -> bool {
        [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| self.get(x + dx, y + dy) == Tile::Water)
    }

    pub fn exit_at(&self, x: i32, y: i32) -> Option<&Exit> {
        self.exits.iter().find(|e| e.x == x && e.y == y)
    }

    pub fn draw(&self, s: &mut Screen, cam_x: i32, cam_y: i32, season: Season, t: f32) {
        let tx0 = cam_x.div_euclid(TILE);
        let ty0 = cam_y.div_euclid(TILE);
        let tx1 = (cam_x + s.w as i32) / TILE + 1;
        let ty1 = (cam_y + s.h as i32) / TILE + 1;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let px = tx * TILE - cam_x;
                let py = ty * TILE - cam_y;
                draw_tile(s, self, tx, ty, px, py, season, t);
            }
        }
    }
}

const TOWN: &[&str] = &[
    "TTTTTTTTTTTTTTTTTTTTTTT==TTTTTTTTTTTTTTTTTTTTTTT",
    "TTTTTTTTTTTTTTTTTTTTTTT==TTTTTTTTTTTTTTTTTTTTTTT",
    "TT.....................==.....................TT",
    "TT.*.LLLLLL............==...........SSSSS.....TT",
    "TT...LLLLLL..*....*....==.....*.....SSSSS...*.TT",
    "TT...LLLLLL............==....*......SSSSS.....TT",
    "TT...LLLLLL............==...........SSSSS.....TT",
    "TT.....==..............==.............==......TT",
    "TT.....==...i.........i==i.........i..==......TT",
    "TT...======================================..0TT",
    "TT...======================================...TT",
    "TT..............i==============i..............TT",
    "TT.*.............==============...T.........*.TT",
    "TT........\"..T...====~~~~~~====......\"........TT",
    "TT0...........*..====~~~~~~====..*............TT",
    "TT.......*.......====~~~~~~====........*......TT",
    "TT..T............==============............T..TT",
    "TT..............i==============i..............TT",
    "TT....\"................==................\"....TT",
    "TT.....................==.....................TT",
    "TT..*.......*..........==..........*..........TT",
    "TT.....................==..................*..TT",
    "TT......T..............==.....................TT",
    "TT.................AAAAAAAAAA.................TT",
    "TT.................AAAAAAAAAA..........T......TT",
    "TT.............T...AAAAAAAAAA...T.............TT",
    "TT.................AAAAAAAAAA.................TT",
    "TT.................AAAAAAAAAA.................TT",
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
    "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT",
];

/// Dig a 2-tile-wide winding corridor from `from` to `to`, bridging water when the floor is a path.
fn carve(tiles: &mut [Tile], w: i32, h: i32, from: (i32, i32), to: (i32, i32), floor: Tile, salt: u32) {
    let (mut x, mut y) = from;
    for i in 0..5000 {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let (xx, yy) = (x + dx, y + dy);
            if xx < 0 || yy < 0 || xx >= w || yy >= h {
                continue;
            }
            let t = &mut tiles[(yy * w + xx) as usize];
            *t = if *t == Tile::Water && floor == Tile::Path { Tile::Bridge } else { floor };
        }
        if (x, y) == to {
            break;
        }
        let (ddx, ddy) = (to.0 - x, to.1 - y);
        let r = hash(i, 0, salt) % 10;
        let jitter = if hash(i, 1, salt) % 2 == 0 { 1 } else { -1 };
        if r < 2 && ddx.abs() + ddy.abs() > 4 {
            if ddx.abs() > ddy.abs() {
                y = (y + jitter).clamp(1, h - 3);
            } else {
                x = (x + jitter).clamp(1, w - 3);
            }
        } else if ddx.abs() > ddy.abs() {
            x += ddx.signum();
        } else {
            y += ddy.signum();
        }
    }
}

/// Smooth value noise in 0..1.
fn vnoise(x: f32, y: f32, salt: u32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let sm = |t: f32| t * t * (3.0 - 2.0 * t);
    let v = |xx: i32, yy: i32| (hash(xx, yy, salt) % 1024) as f32 / 1023.0;
    let a = v(x0, y0) + (v(x0 + 1, y0) - v(x0, y0)) * sm(fx);
    let b = v(x0, y0 + 1) + (v(x0 + 1, y0 + 1) - v(x0, y0 + 1)) * sm(fx);
    a + (b - a) * sm(fy)
}

fn fbm(x: f32, y: f32, salt: u32) -> f32 {
    (vnoise(x, y, salt) * 0.6 + vnoise(x * 2.0, y * 2.0, salt + 1) * 0.3 + vnoise(x * 4.0, y * 4.0, salt + 2) * 0.1).clamp(0.0, 1.0)
}

/// Cheap deterministic hash for per-pixel texture noise.
pub fn hash(x: i32, y: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ salt.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^ (h >> 16)
}

pub fn grass_cols(season: Season) -> (Rgb, Rgb, Rgb) {
    match season {
        Season::Spring => (Rgb(110, 190, 90), Rgb(95, 172, 80), Rgb(140, 210, 110)),
        Season::Summer => (Rgb(86, 168, 70), Rgb(72, 150, 60), Rgb(118, 190, 88)),
        Season::Autumn => (Rgb(150, 160, 80), Rgb(132, 140, 66), Rgb(178, 172, 96)),
        Season::Winter => (Rgb(226, 234, 242), Rgb(206, 216, 230), Rgb(244, 248, 252)),
    }
}

fn leaf_cols(season: Season, var: u32) -> (Rgb, Rgb, Rgb) {
    match season {
        Season::Spring if var % 3 == 0 => (Rgb(240, 170, 200), Rgb(214, 130, 170), Rgb(255, 214, 230)),
        Season::Spring | Season::Summer => (Rgb(50, 130, 60), Rgb(36, 100, 48), Rgb(80, 160, 76)),
        Season::Autumn => match var % 3 {
            0 => (Rgb(214, 110, 40), Rgb(170, 76, 30), Rgb(240, 150, 60)),
            1 => (Rgb(200, 60, 40), Rgb(150, 40, 30), Rgb(230, 100, 60)),
            _ => (Rgb(220, 170, 50), Rgb(180, 130, 30), Rgb(246, 206, 90)),
        },
        Season::Winter => (Rgb(60, 100, 80), Rgb(44, 76, 62), Rgb(236, 242, 250)),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tile(s: &mut Screen, m: &Map, tx: i32, ty: i32, px: i32, py: i32, season: Season, t: f32) {
    let tile = m.get(tx, ty);
    let (g0, g1, g2) = grass_cols(season);
    let wx = tx * TILE;
    let wy = ty * TILE;
    let picked = m.picked.contains(&(tx, ty));
    let fill = |s: &mut Screen, f: &dyn Fn(i32, i32) -> Rgb| {
        for y in 0..TILE {
            for x in 0..TILE {
                s.set(px + x, py + y, f(x, y));
            }
        }
    };
    let sand = |x: i32, y: i32| if hash(wx + x, wy + y, 3) % 9 == 0 { Rgb(220, 196, 140) } else { Rgb(236, 214, 160) };
    let cave_floor = |x: i32, y: i32| match hash(wx + x, wy + y, 9) % 13 {
        0 => Rgb(96, 88, 104),
        1 | 2 => Rgb(62, 56, 70),
        _ => Rgb(76, 70, 86),
    };
    // Base ground
    match tile {
        Tile::Sand | Tile::Shell | Tile::Palm | Tile::Lighthouse => fill(s, &sand),
        Tile::CaveFloor | Tile::CaveWall | Tile::Crystal | Tile::Lava => fill(s, &cave_floor),
        Tile::CaveMouth if m.id == MapId::Caves => fill(s, &cave_floor),
        _ => fill(s, &|x, y| match hash(wx + x, wy + y, 1) % 16 {
            0 | 1 => g1,
            2 => g2,
            _ => g0,
        }),
    }
    match tile {
        Tile::Grass | Tile::Sand | Tile::CaveFloor => {}
        Tile::TallGrass => {
            let sway = ((t * 2.0 + tx as f32 * 0.7).sin() * 1.2).round() as i32;
            for i in 0..4 {
                let bx = (i * 2 + (hash(tx, ty, i as u32) % 2) as i32) % TILE;
                for h in 0..5 {
                    let off = if h < 2 { sway } else { 0 };
                    let c = if h == 0 { g2 } else { g1.lerp(Rgb(30, 70, 30), 0.35) };
                    s.set(px + bx + off, py + 7 - h - (i % 2), c);
                }
            }
        }
        Tile::Flower => {
            let petals = [Rgb(240, 90, 110), Rgb(250, 210, 80), Rgb(170, 130, 240), Rgb(255, 255, 255)];
            for i in 0..3u32 {
                let fx = 1 + (hash(tx, ty, i + 10) % 6) as i32;
                let fy = 1 + (hash(tx, ty, i + 20) % 6) as i32;
                let c = petals[(hash(tx, ty, i + 30) % 4) as usize];
                s.set(px + fx, py + fy - 1, c);
                s.set(px + fx - 1, py + fy, c);
                s.set(px + fx + 1, py + fy, c);
                s.set(px + fx, py + fy, Rgb(250, 200, 60));
                s.set(px + fx, py + fy + 1, Rgb(60, 120, 50));
            }
        }
        Tile::Path => fill(s, &|x, y| match hash(wx + x, wy + y, 2) % 10 {
            0 => Rgb(170, 140, 100),
            1 | 2 => Rgb(205, 175, 128),
            _ => Rgb(192, 162, 116),
        }),
        Tile::Water => {
            let cave = m.id == MapId::Caves;
            for y in 0..TILE {
                for x in 0..TILE {
                    let wave = ((wx + x) as f32 * 0.4 + (wy + y) as f32 * 0.6 + t * 2.5).sin();
                    let mut c = if cave { Rgb(30, 60, 110).lerp(Rgb(50, 90, 150), (wave + 1.0) * 0.3) } else { Rgb(58, 120, 200).lerp(Rgb(80, 150, 220), (wave + 1.0) * 0.3) };
                    if season == Season::Winter && !cave {
                        c = c.lerp(Rgb(200, 225, 245), 0.6);
                    }
                    if wave > 0.92 && hash(wx + x, wy + y, 4) % 3 == 0 {
                        c = Rgb(200, 230, 255);
                    }
                    // Foam where water meets land above or to the left.
                    let foam = (y == 0 && m.get(tx, ty - 1) != Tile::Water) || (x == 0 && m.get(tx - 1, ty) != Tile::Water && ((t * 3.0 + y as f32).sin() > -0.3));
                    if foam {
                        c = Rgb(170, 210, 240);
                    }
                    s.set(px + x, py + y, c);
                }
            }
        }
        Tile::Soil => fill(s, &|x, y| {
            if y % 3 == 0 {
                Rgb(96, 62, 40)
            } else if hash(wx + x, wy + y, 5) % 7 == 0 {
                Rgb(130, 90, 60)
            } else {
                Rgb(116, 78, 50)
            }
        }),
        Tile::Fence => {
            let wood = Rgb(170, 120, 70);
            let dark = Rgb(120, 80, 45);
            let h = m.get(tx - 1, ty) == Tile::Fence || m.get(tx + 1, ty) == Tile::Fence;
            let v = m.get(tx, ty - 1) == Tile::Fence || m.get(tx, ty + 1) == Tile::Fence;
            if h {
                s.fill_rect(px, py + 3, TILE, 1, wood);
                s.fill_rect(px, py + 5, TILE, 1, dark);
            }
            if v || !h {
                s.fill_rect(px + 3, py, 2, TILE, wood);
            }
            s.fill_rect(px + 3, py + 1, 2, 6, wood);
            s.set(px + 4, py + 6, dark);
        }
        Tile::Rock => {
            let rock = ["........", "..####..", ".#hhh##.", "#hh####d", "#h####dd", "#####ddd", ".#dddd#.", "........"];
            sprite_rows(s, px, py, &rock, &|c| match c {
                '#' => Some(Rgb(140, 140, 150)),
                'h' => Some(Rgb(185, 185, 195)),
                'd' => Some(Rgb(100, 100, 112)),
                _ => None,
            });
        }
        Tile::Tree if m.id == MapId::Woods && hash(tx, ty, 17) % 2 == 0 => {
            // Pines in the Latent Woods.
            let (dark, mid) = (Rgb(30, 86, 60), Rgb(44, 112, 74));
            let snow = season == Season::Winter;
            for y in 0..7 {
                let half = (y + 2) / 2;
                for x in 4 - half..4 + half {
                    let c = if snow && y % 3 == 0 { Rgb(236, 242, 250) } else if x < 4 { mid } else { dark };
                    s.set(px + x, py + y, c);
                }
            }
            s.fill_rect(px + 3, py + 7, 2, 1, Rgb(100, 66, 40));
        }
        Tile::Tree => {
            let (l0, l1, l2) = leaf_cols(season, hash(tx, ty, 7));
            // Round canopy filling the tile, with trunk peeking at the bottom.
            for y in 0..TILE {
                for x in 0..TILE {
                    let dx = x as f32 - 3.5;
                    let dy = y as f32 - 3.2;
                    let d = (dx * dx + dy * dy).sqrt();
                    if y >= 6 && (3..=4).contains(&x) {
                        s.set(px + x, py + y, Rgb(110, 72, 44));
                    } else if d < 3.9 {
                        let n = hash(wx + x, wy + y, 8) % 6;
                        let c = if dx + dy < -2.5 || n == 0 { l2 } else if dx + dy > 2.0 { l1 } else { l0 };
                        s.set(px + x, py + y, c);
                    }
                }
            }
        }
        Tile::Palm => {
            let palm = ["gg.G.gg.", ".gGGGg..", "g.gtg.g.", "...t....", "...tt...", "....t...", "....t...", "...ttt.."];
            sprite_rows(s, px, py, &palm, &|c| match c {
                'g' => Some(Rgb(60, 150, 70)),
                'G' => Some(Rgb(100, 190, 90)),
                't' => Some(Rgb(150, 105, 60)),
                _ => None,
            });
        }
        Tile::Shell if !picked => {
            let shell = ["........", "........", "...pp...", "..pPPp..", ".pPwPPp.", ".pPPwPp.", "..pppp..", "........"];
            sprite_rows(s, px, py, &shell, &|c| match c {
                'p' => Some(Rgb(236, 150, 160)),
                'P' => Some(Rgb(250, 200, 200)),
                'w' => Some(Rgb(255, 245, 240)),
                _ => None,
            });
        }
        Tile::Shell => {}
        Tile::Mushroom => {
            let art: [&str; 8] = if picked {
                ["........", "........", "........", "........", "........", "...ss...", "...ss...", "........"]
            } else {
                ["........", "..rrrr..", ".rwrrwr.", "rrrrrrrr", "...ss...", "...ss...", "..ssss..", "........"]
            };
            let glow = 0.5 + (t * 2.0 + tx as f32).sin() * 0.5;
            sprite_rows(s, px, py, &art, &|c| match c {
                'r' => Some(Rgb(70, 200, 170).lerp(Rgb(140, 255, 220), glow)),
                'w' => Some(Rgb(240, 255, 250)),
                's' => Some(Rgb(230, 220, 200)),
                _ => None,
            });
        }
        Tile::CaveWall | Tile::Crystal => {
            let open_below = !m.get(tx, ty + 1).solid();
            fill(s, &|x, y| {
                let n = hash(wx + x, wy + y, 11) % 11;
                let base = if n == 0 { Rgb(58, 52, 66) } else if n < 3 { Rgb(36, 32, 44) } else { Rgb(46, 41, 55) };
                // Lit rock face where the wall meets open floor.
                if open_below && y >= 5 { base.lerp(Rgb(110, 100, 120), 0.4 + (y - 5) as f32 * 0.1) } else { base }
            });
            if tile == Tile::Crystal {
                let shine = if picked { 0.0 } else { 0.5 + (t * 1.5 + tx as f32 * 0.9).sin() * 0.5 };
                let c = if picked { Rgb(70, 110, 130) } else { Rgb(90, 210, 240).lerp(Rgb(200, 255, 255), shine) };
                for (x, y0) in [(2, 3), (4, 1), (5, 4)] {
                    for y in y0..7 {
                        s.set(px + x, py + y, c);
                    }
                    s.set(px + x - 1, py + 6, c.lerp(Rgb(40, 60, 80), 0.4));
                }
            }
        }
        Tile::Lava => fill(s, &|x, y| {
            let f = ((wx + x) as f32 * 0.5 + (t * 1.7) + ((wy + y) as f32 * 0.3).sin() * 2.0).sin();
            let crust = hash(wx + x, wy + y, 13) % 17 == 0;
            if crust { Rgb(90, 40, 30) } else { Rgb(220, 70, 30).lerp(Rgb(255, 200, 80), (f + 1.0) * 0.5) }
        }),
        Tile::CaveMouth => {
            if m.id == MapId::Caves {
                // Daylight spilling in from the woods above.
                fill(s, &|_, y| Rgb(150, 190, 140).lerp(Rgb(76, 70, 86), y as f32 / 8.0));
            } else {
                fill(s, &|x, y| {
                    let n = hash(wx + x, wy + y, 15) % 9;
                    let rock = if n == 0 { Rgb(110, 110, 122) } else { Rgb(140, 140, 150) };
                    let lx = if tx % 2 == 0 { x } else { x + TILE }; // the arch spans two tiles
                    let d = ((lx as f32 - 7.5).powi(2) / 49.0 + (y as f32 - 8.0).powi(2) / 49.0).sqrt();
                    if d < 1.0 { Rgb(12, 10, 16) } else { rock }
                });
            }
        }
        Tile::Lamp => {
            let lamp = ["..kkkk..", ".kYYYYk.", ".kYWWYk.", "..kkkk..", "...kk...", "...kk...", "...kk...", "..kkkk.."];
            sprite_rows(s, px, py, &lamp, &|c| match c {
                'k' => Some(Rgb(50, 50, 62)),
                'Y' => Some(Rgb(255, 220, 130)),
                'W' => Some(Rgb(255, 250, 230)),
                _ => None,
            });
        }
        Tile::House | Tile::Lab | Tile::Shop | Tile::Arena => draw_building_part(s, m, tx, ty, px, py, tile),
        Tile::Lighthouse => {
            let (ox, oy, bw, bh) = building_bounds(m, tx, ty);
            let (lx, ly) = ((tx - ox) * TILE, (ty - oy) * TILE);
            let (w, h) = (bw * TILE, bh * TILE);
            for y in 0..TILE {
                for x in 0..TILE {
                    let (gx, gy) = (lx + x, ly + y);
                    // Tapering tower: wider at the base.
                    let inset = 3 - gy * 3 / h;
                    if gx < inset || gx >= w - inset {
                        continue;
                    }
                    let c = if gy < 4 {
                        if gy == 0 || gx == inset || gx == w - inset - 1 { Rgb(60, 56, 70) } else { Rgb(90, 86, 104) } // lamp room
                    } else if gy < 6 {
                        Rgb(60, 56, 70) // gallery
                    } else if gy >= h - 6 && (w / 2 - 2..w / 2 + 2).contains(&gx) {
                        Rgb(110, 76, 52) // door
                    } else if (gy / 5) % 2 == 0 {
                        Rgb(220, 70, 60)
                    } else {
                        Rgb(245, 242, 236)
                    };
                    s.set(px + x, py + y, c);
                }
            }
        }
        Tile::Page => {
            let art: [&str; 8] = if picked {
                ["........", "........", "........", "........", "..kkkk..", "...kk...", "...kk...", "..kkkk.."]
            } else {
                ["........", "..pppp..", "..pPPp..", "..pppp..", "..kkkk..", "...kk...", "...kk...", "..kkkk.."]
            };
            let bob = if !picked && (t * 2.0).sin() > 0.3 { -1 } else { 0 };
            for (y, row) in art.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let c = match ch {
                        'p' => Rgb(250, 240, 210),
                        'P' => Rgb(150, 120, 90),
                        'k' => Rgb(110, 80, 56),
                        _ => continue,
                    };
                    let dy = if ch == 'p' || ch == 'P' { bob } else { 0 };
                    s.set(px + x as i32, py + y as i32 + dy, c);
                }
            }
        }
        Tile::Shrine => {
            let shrine = ["..kkkk..", ".kKKKKk.", "kKsosoKk", "kKoOOoKk", "kKsosoKk", "kKKKKKKk", ".kkkkkk.", "kkkkkkkk"];
            let glow = (t * 1.5).sin() * 0.5 + 0.5;
            sprite_rows(s, px, py, &shrine, &|c| match c {
                'k' => Some(Rgb(90, 88, 100)),
                'K' => Some(Rgb(130, 128, 140)),
                'o' => Some(Rgb(218, 119, 86).lerp(Rgb(255, 220, 180), glow)),
                'O' => Some(Rgb(255, 240, 220)),
                's' => Some(Rgb(150, 148, 160)),
                _ => None,
            });
        }
        Tile::Altar(k) => {
            let altar = ["...gg...", "..gGGg..", "...gg...", "..kkkk..", "..kKKk..", "..kKKk..", ".kkkkkk.", "kkkkkkkk"];
            let gem = ALTAR_COLORS[k as usize % 4];
            let glow = (t * 2.0 + k as f32).sin() * 0.5 + 0.5;
            sprite_rows(s, px, py - if glow > 0.5 { 1 } else { 0 }, &altar[..3], &|c| match c {
                'g' => Some(gem),
                'G' => Some(gem.lerp(Rgb(255, 255, 255), 0.4 + glow * 0.4)),
                _ => None,
            });
            sprite_rows(s, px, py + 3, &altar[3..], &|c| match c {
                'k' => Some(Rgb(150, 146, 170)),
                'K' => Some(Rgb(196, 192, 214)),
                _ => None,
            });
        }
        Tile::Bridge => fill(s, &|x, y| {
            if (wy + y) % 3 == 0 {
                Rgb(130, 90, 55)
            } else if hash(wx + x, wy + y, 6) % 9 == 0 {
                Rgb(150, 105, 65)
            } else {
                Rgb(175, 128, 80)
            }
        }),
        Tile::Bush(f) => {
            let (l0, l1, l2) = leaf_cols(if season == Season::Autumn { Season::Summer } else { season }, 1);
            for y in 1..TILE {
                for x in 0..TILE {
                    let dx = x as f32 - 3.5;
                    let dy = y as f32 - 4.5;
                    if dx * dx + dy * dy * 1.4 < 15.0 {
                        let c = if dx + dy < -2.0 { l2 } else if dx + dy > 2.0 { l1 } else { l0 };
                        s.set(px + x, py + y, c);
                    }
                }
            }
            if !picked {
                let berry = crate::creatures::FLAVORS[f as usize].color();
                for (bx, by) in [(2, 3), (5, 2), (4, 5), (1, 6), (6, 5)] {
                    s.set(px + bx, py + by, berry);
                }
                s.set(px + 5, py + 1, berry.lerp(Rgb(255, 255, 255), 0.6));
            }
        }
    }
}

pub const ALTAR_COLORS: [Rgb; 4] = [Rgb(255, 150, 200), Rgb(255, 120, 90), Rgb(120, 190, 255), Rgb(255, 220, 130)];

/// The building a tile belongs to: its top-left tile and size in tiles.
pub fn building_bounds(m: &Map, tx: i32, ty: i32) -> (i32, i32, i32, i32) {
    let tile = m.get(tx, ty);
    let (mut ox, mut oy) = (tx, ty);
    while m.get(ox - 1, ty) == tile {
        ox -= 1;
    }
    while m.get(tx, oy - 1) == tile {
        oy -= 1;
    }
    let (mut w, mut h) = (1, 1);
    while m.get(ox + w, oy) == tile {
        w += 1;
    }
    while m.get(ox, oy + h) == tile {
        h += 1;
    }
    (ox, oy, w, h)
}

/// Buildings span several tiles; each tile draws its slice of the whole picture.
fn draw_building_part(s: &mut Screen, m: &Map, tx: i32, ty: i32, px: i32, py: i32, tile: Tile) {
    let (ox, oy, bw, bh) = building_bounds(m, tx, ty);
    let (lx, ly) = ((tx - ox) * TILE, (ty - oy) * TILE);
    let (w, h) = (bw * TILE, bh * TILE);
    // (roof, roof shade, wall, wall shade, window)
    let (roof, roof_d, wall, wall_d, glass) = match tile {
        Tile::Lab => (Rgb(218, 119, 86), Rgb(176, 88, 62), Rgb(246, 243, 236), Rgb(210, 205, 198), Rgb(120, 222, 222)),
        Tile::Shop => (Rgb(82, 132, 204), Rgb(56, 96, 160), Rgb(242, 228, 204), Rgb(210, 194, 166), Rgb(150, 200, 235)),
        Tile::Arena => (Rgb(132, 102, 182), Rgb(100, 76, 150), Rgb(192, 188, 202), Rgb(150, 146, 164), Rgb(150, 200, 235)),
        _ => (Rgb(214, 108, 74), Rgb(170, 80, 56), Rgb(244, 230, 204), Rgb(214, 196, 166), Rgb(150, 200, 235)),
    };
    let eave = h * 45 / 100;
    let dome = tile == Tile::Arena;
    for y in 0..TILE {
        for x in 0..TILE {
            let (gx, gy) = (lx + x, ly + y);
            let door = (w / 2 - 3..w / 2 + 3).contains(&gx) && gy >= h - 8;
            let window = (eave + 3..eave + 7).contains(&gy) && ((w / 4 - 2..w / 4 + 3).contains(&gx) || (3 * w / 4 - 2..3 * w / 4 + 3).contains(&gx));
            let c = if gy < eave {
                let inside = if dome {
                    let dx = (gx as f32 - w as f32 / 2.0) / (w as f32 / 2.0);
                    let dy = (eave - gy) as f32 / eave as f32;
                    dx * dx + dy * dy <= 1.0
                } else {
                    let inset = (eave - gy) / 3;
                    gx >= inset && gx < w - inset
                };
                if !inside {
                    continue;
                }
                if gy % 3 == 2 { roof_d } else { roof }
            } else if gy == eave {
                roof_d
            } else if door {
                if gx == w / 2 + 1 && gy == h - 4 { Rgb(250, 210, 90) } else if dome { Rgb(70, 60, 90) } else { Rgb(120, 76, 50) }
            } else if tile == Tile::Shop && (h - 10..h - 8).contains(&gy) && (w / 2 - 5..w / 2 + 5).contains(&gx) {
                if (gx / 2) % 2 == 0 { Rgb(220, 70, 70) } else { Rgb(250, 245, 240) } // striped awning
            } else if dome && (eave + 2..eave + 9).contains(&gy) && (gx == w / 6 || gx == w / 6 + 1 || gx == 5 * w / 6 || gx == 5 * w / 6 - 1) {
                Rgb(218, 119, 86) // banners
            } else if window && !dome {
                if gy == eave + 5 || gx == w / 4 || gx == 3 * w / 4 { wall_d } else { glass }
            } else if gx == 0 || gx == w - 1 || gy == h - 1 {
                wall_d
            } else {
                wall
            };
            s.set(px + x, py + y, c);
        }
    }
    if tile == Tile::Lab {
        // The ✻ emblem on the roof.
        let (cx, cy) = (w / 2, eave / 2 + 1);
        for (dx, dy) in [(0, 0), (0, -1), (0, -2), (0, 1), (0, 2), (-1, 0), (-2, 0), (1, 0), (2, 0), (-1, -1), (1, 1), (1, -1), (-1, 1), (-2, -2), (2, 2), (2, -2), (-2, 2)] {
            let (gx, gy) = (cx + dx, cy + dy);
            if (lx..lx + TILE).contains(&gx) && (ly..ly + TILE).contains(&gy) {
                s.set(px + gx - lx, py + gy - ly, Rgb(255, 236, 220));
            }
        }
    }
}

/// Draw a sprite from string rows using a palette function ('.' = transparent).
pub fn sprite_rows(s: &mut Screen, px: i32, py: i32, rows: &[&str], pal: &dyn Fn(char) -> Option<Rgb>) {
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some(c) = pal(ch) {
                s.set(px + x as i32, py + y as i32, c);
            }
        }
    }
}
