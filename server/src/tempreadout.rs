use std::time::Duration;

use chrono::Utc;
use log::error;
use tokio::time::Instant;

use crate::tempsensor::TempSensors;

use packet::TempReadout;

fn read(sensors: &mut TempSensors<impl embedded_hal::i2c::I2c>) -> Result<TempReadout, String> {
    let now = Utc::now();
    let readings = sensors.read_all()?;
    Ok(TempReadout { now, readings })
}

pub struct TempReader {}

impl TempReader {
    pub fn run(
        sensors: TempSensors<impl embedded_hal::i2c::I2c + Send + 'static>,
        interval: Duration,
        capacity: usize,
    ) -> tokio::sync::broadcast::Receiver<TempReadout> {
        let (sender, receiver) = tokio::sync::broadcast::channel::<TempReadout>(capacity);

        tokio::task::spawn({
            async move {
                let mut sensors = sensors;
                loop {
                    let now = Instant::now();
                    if let Ok(read) = read(&mut sensors) {
                        match sender.send(read) {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Error sending temp readout: {:?}", e);
                                break;
                            }
                        }
                    }
                    if Instant::now().duration_since(now) < interval {
                        tokio::time::sleep(interval - Instant::now().duration_since(now)).await;
                    }
                }
            }
        });
        receiver
    }
}
