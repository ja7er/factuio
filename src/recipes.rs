//! Recipes and the research tree.

use crate::items::Item;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Hand-craftable, or made in a Fabricator.
    Craft,
    /// Made in a Kiln or Arc furnace.
    Smelt,
    /// Made in a Chem plant.
    Chem,
}

#[derive(Debug)]
pub struct Recipe {
    pub id: &'static str,
    pub inputs: &'static [(Item, u32)],
    pub outputs: &'static [(Item, u32)],
    /// Seconds at speed 1.
    pub time: f32,
    pub category: Category,
    /// Available from the start, without research.
    pub default: bool,
}

impl Recipe {
    pub fn main_output(&self) -> Item {
        self.outputs[0].0
    }

    pub fn input_count(&self, item: Item) -> u32 {
        self.inputs
            .iter()
            .filter(|(i, _)| *i == item)
            .map(|(_, n)| n)
            .sum()
    }

    pub fn name(&self) -> &'static str {
        self.main_output().name()
    }
}

use Category::*;
use Item::*;

macro_rules! r {
    ($id:literal, [$($ii:ident $in:literal),*] => [$($oi:ident $on:literal),*], $t:literal, $cat:ident, $def:literal) => {
        Recipe {
            id: $id,
            inputs: &[$(($ii, $in)),*],
            outputs: &[$(($oi, $on)),*],
            time: $t,
            category: $cat,
            default: $def,
        }
    };
}

pub static RECIPES: &[Recipe] = &[
    // Smelting
    r!("iron-ingot", [FerriteOre 1] => [IronIngot 1], 3.2, Smelt, true),
    r!("copper-ingot", [MalachiteOre 1] => [CopperIngot 1], 3.2, Smelt, true),
    r!("glass", [Quartz 2] => [Glass 1], 3.2, Smelt, true),
    r!("steel", [IronIngot 5] => [Steel 1], 16.0, Smelt, false),
    r!("coke", [Lignite 2] => [Coke 1], 6.4, Smelt, false),
    // Chemistry
    r!("resin", [Lignite 1, Water 2] => [Resin 1], 2.0, Chem, false),
    r!("battery", [Resin 2, CopperIngot 2, Steel 1] => [Battery 1], 5.0, Chem, false),
    // Intermediates
    r!("cog", [IronIngot 2] => [Cog 1], 0.5, Craft, true),
    r!("coil", [CopperIngot 1] => [Coil 2], 0.5, Craft, true),
    r!("chip", [IronIngot 1, Coil 3] => [Chip 1], 0.5, Craft, true),
    r!("lens", [Glass 3, Coil 1] => [Lens 1], 2.0, Craft, false),
    r!("motor", [Steel 1, Cog 2, Coil 4] => [Motor 1], 4.0, Craft, false),
    r!("processor", [Chip 2, Lens 1, Coil 4] => [Processor 1], 6.0, Craft, false),
    r!("relay-module", [Processor 1, Motor 1, Steel 2] => [RelayModule 1], 10.0, Craft, false),
    r!("speed-module", [Processor 1, Chip 4, Resin 2] => [SpeedModule 1], 15.0, Craft, false),
    r!("productivity-module", [Processor 2, Lens 2, Resin 4, Battery 1] => [ProductivityModule 1], 20.0, Craft, false),
    r!("landfill", [Quartz 8] => [Landfill 1], 1.0, Craft, false),
    // Datapacks
    r!("alpha-pack", [Cog 1, CopperIngot 1] => [AlphaPack 1], 5.0, Craft, true),
    r!("beta-pack", [Chip 1, Glass 2] => [BetaPack 1], 6.0, Craft, false),
    r!("gamma-pack", [Motor 1, Lens 1, Steel 1] => [GammaPack 2], 12.0, Craft, false),
    r!("delta-pack", [Processor 1, Battery 1, Resin 2] => [DeltaPack 2], 16.0, Craft, false),
    // Buildings
    r!("conveyor", [IronIngot 1, Cog 1] => [Conveyor 2], 0.5, Craft, true),
    r!("express-conveyor", [Conveyor 1, Cog 2] => [ExpressConveyor 1], 1.0, Craft, false),
    r!("arm", [IronIngot 1, Cog 1, Chip 1] => [Arm 1], 0.5, Craft, true),
    r!("swift-arm", [Arm 1, Chip 2] => [SwiftArm 1], 0.5, Craft, false),
    r!("long-arm", [Arm 1, Cog 1, IronIngot 1] => [LongArm 1], 0.5, Craft, false),
    r!("filter-arm", [SwiftArm 1, Chip 3] => [FilterArm 1], 0.5, Craft, false),
    r!("splitter", [Conveyor 2, Cog 2, Chip 1] => [Splitter 1], 1.0, Craft, false),
    r!("tunnel", [Conveyor 4, IronIngot 4] => [Tunnel 2], 1.0, Craft, false),
    r!("digger", [Cog 3, IronIngot 3, Quartz 5] => [Digger 1], 2.0, Craft, true),
    r!("bore", [Chip 3, Cog 5, IronIngot 10] => [Bore 1], 2.0, Craft, false),
    r!("kiln", [Quartz 5] => [Kiln 1], 0.5, Craft, true),
    r!("arc-furnace", [Steel 10, Chip 5, Quartz 10] => [ArcFurnace 1], 5.0, Craft, false),
    r!("crate", [IronIngot 4] => [Crate 1], 0.5, Craft, true),
    r!("generator", [Cog 5, IronIngot 5, Kiln 1] => [Generator 1], 2.0, Craft, true),
    r!("pylon", [IronIngot 1, Coil 2] => [Pylon 1], 0.5, Craft, true),
    r!("sun-panel", [Steel 5, Glass 5, Chip 3] => [SunPanel 1], 10.0, Craft, false),
    r!("lab", [Chip 10, Cog 10, Conveyor 4] => [Lab 1], 2.0, Craft, true),
    r!("fabricator", [Chip 3, Cog 5, IronIngot 9] => [Fabricator 1], 0.5, Craft, false),
    r!("uplink", [Steel 50, Chip 20, Glass 20, Motor 10] => [Uplink 1], 30.0, Craft, false),
    r!("pump", [IronIngot 5, Cog 3, Motor 1] => [Pump 1], 2.0, Craft, false),
    r!("steam-turbine", [Steel 10, Cog 10, Motor 2, Generator 1] => [SteamTurbine 1], 5.0, Craft, false),
    r!("sifter", [Steel 4, Cog 6, Motor 1, Chip 2] => [Sifter 1], 2.0, Craft, false),
    r!("chem-plant", [Steel 8, Glass 10, Chip 5, Motor 2] => [ChemPlant 1], 5.0, Craft, false),
    r!("battery-bank", [Battery 10, Steel 4, Chip 2] => [BatteryBank 1], 5.0, Craft, false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecipeId(pub u16);

impl RecipeId {
    pub fn get(self) -> &'static Recipe {
        &RECIPES[self.0 as usize]
    }

    pub fn by_id(id: &str) -> Option<RecipeId> {
        RECIPES
            .iter()
            .position(|r| r.id == id)
            .map(|i| RecipeId(i as u16))
    }

    pub fn all() -> impl Iterator<Item = RecipeId> {
        (0..RECIPES.len()).map(|i| RecipeId(i as u16))
    }

    /// Recipes (of any category) that use `item` as an ingredient.
    pub fn consumers(item: Item) -> Vec<RecipeId> {
        RecipeId::all()
            .filter(|r| r.get().inputs.iter().any(|&(i, _)| i == item))
            .collect()
    }

    /// The crafting recipe that makes `item`, if any.
    pub fn producing(item: Item) -> Option<RecipeId> {
        RecipeId::all()
            .find(|r| r.get().category == Category::Craft && r.get().main_output() == item)
    }
}

impl Serialize for RecipeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.get().id)
    }
}

impl<'de> Deserialize<'de> for RecipeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        RecipeId::by_id(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown recipe {s}")))
    }
}

#[derive(Debug)]
pub struct Tech {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub packs: &'static [Item],
    pub units: u32,
    /// Seconds per unit at lab speed 1.
    pub unit_time: f32,
    pub prereqs: &'static [&'static str],
    pub unlocks: &'static [&'static str],
    /// Endless research: each level costs `units` more than the last and adds a bonus.
    pub repeatable: bool,
}

pub static TECHS: &[Tech] = &[
    Tech {
        id: "fabrication",
        name: "Fabrication",
        desc: "Electric Fabricators that craft recipes automatically.",
        packs: &[AlphaPack],
        units: 10,
        unit_time: 3.0,
        prereqs: &[],
        unlocks: &["fabricator"],
        repeatable: false,
    },
    Tech {
        id: "logistics",
        name: "Logistics",
        desc: "Express conveyors, swift arms and long arms that reach two tiles.",
        packs: &[AlphaPack],
        units: 20,
        unit_time: 3.0,
        prereqs: &["fabrication"],
        unlocks: &["express-conveyor", "swift-arm", "long-arm"],
        repeatable: false,
    },
    Tech {
        id: "routing",
        name: "Belt routing",
        desc: "Splitters share one belt between up to three, and tunnel belts pass under buildings.",
        packs: &[AlphaPack],
        units: 25,
        unit_time: 3.0,
        prereqs: &["logistics"],
        unlocks: &["splitter", "tunnel"],
        repeatable: false,
    },
    Tech {
        id: "logic",
        name: "Logic theory",
        desc: "Beta datapacks, made from chips and glass.",
        packs: &[AlphaPack],
        units: 30,
        unit_time: 3.0,
        prereqs: &["fabrication"],
        unlocks: &["beta-pack"],
        repeatable: false,
    },
    Tech {
        id: "metallurgy",
        name: "Metallurgy",
        desc: "Smelt iron ingots into steel beams.",
        packs: &[AlphaPack],
        units: 30,
        unit_time: 3.0,
        prereqs: &["fabrication"],
        unlocks: &["steel"],
        repeatable: false,
    },
    Tech {
        id: "optics",
        name: "Optics",
        desc: "Grind glass into lenses.",
        packs: &[AlphaPack, BetaPack],
        units: 25,
        unit_time: 4.0,
        prereqs: &["logic"],
        unlocks: &["lens"],
        repeatable: false,
    },
    Tech {
        id: "bores",
        name: "Electric mining",
        desc: "Electric bores: fast miners that need no fuel.",
        packs: &[AlphaPack, BetaPack],
        units: 30,
        unit_time: 4.0,
        prereqs: &["logic"],
        unlocks: &["bore"],
        repeatable: false,
    },
    Tech {
        id: "sorting",
        name: "Sorting",
        desc: "Filter arms that move only the item you choose.",
        packs: &[AlphaPack, BetaPack],
        units: 30,
        unit_time: 4.0,
        prereqs: &["logistics", "logic"],
        unlocks: &["filter-arm"],
        repeatable: false,
    },
    Tech {
        id: "motors",
        name: "Motors",
        desc: "Motors from steel, cogs and coils.",
        packs: &[AlphaPack, BetaPack],
        units: 40,
        unit_time: 4.0,
        prereqs: &["logic", "metallurgy"],
        unlocks: &["motor"],
        repeatable: false,
    },
    Tech {
        id: "solar",
        name: "Solar power",
        desc: "Sun panels: fuel-free power.",
        packs: &[AlphaPack, BetaPack],
        units: 40,
        unit_time: 4.0,
        prereqs: &["optics", "metallurgy"],
        unlocks: &["sun-panel"],
        repeatable: false,
    },
    Tech {
        id: "arc-smelting",
        name: "Arc smelting",
        desc: "Arc furnaces: fast electric smelting.",
        packs: &[AlphaPack, BetaPack],
        units: 50,
        unit_time: 4.0,
        prereqs: &["metallurgy", "logic"],
        unlocks: &["arc-furnace"],
        repeatable: false,
    },
    Tech {
        id: "landfill",
        name: "Land reclamation",
        desc: "Landfill: turns a water tile into sand you can build on.",
        packs: &[AlphaPack],
        units: 20,
        unit_time: 3.0,
        prereqs: &["fabrication"],
        unlocks: &["landfill"],
        repeatable: false,
    },
    Tech {
        id: "hydraulics",
        name: "Hydraulics",
        desc: "Pumps that draw water, and steam turbines that turn it into lots of power.",
        packs: &[AlphaPack, BetaPack],
        units: 30,
        unit_time: 4.0,
        prereqs: &["metallurgy", "motors"],
        unlocks: &["pump", "steam-turbine"],
        repeatable: false,
    },
    Tech {
        id: "coking",
        name: "Coking",
        desc: "Arc furnaces bake lignite into coke, a denser fuel (2.5x the energy).",
        packs: &[AlphaPack, BetaPack],
        units: 30,
        unit_time: 4.0,
        prereqs: &["arc-smelting"],
        unlocks: &["coke"],
        repeatable: false,
    },
    Tech {
        id: "sifting",
        name: "Sifting",
        desc: "Sifters: electric machines that pull endless quartz out of sand.",
        packs: &[AlphaPack, BetaPack],
        units: 25,
        unit_time: 4.0,
        prereqs: &["bores", "motors"],
        unlocks: &["sifter"],
        repeatable: false,
    },
    Tech {
        id: "advanced-research",
        name: "Advanced research",
        desc: "Gamma datapacks.",
        packs: &[AlphaPack, BetaPack],
        units: 50,
        unit_time: 5.0,
        prereqs: &["motors", "optics"],
        unlocks: &["gamma-pack"],
        repeatable: false,
    },
    Tech {
        id: "computing",
        name: "Computing",
        desc: "Processors built from chips, lenses and coils.",
        packs: &[AlphaPack, BetaPack, GammaPack],
        units: 40,
        unit_time: 5.0,
        prereqs: &["advanced-research"],
        unlocks: &["processor"],
        repeatable: false,
    },
    Tech {
        id: "chemistry",
        name: "Chemistry",
        desc: "Chem plants that make resin from lignite and water.",
        packs: &[AlphaPack, BetaPack, GammaPack],
        units: 50,
        unit_time: 5.0,
        prereqs: &["hydraulics", "advanced-research"],
        unlocks: &["chem-plant", "resin"],
        repeatable: false,
    },
    Tech {
        id: "energy-storage",
        name: "Energy storage",
        desc: "Batteries, and battery banks that keep solar power for the night.",
        packs: &[AlphaPack, BetaPack, GammaPack],
        units: 50,
        unit_time: 5.0,
        prereqs: &["chemistry", "solar"],
        unlocks: &["battery", "battery-bank"],
        repeatable: false,
    },
    Tech {
        id: "speed-modules",
        name: "Speed modules",
        desc: "Modules that make a machine 50% faster for 50% more power.",
        packs: &[AlphaPack, BetaPack, GammaPack],
        units: 60,
        unit_time: 5.0,
        prereqs: &["chemistry", "computing"],
        unlocks: &["speed-module"],
        repeatable: false,
    },
    Tech {
        id: "field-theory",
        name: "Field theory",
        desc: "Delta datapacks, made from processors, batteries and resin.",
        packs: &[AlphaPack, BetaPack, GammaPack],
        units: 60,
        unit_time: 5.0,
        prereqs: &["energy-storage", "computing"],
        unlocks: &["delta-pack"],
        repeatable: false,
    },
    Tech {
        id: "productivity-modules",
        name: "Productivity modules",
        desc: "Modules that add 10% extra output, at 15% less speed and 50% more power.",
        packs: &[AlphaPack, BetaPack, GammaPack, DeltaPack],
        units: 80,
        unit_time: 6.0,
        prereqs: &["speed-modules", "field-theory"],
        unlocks: &["productivity-module"],
        repeatable: false,
    },
    Tech {
        id: "uplink",
        name: "Uplink theory",
        desc: "The Uplink array and its relay modules. Transmit to win.",
        packs: &[AlphaPack, BetaPack, GammaPack, DeltaPack],
        units: 75,
        unit_time: 5.0,
        prereqs: &["computing", "solar", "field-theory"],
        unlocks: &["uplink", "relay-module"],
        repeatable: false,
    },
    Tech {
        id: "mining-productivity",
        name: "Mining productivity",
        desc: "Endless: every level makes diggers, bores, sifters and pumps 10% faster.",
        packs: &[AlphaPack, BetaPack, GammaPack, DeltaPack],
        units: 50,
        unit_time: 6.0,
        prereqs: &["productivity-modules"],
        unlocks: &[],
        repeatable: true,
    },
    Tech {
        id: "servo-tuning",
        name: "Servo tuning",
        desc: "Endless: every level makes all arms 10% faster.",
        packs: &[AlphaPack, BetaPack, GammaPack, DeltaPack],
        units: 40,
        unit_time: 6.0,
        prereqs: &["field-theory"],
        unlocks: &[],
        repeatable: true,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TechId(pub u16);

impl TechId {
    pub fn get(self) -> &'static Tech {
        &TECHS[self.0 as usize]
    }

    pub fn by_id(id: &str) -> Option<TechId> {
        TECHS
            .iter()
            .position(|t| t.id == id)
            .map(|i| TechId(i as u16))
    }

    pub fn all() -> impl Iterator<Item = TechId> {
        (0..TECHS.len()).map(|i| TechId(i as u16))
    }
}

impl Serialize for TechId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.get().id)
    }
}

impl<'de> Deserialize<'de> for TechId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        TechId::by_id(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown tech {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_ids_unique() {
        let mut ids: Vec<_> = RECIPES.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), RECIPES.len());
    }

    #[test]
    fn tech_references_resolve() {
        for t in TECHS {
            for p in t.prereqs {
                assert!(TechId::by_id(p).is_some(), "{} prereq {p}", t.id);
            }
            for u in t.unlocks {
                let r = RecipeId::by_id(u).unwrap_or_else(|| panic!("{} unlocks {u}", t.id));
                assert!(
                    !r.get().default,
                    "{u} is unlocked by default and by {}",
                    t.id
                );
            }
        }
    }

    #[test]
    fn every_locked_recipe_has_a_tech() {
        for r in RECIPES.iter().filter(|r| !r.default) {
            assert!(
                TECHS.iter().any(|t| t.unlocks.contains(&r.id)),
                "{} unreachable",
                r.id
            );
        }
    }

    #[test]
    fn every_crafted_item_has_one_craft_recipe() {
        for item in Item::ALL {
            let n = RECIPES
                .iter()
                .filter(|r| r.category == Category::Craft && r.main_output() == item)
                .count();
            assert!(n <= 1, "{item:?} has {n} craft recipes");
        }
    }

    #[test]
    fn recipe_id_serde_round_trip() {
        let id = RecipeId::by_id("chip").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        assert_eq!(s, "\"chip\"");
        let back: RecipeId = serde_json::from_str(&s).unwrap();
        assert_eq!(back, id);
    }
}
