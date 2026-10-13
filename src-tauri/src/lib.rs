pub mod categories;
pub mod clock;
pub mod commands;
pub mod db;
pub mod keys;
pub mod migrate;
/// Dev demo data, used by `examples/seed.rs`.
pub mod seed;
pub mod vault;
#[cfg(test)]
mod schema_tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager, WindowEvent};

use crate::keys::KeychainStore;
use crate::vault::Vault;

const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);

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

pub fn run() {
    install_safe_panic_hook();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard").on_navigation(|_, url| is_app_url(url)).build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = KeychainStore::new().expect("keychain entry");
            let vault = Arc::new(Vault::new(dir, Box::new(store)));
            app.manage(vault.clone());

            // Idle lock. The UI is told so it can show the lock screen.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(IDLE_CHECK_EVERY);
                if vault.lock_if_idle(Instant::now()) {
                    let _ = handle.emit("vault-locked", ());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let vault = window.state::<Arc<Vault>>();
            let Ok(prefs) = vault.lock_prefs() else { return };
            match event {
                WindowEvent::CloseRequested { .. } if prefs.lock_on_close => vault.lock(),
                WindowEvent::Resized(_) if prefs.lock_on_minimize && window.is_minimized().unwrap_or(false) => {
                    vault.lock();
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running meowNY");
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
