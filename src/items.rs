//! Items and the buildings they place.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    // Raw resources
    FerriteOre,
    MalachiteOre,
    Lignite,
    Quartz,
    // Smelted
    IronIngot,
    CopperIngot,
    Glass,
    Steel,
    // Intermediates
    Cog,
    Coil,
    Chip,
    Lens,
    Motor,
    Processor,
    RelayModule,
    // Research
    AlphaPack,
    BetaPack,
    GammaPack,
    // Buildings
    Digger,
    Bore,
    Kiln,
    ArcFurnace,
    Conveyor,
    ExpressConveyor,
    Arm,
    SwiftArm,
    Crate,
    Generator,
    Pylon,
    SunPanel,
    Lab,
    Fabricator,
    Uplink,
}

impl Item {
    #[cfg(test)]
    pub const ALL: [Item; 33] = [
        Item::FerriteOre,
        Item::MalachiteOre,
        Item::Lignite,
        Item::Quartz,
        Item::IronIngot,
        Item::CopperIngot,
        Item::Glass,
        Item::Steel,
        Item::Cog,
        Item::Coil,
        Item::Chip,
        Item::Lens,
        Item::Motor,
        Item::Processor,
        Item::RelayModule,
        Item::AlphaPack,
        Item::BetaPack,
        Item::GammaPack,
        Item::Digger,
        Item::Bore,
        Item::Kiln,
        Item::ArcFurnace,
        Item::Conveyor,
        Item::ExpressConveyor,
        Item::Arm,
        Item::SwiftArm,
        Item::Crate,
        Item::Generator,
        Item::Pylon,
        Item::SunPanel,
        Item::Lab,
        Item::Fabricator,
        Item::Uplink,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Item::FerriteOre => "Ferrite ore",
            Item::MalachiteOre => "Malachite ore",
            Item::Lignite => "Lignite",
            Item::Quartz => "Quartz",
            Item::IronIngot => "Iron ingot",
            Item::CopperIngot => "Copper ingot",
            Item::Glass => "Glass",
            Item::Steel => "Steel beam",
            Item::Cog => "Cog",
            Item::Coil => "Copper coil",
            Item::Chip => "Logic chip",
            Item::Lens => "Lens",
            Item::Motor => "Motor",
            Item::Processor => "Processor",
            Item::RelayModule => "Relay module",
            Item::AlphaPack => "Alpha datapack",
            Item::BetaPack => "Beta datapack",
            Item::GammaPack => "Gamma datapack",
            Item::Digger => "Digger",
            Item::Bore => "Electric bore",
            Item::Kiln => "Kiln",
            Item::ArcFurnace => "Arc furnace",
            Item::Conveyor => "Conveyor",
            Item::ExpressConveyor => "Express conveyor",
            Item::Arm => "Arm",
            Item::SwiftArm => "Swift arm",
            Item::Crate => "Crate",
            Item::Generator => "Generator",
            Item::Pylon => "Pylon",
            Item::SunPanel => "Sun panel",
            Item::Lab => "Lab",
            Item::Fabricator => "Fabricator",
            Item::Uplink => "Uplink array",
        }
    }

    /// Single character used when the item is drawn on a conveyor or in an arm.
    pub fn glyph(self) -> char {
        match self {
            Item::FerriteOre => 'f',
            Item::MalachiteOre => 'm',
            Item::Lignite => 'l',
            Item::Quartz => 'q',
            Item::IronIngot => 'i',
            Item::CopperIngot => 'c',
            Item::Glass => 'g',
            Item::Steel => 's',
            Item::Cog => '*',
            Item::Coil => '~',
            Item::Chip => '#',
            Item::Lens => 'o',
            Item::Motor => '&',
            Item::Processor => '%',
            Item::RelayModule => '@',
            Item::AlphaPack => '1',
            Item::BetaPack => '2',
            Item::GammaPack => '3',
            _ => '+',
        }
    }

    /// Energy released when burned, in kJ.
    pub fn fuel_value(self) -> Option<f32> {
        match self {
            Item::Lignite => Some(8000.0),
            _ => None,
        }
    }

    pub fn is_fuel(self) -> bool {
        self.fuel_value().is_some()
    }

    pub fn is_pack(self) -> bool {
        matches!(self, Item::AlphaPack | Item::BetaPack | Item::GammaPack)
    }

    pub fn building(self) -> Option<EntityKind> {
        EntityKind::ALL.iter().copied().find(|k| k.item() == self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntityKind {
    Digger,
    Bore,
    Kiln,
    ArcFurnace,
    Conveyor,
    ExpressConveyor,
    Arm,
    SwiftArm,
    Crate,
    Generator,
    Pylon,
    SunPanel,
    Lab,
    Fabricator,
    Uplink,
}

impl EntityKind {
    pub const ALL: [EntityKind; 15] = [
        EntityKind::Digger,
        EntityKind::Bore,
        EntityKind::Kiln,
        EntityKind::ArcFurnace,
        EntityKind::Conveyor,
        EntityKind::ExpressConveyor,
        EntityKind::Arm,
        EntityKind::SwiftArm,
        EntityKind::Crate,
        EntityKind::Generator,
        EntityKind::Pylon,
        EntityKind::SunPanel,
        EntityKind::Lab,
        EntityKind::Fabricator,
        EntityKind::Uplink,
    ];

    pub fn item(self) -> Item {
        match self {
            EntityKind::Digger => Item::Digger,
            EntityKind::Bore => Item::Bore,
            EntityKind::Kiln => Item::Kiln,
            EntityKind::ArcFurnace => Item::ArcFurnace,
            EntityKind::Conveyor => Item::Conveyor,
            EntityKind::ExpressConveyor => Item::ExpressConveyor,
            EntityKind::Arm => Item::Arm,
            EntityKind::SwiftArm => Item::SwiftArm,
            EntityKind::Crate => Item::Crate,
            EntityKind::Generator => Item::Generator,
            EntityKind::Pylon => Item::Pylon,
            EntityKind::SunPanel => Item::SunPanel,
            EntityKind::Lab => Item::Lab,
            EntityKind::Fabricator => Item::Fabricator,
            EntityKind::Uplink => Item::Uplink,
        }
    }

    pub fn name(self) -> &'static str {
        self.item().name()
    }

    /// Compact name for the hotbar.
    pub fn short_name(self) -> &'static str {
        match self {
            EntityKind::Digger => "Digger",
            EntityKind::Bore => "Bore",
            EntityKind::Kiln => "Kiln",
            EntityKind::ArcFurnace => "Arc",
            EntityKind::Conveyor => "Belt",
            EntityKind::ExpressConveyor => "XBelt",
            EntityKind::Arm => "Arm",
            EntityKind::SwiftArm => "SArm",
            EntityKind::Crate => "Crate",
            EntityKind::Generator => "Gen",
            EntityKind::Pylon => "Pylon",
            EntityKind::SunPanel => "Solar",
            EntityKind::Lab => "Lab",
            EntityKind::Fabricator => "Fab",
            EntityKind::Uplink => "Uplink",
        }
    }

    /// Buildings in the same family can be swapped in place (upgrade or downgrade).
    pub fn family(self) -> Option<u8> {
        match self {
            EntityKind::Conveyor | EntityKind::ExpressConveyor => Some(0),
            EntityKind::Arm | EntityKind::SwiftArm => Some(1),
            EntityKind::Digger | EntityKind::Bore => Some(2),
            EntityKind::Kiln | EntityKind::ArcFurnace => Some(3),
            _ => None,
        }
    }

    pub fn is_belt(self) -> bool {
        matches!(self, EntityKind::Conveyor | EntityKind::ExpressConveyor)
    }

    pub fn is_arm(self) -> bool {
        matches!(self, EntityKind::Arm | EntityKind::SwiftArm)
    }

    pub fn is_miner(self) -> bool {
        matches!(self, EntityKind::Digger | EntityKind::Bore)
    }

    /// Whether the building has a meaningful facing direction.
    pub fn directional(self) -> bool {
        self.is_belt() || self.is_arm() || self.is_miner()
    }

    /// Belt movement in sub-tile units per tick.
    pub fn belt_speed(self) -> u8 {
        match self {
            EntityKind::ExpressConveyor => 2,
            _ => 1,
        }
    }

    /// Fraction of a full swing completed per tick.
    pub fn arm_speed(self) -> f32 {
        match self {
            EntityKind::SwiftArm => 0.25,
            _ => 0.1,
        }
    }

    /// Electric draw in kW while working.
    pub fn power_draw(self) -> f32 {
        match self {
            EntityKind::Arm => 15.0,
            EntityKind::SwiftArm => 40.0,
            EntityKind::Bore => 90.0,
            EntityKind::ArcFurnace => 180.0,
            EntityKind::Fabricator => 75.0,
            EntityKind::Lab => 60.0,
            EntityKind::Uplink => 600.0,
            _ => 0.0,
        }
    }

    pub fn uses_power(self) -> bool {
        self.power_draw() > 0.0
    }

    /// Fuel burn rate in kW while working (burner buildings).
    pub fn fuel_draw(self) -> f32 {
        match self {
            EntityKind::Digger => 150.0,
            EntityKind::Kiln => 90.0,
            _ => 0.0,
        }
    }

    /// Maximum electric output in kW.
    pub fn power_output(self) -> f32 {
        match self {
            EntityKind::Generator => 900.0,
            EntityKind::SunPanel => 60.0,
            _ => 0.0,
        }
    }

    pub fn is_power_node(self) -> bool {
        self.uses_power() || self.power_output() > 0.0
    }

    /// Crafting / mining speed multiplier.
    pub fn speed(self) -> f32 {
        match self {
            EntityKind::Digger => 0.5,
            EntityKind::Bore => 1.0,
            EntityKind::Kiln => 1.0,
            EntityKind::ArcFurnace => 2.0,
            EntityKind::Fabricator => 1.0,
            EntityKind::Lab => 1.0,
            _ => 1.0,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            EntityKind::Digger => "Burns lignite to mine the 3x3 area beneath it. Outputs ahead.",
            EntityKind::Bore => {
                "Electric miner for the 3x3 area beneath it. Twice the Digger's speed."
            }
            EntityKind::Kiln => "Burns lignite to smelt ore into ingots and glass.",
            EntityKind::ArcFurnace => "Electric smelter, twice as fast as a Kiln. No fuel needed.",
            EntityKind::Conveyor => "Carries items in the direction it faces.",
            EntityKind::ExpressConveyor => "A conveyor that moves twice as fast.",
            EntityKind::Arm => {
                "Grabs from the tile behind and drops onto the tile ahead. Needs power."
            }
            EntityKind::SwiftArm => "A faster arm. Needs more power.",
            EntityKind::Crate => "Stores up to 800 items.",
            EntityKind::Generator => {
                "Burns lignite to power everything on its pylon network (900 kW)."
            }
            EntityKind::Pylon => {
                "Powers machines within 2 tiles and links to pylons up to 7 tiles away."
            }
            EntityKind::SunPanel => "Free, steady 60 kW of power.",
            EntityKind::Lab => "Consumes datapacks to advance the current research.",
            EntityKind::Fabricator => {
                "Electric assembler. Choose a recipe and feed it ingredients."
            }
            EntityKind::Uplink => "Feed it 20 relay modules with power to transmit and win.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Dir {
    #[default]
    North,
    East,
    South,
    West,
}

impl Dir {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir::North => (0, -1),
            Dir::East => (1, 0),
            Dir::South => (0, 1),
            Dir::West => (-1, 0),
        }
    }

    pub fn cw(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
        }
    }

    pub fn opposite(self) -> Dir {
        self.cw().cw()
    }

    pub fn arrow(self, ascii: bool) -> char {
        match (self, ascii) {
            (Dir::North, false) => '↑',
            (Dir::East, false) => '→',
            (Dir::South, false) => '↓',
            (Dir::West, false) => '←',
            (Dir::North, true) => '^',
            (Dir::East, true) => '>',
            (Dir::South, true) => 'v',
            (Dir::West, true) => '<',
        }
    }

    pub fn from_delta(dx: i32, dy: i32) -> Option<Dir> {
        match (dx.signum(), dy.signum()) {
            (0, -1) => Some(Dir::North),
            (1, 0) => Some(Dir::East),
            (0, 1) => Some(Dir::South),
            (-1, 0) => Some(Dir::West),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_items_round_trip() {
        for k in EntityKind::ALL {
            assert_eq!(k.item().building(), Some(k));
        }
        assert_eq!(Item::Cog.building(), None);
    }

    #[test]
    fn all_items_listed_once() {
        let mut v = Item::ALL.to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), Item::ALL.len());
    }

    #[test]
    fn dir_rotation() {
        assert_eq!(Dir::North.cw().cw().cw().cw(), Dir::North);
        assert_eq!(Dir::East.opposite(), Dir::West);
        assert_eq!(Dir::from_delta(0, 3), Some(Dir::South));
    }
}
