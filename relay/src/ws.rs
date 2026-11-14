//! `GET /v1/ws`: challenge, proof of the `auth` key, then frames both ways. The relay forwards `body` strings it
//! cannot read; it holds no key that opens one.

use crate::hub::{now_ms, Out};
use crate::push::Outcome;
use crate::App;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use bobix_link::command::auth_bytes;
use bobix_link::keys::{device_id, verify_auth};
use bobix_link::relay_frames::{ErrorCode, Frame, FrameError};
use bobix_link::{b64, b64_decode, random};
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::time::{interval, timeout, Instant};

/// Before a socket has authenticated, a frame may be this long.
const PRE_AUTH_MAX: usize = 4 * 1024;

pub async fn upgrade(State(app): State<Arc<App>>, ConnectInfo(_addr): ConnectInfo<SocketAddr>, ws: WebSocketUpgrade) -> Response {
    // Frames up to 1 MiB are read so an oversize one (over 256 KiB) can be answered without closing the socket.
    ws.max_message_size(1 << 20).on_upgrade(move |s| socket(app, s))
}

async fn socket(app: Arc<App>, ws: WebSocket) {
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = unbounded_channel::<Out>();
    let writer = tokio::spawn(async move {
        while let Some(o) = rx.recv().await {
            let m = match o {
                Out::Frame(f) => match f.to_text() {
                    Ok(t) => Message::Text(t.into()),
                    Err(_) => continue,
                },
                Out::Ping => Message::Ping(Bytes::new()),
                Out::Close => break,
            };
            if sink.send(m).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    session(&app, &mut stream, &tx).await;
    let _ = tx.send(Out::Close);
    let _ = timeout(Duration::from_secs(2), writer).await;
}

fn reply(tx: &UnboundedSender<Out>, f: Frame) {
    let _ = tx.send(Out::Frame(f));
}

fn error(tx: &UnboundedSender<Out>, code: ErrorCode) {
    reply(tx, Frame::Error { code, who: None });
}

async fn session(app: &Arc<App>, stream: &mut SplitStream<WebSocket>, tx: &UnboundedSender<Out>) {
    let nonce = b64(&random::<32>());
    reply(tx, Frame::Challenge { nonce: nonce.clone() });

    // Ten seconds to prove the key, in one small frame.
    let first = match timeout(app.cfg.auth_within, stream.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => t,
        _ => return,
    };
    if first.len() > PRE_AUTH_MAX {
        return error(tx, ErrorCode::Oversize);
    }
    let Ok(Frame::Auth { id, key, sig, .. }) = Frame::parse(first.as_str()) else { return error(tx, ErrorCode::Auth) };
    let key: Option<[u8; 32]> = b64_decode(&key).and_then(|k| k.try_into().ok());
    let proven = key.is_some_and(|k| device_id(&k) == id && verify_auth(&k, &auth_bytes(&nonce), &b64_decode(&sig).unwrap_or_default()));
    if !proven {
        app.log.line("auth refused");
        return error(tx, ErrorCode::Auth);
    }

    let revoked = app.hub.lock().unwrap().revoked.is_revoked(&id, now_ms());
    if revoked {
        // A revoked device gets this on connect and on every frame after it, and nothing else.
        app.log.line(&format!("revoked device {id} refused"));
        error(tx, ErrorCode::Revoked);
        while let Ok(Some(Ok(_))) = timeout(app.cfg.dead_after, stream.next()).await {
            error(tx, ErrorCode::Revoked);
        }
        return;
    }

    let (guest, conn) = {
        let mut hub = app.hub.lock().unwrap();
        (hub.is_guest(&id), hub.connect(&id, tx.clone()))
    };
    app.log.line(&format!("connected {id}{}", if guest { " (guest)" } else { "" }));

    let mut last_seen = Instant::now();
    let mut ping = interval(app.cfg.ping_every);
    // Silence is judged often, so a device that stopped answering shows offline soon after `dead_after`.
    let mut check = interval((app.cfg.dead_after / 10).max(Duration::from_millis(20)));
    loop {
        tokio::select! {
            m = stream.next() => {
                let Some(Ok(m)) = m else { break };
                last_seen = Instant::now();
                match m {
                    Message::Text(t) => match Frame::parse(t.as_str()) {
                        Ok(f) => dispatch(app, &id, guest, f),
                        Err(FrameError::Oversize) => error(tx, ErrorCode::Oversize),
                        Err(FrameError::Malformed) => error(tx, ErrorCode::Malformed),
                    },
                    Message::Binary(_) => error(tx, ErrorCode::Malformed),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = ping.tick() => { let _ = tx.send(Out::Ping); }
            _ = check.tick() => {
                if last_seen.elapsed() > app.cfg.dead_after {
                    app.log.line(&format!("no answer to ping from {id}"));
                    break;
                }
            }
        }
    }
    app.hub.lock().unwrap().disconnect(&id, conn);
    app.log.line(&format!("disconnected {id}"));
}

fn dispatch(app: &Arc<App>, id: &str, guest: bool, f: Frame) {
    let jobs = app.hub.lock().unwrap().frame(id, guest, f);
    for job in jobs {
        let app = app.clone();
        tokio::spawn(async move {
            match app.sender.send(job.req.clone()).await {
                Outcome::Sent => app.log.line(&format!("push to {} sent", job.to)),
                Outcome::TokenGone => {
                    app.log.line(&format!("push token of {} rejected, forgotten", job.to));
                    app.hub.lock().unwrap().forget_token(&job.to, &job.req.token);
                }
                Outcome::Failed(why) => app.log.line(&format!("push to {} failed: {why}", job.to)),
            }
        });
    }
}
