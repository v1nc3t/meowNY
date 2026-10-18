//! The commands, worked out from the app's own description of its routes (`schema.json`, written
//! by `cargo run --example dump_schema` from the route table and checked against it by a test). So
//! a route has a command by construction, and a flag exists for every argument the route takes.
//!
//! A route `transactions.create` is the command `meowny transactions create`; an argument
//! `paymentDate` is the flag `--payment-date`; an argument that can be cleared also has
//! `--clear-payment-date`; the argument called `id` may be given as the first plain word.

use serde_json::Value;

const EMBEDDED: &str = include_str!("../schema.json");

/// The API version this CLI was built for.
pub const API_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// As in the request body, camelCase.
    pub name: String,
    /// `--kebab-case`, without the dashes. The argument that carries the bytes of a file is `--file`, whatever it is called in the body.
    pub flag: String,
    /// string, integer, boolean, date, month, year, id, money, uuid, ids, path or bytes.
    pub kind: String,
    pub required: bool,
    pub nullable: bool,
    pub summary: String,
    pub values: Vec<String>,
}

/// What a command does when it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Calls the route of the same name.
    Route,
    /// Saves the token read from standard input.
    Login,
    Logout,
    /// Prints this catalog as JSON, without the app (or asks the app with `--app`).
    Schema,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// `["transactions", "create"]`, or `["status"]`.
    pub words: Vec<String>,
    /// The route it calls. None for the commands that only touch this computer.
    pub route: Option<String>,
    /// The scope the route needs: "read" or "write".
    pub scope: Option<String>,
    pub summary: String,
    pub params: Vec<Param>,
    /// The person has to allow each run in the app, so it can take a while.
    pub confirm: bool,
    pub action: Action,
}

impl Command {
    pub fn name(&self) -> String {
        self.words.join(" ")
    }

    pub fn verb(&self) -> &str {
        self.words.last().map(String::as_str).unwrap_or("")
    }

    /// Commands that delete or end something ask for `--yes`, so a script cannot do it by accident.
    pub fn destructive(&self) -> bool {
        self.route.is_some() && matches!(self.verb(), "delete" | "remove" | "end")
    }

    /// The `id` argument, or for a command that sends a file the file, which may be given without a flag.
    pub fn positional(&self) -> Option<&Param> {
        self.params.iter().find(|p| p.name == "id" && p.required).or_else(|| self.params.iter().find(|p| p.kind == "bytes" && p.required))
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub commands: Vec<Command>,
}

/// `categoryId` is `category-id`.
pub fn kebab(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    value.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("the schema has no `{key}`"))
}

fn local(words: &[&str], summary: &str, action: Action) -> Command {
    Command { words: words.iter().map(|w| w.to_string()).collect(), route: None, scope: None, summary: summary.into(), params: Vec::new(), confirm: false, action }
}

impl Catalog {
    pub fn embedded() -> Catalog {
        Catalog::from_json(EMBEDDED).expect("schema.json is checked by a test")
    }

    pub fn from_json(text_of_schema: &str) -> Result<Catalog, String> {
        let schema: Value = serde_json::from_str(text_of_schema).map_err(|e| e.to_string())?;
        if schema.get("api").and_then(Value::as_u64) != Some(API_VERSION) {
            return Err("the schema is for another API version".into());
        }
        let mut commands = Vec::new();
        for route in schema.get("routes").and_then(Value::as_array).ok_or("the schema has no routes")? {
            let name = text(route, "name")?;
            let mut params = Vec::new();
            for p in route.get("params").and_then(Value::as_array).ok_or("a route has no params")? {
                let pname = text(p, "name")?;
                let kind = text(p, "type")?;
                params.push(Param {
                    flag: if kind == "bytes" { "file".to_string() } else { kebab(&pname) },
                    name: pname,
                    kind,
                    required: p.get("required").and_then(Value::as_bool).unwrap_or(false),
                    nullable: p.get("nullable").and_then(Value::as_bool).unwrap_or(false),
                    summary: text(p, "summary")?,
                    values: p
                        .get("values")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect())
                        .unwrap_or_default(),
                });
            }
            // A command that sends a file reads it itself, and keeps it under the name of the file unless told another.
            if params.iter().any(|p| p.kind == "bytes") {
                for p in params.iter_mut().filter(|p| p.name == "name") {
                    p.required = false;
                    p.summary = "The name to keep it under, up to 120 characters. The name of the file if left out.".into();
                }
            }
            // `schema` is the one route that is also answered here, without the app.
            let action = if name == "schema" { Action::Schema } else { Action::Route };
            commands.push(Command {
                words: name.split('.').map(str::to_string).collect(),
                route: Some(name),
                scope: Some(text(route, "scope")?),
                summary: text(route, "summary")?,
                params,
                confirm: route.get("confirm").and_then(Value::as_bool).unwrap_or(false),
                action,
            });
        }
        commands.push(local(&["login"], "Saves a token, read from standard input, for the commands that follow.", Action::Login));
        commands.push(local(&["logout"], "Removes the saved token.", Action::Logout));
        Ok(Catalog { commands })
    }

    pub fn find(&self, words: &[&str]) -> Option<&Command> {
        self.commands.iter().find(|c| c.words.iter().map(String::as_str).eq(words.iter().copied()))
    }

    /// The first words that have more than one command under them, in schema order: `transactions`, `budgets`.
    pub fn nouns(&self) -> Vec<&str> {
        let mut nouns: Vec<&str> = Vec::new();
        for c in &self.commands {
            if c.words.len() == 2 && !nouns.contains(&c.words[0].as_str()) {
                nouns.push(&c.words[0]);
            }
        }
        nouns
    }

    pub fn verbs(&self, noun: &str) -> Vec<&Command> {
        self.commands.iter().filter(|c| c.words.len() == 2 && c.words[0] == noun).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_schema_loads_and_every_command_name_is_unique() {
        let catalog = Catalog::embedded();
        let mut names: Vec<String> = catalog.commands.iter().map(Command::name).collect();
        let total = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), total, "two commands share a name");
        assert!(total > 30);
        for c in &catalog.commands {
            assert!((1..=2).contains(&c.words.len()), "{}", c.name());
            assert!(!c.summary.is_empty(), "{}", c.name());
        }
    }

    #[test]
    fn deleting_and_ending_need_yes_and_nothing_else_does() {
        let catalog = Catalog::embedded();
        let destructive: Vec<String> = catalog.commands.iter().filter(|c| c.destructive()).map(Command::name).collect();
        assert_eq!(
            destructive,
            ["categories delete", "transactions delete", "budgets remove", "recurring delete", "recurring end", "documents delete", "ledger delete"]
        );
        assert!(!catalog.find(&["login"]).unwrap().destructive());
        assert!(!catalog.find(&["logout"]).unwrap().destructive());
    }

    #[test]
    fn the_commands_that_write_files_are_the_ones_the_person_must_allow() {
        let catalog = Catalog::embedded();
        let asking: Vec<String> = catalog.commands.iter().filter(|c| c.confirm).map(Command::name).collect();
        assert_eq!(asking, ["export json", "export csv", "export xlsx", "export pdf", "backup create", "documents get", "export debts"]);
        let verbs: Vec<&str> = catalog.verbs("export").iter().map(|c| c.verb()).collect();
        assert_eq!(verbs, ["preview", "json", "csv", "xlsx", "pdf", "debts"]);
        assert_eq!(catalog.verbs("backup").iter().map(|c| c.verb()).collect::<Vec<_>>(), ["schedule", "create"]);
        let out = catalog.find(&["export", "json"]).unwrap().params.iter().find(|p| p.name == "out").unwrap();
        assert!(out.required && out.kind == "path");
        assert!(catalog.find(&["export", "preview"]).is_some_and(|c| !c.confirm), "a preview needs no yes");
        assert!(catalog.find(&["backup", "schedule"]).is_some_and(|c| !c.confirm));
        assert!(catalog.commands.iter().all(|c| c.name() != "backup restore"), "restoring is done in the app");
    }

    #[test]
    fn a_broken_schema_is_refused() {
        assert!(Catalog::from_json("not json").is_err());
        assert!(Catalog::from_json(r#"{"api":2,"routes":[]}"#).is_err(), "another API version");
        assert!(Catalog::from_json(r#"{"api":1}"#).is_err());
        assert!(Catalog::from_json(r#"{"api":1,"routes":[{"name":"x"}]}"#).is_err());
    }

}
