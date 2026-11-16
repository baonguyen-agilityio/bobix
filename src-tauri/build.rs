// The pages are served from 127.0.0.1 (a "remote" origin to Tauri): their commands must be declared here, and main.rs
// grants them to that origin only. Keep in step with COMMANDS in src/main.rs.
const COMMANDS: &[&str] = &[
    "state", "rects", "pet_click", "pet_set", "answer", "test_event", "page_error", "smoke_page", "install_hooks", "uninstall_hooks",
    "open_session", "settings_set", "settings_reset", "reveal_settings", "diagnostics_copy", "feedback_info", "feedback_send", "work_refresh", "work_open",
    "work_cancel", "work_start", "work_brief", "work_link", "work_find", "work_clone", "work_repos", "work_page", "work_new_review", "skills_mine", "skills_audit", "skills_change", "skills_backups", "skills_undo", "skills_rules", "habit_skills", "room", "process_status", "process_import", "pet_choose", "pet_original", "pet_save", "pet_reset",
    "habits_read", "habits_clear", "habits_note", "show_board", "usage_prices",
    "ticket_prep", "ticket_start", "ticket_classify", "ticket_answer", "ticket_seen", "ticket_summary", "ticket_clean", "ticket_review", "ticket_mr", "claude_tests", "ticket_ci", "ticket_detail", "ticket_remove", "usage_effects", "update_act",
    "question_answer",
];
// Declared only when the `phones` feature is on (Cargo sets CARGO_FEATURE_PHONES for the build script).
const PHONE_COMMANDS: &[&str] = &["phones_state", "phones_enable", "phones_relay", "phones_pair_start", "phones_pair_cancel", "phones_pair_confirm", "phones_rights", "phones_disconnect", "phones_new_identity", "phones_log", "phones_invite_create", "phones_invite_redeem", "phones_invite_remove"];

fn main() {
    let mut commands = COMMANDS.to_vec();
    if std::env::var_os("CARGO_FEATURE_PHONES").is_some() { commands.extend_from_slice(PHONE_COMMANDS); }
    let commands: &'static [&'static str] = Box::leak(commands.into_boxed_slice()); // the manifest wants a static list
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(commands)))
        .expect("tauri build script failed");
}
