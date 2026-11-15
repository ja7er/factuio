//! Speedrun records: how fast each win was, kept next to the saves.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub seed: u64,
    /// Game ticks from the start to the win.
    pub ticks: u64,
    /// Scenario id; empty for free play.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scenario: String,
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
    /// The best earlier time on the same seed, and on any seed, in the same scenario.
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
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Adds a win. Runs only compare with others from the same scenario.
    pub fn add(&mut self, scenario: &str, seed: u64, ticks: u64) -> Placing {
        let same = || self.runs.iter().filter(|r| r.scenario == scenario);
        let prev_seed_best = same().filter(|r| r.seed == seed).map(|r| r.ticks).min();
        let prev_best = same().map(|r| r.ticks).min();
        self.runs.push(Run {
            seed,
            ticks,
            scenario: scenario.to_string(),
        });
        Placing {
            ticks,
            prev_seed_best,
            prev_best,
        }
    }

    /// The fastest win in a scenario.
    pub fn best(&self, scenario: &str) -> Option<u64> {
        self.runs
            .iter()
            .filter(|r| r.scenario == scenario)
            .map(|r| r.ticks)
            .min()
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
        let first = r.add("", 7, 1000);
        assert!(first.is_record() && first.is_seed_record());
        let slower = r.add("", 7, 1500);
        assert!(!slower.is_record() && !slower.is_seed_record());
        let other = r.add("", 9, 1200);
        assert!(other.is_seed_record() && !other.is_record());
        // Challenges keep their own records.
        let sprint = r.add("sprint", 777, 5000);
        assert!(sprint.is_record() && sprint.prev_best.is_none());
        assert_eq!(r.best(""), Some(1000));
        assert_eq!(r.best("sprint"), Some(5000));
        r.save(&path).unwrap();
        assert_eq!(Records::load(&path), r);
        std::fs::remove_dir_all(&dir).ok();
    }
}
