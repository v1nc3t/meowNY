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
    /// `--kebab-case`, without the dashes.
    pub flag: String,
    /// string, integer, boolean, date, month, year, id, money or uuid.
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

    /// The `id` argument, which may be given without a flag.
    pub fn positional(&self) -> Option<&Param> {
        self.params.iter().find(|p| p.name == "id" && p.required)
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
    Command { words: words.iter().map(|w| w.to_string()).collect(), route: None, scope: None, summary: summary.into(), params: Vec::new(), action }
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
                params.push(Param {
                    flag: kebab(&pname),
                    name: pname,
                    kind: text(p, "type")?,
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
            // `schema` is the one route that is also answered here, without the app.
            let action = if name == "schema" { Action::Schema } else { Action::Route };
            commands.push(Command {
                words: name.split('.').map(str::to_string).collect(),
                route: Some(name),
                scope: Some(text(route, "scope")?),
                summary: text(route, "summary")?,
                params,
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
    fn flags_are_the_argument_names_in_kebab_case() {
        for (name, flag) in [("categoryId", "category-id"), ("paymentDate", "payment-date"), ("q", "q"), ("withinDays", "within-days"), ("clientUuid", "client-uuid"), ("amountLimit", "amount-limit")] {
            assert_eq!(kebab(name), flag);
        }
    }

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
    fn a_route_becomes_a_command_with_a_flag_per_argument() {
        let catalog = Catalog::embedded();
        let create = catalog.find(&["transactions", "create"]).unwrap();
        assert_eq!(create.route.as_deref(), Some("transactions.create"));
        assert_eq!(create.scope.as_deref(), Some("write"));
        let flags: Vec<&str> = create.params.iter().map(|p| p.flag.as_str()).collect();
        assert_eq!(flags, ["name", "amount", "payment-date", "category-id", "description", "client-uuid"]);
        let amount = create.params.iter().find(|p| p.name == "amount").unwrap();
        assert!(amount.required && amount.kind == "money" && !amount.nullable);
        assert_eq!(catalog.find(&["status"]).unwrap().words, ["status"]);
    }

    #[test]
    fn values_and_nullable_arguments_carry_over() {
        let catalog = Catalog::embedded();
        let side = catalog.find(&["categories", "list"]).unwrap().params.iter().find(|p| p.name == "type").unwrap();
        assert_eq!(side.values, ["INCOME", "EXPENSE"]);
        let update = catalog.find(&["transactions", "update"]).unwrap();
        assert!(update.params.iter().find(|p| p.name == "description").unwrap().nullable);
        assert!(!update.params.iter().find(|p| p.name == "name").unwrap().nullable);
    }

    #[test]
    fn deleting_and_ending_need_yes_and_nothing_else_does() {
        let catalog = Catalog::embedded();
        let destructive: Vec<String> = catalog.commands.iter().filter(|c| c.destructive()).map(Command::name).collect();
        assert_eq!(
            destructive,
            ["categories delete", "transactions delete", "budgets remove", "recurring delete", "recurring end"]
        );
        assert!(!catalog.find(&["login"]).unwrap().destructive());
        assert!(!catalog.find(&["logout"]).unwrap().destructive());
    }

    #[test]
    fn the_id_is_the_positional_argument_when_it_is_required() {
        let catalog = Catalog::embedded();
        assert_eq!(catalog.find(&["transactions", "delete"]).unwrap().positional().unwrap().name, "id");
        assert!(catalog.find(&["transactions", "list"]).unwrap().positional().is_none());
        assert!(catalog.find(&["stats", "trend"]).unwrap().positional().is_none(), "categoryId is not `id`");
    }

    #[test]
    fn local_commands_are_added_and_the_schema_route_is_answered_here() {
        let catalog = Catalog::embedded();
        assert_eq!(catalog.find(&["login"]).unwrap().action, Action::Login);
        assert_eq!(catalog.find(&["logout"]).unwrap().route, None);
        assert_eq!(catalog.find(&["schema"]).unwrap().action, Action::Schema);
        assert_eq!(catalog.find(&["status"]).unwrap().action, Action::Route);
    }

    #[test]
    fn nouns_group_their_verbs() {
        let catalog = Catalog::embedded();
        let nouns = catalog.nouns();
        for noun in ["settings", "categories", "transactions", "budgets", "recurring", "stats"] {
            assert!(nouns.contains(&noun), "{noun}");
        }
        assert!(!nouns.contains(&"status"));
        let verbs: Vec<&str> = catalog.verbs("categories").iter().map(|c| c.verb()).collect();
        assert_eq!(verbs, ["list", "create", "rename", "delete"]);
    }

    #[test]
    fn a_broken_schema_is_refused() {
        assert!(Catalog::from_json("not json").is_err());
        assert!(Catalog::from_json(r#"{"api":2,"routes":[]}"#).is_err(), "another API version");
        assert!(Catalog::from_json(r#"{"api":1}"#).is_err());
        assert!(Catalog::from_json(r#"{"api":1,"routes":[{"name":"x"}]}"#).is_err());
    }
}
