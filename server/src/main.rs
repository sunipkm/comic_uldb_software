use log::*;
use mcp9808::address::SlaveAddress;
use mcp9808::reg_res::ResolutionVal;
use packet::{CameraCommand, TempReadout};
use refimage::GenericImageOwned;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

mod camera;
mod config;
mod network;
#[macro_use]
mod tempsensor;
mod tempreadout;

use camera::camera_thread;

#[tokio::main]
async fn main() {
    env_logger::init();
    // Load static config
    let config = config::ProgConfig::from_file(&PathBuf::from("config.json")).unwrap_or({
        let cfg = config::ProgConfig {
            progname: "ASICam".to_string(),
            rootdir: "/tmp".to_string(),
            camconf: packet::CameraConfig::default(),
        };
        serde_json::to_writer_pretty(
            File::create("config.json").expect("Failed to create config file"),
            &cfg,
        )
        .expect("Failed to write config file");
        cfg
    });
    // open TCP port
    let addr = "0.0.0.0:52000";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    info!("Listening on: {}", addr);

    // create channels
    let (image_source, _) = broadcast::channel::<GenericImageOwned>(10);
    let (ccdtemp_source, _ccdtemp_sink) = broadcast::channel::<TempReadout>(1);
    let (config_source, _) = broadcast::channel::<CameraCommand>(1);
    let main_run = Arc::new(AtomicBool::new(true));

    // handle SIGINT
    tokio::spawn({
        let main_run = main_run.clone();
        async move {
            tokio::signal::ctrl_c().await.unwrap();
            main_run.store(false, Ordering::SeqCst);
        }
    });

    // temp sensor thread
    let i2c = linux_embedded_hal::I2cdev::new("/dev/i2c-1").expect("Failed to open I2C device");
    let sensors = TempSensors!(
        i2c,
        ResolutionVal::Deg_0_125C,
        ("FCL", SlaveAddress::from_u8(0x1b).unwrap()),
        ("EPL", SlaveAddress::from_u8(0x1c).unwrap()),
        ("LWL", SlaveAddress::from_u8(0x19).unwrap()),
        ("RWL", SlaveAddress::from_u8(0x18).unwrap())
    )
    .expect("Failed to create temp sensors");
    let _temp_readout =
        tempreadout::TempReader::run(sensors, std::time::Duration::from_secs(1), 10);

    // network client thread
    tokio::spawn({
        let main_run = main_run.clone();
        let image_sink = image_source.clone();
        let config_per = config_source.clone();
        async move {
            while let Ok((stream, _)) = listener.accept().await {
                let peer = stream
                    .peer_addr()
                    .expect("connected streams should have a peer address");
                info!("Peer address: {}", peer);
                let config = config_per.clone();
                tokio::spawn({
                    let receiver = image_sink.subscribe();
                    network::accept_connection(peer, stream, receiver, config, main_run.clone())
                });
            }
        }
    });

    // camera thread
    let handle = tokio::task::spawn_blocking(move || {
        camera_thread(
            main_run,
            config.camconf,
            image_source,
            ccdtemp_source,
            config_source.subscribe(),
        );
    });

    handle.await.expect("Failed to join handle");
    info!("Server exiting");
}
