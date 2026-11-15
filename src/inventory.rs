//! A simple item counter used by the player and by buildings.

use crate::items::Item;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory(BTreeMap<Item, u32>);

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self, item: Item) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }

    pub fn add(&mut self, item: Item, n: u32) {
        if n > 0 {
            *self.0.entry(item).or_insert(0) += n;
        }
    }

    /// Removes exactly `n` items, or nothing if there are fewer.
    pub fn remove(&mut self, item: Item, n: u32) -> bool {
        let have = self.count(item);
        if have < n {
            return false;
        }
        self.take_up_to(item, n);
        true
    }

    /// Removes up to `n` items and returns how many were removed.
    pub fn take_up_to(&mut self, item: Item, n: u32) -> u32 {
        let have = self.count(item);
        let take = have.min(n);
        if take == have {
            self.0.remove(&item);
        } else {
            self.0.insert(item, have - take);
        }
        take
    }

    pub fn has_all(&self, items: &[(Item, u32)], times: u32) -> bool {
        items.iter().all(|&(i, n)| self.count(i) >= n * times)
    }

    pub fn remove_all(&mut self, items: &[(Item, u32)], times: u32) -> bool {
        if !self.has_all(items, times) {
            return false;
        }
        for &(i, n) in items {
            self.remove(i, n * times);
        }
        true
    }

    pub fn add_all(&mut self, items: &[(Item, u32)], times: u32) {
        for &(i, n) in items {
            self.add(i, n * times);
        }
    }

    pub fn total(&self) -> u32 {
        self.0.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Item, u32)> + '_ {
        self.0.iter().map(|(&i, &n)| (i, n))
    }

    pub fn items(&self) -> Vec<Item> {
        self.0.keys().copied().collect()
    }

    pub fn first(&self) -> Option<Item> {
        self.0.keys().next().copied()
    }

    pub fn drain_into(&mut self, other: &mut Inventory) {
        for (i, n) in std::mem::take(&mut self.0) {
            other.add(i, n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove() {
        let mut inv = Inventory::new();
        inv.add(Item::Cog, 5);
        assert!(!inv.remove(Item::Cog, 6));
        assert!(inv.remove(Item::Cog, 5));
        assert!(inv.is_empty());
        inv.add(Item::Coil, 3);
        assert_eq!(inv.take_up_to(Item::Coil, 10), 3);
        assert_eq!(inv.count(Item::Coil), 0);
    }

    #[test]
    fn multi() {
        let mut inv = Inventory::new();
        inv.add(Item::IronIngot, 4);
        inv.add(Item::Coil, 6);
        let chip = [(Item::IronIngot, 1), (Item::Coil, 3)];
        assert!(inv.has_all(&chip, 2));
        assert!(!inv.has_all(&chip, 3));
        assert!(inv.remove_all(&chip, 2));
        assert_eq!(inv.count(Item::IronIngot), 2);
        assert_eq!(inv.count(Item::Coil), 0);
    }

    #[test]
    fn serde_round_trip() {
        let mut inv = Inventory::new();
        inv.add(Item::Lignite, 7);
        let s = serde_json::to_string(&inv).unwrap();
        let back: Inventory = serde_json::from_str(&s).unwrap();
        assert_eq!(inv, back);
    }
}
