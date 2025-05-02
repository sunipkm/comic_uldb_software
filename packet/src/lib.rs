use std::collections::HashMap;
use chrono::{DateTime, Utc};
pub use config::{CameraConfig, CameraImageSav, CameraRoi, CameraSettings, OptimumExposureConf};
use generic_camera::{server::{GenSrvCmd, GenSrvOutput}, GenCamDescriptor};
use refimage::GenericImageOwned;
use serde::{Deserialize, Serialize};
use ublox_gps_tec::{GpsPacket, NmeaGpsInfo, NmeaMsgGroup, UbxGpsInfo};

pub mod config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CameraCommand {
    ExposureConf(u8, OptimumExposureConf),
    Settings(u8, CameraSettings),
    Roi(u8, CameraRoi),
    ImageSav(u8, CameraImageSav),
    FullConf(u8, CameraConfig),
}

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
    Ack(u8),
    Nack(u8),
    ImageData(GenericImageOwned),
    CamCommand(CameraCommand),
    TempData(TempReadout),
    GpsLocation(NmeaGpsInfo),
    GpsFullMessage(UbxGpsInfo),
    GpsRawMessage(Vec<u8>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TempReadout {
    pub now: DateTime<Utc>,
    pub readings: Vec<(String, f32)>,
}