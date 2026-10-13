// Every webview command is listed here so Tauri generates one permission per command
// (`allow-<name>`). A command missing from this list cannot be called from the webview.
// `src/commands.rs` has a test that keeps this list and its own in sync.
const COMMANDS: &[&str] = &[
    "vault_status",
    "create_vault",
    "unlock_vault",
    "lock_vault",
    "touch_activity",
    "get_settings",
    "save_settings",
    "get_lock_prefs",
    "save_lock_prefs",
    "data_location",
    "wipe_all_data",
    "list_categories",
    "create_category",
    "rename_category",
    "delete_category",
    "list_transactions",
    "create_transaction",
    "update_transaction",
    "delete_transaction",
    "today",
    "set_budget",
    "budget_overview",
    "recover_vault",
    "new_recovery_key",
    "save_recovery_key",
    "budget_history",
    "change_currency",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri build script");
}
