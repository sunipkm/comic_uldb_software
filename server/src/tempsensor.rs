#![allow(dead_code)]
use embedded_hal::i2c::{I2c, SevenBitAddress};
use mcp9808::{
    address::SlaveAddress,
    error::Error,
    reg_res::{Resolution, ResolutionVal},
    reg_temp_generic::ReadableTempRegister,
    MCP9808,
};
use packet::TempReadout;
use std::time::Instant;

use crate::REFCLK;

/// A struct representing a temperature sensor with its location, address, and resolution.
pub struct Mcp9808Sensor<I2C> {
    loc: String,
    sensor: MCP9808<I2C>,
}

impl<I2C> std::fmt::Debug for Mcp9808Sensor<I2C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Mcp9808Sensor {{ loc: {}, addr: {:?}, res: {:?} }}",
            self.loc,
            self.sensor.get_address(),
            self.sensor.resolution()
        )
    }
}

impl<I2C> Mcp9808Sensor<I2C>
where
    I2C: I2c<SevenBitAddress>,
    I2C::Error: Into<Error<I2C::Error>>,
{
    pub fn new(
        loc: &str,
        addr: SlaveAddress,
        res: ResolutionVal,
        i2c: &mut I2C,
    ) -> Result<Self, Error<I2C::Error>> {
        Ok(Self {
            loc: loc.to_string(),
            sensor: MCP9808::new(addr, res, i2c)?,
        })
    }

    pub fn get_sensor(&mut self) -> &mut MCP9808<I2C> {
        &mut self.sensor
    }

    pub fn read_temperature(&mut self, i2c: &mut I2C) -> Result<TempReadout, String> {
        let now = Instant::now();
        let tmp = self
            .sensor
            .read_temperature(i2c)
            .map_err(|err| format!("{err:?}"))
            .map(|temp| temp.get_milli_celsius(self.sensor.resolution()))?;
        Ok(TempReadout {
            now: now - *REFCLK,
            readings: (self.loc.clone(), tmp),
        })
    }
}

#[derive(Debug)]
pub struct Mcp9808Sensors<I2C> {
    pub sensors: Vec<Mcp9808Sensor<I2C>>,
}

impl<I2C> Mcp9808Sensors<I2C>
where
    I2C: I2c<SevenBitAddress>,
    I2C::Error: Into<Error<I2C::Error>>,
{
    /// Creates a new instance of `Mcp9808Sensors`.
    pub fn new() -> Self {
        Self {
            sensors: Vec::new(),
        }
    }
    /// Adds a new sensor to the list of sensors.
    pub fn add_sensor(&mut self, loc: &str, addr: SlaveAddress, res: ResolutionVal, i2c: &mut I2C) {
        if let Ok(sensor) = Mcp9808Sensor::new(loc, addr, res, i2c) {
            self.sensors.push(sensor);
        }
    }

    pub fn set_resolution(&mut self, i2c: &mut I2C, res: ResolutionVal) -> Result<(), String> {
        for sensor in &mut self.sensors {
            let mut cur_res = sensor
                .sensor
                .read_resolution(i2c)
                .map_err(|err| format!("{err:?}"))?;
            cur_res.set_resolution(res);
        }
        Ok(())
    }

    pub fn read_all(&mut self, i2c: &mut I2C) -> Result<Vec<(String, f32)>, String> {
        let mut results = Vec::new();
        for sensor in &mut self.sensors {
            let temp = sensor
                .sensor
                .read_temperature(i2c)
                .map_err(|err| format!("{err:?}"))?
                .get_celsius(sensor.sensor.resolution());
            results.push((sensor.loc.clone(), temp));
        }
        Ok(results)
    }
}

// macro_rules! TempSensors {
//     [$i2c: expr, $res: expr, $(($name:expr, $addr:expr)),* $(,)?] => {
//         {
//             let mut sensors = crate::tempsensor::Mcp9808Sensors { sensors: Vec::new() };
//             $(
//                 sensors.add_sensor($name, $addr, $res, $i2c);
//             )*
//             sensors
//         }
//     };
// }
