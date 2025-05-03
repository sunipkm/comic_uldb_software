#![deny(missing_docs)]
//! # UBX Parser
//!
//! This crate provides a parser for UBX messages from a serial port.

use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};

use bitfield_struct::bitfield;
use chrono::{DateTime, TimeDelta, Utc};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::{nmea::{GnssSatellite, NmeaGpsInfo}, NmeaMsgGroup};

const GPS_EPOCH: DateTime<Utc> = DateTime::from_timestamp_nanos(315_964_800_000_000_000);

#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
/// UBX message classes
pub enum UbxClass {
    /// Receiver messages
    Receiver(UbxRxm) = 0x2,
    /// Acknowledgement messages
    Ack(UbxAck) = 0x5,
}

impl TryFrom<(u8, u8)> for UbxClass {
    type Error = &'static str;

    fn try_from(value: (u8, u8)) -> Result<Self, Self::Error> {
        let (cls, id) = value;
        let res = match cls {
            0x2 => UbxClass::Receiver({
                match id {
                    0x14 => UbxRxm::MeasX,
                    0x15 => UbxRxm::RawX,
                    0x59 => UbxRxm::Rlm,
                    0x13 => UbxRxm::SfrbX,
                    _ => {
                        warn!("Invalid UBX RXM ID: {}", id);
                        return Err("Invalid UBX RXM ID");
                    }
                }
            }),
            0x5 => UbxClass::Ack({
                match id {
                    0x1 => UbxAck::Ack,
                    0x0 => UbxAck::Nack,
                    _ => {
                        warn!("Invalid UBX ACK ID: {}", id);
                        return Err("Invalid UBX ACK ID");
                    }
                }
            }),
            _ => {
                warn!("Invalid UBX class ID: {}", cls);
                return Err("Invalid UBX class ID");
            }
        };
        Ok(res)
    }
}

#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
/// UBX message acknowledgement types
pub enum UbxAck {
    /// Ack
    Ack = 0x1,
    /// Nack
    Nack = 0x0,
}

#[non_exhaustive]
#[repr(u8)]
#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
/// UBX RXM message types
pub enum UbxRxm {
    /// Measured navigation data
    MeasX = 0x14,
    /// Raw receiver measurements
    RawX = 0x15,
    /// Galileo SAR RLM report
    Rlm = 0x59,
    /// Broadcast Navigation Data Subframe
    SfrbX = 0x13,
}

/// Convert UBX message to a specific UBX format
pub trait UbxFormat {
    /// Convert UBX message to a specific UBX format
    fn from_message(message: UbxMessage) -> Result<Self, &'static str>
    where
        Self: Sized;
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
/// A GNSS frequency channel
pub enum GnssFreq {
    /// GPS frequency channel
    Gps(GpsFreq),
    /// Galileo frequency channel
    Galileo(GalileoFreq),
    /// Beidou frequency channel
    Beidou(BeidouFreq),
    /// Glonass frequency channel
    Glonass(GlonassFreq),
    /// QZSS frequency channel
    Qzss(QzssFreq),
}

impl Frequency for GnssFreq {
    fn get_freq(&self) -> f64 {
        match self {
            GnssFreq::Gps(freq) => freq.get_freq(),
            GnssFreq::Galileo(freq) => freq.get_freq(),
            GnssFreq::Beidou(freq) => freq.get_freq(),
            GnssFreq::Glonass(freq) => freq.get_freq(),
            GnssFreq::Qzss(freq) => freq.get_freq(),
        }
    }
}

fn parse_sat_ids(
    gnss_id: u8,
    sat_id: u8,
    sig_id: u8,
    glonass: i8,
) -> Result<(GnssSatellite, GnssFreq), &'static str> {
    use GnssSatellite::*;
    let sat = GnssSatellite::from_ubx(gnss_id, sat_id);
    let freq = match sat {
        Gps(_) => GpsFreq::try_from(sig_id)?.into(),
        Sbas(_) => GpsFreq::try_from(sig_id)?.into(),
        Galileo(_) => GalileoFreq::try_from(sig_id)?.into(),
        Beidou(_) => BeidouFreq::try_from(sig_id)?.into(),
        Qzss(_) => QzssFreq::try_from(sig_id)?.into(),
        Glonass(_) => GlonassFreq::try_from((sig_id, glonass))?.into(),
    };
    Ok((sat, freq))
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
/// GPS frequency channels
pub enum GpsFreq {
    /// GPS L1 C/A frequency
    L1CA,
    /// GPS L2C-L frequency
    L2CL,
    /// GPS L2C-M frequency
    L2CM,
    /// GPS L5 frequency
    L5,
}

impl From<GpsFreq> for GnssFreq {
    fn from(val: GpsFreq) -> Self {
        GnssFreq::Gps(val)
    }
}

impl TryFrom<u8> for GpsFreq {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GpsFreq::L1CA),
            3 => Ok(GpsFreq::L2CL),
            4 => Ok(GpsFreq::L2CM),
            6 | 7 => Ok(GpsFreq::L5),
            _ => {
                warn!("Invalid GPS frequency ID: {}", value);
                Err("Invalid GPS frequency ID")
            }
        }
    }
}

impl Frequency for GpsFreq {
    fn get_freq(&self) -> f64 {
        use GpsFreq::*;
        match self {
            L1CA => 1575.42e6,
            L2CL | L2CM => 1227.60e6,
            L5 => 1176.45e6,
        }
    }
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
/// Galileo frequency channels
pub enum GalileoFreq {
    /// Galileo E1C frequency
    E1C,
    /// Galileo E1B frequency
    E1B,
    /// Galileo E5a-I frequency
    E5aI,
    /// Galileo E5a-Q frequency
    E5aQ,
    /// Galileo E5b-I frequency
    E5bI,
    /// Galileo E5b-Q frequency
    E5bQ,
}

impl From<GalileoFreq> for GnssFreq {
    fn from(val: GalileoFreq) -> Self {
        GnssFreq::Galileo(val)
    }
}

impl TryFrom<u8> for GalileoFreq {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GalileoFreq::E1C),
            1 => Ok(GalileoFreq::E1B),
            3 => Ok(GalileoFreq::E5aI),
            4 => Ok(GalileoFreq::E5aQ),
            5 => Ok(GalileoFreq::E5bI),
            6 => Ok(GalileoFreq::E5bQ),
            _ => {
                warn!("Invalid Galileo frequency ID: {}", value);
                Err("Invalid Galileo frequency ID")
            }
        }
    }
}

impl Frequency for GalileoFreq {
    fn get_freq(&self) -> f64 {
        use GalileoFreq::*;
        match self {
            E1C | E1B => 1575.42e6,
            E5aI | E5aQ => 1176.45e6,
            E5bI | E5bQ => 1207.14e6,
        }
    }
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
/// Beidou frequency channels
pub enum BeidouFreq {
    /// Beidou B1I D1 frequency
    B1I_D1,
    /// Beidou B1I D2 frequency
    B1I_D2,
    /// Beidou B2I D1 frequency
    B2I_D1,
    /// Beidou B2I D2 frequency
    B2I_D2,
    /// Beidou B2A frequency
    B2A,
}

impl Frequency for BeidouFreq {
    fn get_freq(&self) -> f64 {
        use BeidouFreq::*;
        match self {
            B1I_D1 | B1I_D2 => 1561.098e6,
            B2I_D1 | B2I_D2 => 1207.14e6,
            B2A => 1176.45e6,
        }
    }
}

impl From<BeidouFreq> for GnssFreq {
    fn from(val: BeidouFreq) -> Self {
        GnssFreq::Beidou(val)
    }
}

impl TryFrom<u8> for BeidouFreq {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(BeidouFreq::B1I_D1),
            1 => Ok(BeidouFreq::B1I_D2),
            2 => Ok(BeidouFreq::B2I_D1),
            3 => Ok(BeidouFreq::B2I_D2),
            7 => Ok(BeidouFreq::B2A),
            _ => {
                warn!("Invalid Beidou frequency ID: {}", value);
                Err("Invalid Beidou frequency ID")
            }
        }
    }
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
/// Glonass frequency channels
pub enum GlonassFreq {
    /// Glonass L1OF frequency (Channel: -7 to 6)
    L1OF(i8),
    /// Glonass L2OF frequency (Channel: -7 to 6)
    L2OF(i8),
}

impl From<GlonassFreq> for GnssFreq {
    fn from(val: GlonassFreq) -> Self {
        GnssFreq::Glonass(val)
    }
}

impl TryFrom<(u8, i8)> for GlonassFreq {
    type Error = &'static str;

    fn try_from(value: (u8, i8)) -> Result<Self, Self::Error> {
        let (value, channel) = value;
        match value {
            0 => Ok(GlonassFreq::L1OF(channel)),
            2 => Ok(GlonassFreq::L2OF(channel)),
            _ => {
                warn!("Invalid Glonass frequency ID: {}", value);
                Err("Invalid Glonass frequency ID")
            }
        }
    }
}

impl Frequency for GlonassFreq {
    fn get_freq(&self) -> f64 {
        use GlonassFreq::*;
        match self {
            L1OF(k) => 1602.0e6 + 562.5e3 * (*k as f64),
            L2OF(k) => 1246.0e6 + 437.5e3 * (*k as f64),
        }
    }
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
/// QZSS frequency channels
pub enum QzssFreq {
    /// QZSS L1CA frequency
    L1CA,
    /// QZSS L1S frequency
    L1S,
    /// QZSS L2CM frequency
    L2CM,
    /// QZSS L2CL frequency
    L2CL,
    /// QZSS L5 frequency
    L5,
}

impl From<QzssFreq> for GnssFreq {
    fn from(val: QzssFreq) -> Self {
        GnssFreq::Qzss(val)
    }
}

impl Frequency for QzssFreq {
    fn get_freq(&self) -> f64 {
        match self {
            QzssFreq::L1CA => 1575.42e6,
            QzssFreq::L1S => 1575.42e6,
            QzssFreq::L2CM => 1227.60e6,
            QzssFreq::L2CL => 1227.60e6,
            QzssFreq::L5 => 1176.45e6,
        }
    }
}

impl TryFrom<u8> for QzssFreq {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(QzssFreq::L1CA),
            1 => Ok(QzssFreq::L1S),
            4 => Ok(QzssFreq::L2CM),
            5 => Ok(QzssFreq::L2CL),
            7 | 8 => Ok(QzssFreq::L5),
            _ => {
                warn!("Invalid QZSS frequency ID: {}", value);
                Err("Invalid QZSS frequency ID")
            }
        }
    }
}

/// Get the channel frequency
pub(crate) trait Frequency {
    /// Get the frequency associated with the GPS channel
    fn get_freq(&self) -> f64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// UBX RXM-RAWX message
pub struct UbxRxmRawx {
    /// Timestamp of the message
    pub timestamp: DateTime<Utc>,
    /// Receiver status
    pub receiver_status: RecvStat,
    /// Message version (0x1)
    pub version: u8,
    /// Carrier pseudorange, phase and Doppler measurements
    pub meas: HashMap<GnssSatellite, Vec<CarrierMeas>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Carrier phase and Doppler measurements
pub struct CarrierMeas {
    /// GNSS satellite and frequency channel
    pub channel: GnssFreq,
    /// Pseudo-range measurement and standard deviation (m)
    pub pseudo_range: Option<(f64, f32)>,
    /// Carrier-phase measurement and standard deviation (Hz)
    pub carrier_phase: Option<(f64, f32)>,
    /// Doppler measurement and standard deviation (Hz)
    ///
    /// # Note: Positive Doppler indicates satellite moving towards the receiver
    pub doppler: (f32, f32),
    /// Carrier phase locktime counter (ms, max. 64500 ms)
    pub locktime: u16,
    /// Carrier-to-noise ratio (dB-Hz)
    pub carrier_snr: u8,
    /// Tracking status and phase lock flags
    pub trk_stat: TrkStat,
}

#[bitfield(u8)]
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
/// Tracking status and phase lock flags for carrier phase and pseudo-range measurements
pub struct TrkStat {
    #[bits(1)]
    /// Pseudo-range measurement is valid
    pr_valid: bool,
    #[bits(1)]
    /// Carrier-phase measurement is valid
    cp_valid: bool,
    #[bits(1)]
    /// Half-cycle ambiguity is fixed
    half_cycle: bool,
    #[bits(1)]
    /// Sub-half-cycle ambiguity is fixed
    sub_half_cycle: bool,
    #[bits(4)]
    _reserved: u8,
}

#[bitfield(u8)]
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
/// Receiver status flags
pub struct RecvStat {
    #[bits(1)]
    /// Leap second information is available
    leap_second_ready: bool,
    #[bits(1)]
    /// Receiver clock is reset. Usually the receiver clock
    /// is changed in increments of integer milliseconds.
    clk_reset: bool,
    #[bits(6)]
    _reserved: u8,
}

impl UbxRxmRawx {
    /// Remove carrier phase and pseudo-range measurements where only one frequency band is available
    pub fn remove_single_band(&mut self) {
        self.meas.retain(|_, v| v.len() > 1);
    }
}

impl UbxFormat for UbxRxmRawx {
    fn from_message(message: UbxMessage) -> Result<Self, &'static str>
    where
        Self: Sized,
    {
        if message.class != 0x2 {
            return Err("Invalid UBX message class");
        }
        if message.id != 0x15 {
            return Err("Invalid UBX message ID");
        }
        if message.payload.len() < 16 {
            return Err("Invalid UBX message length, malformed message");
        }
        let num_mes = (message.payload.len() - 16) / 32;
        if message.payload[11] != num_mes as u8 {
            warn!(
                "Invalid number of measurements: {} != {}",
                message.payload[11], num_mes
            );
            return Err("Invalid number of measurements, malformed message");
        }
        let time_of_week = f64::from_le_bytes(
            message.payload[0..8]
                .try_into()
                .map_err(|_| "Failed to convert bytes to f64")?,
        );
        let week = u16::from_le_bytes(
            message.payload[8..10]
                .try_into()
                .map_err(|_| "Failed to convert bytes to u16")?,
        );
        let leap_second = i8::from_le_bytes(
            message.payload[10..11]
                .try_into()
                .map_err(|_| "Failed to convert bytes to i8")?,
        );
        let mut week =
            TimeDelta::try_weeks(week as i64).ok_or("Failed to convert week to duration")?;
        let dur = Duration::from_secs_f64(time_of_week) + Duration::from_secs(leap_second as u64);
        week += TimeDelta::from_std(dur).map_err(|_| "Failed to convert duration to time delta")?;
        let mut msg = UbxRxmRawx {
            timestamp: GPS_EPOCH + week,
            receiver_status: message.payload[12].into(),
            version: message.payload[13],
            meas: Default::default(),
        };
        for i in 0..num_mes {
            let start = 16 + i * 32;
            let gnss_id = message.payload[start + 20];
            let sat_id = message.payload[start + 21];
            let sig_id = message.payload[start + 22];
            let glonass = message.payload[start + 23] as i8;
            match parse_sat_ids(gnss_id, sat_id, sig_id, glonass) {
                Ok((sat, freq)) => {
                    let trk_stat: TrkStat = message.payload[start + 30].into();
                    let pr = if trk_stat.cp_valid() {
                        let pr = f64::from_le_bytes(
                            message.payload[start..start + 8]
                                .try_into()
                                .map_err(|_| "Failed to convert bytes to pseudo-range")?,
                        );
                        let pr_std =
                            0.01f32 * ((2i32.pow(message.payload[start + 27].into())) as f32);
                        Some((pr, pr_std))
                    } else {
                        None
                    };
                    let cp = if trk_stat.cp_valid() {
                        let cp = f64::from_le_bytes(
                            message.payload[start + 8..start + 16]
                                .try_into()
                                .map_err(|_| "Failed to convert bytes to carrier-phase")?,
                        );
                        let cp_std = 0.004f32 * message.payload[start + 28] as f32;
                        Some((cp, cp_std))
                    } else {
                        None
                    };
                    let doppler = {
                        let doppler = f32::from_le_bytes(
                            message.payload[start + 16..start + 20]
                                .try_into()
                                .map_err(|_| "Failed to convert bytes to doppler")?,
                        );
                        let doppler_std =
                            0.002f32 * ((2i32.pow(message.payload[start + 29].into())) as f32);
                        (doppler, doppler_std)
                    };
                    let locktime = u16::from_le_bytes(
                        message.payload[start + 24..start + 26]
                            .try_into()
                            .map_err(|_| "Failed to convert bytes to locktime")?,
                    );
                    let mes = {
                        CarrierMeas {
                            channel: freq,
                            pseudo_range: pr,
                            carrier_phase: cp,
                            doppler,
                            locktime,
                            carrier_snr: message.payload[start + 26],
                            trk_stat,
                        }
                    };
                    match msg.meas.entry(sat) {
                        Entry::Vacant(e) => {
                            e.insert(vec![mes]);
                        }
                        Entry::Occupied(mut e) => {
                            e.get_mut().push(mes);
                        }
                    }
                    for (_, v) in msg.meas.iter_mut() {
                        v.sort_by(|a, b| {
                            a.channel
                                .get_freq()
                                .partial_cmp(&b.channel.get_freq())
                                .expect("Failed to compare frequencies?")
                        });
                        v.reverse();
                    }
                }
                Err(e) => {
                    warn!("Error parsing satellite IDs: {e}, {gnss_id} {sat_id} {sig_id}");
                    continue;
                }
            }
        }
        Ok(msg)
    }
}

/// UBX message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbxMessage {
    /// Message class
    pub class: u8,
    /// Message ID
    pub id: u8,
    /// Message payload
    pub payload: Vec<u8>,
}

/// Remove UBX message bytes from buffer,
/// parse and return UBX messages, and return the remaining bytes
pub fn split_ubx(mut buf: Vec<u8>) -> (Vec<UbxMessage>, Vec<u8>) {
    let mut messages = Vec::with_capacity(1);
    while let Ok((start, end, class, id)) = find_rxm_raw(&buf) {
        let mut payload: Vec<u8> = buf.drain(start..end).collect();
        let mut payload = payload.split_off(6);
        let _ = payload.pop().unwrap();
        let _ = payload.pop().unwrap();
        messages.push(UbxMessage { class, id, payload });
    }
    (messages, buf)
}

fn find_rxm_raw(buf: &[u8]) -> Result<(usize, usize, u8, u8), &'static str> {
    let mut abs_start = 0;
    let buf = {
        let mut start = None;
        for i in 0..buf.len() {
            if buf[i] == 0xB5 {
                if let Some(idx) = buf.get(i + 1) {
                    if *idx == 0x62 {
                        abs_start = i;
                        start = Some(i + 2);
                    }
                } else {
                    break;
                }
            }
        }
        if let Some(start) = start {
            &buf[start..]
        } else {
            warn!("No UBX message found");
            return Err("No UBX message found");
        }
    };
    if buf.len() < 4 {
        return Err("Insufficient data to get packet length");
    }
    let class = buf[0];
    let id = buf[1];
    let length = u16::from_le_bytes(
        buf[2..4]
            .try_into()
            .map_err(|_| "Failed to convert bytes to u16")?,
    );
    let end = length as usize + 4;
    let abs_end = abs_start + 2 + end + 2;
    if end + 1 > buf.len() {
        return Err("Incomplete packet");
    }
    let (ck_a, ck_b) = rxm_checksum(&buf[..end]);
    if ck_a != buf[end] || ck_b != buf[end + 1] {
        return Err("Checksum mismatch");
    }
    Ok((abs_start, abs_end, class, id))
}

fn rxm_checksum(buf: &[u8]) -> (u8, u8) {
    let mut ck_a: u8 = 0;
    let mut ck_b: u8 = 0;
    for byte in buf {
        ck_a = ck_a.wrapping_add(*byte);
        ck_b = ck_b.wrapping_add(ck_a);
    }
    (ck_a, ck_b)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// U-Blox Satellite Carrier Phase Measurements
pub struct SatPathInfo {
    /// Satellite elevation angle (degrees)
    pub elevation: i8,
    /// Satellite azimuth (degrees)
    pub azimuth: u16,
    /// Pseudo-range and carrier phase measurements
    pub meas: Vec<CarrierMeas>,
}

/// U-Blox Combined GPS info and Carrier Phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbxGpsInfo {
    /// Timestamp of the message
    pub timestamp: DateTime<Utc>,
    /// Location of the fix
    loc: (f64, f64, f32),
    /// Altitude above mean sea level
    msl: f32,
    /// True heading
    true_heading: f32,
    /// Magnetic heading
    mag_heading: f32,
    /// Ground speed
    ground_speed: f32,
    /// Quality of the fix
    quality: u8,
    /// Horizontal dilution of precision
    hdop: f32,
    /// Vertical dilution of precision
    vdop: f32,
    /// Position dilution of precision
    pdop: f32,
    /// Position and carrier phase measurements
    meas: HashMap<GnssSatellite, SatPathInfo>,
    /// Receiver status
    receiver_status: Option<RecvStat>,
    /// Raw NMEA messages
    nmea_raw: NmeaMsgGroup,
}

impl UbxGpsInfo {
    /// Create a new UBX GPS info struct
    pub fn new(nmea: NmeaGpsInfo, rxm: Option<UbxRxmRawx>, nmea_raw: NmeaMsgGroup) -> Self {
        let mut meas = HashMap::new();
        let mut recv_stat = None;
        if let Some(rxm) = rxm {
            for (sat, v) in rxm.meas {
                let (el, az) = nmea.sat_views.get(&sat).unwrap_or(&(-1, 0));
                meas.insert(
                    sat,
                    SatPathInfo {
                        elevation: *el,
                        azimuth: *az,
                        meas: v,
                    },
                );
            }
            recv_stat = Some(rxm.receiver_status);
        }
        UbxGpsInfo {
            timestamp: nmea.time,
            loc: nmea.loc,
            msl: nmea.msl,
            true_heading: nmea.true_heading,
            mag_heading: nmea.mag_heading,
            ground_speed: nmea.ground_speed,
            quality: nmea.quality,
            hdop: nmea.hdop,
            vdop: nmea.vdop,
            pdop: nmea.pdop,
            meas,
            receiver_status: recv_stat,
            nmea_raw,
        }
    }

    /// Get the timestamp of the message
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get the location of the fix
    ///
    /// Returns a tuple of (latitude in deg, longitude in deg, altitude in m)
    pub fn location(&self) -> (f64, f64, f32) {
        self.loc
    }

    /// Get the altitude above mean sea level (m)
    pub fn msl(&self) -> f32 {
        self.msl
    }

    /// Get the true heading (degrees)
    pub fn true_heading(&self) -> f32 {
        self.true_heading
    }

    /// Get the magnetic heading (degrees)
    pub fn mag_heading(&self) -> f32 {
        self.mag_heading
    }

    /// Get the ground speed (km/h)
    pub fn ground_speed(&self) -> f32 {
        self.ground_speed
    }

    /// Get the quality of the fix
    pub fn quality(&self) -> u8 {
        self.quality
    }

    /// Get the horizontal dilution of precision
    pub fn hdop(&self) -> f32 {
        self.hdop
    }

    /// Get the vertical dilution of precision
    pub fn vdop(&self) -> f32 {
        self.vdop
    }

    /// Get the position dilution of precision
    pub fn pdop(&self) -> f32 {
        self.pdop
    }

    /// Get the receiver status
    pub fn receiver_status(&self) -> Option<RecvStat> {
        self.receiver_status
    }

    /// Get the carrier phase measurements
    pub fn carrier_phase(&self) -> &HashMap<GnssSatellite, SatPathInfo> {
        &self.meas
    }

    /// Remove the carrier phase measurements
    pub fn remove_carrier_phase(&mut self) -> HashMap<GnssSatellite, SatPathInfo> {
        self.meas.drain().collect()
    }

    /// Calculate the total electron content (TEC) from the carrier phase measurements
    pub fn calculate_tec(&self) {
        todo!()
    }
}

mod test {
    #[test]
    fn test_rxm_checksum() {
        use super::UbxFormat;
        let payload = [
            0xB5, 0x62, 0x02, 0x15, 0x70, 0x03, 0x17, 0xD9, 0xCE, 0xF7, 0x3F, 0xB4, 0x14, 0x41,
            0x1E, 0x09, 0x12, 0x1B, 0x01, 0x01, 0xE1, 0xD8, 0x60, 0xF4, 0x00, 0xDD, 0x8F, 0x1E,
            0x71, 0x41, 0xB8, 0xA3, 0x17, 0x26, 0xA2, 0x7D, 0x96, 0x41, 0x02, 0x47, 0xB5, 0x44,
            0x00, 0x10, 0x00, 0x00, 0xF4, 0xFB, 0x2A, 0x04, 0x02, 0x07, 0x0F, 0x00, 0x31, 0x15,
            0x50, 0x98, 0x60, 0x99, 0x81, 0x41, 0x4E, 0x66, 0xA1, 0xC6, 0xFA, 0x1E, 0xA7, 0x41,
            0xCF, 0x7C, 0x32, 0xC3, 0x01, 0x83, 0x00, 0x00, 0xF4, 0xFB, 0x29, 0x04, 0x02, 0x07,
            0x07, 0x00, 0x2D, 0x48, 0x60, 0x42, 0x01, 0xB6, 0x77, 0x41, 0x38, 0x33, 0xC9, 0x5B,
            0x04, 0xDE, 0x9E, 0x41, 0xDE, 0x33, 0x64, 0xC5, 0x03, 0x29, 0x00, 0x00, 0xF4, 0xFB,
            0x1E, 0x06, 0x06, 0x08, 0x07, 0x00, 0xE4, 0x9B, 0x46, 0x4C, 0x2F, 0x16, 0x73, 0x41,
            0x27, 0xD2, 0x2A, 0x5A, 0xE9, 0xD8, 0x98, 0x41, 0x64, 0xC0, 0x50, 0x44, 0x03, 0x25,
            0x00, 0x00, 0x00, 0x00, 0x12, 0x09, 0x0F, 0x0C, 0x01, 0x00, 0x39, 0xA1, 0x82, 0xCA,
            0x7B, 0x05, 0x77, 0x41, 0xC0, 0xC1, 0x86, 0x9D, 0x3A, 0xF8, 0x9D, 0x41, 0x29, 0x31,
            0x2C, 0x45, 0x03, 0x1C, 0x00, 0x00, 0xF4, 0xFB, 0x1E, 0x06, 0x07, 0x09, 0x07, 0x00,
            0xE8, 0xAD, 0x37, 0x53, 0xBF, 0x29, 0x74, 0x41, 0x12, 0x56, 0x6D, 0xEC, 0xA4, 0x3F,
            0x9A, 0x41, 0x73, 0x2B, 0x2E, 0xC5, 0x03, 0x20, 0x00, 0x00, 0xF4, 0xFB, 0x28, 0x04,
            0x02, 0x07, 0x0F, 0x00, 0xB2, 0x29, 0x59, 0x17, 0x31, 0x01, 0x73, 0x41, 0x61, 0x94,
            0x1B, 0x62, 0x64, 0x5A, 0x99, 0x41, 0x7D, 0x2F, 0x8D, 0xC5, 0x06, 0x02, 0x00, 0x03,
            0xF4, 0xFB, 0x23, 0x06, 0x04, 0x08, 0x0F, 0x00, 0x6F, 0x50, 0x69, 0x78, 0x5E, 0x5D,
            0x70, 0x41, 0x04, 0xDE, 0x49, 0xFC, 0x80, 0xE6, 0x95, 0x41, 0xDF, 0xC5, 0xBB, 0xC4,
            0x06, 0x03, 0x00, 0x0C, 0xF4, 0xFB, 0x27, 0x06, 0x02, 0x07, 0x07, 0x00, 0x8D, 0x99,
            0xB5, 0xDE, 0x15, 0x97, 0x74, 0x41, 0xA1, 0xE5, 0x06, 0x2A, 0xF1, 0x0C, 0x9B, 0x41,
            0xD7, 0x20, 0x78, 0x44, 0x02, 0x18, 0x00, 0x00, 0xF4, 0xFB, 0x27, 0x04, 0x02, 0x07,
            0x07, 0x00, 0x01, 0xAA, 0x0A, 0xBA, 0xD4, 0xEC, 0x75, 0x41, 0xF7, 0xA6, 0x15, 0x70,
            0xE7, 0xCD, 0x9C, 0x41, 0xFE, 0x5A, 0x26, 0xC5, 0x02, 0x06, 0x00, 0x00, 0xF4, 0xFB,
            0x1F, 0x06, 0x06, 0x08, 0x07, 0x00, 0xF7, 0x37, 0x37, 0xF7, 0x05, 0xD6, 0x71, 0x41,
            0x1E, 0xEE, 0x35, 0x9D, 0xE0, 0xC4, 0x97, 0x41, 0xFF, 0x0D, 0x02, 0x45, 0x06, 0x0E,
            0x00, 0x00, 0xF4, 0xFB, 0x26, 0x06, 0x03, 0x07, 0x07, 0x00, 0xF3, 0xDB, 0x58, 0xE2,
            0x8C, 0xF7, 0x70, 0x41, 0xF6, 0xB2, 0xA2, 0xB4, 0x60, 0x4A, 0x96, 0x41, 0x1A, 0x41,
            0x98, 0xC4, 0x00, 0x1A, 0x00, 0x00, 0x00, 0x00, 0x19, 0x09, 0x0F, 0x0A, 0x01, 0x00,
            0xE8, 0x35, 0x29, 0xF8, 0x67, 0xD8, 0x77, 0x41, 0x17, 0x56, 0xED, 0x56, 0xB8, 0x53,
            0x9F, 0x41, 0xE0, 0xE7, 0x1F, 0x45, 0x02, 0x19, 0x00, 0x00, 0xF4, 0xFB, 0x21, 0x06,
            0x05, 0x08, 0x07, 0x00, 0x26, 0x60, 0x54, 0x6E, 0xE2, 0x9A, 0x74, 0x41, 0x29, 0x8F,
            0x06, 0x9B, 0xEE, 0x11, 0x9B, 0x41, 0x9B, 0x56, 0x6A, 0x45, 0x00, 0x1B, 0x00, 0x00,
            0xF4, 0xFB, 0x27, 0x04, 0x02, 0x07, 0x07, 0x00, 0x7B, 0x46, 0xEC, 0x9E, 0x14, 0xB5,
            0x74, 0x41, 0x00, 0x80, 0xE6, 0x31, 0x57, 0x34, 0x9B, 0x41, 0xDC, 0x67, 0xDB, 0xC4,
            0x02, 0x09, 0x00, 0x00, 0xF4, 0xFB, 0x25, 0x04, 0x02, 0x07, 0x07, 0x00, 0xE0, 0x44,
            0x3D, 0x1B, 0xAD, 0x0D, 0x76, 0x41, 0x98, 0x37, 0x5B, 0x13, 0x0F, 0xF9, 0x9C, 0x41,
            0x58, 0xC5, 0x74, 0xC5, 0x00, 0x20, 0x00, 0x00, 0x54, 0xFB, 0x19, 0x08, 0x0A, 0x08,
            0x07, 0x00, 0x6D, 0xAB, 0xF6, 0x90, 0xF0, 0x95, 0x73, 0x41, 0x7D, 0x98, 0x0B, 0x65,
            0x1B, 0xBB, 0x99, 0x41, 0x8D, 0x99, 0x1E, 0xC5, 0x00, 0x03, 0x00, 0x00, 0xF4, 0xFB,
            0x24, 0x05, 0x03, 0x08, 0x07, 0x00, 0x42, 0x70, 0xDF, 0xDC, 0xD4, 0xEC, 0x75, 0x41,
            0x60, 0xAC, 0x94, 0x59, 0x22, 0x12, 0x96, 0x41, 0xCE, 0xFE, 0xFE, 0xC4, 0x02, 0x06,
            0x06, 0x00, 0xF4, 0xFB, 0x23, 0x05, 0x03, 0x08, 0x07, 0x00, 0x86, 0x12, 0x04, 0xF4,
            0x15, 0x97, 0x74, 0x41, 0x76, 0x7E, 0x4F, 0x59, 0x20, 0xBA, 0x94, 0x41, 0xB0, 0x19,
            0x3E, 0x44, 0x02, 0x18, 0x06, 0x00, 0xF4, 0xFB, 0x26, 0x04, 0x03, 0x07, 0x07, 0x00,
            0x40, 0x9E, 0x2B, 0xE3, 0x4F, 0x0E, 0x78, 0x41, 0xC8, 0xB1, 0xAB, 0xE3, 0x3C, 0x37,
            0x98, 0x41, 0xAE, 0x3A, 0x15, 0xC5, 0x02, 0x04, 0x06, 0x00, 0xF4, 0xFB, 0x22, 0x05,
            0x04, 0x08, 0x07, 0x00, 0x7F, 0x4F, 0xDB, 0xB5, 0x14, 0xB5, 0x74, 0x41, 0xE8, 0xD1,
            0xF4, 0x58, 0x50, 0xD8, 0x94, 0x41, 0xCC, 0x27, 0xA8, 0xC4, 0x02, 0x09, 0x06, 0x00,
            0xF4, 0xFB, 0x22, 0x05, 0x03, 0x08, 0x07, 0x00, 0x53, 0xB2, 0x2B, 0xF0, 0x68, 0xD8,
            0x77, 0x41, 0x7B, 0xF7, 0x23, 0x07, 0xFD, 0x00, 0x98, 0x41, 0x36, 0x13, 0xF5, 0x44,
            0x02, 0x19, 0x06, 0x00, 0xF4, 0xFB, 0x22, 0x05, 0x03, 0x07, 0x07, 0x00, 0xFB, 0x1C,
            0x99, 0x6E, 0xF0, 0x95, 0x73, 0x41, 0xE6, 0xEE, 0x30, 0x86, 0xCF, 0x0C, 0x94, 0x41,
            0x48, 0x20, 0xF7, 0xC4, 0x00, 0x03, 0x03, 0x00, 0x00, 0x00, 0x1A, 0x08, 0x0F, 0x09,
            0x01, 0x00, 0x66, 0x53, 0x5C, 0x67, 0x31, 0x01, 0x73, 0x41, 0xDF, 0x4C, 0xFD, 0x6C,
            0x16, 0xB8, 0x93, 0x41, 0x05, 0xA0, 0x5B, 0xC5, 0x06, 0x02, 0x02, 0x03, 0xF4, 0xFB,
            0x25, 0x06, 0x03, 0x07, 0x0F, 0x00, 0x18, 0xF8, 0xB1, 0xDA, 0xE2, 0x9A, 0x74, 0x41,
            0xB8, 0x2B, 0xFA, 0xB6, 0xF3, 0x17, 0x95, 0x41, 0x32, 0x94, 0x36, 0x45, 0x00, 0x1B,
            0x03, 0x00, 0xF4, 0xFB, 0x22, 0x07, 0x04, 0x08, 0x07, 0x00, 0x23, 0xBC, 0xD2, 0x64,
            0x5E, 0x5D, 0x70, 0x41, 0xCA, 0x24, 0x51, 0x0E, 0x9D, 0x08, 0x91, 0x41, 0xB3, 0x06,
            0x92, 0xC4, 0x06, 0x03, 0x02, 0x0C, 0xF4, 0xFB, 0x1F, 0x07, 0x06, 0x09, 0x07, 0x00,
            0xD8, 0xAE, 0x6E, 0x85, 0x06, 0xD6, 0x71, 0x41, 0x61, 0x06, 0x48, 0x66, 0xAD, 0x7C,
            0x92, 0x41, 0xC4, 0x56, 0xCA, 0x44, 0x06, 0x0E, 0x02, 0x00, 0xD0, 0x98, 0x1B, 0x08,
            0x08, 0x08, 0x07, 0x00, 0x21, 0xF2,
        ];
        let payload2 = [
            0xB5, 0x62, 0x02, 0x15, 0xB0, 0x03, 0x17, 0xD9, 0xCE, 0xF7, 0x67, 0xA1, 0x14, 0x41,
            0x1E, 0x09, 0x12, 0x1D, 0x01, 0x01, 0xF1, 0x71, 0x29, 0x58, 0x20, 0x10, 0x2E, 0x83,
            0x71, 0x41, 0x3D, 0xFB, 0xB6, 0x02, 0xD2, 0x01, 0x97, 0x41, 0x5C, 0x14, 0x05, 0x45,
            0x00, 0x10, 0x00, 0x00, 0xF4, 0xFB, 0x2F, 0x03, 0x01, 0x06, 0x0F, 0x00, 0xB4, 0x3E,
            0x51, 0x35, 0x4C, 0x94, 0x81, 0x41, 0xC1, 0xA7, 0x4A, 0x5A, 0x4E, 0x18, 0xA7, 0x41,
            0xAF, 0x62, 0x3A, 0xC3, 0x01, 0x83, 0x00, 0x00, 0xF4, 0xFB, 0x29, 0x04, 0x02, 0x06,
            0x07, 0x00, 0x16, 0x84, 0x7D, 0xCB, 0xA1, 0xE5, 0x76, 0x41, 0xB5, 0xE7, 0x68, 0xAA,
            0xC1, 0xCE, 0x9D, 0x41, 0xBE, 0x76, 0x67, 0xC5, 0x03, 0x29, 0x00, 0x00, 0xF4, 0xFB,
            0x23, 0x05, 0x04, 0x08, 0x07, 0x00, 0x70, 0xB4, 0x66, 0x06, 0x94, 0x4E, 0x73, 0x41,
            0x96, 0x98, 0x39, 0x69, 0x54, 0x22, 0x99, 0x41, 0x82, 0x79, 0x90, 0x44, 0x03, 0x25,
            0x00, 0x00, 0xF4, 0xFB, 0x23, 0x05, 0x04, 0x08, 0x0F, 0x00, 0x13, 0x1B, 0xA8, 0xFA,
            0xF9, 0x2A, 0x74, 0x41, 0x6E, 0x8E, 0x54, 0x4B, 0x3E, 0x41, 0x9A, 0x41, 0x82, 0x0C,
            0x83, 0x44, 0x03, 0x14, 0x00, 0x00, 0x00, 0x00, 0x15, 0x09, 0x0F, 0x0C, 0x01, 0x00,
            0x1F, 0x7D, 0xAE, 0x57, 0x93, 0x08, 0x73, 0x41, 0x37, 0x49, 0x5E, 0xE1, 0x31, 0xC7,
            0x98, 0x41, 0xAC, 0xCC, 0xB7, 0xC4, 0x03, 0x17, 0x00, 0x00, 0x00, 0x00, 0x14, 0x09,
            0x0F, 0x0C, 0x01, 0x00, 0xD2, 0x3E, 0xF4, 0x0A, 0x72, 0x99, 0x73, 0x41, 0x37, 0x38,
            0x28, 0x63, 0xCA, 0x83, 0x99, 0x41, 0x8C, 0xE5, 0x0F, 0xC5, 0x03, 0x20, 0x00, 0x00,
            0xF4, 0xFB, 0x2A, 0x03, 0x02, 0x06, 0x0F, 0x00, 0x51, 0x49, 0xA2, 0x95, 0x2F, 0x13,
            0x72, 0x41, 0xD5, 0xE4, 0x5C, 0x13, 0xE2, 0x1C, 0x98, 0x41, 0x5D, 0xE4, 0x7E, 0xC5,
            0x06, 0x02, 0x00, 0x03, 0xF4, 0xFB, 0x27, 0x05, 0x02, 0x07, 0x0F, 0x00, 0x76, 0x93,
            0xB7, 0xC0, 0x60, 0x28, 0x70, 0x41, 0xB1, 0xC4, 0xC5, 0x32, 0x96, 0x9F, 0x95, 0x41,
            0x10, 0xAE, 0xD3, 0xC3, 0x06, 0x03, 0x00, 0x0C, 0xF4, 0xFB, 0x22, 0x06, 0x04, 0x08,
            0x07, 0x00, 0xA8, 0xB4, 0xB2, 0xDE, 0x9A, 0x63, 0x77, 0x41, 0x9A, 0x37, 0x7A, 0xD9,
            0x44, 0xBA, 0x9E, 0x41, 0x22, 0xA9, 0x39, 0xC5, 0x02, 0x04, 0x00, 0x00, 0xA4, 0xA1,
            0x1A, 0x08, 0x0A, 0x08, 0x07, 0x00, 0x01, 0x3C, 0x2B, 0xB7, 0x27, 0xDA, 0x74, 0x41,
            0x2B, 0x3F, 0x78, 0xED, 0x0D, 0x65, 0x9B, 0x41, 0x48, 0xC7, 0xAE, 0x44, 0x02, 0x18,
            0x00, 0x00, 0xF4, 0xFB, 0x2B, 0x04, 0x01, 0x05, 0x07, 0x00, 0x1B, 0x15, 0x3F, 0x96,
            0x26, 0x60, 0x75, 0x41, 0x67, 0xC6, 0x44, 0xBD, 0x15, 0x15, 0x9C, 0x41, 0x66, 0x32,
            0x13, 0xC5, 0x02, 0x06, 0x00, 0x00, 0xF4, 0xFB, 0x23, 0x05, 0x03, 0x07, 0x07, 0x00,
            0x87, 0x24, 0x35, 0x7C, 0x07, 0x55, 0x72, 0x41, 0xEC, 0x80, 0x8E, 0xE9, 0x20, 0x6E,
            0x98, 0x41, 0xE3, 0x08, 0x1D, 0x45, 0x06, 0x0E, 0x00, 0x00, 0xF4, 0xFB, 0x25, 0x06,
            0x03, 0x07, 0x07, 0x00, 0x81, 0x1B, 0x1A, 0xF6, 0xAA, 0x6B, 0x78, 0x41, 0x94, 0xBB,
            0x1D, 0x65, 0x97, 0x0A, 0xA0, 0x41, 0xB9, 0x00, 0x28, 0x45, 0x02, 0x19, 0x00, 0x00,
            0xF4, 0xFB, 0x22, 0x06, 0x04, 0x08, 0x07, 0x00, 0x4E, 0x9E, 0xE2, 0xA8, 0x2C, 0x6D,
            0x75, 0x41, 0x99, 0xBF, 0x0C, 0x93, 0x32, 0x26, 0x9C, 0x41, 0x04, 0x01, 0x69, 0x45,
            0x00, 0x1B, 0x00, 0x00, 0xF4, 0xFB, 0x23, 0x05, 0x04, 0x08, 0x07, 0x00, 0xB3, 0x86,
            0x5B, 0x17, 0xE7, 0x5E, 0x74, 0x41, 0xA5, 0xE8, 0x0D, 0xA5, 0x1F, 0xC3, 0x9A, 0x41,
            0xD2, 0x1B, 0xA5, 0xC4, 0x02, 0x09, 0x00, 0x00, 0xF4, 0xFB, 0x21, 0x06, 0x05, 0x08,
            0x07, 0x00, 0x66, 0x2F, 0xC6, 0xDD, 0x15, 0x35, 0x75, 0x41, 0xDA, 0xC2, 0xED, 0x53,
            0x82, 0xDC, 0x9B, 0x41, 0xF0, 0x61, 0x6D, 0xC5, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00,
            0x19, 0x09, 0x0F, 0x0A, 0x01, 0x00, 0xB0, 0xDB, 0xFF, 0x79, 0xAA, 0x17, 0x73, 0x41,
            0x9A, 0x7F, 0x7F, 0xBD, 0x36, 0x15, 0x99, 0x41, 0x74, 0x68, 0xF3, 0xC4, 0x00, 0x03,
            0x00, 0x00, 0xF4, 0xFB, 0x26, 0x05, 0x02, 0x07, 0x07, 0x00, 0x9E, 0xA0, 0x6C, 0xA4,
            0x26, 0x60, 0x75, 0x41, 0xE0, 0x79, 0xC0, 0x0C, 0x85, 0x84, 0x95, 0x41, 0x1C, 0x8E,
            0xE1, 0xC4, 0x02, 0x06, 0x06, 0x00, 0xF4, 0xFB, 0x1E, 0x06, 0x06, 0x08, 0x07, 0x00,
            0xB7, 0xDD, 0xF1, 0xBA, 0x27, 0xDA, 0x74, 0x41, 0x09, 0xB0, 0xAA, 0x0E, 0xA4, 0xFD,
            0x94, 0x41, 0xD4, 0xED, 0x85, 0x44, 0x02, 0x18, 0x06, 0x00, 0xF4, 0xFB, 0x26, 0x04,
            0x03, 0x07, 0x07, 0x00, 0x10, 0xDB, 0x04, 0xCE, 0x9B, 0x63, 0x77, 0x41, 0x2E, 0x35,
            0xA4, 0xF1, 0x66, 0x8B, 0x97, 0x41, 0x94, 0x3E, 0x0E, 0xC5, 0x02, 0x04, 0x06, 0x00,
            0xF4, 0xFB, 0x20, 0x05, 0x06, 0x08, 0x07, 0x00, 0x0A, 0xBF, 0x89, 0x2E, 0xE7, 0x5E,
            0x74, 0x41, 0x85, 0x3C, 0x18, 0x31, 0x90, 0x81, 0x94, 0x41, 0x08, 0x1F, 0x7D, 0xC4,
            0x02, 0x09, 0x06, 0x00, 0xF4, 0xFB, 0x1F, 0x06, 0x07, 0x08, 0x07, 0x00, 0x4D, 0x49,
            0x22, 0x71, 0xA4, 0xE5, 0x76, 0x41, 0x7E, 0xFC, 0x76, 0xE4, 0x99, 0x0C, 0x97, 0x41,
            0xA0, 0x49, 0x3B, 0xC5, 0x03, 0x29, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x0A, 0x0F, 0x0C,
            0x01, 0x00, 0xDB, 0x96, 0x68, 0xEC, 0xAB, 0x6B, 0x78, 0x41, 0x7E, 0x92, 0x01, 0xAA,
            0x39, 0x95, 0x98, 0x41, 0x1C, 0xBB, 0x00, 0x45, 0x02, 0x19, 0x06, 0x00, 0xF4, 0xFB,
            0x21, 0x05, 0x04, 0x08, 0x07, 0x00, 0xCB, 0xF3, 0x2A, 0x33, 0xAA, 0x17, 0x73, 0x41,
            0x54, 0xC2, 0x78, 0x51, 0x8B, 0x8B, 0x93, 0x41, 0xD6, 0xA2, 0xBD, 0xC4, 0x00, 0x03,
            0x03, 0x00, 0xF4, 0xFB, 0x1A, 0x08, 0x09, 0x08, 0x07, 0x00, 0x42, 0x3F, 0xAD, 0x2E,
            0x17, 0x35, 0x75, 0x41, 0x34, 0xF4, 0x01, 0xF5, 0xCD, 0xB5, 0x95, 0x41, 0xBF, 0x28,
            0x39, 0xC5, 0x00, 0x20, 0x03, 0x00, 0x00, 0x00, 0x16, 0x09, 0x0F, 0x0C, 0x01, 0x00,
            0xCE, 0x16, 0x70, 0xBF, 0x2F, 0x13, 0x72, 0x41, 0x6B, 0xBF, 0x13, 0xCF, 0x21, 0xC1,
            0x92, 0x41, 0xF3, 0x3B, 0x46, 0xC5, 0x06, 0x02, 0x02, 0x03, 0xF4, 0xFB, 0x24, 0x06,
            0x03, 0x07, 0x0F, 0x00, 0xD1, 0xAD, 0xDA, 0x32, 0x2D, 0x6D, 0x75, 0x41, 0xE3, 0x11,
            0x50, 0x0E, 0x39, 0xEF, 0x95, 0x41, 0xD3, 0x91, 0x35, 0x45, 0x00, 0x1B, 0x03, 0x00,
            0xF4, 0xFB, 0x21, 0x06, 0x04, 0x08, 0x07, 0x00, 0x0E, 0x4F, 0x90, 0xF9, 0x07, 0x55,
            0x72, 0x41, 0x00, 0x6F, 0x46, 0x58, 0x51, 0x00, 0x93, 0x41, 0x2A, 0x32, 0xF4, 0x44,
            0x06, 0x0E, 0x02, 0x00, 0x5C, 0x9E, 0x20, 0x07, 0x05, 0x08, 0x07, 0x00, 0xC7, 0x82,
        ];

        let (msg, _) = super::split_ubx(payload.to_vec());
        {
            for m in msg {
                if let Ok(rxm) = super::UbxRxmRawx::from_message(m) {
                    println!("Message: {:?}", rxm);
                }
            }
        }
        let (msg, _) = super::split_ubx(payload2.to_vec());
        {
            for m in msg {
                if let Ok(rxm) = super::UbxRxmRawx::from_message(m) {
                    println!("Message: {:?}", rxm);
                }
            }
        }

        match super::find_rxm_raw(&payload) {
            Ok(msg) => {
                println!("Message: {:?}", msg);
            }
            Err(err) => {
                eprintln!("Error: {}", err);
            }
        }
        match super::find_rxm_raw(&payload2) {
            Ok(msg) => {
                println!("Message: {:?}", msg);
            }
            Err(err) => {
                eprintln!("Error: {}", err);
            }
        }
    }
}
