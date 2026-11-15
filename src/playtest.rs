//! Scripted play used by integration tests and screenshot dumps.

use crate::game::Game;
use crate::items::{Dir, Item};
use crate::recipes::{RecipeId, TechId};
use crate::world::{State, TICKS_PER_SEC, Terrain};

pub fn run(g: &mut Game, secs: u32) {
    for _ in 0..secs * TICKS_PER_SEC {
        g.tick();
    }
}

fn free(g: &Game, x: i32, y: i32) -> bool {
    g.world
        .tile(x, y)
        .is_some_and(|t| t.terrain != Terrain::Water)
        && g.world.entity(x, y).is_none()
}

/// Finds a spot on `ore` near spawn where a digger facing east has `len` free tiles after it.
pub fn find_line(g: &Game, ore: Item, len: i32) -> Option<(i32, i32)> {
    let (cx, cy) = g.world.spawn();
    let mut best: Option<((i32, i32), i32)> = None;
    for y in cy - 30..cy + 30 {
        for x in cx - 30..cx + 30 {
            let Some(t) = g.world.tile(x, y) else {
                continue;
            };
            if t.ore != Some(ore) || !free(g, x, y) {
                continue;
            }
            // Leave a clear row above/below for pylons.
            if !(1..=len).all(|k| free(g, x + k, y)) {
                continue;
            }
            let d = (x - cx).abs() + (y - cy).abs();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some(((x, y), d));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Places a pylon at or next to (x, y).
fn pylon_near(g: &mut Game, x: i32, y: i32) -> Option<(i32, i32)> {
    for (dx, dy) in [
        (0, 0),
        (0, 1),
        (0, -1),
        (1, 0),
        (-1, 0),
        (1, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
    ] {
        if free(g, x + dx, y + dy) && g.place(x + dx, y + dy, Item::Pylon, Dir::North).is_ok() {
            return Some((x + dx, y + dy));
        }
    }
    None
}

/// Lays a chain of pylons from `a` to `b`.
pub fn wire(g: &mut Game, a: (i32, i32), b: (i32, i32)) {
    let mut cur = a;
    loop {
        let (dx, dy) = (b.0 - cur.0, b.1 - cur.1);
        if dx * dx + dy * dy <= 30 {
            return;
        }
        let step = 5;
        let nx = cur.0 + dx.signum() * step.min(dx.abs());
        let ny = cur.1
            + if dx.abs() < step {
                dy.signum() * step.min(dy.abs())
            } else {
                0
            };
        g.player.inventory.add(Item::Pylon, 1);
        match pylon_near(g, nx, ny) {
            Some(p) => cur = p,
            None => return,
        }
    }
}

pub struct Line {
    pub digger: (i32, i32),
    pub end: (i32, i32),
}

/// Digger → kiln → arm → crate on the given ore. Returns positions.
pub fn smelting_line(g: &mut Game, ore: Item) -> Line {
    let (x, y) = find_line(g, ore, 3).expect("ore patch");
    g.place(x, y, Item::Digger, Dir::East).unwrap();
    g.place(x + 1, y, Item::Kiln, Dir::North).unwrap();
    g.place(x + 2, y, Item::Arm, Dir::East).unwrap();
    g.place(x + 3, y, Item::Crate, Dir::North).unwrap();
    g.insert(x, y, Item::Lignite, 5);
    g.insert(x + 1, y, Item::Lignite, 5);
    Line {
        digger: (x, y),
        end: (x + 3, y),
    }
}

/// A small but complete starter factory: two smelting lines, lignite mining, power.
pub fn build_demo_factory(g: &mut Game) {
    // Fuel source: lignite into a crate.
    let (lx, ly) = find_line(g, Item::Lignite, 1).expect("lignite");
    g.place(lx, ly, Item::Digger, Dir::East).unwrap();
    g.place(lx + 1, ly, Item::Crate, Dir::North).unwrap();
    g.insert(lx, ly, Item::Lignite, 5);

    let iron = smelting_line(g, Item::FerriteOre);
    let copper = smelting_line(g, Item::MalachiteOre);

    // Power: generator next to the iron line, pylons to both arms.
    let (ax, ay) = (iron.end.0 - 1, iron.end.1);
    let p1 = pylon_near(g, ax, ay + 1)
        .or_else(|| pylon_near(g, ax, ay - 1))
        .expect("pylon");
    let mut placed = false;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1)] {
        if free(g, p1.0 + dx, p1.1 + dy)
            && g.place(p1.0 + dx, p1.1 + dy, Item::Generator, Dir::North)
                .is_ok()
        {
            g.insert(p1.0 + dx, p1.1 + dy, Item::Lignite, 15);
            placed = true;
            break;
        }
    }
    assert!(placed, "no room for generator");
    let (bx, by) = (copper.end.0 - 1, copper.end.1);
    let p2 = pylon_near(g, bx, by + 1)
        .or_else(|| pylon_near(g, bx, by - 1))
        .expect("pylon");
    wire(g, p1, p2);

    // A conveyor showcase: a short belt from the lignite crate area.
    let start = (lx + 2, ly);
    for k in 0..6 {
        if free(g, start.0 + k, start.1) {
            let _ = g.place(start.0 + k, start.1, Item::Conveyor, Dir::East);
        }
    }
    if let Some(e) = g.world.entity_mut(start.0, start.1)
        && let State::Belt { items, .. } = &mut e.state
    {
        items.push(crate::world::BeltItem {
            item: Item::Lignite,
            pos: 0,
        });
    }

    // Lab next to the copper line, powered by the second pylon.
    for (dx, dy) in [(0, 1), (1, 0), (-1, 0), (0, -1)] {
        if free(g, p2.0 + dx, p2.1 + dy)
            && g.place(p2.0 + dx, p2.1 + dy, Item::Lab, Dir::North).is_ok()
        {
            break;
        }
    }
    g.player.cursor = (iron.digger.0 + 1, iron.digger.1);
    run(g, 30);
}

/// Finds a clear grass rectangle near spawn.
fn find_clear(g: &Game, w: i32, h: i32) -> Option<(i32, i32)> {
    let (cx, cy) = g.world.spawn();
    let mut best: Option<((i32, i32), i32)> = None;
    for y in cy - 35..cy + 35 {
        for x in cx - 45..cx + 45 {
            let ok = (0..h).all(|dy| {
                (0..w).all(|dx| {
                    g.world
                        .tile(x + dx, y + dy)
                        .is_some_and(|t| t.terrain != Terrain::Water && t.ore.is_none())
                        && g.world.entity(x + dx, y + dy).is_none()
                })
            });
            let d = (x + w / 2 - cx).abs() + (y + h / 2 - cy).abs();
            if ok && best.is_none_or(|(_, bd)| d < bd) {
                best = Some(((x, y), d));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// A busier mid-game block for screenshots: two conveyor lines feeding rows of fabricators.
/// Returns the position of one fabricator.
pub fn build_showcase(g: &mut Game) -> (i32, i32) {
    for t in ["fabrication", "logistics", "logic", "metallurgy"] {
        g.world.research.complete(TechId::by_id(t).unwrap());
    }
    for (i, n) in [
        (Item::Fabricator, 6),
        (Item::Arm, 12),
        (Item::SwiftArm, 2),
        (Item::Conveyor, 20),
        (Item::ExpressConveyor, 20),
        (Item::Crate, 8),
        (Item::Pylon, 8),
        (Item::Generator, 1),
        (Item::IronIngot, 400),
        (Item::CopperIngot, 400),
        (Item::Lignite, 50),
    ] {
        g.player.inventory.add(i, n);
    }
    let (x0, y0) = find_clear(g, 22, 13).expect("clear area");
    let (top, bot, mid) = (y0 + 4, y0 + 8, y0 + 6);
    let place = |g: &mut Game, x, y, item, d| g.place(x, y, item, d).unwrap();
    // Crates of ingots loaded onto the two lines by swift arms.
    place(g, x0, top, Item::Crate, Dir::North);
    g.insert(x0, top, Item::IronIngot, 400);
    place(g, x0 + 1, top, Item::SwiftArm, Dir::East);
    place(g, x0, bot, Item::Crate, Dir::North);
    g.insert(x0, bot, Item::CopperIngot, 400);
    place(g, x0 + 1, bot, Item::SwiftArm, Dir::East);
    for x in x0 + 2..x0 + 20 {
        place(g, x, top, Item::ExpressConveyor, Dir::East);
        place(g, x, bot, Item::Conveyor, Dir::East);
    }
    let mut fab = (0, 0);
    for (k, x) in [x0 + 5, x0 + 10, x0 + 15].into_iter().enumerate() {
        // Cogs above the iron line.
        place(g, x, top - 1, Item::Arm, Dir::North);
        place(g, x, top - 2, Item::Fabricator, Dir::North);
        g.set_recipe(x, top - 2, RecipeId::by_id("cog")).unwrap();
        place(g, x, top - 3, Item::Arm, Dir::North);
        place(g, x, top - 4, Item::Crate, Dir::North);
        place(g, x + 1, top - 2, Item::Pylon, Dir::North);
        // Coils below the copper line.
        place(g, x, bot + 1, Item::Arm, Dir::South);
        place(g, x, bot + 2, Item::Fabricator, Dir::North);
        g.set_recipe(x, bot + 2, RecipeId::by_id("coil")).unwrap();
        place(g, x, bot + 3, Item::Arm, Dir::South);
        place(g, x, bot + 4, Item::Crate, Dir::North);
        place(g, x + 1, bot + 2, Item::Pylon, Dir::North);
        if k == 1 {
            fab = (x, top - 2);
        }
    }
    // Power between the two lines.
    place(g, x0 + 3, mid, Item::Pylon, Dir::North);
    place(g, x0 + 6, mid, Item::Pylon, Dir::North);
    place(g, x0 + 7, mid, Item::Generator, Dir::North);
    g.insert(x0 + 7, mid, Item::Lignite, 50);
    run(g, 40);
    g.player.cursor = fab;
    fab
}

fn crate_take(g: &mut Game, pos: (i32, i32)) -> u32 {
    g.take_output(pos.0, pos.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_game_playthrough_researches_fabrication() {
        let mut g = Game::new(2024);
        build_demo_factory(&mut g);
        let iron = smelting_line_ends(&g, Item::FerriteOre);
        run(&mut g, 120);
        // Ingots are flowing into the crates via powered arms.
        let iron_before = g.player.inventory.count(Item::IronIngot);
        for p in iron {
            crate_take(&mut g, p);
        }
        assert!(
            g.player.inventory.count(Item::IronIngot) > iron_before + 20,
            "iron line produced {}",
            g.player.inventory.count(Item::IronIngot) - iron_before
        );
        let copper_crates = smelting_line_ends(&g, Item::MalachiteOre);
        for p in copper_crates {
            crate_take(&mut g, p);
        }
        assert!(g.player.inventory.count(Item::CopperIngot) >= 10);

        // Hand-craft alpha packs and research Fabrication in the lab.
        let alpha = RecipeId::by_id("alpha-pack").unwrap();
        let n = g.queue_craft(alpha, 10).unwrap();
        assert_eq!(n, 10);
        run(&mut g, 70);
        assert_eq!(g.player.inventory.count(Item::AlphaPack), 10);
        let lab = lab_pos(&g);
        assert_eq!(g.insert(lab.0, lab.1, Item::AlphaPack, 10), 10);
        let fab = TechId::by_id("fabrication").unwrap();
        g.start_research(fab).unwrap();
        // Keep the generator fueled from the lignite crate while we wait.
        for _ in 0..6 {
            run(&mut g, 10);
            refuel_all(&mut g);
        }
        assert!(
            g.world.research.is_done(fab),
            "lab status {:?}",
            g.world.entity(lab.0, lab.1).unwrap().status
        );
        assert!(g.messages.iter().any(|m| m.text.contains("Fabrication")));

        // Now a fabricator can be crafted, placed and run.
        let fr = RecipeId::by_id("fabricator").unwrap();
        g.player.inventory.add(Item::IronIngot, 30);
        g.player.inventory.add(Item::CopperIngot, 10);
        g.queue_craft(fr, 1).unwrap();
        run(&mut g, 10);
        assert_eq!(g.player.inventory.count(Item::Fabricator), 1);

        // Save and load the whole thing.
        let json = g.to_json().unwrap();
        let back = Game::from_json(&json).unwrap();
        assert!(back.world.research.is_done(fab));
        assert_eq!(
            back.world.entities.iter().flatten().count(),
            g.world.entities.iter().flatten().count()
        );
    }

    fn smelting_line_ends(g: &Game, ore: Item) -> Vec<(i32, i32)> {
        let mut v = vec![];
        for (i, e) in g.world.entities.iter().enumerate() {
            if let Some(e) = e
                && e.kind == crate::items::EntityKind::Digger
            {
                let (x, y) = g.world.pos(i);
                let is_line = g
                    .world
                    .entity(x + 1, y)
                    .is_some_and(|k| k.kind == crate::items::EntityKind::Kiln);
                if is_line {
                    let k = g.world.idx(x + 1, y).unwrap();
                    let smelts = match &g.world.entities[k].as_ref().unwrap().state {
                        State::Furnace { output, job, .. } => {
                            let target = if ore == Item::FerriteOre {
                                Item::IronIngot
                            } else {
                                Item::CopperIngot
                            };
                            output.count(target) > 0
                                || job.is_some_and(|j| j.get().main_output() == target)
                        }
                        _ => false,
                    };
                    if smelts {
                        v.push((x + 3, y));
                        v.push((x + 1, y));
                    }
                }
            }
        }
        v
    }

    fn lab_pos(g: &Game) -> (i32, i32) {
        let i = g
            .world
            .entities
            .iter()
            .position(|e| {
                e.as_ref()
                    .is_some_and(|e| e.kind == crate::items::EntityKind::Lab)
            })
            .unwrap();
        g.world.pos(i)
    }

    fn refuel_all(g: &mut Game) {
        let positions: Vec<(i32, i32)> = g
            .world
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_some())
            .map(|(i, _)| g.world.pos(i))
            .collect();
        for (x, y) in positions {
            if g.player.inventory.count(Item::Lignite) < 5 {
                g.player.inventory.add(Item::Lignite, 20);
            }
            g.insert(x, y, Item::Lignite, 5);
        }
    }

    #[test]
    fn whole_tech_tree_and_victory_are_reachable() {
        use crate::items::EntityKind;
        use crate::world::World;
        let mut g = Game::new(7);
        g.world = World::empty(40, 30, 7);
        g.player.cursor = (10, 10);
        // Power: generator + pylon, plus labs around the pylon.
        g.player.inventory.add(Item::Lab, 3);
        g.player.inventory.add(Item::Lignite, 200);
        g.place(10, 10, Item::Pylon, Dir::North).unwrap();
        g.place(11, 10, Item::Generator, Dir::North).unwrap();
        g.insert(11, 10, Item::Lignite, 50);
        let labs = [(9, 9), (10, 9), (11, 9), (9, 10)];
        for &(x, y) in &labs {
            g.place(x, y, Item::Lab, Dir::North).unwrap();
        }
        let mut guard = 0;
        while !g.world.research.all_done() {
            guard += 1;
            assert!(guard < 50, "research stalled");
            let t = TechId::all()
                .find(|&t| g.world.research.is_available(t))
                .expect("an available tech");
            g.start_research(t).unwrap();
            let tech = t.get();
            for &p in tech.packs {
                g.player.inventory.add(p, tech.units);
            }
            let per_lab = tech.units.div_ceil(labs.len() as u32);
            for &(x, y) in &labs {
                for &p in tech.packs {
                    g.insert(x, y, p, per_lab);
                }
            }
            let secs = (per_lab as f32 * tech.unit_time) as u32 + 10;
            for _ in 0..secs / 10 + 1 {
                run(&mut g, 10);
                g.insert(11, 10, Item::Lignite, 50);
            }
            assert!(g.world.research.is_done(t), "{} not finished", tech.name);
        }
        // Every recipe is now unlocked.
        assert!(RecipeId::all().all(|r| g.world.research.is_unlocked(r)));
        // Build the uplink by hand from raw-ish materials and win.
        for (i, n) in [
            (Item::Steel, 200),
            (Item::IronIngot, 200),
            (Item::CopperIngot, 300),
            (Item::Glass, 200),
        ] {
            g.player.inventory.add(i, n);
        }
        let up = RecipeId::by_id("uplink").unwrap();
        assert_eq!(g.queue_craft(up, 1).unwrap(), 1);
        let relay = RecipeId::by_id("relay-module").unwrap();
        assert_eq!(g.queue_craft(relay, 20).unwrap(), 20);
        run(&mut g, 900);
        assert_eq!(g.player.inventory.count(Item::Uplink), 1);
        assert_eq!(g.player.inventory.count(Item::RelayModule), 20);
        g.place(10, 11, Item::Uplink, Dir::North).unwrap();
        assert_eq!(g.insert(10, 11, Item::RelayModule, 20), 20);
        for _ in 0..12 {
            run(&mut g, 10);
            g.insert(11, 10, Item::Lignite, 50);
        }
        assert!(g.world.won);
        assert!(g.victory_pending);
        assert_eq!(g.world.entity(10, 11).unwrap().kind, EntityKind::Uplink);
    }

    #[test]
    fn showcase_factory_runs() {
        let mut g = Game::new(11);
        build_demo_factory(&mut g);
        let (x, y) = build_showcase(&mut g);
        run(&mut g, 20);
        let e = g.world.entity(x, y).unwrap();
        assert!(!e.status.is_problem(), "fabricator status {:?}", e.status);
        assert!(g.world.produced.get(&Item::Cog).copied().unwrap_or(0) > 20);
        assert!(g.world.produced.get(&Item::Coil).copied().unwrap_or(0) > 20);
    }
}
