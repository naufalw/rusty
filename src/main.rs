use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::{SinkExt, TryStreamExt};
use log::{debug, error, info};
use tokio::{net::TcpListener, sync::oneshot, time::sleep};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    env_logger::init();

    let mock_ws = Arc::new(MockWS::new("127.0.0.1:8080".to_string()));

    mock_ws.start();

    info!("Started");

    sleep(Duration::from_secs(10)).await;

    mock_ws.stop();
    info!("Stopped");

    sleep(Duration::from_secs(10)).await;

    mock_ws.start();
    info!("Starting agian");

    sleep(Duration::from_secs(10)).await;
}

struct MockWS {
    addr: SocketAddr,
    sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    receiver: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    connection_senders: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
}

impl MockWS {
    fn new(addr: String) -> Self {
        let addr: SocketAddr = addr.parse().expect("Failed to parse address");
        let (sender, receiver) = oneshot::channel();
        Self {
            addr,
            sender: Arc::new(Mutex::new(Some(sender))),
            receiver: Arc::new(Mutex::new(Some(receiver))),
            connection_senders: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start(self: &Arc<Self>) {
        self.stop();

        let self_clone = self.clone();

        self_clone.reset();

        tokio::spawn(async move { self_clone.websocket().await });
    }

    fn reset(&self) {
        let (sender, receiver) = oneshot::channel();

        // Reset the sender
        *self.sender.lock().unwrap() = Some(sender);

        // Reset the receiver
        *self.receiver.lock().unwrap() = Some(receiver);

        // Clear connection senders
        self.connection_senders.lock().unwrap().clear();
    }

    async fn websocket(&self) -> io::Result<()> {
        let receiver = self
            .receiver
            .lock()
            .unwrap()
            .take()
            .expect("Receiver already taken");

        let listener = TcpListener::bind(self.addr.clone()).await?;

        tokio::select! {
            _ = async {
                loop {
                    info!("Waiting for connection");

                    match listener.accept().await {
                        Ok((socket, peer_addr)) => {
                                                    info!("Accepted connection from: {}", peer_addr);
                                                    let (conn_sender, conn_receiver) = oneshot::channel();
                                                    self.connection_senders.lock().unwrap().push(conn_sender);
                                                    tokio::spawn(handle_connection(socket, peer_addr, conn_receiver));
                                                }
                        Err(e) => {
                            error!("failed to accept socket; error = {:?}", e);
                        }
                    }
                }

                // Help the rust type inferencer out

            } => {}
            _ = receiver => {
                debug!("terminating accept loop");

                // Signal all active connections to terminate
                let senders = std::mem::take(&mut *self.connection_senders.lock().unwrap());
                for sender in senders {
                    let _ = sender.send(());
                }
            }
        }

        Ok(())
    }

    fn stop(&self) {
        if let Some(sender) = self.sender.lock().unwrap().take() {
            sender.send(()).expect("Failed to send stop signal");
        }
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer_addr: SocketAddr,
    mut shutdown: oneshot::Receiver<()>,
) {
    info!("Incoming TCP connection from: {}", peer_addr);

    if let Ok(mut ws_stream) = accept_async(stream).await {
        info!("WebSocket connection established: {}", peer_addr);

        loop {
            tokio::select! {
                msg = ws_stream.try_next() => {
                    match msg {
                        Ok(Some(message)) => {
                            match message {
                                Message::Text(text) => {
                                    debug!("Received text message: {}", text);
                                }
                                Message::Binary(bin) => {
                                    debug!("Received binary message: {:?}", bin);
                                }
                                Message::Ping(payload) => {
                                    debug!("Received ping message: {:?}", payload);
                                    ws_stream.send(Message::Pong(payload)).await.expect("Failed to send pong");
                                }
                                Message::Pong(_) => {
                                    debug!("Received pong message");
                                }
                                Message::Close(_) => {
                                    debug!("Received close message");
                                    break;
                                }
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                _ = &mut shutdown => {
                    debug!("Connection shutdown received for {}", peer_addr);
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }
    }

    info!("Connection closed for: {}", peer_addr);
}
