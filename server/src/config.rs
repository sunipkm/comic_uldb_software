use std::{fs::File, path::Path};

use packet::CameraConfig;
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProgConfig {
    pub progname: String,
    pub rootdir: String,
    pub camconf: CameraConfig,
}

impl ProgConfig {
    pub fn from_file(path: &Path) -> Result<Self, &'static str> {
        if !path.exists() {
            return Err("Config file does not exist");
        }
        let config =
            serde_json::from_reader(File::open(path).map_err(|_| "Failed to open config file")?)
                .map_err(|_| "Failed to parse config file")?;
        Ok(config)
    }
}