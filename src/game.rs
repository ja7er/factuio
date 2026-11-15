//! Player state and game-level actions (building, crafting, save/load).

use crate::inventory::Inventory;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId, TechId};
use crate::world::{State, TICKS_PER_SEC, World, WorldSave};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;

pub const SAVE_VERSION: u32 = 1;
pub const HAND_MINE_TIME: f32 = 0.6;
pub const MAP_W: i32 = 160;
pub const MAP_H: i32 = 120;

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
}

pub struct Game {
    pub world: World,
    pub player: Player,
    pub messages: Vec<String>,
    pub victory_pending: bool,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let world = World::generate(MAP_W, MAP_H, seed);
        let spawn = world.spawn();
        Game {
            world,
            player: Player::new(spawn),
            messages: vec!["Welcome to factuio! Press ? for help.".into()],
            victory_pending: false,
        }
    }

    #[cfg(test)]
    pub fn from_world(world: World) -> Self {
        let spawn = world.spawn();
        Game {
            world,
            player: Player::new(spawn),
            messages: vec![],
            victory_pending: false,
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.messages.push(msg.into());
        if self.messages.len() > 200 {
            self.messages.drain(0..100);
        }
    }

    pub fn tick(&mut self) {
        let was_won = self.world.won;
        self.world.tick();
        self.tick_crafting();
        self.tick_mining();
        for e in std::mem::take(&mut self.world.events) {
            self.log(e);
        }
        if self.world.won && !was_won {
            self.victory_pending = true;
        }
    }

    // ---- Building ----

    pub fn place(&mut self, x: i32, y: i32, item: Item, dir: Dir) -> Result<(), String> {
        let kind = item
            .building()
            .ok_or_else(|| format!("{} can't be placed", item.name()))?;
        if self.player.inventory.count(item) == 0 {
            return Err(format!("No {} in inventory", item.name()));
        }
        self.world.place(x, y, kind, dir)?;
        self.player.inventory.remove(item, 1);
        Ok(())
    }

    /// Picks up the building at (x, y) along with everything inside it.
    pub fn remove(&mut self, x: i32, y: i32) -> Result<EntityKind, String> {
        let e = self.world.remove(x, y).ok_or("Nothing to remove here")?;
        let mut c = e.contents();
        c.add(e.kind.item(), 1);
        c.drain_into(&mut self.player.inventory);
        Ok(e.kind)
    }

    /// Takes the outputs (or stored items) of the building at (x, y).
    pub fn take_output(&mut self, x: i32, y: i32) -> u32 {
        let Some(e) = self.world.entity_mut(x, y) else {
            return 0;
        };
        let mut got = Inventory::new();
        match &mut e.state {
            State::Belt { items } => {
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
            | State::Chest { inv: output } => {
                output.drain_into(&mut got);
            }
            State::Lab { inv, .. } | State::Uplink { inv, .. } => inv.drain_into(&mut got),
            State::Generator { .. } | State::Pylon | State::Solar => {}
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
        if let Some(r) = r {
            if r.get().category != Category::Craft {
                return Err("Fabricators can't smelt".into());
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
        };
        serde_json::to_string(&s).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Game, String> {
        let s: SaveFile = serde_json::from_str(json).map_err(|e| format!("Bad save file: {e}"))?;
        if s.version != SAVE_VERSION {
            return Err(format!("Unsupported save version {}", s.version));
        }
        Ok(Game {
            world: World::from_save(s.world)?,
            player: s.player,
            messages: vec!["Game loaded.".into()],
            victory_pending: false,
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = self.to_json()?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
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
        if w.won {
            return "You transmitted! Keep building as long as you like.".into();
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
            return "Automate datapacks and research toward Uplink theory.".into();
        }
        if !has(EntityKind::Uplink) {
            return "Craft and place the Uplink array, then power it.".into();
        }
        "Feed the Uplink 20 relay modules while it has power.".into()
    }

    pub fn play_time(&self) -> String {
        let secs = self.world.tick / TICKS_PER_SEC as u64;
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
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
}
