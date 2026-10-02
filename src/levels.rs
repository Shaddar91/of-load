//Stress levels: the mounted or embedded levels.json, clamped to the configured limits.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const EMBEDDED: &str = include_str!("../levels.json");
const NAMES: [&str; 3] = ["easy", "medium", "high"];

#[derive(Deserialize, Serialize)]
pub struct Level {
    pub name: String,
    pub concurrency: u64,
    pub duration_seconds: u64,
    pub cpu_ms: u64,
    pub mem_mib: u64,
}

#[derive(Deserialize, Serialize)]
pub struct Levels {
    levels: Vec<Level>,
}

impl Levels {
    pub fn load(file: Option<&Path>, max_cpu_ms: u64, max_mem_mib: u64) -> Result<Self, String> {
        let raw = match file {
            Some(path) => fs::read_to_string(path)
                .map_err(|error| format!("STRESS_LEVELS_FILE {}: {error}", path.display()))?,
            None => EMBEDDED.to_owned(),
        };
        let parsed: Self =
            serde_json::from_str(&raw).map_err(|error| format!("levels file: {error}"))?;
        let levels = NAMES
            .iter()
            .map(|name| {
                parsed
                    .levels
                    .iter()
                    .find(|level| level.name == *name)
                    .map(|level| level.clamped(max_cpu_ms, max_mem_mib))
                    .ok_or_else(|| format!("levels file: level {name} is missing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { levels })
    }

    pub fn find(&self, name: &str) -> Option<&Level> {
        self.levels.iter().find(|level| level.name == name)
    }
}

impl Level {
    fn clamped(&self, max_cpu_ms: u64, max_mem_mib: u64) -> Self {
        Self {
            name: self.name.clone(),
            concurrency: self.concurrency.clamp(1, 64),
            duration_seconds: self.duration_seconds.clamp(1, 900),
            cpu_ms: self.cpu_ms.min(max_cpu_ms),
            mem_mib: self.mem_mib.min(max_mem_mib),
        }
    }
}
