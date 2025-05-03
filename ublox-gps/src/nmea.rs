use chrono::{DateTime, TimeZone, Utc};
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::NmeaMsgGroup;

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
/// A GNSS satellite
pub enum GnssSatellite {
    /// A GPS satellite (ID: 0 - 32)
    Gps(u8),
    /// A SBAS satellite (ID: 120 - 158)
    Sbas(u8),
    /// A Galileo satellite (ID: 1 - 36)
    Galileo(u8),
    /// A Beidou satellite (ID: 1 - 37)
    Beidou(u8),
    /// A QZSS satellite (ID: 1-5)
    Qzss(u8),
    /// A Glonass satellite (ID: 1 - 32)
    Glonass(u8),
}

impl Serialize for GnssSatellite {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Gps(svid) => serializer.serialize_str(&format!("G{:02X}", svid)),
            Self::Sbas(svid) => serializer.serialize_str(&format!("S{:02X}", svid)),
            Self::Galileo(svid) => serializer.serialize_str(&format!("E{:02X}", svid)),
            Self::Beidou(svid) => serializer.serialize_str(&format!("C{:02X}", svid)),
            Self::Qzss(svid) => serializer.serialize_str(&format!("J{:02X}", svid)),
            Self::Glonass(svid) => serializer.serialize_str(&format!("R{:02X}", svid)),
        }
    }
}

impl<'de> Deserialize<'de> for GnssSatellite {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.len() != 3 {
            return Err(serde::de::Error::custom("Invalid length"));
        }
        let svid = u8::from_str_radix(&s[1..], 16).unwrap_or_default();
        Ok(GnssSatellite::from_internal(s.as_bytes()[0], svid))
    }
}

impl GnssSatellite {
    pub(crate) fn from_nmea_svid(cls: &[u8], svid: u8) -> Self {
        match cls {
            b"GP" => Self::Gps(svid),
            b"GB" => Self::Beidou(svid),
            b"GA" => Self::Galileo(svid),
            b"GL" => Self::Glonass(svid - 64),
            b"GN" => Self::Sbas(svid),
            b"GQ" => Self::Qzss(svid),
            _ => unreachable!(),
        }
    }

    pub(crate) fn from_internal(cls: u8, svid: u8) -> Self {
        match cls {
            b'G' => Self::Gps(svid),
            b'C' => Self::Beidou(svid),
            b'E' => Self::Galileo(svid),
            b'R' => Self::Glonass(svid - 64),
            b'S' => Self::Sbas(svid),
            b'J' => Self::Qzss(svid),
            _ => unreachable!(),
        }
    }

    pub(crate) fn from_ubx(cls: u8, svid: u8) -> Self {
        match cls {
            0 => Self::Gps(svid),
            1 => Self::Sbas(svid),
            2 => Self::Galileo(svid),
            3 => Self::Beidou(svid),
            5 => Self::Qzss(svid),
            6 => Self::Glonass(svid),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A struct containing raw NMEA data
pub struct RawNmea {
    /// The 'Talker ID'
    id: [u8; 2],
    data: String,
}

impl RawNmea {
    /// Parse a string of NMEA data into a hashmap of [`RawNmea`] data
    pub fn parse_str(data: &str) -> NmeaMsgGroup {
        lazy_static! {
            static ref RE: Regex = Regex::new(
                r"\$(?P<payload>(?P<id>[A-Z]{2})(?P<kind>[A-Z]{3})\,(?P<data>.*?))\*(?P<cksum>[A-F0-9]{2})"
            ).expect("Failed to compile regex");
        }
        let mut res = HashMap::new();
        for caps in RE.captures_iter(data) {
            let calc_cksum = caps["payload"].as_bytes().iter().fold(0, |acc, &x| acc ^ x);
            if let Ok(cksum) = u8::from_str_radix(&caps["cksum"], 16) {
                if calc_cksum == cksum {
                    let id = caps["id"].as_bytes()[..2]
                        .try_into()
                        .expect("Failed to convert ID");
                    let kind = caps["kind"].as_bytes()[..3]
                        .try_into()
                        .expect("Failed to convert kind");
                    res.entry(kind)
                        .and_modify(|e: &mut Vec<RawNmea>| {
                            e.push(RawNmea {
                                id,
                                data: caps["data"].to_string(),
                            })
                        })
                        .or_insert_with(|| {
                            vec![RawNmea {
                                id,
                                data: caps["data"].to_string(),
                            }]
                        });
                }
            }
        }
        res
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
/// A struct containing GPS information
pub struct NmeaGpsInfo {
    /// Timestamp of the fix
    pub time: DateTime<Utc>,
    /// Location of the fix
    pub loc: (f64, f64, f32),
    /// Altitude above mean sea level
    pub msl: f32,
    /// True heading
    pub true_heading: f32,
    /// Magnetic heading
    pub mag_heading: f32,
    /// Ground speed
    pub ground_speed: f32,
    /// Quality of the fix
    pub quality: u8,
    /// Horizontal dilution of precision
    pub hdop: f32,
    /// Vertical dilution of precision
    pub vdop: f32,
    /// Position dilution of precision
    pub pdop: f32,
    /// Elevation and azimuth of satellites
    pub sat_views: HashMap<GnssSatellite, (i8, u16)>,
}

#[derive(Error, Clone, Debug)]
/// An error type for GPS parsing
pub enum GpsError {
    /// No fix has been acquired
    #[error("No ZDA data, has fix been acquired?")]
    NoFix,
    /// The pattern was not found
    #[error("Pattern not found")]
    PatternNotFound,
    /// Failed to parse data
    #[error("Failed to parse ZDA data: {0}")]
    ParseError(String),
}

impl NmeaGpsInfo {
    /// Create a new GPS info struct from a hashmap of [`RawNmea`] data.
    /// 
    /// # Arguments
    /// - `data`: A mutable reference to a hashmap of [`RawNmea`] data
    /// - `process_gsv`: A boolean indicating whether to process GSV data
    ///
    /// # Returns
    /// - A result containing the GPS info struct or a [`GpsError`]
    /// 
    /// # Errors
    /// - Returns a [`GpsError`] if the ZDA or GGA data is not found or if parsing fails
    /// 
    pub(crate) fn create(
        data: &mut NmeaMsgGroup,
        process_gsv: bool,
    ) -> Result<Self, GpsError> {
        let time = if let Some(mut zda) = data.remove(b"ZDA") {
            let time = loop {
                if let Some(val) = zda.pop() {
                    if let Ok(zda) = parse_zda(&val.data) {
                        break Some(zda);
                    }
                } else {
                    break None;
                }
            };
            if let Some(time) = time {
                time
            } else {
                return Err(GpsError::NoFix);
            }
        } else {
            return Err(GpsError::NoFix);
        };

        let (loc, quality, msl) = if let Some(mut zda) = data.remove(b"GGA") {
            let time = loop {
                if let Some(val) = zda.pop() {
                    if let Ok(gga) = parse_gga(&val.data) {
                        let loc = (
                            parse_lat(&gga["lat"], &gga["lat_dir"])?,
                            parse_lon(&gga["lon"], &gga["lon_dir"])?,
                            gga["alt"]
                                .parse::<f32>()
                                .map_err(|_| GpsError::ParseError("Altitude".into()))?,
                        );
                        let quality = u8::from_str_radix(&gga["quality"], 16)
                            .map_err(|_| GpsError::ParseError("Quality".into()))?;
                        let msl = gga["msl"].parse::<f32>().unwrap_or_default();
                        break Some((loc, quality, msl));
                    }
                } else {
                    break None;
                }
            };
            if let Some(time) = time {
                time
            } else {
                return Err(GpsError::NoFix);
            }
        } else {
            return Err(GpsError::NoFix);
        };

        let mut info = Self {
            time,
            loc,
            msl,
            quality,
            ..Default::default()
        };

        if let Some(mut vtg) = data.remove(b"VTG") {
            if let Some(vtg) = vtg.pop() {
                if let Ok(vtg) = parse_vtg(&vtg.data) {
                    info.true_heading = vtg["true_heading"].parse().unwrap_or_default();
                    info.ground_speed = vtg["ground_speed"].parse().unwrap_or_default();
                    info.mag_heading = vtg["mag_heading"].parse().unwrap_or_default();
                }
            }
        }

        if let Some(mut gsa) = data.remove(b"GSA") {
            if let Some(gsa) = gsa.pop() {
                if let Ok(gsa) = parse_gsa(&gsa.data) {
                    info.pdop = gsa["pdop"].parse().unwrap_or_default();
                    info.hdop = gsa["hdop"].parse().unwrap_or_default();
                    info.vdop = gsa["vdop"].parse().unwrap_or_default();
                }
            }
        }

        if process_gsv {
            if let Some(gsv) = data.remove(b"GSV") {
                gsv.iter()
                    .filter_map(|x| {
                        let id = x.id;
                        parse_gsv(&x.data).ok().map(|x| {
                            x.into_iter().map(move |(svid, elev, az)| {
                                (GnssSatellite::from_nmea_svid(&id, svid), (elev, az))
                            })
                        })
                    })
                    .flatten()
                    .for_each(|(svid, (elev, az))| {
                        info.sat_views
                            .entry(svid)
                            .and_modify(|e| {
                                e.0 = elev;
                                e.1 = az;
                            })
                            .or_insert((elev, az));
                    });
            }
        }
        Ok(info)
    }

    /// Insert GSV data into the GPS info struct.
    /// 
    /// # Note
    /// [NmeaGpsInfo::insert_gsv] is intended to be used when NMEA
    /// data is processed partially using the [crate::parse_partial]
    /// function. It allows for the insertion of GSV data after
    /// the initial parsing of time, location and heading data.
    /// It is up to the user to ensure that the GSV data from the
    /// correct packet is inserted.
    ///
    /// # Arguments
    /// - `msgs`: A [`NmeaMsgGroup`] containing GSV data.
    /// 
    pub fn insert_gsv(&mut self, msgs: &mut NmeaMsgGroup) {
        if let Some(gsv) = msgs.remove(b"GSV") {
            gsv.iter()
                .filter_map(|x| {
                    let id = x.id;
                    parse_gsv(&x.data).ok().map(|x| {
                        x.into_iter().map(move |(svid, elev, az)| {
                            (GnssSatellite::from_nmea_svid(&id, svid), (elev, az))
                        })
                    })
                })
                .flatten()
                .for_each(|(svid, (elev, az))| {
                    self.sat_views
                        .entry(svid)
                        .and_modify(|e| {
                            e.0 = elev;
                            e.1 = az;
                        })
                        .or_insert((elev, az));
                });
        }
    }
}

fn parse_lat(inp: &str, dir: &str) -> Result<f64, GpsError> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"(?<deg>\d{2})(?<min>\d{2}\.\d{5})").expect("Failed to compile regex");
    }
    if let Some(inp) = RE.captures(inp) {
        let deg = inp["deg"]
            .parse::<f64>()
            .map_err(|_| GpsError::ParseError("Latitude degrees".into()))?;
        let min = inp["min"]
            .parse::<f64>()
            .map_err(|_| GpsError::ParseError("Latitude minutes".into()))?;
        let lat = deg + min / 60.0;
        Ok(if dir == "S" { -lat } else { lat })
    } else {
        Err(GpsError::PatternNotFound)
    }
}

fn parse_lon(inp: &str, dir: &str) -> Result<f64, GpsError> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"(?<deg>\d{3})(?<min>\d{2}\.\d{5})").expect("Failed to compile regex");
    }
    if let Some(inp) = RE.captures(inp) {
        let deg = inp["deg"]
            .parse::<f64>()
            .map_err(|_| GpsError::ParseError("Longitude degrees".into()))?;
        let min = inp["min"]
            .parse::<f64>()
            .map_err(|_| GpsError::ParseError("Longitude minutes".into()))?;
        let lon = deg + min / 60.0;
        Ok(if dir == "W" { -lon } else { lon })
    } else {
        Err(GpsError::PatternNotFound)
    }
}

fn parse_zda(inp: &str) -> Result<DateTime<Utc>, GpsError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"(?<hour>\d{2})(?<minute>\d{2})(?<second>\d{2}\.\d{2}),(?<day>\d{2}),(?<month>\d{2}),(?<year>\d{4})"
        )
        .expect("Failed to compile regex");
    }
    if let Some(inp) = RE.captures(inp) {
        let inp = format!(
            "{}-{}-{}T{}:{}:{}0",
            &inp["year"], &inp["month"], &inp["day"], &inp["hour"], &inp["minute"], &inp["second"]
        );
        #[allow(deprecated)]
        TimeZone::datetime_from_str(&Utc, &inp, "%Y-%m-%dT%H:%M:%S%.f")
            .map_err(|e| GpsError::ParseError(e.to_string()))
    } else {
        Err(GpsError::PatternNotFound)
    }
}

fn parse_gga(inp: &str) -> Result<Captures, GpsError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"\d{6}\.\d{2},(?<lat>[\d\.]*),(?<lat_dir>[NS]),(?<lon>[\d\.]*),(?<lon_dir>[EW]),(?<quality>[0-9A-F]),(?<sat_views>\d*),[\d\.]*,(?<alt>[\-\d\.]*),M,(?<msl>[\-\d\.]*),M,(?<sep>[\-\d\.]*),"
        ).expect("Failed to compile regex");
    }
    RE.captures(inp).ok_or(GpsError::PatternNotFound)
}

fn parse_vtg(inp: &str) -> Result<Captures, GpsError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"(?<true_heading>[\-\d\.]*),T,(?<mag_heading>[\-\d\.]*),M,[\d\.]*,N,(?<ground_speed>[\d\.]*),K,"
        ).expect("Failed to compile regex");
    }
    RE.captures(inp).ok_or(GpsError::PatternNotFound)
}

fn parse_gsa(inp: &str) -> Result<Captures, GpsError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"[AM],\d,\d*,\d*,\d*,\d*,\d*,\d*,\d*,\d*,\d*,\d*,\d*,\d*,(?<pdop>[\d\.]*),(?<hdop>[\d\.]*),(?<vdop>[\d\.]*),"
        ).expect("Failed to compile regex");
    }
    RE.captures(inp).ok_or(GpsError::PatternNotFound)
}

fn parse_gsv(inp: &str) -> Result<Vec<(u8, i8, u16)>, GpsError> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"\d*,\d*,\d*,(?<payload>[\d\,]*)[0-9A-F]")
            .expect("Failed to compile regex");
        static ref XTRACT: Regex =
            Regex::new(r"(?<svid>\d*),(?<elevation>\d*),(?<azimuth>\d*),(?<snr>\d*),")
                .expect("Failed to compile regex");
    }
    let cap = RE.captures(inp).ok_or(GpsError::PatternNotFound)?;
    Ok(XTRACT
        .captures_iter(&cap["payload"])
        .map(|x| {
            (
                x["svid"].parse::<u8>().unwrap_or_default(),
                x["elevation"].parse::<i8>().unwrap_or_default(),
                x["azimuth"].parse::<u16>().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>())
}

mod test {

    #[test]
    fn parse_test() {
        let payload =
            "22:15:15  $GNRMC,221515.00,A,4238.96342,N,07118.97943,W,0.046,,031024,,,D,V*0D
22:15:15  done $GNVTG,,T,,M,0.046,N,0.086,K,D*34
22:15:15  done $GNGGA,221515.00,4238.96342,N,07118.97943,W,2,12,1.04,36.7,M,-33.0,M,,0131*41
22:15:15  done $GNGSA,A,3,03,27,46,44,31,26,04,16,,,,,1.83,1.04,1.51,1*0A
22:15:15  done $GNGSA,A,3,68,78,79,67,,,,,,,,,1.83,1.04,1.51,2*06
22:15:15  done $GNGSA,A,3,21,29,19,,,,,,,,,,1.83,1.04,1.51,3*09
22:15:15  done $GNGSA,A,3,25,23,41,32,,,,,,,,,1.83,1.04,1.51,4*0C
22:15:15  done $GNGSA,A,3,,,,,,,,,,,,,1.83,1.04,1.51,5*0F
22:15:15  $GPGSV,3,1,10,03,26,248,42,04,48,306,17,16,68,221,41,26,72,052,18,1*61
22:15:15  $GPGSV,3,2,10,27,18,171,36,29,16,041,11,31,62,067,22,32,00,145,12,1*66
22:15:15  $GPGSV,3,3,10,44,23,237,44,46,15,247,33,1*65
22:15:15  $GPGSV,1,1,03,03,26,248,27,04,48,306,16,27,18,171,36,6*58
22:15:15  $GPGSV,1,1,02,09,16,316,,28,30,090,,0*6D
22:15:15  $GLGSV,2,1,05,67,20,174,38,68,63,216,41,78,65,004,21,79,41,266,37,1*79
22:15:15  $GLGSV,2,2,05,86,05,011,20,1*44
22:15:15  $GLGSV,2,1,05,67,20,174,34,68,63,216,33,78,65,004,13,79,41,266,22,3*77
22:15:15  $GLGSV,2,2,05,86,05,011,24,3*42
22:15:15  $GLGSV,1,1,04,69,45,316,,77,15,054,,87,17,056,,88,09,111,,0*70
22:15:15  $GAGSV,1,1,02,19,74,181,28,29,30,147,35,2*71
22:15:15  $GAGSV,1,1,03,19,74,181,35,21,78,057,09,29,30,147,20,7*4A
22:15:15  $GAGSV,1,1,03,04,39,310,,06,14,315,,27,24,050,,0*49
22:15:15  $GBGSV,2,1,07,23,56,275,33,25,62,050,18,32,43,291,27,33,07,172,35,1*73
22:15:15  $GBGSV,2,2,07,37,07,259,29,41,43,210,44,43,09,146,27,1*4E
22:15:15  $GBGSV,1,1,01,41,43,210,08,B*3D
22:15:15  $GBGSV,1,1,04,20,03,330,,24,11,071,,34,22,102,,44,13,049,,0*79
22:15:15  $GQGSV,1,1,00,0*64
22:15:15  $GNGLL,4238.96342,N,07118.97943,W,221515.00,A,D*68
22:15:15  $GNZDA,221515.00,03,10,2024,00,00*7E";
        let mut nmea = super::RawNmea::parse_str(payload);
        // println!("{:#?}", nmea);
        // nmea[b"GGA"]
        //     .iter()
        //     .for_each(|x| println!("{:?}", super::parse_gga(&x.data)));
        // nmea[b"ZDA"]
        //     .iter()
        //     .for_each(|x| println!("{:?}", super::parse_zda(&x.data)));
        // nmea[b"VTG"]
        //     .iter()
        //     .for_each(|x| println!("{:?}", super::parse_vtg(&x.data)));
        // nmea[b"GSA"]
        //     .iter()
        //     .for_each(|x| println!("{:?}", super::parse_gsa(&x.data)));
        // nmea[b"GSV"].iter().for_each(|x| {
        //     println!(
        //         "{}: {:?}",
        //         str::from_utf8(&x.id).unwrap_or(""),
        //         super::parse_gsv(&x.data)
        //     )
        // });
        println!("{:?}", super::NmeaGpsInfo::create(&mut nmea, false));
        println!("{:?}", nmea);
    }
}
