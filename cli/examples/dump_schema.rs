//! Writes the app's description of its routes, which the CLI embeds as `schema.json`:
//!
//!   cargo run --example dump_schema > schema.json
//!
//! A test fails when the file and the route table differ, and says to run this.

use meowny_lib::api::{routes, API_VERSION};
use serde_json::json;

fn main() {
    let routes: Vec<_> = routes::ALL.iter().map(|r| r.describe()).collect();
    println!("{}", serde_json::to_string_pretty(&json!({ "api": API_VERSION, "routes": routes })).unwrap());
}
