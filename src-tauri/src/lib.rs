#[allow(dead_code)] // Not called yet: the app opens the database from phase 1 on.
pub mod db;
#[allow(dead_code)] // Called by the startup path once the app opens its database.
pub mod migrate;
/// Dev demo data, used by `examples/seed.rs`.
pub mod seed;
#[cfg(test)]
mod schema_tests;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running meowNY");
}
