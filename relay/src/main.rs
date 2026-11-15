//! `relay [--dev]`. Configuration is by environment variables (see README); `--dev` serves plain WebSocket on
//! 127.0.0.1:8787 with a recording fake instead of APNs and FCM.

use bobix_relay::{config::Config, run, App};

#[tokio::main]
async fn main() {
    let dev = std::env::args().skip(1).any(|a| a == "--dev");
    let cfg = Config::from_env(dev).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let listener = tokio::net::TcpListener::bind(&cfg.addr).await.unwrap_or_else(|e| {
        eprintln!("cannot listen on {}: {e} (is another process using it?)", cfg.addr);
        std::process::exit(1)
    });
    if dev && !cfg.addr.starts_with("127.") && !cfg.addr.starts_with("localhost") && !cfg.addr.starts_with("[::1]") {
        eprintln!("WARNING: --dev on {} because RELAY_DEV_FORCE_BIND=1: nothing is kept on disk and no real push is sent", cfg.addr);
    }
    println!("relay listening on {}{}", cfg.addr, if dev { " (--dev: plain ws, recording push fake)" } else { "" });
    let app = App::from_config(cfg).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    // As PID 1 in a container nothing handles SIGTERM by default, so a redeploy would wait for the kill timeout.
    // Mailboxes live in memory only; the revoked list is saved while the hub lock is held, so taking the lock
    // before exiting means no save is half-done (a `.tmp` written but not yet renamed).
    #[cfg(unix)]
    {
        let app = app.clone();
        tokio::spawn(async move {
            if let Ok(mut t) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                t.recv().await;
                let _hub = app.hub.lock().unwrap_or_else(|e| e.into_inner());
                std::process::exit(0)
            }
        });
    }
    if let Err(e) = run(app, listener).await {
        eprintln!("relay stopped: {e}");
        std::process::exit(1)
    }
}
