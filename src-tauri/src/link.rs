// The relay link: one outbound WebSocket (wss://, or ws:// to a relay on this computer), reconnecting with backoff and
// jitter while phone access wants it. It only moves frames; phones.rs decides what they mean.
use crate::{phones::{self, Out}, Bobix};
use bobix_link::relay_frames::Frame;
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio_tungstenite::{tungstenite::Message, Connector};

pub fn spawn(b: Arc<Bobix>) {
    let (tx, rx) = unbounded_channel();
    b.core.lock().unwrap().phones.tx = Some(tx);
    tauri::async_runtime::spawn(run(b, rx));
}

enum Ended { Closed, Lost }

async fn run(b: Arc<Bobix>, mut rx: UnboundedReceiver<Out>) {
    let mut wait = 1.0f64;
    while phones::wants_link(&b) {
        match session(&b, &mut rx).await {
            Ended::Closed => wait = 1.0,
            Ended::Lost => {
                phones::link_lost(&b);
                let jitter = bobix_link::random::<1>()[0] as f64 / 255.0 * wait / 2.0;
                tokio::time::sleep(Duration::from_secs_f64(wait + jitter)).await;
                wait = (wait * 2.0).min(60.0);
            }
        }
    }
    phones::link_ended(&b);
}

// What the relay may send: a frame is at most MAX_FRAME, so a message over a little more is refused instead of buffered.
fn ws_config() -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
    tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(bobix_link::messages::MAX_FRAME + 32 * 1024))
        .max_frame_size(Some(bobix_link::messages::MAX_FRAME + 32 * 1024))
}

fn tls() -> Connector {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions().expect("protocol versions").with_root_certificates(roots).with_no_client_auth();
    Connector::Rustls(Arc::new(cfg))
}

async fn session(b: &Arc<Bobix>, rx: &mut UnboundedReceiver<Out>) -> Ended {
    let url = phones::relay_url(b);
    if url.is_empty() { return Ended::Lost; }
    let (mut ws, _) = match tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async_tls_with_config(url.as_str(), Some(ws_config()), false, Some(tls()))).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => { eprintln!("[bobix] phones: cannot reach the relay: {e}"); return Ended::Lost; }
        Err(_) => { eprintln!("[bobix] phones: the relay did not answer in time"); return Ended::Lost; }
    };
    // the relay speaks first: a challenge, answered with a signature for the address this connection was opened to
    let Some(nonce) = next_frame(&mut ws, Duration::from_secs(10)).await.and_then(|f| if let Frame::Challenge { nonce } = f { Some(nonce) } else { None }) else { eprintln!("[bobix] phones: the relay sent no challenge"); return Ended::Lost };
    let mut pace = Pace::new();
    let Some(auth) = phones::auth_frame(b, &url, &nonce) else { eprintln!("[bobix] phones: no identity or an unusable relay address"); return Ended::Lost };
    if send(&mut ws, &mut pace, &auth).await.is_err() { return Ended::Lost; }
    for f in phones::on_connected(b) { if send(&mut ws, &mut pace, &f).await.is_err() { return Ended::Lost; } }
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            m = ws.next() => match m {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => { eprintln!("[bobix] phones: the relay closed the connection"); return Ended::Lost }
                Some(Ok(Message::Text(t))) => if let Ok(f) = Frame::parse(t.as_str()) {
                    for r in phones::on_frame(b, f) { if send(&mut ws, &mut pace, &r).await.is_err() { return Ended::Lost; } }
                },
                Some(Ok(_)) => {}
            },
            o = rx.recv() => match o {
                Some(Out::Frame(f)) => if send(&mut ws, &mut pace, &f).await.is_err() { return Ended::Lost; },
                Some(Out::Close) | None => { let _ = ws.close(None).await; return Ended::Closed; }
            },
            _ = tick.tick() => {
                for r in phones::tick(b) { if send(&mut ws, &mut pace, &r).await.is_err() { return Ended::Lost; } }
                if phones::url_changed(b, &url) { let _ = ws.close(None).await; return Ended::Closed; }
            }
        }
    }
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// The relay closes a socket that sends over 100 frames in a second, counted in its own windows that this side cannot see:
// so no 1-second span may hold more than FRAMES_PER_SEC (a sliding window, not a fixed one: two bursts at the edge of
// two fixed windows would still be 2 x the limit inside one of the relay's). A burst waits.
const FRAMES_PER_SEC: usize = 45;
struct Pace { sent: std::collections::VecDeque<std::time::Instant> }
impl Pace {
    fn new() -> Pace { Pace { sent: Default::default() } }
    // How long a frame must wait if it is to go out at `now`.
    fn delay(&mut self, now: std::time::Instant) -> Duration {
        while self.sent.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1)) { self.sent.pop_front(); }
        if self.sent.len() < FRAMES_PER_SEC { return Duration::ZERO; }
        (self.sent[self.sent.len() - FRAMES_PER_SEC] + Duration::from_secs(1)).saturating_duration_since(now)
    }
    fn record(&mut self, at: std::time::Instant) { self.sent.push_back(at); }
    async fn turn(&mut self) {
        let d = self.delay(std::time::Instant::now());
        if !d.is_zero() { tokio::time::sleep(d).await; }
        self.record(std::time::Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_second_holds_more_frames_than_the_limit_however_the_bursts_fall() {
        let mut pace = Pace::new();
        let start = std::time::Instant::now();
        let mut now = start;
        let mut at = vec![];
        // 5 bursts of 120 frames, 700 ms apart: two of them would fit in two fixed one-second windows
        for burst in 0..5 {
            now = now.max(start + Duration::from_millis(700 * burst));
            for _ in 0..120 {
                now += pace.delay(now);
                pace.record(now);
                at.push(now);
            }
        }
        for (i, t) in at.iter().enumerate() {
            let n = at[i..].iter().take_while(|u| u.duration_since(*t) < Duration::from_secs(1)).count();
            assert!(n <= FRAMES_PER_SEC, "{n} frames in the second after frame {i}");
        }
        assert!(at.len() == 600 && at.last().unwrap().duration_since(start) > Duration::from_secs(10), "all of them go, only later");
        assert_eq!(Pace::new().delay(start), Duration::ZERO, "an idle link never waits");
    }
}

async fn send(ws: &mut Ws, pace: &mut Pace, f: &Frame) -> Result<(), ()> {
    let Ok(t) = f.to_text() else { return Ok(()) }; // a frame over the limit is dropped, not a lost connection
    pace.turn().await;
    ws.send(Message::text(t)).await.map_err(|_| ())
}

async fn next_frame(ws: &mut Ws, wait: Duration) -> Option<Frame> {
    let end = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(end, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => if let Ok(f) = Frame::parse(t.as_str()) { return Some(f); },
            Ok(Some(Ok(_))) => {}
            _ => return None,
        }
    }
}
