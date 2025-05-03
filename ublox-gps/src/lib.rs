#![deny(missing_docs)]
//! # UBX GPS Parser
//! A limited capability parser for UBX GPS messages.
//!
//! Parses NMEA GGA, GSA, GSV and VTG messages, along with UBX-RXM-RAWX messages.
//! Provides a simple interface to extract timestamp, location, carrier phase
//! and satellite information.
mod nmea;
mod read_until;
mod tec;
mod ubx;
mod uncertain;

use std::io::Read;

use log::warn;
pub use nmea::{GnssSatellite, GpsError, NmeaGpsInfo};
use serde::{Deserialize, Serialize};
pub use ubx::{
    BeidouFreq, CarrierMeas, GalileoFreq, GlonassFreq, GnssFreq, GpsFreq, QzssFreq, SatPathInfo,
    UbxGpsInfo,
};

pub use tec::{TecData, TecInfo};
pub use uncertain::Uncertain;

use nmea::RawNmea;
use ubx::{split_ubx, UbxFormat, UbxMessage, UbxRxmRawx};

/// Default delimiter for separating UBX messages in a datafile
pub const DEFAULT_DELIM: [u8; 8] = *b"\r\r\n\n\r\r\n\n";

/// A collection of NMEA messages, grouped by message type
/// The key is the first three bytes of the message, e.g. "GGA", "GSA", etc.
/// The value is a vector of RawNmea messages.
/// The key is a 3-byte array, and the value is a vector of NMEA message strings.
pub type NmeaMsgGroup = std::collections::HashMap<[u8; 3], Vec<RawNmea>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A GPS Packet, comprised of NMEA message and RXM carrier data
pub struct GpsPacket {
    /// Processed NMEA data
    pub nmea: NmeaGpsInfo,
    /// Raw NMEA data
    pub nmea_raw: NmeaMsgGroup,
    /// Raw RXM carrier data
    pub rxm: Option<UbxRxmRawx>,
}

impl From<GpsPacket> for UbxGpsInfo {
    fn from(value: GpsPacket) -> Self {
        let mut value = value;
        if value.nmea.sat_views.is_empty() {
            value.nmea.insert_gsv(&mut value.nmea_raw);
        }
        UbxGpsInfo::new(value.nmea, value.rxm, value.nmea_raw)
    }
}

/// Parse a buffer to extract GPS positional information from NMEA messages only.
///
/// This function is used to parse NMEA messages from a buffer.
///
/// # Arguments
/// - `buf` - A vector of bytes containing the NMEA messages.
///
/// # Returns
/// - A tuple containing the parsed NMEA GPS information and a group of unprocessed NMEA messages.
///
pub fn parse_nmea(buf: Vec<u8>) -> Result<(NmeaGpsInfo, NmeaMsgGroup), GpsError> {
    let buf = std::str::from_utf8(&buf).map_err(|e| GpsError::ParseError(e.to_string()))?;
    let mut gpsmsg = RawNmea::parse_str(buf);
    let nmea = NmeaGpsInfo::create(&mut gpsmsg, true)?;
    Ok((nmea, gpsmsg))
}

/// Parse a buffer to extract GPS positional information and satellite carrier phase information.
pub fn parse_messages(buf: Vec<u8>) -> Result<UbxGpsInfo, GpsError> {
    // 1. Separate into UBX and NMEA messages
    let (ubx, buf) = split_ubx(buf);
    // 2. Parse UBX messages
    let mut rxm = Vec::new();
    for msg in ubx {
        match UbxRxmRawx::from_message(msg) {
            Ok(msg) => {
                rxm.push(msg);
            }
            Err(e) => warn!("Error parsing UBX message: {}", e),
        }
    }
    // 3. Parse NMEA messages
    let buf = std::str::from_utf8(&buf).map_err(|e| GpsError::ParseError(e.to_string()))?;
    let mut gpsmsg = RawNmea::parse_str(buf);
    let nmea = NmeaGpsInfo::create(&mut gpsmsg, true)?;
    let gpsinfo = UbxGpsInfo::new(nmea, rxm.pop(), gpsmsg);
    Ok(gpsinfo)
}

/// Parse a buffer into a GPS Packet
pub fn parse_binary(buf: Vec<u8>) -> Result<GpsPacket, GpsError> {
    // 1. Separate into UBX and NMEA messages
    let (ubx, buf) = split_ubx(buf);
    // 2. Parse UBX messages
    let mut rxm = Vec::with_capacity(1);
    for msg in ubx {
        match UbxRxmRawx::from_message(msg) {
            Ok(msg) => {
                rxm.push(msg);
            }
            Err(e) => warn!("Error parsing UBX message: {}", e),
        }
    }
    if rxm.len() > 1 {
        warn!("More than one RXM message in buffer.");
    }
    // 3. Parse NMEA messages
    let buf = std::str::from_utf8(&buf).map_err(|e| GpsError::ParseError(e.to_string()))?;
    let mut gpsmsg = RawNmea::parse_str(buf);
    let nmea = NmeaGpsInfo::create(&mut gpsmsg, true)?;
    Ok(GpsPacket {
        nmea,
        nmea_raw: gpsmsg,
        rxm: rxm.pop(),
    })
}

/// Parse a buffer to extract NMEA messages and the binary UBX payload.
pub fn parse_partial(
    buf: Vec<u8>,
    process_gsv: bool,
) -> Result<(NmeaGpsInfo, NmeaMsgGroup, Option<UbxMessage>), GpsError> {
    // 1. Separate into UBX and NMEA messages
    let (mut ubx, buf) = split_ubx(buf);
    // 2. Parse NMEA messages
    let buf = std::str::from_utf8(&buf).map_err(|e| GpsError::ParseError(e.to_string()))?;
    let mut gpsmsg = RawNmea::parse_str(buf);
    let nmea = NmeaGpsInfo::create(&mut gpsmsg, process_gsv)?;
    Ok((nmea, gpsmsg, ubx.pop()))
}

/// Parse a buffer partially processed using [parse_partial] into a [GpsPacket]. 
pub fn from_partial(
    nmea: NmeaGpsInfo,
    nmea_raw: NmeaMsgGroup,
    ubx: Option<UbxMessage>,
) -> Result<GpsPacket, GpsError> {
    let mut rxm = Vec::new();
    if let Some(ubx) = ubx {
        match UbxRxmRawx::from_message(ubx) {
            Ok(msg) => {
                rxm.push(msg);
            }
            Err(e) => warn!("Error parsing UBX message: {}", e),
        }
    }
    if rxm.len() > 1 {
        warn!("More than one RXM message in buffer.");
    }
    Ok(GpsPacket {
        nmea,
        nmea_raw,
        rxm: rxm.pop(),
    })
}

/// Parse a datafile containing multiple UBX messages separated by a pattern
pub fn parse_datafile<T: Read>(
    reader: &mut T,
    pattern: &[u8],
) -> Result<Vec<UbxGpsInfo>, GpsError> {
    let mut reader = read_until::get_reader(reader, pattern);
    let mut buffers = Vec::new();
    loop {
        let mut buf = Vec::with_capacity(2048);
        if let Ok(n) = reader.read_to_end(&mut buf) {
            if n == 0 {
                break;
            }
            if let Ok(res) = parse_messages(buf) {
                buffers.push(res);
            } else {
                warn!("Error parsing datafile: {}", n);
            }
        } else {
            break;
        }
    }
    Ok(buffers)
}

mod test {
    #[test]
    fn test_parse() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/test/datafile.bin");
        let mut datafile = std::fs::File::open(dir).unwrap();
        let buffers = super::parse_datafile(&mut datafile, b"\r\r\n\n\r\r\n\n")
            .expect("Failed to parse datafile");
        println!("Length: {}", buffers.len());
    }
}
