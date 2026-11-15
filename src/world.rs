//! The map and the factory simulation.

use crate::inventory::Inventory;
use crate::items::{BANK_RATE, Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId};
use crate::research::Research;
use crate::scenario::{Goal, Scenario};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const TICKS_PER_SEC: u32 = 20;
/// Length of a conveyor tile in sub-tile units.
pub const TILE_LEN: u8 = 8;
/// Minimum distance between items on a conveyor.
pub const SPACING: u8 = 2;
/// Seconds to mine one ore at speed 1.
pub const MINE_TIME: f32 = 1.0;
pub const PYLON_SUPPLY: i32 = 2;
pub const PYLON_REACH_SQ: i32 = 56;
pub const CRATE_CAP: u32 = 800;
pub const UPLINK_GOAL: u32 = 20;
/// Length of a day and night, in ticks (six minutes).
pub const DAY_TICKS: u64 = 6 * 60 * TICKS_PER_SEC as u64;
/// Energy a battery bank holds, in kJ.
pub const BANK_CAPACITY: f32 = 20_000.0;
/// Water a pump delivers per second.
pub const PUMP_RATE: f32 = 2.0;
/// Energy a steam turbine gets from one water, in kJ (2 water/s at its full 2400 kW).
pub const STEAM_PER_WATER: f32 = 1200.0;
/// A steam turbine gets this much more energy out of fuel than a generator.
pub const TURBINE_EFFICIENCY: f32 = 1.5;
/// Seconds a sifter takes per quartz over a full 3x3 of sand.
pub const SIFT_TIME: f32 = 4.0;
/// Transmissions after the win that mark a milestone: each doubles the last.
pub const MILESTONE_BASE: u32 = 40;
/// How far around spawn the map starts out explored.
pub const START_REVEAL: i32 = 32;
/// Furthest a survey mast sees, and how fast its view grows (tiles per second at full power).
pub const SURVEY_MAX: f32 = 50.0;
pub const SURVEY_SPEED: f32 = 0.5;
/// Rail carts: tiles per second, items carried, items moved per tick at a depot.
pub const CART_SPEED: f32 = 8.0;
pub const CART_CAP: u32 = 400;
pub const CART_TRANSFER: u32 = 2;
/// A loading cart leaves after this many seconds without anything to load, if it has cargo...
pub const CART_IDLE_SECS: u32 = 5;
/// ...and after this long at the depot in any case, if it has cargo.
pub const CART_MAX_STOP_SECS: u32 = 30;

/// How far around itself a newly built building lets you see.
pub fn reveal_radius(kind: EntityKind) -> i32 {
    match kind {
        EntityKind::Pylon => 9,
        EntityKind::SurveyMast => 8,
        _ => 5,
    }
}

/// Share of full sunlight at `tick`: a long day, an hour-long dusk and dawn, and a night.
pub fn daylight_at(tick: u64) -> f32 {
    let p = (tick % DAY_TICKS) as f32 / DAY_TICKS as f32;
    if p < 0.55 {
        1.0
    } else if p < 0.65 {
        (0.65 - p) / 0.10
    } else if p < 0.9 {
        0.0
    } else {
        (p - 0.9) / 0.10
    }
}

/// What a machine's modules do to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModuleEffect {
    pub speed: f32,
    pub power: f32,
    /// Extra share of each output.
    pub productivity: f32,
}

impl ModuleEffect {
    pub fn of(modules: &[Item]) -> ModuleEffect {
        let s = modules.iter().filter(|&&m| m == Item::SpeedModule).count() as f32;
        let p = modules
            .iter()
            .filter(|&&m| m == Item::ProductivityModule)
            .count() as f32;
        ModuleEffect {
            speed: (1.0 + 0.5 * s - 0.15 * p).max(0.2),
            power: 1.0 + 0.5 * (s + p),
            productivity: 0.1 * p,
        }
    }
}
/// Furthest a tunnel exit can be from its entrance, in tiles (so it passes under up to 4).
pub const TUNNEL_REACH: i32 = 5;
const NO_NET: u16 = u16::MAX;
const NO_PAIR: u32 = u32::MAX;

/// Up to three (tile, position) places a transport tile hands items to, and how many there are.
type Outputs = ([(usize, u8); 3], usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Terrain {
    Grass,
    Sand,
    Water,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub terrain: Terrain,
    pub ore: Option<Item>,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Working,
    NoFuel,
    NoPower,
    LowPower,
    Blocked,
    NoInput,
    NoRecipe,
    Depleted,
    NoResearch,
    /// A tunnel entrance or exit without its other end.
    Unlinked,
    /// A pump with no water behind it, or a steam turbine with none to boil.
    NoWater,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Working => "working",
            Status::NoFuel => "no fuel",
            Status::NoPower => "no power",
            Status::LowPower => "low power",
            Status::Blocked => "output blocked",
            Status::NoInput => "waiting for input",
            Status::NoRecipe => "no recipe set",
            Status::Depleted => "no ore left",
            Status::NoResearch => "no research selected",
            Status::Unlinked => "no other end in reach",
            Status::NoWater => "no water",
        }
    }

    pub fn is_problem(self) -> bool {
        matches!(
            self,
            Status::NoFuel
                | Status::NoPower
                | Status::Blocked
                | Status::NoRecipe
                | Status::Depleted
                | Status::NoResearch
                | Status::Unlinked
                | Status::NoWater
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeltItem {
    pub item: Item,
    pub pos: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fuel {
    /// Unburned fuel items.
    pub stock: u32,
    pub item: Option<Item>,
    /// Remaining energy of the item being burned, in kJ.
    pub burning: f32,
}

impl Fuel {
    pub fn has(&self) -> bool {
        self.burning > 0.0 || self.stock > 0
    }

    /// Consumes `kj` of energy. Returns false if there is no fuel.
    pub fn draw(&mut self, kj: f32) -> bool {
        if self.burning <= 0.0 {
            if self.stock == 0 {
                return false;
            }
            self.stock -= 1;
            self.burning += self.item.and_then(|i| i.fuel_value()).unwrap_or(8000.0);
            if self.stock == 0 {
                // keep item around so the UI can name what's burning
            }
        }
        self.burning -= kj;
        true
    }

    pub fn add(&mut self, item: Item, n: u32) {
        self.item = Some(item);
        self.stock += n;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmPhase {
    #[default]
    Pickup,
    Swing,
    Return,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

fn is_zero_f32(n: &f32) -> bool {
    *n == 0.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum State {
    /// Conveyors, splitters and tunnel belts. Items are sorted by `pos`, furthest along last.
    /// A tunnel entrance also holds the items travelling underground, at `pos` beyond `TILE_LEN`.
    Belt {
        items: Vec<BeltItem>,
        /// Which output a splitter tries next.
        #[serde(default, skip_serializing_if = "is_zero")]
        turn: u8,
    },
    Arm {
        hand: Option<Item>,
        phase: ArmPhase,
        t: f32,
        /// Filter arms only move this item.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<Item>,
    },
    Miner {
        fuel: Fuel,
        progress: f32,
        output: Option<Item>,
    },
    Furnace {
        fuel: Fuel,
        input: Inventory,
        output: Inventory,
        job: Option<RecipeId>,
        progress: f32,
    },
    Assembler {
        recipe: Option<RecipeId>,
        input: Inventory,
        output: Inventory,
        working: bool,
        progress: f32,
    },
    /// Crates and depots.
    Chest {
        inv: Inventory,
        /// Depots only: carts unload here instead of loading.
        #[serde(default, skip_serializing_if = "is_false")]
        unload: bool,
    },
    /// Generators and steam turbines. Only turbines use water.
    Generator {
        fuel: Fuel,
        #[serde(default, skip_serializing_if = "is_zero_u32")]
        water: u32,
        /// Energy left from the water being boiled, in kJ.
        #[serde(default, skip_serializing_if = "is_zero_f32")]
        steam: f32,
    },
    Battery {
        /// Stored energy in kJ.
        charge: f32,
    },
    Pylon,
    Solar,
    Rail,
    Survey {
        /// How far around the mast is revealed, in tiles.
        radius: f32,
    },
    Lab {
        inv: Inventory,
        working: bool,
        progress: f32,
        /// Datapacks consumed by the unit in progress (refunded if the lab is picked up).
        #[serde(default)]
        packs: Vec<Item>,
    },
    Uplink {
        inv: Inventory,
        progress: f32,
        working: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub kind: EntityKind,
    pub dir: Dir,
    pub state: State,
    /// Speed and productivity modules (machines with `module_slots`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<Item>,
    /// Progress toward the next extra output from productivity modules.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub bonus: f32,
    #[serde(skip)]
    pub status: Status,
}

impl Entity {
    pub fn new(kind: EntityKind, dir: Dir) -> Self {
        let state = match kind {
            EntityKind::Conveyor
            | EntityKind::ExpressConveyor
            | EntityKind::Splitter
            | EntityKind::TunnelIn
            | EntityKind::TunnelOut => State::Belt {
                items: vec![],
                turn: 0,
            },
            EntityKind::Arm
            | EntityKind::SwiftArm
            | EntityKind::LongArm
            | EntityKind::FilterArm => State::Arm {
                hand: None,
                phase: ArmPhase::Pickup,
                t: 0.0,
                filter: None,
            },
            EntityKind::Digger | EntityKind::Bore | EntityKind::Sifter | EntityKind::Pump => {
                State::Miner {
                    fuel: Fuel::default(),
                    progress: 0.0,
                    output: None,
                }
            }
            EntityKind::Kiln | EntityKind::ArcFurnace => State::Furnace {
                fuel: Fuel::default(),
                input: Inventory::new(),
                output: Inventory::new(),
                job: None,
                progress: 0.0,
            },
            EntityKind::Fabricator | EntityKind::ChemPlant => State::Assembler {
                recipe: None,
                input: Inventory::new(),
                output: Inventory::new(),
                working: false,
                progress: 0.0,
            },
            EntityKind::Crate | EntityKind::Depot => State::Chest {
                inv: Inventory::new(),
                unload: false,
            },
            EntityKind::Rail => State::Rail,
            EntityKind::SurveyMast => State::Survey {
                radius: reveal_radius(kind) as f32,
            },
            EntityKind::Generator | EntityKind::SteamTurbine => State::Generator {
                fuel: Fuel::default(),
                water: 0,
                steam: 0.0,
            },
            EntityKind::BatteryBank => State::Battery { charge: 0.0 },
            EntityKind::Pylon => State::Pylon,
            EntityKind::SunPanel => State::Solar,
            EntityKind::Lab => State::Lab {
                inv: Inventory::new(),
                working: false,
                progress: 0.0,
                packs: vec![],
            },
            EntityKind::Uplink => State::Uplink {
                inv: Inventory::new(),
                progress: 0.0,
                working: false,
            },
        };
        Entity {
            kind,
            dir: if kind.directional() { dir } else { Dir::North },
            state,
            modules: vec![],
            bonus: 0.0,
            status: Status::Idle,
        }
    }

    /// Everything the entity holds, including in-progress inputs, for refunds on removal.
    pub fn contents(&self) -> Inventory {
        let mut inv = Inventory::new();
        for &m in &self.modules {
            inv.add(m, 1);
        }
        match &self.state {
            State::Belt { items, .. } => {
                for b in items {
                    inv.add(b.item, 1);
                }
            }
            State::Arm { hand, .. } => {
                if let Some(h) = hand {
                    inv.add(*h, 1);
                }
            }
            State::Miner { fuel, output, .. } => {
                add_fuel(&mut inv, fuel);
                if let Some(o) = output {
                    inv.add(*o, 1);
                }
            }
            State::Furnace {
                fuel,
                input,
                output,
                job,
                ..
            } => {
                add_fuel(&mut inv, fuel);
                merge(&mut inv, input);
                merge(&mut inv, output);
                if let Some(j) = job {
                    inv.add_all(j.get().inputs, 1);
                }
            }
            State::Assembler {
                recipe,
                input,
                output,
                working,
                ..
            } => {
                merge(&mut inv, input);
                merge(&mut inv, output);
                if let (Some(r), true) = (recipe, working) {
                    inv.add_all(r.get().inputs, 1);
                }
            }
            State::Chest { inv: c, .. } => merge(&mut inv, c),
            State::Generator { fuel, water, .. } => {
                add_fuel(&mut inv, fuel);
                if *water > 0 {
                    inv.add(Item::Water, *water);
                }
            }
            State::Pylon
            | State::Solar
            | State::Battery { .. }
            | State::Rail
            | State::Survey { .. } => {}
            State::Lab {
                inv: l,
                working,
                packs,
                ..
            } => {
                merge(&mut inv, l);
                if *working {
                    for &p in packs {
                        inv.add(p, 1);
                    }
                }
            }
            State::Uplink {
                inv: u, working, ..
            } => {
                merge(&mut inv, u);
                if *working {
                    inv.add(Item::RelayModule, 1);
                }
            }
        }
        inv
    }
}

fn add_fuel(inv: &mut Inventory, fuel: &Fuel) {
    if fuel.stock > 0 {
        inv.add(fuel.item.unwrap_or(Item::Lignite), fuel.stock);
    }
}

fn merge(into: &mut Inventory, from: &Inventory) {
    for (i, n) in from.iter() {
        into.add(i, n);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Net {
    pub capacity: f32,
    pub demand_prev: f32,
    pub demand_cur: f32,
    pub load: f32,
    pub satisfaction: f32,
    /// Energy held in the network's battery banks and their total size, in kJ.
    pub stored: f32,
    pub storage: f32,
    /// Battery flow last tick in kW: positive while charging, negative while discharging.
    pub battery_flow: f32,
}

/// A rail cart: it shuttles between the depots on its track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cart {
    /// The tile it's on.
    pub at: u32,
    /// Tiles still ahead, the next one last (the destination first). Empty while stopped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<u32>,
    /// How far it has rolled toward the next tile, 0 to 1.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub moved: f32,
    #[serde(default)]
    pub cargo: Inventory,
    /// Ticks stopped at this depot, and ticks since it last loaded or unloaded anything.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub stopped: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub idle: u32,
    #[serde(skip)]
    pub status: CartStatus,
}

impl Cart {
    pub fn new(at: usize) -> Cart {
        Cart {
            at: at as u32,
            path: vec![],
            moved: 0.0,
            cargo: Inventory::new(),
            stopped: 0,
            idle: 0,
            status: CartStatus::Waiting,
        }
    }

    /// The tile to draw it on: the next one once it's more than halfway there.
    pub fn shown_at(&self) -> usize {
        match self.path.last() {
            Some(&n) if self.moved >= 0.5 => n as usize,
            _ => self.at as usize,
        }
    }

    /// The depot it's heading for.
    pub fn destination(&self) -> Option<usize> {
        self.path.first().map(|&d| d as usize)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CartStatus {
    #[default]
    Waiting,
    Loading,
    Unloading,
    Moving,
    DepotFull,
    NoRoute,
}

impl CartStatus {
    pub fn label(self) -> &'static str {
        match self {
            CartStatus::Waiting => "waiting for cargo",
            CartStatus::Loading => "loading",
            CartStatus::Unloading => "unloading",
            CartStatus::Moving => "on its way",
            CartStatus::DepotFull => "depot full",
            CartStatus::NoRoute => "no other depot on its track",
        }
    }
}

/// Run lengths of a bit list, starting with a run of `false` (which may be empty).
fn encode_runs(bits: &[bool]) -> Vec<u32> {
    let mut runs = vec![];
    let (mut cur, mut n) = (false, 0u32);
    for &b in bits {
        if b == cur {
            n += 1;
        } else {
            runs.push(n);
            cur = b;
            n = 1;
        }
    }
    runs.push(n);
    runs
}

fn decode_runs(runs: &[u32], len: usize) -> Result<Vec<bool>, String> {
    let mut v = Vec::with_capacity(len);
    let mut cur = false;
    for &r in runs {
        if v.len() + r as usize > len {
            return Err("explored map size mismatch".into());
        }
        v.extend(std::iter::repeat_n(cur, r as usize));
        cur = !cur;
    }
    if v.len() != len {
        return Err("explored map size mismatch".into());
    }
    Ok(v)
}

/// Small deterministic PRNG (SplitMix64).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next_u64() % ((hi - lo + 1) as u64)) as i32
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn lattice(seed: u64, x: i32, y: i32) -> f32 {
    let h = mix(seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn value_noise(seed: u64, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let a = lattice(seed, x0, y0);
    let b = lattice(seed, x0 + 1, y0);
    let c = lattice(seed, x0, y0 + 1);
    let d = lattice(seed, x0 + 1, y0 + 1);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

fn fbm(seed: u64, x: f32, y: f32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..4 {
        total += amp * value_noise(seed.wrapping_add(o * 7919), x * freq, y * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    total / norm
}

/// Ticks between production snapshots used for rolling rates.
pub const HISTORY_INTERVAL: u64 = 5 * TICKS_PER_SEC as u64;
/// Snapshots kept (10 minutes).
pub const HISTORY_LEN: usize = 121;

pub struct World {
    pub width: i32,
    pub height: i32,
    pub seed: u64,
    pub tiles: Vec<Tile>,
    pub entities: Vec<Option<Entity>>,
    pub tick: u64,
    pub research: Research,
    pub won: bool,
    /// Relay modules transmitted by Uplink arrays. Kept on the world so moving an Uplink keeps progress.
    pub transmitted: u32,
    /// Tick the Uplink finished its first 20 transmissions.
    pub won_at: Option<u64>,
    pub produced: BTreeMap<Item, u64>,
    /// Endless research bonuses, refreshed every tick: extra speed for extractors and arms.
    mining_bonus: f32,
    arm_bonus: f32,
    /// Snapshots of `produced` every `HISTORY_INTERVAL` ticks, oldest first. Not saved.
    pub history: std::collections::VecDeque<(u64, BTreeMap<Item, u64>)>,
    pub events: Vec<String>,
    // Caches rebuilt when the layout changes.
    dirty: bool,
    belt_order: Vec<usize>,
    machines: Vec<usize>,
    power_sources: Vec<usize>,
    power_net: Vec<u16>,
    pub nets: Vec<Net>,
    /// For each tunnel piece, the index of its other end (`NO_PAIR` if none).
    pair: Vec<u32>,
    /// Tiles the player has seen. Building needs explored ground.
    pub explored: Vec<bool>,
    pub carts: Vec<Cart>,
    /// The rules this world was made with.
    pub scenario: &'static Scenario,
}

pub const ORES: [Item; 4] = [
    Item::FerriteOre,
    Item::MalachiteOre,
    Item::Lignite,
    Item::Quartz,
];

impl World {
    pub fn empty(width: i32, height: i32, seed: u64) -> Self {
        let n = (width * height) as usize;
        World {
            width,
            height,
            seed,
            tiles: vec![
                Tile {
                    terrain: Terrain::Grass,
                    ore: None,
                    amount: 0
                };
                n
            ],
            entities: (0..n).map(|_| None).collect(),
            tick: 0,
            research: Research::default(),
            won: false,
            transmitted: 0,
            won_at: None,
            produced: BTreeMap::new(),
            mining_bonus: 0.0,
            arm_bonus: 0.0,
            history: std::collections::VecDeque::new(),
            events: vec![],
            dirty: true,
            belt_order: vec![],
            machines: vec![],
            power_sources: vec![],
            power_net: vec![NO_NET; n],
            nets: vec![],
            pair: vec![NO_PAIR; n],
            explored: vec![true; n],
            carts: vec![],
            scenario: Scenario::standard(),
        }
    }

    #[cfg(test)]
    pub fn generate(width: i32, height: i32, seed: u64) -> Self {
        Self::generate_scenario(width, height, seed, Scenario::standard())
    }

    /// A new world with ore, lakes and fog, following the scenario's rules.
    pub fn generate_scenario(width: i32, height: i32, seed: u64, sc: &'static Scenario) -> Self {
        let mut w = World::empty(width, height, seed);
        w.scenario = sc;
        let (cx, cy) = w.spawn();
        // Terrain: lakes and beaches, kept away from spawn.
        for y in 0..height {
            for x in 0..width {
                let n = fbm(seed ^ 0xA5A5, x as f32 / 14.0, y as f32 / 14.0);
                let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
                let bias = ((24.0 - d) / 24.0).max(0.0) * 0.3;
                let v = n + bias;
                let i = w.idx(x, y).unwrap();
                w.tiles[i].terrain = if v < 0.33 {
                    Terrain::Water
                } else if v < 0.37 {
                    Terrain::Sand
                } else {
                    Terrain::Grass
                };
            }
        }
        let mut rng = Rng::new(seed);
        // Guaranteed starter patches around spawn. A scenario can push some far out instead.
        let start_angle = rng.f32() * std::f32::consts::TAU;
        for (k, ore) in ORES.iter().enumerate() {
            let far = sc.far_ores.contains(ore);
            let a = start_angle + k as f32 * std::f32::consts::FRAC_PI_2;
            let dist = if far { 50.0 } else { 9.0 } + rng.f32() * if far { 10.0 } else { 4.0 };
            let px = cx + (a.cos() * dist) as i32;
            let py = cy + (a.sin() * dist) as i32;
            let r = 4.0 + rng.f32() * 1.5;
            let rich = if far {
                2000
            } else {
                (700.0 * sc.starter_ore) as u32
            };
            w.place_patch(&mut rng, *ore, px, py, r, rich);
        }
        // Scattered richer patches further out.
        let count = (width * height / 900).max(8);
        for k in 0..count {
            let ore = ORES[(k as usize) % ORES.len()];
            let near = if sc.far_ores.contains(&ore) { 50 } else { 30 };
            for _ in 0..20 {
                let px = rng.range(4, width - 5);
                let py = rng.range(4, height - 5);
                let d2 = (px - cx).pow(2) + (py - cy).pow(2);
                if d2 > near * near {
                    let rich = 900 + (d2 as f32).sqrt() as u32 * 15;
                    let r = 4.5 + rng.f32() * 4.0;
                    w.place_patch(&mut rng, ore, px, py, r, rich);
                    break;
                }
            }
        }
        // Everything beyond the area around spawn starts out unexplored.
        w.explored.iter_mut().for_each(|e| *e = false);
        w.reveal(cx, cy, START_REVEAL);
        w
    }

    /// Marks every tile within `r` of (x, y) as explored.
    pub fn reveal(&mut self, x: i32, y: i32, r: i32) {
        for yy in (y - r).max(0)..=(y + r).min(self.height - 1) {
            for xx in (x - r).max(0)..=(x + r).min(self.width - 1) {
                if (xx - x).pow(2) + (yy - y).pow(2) <= r * r {
                    self.explored[(yy * self.width + xx) as usize] = true;
                }
            }
        }
    }

    pub fn is_explored(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| self.explored[i])
    }

    /// Share of the map explored, 0 to 1.
    pub fn explored_share(&self) -> f32 {
        self.explored.iter().filter(|&&e| e).count() as f32 / self.explored.len().max(1) as f32
    }

    fn place_patch(&mut self, rng: &mut Rng, ore: Item, px: i32, py: i32, r: f32, rich: u32) {
        let s = rng.next_u64();
        let ri = r.ceil() as i32 + 2;
        for y in py - ri..=py + ri {
            for x in px - ri..=px + ri {
                let Some(i) = self.idx(x, y) else { continue };
                let d = (((x - px).pow(2) + (y - py).pow(2)) as f32).sqrt();
                let wobble = 0.75 + 0.5 * value_noise(s, x as f32 / 2.5, y as f32 / 2.5);
                let rr = r * wobble;
                if d <= rr && self.tiles[i].ore.is_none() {
                    let t = &mut self.tiles[i];
                    t.terrain = Terrain::Grass;
                    t.ore = Some(ore);
                    t.amount = (rich as f32 * (1.2 - d / rr.max(1.0) * 0.8)) as u32 + 50;
                }
            }
        }
    }

    pub fn spawn(&self) -> (i32, i32) {
        (self.width / 2, self.height / 2)
    }

    pub fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            None
        } else {
            Some((y * self.width + x) as usize)
        }
    }

    pub fn pos(&self, i: usize) -> (i32, i32) {
        (i as i32 % self.width, i as i32 / self.width)
    }

    pub fn neighbor(&self, i: usize, d: Dir) -> Option<usize> {
        let (x, y) = self.pos(i);
        let (dx, dy) = d.delta();
        self.idx(x + dx, y + dy)
    }

    pub fn tile(&self, x: i32, y: i32) -> Option<&Tile> {
        self.idx(x, y).map(|i| &self.tiles[i])
    }

    pub fn entity(&self, x: i32, y: i32) -> Option<&Entity> {
        self.idx(x, y).and_then(|i| self.entities[i].as_ref())
    }

    pub fn entity_mut(&mut self, x: i32, y: i32) -> Option<&mut Entity> {
        let i = self.idx(x, y)?;
        self.entities[i].as_mut()
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Indices of the 3x3 area a miner at `i` covers.
    pub fn mining_area(&self, i: usize) -> Vec<usize> {
        let (x, y) = self.pos(i);
        let mut v = vec![i];
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) != (0, 0)
                    && let Some(j) = self.idx(x + dx, y + dy)
                {
                    v.push(j);
                }
            }
        }
        v
    }

    /// How many tiles of the 3x3 area around `i` are sand.
    pub fn sand_in_area(&self, i: usize) -> usize {
        self.mining_area(i)
            .iter()
            .filter(|&&j| self.tiles[j].terrain == Terrain::Sand)
            .count()
    }

    /// Whether the tile behind a building at `i` facing `dir` is water.
    pub fn water_behind(&self, i: usize, dir: Dir) -> bool {
        self.neighbor(i, dir.opposite())
            .is_some_and(|j| self.tiles[j].terrain == Terrain::Water)
    }

    pub fn area_has_ore(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| {
            self.mining_area(i)
                .iter()
                .any(|&j| self.tiles[j].ore.is_some())
        })
    }

    pub fn can_place(&self, x: i32, y: i32, kind: EntityKind, dir: Dir) -> Result<(), String> {
        let Some(i) = self.idx(x, y) else {
            return Err("Out of bounds".into());
        };
        if !self.explored[i] {
            return Err(
                "Unexplored ground: build closer to what you've seen, or use a survey mast".into(),
            );
        }
        if self.entities[i].is_some() {
            return Err("Tile is occupied".into());
        }
        if self.tiles[i].terrain == Terrain::Water {
            return Err("Can't build on water".into());
        }
        if kind.is_miner() && !self.area_has_ore(x, y) {
            return Err(format!("{} must be placed on ore", kind.name()));
        }
        if kind == EntityKind::Sifter && self.sand_in_area(i) == 0 {
            return Err("Sifters must be placed on sand".into());
        }
        if kind == EntityKind::Pump && !self.water_behind(i, dir) {
            return Err("Pumps need water right behind them (r turns them)".into());
        }
        Ok(())
    }

    /// Turns the water tile at (x, y) into sand.
    pub fn fill(&mut self, x: i32, y: i32) -> Result<(), String> {
        let i = self.idx(x, y).ok_or("Out of bounds")?;
        if !self.explored[i] {
            return Err("Unexplored ground: you can't see where to fill".into());
        }
        if self.tiles[i].terrain != Terrain::Water {
            return Err("Landfill only goes on water".into());
        }
        self.tiles[i].terrain = Terrain::Sand;
        self.dirty = true;
        Ok(())
    }

    /// Floods a landfilled tile again (for undo). Only an empty sand tile without ore.
    pub fn unfill(&mut self, x: i32, y: i32) -> bool {
        let Some(i) = self.idx(x, y) else {
            return false;
        };
        let t = &mut self.tiles[i];
        if t.terrain != Terrain::Sand || t.ore.is_some() || self.entities[i].is_some() {
            return false;
        }
        t.terrain = Terrain::Water;
        self.dirty = true;
        true
    }

    pub fn place(&mut self, x: i32, y: i32, kind: EntityKind, dir: Dir) -> Result<(), String> {
        self.can_place(x, y, kind, dir)?;
        let i = self.idx(x, y).unwrap();
        self.entities[i] = Some(Entity::new(kind, dir));
        self.dirty = true;
        self.reveal(x, y, reveal_radius(kind));
        Ok(())
    }

    pub fn remove(&mut self, x: i32, y: i32) -> Option<Entity> {
        let i = self.idx(x, y)?;
        let e = self.entities[i].take();
        if e.is_some() {
            self.dirty = true;
        }
        e
    }

    pub fn rotate(&mut self, x: i32, y: i32) -> bool {
        if let Some(e) = self.entity_mut(x, y)
            && e.kind.directional()
        {
            e.dir = e.dir.cw();
            self.dirty = true;
            return true;
        }
        false
    }

    pub fn net_at(&self, i: usize) -> Option<&Net> {
        let n = *self.power_net.get(i)?;
        if n == NO_NET {
            None
        } else {
            self.nets.get(n as usize)
        }
    }

    fn sat_at(&self, i: usize) -> f32 {
        self.net_at(i).map(|n| n.satisfaction).unwrap_or(0.0)
    }

    fn add_demand(&mut self, i: usize, kw: f32) {
        let n = self.power_net[i];
        if n != NO_NET {
            self.nets[n as usize].demand_cur += kw;
        }
    }

    pub fn pylons(&self) -> Vec<(i32, i32)> {
        self.entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.as_ref().is_some_and(|e| e.kind == EntityKind::Pylon))
            .map(|(i, _)| self.pos(i))
            .collect()
    }

    pub fn ensure_caches(&mut self) {
        if self.dirty {
            self.rebuild();
        }
    }

    /// Where an item leaving tile `i` in direction `o` would land: the tile and the position
    /// on it. `None` if nothing there takes items from that side.
    fn link(&self, i: usize, o: Dir) -> Option<(usize, u8)> {
        let j = self.neighbor(i, o)?;
        let n = self.entities[j].as_ref()?;
        if !n.kind.is_transport() || n.dir == o.opposite() {
            return None;
        }
        // The back of a tunnel exit is underground.
        if n.kind == EntityKind::TunnelOut && n.dir == o {
            return None;
        }
        Some((j, if n.dir == o { 0 } else { TILE_LEN / 2 }))
    }

    /// The tiles a transport building at `i` can pass items on to, in the order a splitter
    /// takes turns between them.
    fn outputs(&self, i: usize, kind: EntityKind, dir: Dir) -> Outputs {
        let mut out = [(0, 0); 3];
        let mut n = 0;
        let mut push = |l: Option<(usize, u8)>| {
            if let Some(l) = l {
                out[n] = l;
                n += 1;
            }
        };
        match kind {
            EntityKind::TunnelIn => push(self.tunnel_pair(i).map(|j| (j, 0))),
            EntityKind::Splitter => {
                for o in [dir, dir.ccw(), dir.cw()] {
                    push(self.link(i, o));
                }
            }
            _ => push(self.link(i, dir)),
        }
        (out, n)
    }

    /// The other end of the tunnel piece at `i`, if it has one.
    pub fn tunnel_pair(&self, i: usize) -> Option<usize> {
        match self.pair.get(i) {
            Some(&p) if p != NO_PAIR => Some(p as usize),
            _ => None,
        }
    }

    /// Length of the belt at `i` in sub-tile units. A linked tunnel entrance includes the
    /// stretch underground.
    fn belt_len(&self, i: usize, kind: EntityKind) -> u8 {
        if kind == EntityKind::TunnelIn
            && let Some(j) = self.tunnel_pair(i)
        {
            let (ax, ay) = self.pos(i);
            let (bx, by) = self.pos(j);
            return TILE_LEN * ((ax - bx).abs() + (ay - by).abs()) as u8;
        }
        TILE_LEN
    }

    /// The entrance a tunnel piece placed at (x, y) facing `dir` would complete: the nearest
    /// tunnel piece behind it within reach, if that is an entrance still waiting for an exit.
    /// Links must be current (`ensure_caches`).
    pub fn tunnel_entrance_behind(&self, x: i32, y: i32, dir: Dir) -> Option<usize> {
        let (dx, dy) = dir.delta();
        for k in 1..=TUNNEL_REACH {
            let j = self.idx(x - dx * k, y - dy * k)?;
            if let Some(e) = self.entities[j].as_ref()
                && e.kind.is_tunnel()
                && e.dir == dir
            {
                return (e.kind == EntityKind::TunnelIn && self.pair[j] == NO_PAIR).then_some(j);
            }
        }
        None
    }

    /// Whether a tunnel piece placed at (x, y) facing `dir` should be an exit.
    pub fn tunnel_wants_exit(&mut self, x: i32, y: i32, dir: Dir) -> bool {
        self.ensure_caches();
        self.tunnel_entrance_behind(x, y, dir).is_some()
    }

    /// After an entrance goes in at (x, y): if the nearest tunnel piece ahead is another
    /// entrance with no exit, it becomes this one's exit. That makes building a tunnel from
    /// the far end backwards work too.
    pub fn tunnel_adopt_ahead(&mut self, x: i32, y: i32, dir: Dir) {
        self.ensure_caches();
        let (dx, dy) = dir.delta();
        for k in 1..=TUNNEL_REACH {
            let Some(j) = self.idx(x + dx * k, y + dy * k) else {
                return;
            };
            if let Some(e) = self.entities[j].as_mut()
                && e.kind.is_tunnel()
                && e.dir == dir
            {
                if e.kind == EntityKind::TunnelIn && self.pair[j] == NO_PAIR {
                    e.kind = EntityKind::TunnelOut;
                    self.dirty = true;
                }
                return;
            }
        }
    }

    /// Turns a tunnel entrance into an exit or back.
    pub fn flip_tunnel(&mut self, x: i32, y: i32) -> bool {
        let Some(e) = self.entity_mut(x, y) else {
            return false;
        };
        e.kind = match e.kind {
            EntityKind::TunnelIn => EntityKind::TunnelOut,
            EntityKind::TunnelOut => EntityKind::TunnelIn,
            _ => return false,
        };
        self.dirty = true;
        true
    }

    fn rebuild(&mut self) {
        self.dirty = false;
        let mut belts = vec![];
        self.machines.clear();
        self.power_sources.clear();
        let mut pylons = vec![];
        for (i, e) in self.entities.iter().enumerate() {
            let Some(e) = e else { continue };
            if e.kind.is_transport() {
                belts.push(i);
            } else if e.kind == EntityKind::Rail {
                // Carts do the work on rails.
            } else if e.kind == EntityKind::Pylon {
                pylons.push(i);
            } else if e.kind.power_output() > 0.0 {
                self.power_sources.push(i);
            } else {
                self.machines.push(i);
            }
        }

        // Link each tunnel entrance to the first free exit ahead of it, facing the same way.
        self.pair.iter_mut().for_each(|p| *p = NO_PAIR);
        for &i in &belts {
            let e = self.entities[i].as_ref().unwrap();
            if e.kind != EntityKind::TunnelIn {
                continue;
            }
            let dir = e.dir;
            let mut cur = i;
            for _ in 0..TUNNEL_REACH {
                let Some(j) = self.neighbor(cur, dir) else {
                    break;
                };
                cur = j;
                if let Some(n) = self.entities[j].as_ref()
                    && n.kind.is_tunnel()
                    && n.dir == dir
                {
                    if n.kind == EntityKind::TunnelOut && self.pair[j] == NO_PAIR {
                        self.pair[i] = j as u32;
                        self.pair[j] = i as u32;
                    }
                    break;
                }
            }
        }

        // Order transport tiles so each one updates after everything it feeds: the end of a
        // line moves first and frees room for what's behind it. Depth-first, post-order;
        // loops are cut wherever the walk meets itself.
        const NEW: u8 = 0;
        const OPEN: u8 = 1;
        const DONE: u8 = 2;
        let mut mark = vec![NEW; self.entities.len()];
        let mut order = Vec::with_capacity(belts.len());
        // (tile, its outputs, next output to visit)
        let mut stack: Vec<(usize, Outputs, usize)> = vec![];
        for &b in &belts {
            if mark[b] != NEW {
                continue;
            }
            let e = self.entities[b].as_ref().unwrap();
            mark[b] = OPEN;
            stack.push((b, self.outputs(b, e.kind, e.dir), 0));
            while let Some(top) = stack.last_mut() {
                let ((outs, n), k) = (top.1, top.2);
                if k < n {
                    let next = outs[k].0;
                    top.2 += 1;
                    if mark[next] == NEW {
                        let e = self.entities[next].as_ref().unwrap();
                        mark[next] = OPEN;
                        stack.push((next, self.outputs(next, e.kind, e.dir), 0));
                    }
                } else {
                    mark[top.0] = DONE;
                    order.push(top.0);
                    stack.pop();
                }
            }
        }
        self.belt_order = order;

        // Power networks: union-find over pylons.
        let mut parent: Vec<usize> = (0..pylons.len()).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            let mut c = x;
            while p[c] != r {
                let n = p[c];
                p[c] = r;
                c = n;
            }
            r
        }
        let ppos: Vec<(i32, i32)> = pylons.iter().map(|&p| self.pos(p)).collect();
        for a in 0..pylons.len() {
            for b in a + 1..pylons.len() {
                let (dx, dy) = (ppos[a].0 - ppos[b].0, ppos[a].1 - ppos[b].1);
                if dx * dx + dy * dy <= PYLON_REACH_SQ {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
        }
        let old: Vec<Net> = std::mem::take(&mut self.nets);
        self.power_net.iter_mut().for_each(|n| *n = NO_NET);
        let mut root_to_net: BTreeMap<usize, u16> = BTreeMap::new();
        for (k, &p) in pylons.iter().enumerate() {
            let r = find(&mut parent, k);
            let next = root_to_net.len() as u16;
            let id = *root_to_net.entry(r).or_insert(next);
            self.power_net[p] = id;
            let (px, py) = ppos[k];
            for y in py - PYLON_SUPPLY..=py + PYLON_SUPPLY {
                for x in px - PYLON_SUPPLY..=px + PYLON_SUPPLY {
                    if let Some(i) = self.idx(x, y)
                        && self.power_net[i] == NO_NET
                        && self.entities[i]
                            .as_ref()
                            .is_some_and(|e| e.kind.is_power_node())
                    {
                        self.power_net[i] = id;
                    }
                }
            }
        }
        self.nets = (0..root_to_net.len())
            .map(|k| {
                let mut n = Net::default();
                if let Some(o) = old.get(k) {
                    n.demand_prev = o.demand_prev;
                }
                n
            })
            .collect();
    }

    pub fn tick(&mut self) {
        self.ensure_caches();
        self.refresh_bonuses();
        self.power_prepass();
        let belts = std::mem::take(&mut self.belt_order);
        for &i in &belts {
            self.update_belt(i);
        }
        self.belt_order = belts;
        let machines = std::mem::take(&mut self.machines);
        for &i in &machines {
            self.update_machine(i);
        }
        self.machines = machines;
        self.update_carts();
        for n in &mut self.nets {
            n.demand_prev = n.demand_cur;
            n.demand_cur = 0.0;
        }
        if let Goal::Produce(item, n) = self.scenario.goal
            && !self.won
            && self.produced.get(&item).copied().unwrap_or(0) >= n
        {
            self.won = true;
            self.won_at = Some(self.tick);
            self.events.push(format!(
                "Challenge complete: {}. You win!",
                self.scenario.goal_text().to_lowercase()
            ));
        }
        self.tick += 1;
        if self.tick.is_multiple_of(HISTORY_INTERVAL) {
            self.history.push_back((self.tick, self.produced.clone()));
            while self.history.len() > HISTORY_LEN {
                self.history.pop_front();
            }
        }
    }

    fn refresh_bonuses(&mut self) {
        let level = |id: &str| {
            crate::recipes::TechId::by_id(id).map_or(0, |t| self.research.level(t)) as f32
        };
        self.mining_bonus = 1.0 + 0.1 * level("mining-productivity");
        self.arm_bonus = 1.0 + 0.1 * level("servo-tuning");
    }

    /// Sunlight now, from 0 (night) to 1 (day).
    pub fn daylight(&self) -> f32 {
        daylight_at(self.tick)
    }

    /// Seconds one transmission takes at full power. After the win it gets harder with every
    /// milestone passed.
    pub fn transmission_secs(&self) -> f32 {
        let mut passed = 0;
        let mut m = MILESTONE_BASE;
        while self.transmitted >= m {
            passed += 1;
            m *= 2;
        }
        5.0 * (1.0 + 0.5 * passed as f32)
    }

    /// Transmissions needed for the next milestone after the win.
    pub fn next_milestone(&self) -> u32 {
        let mut m = MILESTONE_BASE;
        while m <= self.transmitted {
            m *= 2;
        }
        m
    }

    /// Production per minute of `item` over roughly the last `window` seconds.
    pub fn rate_per_min(&self, item: Item, window: u64) -> f32 {
        let now = self.produced.get(&item).copied().unwrap_or(0);
        let want = self.tick.saturating_sub(window * TICKS_PER_SEC as u64);
        let (t0, then) = self
            .history
            .iter()
            .find(|(t, _)| *t >= want)
            .map(|(t, m)| (*t, m.get(&item).copied().unwrap_or(0)))
            .unwrap_or((self.tick, now));
        let dt = self.tick.saturating_sub(t0);
        if dt == 0 {
            return 0.0;
        }
        (now - then) as f32 * 60.0 * TICKS_PER_SEC as f32 / dt as f32
    }

    /// Works out each network's supply for this tick from last tick's demand: sunlight first,
    /// then generators and turbines, then battery banks. Spare supply charges the banks.
    fn power_prepass(&mut self) {
        let nn = self.nets.len();
        let tps = TICKS_PER_SEC as f32;
        let light = self.daylight();
        let mut solar = vec![0.0f32; nn];
        let mut gen_cap = vec![0.0f32; nn];
        let mut gens = vec![0usize; nn];
        let mut bat_out = vec![0.0f32; nn];
        let mut bat_in = vec![0.0f32; nn];
        for n in &mut self.nets {
            n.stored = 0.0;
            n.storage = 0.0;
        }
        for &i in &self.power_sources {
            let net = self.power_net[i];
            if net == NO_NET {
                continue;
            }
            let n = net as usize;
            let e = self.entities[i].as_ref().unwrap();
            match &e.state {
                State::Generator { fuel, water, steam } => {
                    let wet = e.kind != EntityKind::SteamTurbine || *water > 0 || *steam > 0.0;
                    if fuel.has() && wet {
                        gen_cap[n] += e.kind.power_output();
                        gens[n] += 1;
                    }
                }
                State::Battery { charge } => {
                    bat_out[n] += BANK_RATE.min(charge * tps);
                    bat_in[n] += BANK_RATE.min((BANK_CAPACITY - charge) * tps);
                    self.nets[n].stored += charge;
                    self.nets[n].storage += BANK_CAPACITY;
                }
                _ => solar[n] += e.kind.power_output() * light,
            }
        }
        // Per network: (load on each generator, battery charge rate, battery discharge rate).
        let mut plan = vec![(0.0f32, 0.0f32, 0.0f32); nn];
        for (k, n) in self.nets.iter_mut().enumerate() {
            n.capacity = solar[k] + gen_cap[k] + bat_out[k];
            let demand = n.demand_prev;
            n.satisfaction = if n.capacity <= 0.0 {
                0.0
            } else if demand <= 0.0 {
                1.0
            } else {
                (n.capacity / demand).min(1.0)
            };
            n.load = demand.min(n.capacity);
            let from_solar = n.load.min(solar[k]);
            let from_gen = (n.load - from_solar).min(gen_cap[k]);
            let discharge = (n.load - from_solar - from_gen).max(0.0);
            let spare_solar = solar[k] - from_solar;
            let charge = (spare_solar + gen_cap[k] - from_gen).min(bat_in[k]);
            let gen_total = from_gen + (charge - spare_solar).max(0.0);
            n.battery_flow = charge - discharge;
            plan[k] = (gen_total / gens[k].max(1) as f32, charge, discharge);
        }
        for &i in &self.power_sources {
            let net = self.power_net[i];
            let e = self.entities[i].as_mut().unwrap();
            if net == NO_NET {
                e.status = Status::Idle;
                continue;
            }
            let n = net as usize;
            let (share, charge, discharge) = plan[n];
            let kind = e.kind;
            match &mut e.state {
                State::Generator { fuel, water, steam } => {
                    if !fuel.has() {
                        e.status = Status::NoFuel;
                        continue;
                    }
                    if kind == EntityKind::SteamTurbine && *water == 0 && *steam <= 0.0 {
                        e.status = Status::NoWater;
                        continue;
                    }
                    if share <= 0.0 {
                        e.status = Status::Idle;
                        continue;
                    }
                    let kj = share / tps;
                    if kind == EntityKind::SteamTurbine {
                        fuel.draw(kj / TURBINE_EFFICIENCY);
                        *steam -= kj;
                        while *steam <= 0.0 && *water > 0 {
                            *water -= 1;
                            *steam += STEAM_PER_WATER;
                        }
                        *steam = steam.max(0.0);
                    } else {
                        fuel.draw(kj);
                    }
                    e.status = Status::Working;
                }
                State::Battery { charge: stored } => {
                    let out = BANK_RATE.min(*stored * tps);
                    let room = BANK_RATE.min((BANK_CAPACITY - *stored) * tps);
                    let mut flow = 0.0;
                    if discharge > 0.0 && bat_out[n] > 0.0 {
                        flow -= discharge * out / bat_out[n];
                    }
                    if charge > 0.0 && bat_in[n] > 0.0 {
                        flow += charge * room / bat_in[n];
                    }
                    *stored = (*stored + flow / tps).clamp(0.0, BANK_CAPACITY);
                    e.status = if flow.abs() > 0.01 {
                        Status::Working
                    } else {
                        Status::Idle
                    };
                }
                _ => {
                    e.status = if self.nets[n].load > 0.0 && light > 0.0 {
                        Status::Working
                    } else {
                        Status::Idle
                    };
                }
            }
        }
    }

    fn belt_room(items: &[BeltItem], pos: u8) -> bool {
        items.iter().all(|b| b.pos.abs_diff(pos) >= SPACING)
    }

    fn belt_insert(items: &mut Vec<BeltItem>, item: Item, pos: u8) -> bool {
        if !Self::belt_room(items, pos) {
            return false;
        }
        let at = items
            .iter()
            .position(|b| b.pos > pos)
            .unwrap_or(items.len());
        items.insert(at, BeltItem { item, pos });
        true
    }

    fn update_belt(&mut self, i: usize) {
        let Some(mut e) = self.entities[i].take() else {
            return;
        };
        let (kind, dir) = (e.kind, e.dir);
        let speed = kind.belt_speed();
        let len = self.belt_len(i, kind);
        if let State::Belt { items, turn } = &mut e.state {
            let n = items.len();
            for k in (0..n).rev() {
                let limit = if k + 1 < n {
                    items[k + 1].pos.saturating_sub(SPACING)
                } else {
                    len
                };
                let p = items[k].pos;
                items[k].pos = (p + speed).min(limit).max(p);
            }
            if let Some(front) = items.last()
                && front.pos >= len
            {
                // Hand the front item on. A splitter starts with whichever output is next in
                // turn and falls through to the others if that one is full.
                let item = front.item;
                let (outs, n) = self.outputs(i, kind, dir);
                for k in 0..n {
                    let pick = (*turn as usize + k) % n;
                    let (j, pos) = outs[pick];
                    if let Some(Entity {
                        state: State::Belt { items: nitems, .. },
                        ..
                    }) = self.entities[j].as_mut()
                        && Self::belt_insert(nitems, item, pos)
                    {
                        items.pop();
                        *turn = ((pick + 1) % n) as u8;
                        break;
                    }
                }
            }
            e.status = if kind.is_tunnel() && self.tunnel_pair(i).is_none() {
                Status::Unlinked
            } else if items.is_empty() {
                Status::Idle
            } else {
                Status::Working
            };
        }
        self.entities[i] = Some(e);
    }

    /// How many of `item` the entity at `i` will accept. `manual` is a player insert (bigger buffers).
    pub fn accept_capacity(&self, i: usize, item: Item, manual: bool) -> u32 {
        let Some(e) = self.entities[i].as_ref() else {
            return 0;
        };
        // Modules only go in by hand.
        if item.is_module() {
            let room = e.kind.module_slots().saturating_sub(e.modules.len()) as u32;
            return if manual { room } else { 0 };
        }
        let fuel_cap: u32 = if manual { 50 } else { 5 };
        let fuel_room = |f: &Fuel| {
            if item.is_fuel() && (f.item.is_none() || f.item == Some(item) || f.stock == 0) {
                fuel_cap.saturating_sub(f.stock)
            } else {
                0
            }
        };
        match &e.state {
            State::Belt { items, .. } => {
                if !manual && Self::belt_room(items, TILE_LEN / 2) {
                    1
                } else {
                    0
                }
            }
            State::Arm { .. }
            | State::Pylon
            | State::Solar
            | State::Battery { .. }
            | State::Rail
            | State::Survey { .. } => 0,
            State::Miner { fuel, .. } => {
                if e.kind == EntityKind::Digger {
                    fuel_room(fuel)
                } else {
                    0
                }
            }
            State::Furnace { fuel, input, .. } => {
                if e.kind == EntityKind::Kiln && item.is_fuel() {
                    return fuel_room(fuel);
                }
                if !self.smeltable(item) {
                    return 0;
                }
                if input.iter().any(|(i, _)| i != item) {
                    return 0;
                }
                let cap = if manual { 100 } else { 10 };
                cap - input.count(item).min(cap)
            }
            State::Assembler { recipe, input, .. } => {
                let Some(r) = recipe else { return 0 };
                let need = r.get().input_count(item);
                if need == 0 {
                    return 0;
                }
                let cap = if manual { need * 20 } else { need * 2 };
                cap.saturating_sub(input.count(item))
            }
            State::Chest { inv, .. } => CRATE_CAP.saturating_sub(inv.total()),
            State::Generator { fuel, water, .. } => {
                if item == Item::Water && e.kind == EntityKind::SteamTurbine {
                    let cap = if manual { 200 } else { 20 };
                    cap - (*water).min(cap)
                } else {
                    fuel_room(fuel)
                }
            }
            State::Lab { inv, .. } => {
                if item.is_pack() {
                    let cap = if manual { 200 } else { 10 };
                    cap - inv.count(item).min(cap)
                } else {
                    0
                }
            }
            State::Uplink { inv, .. } => {
                if item == Item::RelayModule {
                    let cap = if manual { 100 } else { 5 };
                    cap - inv.count(item).min(cap)
                } else {
                    0
                }
            }
        }
    }

    pub fn smeltable(&self, item: Item) -> bool {
        RecipeId::all().any(|r| {
            let rec = r.get();
            rec.category == Category::Smelt
                && rec.inputs[0].0 == item
                && self.research.is_unlocked(r)
        })
    }

    /// Inserts up to `n` of `item` into the entity at `i`. Returns how many went in.
    pub fn insert(&mut self, i: usize, item: Item, n: u32, manual: bool) -> u32 {
        let cap = self.accept_capacity(i, item, manual).min(n);
        if cap == 0 {
            return 0;
        }
        let Some(e) = self.entities[i].as_mut() else {
            return 0;
        };
        if item.is_module() {
            e.modules.extend(std::iter::repeat_n(item, cap as usize));
            return cap;
        }
        let kind = e.kind;
        match &mut e.state {
            State::Belt { items, .. } => {
                if Self::belt_insert(items, item, TILE_LEN / 2) {
                    1
                } else {
                    0
                }
            }
            State::Generator { water, .. } if item == Item::Water => {
                *water += cap;
                cap
            }
            State::Miner { fuel, .. } | State::Generator { fuel, .. } => {
                fuel.add(item, cap);
                cap
            }
            State::Furnace { fuel, input, .. } => {
                // Only kilns burn fuel; an arc furnace bakes lignite into coke.
                if item.is_fuel() && kind == EntityKind::Kiln {
                    fuel.add(item, cap);
                } else {
                    input.add(item, cap);
                }
                cap
            }
            State::Assembler { input, .. } => {
                input.add(item, cap);
                cap
            }
            State::Chest { inv, .. } | State::Lab { inv, .. } | State::Uplink { inv, .. } => {
                inv.add(item, cap);
                cap
            }
            State::Arm { .. }
            | State::Pylon
            | State::Solar
            | State::Battery { .. }
            | State::Rail
            | State::Survey { .. } => 0,
        }
    }

    /// Items an arm could pick up from the entity at `i`, front-most / first first.
    pub fn takeable(&self, i: usize) -> Vec<Item> {
        let Some(e) = self.entities[i].as_ref() else {
            return vec![];
        };
        match &e.state {
            State::Belt { items, .. } => {
                // Only what's on the surface: a tunnel entrance also holds items underground.
                let mut v: Vec<Item> = vec![];
                for b in items.iter().rev().filter(|b| b.pos <= TILE_LEN) {
                    if !v.contains(&b.item) {
                        v.push(b.item);
                    }
                }
                v
            }
            State::Furnace { output, .. } | State::Assembler { output, .. } => output.items(),
            State::Chest { inv, .. } => inv.items(),
            _ => vec![],
        }
    }

    /// Removes one `item` from the entity at `i`.
    pub fn take_one(&mut self, i: usize, item: Item) -> bool {
        let Some(e) = self.entities[i].as_mut() else {
            return false;
        };
        match &mut e.state {
            State::Belt { items, .. } => {
                if let Some(k) = items
                    .iter()
                    .rposition(|b| b.item == item && b.pos <= TILE_LEN)
                {
                    items.remove(k);
                    true
                } else {
                    false
                }
            }
            State::Furnace { output, .. }
            | State::Assembler { output, .. }
            | State::Chest { inv: output, .. } => output.remove(item, 1),
            _ => false,
        }
    }

    fn update_machine(&mut self, i: usize) {
        let Some(mut e) = self.entities[i].take() else {
            return;
        };
        match e.kind {
            k if k.is_arm() => self.update_arm(i, &mut e),
            EntityKind::Digger | EntityKind::Bore | EntityKind::Sifter => {
                self.update_miner(i, &mut e)
            }
            EntityKind::Pump => self.update_pump(i, &mut e),
            EntityKind::Kiln | EntityKind::ArcFurnace => self.update_furnace(i, &mut e),
            EntityKind::Fabricator | EntityKind::ChemPlant => self.update_assembler(i, &mut e),
            EntityKind::Lab => self.update_lab(i, &mut e),
            EntityKind::Uplink => self.update_uplink(i, &mut e),
            EntityKind::SurveyMast => self.update_survey(i, &mut e),
            EntityKind::Crate | EntityKind::Depot => {
                e.status = Status::Idle;
            }
            _ => {}
        }
        self.entities[i] = Some(e);
    }

    /// Power-limited rate for an electric entity; registers its demand.
    fn electric(&mut self, i: usize, kind: EntityKind, status: &mut Status) -> f32 {
        self.electric_kw(i, kind.power_draw(), status)
    }

    /// Like `electric`, for a draw changed by modules.
    fn electric_kw(&mut self, i: usize, kw: f32, status: &mut Status) -> f32 {
        let sat = self.sat_at(i);
        self.add_demand(i, kw);
        if sat <= 0.0 {
            *status = Status::NoPower;
        } else if sat < 0.999 {
            *status = Status::LowPower;
        } else {
            *status = Status::Working;
        }
        sat
    }

    /// The tiles an arm at `i` facing `dir` takes from and drops to.
    pub fn arm_ends(&self, i: usize, kind: EntityKind, dir: Dir) -> (Option<usize>, Option<usize>) {
        let (x, y) = self.pos(i);
        let (dx, dy) = dir.delta();
        let n = kind.arm_reach();
        (
            self.idx(x - dx * n, y - dy * n),
            self.idx(x + dx * n, y + dy * n),
        )
    }

    fn update_arm(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let (back, front) = self.arm_ends(i, kind, e.dir);
        let mut status = Status::Idle;
        let State::Arm {
            hand,
            phase,
            t,
            filter,
        } = &mut e.state
        else {
            return;
        };
        let want = *filter;
        match phase {
            ArmPhase::Pickup => {
                // An unpowered arm can't move, so it mustn't grab anything either.
                let powered = self.sat_at(i) > 0.0;
                if hand.is_none() && !powered {
                    status = Status::NoPower;
                }
                if hand.is_none()
                    && powered
                    && let (Some(b), Some(f)) = (back, front)
                    && self.entities[f].is_some()
                {
                    let candidates = self.takeable(b);
                    if let Some(item) = candidates
                        .into_iter()
                        .filter(|&it| want.is_none_or(|w| w == it))
                        .find(|&it| self.accept_capacity(f, it, false) > 0)
                        && self.take_one(b, item)
                    {
                        *hand = Some(item);
                    }
                }
                if hand.is_some() {
                    *phase = ArmPhase::Swing;
                    *t = 0.0;
                }
            }
            ArmPhase::Swing => {
                let sat = self.electric(i, kind, &mut status);
                *t += kind.arm_speed() * self.arm_bonus * sat;
                if *t >= 1.0 {
                    *t = 1.0;
                    let item = hand.unwrap();
                    if front.is_some_and(|f| self.insert(f, item, 1, false) == 1) {
                        *hand = None;
                        *phase = ArmPhase::Return;
                        *t = 0.0;
                    } else {
                        status = Status::Blocked;
                    }
                }
            }
            ArmPhase::Return => {
                let sat = self.electric(i, kind, &mut status);
                *t += kind.arm_speed() * self.arm_bonus * sat;
                if *t >= 1.0 {
                    *t = 0.0;
                    *phase = ArmPhase::Pickup;
                }
            }
        }
        e.status = status;
    }

    /// Diggers and bores mining ore, and sifters working sand. Output goes to the tile ahead.
    fn update_miner(&mut self, i: usize, e: &mut Entity) {
        let front = self.neighbor(i, e.dir);
        let kind = e.kind;
        let fx = ModuleEffect::of(&e.modules);
        let mut status = Status::Working;
        // What to dig and how fast: a sifter's speed depends on how much sand it covers.
        let (ore_tile, area_share, cycle) = if kind == EntityKind::Sifter {
            (None, self.sand_in_area(i) as f32 / 9.0, SIFT_TIME)
        } else {
            let area = self.mining_area(i);
            let t = area.into_iter().find(|&j| self.tiles[j].ore.is_some());
            (t, 1.0, MINE_TIME)
        };
        let State::Miner {
            fuel,
            progress,
            output,
        } = &mut e.state
        else {
            return;
        };
        if let Some(item) = *output {
            if front.is_some_and(|f| self.insert(f, item, 1, false) == 1) {
                *output = None;
            } else {
                e.status = Status::Blocked;
                return;
            }
        }
        if kind != EntityKind::Sifter && ore_tile.is_none() || area_share <= 0.0 {
            e.status = Status::Depleted;
            return;
        }
        let base = if kind.uses_power() {
            self.electric_kw(i, kind.power_draw() * fx.power, &mut status)
        } else if fuel.draw(kind.fuel_draw() / TICKS_PER_SEC as f32) {
            1.0
        } else {
            e.status = Status::NoFuel;
            return;
        };
        let rate = base * kind.speed() * fx.speed * self.mining_bonus * area_share;
        *progress += rate / (cycle * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *progress -= 1.0;
            let got = match ore_tile {
                Some(ot) => {
                    let tile = &mut self.tiles[ot];
                    let ore = tile.ore.unwrap();
                    // Productivity modules sometimes mine an ore without using any up.
                    if e.bonus >= 1.0 {
                        e.bonus -= 1.0;
                    } else {
                        tile.amount = tile.amount.saturating_sub(1);
                        if tile.amount == 0 {
                            tile.ore = None;
                        }
                        e.bonus += fx.productivity;
                    }
                    ore
                }
                None => Item::Quartz,
            };
            *output = Some(got);
            *self.produced.entry(got).or_insert(0) += 1;
            if front.is_some_and(|f| self.insert(f, got, 1, false) == 1) {
                *output = None;
            }
        }
        e.status = status;
    }

    /// Pumps push water from the tile behind them to the one ahead. They need no power.
    fn update_pump(&mut self, i: usize, e: &mut Entity) {
        let front = self.neighbor(i, e.dir);
        let wet = self.water_behind(i, e.dir);
        let State::Miner {
            progress, output, ..
        } = &mut e.state
        else {
            return;
        };
        if output.is_some() {
            if front.is_some_and(|f| self.insert(f, Item::Water, 1, false) == 1) {
                *output = None;
            } else {
                e.status = Status::Blocked;
                return;
            }
        }
        if !wet {
            e.status = Status::NoWater;
            return;
        }
        *progress += PUMP_RATE * self.mining_bonus / TICKS_PER_SEC as f32;
        if *progress >= 1.0 {
            *progress -= 1.0;
            *self.produced.entry(Item::Water).or_insert(0) += 1;
            if !front.is_some_and(|f| self.insert(f, Item::Water, 1, false) == 1) {
                *output = Some(Item::Water);
            }
        }
        e.status = Status::Working;
    }

    fn update_furnace(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let fx = ModuleEffect::of(&e.modules);
        let mut status = Status::Working;
        let State::Furnace {
            fuel,
            input,
            output,
            job,
            progress,
        } = &mut e.state
        else {
            return;
        };
        if job.is_none() {
            let mut blocked = false;
            for r in RecipeId::all() {
                let rec = r.get();
                if rec.category != Category::Smelt || !self.research.is_unlocked(r) {
                    continue;
                }
                if !input.has_all(rec.inputs, 1) {
                    continue;
                }
                let (out, n) = rec.outputs[0];
                if output.iter().any(|(o, _)| o != out) || output.count(out) + n > 100 {
                    blocked = true;
                    continue;
                }
                input.remove_all(rec.inputs, 1);
                *job = Some(r);
                *progress = 0.0;
                break;
            }
            if job.is_none() {
                e.status = if blocked {
                    Status::Blocked
                } else {
                    Status::Idle
                };
                return;
            }
        }
        let rec = job.unwrap().get();
        let rate = if kind == EntityKind::ArcFurnace {
            self.electric_kw(i, kind.power_draw() * fx.power, &mut status) * kind.speed() * fx.speed
        } else if fuel.draw(kind.fuel_draw() / TICKS_PER_SEC as f32) {
            kind.speed()
        } else {
            e.status = Status::NoFuel;
            return;
        };
        *progress += rate / (rec.time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            let times = Self::with_bonus(&mut e.bonus, fx.productivity);
            output.add_all(rec.outputs, times);
            for &(o, n) in rec.outputs {
                *self.produced.entry(o).or_insert(0) += (n * times) as u64;
            }
            *job = None;
            *progress = 0.0;
        }
        e.status = status;
    }

    /// How many batches one finished craft yields: one, plus one more each time the
    /// productivity bonus fills up.
    fn with_bonus(bonus: &mut f32, productivity: f32) -> u32 {
        *bonus += productivity;
        if *bonus >= 1.0 {
            *bonus -= 1.0;
            2
        } else {
            1
        }
    }

    fn update_assembler(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let fx = ModuleEffect::of(&e.modules);
        let mut status = Status::Working;
        let State::Assembler {
            recipe,
            input,
            output,
            working,
            progress,
        } = &mut e.state
        else {
            return;
        };
        let Some(r) = *recipe else {
            e.status = Status::NoRecipe;
            return;
        };
        let rec = r.get();
        if !*working {
            let (out, n) = rec.outputs[0];
            if output.count(out) + n > 100 {
                e.status = Status::Blocked;
                return;
            }
            if !input.remove_all(rec.inputs, 1) {
                e.status = Status::NoInput;
                return;
            }
            *working = true;
            *progress = 0.0;
        }
        let rate = self.electric_kw(i, kind.power_draw() * fx.power, &mut status)
            * kind.speed()
            * fx.speed;
        *progress += rate / (rec.time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            let times = Self::with_bonus(&mut e.bonus, fx.productivity);
            output.add_all(rec.outputs, times);
            for &(o, n) in rec.outputs {
                *self.produced.entry(o).or_insert(0) += (n * times) as u64;
            }
            *working = false;
            *progress = 0.0;
        }
        e.status = status;
    }

    fn update_lab(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let fx = ModuleEffect::of(&e.modules);
        let mut status = Status::Working;
        let Some(tech) = self.research.current else {
            e.status = Status::NoResearch;
            return;
        };
        let State::Lab {
            inv,
            working,
            progress,
            packs,
        } = &mut e.state
        else {
            return;
        };
        let t = tech.get();
        if !*working {
            if !t.packs.iter().all(|&p| inv.count(p) >= 1) {
                e.status = Status::NoInput;
                return;
            }
            packs.clear();
            for &p in t.packs {
                inv.remove(p, 1);
                packs.push(p);
            }
            *working = true;
            *progress = 0.0;
        }
        let rate = self.electric_kw(i, kind.power_draw() * fx.power, &mut status)
            * kind.speed()
            * fx.speed;
        *progress += rate / (t.unit_time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *working = false;
            *progress = 0.0;
            packs.clear();
            for _ in 0..Self::with_bonus(&mut e.bonus, fx.productivity) {
                if let Some(done) = self.research.add_unit() {
                    let level = self.research.level(done);
                    let mut msg = if done.get().repeatable {
                        format!("Research complete: {} {level}", done.get().name)
                    } else {
                        format!("Research complete: {}", done.get().name)
                    };
                    if let Some(next) = self.research.current
                        && next != done
                    {
                        msg.push_str(&format!(". Now researching {}", next.get().name));
                    }
                    self.events.push(msg);
                }
            }
        }
        e.status = status;
    }

    fn update_uplink(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let mut status = Status::Working;
        let State::Uplink {
            inv,
            progress,
            working,
        } = &mut e.state
        else {
            return;
        };
        if !*working {
            if !inv.remove(Item::RelayModule, 1) {
                e.status = Status::NoInput;
                return;
            }
            *working = true;
            *progress = 0.0;
        }
        let rate = self.electric(i, kind, &mut status);
        *progress += rate / (self.transmission_secs() * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *working = false;
            *progress = 0.0;
            let before = self.next_milestone();
            self.transmitted += 1;
            if self.transmitted >= UPLINK_GOAL && !self.won && self.scenario.goal == Goal::Uplink {
                self.won = true;
                self.won_at = Some(self.tick);
                self.events
                    .push("The Uplink is transmitting. You win!".into());
            } else if self.won && self.transmitted >= before {
                self.events.push(format!(
                    "Milestone: {} transmissions. Each one now takes {:.1} s; next milestone at {}.",
                    self.transmitted,
                    self.transmission_secs(),
                    self.next_milestone()
                ));
            }
        }
        e.status = status;
    }

    fn update_survey(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
        let State::Survey { radius } = &mut e.state else {
            return;
        };
        if *radius >= SURVEY_MAX {
            e.status = Status::Idle;
            return;
        }
        let mut status = Status::Working;
        let rate = self.electric(i, kind, &mut status);
        let before = radius.floor();
        *radius = (*radius + SURVEY_SPEED * rate / TICKS_PER_SEC as f32).min(SURVEY_MAX);
        if radius.floor() > before {
            let (x, y) = self.pos(i);
            self.reveal(x, y, *radius as i32);
        }
        e.status = status;
    }

    // ---- Rail carts ----

    fn is_track(&self, i: usize) -> bool {
        self.entities[i].as_ref().is_some_and(|e| e.kind.is_track())
    }

    /// Whether the depot at `i` unloads carts, or `None` if there is no depot there.
    pub fn depot_unloads(&self, i: usize) -> Option<bool> {
        match self.entities[i].as_ref() {
            Some(Entity {
                kind: EntityKind::Depot,
                state: State::Chest { unload, .. },
                ..
            }) => Some(*unload),
            _ => None,
        }
    }

    /// Switches a depot between loading and unloading carts. Returns the new setting.
    pub fn toggle_depot(&mut self, x: i32, y: i32) -> Option<bool> {
        match self.entity_mut(x, y) {
            Some(Entity {
                kind: EntityKind::Depot,
                state: State::Chest { unload, .. },
                ..
            }) => {
                *unload = !*unload;
                Some(*unload)
            }
            _ => None,
        }
    }

    /// Puts a new, empty cart on the rail or depot at (x, y).
    pub fn add_cart(&mut self, x: i32, y: i32) -> Result<(), String> {
        let i = self.idx(x, y).ok_or("Out of bounds")?;
        if !self.is_track(i) {
            return Err("Rail carts go on a rail or a depot".into());
        }
        self.carts.push(Cart::new(i));
        Ok(())
    }

    /// The carts shown on tile `i`.
    pub fn carts_at(&self, i: usize) -> impl Iterator<Item = &Cart> {
        self.carts.iter().filter(move |c| c.shown_at() == i)
    }

    /// Takes the carts on (or just leaving) tile `i` off the map, newest first.
    pub fn take_carts(&mut self, i: usize, max: usize) -> Vec<Cart> {
        let mut out = vec![];
        let mut k = self.carts.len();
        while k > 0 && out.len() < max {
            k -= 1;
            if self.carts[k].at as usize == i || self.carts[k].shown_at() == i {
                out.push(self.carts.remove(k));
            }
        }
        out
    }

    /// Breadth-first search along the track from `from`: each reached tile's previous tile,
    /// and the tiles in the order they were reached.
    fn track_search(&self, from: usize) -> (HashMap<usize, usize>, Vec<usize>) {
        let mut prev = HashMap::new();
        prev.insert(from, from);
        let mut order = vec![from];
        let mut k = 0;
        while k < order.len() {
            let cur = order[k];
            k += 1;
            for d in [Dir::North, Dir::East, Dir::South, Dir::West] {
                if let Some(n) = self.neighbor(cur, d)
                    && !prev.contains_key(&n)
                    && self.is_track(n)
                {
                    prev.insert(n, cur);
                    order.push(n);
                }
            }
        }
        (prev, order)
    }

    /// Carts and depots on the track that `i` is part of.
    pub fn track_summary(&self, i: usize) -> (usize, usize) {
        if !self.is_track(i) {
            return (0, 0);
        }
        let (prev, order) = self.track_search(i);
        let carts = self
            .carts
            .iter()
            .filter(|c| prev.contains_key(&(c.at as usize)))
            .count();
        let depots = order
            .iter()
            .filter(|&&t| self.depot_unloads(t).is_some())
            .count();
        (carts, depots)
    }

    /// The tiles from `from` to `to`, `to` first and the first step last.
    fn path_back(prev: &HashMap<usize, usize>, from: usize, to: usize) -> Vec<u32> {
        let mut v = vec![];
        let mut cur = to;
        while cur != from {
            v.push(cur as u32);
            cur = prev[&cur];
        }
        v
    }

    /// A path from `from` to the depot `to` along the track.
    fn path_between(&self, from: usize, to: usize) -> Option<Vec<u32>> {
        let (prev, _) = self.track_search(from);
        (to != from && prev.contains_key(&to)).then(|| Self::path_back(&prev, from, to))
    }

    /// A path from `from` to the next depot on the same track: the one after it in map order,
    /// wrapping around. Two depots make a shuttle; more are visited in turn.
    pub fn route(&self, from: usize) -> Option<Vec<u32>> {
        let (prev, order) = self.track_search(from);
        let depots: Vec<usize> = order
            .into_iter()
            .filter(|&d| d != from && self.depot_unloads(d).is_some())
            .collect();
        let to = depots
            .iter()
            .copied()
            .filter(|&d| d > from)
            .min()
            .or_else(|| depots.iter().copied().min())?;
        Some(Self::path_back(&prev, from, to))
    }

    fn update_carts(&mut self) {
        let mut carts = std::mem::take(&mut self.carts);
        for c in &mut carts {
            self.update_cart(c);
        }
        self.carts = carts;
    }

    fn depart(&self, c: &mut Cart) {
        match self.route(c.at as usize) {
            Some(path) => {
                c.path = path;
                c.moved = 0.0;
                c.stopped = 0;
                c.idle = 0;
                c.status = CartStatus::Moving;
            }
            None => c.status = CartStatus::NoRoute,
        }
    }

    /// Moves up to `CART_TRANSFER` items between a stopped cart and its depot.
    fn transfer(&mut self, c: &mut Cart, depot: usize, unload: bool) -> u32 {
        let Some(Entity {
            state: State::Chest { inv, .. },
            ..
        }) = self.entities[depot].as_mut()
        else {
            return 0;
        };
        let mut moved = 0;
        for _ in 0..CART_TRANSFER {
            let (from, to, cap) = if unload {
                (&mut c.cargo, &mut *inv, CRATE_CAP)
            } else {
                (&mut *inv, &mut c.cargo, CART_CAP)
            };
            if to.total() >= cap {
                break;
            }
            let Some(it) = from.first() else { break };
            from.remove(it, 1);
            to.add(it, 1);
            moved += 1;
        }
        moved
    }

    fn update_cart(&mut self, c: &mut Cart) {
        let tps = TICKS_PER_SEC;
        if !c.path.is_empty() {
            c.status = CartStatus::Moving;
            c.moved += CART_SPEED / tps as f32;
            while c.moved >= 1.0 {
                let next = *c.path.last().unwrap() as usize;
                if !self.is_track(next) {
                    // The track ahead was taken up: find another way there, or wait here.
                    let dest = c.path[0] as usize;
                    c.moved = 0.0;
                    c.path = self.path_between(c.at as usize, dest).unwrap_or_default();
                    if c.path.is_empty() {
                        c.status = CartStatus::NoRoute;
                    }
                    return;
                }
                c.path.pop();
                c.at = next as u32;
                c.moved -= 1.0;
                if c.path.is_empty() {
                    c.moved = 0.0;
                    c.stopped = 0;
                    c.idle = 0;
                    break;
                }
            }
            return;
        }
        let here = c.at as usize;
        c.stopped += 1;
        let Some(unload) = self.depot_unloads(here) else {
            // Stopped out on the line, maybe because its depot went away: look for another.
            if c.stopped % tps == 1 {
                self.depart(c);
            }
            return;
        };
        if self.transfer(c, here, unload) > 0 {
            c.idle = 0;
            c.status = if unload {
                CartStatus::Unloading
            } else {
                CartStatus::Loading
            };
            return;
        }
        c.idle += 1;
        let leave = if unload {
            c.cargo.is_empty()
        } else {
            c.cargo.total() >= CART_CAP
                || (!c.cargo.is_empty()
                    && (c.idle >= CART_IDLE_SECS * tps || c.stopped >= CART_MAX_STOP_SECS * tps))
        };
        if !leave {
            c.status = if unload {
                CartStatus::DepotFull
            } else {
                CartStatus::Waiting
            };
        } else if c.status != CartStatus::NoRoute || c.idle % tps == 1 {
            self.depart(c);
        }
    }

    // ---- Serialization helpers ----

    pub fn to_save(&self) -> WorldSave {
        let terrain: String = self
            .tiles
            .iter()
            .map(|t| match t.terrain {
                Terrain::Grass => 'g',
                Terrain::Sand => 's',
                Terrain::Water => 'w',
            })
            .collect();
        let ores = self
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.ore.map(|o| (i as u32, o, t.amount)))
            .collect();
        let entities = self
            .entities
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.clone().map(|e| (i as u32, e)))
            .collect();
        WorldSave {
            width: self.width,
            height: self.height,
            seed: self.seed,
            tick: self.tick,
            terrain,
            ores,
            entities,
            research: self.research.clone(),
            won: self.won,
            transmitted: self.transmitted,
            won_at: self.won_at,
            produced: self.produced.clone(),
            scenario: if self.scenario.is_standard() {
                String::new()
            } else {
                self.scenario.id.to_string()
            },
            explored: (!self.explored.iter().all(|&e| e)).then(|| encode_runs(&self.explored)),
            carts: self.carts.clone(),
        }
    }

    pub fn from_save(s: WorldSave) -> Result<World, String> {
        if !(1..=4096).contains(&s.width) || !(1..=4096).contains(&s.height) {
            return Err(format!("bad map size {}x{}", s.width, s.height));
        }
        let mut w = World::empty(s.width, s.height, s.seed);
        let n = (s.width * s.height) as usize;
        if s.terrain.chars().count() != n {
            return Err("terrain size mismatch".into());
        }
        for (i, c) in s.terrain.chars().enumerate() {
            w.tiles[i].terrain = match c {
                's' => Terrain::Sand,
                'w' => Terrain::Water,
                _ => Terrain::Grass,
            };
        }
        for (i, o, a) in s.ores {
            let t = w
                .tiles
                .get_mut(i as usize)
                .ok_or("ore index out of range")?;
            t.ore = Some(o);
            t.amount = a;
        }
        for (i, e) in s.entities {
            *w.entities
                .get_mut(i as usize)
                .ok_or("entity index out of range")? = Some(e);
        }
        w.tick = s.tick;
        w.research = s.research;
        w.won = s.won;
        w.transmitted = s.transmitted;
        w.won_at = s.won_at;
        w.produced = s.produced;
        if !s.scenario.is_empty() {
            w.scenario = Scenario::by_id(&s.scenario)
                .ok_or_else(|| format!("unknown scenario {}", s.scenario))?;
        }
        if let Some(runs) = &s.explored {
            w.explored = decode_runs(runs, n)?;
        }
        for c in &s.carts {
            if c.at as usize >= n || c.path.iter().any(|&p| p as usize >= n) {
                return Err("cart position out of range".into());
            }
        }
        w.carts = s.carts;
        Ok(w)
    }
}

#[derive(Serialize, Deserialize)]
pub struct WorldSave {
    pub width: i32,
    pub height: i32,
    pub seed: u64,
    pub tick: u64,
    pub terrain: String,
    pub ores: Vec<(u32, Item, u32)>,
    pub entities: Vec<(u32, Entity)>,
    pub research: Research,
    pub won: bool,
    #[serde(default)]
    pub transmitted: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub won_at: Option<u64>,
    pub produced: BTreeMap<Item, u64>,
    /// Empty for free play.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scenario: String,
    /// Explored tiles as run lengths (see `encode_runs`). Missing in older saves: all explored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explored: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carts: Vec<Cart>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: i32, h: i32) -> World {
        World::empty(w, h, 1)
    }

    fn set_ore(w: &mut World, x: i32, y: i32, ore: Item, n: u32) {
        let i = w.idx(x, y).unwrap();
        w.tiles[i].ore = Some(ore);
        w.tiles[i].amount = n;
    }

    fn run(w: &mut World, secs: u32) {
        for _ in 0..secs * TICKS_PER_SEC {
            w.tick();
        }
    }

    fn crate_count(w: &World, x: i32, y: i32, item: Item) -> u32 {
        match &w.entity(x, y).unwrap().state {
            State::Chest { inv, .. } => inv.count(item),
            _ => panic!("not a crate"),
        }
    }

    fn fuel(w: &mut World, x: i32, y: i32, n: u32) {
        let i = w.idx(x, y).unwrap();
        assert_eq!(w.insert(i, Item::Lignite, n, true), n);
    }

    /// A pylon and a fueled generator powering the area around (x, y).
    fn power(w: &mut World, x: i32, y: i32) {
        w.place(x, y, EntityKind::Pylon, Dir::North).unwrap();
        w.place(x + 1, y, EntityKind::Generator, Dir::North)
            .unwrap();
        fuel(w, x + 1, y, 50);
    }

    #[test]
    fn generated_map_has_starter_ores_near_spawn() {
        let w = World::generate(160, 120, 42);
        let (cx, cy) = w.spawn();
        for ore in ORES {
            let near = (cy - 20..=cy + 20).any(|y| {
                (cx - 20..=cx + 20).any(|x| w.tile(x, y).is_some_and(|t| t.ore == Some(ore)))
            });
            assert!(near, "{ore:?} missing near spawn");
        }
        assert_ne!(w.tile(cx, cy).unwrap().terrain, Terrain::Water);
    }

    #[test]
    fn generation_is_deterministic() {
        let a = World::generate(80, 60, 7);
        let b = World::generate(80, 60, 7);
        assert_eq!(a.tiles, b.tiles);
        let c = World::generate(80, 60, 8);
        assert_ne!(a.tiles, c.tiles);
    }

    #[test]
    fn belt_carries_items_along_a_line() {
        let mut w = flat(20, 5);
        for x in 0..10 {
            w.place(x, 2, EntityKind::Conveyor, Dir::East).unwrap();
        }
        let i = w.idx(0, 2).unwrap();
        w.ensure_caches();
        assert_eq!(w.insert(i, Item::Cog, 1, false), 1);
        run(&mut w, 5);
        // Item should have reached the end and be waiting there.
        match &w.entity(9, 2).unwrap().state {
            State::Belt { items, .. } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].pos, TILE_LEN);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn belt_respects_spacing_and_backs_up() {
        let mut w = flat(10, 3);
        for x in 0..3 {
            w.place(x, 1, EntityKind::Conveyor, Dir::East).unwrap();
        }
        w.ensure_caches();
        let src = w.idx(0, 1).unwrap();
        let mut inserted = 0;
        for _ in 0..400 {
            if w.insert(src, Item::Coil, 1, false) == 1 {
                inserted += 1;
            }
            w.tick();
        }
        let total: usize = (0..3)
            .map(|x| match &w.entity(x, 1).unwrap().state {
                State::Belt { items, .. } => {
                    for pair in items.windows(2) {
                        assert!(pair[1].pos - pair[0].pos >= SPACING);
                    }
                    items.len()
                }
                _ => 0,
            })
            .sum();
        assert_eq!(total, inserted);
        assert!(total >= 10, "belt should fill up, got {total}");
    }

    #[test]
    fn side_loading_merges_lines() {
        let mut w = flat(10, 10);
        // Main line going east along y=5, feeder from the north at x=3.
        for x in 0..6 {
            w.place(x, 5, EntityKind::Conveyor, Dir::East).unwrap();
        }
        w.place(3, 4, EntityKind::Conveyor, Dir::South).unwrap();
        w.ensure_caches();
        let f = w.idx(3, 4).unwrap();
        w.insert(f, Item::Glass, 1, false);
        run(&mut w, 3);
        match &w.entity(5, 5).unwrap().state {
            State::Belt { items, .. } => assert_eq!(items[0].item, Item::Glass),
            _ => unreachable!(),
        }
    }

    #[test]
    fn digger_feeds_kiln_and_arm_moves_ingots_to_crate() {
        let mut w = flat(12, 12);
        for y in 4..7 {
            for x in 4..7 {
                set_ore(&mut w, x, y, Item::FerriteOre, 100);
            }
        }
        // Digger at (5,5) facing east into a kiln at (6,5); arm at (7,5) to crate at (8,5).
        w.place(5, 5, EntityKind::Digger, Dir::East).unwrap();
        w.place(6, 5, EntityKind::Kiln, Dir::North).unwrap();
        w.place(7, 5, EntityKind::Arm, Dir::East).unwrap();
        w.place(8, 5, EntityKind::Crate, Dir::North).unwrap();
        power(&mut w, 7, 7);
        fuel(&mut w, 5, 5, 10);
        fuel(&mut w, 6, 5, 10);
        run(&mut w, 60);
        let n = crate_count(&w, 8, 5, Item::IronIngot);
        assert!(n >= 10, "expected ingots in crate, got {n}");
        assert!(w.produced[&Item::FerriteOre] >= n as u64);
    }

    #[test]
    fn arms_need_power() {
        let mut w = flat(10, 10);
        w.place(2, 2, EntityKind::Crate, Dir::North).unwrap();
        w.place(3, 2, EntityKind::Arm, Dir::East).unwrap();
        w.place(4, 2, EntityKind::Crate, Dir::North).unwrap();
        let c = w.idx(2, 2).unwrap();
        w.insert(c, Item::Cog, 10, true);
        run(&mut w, 5);
        assert_eq!(crate_count(&w, 4, 2, Item::Cog), 0);
        assert_eq!(w.entity(3, 2).unwrap().status, Status::NoPower);
        // An unpowered arm doesn't grab anything either.
        assert_eq!(crate_count(&w, 2, 2, Item::Cog), 10);
        match &w.entity(3, 2).unwrap().state {
            State::Arm { hand, .. } => assert_eq!(*hand, None),
            _ => unreachable!(),
        }
        power(&mut w, 3, 4);
        run(&mut w, 5);
        assert!(crate_count(&w, 4, 2, Item::Cog) >= 4);
    }

    #[test]
    fn arm_only_picks_what_target_accepts() {
        let mut w = flat(10, 10);
        w.place(2, 2, EntityKind::Crate, Dir::North).unwrap();
        w.place(3, 2, EntityKind::Arm, Dir::East).unwrap();
        w.place(4, 2, EntityKind::Lab, Dir::North).unwrap();
        power(&mut w, 3, 4);
        let c = w.idx(2, 2).unwrap();
        w.insert(c, Item::Cog, 10, true);
        w.insert(c, Item::AlphaPack, 3, true);
        run(&mut w, 10);
        assert_eq!(crate_count(&w, 2, 2, Item::Cog), 10);
        assert_eq!(crate_count(&w, 2, 2, Item::AlphaPack), 0);
    }

    #[test]
    fn fabricator_crafts_with_power() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::Fabricator, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let i = w.idx(3, 3).unwrap();
        if let State::Assembler { recipe, .. } = &mut w.entities[i].as_mut().unwrap().state {
            *recipe = RecipeId::by_id("cog");
        }
        w.insert(i, Item::IronIngot, 10, true);
        run(&mut w, 5);
        match &w.entity(3, 3).unwrap().state {
            State::Assembler { output, .. } => assert_eq!(output.count(Item::Cog), 5),
            _ => unreachable!(),
        }
    }

    #[test]
    fn overloaded_network_runs_slower() {
        let mut w = flat(30, 10);
        // One sun panel (60 kW) against two fabricators (150 kW).
        w.place(5, 5, EntityKind::Pylon, Dir::North).unwrap();
        w.place(6, 5, EntityKind::SunPanel, Dir::North).unwrap();
        for x in [4, 5] {
            w.place(x, 4, EntityKind::Fabricator, Dir::North).unwrap();
            let i = w.idx(x, 4).unwrap();
            if let State::Assembler { recipe, .. } = &mut w.entities[i].as_mut().unwrap().state {
                *recipe = RecipeId::by_id("cog");
            }
            w.insert(i, Item::IronIngot, 40, true);
        }
        run(&mut w, 10);
        let sat = w.net_at(w.idx(5, 5).unwrap()).unwrap().satisfaction;
        assert!((sat - 0.4).abs() < 0.05, "sat {sat}");
        assert_eq!(w.entity(4, 4).unwrap().status, Status::LowPower);
    }

    #[test]
    fn pylons_link_within_reach() {
        let mut w = flat(40, 10);
        w.place(2, 5, EntityKind::Pylon, Dir::North).unwrap();
        w.place(9, 5, EntityKind::Pylon, Dir::North).unwrap();
        w.place(30, 5, EntityKind::Pylon, Dir::North).unwrap();
        w.ensure_caches();
        assert_eq!(w.nets.len(), 2);
    }

    #[test]
    fn labs_research_and_unlock() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::Lab, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let fab = crate::recipes::TechId::by_id("fabrication").unwrap();
        w.research.start(fab).unwrap();
        let i = w.idx(3, 3).unwrap();
        w.insert(i, Item::AlphaPack, 10, true);
        run(&mut w, 40);
        assert!(w.research.is_done(fab));
        assert!(w.events.iter().any(|e| e.contains("Fabrication")));
    }

    #[test]
    fn picking_up_a_busy_lab_refunds_its_packs() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::Lab, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let fab = crate::recipes::TechId::by_id("fabrication").unwrap();
        w.research.start(fab).unwrap();
        let i = w.idx(3, 3).unwrap();
        w.insert(i, Item::AlphaPack, 2, true);
        run(&mut w, 1);
        let e = w.remove(3, 3).unwrap();
        // One pack in progress plus one waiting: both come back.
        assert_eq!(e.contents().count(Item::AlphaPack), 2);
    }

    #[test]
    fn rolling_rate_tracks_recent_production() {
        let mut w = flat(6, 6);
        set_ore(&mut w, 2, 2, Item::Quartz, 10_000);
        w.place(2, 2, EntityKind::Digger, Dir::East).unwrap();
        w.place(3, 2, EntityKind::Crate, Dir::North).unwrap();
        fuel(&mut w, 2, 2, 50);
        run(&mut w, 70);
        // A digger mines 0.5 ore/s = 30/min.
        let r = w.rate_per_min(Item::Quartz, 60);
        assert!((r - 30.0).abs() < 3.0, "rate {r}");
    }

    #[test]
    fn steel_needs_metallurgy() {
        let w = flat(4, 4);
        assert!(!w.smeltable(Item::IronIngot));
        let mut w = w;
        w.research
            .complete(crate::recipes::TechId::by_id("metallurgy").unwrap());
        assert!(w.smeltable(Item::IronIngot));
    }

    #[test]
    fn miners_deplete_ore() {
        let mut w = flat(6, 6);
        set_ore(&mut w, 2, 2, Item::Quartz, 3);
        w.place(2, 2, EntityKind::Digger, Dir::East).unwrap();
        w.place(3, 2, EntityKind::Crate, Dir::North).unwrap();
        fuel(&mut w, 2, 2, 5);
        run(&mut w, 20);
        assert_eq!(crate_count(&w, 3, 2, Item::Quartz), 3);
        assert_eq!(w.tile(2, 2).unwrap().ore, None);
        assert_eq!(w.entity(2, 2).unwrap().status, Status::Depleted);
    }

    #[test]
    fn uplink_wins() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::Uplink, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let i = w.idx(3, 3).unwrap();
        w.insert(i, Item::RelayModule, UPLINK_GOAL, true);
        run(&mut w, 5 * UPLINK_GOAL + 5);
        assert!(w.won);
    }

    fn research(w: &mut World, id: &str) {
        w.research
            .complete(crate::recipes::TechId::by_id(id).unwrap());
    }

    fn set_terrain(w: &mut World, x: i32, y: i32, t: Terrain) {
        let i = w.idx(x, y).unwrap();
        w.tiles[i].terrain = t;
    }

    fn assembler_output(w: &World, x: i32, y: i32, item: Item) -> u32 {
        match &w.entity(x, y).unwrap().state {
            State::Assembler { output, .. } | State::Furnace { output, .. } => output.count(item),
            _ => panic!("not a machine"),
        }
    }

    fn set_recipe(w: &mut World, x: i32, y: i32, id: &str) {
        if let State::Assembler { recipe, .. } = &mut w.entity_mut(x, y).unwrap().state {
            *recipe = RecipeId::by_id(id);
        }
    }

    #[test]
    fn days_and_nights_follow_a_cycle() {
        assert_eq!(daylight_at(0), 1.0);
        assert_eq!(daylight_at(DAY_TICKS / 2), 1.0);
        assert_eq!(daylight_at(DAY_TICKS * 3 / 4), 0.0);
        assert_eq!(daylight_at(DAY_TICKS + 1), 1.0);
        let dusk = daylight_at(DAY_TICKS * 6 / 10);
        assert!(dusk > 0.0 && dusk < 1.0);
        let avg: f32 = (0..DAY_TICKS).map(daylight_at).sum::<f32>() / DAY_TICKS as f32;
        assert!((avg - 0.65).abs() < 0.01, "{avg}");
    }

    #[test]
    fn battery_banks_store_sunlight_for_the_night() {
        // Ten panels (600 kW at noon) run a lab (60 kW) and fill a bank by day.
        let mut w = flat(20, 10);
        w.place(5, 5, EntityKind::Pylon, Dir::North).unwrap();
        for k in 0..5 {
            w.place(3 + k, 3, EntityKind::SunPanel, Dir::North).unwrap();
            w.place(3 + k, 7, EntityKind::SunPanel, Dir::North).unwrap();
        }
        w.place(4, 5, EntityKind::BatteryBank, Dir::North).unwrap();
        w.place(6, 5, EntityKind::Fabricator, Dir::North).unwrap();
        set_recipe(&mut w, 6, 5, "cog");
        let fab = w.idx(6, 5).unwrap();
        w.insert(fab, Item::IronIngot, 40, true);
        run(&mut w, 60);
        let charged = match w.entity(4, 5).unwrap().state {
            State::Battery { charge } => charge,
            _ => unreachable!(),
        };
        assert!(charged > 15_000.0, "{charged}");
        let net = *w.net_at(fab).unwrap();
        assert!(net.storage == BANK_CAPACITY && net.stored > 15_000.0);
        // Jump to the middle of the night: the bank keeps the fabricator going.
        w.insert(fab, Item::IronIngot, 40, true);
        w.tick = DAY_TICKS * 3 / 4;
        w.tick();
        w.tick();
        assert_eq!(w.daylight(), 0.0);
        assert!(w.net_at(fab).unwrap().battery_flow < 0.0);
        let before = assembler_output(&w, 6, 5, Item::Cog);
        run(&mut w, 10);
        assert_eq!(assembler_output(&w, 6, 5, Item::Cog), before + 20);
        let after = match w.entity(4, 5).unwrap().state {
            State::Battery { charge } => charge,
            _ => unreachable!(),
        };
        assert!(after < charged);
    }

    #[test]
    fn solar_alone_stops_at_night() {
        let mut w = flat(10, 10);
        w.place(5, 5, EntityKind::Pylon, Dir::North).unwrap();
        w.place(4, 5, EntityKind::SunPanel, Dir::North).unwrap();
        w.place(6, 5, EntityKind::Lab, Dir::North).unwrap();
        w.tick = DAY_TICKS * 3 / 4;
        run(&mut w, 1);
        let i = w.idx(6, 5).unwrap();
        assert_eq!(w.net_at(i).unwrap().capacity, 0.0);
    }

    #[test]
    fn pumps_feed_steam_turbines() {
        let mut w = flat(12, 8);
        for y in 0..8 {
            set_terrain(&mut w, 0, y, Terrain::Water);
        }
        // A pump with no water behind can't go down, and a turned one reports it.
        assert!(w.place(3, 3, EntityKind::Pump, Dir::East).is_err());
        w.place(1, 3, EntityKind::Pump, Dir::East).unwrap();
        w.place(2, 3, EntityKind::SteamTurbine, Dir::North).unwrap();
        w.place(2, 4, EntityKind::Pylon, Dir::North).unwrap();
        w.place(3, 4, EntityKind::Fabricator, Dir::North).unwrap();
        set_recipe(&mut w, 3, 4, "cog");
        let t = w.idx(2, 3).unwrap();
        let fab = w.idx(3, 4).unwrap();
        w.insert(fab, Item::IronIngot, 40, true);
        // Fuel but no water yet: a turbine needs both.
        w.place(1, 6, EntityKind::Crate, Dir::North).unwrap();
        assert_eq!(w.insert(t, Item::Coke, 10, true), 10);
        run(&mut w, 5);
        assert!(assembler_output(&w, 3, 4, Item::Cog) > 0);
        assert_eq!(w.entity(2, 3).unwrap().status, Status::Working);
        assert!(w.produced[&Item::Water] >= 9);
        // Turn the pump away from the water: it runs dry.
        w.rotate(1, 3);
        run(&mut w, 1);
        assert_eq!(w.entity(1, 3).unwrap().status, Status::NoWater);
        // A turbine with fuel but no water says so.
        w.place(3, 5, EntityKind::SteamTurbine, Dir::North).unwrap();
        let dry = w.idx(3, 5).unwrap();
        w.insert(dry, Item::Lignite, 5, true);
        run(&mut w, 1);
        assert_eq!(w.entity(3, 5).unwrap().status, Status::NoWater);
        assert!(w.entity(3, 5).unwrap().status.is_problem());
    }

    #[test]
    fn turbines_get_more_out_of_fuel_than_generators() {
        // The same steady 75 kW load, once on a generator and once on a turbine.
        let burn = |kind: EntityKind| {
            let mut w = flat(10, 10);
            w.place(5, 5, EntityKind::Pylon, Dir::North).unwrap();
            w.place(4, 5, kind, Dir::North).unwrap();
            w.place(6, 5, EntityKind::Fabricator, Dir::North).unwrap();
            set_recipe(&mut w, 6, 5, "cog");
            let g = w.idx(4, 5).unwrap();
            let f = w.idx(6, 5).unwrap();
            w.insert(g, Item::Lignite, 1, true);
            w.insert(g, Item::Water, 200, true);
            w.insert(f, Item::IronIngot, 100, true);
            let mut ticks = 0;
            while w.entity(4, 5).unwrap().status != Status::NoFuel && ticks < 20_000 {
                w.insert(f, Item::IronIngot, 2, true);
                w.take_one(f, Item::Cog);
                w.tick();
                ticks += 1;
            }
            assert!(ticks < 20_000);
            ticks as f32
        };
        let generator = burn(EntityKind::Generator);
        let turbine = burn(EntityKind::SteamTurbine);
        let ratio = turbine / generator;
        assert!((ratio - TURBINE_EFFICIENCY).abs() < 0.05, "{ratio}");
    }

    #[test]
    fn sifters_sift_quartz_from_sand() {
        let mut w = flat(10, 10);
        assert!(w.place(3, 3, EntityKind::Sifter, Dir::East).is_err());
        for y in 2..=4 {
            for x in 2..=4 {
                set_terrain(&mut w, x, y, Terrain::Sand);
            }
        }
        w.place(3, 3, EntityKind::Sifter, Dir::East).unwrap();
        w.place(4, 3, EntityKind::Crate, Dir::North).unwrap();
        power(&mut w, 5, 5);
        run(&mut w, 41);
        assert_eq!(crate_count(&w, 4, 3, Item::Quartz), 10);
    }

    #[test]
    fn landfill_turns_water_into_sand() {
        let mut w = flat(6, 6);
        set_terrain(&mut w, 2, 2, Terrain::Water);
        assert!(w.can_place(2, 2, EntityKind::Crate, Dir::North).is_err());
        assert!(w.fill(3, 3).is_err());
        w.fill(2, 2).unwrap();
        assert_eq!(w.tile(2, 2).unwrap().terrain, Terrain::Sand);
        assert!(w.can_place(2, 2, EntityKind::Crate, Dir::North).is_ok());
        assert!(w.unfill(2, 2));
        assert_eq!(w.tile(2, 2).unwrap().terrain, Terrain::Water);
        // Terrain changes are saved.
        w.fill(2, 2).unwrap();
        let back = World::from_save(w.to_save()).unwrap();
        assert_eq!(back.tile(2, 2).unwrap().terrain, Terrain::Sand);
    }

    #[test]
    fn arc_furnaces_bake_coke_and_kilns_burn_it() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::ArcFurnace, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let a = w.idx(3, 3).unwrap();
        assert_eq!(w.insert(a, Item::Lignite, 10, true), 0);
        research(&mut w, "coking");
        assert_eq!(w.insert(a, Item::Lignite, 10, true), 10);
        run(&mut w, 20);
        assert_eq!(assembler_output(&w, 3, 3, Item::Coke), 5);
        // A kiln takes coke as fuel and smelts with it.
        w.place(7, 7, EntityKind::Kiln, Dir::North).unwrap();
        let k = w.idx(7, 7).unwrap();
        assert_eq!(w.insert(k, Item::Coke, 2, true), 2);
        w.insert(k, Item::FerriteOre, 3, true);
        run(&mut w, 10);
        assert_eq!(assembler_output(&w, 7, 7, Item::IronIngot), 3);
    }

    #[test]
    fn chem_plants_make_resin() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::ChemPlant, Dir::North).unwrap();
        power(&mut w, 3, 5);
        research(&mut w, "chemistry");
        set_recipe(&mut w, 3, 3, "resin");
        let c = w.idx(3, 3).unwrap();
        w.insert(c, Item::Water, 6, true);
        w.insert(c, Item::Lignite, 3, true);
        run(&mut w, 7);
        assert_eq!(assembler_output(&w, 3, 3, Item::Resin), 3);
    }

    #[test]
    fn modules_go_in_by_hand_and_change_machines() {
        let cogs = |modules: &[Item]| {
            let mut w = flat(10, 10);
            w.place(3, 3, EntityKind::Fabricator, Dir::North).unwrap();
            power(&mut w, 3, 5);
            set_recipe(&mut w, 3, 3, "cog");
            let f = w.idx(3, 3).unwrap();
            for &m in modules {
                assert_eq!(w.insert(f, m, 1, false), 0, "arms can't insert modules");
                assert_eq!(w.insert(f, m, 1, true), 1);
            }
            for _ in 0..20 * TICKS_PER_SEC {
                w.insert(f, Item::IronIngot, 2, true);
                w.tick();
            }
            let n = w.idx(3, 5).unwrap();
            (
                assembler_output(&w, 3, 3, Item::Cog),
                w.net_at(n).unwrap().load,
            )
        };
        let (plain, kw) = cogs(&[]);
        let (fast, fast_kw) = cogs(&[Item::SpeedModule, Item::SpeedModule]);
        let (prod, _) = cogs(&[Item::ProductivityModule, Item::ProductivityModule]);
        assert_eq!(plain, 40);
        assert_eq!(fast, 80);
        assert!((fast_kw / kw - 2.0).abs() < 0.01, "{fast_kw} vs {kw}");
        // 30% slower, but one extra batch every five.
        assert!((30..=34).contains(&prod), "{prod}");
        // Only two fit, and they come back when the building is picked up.
        let mut w = flat(6, 6);
        w.place(2, 2, EntityKind::Lab, Dir::North).unwrap();
        let l = w.idx(2, 2).unwrap();
        assert_eq!(w.insert(l, Item::SpeedModule, 5, true), 2);
        let e = w.remove(2, 2).unwrap();
        assert_eq!(e.contents().count(Item::SpeedModule), 2);
        // Kilns and arms have no slots.
        w.place(3, 3, EntityKind::Kiln, Dir::North).unwrap();
        assert_eq!(
            w.insert(w.idx(3, 3).unwrap(), Item::SpeedModule, 1, true),
            0
        );
    }

    #[test]
    fn endless_research_speeds_up_mining_and_arms() {
        let ore_after = |level: u32| {
            let mut w = flat(10, 10);
            set_ore(&mut w, 3, 3, Item::FerriteOre, 1000);
            w.place(3, 3, EntityKind::Bore, Dir::East).unwrap();
            w.place(4, 3, EntityKind::Crate, Dir::North).unwrap();
            power(&mut w, 3, 5);
            let t = crate::recipes::TechId::by_id("mining-productivity").unwrap();
            w.research.levels.insert(t, level);
            run(&mut w, 20);
            crate_count(&w, 4, 3, Item::FerriteOre)
        };
        assert_eq!(ore_after(0), 20);
        assert!((29..=30).contains(&ore_after(5)));
    }

    #[test]
    fn transmissions_continue_after_the_win() {
        let mut w = flat(10, 10);
        w.place(3, 3, EntityKind::Uplink, Dir::North).unwrap();
        power(&mut w, 3, 5);
        let i = w.idx(3, 3).unwrap();
        w.insert(i, Item::RelayModule, 100, true);
        run(&mut w, 5 * UPLINK_GOAL + 2);
        assert!(w.won);
        let won_at = w.won_at.unwrap();
        assert!(won_at <= (5 * UPLINK_GOAL + 2) as u64 * TICKS_PER_SEC as u64);
        assert_eq!(w.transmission_secs(), 5.0);
        w.events.clear();
        while w.transmitted < MILESTONE_BASE {
            w.insert(i, Item::RelayModule, 1, true);
            w.tick();
        }
        assert!(w.events.iter().any(|e| e.starts_with("Milestone: 40")));
        assert_eq!(w.transmission_secs(), 7.5);
        assert_eq!(w.next_milestone(), 80);
        assert_eq!(w.won_at, Some(won_at));
        let back = World::from_save(w.to_save()).unwrap();
        assert_eq!(back.won_at, Some(won_at));
    }

    #[test]
    fn new_buildings_survive_a_save() {
        let mut w = flat(10, 10);
        for y in 0..10 {
            set_terrain(&mut w, 0, y, Terrain::Water);
        }
        w.place(1, 1, EntityKind::Pump, Dir::East).unwrap();
        w.place(2, 1, EntityKind::SteamTurbine, Dir::North).unwrap();
        w.place(2, 2, EntityKind::Pylon, Dir::North).unwrap();
        w.place(3, 2, EntityKind::BatteryBank, Dir::North).unwrap();
        w.place(3, 3, EntityKind::Lab, Dir::North).unwrap();
        let t = w.idx(2, 1).unwrap();
        let l = w.idx(3, 3).unwrap();
        w.insert(t, Item::Lignite, 5, true);
        w.insert(l, Item::ProductivityModule, 1, true);
        run(&mut w, 10);
        let json = serde_json::to_string(&w.to_save()).unwrap();
        let back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        for (a, b) in w.entities.iter().zip(&back.entities) {
            assert_eq!(
                a.as_ref().map(|e| (&e.state, &e.modules)),
                b.as_ref().map(|e| (&e.state, &e.modules))
            );
        }
    }

    fn belt_items(w: &World, x: i32, y: i32) -> Vec<Item> {
        match &w.entity(x, y).unwrap().state {
            State::Belt { items, .. } => items.iter().map(|b| b.item).collect(),
            _ => panic!("not a belt"),
        }
    }

    /// Puts `n` of `item` on the belt at (x, y), one at a time as room appears.
    fn feed(w: &mut World, x: i32, y: i32, item: Item, n: u32) {
        let i = w.idx(x, y).unwrap();
        w.ensure_caches();
        let mut left = n;
        for _ in 0..2000 {
            if left > 0 && w.insert(i, item, 1, false) == 1 {
                left -= 1;
            }
            w.tick();
        }
        assert_eq!(left, 0, "belt never took everything");
    }

    fn line(w: &mut World, xs: std::ops::Range<i32>, y: i32, kind: EntityKind) {
        for x in xs {
            w.place(x, y, kind, Dir::East).unwrap();
        }
    }

    #[test]
    fn splitter_shares_items_between_three_outputs() {
        let mut w = flat(12, 7);
        line(&mut w, 0..5, 3, EntityKind::Conveyor);
        w.place(5, 3, EntityKind::Splitter, Dir::East).unwrap();
        line(&mut w, 6..9, 3, EntityKind::Conveyor);
        for y in [2, 1] {
            w.place(5, y, EntityKind::Conveyor, Dir::North).unwrap();
        }
        for y in [4, 5] {
            w.place(5, y, EntityKind::Conveyor, Dir::South).unwrap();
        }
        feed(&mut w, 0, 3, Item::Cog, 12);
        let ahead: usize = (6..9).map(|x| belt_items(&w, x, 3).len()).sum();
        let left: usize = [2, 1].iter().map(|&y| belt_items(&w, 5, y).len()).sum();
        let right: usize = [4, 5].iter().map(|&y| belt_items(&w, 5, y).len()).sum();
        assert_eq!((ahead, left, right), (4, 4, 4));
        assert!(belt_items(&w, 5, 3).is_empty());
    }

    #[test]
    fn splitter_uses_the_free_output_when_another_is_full() {
        let mut w = flat(16, 5);
        line(&mut w, 0..3, 2, EntityKind::Conveyor);
        w.place(3, 2, EntityKind::Splitter, Dir::East).unwrap();
        line(&mut w, 4..12, 2, EntityKind::Conveyor);
        // A one-tile dead end on the left fills up after five items.
        w.place(3, 1, EntityKind::Conveyor, Dir::North).unwrap();
        feed(&mut w, 0, 2, Item::Coil, 20);
        assert_eq!(belt_items(&w, 3, 1).len(), 5);
        let ahead: usize = (4..12).map(|x| belt_items(&w, x, 2).len()).sum();
        assert_eq!(ahead, 15);
    }

    #[test]
    fn splitter_never_feeds_a_belt_that_points_into_it() {
        let mut w = flat(10, 5);
        line(&mut w, 0..2, 2, EntityKind::Conveyor);
        w.place(2, 2, EntityKind::Splitter, Dir::East).unwrap();
        line(&mut w, 3..6, 2, EntityKind::Conveyor);
        // This belt merges in from the side, it is not an output.
        w.place(2, 1, EntityKind::Conveyor, Dir::South).unwrap();
        feed(&mut w, 0, 2, Item::Cog, 6);
        assert!(belt_items(&w, 2, 1).is_empty());
        let ahead: usize = (3..6).map(|x| belt_items(&w, x, 2).len()).sum();
        assert_eq!(ahead, 6);
        // And what it carries joins the flow.
        feed(&mut w, 2, 1, Item::Coil, 2);
        let coils = (3..6)
            .flat_map(|x| belt_items(&w, x, 2))
            .filter(|&i| i == Item::Coil)
            .count();
        assert_eq!(coils, 2);
    }

    #[test]
    fn tunnel_carries_items_under_buildings() {
        let mut w = flat(14, 5);
        line(&mut w, 0..3, 2, EntityKind::Conveyor);
        w.place(3, 2, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(4, 2, EntityKind::Crate, Dir::North).unwrap();
        // A belt crossing over the top must not pick anything up.
        w.place(5, 2, EntityKind::Conveyor, Dir::South).unwrap();
        w.place(6, 2, EntityKind::Crate, Dir::North).unwrap();
        w.place(7, 2, EntityKind::TunnelOut, Dir::East).unwrap();
        line(&mut w, 8..11, 2, EntityKind::Conveyor);
        w.ensure_caches();
        let (a, b) = (w.idx(3, 2).unwrap(), w.idx(7, 2).unwrap());
        assert_eq!(w.tunnel_pair(a), Some(b));
        assert_eq!(w.tunnel_pair(b), Some(a));
        feed(&mut w, 0, 2, Item::Chip, 6);
        let out: usize = (8..11).map(|x| belt_items(&w, x, 2).len()).sum();
        assert_eq!(out, 6);
        assert!(belt_items(&w, 5, 2).is_empty());
        assert_eq!(w.entity(4, 2).unwrap().contents().total(), 0);
        assert_eq!(w.entity(3, 2).unwrap().status, Status::Idle);
    }

    #[test]
    fn tunnel_holds_a_full_line_underground() {
        // Nothing takes items at the far end, so the whole run backs up, spaced as on a belt.
        let mut w = flat(10, 3);
        w.place(1, 1, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(5, 1, EntityKind::TunnelOut, Dir::East).unwrap();
        let i = w.idx(1, 1).unwrap();
        w.ensure_caches();
        let mut put = 0;
        for _ in 0..1000 {
            put += w.insert(i, Item::Cog, 1, false);
            w.tick();
        }
        let (State::Belt { items: under, .. }, State::Belt { items: exit, .. }) = (
            &w.entity(1, 1).unwrap().state,
            &w.entity(5, 1).unwrap().state,
        ) else {
            unreachable!()
        };
        assert_eq!(under.len() + exit.len(), put as usize);
        // The exit tile is full, and so is the stretch underground beyond the entrance tile.
        assert_eq!(exit.len(), 5);
        assert!(under.iter().filter(|b| b.pos > TILE_LEN).count() >= 12);
        for pair in under.windows(2) {
            assert!(pair[1].pos - pair[0].pos >= SPACING);
        }
        // Arms only reach what is on the surface.
        assert!(w.takeable(i).len() <= 1);
        let before = under.len();
        while w.take_one(i, Item::Cog) {}
        let State::Belt { items: under, .. } = &w.entity(1, 1).unwrap().state else {
            unreachable!()
        };
        assert!(under.iter().all(|b| b.pos > TILE_LEN));
        assert!(under.len() < before && !under.is_empty());
    }

    #[test]
    fn tunnel_ends_out_of_reach_or_misaligned_stay_unlinked() {
        let mut w = flat(20, 8);
        // Too far apart.
        w.place(1, 1, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(1 + TUNNEL_REACH + 1, 1, EntityKind::TunnelOut, Dir::East)
            .unwrap();
        // Facing different ways.
        w.place(1, 3, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(4, 3, EntityKind::TunnelOut, Dir::West).unwrap();
        // Two entrances share the nearer exit: only the nearer entrance gets it.
        w.place(1, 5, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(3, 5, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(5, 5, EntityKind::TunnelOut, Dir::East).unwrap();
        w.tick();
        for (x, y) in [(1, 1), (7, 1), (1, 3), (4, 3), (1, 5)] {
            let i = w.idx(x, y).unwrap();
            assert_eq!(w.tunnel_pair(i), None, "({x}, {y})");
            assert_eq!(w.entity(x, y).unwrap().status, Status::Unlinked);
        }
        assert_eq!(w.tunnel_pair(w.idx(3, 5).unwrap()), w.idx(5, 5));
        // An unlinked entrance keeps what it is given on the surface.
        let i = w.idx(1, 1).unwrap();
        assert_eq!(w.insert(i, Item::Cog, 1, false), 1);
        run(&mut w, 3);
        assert_eq!(belt_items(&w, 1, 1), vec![Item::Cog]);
        // Removing the exit of a working tunnel strands nothing: the entrance still holds it.
        let i = w.idx(3, 5).unwrap();
        assert_eq!(w.insert(i, Item::Chip, 1, false), 1);
        for _ in 0..3 {
            w.tick();
        }
        w.remove(5, 5);
        run(&mut w, 2);
        assert_eq!(belt_items(&w, 3, 5), vec![Item::Chip]);
        assert_eq!(w.tunnel_pair(i), None);
        // It waits underground, out of an arm's reach, until there is an exit again.
        assert!(w.takeable(i).is_empty());
        w.place(5, 5, EntityKind::TunnelOut, Dir::East).unwrap();
        run(&mut w, 2);
        assert_eq!(belt_items(&w, 5, 5), vec![Item::Chip]);
    }

    #[test]
    fn belt_loops_keep_moving() {
        // A closed loop has no "end" to start from; the update order must still move it.
        let mut w = flat(6, 6);
        w.place(1, 1, EntityKind::Conveyor, Dir::East).unwrap();
        w.place(2, 1, EntityKind::Conveyor, Dir::South).unwrap();
        w.place(2, 2, EntityKind::Conveyor, Dir::West).unwrap();
        w.place(1, 2, EntityKind::Conveyor, Dir::North).unwrap();
        let i = w.idx(1, 1).unwrap();
        w.ensure_caches();
        assert_eq!(w.insert(i, Item::Cog, 1, false), 1);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            w.tick();
            for (x, y) in [(1, 1), (2, 1), (2, 2), (1, 2)] {
                if !belt_items(&w, x, y).is_empty() {
                    seen.insert((x, y));
                }
            }
        }
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn long_arm_reaches_over_a_tile() {
        let mut w = flat(12, 8);
        w.place(2, 2, EntityKind::Crate, Dir::North).unwrap();
        // The tiles next to the arm are in the way of an ordinary one.
        w.place(3, 2, EntityKind::Conveyor, Dir::North).unwrap();
        w.place(4, 2, EntityKind::LongArm, Dir::East).unwrap();
        w.place(5, 2, EntityKind::Conveyor, Dir::North).unwrap();
        w.place(6, 2, EntityKind::Crate, Dir::North).unwrap();
        power(&mut w, 4, 4);
        let c = w.idx(2, 2).unwrap();
        w.insert(c, Item::Cog, 5, true);
        run(&mut w, 20);
        assert_eq!(crate_count(&w, 6, 2, Item::Cog), 5);
        assert!(belt_items(&w, 3, 2).is_empty());
        assert!(belt_items(&w, 5, 2).is_empty());
    }

    #[test]
    fn filter_arm_moves_only_its_item() {
        let mut w = flat(12, 8);
        w.place(2, 2, EntityKind::Crate, Dir::North).unwrap();
        w.place(3, 2, EntityKind::FilterArm, Dir::East).unwrap();
        w.place(4, 2, EntityKind::Crate, Dir::North).unwrap();
        power(&mut w, 3, 4);
        let c = w.idx(2, 2).unwrap();
        w.insert(c, Item::Cog, 4, true);
        w.insert(c, Item::Coil, 4, true);
        if let State::Arm { filter, .. } = &mut w.entity_mut(3, 2).unwrap().state {
            *filter = Some(Item::Coil);
        }
        run(&mut w, 15);
        assert_eq!(crate_count(&w, 4, 2, Item::Coil), 4);
        assert_eq!(crate_count(&w, 4, 2, Item::Cog), 0);
        // With no filter it moves whatever is there.
        if let State::Arm { filter, .. } = &mut w.entity_mut(3, 2).unwrap().state {
            *filter = None;
        }
        run(&mut w, 15);
        assert_eq!(crate_count(&w, 4, 2, Item::Cog), 4);
    }

    #[test]
    fn logistics_state_survives_a_save() {
        let mut w = flat(12, 6);
        line(&mut w, 0..2, 2, EntityKind::Conveyor);
        w.place(2, 2, EntityKind::Splitter, Dir::East).unwrap();
        w.place(3, 2, EntityKind::TunnelIn, Dir::East).unwrap();
        w.place(6, 2, EntityKind::TunnelOut, Dir::East).unwrap();
        w.place(2, 1, EntityKind::Conveyor, Dir::North).unwrap();
        w.place(8, 4, EntityKind::FilterArm, Dir::South).unwrap();
        if let State::Arm { filter, .. } = &mut w.entity_mut(8, 4).unwrap().state {
            *filter = Some(Item::Lens);
        }
        feed(&mut w, 0, 2, Item::Cog, 7);
        let json = serde_json::to_string(&w.to_save()).unwrap();
        let mut back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        let states = |w: &World| -> Vec<Option<(EntityKind, Dir, State)>> {
            w.entities
                .iter()
                .map(|e| e.as_ref().map(|e| (e.kind, e.dir, e.state.clone())))
                .collect()
        };
        assert_eq!(states(&back), states(&w));
        // Links are rebuilt, and both worlds carry on identically.
        for _ in 0..200 {
            w.tick();
            back.tick();
        }
        assert_eq!(back.entities, w.entities);
        assert_eq!(back.tunnel_pair(back.idx(3, 2).unwrap()), back.idx(6, 2));
    }

    #[test]
    fn save_round_trip() {
        let mut w = World::generate(60, 40, 3);
        let (cx, cy) = w.spawn();
        w.place(cx, cy, EntityKind::Crate, Dir::North).unwrap();
        let i = w.idx(cx, cy).unwrap();
        w.insert(i, Item::Chip, 7, true);
        w.tick();
        let json = serde_json::to_string(&w.to_save()).unwrap();
        let back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(back.tiles, w.tiles);
        assert_eq!(
            back.entity(cx, cy).unwrap().state,
            w.entity(cx, cy).unwrap().state
        );
        assert_eq!(back.tick, 1);
    }

    // ---- Fog of war ----

    #[test]
    fn fog_hides_the_map_until_buildings_reveal_it() {
        let mut w = World::generate(320, 240, 5);
        let (cx, cy) = w.spawn();
        assert!(w.is_explored(cx, cy));
        assert!(w.is_explored(cx + START_REVEAL - 1, cy));
        assert!(!w.is_explored(cx + START_REVEAL + 6, cy));
        assert!(w.explored_share() < 0.15);
        // Find dry land just inside the edge of what's been seen and build a pylon there.
        let x = (cx..cx + START_REVEAL)
            .rev()
            .find(|&x| w.tile(x, cy).unwrap().terrain != Terrain::Water && w.is_explored(x, cy))
            .unwrap();
        let far = (x + 8, cy);
        let err = w.can_place(far.0, far.1, EntityKind::Crate, Dir::North);
        assert!(err.unwrap_err().contains("Unexplored"));
        w.place(x, cy, EntityKind::Pylon, Dir::North).unwrap();
        assert!(w.is_explored(far.0, far.1));
        // Landfill can't go on water you haven't seen either.
        assert!(w.fill(cx + 100, cy).is_err());
    }

    #[test]
    fn survey_masts_reveal_further_while_powered() {
        let mut w = flat(80, 80);
        w.explored.iter_mut().for_each(|e| *e = false);
        w.reveal(40, 40, 6);
        w.place(40, 40, EntityKind::SurveyMast, Dir::North).unwrap();
        assert!(w.is_explored(40 + 8, 40));
        assert!(!w.is_explored(40 + 12, 40));
        run(&mut w, 10);
        // No power: it stays put.
        assert!(!w.is_explored(40 + 12, 40));
        power(&mut w, 41, 41);
        run(&mut w, 10);
        assert!(w.is_explored(40 + 12, 40));
        assert!(!w.is_explored(40 + 20, 40));
        run(&mut w, 100);
        assert!(w.is_explored(40 + 39, 40));
        match &w.entity(40, 40).unwrap().state {
            State::Survey { radius } => assert_eq!(*radius, SURVEY_MAX),
            s => panic!("{s:?}"),
        }
        // Done: it stops drawing power.
        assert_eq!(w.entity(40, 40).unwrap().status, Status::Idle);
    }

    #[test]
    fn fog_survives_a_save_and_old_saves_are_all_explored() {
        let w = World::generate(120, 90, 8);
        let save = w.to_save();
        assert!(save.explored.as_ref().unwrap().len() < 400);
        let json = serde_json::to_string(&save).unwrap();
        let back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(back.explored, w.explored);
        // A world without fog (like every save before fog existed) writes nothing for it.
        let open = flat(10, 10);
        let mut v = serde_json::to_value(open.to_save()).unwrap();
        assert!(v.get("explored").is_none());
        v.as_object_mut().unwrap().remove("carts");
        let back = World::from_save(serde_json::from_value(v).unwrap()).unwrap();
        assert!(back.explored.iter().all(|&e| e));
        assert_eq!(
            decode_runs(&encode_runs(&[true, true, false]), 3).unwrap(),
            [true, true, false]
        );
        assert!(decode_runs(&[2, 5], 3).is_err());
    }

    // ---- Rail carts ----

    fn rails(w: &mut World, xs: std::ops::RangeInclusive<i32>, y: i32) {
        for x in xs {
            w.place(x, y, EntityKind::Rail, Dir::North).unwrap();
        }
    }

    fn chest(w: &World, x: i32, y: i32) -> Inventory {
        match &w.entity(x, y).unwrap().state {
            State::Chest { inv, .. } => inv.clone(),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn carts_shuttle_cargo_between_depots() {
        let mut w = flat(60, 12);
        // Crate → arm → depot A ... rails ... depot B → arm → crate.
        w.place(1, 5, EntityKind::Crate, Dir::North).unwrap();
        w.place(2, 5, EntityKind::Arm, Dir::East).unwrap();
        w.place(3, 5, EntityKind::Depot, Dir::North).unwrap();
        rails(&mut w, 4..=45, 5);
        w.place(46, 5, EntityKind::Depot, Dir::North).unwrap();
        w.place(47, 5, EntityKind::Arm, Dir::East).unwrap();
        w.place(48, 5, EntityKind::Crate, Dir::North).unwrap();
        power(&mut w, 2, 7);
        power(&mut w, 47, 7);
        assert_eq!(w.toggle_depot(46, 5), Some(true));
        let src = w.idx(1, 5).unwrap();
        w.insert(src, Item::FerriteOre, 300, true);
        w.add_cart(3, 5).unwrap();
        assert!(w.add_cart(3, 6).is_err());
        run(&mut w, 120);
        let got = chest(&w, 48, 5).count(Item::FerriteOre);
        assert!(got > 60, "only {got} ore arrived");
        let c = &w.carts[0];
        assert!(c.cargo.total() <= CART_CAP);
        // Nothing is lost on the way.
        let total = chest(&w, 1, 5).total()
            + chest(&w, 3, 5).total()
            + chest(&w, 46, 5).total()
            + chest(&w, 48, 5).total()
            + c.cargo.total()
            + [2, 47]
                .iter()
                .filter(|&&x| {
                    matches!(
                        w.entity(x, 5).unwrap().state,
                        State::Arm { hand: Some(_), .. }
                    )
                })
                .count() as u32;
        assert_eq!(total, 300);
    }

    #[test]
    fn a_cart_leaves_with_a_part_load_once_the_depot_runs_dry() {
        let mut w = flat(40, 8);
        w.place(2, 3, EntityKind::Depot, Dir::North).unwrap();
        rails(&mut w, 3..=20, 3);
        w.place(21, 3, EntityKind::Depot, Dir::North).unwrap();
        w.toggle_depot(21, 3);
        let a = w.idx(2, 3).unwrap();
        w.insert(a, Item::Glass, 10, true);
        w.add_cart(2, 3).unwrap();
        run(&mut w, 1);
        assert_eq!(w.carts[0].cargo.count(Item::Glass), 10);
        assert_eq!(w.carts[0].status, CartStatus::Waiting);
        // It waits a few seconds in case more turns up, then goes.
        run(&mut w, CART_IDLE_SECS + 1);
        assert_eq!(w.carts[0].status, CartStatus::Moving);
        run(&mut w, 5);
        assert_eq!(chest(&w, 21, 3).count(Item::Glass), 10);
        // Empty, it heads back and waits there for cargo.
        run(&mut w, 5);
        assert_eq!(w.carts[0].at as usize, a);
        assert_eq!(w.carts[0].status, CartStatus::Waiting);
    }

    #[test]
    fn carts_go_round_more_depots_in_turn_and_wait_when_one_is_full() {
        let mut w = flat(40, 12);
        // An L-shaped track with depots at both ends and one on a spur.
        w.place(2, 2, EntityKind::Depot, Dir::North).unwrap();
        rails(&mut w, 3..=10, 2);
        for y in 3..=8 {
            w.place(10, y, EntityKind::Rail, Dir::North).unwrap();
        }
        w.place(10, 9, EntityKind::Depot, Dir::North).unwrap();
        w.place(11, 5, EntityKind::Depot, Dir::North).unwrap();
        let (a, b, c) = (
            w.idx(2, 2).unwrap(),
            w.idx(10, 9).unwrap(),
            w.idx(11, 5).unwrap(),
        );
        // From A the next depot in map order is the spur (row 5), then the far end (row 9).
        assert_eq!(w.route(a).unwrap()[0] as usize, c);
        assert_eq!(w.route(c).unwrap()[0] as usize, b);
        assert_eq!(w.route(b).unwrap()[0] as usize, a);
        // The path runs along the track, one tile at a time.
        let p = w.route(a).unwrap();
        assert_eq!(p.len(), 8 + 3 + 1);
        // A full unloading depot keeps the cart waiting.
        w.toggle_depot(11, 5);
        w.insert(c, Item::Cog, CRATE_CAP, true);
        w.insert(a, Item::Coil, 5, true);
        w.add_cart(2, 2).unwrap();
        run(&mut w, 12);
        assert_eq!(w.carts[0].at as usize, c);
        assert_eq!(w.carts[0].status, CartStatus::DepotFull);
        let i = w.idx(11, 5).unwrap();
        for _ in 0..5 {
            w.take_one(i, Item::Cog);
        }
        run(&mut w, 1);
        assert!(w.carts[0].cargo.is_empty());
        assert_eq!(chest(&w, 11, 5).count(Item::Coil), 5);
    }

    #[test]
    fn carts_find_another_way_or_stop_when_track_is_taken_up() {
        let mut w = flat(40, 12);
        w.place(2, 2, EntityKind::Depot, Dir::North).unwrap();
        rails(&mut w, 3..=29, 2);
        w.place(30, 2, EntityKind::Depot, Dir::North).unwrap();
        w.toggle_depot(30, 2);
        let a = w.idx(2, 2).unwrap();
        w.insert(a, Item::Steel, CART_CAP, true);
        w.add_cart(2, 2).unwrap();
        run(&mut w, 11);
        assert_eq!(w.carts[0].status, CartStatus::Moving);
        // Cut the line ahead of it: it turns back to the depot it can still reach and waits.
        let ahead = w.pos(w.carts[0].at as usize).0 + 4;
        w.remove(ahead, 2);
        run(&mut w, 3);
        assert_eq!(w.carts[0].at as usize, a);
        assert_eq!(w.carts[0].status, CartStatus::NoRoute);
        assert_eq!(w.carts[0].cargo.count(Item::Steel), CART_CAP);
        // A detour round the gap gets it going again.
        for (x, y) in [(ahead - 1, 3), (ahead, 3), (ahead + 1, 3)] {
            w.place(x, y, EntityKind::Rail, Dir::North).unwrap();
        }
        run(&mut w, 6);
        assert_eq!(w.carts[0].at as usize, w.idx(30, 2).unwrap());
        assert_eq!(w.carts[0].status, CartStatus::Unloading);
    }

    #[test]
    fn carts_and_depot_settings_survive_a_save() {
        let mut w = flat(30, 6);
        w.place(1, 1, EntityKind::Depot, Dir::North).unwrap();
        rails(&mut w, 2..=20, 1);
        w.place(21, 1, EntityKind::Depot, Dir::North).unwrap();
        w.toggle_depot(21, 1);
        let a = w.idx(1, 1).unwrap();
        w.insert(a, Item::Lens, CART_CAP, true);
        w.add_cart(1, 1).unwrap();
        run(&mut w, 11);
        assert_eq!(w.carts[0].status, CartStatus::Moving);
        let json = serde_json::to_string(&w.to_save()).unwrap();
        let mut back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        let where_ = |w: &World| -> Vec<(u32, Vec<u32>, f32, Inventory)> {
            w.carts
                .iter()
                .map(|c| (c.at, c.path.clone(), c.moved, c.cargo.clone()))
                .collect()
        };
        assert_eq!(where_(&back), where_(&w));
        assert_eq!(back.depot_unloads(w.idx(21, 1).unwrap()), Some(true));
        for _ in 0..100 {
            w.tick();
            back.tick();
        }
        assert_eq!(back.carts, w.carts);
        assert_eq!(
            chest(&back, 21, 1).count(Item::Lens),
            chest(&w, 21, 1).count(Item::Lens)
        );
        // Bad cart positions are refused.
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v["carts"][0]["at"] = 100_000.into();
        assert!(World::from_save(serde_json::from_value(v).unwrap()).is_err());
    }

    // ---- Scenarios ----

    fn ore_near(w: &World, ore: Item, r: i32) -> u32 {
        let (cx, cy) = w.spawn();
        let mut n = 0;
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if let Some(t) = w.tile(x, y)
                    && t.ore == Some(ore)
                    && (x - cx).pow(2) + (y - cy).pow(2) <= r * r
                {
                    n += t.amount;
                }
            }
        }
        n
    }

    #[test]
    fn scenarios_shape_the_map() {
        let std = World::generate(320, 240, 99);
        let far = World::generate_scenario(320, 240, 99, Scenario::by_id("far-fields").unwrap());
        let lean = World::generate_scenario(320, 240, 99, Scenario::by_id("lean").unwrap());
        for ore in [Item::MalachiteOre, Item::Quartz] {
            assert!(ore_near(&std, ore, 20) > 0);
            assert_eq!(ore_near(&far, ore, 45), 0, "{ore:?} near spawn");
            assert!(ore_near(&far, ore, 70) > 0, "no {ore:?} out there");
        }
        assert!(ore_near(&far, Item::FerriteOre, 20) > 0);
        for ore in ORES {
            let (s, l) = (ore_near(&std, ore, 20), ore_near(&lean, ore, 20));
            assert!(l * 2 < s, "{ore:?}: lean {l} vs {s}");
        }
    }

    #[test]
    fn a_production_goal_wins_its_challenge() {
        let mut w = flat(10, 10);
        w.scenario = Scenario::by_id("sprint").unwrap();
        let Goal::Produce(item, n) = w.scenario.goal else {
            panic!()
        };
        w.produced.insert(item, n - 1);
        w.tick();
        assert!(!w.won);
        w.produced.insert(item, n);
        w.tick();
        assert!(w.won && w.won_at == Some(1));
        assert!(w.events.iter().any(|e| e.contains("Challenge complete")));
        // The scenario is saved with the world.
        let json = serde_json::to_string(&w.to_save()).unwrap();
        assert!(json.contains("\"scenario\":\"sprint\""));
        let back = World::from_save(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(back.scenario.id, "sprint");
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v["scenario"] = "nope".into();
        assert!(World::from_save(serde_json::from_value(v).unwrap()).is_err());
    }
}
