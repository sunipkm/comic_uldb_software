use log::*;
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

mod camera;
mod config;
mod network;
#[macro_use]
mod tempsensor;
mod filestor;
mod gps;
mod i2csensors;

use camera::camera_thread;
use i2csensors::i2c_sensors_task;

pub static REFCLK: LazyLock<Instant> = LazyLock::new(Instant::now);

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Initialize time
    let _ = &*REFCLK;
    env_logger::init();
    // Load static config
    let mut config = config::ProgConfig::from_file(&PathBuf::from("config.json")).unwrap_or({
        let cfg = config::ProgConfig {
            progname: "CoMIC_ULDB".to_string(),
            rootdir: "./".to_string(),
            bindaddr: "0.0.0.0".to_string(),
            bindport: 52000,
            camconf: packet::CameraConfig::default(),
            i2cdev: PathBuf::from("/dev/i2c-3"),
            i2c_cadence: Duration::from_secs_f32(0.5),
            bnosensors: Vec::new(),
            mcpsensors: Vec::new(),
            gpsdev: String::from("/dev/ttyAMA0"),
            gpsbaud: 115200,
        };
        serde_json::to_writer_pretty(
            File::create("config.json").expect("Failed to create config file"),
            &cfg,
        )
        .expect("Failed to write config file");
        cfg
    });
    // Create channels
    let (data_sender, _) = broadcast::channel(100);
    let (config_sender, config_receiver) = broadcast::channel(10);

    // create main thread control
    let main_run = Arc::new(AtomicBool::new(true));

    // Create Data Storage thread
    let (comhdl, imghdl, i2cstorhdl) =
        filestor::filestore_task(&config.rootdir, main_run.clone(), data_sender.subscribe());

    // GPS thread
    let gpshandle = tokio::task::spawn_blocking({
        let gpsdev = config.gpsdev.clone();
        let data_sender = data_sender.clone();
        let run = main_run.clone();
        move || gps::gps_task(gpsdev, config.gpsbaud, run, data_sender)
    });

    // Open I2C port
    if let Ok(i2cdev) = linux_embedded_hal::I2cdev::new(&config.i2cdev) {
        i2c_sensors_task(
            i2cdev,
            &config.bnosensors,
            &config.mcpsensors,
            config.i2c_cadence,
            main_run.clone(),
            data_sender.clone(),
        );
    }

    // open TCP port
    let addr = format!("{}:{}", config.bindaddr, config.bindport);
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    trace!("Listening on: {}", addr);

    // handle SIGINT
    let _ctrlchdl = tokio::spawn({
        let main_run = main_run.clone();
        async move {
            tokio::signal::ctrl_c().await.unwrap();
            main_run.store(false, Ordering::SeqCst);
        }
    });

    // network client thread
    let nethandle = tokio::spawn({
        let run = main_run.clone();
        let data = data_sender.clone();
        let config_per = config_sender.clone();
        async move {
            while run.load(Ordering::Relaxed) {
                tokio::select! {
                    msg = listener.accept() => {
                        if let Ok((stream, _)) = msg {
                            let peer = stream
                            .peer_addr()
                            .expect("connected streams should have a peer address");
                        trace!("Peer address: {}", peer);
                        let config = config_per.clone();
                        tokio::spawn({
                            let receiver = data.subscribe();
                            network::accept_connection(peer, stream, receiver, config, run.clone())
                        });
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {

                    }
                }
            }
            trace!("Network accept thread exiting");
        }
    });

    // camera thread
    let camerahandle = tokio::task::spawn(async move {
        camera_thread(
            main_run,
            &mut config.camconf,
            data_sender.clone(),
            config_receiver,
        )
        .await;
    });
    let _ = tokio::join!(
        gpshandle,
        camerahandle,
        comhdl,
        imghdl,
        i2cstorhdl,
        nethandle,
    );
    info!("Server exiting");
}
