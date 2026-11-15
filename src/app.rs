//! Application state and keyboard handling.

use crate::game::Game;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::{Category, RecipeId, TechId};
use crate::world::State;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

pub const AUTOSAVE_SECS: u64 = 300;

#[derive(Clone, Debug, PartialEq)]
pub enum EntitySub {
    Main,
    Insert,
    Recipe,
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConfirmAction {
    LoadSave,
    QuitWithoutSaving,
}

impl ConfirmAction {
    pub fn question(self) -> &'static str {
        match self {
            ConfirmAction::LoadSave => "Load the last save? Progress since then will be lost.",
            ConfirmAction::QuitWithoutSaving => {
                "Quit without saving? Progress since the last save will be lost."
            }
        }
    }
}

pub const MENU_ITEMS: [&str; 6] = [
    "Resume",
    "Save",
    "Load",
    "Help",
    "Save & quit to title",
    "Quit without saving",
];

pub struct App {
    pub game: Option<Game>,
    pub modal: Modal,
    pub selected: Option<Item>,
    pub place_dir: Dir,
    pub paint: bool,
    pub paused: bool,
    pub fast: bool,
    pub ascii: bool,
    pub power_overlay: bool,
    pub save_path: PathBuf,
    pub seed: Option<u64>,
    pub quit: bool,
    pub title_sel: usize,
    pub title_msg: Option<String>,
    pub last_autosave_tick: u64,
}

impl App {
    pub fn new(save_path: PathBuf, seed: Option<u64>, ascii: bool) -> Self {
        App {
            game: None,
            modal: Modal::None,
            selected: None,
            place_dir: Dir::East,
            paint: false,
            paused: false,
            fast: false,
            ascii,
            power_overlay: false,
            save_path,
            seed,
            quit: false,
            title_sel: 0,
            title_msg: None,
            last_autosave_tick: 0,
        }
    }

    /// Where autosaves (and emergency saves on Ctrl+C or a closed terminal) go,
    /// so they never overwrite a save the player made on purpose.
    pub fn autosave_path(&self) -> PathBuf {
        let stem = self
            .save_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "save".into());
        self.save_path
            .with_file_name(format!("{stem}.autosave.json"))
    }

    pub fn save_exists(&self) -> bool {
        self.save_path.exists() || self.autosave_path().exists()
    }

    /// The most recently written of the main save and the autosave.
    pub fn newest_save(&self) -> PathBuf {
        let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let auto = self.autosave_path();
        match (mtime(&self.save_path), mtime(&auto)) {
            (Some(a), Some(b)) if b > a => auto,
            (None, Some(_)) => auto,
            _ => self.save_path.clone(),
        }
    }

    pub fn title_items(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.save_exists() {
            v.push("Continue");
        }
        v.extend(["New game", "Help", "Quit"]);
        v
    }

    pub fn new_game(&mut self) {
        let seed = self.seed.take().unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
        });
        let mut g = Game::new(seed);
        g.log(format!(
            "New world, seed {seed}. Ores are nearby: look around with the arrow keys."
        ));
        self.start(g);
    }

    fn start(&mut self, g: Game) {
        self.last_autosave_tick = g.world.tick;
        self.game = Some(g);
        self.modal = Modal::None;
        self.selected = None;
        self.paint = false;
        self.paused = false;
    }

    /// Loads the main save (F9, menu Load).
    pub fn load_game(&mut self) -> Result<(), String> {
        let g = Game::load(&self.save_path)?;
        self.start(g);
        Ok(())
    }

    /// Loads whichever of the main save and the autosave is newer (Continue, --continue).
    pub fn continue_game(&mut self) -> Result<(), String> {
        let path = self.newest_save();
        let mut g = Game::load(&path)?;
        if path != self.save_path {
            g.log("Continued from the autosave.");
        }
        self.start(g);
        Ok(())
    }

    pub fn save_game(&mut self) {
        let path = self.save_path.clone();
        if let Some(g) = self.game.as_mut() {
            match g.save(&path) {
                Ok(()) => {
                    g.log(format!("Saved to {}", path.display()));
                    self.last_autosave_tick = g.world.tick;
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
                Modal::Menu { .. } | Modal::Help { .. } | Modal::Victory
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
        if g.victory_pending {
            g.victory_pending = false;
            self.modal = Modal::Victory;
        }
        let tps = crate::world::TICKS_PER_SEC as u64;
        if g.world.tick >= self.last_autosave_tick + AUTOSAVE_SECS * tps {
            self.autosave();
        }
    }

    pub fn handle_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.autosave();
            self.quit = true;
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
                        ConfirmAction::LoadSave => {
                            if let Err(e) = self.load_game() {
                                self.game().log(e);
                            }
                        }
                        ConfirmAction::QuitWithoutSaving => self.quit = true,
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = Modal::None
                }
                _ => {}
            },
        }
    }

    fn title_key(&mut self, k: KeyEvent) {
        let items = self.title_items();
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
            KeyCode::Char('n') => self.new_game(),
            KeyCode::Char('?') => self.modal = Modal::Help { scroll: 0 },
            KeyCode::Enter | KeyCode::Char(' ') => match items[self.title_sel.min(items.len() - 1)]
            {
                "Continue" => {
                    if let Err(e) = self.continue_game() {
                        self.title_msg = Some(e);
                    }
                }
                "New game" => self.new_game(),
                "Help" => self.modal = Modal::Help { scroll: 0 },
                _ => self.quit = true,
            },
            _ => {}
        }
    }

    fn game(&mut self) -> &mut Game {
        self.game.as_mut().expect("game running")
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
                let is_belt = item.building().is_some_and(|k| k.is_belt());
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
                let g = self.game.as_mut().unwrap();
                if g.world.entity(nx, ny).is_none()
                    && let Err(e) = g.place(nx, ny, item, pd)
                {
                    g.log(e);
                    if g.player.inventory.count(item) == 0 {
                        self.paint = false;
                    }
                }
            }
        }
    }

    fn play_key(&mut self, k: KeyEvent) {
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let big = if shift { 8 } else { 1 };
        match k.code {
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
                let (x, y) = self.game().player.cursor;
                if let Some(item) = self.selected {
                    let d = self.place_dir;
                    if let Err(e) = self.game().place(x, y, item, d) {
                        if self.game().world.entity(x, y).is_some() {
                            self.open_entity();
                        } else {
                            self.game().log(e);
                        }
                    }
                } else {
                    self.open_entity();
                }
            }
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => {
                let (x, y) = self.game().player.cursor;
                match self.game().remove(x, y) {
                    Ok(kind) => self.game().log(format!("Picked up {}", kind.name())),
                    Err(e) => self.game().log(e),
                }
            }
            KeyCode::Char('r') => {
                if self.selected.is_some() {
                    self.place_dir = self.place_dir.cw();
                } else {
                    let (x, y) = self.game().player.cursor;
                    self.game().world.rotate(x, y);
                }
            }
            KeyCode::Char('R') => {
                let (x, y) = self.game().player.cursor;
                self.game().world.rotate(x, y);
            }
            KeyCode::Char('q') => {
                if self.selected.is_some() {
                    self.selected = None;
                    self.paint = false;
                } else {
                    let (x, y) = self.game().player.cursor;
                    if let Some((kind, dir)) =
                        self.game().world.entity(x, y).map(|e| (e.kind, e.dir))
                    {
                        self.selected = Some(kind.item());
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
                    self.paint = !self.paint;
                    if self.paint {
                        let (x, y) = self.game().player.cursor;
                        let (item, d) = (self.selected.unwrap(), self.place_dir);
                        if self.game().world.entity(x, y).is_none() {
                            let _ = self.game().place(x, y, item, d);
                        }
                        self.game()
                            .log("Paint mode: moving the cursor builds. Press v to stop.");
                    }
                }
            }
            KeyCode::Char(c @ '0'..='9') => {
                let slot = if c == '0' {
                    9
                } else {
                    c as usize - '1' as usize
                };
                let item = self.game().player.hotbar[slot];
                if self.selected == item {
                    self.selected = None;
                    self.paint = false;
                } else {
                    self.selected = item;
                }
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
                        action: ConfirmAction::LoadSave,
                    };
                } else {
                    self.game().log("No save yet: press F5 to save first.");
                }
            }
            KeyCode::Esc => {
                if self.paint {
                    self.paint = false;
                } else if self.selected.is_some() {
                    self.selected = None;
                } else {
                    self.modal = Modal::Menu { sel: 0 };
                }
            }
            _ => {}
        }
    }

    fn open_entity(&mut self) {
        let (x, y) = self.game().player.cursor;
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
        let n = EntityKind::ALL.len();
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
                self.selected = Some(EntityKind::ALL[sel].item());
                self.modal = Modal::None;
            }
            KeyCode::Char(c @ '0'..='9') => {
                let slot = if c == '0' {
                    9
                } else {
                    c as usize - '1' as usize
                };
                let item = EntityKind::ALL[sel].item();
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
        match k.code {
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
                self.modal = Modal::Research {
                    sel: (sel + n - 1) % n,
                }
            }
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
                self.modal = Modal::Research { sel: (sel + 1) % n }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let t = TechId(sel as u16);
                let g = self.game();
                match g.start_research(t) {
                    Ok(()) => g.log(format!("Researching {}", t.get().name)),
                    Err(e) => g.log(e),
                }
            }
            c if is_close(c, &['t']) => self.modal = Modal::None,
            _ => {}
        }
    }

    pub fn recipe_choices(&self) -> Vec<Option<RecipeId>> {
        let r = &self.game.as_ref().unwrap().world.research;
        let mut v = vec![None];
        v.extend(
            RecipeId::all()
                .filter(|&id| id.get().category == Category::Craft && r.is_unlocked(id))
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
                KeyCode::Char('f') => {
                    let n = self.game().insert(x, y, Item::Lignite, 50);
                    if n > 0 {
                        self.game().log(format!("Added {n} lignite"));
                    } else {
                        self.game()
                            .log("No fuel added (none in inventory, full, or no fuel slot)");
                    }
                }
                KeyCode::Char('r') if kind == EntityKind::Fabricator => {
                    let cur = match &self.game().world.entity(x, y).unwrap().state {
                        State::Assembler { recipe, .. } => *recipe,
                        _ => None,
                    };
                    let pos = self
                        .recipe_choices()
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
                let list = self.recipe_choices();
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
                1 => {
                    self.save_game();
                    self.modal = Modal::None;
                }
                2 => {
                    self.modal = if self.save_path.exists() {
                        Modal::Confirm {
                            action: ConfirmAction::LoadSave,
                        }
                    } else {
                        self.game().log("No save yet: choose Save first.");
                        Modal::None
                    };
                }
                3 => self.modal = Modal::Help { scroll: 0 },
                4 => {
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
        a.modal = Modal::Menu { sel: 5 };
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
        a.handle_key(key(KeyCode::Char('f')));
        match &a.game.as_ref().unwrap().world.entity(3, 3).unwrap().state {
            State::Furnace { fuel, .. } => assert_eq!(fuel.stock, 30),
            _ => unreachable!(),
        }
    }
}
