//! Speedrun records: how fast each win was, kept next to the saves.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub seed: u64,
    /// Game ticks from the start to the 20th transmission.
    pub ticks: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Records {
    #[serde(default)]
    pub runs: Vec<Run>,
}

/// How a new win compares with the earlier ones.
#[derive(Clone, Debug, PartialEq)]
pub struct Placing {
    pub ticks: u64,
    /// The best earlier time on the same seed, and overall.
    pub prev_seed_best: Option<u64>,
    pub prev_best: Option<u64>,
}

impl Placing {
    pub fn is_seed_record(&self) -> bool {
        self.prev_seed_best.is_none_or(|b| self.ticks < b)
    }

    pub fn is_record(&self) -> bool {
        self.prev_best.is_none_or(|b| self.ticks < b)
    }
}

impl Records {
    /// Reads the records file. A missing or unreadable file counts as no records.
    pub fn load(path: &Path) -> Records {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn add(&mut self, seed: u64, ticks: u64) -> Placing {
        let prev_seed_best = self
            .runs
            .iter()
            .filter(|r| r.seed == seed)
            .map(|r| r.ticks)
            .min();
        let prev_best = self.runs.iter().map(|r| r.ticks).min();
        self.runs.push(Run { seed, ticks });
        Placing {
            ticks,
            prev_seed_best,
            prev_best,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_compare_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("factuio-records-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("records.json");
        let mut r = Records::load(&path);
        let first = r.add(7, 1000);
        assert!(first.is_record() && first.is_seed_record());
        let slower = r.add(7, 1500);
        assert!(!slower.is_record() && !slower.is_seed_record());
        let other = r.add(9, 1200);
        assert!(other.is_seed_record() && !other.is_record());
        r.save(&path).unwrap();
        assert_eq!(Records::load(&path), r);
        std::fs::remove_dir_all(&dir).ok();
    }
}
