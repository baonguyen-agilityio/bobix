//! Runs the in-process relay stub on its own, for trying `fake-phone` and `fake-desktop` by hand:
//! `cargo run --example relay-stub` prints a `ws://` address to give to both tools.
mod common;

#[tokio::main]
async fn main() {
    println!("{}", common::stub::spawn().await);
    tokio::signal::ctrl_c().await.ok();
}
