//! `GET /v1/ws`: challenge, proof of the `auth` key, then frames both ways. The relay forwards `body` strings it
//! cannot read; it holds no key that opens one.

use crate::hub::{Out, Tx};
use crate::limits::client_ip;
use crate::log::quote;
use crate::push::Outcome;
use crate::App;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bobix_link::command::{verify_auth_frame, AuthRefusal};
use bobix_link::messages::MAX_FRAME;
use bobix_link::relay_frames::{ErrorCode, Frame, FrameError};
use bobix_link::{b64, random};
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, timeout, Instant};

/// Before a socket has authenticated, a frame may be this long.
const PRE_AUTH_MAX: usize = 4 * 1024;

pub async fn upgrade(State(app): State<Arc<App>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let ip = client_ip(&headers, addr, &app.cfg.client_ip_header);
    if !app.ips.lock().unwrap().admit(&ip) {
        app.log.line("connection refused: too many sockets or connections from one address");
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    // A little over one frame: an oversize frame (over 256 KiB) is answered and the socket stays; a bigger one ends it.
    ws.max_message_size(MAX_FRAME + 32 * 1024).on_upgrade(move |s| socket(app, s, ip))
}

async fn socket(app: Arc<App>, ws: WebSocket, ip: String) {
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = Tx::new(app.cfg.out_queue);
    let mut writer = tokio::spawn(async move {
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
    let mut slot = Some(ip.clone());
    session(&app, &mut stream, &tx, &mut slot).await;
    if let Some(ip) = slot {
        app.ips.lock().unwrap().release(&ip);
    }
    // A reader that stopped reading leaves the writer stuck: it is cut, not waited for.
    if !tx.send(Out::Close) || timeout(Duration::from_secs(2), &mut writer).await.is_err() {
        writer.abort();
    }
}

fn reply(tx: &Tx, f: Frame) {
    tx.send(Out::Frame(f));
}

fn error(tx: &Tx, code: ErrorCode) {
    reply(tx, Frame::Error { code, who: None });
}

async fn session(app: &Arc<App>, stream: &mut SplitStream<WebSocket>, tx: &Tx, slot: &mut Option<String>) {
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
    let Ok(Frame::Auth { id, key, relay, sig, .. }) = Frame::parse(first.as_str()) else { return error(tx, ErrorCode::Auth) };
    if let Err(refusal) = verify_auth_frame(&app.cfg.public_url, &nonce, &id, &key, &relay, &sig) {
        match refusal {
            AuthRefusal::OtherRelay => app.log.line(&format!(
                "auth refused: {id} signed for {}, but this relay's RELAY_PUBLIC_URL is {} (a wrong RELAY_PUBLIC_URL looks like this)",
                quote(&relay),
                app.cfg.public_url
            )),
            AuthRefusal::Invalid => app.log.line("auth refused: bad key or signature"),
        }
        return error(tx, refusal.code());
    }
    // Authenticated: this socket no longer counts as unauthenticated for its address.
    if let Some(ip) = slot.take() {
        app.ips.lock().unwrap().release(&ip);
    }

    let mut frames = (Instant::now(), 0u32);
    let mut over_rate = |cfg_max: u32| {
        if frames.0.elapsed() >= Duration::from_secs(1) {
            frames = (Instant::now(), 0);
        }
        frames.1 += 1;
        frames.1 > cfg_max
    };

    let guest = {
        let mut hub = app.hub.lock().unwrap();
        hub.is_guest(&id)
    };
    let conn = app.hub.lock().unwrap().connect(&id, tx.clone());
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
                if over_rate(app.cfg.device_frames_per_sec) {
                    app.log.line(&format!("{id} sent more than {} frames in a second: closed", app.cfg.device_frames_per_sec));
                    break;
                }
                match m {
                    Message::Text(t) => match Frame::parse(t.as_str()) {
                        Ok(f) => dispatch(app, &id, f),
                        Err(FrameError::Oversize) => error(tx, ErrorCode::Oversize),
                        Err(FrameError::Malformed) => error(tx, ErrorCode::Malformed),
                    },
                    Message::Binary(_) => error(tx, ErrorCode::Malformed),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = ping.tick() => { tx.send(Out::Ping); }
            _ = check.tick() => {
                if last_seen.elapsed() > app.cfg.dead_after {
                    app.log.line(&format!("no answer to ping from {id}"));
                    break;
                }
            }
            _ = tx.kill.notified() => {
                app.log.line(&format!("{id} does not read fast enough: closed"));
                break;
            }
        }
    }
    app.hub.lock().unwrap().disconnect(&id, conn);
    app.log.line(&format!("disconnected {id}"));
}

fn dispatch(app: &Arc<App>, id: &str, f: Frame) {
    let jobs = app.hub.lock().unwrap().frame(id, f);
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
