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
    println!("relay listening on {}{}", cfg.addr, if dev { " (--dev: plain ws, recording push fake)" } else { "" });
    let app = App::from_config(cfg).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    if let Err(e) = run(app, listener).await {
        eprintln!("relay stopped: {e}");
        std::process::exit(1)
    }
}
