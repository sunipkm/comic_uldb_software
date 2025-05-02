use embedded_hal::i2c::I2c;
use mcp9808::{
    address::SlaveAddress,
    reg_res::{Resolution, ResolutionVal},
    reg_temp_generic::ReadableTempRegister,
    MCP9808,
};

/// A struct representing a temperature sensor with its location, address, and resolution.
#[derive(Debug)]
pub struct TempSensor {
    loc: String,
    addr: SlaveAddress,
    res: ResolutionVal,
}

impl TempSensor {
    pub fn new(loc: &str, addr: SlaveAddress, res: ResolutionVal) -> Self {
        Self {
            loc: loc.to_string(),
            addr,
            res,
        }
    }

    pub fn get_device<'a, T: I2c>(&self, i2c: &'a mut T) -> MCP9808<'a, T> {
        let mut dev = MCP9808::new(i2c);
        dev.set_address(self.addr);
        dev
    }
}

#[derive(Debug)]
pub struct TempSensors<T> {
    pub(crate) sensors: Vec<TempSensor>,
    pub(crate) i2c: T,
}

impl<T: embedded_hal::i2c::I2c> TempSensors<T> {
    pub(crate) fn add_sensor(&mut self, loc: &str, addr: SlaveAddress, res: ResolutionVal) {
        self.sensors.push(TempSensor::new(loc, addr, res));
    }

    pub fn set_resolution(&mut self, res: ResolutionVal) -> Result<(), String> {
        for sensor in &mut self.sensors {
            let i2c = &mut self.i2c;
            let mut device = sensor.get_device(i2c);
            let mut cur_res = device.read_resolution().map_err(|err| format!("{err:?}"))?;
            cur_res.set_resolution(res);
            sensor.res = res;
        }
        Ok(())
    }

    pub fn read_all(&mut self) -> Result<Vec<(String, f32)>, String> {
        let mut results = Vec::new();
        for sensor in &self.sensors {
            let i2c = &mut self.i2c;
            let mut device = sensor.get_device(i2c);
            let temp = device
                .read_temperature()
                .map_err(|err| format!("{err:?}"))?
                .get_celsius(sensor.res);
            results.push((sensor.loc.clone(), temp));
        }
        Ok(results)
    }
}

macro_rules! TempSensors {
    [$i2c: expr, $res: expr, $(($name:expr, $addr:expr)),* $(,)?] => {
        {
            let mut sensors = crate::tempsensor::TempSensors { sensors: Vec::new(), i2c: $i2c };
            $(
                sensors.add_sensor($name, $addr, $res);
            )*
            if let Err(e) = sensors.set_resolution($res) {
                Err(e)
            } else {
                Ok(sensors)
            }
        }
    };
}

#[cfg(test)]
mod test {
    use super::*;
    use linux_embedded_hal::I2cdev;
    use mcp9808::address::SlaveAddress;

    #[test]
    fn test_temp_sensor() {
        let mut i2c = I2cdev::new("/dev/i2c-1").unwrap();
        let sensor = TempSensor::new(
            "Test Sensor",
            SlaveAddress::Default,
            ResolutionVal::Deg_0_125C,
        );
        let mut device = sensor.get_device(&mut i2c);
        device.read_temperature().unwrap();
        let _sensors = {
            TempSensors!(
                i2c,
                ResolutionVal::Deg_0_125C,
                ("Sensor1", SlaveAddress::Default),
                ("Sensor2", SlaveAddress::from_u8(0x18).unwrap()),
                ("Sensor3", SlaveAddress::from_u8(0x19).unwrap())
            )
        };
    }
}
