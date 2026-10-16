//! Dev only: a desktop without a window, to try the phone app against over the real network.
//!
//!   cargo run --example sync_desktop -- <data-dir> setup            makes a data file with a few rows (passphrase below)
//!   cargo run --example sync_desktop -- <data-dir> add Coffee 3.50   adds a transaction to the category "Food"
//!   cargo run --example sync_desktop -- <data-dir> edit Coffee 4.00  changes the amount of a transaction
//!   cargo run --example sync_desktop -- <data-dir> list              prints the transactions and the currency
//!   cargo run --example sync_desktop -- <data-dir> pair              opens a window for a new phone
//!   cargo run --example sync_desktop -- <data-dir> sync both|to_desktop|to_phone
//!   cargo run --example sync_desktop -- <data-dir> forget            forgets every paired phone
//!
//! `pair` and `sync` print one line for every change of the status (`STATUS {json}`), with the code to scan (`QR ...`) and the
//! six digits to type on the phone (`DIGITS ...`). When there are conflicts to review they are answered with `CHOOSE=mine` or
//! `CHOOSE=theirs` from the environment (the desktop's version, or the phone's). `ADDRESS=ip` picks the address to listen on.
//!
//! The data file has the passphrase `desk-test-passphrase` and keeps its secret in a file in the folder, not in the keychain.
//! It is a tool for trying things with a test phone; it never touches the data of the app.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use meowny_lib::categories::{self, CategoryType};
use meowny_lib::keys::FileStore;
use meowny_lib::sync::engine::Choice;
use meowny_lib::sync::service::{Status, SyncService};
use meowny_lib::sync::wire::Mode;
use meowny_lib::transactions::{self, NewTransaction};
use meowny_lib::vault::{Policy, Settings, Vault, VaultStatus};

const PASSPHRASE: &str = "desk-test-passphrase";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(dir), Some(command)) = (args.get(1).map(PathBuf::from), args.get(2).map(String::as_str)) else {
        eprintln!("usage: sync_desktop <data-dir> setup|add|edit|list|pair|sync|forget ...");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&dir)?;
    let vault = Arc::new(Vault::with_policy(dir.clone(), Box::new(FileStore::new(dir.join("desktop.secret"))), Policy::DESKTOP));
    if command == "setup" {
        if vault.status() != VaultStatus::NotSetUp {
            vault.wipe("DELETE")?;
        }
        let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() };
        vault.create(&settings, Some(PASSPHRASE))?;
        vault.with_conn(|c| {
            let food = categories::create(c, "Food", CategoryType::Expense)?.id;
            categories::create(c, "Salary", CategoryType::Income)?;
            for (name, amount) in [("Coffee", "3.50"), ("Tea", "2.00")] {
                transactions::create(c, &NewTransaction { name: name.into(), amount: amount.into(), payment_date: "2026-10-01".into(), category_id: food.to_string(), description: None, client_uuid: None })?;
            }
            Ok(())
        })?;
        println!("set up {}", dir.display());
        return Ok(());
    }
    vault.unlock(Some(PASSPHRASE))?;
    match command {
        "add" => {
            let (name, amount) = (args.get(3).ok_or("a name")?, args.get(4).ok_or("an amount")?);
            vault.with_conn(|c| {
                let food: i64 = c.query_row("SELECT id FROM categories WHERE name = 'Food'", [], |r| r.get(0))?;
                transactions::create(c, &NewTransaction { name: name.clone(), amount: amount.clone(), payment_date: "2026-10-02".into(), category_id: food.to_string(), description: None, client_uuid: None })?;
                Ok(())
            })?;
            println!("added {name} {amount}");
        }
        "edit" => {
            let (name, amount) = (args.get(3).ok_or("a name")?, args.get(4).ok_or("an amount")?);
            let minor = meowny_lib::money::parse_amount(amount)?;
            let rows = vault.with_conn(|c| Ok(c.execute("UPDATE transactions SET amount_minor = ?1 WHERE name = ?2", rusqlite::params![minor, name])?))?;
            println!("changed {rows} row(s)");
        }
        "list" => {
            let lines = vault.with_conn(|c| {
                let currency: String = c.query_row("SELECT currency FROM settings", [], |r| r.get(0))?;
                let mut stmt = c.prepare("SELECT name, amount_minor FROM transactions ORDER BY name")?;
                let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
                Ok((currency, rows))
            })?;
            println!("currency {}", lines.0);
            for (name, minor) in &lines.1 {
                println!("tx {name} {}", meowny_lib::money::format_amount(*minor));
            }
            println!("count {}", lines.1.len());
        }
        "forget" => {
            let service = SyncService::new(vault.clone());
            for device in service.devices()? {
                service.remove_device(&device.device_id)?;
            }
            println!("forgot every phone");
        }
        "pair" | "sync" => {
            let address = std::env::var("ADDRESS").ok().filter(|a| !a.is_empty()).map(|a| a.parse()).transpose()?;
            let service = SyncService::new(vault.clone());
            if command == "pair" {
                service.start_pairing("Test desktop", address)?;
            } else {
                let mode = match args.get(3).map(String::as_str) {
                    Some("to_desktop") => Mode::ToDesktop,
                    Some("to_phone") => Mode::ToPhone,
                    _ => Mode::Both,
                };
                service.start_sync("Test desktop", address, mode)?;
            }
            let choose = match std::env::var("CHOOSE").as_deref() {
                Ok("theirs") => Choice::Theirs,
                _ => Choice::Mine,
            };
            let (until, mut last) = (Instant::now() + Duration::from_secs(600), String::new());
            loop {
                let status = service.status();
                let line = serde_json::to_string(&status)?;
                if line != last {
                    println!("STATUS {line}");
                    match &status {
                        Status::Waiting { qr, .. } => println!("QR {qr}"),
                        Status::ShowCode { code, .. } => println!("DIGITS {code}"),
                        Status::Reviewing { conflicts, .. } => {
                            println!("CHOOSING {choose:?} for {} conflict(s)", conflicts.len());
                            service.decide(vec![choose; conflicts.len()])?;
                        }
                        Status::Done { .. } => {
                            println!("DONE");
                            return Ok(());
                        }
                        Status::Failed { reason, .. } => {
                            println!("FAILED {reason}");
                            std::process::exit(1);
                        }
                        _ => {}
                    }
                    last = line;
                }
                if Instant::now() > until {
                    service.cancel();
                    println!("FAILED timeout");
                    std::process::exit(1);
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        other => {
            eprintln!("unknown command {other}");
            std::process::exit(2);
        }
    }
    Ok(())
}
