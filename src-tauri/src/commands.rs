//! The only functions the webview can call. Each is a thin wrapper over the vault;
//! failures cross the boundary as fixed codes (see `AppError`).

use std::sync::Arc;

use tauri::State;

use crate::categories::{self, Category, CategoryType};
use crate::budgets::{self, Budget, Overview, SetBudget};
use crate::clock;
use crate::currency::{self, Conversion};
use crate::transactions::{self, Filter, NewTransaction, Page, Transaction, TransactionPatch};
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

#[tauri::command]
pub fn vault_status(vault: Shared) -> VaultStatus {
    vault.status()
}

#[tauri::command]
pub fn create_vault(vault: Shared, settings: Settings, passphrase: Option<String>) -> Result<String, AppError> {
    vault.create(&settings, passphrase.as_deref())
}

/// "Forgot my passphrase": unlock with the recovery key and choose a new passphrase (or none).
#[tauri::command]
pub fn recover_vault(vault: Shared, recovery_key: String, new_passphrase: Option<String>) -> Result<(), AppError> {
    vault.recover(&recovery_key, new_passphrase.as_deref())
}

/// Saves the recovery key as a text file the person picks a place for. Returns false if they cancel.
/// Only a valid recovery key can be written, so this cannot be used to write other content to disk.
#[tauri::command]
pub async fn save_recovery_key(app: tauri::AppHandle, code: String) -> Result<bool, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let text = crate::recovery_file::text(&code)?;
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name("meowny-recovery-key.txt")
        .add_filter("Text file", &["txt"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|_| AppError::Internal)?;
    crate::recovery_file::write(&path, &text)?;
    Ok(true)
}

/// Makes a new recovery key and invalidates the old one. Asks for the passphrase again.
#[tauri::command]
pub fn new_recovery_key(vault: Shared, passphrase: Option<String>) -> Result<String, AppError> {
    vault.new_recovery_key(passphrase.as_deref())
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

#[tauri::command]
pub fn list_categories(vault: Shared, kind: Option<CategoryType>) -> Result<Vec<Category>, AppError> {
    vault.with_conn(|c| categories::list(c, kind))
}

#[tauri::command]
pub fn create_category(vault: Shared, name: String, kind: CategoryType) -> Result<Category, AppError> {
    vault.with_conn(|c| categories::create(c, &name, kind))
}

#[tauri::command]
pub fn rename_category(vault: Shared, id: String, name: String) -> Result<Category, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| categories::rename(c, id, &name))
}

#[tauri::command]
pub fn delete_category(vault: Shared, id: String) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| categories::delete(c, id, chrono::Utc::now()))
}

#[tauri::command]
pub fn list_transactions(vault: Shared, filter: Filter) -> Result<Page, AppError> {
    vault.with_conn(|c| transactions::list(c, &filter))
}

#[tauri::command]
pub fn create_transaction(vault: Shared, input: NewTransaction) -> Result<Transaction, AppError> {
    vault.with_conn(|c| transactions::create(c, &input))
}

#[tauri::command]
pub fn update_transaction(vault: Shared, id: String, patch: TransactionPatch) -> Result<Transaction, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| transactions::update(c, id, &patch))
}

#[tauri::command]
pub fn delete_transaction(vault: Shared, id: String) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| transactions::delete(c, id))
}

#[tauri::command]
pub fn set_budget(vault: Shared, input: SetBudget) -> Result<Budget, AppError> {
    vault.with_conn(|c| budgets::set(c, &input, chrono::Utc::now()))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeCurrency {
    pub currency: String,
    /// How many units of the new currency one unit of the current one is worth. Null only relabels.
    pub rate: Option<String>,
}

/// Changes the currency, converting every amount at the rate the person typed, or only relabelling.
#[tauri::command]
pub fn change_currency(vault: Shared, input: ChangeCurrency) -> Result<Conversion, AppError> {
    let timezone = vault.settings()?.timezone;
    let today = clock::today(chrono::Utc::now(), &timezone)?.to_string();
    vault.with_conn(|c| currency::change(c, &input.currency, input.rate.as_deref(), &today))
}

#[tauri::command]
pub fn budget_history(vault: Shared) -> Result<Vec<budgets::MonthHistory>, AppError> {
    vault.with_conn(|c| budgets::history(c, chrono::Utc::now()))
}

#[tauri::command]
pub fn budget_overview(vault: Shared, month: String) -> Result<Vec<Overview>, AppError> {
    vault.with_conn(|c| budgets::overview(c, &month))
}

/// Today and this month in the user's timezone setting, so the UI never guesses from the browser clock.
#[tauri::command]
pub fn today(vault: Shared) -> Result<Today, AppError> {
    let timezone = vault.settings()?.timezone;
    let date = clock::today(chrono::Utc::now(), &timezone)?;
    Ok(Today { date: date.to_string(), month: date.format("%Y-%m").to_string() })
}

#[derive(serde::Serialize)]
pub struct Today {
    pub date: String,
    pub month: String,
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
