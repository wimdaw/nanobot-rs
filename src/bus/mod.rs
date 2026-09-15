pub mod events;

pub use events::*;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};

#[derive(Clone)]
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Arc<Mutex<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: Arc<Mutex<mpsc::Receiver<OutboundMessage>>>,
    event_tx: broadcast::Sender<StreamEvent>,
}

impl MessageBus {
    pub fn new(capacity: usize) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(capacity);
        let (outbound_tx, outbound_rx) = mpsc::channel(capacity);
        let (event_tx, _) = broadcast::channel(capacity * 2);

        Self {
            inbound_tx,
            inbound_rx: Arc::new(Mutex::new(inbound_rx)),
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(outbound_rx)),
            event_tx,
        }
    }

    pub async fn send_inbound(&self, msg: InboundMessage) -> anyhow::Result<()> {
        self.inbound_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send inbound: {}", e))
    }

    pub async fn recv_inbound(&self) -> Option<InboundMessage> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await
    }

    pub async fn send_outbound(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        self.outbound_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send outbound: {}", e))
    }

    pub async fn recv_outbound(&self) -> Option<OutboundMessage> {
        let mut rx = self.outbound_rx.lock().await;
        rx.recv().await
    }

    pub fn publish_event(&self, event: StreamEvent) {
        let _ = self.event_tx.send(event);
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_tx.subscribe()
    }
}
