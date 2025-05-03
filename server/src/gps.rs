use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

use packet::{GpsRawMessage, Outgoing};
use tokio::io::AsyncReadExt;
use tokio_serial::SerialPortBuilderExt;

use crate::REFCLK;

pub async fn gps_task(
    gpsdev: String,
    gpsbaud: u32,
    data_sink: tokio::sync::broadcast::Sender<Outgoing>,
) {
    match tokio_serial::new(&gpsdev, gpsbaud)
        .timeout(Duration::from_millis(100))
        .open_native_async()
    {
        Ok(mut port) => {
            log::info!("Opened GPS device: {}", &gpsdev);
            loop {
                let mut buf = Vec::with_capacity(2048);
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
                log::info!("Received {} bytes from GPS device", buf.len());
                if let Err(e) = data_sink.send({
                    GpsRawMessage {
                        now: now - *REFCLK,
                        msg: buf,
                    }
                    .into()
                }) {
                    log::error!("Failed to send GPS data: {}", e);
                    break;
                } else {
                    log::info!("Sent GPS data to sink");
                }
            }
        }

        Err(e) => {
            log::error!("Failed to open GPS device: {}", e);
        }
    }
}
