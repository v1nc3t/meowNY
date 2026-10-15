pub mod api;
pub mod backup;
pub mod budgets;
pub mod categories;
pub mod clock;
pub mod commands;
pub mod currency;
pub mod db;
pub mod display;
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

/// Whether this is the phone's app: always on a phone, and in a debug build when `MEOWNY_PROFILE=phone` is set, so the phone's
/// screens can be tried in a window on a computer (with a separate data folder; the vault is of another kind).
fn is_phone() -> bool {
    cfg!(any(target_os = "android", target_os = "ios")) || (cfg!(debug_assertions) && std::env::var("MEOWNY_PROFILE").is_ok_and(|v| v == "phone"))
}

/// The phone has no keychain to rely on, so its vault keeps the secret in a file and demands a passphrase.
fn phone_vault(dir: std::path::PathBuf) -> Vault {
    let secret = dir.join("phone.secret");
    Vault::with_policy(dir, Box::new(FileStore::new(secret)), Policy::PHONE)
}

#[cfg(not(target_os = "android"))]
fn make_vault(dir: std::path::PathBuf) -> Vault {
    if is_phone() {
        return phone_vault(dir);
    }
    Vault::new(dir, Box::new(crate::keys::KeychainStore::new().expect("keychain entry")))
}

#[cfg(target_os = "android")]
fn make_vault(dir: std::path::PathBuf) -> Vault {
    phone_vault(dir)
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

pub fn run() {
    install_safe_panic_hook();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard").on_navigation(|_, url| is_app_url(url)).build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let vault = Arc::new(make_vault(dir));
            app.manage(vault.clone());

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
            commands::get_api_status,
            commands::set_api_enabled,
            commands::list_api_tokens,
            commands::create_api_token,
            commands::revoke_api_token,
            commands::app_profile,
            commands::unlock_attempts,
            commands::sync_status,
            commands::sync_start_pairing,
            commands::sync_answer_pairing,
            commands::sync_start,
            commands::sync_scan,
            commands::sync_cancel,
            commands::sync_acknowledge,
            commands::sync_devices,
            commands::sync_remove_device,
        ])
        .build(tauri::generate_context!())
        .expect("error while building meowNY")
        .run(|handle, event| {
            // Remove the port file on a normal exit; a crash leaves it and the next start cleans it up.
            if let tauri::RunEvent::Exit = event {
                if let Some(host) = handle.try_state::<Arc<api::Host>>() {
                    host.stop();
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
