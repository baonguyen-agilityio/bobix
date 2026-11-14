//! Shared code of the fake tools and the in-process relay stub. All data here is made up.
#![allow(dead_code)]

pub mod desktop;
pub mod phone;
pub mod scenario;
pub mod stub;

use bobix_link::counters::RecvState;
use bobix_link::envelope::{Direction, VersionRange, CURRENT_VERSION};
use bobix_link::keys::Identity;
use bobix_link::link::Link;
use bobix_link::relay_frames::Frame;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_millis() as u64
}

/// A link to one peer. The fakes do not persist counter reservations: a real device stores `Sealed::reserve` first.
pub fn new_link(sends: Direction, own: &Identity, peer_box_public: [u8; 32]) -> Link {
    Link::new(sends, *own.box_secret(), peer_box_public, CURRENT_VERSION, VersionRange::current(), [0; 3], now_ms(), RecvState::default())
}

pub enum Recv {
    Frame(Frame),
    Timeout,
    Closed,
}

/// A WebSocket that speaks relay frames.
pub struct Conn {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Conn {
    pub async fn connect(url: &str) -> Result<Conn, String> {
        let (ws, _) = tokio_tungstenite::connect_async(url).await.map_err(|e| format!("cannot reach the relay at {url}: {e}"))?;
        Ok(Conn { ws })
    }

    pub async fn send(&mut self, f: &Frame) -> Result<(), String> {
        let text = f.to_text().map_err(|e| format!("frame refused: {e:?}"))?;
        self.ws.send(Message::text(text)).await.map_err(|e| format!("the relay closed the connection: {e}"))
    }

    pub async fn recv(&mut self, wait: Duration) -> Recv {
        let end = tokio::time::Instant::now() + wait;
        loop {
            match tokio::time::timeout_at(end, self.ws.next()).await {
                Err(_) => return Recv::Timeout,
                Ok(None) | Ok(Some(Err(_))) => return Recv::Closed,
                Ok(Some(Ok(Message::Text(t)))) => {
                    if let Ok(f) = Frame::parse(t.as_str()) {
                        return Recv::Frame(f);
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) => return Recv::Closed,
                Ok(Some(Ok(_))) => {}
            }
        }
    }

    /// Answers the relay's challenge.
    pub async fn login(&mut self, id: &Identity) -> Result<(), String> {
        let Recv::Frame(Frame::Challenge { nonce }) = self.recv(Duration::from_secs(5)).await else {
            return Err("the relay sent no challenge".into());
        };
        let sig = id.sign_auth(&bobix_link::command::auth_bytes(&nonce));
        self.send(&Frame::Auth {
            id: id.device_id(),
            key: bobix_link::b64(&id.public().auth),
            sig: bobix_link::b64(&sig),
            versions: VersionRange::current(),
        })
        .await
    }
}

pub fn pid() -> String {
    bobix_link::hex(&bobix_link::random::<8>())
}
