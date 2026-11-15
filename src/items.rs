//! Items and the buildings they place.

use serde::{Deserialize, Serialize};

/// Most a battery bank charges or discharges at, in kW.
pub const BANK_RATE: f32 = 600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    // Raw resources
    FerriteOre,
    MalachiteOre,
    Lignite,
    Quartz,
    Water,
    // Smelted
    IronIngot,
    CopperIngot,
    Glass,
    Steel,
    Coke,
    // Intermediates
    Cog,
    Coil,
    Chip,
    Lens,
    Motor,
    Processor,
    RelayModule,
    Resin,
    Battery,
    SpeedModule,
    ProductivityModule,
    // Research
    AlphaPack,
    BetaPack,
    GammaPack,
    DeltaPack,
    // Tools
    Landfill,
    // Buildings
    Digger,
    Bore,
    Kiln,
    ArcFurnace,
    Conveyor,
    ExpressConveyor,
    Splitter,
    Tunnel,
    Arm,
    SwiftArm,
    LongArm,
    FilterArm,
    Crate,
    Generator,
    Pylon,
    SunPanel,
    Lab,
    Fabricator,
    Uplink,
    Pump,
    SteamTurbine,
    Sifter,
    ChemPlant,
    BatteryBank,
}

impl Item {
    pub const ALL: [Item; 50] = [
        Item::FerriteOre,
        Item::MalachiteOre,
        Item::Lignite,
        Item::Quartz,
        Item::Water,
        Item::IronIngot,
        Item::CopperIngot,
        Item::Glass,
        Item::Steel,
        Item::Coke,
        Item::Cog,
        Item::Coil,
        Item::Chip,
        Item::Lens,
        Item::Motor,
        Item::Processor,
        Item::RelayModule,
        Item::Resin,
        Item::Battery,
        Item::SpeedModule,
        Item::ProductivityModule,
        Item::AlphaPack,
        Item::BetaPack,
        Item::GammaPack,
        Item::DeltaPack,
        Item::Landfill,
        Item::Digger,
        Item::Bore,
        Item::Kiln,
        Item::ArcFurnace,
        Item::Conveyor,
        Item::ExpressConveyor,
        Item::Splitter,
        Item::Tunnel,
        Item::Arm,
        Item::SwiftArm,
        Item::LongArm,
        Item::FilterArm,
        Item::Crate,
        Item::Generator,
        Item::Pylon,
        Item::SunPanel,
        Item::Lab,
        Item::Fabricator,
        Item::Uplink,
        Item::Pump,
        Item::SteamTurbine,
        Item::Sifter,
        Item::ChemPlant,
        Item::BatteryBank,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Item::FerriteOre => "Ferrite ore",
            Item::MalachiteOre => "Malachite ore",
            Item::Lignite => "Lignite",
            Item::Quartz => "Quartz",
            Item::Water => "Water",
            Item::Coke => "Coke",
            Item::Resin => "Resin",
            Item::Battery => "Battery",
            Item::SpeedModule => "Speed module",
            Item::ProductivityModule => "Productivity module",
            Item::DeltaPack => "Delta datapack",
            Item::Landfill => "Landfill",
            Item::Pump => "Pump",
            Item::SteamTurbine => "Steam turbine",
            Item::Sifter => "Sifter",
            Item::ChemPlant => "Chem plant",
            Item::BatteryBank => "Battery bank",
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
            Item::Splitter => "Splitter",
            Item::Tunnel => "Tunnel belt",
            Item::Arm => "Arm",
            Item::SwiftArm => "Swift arm",
            Item::LongArm => "Long arm",
            Item::FilterArm => "Filter arm",
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
            Item::DeltaPack => '4',
            Item::Water => 'w',
            Item::Coke => 'k',
            Item::Resin => 'r',
            Item::Battery => 'b',
            Item::SpeedModule => '>',
            Item::ProductivityModule => '$',
            Item::Landfill => '=',
            _ => '+',
        }
    }

    /// Energy released when burned, in kJ.
    pub fn fuel_value(self) -> Option<f32> {
        match self {
            Item::Lignite => Some(8000.0),
            Item::Coke => Some(20000.0),
            _ => None,
        }
    }

    pub fn is_fuel(self) -> bool {
        self.fuel_value().is_some()
    }

    pub fn is_pack(self) -> bool {
        matches!(
            self,
            Item::AlphaPack | Item::BetaPack | Item::GammaPack | Item::DeltaPack
        )
    }

    pub fn is_module(self) -> bool {
        matches!(self, Item::SpeedModule | Item::ProductivityModule)
    }

    /// Items that can be put down on the map: buildings, plus landfill.
    pub fn placeable() -> impl Iterator<Item = Item> {
        EntityKind::menu()
            .map(EntityKind::item)
            .chain(std::iter::once(Item::Landfill))
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
    Splitter,
    /// Where a tunnel belt goes underground.
    TunnelIn,
    /// Where a tunnel belt comes back up.
    TunnelOut,
    Arm,
    SwiftArm,
    LongArm,
    FilterArm,
    Crate,
    Generator,
    Pylon,
    SunPanel,
    Lab,
    Fabricator,
    Uplink,
    /// Draws water from the tile behind it.
    Pump,
    SteamTurbine,
    /// Sifts quartz out of the sand beneath it.
    Sifter,
    ChemPlant,
    BatteryBank,
}

impl EntityKind {
    pub const ALL: [EntityKind; 25] = [
        EntityKind::Digger,
        EntityKind::Bore,
        EntityKind::Kiln,
        EntityKind::ArcFurnace,
        EntityKind::Conveyor,
        EntityKind::ExpressConveyor,
        EntityKind::Splitter,
        EntityKind::TunnelIn,
        EntityKind::TunnelOut,
        EntityKind::Arm,
        EntityKind::SwiftArm,
        EntityKind::LongArm,
        EntityKind::FilterArm,
        EntityKind::Crate,
        EntityKind::Generator,
        EntityKind::Pylon,
        EntityKind::SunPanel,
        EntityKind::Lab,
        EntityKind::Fabricator,
        EntityKind::Uplink,
        EntityKind::Pump,
        EntityKind::SteamTurbine,
        EntityKind::Sifter,
        EntityKind::ChemPlant,
        EntityKind::BatteryBank,
    ];

    pub fn item(self) -> Item {
        match self {
            EntityKind::Digger => Item::Digger,
            EntityKind::Bore => Item::Bore,
            EntityKind::Kiln => Item::Kiln,
            EntityKind::ArcFurnace => Item::ArcFurnace,
            EntityKind::Conveyor => Item::Conveyor,
            EntityKind::ExpressConveyor => Item::ExpressConveyor,
            EntityKind::Splitter => Item::Splitter,
            EntityKind::TunnelIn | EntityKind::TunnelOut => Item::Tunnel,
            EntityKind::Arm => Item::Arm,
            EntityKind::SwiftArm => Item::SwiftArm,
            EntityKind::LongArm => Item::LongArm,
            EntityKind::FilterArm => Item::FilterArm,
            EntityKind::Crate => Item::Crate,
            EntityKind::Generator => Item::Generator,
            EntityKind::Pylon => Item::Pylon,
            EntityKind::SunPanel => Item::SunPanel,
            EntityKind::Lab => Item::Lab,
            EntityKind::Fabricator => Item::Fabricator,
            EntityKind::Uplink => Item::Uplink,
            EntityKind::Pump => Item::Pump,
            EntityKind::SteamTurbine => Item::SteamTurbine,
            EntityKind::Sifter => Item::Sifter,
            EntityKind::ChemPlant => Item::ChemPlant,
            EntityKind::BatteryBank => Item::BatteryBank,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EntityKind::TunnelIn => "Tunnel entrance",
            EntityKind::TunnelOut => "Tunnel exit",
            _ => self.item().name(),
        }
    }

    /// The kinds a player can pick to build. A tunnel exit is placed with the same item as an
    /// entrance, so it isn't listed on its own.
    pub fn menu() -> impl Iterator<Item = EntityKind> {
        EntityKind::ALL
            .into_iter()
            .filter(|&k| k != EntityKind::TunnelOut)
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
            EntityKind::Splitter => "Split",
            EntityKind::TunnelIn | EntityKind::TunnelOut => "Tunnel",
            EntityKind::Arm => "Arm",
            EntityKind::SwiftArm => "SArm",
            EntityKind::LongArm => "LArm",
            EntityKind::FilterArm => "FArm",
            EntityKind::Crate => "Crate",
            EntityKind::Generator => "Gen",
            EntityKind::Pylon => "Pylon",
            EntityKind::SunPanel => "Solar",
            EntityKind::Lab => "Lab",
            EntityKind::Fabricator => "Fab",
            EntityKind::Uplink => "Uplink",
            EntityKind::Pump => "Pump",
            EntityKind::SteamTurbine => "Turb",
            EntityKind::Sifter => "Sift",
            EntityKind::ChemPlant => "Chem",
            EntityKind::BatteryBank => "Batt",
        }
    }

    /// Buildings in the same family can be swapped in place (upgrade or downgrade).
    pub fn family(self) -> Option<u8> {
        match self {
            EntityKind::Conveyor | EntityKind::ExpressConveyor => Some(0),
            EntityKind::Arm | EntityKind::SwiftArm | EntityKind::FilterArm => Some(1),
            EntityKind::Digger | EntityKind::Bore => Some(2),
            EntityKind::Kiln | EntityKind::ArcFurnace => Some(3),
            _ => None,
        }
    }

    /// A plain surface conveyor.
    pub fn is_belt(self) -> bool {
        matches!(self, EntityKind::Conveyor | EntityKind::ExpressConveyor)
    }

    pub fn is_tunnel(self) -> bool {
        matches!(self, EntityKind::TunnelIn | EntityKind::TunnelOut)
    }

    /// Anything items ride on: conveyors, splitters and tunnel belts.
    pub fn is_transport(self) -> bool {
        self.is_belt() || self.is_tunnel() || self == EntityKind::Splitter
    }

    pub fn is_arm(self) -> bool {
        matches!(
            self,
            EntityKind::Arm | EntityKind::SwiftArm | EntityKind::LongArm | EntityKind::FilterArm
        )
    }

    /// How many tiles away an arm picks up and drops.
    pub fn arm_reach(self) -> i32 {
        match self {
            EntityKind::LongArm => 2,
            _ => 1,
        }
    }

    pub fn is_miner(self) -> bool {
        matches!(self, EntityKind::Digger | EntityKind::Bore)
    }

    /// Buildings that work the 3x3 area beneath them and push what they get out ahead.
    pub fn is_extractor(self) -> bool {
        self.is_miner() || self == EntityKind::Sifter
    }

    /// Buildings that take speed and productivity modules, and how many.
    pub fn module_slots(self) -> usize {
        match self {
            EntityKind::Bore
            | EntityKind::Sifter
            | EntityKind::ArcFurnace
            | EntityKind::Fabricator
            | EntityKind::ChemPlant
            | EntityKind::Lab => 2,
            _ => 0,
        }
    }

    /// The recipe category an assembler of this kind runs.
    pub fn recipe_category(self) -> Option<crate::recipes::Category> {
        match self {
            EntityKind::Fabricator => Some(crate::recipes::Category::Craft),
            EntityKind::ChemPlant => Some(crate::recipes::Category::Chem),
            _ => None,
        }
    }

    /// Whether the building has a meaningful facing direction.
    pub fn directional(self) -> bool {
        self.is_transport() || self.is_arm() || self.is_extractor() || self == EntityKind::Pump
    }

    /// Belt movement in sub-tile units per tick.
    pub fn belt_speed(self) -> u8 {
        match self {
            EntityKind::ExpressConveyor
            | EntityKind::Splitter
            | EntityKind::TunnelIn
            | EntityKind::TunnelOut => 2,
            _ => 1,
        }
    }

    /// Fraction of a full swing completed per tick.
    pub fn arm_speed(self) -> f32 {
        match self {
            EntityKind::SwiftArm | EntityKind::FilterArm => 0.25,
            EntityKind::LongArm => 0.125,
            _ => 0.1,
        }
    }

    /// Electric draw in kW while working.
    pub fn power_draw(self) -> f32 {
        match self {
            EntityKind::Arm => 15.0,
            EntityKind::SwiftArm => 40.0,
            EntityKind::LongArm => 25.0,
            EntityKind::FilterArm => 45.0,
            EntityKind::Bore => 90.0,
            EntityKind::ArcFurnace => 180.0,
            EntityKind::Fabricator => 75.0,
            EntityKind::Lab => 60.0,
            EntityKind::Uplink => 600.0,
            EntityKind::Sifter => 90.0,
            EntityKind::ChemPlant => 120.0,
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
            EntityKind::SteamTurbine => 2400.0,
            EntityKind::SunPanel => 60.0,
            EntityKind::BatteryBank => BANK_RATE,
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
            EntityKind::Sifter => 1.0,
            EntityKind::ChemPlant => 1.0,
            _ => 1.0,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            EntityKind::Digger => "Burns lignite to mine the 3x3 area beneath it. Outputs ahead.",
            EntityKind::Bore => {
                "Electric miner for the 3x3 area beneath it. Twice the Digger's speed."
            }
            EntityKind::Kiln => "Burns lignite or coke to smelt ore into ingots and glass.",
            EntityKind::ArcFurnace => {
                "Electric smelter, twice as fast as a Kiln. No fuel needed. Bakes lignite into coke (Coking)."
            }
            EntityKind::Conveyor => "Carries items in the direction it faces.",
            EntityKind::ExpressConveyor => "A conveyor that moves twice as fast.",
            EntityKind::Arm => {
                "Grabs from the tile behind and drops onto the tile ahead. Needs power."
            }
            EntityKind::SwiftArm => "A faster arm. Needs more power.",
            EntityKind::LongArm => {
                "Reaches two tiles: grabs from two behind and drops two ahead, over whatever is between."
            }
            EntityKind::FilterArm => {
                "A swift arm that only moves the item you choose (inspect it with e, then f)."
            }
            EntityKind::Splitter => {
                "Shares incoming items evenly between the conveyors ahead and to its sides."
            }
            EntityKind::TunnelIn | EntityKind::TunnelOut => {
                "Carries items underground past up to 4 tiles. Place the entrance, then the exit further along."
            }
            EntityKind::Crate => "Stores up to 800 items.",
            EntityKind::Generator => {
                "Burns lignite or coke to power everything on its pylon network (900 kW)."
            }
            EntityKind::Pump => {
                "Place it with water right behind it. Pumps 2 water a second out ahead. No power needed."
            }
            EntityKind::SteamTurbine => {
                "Burns lignite or coke and boils water: 2400 kW, getting 50% more from its fuel than a generator. Uses 2 water a second at full load."
            }
            EntityKind::Sifter => {
                "Sifts quartz out of the sand under it (3x3, never runs out). Electric. Outputs ahead."
            }
            EntityKind::ChemPlant => {
                "Electric plant for chemistry recipes: resin and batteries. Choose a recipe and feed it."
            }
            EntityKind::BatteryBank => {
                "Stores 20 MJ from spare power on its network and gives it back when demand outruns supply (up to 600 kW)."
            }
            EntityKind::Pylon => {
                "Powers machines within 2 tiles and links to pylons up to 7 tiles away."
            }
            EntityKind::SunPanel => {
                "Free power: 60 kW in full daylight, less at dawn and dusk, none at night."
            }
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

    pub fn ccw(self) -> Dir {
        self.cw().cw().cw()
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
        for k in EntityKind::menu() {
            assert_eq!(k.item().building(), Some(k));
        }
        assert_eq!(
            EntityKind::TunnelOut.item().building(),
            Some(EntityKind::TunnelIn)
        );
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
