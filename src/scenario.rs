//! Ways to play: free play, the tutorial, and seeded challenges with their own rules.

use crate::game::Game;
use crate::items::{EntityKind, Item};
use crate::recipes::TechId;
use crate::world::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Feed the Uplink array 20 relay modules.
    Uplink,
    /// Make this many of an item in machines (hand crafting doesn't count).
    Produce(Item, u64),
}

#[derive(Debug, PartialEq)]
pub struct Scenario {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    /// The same map for everyone. `None` picks a fresh seed (or the `--seed` one).
    pub seed: Option<u64>,
    pub goal: Goal,
    /// Ore in the starter patches, as a share of the usual amount.
    pub starter_ore: f32,
    /// Ores without a patch near spawn: their nearest patch is 50-60 tiles out.
    pub far_ores: &'static [Item],
    /// Added to the usual starting inventory.
    pub kit: &'static [(Item, u32)],
    /// Researched from the start.
    pub techs: &'static [&'static str],
    pub tutorial: bool,
}

pub static SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "standard",
        name: "Free play",
        blurb: "A new random world. Build up to the Uplink array and transmit.",
        seed: None,
        goal: Goal::Uplink,
        starter_ore: 1.0,
        far_ores: &[],
        kit: &[],
        techs: &[],
        tutorial: false,
    },
    Scenario {
        id: "tutorial",
        name: "Tutorial",
        blurb: "Learn the basics step by step: mining, smelting, power, belts, arms and research.",
        seed: Some(2024),
        goal: Goal::Uplink,
        starter_ore: 1.0,
        far_ores: &[],
        kit: &[],
        techs: &[],
        tutorial: true,
    },
    Scenario {
        id: "far-fields",
        name: "Far fields",
        blurb: "No malachite or quartz near spawn: the nearest is 50-60 tiles away, out in the fog. You start with rails, a cart and a survey mast.",
        seed: Some(31337),
        goal: Goal::Uplink,
        starter_ore: 1.0,
        far_ores: &[Item::MalachiteOre, Item::Quartz],
        kit: &[
            (Item::Rail, 120),
            (Item::Depot, 2),
            (Item::Cart, 1),
            (Item::SurveyMast, 1),
            (Item::CopperIngot, 30),
            (Item::Quartz, 20),
        ],
        techs: &["fabrication", "surveying", "railways"],
        tutorial: false,
    },
    Scenario {
        id: "lean",
        name: "Lean ores",
        blurb: "The starter patches hold a third of the usual ore. Expand early or run dry.",
        seed: Some(4242),
        goal: Goal::Uplink,
        starter_ore: 0.3,
        far_ores: &[],
        kit: &[],
        techs: &[],
        tutorial: false,
    },
    Scenario {
        id: "sprint",
        name: "Sprint",
        blurb: "Make 60 Beta datapacks in fabricators as fast as you can.",
        seed: Some(777),
        goal: Goal::Produce(Item::BetaPack, 60),
        starter_ore: 1.0,
        far_ores: &[],
        kit: &[],
        techs: &[],
        tutorial: false,
    },
];

impl Scenario {
    pub fn standard() -> &'static Scenario {
        &SCENARIOS[0]
    }

    pub fn by_id(id: &str) -> Option<&'static Scenario> {
        SCENARIOS.iter().find(|s| s.id == id)
    }

    pub fn is_standard(&self) -> bool {
        self.id == "standard"
    }

    /// What winning means, for the objective line and the victory screen.
    pub fn goal_text(&self) -> String {
        match self.goal {
            Goal::Uplink => "Transmit 20 relay modules with the Uplink array".into(),
            Goal::Produce(item, n) => format!("Make {n} {} in machines", plural(item)),
        }
    }
}

fn plural(item: Item) -> String {
    let n = item.name();
    if n.ends_with('s') {
        n.to_string()
    } else {
        format!("{n}s")
    }
}

/// Where the player is in the tutorial.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tutorial {
    /// Index into `STEPS`; `STEPS.len()` once it's finished.
    pub step: usize,
    /// Tick the current step started.
    pub since: u64,
}

impl Tutorial {
    pub fn finished(&self) -> bool {
        self.step >= STEPS.len()
    }
}

pub struct Step {
    pub title: &'static str,
    /// `{action}` stands for the key bound to that action (see `settings::ACTIONS`).
    pub text: &'static str,
    pub done: fn(&Game, &Tutorial) -> bool,
}

fn has(g: &Game, kind: EntityKind) -> bool {
    g.world.entities.iter().flatten().any(|e| e.kind == kind)
}

fn count(g: &Game, kind: EntityKind) -> usize {
    g.world
        .entities
        .iter()
        .flatten()
        .filter(|e| e.kind == kind)
        .count()
}

fn fuelled(g: &Game, kind: EntityKind) -> bool {
    g.world.entities.iter().flatten().any(|e| {
        e.kind == kind
            && match &e.state {
                State::Miner { fuel, .. }
                | State::Furnace { fuel, .. }
                | State::Generator { fuel, .. } => fuel.has(),
                _ => false,
            }
    })
}

fn made(g: &Game, item: Item) -> u64 {
    g.world.produced.get(&item).copied().unwrap_or(0)
}

fn tech(g: &Game, id: &str) -> bool {
    TechId::by_id(id).is_some_and(|t| g.world.research.is_done(t))
}

pub static STEPS: &[Step] = &[
    Step {
        title: "Look around",
        text: "Move the cursor with the arrow keys or WASD (Shift moves 8 tiles). Ore shows as coloured dots: put the cursor on some.",
        done: |g, _| {
            let (x, y) = g.player.cursor;
            g.world.tile(x, y).is_some_and(|t| t.ore.is_some()) && g.world.is_explored(x, y)
        },
    },
    Step {
        title: "Dig by hand",
        text: "Press {mine} with the cursor on ore and keep it still. Dig up 5 ore.",
        done: |g, _| g.hand_mined >= 5,
    },
    Step {
        title: "Place a digger",
        text: "Press 3 to pick a Digger, turn it with {rotate}, and press Enter on ferrite ore (blue). It mines the 3x3 area under it.",
        done: |g, _| has(g, EntityKind::Digger) || has(g, EntityKind::Bore),
    },
    Step {
        title: "Fuel it",
        text: "Put the cursor on the digger, press {inspect} to open it and f to add 10 lignite. Esc closes the panel.",
        done: |g, _| fuelled(g, EntityKind::Digger),
    },
    Step {
        title: "Smelt",
        text: "Put a Kiln (4) on the tile the digger's arrow points at, then fuel the kiln the same way.",
        done: |g, _| fuelled(g, EntityKind::Kiln),
    },
    Step {
        title: "Ingots",
        text: "Ore goes into the kiln and comes out as ingots. Wait for 3. To take them, open the kiln ({inspect}) and press t.",
        done: |g, _| made(g, Item::IronIngot) + made(g, Item::CopperIngot) >= 3,
    },
    Step {
        title: "Conveyors",
        text: "Pick Conveyors (1), press {paint} for paint mode and move: each step lays a belt in the direction you go. Lay 5. {paint} again stops.",
        done: |g, _| count(g, EntityKind::Conveyor) >= 5,
    },
    Step {
        title: "Power",
        text: "Arms and labs need electricity. Place a Pylon (6), a Generator (7) right beside it, and fuel the generator. Out of lignite? Dig some from a dark patch with {mine}.",
        done: |g, _| has(g, EntityKind::Pylon) && fuelled(g, EntityKind::Generator),
    },
    Step {
        title: "Arms",
        text: "An Arm (2) within 2 tiles of a pylon grabs from behind it and drops ahead. Use one to move ingots or ore into a Crate (5).",
        done: |g, _| {
            g.world.entities.iter().flatten().any(|e| {
                e.kind == EntityKind::Crate
                    && matches!(&e.state, State::Chest { inv, .. } if !inv.is_empty())
            }) && has(g, EntityKind::Arm)
        },
    },
    Step {
        title: "Research",
        text: "Place the Lab (8) near a pylon. Open research with {research} and pick Fabrication.",
        done: |g, _| g.world.research.current.is_some() || tech(g, "fabrication"),
    },
    Step {
        title: "Datapacks",
        text: "Craft Alpha datapacks ({craft}, then Enter) and put them in the lab: {inspect}, then i. Research Fabrication.",
        done: |g, _| tech(g, "fabrication"),
    },
    Step {
        title: "Off you go",
        text: "That's the basics! The Goal in the sidebar always says what to do next, all the way to the Uplink. Press ? for every key.",
        done: |g, t| g.world.tick >= t.since + 30 * crate::world::TICKS_PER_SEC as u64,
    },
];

/// Replaces each `{action}` in `text` with `key(action)`.
pub fn fill_keys(text: &str, key: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push_str(&rest[..a]);
        out.push_str(&key(&rest[a + 1..a + b]));
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenarios_are_consistent() {
        let mut ids: Vec<_> = SCENARIOS.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), SCENARIOS.len());
        assert!(Scenario::standard().is_standard());
        for s in SCENARIOS {
            for t in s.techs {
                assert!(
                    TechId::by_id(t).is_some(),
                    "{} gives unknown tech {t}",
                    s.id
                );
            }
            assert_eq!(Scenario::by_id(s.id), Some(s));
        }
    }

    #[test]
    fn key_placeholders_fill_in() {
        let s = fill_keys("press {mine}, then {inspect}!", |a| a.to_uppercase());
        assert_eq!(s, "press MINE, then INSPECT!");
        assert_eq!(fill_keys("no keys", |_| "?".into()), "no keys");
        for st in STEPS {
            let filled = fill_keys(st.text, |a| {
                assert!(
                    crate::settings::ACTIONS.iter().any(|(n, _)| *n == a),
                    "{} uses unknown action {a}",
                    st.title
                );
                "k".into()
            });
            assert!(!filled.contains('{'));
        }
    }
}
