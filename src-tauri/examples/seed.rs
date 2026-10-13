//! Dev only: creates an encrypted database filled with demo data.
//!
//!   cargo run --example seed -- <db-path> <64-hex-char-key>
//!
//! The key is a throwaway dev key. Use a new path; seeding refuses a database with data.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, key_hex] = args.as_slice() else {
        eprintln!("usage: seed <db-path> <64-hex-char-key>");
        std::process::exit(2);
    };
    if let Err(e) = seed(Path::new(path), key_hex) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    println!("seeded {path}");
}

fn seed(path: &Path, key_hex: &str) -> Result<(), Box<dyn std::error::Error>> {
    if key_hex.len() != 64 || !key_hex.is_ascii() {
        return Err("key must be 64 hex characters".into());
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&key_hex[i * 2..i * 2 + 2], 16)?;
    }
    let mut conn = meowny_lib::db::open_and_migrate(path, &meowny_lib::db::DbKey::from_bytes(key))?;
    meowny_lib::seed::run(&mut conn)?;
    Ok(())
}
