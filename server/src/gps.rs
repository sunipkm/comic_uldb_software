use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

use packet::{GpsRawMessage, Outgoing};
use tokio::io::AsyncReadExt;
use tokio_serial::SerialPortBuilderExt;

use crate::REFCLK;

pub fn gps_task(
    gpsdev: &str,
    gpsbaud: u32,
    data_sink: tokio::sync::broadcast::Sender<Outgoing>,
) -> Result<tokio::task::JoinHandle<()>, String> {

    match tokio_serial::new(gpsdev, gpsbaud).timeout(Duration::from_millis(200)).open_native_async() {
        Ok(mut port) => {
            log::info!("Opened GPS device: {}", gpsdev);
            Ok(tokio::task::spawn(async move {
                loop {
                    let mut buf = Vec::with_capacity(8192);
                    if let Err(err) = port.read_to_end(&mut buf).await {
                        if err.kind() != ErrorKind::TimedOut {
                            log::error!("Error reading from GPS device: {}", err);
                            break;
                        }
                    }
                    let now = Instant::now();
                    if buf.is_empty() {
                        continue;
                    }
                    if let Err(e) = data_sink.send({
                        GpsRawMessage {
                            now: now - *REFCLK,
                            msg: buf,
                        }.into()
                    }) {
                        log::error!("Failed to send GPS data: {}", e);
                        break;
                    }
                }
            }))
        }
        Err(e) => {
            log::error!("Failed to open GPS device: {}", e);
            Err(format!("Failed to open GPS device: {}", e))
        }
    }
}
