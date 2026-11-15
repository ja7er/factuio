//! Player tools that work on many buildings at once: area selection, blueprints and undo.

use crate::game::Game;
use crate::items::{Dir, EntityKind, Item};
use crate::recipes::RecipeId;
use crate::world::State;

/// One building in a blueprint, relative to its top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct BpCell {
    pub dx: i32,
    pub dy: i32,
    pub kind: EntityKind,
    pub dir: Dir,
    pub recipe: Option<RecipeId>,
    pub filter: Option<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Blueprint {
    pub w: i32,
    pub h: i32,
    pub cells: Vec<BpCell>,
}

/// Inclusive rectangle spanned by two corners.
pub fn rect(a: (i32, i32), b: (i32, i32)) -> (i32, i32, i32, i32) {
    (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

pub fn recipe_of(state: &State) -> Option<RecipeId> {
    match state {
        State::Assembler { recipe, .. } => *recipe,
        _ => None,
    }
}

pub fn filter_of(state: &State) -> Option<Item> {
    match state {
        State::Arm { filter, .. } => *filter,
        _ => None,
    }
}

/// Builds `kind` and restores its settings. Shared by blueprint paste and undo.
fn rebuild(
    game: &mut Game,
    (x, y): (i32, i32),
    kind: EntityKind,
    dir: Dir,
    recipe: Option<RecipeId>,
    filter: Option<Item>,
) -> bool {
    if game.place_kind(x, y, kind, dir).is_err() {
        return false;
    }
    if recipe.is_some() {
        let _ = game.set_recipe(x, y, recipe);
    }
    if filter.is_some() {
        let _ = game.set_filter(x, y, filter);
    }
    true
}

impl Blueprint {
    /// Copies the buildings in the rectangle between two corners. `None` if it holds none.
    pub fn copy(game: &Game, a: (i32, i32), b: (i32, i32)) -> Option<Blueprint> {
        let (x0, y0, x1, y1) = rect(a, b);
        let mut cells = vec![];
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(e) = game.world.entity(x, y) {
                    cells.push(BpCell {
                        dx: x - x0,
                        dy: y - y0,
                        kind: e.kind,
                        dir: e.dir,
                        recipe: recipe_of(&e.state),
                        filter: filter_of(&e.state),
                    });
                }
            }
        }
        if cells.is_empty() {
            return None;
        }
        Some(Blueprint {
            w: x1 - x0 + 1,
            h: y1 - y0 + 1,
            cells,
        })
    }

    /// Rotates a quarter turn clockwise.
    pub fn rotate(&mut self) {
        for c in &mut self.cells {
            let (dx, dy) = (self.h - 1 - c.dy, c.dx);
            c.dx = dx;
            c.dy = dy;
            if c.kind.directional() {
                c.dir = c.dir.cw();
            }
        }
        std::mem::swap(&mut self.w, &mut self.h);
    }

    /// How many of each building the blueprint needs.
    pub fn cost(&self) -> crate::inventory::Inventory {
        let mut inv = crate::inventory::Inventory::new();
        for c in &self.cells {
            inv.add(c.kind.item(), 1);
        }
        inv
    }

    /// Places what it can with the top-left corner at (x, y). Returns (placed, skipped) and the
    /// undo steps for what was placed.
    pub fn paste(&self, game: &mut Game, x: i32, y: i32) -> (usize, usize, Vec<UndoStep>) {
        let (mut placed, mut skipped) = (0, 0);
        let mut steps = vec![];
        for c in &self.cells {
            let (px, py) = (x + c.dx, y + c.dy);
            if game.world.entity(px, py).is_some_and(|e| e.kind == c.kind) {
                continue;
            }
            if rebuild(game, (px, py), c.kind, c.dir, c.recipe, c.filter) {
                steps.push(UndoStep::Placed {
                    x: px,
                    y: py,
                    kind: c.kind,
                });
                placed += 1;
            } else {
                skipped += 1;
            }
        }
        (placed, skipped, steps)
    }
}

/// Something the player did that can be taken back.
#[derive(Clone, Debug, PartialEq)]
pub enum UndoStep {
    Placed {
        x: i32,
        y: i32,
        kind: EntityKind,
    },
    /// The building was picked up (its contents went to the inventory and stay there).
    Removed {
        x: i32,
        y: i32,
        kind: EntityKind,
        dir: Dir,
        recipe: Option<RecipeId>,
        filter: Option<Item>,
    },
    /// Landfill turned this water tile into sand.
    Filled {
        x: i32,
        y: i32,
    },
}

impl UndoStep {
    pub fn removed(x: i32, y: i32, e: &crate::world::Entity) -> UndoStep {
        UndoStep::Removed {
            x,
            y,
            kind: e.kind,
            dir: e.dir,
            recipe: recipe_of(&e.state),
            filter: filter_of(&e.state),
        }
    }

    /// Reverses the step. Returns false if it no longer applies (the map changed since).
    pub fn undo(&self, game: &mut Game) -> bool {
        match *self {
            UndoStep::Placed { x, y, kind } => {
                if game.world.entity(x, y).is_some_and(|e| e.kind == kind) {
                    game.remove(x, y).is_ok()
                } else {
                    false
                }
            }
            UndoStep::Removed {
                x,
                y,
                kind,
                dir,
                recipe,
                filter,
            } => {
                game.world.entity(x, y).is_none()
                    && rebuild(game, (x, y), kind, dir, recipe, filter)
            }
            UndoStep::Filled { x, y } => {
                if game.world.unfill(x, y) {
                    game.player.inventory.add(Item::Landfill, 1);
                    true
                } else {
                    false
                }
            }
        }
    }
}

/// Undo history: each entry is one player action (a click, a paint stroke, a paste).
#[derive(Default)]
pub struct UndoStack {
    groups: Vec<Vec<UndoStep>>,
    /// While true (a paint stroke or drag), new steps join the last group.
    pub open: bool,
}

pub const UNDO_LIMIT: usize = 50;

impl UndoStack {
    pub fn push(&mut self, step: UndoStep) {
        if !self.open || self.groups.is_empty() {
            self.groups.push(vec![]);
        }
        self.groups.last_mut().unwrap().push(step);
        if self.groups.len() > UNDO_LIMIT {
            self.groups.remove(0);
        }
    }

    pub fn push_group(&mut self, steps: Vec<UndoStep>) {
        if steps.is_empty() {
            return;
        }
        self.groups.push(steps);
        if self.groups.len() > UNDO_LIMIT {
            self.groups.remove(0);
        }
    }

    /// Starts a group that later steps join until `close`.
    pub fn begin(&mut self) {
        self.groups.push(vec![]);
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        if self.groups.last().is_some_and(|g| g.is_empty()) {
            self.groups.pop();
        }
    }

    pub fn pop(&mut self) -> Option<Vec<UndoStep>> {
        while let Some(g) = self.groups.pop() {
            if !g.is_empty() {
                return Some(g);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.groups.iter().filter(|g| !g.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.groups.clear();
        self.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    fn game() -> Game {
        Game::from_world(World::empty(30, 30, 1))
    }

    #[test]
    fn copy_rotate_paste() {
        let mut g = game();
        g.place(2, 2, Item::Conveyor, Dir::East).unwrap();
        g.place(3, 2, Item::Arm, Dir::East).unwrap();
        g.place(4, 2, Item::Crate, Dir::North).unwrap();
        let mut bp = Blueprint::copy(&g, (2, 2), (4, 3)).unwrap();
        assert_eq!((bp.w, bp.h, bp.cells.len()), (3, 2, 3));
        bp.rotate();
        assert_eq!((bp.w, bp.h), (2, 3));
        // The east-facing conveyor at (0,0) ends up at the top right, facing south.
        let c = bp
            .cells
            .iter()
            .find(|c| c.kind == EntityKind::Conveyor)
            .unwrap();
        assert_eq!((c.dx, c.dy, c.dir), (1, 0, Dir::South));
        let (placed, skipped, steps) = bp.paste(&mut g, 10, 10);
        assert_eq!((placed, skipped), (3, 0));
        assert_eq!(g.world.entity(11, 10).unwrap().dir, Dir::South);
        // Undo takes them all back.
        for s in steps.iter().rev() {
            assert!(s.undo(&mut g));
        }
        assert!(g.world.entity(11, 10).is_none());
    }

    #[test]
    fn undo_restores_a_removed_fabricator_recipe() {
        let mut g = game();
        g.world
            .research
            .complete(crate::recipes::TechId::by_id("fabrication").unwrap());
        g.player.inventory.add(Item::Fabricator, 1);
        g.place(5, 5, Item::Fabricator, Dir::North).unwrap();
        g.set_recipe(5, 5, RecipeId::by_id("cog")).unwrap();
        let step = UndoStep::removed(5, 5, g.world.entity(5, 5).unwrap());
        g.remove(5, 5).unwrap();
        assert!(step.undo(&mut g));
        assert_eq!(
            recipe_of(&g.world.entity(5, 5).unwrap().state),
            RecipeId::by_id("cog")
        );
    }

    #[test]
    fn blueprints_and_undo_keep_filters_and_tunnel_ends() {
        let mut g = game();
        g.player.inventory.add(Item::Tunnel, 4);
        g.player.inventory.add(Item::FilterArm, 2);
        g.place(2, 2, Item::Tunnel, Dir::East).unwrap();
        g.place(5, 2, Item::Tunnel, Dir::East).unwrap();
        g.place(3, 4, Item::FilterArm, Dir::South).unwrap();
        g.set_filter(3, 4, Some(Item::Glass)).unwrap();
        let bp = Blueprint::copy(&g, (2, 2), (5, 4)).unwrap();
        let (placed, skipped, _) = bp.paste(&mut g, 2, 10);
        assert_eq!((placed, skipped), (3, 0));
        // The copy keeps which end is which, even pasted somewhere a fresh piece would guess.
        assert_eq!(g.world.entity(2, 10).unwrap().kind, EntityKind::TunnelIn);
        assert_eq!(g.world.entity(5, 10).unwrap().kind, EntityKind::TunnelOut);
        assert_eq!(
            filter_of(&g.world.entity(3, 12).unwrap().state),
            Some(Item::Glass)
        );
        // Undoing a pick-up brings the exit back as an exit, with its filter arm neighbour.
        let steps: Vec<UndoStep> = [(5, 2), (3, 4)]
            .iter()
            .map(|&(x, y)| UndoStep::removed(x, y, g.world.entity(x, y).unwrap()))
            .collect();
        g.remove(5, 2).unwrap();
        g.remove(3, 4).unwrap();
        for s in steps.iter().rev() {
            assert!(s.undo(&mut g));
        }
        assert_eq!(g.world.entity(5, 2).unwrap().kind, EntityKind::TunnelOut);
        assert_eq!(
            filter_of(&g.world.entity(3, 4).unwrap().state),
            Some(Item::Glass)
        );
    }

    #[test]
    fn grouped_steps_undo_together() {
        let mut u = UndoStack::default();
        u.begin();
        for x in 0..3 {
            u.push(UndoStep::Placed {
                x,
                y: 0,
                kind: EntityKind::Conveyor,
            });
        }
        u.close();
        u.push(UndoStep::Placed {
            x: 9,
            y: 9,
            kind: EntityKind::Crate,
        });
        assert_eq!(u.len(), 2);
        assert_eq!(u.pop().unwrap().len(), 1);
        assert_eq!(u.pop().unwrap().len(), 3);
        assert!(u.pop().is_none());
    }
}
