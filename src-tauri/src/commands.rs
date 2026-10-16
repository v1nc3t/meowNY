//! The only functions the webview can call. Each is a thin wrapper over the vault;
//! failures cross the boundary as fixed codes (see `AppError`).

use std::sync::Arc;

use tauri::State;

use crate::api::{Host, HostStatus};
use crate::backup;
use crate::categories::{self, Category, CategoryType};
use crate::budgets::{self, Budget, Overview, SetBudget};
use crate::clock;
use crate::currency::{self, Conversion};
use crate::display::DisplayFormat;
use crate::documents::library as docs;
use crate::documents::names as doc_names;
use crate::export::{self, Selection};
use crate::report::{self, ReportOptions};
use crate::recurring::{self, Dues, NewRecurring, Recurring, RecurringPatch};
use crate::schedule::{self, Schedule};
use crate::tokens::{self, CreatedToken, NewToken, TokenInfo};
use crate::stats::{self, BudgetVsActual, CategoryTrend, Commitments, PlannedVsPaid, TopCategory, YearPoint};
use crate::transactions::{self, Filter, NewTransaction, Page, Transaction, TransactionPatch};
use crate::sync::engine::SyncError;
use crate::sync::peers::Peer;
use crate::sync::service::{Status as SyncStatus, SyncService};
use crate::vault::{AppError, Attempts, LockPrefs, Policy, Settings, Vault, VaultStatus};

type Shared<'a> = State<'a, Arc<Vault>>;
type Syncing<'a> = State<'a, Arc<SyncService<Vault>>>;

/// Asking the person for a file or a folder. Only the desktop does it: the phone has no backup, export or recovery key file
/// to save or load, so its build has neither these dialogs nor the dialog plugin (see `Cargo.toml`), and asking is refused.
#[cfg(desktop)]
mod ask {
    use std::path::PathBuf;

    use tauri_plugin_dialog::DialogExt;

    use crate::vault::AppError;

    pub fn save(app: &tauri::AppHandle, name: &str, kind: &str, extension: &str) -> Result<Option<PathBuf>, AppError> {
        let picked = app.dialog().file().set_file_name(name).add_filter(kind, &[extension]).blocking_save_file();
        picked.map(|path| path.into_path().map_err(|_| AppError::Internal)).transpose()
    }

    /// A place to save a file of any kind, as a copy of a document: no filter, since its type is whatever the person's file was.
    pub fn save_any(app: &tauri::AppHandle, name: &str) -> Result<Option<PathBuf>, AppError> {
        let picked = app.dialog().file().set_file_name(name).blocking_save_file();
        picked.map(|path| path.into_path().map_err(|_| AppError::Internal)).transpose()
    }

    pub fn pick(app: &tauri::AppHandle, kind: &str, extension: &str) -> Result<Option<PathBuf>, AppError> {
        let picked = app.dialog().file().add_filter(kind, &[extension]).blocking_pick_file();
        picked.map(|path| path.into_path().map_err(|_| AppError::Internal)).transpose()
    }

    pub fn folder(app: &tauri::AppHandle) -> Result<Option<PathBuf>, AppError> {
        let picked = app.dialog().file().blocking_pick_folder();
        picked.map(|path| path.into_path().map_err(|_| AppError::Internal)).transpose()
    }
}

#[cfg(mobile)]
mod ask {
    use std::path::PathBuf;

    use crate::vault::AppError;

    pub fn save(_: &tauri::AppHandle, _: &str, _: &str, _: &str) -> Result<Option<PathBuf>, AppError> {
        Err(AppError::InvalidInput)
    }

    pub fn save_any(_: &tauri::AppHandle, _: &str) -> Result<Option<PathBuf>, AppError> {
        Err(AppError::InvalidInput)
    }

    pub fn pick(_: &tauri::AppHandle, _: &str, _: &str) -> Result<Option<PathBuf>, AppError> {
        Err(AppError::InvalidInput)
    }

    pub fn folder(_: &tauri::AppHandle) -> Result<Option<PathBuf>, AppError> {
        Err(AppError::InvalidInput)
    }
}

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
    "get_backup_schedule",
    "set_backup_schedule",
    "clear_backup_schedule",
    "export_json",
    "export_csv",
    "export_xlsx",
    "export_pdf",
    "export_preview",
    "get_api_status",
    "set_api_enabled",
    "list_api_tokens",
    "create_api_token",
    "revoke_api_token",
    "app_profile",
    "unlock_attempts",
    "sync_status",
    "sync_start_pairing",
    "sync_enter_code",
    "sync_decide",
    "sync_start",
    "sync_scan",
    "sync_cancel",
    "sync_acknowledge",
    "sync_devices",
    "sync_remove_device",
    "list_documents",
    "get_document",
    "update_document",
    "delete_document",
    "link_document",
    "unlink_document",
    "document_links",
    "document_library_size",
    "document_counts",
    "check_documents",
    "read_document_image",
    "open_document",
    "save_document_copy",
    "document_upload_begin",
    "document_upload_chunk",
    "document_upload_finish",
    "document_upload_cancel",
    "document_add_dropped",
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
    let text = crate::recovery_file::text(&code)?;
    let Some(path) = ask::save(&app, "meowny-recovery-key.txt", "Text file", "txt")? else { return Ok(false) };
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
pub fn lock_vault(vault: Shared, syncing: Syncing) {
    vault.lock();
    syncing.cancel();
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
pub fn wipe_all_data(vault: Shared, api: State<Arc<Host>>, confirmation: String) -> Result<(), AppError> {
    vault.wipe(&confirmation)?;
    // The data is gone, so the command API has nothing left to serve.
    api.stop();
    Ok(())
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
    // The currency is changed on the desktop and reaches the phone with a sync; a phone that changed it too would never agree.
    if is_phone(&vault) {
        return Err(AppError::InvalidInput);
    }
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
    ask::pick(app, "meowNY backup", backup::EXTENSION)
}

/// Writes all data to one encrypted file in a place the person picks, locked with the passphrase
/// they chose for this backup, and the files of the documents too if `include_documents`. What it held of the documents, or None
/// if they cancel the save dialog. The file is written as it is read, so a big library does not have to fit in memory.
#[tauri::command]
pub async fn create_backup(app: tauri::AppHandle, vault: Shared<'_>, passphrase: String, include_documents: bool) -> Result<Option<backup::Written>, AppError> {
    backup::check_passphrase(&passphrase)?;
    let name = format!("meowny-backup-{}.{}", today_text(&vault)?, backup::EXTENSION);
    let Some(path) = ask::save(&app, &name, "meowNY backup", backup::EXTENSION)? else { return Ok(None) };
    // the rows are read after the person has chosen the place, so the backup is of this moment and not of the one before the dialog
    let snapshot = vault.with_conn(|c| backup::dump(c))?;
    let mut files = backup::VaultFiles(&vault);
    let written = backup::create_file(&path, &snapshot, &passphrase, include_documents.then_some(&mut files as &mut dyn backup::FileSource))?;
    Ok(Some(written))
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
    vault.restore_backup(path.as_path(), &passphrase, &confirmation)?;
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
    let key = vault.create_from_backup(path.as_path(), &passphrase, vault_passphrase.as_deref())?;
    Ok(Some(key))
}

/// The scheduled backup setup, without its passphrase. None when there is none.
#[tauri::command]
pub fn get_backup_schedule(vault: Shared) -> Result<Option<Schedule>, AppError> {
    vault.with_conn(|c| schedule::get(c))
}

/// Starts (or replaces) scheduled backups: asks for the folder, then makes the first backup there.
/// `every_days` is 1, 7 or 30; `include_documents` puts the files of the documents in every backup. Returns false if the person
/// cancels the folder dialog.
#[tauri::command]
pub async fn set_backup_schedule(
    app: tauri::AppHandle,
    vault: Shared<'_>,
    passphrase: String,
    every_days: u32,
    include_documents: bool,
) -> Result<bool, AppError> {
    backup::check_passphrase(&passphrase)?;
    // Scheduled backups are a desktop feature: a phone cannot pick a folder for the app to write into, and its data is on the
    // desktop too (`ask::folder` refuses there).
    let Some(folder) = ask::folder(&app)? else { return Ok(false) };
    schedule::start(&vault, &folder, &passphrase, every_days, include_documents, chrono::Utc::now())?;
    Ok(true)
}

#[tauri::command]
pub fn clear_backup_schedule(vault: Shared) -> Result<(), AppError> {
    vault.with_conn(|c| schedule::stop(c))
}

/// Asks where to save `text`, then writes it there readable by the owner only. False if the person cancels.
/// These files are not encrypted; the export screen says so.
fn save_export(app: &tauri::AppHandle, name: String, kind: &str, extension: &str, bytes: &[u8]) -> Result<bool, AppError> {
    let Some(path) = ask::save(app, &name, kind, extension)? else { return Ok(false) };
    backup::write_file(&path, bytes)?;
    Ok(true)
}

/// What an export of this selection would contain: counts, the span of dates and the totals.
#[tauri::command]
pub fn export_preview(vault: Shared, selection: Selection) -> Result<export::Preview, AppError> {
    vault.with_conn(|c| export::preview(c, &selection))
}

/// The data as a JSON file, or part of it: see `Selection`. Not encrypted.
#[tauri::command]
pub async fn export_json(app: tauri::AppHandle, vault: Shared<'_>, selection: Selection) -> Result<bool, AppError> {
    let text = vault.with_conn(|c| export::json(c, &selection, chrono::Utc::now()))?;
    save_export(&app, format!("meowny-export-{}.json", today_text(&vault)?), "JSON", "json", text.as_bytes())
}

/// Transactions as a CSV file. Cells that start like a formula are written as text. Not encrypted.
#[tauri::command]
pub async fn export_csv(app: tauri::AppHandle, vault: Shared<'_>, selection: Selection) -> Result<bool, AppError> {
    let text = vault.with_conn(|c| export::csv(c, &selection))?;
    save_export(&app, format!("meowny-transactions-{}.csv", today_text(&vault)?), "CSV", "csv", text.as_bytes())
}

/// An Excel workbook with sheets for transactions, categories, budgets, recurring items and a monthly
/// summary. `format` sets how dates and money look; leave it out for ISO dates and a plain number.
/// Text cells that start like a formula are written as text. Not encrypted.
#[tauri::command]
pub async fn export_xlsx(
    app: tauri::AppHandle,
    vault: Shared<'_>,
    selection: Selection,
    format: Option<DisplayFormat>,
) -> Result<bool, AppError> {
    let bytes = vault.with_conn(|c| export::workbook(c, &selection, &format.unwrap_or_default()))?;
    save_export(&app, format!("meowny-{}.xlsx", today_text(&vault)?), "Excel workbook", "xlsx", &bytes)
}

/// A PDF report for a month or a range of months: totals, charts, budget against actual and, if asked,
/// the transactions. Made on this computer. Not encrypted, so `showDescriptions` off is the way to share
/// one without private details.
#[tauri::command]
pub async fn export_pdf(app: tauri::AppHandle, vault: Shared<'_>, options: ReportOptions) -> Result<bool, AppError> {
    let report = vault.with_conn(|c| report::read(c, &options))?;
    // Laying out and writing the file can take a moment for a long list; the database is free meanwhile.
    let bytes = report.pdf(&options)?;
    let name = format!("meowny-report-{}-{}.pdf", options.from_month, options.to_month);
    save_export(&app, name, "PDF report", "pdf", &bytes)
}

/// Whether the command API is on and listening, and on which port.
#[tauri::command]
pub fn get_api_status(api: State<Arc<Host>>) -> HostStatus {
    api.status()
}

/// Turns the command API on or off. Needs the unlocked app. Returns the state after the change.
#[tauri::command]
pub fn set_api_enabled(api: State<Arc<Host>>, on: bool) -> Result<HostStatus, AppError> {
    api.set_enabled(on)?;
    Ok(api.status())
}

/// Every access token, newest first, without its secret.
#[tauri::command]
pub fn list_api_tokens(vault: Shared) -> Result<Vec<TokenInfo>, AppError> {
    vault.with_conn(|c| tokens::list(c, chrono::Utc::now()))
}

/// Makes a token. The answer holds the secret, the only time it can be read. Tokens are made here,
/// in the app's window, and never through the command API, so a token cannot make another token.
#[tauri::command]
pub fn create_api_token(vault: Shared, input: NewToken) -> Result<CreatedToken, AppError> {
    vault.with_conn(|c| tokens::create(c, &input, chrono::Utc::now()))
}

#[tauri::command]
pub fn revoke_api_token(vault: Shared, id: String) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_conn(|c| tokens::revoke(c, id))
}

// --- the phone and the sync (phase 8) ---------------------------------------------------------------------------------

/// Which app this is: the desktop's, or the phone's. The screens differ, and so does the vault (the phone's needs a passphrase
/// and erases itself after 5 wrong ones).
#[tauri::command]
pub fn app_profile(vault: Shared) -> &'static str {
    if vault.policy() == Policy::PHONE {
        "phone"
    } else {
        "desktop"
    }
}

/// How many wrong passphrases have been typed and how many are left before the erase; none on a device that never erases.
#[tauri::command]
pub fn unlock_attempts(vault: Shared) -> Result<Option<Attempts>, AppError> {
    vault.attempts()
}

fn sync_error(e: SyncError) -> AppError {
    match e {
        SyncError::Locked => AppError::Locked,
        SyncError::Invalid(_) => AppError::InvalidInput,
        _ => AppError::Internal,
    }
}

fn is_phone(vault: &Vault) -> bool {
    vault.policy() == Policy::PHONE
}

#[tauri::command]
pub fn sync_status(syncing: Syncing) -> SyncStatus {
    syncing.status()
}

/// The address a person typed to use instead of the one the system picks, for a computer on several networks. Empty means the
/// system's choice. Only an address of a home network is taken, and not the computer's own loopback address: a phone cannot
/// reach that.
fn typed_address(text: Option<&str>) -> Result<Option<std::net::IpAddr>, AppError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    let ip: std::net::IpAddr = text.parse().map_err(|_| AppError::InvalidInput)?;
    if ip.is_loopback() || !crate::sync::qr::is_local(&ip) {
        return Err(AppError::InvalidInput);
    }
    Ok(Some(ip))
}

/// Desktop: opens a window for a new phone and shows its code (read it with `sync_status`). `address` is optional.
#[tauri::command]
pub fn sync_start_pairing(vault: Shared, syncing: Syncing, name: String, address: Option<String>) -> Result<(), AppError> {
    if is_phone(&vault) {
        return Err(AppError::InvalidInput);
    }
    let address = typed_address(address.as_deref())?;
    syncing.start_pairing(&name, address).map_err(sync_error)
}

/// Phone: the person typed the six digits that the desktop shows. Checked here; three wrong tries end the pairing.
#[tauri::command]
pub fn sync_enter_code(vault: Shared, syncing: Syncing, code: String) -> Result<(), AppError> {
    if !is_phone(&vault) {
        return Err(AppError::InvalidInput);
    }
    syncing.enter_code(&code);
    Ok(())
}

/// The person chose, for each conflict in the order it was shown, which version is true (`mine` or `theirs`).
#[tauri::command]
pub fn sync_decide(syncing: Syncing, choices: Vec<crate::sync::engine::Choice>) -> Result<(), AppError> {
    syncing.decide(choices).map_err(sync_error)
}

/// Desktop: opens a window for a paired phone and shows its code. `address` is optional. `mode` says which way the sync goes:
/// both ways, the phone's changes to the desktop only, or the desktop's changes to the phone only.
#[tauri::command]
pub fn sync_start(vault: Shared, syncing: Syncing, name: String, address: Option<String>, mode: Option<crate::sync::wire::Mode>) -> Result<(), AppError> {
    if is_phone(&vault) {
        return Err(AppError::InvalidInput);
    }
    let address = typed_address(address.as_deref())?;
    syncing.start_sync(&name, address, mode.unwrap_or_default()).map_err(sync_error)
}

/// Phone: the text of a scanned code. A pairing code pairs, a sync code syncs.
#[tauri::command]
pub fn sync_scan(vault: Shared, syncing: Syncing, text: String, name: String) -> Result<(), AppError> {
    if !is_phone(&vault) {
        return Err(AppError::InvalidInput);
    }
    syncing.scan(&text, &name).map_err(sync_error)
}

#[tauri::command]
pub fn sync_cancel(syncing: Syncing) {
    syncing.cancel();
}

#[tauri::command]
pub fn sync_acknowledge(syncing: Syncing) {
    syncing.acknowledge();
}

#[tauri::command]
pub fn sync_devices(syncing: Syncing) -> Result<Vec<Peer>, AppError> {
    syncing.devices().map_err(sync_error)
}

/// Forgets a paired device: its secret is deleted and it can never sync again.
#[tauri::command]
pub fn sync_remove_device(syncing: Syncing, device_id: String) -> Result<bool, AppError> {
    syncing.remove_device(&device_id).map_err(sync_error)
}

// -- documents ------------------------------------------------------------------------------------------------------------

#[tauri::command]
pub fn list_documents(vault: Shared, filter: docs::Filter) -> Result<docs::Page, AppError> {
    vault.with_files(|conn, files| docs::list(conn, files, &filter))
}

#[tauri::command]
pub fn get_document(vault: Shared, id: String) -> Result<docs::Document, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, files| docs::get(conn, files, id))
}

#[tauri::command]
pub fn update_document(vault: Shared, id: String, patch: docs::DocumentPatch) -> Result<docs::Document, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, files| docs::update(conn, files, id, &patch))
}

/// Deletes the document, its links and its file. The transactions it was linked to stay.
#[tauri::command]
pub fn delete_document(vault: Shared, id: String) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, files| docs::delete(conn, files, id))
}

#[tauri::command]
pub fn link_document(vault: Shared, id: String, target: docs::LinkTarget) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, _| docs::link(conn, id, &target))
}

#[tauri::command]
pub fn unlink_document(vault: Shared, id: String, target: docs::LinkTarget) -> Result<(), AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, _| docs::unlink(conn, id, &target))
}

/// What a document is linked to.
#[tauri::command]
pub fn document_links(vault: Shared, id: String) -> Result<Vec<docs::LinkView>, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, _| docs::links_of(conn, id))
}

/// How many documents there are and how much they hold, and how much of it is on this device.
#[tauri::command]
pub fn document_library_size(vault: Shared) -> Result<docs::Size, AppError> {
    vault.with_files(docs::library_size)
}

/// How many documents are linked to each of these transactions (or recurring items), for the mark on a list.
#[tauri::command]
pub fn document_counts(vault: Shared, kind: docs::TargetKind, ids: Vec<String>) -> Result<std::collections::BTreeMap<String, u32>, AppError> {
    vault.with_files(|conn, _| docs::link_counts(conn, kind, &ids))
}

/// Compares the folder of the files with the documents (removing what no document names) and says how many have no file here.
#[tauri::command]
pub fn check_documents(vault: Shared) -> Result<docs::Report, AppError> {
    vault.check_documents()
}

/// An image for the page to draw, if the app may draw it (see `documents::library::read_image`). Anything else is invalid input,
/// and the page offers the system's viewer.
#[tauri::command]
pub fn read_document_image(vault: Shared, id: String) -> Result<docs::ImageData, AppError> {
    let id = categories::parse_id(&id)?;
    vault.with_files(|conn, files| docs::read_image(conn, files, id))
}

/// Opens a document in the system's own viewer, from a plain copy that is removed when the app locks or closes. Not for a kind of
/// file the app could not identify. The copy is made inside the call that holds the vault, so a lock that comes next removes it.
#[tauri::command]
pub fn open_document(app: tauri::AppHandle, vault: Shared, id: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let id = categories::parse_id(&id)?;
    let opened = vault.opened();
    let path = vault.with_files(|conn, files| docs::open_copy(conn, files, &opened, id))?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|_| AppError::Internal)
}

/// Saves a copy of the file where the person chooses. Returns false if they cancel. The copy is not encrypted. The phone has no
/// file dialogs, so there it is refused.
#[tauri::command]
pub async fn save_document_copy(app: tauri::AppHandle, vault: Shared<'_>, id: String) -> Result<bool, AppError> {
    let id = categories::parse_id(&id)?;
    let (name, bytes) = vault.with_files(|conn, files| {
        let document = docs::get(conn, files, id)?;
        Ok((document.name, docs::read(conn, files, id)?))
    })?;
    let Some(path) = ask::save_any(&app, &doc_names::disk_name(&name))? else { return Ok(false) };
    backup::write_file(&path, &bytes)?;
    Ok(true)
}

/// Starts sending a file in: what to call it, which date, which links, and how many bytes it has. The file follows in pieces
/// (`document_upload_chunk`), because a request with raw bytes does not work on Android, and ends with `document_upload_finish`.
#[tauri::command]
pub fn document_upload_begin(vault: Shared, input: docs::NewDocument, size: u64) -> Result<(), AppError> {
    if vault.status() != VaultStatus::Unlocked {
        return Err(AppError::Locked);
    }
    vault.uploads().begin(input, size)
}

/// The next piece of the file as base64url text, which must start where the last one ended (`offset`). Returns how many bytes there
/// are now.
#[tauri::command]
pub fn document_upload_chunk(vault: Shared, offset: u64, data: String) -> Result<u64, AppError> {
    vault.uploads().push(offset, &data)
}

/// The file is all here: stores it. If the same bytes are in the library already, the answer says so and nothing is stored twice.
#[tauri::command]
pub fn document_upload_finish(vault: Shared) -> Result<docs::AddOutcome, AppError> {
    let today = today_text(&vault)?;
    let (input, bytes) = vault.uploads().finish()?;
    vault.with_files(|conn, files| docs::add(conn, files, &input, &bytes, &today))
}

#[tauri::command]
pub fn document_upload_cancel(vault: Shared) {
    vault.uploads().clear();
}

/// Adds the files that were just dropped on the window, each as a document of its own named after the file. The page does not name a
/// path: the window took the drop (the web view cannot be relied on to give a page the files) and Rust remembers which files they were.
/// What each came to is in the answer, and one that cannot be added does not stop the others.
#[tauri::command]
pub async fn document_add_dropped(vault: Shared<'_>) -> Result<crate::documents::dropped::DroppedResult, AppError> {
    if vault.status() != VaultStatus::Unlocked {
        return Err(AppError::Locked);
    }
    let today = today_text(&vault)?;
    let pending = vault.dropped().take();
    Ok(crate::documents::dropped::add_all(pending, |input, bytes| vault.with_files(|conn, files| docs::add(conn, files, input, bytes, &today))))
}

#[cfg(test)]
mod tests {
    use super::{typed_address, COMMANDS};

    #[test]
    fn an_address_typed_in_the_app_must_be_one_of_a_home_network() {
        assert_eq!(typed_address(None).unwrap(), None);
        assert_eq!(typed_address(Some("  ")).unwrap(), None, "empty means the system's choice");
        assert_eq!(typed_address(Some(" 192.168.1.20 ")).unwrap(), Some("192.168.1.20".parse().unwrap()));
        assert_eq!(typed_address(Some("10.0.0.7")).unwrap(), Some("10.0.0.7".parse().unwrap()));
        assert_eq!(typed_address(Some("fd12:3456:789a::1")).unwrap(), Some("fd12:3456:789a::1".parse().unwrap()));
        for bad in ["8.8.8.8", "127.0.0.1", "::1", "0.0.0.0", "192.168.1.20:5000", "my-computer.local", "192.168.1", "2001:db8::1"] {
            assert!(typed_address(Some(bad)).is_err(), "{bad}");
        }
    }

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
    fn every_command_is_handed_to_the_app() {
        // a command that is defined, listed and permitted but missing from the handler cannot be called, and nothing else notices
        let app = include_str!("lib.rs");
        for name in COMMANDS {
            assert!(app.contains(&format!("commands::{name},")), "lib.rs does not register {name}");
        }
    }

    #[test]
    fn every_command_function_is_registered() {
        let source = include_str!("commands.rs");
        let defined = source.lines().filter(|l| l.trim() == "#[tauri::command]").count();
        assert_eq!(defined, COMMANDS.len(), "a command is defined but not listed");
    }
}
