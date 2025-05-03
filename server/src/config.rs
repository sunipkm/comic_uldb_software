use std::{fs::File, path::{Path, PathBuf}, time::Duration};

use packet::CameraConfig;
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProgConfig {
    pub progname: String,
    pub rootdir: String,
    pub bindaddr: String,
    pub bindport: u16,
    pub camconf: CameraConfig,
    pub i2cdev: PathBuf,
    pub i2c_cadence: Duration,
    pub bnosensors: Vec<(String, u8)>,
    pub mcpsensors: Vec<(String, u8)>,
    pub gpsdev: String,
    pub gpsbaud: u32,
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_config() {
        println!("{:#?}", ProgConfig::from_file(Path::new("test/config.json")));
    }
}