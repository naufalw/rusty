use futures_util::StreamExt;
use log::{error, info};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::accept_async;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mock_ws = Arc::new(MockWs::new("127.0.0.1:8080"));

    mock_ws.start();

    sleep(Duration::from_secs(10)).await;

    info!("Timer finished");

    mock_ws.stop();

    info!("Start again");

    mock_ws.start();
}

struct MockWs {
    address: SocketAddr,
    should_run: Arc<AtomicBool>,
}

impl MockWs {
    fn new(address: &str) -> Self {
        let addr: SocketAddr = address.parse().expect("Invalid MockWs Server Address");

        Self {
            address: addr,
            should_run: Arc::new(AtomicBool::new(false)),
        }
    }

    fn start(self: &Arc<Self>) {
        let self_clone = Arc::clone(self);
        tokio::spawn(async move {
            self_clone.websocket().await;
        });
    }

    async fn websocket(&self) {
        let listener = TcpListener::bind(&self.address)
            .await
            .expect("Failed to bind");
        info!("Listening on: {}", self.address);

        let is_running = self.should_run.clone();
        is_running.store(true, Ordering::SeqCst);

        while is_running.load(Ordering::SeqCst) {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    tokio::spawn(Self::handle_connection(stream, peer_addr));
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e)
                }
            }
        }
    }

    fn stop(&self) {
        self.should_run.store(false, Ordering::SeqCst)
    }

    async fn handle_connection(stream: tokio::net::TcpStream, peer_addr: SocketAddr) {
        info!("Incoming TCP connection from: {}", peer_addr);

        if let Ok(mut ws_stream) = accept_async(stream).await {
            info!("WebSocket connection established: {}", peer_addr);

            while let Some(message) = ws_stream.next().await {
                if let Ok(msg) = message {
                    info!("Received message from {}: {}", peer_addr, msg);
                }
            }
        }
    }
}
