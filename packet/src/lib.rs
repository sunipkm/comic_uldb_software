use std::time::Duration;

pub use config::{CameraConfig, CameraRoi, CameraSettings, OptimumExposureConf};
use mint::Quaternion;
use refimage::GenericImageOwned;
use serde::{Deserialize, Serialize};
use ublox_gps_tec::{NmeaGpsInfo, UbxGpsInfo};

pub mod config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CameraCommand {
    ExposureConf(u8, OptimumExposureConf),
    Settings(u8, CameraSettings),
    Roi(u8, CameraRoi),
    FullConf(u8, CameraConfig),
}

#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Packet {
    Outgoing(Outgoing),
    Incoming(CameraCommand),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Outgoing {
    Ack(u8),
    Nack(u8),
    ImageData(GenericImageOwned),
    TempData(TempReadout),
    OrientationData(QuaternionReadout),
    GpsLocation(Duration, NmeaGpsInfo),
    GpsFullMessage(Duration, UbxGpsInfo),
    GpsRawMessage(GpsRawMessage),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TempReadout {
    pub now: Duration,
    pub readings: (String, i32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpsRawMessage {
    pub now: Duration,
    pub msg: Vec<u8>,
}

impl From<Outgoing> for Packet {
    fn from(packet: Outgoing) -> Self {
        Packet::Outgoing(packet)
    }
}

impl From<TempReadout> for Outgoing {
    fn from(readout: TempReadout) -> Self {
        Outgoing::TempData(readout)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuaternionReadout {
    pub now: Duration,
    pub readings: (String, Quaternion<f32>),
}

impl From<QuaternionReadout> for Outgoing {
    fn from(readout: QuaternionReadout) -> Self {
        Outgoing::OrientationData(readout)
    }
}

impl From<GenericImageOwned> for Outgoing {
    fn from(image: GenericImageOwned) -> Self {
        Outgoing::ImageData(image)
    }
}

impl TryFrom<Packet> for CameraCommand {
    type Error = String;

    fn try_from(packet: Packet) -> Result<Self, Self::Error> {
        match packet {
            Packet::Incoming(cmd) => Ok(cmd),
            _ => Err("Not an incoming packet".to_string()),
        }
    }
}

impl From<(Duration, NmeaGpsInfo)> for Outgoing {
    fn from(gps: (Duration, NmeaGpsInfo)) -> Self {
        Outgoing::GpsLocation(gps.0, gps.1)
    }
}

impl From<(Duration, UbxGpsInfo)> for Outgoing {
    fn from(gps: (Duration, UbxGpsInfo)) -> Self {
        Outgoing::GpsFullMessage(gps.0, gps.1)
    }
}

impl From<GpsRawMessage> for Outgoing {
    fn from(gps: GpsRawMessage) -> Self {
        Outgoing::GpsRawMessage(gps)
    }
}
