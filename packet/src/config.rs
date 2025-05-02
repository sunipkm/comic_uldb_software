use refimage::{OptimumExposure, OptimumExposureBuilder};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::Path, time::Duration};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OptimumExposureConf {
    percentile_pix: f32,
    pixel_tgt: f32,
    pixel_uncertainty: f32,
    pixel_exclusion: u32,
    min_allowed_exp: Option<Duration>,
    max_allowed_exp: Option<Duration>,
    max_allowed_bin: Option<u16>,
}

impl Default for OptimumExposureConf {
    fn default() -> Self {
        Self {
            percentile_pix: 95.0,
            pixel_tgt: 30000.0 / 65536.0,
            pixel_uncertainty: 2000.0 / 65536.0,
            pixel_exclusion: 100,
            min_allowed_exp: Some(Duration::from_secs(1)),
            max_allowed_exp: Some(Duration::from_secs(120)),
            max_allowed_bin: None,
        }
    }
}

impl OptimumExposureConf {
    pub fn get_controller(self) -> OptimumExposureBuilder {
        let mut builder = OptimumExposureBuilder::default()
            .percentile_pix(self.percentile_pix)
            .pixel_tgt(self.pixel_tgt)
            .pixel_uncertainty(self.pixel_uncertainty)
            .pixel_exclusion(self.pixel_exclusion);
        if let Some(max_bin) = self.max_allowed_bin {
            builder = builder.max_allowed_bin(max_bin);
        }
        if let Some(min_exp) = self.min_allowed_exp {
            builder = builder.min_allowed_exp(min_exp);
        }
        if let Some(max_exp) = self.max_allowed_exp {
            builder = builder.max_allowed_exp(max_exp);
        }
        builder
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CameraSettings {
    pub cadence: Duration,
    pub gain: Option<f64>,
    pub target_temp: f32,
    pub pix8b: bool,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            cadence: Duration::from_secs(10),
            gain: None,
            target_temp: -10.0,
            pix8b: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CameraRoi {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl CameraRoi {
    pub fn change_roi(&self) -> bool {
        self.x_min != 0 || self.x_max != 0 || self.y_min != 0 || self.y_max != 0
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CameraImageSav {
    pub savedir: String,
    pub save_fits: bool,
    pub save_png: bool,
}

impl Default for CameraImageSav {
    fn default() -> Self {
        Self {
            savedir: "./imagedata".to_string(),
            save_fits: false,
            save_png: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CameraConfig {
    pub name: Option<String>,
    pub settings: CameraSettings,
    pub roi: CameraRoi,
    pub autoexp: OptimumExposureConf,
    pub images: CameraImageSav,
}
