#[allow(dead_code)] // Not called yet: the app opens the database from phase 1 on.
mod db;
#[allow(dead_code)] // Called by the startup path once the app opens its database.
mod migrate;
#[cfg(test)]
mod schema_tests;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running meowNY");
}
