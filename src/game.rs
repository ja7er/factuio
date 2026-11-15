//! Player state and game-level actions (building, crafting, save/load).

use crate::inventory::Inventory;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId, TechId};
use crate::scenario::{Goal, STEPS, Scenario, Tutorial};
use crate::world::{Entity, State, TICKS_PER_SEC, World, WorldSave};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

/// Bump this whenever the save format changes, and add a step to `migrate_save`.
/// New fields should also carry `#[serde(default)]` so older saves still parse.
pub const SAVE_VERSION: u32 = 2;
/// Seconds an important message (research done, victory) stays highlighted.
pub const NOTICE_SECS: u64 = 8;
pub const HAND_MINE_TIME: f32 = 0.6;
pub const MAP_W: i32 = 320;
pub const MAP_H: i32 = 240;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CraftJob {
    pub recipe: RecipeId,
    pub progress: f32,
    pub started: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub inventory: Inventory,
    pub queue: VecDeque<CraftJob>,
    pub cursor: (i32, i32),
    pub hotbar: [Option<Item>; 10],
    pub mining: Option<(i32, i32)>,
    pub mine_progress: f32,
}

impl Player {
    pub fn new(cursor: (i32, i32)) -> Self {
        let mut inventory = Inventory::new();
        for (item, n) in [
            (Item::Digger, 3),
            (Item::Kiln, 4),
            (Item::Generator, 1),
            (Item::Pylon, 8),
            (Item::Conveyor, 40),
            (Item::Arm, 8),
            (Item::Crate, 4),
            (Item::Lab, 1),
            (Item::Lignite, 30),
            (Item::IronIngot, 20),
            (Item::CopperIngot, 10),
            (Item::Quartz, 10),
        ] {
            inventory.add(item, n);
        }
        Player {
            inventory,
            queue: VecDeque::new(),
            cursor,
            hotbar: [
                Some(Item::Conveyor),
                Some(Item::Arm),
                Some(Item::Digger),
                Some(Item::Kiln),
                Some(Item::Crate),
                Some(Item::Pylon),
                Some(Item::Generator),
                Some(Item::Lab),
                Some(Item::Fabricator),
                Some(Item::Bore),
            ],
            mining: None,
            mine_progress: 0.0,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    world: WorldSave,
    player: Player,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tutorial: Option<Tutorial>,
}

/// Brings an older save up to `SAVE_VERSION`, one step at a time.
fn migrate_save(mut v: serde_json::Value) -> Result<serde_json::Value, String> {
    let version = v
        .get("version")
        .and_then(|x| x.as_u64())
        .ok_or("Bad save file: no version")? as u32;
    if version > SAVE_VERSION {
        return Err(format!(
            "This save was made by a newer factuio (save version {version}). Please update."
        ));
    }
    if version == 0 {
        return Err("Unsupported save version 0".into());
    }
    if version < 2 {
        // v2 moved Uplink transmission progress from each Uplink onto the world.
        let mut transmitted = 0;
        if let Some(ents) = v
            .pointer_mut("/world/entities")
            .and_then(|e| e.as_array_mut())
        {
            for pair in ents {
                if let Some(up) = pair
                    .pointer_mut("/1/state/Uplink")
                    .and_then(|u| u.as_object_mut())
                    && let Some(c) = up.remove("charge")
                {
                    transmitted += c.as_u64().unwrap_or(0);
                }
            }
        }
        if let Some(w) = v.get_mut("world").and_then(|w| w.as_object_mut()) {
            w.insert("transmitted".into(), transmitted.into());
        }
    }
    v["version"] = SAVE_VERSION.into();
    Ok(v)
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub tick: u64,
    pub text: String,
    /// Research results, victory: shown highlighted for a while.
    pub important: bool,
}

/// Game time as h:mm:ss.
pub fn clock(ticks: u64) -> String {
    let secs = ticks / TICKS_PER_SEC as u64;
    format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

pub struct Game {
    pub world: World,
    pub player: Player,
    pub messages: Vec<LogLine>,
    pub victory_pending: bool,
    /// Tutorial progress, in the tutorial scenario.
    pub tutorial: Option<Tutorial>,
    /// Ore dug by hand this session (for the tutorial).
    pub hand_mined: u32,
}

impl Game {
    #[cfg(test)]
    pub fn new(seed: u64) -> Self {
        Self::with_scenario(Scenario::standard(), seed)
    }

    /// A new game of `sc`. Scenarios with a fixed map ignore `seed`.
    pub fn with_scenario(sc: &'static Scenario, seed: u64) -> Self {
        let seed = sc.seed.unwrap_or(seed);
        let world = World::generate_scenario(MAP_W, MAP_H, seed, sc);
        let spawn = world.spawn();
        let mut g = Game {
            world,
            player: Player::new(spawn),
            messages: vec![],
            victory_pending: false,
            tutorial: sc.tutorial.then_some(Tutorial { step: 0, since: 0 }),
            hand_mined: 0,
        };
        for &(item, n) in sc.kit {
            g.player.inventory.add(item, n);
        }
        for t in sc.techs {
            if let Some(t) = TechId::by_id(t) {
                g.world.research.complete(t);
            }
        }
        g.log("Welcome to factuio! Press ? for help.");
        if sc.tutorial {
            g.notify("Tutorial: follow the steps in the card at the top right of the map.");
        } else if !sc.is_standard() {
            g.notify(format!("{}: {}.", sc.name, sc.goal_text()));
        }
        g
    }

    #[cfg(test)]
    pub fn from_world(world: World) -> Self {
        let spawn = world.spawn();
        Game {
            world,
            player: Player::new(spawn),
            messages: vec![],
            victory_pending: false,
            tutorial: None,
            hand_mined: 0,
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.push_message(msg.into(), false);
    }

    /// Logs a message that stays highlighted for `NOTICE_SECS`.
    pub fn notify(&mut self, msg: impl Into<String>) {
        self.push_message(msg.into(), true);
    }

    fn push_message(&mut self, text: String, important: bool) {
        self.messages.push(LogLine {
            tick: self.world.tick,
            text,
            important,
        });
        if self.messages.len() > 400 {
            self.messages.drain(0..200);
        }
    }

    /// The message to show in the status line: a recent important one wins over newer chatter.
    pub fn status_message(&self) -> Option<&LogLine> {
        let window = NOTICE_SECS * TICKS_PER_SEC as u64;
        self.messages
            .iter()
            .rev()
            .find(|m| m.important && m.tick + window >= self.world.tick)
            .or_else(|| self.messages.last())
    }

    pub fn tick(&mut self) {
        let was_won = self.world.won;
        self.world.tick();
        self.tick_crafting();
        self.tick_mining();
        for e in std::mem::take(&mut self.world.events) {
            self.notify(e);
        }
        if self.world.won && !was_won {
            self.victory_pending = true;
        }
        self.tick_tutorial();
    }

    fn tick_tutorial(&mut self) {
        let Some(t) = self.tutorial else { return };
        if t.finished() || !(STEPS[t.step].done)(self, &t) {
            return;
        }
        let next = Tutorial {
            step: t.step + 1,
            since: self.world.tick,
        };
        self.tutorial = Some(next);
        match STEPS.get(next.step) {
            Some(s) => self.notify(format!("Tutorial: {}", s.title)),
            None => self.log("Tutorial complete."),
        }
    }

    /// The tutorial step to show, if one is running.
    pub fn tutorial_step(&self) -> Option<(usize, &'static crate::scenario::Step)> {
        let t = self.tutorial?;
        STEPS.get(t.step).map(|s| (t.step, s))
    }

    // ---- Building ----

    /// What `item` becomes when built at (x, y). A tunnel belt turns into an exit when it
    /// completes an entrance waiting behind it, and an entrance otherwise.
    fn kind_for(&mut self, x: i32, y: i32, item: Item, dir: Dir) -> Result<EntityKind, String> {
        let kind = item
            .building()
            .ok_or_else(|| format!("{} can't be placed", item.name()))?;
        if kind == EntityKind::TunnelIn && self.world.tunnel_wants_exit(x, y, dir) {
            return Ok(EntityKind::TunnelOut);
        }
        Ok(kind)
    }

    /// Builds `item` from the inventory at (x, y). Returns what was built.
    pub fn place(&mut self, x: i32, y: i32, item: Item, dir: Dir) -> Result<EntityKind, String> {
        let kind = self.kind_for(x, y, item, dir)?;
        self.place_kind(x, y, kind, dir)?;
        if kind == EntityKind::TunnelIn {
            self.world.tunnel_adopt_ahead(x, y, dir);
        }
        Ok(kind)
    }

    /// Builds exactly `kind` (blueprints and undo, which must not re-decide tunnel ends).
    pub fn place_kind(
        &mut self,
        x: i32,
        y: i32,
        kind: EntityKind,
        dir: Dir,
    ) -> Result<EntityKind, String> {
        let item = kind.item();
        if self.player.inventory.count(item) == 0 {
            return Err(format!("No {} in inventory", item.name()));
        }
        self.world.place(x, y, kind, dir)?;
        self.player.inventory.remove(item, 1);
        Ok(kind)
    }

    /// Swaps the building at (x, y) for `item` from the same family (conveyor → express, kiln →
    /// arc furnace, ...). The old building and its contents go back to the inventory; belt items stay
    /// on the belt. Returns the old entity.
    pub fn replace(&mut self, x: i32, y: i32, item: Item, dir: Dir) -> Result<Entity, String> {
        let kind = self.kind_for(x, y, item, dir)?;
        let old = self.world.entity(x, y).ok_or("Nothing to replace here")?;
        if old.kind.item() == item
            || old.kind.family().is_none()
            || old.kind.family() != kind.family()
        {
            return Err(format!(
                "Can't put a {} over a {}",
                kind.name(),
                old.kind.name()
            ));
        }
        if self.player.inventory.count(item) == 0 {
            return Err(format!("No {} in inventory", item.name()));
        }
        let old = self.world.remove(x, y).expect("checked above");
        let dir = if kind.is_transport() { dir } else { old.dir };
        self.world
            .place(x, y, kind, dir)
            .expect("the tile was just cleared");
        self.player.inventory.remove(item, 1);
        self.player.inventory.add(old.kind.item(), 1);
        match (
            &old.state,
            self.world.entity_mut(x, y).map(|e| &mut e.state),
        ) {
            (State::Belt { items, .. }, Some(State::Belt { items: new, .. })) => {
                *new = items.clone()
            }
            (
                State::Arm { hand, filter, .. },
                Some(State::Arm {
                    hand: new_hand,
                    filter: new_filter,
                    ..
                }),
            ) => {
                *new_hand = *hand;
                if kind == EntityKind::FilterArm {
                    *new_filter = *filter;
                }
            }
            _ => old.contents().drain_into(&mut self.player.inventory),
        }
        Ok(old)
    }

    /// Drops landfill from the inventory on the water tile at (x, y), making it sand.
    pub fn landfill(&mut self, x: i32, y: i32) -> Result<(), String> {
        if self.player.inventory.count(Item::Landfill) == 0 {
            return Err("No Landfill in inventory".into());
        }
        self.world.fill(x, y)?;
        self.player.inventory.remove(Item::Landfill, 1);
        Ok(())
    }

    /// Takes the modules out of the building at (x, y). Returns how many.
    pub fn take_modules(&mut self, x: i32, y: i32) -> usize {
        let Some(e) = self.world.entity_mut(x, y) else {
            return 0;
        };
        let n = e.modules.len();
        for m in e.modules.drain(..) {
            self.player.inventory.add(m, 1);
        }
        e.bonus = 0.0;
        n
    }

    /// Adds up to 10 fuel from the inventory: lignite first, then coke. Returns what went in.
    /// A small step, so the starting lignite stretches over a digger, a kiln and a generator.
    pub fn add_fuel(&mut self, x: i32, y: i32) -> Option<(Item, u32)> {
        for f in [Item::Lignite, Item::Coke] {
            let n = self.insert(x, y, f, 10);
            if n > 0 {
                return Some((f, n));
            }
        }
        None
    }

    /// Sets what a filter arm moves. `None` lets it move anything.
    pub fn set_filter(&mut self, x: i32, y: i32, item: Option<Item>) -> Result<(), String> {
        let e = self.world.entity_mut(x, y).ok_or("No building here")?;
        match &mut e.state {
            State::Arm { filter, .. } if e.kind == EntityKind::FilterArm => {
                *filter = item;
                Ok(())
            }
            _ => Err("Only filter arms take a filter".into()),
        }
    }

    /// Buildings that currently report a problem, in map order.
    pub fn problems(&self) -> Vec<(i32, i32, EntityKind, crate::world::Status)> {
        self.world
            .entities
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let e = e.as_ref()?;
                e.status.is_problem().then(|| {
                    let (x, y) = self.world.pos(i);
                    (x, y, e.kind, e.status)
                })
            })
            .collect()
    }

    /// Picks up the building at (x, y) along with everything inside it, and any carts on it.
    pub fn remove(&mut self, x: i32, y: i32) -> Result<EntityKind, String> {
        let e = self.world.remove(x, y).ok_or("Nothing to remove here")?;
        self.take_carts(x, y, usize::MAX);
        let mut c = e.contents();
        c.add(e.kind.item(), 1);
        c.drain_into(&mut self.player.inventory);
        Ok(e.kind)
    }

    /// Puts a rail cart from the inventory on the rail or depot at (x, y).
    pub fn add_cart(&mut self, x: i32, y: i32) -> Result<(), String> {
        if self.player.inventory.count(Item::Cart) == 0 {
            return Err("No Rail cart in inventory".into());
        }
        self.world.add_cart(x, y)?;
        self.player.inventory.remove(Item::Cart, 1);
        Ok(())
    }

    /// Picks up to `max` carts on (x, y), with their cargo. Returns how many.
    pub fn take_carts(&mut self, x: i32, y: i32, max: usize) -> usize {
        let Some(i) = self.world.idx(x, y) else {
            return 0;
        };
        let carts = self.world.take_carts(i, max);
        for mut c in carts.iter().cloned() {
            c.cargo.drain_into(&mut self.player.inventory);
            self.player.inventory.add(Item::Cart, 1);
        }
        carts.len()
    }

    /// Takes the outputs (or stored items) of the building at (x, y).
    pub fn take_output(&mut self, x: i32, y: i32) -> u32 {
        let Some(e) = self.world.entity_mut(x, y) else {
            return 0;
        };
        let mut got = Inventory::new();
        match &mut e.state {
            State::Belt { items, .. } => {
                for b in items.drain(..) {
                    got.add(b.item, 1);
                }
            }
            State::Arm { hand, .. } => {
                if let Some(h) = hand.take() {
                    got.add(h, 1);
                }
            }
            State::Miner { output, .. } => {
                if let Some(o) = output.take() {
                    got.add(o, 1);
                }
            }
            State::Furnace { output, .. }
            | State::Assembler { output, .. }
            | State::Chest { inv: output, .. } => {
                output.drain_into(&mut got);
            }
            State::Lab { inv, .. } | State::Uplink { inv, .. } => inv.drain_into(&mut got),
            State::Generator { .. }
            | State::Pylon
            | State::Solar
            | State::Battery { .. }
            | State::Rail
            | State::Survey { .. } => {}
        }
        let n = got.total();
        got.drain_into(&mut self.player.inventory);
        n
    }

    /// Moves up to `n` of `item` from the player into the building at (x, y).
    pub fn insert(&mut self, x: i32, y: i32, item: Item, n: u32) -> u32 {
        let Some(i) = self.world.idx(x, y) else {
            return 0;
        };
        let n = n.min(self.player.inventory.count(item));
        let put = self.world.insert(i, item, n, true);
        self.player.inventory.remove(item, put);
        put
    }

    /// Items from the player's inventory that the building at (x, y) accepts.
    pub fn insertable(&self, x: i32, y: i32) -> Vec<(Item, u32)> {
        let Some(i) = self.world.idx(x, y) else {
            return vec![];
        };
        self.player
            .inventory
            .iter()
            .filter(|&(it, _)| self.world.accept_capacity(i, it, true) > 0)
            .collect()
    }

    pub fn set_recipe(&mut self, x: i32, y: i32, r: Option<RecipeId>) -> Result<(), String> {
        let kind = self.world.entity(x, y).ok_or("No building here")?.kind;
        if let Some(r) = r {
            match kind.recipe_category() {
                Some(c) if c == r.get().category => {}
                Some(Category::Chem) => return Err("Chem plants only run chemistry recipes".into()),
                Some(_) => return Err("Fabricators can't smelt or do chemistry".into()),
                None => return Err("Only fabricators and chem plants take a recipe".into()),
            }
            if !self.world.research.is_unlocked(r) {
                return Err("Recipe not researched yet".into());
            }
        }
        let e = self.world.entity_mut(x, y).ok_or("No building here")?;
        let refund = e.contents();
        let State::Assembler {
            recipe,
            input,
            output,
            working,
            progress,
        } = &mut e.state
        else {
            return Err("Only fabricators take a recipe".into());
        };
        if *recipe == r {
            return Ok(());
        }
        *recipe = r;
        *input = Inventory::new();
        *output = Inventory::new();
        *working = false;
        *progress = 0.0;
        let mut refund = refund;
        refund.drain_into(&mut self.player.inventory);
        Ok(())
    }

    pub fn start_research(&mut self, t: TechId) -> Result<(), String> {
        self.world.research.start(t)
    }

    // ---- Hand crafting ----

    /// Inventory as it will be once every queued (not yet started) job runs.
    fn projected_inventory(&self) -> Inventory {
        let mut inv = self.player.inventory.clone();
        for job in &self.player.queue {
            let r = job.recipe.get();
            if job.started || inv.remove_all(r.inputs, 1) {
                inv.add_all(r.outputs, 1);
            }
        }
        inv
    }

    fn plan(
        &self,
        r: RecipeId,
        inv: &mut Inventory,
        jobs: &mut Vec<RecipeId>,
        depth: u32,
    ) -> Result<(), Item> {
        let rec = r.get();
        for &(item, n) in rec.inputs {
            let have = inv.count(item);
            if have >= n {
                inv.remove(item, n);
                continue;
            }
            let missing = n - have;
            let sub = RecipeId::producing(item)
                .filter(|&s| depth < 6 && self.world.research.is_unlocked(s))
                .ok_or(item)?;
            let per = sub.get().outputs[0].1;
            for _ in 0..missing.div_ceil(per) {
                self.plan(sub, inv, jobs, depth + 1)?;
            }
            // Reserve as we go so later sub-crafts can't eat earlier ingredients.
            if !inv.remove(item, n) {
                return Err(item);
            }
        }
        inv.add_all(rec.outputs, 1);
        jobs.push(r);
        Ok(())
    }

    /// Queues `count` hand-crafts of `r`, including any intermediates needed.
    pub fn queue_craft(&mut self, r: RecipeId, count: u32) -> Result<u32, String> {
        let rec = r.get();
        if rec.category != Category::Craft {
            return Err(format!("{} must be smelted in a kiln", rec.name()));
        }
        if !self.world.research.is_unlocked(r) {
            return Err(format!("{} is not researched yet", rec.name()));
        }
        let mut inv = self.projected_inventory();
        let mut done = 0;
        let mut missing = None;
        for _ in 0..count {
            let mut trial = inv.clone();
            let mut jobs = vec![];
            match self.plan(r, &mut trial, &mut jobs, 0) {
                Ok(()) => {
                    inv = trial;
                    for j in jobs {
                        self.player.queue.push_back(CraftJob {
                            recipe: j,
                            progress: 0.0,
                            started: false,
                        });
                    }
                    done += 1;
                }
                Err(item) => {
                    missing = Some(item);
                    break;
                }
            }
        }
        if done == 0 {
            return Err(format!(
                "Missing {} for {}",
                missing.map(|i| i.name()).unwrap_or("items"),
                rec.name()
            ));
        }
        Ok(done)
    }

    /// How many of `r` could be hand-crafted right now (capped at 99).
    pub fn craftable_count(&self, r: RecipeId) -> u32 {
        if r.get().category != Category::Craft || !self.world.research.is_unlocked(r) {
            return 0;
        }
        let mut inv = self.projected_inventory();
        let mut n = 0;
        while n < 99 {
            let mut jobs = vec![];
            let mut trial = inv.clone();
            if self.plan(r, &mut trial, &mut jobs, 0).is_err() {
                break;
            }
            inv = trial;
            n += 1;
        }
        n
    }

    pub fn cancel_last_craft(&mut self) -> bool {
        if let Some(job) = self.player.queue.pop_back() {
            if job.started {
                self.player.inventory.add_all(job.recipe.get().inputs, 1);
            }
            true
        } else {
            false
        }
    }

    fn tick_crafting(&mut self) {
        let Some(job) = self.player.queue.front_mut() else {
            return;
        };
        let rec = job.recipe.get();
        if !job.started {
            if self.player.inventory.remove_all(rec.inputs, 1) {
                job.started = true;
            } else {
                self.player.queue.pop_front();
                self.log(format!(
                    "Crafting {} cancelled: missing ingredients",
                    rec.name()
                ));
                return;
            }
        }
        job.progress += 1.0 / (rec.time * TICKS_PER_SEC as f32);
        if job.progress >= 1.0 - 1e-4 {
            self.player.queue.pop_front();
            self.player.inventory.add_all(rec.outputs, 1);
        }
    }

    // ---- Hand mining ----

    pub fn toggle_mining(&mut self) -> Result<bool, String> {
        if self.player.mining.is_some() {
            self.player.mining = None;
            return Ok(false);
        }
        let (x, y) = self.player.cursor;
        if !self.world.is_explored(x, y) {
            return Err("You can't see what's there yet".into());
        }
        match self.world.tile(x, y) {
            Some(t) if t.ore.is_some() => {
                self.player.mining = Some((x, y));
                self.player.mine_progress = 0.0;
                Ok(true)
            }
            _ => Err("No ore under the cursor".into()),
        }
    }

    fn tick_mining(&mut self) {
        let Some(pos) = self.player.mining else {
            return;
        };
        if pos != self.player.cursor {
            self.player.mining = None;
            return;
        }
        self.player.mine_progress += 1.0 / (HAND_MINE_TIME * TICKS_PER_SEC as f32);
        if self.player.mine_progress >= 1.0 - 1e-4 {
            self.player.mine_progress = 0.0;
            let i = self.world.idx(pos.0, pos.1).unwrap();
            let t = &mut self.world.tiles[i];
            if let Some(ore) = t.ore {
                t.amount = t.amount.saturating_sub(1);
                if t.amount == 0 {
                    t.ore = None;
                    self.player.mining = None;
                }
                self.player.inventory.add(ore, 1);
                self.hand_mined += 1;
            } else {
                self.player.mining = None;
            }
        }
    }

    // ---- Save / load ----

    pub fn to_json(&self) -> Result<String, String> {
        let s = SaveFile {
            version: SAVE_VERSION,
            world: self.world.to_save(),
            player: self.player.clone(),
            tutorial: self.tutorial,
        };
        serde_json::to_string(&s).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Game, String> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("Bad save file: {e}"))?;
        let v = migrate_save(v)?;
        let s: SaveFile = serde_json::from_value(v).map_err(|e| format!("Bad save file: {e}"))?;
        let mut g = Game {
            world: World::from_save(s.world)?,
            player: s.player,
            messages: vec![],
            victory_pending: false,
            tutorial: s.tutorial,
            hand_mined: 0,
        };
        g.log("Game loaded.");
        Ok(g)
    }

    /// Writes the save atomically, keeping the previous file as `<name>.bak`.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = self.to_json()?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        if path.exists() {
            let _ = std::fs::copy(path, path.with_extension("bak"));
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Game, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        Game::from_json(&json)
    }

    /// A short hint about what to do next.
    pub fn objective(&self) -> String {
        let w = &self.world;
        let has = |k: EntityKind| w.entities.iter().flatten().any(|e| e.kind == k);
        let r = &w.research;
        let tech = |id: &str| TechId::by_id(id).is_some_and(|t| r.is_done(t));
        if let Goal::Produce(item, n) = w.scenario.goal {
            let have = w.produced.get(&item).copied().unwrap_or(0);
            if w.won {
                return format!(
                    "Challenge complete in {}! Keep building, or try another from the title screen.",
                    clock(w.won_at.unwrap_or(w.tick))
                );
            }
            let goal = format!("{} ({}/{n}).", w.scenario.goal_text(), have.min(n));
            if tech("logic") && has(EntityKind::Fabricator) {
                return goal;
            }
            return format!("{goal} First: {}", self.uplink_objective());
        }
        self.uplink_objective()
    }

    fn uplink_objective(&self) -> String {
        let w = &self.world;
        let has = |k: EntityKind| w.entities.iter().flatten().any(|e| e.kind == k);
        let r = &w.research;
        let tech = |id: &str| TechId::by_id(id).is_some_and(|t| r.is_done(t));
        if w.won {
            return format!(
                "You won! Keep the Uplink fed for the next milestone ({}) or try endless research.",
                w.next_milestone()
            );
        }
        if !has(EntityKind::Digger) && !has(EntityKind::Bore) {
            return "Place a Digger (3) on ore, then fuel it: e, f.".into();
        }
        if !has(EntityKind::Kiln) && !has(EntityKind::ArcFurnace) {
            return "Put a Kiln (4) right in front of a Digger on ferrite or malachite.".into();
        }
        if !has(EntityKind::Generator) {
            return "Place a Generator (7) beside a Pylon (6) and fuel it to power arms and labs."
                .into();
        }
        if !tech("fabrication") {
            if !has(EntityKind::Lab) {
                return "Place the Lab (8) within a pylon's reach.".into();
            }
            if r.current.is_none() {
                return "Choose Fabrication in the research screen (t).".into();
            }
            return "Craft Alpha datapacks (c) and load them into the Lab (e, i).".into();
        }
        if !has(EntityKind::Fabricator) {
            return "Craft a Fabricator and automate cogs or datapacks.".into();
        }
        if !tech("uplink") {
            if r.current.is_none() {
                return "Pick the next research (t). The goal is Uplink theory.".into();
            }
            if tech("chemistry") && !tech("field-theory") {
                return "Pump water into chem plants for resin, then batteries and Delta datapacks."
                    .into();
            }
            return "Automate datapacks and research toward Uplink theory.".into();
        }
        if !has(EntityKind::Uplink) {
            return "Craft and place the Uplink array, then power it.".into();
        }
        "Feed the Uplink 20 relay modules while it has power.".into()
    }

    pub fn play_time(&self) -> String {
        clock(self.world.tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    fn game() -> Game {
        Game::from_world(World::empty(30, 30, 1))
    }

    fn run(g: &mut Game, secs: u32) {
        for _ in 0..secs * TICKS_PER_SEC {
            g.tick();
        }
    }

    #[test]
    fn hand_craft_expands_intermediates() {
        let mut g = game();
        g.player.inventory = Inventory::new();
        g.player.inventory.add(Item::IronIngot, 3);
        g.player.inventory.add(Item::CopperIngot, 2);
        // Arm = 1 iron + 1 cog (2 iron) + 1 chip (1 iron + 3 coil); 3 coil = 2 crafts of 1 copper.
        let arm = RecipeId::by_id("arm").unwrap();
        assert_eq!(g.craftable_count(arm), 0);
        g.player.inventory.add(Item::IronIngot, 1);
        assert_eq!(g.craftable_count(arm), 1);
        assert_eq!(g.queue_craft(arm, 5).unwrap(), 1);
        run(&mut g, 5);
        assert_eq!(g.player.inventory.count(Item::Arm), 1);
        // One leftover coil from the second coil craft.
        assert_eq!(g.player.inventory.count(Item::Coil), 1);
        assert_eq!(g.player.inventory.count(Item::IronIngot), 0);
    }

    #[test]
    fn locked_recipes_cannot_be_crafted() {
        let mut g = game();
        g.player.inventory.add(Item::Chip, 50);
        g.player.inventory.add(Item::Cog, 50);
        let fab = RecipeId::by_id("fabricator").unwrap();
        assert!(g.queue_craft(fab, 1).is_err());
        g.world
            .research
            .complete(TechId::by_id("fabrication").unwrap());
        assert!(g.queue_craft(fab, 1).is_ok());
    }

    #[test]
    fn cancel_refunds_started_job() {
        let mut g = game();
        g.player.inventory = Inventory::new();
        g.player.inventory.add(Item::IronIngot, 2);
        g.queue_craft(RecipeId::by_id("cog").unwrap(), 1).unwrap();
        g.tick();
        assert_eq!(g.player.inventory.count(Item::IronIngot), 0);
        assert!(g.cancel_last_craft());
        assert_eq!(g.player.inventory.count(Item::IronIngot), 2);
    }

    #[test]
    fn place_and_remove_refunds_contents() {
        let mut g = game();
        g.place(5, 5, Item::Crate, Dir::North).unwrap();
        assert_eq!(g.player.inventory.count(Item::Crate), 3);
        assert_eq!(g.insert(5, 5, Item::Lignite, 10), 10);
        assert_eq!(g.player.inventory.count(Item::Lignite), 20);
        g.remove(5, 5).unwrap();
        assert_eq!(g.player.inventory.count(Item::Crate), 4);
        assert_eq!(g.player.inventory.count(Item::Lignite), 30);
        assert!(g.place(5, 5, Item::Cog, Dir::North).is_err());
    }

    #[test]
    fn hand_mining_collects_ore() {
        let mut g = game();
        let i = g.world.idx(3, 3).unwrap();
        g.world.tiles[i].ore = Some(Item::Quartz);
        g.world.tiles[i].amount = 100;
        g.player.cursor = (3, 3);
        let before = g.player.inventory.count(Item::Quartz);
        assert!(g.toggle_mining().unwrap());
        run(&mut g, 3);
        assert_eq!(g.player.inventory.count(Item::Quartz), before + 5);
        g.player.cursor = (4, 3);
        g.tick();
        assert!(g.player.mining.is_none());
    }

    #[test]
    fn changing_recipe_refunds() {
        let mut g = game();
        g.world
            .research
            .complete(TechId::by_id("fabrication").unwrap());
        g.player.inventory.add(Item::Fabricator, 1);
        g.place(4, 4, Item::Fabricator, Dir::North).unwrap();
        g.set_recipe(4, 4, RecipeId::by_id("cog")).unwrap();
        assert_eq!(g.insert(4, 4, Item::IronIngot, 10), 10);
        g.set_recipe(4, 4, RecipeId::by_id("coil")).unwrap();
        assert_eq!(g.player.inventory.count(Item::IronIngot), 20);
        assert!(g.set_recipe(4, 4, RecipeId::by_id("iron-ingot")).is_err());
    }

    #[test]
    fn json_round_trip() {
        let mut g = Game::new(99);
        let (x, y) = g.player.cursor;
        g.place(x, y, Item::Crate, Dir::North).unwrap();
        g.queue_craft(RecipeId::by_id("cog").unwrap(), 2).unwrap();
        run(&mut g, 1);
        let json = g.to_json().unwrap();
        let back = Game::from_json(&json).unwrap();
        assert_eq!(back.player.inventory, g.player.inventory);
        assert_eq!(back.player.queue, g.player.queue);
        assert_eq!(back.world.tick, g.world.tick);
        assert!(back.world.entity(x, y).is_some());
        assert!(Game::from_json("{}").is_err());
    }

    #[test]
    fn v1_saves_still_load() {
        let g = Game::from_json(include_str!("testdata/save_v1.json")).unwrap();
        // The Uplink's own charge moved onto the world.
        assert_eq!(g.world.transmitted, 2);
        assert!(
            g.world
                .entities
                .iter()
                .flatten()
                .any(|e| e.kind == EntityKind::Uplink)
        );
        // And it saves as the current version.
        let json = g.to_json().unwrap();
        assert!(json.contains(&format!("\"version\":{SAVE_VERSION}")));
        assert_eq!(Game::from_json(&json).unwrap().world.transmitted, 2);
    }

    #[test]
    fn newer_saves_are_refused_clearly() {
        let mut v: serde_json::Value =
            serde_json::from_str(&Game::new(1).to_json().unwrap()).unwrap();
        v["version"] = (SAVE_VERSION + 1).into();
        let err = Game::from_json(&v.to_string()).err().unwrap();
        assert!(err.contains("newer"), "{err}");
    }

    #[test]
    fn picking_up_the_uplink_keeps_transmission_progress() {
        let mut g = game();
        g.player.inventory.add(Item::Uplink, 1);
        g.place(3, 3, Item::Uplink, Dir::North).unwrap();
        g.place(3, 5, Item::Pylon, Dir::North).unwrap();
        g.place(4, 5, Item::Generator, Dir::North).unwrap();
        g.insert(4, 5, Item::Lignite, 50);
        g.player.inventory.add(Item::RelayModule, 10);
        g.insert(3, 3, Item::RelayModule, 10);
        run(&mut g, 31);
        let sent = g.world.transmitted;
        assert!(sent >= 5, "sent {sent}");
        g.remove(3, 3).unwrap();
        // Nothing is lost: transmitted modules stay counted, the rest come back.
        assert_eq!(sent + g.player.inventory.count(Item::RelayModule), 10);
        g.place(3, 3, Item::Uplink, Dir::North).unwrap();
        assert_eq!(g.world.transmitted, sent);
    }

    #[test]
    fn important_messages_stay_visible() {
        let mut g = game();
        g.notify("Research complete: Fabrication");
        g.log("Picked up Crate");
        assert!(g.status_message().unwrap().text.contains("Research"));
        run(&mut g, NOTICE_SECS as u32 + 1);
        g.log("Picked up Crate");
        assert_eq!(g.status_message().unwrap().text, "Picked up Crate");
    }

    #[test]
    fn tunnel_pieces_pick_their_end_when_placed() {
        let mut g = game();
        g.player.inventory.add(Item::Tunnel, 6);
        // Built in the usual order: entrance first, then the exit ahead.
        assert_eq!(
            g.place(2, 2, Item::Tunnel, Dir::East),
            Ok(EntityKind::TunnelIn)
        );
        assert_eq!(
            g.place(5, 2, Item::Tunnel, Dir::East),
            Ok(EntityKind::TunnelOut)
        );
        // That pair is complete, so the next piece starts a new tunnel.
        assert_eq!(
            g.place(7, 2, Item::Tunnel, Dir::East),
            Ok(EntityKind::TunnelIn)
        );
        // Built backwards: the far end goes down first and becomes the exit afterwards.
        g.place(9, 6, Item::Tunnel, Dir::West).unwrap();
        assert_eq!(
            g.place(12, 6, Item::Tunnel, Dir::West),
            Ok(EntityKind::TunnelIn)
        );
        assert_eq!(g.world.entity(9, 6).unwrap().kind, EntityKind::TunnelOut);
        g.world.ensure_caches();
        let i = g.world.idx(12, 6).unwrap();
        assert_eq!(g.world.tunnel_pair(i), g.world.idx(9, 6));
        // Flipping turns an end around by hand.
        assert!(g.world.flip_tunnel(7, 2));
        assert_eq!(g.world.entity(7, 2).unwrap().kind, EntityKind::TunnelOut);
        // Both ends give back the same item.
        g.remove(5, 2).unwrap();
        g.remove(2, 2).unwrap();
        assert_eq!(g.player.inventory.count(Item::Tunnel), 3);
    }

    #[test]
    fn swapping_arms_keeps_what_they_hold() {
        let mut g = game();
        g.player.inventory.add(Item::FilterArm, 1);
        g.player.inventory.add(Item::Arm, 1);
        g.place(4, 4, Item::FilterArm, Dir::East).unwrap();
        g.set_filter(4, 4, Some(Item::Coil)).unwrap();
        assert!(g.set_filter(5, 5, Some(Item::Coil)).is_err());
        if let State::Arm { hand, .. } = &mut g.world.entity_mut(4, 4).unwrap().state {
            *hand = Some(Item::Coil);
        }
        let old = g.replace(4, 4, Item::Arm, Dir::North).unwrap();
        assert_eq!(old.kind, EntityKind::FilterArm);
        let e = g.world.entity(4, 4).unwrap();
        assert_eq!(e.kind, EntityKind::Arm);
        // Arms keep facing the way they did, and the held item isn't duplicated.
        assert_eq!(e.dir, Dir::East);
        assert!(matches!(
            e.state,
            State::Arm {
                hand: Some(Item::Coil),
                filter: None,
                ..
            }
        ));
        assert_eq!(g.player.inventory.count(Item::Coil), 0);
        assert_eq!(g.player.inventory.count(Item::FilterArm), 1);
    }

    #[test]
    fn modules_and_coke_from_the_inventory() {
        let mut g = game();
        g.player.inventory = Inventory::new();
        g.player.inventory.add(Item::Lab, 1);
        g.player.inventory.add(Item::Kiln, 1);
        g.player.inventory.add(Item::SpeedModule, 3);
        g.player.inventory.add(Item::Coke, 10);
        g.place(3, 3, Item::Lab, Dir::North).unwrap();
        assert_eq!(g.insert(3, 3, Item::SpeedModule, 3), 2);
        assert_eq!(g.take_modules(3, 3), 2);
        assert_eq!(g.player.inventory.count(Item::SpeedModule), 3);
        // With no lignite, adding fuel uses coke.
        g.place(5, 5, Item::Kiln, Dir::North).unwrap();
        assert_eq!(g.add_fuel(5, 5), Some((Item::Coke, 10)));
        assert_eq!(g.add_fuel(5, 5), None);
    }

    #[test]
    fn scenarios_start_with_their_kit_techs_and_map() {
        let sc = Scenario::by_id("far-fields").unwrap();
        let a = Game::with_scenario(sc, 1);
        let b = Game::with_scenario(sc, 2);
        // A fixed map: the seed asked for doesn't matter.
        assert_eq!(a.world.seed, 31337);
        assert_eq!(a.world.tiles, b.world.tiles);
        assert_eq!(a.player.inventory.count(Item::Rail), 120);
        assert_eq!(a.player.inventory.count(Item::Cart), 1);
        assert!(a.world.research.is_done(TechId::by_id("railways").unwrap()));
        assert!(a.status_message().unwrap().text.contains("Far fields"));
        // Free play uses the seed it's given, and the save remembers the scenario.
        let f = Game::with_scenario(Scenario::standard(), 9);
        assert_eq!(f.world.seed, 9);
        let back = Game::from_json(&a.to_json().unwrap()).unwrap();
        assert_eq!(back.world.scenario.id, "far-fields");
        assert_eq!(back.world.explored, a.world.explored);
    }

    #[test]
    fn a_sprint_objective_counts_toward_its_goal() {
        let mut g = Game::with_scenario(Scenario::by_id("sprint").unwrap(), 0);
        assert!(g.objective().contains("Make 60 Beta datapacks"));
        assert!(g.objective().contains("First:"));
        g.world.produced.insert(Item::BetaPack, 60);
        g.tick();
        assert!(g.victory_pending && g.world.won);
        assert!(g.objective().contains("Challenge complete"));
    }

    #[test]
    fn hand_mining_needs_explored_ground() {
        let mut g = game();
        let i = g.world.idx(3, 3).unwrap();
        g.world.tiles[i].ore = Some(Item::Quartz);
        g.world.tiles[i].amount = 10;
        g.world.explored[i] = false;
        g.player.cursor = (3, 3);
        assert!(g.toggle_mining().is_err());
        g.world.explored[i] = true;
        assert!(g.toggle_mining().unwrap());
        run(&mut g, 2);
        assert_eq!(g.hand_mined, 3);
    }

    #[test]
    fn picking_up_track_takes_its_carts_and_cargo() {
        let mut g = game();
        g.player.inventory.add(Item::Depot, 1);
        g.player.inventory.add(Item::Cart, 2);
        g.place(4, 4, Item::Depot, Dir::North).unwrap();
        assert!(g.add_cart(5, 5).is_err());
        g.add_cart(4, 4).unwrap();
        g.add_cart(4, 4).unwrap();
        assert!(g.add_cart(4, 4).is_err(), "out of carts");
        g.world.carts[0].cargo.add(Item::Chip, 40);
        g.remove(4, 4).unwrap();
        assert!(g.world.carts.is_empty());
        assert_eq!(g.player.inventory.count(Item::Cart), 2);
        assert_eq!(g.player.inventory.count(Item::Depot), 1);
        assert_eq!(g.player.inventory.count(Item::Chip), 40);
    }

    #[test]
    fn the_tutorial_moves_on_as_each_step_is_done() {
        let mut g = Game::with_scenario(Scenario::by_id("tutorial").unwrap(), 0);
        let step = |g: &Game| g.tutorial.unwrap().step;
        assert_eq!(g.tutorial_step().unwrap().1.title, "Look around");
        g.tick();
        assert_eq!(step(&g), 0);
        // 1: put the cursor on ferrite ore.
        let (cx, cy) = g.world.spawn();
        let ore = (cy - 20..cy + 20)
            .flat_map(|y| (cx - 20..cx + 20).map(move |x| (x, y)))
            .find(|&(x, y)| {
                g.world.tile(x, y).unwrap().ore == Some(Item::FerriteOre)
                    && g.world.tile(x + 1, y).unwrap().ore == Some(Item::FerriteOre)
                    && g.world.entity(x, y).is_none()
            })
            .unwrap();
        g.player.cursor = ore;
        g.tick();
        assert_eq!(step(&g), 1);
        assert!(g.status_message().unwrap().text.contains("Dig by hand"));
        // 2: dig 5 by hand.
        g.toggle_mining().unwrap();
        run(&mut g, 4);
        assert_eq!(step(&g), 2);
        // 3 and 4: a digger, fuelled.
        g.place(ore.0, ore.1, Item::Digger, Dir::East).unwrap();
        g.tick();
        assert_eq!(step(&g), 3);
        assert_eq!(g.add_fuel(ore.0, ore.1), Some((Item::Lignite, 10)));
        g.tick();
        assert_eq!(step(&g), 4);
        // Progress is saved.
        let back = Game::from_json(&g.to_json().unwrap()).unwrap();
        assert_eq!(back.tutorial, g.tutorial);
        // The last step closes itself after a while.
        g.tutorial = Some(Tutorial {
            step: STEPS.len() - 1,
            since: g.world.tick,
        });
        run(&mut g, 31);
        assert!(g.tutorial.unwrap().finished());
        assert!(g.tutorial_step().is_none());
    }
}
