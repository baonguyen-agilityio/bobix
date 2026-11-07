// The pets page is served from 127.0.0.1 (a "remote" origin to Tauri): its commands must be declared here
// and granted in capabilities/default.json, or Tauri rejects them.
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["rects", "pet_click"]),
    ))
    .expect("tauri build script failed");
}
