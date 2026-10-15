//! Dev only: serves the command API on demo data, so it can be tried with curl before (or without)
//! the desktop app.
//!
//!   cargo run --example api_demo
//!
//! It builds a throwaway encrypted vault in a temporary folder, seeds it, makes a read token and a
//! write token, prints them with a curl line and serves until Ctrl+C. The real app's data and the OS
//! keychain are not touched: the key is kept in memory, so the temporary folder left behind is unreadable.
//! Tokens print here because this is a demo; the app shows a token once, when it is made.

use std::sync::{Arc, Mutex};

use meowny_lib::api::{routes, ApiServer, Limits};
use meowny_lib::keys::{KeyError, SecretStore, SECRET_LEN};
use meowny_lib::tokens::{self, NewToken, Scope};
use meowny_lib::vault::{Settings, Vault};
use zeroize::Zeroizing;

#[derive(Default)]
struct MemoryStore(Mutex<Option<[u8; SECRET_LEN]>>);

impl SecretStore for MemoryStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; SECRET_LEN]>>, KeyError> {
        Ok(self.0.lock().unwrap().map(Zeroizing::new))
    }
    fn set(&self, secret: &[u8; SECRET_LEN]) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = Some(*secret);
        Ok(())
    }
    fn delete(&self) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("meowny-api-demo-{}", std::process::id()));
    let vault = Arc::new(Vault::new(dir.clone(), Box::new(MemoryStore::default())));
    let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "Europe/Dublin".into(), language: "en".into() };
    vault.create(&settings, None)?;
    let mut seeded = Ok(());
    vault.with_conn(|c| {
        seeded = meowny_lib::seed::run(c);
        Ok(())
    })?;
    seeded?;

    let make = |name: &str, scope| {
        let input = NewToken { name: name.into(), scope, expires_in_days: 1 };
        vault.with_conn(|c| tokens::create(c, &input, chrono::Utc::now())).map(|made| made.token)
    };
    let (read, write) = (make("demo read", Scope::Read)?, make("demo write", Scope::Write)?);

    let server = ApiServer::start(vault.clone(), routes::ALL, Limits::default())?;
    let port = server.port();
    println!("serving on 127.0.0.1:{port} (data in {}), Ctrl+C to stop\n", dir.display());
    println!("read token:  {read}");
    println!("write token: {write}\n");
    println!("curl -s -X POST http://127.0.0.1:{port}/v1/status -H 'Authorization: Bearer {read}'");
    println!("curl -s -X POST http://127.0.0.1:{port}/v1/schema -H 'Authorization: Bearer {read}'");
    loop {
        std::thread::park();
    }
}
