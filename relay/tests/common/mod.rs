//! Shared helpers of the integration tests: an in-process relay and a small WebSocket client that speaks frames.
#![allow(dead_code)]

use bobix_link::envelope::VersionRange;
use bobix_link::keys::Identity;
use bobix_link::relay_frames::*;
use bobix_link::{b64, sha256};
use bobix_relay::config::Config;
use bobix_relay::{run, App};
use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub struct Server {
    pub addr: SocketAddr,
    pub app: Arc<App>,
}

/// Generous timings, so a slow runner never trips a timer by accident: a device is gone after 30 s of silence and has
/// 5 s to authenticate. The tests about those timers shrink them for themselves. Waits for an event (`expect`) are
/// long; only the checks that something does NOT happen use short waits.
pub fn cfg() -> Config {
    let mut c = Config::base(true);
    c.relay_id = "test-relay".into();
    c.ping_every = Duration::from_millis(100);
    c.dead_after = Duration::from_secs(30);
    c.auth_within = Duration::from_secs(5);
    c.sweep_every = Duration::from_millis(50);
    c
}

pub async fn start(mut cfg: Config) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    cfg.public_url = format!("ws://{addr}");
    let app = App::with_fake(cfg);
    tokio::spawn(run(app.clone(), listener));
    Server { addr, app }
}

pub fn ident() -> Arc<Identity> {
    Arc::new(Identity::generate())
}

pub fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bobix-relay-test-{name}-{}", bobix_link::hex(&bobix_link::random::<6>())))
}

/// The `auth` frame of a device that is connected to the relay at `url`.
pub fn auth_frame(id: &Identity, url: &str, nonce: &str) -> Frame {
    bobix_link::command::auth_frame(id, url, nonce, VersionRange::current()).expect("an acceptable relay address")
}

pub fn send(to: &str, pid: &str, ttl: u32, collapse: Option<&str>, body: &str) -> Frame {
    Frame::Send { pid: pid.into(), to: to.into(), ttl, collapse: collapse.map(Into::into), push: None, body: body.into() }
}

/// A pairing token and the hash a desktop gives the relay for it.
pub fn token_pair(n: u8) -> (String, String) {
    let token = sha256(&[b"test-token", &[n]]);
    (b64(&token), b64(&sha256(&[&token])))
}

/// A push grant, minted the way a phone would.
pub fn grant(id: &Identity, token: &str, relay: &str, expires_ms: u64) -> String {
    let sig = id.sign_auth(&bobix_relay::push::grant_bytes(token, relay, expires_ms));
    bobix_relay::push::encode_grant(&id.public().auth, expires_ms, &sig)
}

pub fn now_ms() -> u64 {
    bobix_relay::log::now_ms()
}

pub struct Dev {
    pub ident: Arc<Identity>,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub url: String,
    pending: VecDeque<Frame>,
}

impl Dev {
    /// Connected, with no `auth` sent.
    pub async fn raw(s: &Server) -> Dev {
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/v1/ws", s.addr)).await.unwrap();
        Dev { ident: ident(), ws, url: format!("ws://{}", s.addr), pending: VecDeque::new() }
    }

    /// Connected and authenticated; returns once the relay has processed a frame of this device.
    pub async fn connect(s: &Server, id: Arc<Identity>) -> Dev {
        let mut d = Dev::raw(s).await;
        d.ident = id;
        let Frame::Challenge { nonce } = d.next(2000).await else { panic!("no challenge") };
        let frame = auth_frame(&d.ident, &d.url, &nonce);
        d.send(&frame).await;
        d.sync().await;
        d
    }

    pub fn id(&self) -> String {
        self.ident.device_id()
    }

    pub async fn send(&mut self, f: &Frame) {
        self.send_text(&f.to_text().unwrap()).await
    }

    pub async fn send_text(&mut self, t: &str) {
        self.ws.send(Message::text(t.to_owned())).await.unwrap();
    }

    /// Like `send_text`, but false when the relay has closed the socket (which is what a flood is meant to cause).
    pub async fn try_send_text(&mut self, t: &str) -> bool {
        self.ws.send(Message::text(t.to_owned())).await.is_ok()
    }

    /// Everything that arrives before the answer to a `withdraw` of an unknown pid is kept; the answer is not.
    pub async fn sync(&mut self) {
        self.send(&Frame::Withdraw { pid: "sync".into() }).await;
        loop {
            match self.read(2000).await {
                Some(Frame::TooLate { pid }) if pid == "sync" => return,
                Some(f @ Frame::Error { .. }) => {
                    self.pending.push_back(f);
                    return;
                }
                Some(f) => self.pending.push_back(f),
                None => return,
            }
        }
    }

    async fn read(&mut self, ms: u64) -> Option<Frame> {
        let end = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(end, self.ws.next()).await {
                Err(_) | Ok(None) | Ok(Some(Err(_))) => return None,
                Ok(Some(Ok(Message::Text(t)))) => return Frame::parse(t.as_str()).ok(),
                Ok(Some(Ok(Message::Close(_)))) => return None,
                Ok(Some(Ok(_))) => {}
            }
        }
    }

    async fn pull(&mut self, ms: u64) -> Option<Frame> {
        match self.pending.pop_front() {
            Some(f) => Some(f),
            None => self.read(ms).await,
        }
    }

    pub async fn next(&mut self, ms: u64) -> Frame {
        self.pull(ms).await.expect("a frame within the time")
    }

    /// Reads until a frame matches (others are skipped), or panics at the deadline.
    pub async fn expect(&mut self, pred: impl Fn(&Frame) -> bool, ms: u64) -> Frame {
        let ms = ms.max(8000); // an event that should happen: wait for it, a failure still ends the test
        let end = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let left = end.saturating_duration_since(tokio::time::Instant::now()).as_millis() as u64;
            match self.pull(left).await {
                Some(f) if pred(&f) => return f,
                Some(_) => {}
                None => panic!("no matching frame within {ms} ms"),
            }
        }
    }

    /// Asserts that no frame matching `pred` arrives within `ms`.
    pub async fn none(&mut self, pred: impl Fn(&Frame) -> bool, ms: u64) {
        let end = tokio::time::Instant::now() + Duration::from_millis(ms);
        while let Some(f) = {
            let left = end.saturating_duration_since(tokio::time::Instant::now()).as_millis() as u64;
            self.pull(left).await
        } {
            assert!(!pred(&f), "unexpected frame {:?}", std::mem::discriminant(&f));
        }
    }

    /// Asserts that the relay closes the socket within `ms`.
    pub async fn closed(&mut self, ms: u64) {
        let end = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(end, self.ws.next()).await {
                Err(_) => panic!("the socket stayed open"),
                Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return,
                Ok(Some(Ok(_))) => {}
            }
        }
    }
}

/// A desktop and a phone, connected, each allowing the other.
pub async fn pair_up(s: &Server) -> (Dev, Dev) {
    let (mut d, mut p) = (Dev::connect(s, ident()).await, Dev::connect(s, ident()).await);
    d.send(&Frame::Peers { allow: vec![p.id()] }).await;
    p.send(&Frame::Peers { allow: vec![d.id()] }).await;
    // Presence is shared once both allow each other.
    d.expect(|f| matches!(f, Frame::Presence { state: PresenceState::Online, .. }), 2000).await;
    p.expect(|f| matches!(f, Frame::Presence { state: PresenceState::Online, .. }), 2000).await;
    (d, p)
}
