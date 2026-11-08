// Local HTTP server: receives hook events, streams session state (SSE) and serves the UI pages. Port of
// the 0.1 (Electron) app's core/server.js. Bound to 127.0.0.1. Host is checked (DNS rebinding). Events and the state stream need the
// per-user token (x-bobix header / ?t=): other local users can't feed or read it, and a web page can't send the header
// cross-origin without a CORS preflight we never answer.
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse, Response},
    Router,
};
use futures_util::stream;
use serde_json::Value;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{broadcast, oneshot};

// What the app does with an event, for the hook that sent it.
pub enum Reply {
    // A session title (SessionStart): answered as soon as it is known, well inside the hook's wait.
    Output(oneshot::Receiver<Option<Value>>),
    // A permission request held for an answer: the reply's headers go out at once with the wait time (seconds or
    // "forever"), so the hook knows when to give up. `gone` is dropped when the reply ends, also when the hook's
    // request closes first (Claude moved on).
    Answer { secs: String, rx: oneshot::Receiver<Option<Value>>, gone: oneshot::Sender<()> },
}

pub type OnEvent = dyn Fn(Value, bool) -> Option<Reply> + Send + Sync;

pub struct Ctx {
    pub token: String,
    pub on_event: Box<OnEvent>,
    pub snapshot: Box<dyn Fn() -> String + Send + Sync>,
    pub asset: Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>,
    pub tx: broadcast::Sender<String>,
    pub on_show: Box<dyn Fn(String) + Send + Sync>, // another copy of Bobix was opened: its version
    pub pet_dir: Option<std::path::PathBuf>, // project pet pictures (petstore)
}

const MAX_BODY: usize = 8_000_000;
const ASSETS: [&str; 11] = ["theme.css", "creature.js", "i18n.js", "markdown.js", "log.js", "pet-import.js", "pixelate.js", "pixelate-worker.js",
    "gif/lzw.js", "gif/deinterlace.js", "gif/gifenc.js"];

fn ok_host(h: Option<&HeaderValue>) -> bool {
    let Some(h) = h.and_then(|h| h.to_str().ok()) else { return false };
    let (name, port) = h.split_once(':').map(|(n, p)| (n, Some(p))).unwrap_or((h, None));
    (name == "127.0.0.1" || name == "localhost") && port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

// Constant time, so the token can't be guessed byte by byte.
fn ok_token(token: &str, t: Option<&str>) -> bool {
    let Some(t) = t else { return false };
    t.len() == token.len() && t.bytes().zip(token.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0
}

fn query_t(q: Option<&str>) -> Option<String> {
    q?.split('&').find_map(|kv| kv.strip_prefix("t=")).map(|v| percent_decode(v))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) { out.push(v); i += 3; continue; }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn status(code: StatusCode) -> Response { code.into_response() }

fn font_ok(rel: &str) -> bool {
    let Some(name) = rel.strip_prefix("fonts/") else { return false };
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    (ext == "woff2" || ext == "css") && !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

async fn handle(State(ctx): State<Arc<Ctx>>, req: Request) -> Response {
    if !ok_host(req.headers().get(header::HOST)) { return status(StatusCode::FORBIDDEN); }
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(String::from);

    if req.method() == Method::POST && path == "/event" {
        if !ok_token(&ctx.token, req.headers().get("x-bobix").and_then(|v| v.to_str().ok())) { return status(StatusCode::FORBIDDEN); }
        let wait = req.headers().get("x-bobix-wait").is_some_and(|v| v == "1");
        let Ok(body) = to_bytes(req.into_body(), MAX_BODY).await else { return status(StatusCode::PAYLOAD_TOO_LARGE) };
        let e = match serde_json::from_str::<Value>(&String::from_utf8_lossy(&body)) {
            Ok(e) => e,
            Err(err) => { eprintln!("[bobix] bad event: {err}"); return status(StatusCode::NO_CONTENT); }
        };
        return match (ctx.on_event)(e, wait) {
            Some(Reply::Output(rx)) => match rx.await.ok().flatten() {
                Some(o) => ([(header::CONTENT_TYPE, "application/json")], o.to_string()).into_response(),
                None => status(StatusCode::NO_CONTENT),
            },
            Some(Reply::Answer { secs, rx, gone }) if wait => {
                let body = stream::once(async move {
                    let a = rx.await.ok().flatten();
                    drop(gone);
                    Ok::<Bytes, Infallible>(Bytes::from(a.map(|v| v.to_string()).unwrap_or_default()))
                });
                Response::builder().header(header::CONTENT_TYPE, "application/json").header("x-bobix-wait", secs)
                    .body(Body::from_stream(body)).unwrap()
            }
            _ => status(StatusCode::NO_CONTENT),
        };
    }
    // One Bobix at a time: a copy that finds the port taken knocks here (show_running) and quits.
    if req.method() == Method::POST && path == "/show" {
        if !ok_token(&ctx.token, req.headers().get("x-bobix").and_then(|v| v.to_str().ok())) { return status(StatusCode::FORBIDDEN); }
        let Ok(body) = to_bytes(req.into_body(), 4096).await else { return status(StatusCode::PAYLOAD_TOO_LARGE) };
        let v = serde_json::from_slice::<Value>(&body).ok().and_then(|v| v["version"].as_str().map(|s| s.chars().take(40).collect())).unwrap_or_default();
        (ctx.on_show)(v);
        return status(StatusCode::NO_CONTENT);
    }
    if req.method() != Method::GET { return status(StatusCode::METHOD_NOT_ALLOWED); }
    if path == "/events" {
        if !ok_token(&ctx.token, query_t(query.as_deref()).as_deref()) { return status(StatusCode::FORBIDDEN); }
        let first = (ctx.snapshot)();
        let rx = ctx.tx.subscribe();
        let rest = stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(d) => return Some((Ok::<Event, Infallible>(Event::default().data(d)), rx)),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue, // the next snapshot is whole anyway
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
        let all = futures_util::StreamExt::chain(stream::once(async move { Ok(Event::default().data(first)) }), rest);
        // Keep-alive comment so proxies / sleeping laptops don't silently drop the stream.
        return Sse::new(all).keep_alive(KeepAlive::new().interval(Duration::from_secs(25)).text("ping")).into_response();
    }
    // Pictures chosen for a project's pet: token required (they are the person's own files); the originals folder is
    // out of reach (a name is 24 hex characters and an extension, nothing else).
    if let Some(name) = path.strip_prefix("/pet-image/") {
        let ok_name = name.split_once('.').is_some_and(|(h, e)| h.len() == 24 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && (e == "png" || e == "gif"));
        if !ok_name { return status(StatusCode::NOT_FOUND); }
        let (Some(dir), true) = (ctx.pet_dir.as_ref(), ok_token(&ctx.token, query_t(query.as_deref()).as_deref())) else { return status(StatusCode::FORBIDDEN) };
        let Ok(bytes) = tokio::fs::read(dir.join(name)).await else { return status(StatusCode::NOT_FOUND) };
        let ty = if name.ends_with(".png") { "image/png" } else { "image/gif" };
        return ([(header::CONTENT_TYPE, ty), (header::CACHE_CONTROL, "private, max-age=31536000, immutable")], bytes).into_response();
    }
    let rel = path.strip_prefix("/ui/").unwrap_or("");
    let file = match path.as_str() {
        "/" => "panel.html",
        "/pets" => "pets.html",
        _ if ASSETS.contains(&rel) || font_ok(rel) => rel,
        _ => return status(StatusCode::NOT_FOUND),
    };
    let Some(bytes) = (ctx.asset)(file) else { return status(StatusCode::NOT_FOUND) };
    let ty = match file.rsplit_once('.').map(|x| x.1) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        _ => "font/woff2",
    };
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ty));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (h, bytes).into_response()
}

// Tells the Bobix that holds `port` to show its panel; true when it answered (then this copy quits).
pub fn show_running(port: u16, token: &str, version: &str) -> bool {
    use std::io::{Read, Write};
    let Ok(mut c) = std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_secs(2)) else { return false };
    let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
    let body = serde_json::json!({ "version": version }).to_string();
    let req = format!("POST /show HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nx-bobix: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let mut out = String::new();
    c.write_all(req.as_bytes()).is_ok() && c.read_to_string(&mut out).is_ok() && out.starts_with("HTTP/1.1 204")
}

// Binds now (so a busy port is known before any window opens) and serves on the async runtime.
pub fn start(port: u16, ctx: Arc<Ctx>) -> std::io::Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", port))?;
    l.set_nonblocking(true)?;
    let port = l.local_addr()?.port();
    let app = Router::new().fallback(handle).with_state(ctx);
    tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::from_std(l) {
            Ok(l) => { if let Err(e) = axum::serve(l, app).await { eprintln!("[bobix] server: {e}"); } }
            Err(e) => eprintln!("[bobix] server: {e}"),
        }
    });
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpStream, sync::Mutex};

    fn http(port: u16, req: &str) -> u16 {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        let mut b = [0u8; 12];
        s.read_exact(&mut b).unwrap();
        String::from_utf8_lossy(&b[9..12]).parse().unwrap()
    }

    #[test]
    fn header_and_host_guard_events_reach_on_event() {
        let token = "a".repeat(48);
        let got = Arc::new(Mutex::new(0));
        let g = got.clone();
        let shown = Arc::new(Mutex::new(String::new()));
        let pets = std::env::temp_dir().join(format!("bobix-pets-route-{}", std::process::id()));
        let e = crate::petstore::save_import(&pets, &serde_json::json!({}), "/p", b"\x89PNG\r\n\x1a\n original", "png", b"\x89PNG\r\n\x1a\n result", "png", &serde_json::json!({})).unwrap()["/p"].clone();
        let s = shown.clone();
        let (tx, _) = broadcast::channel(4);
        let ctx = Arc::new(Ctx {
            token: token.clone(),
            on_event: Box::new(move |_, _| { *g.lock().unwrap() += 1; None }),
            snapshot: Box::new(|| r#"{"sessions":[]}"#.into()),
            asset: Box::new(|f| (f == "pets.html").then(|| b"<p>".to_vec())),
            tx,
            on_show: Box::new(move |v| *s.lock().unwrap() = v),
            pet_dir: Some(pets.clone()),
        });
        let port = start(0, ctx).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let get = |p: &str| http(port, &format!("GET {p} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"));
        assert_eq!(get("/events"), 403, "state stream needs the token");
        assert_eq!(get(&format!("/events?t={token}")), 200);
        assert_eq!(get("/pets"), 200);
        assert_eq!(get("/ui/../main.js"), 404);
        assert_eq!(get("/ui/fonts/x.woff2"), 404, "allowed name, no such asset");
        let body = r#"{"session_id":"s1","hook_event_name":"Stop"}"#;
        let post = |h: &str, host: &str| http(port, &format!("POST /event HTTP/1.1\r\nHost: {host}\r\n{h}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()));
        let local = format!("127.0.0.1:{port}");
        assert_eq!(post("Content-Type: application/json\r\n", &local), 403, "no header");
        assert_eq!(post("x-bobix: 1\r\n", &local), 403, "wrong token");
        assert_eq!(post(&format!("x-bobix: {token}\r\n"), "evil.example:80"), 403, "foreign Host");
        assert_eq!(post(&format!("x-bobix: {token}\r\n"), &local), 204);
        assert_eq!(http(port, &format!("PUT / HTTP/1.1\r\nHost: {local}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")), 405);
        assert_eq!(*got.lock().unwrap(), 1);
        assert!(!show_running(port, "wrong", "9.9.9"), "a copy without the token can't reach the panel");
        assert!(show_running(port, &token, "9.9.9"));
        assert_eq!(*shown.lock().unwrap(), "9.9.9", "the running copy learns the other's version");
        let (file, src) = (e["file"].as_str().unwrap(), e["src"].as_str().unwrap());
        assert_eq!(get(&format!("/pet-image/{file}?t={token}")), 200);
        assert_eq!(get(&format!("/pet-image/{file}")), 403, "token required");
        assert_eq!(get(&format!("/pet-image/{src}?t={token}")), 404, "the original is not served by its name");
        assert_eq!(get(&format!("/pet-image/originals/{src}?t={token}")), 404);
        assert_eq!(get(&format!("/pet-image/originals%2F{src}?t={token}")), 404);
        let _ = std::fs::remove_dir_all(&pets);
    }

    // The real hook.js (node) against this server: a held permission request prints the answer for Claude Code,
    // one nobody answers gives up soon after its wait and closes (the app sees it go), and headless runs never wait.
    #[test]
    fn hook_js_waits_for_the_answer() {
        use std::process::{Command, Stdio};
        let home = std::env::temp_dir().join(format!("bobix-hookjs-{}", std::process::id()));
        let token = crate::paths::ensure_token_in(&home).unwrap();
        let mode = Arc::new(Mutex::new(0)); // 0: answer after 300 ms, 1: never answer, 2: record wait flags
        let (gone_seen, waits) = (Arc::new(Mutex::new(false)), Arc::new(Mutex::new(vec![])));
        let (m, g, w) = (mode.clone(), gone_seen.clone(), waits.clone());
        let (tx, _) = broadcast::channel(4);
        let ctx = Arc::new(Ctx {
            token,
            on_event: Box::new(move |_, wait| {
                w.lock().unwrap().push(wait);
                if !wait { return None; }
                let (atx, rx) = oneshot::channel();
                let (gone, gone_rx) = oneshot::channel::<()>();
                let mode = *m.lock().unwrap();
                let g = g.clone();
                std::thread::spawn(move || {
                    if mode == 0 {
                        std::thread::sleep(Duration::from_millis(300));
                        let _ = atx.send(Some(serde_json::json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": { "behavior": "allow" } } })));
                    } else {
                        let _keep = atx; // nobody answers
                        if tauri::async_runtime::block_on(gone_rx).is_err() { *g.lock().unwrap() = true; }
                    }
                });
                Some(Reply::Answer { secs: if mode == 0 { "5".into() } else { "1".into() }, rx, gone })
            }),
            snapshot: Box::new(|| "{}".into()),
            asset: Box::new(|_| None),
            tx,
            on_show: Box::new(|_| {}),
            pet_dir: None,
        });
        let port = start(0, ctx).unwrap();
        let hook = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/hook/hook.js");
        let run = |ev: &str, sdk: bool| -> (String, Duration) {
            let t0 = std::time::Instant::now();
            let mut c = Command::new("node");
            c.arg(hook).arg(port.to_string()).env("HOME", &home).env("USERPROFILE", &home).env_remove("CLAUDE_CODE_ENTRYPOINT")
                .stdin(Stdio::piped()).stdout(Stdio::piped());
            if sdk { c.env("CLAUDE_CODE_ENTRYPOINT", "sdk-cli"); }
            let mut child = c.spawn().expect("node");
            child.stdin.take().unwrap().write_all(ev.as_bytes()).unwrap();
            let out = child.wait_with_output().unwrap();
            (String::from_utf8_lossy(&out.stdout).into_owned(), t0.elapsed())
        };
        let ask = r#"{"session_id":"s3","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        let (out, took) = run(ask, false);
        let v: serde_json::Value = serde_json::from_str(&out).expect("decision printed");
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert!(took < Duration::from_secs(2), "exits as soon as the answer is in");
        *mode.lock().unwrap() = 1;
        let (out, took) = run(ask, false);
        assert_eq!(out, "", "no answer: nothing printed, Claude asks as usual");
        assert!(took < Duration::from_secs(6), "gives up soon after the wait time");
        std::thread::sleep(Duration::from_millis(300));
        assert!(*gone_seen.lock().unwrap(), "the app sees the hook go");
        waits.lock().unwrap().clear();
        let (out, _) = run(ask, true);
        assert_eq!(out, "");
        assert_eq!(*waits.lock().unwrap(), [false], "headless runs arrive without asking the app to wait");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn query_token_is_decoded() {
        assert_eq!(query_t(Some("x=1&t=ab%2Bc")).as_deref(), Some("ab+c"));
        assert_eq!(query_t(None), None);
        assert!(ok_host(Some(&HeaderValue::from_static("localhost:4747"))));
        assert!(!ok_host(Some(&HeaderValue::from_static("127.0.0.1.evil:4747"))));
        assert!(!ok_host(None));
    }
}
