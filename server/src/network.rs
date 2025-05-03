use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

use tokio::{net::TcpStream, sync::broadcast::Receiver};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Message, Result},
};

use packet::{CameraCommand, Outgoing};

pub async fn accept_connection(
    peer: SocketAddr,
    stream: TcpStream,
    receiver: Receiver<Outgoing>,
    sender: Sender<CameraCommand>,
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
    mut receiver: Receiver<Outgoing>,
    sender: Sender<CameraCommand>,
    run: Arc<AtomicBool>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await.expect("Failed to accept");
    info!("New WebSocket connection: {}", peer);
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    tokio::spawn({
        let run = run.clone();
        async move {
            while run.load(Ordering::SeqCst) {
                let msg = ws_receiver.next().await;
                if let Some(msg) = msg {
                    match msg {
                        Ok(msg) => {
                            if let Ok(msg) = msg.to_text() {
                                info!("Received message: {}", msg);
                                match serde_json::from_str(msg) {
                                    Ok(config) => {
                                        if let Err(e) = sender.send(config) {
                                            error!("Error sending config: {}", e);
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Error parsing message: {}", e);
                                    }
                                }
                            } else {
                                error!("Error converting message to text: {}", msg);
                            }
                        }
                        Err(e) => match e {
                            Error::ConnectionClosed => {
                                info!("Client disconnected: {}", peer);
                                break;
                            }
                            Error::AlreadyClosed => {
                                info!("Connection already closed: {}", peer);
                                break;
                            }
                            _ => {
                                error!("Error receiving message: {}", e);
                                continue;
                            }
                        },
                    }
                } else {
                    info!("Client disconnected: {}", peer);
                    break;
                }
            }
            log::info!("WebSocket receiver thread exiting");
        }
    });

    while run.load(Ordering::SeqCst) {
        match receiver.recv().await {
            Ok(tosend) => match bincode::serialize(&tosend) {
                Ok(tosend) => {
                    if let Err(e) = ws_sender.send(Message::Binary(tosend.into())).await {
                        error!("Error sending message: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Error serializing message: {}", e);
                }
            },
            Err(e) => {
                if e == tokio::sync::broadcast::error::RecvError::Closed {
                    info!("Connection closed: {}", peer);
                    break;
                } else {
                    error!("Error receiving message: {}", e);
                }
            }
        }
    }
    log::info!("WebSocket sender thread exiting");
    ws_sender.close().await?;
    log::info!("WebSocket connection closed: {}", peer);
    Ok(())
}
