use std::{
    io::ErrorKind, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::{Duration, Instant}
};

use packet::{GpsRawMessage, Outgoing};

use crate::REFCLK;

pub fn gps_task(
    gpsdev: String,
    gpsbaud: u32,
    run: Arc<AtomicBool>,
    data_sink: tokio::sync::broadcast::Sender<Outgoing>,
) {
    match serialport::new(&gpsdev, gpsbaud)
    .open()
    // Set the timeout on the serial port
    {
        Ok(mut port) => {
            port.set_timeout(Duration::from_millis(100))
            .expect("Failed to set timeout");
            log::trace!("Opened GPS device: {}", &gpsdev);
            while run.load(Ordering::Relaxed) {
                let mut buf = Vec::with_capacity(8192);
                if let Err(err) = port.read_to_end(&mut buf) {
                    if err.kind() != ErrorKind::TimedOut {
                        log::error!("Error reading from GPS device: {}", err);
                        break;
                    }
                }
                let now = Instant::now();
                if buf.is_empty() {
                    continue;
                }
                log::trace!("Received {} bytes from GPS device", buf.len());
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
                    log::trace!("Sent GPS data to sink");
                }
            }
        }

        Err(e) => {
            log::error!("Failed to open GPS device: {}", e);
        }
    }
    log::info!("GPS task finished");
}
