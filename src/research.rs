//! Research progress and recipe unlocks.

use crate::recipes::{RecipeId, TECHS, TechId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Research {
    pub done: BTreeSet<TechId>,
    pub current: Option<TechId>,
    /// Completed units per tech.
    pub progress: BTreeMap<TechId, u32>,
}

impl Research {
    pub fn is_done(&self, t: TechId) -> bool {
        self.done.contains(&t)
    }

    pub fn is_available(&self, t: TechId) -> bool {
        !self.is_done(t)
            && t.get()
                .prereqs
                .iter()
                .all(|p| TechId::by_id(p).is_some_and(|p| self.is_done(p)))
    }

    pub fn units_done(&self, t: TechId) -> u32 {
        self.progress.get(&t).copied().unwrap_or(0)
    }

    pub fn fraction(&self, t: TechId) -> f32 {
        self.units_done(t) as f32 / t.get().units as f32
    }

    pub fn is_unlocked(&self, r: RecipeId) -> bool {
        let rec = r.get();
        rec.default || self.done.iter().any(|t| t.get().unlocks.contains(&rec.id))
    }

    pub fn start(&mut self, t: TechId) -> Result<(), String> {
        if self.is_done(t) {
            return Err(format!("{} is already researched", t.get().name));
        }
        if !self.is_available(t) {
            return Err(format!("{} needs its prerequisites first", t.get().name));
        }
        self.current = Some(t);
        Ok(())
    }

    /// Adds one completed unit to the current research. Returns the tech if this finished it.
    pub fn add_unit(&mut self) -> Option<TechId> {
        let t = self.current?;
        let n = self.progress.entry(t).or_insert(0);
        *n += 1;
        if *n >= t.get().units {
            self.done.insert(t);
            self.current = None;
            return Some(t);
        }
        None
    }

    #[cfg(test)]
    pub fn complete(&mut self, t: TechId) {
        self.progress.insert(t, t.get().units);
        self.done.insert(t);
        if self.current == Some(t) {
            self.current = None;
        }
    }

    pub fn all_done(&self) -> bool {
        self.done.len() == TECHS.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prereqs_gate_research() {
        let mut r = Research::default();
        let fab = TechId::by_id("fabrication").unwrap();
        let log = TechId::by_id("logistics").unwrap();
        assert!(r.is_available(fab));
        assert!(!r.is_available(log));
        assert!(r.start(log).is_err());
        r.start(fab).unwrap();
        for _ in 0..fab.get().units - 1 {
            assert_eq!(r.add_unit(), None);
        }
        assert_eq!(r.add_unit(), Some(fab));
        assert!(r.is_unlocked(RecipeId::by_id("fabricator").unwrap()));
        assert!(r.is_available(log));
        assert_eq!(r.current, None);
    }
}
