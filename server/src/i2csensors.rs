use std::{sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use crate::{tempsensor::*, REFCLK};
use bno055::{BNO055OperationMode, Bno055};
use linux_embedded_hal::I2cdev;
use mcp9808::{address::SlaveAddress, reg_res::ResolutionVal};
use packet::{Outgoing, QuaternionReadout};
use std::time::Instant;

pub fn i2c_sensors_task(
    i2c: I2cdev,
    bnosensors: &Vec<(String, u8)>,
    mcpsensors: &Vec<(String, u8)>,
    cadence: Duration,
    run: Arc<AtomicBool>,
    data_sink: tokio::sync::broadcast::Sender<Outgoing>,
) -> tokio::task::JoinHandle<()> {
    let mut i2c = i2c;
    let mut delay = linux_embedded_hal::Delay;
    let mut mcps = Mcp9808Sensors::new();
    for (loc, addr) in mcpsensors {
        if let Ok(addr) = SlaveAddress::from_u8(*addr) {
            mcps.add_sensor(loc, addr, ResolutionVal::Deg_0_125C, &mut i2c)
        }
    }
    let mut bnos = Vec::new();
    for (loc, addr) in bnosensors {
        if !(0x28..=0x29).contains(addr) {
            log::error!("Invalid BNO055 address: {}", addr);
            continue;
        }
        if let Ok(mut bno) = Bno055::new(*addr == 0x28, &mut i2c, &mut delay) {
            if bno
                .set_mode(BNO055OperationMode::NDOF, &mut i2c, &mut delay)
                .is_err()
            {
                log::error!("Failed to set BNO055 mode");
                continue;
            }
            bnos.push((loc.clone(), bno));
        } else {
            log::error!("Failed to initialize BNO055 sensor: {}", loc);
            continue;
        }
    }
    std::thread::sleep(Duration::from_secs(1)); // Give time for sensors to stabilize
    tokio::task::spawn({
        async move {
            let mut bnos = bnos;
            let mut mcps = mcps;
            let mut i2c = i2c;
            while run.load(Ordering::Relaxed) {
                let i2c = &mut i2c;
                let start = Instant::now();
                for (loc, bno) in &mut bnos {
                    let now = Instant::now();
                    if let Ok(quart) = bno.quaternion(i2c) {
                        let quart = QuaternionReadout {
                            now: now - *REFCLK,
                            readings: (loc.clone(), quart),
                        };
                        if let Err(e) =
                            data_sink.send(Outgoing::OrientationData(quart.clone()))
                        {
                            log::error!("Failed to send quaternion data: {}", e);
                        }
                    }
                }
                for mcp in &mut mcps.sensors {
                    if let Ok(reading) = mcp.read_temperature(i2c) {
                        if let Err(e) = data_sink.send(Outgoing::TempData(reading)) {
                            log::error!("Failed to send temperature data: {}", e);
                        }
                    }
                }
                let elapsed = start.elapsed();
                if elapsed < cadence {
                    let sleep_duration = cadence - elapsed;
                    tokio::time::sleep(sleep_duration).await;
                } else {
                    log::warn!("Sensor read took too long: {:?}", elapsed);
                }
            }
        }
    })
}
