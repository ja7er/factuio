//! Application state, keyboard and mouse handling.

use crate::game::Game;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId, TechId};
use crate::records::{Placing, Records};
use crate::scenario::{SCENARIOS, Scenario};
use crate::settings::{ACTIONS, KeyMap, Settings};
use crate::tools::{Blueprint, UndoStack, UndoStep, filter_of, recipe_of, rect};
use crate::world::State;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

/// Numbered save slots (slot 1 is the `--save` file).
pub const SAVE_SLOTS: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub enum EntitySub {
    Main,
    Insert,
    Recipe,
    Filter,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    None,
    Help {
        scroll: u16,
    },
    Craft {
        sel: usize,
    },
    Build {
        sel: usize,
    },
    Research {
        sel: usize,
    },
    Entity {
        x: i32,
        y: i32,
        sub: EntitySub,
        sel: usize,
    },
    Menu {
        sel: usize,
    },
    Stats,
    Victory,
    /// Message history.
    Log {
        scroll: u16,
    },
    /// A yes/no question before something that loses progress.
    Confirm {
        action: ConfirmAction,
    },
    /// The whole map at a glance.
    Minimap,
    /// Pick a save slot to save to or load from.
    Slots {
        save: bool,
        sel: usize,
    },
    /// Pick what to play: free play, the tutorial or a challenge.
    NewGame {
        sel: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConfirmAction {
    LoadFrom(PathBuf),
    SaveOver(PathBuf),
    QuitWithoutSaving,
}

impl ConfirmAction {
    pub fn question(&self) -> &'static str {
        match self {
            ConfirmAction::LoadFrom(_) => {
                "Load this save? Progress since your last save will be lost."
            }
            ConfirmAction::SaveOver(_) => "Save over this slot? The game in it will be replaced.",
            ConfirmAction::QuitWithoutSaving => {
                "Quit without saving? Progress since the last save will be lost."
            }
        }
    }
}

pub const MENU_ITEMS: [&str; 7] = [
    "Resume",
    "Save to slot…",
    "Load slot…",
    "Help",
    "Colours",
    "Save & quit to title",
    "Quit without saving",
];

/// A save slot as shown in the slot picker.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotInfo {
    pub label: String,
    pub path: PathBuf,
    pub exists: bool,
    /// "0:42:10 played · 5 min ago", or why it can't be read.
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Left button with a building selected: lay buildings along the path.
    Paint,
    /// Left button on empty ground: select an area.
    Area,
    /// Right button: pick up along the path.
    Remove,
}

/// Where the minimap was drawn: its area and how many tiles one cell covers (x, and y per half-cell).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MinimapGeom {
    pub area: Rect,
    pub sx: i32,
    pub sy: i32,
}

pub struct App {
    pub game: Option<Game>,
    pub modal: Modal,
    pub selected: Option<Item>,
    /// Recipe copied with `q` from a fabricator; new fabricators get it.
    pub selected_recipe: Option<RecipeId>,
    /// How the win in this session placed in the speedrun records.
    pub last_win: Option<Placing>,
    /// Filter copied with `q` from a filter arm, applied to the ones built next.
    pub selected_filter: Option<Item>,
    pub place_dir: Dir,
    pub paint: bool,
    pub paused: bool,
    pub fast: bool,
    pub ascii: bool,
    pub power_overlay: bool,
    /// The slot F5 and F9 use: the one last saved to or loaded from.
    pub save_path: PathBuf,
    /// The `--save` path: slot 1, and where the other slots, the autosave and settings live.
    pub base_save: PathBuf,
    pub seed: Option<u64>,
    /// The `--scenario` to start, if any.
    pub scenario: Option<&'static Scenario>,
    pub quit: bool,
    pub title_sel: usize,
    pub title_msg: Option<String>,
    pub last_autosave_tick: u64,
    pub settings: Settings,
    pub keymap: KeyMap,
    /// First corner of an area selection (the cursor is the other).
    pub area_anchor: Option<(i32, i32)>,
    /// A copied layout, placed with Enter.
    pub blueprint: Option<Blueprint>,
    pub undo: UndoStack,
    pub slots: Vec<SlotInfo>,
    /// Top-left tile of the map view. Kept between frames so the view only scrolls near its edges.
    pub camera: Cell<Option<(i32, i32)>>,
    /// Where the map was last drawn, for mouse clicks.
    pub map_rect: Cell<Rect>,
    /// Hotbar slot areas from the last frame.
    pub hotbar_hits: RefCell<Vec<(Rect, usize)>>,
    pub minimap_geom: Cell<MinimapGeom>,
    drag: Option<Drag>,
    /// Counts redraw ticks, paused or not, for blinking markers.
    pub frame: u64,
    /// Show research as a plain list instead of the tree (Tab switches).
    pub research_list: bool,
    /// Whether the last frame drew the research tree (it needs room), for arrow keys.
    pub tree_shown: Cell<bool>,
}

impl App {
    pub fn new(save_path: PathBuf, seed: Option<u64>, ascii: bool) -> Self {
        App {
            game: None,
            modal: Modal::None,
            selected: None,
            selected_recipe: None,
            selected_filter: None,
            last_win: None,
            place_dir: Dir::East,
            paint: false,
            paused: false,
            fast: false,
            ascii,
            power_overlay: false,
            base_save: save_path.clone(),
            save_path,
            seed,
            scenario: None,
            quit: false,
            title_sel: 0,
            title_msg: None,
            last_autosave_tick: 0,
            settings: Settings::default(),
            keymap: KeyMap::default(),
            area_anchor: None,
            blueprint: None,
            undo: UndoStack::default(),
            slots: vec![],
            camera: Cell::new(None),
            map_rect: Cell::new(Rect::default()),
            hotbar_hits: RefCell::new(vec![]),
            minimap_geom: Cell::new(MinimapGeom::default()),
            drag: None,
            frame: 0,
            research_list: false,
            tree_shown: Cell::new(false),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.base_save.with_file_name("settings.json")
    }

    pub fn records_path(&self) -> PathBuf {
        self.base_save.with_file_name("records.json")
    }

    /// Adds a win to the speedrun records and remembers how it placed for the victory screen.
    fn record_win(&mut self, scenario: &str, seed: u64, ticks: u64) {
        let path = self.records_path();
        let mut r = Records::load(&path);
        let id = if scenario == Scenario::standard().id {
            ""
        } else {
            scenario
        };
        let placing = r.add(id, seed, ticks);
        if let Err(e) = r.save(&path) {
            self.game().log(format!("Couldn't save records: {e}"));
        }
        self.last_win = Some(placing);
    }

    pub fn apply_settings(&mut self, s: Settings) {
        let (map, problems) = s.key_map();
        self.keymap = map;
        self.ascii |= s.ascii;
        crate::ui::set_colorblind(s.colorblind());
        if !problems.is_empty() {
            self.title_msg = Some(format!("settings.json: {}", problems.join("; ")));
        }
        self.settings = s;
    }

    fn base_stem(&self) -> String {
        self.base_save
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "save".into())
    }

    /// Where autosaves (and emergency saves on Ctrl+C or a closed terminal) go,
    /// so they never overwrite a save the player made on purpose.
    pub fn autosave_path(&self) -> PathBuf {
        self.base_save
            .with_file_name(format!("{}.autosave.json", self.base_stem()))
    }

    /// The numbered slots, slot 1 first.
    pub fn slot_paths(&self) -> Vec<(String, PathBuf)> {
        let stem = self.base_stem();
        let mut v = vec![("Slot 1".to_string(), self.base_save.clone())];
        for n in 2..=SAVE_SLOTS {
            v.push((
                format!("Slot {n}"),
                self.base_save.with_file_name(format!("{stem}-{n}.json")),
            ));
        }
        v
    }

    fn all_save_paths(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.slot_paths().into_iter().map(|(_, p)| p).collect();
        v.push(self.autosave_path());
        v
    }

    pub fn save_exists(&self) -> bool {
        self.all_save_paths().iter().any(|p| p.exists())
    }

    /// The most recently written save of any slot, including the autosave.
    pub fn newest_save(&self) -> PathBuf {
        let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        self.all_save_paths()
            .into_iter()
            .filter_map(|p| mtime(&p).map(|t| (t, p)))
            .max_by_key(|(t, _)| *t)
            .map(|(_, p)| p)
            .unwrap_or_else(|| self.base_save.clone())
    }

    /// Fills `slots` for the slot picker. The autosave is listed only for loading.
    pub fn refresh_slots(&mut self, for_load: bool) {
        let mut v: Vec<(String, PathBuf)> = self.slot_paths();
        if for_load {
            v.push(("Autosave".into(), self.autosave_path()));
        }
        self.slots = v
            .into_iter()
            .map(|(label, path)| slot_info(label, path))
            .collect();
    }

    pub fn title_items(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.save_exists() {
            v.push("Continue");
        }
        v.push("New game");
        if self.save_exists() {
            v.push("Load game");
        }
        v.extend(["Help", "Quit"]);
        v
    }

    /// Starts free play, or the `--scenario` given on the command line.
    pub fn new_game(&mut self) {
        let sc = self.scenario.take().unwrap_or(Scenario::standard());
        self.new_scenario(sc);
    }

    pub fn new_scenario(&mut self, sc: &'static Scenario) {
        let seed = sc.seed.or(self.seed.take()).unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
        });
        let mut g = Game::with_scenario(sc, seed);
        if sc.is_standard() {
            g.log(format!(
                "New world, seed {seed}. Ores are nearby: look around with the arrow keys."
            ));
        }
        self.start(g);
    }

    /// Fastest wins so far, by scenario id (free play is "").
    pub fn best_times(&self) -> Vec<Option<u64>> {
        let r = Records::load(&self.records_path());
        SCENARIOS
            .iter()
            .map(|s| r.best(if s.is_standard() { "" } else { s.id }))
            .collect()
    }

    /// The key bound to a remappable action, for on-screen hints.
    pub fn key_label(&self, action: &str) -> String {
        self.settings
            .keys
            .get(action)
            .cloned()
            .or_else(|| {
                ACTIONS
                    .iter()
                    .find(|(a, _)| *a == action)
                    .map(|(_, c)| c.to_string())
            })
            .unwrap_or_else(|| action.to_string())
    }

    fn start(&mut self, g: Game) {
        self.last_autosave_tick = g.world.tick;
        self.game = Some(g);
        self.modal = Modal::None;
        self.selected = None;
        self.selected_recipe = None;
        self.selected_filter = None;
        self.paint = false;
        self.paused = false;
        self.area_anchor = None;
        self.blueprint = None;
        self.undo.clear();
        self.camera.set(None);
        self.drag = None;
    }

    /// Loads `path`. Loading the autosave keeps the current slot for later saves.
    pub fn load_from(&mut self, path: PathBuf) -> Result<(), String> {
        let mut g = Game::load(&path)?;
        if path == self.autosave_path() {
            g.log("Continued from the autosave.");
        } else {
            self.save_path = path;
        }
        self.start(g);
        Ok(())
    }

    /// Loads the newest save of any slot (Continue, --continue).
    pub fn continue_game(&mut self) -> Result<(), String> {
        let path = self.newest_save();
        self.load_from(path)
    }

    pub fn save_game(&mut self) {
        let path = self.save_path.clone();
        self.save_to(path);
    }

    pub fn save_to(&mut self, path: PathBuf) {
        if let Some(g) = self.game.as_mut() {
            match g.save(&path) {
                Ok(()) => {
                    g.log(format!("Saved to {}", path.display()));
                    self.last_autosave_tick = g.world.tick;
                    self.save_path = path;
                }
                Err(e) => g.log(format!("Save failed: {e}")),
            }
        }
    }

    /// Saves to the autosave slot. Returns false if that failed.
    pub fn autosave(&mut self) -> bool {
        let path = self.autosave_path();
        let Some(g) = self.game.as_mut() else {
            return true;
        };
        match g.save(&path) {
            Ok(()) => {
                g.log("Autosaved.");
                self.last_autosave_tick = g.world.tick;
                true
            }
            Err(e) => {
                g.log(format!("Autosave failed: {e}"));
                false
            }
        }
    }

    /// Whether the simulation should advance right now.
    pub fn running(&self) -> bool {
        self.game.is_some()
            && !self.paused
            && !matches!(
                self.modal,
                Modal::Menu { .. }
                    | Modal::Help { .. }
                    | Modal::Victory
                    | Modal::Confirm { .. }
                    | Modal::Slots { .. }
            )
    }

    pub fn ticks_per_step(&self) -> u32 {
        if self.fast { 4 } else { 1 }
    }

    pub fn step(&mut self) {
        let n = self.ticks_per_step();
        let Some(g) = self.game.as_mut() else { return };
        for _ in 0..n {
            g.tick();
        }
        let won = g.victory_pending.then(|| {
            (
                g.world.scenario.id,
                g.world.seed,
                g.world.won_at.unwrap_or(g.world.tick),
            )
        });
        g.victory_pending = false;
        let tick = g.world.tick;
        if let Some((scenario, seed, ticks)) = won {
            self.modal = Modal::Victory;
            self.record_win(scenario, seed, ticks);
        }
        let tps = crate::world::TICKS_PER_SEC as u64;
        let every = self.settings.autosave_minutes * 60 * tps;
        if every > 0 && tick >= self.last_autosave_tick + every {
            self.autosave();
        }
    }

    pub fn handle_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.autosave();
            self.quit = true;
            return;
        }
        if let Modal::Slots { save, sel } = self.modal {
            self.slots_key(k, save, sel);
            return;
        }
        if self.game.is_none() {
            self.title_key(k);
            return;
        }
        match self.modal.clone() {
            Modal::None => self.play_key(k),
            Modal::Help { scroll } => match k.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => {
                    self.modal = Modal::Help {
                        scroll: scroll.saturating_sub(1),
                    }
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => {
                    self.modal = Modal::Help { scroll: scroll + 1 }
                }
                KeyCode::PageUp => {
                    self.modal = Modal::Help {
                        scroll: scroll.saturating_sub(10),
                    }
                }
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    self.modal = Modal::Help {
                        scroll: scroll + 10,
                    }
                }
                _ => self.modal = Modal::None,
            },
            Modal::Craft { sel } => self.craft_key(k, sel),
            Modal::Build { sel } => self.build_key(k, sel),
            Modal::Research { sel } => self.research_key(k, sel),
            Modal::Entity { x, y, sub, sel } => self.entity_key(k, x, y, sub, sel),
            Modal::Menu { sel } => self.menu_key(k, sel),
            Modal::Stats => {
                if is_close(k.code, &['g']) || k.code == KeyCode::Enter {
                    self.modal = Modal::None;
                }
            }
            Modal::Victory => {
                if matches!(k.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ')) {
                    self.modal = Modal::None;
                }
            }
            Modal::Log { scroll } => match k.code {
                KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => {
                    self.modal = Modal::Log {
                        scroll: scroll.saturating_add(1),
                    }
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => {
                    self.modal = Modal::Log {
                        scroll: scroll.saturating_sub(1),
                    }
                }
                KeyCode::PageUp => {
                    self.modal = Modal::Log {
                        scroll: scroll.saturating_add(10),
                    }
                }
                KeyCode::PageDown => {
                    self.modal = Modal::Log {
                        scroll: scroll.saturating_sub(10),
                    }
                }
                c if is_close(c, &['l']) => self.modal = Modal::None,
                _ => {}
            },
            Modal::Confirm { action } => match k.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.modal = Modal::None;
                    match action {
                        ConfirmAction::LoadFrom(path) => {
                            if let Err(e) = self.load_from(path) {
                                self.game().log(e);
                            }
                        }
                        ConfirmAction::SaveOver(path) => self.save_to(path),
                        ConfirmAction::QuitWithoutSaving => self.quit = true,
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = Modal::None
                }
                _ => {}
            },
            Modal::Minimap => self.minimap_key(k),
            Modal::Slots { .. } | Modal::NewGame { .. } => self.modal = Modal::None,
        }
    }

    fn new_game_key(&mut self, k: KeyEvent, sel: usize) {
        let n = SCENARIOS.len();
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::NewGame {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::NewGame { sel: (sel + 1) % n }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.modal = Modal::None;
                self.new_scenario(&SCENARIOS[sel.min(n - 1)]);
            }
            c if is_close(c, &['n']) => self.modal = Modal::None,
            _ => {}
        }
    }

    fn title_key(&mut self, k: KeyEvent) {
        let items = self.title_items();
        if let Modal::NewGame { sel } = self.modal {
            self.new_game_key(k, sel);
            return;
        }
        if let Modal::Help { scroll } = self.modal {
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.modal = Modal::Help { scroll: scroll + 1 }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.modal = Modal::Help {
                        scroll: scroll.saturating_sub(1),
                    }
                }
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    self.modal = Modal::Help {
                        scroll: scroll + 10,
                    }
                }
                KeyCode::PageUp => {
                    self.modal = Modal::Help {
                        scroll: scroll.saturating_sub(10),
                    }
                }
                _ => self.modal = Modal::None,
            }
            return;
        }
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.title_sel = (self.title_sel + items.len() - 1) % items.len()
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.title_sel = (self.title_sel + 1) % items.len()
            }
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('n') => self.modal = Modal::NewGame { sel: 0 },
            KeyCode::Char('?') => self.modal = Modal::Help { scroll: 0 },
            KeyCode::Enter | KeyCode::Char(' ') => match items[self.title_sel.min(items.len() - 1)]
            {
                "Continue" => {
                    if let Err(e) = self.continue_game() {
                        self.title_msg = Some(e);
                    }
                }
                "New game" => self.modal = Modal::NewGame { sel: 0 },
                "Load game" => self.open_slots(false),
                "Help" => self.modal = Modal::Help { scroll: 0 },
                _ => self.quit = true,
            },
            _ => {}
        }
    }

    fn open_slots(&mut self, save: bool) {
        self.refresh_slots(!save);
        let sel = self
            .slots
            .iter()
            .position(|s| s.path == self.save_path)
            .unwrap_or(0);
        self.modal = Modal::Slots { save, sel };
    }

    fn slots_key(&mut self, k: KeyEvent, save: bool, sel: usize) {
        let n = self.slots.len().max(1);
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Slots {
                    save,
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Slots {
                    save,
                    sel: (sel + 1) % n,
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let Some(slot) = self.slots.get(sel).cloned() else {
                    return;
                };
                self.modal = Modal::None;
                if save {
                    if slot.exists && slot.path != self.save_path {
                        self.modal = Modal::Confirm {
                            action: ConfirmAction::SaveOver(slot.path),
                        };
                    } else {
                        self.save_to(slot.path);
                    }
                } else if !slot.exists {
                    self.modal = Modal::Slots { save, sel };
                } else if self.game.is_some() {
                    self.modal = Modal::Confirm {
                        action: ConfirmAction::LoadFrom(slot.path),
                    };
                } else if let Err(e) = self.load_from(slot.path) {
                    self.title_msg = Some(e);
                }
            }
            c if is_close(c, &[]) => self.modal = Modal::None,
            _ => {}
        }
    }

    fn game(&mut self) -> &mut Game {
        self.game.as_mut().expect("game running")
    }

    fn cursor(&self) -> (i32, i32) {
        self.game
            .as_ref()
            .map(|g| g.player.cursor)
            .unwrap_or((0, 0))
    }

    /// Moves the cursor somewhere far away and recentres the view on it.
    fn jump_to(&mut self, x: i32, y: i32) {
        self.game().player.cursor = (x, y);
        self.camera.set(None);
    }

    /// Places `item`, or swaps it for a building of the same family that is already there.
    /// Records undo steps. Returns true if anything changed.
    fn build_at(&mut self, x: i32, y: i32, item: Item, dir: Dir, quiet: bool) -> bool {
        if item == Item::Cart {
            let g = self.game.as_mut().unwrap();
            return match g.add_cart(x, y) {
                Ok(()) => {
                    self.undo.push(UndoStep::CartAdded { x, y });
                    true
                }
                Err(e) => {
                    if !quiet {
                        g.log(e);
                    }
                    false
                }
            };
        }
        if item == Item::Landfill {
            let g = self.game.as_mut().unwrap();
            return match g.landfill(x, y) {
                Ok(()) => {
                    self.undo.push(UndoStep::Filled { x, y });
                    true
                }
                Err(e) => {
                    if !quiet {
                        g.log(e);
                    }
                    false
                }
            };
        }
        let Some(kind) = item.building() else {
            return false;
        };
        let (recipe, filter) = (self.selected_recipe, self.selected_filter);
        let g = self.game.as_mut().unwrap();
        let existing = g.world.entity(x, y).map(|e| (e.kind, e.dir));
        match existing {
            None => match g.place(x, y, item, dir) {
                Ok(kind) => {
                    if kind == EntityKind::Fabricator && recipe.is_some() {
                        let _ = g.set_recipe(x, y, recipe);
                    }
                    if kind == EntityKind::FilterArm && filter.is_some() {
                        let _ = g.set_filter(x, y, filter);
                    }
                    self.undo.push(UndoStep::Placed { x, y, kind });
                    true
                }
                Err(e) => {
                    if !quiet {
                        g.log(e);
                    }
                    false
                }
            },
            Some((k, d)) if k.item() == item => {
                if kind.is_transport() && d != dir {
                    // Building a belt over the same belt turns it.
                    g.world.entity_mut(x, y).unwrap().dir = dir;
                    g.world.mark_dirty();
                    true
                } else {
                    false
                }
            }
            Some((k, _)) if k.family().is_some() && k.family() == kind.family() => {
                match g.replace(x, y, item, dir) {
                    Ok(old) => {
                        let kind = g.world.entity(x, y).map_or(kind, |e| e.kind);
                        if !quiet {
                            g.log(format!("Swapped {} for {}", old.kind.name(), kind.name()));
                        }
                        let steps = vec![
                            UndoStep::removed(x, y, &old),
                            UndoStep::Placed { x, y, kind },
                        ];
                        if self.undo.open {
                            for s in steps {
                                self.undo.push(s);
                            }
                        } else {
                            self.undo.push_group(steps);
                        }
                        true
                    }
                    Err(e) => {
                        if !quiet {
                            g.log(e);
                        }
                        false
                    }
                }
            }
            Some(_) => false,
        }
    }

    /// Picks up the building at (x, y), recording an undo step. A single pick-up (not `quiet`)
    /// on a tile with rail carts takes just the carts, so the track stays.
    fn remove_at(&mut self, x: i32, y: i32, quiet: bool) -> bool {
        let g = self.game.as_mut().unwrap();
        let carts = g.world.idx(x, y).map_or(0, |i| g.world.carts_at(i).count());
        if carts > 0 && !quiet {
            let n = g.take_carts(x, y, usize::MAX);
            g.log(format!(
                "Picked up {n} rail cart{} with {}",
                if n == 1 { "" } else { "s" },
                if n == 1 { "its cargo" } else { "their cargo" }
            ));
            self.undo.push(UndoStep::CartsTaken { x, y, n });
            return true;
        }
        let Some(step) = g.world.entity(x, y).map(|e| UndoStep::removed(x, y, e)) else {
            return false;
        };
        match g.remove(x, y) {
            Ok(kind) => {
                if !quiet {
                    g.log(format!("Picked up {}", kind.name()));
                }
                let steps = if carts > 0 {
                    vec![UndoStep::CartsTaken { x, y, n: carts }, step]
                } else {
                    vec![step]
                };
                if self.undo.open {
                    for s in steps {
                        self.undo.push(s);
                    }
                } else {
                    self.undo.push_group(steps);
                }
                true
            }
            Err(e) => {
                if !quiet {
                    g.log(e);
                }
                false
            }
        }
    }

    fn move_cursor(&mut self, dx: i32, dy: i32, steps: i32) {
        let dir = Dir::from_delta(dx, dy).unwrap();
        for _ in 0..steps {
            let g = self.game.as_mut().unwrap();
            let (ox, oy) = g.player.cursor;
            let (nx, ny) = (
                (ox + dx).clamp(0, g.world.width - 1),
                (oy + dy).clamp(0, g.world.height - 1),
            );
            if (nx, ny) == (ox, oy) {
                break;
            }
            g.player.cursor = (nx, ny);
            if self.paint
                && let Some(item) = self.selected
            {
                let is_belt = item.building().is_some_and(|k| k.is_transport());
                if is_belt {
                    self.place_dir = dir;
                    let g = self.game.as_mut().unwrap();
                    if let Some(e) = g.world.entity_mut(ox, oy)
                        && e.kind.is_belt()
                        && e.dir != dir
                    {
                        e.dir = dir;
                        g.world.mark_dirty();
                    }
                }
                let pd = self.place_dir;
                let occupied = self.game().world.entity(nx, ny).is_some();
                let built = self.build_at(nx, ny, item, pd, occupied);
                let g = self.game.as_mut().unwrap();
                if !built && !occupied && g.player.inventory.count(item) == 0 {
                    self.paint = false;
                    self.undo.close();
                }
            }
        }
    }

    fn set_paint(&mut self, on: bool) {
        if on == self.paint {
            return;
        }
        self.paint = on;
        if on {
            self.undo.begin();
        } else {
            self.undo.close();
        }
    }

    fn clear_tools(&mut self) {
        self.selected = None;
        self.selected_recipe = None;
        self.selected_filter = None;
        self.blueprint = None;
        self.area_anchor = None;
        self.set_paint(false);
    }

    fn undo_last(&mut self) {
        let Some(steps) = self.undo.pop() else {
            self.game().log("Nothing to undo.");
            return;
        };
        let g = self.game.as_mut().unwrap();
        let n = steps.len();
        let ok = steps.iter().rev().filter(|s| s.undo(g)).count();
        if ok == n {
            g.log(format!(
                "Undid {n} change{}.",
                if n == 1 { "" } else { "s" }
            ));
        } else {
            g.log(format!(
                "Undid {ok} of {n} changes (the rest no longer apply or need buildings you don't have)."
            ));
        }
    }

    /// Picks up everything in the selected area as one undoable action.
    fn remove_area(&mut self) {
        let Some(a) = self.area_anchor.take() else {
            return;
        };
        let (x0, y0, x1, y1) = rect(a, self.cursor());
        self.undo.begin();
        let mut n = 0;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.remove_at(x, y, true) {
                    n += 1;
                }
            }
        }
        self.undo.close();
        self.game().log(format!("Picked up {n} buildings."));
    }

    fn copy_area(&mut self) {
        let Some(a) = self.area_anchor.take() else {
            return;
        };
        let cur = self.cursor();
        let g = self.game.as_mut().unwrap();
        match Blueprint::copy(g, a, cur) {
            Some(bp) => {
                g.log(format!(
                    "Copied {} buildings. Enter places them at the cursor, r rotates, Esc drops.",
                    bp.cells.len()
                ));
                let (x0, y0, _, _) = rect(a, cur);
                g.player.cursor = (x0, y0);
                self.selected = None;
                self.set_paint(false);
                self.blueprint = Some(bp);
            }
            None => g.log("Nothing to copy there."),
        }
    }

    fn paste_blueprint(&mut self) {
        let Some(bp) = self.blueprint.clone() else {
            return;
        };
        let (x, y) = self.cursor();
        let g = self.game.as_mut().unwrap();
        let (placed, skipped, steps) = bp.paste(g, x, y);
        if skipped > 0 {
            g.log(format!(
                "Placed {placed}; {skipped} couldn't be placed (blocked, or not in your inventory)."
            ));
        } else {
            g.log(format!("Placed {placed} buildings."));
        }
        self.undo.push_group(steps);
    }

    fn next_alert(&mut self) {
        let g = self.game.as_ref().unwrap();
        let probs = g.problems();
        if probs.is_empty() {
            self.game().log("No problems right now.");
            return;
        }
        let (cx, cy) = g.player.cursor;
        let here = cy * g.world.width + cx;
        let k = probs
            .iter()
            .position(|&(x, y, _, _)| y * g.world.width + x > here)
            .unwrap_or(0);
        let (x, y, kind, status) = probs[k];
        self.jump_to(x, y);
        self.game().log(format!(
            "{} at ({x}, {y}): {} [{}/{}]",
            kind.name(),
            status.label(),
            k + 1,
            probs.len()
        ));
    }

    fn play_key(&mut self, k: KeyEvent) {
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let big = if shift { 8 } else { 1 };
        // Remapped action keys from settings.json.
        let code = match k.code {
            KeyCode::Char(c) => match self.keymap.translate(c) {
                Some(t) => KeyCode::Char(t),
                None => return,
            },
            other => other,
        };
        // Area selection takes a few keys of its own.
        if self.area_anchor.is_some() {
            match code {
                KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                    return self.remove_area();
                }
                KeyCode::Char('y') | KeyCode::Char('c') => return self.copy_area(),
                KeyCode::Esc | KeyCode::Char('z') => {
                    self.area_anchor = None;
                    return;
                }
                _ => {}
            }
        }
        match code {
            KeyCode::Up => self.move_cursor(0, -1, big),
            KeyCode::Down => self.move_cursor(0, 1, big),
            KeyCode::Left => self.move_cursor(-1, 0, big),
            KeyCode::Right => self.move_cursor(1, 0, big),
            KeyCode::Char('w') => self.move_cursor(0, -1, 1),
            KeyCode::Char('s') => self.move_cursor(0, 1, 1),
            KeyCode::Char('a') => self.move_cursor(-1, 0, 1),
            KeyCode::Char('d') => self.move_cursor(1, 0, 1),
            KeyCode::Char('W') => self.move_cursor(0, -1, 8),
            KeyCode::Char('S') => self.move_cursor(0, 1, 8),
            KeyCode::Char('A') => self.move_cursor(-1, 0, 8),
            KeyCode::Char('D') => self.move_cursor(1, 0, 8),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let (x, y) = self.cursor();
                if self.blueprint.is_some() {
                    self.paste_blueprint();
                } else if let Some(item) = self.selected {
                    let d = self.place_dir;
                    let occupied = self.game().world.entity(x, y).is_some();
                    if !self.build_at(x, y, item, d, occupied) && occupied {
                        self.open_entity();
                    }
                } else {
                    self.open_entity();
                }
            }
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                let (x, y) = self.cursor();
                self.remove_at(x, y, false);
            }
            KeyCode::Char('r') => {
                if let Some(bp) = self.blueprint.as_mut() {
                    bp.rotate();
                } else if self.selected.is_some() {
                    self.place_dir = self.place_dir.cw();
                } else {
                    let (x, y) = self.cursor();
                    self.game().world.rotate(x, y);
                }
            }
            KeyCode::Char('R') => {
                let (x, y) = self.cursor();
                self.game().world.rotate(x, y);
            }
            KeyCode::Char('q') => {
                if self.selected.is_some() || self.blueprint.is_some() {
                    self.clear_tools();
                } else {
                    let (x, y) = self.cursor();
                    if let Some((kind, dir, recipe, filter)) = self
                        .game()
                        .world
                        .entity(x, y)
                        .map(|e| (e.kind, e.dir, recipe_of(&e.state), filter_of(&e.state)))
                    {
                        self.selected = Some(kind.item());
                        self.selected_recipe = recipe;
                        self.selected_filter = filter;
                        if kind.directional() {
                            self.place_dir = dir;
                        }
                    }
                }
            }
            KeyCode::Char('v') => {
                if self.selected.is_none() {
                    self.game()
                        .log("Select a building first (number keys or b), then press v to paint.");
                } else {
                    let on = !self.paint;
                    self.set_paint(on);
                    if on {
                        let (x, y) = self.cursor();
                        let (item, d) = (self.selected.unwrap(), self.place_dir);
                        if self.game().world.entity(x, y).is_none() {
                            self.build_at(x, y, item, d, true);
                        }
                        self.game()
                            .log("Paint mode: moving the cursor builds. Press v to stop.");
                    }
                }
            }
            KeyCode::Char('z') => {
                self.blueprint = None;
                self.set_paint(false);
                self.area_anchor = Some(self.cursor());
                self.game().log(
                    "Area select: move to the other corner, then x to pick up or y to copy. Esc cancels.",
                );
            }
            KeyCode::Char('u') => self.undo_last(),
            KeyCode::Char('n') => self.next_alert(),
            KeyCode::Char('M') => self.modal = Modal::Minimap,
            KeyCode::Char(c @ '0'..='9') => {
                let slot = if c == '0' {
                    9
                } else {
                    c as usize - '1' as usize
                };
                self.select_hotbar(slot);
            }
            KeyCode::Char('m') => match self.game().toggle_mining() {
                Ok(true) => self.game().log("Mining by hand. Keep the cursor still."),
                Ok(false) => self.game().log("Stopped mining."),
                Err(e) => self.game().log(e),
            },
            KeyCode::Char('e') => self.open_entity(),
            KeyCode::Char('b') => self.modal = Modal::Build { sel: 0 },
            KeyCode::Char('c') | KeyCode::Char('i') | KeyCode::Tab => {
                self.modal = Modal::Craft { sel: 0 }
            }
            KeyCode::Char('t') => {
                let r = &self.game().world.research;
                let sel = r
                    .current
                    .or_else(|| TechId::all().find(|&t| r.is_available(t)))
                    .map(|t| t.0 as usize)
                    .unwrap_or(0);
                self.modal = Modal::Research { sel };
            }
            KeyCode::Char('g') => self.modal = Modal::Stats,
            KeyCode::Char('l') => self.modal = Modal::Log { scroll: 0 },
            KeyCode::Char('o') => self.power_overlay = !self.power_overlay,
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('f') => self.fast = !self.fast,
            KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::F(1) => {
                self.modal = Modal::Help { scroll: 0 }
            }
            KeyCode::F(5) => self.save_game(),
            KeyCode::F(9) => {
                if self.save_path.exists() {
                    self.modal = Modal::Confirm {
                        action: ConfirmAction::LoadFrom(self.save_path.clone()),
                    };
                } else {
                    self.game()
                        .log("No save in this slot yet: press F5 to save first.");
                }
            }
            KeyCode::Esc => {
                if self.paint {
                    self.set_paint(false);
                } else if self.selected.is_some() || self.blueprint.is_some() {
                    self.clear_tools();
                } else {
                    self.modal = Modal::Menu { sel: 0 };
                }
            }
            _ => {}
        }
    }

    fn select_hotbar(&mut self, slot: usize) {
        let item = self.game().player.hotbar[slot];
        self.blueprint = None;
        self.selected_recipe = None;
        self.selected_filter = None;
        if self.selected == item {
            self.selected = None;
            self.set_paint(false);
        } else {
            self.selected = item;
        }
    }

    fn minimap_key(&mut self, k: KeyEvent) {
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let step = if shift { 16 } else { 4 };
        let (dx, dy) = match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => (0, -1),
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => (0, 1),
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => (-1, 0),
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') => (1, 0),
            KeyCode::Char('W') => (0, -4),
            KeyCode::Char('S') => (0, 4),
            KeyCode::Char('A') => (-4, 0),
            KeyCode::Char('D') => (4, 0),
            c if is_close(c, &['M']) || c == KeyCode::Enter => {
                self.modal = Modal::None;
                self.camera.set(None);
                return;
            }
            _ => return,
        };
        let g = self.game();
        let (x, y) = g.player.cursor;
        g.player.cursor = (
            (x + dx * step).clamp(0, g.world.width - 1),
            (y + dy * step).clamp(0, g.world.height - 1),
        );
    }

    // ---- Mouse ----

    /// The map tile under a screen cell, if the map was drawn there.
    pub fn tile_at(&self, col: u16, row: u16) -> Option<(i32, i32)> {
        let r = self.map_rect.get();
        if col < r.x || row < r.y || col >= r.x + r.width / 2 * 2 || row >= r.y + r.height {
            return None;
        }
        let (cx, cy) = self.camera.get()?;
        let (x, y) = (cx + ((col - r.x) / 2) as i32, cy + (row - r.y) as i32);
        let w = &self.game.as_ref()?.world;
        w.idx(x, y).map(|_| (x, y))
    }

    pub fn handle_mouse(&mut self, m: MouseEvent) {
        if self.game.is_none() {
            return;
        }
        match self.modal {
            Modal::None => self.map_mouse(m),
            Modal::Minimap => self.minimap_mouse(m),
            _ => {
                // In menus and lists the wheel scrolls and a right click backs out.
                let key = match m.kind {
                    MouseEventKind::ScrollUp => KeyCode::Up,
                    MouseEventKind::ScrollDown => KeyCode::Down,
                    MouseEventKind::Down(MouseButton::Right) => KeyCode::Esc,
                    _ => return,
                };
                self.handle_key(plain_key(key));
            }
        }
    }

    fn minimap_mouse(&mut self, m: MouseEvent) {
        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            let geo = self.minimap_geom.get();
            let a = geo.area;
            if m.column >= a.x && m.row >= a.y && m.column < a.x + a.width && m.row < a.y + a.height
            {
                let x = (m.column - a.x) as i32 * geo.sx;
                let y = (m.row - a.y) as i32 * geo.sy * 2;
                let g = self.game();
                let (w, h) = (g.world.width, g.world.height);
                self.jump_to(x.min(w - 1), y.min(h - 1));
                self.modal = Modal::None;
            }
        } else if let MouseEventKind::Down(MouseButton::Right) = m.kind {
            self.modal = Modal::None;
        }
    }

    fn map_mouse(&mut self, m: MouseEvent) {
        let shift = m.modifiers.contains(KeyModifiers::SHIFT);
        match m.kind {
            MouseEventKind::ScrollUp if shift => self.move_cursor(-1, 0, 3),
            MouseEventKind::ScrollDown if shift => self.move_cursor(1, 0, 3),
            MouseEventKind::ScrollUp => self.move_cursor(0, -1, 3),
            MouseEventKind::ScrollDown => self.move_cursor(0, 1, 3),
            MouseEventKind::ScrollLeft => self.move_cursor(-1, 0, 3),
            MouseEventKind::ScrollRight => self.move_cursor(1, 0, 3),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self
                    .hotbar_hits
                    .borrow()
                    .iter()
                    .find(|(r, _)| {
                        m.column >= r.x
                            && m.column < r.x + r.width
                            && m.row >= r.y
                            && m.row < r.y + r.height
                    })
                    .map(|&(_, s)| s);
                if let Some(slot) = hit {
                    return self.select_hotbar(slot);
                }
                let Some((x, y)) = self.tile_at(m.column, m.row) else {
                    return;
                };
                self.game().player.cursor = (x, y);
                if self.area_anchor.is_some() {
                    self.area_anchor = None;
                }
                if self.blueprint.is_some() {
                    self.paste_blueprint();
                } else if let Some(item) = self.selected {
                    let occupied = self.game().world.entity(x, y).is_some();
                    self.undo.begin();
                    let built = self.build_at(x, y, item, self.place_dir, occupied);
                    if !built && occupied && !self.paint {
                        self.undo.close();
                        self.open_entity();
                    } else {
                        self.drag = Some(Drag::Paint);
                    }
                } else if self.game().world.entity(x, y).is_some() {
                    self.open_entity();
                } else {
                    self.area_anchor = Some((x, y));
                    self.drag = Some(Drag::Area);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if self.selected.is_some() || self.blueprint.is_some() {
                    // Like Esc: put the tool away first.
                    self.clear_tools();
                    return;
                }
                let Some((x, y)) = self.tile_at(m.column, m.row) else {
                    return;
                };
                self.game().player.cursor = (x, y);
                self.undo.begin();
                self.remove_at(x, y, false);
                self.drag = Some(Drag::Remove);
            }
            MouseEventKind::Drag(_) => {
                let Some(target) = self.tile_at(m.column, m.row) else {
                    return;
                };
                match self.drag {
                    Some(Drag::Paint) => self.walk_to(target, true),
                    Some(Drag::Remove) => self.walk_to(target, false),
                    Some(Drag::Area) => self.game().player.cursor = target,
                    None => {}
                }
            }
            MouseEventKind::Up(_) => {
                match self.drag.take() {
                    Some(Drag::Paint) | Some(Drag::Remove) => self.undo.close(),
                    Some(Drag::Area) => {
                        if self.area_anchor == Some(self.cursor()) {
                            // A plain click on empty ground just moves the cursor.
                            self.area_anchor = None;
                        } else {
                            self.game()
                                .log("Area selected: x picks up, y copies, Esc cancels.");
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Steps the cursor towards `target` one tile at a time, building (paint) or picking up.
    fn walk_to(&mut self, target: (i32, i32), build: bool) {
        let was_paint = self.paint;
        for _ in 0..512 {
            let (cx, cy) = self.cursor();
            if (cx, cy) == target {
                break;
            }
            let (dx, dy) = if cx != target.0 {
                ((target.0 - cx).signum(), 0)
            } else {
                (0, (target.1 - cy).signum())
            };
            if build {
                // Reuse paint mode so belts follow the path and turn corners.
                self.paint = true;
                self.move_cursor(dx, dy, 1);
            } else {
                self.move_cursor(dx, dy, 1);
                let (x, y) = self.cursor();
                self.remove_at(x, y, true);
            }
        }
        self.paint = was_paint;
    }

    fn open_entity(&mut self) {
        let (x, y) = self.cursor();
        if self.game().world.entity(x, y).is_some() {
            self.modal = Modal::Entity {
                x,
                y,
                sub: EntitySub::Main,
                sel: 0,
            };
        }
    }

    /// Hand-craftable recipes shown in the crafting screen, unlocked first.
    pub fn craft_list(&self) -> Vec<RecipeId> {
        let r = &self.game.as_ref().unwrap().world.research;
        let mut v: Vec<RecipeId> = RecipeId::all()
            .filter(|r| r.get().category == Category::Craft)
            .collect();
        v.sort_by_key(|&id| !r.is_unlocked(id));
        v
    }

    fn craft_key(&mut self, k: KeyEvent, sel: usize) {
        let list = self.craft_list();
        let n = list.len();
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Craft {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Craft { sel: (sel + 1) % n }
            }
            KeyCode::Enter
            | KeyCode::Char(' ')
            | KeyCode::Char('+')
            | KeyCode::Char('f')
            | KeyCode::Char('F') => {
                let count = match k.code {
                    KeyCode::Char('+') | KeyCode::Char('f') => 5,
                    KeyCode::Char('F') => 99,
                    _ => 1,
                };
                let r = list[sel.min(n - 1)];
                let g = self.game();
                match g.queue_craft(r, count) {
                    Ok(q) => g.log(format!("Queued {q} x {}", r.get().name())),
                    Err(e) => g.log(e),
                }
            }
            KeyCode::Char('x') => {
                self.game().cancel_last_craft();
            }
            c if is_close(c, &['c', 'i']) || c == KeyCode::Tab => self.modal = Modal::None,
            _ => {}
        }
    }

    fn build_key(&mut self, k: KeyEvent, sel: usize) {
        let items: Vec<Item> = Item::placeable().collect();
        let n = items.len();
        let sel = sel.min(n - 1);
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Build {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Build { sel: (sel + 1) % n }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.selected = Some(items[sel]);
                self.selected_recipe = None;
                self.selected_filter = None;
                self.modal = Modal::None;
            }
            KeyCode::Char(c @ '0'..='9') => {
                let slot = if c == '0' {
                    9
                } else {
                    c as usize - '1' as usize
                };
                let item = items[sel];
                let g = self.game();
                g.player.hotbar[slot] = Some(item);
                g.log(format!("{} assigned to hotbar slot {c}", item.name()));
            }
            c if is_close(c, &['b']) => self.modal = Modal::None,
            _ => {}
        }
    }

    fn research_key(&mut self, k: KeyEvent, sel: usize) {
        let n = TechId::all().count();
        if self.tree_shown.get() {
            let tiers = crate::research::tech_tiers();
            let (c, r) = crate::research::tree_pos(&tiers, TechId(sel as u16));
            let to = match k.code {
                KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                    Some((c, (r + tiers[c].len() - 1) % tiers[c].len()))
                }
                KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                    Some((c, (r + 1) % tiers[c].len()))
                }
                KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => {
                    let c = (c + tiers.len() - 1) % tiers.len();
                    Some((c, r.min(tiers[c].len() - 1)))
                }
                KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') => {
                    let c = (c + 1) % tiers.len();
                    Some((c, r.min(tiers[c].len() - 1)))
                }
                _ => None,
            };
            if let Some((c, r)) = to {
                self.modal = Modal::Research {
                    sel: tiers[c][r].0 as usize,
                };
                return;
            }
        }
        match k.code {
            KeyCode::Tab => self.research_list = !self.research_list,
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Research {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Research { sel: (sel + 1) % n }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Research it now if nothing is running, otherwise add it (and anything it
                // needs) to the queue. Pressing it again on a queued tech takes it back out.
                let t = TechId(sel as u16);
                let r = &mut self.game().world.research;
                let msg = if r.current == Some(t) {
                    format!("Already researching {}", t.get().name)
                } else if r.unqueue(t) {
                    format!("Removed {} from the queue", t.get().name)
                } else {
                    let was_idle = r.current.is_none();
                    match r.enqueue(t) {
                        Ok(_) if was_idle => match r.current {
                            Some(c) if c == t => format!("Researching {}", t.get().name),
                            Some(c) => format!(
                                "Researching {} first, then the rest of the way to {}",
                                c.get().name,
                                t.get().name
                            ),
                            None => format!("Queued {}", t.get().name),
                        },
                        Ok(n) => format!(
                            "Queued {}{}",
                            t.get().name,
                            if n > 1 {
                                format!(" with {} prerequisite(s)", n - 1)
                            } else {
                                String::new()
                            }
                        ),
                        Err(e) => e,
                    }
                };
                self.game().log(msg);
            }
            KeyCode::Char('n') => {
                // Switch right now; the current tech keeps its progress and goes back in line.
                let t = TechId(sel as u16);
                let g = self.game();
                match g.start_research(t) {
                    Ok(()) => g.log(format!("Switched to {}", t.get().name)),
                    Err(e) => g.log(e),
                }
            }
            KeyCode::Char('x') => {
                let g = self.game();
                g.world.research.queue.clear();
                g.log("Research queue cleared.");
            }
            c if is_close(c, &['t']) => self.modal = Modal::None,
            _ => {}
        }
    }

    /// Recipes a fabricator or chem plant can be set to, after "none".
    pub fn recipe_choices(&self, kind: EntityKind) -> Vec<Option<RecipeId>> {
        let r = &self.game.as_ref().unwrap().world.research;
        let cat = kind.recipe_category();
        let mut v = vec![None];
        v.extend(
            RecipeId::all()
                .filter(|&id| Some(id.get().category) == cat && r.is_unlocked(id))
                .map(Some),
        );
        v
    }

    fn entity_key(&mut self, k: KeyEvent, x: i32, y: i32, sub: EntitySub, sel: usize) {
        let Some(e) = self.game().world.entity(x, y) else {
            self.modal = Modal::None;
            return;
        };
        let kind = e.kind;
        let set = |sub, sel| Modal::Entity { x, y, sub, sel };
        match sub {
            EntitySub::Main => match k.code {
                KeyCode::Char('t') => {
                    let n = self.game().take_output(x, y);
                    self.game().log(format!("Took {n} items"));
                }
                KeyCode::Char('i') => self.modal = set(EntitySub::Insert, 0),
                KeyCode::Char('f') if kind == EntityKind::FilterArm => {
                    let cur = filter_of(&self.game().world.entity(x, y).unwrap().state);
                    let pos = filter_choices().iter().position(|c| *c == cur).unwrap_or(0);
                    self.modal = set(EntitySub::Filter, pos);
                }
                KeyCode::Char('v') if kind.is_tunnel() => {
                    let g = self.game();
                    g.world.flip_tunnel(x, y);
                    let now = g.world.entity(x, y).map_or(kind, |e| e.kind);
                    g.log(format!("Now a {}", now.name().to_lowercase()));
                }
                KeyCode::Char('v') if kind == EntityKind::Depot => {
                    let g = self.game();
                    if let Some(unload) = g.world.toggle_depot(x, y) {
                        g.log(if unload {
                            "Carts now unload at this depot"
                        } else {
                            "Carts now load at this depot"
                        });
                    }
                }
                KeyCode::Char('f') => match self.game().add_fuel(x, y) {
                    Some((f, n)) => {
                        let what = f.name().to_lowercase();
                        self.game().log(format!("Added {n} {what}"));
                    }
                    None => self
                        .game()
                        .log("No fuel added (none in inventory, full, or no fuel slot)"),
                },
                KeyCode::Char('m') if kind.module_slots() > 0 => {
                    let n = self.game().take_modules(x, y);
                    self.game().log(format!("Took out {n} modules"));
                }
                KeyCode::Char('r') if kind.recipe_category().is_some() => {
                    let cur = match &self.game().world.entity(x, y).unwrap().state {
                        State::Assembler { recipe, .. } => *recipe,
                        _ => None,
                    };
                    let pos = self
                        .recipe_choices(kind)
                        .iter()
                        .position(|c| *c == cur)
                        .unwrap_or(0);
                    self.modal = set(EntitySub::Recipe, pos);
                }
                KeyCode::Char('r') => {
                    self.game().world.rotate(x, y);
                }
                KeyCode::Char('x') => {
                    if let Ok(kind) = self.game().remove(x, y) {
                        self.game().log(format!("Picked up {}", kind.name()));
                    }
                    self.modal = Modal::None;
                }
                c if is_close(c, &['e']) || c == KeyCode::Enter => self.modal = Modal::None,
                _ => {}
            },
            EntitySub::Insert => {
                let list = self.game().insertable(x, y);
                let n = list.len().max(1);
                match k.code {
                    KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                        self.modal = set(sub, (sel + n - 1) % n)
                    }
                    KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                        self.modal = set(sub, (sel + 1) % n)
                    }
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('1') => {
                        if let Some(&(item, have)) = list.get(sel) {
                            let amount = if k.code == KeyCode::Char('1') {
                                1
                            } else {
                                have
                            };
                            let put = self.game().insert(x, y, item, amount);
                            self.game().log(format!("Inserted {put} {}", item.name()));
                            let left = self.game().insertable(x, y).len();
                            if left == 0 {
                                self.modal = set(EntitySub::Main, 0);
                            } else {
                                self.modal = set(sub, sel.min(left - 1));
                            }
                        }
                    }
                    c if is_close(c, &['i']) => self.modal = set(EntitySub::Main, 0),
                    _ => {}
                }
            }
            EntitySub::Recipe => {
                let list = self.recipe_choices(kind);
                let n = list.len();
                match k.code {
                    KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                        self.modal = set(sub, (sel + n - 1) % n)
                    }
                    KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                        self.modal = set(sub, (sel + 1) % n)
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let r = list[sel.min(n - 1)];
                        if let Err(e) = self.game().set_recipe(x, y, r) {
                            self.game().log(e);
                        }
                        self.modal = set(EntitySub::Main, 0);
                    }
                    c if is_close(c, &['r']) => self.modal = set(EntitySub::Main, 0),
                    _ => {}
                }
            }
            EntitySub::Filter => {
                let list = filter_choices();
                let n = list.len();
                match k.code {
                    KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                        self.modal = set(sub, (sel + n - 1) % n)
                    }
                    KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                        self.modal = set(sub, (sel + 1) % n)
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let f = list[sel.min(n - 1)];
                        match self.game().set_filter(x, y, f) {
                            Ok(()) => {
                                let what = f.map_or("anything", |i| i.name());
                                self.game().log(format!("Filter arm now moves {what}"));
                            }
                            Err(e) => self.game().log(e),
                        }
                        self.modal = set(EntitySub::Main, 0);
                    }
                    c if is_close(c, &['f']) => self.modal = set(EntitySub::Main, 0),
                    _ => {}
                }
            }
        }
    }

    /// Switches between the normal and the colour-blind safe palette and remembers the choice.
    pub fn toggle_palette(&mut self) {
        let on = !self.settings.colorblind();
        self.settings.palette = if on { "colorblind" } else { "default" }.into();
        crate::ui::set_colorblind(on);
        let msg = match self.settings.save(&self.settings_path()) {
            Ok(()) if on => {
                "Colour-blind safe palette on: blue for good, orange for problems.".into()
            }
            Ok(()) => "Normal palette.".to_string(),
            Err(e) => format!("Palette changed, but settings.json couldn't be saved: {e}"),
        };
        if let Some(g) = self.game.as_mut() {
            g.log(msg);
        }
    }

    fn menu_key(&mut self, k: KeyEvent, sel: usize) {
        let n = MENU_ITEMS.len();
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Menu {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Menu { sel: (sel + 1) % n }
            }
            KeyCode::Esc => self.modal = Modal::None,
            KeyCode::Enter | KeyCode::Char(' ') => match sel {
                0 => self.modal = Modal::None,
                1 => self.open_slots(true),
                2 => self.open_slots(false),
                3 => self.modal = Modal::Help { scroll: 0 },
                4 => self.toggle_palette(),
                5 => {
                    self.save_game();
                    self.game = None;
                    self.modal = Modal::None;
                    self.title_sel = 0;
                }
                _ => {
                    self.modal = Modal::Confirm {
                        action: ConfirmAction::QuitWithoutSaving,
                    }
                }
            },
            KeyCode::Char('q') => self.modal = Modal::None,
            _ => {}
        }
    }
}

/// What a filter arm can be set to: anything (`None`), then every item.
pub fn filter_choices() -> Vec<Option<Item>> {
    std::iter::once(None)
        .chain(Item::ALL.into_iter().map(Some))
        .collect()
}

fn plain_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Describes a save file for the slot picker without loading the whole game.
fn slot_info(label: String, path: PathBuf) -> SlotInfo {
    let Ok(meta) = std::fs::metadata(&path) else {
        return SlotInfo {
            label,
            path,
            exists: false,
            detail: "empty".into(),
        };
    };
    let ago = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| {
            let m = d.as_secs() / 60;
            match m {
                0 => "just now".to_string(),
                1..=59 => format!("{m} min ago"),
                60..=2879 => format!("{} h ago", m / 60),
                _ => format!("{} days ago", m / 1440),
            }
        })
        .unwrap_or_default();
    let played = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.pointer("/world/tick").and_then(|t| t.as_u64()));
    let detail = match played {
        Some(t) => {
            let secs = t / crate::world::TICKS_PER_SEC as u64;
            format!(
                "{}:{:02}:{:02} played · {ago}",
                secs / 3600,
                (secs / 60) % 60,
                secs % 60
            )
        }
        None => format!("unreadable · {ago}"),
    };
    SlotInfo {
        label,
        path,
        exists: true,
        detail,
    }
}

/// Esc, `q`, or one of the screen's own keys closes it; anything else is ignored.
fn is_close(code: KeyCode, own: &[char]) -> bool {
    match code {
        KeyCode::Esc => true,
        KeyCode::Char(c) => c == 'q' || own.contains(&c),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn app() -> App {
        let dir = std::env::temp_dir().join(format!("factuio-test-{}", std::process::id()));
        let mut a = App::new(dir.join("save.json"), Some(5), false);
        a.new_game();
        a
    }

    #[test]
    fn research_tree_arrows_move_between_tiers() {
        let mut a = app();
        let tiers = crate::research::tech_tiers();
        let at = |c: usize, r: usize| Modal::Research {
            sel: tiers[c][r].0 as usize,
        };
        a.modal = at(0, 0);
        a.tree_shown.set(true);
        a.handle_key(key(KeyCode::Right));
        assert_eq!(a.modal, at(1, 0));
        a.handle_key(key(KeyCode::Down));
        assert_eq!(a.modal, at(1, 1));
        a.handle_key(key(KeyCode::Up));
        a.handle_key(key(KeyCode::Up));
        assert_eq!(a.modal, at(1, tiers[1].len() - 1));
        // Going to a shorter column lands on its last tech.
        a.handle_key(key(KeyCode::Left));
        assert_eq!(a.modal, at(0, 0));
        a.handle_key(key(KeyCode::Tab));
        assert!(a.research_list);
    }

    #[test]
    fn palette_toggle_is_remembered() {
        let mut a = app();
        a.toggle_palette();
        assert!(a.settings.colorblind());
        let (s, _) = Settings::load_or_create(&a.settings_path());
        assert!(s.colorblind());
        a.toggle_palette();
        assert!(!a.settings.colorblind());
        let (s, _) = Settings::load_or_create(&a.settings_path());
        assert!(!s.colorblind());
    }

    #[test]
    fn paint_mode_lays_a_belt_line() {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world = crate::world::World::empty(40, 40, 1);
        g.player.cursor = (5, 5);
        a.handle_key(key(KeyCode::Char('1'))); // conveyor
        a.handle_key(key(KeyCode::Char('v')));
        for _ in 0..4 {
            a.handle_key(key(KeyCode::Right));
        }
        a.handle_key(key(KeyCode::Down));
        let g = a.game.as_ref().unwrap();
        for x in 5..9 {
            let e = g.world.entity(x, 5).unwrap();
            assert!(e.kind.is_belt());
            assert_eq!(e.dir, Dir::East, "belt at {x}");
        }
        // The corner turns to follow the path.
        assert_eq!(g.world.entity(9, 5).unwrap().dir, Dir::South);
        assert_eq!(g.world.entity(9, 6).unwrap().dir, Dir::South);
    }

    #[test]
    fn keys_open_and_close_screens() {
        let mut a = app();
        for (c, check) in [
            ('c', Modal::Craft { sel: 0 }),
            ('b', Modal::Build { sel: 0 }),
            ('g', Modal::Stats),
            ('?', Modal::Help { scroll: 0 }),
        ] {
            a.handle_key(key(KeyCode::Char(c)));
            assert_eq!(a.modal, check);
            a.handle_key(key(KeyCode::Esc));
            assert_eq!(a.modal, Modal::None);
        }
        a.handle_key(key(KeyCode::Esc));
        assert_eq!(a.modal, Modal::Menu { sel: 0 });
    }

    #[test]
    fn stray_keys_dont_close_screens() {
        let mut a = app();
        a.handle_key(key(KeyCode::Char('c')));
        a.handle_key(key(KeyCode::Char('z')));
        assert!(matches!(a.modal, Modal::Craft { .. }));
        a.handle_key(key(KeyCode::Char('q')));
        assert_eq!(a.modal, Modal::None);
    }

    #[test]
    fn loading_and_quitting_ask_first() {
        let mut a = app();
        a.save_game();
        a.handle_key(key(KeyCode::F(9)));
        assert!(matches!(a.modal, Modal::Confirm { .. }));
        a.handle_key(key(KeyCode::Char('n')));
        assert_eq!(a.modal, Modal::None);
        a.modal = Modal::Menu {
            sel: MENU_ITEMS.len() - 1,
        };
        a.handle_key(key(KeyCode::Enter));
        assert!(!a.quit);
        a.handle_key(key(KeyCode::Char('y')));
        assert!(a.quit);
    }

    #[test]
    fn autosave_uses_its_own_file() {
        let dir = std::env::temp_dir().join(format!("factuio-auto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = App::new(dir.join("save.json"), Some(5), false);
        a.new_game();
        assert!(a.autosave());
        assert!(!a.save_path.exists());
        assert!(a.autosave_path().exists());
        assert_eq!(a.newest_save(), a.autosave_path());
        a.game = None;
        a.continue_game().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn render(a: &App) {
        let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        t.draw(|f| crate::ui::draw(f, a)).unwrap();
    }

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Screen cell of a map tile, after a render.
    fn cell_of(a: &App, x: i32, y: i32) -> (u16, u16) {
        let r = a.map_rect.get();
        let (cx, cy) = a.camera.get().unwrap();
        (r.x + ((x - cx) * 2) as u16, r.y + (y - cy) as u16)
    }

    fn flat_app() -> App {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world = crate::world::World::empty(60, 40, 1);
        g.player.cursor = (30, 20);
        a.camera.set(None);
        render(&a);
        a
    }

    #[test]
    fn mouse_drag_lays_belts_and_right_click_picks_up() {
        let mut a = flat_app();
        a.handle_key(key(KeyCode::Char('1'))); // conveyor
        let (c0, r0) = cell_of(&a, 25, 20);
        a.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), c0, r0));
        let (c1, r1) = cell_of(&a, 29, 22);
        a.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), c1, r1));
        a.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), c1, r1));
        let w = &a.game.as_ref().unwrap().world;
        for x in 25..=29 {
            assert!(w.entity(x, 20).is_some_and(|e| e.kind.is_belt()), "{x}");
        }
        assert_eq!(w.entity(28, 20).unwrap().dir, Dir::East);
        assert_eq!(w.entity(29, 21).unwrap().dir, Dir::South);
        // The whole stroke is one undo step.
        assert_eq!(a.undo.len(), 1);
        a.handle_key(key(KeyCode::Char('u')));
        assert!(a.game.as_ref().unwrap().world.entity(27, 20).is_none());
        // Right click picks up.
        a.handle_key(key(KeyCode::Char('q')));
        a.game
            .as_mut()
            .unwrap()
            .place(10, 10, Item::Crate, Dir::North)
            .unwrap();
        a.handle_key(key(KeyCode::Char('2')));
        let (c, r) = cell_of(&a, 10, 10);
        // First right click only puts the selected tool away.
        a.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), c, r));
        assert!(a.selected.is_none());
        a.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), c, r));
        assert!(a.game.as_ref().unwrap().world.entity(10, 10).is_none());
    }

    #[test]
    fn placing_express_over_a_conveyor_upgrades_it() {
        let mut a = flat_app();
        let g = a.game.as_mut().unwrap();
        g.place(5, 5, Item::Conveyor, Dir::East).unwrap();
        let i = g.world.idx(5, 5).unwrap();
        g.world.ensure_caches();
        g.world.insert(i, Item::Cog, 1, false);
        g.player.inventory.add(Item::ExpressConveyor, 1);
        let belts = g.player.inventory.count(Item::Conveyor);
        g.player.cursor = (5, 5);
        a.selected = Some(Item::ExpressConveyor);
        a.place_dir = Dir::East;
        a.handle_key(key(KeyCode::Enter));
        let g = a.game.as_ref().unwrap();
        let e = g.world.entity(5, 5).unwrap();
        assert_eq!(e.kind, EntityKind::ExpressConveyor);
        // The cog stays on the belt and the old conveyor comes back.
        assert!(matches!(&e.state, State::Belt { items, .. } if items.len() == 1));
        assert_eq!(g.player.inventory.count(Item::Conveyor), belts + 1);
        // Undo puts the old conveyor back.
        a.handle_key(key(KeyCode::Char('u')));
        let e = a.game.as_ref().unwrap().world.entity(5, 5).unwrap();
        assert_eq!(e.kind, EntityKind::Conveyor);
    }

    #[test]
    fn area_copy_and_paste_with_keys() {
        let mut a = flat_app();
        let g = a.game.as_mut().unwrap();
        g.place(2, 2, Item::Crate, Dir::North).unwrap();
        g.place(3, 2, Item::Arm, Dir::East).unwrap();
        g.player.cursor = (2, 2);
        a.handle_key(key(KeyCode::Char('z')));
        a.handle_key(key(KeyCode::Right));
        a.handle_key(key(KeyCode::Char('y')));
        assert!(a.blueprint.is_some());
        a.game.as_mut().unwrap().player.cursor = (10, 10);
        a.handle_key(key(KeyCode::Enter));
        let w = &a.game.as_ref().unwrap().world;
        assert_eq!(w.entity(10, 10).unwrap().kind, EntityKind::Crate);
        assert_eq!(w.entity(11, 10).unwrap().kind, EntityKind::Arm);
        // Area pick-up is one undoable action.
        a.handle_key(key(KeyCode::Esc));
        a.handle_key(key(KeyCode::Char('z')));
        a.handle_key(key(KeyCode::Right));
        a.handle_key(key(KeyCode::Char('x')));
        assert!(a.game.as_ref().unwrap().world.entity(11, 10).is_none());
        a.handle_key(key(KeyCode::Char('u')));
        assert!(a.game.as_ref().unwrap().world.entity(11, 10).is_some());
    }

    #[test]
    fn next_alert_jumps_to_a_problem() {
        let mut a = flat_app();
        let g = a.game.as_mut().unwrap();
        g.player.inventory.add(Item::Fabricator, 1);
        g.place(40, 30, Item::Fabricator, Dir::North).unwrap();
        g.tick();
        a.handle_key(key(KeyCode::Char('n')));
        assert_eq!(a.game.as_ref().unwrap().player.cursor, (40, 30));
    }

    #[test]
    fn research_enter_queues_after_the_current_tech() {
        let mut a = app();
        let fab = TechId::by_id("fabrication").unwrap();
        let logic = TechId::by_id("logic").unwrap();
        a.modal = Modal::Research {
            sel: fab.0 as usize,
        };
        a.handle_key(key(KeyCode::Enter));
        a.modal = Modal::Research {
            sel: logic.0 as usize,
        };
        a.handle_key(key(KeyCode::Enter));
        let r = &a.game.as_ref().unwrap().world.research;
        assert_eq!(r.current, Some(fab));
        assert_eq!(r.queue, vec![logic]);
    }

    #[test]
    fn save_slots_round_trip() {
        let dir = std::env::temp_dir().join(format!("factuio-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = App::new(dir.join("save.json"), Some(5), false);
        a.new_game();
        a.open_slots(true);
        a.handle_key(key(KeyCode::Down));
        a.handle_key(key(KeyCode::Enter));
        let slot2 = dir.join("save-2.json");
        assert!(slot2.exists());
        assert_eq!(a.save_path, slot2);
        a.game = None;
        a.open_slots(false);
        assert!(a.slots[1].exists && !a.slots[0].exists);
        assert!(a.slots[1].detail.contains("played"));
        a.handle_key(key(KeyCode::Enter));
        assert!(a.game.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_then_inspect_and_fuel() {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world = crate::world::World::empty(20, 20, 1);
        g.player.cursor = (3, 3);
        a.handle_key(key(KeyCode::Char('4'))); // kiln
        a.handle_key(key(KeyCode::Enter));
        assert_eq!(
            a.game.as_ref().unwrap().world.entity(3, 3).unwrap().kind,
            EntityKind::Kiln
        );
        a.handle_key(key(KeyCode::Char('q')));
        a.handle_key(key(KeyCode::Char('e')));
        assert!(matches!(a.modal, Modal::Entity { .. }));
        // Each press adds 10.
        a.handle_key(key(KeyCode::Char('f')));
        a.handle_key(key(KeyCode::Char('f')));
        match &a.game.as_ref().unwrap().world.entity(3, 3).unwrap().state {
            State::Furnace { fuel, .. } => assert_eq!(fuel.stock, 20),
            _ => unreachable!(),
        }
        let g = a.game.as_ref().unwrap();
        assert_eq!(g.player.inventory.count(Item::Lignite), 10);
    }

    #[test]
    fn landfill_is_placed_from_the_build_menu_and_undone() {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        g.world = crate::world::World::empty(20, 20, 1);
        let i = g.world.idx(6, 6).unwrap();
        g.world.tiles[i].terrain = crate::world::Terrain::Water;
        g.player.cursor = (6, 6);
        g.player.inventory.add(Item::Landfill, 2);
        a.handle_key(key(KeyCode::Char('b')));
        let pos = Item::placeable().position(|i| i == Item::Landfill).unwrap();
        a.modal = Modal::Build { sel: pos };
        a.handle_key(key(KeyCode::Enter));
        assert_eq!(a.selected, Some(Item::Landfill));
        a.handle_key(key(KeyCode::Enter));
        let g = a.game.as_ref().unwrap();
        assert_eq!(g.world.tiles[i].terrain, crate::world::Terrain::Sand);
        assert_eq!(g.player.inventory.count(Item::Landfill), 1);
        a.handle_key(key(KeyCode::Char('q')));
        a.handle_key(key(KeyCode::Char('u')));
        let g = a.game.as_ref().unwrap();
        assert_eq!(g.world.tiles[i].terrain, crate::world::Terrain::Water);
        assert_eq!(g.player.inventory.count(Item::Landfill), 2);
    }

    #[test]
    fn chem_plants_only_list_chemistry_recipes() {
        let mut a = app();
        let g = a.game.as_mut().unwrap();
        for id in ["chemistry", "energy-storage", "fabrication"] {
            g.world.research.complete(TechId::by_id(id).unwrap());
        }
        let chem: Vec<_> = a
            .recipe_choices(EntityKind::ChemPlant)
            .into_iter()
            .flatten()
            .map(|r| r.get().id)
            .collect();
        assert_eq!(chem, vec!["resin", "battery"]);
        assert!(
            a.recipe_choices(EntityKind::Fabricator)
                .into_iter()
                .flatten()
                .all(|r| r.get().category == Category::Craft)
        );
        let g = a.game.as_mut().unwrap();
        g.player.inventory.add(Item::ChemPlant, 1);
        g.player.inventory.add(Item::Fabricator, 1);
        g.world = crate::world::World::empty(20, 20, 1);
        for id in ["chemistry", "fabrication"] {
            g.world.research.complete(TechId::by_id(id).unwrap());
        }
        g.place(3, 3, Item::ChemPlant, Dir::North).unwrap();
        g.place(5, 3, Item::Fabricator, Dir::North).unwrap();
        assert!(g.set_recipe(3, 3, RecipeId::by_id("cog")).is_err());
        assert!(g.set_recipe(5, 3, RecipeId::by_id("resin")).is_err());
        assert!(g.set_recipe(3, 3, RecipeId::by_id("resin")).is_ok());
    }

    #[test]
    fn new_game_offers_every_scenario() {
        let dir = std::env::temp_dir().join(format!("factuio-pick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut a = App::new(dir.join("save.json"), None, false);
        a.handle_key(key(KeyCode::Char('n')));
        assert_eq!(a.modal, Modal::NewGame { sel: 0 });
        a.handle_key(key(KeyCode::Esc));
        assert_eq!(a.modal, Modal::None);
        assert!(a.game.is_none());
        // Pick the third one: Far fields.
        a.handle_key(key(KeyCode::Enter));
        assert_eq!(a.modal, Modal::NewGame { sel: 0 });
        a.handle_key(key(KeyCode::Down));
        a.handle_key(key(KeyCode::Down));
        a.handle_key(key(KeyCode::Enter));
        let g = a.game.as_ref().unwrap();
        assert_eq!(g.world.scenario.id, "far-fields");
        assert_eq!(a.best_times().len(), SCENARIOS.len());
        // A win is recorded under its scenario.
        a.game.as_mut().unwrap().victory_pending = true;
        a.game.as_mut().unwrap().world.won_at = Some(1234);
        a.step();
        assert_eq!(a.modal, Modal::Victory);
        let r = Records::load(&a.records_path());
        assert_eq!(r.best("far-fields"), Some(1234));
        assert_eq!(r.best(""), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn key_labels_follow_remapped_keys() {
        let mut a = app();
        assert_eq!(a.key_label("mine"), "m");
        let mut s = Settings::default();
        s.keys.insert("mine".into(), "k".into());
        a.apply_settings(s);
        assert_eq!(a.key_label("mine"), "k");
    }

    #[test]
    fn carts_go_on_track_and_come_off_with_undo() {
        let mut a = flat_app();
        let g = a.game.as_mut().unwrap();
        g.player.inventory.add(Item::Depot, 2);
        g.player.inventory.add(Item::Rail, 10);
        g.player.inventory.add(Item::Cart, 1);
        g.place(30, 20, Item::Depot, Dir::North).unwrap();
        a.selected = Some(Item::Cart);
        a.handle_key(key(KeyCode::Enter));
        let g = a.game.as_mut().unwrap();
        assert_eq!(g.world.carts.len(), 1);
        assert_eq!(g.player.inventory.count(Item::Cart), 0);
        a.handle_key(key(KeyCode::Char('u')));
        let g = a.game.as_mut().unwrap();
        assert!(g.world.carts.is_empty());
        assert_eq!(g.player.inventory.count(Item::Cart), 1);
        // x on a depot with a cart takes the cart first, then the depot; undo puts both back.
        g.add_cart(30, 20).unwrap();
        a.selected = None;
        a.handle_key(key(KeyCode::Char('x')));
        let g = a.game.as_mut().unwrap();
        assert!(g.world.carts.is_empty());
        assert!(g.world.entity(30, 20).is_some());
        a.handle_key(key(KeyCode::Char('u')));
        assert_eq!(a.game.as_ref().unwrap().world.carts.len(), 1);
        // Area pick-up takes depot and cart together, and undoes together.
        a.handle_key(key(KeyCode::Char('z')));
        a.handle_key(key(KeyCode::Char('x')));
        let g = a.game.as_ref().unwrap();
        assert!(g.world.carts.is_empty() && g.world.entity(30, 20).is_none());
        a.handle_key(key(KeyCode::Char('u')));
        let g = a.game.as_ref().unwrap();
        assert_eq!(g.world.carts.len(), 1);
        assert!(g.world.entity(30, 20).is_some());
    }

    #[test]
    fn depots_switch_mode_and_keep_it_through_undo() {
        let mut a = flat_app();
        let g = a.game.as_mut().unwrap();
        g.player.inventory.add(Item::Depot, 1);
        g.place(30, 20, Item::Depot, Dir::North).unwrap();
        let i = g.world.idx(30, 20).unwrap();
        a.handle_key(key(KeyCode::Char('e')));
        a.handle_key(key(KeyCode::Char('v')));
        assert_eq!(a.game.as_ref().unwrap().world.depot_unloads(i), Some(true));
        a.handle_key(key(KeyCode::Esc));
        a.handle_key(key(KeyCode::Char('x')));
        assert!(a.game.as_ref().unwrap().world.entity(30, 20).is_none());
        a.handle_key(key(KeyCode::Char('u')));
        assert_eq!(a.game.as_ref().unwrap().world.depot_unloads(i), Some(true));
    }
}
