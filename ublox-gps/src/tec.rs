use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ubx::{Frequency, TrkStat},
    uncertain::Uncertain,
    GnssFreq, GnssSatellite, UbxGpsInfo,
};

fn factor(f1: f64, f2: f64) -> f64 {
    const K: f64 = 1e-16 / 40.308;
    let a = f1 * f1;
    let b = f2 * f2;
    K * a * b / (a - b)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Inferred Total Electron Content information
/// from carrier phase measurements of dual-frequency
/// GNSS receivers.
pub struct TecData {
    source: GnssSatellite,
    pointing: (u16, i8),
    channels: (GnssFreq, GnssFreq),
    phase_tec: Option<Uncertain<f64>>,
    range_tec: Option<Uncertain<f64>>,
    trk_stat: (TrkStat, TrkStat),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Total Electron Content information
/// derived from carrier phase measurements
/// of dual-frequency GNSS receivers in [`UbxGpsInfo`].
pub struct TecInfo {
    timestamp: DateTime<Utc>,
    location: (f64, f64, f32),
    tec: Vec<TecData>,
}

impl TecInfo {
    /// Assimilate carrier phase measurements from a [`UbxGpsInfo`] object
    /// to extract Total Electron Content information.
    pub fn assimilate(src: &UbxGpsInfo) -> Option<Self> {
        let timestamp = src.timestamp();
        let location = src.location();
        let mut tec = Vec::new();
        for (sat, ch) in src.carrier_phase() {
            if ch.meas.len() < 2 {
                continue;
            }
            let m0 = &ch.meas[0];
            let m1 = &ch.meas[1];
            let f0 = m0.channel.get_freq();
            let f1 = m1.channel.get_freq();
            let fac = factor(f0, f1);
            let fac = Uncertain::new(fac, 0.0);
            const SPEED_OF_LIGHT: f64 = 299_792_458.0;
            let phase_tec = if let Some((m0_phase, m0_perr)) = m0.carrier_phase {
                if let Some((m1_phase, m1_perr)) = m1.carrier_phase {
                    let m0_phase = Uncertain::new(m0_phase, m0_perr as _);
                    let m1_phase = Uncertain::new(m1_phase, m1_perr as _);
                    let phase_tec = ((m0_phase * (SPEED_OF_LIGHT / f0).into())
                        - (m1_phase * (SPEED_OF_LIGHT / f1).into()))
                        * fac;
                    Some(phase_tec)
                } else {
                    None
                }
            } else {
                None
            };
            let range_tec = if let Some((m0_range, m0_rerr)) = m0.pseudo_range {
                if let Some((m1_range, m1_rerr)) = m1.pseudo_range {
                    let m0_range = Uncertain::new(m0_range, m0_rerr as _);
                    let m1_range = Uncertain::new(m1_range, m1_rerr as _);
                    let range_tec = (m1_range - m0_range) * fac;
                    Some(range_tec)
                } else {
                    None
                }
            } else {
                None
            };
            if phase_tec.is_none() && range_tec.is_none() {
                continue;
            }
            let pointing = (ch.azimuth, ch.elevation);
            let channels = (m0.channel, m1.channel);
            let trk_stat = (m0.trk_stat, m1.trk_stat);
            tec.push(TecData {
                source: *sat,
                pointing,
                channels,
                phase_tec,
                range_tec,
                trk_stat,
            });
        }

        if tec.is_empty() {
            None
        } else {
            tec.sort_by(|a, b| a.source.cmp(&b.source));
            Some(TecInfo {
                timestamp,
                location,
                tec,
            })
        }
    }

    /// Get the timestamp of the TEC information
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get the location of the TEC information
    /// in (latitude, longitude, altitude) format
    pub fn location(&self) -> (f64, f64, f32) {
        self.location
    }

    /// Get the TEC information
    pub fn tec(&self) -> &Vec<TecData> {
        &self.tec
    }
}

impl TecData {
    /// Get the satellite source of the TEC data
    pub fn source(&self) -> GnssSatellite {
        self.source
    }

    /// Get the azimuth of the source satellite
    pub fn azimuth(&self) -> u16 {
        self.pointing.0
    }

    /// Get the elevation of the source satellite
    pub fn elevation(&self) -> i8 {
        self.pointing.1
    }

    /// Get the carrier frequency channels of the TEC data
    pub fn channels(&self) -> (GnssFreq, GnssFreq) {
        self.channels
    }

    /// Get the phase TEC information
    pub fn phase_tec(&self) -> Option<Uncertain<f64>> {
        self.phase_tec
    }

    /// Get the range TEC information
    pub fn range_tec(&self) -> Option<Uncertain<f64>> {
        self.range_tec
    }

    /// Get the signal status of the TEC data source
    pub fn signal_status(&self) -> (TrkStat, TrkStat) {
        self.trk_stat
    }
}
