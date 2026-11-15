//! The map and the factory simulation.

use crate::inventory::Inventory;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId};
use crate::research::Research;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
const NO_NET: u16 = u16::MAX;

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
    Done,
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
            Status::Done => "complete",
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum State {
    Belt {
        items: Vec<BeltItem>,
    },
    Arm {
        hand: Option<Item>,
        phase: ArmPhase,
        t: f32,
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
    Chest {
        inv: Inventory,
    },
    Generator {
        fuel: Fuel,
    },
    Pylon,
    Solar,
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
    #[serde(skip)]
    pub status: Status,
}

impl Entity {
    pub fn new(kind: EntityKind, dir: Dir) -> Self {
        let state = match kind {
            EntityKind::Conveyor | EntityKind::ExpressConveyor => State::Belt { items: vec![] },
            EntityKind::Arm | EntityKind::SwiftArm => State::Arm {
                hand: None,
                phase: ArmPhase::Pickup,
                t: 0.0,
            },
            EntityKind::Digger | EntityKind::Bore => State::Miner {
                fuel: Fuel::default(),
                progress: 0.0,
                output: None,
            },
            EntityKind::Kiln | EntityKind::ArcFurnace => State::Furnace {
                fuel: Fuel::default(),
                input: Inventory::new(),
                output: Inventory::new(),
                job: None,
                progress: 0.0,
            },
            EntityKind::Fabricator => State::Assembler {
                recipe: None,
                input: Inventory::new(),
                output: Inventory::new(),
                working: false,
                progress: 0.0,
            },
            EntityKind::Crate => State::Chest {
                inv: Inventory::new(),
            },
            EntityKind::Generator => State::Generator {
                fuel: Fuel::default(),
            },
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
            status: Status::Idle,
        }
    }

    /// Everything the entity holds, including in-progress inputs, for refunds on removal.
    pub fn contents(&self) -> Inventory {
        let mut inv = Inventory::new();
        match &self.state {
            State::Belt { items } => {
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
            State::Chest { inv: c } => merge(&mut inv, c),
            State::Generator { fuel } => add_fuel(&mut inv, fuel),
            State::Pylon | State::Solar => {}
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
    pub produced: BTreeMap<Item, u64>,
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
            produced: BTreeMap::new(),
            history: std::collections::VecDeque::new(),
            events: vec![],
            dirty: true,
            belt_order: vec![],
            machines: vec![],
            power_sources: vec![],
            power_net: vec![NO_NET; n],
            nets: vec![],
        }
    }

    pub fn generate(width: i32, height: i32, seed: u64) -> Self {
        let mut w = World::empty(width, height, seed);
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
        // Guaranteed starter patches around spawn.
        let start_angle = rng.f32() * std::f32::consts::TAU;
        for (k, ore) in ORES.iter().enumerate() {
            let a = start_angle + k as f32 * std::f32::consts::FRAC_PI_2;
            let dist = 9.0 + rng.f32() * 4.0;
            let px = cx + (a.cos() * dist) as i32;
            let py = cy + (a.sin() * dist) as i32;
            let r = 4.0 + rng.f32() * 1.5;
            w.place_patch(&mut rng, *ore, px, py, r, 700);
        }
        // Scattered richer patches further out.
        let count = (width * height / 900).max(8);
        for k in 0..count {
            let ore = ORES[(k as usize) % ORES.len()];
            for _ in 0..20 {
                let px = rng.range(4, width - 5);
                let py = rng.range(4, height - 5);
                let d2 = (px - cx).pow(2) + (py - cy).pow(2);
                if d2 > 30 * 30 {
                    let rich = 900 + (d2 as f32).sqrt() as u32 * 15;
                    let r = 4.5 + rng.f32() * 4.0;
                    w.place_patch(&mut rng, ore, px, py, r, rich);
                    break;
                }
            }
        }
        w
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

    pub fn area_has_ore(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| {
            self.mining_area(i)
                .iter()
                .any(|&j| self.tiles[j].ore.is_some())
        })
    }

    pub fn can_place(&self, x: i32, y: i32, kind: EntityKind) -> Result<(), String> {
        let Some(i) = self.idx(x, y) else {
            return Err("Out of bounds".into());
        };
        if self.entities[i].is_some() {
            return Err("Tile is occupied".into());
        }
        if self.tiles[i].terrain == Terrain::Water {
            return Err("Can't build on water".into());
        }
        if kind.is_miner() && !self.area_has_ore(x, y) {
            return Err(format!("{} must be placed on ore", kind.name()));
        }
        Ok(())
    }

    pub fn place(&mut self, x: i32, y: i32, kind: EntityKind, dir: Dir) -> Result<(), String> {
        self.can_place(x, y, kind)?;
        let i = self.idx(x, y).unwrap();
        self.entities[i] = Some(Entity::new(kind, dir));
        self.dirty = true;
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

    fn belt_next(&self, i: usize) -> Option<usize> {
        let e = self.entities[i].as_ref()?;
        let j = self.neighbor(i, e.dir)?;
        let n = self.entities[j].as_ref()?;
        (n.kind.is_belt() && n.dir != e.dir.opposite()).then_some(j)
    }

    fn rebuild(&mut self) {
        self.dirty = false;
        let mut belts = vec![];
        self.machines.clear();
        self.power_sources.clear();
        let mut pylons = vec![];
        for (i, e) in self.entities.iter().enumerate() {
            let Some(e) = e else { continue };
            if e.kind.is_belt() {
                belts.push(i);
            } else if e.kind == EntityKind::Pylon {
                pylons.push(i);
            } else if e.kind.power_output() > 0.0 {
                self.power_sources.push(i);
            } else {
                self.machines.push(i);
            }
        }

        // Order belts so the ones nearest the end of a line update first.
        let mut depth: Vec<i32> = vec![-1; self.entities.len()];
        for &b in &belts {
            if depth[b] >= 0 {
                continue;
            }
            let mut path = vec![];
            let mut cur = Some(b);
            let mut base = 0;
            while let Some(c) = cur {
                if depth[c] >= 0 {
                    base = depth[c] + 1;
                    break;
                }
                if depth[c] == -2 {
                    // Loop: break it here.
                    break;
                }
                depth[c] = -2;
                path.push(c);
                cur = self.belt_next(c);
            }
            for (k, &p) in path.iter().rev().enumerate() {
                depth[p] = base + k as i32;
            }
        }
        belts.sort_by_key(|&b| (depth[b], b));
        self.belt_order = belts;

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
        for n in &mut self.nets {
            n.demand_prev = n.demand_cur;
            n.demand_cur = 0.0;
        }
        self.tick += 1;
        if self.tick.is_multiple_of(HISTORY_INTERVAL) {
            self.history.push_back((self.tick, self.produced.clone()));
            while self.history.len() > HISTORY_LEN {
                self.history.pop_front();
            }
        }
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

    fn power_prepass(&mut self) {
        let mut solar = vec![0.0f32; self.nets.len()];
        let mut gens: Vec<Vec<usize>> = vec![vec![]; self.nets.len()];
        for n in &mut self.nets {
            n.capacity = 0.0;
        }
        for &i in &self.power_sources {
            let net = self.power_net[i];
            let e = self.entities[i].as_ref().unwrap();
            let fueled = match &e.state {
                State::Generator { fuel } => fuel.has(),
                _ => true,
            };
            if net == NO_NET {
                continue;
            }
            let n = net as usize;
            if fueled {
                self.nets[n].capacity += e.kind.power_output();
                if e.kind == EntityKind::SunPanel {
                    solar[n] += e.kind.power_output();
                } else {
                    gens[n].push(i);
                }
            }
        }
        for n in self.nets.iter_mut() {
            n.satisfaction = if n.capacity <= 0.0 {
                0.0
            } else if n.demand_prev <= 0.0 {
                1.0
            } else {
                (n.capacity / n.demand_prev).min(1.0)
            };
            n.load = n.demand_prev.min(n.capacity);
        }
        for &i in &self.power_sources {
            let net = self.power_net[i];
            let e = self.entities[i].as_mut().unwrap();
            if net == NO_NET {
                e.status = Status::Idle;
                continue;
            }
            let n = net as usize;
            let net_info = self.nets[n];
            match &mut e.state {
                State::Generator { fuel } => {
                    if !fuel.has() {
                        e.status = Status::NoFuel;
                        continue;
                    }
                    let gen_load = (net_info.load - solar[n]).max(0.0);
                    let share = gen_load / gens[n].len().max(1) as f32;
                    if share > 0.0 {
                        fuel.draw(share / TICKS_PER_SEC as f32);
                        e.status = Status::Working;
                    } else {
                        e.status = Status::Idle;
                    }
                }
                _ => {
                    e.status = if net_info.load > 0.0 {
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
        let speed = e.kind.belt_speed();
        let dir = e.dir;
        let next = self.belt_next_with(i, dir);
        if let State::Belt { items } = &mut e.state {
            let n = items.len();
            for k in (0..n).rev() {
                let limit = if k + 1 < n {
                    items[k + 1].pos.saturating_sub(SPACING)
                } else {
                    TILE_LEN
                };
                let p = items[k].pos;
                items[k].pos = (p + speed).min(limit).max(p);
            }
            if let Some(front) = items.last()
                && front.pos >= TILE_LEN
                && let Some(j) = next
                && let Some(Entity {
                    state: State::Belt { items: nitems },
                    dir: ndir,
                    ..
                }) = self.entities[j].as_mut()
            {
                let pos = if *ndir == dir { 0 } else { TILE_LEN / 2 };
                if Self::belt_insert(nitems, front.item, pos) {
                    items.pop();
                }
            }
            e.status = if items.is_empty() {
                Status::Idle
            } else {
                Status::Working
            };
        }
        self.entities[i] = Some(e);
    }

    fn belt_next_with(&self, i: usize, dir: Dir) -> Option<usize> {
        let j = self.neighbor(i, dir)?;
        let n = self.entities[j].as_ref()?;
        (n.kind.is_belt() && n.dir != dir.opposite()).then_some(j)
    }

    /// How many of `item` the entity at `i` will accept. `manual` is a player insert (bigger buffers).
    pub fn accept_capacity(&self, i: usize, item: Item, manual: bool) -> u32 {
        let Some(e) = self.entities[i].as_ref() else {
            return 0;
        };
        let fuel_cap: u32 = if manual { 50 } else { 5 };
        let fuel_room = |f: &Fuel| {
            if item.is_fuel() && (f.item.is_none() || f.item == Some(item) || f.stock == 0) {
                fuel_cap.saturating_sub(f.stock)
            } else {
                0
            }
        };
        match &e.state {
            State::Belt { items } => {
                if !manual && Self::belt_room(items, TILE_LEN / 2) {
                    1
                } else {
                    0
                }
            }
            State::Arm { .. } | State::Pylon | State::Solar => 0,
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
            State::Chest { inv } => CRATE_CAP.saturating_sub(inv.total()),
            State::Generator { fuel } => fuel_room(fuel),
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
        match &mut e.state {
            State::Belt { items } => {
                if Self::belt_insert(items, item, TILE_LEN / 2) {
                    1
                } else {
                    0
                }
            }
            State::Miner { fuel, .. } | State::Generator { fuel } => {
                fuel.add(item, cap);
                cap
            }
            State::Furnace { fuel, input, .. } => {
                if item.is_fuel() {
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
            State::Chest { inv } | State::Lab { inv, .. } | State::Uplink { inv, .. } => {
                inv.add(item, cap);
                cap
            }
            State::Arm { .. } | State::Pylon | State::Solar => 0,
        }
    }

    /// Items an arm could pick up from the entity at `i`, front-most / first first.
    pub fn takeable(&self, i: usize) -> Vec<Item> {
        let Some(e) = self.entities[i].as_ref() else {
            return vec![];
        };
        match &e.state {
            State::Belt { items } => {
                let mut v: Vec<Item> = vec![];
                for b in items.iter().rev() {
                    if !v.contains(&b.item) {
                        v.push(b.item);
                    }
                }
                v
            }
            State::Furnace { output, .. } | State::Assembler { output, .. } => output.items(),
            State::Chest { inv } => inv.items(),
            _ => vec![],
        }
    }

    /// Removes one `item` from the entity at `i`.
    pub fn take_one(&mut self, i: usize, item: Item) -> bool {
        let Some(e) = self.entities[i].as_mut() else {
            return false;
        };
        match &mut e.state {
            State::Belt { items } => {
                if let Some(k) = items.iter().rposition(|b| b.item == item) {
                    items.remove(k);
                    true
                } else {
                    false
                }
            }
            State::Furnace { output, .. }
            | State::Assembler { output, .. }
            | State::Chest { inv: output } => output.remove(item, 1),
            _ => false,
        }
    }

    fn update_machine(&mut self, i: usize) {
        let Some(mut e) = self.entities[i].take() else {
            return;
        };
        match e.kind {
            EntityKind::Arm | EntityKind::SwiftArm => self.update_arm(i, &mut e),
            EntityKind::Digger | EntityKind::Bore => self.update_miner(i, &mut e),
            EntityKind::Kiln | EntityKind::ArcFurnace => self.update_furnace(i, &mut e),
            EntityKind::Fabricator => self.update_assembler(i, &mut e),
            EntityKind::Lab => self.update_lab(i, &mut e),
            EntityKind::Uplink => self.update_uplink(i, &mut e),
            EntityKind::Crate => {
                e.status = Status::Idle;
            }
            _ => {}
        }
        self.entities[i] = Some(e);
    }

    /// Power-limited rate for an electric entity; registers its demand.
    fn electric(&mut self, i: usize, kind: EntityKind, status: &mut Status) -> f32 {
        let sat = self.sat_at(i);
        self.add_demand(i, kind.power_draw());
        if sat <= 0.0 {
            *status = Status::NoPower;
        } else if sat < 0.999 {
            *status = Status::LowPower;
        } else {
            *status = Status::Working;
        }
        sat
    }

    fn update_arm(&mut self, i: usize, e: &mut Entity) {
        let back = self.neighbor(i, e.dir.opposite());
        let front = self.neighbor(i, e.dir);
        let kind = e.kind;
        let mut status = Status::Idle;
        let State::Arm { hand, phase, t } = &mut e.state else {
            return;
        };
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
                *t += kind.arm_speed() * sat;
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
                *t += kind.arm_speed() * sat;
                if *t >= 1.0 {
                    *t = 0.0;
                    *phase = ArmPhase::Pickup;
                }
            }
        }
        e.status = status;
    }

    fn update_miner(&mut self, i: usize, e: &mut Entity) {
        let front = self.neighbor(i, e.dir);
        let kind = e.kind;
        let area = self.mining_area(i);
        let ore_tile = area.into_iter().find(|&j| self.tiles[j].ore.is_some());
        let mut status = Status::Working;
        let mut electric_rate = None;
        if kind == EntityKind::Bore {
            electric_rate = Some(0.0);
        }
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
        let Some(ot) = ore_tile else {
            e.status = Status::Depleted;
            return;
        };
        let rate = if electric_rate.is_some() {
            let sat = self.electric(i, kind, &mut status);
            sat * kind.speed()
        } else if fuel.draw(kind.fuel_draw() / TICKS_PER_SEC as f32) {
            kind.speed()
        } else {
            e.status = Status::NoFuel;
            return;
        };
        *progress += rate / (MINE_TIME * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *progress -= 1.0;
            let tile = &mut self.tiles[ot];
            let ore = tile.ore.unwrap();
            tile.amount = tile.amount.saturating_sub(1);
            if tile.amount == 0 {
                tile.ore = None;
            }
            *output = Some(ore);
            *self.produced.entry(ore).or_insert(0) += 1;
            if front.is_some_and(|f| self.insert(f, ore, 1, false) == 1) {
                *output = None;
            }
        }
        e.status = status;
    }

    fn update_furnace(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
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
            self.electric(i, kind, &mut status) * kind.speed()
        } else if fuel.draw(kind.fuel_draw() / TICKS_PER_SEC as f32) {
            kind.speed()
        } else {
            e.status = Status::NoFuel;
            return;
        };
        *progress += rate / (rec.time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            output.add_all(rec.outputs, 1);
            for &(o, n) in rec.outputs {
                *self.produced.entry(o).or_insert(0) += n as u64;
            }
            *job = None;
            *progress = 0.0;
        }
        e.status = status;
    }

    fn update_assembler(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
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
        let rate = self.electric(i, kind, &mut status) * kind.speed();
        *progress += rate / (rec.time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            output.add_all(rec.outputs, 1);
            for &(o, n) in rec.outputs {
                *self.produced.entry(o).or_insert(0) += n as u64;
            }
            *working = false;
            *progress = 0.0;
        }
        e.status = status;
    }

    fn update_lab(&mut self, i: usize, e: &mut Entity) {
        let kind = e.kind;
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
        let rate = self.electric(i, kind, &mut status) * kind.speed();
        *progress += rate / (t.unit_time * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *working = false;
            *progress = 0.0;
            packs.clear();
            if let Some(done) = self.research.add_unit() {
                self.events
                    .push(format!("Research complete: {}", done.get().name));
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
        if self.transmitted >= UPLINK_GOAL && !*working && inv.count(Item::RelayModule) == 0 {
            e.status = Status::Done;
            return;
        }
        if !*working {
            if !inv.remove(Item::RelayModule, 1) {
                e.status = Status::NoInput;
                return;
            }
            *working = true;
            *progress = 0.0;
        }
        let rate = self.electric(i, kind, &mut status);
        *progress += rate / (5.0 * TICKS_PER_SEC as f32);
        if *progress >= 1.0 {
            *working = false;
            *progress = 0.0;
            self.transmitted += 1;
            if self.transmitted >= UPLINK_GOAL && !self.won {
                self.won = true;
                self.events
                    .push("The Uplink is transmitting. You win!".into());
            }
        }
        e.status = status;
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
            produced: self.produced.clone(),
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
        w.produced = s.produced;
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
    pub produced: BTreeMap<Item, u64>,
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
            State::Chest { inv } => inv.count(item),
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
            State::Belt { items } => {
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
                State::Belt { items } => {
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
            State::Belt { items } => assert_eq!(items[0].item, Item::Glass),
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
}
