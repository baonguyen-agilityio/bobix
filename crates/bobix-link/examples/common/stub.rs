//! A small in-process relay: challenge and auth, peers, send and ack, queueing for an offline peer, withdraw,
//! presence, pairing. It is not the real relay: no TLS, no limits, no push, nothing kept after it exits.

use bobix_link::relay_frames::*;
use bobix_link::{b64, b64_decode, sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[derive(Default)]
struct State {
    next_conn: u64,
    online: HashMap<String, (u64, UnboundedSender<Frame>)>,
    /// `allow[x]` are the devices that may send to `x`.
    allow: HashMap<String, HashSet<String>>,
    queue: HashMap<String, Vec<(String, String, String)>>,
    senders: HashMap<String, String>,
    pairs: HashMap<String, (String, Instant)>,
}

type Shared = Arc<Mutex<State>>;

fn tell(st: &State, id: &str, f: Frame) {
    if let Some((_, tx)) = st.online.get(id) {
        let _ = tx.send(f);
    }
}

fn presence(st: &State, id: &str, state: PresenceState) {
    for p in st.allow.get(id).into_iter().flatten() {
        tell(st, p, Frame::Presence { peer: id.into(), state });
    }
}

/// Starts the stub on a free local port and returns its `ws://` address, which is also its public URL.
pub async fn spawn() -> String {
    spawn_with_public_url(None).await
}

/// `public_url` is what the stub believes its own address is (a real relay has it in its settings; `relay --dev`
/// derives it from the bind address). A device that signed another address is refused with `relayUrl`.
pub async fn spawn_with_public_url(public_url: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("ws://{}", listener.local_addr().expect("addr"));
    let public = std::sync::Arc::new(public_url.map_or_else(|| url.clone(), str::to_string));
    let st: Shared = Shared::default();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let (st, public) = (st.clone(), public.clone());
            tokio::spawn(async move {
                if let Ok(ws) = tokio_tungstenite::accept_async(sock).await {
                    let _ = serve(ws, st, &public).await;
                }
            });
        }
    });
    url
}

async fn serve(ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, st: Shared, public: &str) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = unbounded_channel::<Frame>();
    tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if sink.send(Message::text(f.to_text().expect("frame"))).await.is_err() {
                break;
            }
        }
    });
    let nonce = b64(&bobix_link::random::<32>());
    let _ = tx.send(Frame::Challenge { nonce: nonce.clone() });
    let first = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.map_err(|_| "no auth")?;
    let Some(Ok(Message::Text(t))) = first else { return Err("no auth".into()) };
    let Ok(Frame::Auth { id, key, relay, sig, .. }) = Frame::parse(t.as_str()) else { return Err("no auth".into()) };
    if let Err(refusal) = bobix_link::command::verify_auth_frame(public, &nonce, &id, &key, &relay, &sig) {
        let _ = tx.send(Frame::Error { code: refusal.code(), who: None });
        tokio::time::sleep(Duration::from_millis(50)).await; // let the writer task send the error before the socket closes
        return Err(format!("auth refused: {refusal:?}"));
    }
    let conn_id = {
        let mut s = st.lock().unwrap();
        s.next_conn += 1;
        let c = s.next_conn;
        s.online.insert(id.clone(), (c, tx.clone()));
        presence(&s, &id, PresenceState::Online);
        for (pid, from, body) in s.queue.remove(&id).unwrap_or_default() {
            let _ = tx.send(Frame::Msg { pid, from, body });
        }
        c
    };
    while let Some(Ok(m)) = stream.next().await {
        let Message::Text(t) = m else { continue };
        match Frame::parse(t.as_str()) {
            Ok(f) => handle(&st, &id, &tx, f),
            Err(FrameError::Oversize) => drop(tx.send(Frame::Error { code: ErrorCode::Oversize, who: None })),
            Err(_) => drop(tx.send(Frame::Error { code: ErrorCode::Malformed, who: None })),
        }
    }
    let mut s = st.lock().unwrap();
    if s.online.get(&id).is_some_and(|(c, _)| *c == conn_id) {
        s.online.remove(&id);
        presence(&s, &id, PresenceState::Offline);
    }
    Ok(())
}

fn handle(st: &Shared, me: &str, tx: &UnboundedSender<Frame>, f: Frame) {
    let mut s = st.lock().unwrap();
    match f {
        Frame::Peers { allow } => {
            s.allow.insert(me.into(), allow.into_iter().collect());
        }
        Frame::Revoke { id } => {
            if let Some(a) = s.allow.get_mut(me) {
                a.remove(&id);
            }
            s.queue.remove(&id);
            tell(&s, &id, Frame::Error { code: ErrorCode::Revoked, who: None });
        }
        Frame::State { asleep } => presence(&s, me, if asleep { PresenceState::Asleep } else { PresenceState::Online }),
        Frame::Send { pid, to, body, .. } => {
            if !s.allow.get(&to).is_some_and(|a| a.contains(me)) {
                let _ = tx.send(Frame::Error { code: ErrorCode::NoPeer, who: None });
                return;
            }
            s.senders.insert(pid.clone(), me.into());
            if s.online.contains_key(&to) {
                tell(&s, &to, Frame::Msg { pid, from: me.into(), body });
            } else {
                s.queue.entry(to).or_default().push((pid.clone(), me.into(), body));
                let _ = tx.send(Frame::Queued { pid });
            }
        }
        Frame::Ack { pid } => {
            if let Some(from) = s.senders.remove(&pid) {
                tell(&s, &from, Frame::Delivered { pid });
            }
        }
        Frame::Withdraw { pid } => {
            let mut found = false;
            for q in s.queue.values_mut() {
                let n = q.len();
                q.retain(|(p, _, _)| *p != pid);
                found |= q.len() != n;
            }
            let _ = tx.send(if found { Frame::Withdrawn { pid } } else { Frame::TooLate { pid } });
        }
        Frame::PairOpen { relay_token_hash, ttl } => {
            s.pairs.insert(relay_token_hash, (me.into(), Instant::now() + Duration::from_secs(ttl as u64)));
        }
        Frame::PairSend { relay_token, body } => {
            let hash = b64_decode(&relay_token).map(|t| b64(&sha256(&[&t]))).unwrap_or_default();
            match s.pairs.get(&hash).cloned() {
                None => drop(tx.send(Frame::Error { code: ErrorCode::NoPeer, who: None })),
                Some((_, until)) if until < Instant::now() => drop(tx.send(Frame::Error { code: ErrorCode::PairExpired, who: None })),
                Some((desktop, _)) => tell(&s, &desktop, Frame::Msg { pid: super::pid(), from: me.into(), body }),
            }
        }
        _ => {}
    }
}
