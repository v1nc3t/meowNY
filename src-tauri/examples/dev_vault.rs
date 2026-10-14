//! Dev only: replaces the app's data with demo data, as a fresh install would have it.
//!
//!   cargo run --example dev_vault -- <data-dir>
//!   MEOWNY_DEV_PASSPHRASE='a passphrase' cargo run --example dev_vault -- <data-dir>
//!
//! `<data-dir>` is the folder the app keeps its data file in (on Linux `~/.local/share/org.meowny.app`).
//! It does what "delete all my data" does (the data file, the key in the OS keychain, the settings file and any
//! backups the app made in its folder), then sets the app up again with demo data and prints the new recovery
//! key. The lock settings are kept. Quit the app first, or restart it after: a running app keeps showing the
//! data it had open.
//!
//! The data file ends up without a passphrase unless `MEOWNY_DEV_PASSPHRASE` is set (it is read from the
//! environment so it is not left in the shell history). Run it again to start over; it is meant to be repeated.

use std::path::PathBuf;

use meowny_lib::keys::{KeychainStore, SecretStore};
use meowny_lib::vault::{AppError, Settings, Vault, VaultStatus};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(dir) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: dev_vault <data-dir>");
        std::process::exit(2);
    };
    let passphrase = std::env::var("MEOWNY_DEV_PASSPHRASE").ok().filter(|p| !p.is_empty());
    // Nothing is deleted unless the keychain can be reached, so a failure leaves the data as it was.
    let store = KeychainStore::new()?;
    store.get().map_err(|_| "the OS keychain cannot be reached; nothing was changed")?;
    let vault = Vault::new(dir.clone(), Box::new(store));

    // The lock settings belong to the app, not to the data, and can be read while it is locked.
    let existing = vault.status() != VaultStatus::NotSetUp;
    let (prefs, notify) = if existing { (vault.lock_prefs().ok(), vault.notify_due().ok()) } else { (None, None) };
    if existing {
        vault.wipe("DELETE")?;
        println!("deleted the data in {}", dir.display());
    }

    let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() };
    let recovery = vault.create(&settings, passphrase.as_deref())?;
    let mut seeded = Ok(());
    vault.with_conn(|c| {
        seeded = meowny_lib::seed::run(c);
        Ok(())
    })?;
    seeded?;
    if let Some(prefs) = prefs {
        vault.save_lock_prefs(&prefs)?;
    }
    if let Some(on) = notify {
        vault.set_notify_due(on)?;
    }

    let counts = vault.with_conn(|c| {
        let count = |table: &str| -> Result<i64, AppError> { Ok(c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?) };
        Ok((count("categories")?, count("budgets")?, count("recurring_transactions")?, count("transactions")?))
    })?;
    println!("seeded {} categories, {} budget rows, {} recurring items and {} transactions", counts.0, counts.1, counts.2, counts.3);
    println!("passphrase: {}", if passphrase.is_some() { "the one in MEOWNY_DEV_PASSPHRASE" } else { "none" });
    println!("recovery key: {recovery}");
    Ok(())
}
