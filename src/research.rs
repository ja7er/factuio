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
    /// Techs to research next, in order, once the current one finishes.
    #[serde(default)]
    pub queue: Vec<TechId>,
    /// Levels finished of each endless tech.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub levels: BTreeMap<TechId, u32>,
}

impl Research {
    pub fn is_done(&self, t: TechId) -> bool {
        self.done.contains(&t)
    }

    /// Whether `t` can be researched now. Endless techs stay available after their first level.
    pub fn is_available(&self, t: TechId) -> bool {
        (t.get().repeatable || !self.is_done(t))
            && t.get()
                .prereqs
                .iter()
                .all(|p| TechId::by_id(p).is_some_and(|p| self.is_done(p)))
    }

    pub fn units_done(&self, t: TechId) -> u32 {
        self.progress.get(&t).copied().unwrap_or(0)
    }

    /// Finished levels of an endless tech (0 for others).
    pub fn level(&self, t: TechId) -> u32 {
        self.levels.get(&t).copied().unwrap_or(0)
    }

    /// Units the next level of `t` takes. Each level of an endless tech costs one base amount more.
    pub fn units_needed(&self, t: TechId) -> u32 {
        let base = t.get().units;
        if t.get().repeatable {
            base * (self.level(t) + 1)
        } else {
            base
        }
    }

    pub fn fraction(&self, t: TechId) -> f32 {
        self.units_done(t) as f32 / self.units_needed(t) as f32
    }

    pub fn is_unlocked(&self, r: RecipeId) -> bool {
        let rec = r.get();
        rec.default || self.done.iter().any(|t| t.get().unlocks.contains(&rec.id))
    }

    pub fn start(&mut self, t: TechId) -> Result<(), String> {
        if self.is_done(t) && !t.get().repeatable {
            return Err(format!("{} is already researched", t.get().name));
        }
        if !self.is_available(t) {
            return Err(format!("{} needs its prerequisites first", t.get().name));
        }
        self.queue.retain(|&q| q != t);
        if let Some(old) = self.current.replace(t)
            && old != t
        {
            // Switching keeps the old tech's progress and puts it back first in line.
            self.queue.insert(0, old);
        }
        Ok(())
    }

    /// Adds one completed unit to the current research. Returns the tech if this finished it.
    /// A finished tech hands over to the first queued tech that is now available.
    pub fn add_unit(&mut self) -> Option<TechId> {
        let t = self.current?;
        let need = self.units_needed(t);
        let n = self.progress.entry(t).or_insert(0);
        *n += 1;
        if *n >= need {
            self.done.insert(t);
            if t.get().repeatable {
                // On to the next level, unless something else is waiting in the queue.
                *self.levels.entry(t).or_insert(0) += 1;
                self.progress.insert(t, 0);
                self.start_next_queued();
                if self.current.is_none() {
                    self.current = Some(t);
                }
            } else {
                self.current = None;
                self.start_next_queued();
            }
            return Some(t);
        }
        None
    }

    fn start_next_queued(&mut self) {
        let done = &self.done;
        self.queue
            .retain(|q| q.get().repeatable || !done.contains(q));
        self.current = None;
        if let Some(k) = self.queue.iter().position(|&q| self.is_available(q)) {
            self.current = Some(self.queue.remove(k));
        }
    }

    /// Queues `t` along with any prerequisites it still needs, in a workable order.
    /// Starts right away if nothing is being researched. Returns how many techs were added.
    pub fn enqueue(&mut self, t: TechId) -> Result<usize, String> {
        if self.is_done(t) && !t.get().repeatable {
            return Err(format!("{} is already researched", t.get().name));
        }
        let mut order = vec![];
        self.collect_missing(t, &mut order);
        let before = self.queue.len();
        for x in order {
            if Some(x) != self.current && !self.queue.contains(&x) {
                self.queue.push(x);
            }
        }
        let added = self.queue.len() - before;
        if self.current.is_none() {
            self.start_next_queued();
        }
        if added == 0 && self.current != Some(t) {
            return Err(format!("{} is already queued", t.get().name));
        }
        Ok(added)
    }

    fn collect_missing(&self, t: TechId, out: &mut Vec<TechId>) {
        if out.contains(&t) {
            return;
        }
        if self.is_done(t) && !(t.get().repeatable && out.is_empty()) {
            return;
        }
        for p in t.get().prereqs {
            if let Some(p) = TechId::by_id(p) {
                self.collect_missing(p, out);
            }
        }
        out.push(t);
    }

    pub fn unqueue(&mut self, t: TechId) -> bool {
        let n = self.queue.len();
        self.queue.retain(|&q| q != t);
        n != self.queue.len()
    }

    /// Marks `t` researched (scenario head starts, and tests).
    pub fn complete(&mut self, t: TechId) {
        self.progress.insert(t, t.get().units);
        self.done.insert(t);
        if self.current == Some(t) {
            self.current = None;
        }
    }

    /// Every tech except the endless ones is researched.
    pub fn all_done(&self) -> bool {
        TECHS
            .iter()
            .enumerate()
            .all(|(i, t)| t.repeatable || self.done.contains(&TechId(i as u16)))
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

    #[test]
    fn queueing_pulls_in_prerequisites_and_runs_in_order() {
        let mut r = Research::default();
        let optics = TechId::by_id("optics").unwrap();
        // Optics needs Logic theory, which needs Fabrication.
        assert_eq!(r.enqueue(optics).unwrap(), 3);
        assert_eq!(r.current, TechId::by_id("fabrication"));
        let mut finished = vec![];
        for _ in 0..200 {
            if let Some(t) = r.add_unit() {
                finished.push(t.get().id);
            }
        }
        assert_eq!(finished, vec!["fabrication", "logic", "optics"]);
        assert!(r.queue.is_empty());
        assert!(r.enqueue(optics).is_err());
    }

    #[test]
    fn endless_research_keeps_going_and_gets_dearer() {
        let mut r = Research::default();
        let servo = TechId::by_id("servo-tuning").unwrap();
        for t in TechId::all() {
            if t != servo && !t.get().repeatable {
                r.complete(t);
            }
        }
        assert!(r.all_done());
        r.start(servo).unwrap();
        let base = servo.get().units;
        let mut levels = vec![];
        for _ in 0..base * 6 {
            if r.add_unit().is_some() {
                levels.push(r.level(servo));
            }
        }
        // 1 + 2 + 3 bases of units for three levels.
        assert_eq!(levels, vec![1, 2, 3]);
        assert_eq!(r.current, Some(servo));
        assert_eq!(r.units_needed(servo), base * 4);
        assert!(r.is_available(servo));
        // Something queued takes over when a level finishes, and the endless tech can be queued again.
        let mining = TechId::by_id("mining-productivity").unwrap();
        r.enqueue(mining).unwrap();
        while r.current == Some(servo) {
            r.add_unit();
        }
        assert_eq!(r.current, Some(mining));
        assert_eq!(r.level(servo), 4);
        assert_eq!(r.enqueue(servo).unwrap(), 1);
    }
}
