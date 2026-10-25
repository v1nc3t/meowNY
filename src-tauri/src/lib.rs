pub mod api;
pub mod backup;
pub mod budgets;
pub mod categories;
pub mod clock;
pub mod commands;
pub mod currency;
pub mod db;
pub mod debts;
pub mod display;
pub mod documents;
pub mod export;
pub mod keys;
pub mod migrate;
pub mod recovery_file;
pub mod recurring;
pub mod report;
pub mod money;
pub mod schedule;
/// Dev demo data, used by `examples/seed.rs`.
pub mod seed;
pub mod sync;
pub mod stats;
pub mod tokens;
pub mod transactions;
pub mod vault;
#[cfg(test)]
mod schema_tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, WindowEvent};

use crate::keys::FileStore;
use crate::sync::service::SyncService;
use crate::vault::{Policy, Vault};

const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);
const SCHEDULE_CHECK_EVERY: Duration = Duration::from_secs(60);

/// The phone has no keychain to rely on, so its vault keeps the secret in a file and demands a passphrase.
fn phone_vault(dir: std::path::PathBuf) -> Vault {
    let secret = dir.join("phone.secret");
    Vault::with_policy(dir, Box::new(FileStore::new(secret)), Policy::PHONE)
}

/// A computer keeps the secret in the OS keychain. In a debug build `MEOWNY_PROFILE=phone` gives the phone's vault instead, so
/// the phone's screens can be tried in a window on a computer (with a separate data folder; the vault is of another kind).
#[cfg(desktop)]
fn make_vault(dir: std::path::PathBuf) -> Vault {
    if cfg!(debug_assertions) && std::env::var("MEOWNY_PROFILE").is_ok_and(|v| v == "phone") {
        return phone_vault(dir);
    }
    Vault::new(dir, Box::new(crate::keys::KeychainStore::new().expect("keychain entry")))
}

/// A phone (Android or iOS) always has the phone's vault.
#[cfg(mobile)]
fn make_vault(dir: std::path::PathBuf) -> Vault {
    phone_vault(dir)
}

/// iOS copies an app's folder to iCloud backups by default, with the encrypted data and the secret file side by side; the erase
/// after 5 wrong passphrases would then protect nothing (Android's backup is off for the same reason). The flag on the folder
/// covers everything in it. Without it the app does not start.
#[cfg(target_os = "ios")]
fn keep_out_of_backups(dir: &std::path::Path) -> std::io::Result<()> {
    use objc2_foundation::{NSNumber, NSString, NSURLIsExcludedFromBackupKey, NSURL};
    std::fs::create_dir_all(dir)?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
    let yes = NSNumber::numberWithBool(true);
    // SAFETY: the value for this key is an NSNumber holding a bool.
    unsafe { url.setResourceValue_forKey_error(Some(yes.as_ref()), NSURLIsExcludedFromBackupKey) }
        .map_err(|_| std::io::Error::other("the data folder could not be kept out of backups"))
}

/// WebKitGTK's GPU drawing leaves stale pixels behind on some Linux graphics setups (seen with WebKitGTK 2.52 on X11 with Intel
/// and NVIDIA graphics): old card borders stay on screen as lines when the window is resized. Drawing on the CPU has no such
/// lines and stays smooth; turning off the DMA-BUF renderer instead also removed them, but made every page lag. A value the
/// person set themselves is kept. It must run before any thread or the webview starts.
#[cfg(target_os = "linux")]
fn avoid_webkit_repaint_glitch() {
    if std::env::var_os("WEBKIT_SKIA_ENABLE_CPU_RENDERING").is_none() {
        std::env::set_var("WEBKIT_SKIA_ENABLE_CPU_RENDERING", "1");
    }
}

/// Panic messages can quote data (an amount, a description), so only the location is printed.
fn install_safe_panic_hook() {
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(at) => eprintln!("meowNY panicked at {}:{}", at.file(), at.line()),
        None => eprintln!("meowNY panicked"),
    }));
}

/// The webview may only show the app itself. Anything else is refused; links to the outside
/// go through the opener plugin, which hands them to the system browser.
// The local server of `tauri dev` is allowed in a debug build only; clippy sees that `cfg!` as a plain `true`.
#[allow(clippy::match_like_matches_macro)]
fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "about" => true,
        "http" | "https" => match url.host_str() {
            Some("tauri.localhost") => true,
            Some("127.0.0.1" | "localhost") => cfg!(debug_assertions),
            _ => false,
        },
        _ => false,
    }
}

/// Asks the person to allow something a token wants to do, in a native dialog over the app's window.
struct AllowDialog(tauri::AppHandle);

#[cfg(desktop)]
impl api::Show for AllowDialog {
    fn show(&self, approval: &api::Approval) -> bool {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        // The window may be behind others or minimised; the question must not wait unseen.
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        self.0
            .dialog()
            .message(&approval.text)
            .title(&approval.title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Allow".to_string(), "Deny".to_string()))
            .blocking_show()
    }
}

/// Only a computer has the command API that asks; a phone never starts it, so on a phone the answer is always no.
#[cfg(mobile)]
impl api::Show for AllowDialog {
    fn show(&self, _: &api::Approval) -> bool {
        let _ = &self.0;
        false
    }
}

// On a phone the app starts through this entry point (Tauri's generated Android and iOS projects call it); on a computer `main`
// does.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    avoid_webkit_repaint_glitch();
    install_safe_panic_hook();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard").on_navigation(|_, url| is_app_url(url)).build());

    // The camera that reads the code of the desktop, on a phone only.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    // The file dialogs, on a computer only (the phone has no backups or exports to save and load).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_dialog::init());

    builder
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            #[cfg(target_os = "ios")]
            keep_out_of_backups(&dir)?;
            let vault = Arc::new(make_vault(dir));
            app.manage(vault.clone());
            // Plain copies for the system's viewer are removed at every lock and exit; a crash may have left some.
            vault.clear_open_copies();

            // Syncing with the phone: a window on the home network for a minute or two, only while the app is unlocked.
            let syncing = Arc::new(SyncService::new(vault.clone()));
            app.manage(syncing.clone());

            // The command API for the CLI, if the person left it on. Before it writes a file for a token, the person
            // is asked in a dialog.
            let prompter = api::Prompter::new(AllowDialog(app.handle().clone()));
            let api_host = Arc::new(api::Host::with_prompter(vault.clone(), prompter));
            app.manage(api_host.clone());
            // The command API belongs to the desktop app; the phone has none.
            if vault.policy() == Policy::DESKTOP {
                api_host.start_if_enabled();
            }

            // Scheduled backups, while the app is unlocked.
            let scheduled = vault.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(SCHEDULE_CHECK_EVERY);
                schedule::run_if_due(&scheduled, chrono::Utc::now());
            });

            // Idle lock. The UI is told so it can show the lock screen.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(IDLE_CHECK_EVERY);
                if vault.lock_if_idle(Instant::now()) {
                    syncing.cancel();
                    let _ = handle.emit("vault-locked", ());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let vault = window.state::<Arc<Vault>>();
            // Files dropped on the window: the web view cannot be relied on to hand them to the page, so the window takes them.
            if let WindowEvent::DragDrop(drag) = event {
                for (name, payload) in documents::dropped::on_drag(&vault, drag) {
                    let _ = window.emit(name, payload);
                }
                return;
            }
            let Ok(prefs) = vault.lock_prefs() else { return };
            match event {
                WindowEvent::CloseRequested { .. } if prefs.lock_on_close => {
                    vault.lock();
                    window.state::<Arc<SyncService<Vault>>>().cancel();
                }
                WindowEvent::Resized(_) if prefs.lock_on_minimize && window.is_minimized().unwrap_or(false) => {
                    vault.lock();
                    window.state::<Arc<SyncService<Vault>>>().cancel();
                    let _ = window.emit("vault-locked", ());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::touch_activity,
            commands::get_settings,
            commands::save_settings,
            commands::get_lock_prefs,
            commands::save_lock_prefs,
            commands::data_location,
            commands::wipe_all_data,
            commands::list_categories,
            commands::create_category,
            commands::rename_category,
            commands::delete_category,
            commands::list_categories_in_month,
            commands::reorder_categories,
            commands::list_transactions,
            commands::create_transaction,
            commands::update_transaction,
            commands::delete_transaction,
            commands::today,
            commands::set_budget,
            commands::budget_overview,
            commands::recover_vault,
            commands::new_recovery_key,
            commands::save_recovery_key,
            commands::budget_history,
            commands::change_currency,
            commands::list_recurring,
            commands::create_recurring,
            commands::update_recurring,
            commands::delete_recurring,
            commands::pay_recurring,
            commands::skip_recurring,
            commands::set_recurring_active,
            commands::end_recurring,
            commands::recurring_dues,
            commands::get_notify_due,
            commands::set_notify_due,
            commands::show_due_notification,
            commands::budget_vs_actual,
            commands::category_trend,
            commands::year_trend,
            commands::top_categories,
            commands::recurring_commitments,
            commands::recurring_planned_vs_paid,
            commands::create_backup,
            commands::restore_backup,
            commands::restore_backup_new_vault,
            commands::get_backup_schedule,
            commands::set_backup_schedule,
            commands::clear_backup_schedule,
            commands::export_json,
            commands::export_csv,
            commands::export_xlsx,
            commands::export_pdf,
            commands::export_preview,
            commands::export_preview_text,
            commands::export_preview_workbook,
            commands::export_preview_pdf,
            commands::get_api_status,
            commands::set_api_enabled,
            commands::list_api_tokens,
            commands::create_api_token,
            commands::revoke_api_token,
            commands::app_profile,
            commands::unlock_attempts,
            commands::sync_status,
            commands::sync_start_pairing,
            commands::sync_enter_code,
            commands::sync_decide,
            commands::sync_start,
            commands::sync_scan,
            commands::sync_cancel,
            commands::sync_acknowledge,
            commands::sync_devices,
            commands::sync_remove_device,
            commands::list_documents,
            commands::get_document,
            commands::update_document,
            commands::delete_document,
            commands::link_document,
            commands::unlink_document,
            commands::document_links,
            commands::document_library_size,
            commands::document_counts,
            commands::check_documents,
            commands::read_document_image,
            commands::read_document_preview,
            commands::open_document,
            commands::save_document_copy,
            commands::document_upload_begin,
            commands::document_upload_chunk,
            commands::document_upload_finish,
            commands::document_upload_cancel,
            commands::document_add_dropped,
            commands::list_people,
            commands::create_person,
            commands::update_person,
            commands::archive_person,
            commands::unarchive_person,
            commands::person_statement,
            commands::debts_summary,
            commands::debts_due_payments,
            commands::debts_history,
            commands::add_ledger_entry,
            commands::update_ledger_entry,
            commands::delete_ledger_entry,
            commands::settle_person,
            commands::split_preview,
            commands::set_transaction_split,
            commands::clear_transaction_split,
            commands::set_recurring_split,
            commands::clear_recurring_split,
            commands::export_debts,
            commands::export_debts_preview,
            commands::bill_code,
            commands::recurring_code,
            commands::bill_preview,
            commands::bill_accept,
            commands::debts_reminders,
        ])
        .build(tauri::generate_context!())
        .expect("error while building meowNY")
        .run(|handle, event| {
            // Remove the port file on a normal exit; a crash leaves it and the next start cleans it up.
            if let tauri::RunEvent::Exit = event {
                if let Some(host) = handle.try_state::<Arc<api::Host>>() {
                    host.stop();
                }
                if let Some(vault) = handle.try_state::<Arc<Vault>>() {
                    vault.clear_open_copies();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::is_app_url;

    fn url(text: &str) -> tauri::Url {
        text.parse().unwrap()
    }

    #[test]
    fn app_urls_are_allowed() {
        assert!(is_app_url(&url("tauri://localhost/index.html")));
        assert!(is_app_url(&url("http://tauri.localhost/app")));
    }

    #[test]
    fn remote_urls_are_refused() {
        assert!(!is_app_url(&url("https://example.com/")));
        assert!(!is_app_url(&url("http://tauri.localhost.evil.test/")));
        assert!(!is_app_url(&url("file:///etc/passwd")));
        assert!(!is_app_url(&url("javascript:alert(1)")));
    }
}
