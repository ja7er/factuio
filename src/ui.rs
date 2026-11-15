//! Rendering with ratatui.

use crate::app::{App, EntitySub, MENU_ITEMS, Modal};
use crate::game::Game;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::TechId;
use crate::world::{
    ArmPhase, CRATE_CAP, Entity, PYLON_SUPPLY, State, Status, TUNNEL_REACH, Terrain, UPLINK_GOAL,
};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget, Wrap};
use std::sync::atomic::{AtomicBool, Ordering};

static TRUECOLOR: AtomicBool = AtomicBool::new(true);

pub fn detect_color_support() {
    let ct = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_lowercase();
    let term_prog = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let truecolor = ct.contains("truecolor")
        || ct.contains("24bit")
        || (term_prog != "Apple_Terminal" && ct.is_empty() && std::env::var("WT_SESSION").is_ok());
    TRUECOLOR.store(truecolor, Ordering::Relaxed);
}

pub fn set_truecolor(on: bool) {
    TRUECOLOR.store(on, Ordering::Relaxed);
}

type Rgb = (u8, u8, u8);

fn col((r, g, b): Rgb) -> Color {
    if TRUECOLOR.load(Ordering::Relaxed) {
        Color::Rgb(r, g, b)
    } else {
        // The xterm 256-colour cube, with its greyscale ramp for colours the cube would turn
        // black (grass, fog, night) and for near-greys. Tinted colours stay in the cube so
        // ore patches keep their hue.
        let q = |v: u8| -> u8 {
            if v < 48 {
                0
            } else if v < 115 {
                1
            } else {
                (v - 35) / 40
            }
        };
        const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
        let dist = |c: (i32, i32, i32)| {
            (c.0 - r as i32).pow(2) + (c.1 - g as i32).pow(2) + (c.2 - b as i32).pow(2)
        };
        let (qr, qg, qb) = (q(r), q(g), q(b));
        let cube = (
            LEVELS[qr as usize],
            LEVELS[qg as usize],
            LEVELS[qb as usize],
        );
        let avg = (r as i32 + g as i32 + b as i32) / 3;
        let k = ((avg - 3) / 10).clamp(0, 23);
        let grey = 8 + 10 * k;
        let spread = r.max(g).max(b) - r.min(g).min(b);
        let dark = qr + qg + qb == 0;
        if (dark || spread <= 12) && dist((grey, grey, grey)) < dist(cube) {
            Color::Indexed(232 + k as u8)
        } else {
            Color::Indexed(16 + 36 * qr + 6 * qg + qb)
        }
    }
}

fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

pub fn item_rgb(item: Item) -> Rgb {
    match item {
        Item::FerriteOre => (150, 170, 210),
        Item::MalachiteOre => (80, 210, 140),
        Item::Lignite => (150, 140, 130),
        Item::Quartz => (235, 225, 190),
        Item::IronIngot => (200, 210, 230),
        Item::CopperIngot => (235, 140, 80),
        Item::Glass => (170, 230, 240),
        Item::Steel => (160, 170, 190),
        Item::Cog => (210, 210, 150),
        Item::Coil => (240, 170, 90),
        Item::Chip => (110, 230, 110),
        Item::Lens => (140, 200, 255),
        Item::Motor => (230, 120, 120),
        Item::Processor => (200, 130, 255),
        Item::RelayModule => (255, 230, 90),
        Item::AlphaPack => (255, 110, 110),
        Item::BetaPack => (120, 255, 120),
        Item::GammaPack => (120, 170, 255),
        _ => (230, 230, 230),
    }
}

fn ore_bg(item: Item) -> Rgb {
    match item {
        Item::FerriteOre => (44, 52, 78),
        Item::MalachiteOre => (26, 70, 56),
        Item::Lignite => (34, 30, 28),
        Item::Quartz => (84, 78, 60),
        _ => (40, 40, 40),
    }
}

fn terrain_bg(t: Terrain) -> Rgb {
    match t {
        Terrain::Grass => (26, 38, 24),
        Terrain::Sand => (66, 58, 38),
        Terrain::Water => (18, 36, 84),
    }
}

fn entity_bg(k: EntityKind) -> Rgb {
    match k {
        EntityKind::Conveyor => (58, 58, 54),
        EntityKind::ExpressConveyor => (72, 52, 92),
        EntityKind::Splitter => (70, 76, 40),
        EntityKind::TunnelIn | EntityKind::TunnelOut => (34, 66, 72),
        EntityKind::Digger => (96, 72, 30),
        EntityKind::Bore => (110, 90, 20),
        EntityKind::Kiln => (104, 44, 24),
        EntityKind::ArcFurnace => (110, 36, 84),
        EntityKind::Fabricator => (30, 60, 110),
        EntityKind::Crate => (92, 70, 48),
        EntityKind::Generator => (90, 28, 28),
        EntityKind::Pylon => (60, 60, 40),
        EntityKind::SunPanel => (24, 36, 100),
        EntityKind::Lab => (30, 92, 92),
        EntityKind::Uplink => (120, 120, 150),
        EntityKind::Pump => (36, 64, 112),
        EntityKind::SteamTurbine => (112, 64, 40),
        EntityKind::Sifter => (104, 92, 52),
        EntityKind::ChemPlant => (34, 96, 70),
        EntityKind::BatteryBank => (64, 84, 36),
        EntityKind::Rail => (48, 42, 36),
        EntityKind::Depot => (72, 56, 96),
        EntityKind::SurveyMast => (36, 84, 76),
        EntityKind::Arm | EntityKind::SwiftArm | EntityKind::LongArm | EntityKind::FilterArm => {
            (40, 40, 40)
        }
    }
}

fn arm_rgb(k: EntityKind) -> Rgb {
    match k {
        EntityKind::SwiftArm => (90, 220, 240),
        EntityKind::LongArm => (255, 150, 70),
        EntityKind::FilterArm => (220, 140, 255),
        _ => (240, 210, 80),
    }
}

fn status_rgb(s: Status) -> Rgb {
    match s {
        Status::Working => (250, 250, 250),
        Status::LowPower => (250, 220, 80),
        s if s.is_problem() => (255, 90, 90),
        _ => (170, 170, 170),
    }
}

fn status_color(s: Status) -> Color {
    match s {
        Status::Working => col((120, 230, 120)),
        Status::LowPower | Status::NoInput => col((240, 210, 90)),
        s if s.is_problem() => col((255, 100, 100)),
        _ => col((170, 170, 170)),
    }
}

type Cell2 = [(char, Rgb, Rgb, bool); 2];

const RAIL_FG: Rgb = (190, 170, 140);
const CART_RGB: Rgb = (255, 200, 90);

/// A rail tile joined to the track on the sides set in `mask` (N=1, E=2, S=4, W=8).
/// The left cell carries the bend or junction and the right one runs on east.
fn rail_cells(mask: u8, ascii: bool, bg: Rgb) -> Cell2 {
    let (n, e, s, w) = (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0, mask & 8 != 0);
    let vertical = n || s;
    let horizontal = e || w;
    let left = if ascii {
        match (vertical, horizontal) {
            (true, true) => '+',
            (true, false) => '|',
            _ => '=',
        }
    } else {
        match (n, e, s, w) {
            (true, true, true, true) => '╬',
            (true, true, true, false) => '╠',
            (true, false, true, true) => '╣',
            (false, true, true, true) => '╦',
            (true, true, false, true) => '╩',
            (true, true, false, false) => '╚',
            (true, false, false, true) => '╝',
            (false, true, true, false) => '╔',
            (false, false, true, true) => '╗',
            _ if vertical && !horizontal => '║',
            _ => '═',
        }
    };
    let right = if e || !vertical {
        if ascii { '=' } else { '═' }
    } else {
        ' '
    };
    [(left, RAIL_FG, bg, false), (right, RAIL_FG, bg, false)]
}

/// A cart drawn over its tile: the cart and what it carries.
fn cart_cells(c: &crate::world::Cart, ascii: bool, bg: Rgb) -> Cell2 {
    let body = (if ascii { 'O' } else { '■' }, CART_RGB, bg, true);
    let load = match c.cargo.first() {
        Some(i) => (i.glyph(), item_rgb(i), bg, true),
        None => (if ascii { '.' } else { '·' }, (150, 150, 150), bg, false),
    };
    [body, load]
}

/// The two characters used to draw an entity, with fg/bg colours and bold flag.
/// `transmitted` is the world's Uplink progress, drawn on Uplink arrays; `tick` animates.
fn entity_cells(e: &Entity, ascii: bool, bg_under: Rgb, transmitted: u32, tick: u64) -> Cell2 {
    let bg = if e.kind.is_arm() {
        bg_under
    } else {
        entity_bg(e.kind)
    };
    let st = status_rgb(e.status);
    let dim = (130, 130, 130);
    let dot = if ascii { '.' } else { '·' };
    let letter = |c: char| (c, st, bg, true);
    match &e.state {
        State::Belt { items, .. } => {
            let arrow = e.dir.arrow(ascii);
            let mut out = [(arrow, dim, bg, false), (arrow, dim, bg, false)];
            // Display order follows the belt direction: `slot(0)` is the back half.
            let slot = |half: usize| match e.dir {
                Dir::East | Dir::South => half,
                Dir::West | Dir::North => 1 - half,
            };
            // Each special piece keeps one half for its own mark and carries items in the other.
            let mark_fg = if e.status.is_problem() {
                (255, 90, 90)
            } else {
                (235, 225, 170)
            };
            let (mark, shown) = match e.kind {
                EntityKind::Splitter => (Some((0, 'Y')), Some(1)),
                EntityKind::TunnelIn => (Some((1, if ascii { 'u' } else { '∪' })), Some(0)),
                EntityKind::TunnelOut => (Some((0, if ascii { 'n' } else { '∩' })), Some(1)),
                _ => (None, None),
            };
            for b in items {
                // What's underground in a tunnel entrance isn't drawn.
                if b.pos > crate::world::TILE_LEN {
                    continue;
                }
                let half = shown.unwrap_or(if b.pos < crate::world::TILE_LEN / 2 {
                    0
                } else {
                    1
                });
                out[slot(half)] = (b.item.glyph(), item_rgb(b.item), bg, true);
            }
            if let Some((half, c)) = mark {
                out[slot(half)] = (c, mark_fg, bg, true);
            }
            out
        }
        State::Arm { hand, .. } => {
            let base = arm_rgb(e.kind);
            let fg = if e.status.is_problem() {
                (255, 90, 90)
            } else {
                base
            };
            let first = match hand {
                Some(i) => (i.glyph(), item_rgb(*i), bg, true),
                None => (if ascii { 'a' } else { 'ɑ' }, fg, bg, true),
            };
            [first, (e.dir.arrow(ascii), fg, bg, true)]
        }
        State::Miner { .. } => [
            letter(match e.kind {
                EntityKind::Digger => 'D',
                EntityKind::Sifter => 'Q',
                EntityKind::Pump => 'W',
                _ => 'B',
            }),
            (e.dir.arrow(ascii), (230, 230, 200), bg, true),
        ],
        State::Furnace { job, output, .. } => {
            let second = match (job, output.first()) {
                (Some(j), _) => (
                    j.get().main_output().glyph(),
                    item_rgb(j.get().main_output()),
                    bg,
                    true,
                ),
                (None, Some(o)) => (o.glyph(), item_rgb(o), bg, false),
                _ => (dot, dim, bg, false),
            };
            [
                letter(if e.kind == EntityKind::Kiln { 'K' } else { 'E' }),
                second,
            ]
        }
        State::Assembler { recipe, .. } => {
            let second = match recipe {
                Some(r) => (
                    r.get().main_output().glyph(),
                    item_rgb(r.get().main_output()),
                    bg,
                    true,
                ),
                None => ('?', (255, 120, 120), bg, true),
            };
            [
                letter(if e.kind == EntityKind::ChemPlant {
                    'H'
                } else {
                    'M'
                }),
                second,
            ]
        }
        State::Chest { unload, .. } if e.kind == EntityKind::Depot => {
            let (c, fg) = if *unload {
                ('-', (255, 170, 110))
            } else {
                ('+', (140, 230, 140))
            };
            [('R', (230, 210, 255), bg, true), (c, fg, bg, true)]
        }
        State::Chest { inv, .. } => {
            let second = match inv.first() {
                Some(i) => (i.glyph(), item_rgb(i), bg, false),
                None => (dot, dim, bg, false),
            };
            [('C', (240, 220, 180), bg, true), second]
        }
        State::Rail => rail_cells(0, ascii, bg),
        State::Survey { .. } => {
            let spin = if ascii {
                ['|', '/', '-', '\\']
            } else {
                ['◜', '◝', '◞', '◟']
            };
            let second = if e.status == Status::Working {
                (spin[(tick / 5 % 4) as usize], (140, 255, 220), bg, true)
            } else {
                (dot, dim, bg, false)
            };
            [letter('V'), second]
        }
        State::Generator { .. } => {
            let s = match e.status {
                Status::Working => '~',
                Status::NoFuel | Status::NoWater => '!',
                _ => dot,
            };
            let l = if e.kind == EntityKind::SteamTurbine {
                'T'
            } else {
                'G'
            };
            [letter(l), (s, st, bg, true)]
        }
        State::Battery { charge } => {
            let f = (charge / crate::world::BANK_CAPACITY).clamp(0.0, 1.0);
            let c = if ascii {
                char::from_digit((f * 9.0).round() as u32, 10).unwrap_or('0')
            } else {
                ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][((f * 7.0).round() as usize).min(7)]
            };
            [letter('A'), (c, (170, 240, 110), bg, true)]
        }
        State::Pylon => [
            ('P', (230, 220, 140), bg, true),
            ('+', (230, 220, 140), bg, false),
        ],
        State::Solar => [
            ('S', (180, 200, 255), bg, true),
            ('*', (255, 240, 120), bg, true),
        ],
        State::Lab { working, .. } => {
            let second = if *working {
                ('o', (120, 255, 200), bg, true)
            } else {
                (dot, dim, bg, false)
            };
            [letter('L'), second]
        }
        State::Uplink { .. } => {
            let c = if transmitted >= UPLINK_GOAL {
                '!'
            } else {
                char::from_digit(transmitted * 10 / UPLINK_GOAL, 10).unwrap_or('0')
            };
            [letter('U'), (c, (255, 240, 120), bg, true)]
        }
    }
}

fn ghost_entity(kind: EntityKind, dir: Dir) -> Entity {
    let mut e = Entity::new(kind, dir);
    e.status = Status::Idle;
    e
}

struct MapView<'a> {
    app: &'a App,
    game: &'a Game,
}

impl MapView<'_> {
    /// The view scrolls only when the cursor gets near an edge, so clicking around doesn't
    /// jerk the map. Remembers the result (and the map's screen area) for mouse input.
    fn camera(&self, area: Rect) -> (i32, i32, i32, i32) {
        let vw = (area.width / 2) as i32;
        let vh = area.height as i32;
        let w = &self.game.world;
        let (cx, cy) = self.game.player.cursor;
        let prev = self.app.camera.get();
        let axis = |prev: Option<i32>, c: i32, view: i32, size: i32| -> i32 {
            if size <= view {
                return -(view - size) / 2;
            }
            let margin = (view / 4).min(6);
            let mut cam = prev.unwrap_or(c - view / 2);
            if c < cam + margin {
                cam = c - margin;
            }
            if c > cam + view - 1 - margin {
                cam = c - (view - 1 - margin);
            }
            cam.clamp(0, size - view)
        };
        let cam_x = axis(prev.map(|p| p.0), cx, vw, w.width);
        let cam_y = axis(prev.map(|p| p.1), cy, vh, w.height);
        self.app.camera.set(Some((cam_x, cam_y)));
        self.app.map_rect.set(area);
        (cam_x, cam_y, vw, vh)
    }
}

impl Widget for MapView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (cam_x, cam_y, vw, vh) = self.camera(area);
        let world = &self.game.world;
        let ascii = self.app.ascii;
        let (cx, cy) = self.game.player.cursor;
        let selected_kind = self.app.selected.and_then(|i| i.building());

        // What to highlight around the cursor.
        let hover_kind = world.entity(cx, cy).map(|e| (e.kind, e.dir));
        let focus = selected_kind
            .map(|k| (k, self.app.place_dir))
            .or(hover_kind);
        // The other end of the tunnel under the cursor, or the entrance a new piece would join.
        let tunnel_mate = match (selected_kind, hover_kind) {
            (Some(k), _) if k.is_tunnel() => {
                world.tunnel_entrance_behind(cx, cy, self.app.place_dir)
            }
            (None, Some((k, _))) if k.is_tunnel() => {
                world.idx(cx, cy).and_then(|i| world.tunnel_pair(i))
            }
            _ => None,
        }
        .map(|i| world.pos(i));
        let pylons = if self.power_overlay_active(focus) {
            world.pylons()
        } else {
            vec![]
        };
        let night = (1.0 - world.daylight()) * 0.45;
        // Carts by the tile they're drawn on (the newest wins where several share one).
        let carts: std::collections::HashMap<usize, &crate::world::Cart> =
            world.carts.iter().map(|c| (c.shown_at(), c)).collect();
        let area_sel = self
            .app
            .area_anchor
            .map(|a| crate::tools::rect(a, (cx, cy)));

        for sy in 0..vh {
            for sx in 0..vw {
                let (x, y) = (cam_x + sx, cam_y + sy);
                let px = area.x + (sx * 2) as u16;
                let py = area.y + sy as u16;
                let Some(tile) = world.tile(x, y) else {
                    for k in 0..2 {
                        if let Some(c) = buf.cell_mut((px + k, py)) {
                            c.set_char(' ').set_bg(Color::Black);
                        }
                    }
                    continue;
                };
                let explored = world.is_explored(x, y);
                let mut bg = tile
                    .ore
                    .map(ore_bg)
                    .unwrap_or_else(|| terrain_bg(tile.terrain));
                let mut cells: Cell2 = match (tile.terrain, tile.ore) {
                    (Terrain::Water, _) => {
                        let c = if (x + y + (world.tick / 20) as i32) % 5 == 0 {
                            '~'
                        } else {
                            ' '
                        };
                        [
                            (c, (70, 110, 200), bg, false),
                            (' ', (70, 110, 200), bg, false),
                        ]
                    }
                    (_, Some(ore)) => {
                        let (a, b) = if tile.amount > 600 {
                            (':', ':')
                        } else if tile.amount > 200 {
                            (':', '.')
                        } else {
                            ('.', ' ')
                        };
                        let fg = blend(item_rgb(ore), bg, 0.35);
                        [(a, fg, bg, false), (b, fg, bg, false)]
                    }
                    (Terrain::Sand, None) => [(' ', bg, bg, false), (' ', bg, bg, false)],
                    (Terrain::Grass, None) => {
                        let h = (x as u32).wrapping_mul(0x9E37_79B1)
                            ^ (y as u32).wrapping_mul(0x85EB_CA77);
                        let speck = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D) >> 28 == 0;
                        let c = if speck {
                            if ascii { '.' } else { '\'' }
                        } else {
                            ' '
                        };
                        [(c, (50, 72, 44), bg, false), (' ', bg, bg, false)]
                    }
                };

                if let Some(e) = world.entity(x, y) {
                    cells = entity_cells(e, ascii, bg, world.transmitted, world.tick);
                    if e.kind == EntityKind::Rail {
                        let mut mask = 0;
                        for (bit, (dx, dy)) in
                            [(1, (0, -1)), (2, (1, 0)), (4, (0, 1)), (8, (-1, 0))]
                        {
                            if world
                                .entity(x + dx, y + dy)
                                .is_some_and(|n| n.kind.is_track())
                            {
                                mask |= bit;
                            }
                        }
                        cells = rail_cells(mask, ascii, cells[0].2);
                    }
                    bg = cells[0].2;
                }
                if let Some(c) = world.idx(x, y).and_then(|i| carts.get(&i)) {
                    cells = cart_cells(c, ascii, bg);
                }
                if !explored {
                    let fog = (10, 10, 14);
                    let c = if ascii { ' ' } else { '░' };
                    cells = [(c, (22, 22, 28), fog, false), (c, (22, 22, 28), fog, false)];
                    bg = fog;
                }

                // Highlights.
                let mut tint: Option<(Rgb, f32)> = None;
                if !pylons.is_empty()
                    && pylons.iter().any(|&(ppx, ppy)| {
                        (x - ppx).abs() <= PYLON_SUPPLY && (y - ppy).abs() <= PYLON_SUPPLY
                    })
                {
                    tint = Some(((60, 120, 255), 0.25));
                }
                if let Some((k, d)) = focus {
                    if k == EntityKind::Pylon
                        && (x - cx).abs() <= PYLON_SUPPLY
                        && (y - cy).abs() <= PYLON_SUPPLY
                    {
                        tint = Some(((80, 140, 255), 0.35));
                    }
                    if k.is_miner() && (x - cx).abs() <= 1 && (y - cy).abs() <= 1 {
                        tint = Some(((255, 220, 80), 0.25));
                    }
                    let (dx, dy) = d.delta();
                    // Miners drop into the tile right in front of them.
                    if k.is_miner() && (x, y) == (cx + dx, cy + dy) {
                        tint = Some(((80, 255, 120), 0.3));
                    }
                    if k.is_arm() {
                        let n = k.arm_reach();
                        if (x, y) == (cx - dx * n, cy - dy * n) {
                            tint = Some(((80, 255, 120), 0.35));
                        } else if (x, y) == (cx + dx * n, cy + dy * n) {
                            tint = Some(((255, 150, 60), 0.4));
                        }
                    }
                    if k.is_tunnel() {
                        if tunnel_mate == Some((x, y)) {
                            tint = Some(((80, 255, 120), 0.4));
                        } else if tunnel_mate.is_none()
                            && k == EntityKind::TunnelIn
                            && (1..=TUNNEL_REACH).any(|n| (x, y) == (cx + dx * n, cy + dy * n))
                        {
                            // Where the exit of this entrance can go.
                            tint = Some(((80, 200, 220), 0.25));
                        }
                    }
                }

                // Area selection and blueprint previews.
                if let Some((x0, y0, x1, y1)) = area_sel
                    && x >= x0
                    && x <= x1
                    && y >= y0
                    && y <= y1
                {
                    tint = Some(((255, 200, 60), 0.35));
                }
                if let Some(bp) = self.app.blueprint.as_ref()
                    && x >= cx
                    && y >= cy
                    && x < cx + bp.w
                    && y < cy + bp.h
                {
                    if let Some(c) = bp.cells.iter().find(|c| (c.dx, c.dy) == (x - cx, y - cy)) {
                        let ok = world.can_place(x, y, c.kind, c.dir).is_ok()
                            || world.entity(x, y).is_some_and(|e| e.kind == c.kind);
                        let g = ghost_entity(c.kind, c.dir);
                        cells = entity_cells(&g, ascii, bg, world.transmitted, world.tick);
                        let gbg = if ok { (60, 90, 150) } else { (150, 40, 40) };
                        for cell in &mut cells {
                            cell.2 = gbg;
                        }
                        tint = None;
                    } else {
                        tint = Some(((80, 120, 200), 0.2));
                    }
                }

                // Ghost / cursor.
                let is_cursor = (x, y) == (cx, cy);
                if is_cursor && self.app.blueprint.is_some() {
                    for c in &mut cells {
                        c.2 = blend(c.2, (230, 230, 230), 0.35);
                    }
                } else if is_cursor {
                    if let Some(k) = selected_kind
                        && world.entity(x, y).is_none()
                    {
                        let ok = world.can_place(x, y, k, self.app.place_dir).is_ok()
                            && self.game.player.inventory.count(k.item()) > 0;
                        let g = ghost_entity(k, self.app.place_dir);
                        cells = entity_cells(&g, ascii, bg, world.transmitted, world.tick);
                        let gbg = if ok { (60, 90, 150) } else { (150, 40, 40) };
                        for c in &mut cells {
                            c.2 = gbg;
                        }
                    } else if self.app.selected == Some(Item::Landfill) {
                        let ok = explored
                            && tile.terrain == Terrain::Water
                            && self.game.player.inventory.count(Item::Landfill) > 0;
                        let gbg = if ok { (120, 104, 70) } else { (150, 40, 40) };
                        cells = [('=', (240, 220, 170), gbg, true); 2];
                    } else if self.app.selected == Some(Item::Cart) {
                        let ok = world.entity(x, y).is_some_and(|e| e.kind.is_track())
                            && self.game.player.inventory.count(Item::Cart) > 0;
                        let gbg = if ok { (60, 90, 150) } else { (150, 40, 40) };
                        cells = [
                            (if ascii { 'O' } else { '■' }, CART_RGB, gbg, true),
                            (' ', CART_RGB, gbg, false),
                        ];
                    } else {
                        for c in &mut cells {
                            c.2 = blend(c.2, (230, 230, 230), 0.55);
                            c.1 = (10, 10, 10);
                        }
                    }
                } else if let Some((t, a)) = tint {
                    for c in &mut cells {
                        c.2 = blend(c.2, t, a);
                    }
                }
                // Night falls over the map, but never over the cursor.
                if night > 0.0 && !is_cursor {
                    for c in &mut cells {
                        c.1 = blend(c.1, (20, 24, 50), night * 0.5);
                        c.2 = blend(c.2, (6, 8, 24), night);
                    }
                }

                for (k, (ch, fg, cbg, bold)) in cells.iter().enumerate() {
                    if let Some(c) = buf.cell_mut((px + k as u16, py)) {
                        let mut style = Style::default().fg(col(*fg)).bg(col(*cbg));
                        if *bold {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        c.set_char(*ch).set_style(style);
                    }
                }
            }
        }
        // Fill an odd leftover column.
        if area.width % 2 == 1 {
            for sy in 0..area.height {
                if let Some(c) = buf.cell_mut((area.x + area.width - 1, area.y + sy)) {
                    c.set_char(' ').set_bg(Color::Black);
                }
            }
        }
    }
}

impl MapView<'_> {
    fn power_overlay_active(&self, focus: Option<(EntityKind, Dir)>) -> bool {
        self.app.power_overlay
            || focus.is_some_and(|(k, _)| k.is_power_node() || k == EntityKind::Pylon)
    }
}

fn bar(frac: f32, width: usize, ascii: bool) -> String {
    let frac = frac.clamp(0.0, 1.0);
    let filled = (frac * width as f32).round() as usize;
    let (f, e) = if ascii { ('#', '-') } else { ('█', '░') };
    std::iter::repeat_n(f, filled)
        .chain(std::iter::repeat_n(e, width - filled))
        .collect()
}

fn item_span(item: Item) -> Span<'static> {
    Span::styled(item.name(), Style::default().fg(col(item_rgb(item))))
}

fn key(s: &str) -> Span<'_> {
    Span::styled(
        s,
        Style::default()
            .fg(col((255, 210, 90)))
            .add_modifier(Modifier::BOLD),
    )
}

fn dim(s: impl Into<String>) -> Span<'static> {
    Span::styled(s.into(), Style::default().fg(col((140, 140, 140))))
}

fn header(s: &str) -> Line<'_> {
    Line::from(Span::styled(
        s,
        Style::default()
            .fg(col((120, 200, 255)))
            .add_modifier(Modifier::BOLD),
    ))
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.buffer_mut().set_style(
        area,
        Style::default()
            .bg(col((12, 12, 16)))
            .fg(col((220, 220, 220))),
    );
    match &app.game {
        None => draw_title(f, app),
        Some(game) => draw_game(f, app, game),
    }
    if let Modal::Help { scroll } = app.modal {
        draw_help(f, scroll);
    }
    if let Modal::Slots { save, sel } = app.modal {
        draw_slots(f, app, save, sel);
    }
    if let Modal::NewGame { sel } = app.modal {
        draw_new_game(f, app, sel);
    }
}

fn draw_new_game(f: &mut Frame, app: &App, sel: usize) {
    let scenarios = crate::scenario::SCENARIOS;
    let area = centered(
        f.area(),
        72,
        (scenarios.len() as u16 * 2 + 9).min(f.area().height),
    );
    f.render_widget(Clear, area);
    let block = modal_block("New game");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let best = app.best_times();
    let mut lines = vec![];
    for (k, sc) in scenarios.iter().enumerate() {
        let mut spans = vec![Span::raw(format!("{:<12}", sc.name))];
        let detail = match (sc.seed, best[k]) {
            (_, Some(b)) => format!("best {}", crate::game::clock(b)),
            (None, None) => "random map".to_string(),
            (Some(_), None) if sc.tutorial => "guided".to_string(),
            (Some(seed), None) => format!("seed {seed}"),
        };
        spans.push(dim(format!("  {detail}")));
        lines.push(list_line(spans, k == sel));
    }
    lines.push(Line::from(""));
    let sc = &scenarios[sel.min(scenarios.len() - 1)];
    lines.push(Line::from(Span::styled(
        sc.blurb,
        Style::default().fg(col((255, 220, 140))),
    )));
    lines.push(Line::from(dim(format!("Goal: {}.", sc.goal_text()))));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        key("Enter"),
        dim(" start  "),
        key("Esc"),
        dim(" back"),
    ]));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

/// The current tutorial step, in a card over the top right of the map.
/// How many lines greedy word wrapping at `width` columns turns `text` into.
fn wrapped_lines(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut lines = 1;
    let mut used = 0;
    for word in text.split_whitespace() {
        let n = word.chars().count();
        if used == 0 {
            used = n;
        } else if used + 1 + n <= width {
            used += 1 + n;
        } else {
            lines += 1;
            used = n;
        }
        // A word longer than the line spills over.
        while used > width {
            lines += 1;
            used -= width;
        }
    }
    lines
}

fn draw_tutorial(f: &mut Frame, app: &App, game: &Game, map: Rect) {
    let Some((k, step)) = game.tutorial_step() else {
        return;
    };
    let text = crate::scenario::fill_keys(step.text, |a| app.key_label(a));
    let w = map.width.saturating_sub(2).min(46);
    if w < 20 {
        return;
    }
    let body_w = (w - 4) as usize;
    let h = (wrapped_lines(&text, body_w) as u16 + 2).min(map.height);
    let area = Rect::new(map.x + map.width - w - 1, map.y + 1, w, h);
    f.render_widget(Clear, area);
    let title = format!(
        " Tutorial {}/{}: {} ",
        k + 1,
        crate::scenario::STEPS.len(),
        step.title
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(col((120, 200, 255))))
        .title(Span::styled(
            title,
            Style::default()
                .fg(col((120, 200, 255)))
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(col((16, 22, 34))));
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(block.padding(ratatui::widgets::Padding::horizontal(1))),
        area,
    );
}

const LOGO: &[&str] = &[
    r"  __               _           _        ",
    r" / _|  __ _   ___ | |_  _   _ (_)  ___  ",
    r"| |_  / _` | / __|| __|| | | || | / _ \ ",
    r"|  _|| (_| || (__ | |_ | |_| || || (_) |",
    r"|_|   \__,_| \___| \__| \__,_||_| \___/ ",
];

fn draw_title(f: &mut Frame, app: &App) {
    let area = f.area();
    let items = app.title_items();
    let mut lines: Vec<Line> = vec![];
    for (k, l) in LOGO.iter().enumerate() {
        let c = blend(
            (255, 170, 60),
            (120, 200, 255),
            k as f32 / LOGO.len() as f32,
        );
        lines.push(Line::from(Span::styled(
            *l,
            Style::default().fg(col(c)).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(dim(
        "mine  ·  smelt  ·  convey  ·  assemble  ·  research  ·  transmit",
    )));
    lines.push(Line::from(""));
    for (k, it) in items.iter().enumerate() {
        let sel = k == app.title_sel.min(items.len() - 1);
        let style = if sel {
            Style::default()
                .fg(Color::Black)
                .bg(col((255, 200, 80)))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(col((220, 220, 220)))
        };
        lines.push(Line::from(Span::styled(format!("  {it:^18}  "), style)));
    }
    lines.push(Line::from(""));
    if let Some(m) = &app.title_msg {
        lines.push(Line::from(Span::styled(
            m.clone(),
            Style::default().fg(col((255, 100, 100))),
        )));
    }
    lines.push(Line::from(dim(format!(
        "save file: {}",
        app.save_path.display()
    ))));
    lines.push(Line::from(dim(
        "↑/↓ choose · Enter select · n new game or challenge · q quit",
    )));
    let h = lines.len() as u16;
    let y = area.height.saturating_sub(h) / 2;
    let r = Rect::new(area.x, area.y + y, area.width, h.min(area.height));
    f.render_widget(Paragraph::new(lines).centered(), r);
}

fn draw_game(f: &mut Frame, app: &App, game: &Game) {
    let area = f.area();
    let [top, mid, hot, msg] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let side_w = if area.width >= 90 { 38 } else { 30 };
    let [map_area, side] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(side_w)]).areas(mid);

    draw_top(f, app, game, top);
    f.render_widget(MapView { app, game }, map_area);
    if app.modal == Modal::None {
        draw_tutorial(f, app, game, map_area);
    }
    draw_sidebar(f, app, game, side);
    draw_hotbar(f, app, game, hot);
    draw_messages(f, app, game, msg);

    match &app.modal {
        Modal::Craft { sel } => draw_craft(f, app, game, *sel),
        Modal::Build { sel } => draw_build(f, game, *sel),
        Modal::Research { sel } => draw_research(f, app, game, *sel),
        Modal::Entity { x, y, sub, sel } => draw_entity_panel(f, app, game, *x, *y, sub, *sel),
        Modal::Menu { sel } => draw_menu(f, *sel),
        Modal::Stats => draw_stats(f, game),
        Modal::Victory => draw_victory(f, app, game),
        Modal::Log { scroll } => draw_log(f, game, *scroll),
        Modal::Confirm { action } => draw_confirm(f, action.question()),
        Modal::Minimap => draw_minimap(f, app, game),
        _ => {}
    }
}

/// A tech's name, with the level an endless one is working toward.
fn tech_label(r: &crate::research::Research, t: TechId) -> String {
    if t.get().repeatable {
        format!("{} {}", t.get().name, r.level(t) + 1)
    } else {
        t.get().name.to_string()
    }
}

fn time_of_day(light: f32, ascii: bool) -> Span<'static> {
    let (sym, word, c) = if light >= 0.999 {
        (if ascii { "*" } else { "☀" }, "day", (255, 220, 90))
    } else if light <= 0.0 {
        (if ascii { ")" } else { "☾" }, "night", (150, 170, 255))
    } else {
        (if ascii { "~" } else { "◐" }, "dusk", (240, 160, 110))
    };
    Span::styled(format!("{sym} {word}"), Style::default().fg(col(c)))
}

fn draw_top(f: &mut Frame, app: &App, game: &Game, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " factuio ",
            Style::default()
                .fg(Color::Black)
                .bg(col((255, 190, 70)))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::raw(game.play_time()),
        Span::raw(" "),
        time_of_day(game.world.daylight(), app.ascii),
        Span::raw("  "),
    ];
    let sc = game.world.scenario;
    if !sc.is_standard() {
        let label = match sc.goal {
            crate::scenario::Goal::Produce(item, n) => format!(
                "{} {}/{n}",
                sc.name,
                game.world.produced.get(&item).copied().unwrap_or(0).min(n)
            ),
            crate::scenario::Goal::Uplink => sc.name.to_string(),
        };
        spans.push(Span::styled(
            label,
            Style::default().fg(col((230, 170, 255))),
        ));
        spans.push(Span::raw("  "));
    }
    if app.paused {
        spans.push(Span::styled(
            "PAUSED",
            Style::default()
                .fg(col((255, 120, 120)))
                .add_modifier(Modifier::BOLD),
        ));
    } else if app.fast {
        spans.push(Span::styled(
            "▶▶ 4x",
            Style::default().fg(col((255, 220, 90))),
        ));
    } else {
        spans.push(dim("▶ 1x"));
    }
    spans.push(Span::raw("   "));
    let r = &game.world.research;
    match r.current {
        Some(t) => {
            spans.push(Span::raw("Research: "));
            spans.push(Span::styled(
                tech_label(r, t),
                Style::default().fg(col((120, 220, 255))),
            ));
            spans.push(Span::raw(format!(
                " {} {}/{}",
                bar(r.fraction(t), 10, app.ascii),
                r.units_done(t),
                r.units_needed(t)
            )));
            if !r.queue.is_empty() {
                spans.push(dim(format!(" +{} queued", r.queue.len())));
            }
        }
        None if r.all_done() => spans.push(Span::styled(
            "All research complete",
            Style::default().fg(col((120, 230, 120))),
        )),
        None => spans.push(Span::styled(
            "No research — press t",
            Style::default().fg(col((255, 200, 90))),
        )),
    }
    if game.world.won {
        spans.push(Span::styled(
            "   ★ TRANSMITTED",
            Style::default()
                .fg(col((255, 230, 90)))
                .add_modifier(Modifier::BOLD),
        ));
    }
    if app.paint {
        spans.push(Span::styled(
            "   PAINT",
            Style::default().fg(Color::Black).bg(col((120, 230, 120))),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(col((30, 30, 40)))),
        area,
    );
}

fn fuel_line(fuel: &crate::world::Fuel) -> Line<'static> {
    let c = if fuel.has() {
        col((220, 220, 220))
    } else {
        col((255, 100, 100))
    };
    Line::from(vec![
        Span::raw("Fuel: "),
        Span::styled(format!("{} lignite", fuel.stock), Style::default().fg(c)),
    ])
}

fn inv_lines(inv: &crate::inventory::Inventory, max: usize) -> Vec<Line<'static>> {
    let mut v: Vec<Line> = inv
        .iter()
        .take(max)
        .map(|(i, n)| Line::from(vec![Span::raw(format!("  {n:>4} ")), item_span(i)]))
        .collect();
    let extra = inv.iter().count().saturating_sub(max);
    if extra > 0 {
        v.push(Line::from(dim(format!("  … {extra} more"))));
    }
    if inv.is_empty() {
        v.push(Line::from(dim("  (empty)")));
    }
    v
}

fn entity_detail(
    app: &App,
    game: &Game,
    x: i32,
    y: i32,
    lines: &mut Vec<Line<'static>>,
    rich: bool,
) {
    let w = &game.world;
    let Some(e) = w.entity(x, y) else { return };
    let ascii = app.ascii;
    lines.push(Line::from(vec![
        Span::styled(e.kind.name(), Style::default().add_modifier(Modifier::BOLD)),
        if e.kind.directional() {
            dim(format!(" facing {:?}", e.dir))
        } else {
            Span::raw("")
        },
    ]));
    lines.push(Line::from(vec![
        Span::raw("Status: "),
        Span::styled(
            e.status.label(),
            Style::default().fg(status_color(e.status)),
        ),
    ]));
    let i = w.idx(x, y).unwrap();
    if e.kind.is_power_node() || e.kind == EntityKind::Pylon {
        match w.net_at(i) {
            Some(n) => lines.push(Line::from(format!(
                "Grid: {:.0}/{:.0} kW  {:.0}%",
                n.load,
                n.capacity,
                n.satisfaction * 100.0
            ))),
            None => lines.push(Line::from(Span::styled(
                "Not on a pylon network",
                Style::default().fg(col((255, 100, 100))),
            ))),
        }
        if let Some(n) = w.net_at(i)
            && n.storage > 0.0
        {
            lines.push(Line::from(dim(format!(
                "Batteries: {:.1}/{:.0} MJ",
                n.stored / 1000.0,
                n.storage / 1000.0
            ))));
        }
    }
    let slots = e.kind.module_slots();
    if slots > 0 && (rich || !e.modules.is_empty()) {
        let mut spans = vec![Span::raw(format!("Modules {}/{slots}", e.modules.len()))];
        for &m in &e.modules {
            spans.push(Span::raw(" "));
            spans.push(item_span(m));
        }
        lines.push(Line::from(spans));
    }
    match &e.state {
        State::Belt { items, .. } => {
            if e.kind.is_tunnel() {
                let mate = w.idx(x, y).and_then(|i| w.tunnel_pair(i));
                lines.push(match mate {
                    Some(j) => {
                        let (mx, my) = w.pos(j);
                        let gap = (mx - x).abs() + (my - y).abs() - 1;
                        let end = if e.kind == EntityKind::TunnelIn {
                            "exit"
                        } else {
                            "entrance"
                        };
                        Line::from(format!("Linked to an {end}, passing under {gap} tiles"))
                    }
                    None => Line::from(Span::styled(
                        "Not linked: needs the other end ahead, facing the same way",
                        Style::default().fg(col((255, 100, 100))),
                    )),
                });
            }
            lines.push(Line::from(format!("Carrying {} items", items.len())));
            let mut inv = crate::inventory::Inventory::new();
            for b in items {
                inv.add(b.item, 1);
            }
            if rich {
                lines.extend(inv_lines(&inv, 6));
            }
        }
        State::Arm {
            hand,
            phase,
            filter,
            ..
        } => {
            if e.kind == EntityKind::FilterArm {
                let f = filter.map(|i| i.name()).unwrap_or("anything");
                lines.push(Line::from(format!("Moves only: {f}")));
            }
            let h = hand.map(|i| i.name()).unwrap_or("nothing");
            lines.push(Line::from(format!("Holding: {h}")));
            let p = match phase {
                ArmPhase::Pickup => "waiting to grab",
                ArmPhase::Swing => "swinging to drop",
                ArmPhase::Return => "returning",
            };
            lines.push(Line::from(dim(p)));
        }
        State::Miner {
            fuel,
            progress,
            output,
        } => {
            if e.kind == EntityKind::Digger {
                lines.push(fuel_line(fuel));
            }
            let verb = match e.kind {
                EntityKind::Pump => "Pumping",
                EntityKind::Sifter => "Sifting",
                _ => "Mining",
            };
            lines.push(Line::from(format!("{verb} {}", bar(*progress, 14, ascii))));
            if e.kind == EntityKind::Sifter {
                lines.push(Line::from(format!(
                    "Sand under it: {}/9 tiles",
                    w.sand_in_area(i)
                )));
            }
            let mut left = std::collections::BTreeMap::new();
            for j in w.mining_area(i) {
                if let Some(o) = w.tiles[j].ore {
                    *left.entry(o).or_insert(0u32) += w.tiles[j].amount;
                }
            }
            for (o, n) in left {
                lines.push(Line::from(vec![
                    Span::raw(format!("  {n:>5} ")),
                    item_span(o),
                    dim(" left"),
                ]));
            }
            if let Some(o) = output {
                lines.push(Line::from(vec![
                    Span::raw("Waiting to output "),
                    item_span(*o),
                ]));
            }
        }
        State::Furnace {
            fuel,
            input,
            output,
            job,
            progress,
        } => {
            if e.kind == EntityKind::Kiln {
                lines.push(fuel_line(fuel));
            }
            match job {
                Some(j) => lines.push(Line::from(vec![
                    Span::raw("Smelting "),
                    item_span(j.get().main_output()),
                ])),
                None => lines.push(Line::from(dim("Not smelting"))),
            }
            lines.push(Line::from(bar(*progress, 16, ascii)));
            lines.push(Line::from("Input:"));
            lines.extend(inv_lines(input, 3));
            lines.push(Line::from("Output:"));
            lines.extend(inv_lines(output, 3));
        }
        State::Assembler {
            recipe,
            input,
            output,
            progress,
            ..
        } => {
            match recipe {
                Some(r) => {
                    let rec = r.get();
                    lines.push(Line::from(vec![
                        Span::raw("Making "),
                        item_span(rec.main_output()),
                    ]));
                    lines.push(Line::from(bar(*progress, 16, ascii)));
                    lines.push(Line::from("Ingredients:"));
                    for &(it, n) in rec.inputs {
                        let have = input.count(it);
                        let c = if have >= n {
                            col((200, 200, 200))
                        } else {
                            col((255, 200, 90))
                        };
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {have:>3}/{n} "), Style::default().fg(c)),
                            item_span(it),
                        ]));
                    }
                }
                None => lines.push(Line::from(Span::styled(
                    "No recipe: e then r",
                    Style::default().fg(col((255, 120, 120))),
                ))),
            }
            lines.push(Line::from("Output:"));
            lines.extend(inv_lines(output, 2));
        }
        State::Chest { inv, unload } => {
            if e.kind == EntityKind::Depot {
                lines.push(Line::from(if *unload {
                    "Carts unload here"
                } else {
                    "Carts load here"
                }));
                if rich {
                    lines.push(Line::from(dim("v switches loading and unloading")));
                }
            }
            lines.push(Line::from(format!("{}/{} items", inv.total(), CRATE_CAP)));
            lines.extend(inv_lines(inv, if rich { 12 } else { 6 }));
        }
        State::Rail => {
            let (carts, depots) = w.track_summary(i);
            let plural = |n: usize, one: &str| match n {
                1 => format!("1 {one}"),
                n => format!("{n} {one}s"),
            };
            lines.push(Line::from(dim(format!(
                "This track: {}, {}",
                plural(depots, "depot"),
                plural(carts, "cart")
            ))));
            if depots < 2 {
                lines.push(Line::from(dim("Carts need two depots to shuttle between")));
            }
        }
        State::Survey { radius } => {
            lines.push(Line::from(format!(
                "Surveyed {:.0} of {:.0} tiles around",
                radius,
                crate::world::SURVEY_MAX
            )));
            lines.push(Line::from(bar(
                radius / crate::world::SURVEY_MAX,
                16,
                ascii,
            )));
            lines.push(Line::from(dim(format!(
                "{:.0}% of the map explored",
                w.explored_share() * 100.0
            ))));
        }
        State::Generator { fuel, water, .. } => {
            lines.push(fuel_line(fuel));
            if e.kind == EntityKind::SteamTurbine {
                lines.push(Line::from(vec![
                    Span::raw(format!("{water:>4} ")),
                    item_span(Item::Water),
                ]));
            }
        }
        State::Battery { charge } => {
            let cap = crate::world::BANK_CAPACITY;
            lines.push(Line::from(format!(
                "Stored {:.1}/{:.0} MJ",
                charge / 1000.0,
                cap / 1000.0
            )));
            lines.push(Line::from(bar(charge / cap, 16, ascii)));
        }
        State::Pylon => {
            lines.push(Line::from(dim(format!(
                "Powers {}x{} area",
                PYLON_SUPPLY * 2 + 1,
                PYLON_SUPPLY * 2 + 1
            ))));
        }
        State::Solar => {}
        State::Lab { inv, progress, .. } => {
            match w.research.current {
                Some(t) => lines.push(Line::from(vec![
                    Span::raw("Researching "),
                    Span::styled(t.get().name, Style::default().fg(col((120, 220, 255)))),
                ])),
                None => lines.push(Line::from(dim("No research selected (t)"))),
            }
            lines.push(Line::from(bar(*progress, 16, ascii)));
            lines.extend(inv_lines(inv, 3));
        }
        State::Uplink { inv, progress, .. } => {
            let sent = w.transmitted;
            if w.won {
                let next = w.next_milestone();
                lines.push(Line::from(format!(
                    "Transmitted {sent}. Next milestone {next}"
                )));
                lines.push(Line::from(dim(format!(
                    "{:.1} s per module now",
                    w.transmission_secs()
                ))));
            } else {
                lines.push(Line::from(format!(
                    "Transmission {}/{}",
                    sent.min(UPLINK_GOAL),
                    UPLINK_GOAL
                )));
                lines.push(Line::from(bar(sent as f32 / UPLINK_GOAL as f32, 16, ascii)));
            }
            lines.push(Line::from(bar(*progress, 16, ascii)));
            lines.push(Line::from(format!(
                "Relay modules: {}",
                inv.count(Item::RelayModule)
            )));
        }
    }
}

/// Lines describing the carts on tile `i`.
fn cart_lines(w: &crate::world::World, i: usize, lines: &mut Vec<Line<'static>>) {
    for c in w.carts_at(i) {
        lines.push(Line::from(vec![
            Span::styled("Rail cart", Style::default().add_modifier(Modifier::BOLD)),
            dim(format!(
                " {}/{} · {}",
                c.cargo.total(),
                crate::world::CART_CAP,
                c.status.label()
            )),
        ]));
        if let Some(d) = c.destination() {
            let (dx, dy) = w.pos(d);
            lines.push(Line::from(dim(format!(
                "  heading for the depot at ({dx}, {dy}), {} tiles",
                c.path.len()
            ))));
        }
        for (it, n) in c.cargo.iter().take(3) {
            lines.push(Line::from(vec![
                Span::raw(format!("  {n:>4} ")),
                item_span(it),
            ]));
        }
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, game: &Game, area: Rect) {
    let mut lines: Vec<Line<'static>> = vec![];
    let (x, y) = game.player.cursor;
    let w = &game.world;
    let tile = w.tile(x, y).unwrap();
    lines.push(header("Cursor"));
    let explored = w.is_explored(x, y);
    let terr = match tile.terrain {
        _ if !explored => "Unexplored",
        Terrain::Grass => "Grass",
        Terrain::Sand => "Sand",
        Terrain::Water => "Water",
    };
    lines.push(Line::from(format!("({x}, {y}) {terr}")));
    if let Some(o) = tile.ore.filter(|_| explored) {
        lines.push(Line::from(vec![
            item_span(o),
            Span::raw(format!(" × {}", tile.amount)),
        ]));
    }
    if !explored {
        lines.push(Line::from(dim(
            "Build nearby to see further, or put up a survey mast.",
        )));
    }
    if let Some(i) = w.idx(x, y) {
        cart_lines(w, i, &mut lines);
    }
    if w.entity(x, y).is_some() {
        lines.push(Line::from(""));
        entity_detail(app, game, x, y, &mut lines, false);
    }
    lines.push(Line::from(""));
    lines.push(header("Goal"));
    lines.push(Line::from(Span::styled(
        game.objective(),
        Style::default().fg(col((255, 220, 140))),
    )));
    let problems = game.problems();
    if !problems.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("Alerts ({})", problems.len()),
                Style::default()
                    .fg(col((255, 110, 110)))
                    .add_modifier(Modifier::BOLD),
            ),
            dim("  n: next"),
        ]));
        let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
        for (_, _, _, s) in &problems {
            *by.entry(s.label()).or_insert(0) += 1;
        }
        let text = by
            .iter()
            .map(|(l, n)| format!("{n} {l}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(col((255, 160, 140))),
        )));
    }
    lines.push(Line::from(""));
    if let Some(a) = app.area_anchor {
        let (x0, y0, x1, y1) = crate::tools::rect(a, (x, y));
        lines.push(header("Area"));
        lines.push(Line::from(format!(
            "{} × {} tiles",
            x1 - x0 + 1,
            y1 - y0 + 1
        )));
        lines.push(Line::from(vec![
            key("x"),
            dim(" pick up  "),
            key("y"),
            dim(" copy  "),
            key("Esc"),
            dim(" cancel"),
        ]));
    } else if let Some(bp) = app.blueprint.as_ref() {
        lines.push(header("Blueprint"));
        lines.push(Line::from(format!(
            "{} buildings, {} × {}",
            bp.cells.len(),
            bp.w,
            bp.h
        )));
        let short: Vec<String> = bp
            .cost()
            .iter()
            .filter_map(|(it, n)| {
                let have = game.player.inventory.count(it);
                (have < n).then(|| format!("{} {}", n - have, it.name()))
            })
            .collect();
        if !short.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("Short: {}", short.join(", ")),
                Style::default().fg(col((255, 170, 90))),
            )));
        }
        lines.push(Line::from(vec![
            key("Enter"),
            dim(" place  "),
            key("r"),
            dim(" rotate  "),
            key("Esc"),
            dim(" drop"),
        ]));
    } else {
        lines.push(header("Build"));
        match app.selected {
            Some(item) => {
                let k = item.building();
                lines.push(Line::from(vec![
                    Span::styled(item.name(), Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!(" ×{}", game.player.inventory.count(item))),
                    if k.is_some_and(|k| k.directional()) {
                        Span::raw(format!(" {}", app.place_dir.arrow(app.ascii)))
                    } else {
                        Span::raw("")
                    },
                ]));
                if let Some(r) = app.selected_recipe {
                    lines.push(Line::from(vec![
                        dim("recipe: "),
                        item_span(r.get().main_output()),
                    ]));
                }
                lines.push(Line::from(dim(match k {
                    Some(k) => k.description(),
                    None => placeable_desc(item),
                })));
            }
            None => lines.push(Line::from(dim("nothing selected (1-0, b)"))),
        }
    }
    if !app.undo.is_empty() {
        lines.push(Line::from(dim(format!("u: undo ({})", app.undo.len()))));
    }
    if let Some((mx, my)) = game.player.mining {
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "Hand mining ({mx},{my}) {}",
            bar(game.player.mine_progress, 8, app.ascii)
        )));
    }
    if let Some(job) = game.player.queue.front() {
        lines.push(Line::from(""));
        lines.push(header("Crafting"));
        lines.push(Line::from(vec![
            item_span(job.recipe.get().main_output()),
            Span::raw(" "),
            Span::raw(bar(job.progress, 10, app.ascii)),
        ]));
        if game.player.queue.len() > 1 {
            lines.push(Line::from(dim(format!(
                "+{} queued",
                game.player.queue.len() - 1
            ))));
        }
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(col((20, 20, 26))));
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

fn draw_hotbar(f: &mut Frame, app: &App, game: &Game, area: Rect) {
    let mut spans = vec![];
    let mut hits = vec![];
    let mut x = area.x;
    for (k, slot) in game.player.hotbar.iter().enumerate() {
        let n = if k == 9 { 0 } else { k + 1 };
        let Some(item) = slot else {
            let s = format!(" {n}:-  ");
            x += s.chars().count() as u16;
            spans.push(dim(s));
            continue;
        };
        let count = game.player.inventory.count(*item);
        let name = item
            .building()
            .map(|b| b.short_name())
            .unwrap_or(item.name());
        let sel = app.selected == Some(*item);
        let style = if sel {
            Style::default()
                .fg(Color::Black)
                .bg(col((255, 200, 80)))
                .add_modifier(Modifier::BOLD)
        } else if count == 0 {
            Style::default().fg(col((110, 110, 110)))
        } else {
            Style::default().fg(col((230, 230, 230)))
        };
        let label = format!(" {n}:{name} {count} ");
        let w = label.chars().count() as u16;
        hits.push((Rect::new(x, area.y, w, 1), k));
        x += w + 1;
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    *app.hotbar_hits.borrow_mut() = hits;
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(col((30, 30, 40)))),
        area,
    );
}

fn draw_messages(f: &mut Frame, _app: &App, game: &Game, area: Rect) {
    let mut spans = vec![];
    if let Some(m) = game.status_message() {
        let style = if m.important {
            Style::default()
                .fg(Color::Black)
                .bg(col((255, 210, 90)))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(col((255, 230, 150)))
        };
        spans.push(Span::styled(format!(" {} ", m.text), style));
        spans.push(Span::raw(" "));
    }
    for (k, d) in [
        ("?", "help"),
        ("b", "build"),
        ("c", "craft"),
        ("t", "research"),
        ("e", "inspect"),
        ("l", "log"),
        ("Esc", "menu"),
    ] {
        spans.push(key(k));
        spans.push(dim(format!(" {d}  ")));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn modal_block(title: &str) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(col((255, 200, 80)))
                .add_modifier(Modifier::BOLD),
        ))
        .style(
            Style::default()
                .bg(col((22, 22, 30)))
                .fg(col((225, 225, 225))),
        )
}

fn list_line(text: Vec<Span<'static>>, selected: bool) -> Line<'static> {
    let mut l = Line::from(text);
    if selected {
        l = l.style(
            Style::default()
                .bg(col((60, 60, 90)))
                .add_modifier(Modifier::BOLD),
        );
    }
    l
}

/// What a placeable item that isn't a building does.
fn placeable_desc(item: Item) -> &'static str {
    match item {
        Item::Cart => {
            "Put it on a depot or rail. It shuttles between the depots on its track, carrying 400 items."
        }
        _ => "Place it on a water tile to turn it into sand you can build on.",
    }
}

/// Keeps the selected row visible in a list of `height` rows.
fn scroll_for(sel: usize, height: u16) -> u16 {
    let h = height.max(1) as usize;
    if sel >= h { (sel + 1 - h) as u16 } else { 0 }
}

fn draw_craft(f: &mut Frame, app: &App, game: &Game, sel: usize) {
    let area = centered(f.area(), 96, 32);
    f.render_widget(Clear, area);
    let block = modal_block("Crafting & inventory");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(34), Constraint::Min(20)]).areas(inner);
    let list = app.craft_list();
    let sel = sel.min(list.len() - 1);
    let r = &game.world.research;
    let rows: Vec<Line> = list
        .iter()
        .enumerate()
        .map(|(k, &id)| {
            let rec = id.get();
            let unlocked = r.is_unlocked(id);
            let n = game.craftable_count(id);
            let out = rec.main_output();
            let name = Span::styled(
                format!("{:<20}", rec.name()),
                Style::default().fg(if unlocked {
                    col(item_rgb(out))
                } else {
                    col((90, 90, 90))
                }),
            );
            let cnt = if !unlocked {
                dim(" locked")
            } else if n == 0 {
                dim("   -")
            } else {
                Span::styled(
                    format!(
                        " {:>3}",
                        if n >= 99 {
                            "99+".to_string()
                        } else {
                            n.to_string()
                        }
                    ),
                    Style::default().fg(col((120, 230, 120))),
                )
            };
            list_line(vec![Span::raw(" "), name, cnt], k == sel)
        })
        .collect();
    let lh = left.height.saturating_sub(1);
    f.render_widget(Paragraph::new(rows).scroll((scroll_for(sel, lh), 0)), left);

    let id = list[sel];
    let rec = id.get();
    let mut lines: Vec<Line<'static>> = vec![header(rec.name())];
    if let Some(k) = rec.main_output().building() {
        lines.push(Line::from(dim(k.description())));
    }
    lines.push(Line::from(format!(
        "Makes {} in {:.1}s",
        rec.outputs
            .iter()
            .map(|(i, n)| format!("{n} {}", i.name()))
            .collect::<Vec<_>>()
            .join(", "),
        rec.time
    )));
    for &(it, n) in rec.inputs {
        let have = game.player.inventory.count(it);
        let c = if have >= n {
            col((200, 200, 200))
        } else {
            col((255, 170, 90))
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {n:>2} × "), Style::default().fg(c)),
            item_span(it),
            dim(format!("  (have {have})")),
        ]));
    }
    if !r.is_unlocked(id) {
        let tech = TechId::all().find(|t| t.get().unlocks.contains(&rec.id));
        if let Some(t) = tech {
            lines.push(Line::from(Span::styled(
                format!("Unlocked by research: {}", t.get().name),
                Style::default().fg(col((255, 120, 120))),
            )));
        }
    }
    let used: Vec<&str> = crate::recipes::RecipeId::consumers(rec.main_output())
        .iter()
        .map(|r| r.get().name())
        .collect();
    if !used.is_empty() {
        lines.push(Line::from(dim(format!("Used in: {}", used.join(", ")))));
    } else if rec.main_output().is_pack() {
        lines.push(Line::from(dim("Used in: labs, for research")));
    }
    lines.push(Line::from(vec![
        key("Enter"),
        dim(" craft 1  "),
        key("f"),
        dim(" craft 5  "),
        key("F"),
        dim(" craft max  "),
        key("x"),
        dim(" cancel last"),
    ]));
    lines.push(Line::from(dim(
        "Missing intermediates are crafted automatically.",
    )));
    if !game.player.queue.is_empty() {
        lines.push(Line::from(format!(
            "Queue: {} job(s)",
            game.player.queue.len()
        )));
    }
    lines.push(Line::from(""));
    lines.push(header("Inventory"));
    let inv: Vec<(Item, u32)> = game.player.inventory.iter().collect();
    let half = inv.len().div_ceil(2);
    for row in 0..half {
        let mut spans = vec![];
        for col_k in 0..2 {
            if let Some(&(it, n)) = inv.get(row + col_k * half) {
                spans.push(Span::raw(format!("{n:>5} ")));
                spans.push(Span::styled(
                    format!("{:<18}", it.name()),
                    Style::default().fg(col(item_rgb(it))),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        right.inner(ratatui::layout::Margin::new(1, 0)),
    );
}

fn draw_build(f: &mut Frame, game: &Game, sel: usize) {
    let area = centered(f.area(), 80, 22);
    f.render_widget(Clear, area);
    let block = modal_block("Buildings");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(28), Constraint::Min(20)]).areas(inner);
    let items: Vec<Item> = Item::placeable().collect();
    let sel = sel.min(items.len() - 1);
    let rows: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(k, item)| {
            let n = game.player.inventory.count(*item);
            let c = if n > 0 {
                col((230, 230, 230))
            } else {
                col((110, 110, 110))
            };
            list_line(
                vec![
                    Span::styled(format!(" {:<20}", item.name()), Style::default().fg(c)),
                    Span::raw(format!("{n:>4}")),
                ],
                k == sel,
            )
        })
        .collect();
    f.render_widget(
        Paragraph::new(rows).scroll((scroll_for(sel, left.height), 0)),
        left,
    );
    let Some(kind) = items[sel].building() else {
        let lines = vec![
            header(items[sel].name()),
            Line::from(placeable_desc(items[sel])),
            Line::from(""),
            Line::from(vec![key("Enter"), dim(" select for placing")]),
            Line::from(vec![key("1-0"), dim(" put on hotbar slot")]),
        ];
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }),
            right.inner(ratatui::layout::Margin::new(1, 0)),
        );
        return;
    };
    let mut lines = vec![
        header(kind.name()),
        Line::from(kind.description()),
        Line::from(""),
    ];
    if kind.uses_power() {
        lines.push(Line::from(format!(
            "Power: {:.0} kW while working",
            kind.power_draw()
        )));
    }
    if kind.power_output() > 0.0 {
        lines.push(Line::from(format!(
            "Produces up to {:.0} kW",
            kind.power_output()
        )));
    }
    if kind.fuel_draw() > 0.0 {
        lines.push(Line::from(format!(
            "Burns lignite or coke: {:.0} kW",
            kind.fuel_draw()
        )));
    }
    if kind.module_slots() > 0 {
        lines.push(Line::from(format!(
            "Takes {} modules (insert them with e, i)",
            kind.module_slots()
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![key("Enter"), dim(" select for placing")]));
    lines.push(Line::from(vec![key("1-0"), dim(" put on hotbar slot")]));
    lines.push(Line::from(dim("Craft more in the crafting screen (c).")));
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        right.inner(ratatui::layout::Margin::new(1, 0)),
    );
}

fn draw_research(f: &mut Frame, app: &App, game: &Game, sel: usize) {
    let area = centered(f.area(), 90, 24);
    f.render_widget(Clear, area);
    let block = modal_block("Research");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).areas(inner);
    let r = &game.world.research;
    let rows: Vec<Line> = TechId::all()
        .enumerate()
        .map(|(k, t)| {
            let queued = r.queue.iter().position(|&q| q == t);
            let (mark, c) = if r.current == Some(t) {
                (">".to_string(), col((120, 220, 255)))
            } else if let Some(q) = queued {
                (format!("{}", q + 1), col((180, 200, 255)))
            } else if t.get().repeatable && r.is_available(t) {
                (
                    if app.ascii { "*" } else { "∞" }.to_string(),
                    col((240, 200, 255)),
                )
            } else if r.is_done(t) {
                (
                    if app.ascii { "+" } else { "✓" }.to_string(),
                    col((120, 230, 120)),
                )
            } else if r.is_available(t) {
                ("o".to_string(), col((240, 240, 240)))
            } else {
                ("x".to_string(), col((110, 110, 110)))
            };
            list_line(
                vec![Span::styled(
                    format!(" {mark:>1} {:<24}", tech_label(r, t)),
                    Style::default().fg(c),
                )],
                k == sel,
            )
        })
        .collect();
    f.render_widget(
        Paragraph::new(rows).scroll((scroll_for(sel, left.height), 0)),
        left,
    );
    let t = TechId(sel as u16);
    let tech = t.get();
    let label = tech_label(r, t);
    let mut lines = vec![header(&label), Line::from(tech.desc), Line::from("")];
    let packs = tech
        .packs
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(" + ");
    lines.push(Line::from(format!(
        "Cost: {} units of {}",
        r.units_needed(t),
        packs
    )));
    lines.push(Line::from(format!(
        "{:.0}s per unit per lab",
        tech.unit_time
    )));
    lines.push(Line::from(format!(
        "Progress: {} {}/{}",
        bar(r.fraction(t), 16, app.ascii),
        r.units_done(t),
        r.units_needed(t)
    )));
    if !tech.prereqs.is_empty() {
        let names: Vec<_> = tech
            .prereqs
            .iter()
            .filter_map(|p| TechId::by_id(p))
            .map(|p| p.get().name)
            .collect();
        lines.push(Line::from(format!("Requires: {}", names.join(", "))));
    }
    lines.push(Line::from("Unlocks:"));
    for u in tech.unlocks {
        if let Some(rid) = crate::recipes::RecipeId::by_id(u) {
            lines.push(Line::from(vec![
                Span::raw("  • "),
                item_span(rid.get().main_output()),
            ]));
        }
    }
    lines.push(Line::from(""));
    let status = if r.is_done(t) && !tech.repeatable {
        Span::styled("Researched", Style::default().fg(col((120, 230, 120))))
    } else if r.current == Some(t) {
        Span::styled(
            "In progress: feed labs with datapacks",
            Style::default().fg(col((120, 220, 255))),
        )
    } else if let Some(q) = r.queue.iter().position(|&q| q == t) {
        Span::styled(
            format!("Queued #{}: Enter removes it", q + 1),
            Style::default().fg(col((180, 200, 255))),
        )
    } else if r.is_available(t) {
        Span::styled(
            match (r.current.is_some(), tech.repeatable && r.is_done(t)) {
                (true, _) => "Enter: add to queue",
                (false, true) => "Enter: research the next level",
                (false, false) => "Enter: research",
            },
            Style::default().fg(col((255, 210, 90))),
        )
    } else {
        Span::styled(
            "Enter: queue it with its prerequisites",
            Style::default().fg(col((255, 160, 110))),
        )
    };
    lines.push(Line::from(status));
    if !r.queue.is_empty() {
        let names: Vec<_> = r.queue.iter().map(|q| q.get().name).collect();
        lines.push(Line::from(dim(format!("Queue: {}", names.join(" → ")))));
    }
    lines.push(Line::from(vec![
        key("Enter"),
        dim(" research/queue  "),
        key("n"),
        dim(" switch now  "),
        key("x"),
        dim(" clear queue"),
    ]));
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        right.inner(ratatui::layout::Margin::new(1, 0)),
    );
}

fn draw_entity_panel(
    f: &mut Frame,
    app: &App,
    game: &Game,
    x: i32,
    y: i32,
    sub: &EntitySub,
    sel: usize,
) {
    let area = centered(f.area(), 70, 22);
    f.render_widget(Clear, area);
    let Some(e) = game.world.entity(x, y) else {
        return;
    };
    let block = modal_block(e.kind.name());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(inner);
    let mut lines = vec![];
    entity_detail(app, game, x, y, &mut lines, true);
    if let Some(i) = game.world.idx(x, y) {
        cart_lines(&game.world, i, &mut lines);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(dim(e.kind.description())));
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        left.inner(ratatui::layout::Margin::new(1, 0)),
    );

    let mut r: Vec<Line> = vec![];
    match sub {
        EntitySub::Main => {
            r.push(header("Actions"));
            r.push(Line::from(vec![key("t"), Span::raw(" take items")]));
            r.push(Line::from(vec![
                key("i"),
                Span::raw(" insert from inventory"),
            ]));
            if e.kind.fuel_draw() > 0.0 || matches!(e.state, State::Generator { .. }) {
                r.push(Line::from(vec![key("f"), Span::raw(" add 10 fuel")]));
            }
            if e.kind.module_slots() > 0 && !e.modules.is_empty() {
                r.push(Line::from(vec![key("m"), Span::raw(" take modules out")]));
            }
            if e.kind.recipe_category().is_some() {
                r.push(Line::from(vec![key("r"), Span::raw(" choose recipe")]));
            } else if e.kind.directional() {
                r.push(Line::from(vec![key("r"), Span::raw(" rotate")]));
            }
            if e.kind == EntityKind::FilterArm {
                r.push(Line::from(vec![
                    key("f"),
                    Span::raw(" choose what it moves"),
                ]));
            }
            if e.kind.is_tunnel() {
                r.push(Line::from(vec![
                    key("v"),
                    Span::raw(" switch entrance / exit"),
                ]));
            }
            if e.kind == EntityKind::Depot {
                r.push(Line::from(vec![
                    key("v"),
                    Span::raw(" switch loading / unloading"),
                ]));
            }
            r.push(Line::from(vec![key("x"), Span::raw(" pick up building")]));
            r.push(Line::from(vec![key("Esc"), Span::raw(" close")]));
        }
        EntitySub::Insert => {
            r.push(header("Insert which item?"));
            let list = game.insertable(x, y);
            if list.is_empty() {
                r.push(Line::from(dim("Nothing in your inventory fits.")));
            }
            for (k, (it, n)) in list.iter().enumerate() {
                r.push(list_line(
                    vec![Span::raw(format!(" {n:>4} ")), item_span(*it)],
                    k == sel,
                ));
            }
            r.push(Line::from(""));
            r.push(Line::from(vec![
                key("Enter"),
                dim(" all  "),
                key("1"),
                dim(" one  "),
                key("Esc"),
                dim(" back"),
            ]));
        }
        EntitySub::Recipe => {
            r.push(header("Choose a recipe"));
            let list = app.recipe_choices(e.kind);
            let rows: Vec<Line> = list
                .iter()
                .enumerate()
                .map(|(k, c)| match c {
                    None => list_line(vec![dim(" (none)")], k == sel),
                    Some(id) => list_line(
                        vec![Span::raw(" "), item_span(id.get().main_output())],
                        k == sel,
                    ),
                })
                .collect();
            let h = right.height.saturating_sub(2);
            let off = scroll_for(sel, h) as usize;
            r.extend(rows.into_iter().skip(off).take(h as usize));
        }
        EntitySub::Filter => {
            r.push(header("Move only which item?"));
            let rows: Vec<Line> = crate::app::filter_choices()
                .iter()
                .enumerate()
                .map(|(k, c)| match c {
                    None => list_line(vec![dim(" (anything)")], k == sel),
                    Some(it) => list_line(vec![Span::raw(" "), item_span(*it)], k == sel),
                })
                .collect();
            let h = right.height.saturating_sub(2);
            let off = scroll_for(sel, h) as usize;
            r.extend(rows.into_iter().skip(off).take(h as usize));
        }
    }
    f.render_widget(Paragraph::new(r), right);
}

fn draw_menu(f: &mut Frame, sel: usize) {
    let area = centered(f.area(), 34, MENU_ITEMS.len() as u16 + 4);
    f.render_widget(Clear, area);
    let block = modal_block("Paused");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows: Vec<Line> = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(k, m)| list_line(vec![Span::raw(format!("  {m}"))], k == sel))
        .collect();
    f.render_widget(
        Paragraph::new(rows),
        inner.inner(ratatui::layout::Margin::new(0, 1)),
    );
}

fn draw_stats(f: &mut Frame, game: &Game) {
    let area = centered(f.area(), 70, 26);
    f.render_widget(Clear, area);
    let block = modal_block("Factory statistics");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(inner);
    let mut l = vec![header("Produced (total, last minute)")];
    for (it, n) in &game.world.produced {
        l.push(Line::from(vec![
            Span::raw(format!("{n:>7} ")),
            item_span(*it),
            dim(format!(" {:.0}/m", game.world.rate_per_min(*it, 60))),
        ]));
    }
    if game.world.produced.is_empty() {
        l.push(Line::from(dim("Nothing automated yet.")));
    }
    f.render_widget(
        Paragraph::new(l),
        left.inner(ratatui::layout::Margin::new(1, 0)),
    );

    let mut counts: std::collections::BTreeMap<EntityKind, u32> = Default::default();
    for e in game.world.entities.iter().flatten() {
        *counts.entry(e.kind).or_insert(0) += 1;
    }
    let mut r = vec![header("Buildings")];
    for (k, n) in counts {
        r.push(Line::from(format!("{n:>5} {}", k.name())));
    }
    r.push(Line::from(""));
    r.push(header("Power grids"));
    for (k, n) in game.world.nets.iter().enumerate() {
        r.push(Line::from(format!(
            "#{k}: {:.0}/{:.0} kW ({:.0}%)",
            n.load,
            n.capacity,
            n.satisfaction * 100.0
        )));
    }
    r.push(Line::from(""));
    r.push(Line::from(format!("Play time {}", game.play_time())));
    r.push(Line::from(format!(
        "Research {}/{}",
        game.world.research.done.len(),
        crate::recipes::TECHS.len()
    )));
    f.render_widget(Paragraph::new(r), right);
}

fn draw_log(f: &mut Frame, game: &Game, scroll: u16) {
    let area = centered(f.area(), 90, f.area().height.saturating_sub(4).max(8));
    f.render_widget(Clear, area);
    let block = modal_block("Message log");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let tps = crate::world::TICKS_PER_SEC as u64;
    let lines: Vec<Line> = game
        .messages
        .iter()
        .map(|m| {
            let secs = m.tick / tps;
            let stamp = dim(format!(
                "{}:{:02}:{:02} ",
                secs / 3600,
                (secs / 60) % 60,
                secs % 60
            ));
            let text = if m.important {
                Span::styled(
                    m.text.clone(),
                    Style::default()
                        .fg(col((255, 220, 120)))
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw(m.text.clone())
            };
            Line::from(vec![stamp, text])
        })
        .collect();
    let h = inner.height.saturating_sub(1) as usize;
    // Newest at the bottom; scroll counts lines back from the end.
    let max_back = lines.len().saturating_sub(h);
    let back = (scroll as usize).min(max_back);
    let start = lines.len().saturating_sub(h + back);
    let shown: Vec<Line> = lines.into_iter().skip(start).take(h).collect();
    let [body, foot] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    f.render_widget(Paragraph::new(shown), body);
    f.render_widget(Paragraph::new(dim("↑/↓ scroll · Esc close")), foot);
}

fn draw_slots(f: &mut Frame, app: &App, save: bool, sel: usize) {
    let area = centered(f.area(), 60, app.slots.len() as u16 * 2 + 5);
    f.render_widget(Clear, area);
    let block = modal_block(if save { "Save to slot" } else { "Load a save" });
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![];
    for (k, s) in app.slots.iter().enumerate() {
        let current = s.path == app.save_path;
        let label = format!(" {}{}", s.label, if current { "  (current)" } else { "" });
        let style = if s.exists {
            Style::default()
        } else {
            Style::default().fg(col((120, 120, 120)))
        };
        lines.push(list_line(vec![Span::styled(label, style)], k == sel));
        lines.push(Line::from(dim(format!("   {}", s.detail))));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        key("Enter"),
        dim(if save { " save here  " } else { " load  " }),
        key("Esc"),
        dim(" back"),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

/// The whole map, two tiles per terminal cell vertically using half blocks.
fn draw_minimap(f: &mut Frame, app: &App, game: &Game) {
    let full = f.area();
    let area = centered(
        full,
        full.width.saturating_sub(4),
        full.height.saturating_sub(2),
    );
    f.render_widget(Clear, area);
    let block = modal_block("Map overview");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [body, foot] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let w = &game.world;
    if body.width == 0 || body.height == 0 {
        return;
    }
    // Tiles per cell horizontally, and per half-cell vertically.
    let sx = (w.width as f32 / body.width as f32).ceil().max(1.0) as i32;
    let sy = (w.height as f32 / (body.height as f32 * 2.0))
        .ceil()
        .max(1.0) as i32;
    let s = sx.max(sy);
    let (sx, sy) = (s, s);
    let cols = ((w.width + sx - 1) / sx).min(body.width as i32);
    let rows = ((w.height + sy * 2 - 1) / (sy * 2)).min(body.height as i32);
    let ox = body.x + (body.width - cols as u16) / 2;
    let oy = body.y + (body.height - rows as u16) / 2;
    app.minimap_geom.set(crate::app::MinimapGeom {
        area: Rect::new(ox, oy, cols as u16, rows as u16),
        sx,
        sy,
    });
    // Colour of the block of tiles starting at (x, y): buildings win, then ore, then terrain.
    let sample = |x0: i32, y0: i32| -> Rgb {
        let mut ore = None;
        let mut terrain = None;
        for y in y0..(y0 + sy).min(w.height) {
            for x in x0..(x0 + sx).min(w.width) {
                if !w.is_explored(x, y) {
                    continue;
                }
                if let Some(e) = w.entity(x, y) {
                    return blend(entity_bg(e.kind), (255, 255, 255), 0.25);
                }
                let t = w.tile(x, y).unwrap();
                if let Some(o) = t.ore {
                    ore = Some(o);
                }
                terrain.get_or_insert(t.terrain);
            }
        }
        match (ore, terrain) {
            (Some(o), _) => blend(ore_bg(o), item_rgb(o), 0.3),
            (None, Some(t)) => terrain_bg(t),
            // Nothing explored in this block.
            _ => (6, 6, 8),
        }
    };
    let carts: std::collections::HashSet<(i32, i32)> = w
        .carts
        .iter()
        .map(|c| {
            let (x, y) = w.pos(c.shown_at());
            // Column, and half-row (top halves even, bottom halves odd).
            (x / sx, y / sy)
        })
        .collect();
    let (cx, cy) = game.player.cursor;
    let buf = f.buffer_mut();
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = (c * sx, r * sy * 2);
            let mut top = sample(x, y);
            let mut bot = if y + sy < w.height {
                sample(x, y + sy)
            } else {
                (12, 12, 16)
            };
            if carts.contains(&(c, r * 2)) {
                top = CART_RGB;
            }
            if carts.contains(&(c, r * 2 + 1)) {
                bot = CART_RGB;
            }
            let in_cell = |yy: i32| cx >= x && cx < x + sx && cy >= yy && cy < yy + sy;
            if in_cell(y) {
                top = (255, 255, 255);
            }
            if in_cell(y + sy) {
                bot = (255, 255, 255);
            }
            if let Some(cell) = buf.cell_mut((ox + c as u16, oy + r as u16)) {
                if app.ascii {
                    cell.set_char(' ')
                        .set_style(Style::default().bg(col(blend(top, bot, 0.5))));
                } else {
                    cell.set_char('▀')
                        .set_style(Style::default().fg(col(top)).bg(col(bot)));
                }
            }
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(vec![
            dim(format!(
                "1 cell = {sx}×{} tiles · {:.0}% explored · ",
                sy * 2,
                w.explored_share() * 100.0
            )),
            key("arrows"),
            dim(" move cursor  "),
            key("click"),
            dim(" jump  "),
            key("Esc"),
            dim(" back"),
        ])),
        foot,
    );
}

fn draw_confirm(f: &mut Frame, question: &str) {
    let area = centered(f.area(), 50, 7);
    f.render_widget(Clear, area);
    let block = modal_block("Are you sure?");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(question.to_string()),
        Line::from(""),
        Line::from(vec![key("y"), dim(" yes   "), key("n"), dim(" no")]),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).centered(),
        inner,
    );
}

fn draw_victory(f: &mut Frame, app: &App, game: &Game) {
    let area = centered(f.area(), 64, 15);
    f.render_widget(Clear, area);
    let sc = game.world.scenario;
    let uplink = sc.goal == crate::scenario::Goal::Uplink;
    let block = modal_block(if uplink {
        "Transmission complete"
    } else {
        "Challenge complete"
    });
    let inner = block.inner(area);
    f.render_widget(block, area);
    let banner = if uplink {
        "★  The Uplink array is transmitting  ★".to_string()
    } else {
        format!("★  {}: done  ★", sc.name)
    };
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            banner,
            Style::default()
                .fg(col((255, 230, 90)))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "You won in {} of game time.",
            crate::game::clock(game.world.won_at.unwrap_or(game.world.tick))
        )),
        Line::from(format!(
            "{} buildings, {} techs researched.",
            game.world.entities.iter().flatten().count(),
            game.world.research.done.len()
        )),
    ];
    let mut lines = lines;
    if let Some(p) = &app.last_win {
        let gold = Style::default().fg(col((255, 230, 90)));
        lines.push(Line::from(""));
        // Challenges are one fixed map, so their seed best is their best.
        let fixed = sc.seed.is_some();
        if p.is_record() && p.prev_best.is_some() {
            lines.push(Line::from(Span::styled("New personal best!", gold)));
        } else if !fixed && p.is_seed_record() && p.prev_seed_best.is_some() {
            lines.push(Line::from(Span::styled("New best on this seed!", gold)));
        }
        if let Some(b) = p.prev_seed_best.filter(|_| !fixed) {
            lines.push(Line::from(dim(format!(
                "Best before on seed {}: {}",
                game.world.seed,
                crate::game::clock(b)
            ))));
        }
        if let Some(b) = p.prev_best {
            lines.push(Line::from(dim(format!(
                "Best before{}: {}",
                if fixed { "" } else { " on any seed" },
                crate::game::clock(b)
            ))));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(dim(if uplink {
        format!(
            "Keep the Uplink fed: milestone {} makes it harder, and endless research awaits.",
            game.world.next_milestone()
        )
    } else {
        "Keep building if you like, or try another challenge from the title screen.".to_string()
    })));
    lines.push(Line::from(dim("Press Enter to continue.")));
    f.render_widget(Paragraph::new(lines).centered(), inner);
}

pub const HELP: &str = r#"FACTUIO — a terminal factory game

GOAL
  Grow from a few hand-mined rocks into an automated factory, research
  Uplink theory, build the Uplink array and feed it 20 relay modules.
  Your winning time goes in records.json next to your saves. After the
  win the Uplink keeps transmitting toward ever harder milestones, and
  two endless techs keep making mining and arms faster.

NEW GAME
  Free play        a new random world (or --seed N)
  Tutorial         a guided first hour, one step at a time
  Far fields       no malachite or quartz near spawn: haul it in by rail
  Lean ores        the starter patches hold a third of the usual ore
  Sprint           make 60 Beta datapacks in fabricators, against the clock
  Challenges use the same map for everyone and keep their own records.
  From the command line: --scenario tutorial (far-fields, lean, sprint).

MOVING AROUND
  Arrows / WASD        move the cursor (Shift or capital WASD: 8 tiles)
  M                    map overview (click or move, Enter to return)
  n                    jump to the next building with a problem
  The map scrolls when the cursor nears an edge. The sidebar describes
  what's under the cursor and lists alerts.
  The world is 320 x 240 tiles and starts out unexplored beyond the area
  around spawn (░). Every building lets you see a few tiles around it
  (pylons further), so building outward explores; a survey mast
  (Surveying) reveals up to 50 tiles around itself while powered. You
  can only build, landfill and mine where you've explored.

MOUSE
  left click           place the selected building, or inspect
  left drag            lay buildings along the path (belts turn corners);
                       with nothing selected, select an area
  right click / drag   pick up (first right click puts a tool away)
  wheel                scroll (Shift+wheel: sideways); click the hotbar

BUILDING
  1-9, 0               select a building from the hotbar (again to clear)
  b                    building list (assign hotbar slots with 1-0 there)
  Space / Enter        place the selected building (or inspect if occupied)
  r                    rotate the building you're placing (or the one under the cursor)
  v                    paint mode: moving the cursor keeps building;
                       conveyors follow the direction you move and turn corners
  q                    clear selection / pick the building under the cursor
  x / Delete           pick up a building and everything inside it
  u                    undo the last build or pick-up (a whole paint
                       stroke, drag or paste counts as one)
  z                    select an area: move to the other corner, then
                         x pick up everything, y copy it as a blueprint
  Blueprints           Enter places a copy at the cursor, r rotates it,
                       Esc drops it. Fabricators keep their recipes.
  Upgrading            placing an express conveyor, swift or filter arm,
                       bore or arc furnace over its basic version swaps it
                       in place
  Tunnels              place a tunnel belt, then another up to 5 tiles
                       further on facing the same way: the second becomes
                       the exit. Items travel underground between them.
  Rails                (Railways) lay rails with paint mode or a drag; they
                       join up with the rails and depots beside them. Put a
                       rail cart (b, then select it) on a depot. x on a
                       cart picks it up with its cargo.
  o                    toggle the power coverage overlay

INTERACTING
  e                    inspect the building under the cursor:
                         t take items, i insert items, f add lignite,
                         r choose recipe (fabricator) or rotate, x pick up,
                         f choose the item (filter arm),
                         v swap entrance and exit (tunnel belt),
                         v switch loading / unloading (depot),
                         m take modules back out
  m                    mine the ore under the cursor by hand (keep still)
  c / i / Tab          crafting and inventory
                         Enter craft 1, f craft 5, F craft max, x cancel
  t                    research tree: Enter researches a tech, or queues
                       it (with anything it needs) after the current one;
                       n switches right away, x clears the queue
  g                    production statistics
  l                    message log

GAME
  p pause   f fast-forward (4x)   F5 quick save   F9 quick load
  Esc menu: save to or load any of 5 slots, quit. F5/F9 use the slot
  you last saved to or loaded. The game autosaves every 5 minutes, and
  when you press Ctrl+C or close the terminal, to a separate autosave
  file; Continue picks whichever save is newest.

SETTINGS
  settings.json next to your save (~/.factuio/) turns the mouse off,
  changes the autosave interval, and remaps keys, for example
  "keys": {"mine": "k", "undo": "U"}. Remappable actions: rotate,
  paint, pick, remove, inspect, mine, build_menu, craft, research,
  stats, log, power_overlay, pause, fast, select_area, undo,
  next_alert, minimap.

MAP LEGEND
  :: .       ore (brighter = richer). ferrite (blue), malachite (green),
             lignite (dark), quartz (pale)
  →  ↑       conveyor; items ride on it shown by their letter
  Y→         splitter: shares items between the belts ahead, left and right
  →∪  ∩→     tunnel entrance and exit (ASCII: u and n)
  ɑ→         arm: grabs from behind, drops ahead (holding item shown);
             yellow arm, cyan swift arm, orange long arm (reaches 2
             tiles), purple filter arm (moves one chosen item)
  D B        digger / electric bore (arrow = output side)
  K E        kiln / arc furnace     M  fabricator      C  crate
  G          generator              P  pylon           S  sun panel
  L          lab                    U  uplink array
  W→ Q→      pump / sifter          T  steam turbine   H  chem plant
  A▅         battery bank (bar shows its charge)
  ═╗ ║       rails (ASCII = | +)    R+ R-  depot: carts load / unload
  ■g         rail cart and its cargo (ASCII O)
  V◜         survey mast (spins while it surveys)
  ░          unexplored
  A bold white letter means working; red means something is wrong.

ITEMS ON BELTS
  f ferrite  m malachite  l lignite  q quartz  i iron  c copper  g glass
  s steel  * cog  ~ coil  # chip  o lens  & motor  % processor  @ relay
  w water  k coke  r resin  b battery  > speed module  $ productivity
  1 2 3 4 alpha / beta / gamma / delta datapacks   = landfill  + a building

HOW THINGS WORK
  Diggers burn lignite and mine the 3x3 area beneath them, pushing ore
  into whatever is in front: a conveyor, a kiln, a crate...
  Kilns burn lignite and smelt: ferrite → iron ingot, malachite → copper
  ingot, 2 quartz → glass, and after Metallurgy 5 iron → steel.
  Arms need electricity. A generator burns lignite to make 900 kW for
  everything connected to its pylon network. Pylons power machines within
  2 tiles and connect to other pylons up to 7 tiles away. The generator
  itself must be within a pylon's reach.
  When demand exceeds supply, every machine on that grid slows down.
  Arms only pick up items the target can use, so one belt can carry
  mixed items to several machines. A filter arm narrows that to one item.
  A splitter takes turns between its outputs and skips one that is full
  or missing; a belt pointing into its side merges in instead.
  Fabricators craft whatever recipe you give them, with power.
  Labs consume datapacks to research the selected technology.
  Days last six minutes. Sun panels give full power by day, fade at dusk
  and stop at night; battery banks store spare power and give it back.
  Pumps sit with water right behind them and push out 2 water a second.
  Steam turbines burn fuel and boil water for 2400 kW. Arc furnaces bake
  lignite into coke, a denser fuel. Chem plants make resin from lignite
  and water, and batteries from resin. Sifters pull quartz out of sand
  forever. Landfill (b to select) turns a water tile into sand.
  Speed and productivity modules go into bores, sifters, arc furnaces,
  fabricators, chem plants and labs by hand (e, i): speed is +50% for
  +50% power, productivity gives a free batch every ten at 15% less
  speed.
  Rail carts run at 8 tiles a second and carry 400 items. A cart visits
  the depots on its track in turn (two depots make a shuttle). At a
  loading depot it fills up and leaves when full, when nothing has
  come for 5 seconds, or after 30 seconds; at an unloading depot it
  empties out, waiting if the depot is full. Depots hold 800 items and
  arms put in and take out like a crate. Carts pass through each
  other. Ore patches far from spawn are much richer.

GETTING STARTED
  1. Find ore near spawn. Place a Digger (3) on lignite facing a Crate,
     and add lignite fuel to it (e, then f).
  2. Put Diggers on ferrite and malachite facing straight into Kilns.
     Fuel everything. Ingots pile up in the kilns; take them with e → t,
     or put an Arm after the kiln pointing at a crate or a conveyor.
  3. Place a Generator with a Pylon next to it and fuel it.
     Extend pylons to wherever your arms, labs and fabricators are.
  4. Craft Alpha datapacks (c), put them into the Lab (e → i), and pick
     Fabrication in the research screen (t).
  5. Automate: fabricators making cogs, chips and datapacks, fed by arms
     from conveyors. Research your way to Uplink theory and transmit!

Press any key to close. ↑/↓ or PageUp/PageDown to scroll."#;

fn draw_help(f: &mut Frame, scroll: u16) {
    let area = centered(f.area(), 84, f.area().height.saturating_sub(2).max(10));
    f.render_widget(Clear, area);
    let block = modal_block("Help");
    let text = Text::from(HELP);
    let max = (text.lines.len() as u16).saturating_sub(area.height.saturating_sub(2));
    f.render_widget(
        Paragraph::new(text)
            .block(block)
            .scroll((scroll.min(max), 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    fn render(app: &App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..h {
            for x in 0..w {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn app() -> App {
        let mut a = App::new(
            std::env::temp_dir().join("factuio-ui-test.json"),
            Some(11),
            false,
        );
        a.new_game();
        a
    }

    #[test]
    fn title_renders() {
        let a = App::new(std::env::temp_dir().join("nope-factuio.json"), None, false);
        let s = render(&a, 100, 30);
        assert!(s.contains("New game"));
    }

    #[test]
    fn every_screen_renders_at_many_sizes() {
        let mut a = app();
        let (x, y) = a.game.as_ref().unwrap().player.cursor;
        a.game
            .as_mut()
            .unwrap()
            .place(x, y, Item::Crate, Dir::North)
            .unwrap();
        let g = a.game.as_mut().unwrap();
        g.player.inventory.add(Item::FilterArm, 1);
        g.player.inventory.add(Item::Tunnel, 1);
        g.place(x + 2, y, Item::FilterArm, Dir::East).unwrap();
        g.place(x + 3, y, Item::Tunnel, Dir::East).unwrap();
        let modals = vec![
            Modal::None,
            Modal::Help { scroll: 3 },
            Modal::Craft { sel: 5 },
            Modal::Build { sel: 3 },
            Modal::Research { sel: 2 },
            Modal::Entity {
                x,
                y,
                sub: EntitySub::Main,
                sel: 0,
            },
            Modal::Entity {
                x,
                y,
                sub: EntitySub::Insert,
                sel: 0,
            },
            Modal::Entity {
                x: x + 2,
                y,
                sub: EntitySub::Main,
                sel: 0,
            },
            Modal::Entity {
                x: x + 2,
                y,
                sub: EntitySub::Filter,
                sel: 30,
            },
            Modal::Entity {
                x: x + 3,
                y,
                sub: EntitySub::Main,
                sel: 0,
            },
            Modal::Build { sel: 40 },
            Modal::Menu { sel: 1 },
            Modal::Stats,
            Modal::Victory,
            Modal::Log { scroll: 2 },
            Modal::Minimap,
            Modal::Slots {
                save: false,
                sel: 0,
            },
            Modal::Confirm {
                action: crate::app::ConfirmAction::QuitWithoutSaving,
            },
        ];
        for m in modals {
            a.modal = m.clone();
            for (w, h) in [(160, 50), (100, 30), (60, 20), (20, 8)] {
                let s = render(&a, w, h);
                assert!(!s.is_empty(), "{m:?} {w}x{h}");
            }
        }
        a.modal = Modal::None;
        let s = render(&a, 120, 40);
        assert!(s.contains("Cursor"));
        assert!(s.contains("Crate"));
    }

    #[test]
    fn every_building_draws_and_has_a_panel() {
        for ascii in [false, true] {
            for k in EntityKind::ALL {
                let e = ghost_entity(k, Dir::East);
                let cells = entity_cells(&e, ascii, (0, 0, 0), 0, 0);
                assert!(cells.iter().all(|c| c.0 != '\0'), "{k:?}");
                if ascii {
                    assert!(cells.iter().all(|c| c.0.is_ascii()), "{k:?} {cells:?}");
                }
            }
        }
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world = crate::world::World::empty(30, 30, 1);
        g.player.cursor = (12, 12);
        for y in 0..30 {
            let i = g.world.idx(0, y).unwrap();
            g.world.tiles[i].terrain = Terrain::Water;
        }
        for (x, y) in [(5, 5), (5, 6), (6, 5), (6, 6)] {
            let i = g.world.idx(x, y).unwrap();
            g.world.tiles[i].terrain = Terrain::Sand;
        }
        let spots = [
            (Item::Pump, 1, 2),
            (Item::SteamTurbine, 3, 2),
            (Item::Sifter, 5, 5),
            (Item::ChemPlant, 8, 8),
            (Item::BatteryBank, 10, 8),
            (Item::Depot, 14, 3),
            (Item::Rail, 15, 3),
            (Item::SurveyMast, 18, 8),
        ];
        for &(item, x, y) in &spots {
            g.player.inventory.add(item, 1);
            g.place(x, y, item, Dir::East).unwrap();
        }
        g.world.ensure_caches();
        for &(_, x, y) in &spots {
            a.modal = Modal::Entity {
                x,
                y,
                sub: EntitySub::Main,
                sel: 0,
            };
            assert!(!render(&a, 120, 40).is_empty());
        }
        // Something that isn't a building can be selected too.
        a.modal = Modal::None;
        a.selected = Some(Item::Landfill);
        assert!(render(&a, 120, 40).contains("Landfill"));
        a.selected = None;
        a.modal = Modal::Victory;
        a.last_win = Some(crate::records::Placing {
            ticks: 100,
            prev_seed_best: Some(200),
            prev_best: Some(50),
        });
        assert!(render(&a, 120, 40).contains("New best on this seed"));
    }

    #[test]
    fn logistics_pieces_have_their_own_marks() {
        for ascii in [false, true] {
            let mut a = app();
            a.ascii = ascii;
            let g = a.game.as_mut().unwrap();
            let (x, y) = g.player.cursor;
            g.player.inventory.add(Item::Splitter, 1);
            g.player.inventory.add(Item::Tunnel, 2);
            g.place(x + 1, y + 2, Item::Splitter, Dir::East).unwrap();
            g.place(x + 3, y + 2, Item::Tunnel, Dir::East).unwrap();
            g.place(x + 6, y + 2, Item::Tunnel, Dir::East).unwrap();
            g.world.ensure_caches();
            let s = render(&a, 120, 40);
            let (inn, out) = if ascii { ('u', 'n') } else { ('∪', '∩') };
            let arrow = Dir::East.arrow(ascii);
            assert!(s.contains(&format!("Y{arrow}")), "{s}");
            assert!(s.contains(&format!("{arrow}{inn}")), "{s}");
            assert!(s.contains(&format!("{out}{arrow}")), "{s}");
        }
    }

    #[test]
    fn ascii_mode_uses_ascii_arrows() {
        let mut a = app();
        a.ascii = true;
        let g = a.game.as_mut().unwrap();
        let (x, y) = g.player.cursor;
        g.place(x + 1, y, Item::Conveyor, Dir::East).unwrap();
        let s = render(&a, 120, 40);
        assert!(s.contains('>'));
        assert!(!s.contains('→'));
    }

    #[test]
    fn word_wrap_counts_lines() {
        assert_eq!(wrapped_lines("", 10), 1);
        assert_eq!(wrapped_lines("one two", 7), 1);
        assert_eq!(wrapped_lines("one two", 6), 2);
        assert_eq!(wrapped_lines("aaaa bbbb cccc", 9), 2);
        assert_eq!(wrapped_lines("abcdefghij", 4), 3);
    }

    #[test]
    fn color_fallback_maps_to_indexed() {
        set_truecolor(false);
        assert_eq!(col((255, 0, 0)), Color::Indexed(196));
        assert_eq!(col((0, 0, 0)), Color::Indexed(16));
        assert_eq!(col((255, 255, 255)), Color::Indexed(231));
        // Dark ground and fog land on the greyscale ramp, apart from each other and from black.
        let grass = col(terrain_bg(Terrain::Grass));
        let fog = col((10, 10, 14));
        assert!(matches!(grass, Color::Indexed(232..)));
        assert!(matches!(fog, Color::Indexed(232..)));
        assert_ne!(grass, fog);
        set_truecolor(true);
        assert_eq!(col((1, 2, 3)), Color::Rgb(1, 2, 3));
    }

    fn color_css(c: Color, default: &str) -> String {
        match c {
            Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
            Color::Black => "#000".into(),
            _ => default.into(),
        }
    }

    /// Writes HTML snapshots of the main screens to $FACTUIO_DUMP (run with --ignored).
    #[test]
    #[ignore]
    fn dump_screens() {
        let Ok(dir) = std::env::var("FACTUIO_DUMP") else {
            return;
        };
        let mut a = app();
        crate::playtest::build_demo_factory(a.game.as_mut().unwrap());
        let kiln = a.game.as_ref().unwrap().player.cursor;
        let (x, y) = crate::playtest::build_showcase(a.game.as_mut().unwrap());
        // Logo on its own, for the README.
        let mut logo = String::from(
            "<html><body style='background:#0c0c10;margin:0'><pre id='logo' style='display:inline-block;font:bold 22px/24px Menlo,monospace;margin:0;padding:18px 28px'>",
        );
        for (k, l) in LOGO.iter().enumerate() {
            let (r, g, b) = blend(
                (255, 170, 60),
                (120, 200, 255),
                k as f32 / LOGO.len() as f32,
            );
            let l = l.replace('&', "&amp;").replace('<', "&lt;");
            logo.push_str(&format!(
                "<span style='color:rgb({r},{g},{b})'>{l}</span>\n"
            ));
        }
        logo.push_str("</pre></body></html>");
        std::fs::write(format!("{dir}/logo.html"), logo).unwrap();
        let shots = vec![
            ("title", None),
            ("factory", Some(Modal::None)),
            ("map", Some(Modal::None)),
            ("craft", Some(Modal::Craft { sel: 3 })),
            ("research", Some(Modal::Research { sel: 0 })),
            (
                "entity",
                Some(Modal::Entity {
                    x,
                    y,
                    sub: EntitySub::Main,
                    sel: 0,
                }),
            ),
            ("build", Some(Modal::Build { sel: 0 })),
            ("help", Some(Modal::Help { scroll: 0 })),
            ("stats", Some(Modal::Stats)),
        ];
        for (name, m) in shots {
            let mut t = Terminal::new(TestBackend::new(140, 42)).unwrap();
            match m {
                None => {
                    let b = App::new(
                        std::path::PathBuf::from("~/.factuio/save.json"),
                        None,
                        false,
                    );
                    t.draw(|f| draw(f, &b)).unwrap();
                }
                Some(m) => {
                    a.modal = m;
                    let g = a.game.as_mut().unwrap();
                    g.player.cursor = if name == "map" { kiln } else { (x, y) };
                    t.draw(|f| draw(f, &a)).unwrap();
                }
            }
            let buf = t.backend().buffer().clone();
            let mut html = String::from(
                "<html><body style='background:#000;margin:0'><pre style='font:14px/16px Menlo,monospace;margin:0'>",
            );
            for yy in 0..42 {
                for xx in 0..140 {
                    let c = &buf[(xx, yy)];
                    let fg = color_css(c.fg, "#ddd");
                    let bg = color_css(c.bg, "#000");
                    let bold = if c.modifier.contains(Modifier::BOLD) {
                        "font-weight:bold;"
                    } else {
                        ""
                    };
                    let sym = c.symbol().replace('&', "&amp;").replace('<', "&lt;");
                    html.push_str(&format!(
                        "<span style='color:{fg};background:{bg};{bold}'>{sym}</span>"
                    ));
                }
                html.push('\n');
            }
            html.push_str("</pre></body></html>");
            std::fs::write(format!("{dir}/{name}.html"), html).unwrap();
        }
    }

    #[test]
    fn the_new_game_picker_lists_every_scenario() {
        let dir = std::env::temp_dir().join(format!("factuio-ui-pick-{}", std::process::id()));
        let mut a = App::new(dir.join("save.json"), None, false);
        a.modal = Modal::NewGame { sel: 4 };
        let s = render(&a, 100, 30);
        for sc in crate::scenario::SCENARIOS {
            assert!(s.contains(sc.name), "{}", sc.name);
        }
        assert!(s.contains("Beta datapacks"));
        for (w, h) in [(60, 20), (20, 8)] {
            assert!(!render(&a, w, h).is_empty());
        }
    }

    #[test]
    fn the_tutorial_card_shows_the_current_step_with_bound_keys() {
        let dir = std::env::temp_dir().join(format!("factuio-ui-tut-{}", std::process::id()));
        let mut a = App::new(dir.join("save.json"), None, false);
        let mut set = crate::settings::Settings::default();
        set.keys.insert("mine".into(), "k".into());
        a.apply_settings(set);
        a.new_scenario(crate::scenario::Scenario::by_id("tutorial").unwrap());
        a.game.as_mut().unwrap().tutorial = Some(crate::scenario::Tutorial { step: 1, since: 0 });
        let s = render(&a, 130, 40);
        assert!(s.contains("Tutorial 2/"));
        assert!(s.contains("Press k with"), "{s}");
        for (w, h) in [(60, 20), (20, 8)] {
            assert!(!render(&a, w, h).is_empty());
        }
        // Hidden behind other screens, and once it's finished.
        a.modal = Modal::Stats;
        assert!(!render(&a, 130, 40).contains("Tutorial 2/"));
        a.modal = Modal::None;
        a.game.as_mut().unwrap().tutorial = Some(crate::scenario::Tutorial { step: 99, since: 0 });
        assert!(!render(&a, 130, 40).contains("Tutorial 1"));
    }

    #[test]
    fn fog_rails_and_carts_draw() {
        for ascii in [false, true] {
            let mut a = app();
            a.ascii = ascii;
            let g = a.game.as_mut().unwrap();
            g.world = crate::world::World::empty(40, 20, 1);
            g.player.cursor = (10, 5);
            let w = &mut g.world;
            w.place(4, 5, EntityKind::Depot, Dir::North).unwrap();
            for x in 5..=10 {
                w.place(x, 5, EntityKind::Rail, Dir::North).unwrap();
            }
            for y in 6..=8 {
                w.place(10, y, EntityKind::Rail, Dir::North).unwrap();
            }
            w.place(10, 9, EntityKind::Depot, Dir::North).unwrap();
            w.add_cart(4, 5).unwrap();
            w.carts[0].cargo.add(Item::Glass, 3);
            for x in 25..40 {
                for y in 0..20 {
                    let i = w.idx(x, y).unwrap();
                    w.explored[i] = false;
                }
            }
            let s = render(&a, 120, 30);
            if ascii {
                assert!(s.contains("Og"), "cart with glass");
                assert!(s.contains("=="));
            } else {
                assert!(s.contains("■g"), "cart with glass");
                assert!(s.contains("═══"));
                assert!(s.contains("╗") && s.contains("║"));
                assert!(s.contains("░░"));
            }
            assert!(s.contains("R+"));
            a.game.as_mut().unwrap().player.cursor = (30, 5);
            assert!(render(&a, 120, 30).contains("Unexplored"));
            a.game.as_mut().unwrap().player.cursor = (4, 5);
            let s = render(&a, 120, 30);
            assert!(s.contains("Rail cart") && s.contains("Carts load here"));
            a.modal = Modal::Entity {
                x: 4,
                y: 5,
                sub: EntitySub::Main,
                sel: 0,
            };
            assert!(render(&a, 120, 30).contains("switch loading"));
            a.modal = Modal::Minimap;
            assert!(render(&a, 120, 30).contains("explored"));
        }
    }

    #[test]
    fn a_challenge_win_says_so() {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world.scenario = crate::scenario::Scenario::by_id("sprint").unwrap();
        g.world.won = true;
        g.world.won_at = Some(20 * 60 * 12);
        a.modal = Modal::Victory;
        a.last_win = Some(crate::records::Placing {
            ticks: 100,
            prev_seed_best: Some(200),
            prev_best: Some(200),
        });
        let s = render(&a, 120, 40);
        assert!(s.contains("Challenge complete"));
        assert!(s.contains("New personal best"));
        assert!(!s.contains("on this seed"));
        a.modal = Modal::None;
        assert!(render(&a, 160, 40).contains("Sprint"));
    }
}
