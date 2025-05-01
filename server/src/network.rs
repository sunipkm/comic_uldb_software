use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use refimage::GenericImageOwned;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::select;
use tokio::sync::broadcast::Sender;

use tokio::{net::TcpStream, sync::broadcast::Receiver};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};

use packet::Packet;

use crate::config::ASICamconfig;

pub async fn accept_connection(
    peer: SocketAddr,
    stream: TcpStream,
    receiver: Receiver<GenericImageOwned>,
    sender: Sender<ASICamconfig>,
    run: Arc<AtomicBool>,
) {
    if let Err(e) = handle_connection(peer, stream, receiver, sender, run).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 | Error::AlreadyClosed => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    mut receiver: Receiver<GenericImageOwned>,
    sender: Sender<ASICamconfig>,
    run: Arc<AtomicBool>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await.expect("Failed to accept");
    info!("New WebSocket connection: {}", peer);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (cfg_ack_sender, mut cfg_ack_receiver) = tokio::sync::mpsc::channel(1);

    tokio::spawn({
        let run = run.clone();
        async move {
            while run.load(Ordering::SeqCst) {
                let msg = ws_receiver.next().await;
                if handle_client_message(msg, &sender, &cfg_ack_sender)
                    .await
                    .is_none()
                {
                    break;
                }
            }
        }
    });

    while run.load(Ordering::SeqCst) {
        select! {
            image = receiver.recv() => {
                if let Ok(image) = image {
                    let packet = Packet::Response(0, 0, Ok(image.into()));
                    let packet = bincode::serialize(&packet).expect("Error serializing packet");
                    ws_sender.send(Message::Binary(packet.into())).await?;
                } else {
                    break;
                }
            }
            reply = cfg_ack_receiver.recv() => {
                if let Some(reply) = reply {
                    ws_sender.send(Message::Text(serde_json::to_string(&reply).unwrap().into())).await?;
                } else {
                    break;
                }
            }
        }
    }

    ws_sender.close().await?;
    Ok(())
}

async fn handle_client_message(
    msg: Option<Result<Message, Error>>,
    sender: &Sender<ASICamconfig>,
    cfg_ack_sender: &tokio::sync::mpsc::Sender<Result<String, String>>,
) -> Option<()> {
    match msg {
        Some(msg) => match msg {
            Ok(msg) => {
                if let Ok(msg) = msg.to_text() {
                    info!("Received message: {}", msg);
                    if let Ok(config) = serde_json::from_str::<ASICamconfig>(msg) {
                        if let Err(e) = sender.send(config) {
                            error!("Error sending config: {}", e);
                            if let Err(e) = cfg_ack_sender
                                .send(Err(format!("Error sending config: {}", e)))
                                .await
                            {
                                error!("Error sending config ack: {}", e);
                            }
                        } else {
                            info!("Config sent to camera thread");
                            if let Err(e) =
                                cfg_ack_sender.send(Ok("Config received".to_string())).await
                            {
                                error!("Error sending config ack: {}", e);
                            } else {
                                info!("Config ack sent");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error receiving message: {}", e);
                return None;
            }
        },
        None => return None,
    }
    Some(())
}
