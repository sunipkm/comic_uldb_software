use std::time::Duration;

use datastor::Binary;
use datastor::FmtInfo;
use packet::Outgoing;
use refimage::FitsCompression;
use refimage::FitsWrite;
use refimage::GenericImageOwned;
use tokio::sync::broadcast;
use tokio::task;

struct Fits {}
impl FmtInfo for Fits {
    fn delimiter() -> &'static [u8] {
        b""
    }

    fn extension() -> &'static str {
        "fits"
    }

    fn initialize<W>(writer: W, _progname: &str) -> std::io::Result<W>
    where
        W: std::io::Write,
    {
        Ok(writer)
    }
}

enum RawData {
    Temperature(Duration, Vec<u8>),
    GpsRaw(Duration, Vec<u8>),
    Orientation(Duration, Vec<u8>),
}

pub fn filestore_task(
    data_dir: &str,
    receiver: broadcast::Receiver<Outgoing>,
) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
    let (img_send, img_recv) = std::sync::mpsc::channel::<GenericImageOwned>();
    let (raw_send, raw_recv) = std::sync::mpsc::channel();

    let commhdl = tokio::task::spawn(async move {
        let mut receiver = receiver;
        let img_sender = img_send;
        let raw_sender = raw_send;
        loop {
            let msg = receiver.recv().await;
            match msg {
                Ok(Outgoing::ImageData(img)) => {
                    img_sender.send(img).expect("Failed to send image data");
                }
                Ok(Outgoing::TempData(temp)) => {
                    let dur = temp.now;
                    match bincode::serialize(&temp) {
                        Ok(temp) => {
                            if let Err(e) = raw_sender.send(RawData::Temperature(dur, temp)) {
                                log::error!(
                                    "Failed to send temperature data to synchronous thread: {}",
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to serialize temperature data: {}", e);
                        }
                    }
                }
                Ok(Outgoing::GpsRawMessage(msg)) => {
                    let dur = msg.now;
                    match bincode::serialize(&msg) {
                        Ok(msg) => {
                            if let Err(e) = raw_sender.send(RawData::GpsRaw(dur, msg)) {
                                log::error!("Failed to send GPS data to synchronous thread: {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to serialize GPS data: {}", e);
                        }
                    }
                }
                Ok(Outgoing::OrientationData(orient)) => {
                    let dur = orient.now;
                    match bincode::serialize(&orient) {
                        Ok(orient) => {
                            if let Err(e) = raw_sender.send(RawData::Orientation(dur, orient)) {
                                log::error!(
                                    "Failed to send orientation data to synchronous thread: {}",
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to serialize orientation data: {}", e);
                        }
                    }
                }
                Ok(_) => {
                    log::warn!("Received unknown packet type");
                }
                Err(e) => match e {
                    broadcast::error::RecvError::Closed => {
                        log::error!("Receiver closed");
                        break;
                    }
                    broadcast::error::RecvError::Lagged(val) => {
                        log::warn!("Receiver lagged by {} messages", val);
                    }
                },
            }
        }
    });

    let imghdl = task::spawn_blocking({
        let data_dir = data_dir.to_string();
        move || {
            let mut imgstor =
                datastor::ExecCountSingleFrame::<Fits>::new(&format!("{}/images", data_dir))
                    .expect("Failed to create image storage");
            loop {
                match img_recv.recv() {
                    Ok(img) => {
                        if let Ok(file) = imgstor.store_custom_writer() {
                            if let Err(e) = img.write_fits(file, FitsCompression::Rice, true) {
                                log::error!("Failed to write image data: {}", e);
                            }
                        } else {
                            log::error!("Failed to write image data");
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to receive image data: {}", e);
                        break;
                    }
                }
            }
        }
    });
    let i2cstorhdl = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_string();
        move || {
            let mut tempstor = datastor::ExecCountHourly::<Binary>::new(
                &format!("{}/temperature", data_dir),
                true,
                env!("CARGO_CRATE_NAME"),
            )
            .expect("Failed to create temperature storage");
            let mut gpsstor = datastor::ExecCountHourly::<Binary>::new(
                &format!("{}/gpsraw", data_dir),
                true,
                env!("CARGO_CRATE_NAME"),
            )
            .expect("Failed to create GPS storage");
            let mut orientstor = datastor::ExecCountHourly::<Binary>::new(
                &format!("{}/orientation", data_dir),
                true,
                env!("CARGO_CRATE_NAME"),
            )
            .expect("Failed to create orientation storage");
            loop {
                match raw_recv.recv() {
                    Ok(RawData::Temperature(dur, data)) => {
                        if let Err(e) = tempstor.store(&dur, data.as_slice()) {
                            log::error!("Failed to store temperature data: {}", e);
                        }
                    }
                    Ok(RawData::GpsRaw(dur, data)) => {
                        if let Err(e) = gpsstor.store(&dur, data.as_slice()) {
                            log::error!("Failed to store GPS data: {}", e);
                        }
                    }
                    Ok(RawData::Orientation(dur, data)) => {
                        if let Err(e) = orientstor.store(&dur, data.as_slice()) {
                            log::error!("Failed to store orientation data: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to receive raw data: {}", e);
                        break;
                    }
                }
            }
        }
    });
    (commhdl, imghdl, i2cstorhdl)
}
