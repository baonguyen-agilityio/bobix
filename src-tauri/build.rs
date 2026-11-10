// The pages are served from 127.0.0.1 (a "remote" origin to Tauri): their commands must be declared here, and main.rs
// grants them to that origin only. Keep in step with COMMANDS in src/main.rs.
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page", "install_hooks", "uninstall_hooks",
        "open_session", "settings_set", "settings_reset", "reveal_settings", "diagnostics_copy", "feedback_info", "feedback_send", "work_refresh", "work_open",
        "work_cancel", "work_start", "work_brief", "work_new_review", "skills_mine", "skills_audit", "skills_change", "skills_backups", "skills_undo", "skills_rules", "process_status", "process_import", "pet_choose", "pet_original", "pet_save", "pet_reset",
        "habits_read", "habits_clear", "habits_note", "show_board", "usage_prices",
        "ticket_prep", "ticket_start", "ticket_classify", "ticket_answer", "ticket_seen", "ticket_summary", "ticket_clean",
    ])))
    .expect("tauri build script failed");
}
