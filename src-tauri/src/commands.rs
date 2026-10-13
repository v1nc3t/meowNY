//! The only functions the webview can call. Each is a thin wrapper over the vault;
//! failures cross the boundary as fixed codes (see `AppError`).

use std::sync::Arc;

use tauri::State;

use crate::vault::{AppError, LockPrefs, Settings, Vault, VaultStatus};

type Shared<'a> = State<'a, Arc<Vault>>;

/// Names of every command, used by the build script to generate one permission each.
pub const COMMANDS: &[&str] = &[
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
];

#[tauri::command]
pub fn vault_status(vault: Shared) -> VaultStatus {
    vault.status()
}

#[tauri::command]
pub fn create_vault(vault: Shared, settings: Settings, passphrase: Option<String>) -> Result<(), AppError> {
    vault.create(&settings, passphrase.as_deref())
}

#[tauri::command]
pub fn unlock_vault(vault: Shared, passphrase: Option<String>) -> Result<(), AppError> {
    vault.unlock(passphrase.as_deref())
}

#[tauri::command]
pub fn lock_vault(vault: Shared) {
    vault.lock();
}

#[tauri::command]
pub fn touch_activity(vault: Shared) {
    vault.touch();
}

#[tauri::command]
pub fn get_settings(vault: Shared) -> Result<Settings, AppError> {
    vault.settings()
}

#[tauri::command]
pub fn save_settings(vault: Shared, settings: Settings) -> Result<(), AppError> {
    vault.save_settings(&settings)
}

#[tauri::command]
pub fn get_lock_prefs(vault: Shared) -> Result<LockPrefs, AppError> {
    vault.lock_prefs()
}

#[tauri::command]
pub fn save_lock_prefs(vault: Shared, prefs: LockPrefs) -> Result<(), AppError> {
    vault.save_lock_prefs(&prefs)
}

#[tauri::command]
pub fn data_location(vault: Shared) -> String {
    vault.data_dir().display().to_string()
}

#[tauri::command]
pub fn wipe_all_data(vault: Shared, confirmation: String) -> Result<(), AppError> {
    vault.wipe(&confirmation)
}

#[cfg(test)]
mod tests {
    use super::COMMANDS;

    #[test]
    fn build_script_lists_every_command() {
        let build = include_str!("../build.rs");
        for name in COMMANDS {
            assert!(build.contains(&format!("\"{name}\"")), "build.rs is missing {name}");
        }
    }

    #[test]
    fn capability_grants_every_command() {
        let capability = include_str!("../capabilities/default.json");
        for name in COMMANDS {
            let permission = format!("allow-{}", name.replace('_', "-"));
            assert!(capability.contains(&permission), "capability is missing {permission}");
        }
    }

    #[test]
    fn every_command_function_is_registered() {
        let source = include_str!("commands.rs");
        let defined = source.lines().filter(|l| l.trim() == "#[tauri::command]").count();
        assert_eq!(defined, COMMANDS.len(), "a command is defined but not listed");
    }
}
