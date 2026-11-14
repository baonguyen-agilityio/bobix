//! A pretend desktop for trying a phone (or `fake-phone`) without the real app.
//!
//!   fake-desktop --relay ws://127.0.0.1:8787 [--scenario normal]
//!
//! It prints a QR string, accepts a pairing, and plays the scenario. Scenarios: see `--scenario list`.
//! It speaks plain `ws://` only: use it with `relay --dev` or the stub (`cargo run --example relay-stub`).
mod common;

use common::{desktop::Desktop, scenario::{Scenario, ALL}};
use std::time::Duration;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = arg(&args, "--scenario").unwrap_or_else(|| "normal".into());
    if name == "list" {
        println!("{}", ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join("\n"));
        return;
    }
    let scenario = Scenario::parse(&name).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let relay = arg(&args, "--relay").unwrap_or_else(|| "ws://127.0.0.1:8787".into());
    let mut d = Desktop::start(&relay, scenario).await.unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    d.print = true;
    println!("scenario: {name}\ncomputer: {}", d.device_id());
    println!("QR string (valid for 2 minutes):\n{}", d.qr());
    d.serve(Duration::from_secs(600)).await;
}
