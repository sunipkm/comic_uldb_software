use log::*;
use refimage::GenericImageOwned;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

mod camera;
mod config;
mod network;

use camera::camera_thread;
use config::ASICamconfig;

#[tokio::main]
async fn main() {
    env_logger::init();

    let (image_source, _) = broadcast::channel::<GenericImageOwned>(10);
    let (config_source, _) = broadcast::channel::<ASICamconfig>(1);
    let main_run = Arc::new(AtomicBool::new(true));

    let addr = "0.0.0.0:9001";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    info!("Listening on: {}", addr);

    // handle SIGINT
    tokio::spawn({
        let main_run = main_run.clone();
        async move {
            tokio::signal::ctrl_c().await.unwrap();
            main_run.store(false, Ordering::SeqCst);
        }
    });

    // accept client connections
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
        camera_thread(main_run, image_source, config_source.subscribe());
    });

    handle.await.expect("Failed to join handle");
    info!("Server exiting");
}
