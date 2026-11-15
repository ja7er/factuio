//! Rendering with ratatui.

use crate::app::{App, EntitySub, MENU_ITEMS, Modal};
use crate::game::Game;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::TechId;
use crate::world::{
    ArmPhase, CRATE_CAP, Entity, PYLON_SUPPLY, State, Status, Terrain, UPLINK_GOAL,
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
        // Map onto the xterm 256-colour cube.
        let q = |v: u8| -> u8 {
            if v < 48 {
                0
            } else if v < 115 {
                1
            } else {
                (v - 35) / 40
            }
        };
        Color::Indexed(16 + 36 * q(r) + 6 * q(g) + q(b))
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
        EntityKind::Arm | EntityKind::SwiftArm => (40, 40, 40),
    }
}

fn status_rgb(s: Status) -> Rgb {
    match s {
        Status::Working | Status::Done => (250, 250, 250),
        Status::LowPower => (250, 220, 80),
        s if s.is_problem() => (255, 90, 90),
        _ => (170, 170, 170),
    }
}

fn status_color(s: Status) -> Color {
    match s {
        Status::Working | Status::Done => col((120, 230, 120)),
        Status::LowPower | Status::NoInput => col((240, 210, 90)),
        s if s.is_problem() => col((255, 100, 100)),
        _ => col((170, 170, 170)),
    }
}

type Cell2 = [(char, Rgb, Rgb, bool); 2];

/// The two characters used to draw an entity, with fg/bg colours and bold flag.
fn entity_cells(e: &Entity, ascii: bool, bg_under: Rgb) -> Cell2 {
    let bg = if e.kind.is_arm() {
        bg_under
    } else {
        entity_bg(e.kind)
    };
    let st = status_rgb(e.status);
    let dim = (130, 130, 130);
    let letter = |c: char| (c, st, bg, true);
    match &e.state {
        State::Belt { items } => {
            let arrow = e.dir.arrow(ascii);
            let mut out = [(arrow, dim, bg, false), (arrow, dim, bg, false)];
            for b in items {
                let half = if b.pos < crate::world::TILE_LEN / 2 {
                    0
                } else {
                    1
                };
                // Display order follows the belt direction.
                let slot = match e.dir {
                    Dir::East | Dir::South => half,
                    Dir::West | Dir::North => 1 - half,
                };
                out[slot] = (b.item.glyph(), item_rgb(b.item), bg, true);
            }
            out
        }
        State::Arm { hand, .. } => {
            let base = if e.kind == EntityKind::SwiftArm {
                (90, 220, 240)
            } else {
                (240, 210, 80)
            };
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
            letter(if e.kind == EntityKind::Digger {
                'D'
            } else {
                'B'
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
                _ => ('·', dim, bg, false),
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
            [letter('M'), second]
        }
        State::Chest { inv } => {
            let second = match inv.first() {
                Some(i) => (i.glyph(), item_rgb(i), bg, false),
                None => ('·', dim, bg, false),
            };
            [('C', (240, 220, 180), bg, true), second]
        }
        State::Generator { .. } => {
            let s = match e.status {
                Status::Working => '~',
                Status::NoFuel => '!',
                _ => '·',
            };
            [letter('G'), (s, st, bg, true)]
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
                ('·', dim, bg, false)
            };
            [letter('L'), second]
        }
        State::Uplink { charge, .. } => {
            let c = if *charge >= UPLINK_GOAL {
                '!'
            } else {
                char::from_digit(charge * 10 / UPLINK_GOAL, 10).unwrap_or('0')
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
    fn camera(&self, area: Rect) -> (i32, i32, i32, i32) {
        let vw = (area.width / 2) as i32;
        let vh = area.height as i32;
        let w = &self.game.world;
        let (cx, cy) = self.game.player.cursor;
        let cam_x = if w.width <= vw {
            -(vw - w.width) / 2
        } else {
            (cx - vw / 2).clamp(0, w.width - vw)
        };
        let cam_y = if w.height <= vh {
            -(vh - w.height) / 2
        } else {
            (cy - vh / 2).clamp(0, w.height - vh)
        };
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
        let pylons = if self.power_overlay_active(focus) {
            world.pylons()
        } else {
            vec![]
        };

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
                    cells = entity_cells(e, ascii, bg);
                    bg = cells[0].2;
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
                    if k.is_miner() && (x, y) == (cx + dx * 2, cy + dy * 2) {
                        tint = Some(((80, 255, 120), 0.3));
                    }
                    if k.is_arm() {
                        if (x, y) == (cx - dx, cy - dy) {
                            tint = Some(((80, 255, 120), 0.35));
                        } else if (x, y) == (cx + dx, cy + dy) {
                            tint = Some(((255, 150, 60), 0.4));
                        }
                    }
                }

                // Ghost / cursor.
                let is_cursor = (x, y) == (cx, cy);
                if is_cursor {
                    if let Some(k) = selected_kind
                        && world.entity(x, y).is_none()
                    {
                        let ok = world.can_place(x, y, k).is_ok()
                            && self.game.player.inventory.count(k.item()) > 0;
                        let g = ghost_entity(k, self.app.place_dir);
                        cells = entity_cells(&g, ascii, bg);
                        let gbg = if ok { (60, 90, 150) } else { (150, 40, 40) };
                        for c in &mut cells {
                            c.2 = gbg;
                        }
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
        "↑/↓ choose · Enter select · n new game · q quit",
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
        Modal::Victory => draw_victory(f, game),
        _ => {}
    }
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
        Span::raw("  "),
    ];
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
                t.get().name,
                Style::default().fg(col((120, 220, 255))),
            ));
            spans.push(Span::raw(format!(
                " {} {}/{}",
                bar(r.fraction(t), 10, app.ascii),
                r.units_done(t),
                t.get().units
            )));
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
    }
    match &e.state {
        State::Belt { items } => {
            lines.push(Line::from(format!("Carrying {} items", items.len())));
            let mut inv = crate::inventory::Inventory::new();
            for b in items {
                inv.add(b.item, 1);
            }
            if rich {
                lines.extend(inv_lines(&inv, 6));
            }
        }
        State::Arm { hand, phase, .. } => {
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
            lines.push(Line::from(format!("Mining {}", bar(*progress, 14, ascii))));
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
        State::Chest { inv } => {
            lines.push(Line::from(format!("{}/{} items", inv.total(), CRATE_CAP)));
            lines.extend(inv_lines(inv, if rich { 12 } else { 6 }));
        }
        State::Generator { fuel } => lines.push(fuel_line(fuel)),
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
        State::Uplink {
            inv,
            charge,
            progress,
            ..
        } => {
            lines.push(Line::from(format!(
                "Transmission {}/{}",
                charge, UPLINK_GOAL
            )));
            lines.push(Line::from(bar(
                *charge as f32 / UPLINK_GOAL as f32,
                16,
                ascii,
            )));
            lines.push(Line::from(bar(*progress, 16, ascii)));
            lines.push(Line::from(format!(
                "Relay modules: {}",
                inv.count(Item::RelayModule)
            )));
        }
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, game: &Game, area: Rect) {
    let mut lines: Vec<Line<'static>> = vec![];
    let (x, y) = game.player.cursor;
    let w = &game.world;
    let tile = w.tile(x, y).unwrap();
    lines.push(header("Cursor"));
    let terr = match tile.terrain {
        Terrain::Grass => "Grass",
        Terrain::Sand => "Sand",
        Terrain::Water => "Water",
    };
    lines.push(Line::from(format!("({x}, {y}) {terr}")));
    if let Some(o) = tile.ore {
        lines.push(Line::from(vec![
            item_span(o),
            Span::raw(format!(" × {}", tile.amount)),
        ]));
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
    lines.push(Line::from(""));
    lines.push(header("Build"));
    match app.selected {
        Some(item) => {
            let k = item.building().unwrap();
            lines.push(Line::from(vec![
                Span::styled(item.name(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" ×{}", game.player.inventory.count(item))),
                if k.directional() {
                    Span::raw(format!(" {}", app.place_dir.arrow(app.ascii)))
                } else {
                    Span::raw("")
                },
            ]));
            lines.push(Line::from(dim(k.description())));
        }
        None => lines.push(Line::from(dim("nothing selected (1-0, b)"))),
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
    for (k, slot) in game.player.hotbar.iter().enumerate() {
        let n = if k == 9 { 0 } else { k + 1 };
        let Some(item) = slot else {
            spans.push(dim(format!(" {n}:-  ")));
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
        spans.push(Span::styled(format!(" {n}:{name} {count} "), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(col((30, 30, 40)))),
        area,
    );
}

fn draw_messages(f: &mut Frame, _app: &App, game: &Game, area: Rect) {
    let mut spans = vec![];
    if let Some(m) = game.messages.last() {
        spans.push(Span::styled(
            format!(" {m} "),
            Style::default().fg(col((255, 230, 150))),
        ));
        spans.push(Span::raw(" "));
    }
    for (k, d) in [
        ("?", "help"),
        ("b", "build"),
        ("c", "craft"),
        ("t", "research"),
        ("e", "inspect"),
        ("m", "mine"),
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
    let rows: Vec<Line> = EntityKind::ALL
        .iter()
        .enumerate()
        .map(|(k, kind)| {
            let n = game.player.inventory.count(kind.item());
            let c = if n > 0 {
                col((230, 230, 230))
            } else {
                col((110, 110, 110))
            };
            list_line(
                vec![
                    Span::styled(format!(" {:<20}", kind.name()), Style::default().fg(c)),
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
    let kind = EntityKind::ALL[sel];
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
            "Burns lignite: {:.0} kW",
            kind.fuel_draw()
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
            let (mark, c) = if r.is_done(t) {
                (if app.ascii { "+" } else { "✓" }, col((120, 230, 120)))
            } else if r.current == Some(t) {
                (">", col((120, 220, 255)))
            } else if r.is_available(t) {
                ("o", col((240, 240, 240)))
            } else {
                ("x", col((110, 110, 110)))
            };
            list_line(
                vec![Span::styled(
                    format!(" {mark} {:<24}", t.get().name),
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
    let mut lines = vec![header(tech.name), Line::from(tech.desc), Line::from("")];
    let packs = tech
        .packs
        .iter()
        .map(|p| p.name())
        .collect::<Vec<_>>()
        .join(" + ");
    lines.push(Line::from(format!(
        "Cost: {} units of {}",
        tech.units, packs
    )));
    lines.push(Line::from(format!(
        "{:.0}s per unit per lab",
        tech.unit_time
    )));
    lines.push(Line::from(format!(
        "Progress: {} {}/{}",
        bar(r.fraction(t), 16, app.ascii),
        r.units_done(t),
        tech.units
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
    let status = if r.is_done(t) {
        Span::styled("Researched", Style::default().fg(col((120, 230, 120))))
    } else if r.current == Some(t) {
        Span::styled(
            "In progress: feed labs with datapacks",
            Style::default().fg(col((120, 220, 255))),
        )
    } else if r.is_available(t) {
        Span::styled(
            "Press Enter to research",
            Style::default().fg(col((255, 210, 90))),
        )
    } else {
        Span::styled(
            "Locked: research prerequisites first",
            Style::default().fg(col((255, 120, 120))),
        )
    };
    lines.push(Line::from(status));
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
            if e.kind.fuel_draw() > 0.0 || e.kind == EntityKind::Generator {
                r.push(Line::from(vec![key("f"), Span::raw(" add lignite")]));
            }
            if e.kind == EntityKind::Fabricator {
                r.push(Line::from(vec![key("r"), Span::raw(" choose recipe")]));
            } else if e.kind.directional() {
                r.push(Line::from(vec![key("r"), Span::raw(" rotate")]));
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
            let list = app.recipe_choices();
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
    let mins = (game.world.tick as f32 / crate::world::TICKS_PER_SEC as f32 / 60.0).max(1.0 / 60.0);
    let mut l = vec![header("Produced (total, per min)")];
    for (it, n) in &game.world.produced {
        l.push(Line::from(vec![
            Span::raw(format!("{n:>7} ")),
            item_span(*it),
            dim(format!(" {:.0}/m", *n as f32 / mins)),
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

fn draw_victory(f: &mut Frame, game: &Game) {
    let area = centered(f.area(), 60, 12);
    f.render_widget(Clear, area);
    let block = modal_block("Transmission complete");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "★  The Uplink array is transmitting  ★",
            Style::default()
                .fg(col((255, 230, 90)))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("You won in {}.", game.play_time())),
        Line::from(format!(
            "{} buildings, {} techs researched.",
            game.world.entities.iter().flatten().count(),
            game.world.research.done.len()
        )),
        Line::from(""),
        Line::from(dim("Keep building if you like. Press Enter to continue.")),
    ];
    f.render_widget(Paragraph::new(lines).centered(), inner);
}

pub const HELP: &str = r#"FACTUIO — a terminal factory game

GOAL
  Grow from a few hand-mined rocks into an automated factory, research
  Uplink theory, build the Uplink array and feed it 20 relay modules.

MOVING AROUND
  Arrows / WASD        move the cursor (Shift or capital WASD: 8 tiles)
  The map scrolls with the cursor. The sidebar describes what's under it.

BUILDING
  1-9, 0               select a building from the hotbar (again to clear)
  b                    building list (assign hotbar slots with 1-0 there)
  Space / Enter        place the selected building (or inspect if occupied)
  r                    rotate the building you're placing (or the one under the cursor)
  v                    paint mode: moving the cursor keeps building;
                       conveyors follow the direction you move and turn corners
  q                    clear selection / pick the building under the cursor
  x / Delete           pick up a building and everything inside it
  o                    toggle the power coverage overlay

INTERACTING
  e                    inspect the building under the cursor:
                         t take items, i insert items, f add lignite,
                         r choose recipe (fabricator) or rotate, x pick up
  m                    mine the ore under the cursor by hand (keep still)
  c / i / Tab          crafting and inventory
                         Enter craft 1, f craft 5, F craft max, x cancel
  t                    research tree (Enter to pick what labs work on)
  g                    production statistics

GAME
  p pause   f fast-forward (4x)   F5 quick save   F9 quick load
  Esc menu (save, load, quit). The game autosaves every 5 minutes
  and when you press Ctrl+C.

MAP LEGEND
  :: .       ore (brighter = richer). ferrite (blue), malachite (green),
             lignite (dark), quartz (pale)
  →  ↑       conveyor; items ride on it shown by their letter
  ɑ→         arm: grabs from behind, drops ahead (holding item shown)
  D B        digger / electric bore (arrow = output side)
  K E        kiln / arc furnace     M  fabricator      C  crate
  G          generator              P  pylon           S  sun panel
  L          lab                    U  uplink array
  A bold white letter means working; red means something is wrong.

ITEMS ON BELTS
  f ferrite  m malachite  l lignite  q quartz  i iron  c copper  g glass
  s steel  * cog  ~ coil  # chip  o lens  & motor  % processor  @ relay
  1 2 3 alpha / beta / gamma datapacks   + a building

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
  mixed items to several machines.
  Fabricators craft whatever recipe you give them, with power.
  Labs consume datapacks to research the selected technology.

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
            Modal::Menu { sel: 1 },
            Modal::Stats,
            Modal::Victory,
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
    fn color_fallback_maps_to_indexed() {
        set_truecolor(false);
        assert!(matches!(col((255, 0, 0)), Color::Indexed(_)));
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
        let (x, y) = a.game.as_ref().unwrap().player.cursor;
        let shots = vec![
            ("title", None),
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
                    let b = App::new(std::env::temp_dir().join("x-none.json"), None, false);
                    t.draw(|f| draw(f, &b)).unwrap();
                }
                Some(m) => {
                    a.modal = m;
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
}
