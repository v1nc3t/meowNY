//! The only functions the webview can call. Each is a thin wrapper over the vault;
//! failures cross the boundary as fixed codes (see `AppError`).

use std::sync::Arc;

use tauri::State;

use crate::backup;
use crate::categories::{self, Category, CategoryType};
use crate::budgets::{self, Budget, Overview, SetBudget};
use crate::clock;
use crate::currency::{self, Conversion};
use crate::recurring::{self, Dues, NewRecurring, Recurring, RecurringPatch};
use crate::stats::{self, BudgetVsActual, CategoryTrend, Commitments, PlannedVsPaid, TopCategory, YearPoint};
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
    "list_recurring",
    "create_recurring",
    "update_recurring",
    "delete_recurring",
    "pay_recurring",
    "skip_recurring",
    "set_recurring_active",
    "end_recurring",
    "recurring_dues",
    "get_notify_due",
    "set_notify_due",
    "show_due_notification",
    "budget_vs_actual",
    "category_trend",
    "year_trend",
    "top_categories",
    "recurring_commitments",
    "recurring_planned_vs_paid",
    "create_backup",
    "restore_backup",
    "restore_backup_new_vault",
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

/// Today in the user's timezone setting; due dates and "overdue" use it, never the system date.
fn today_text(vault: &Vault) -> Result<String, AppError> {
    let timezone = vault.settings()?.timezone;
    Ok(clock::today(chrono::Utc::now(), &timezone)?.to_string())
}

#[tauri::command]
pub fn list_recurring(vault: Shared) -> Result<Vec<Recurring>, AppError> {
    vault.with_conn(|c| recurring::list(c))
}

#[tauri::command]
pub fn create_recurring(vault: Shared, input: NewRecurring) -> Result<Recurring, AppError> {
    vault.with_conn(|c| recurring::create(c, &input))
}

#[tauri::command]
pub fn update_recurring(vault: Shared, id: String, patch: RecurringPatch) -> Result<Recurring, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| recurring::update(c, id, &patch))
}

#[tauri::command]
pub fn delete_recurring(vault: Shared, id: String) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| recurring::delete(c, id))
}

/// Turns the current occurrence into a transaction. `amount` and `paid_on` default to the
/// planned amount and today.
#[tauri::command]
pub fn pay_recurring(
    vault: Shared,
    id: String,
    amount: Option<String>,
    paid_on: Option<String>,
) -> Result<Transaction, AppError> {
    let id = categories::parse_id(&id)?;
    let today = today_text(&vault)?;
    vault.with_conn(|c| recurring::pay(c, id, amount.as_deref(), paid_on.as_deref(), &today))
}

#[tauri::command]
pub fn skip_recurring(vault: Shared, id: String) -> Result<Recurring, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| recurring::skip(c, id))
}

#[tauri::command]
pub fn set_recurring_active(vault: Shared, id: String, active: bool) -> Result<Recurring, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| recurring::set_active(c, id, active))
}

#[tauri::command]
pub fn end_recurring(vault: Shared, id: String) -> Result<Recurring, AppError> {
    let id = categories::parse_id(&id)?;
    let today = today_text(&vault)?;
    vault.with_conn(|c| recurring::end(c, id, &today))
}

#[tauri::command]
pub fn recurring_dues(vault: Shared, within_days: Option<u32>) -> Result<Dues, AppError> {
    let today = today_text(&vault)?;
    vault.with_conn(|c| recurring::dues(c, &today, within_days))
}

#[tauri::command]
pub fn get_notify_due(vault: Shared) -> Result<bool, AppError> {
    vault.notify_due()
}

#[tauri::command]
pub fn set_notify_due(vault: Shared, on: bool) -> Result<(), AppError> {
    vault.set_notify_due(on)
}

/// Shows a desktop notification, only if the person has them on. The text comes from the UI so
/// it is in the chosen language; it is a local notification and nothing leaves the machine.
#[tauri::command]
pub fn show_due_notification(app: tauri::AppHandle, vault: Shared, title: String, body: String) -> Result<(), AppError> {
    use tauri_plugin_notification::NotificationExt;
    if title.chars().count() > 100 || body.chars().count() > 300 {
        return Err(AppError::InvalidInput);
    }
    if vault.status() != VaultStatus::Unlocked || !vault.notify_due()? {
        return Ok(());
    }
    app.notification().builder().title(title).body(body).show().map_err(|_| AppError::Internal)
}

/// Every category's budget against what was spent or earned in one month ("YYYY-MM"), plus totals.
#[tauri::command]
pub fn budget_vs_actual(vault: Shared, month: String) -> Result<BudgetVsActual, AppError> {
    vault.with_conn(|c| stats::budget_vs_actual(c, &month))
}

/// One category month by month, `from` to `to` inclusive ("YYYY-MM").
#[tauri::command]
pub fn category_trend(vault: Shared, category_id: String, from: String, to: String) -> Result<CategoryTrend, AppError> {
    let id = categories::parse_id(&category_id)?;
    vault.with_conn(|c| stats::category_trend(c, id, &from, &to))
}

/// Income, expense and savings for each month of a year ("YYYY").
#[tauri::command]
pub fn year_trend(vault: Shared, year: String) -> Result<Vec<YearPoint>, AppError> {
    vault.with_conn(|c| stats::year_trend(c, &year))
}

/// The categories with the most spending or income in a month, biggest first.
#[tauri::command]
pub fn top_categories(vault: Shared, month: String, kind: CategoryType, limit: u32) -> Result<Vec<TopCategory>, AppError> {
    vault.with_conn(|c| stats::top_categories(c, &month, kind, limit))
}

/// What the active recurring items cost, or bring in, in an average month.
#[tauri::command]
pub fn recurring_commitments(vault: Shared) -> Result<Commitments, AppError> {
    vault.with_conn(|c| stats::commitments(c))
}

/// Planned against paid amounts of the recurring items paid in a month.
#[tauri::command]
pub fn recurring_planned_vs_paid(vault: Shared, month: String) -> Result<Vec<PlannedVsPaid>, AppError> {
    vault.with_conn(|c| stats::planned_vs_paid(c, &month))
}

/// Asks the person to pick a backup file. None if they cancel.
fn pick_backup_file(app: &tauri::AppHandle) -> Result<Option<std::path::PathBuf>, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("meowNY backup", &[backup::EXTENSION])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    Ok(Some(path.into_path().map_err(|_| AppError::Internal)?))
}

/// Writes all data to one encrypted file in a place the person picks, locked with the passphrase
/// they chose for this backup. Returns false if they cancel the save dialog.
#[tauri::command]
pub async fn create_backup(app: tauri::AppHandle, vault: Shared<'_>, passphrase: String) -> Result<bool, AppError> {
    use tauri_plugin_dialog::DialogExt;
    let snapshot = vault.with_conn(|c| backup::dump(c))?;
    let bytes = backup::seal(&snapshot, &passphrase)?;
    let name = format!("meowny-backup-{}.{}", today_text(&vault)?, backup::EXTENSION);
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name(name)
        .add_filter("meowNY backup", &[backup::EXTENSION])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|_| AppError::Internal)?;
    backup::write_file(&path, &bytes)?;
    Ok(true)
}

/// Replaces all data with a backup the person picks. `confirmation` must be "RESTORE". Returns false
/// if they cancel the file dialog. Nothing changes unless the whole backup is valid.
#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    vault: Shared<'_>,
    passphrase: String,
    confirmation: String,
) -> Result<bool, AppError> {
    if confirmation != "RESTORE" {
        return Err(AppError::InvalidInput);
    }
    if vault.status() != VaultStatus::Unlocked {
        return Err(AppError::Locked);
    }
    let Some(path) = pick_backup_file(&app)? else { return Ok(false) };
    vault.restore_backup(&backup::read_file(&path)?, &passphrase, &confirmation)?;
    Ok(true)
}

/// First run from a backup: new keys and a new data file, filled from the backup. `vault_passphrase`
/// is the optional app passphrase for this install. Returns the recovery key, or None if the person
/// cancels the file dialog.
#[tauri::command]
pub async fn restore_backup_new_vault(
    app: tauri::AppHandle,
    vault: Shared<'_>,
    passphrase: String,
    vault_passphrase: Option<String>,
) -> Result<Option<String>, AppError> {
    if vault.status() != VaultStatus::NotSetUp {
        return Err(AppError::AlreadySetUp);
    }
    let Some(path) = pick_backup_file(&app)? else { return Ok(None) };
    let key = vault.create_from_backup(&backup::read_file(&path)?, &passphrase, vault_passphrase.as_deref())?;
    Ok(Some(key))
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
