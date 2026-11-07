// The pages are served from 127.0.0.1 (a "remote" origin to Tauri): their commands must be declared here, and main.rs
// grants them to that origin only. Keep in step with COMMANDS in src/main.rs.
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page",
    ])))
    .expect("tauri build script failed");
}
