//! The route table. Every route the command API offers is one entry here: its name, the scope a
//! token needs, what it takes and what it does. The same table answers `schema`, so what the CLI
//! and the docs say about a route and what the server accepts cannot drift apart: the server
//! refuses any argument the entry does not list.
//!
//! Names are `noun.verb` (`transactions.create`), or one word for the API itself (`status`).

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::{ApiError, API_VERSION};
use crate::migrate;
use crate::tokens::{Grant, Scope};
use crate::vault::Vault;

/// What a handler gets to work with. The vault is the only way to data; it is locked or open as it
/// was when the request was checked.
pub struct Ctx<'a> {
    pub vault: &'a Vault,
    pub grant: Grant,
    pub routes: &'static [Route],
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    String,
    Integer,
    Boolean,
}

impl Kind {
    fn with_article(self) -> &'static str {
        match self {
            Kind::String => "a string",
            Kind::Integer => "an integer",
            Kind::Boolean => "a boolean",
        }
    }

    fn matches(self, value: &Value) -> bool {
        match self {
            Kind::String => value.is_string(),
            Kind::Integer => value.is_i64() || value.is_u64(),
            Kind::Boolean => value.is_boolean(),
        }
    }
}

/// One argument, a key of the JSON object in the request body.
pub struct Param {
    /// camelCase, as in the body.
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    pub summary: &'static str,
    /// The only values a string may have. Empty means any.
    pub values: &'static [&'static str],
}

pub struct Route {
    pub name: &'static str,
    /// The least scope a token needs.
    pub scope: Scope,
    pub summary: &'static str,
    pub params: &'static [Param],
    pub run: fn(&Ctx, Map<String, Value>) -> Result<Value, ApiError>,
}

impl Route {
    pub fn describe(&self) -> Value {
        let params: Vec<Value> = self
            .params
            .iter()
            .map(|p| {
                let mut out = json!({ "name": p.name, "type": p.kind, "required": p.required, "summary": p.summary });
                if !p.values.is_empty() {
                    out["values"] = json!(p.values);
                }
                out
            })
            .collect();
        json!({ "name": self.name, "scope": self.scope, "summary": self.summary, "params": params })
    }

    /// Checks a request body against the entry before the handler sees it: no unknown keys, nothing
    /// missing, every value of the right type and, where the entry lists them, an allowed value.
    pub fn check(&self, args: &Map<String, Value>) -> Result<(), ApiError> {
        if let Some(unknown) = args.keys().find(|key| !self.params.iter().any(|p| p.name == key.as_str())) {
            return Err(ApiError::invalid(format!("unknown argument `{}`", printable(unknown))));
        }
        for param in self.params {
            match args.get(param.name) {
                None if param.required => return Err(ApiError::invalid(format!("missing argument `{}`", param.name))),
                None => {}
                Some(value) if !param.kind.matches(value) => {
                    return Err(ApiError::invalid(format!("`{}` must be {}", param.name, param.kind.with_article())))
                }
                Some(Value::String(text)) if !param.values.is_empty() && !param.values.contains(&text.as_str()) => {
                    return Err(ApiError::invalid(format!("`{}` must be one of: {}", param.name, param.values.join(", "))))
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

/// A key from the request, cut short and without control characters, for an error message.
fn printable(key: &str) -> String {
    key.chars().filter(|c| !c.is_control()).take(40).collect()
}

pub const STATUS: Route = Route {
    name: "status",
    scope: Scope::Read,
    summary: "The app version, the API version and the scope of the token used.",
    params: &[],
    run: status,
};

fn status(ctx: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    Ok(json!({
        "app": "meowny",
        "version": env!("CARGO_PKG_VERSION"),
        "api": API_VERSION,
        "schemaVersion": migrate::latest_version(),
        "scope": ctx.grant.scope,
    }))
}

pub const SCHEMA: Route = Route {
    name: "schema",
    scope: Scope::Read,
    summary: "Every route with its scope and arguments, so a client can discover them.",
    params: &[],
    run: schema,
};

fn schema(ctx: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    Ok(json!({ "api": API_VERSION, "routes": ctx.routes.iter().map(Route::describe).collect::<Vec<_>>() }))
}

/// Everything the app serves.
pub static ALL: &[Route] = &[STATUS, SCHEMA];

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(_: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
        Ok(Value::Null)
    }

    const PARAMS: &[Param] = &[
        Param { name: "text", kind: Kind::String, required: true, summary: "A text.", values: &[] },
        Param { name: "count", kind: Kind::Integer, required: false, summary: "A number.", values: &[] },
        Param { name: "flag", kind: Kind::Boolean, required: false, summary: "A switch.", values: &[] },
        Param { name: "kind", kind: Kind::String, required: false, summary: "A or B.", values: &["A", "B"] },
    ];

    const ROUTE: Route = Route { name: "x.y", scope: Scope::Read, summary: "Test.", params: PARAMS, run: dummy };

    fn check(body: Value) -> Result<(), ApiError> {
        ROUTE.check(body.as_object().unwrap())
    }

    fn message(body: Value) -> String {
        check(body).unwrap_err().message.into_owned()
    }

    #[test]
    fn valid_arguments_pass() {
        assert!(check(json!({ "text": "a" })).is_ok());
        assert!(check(json!({ "text": "", "count": 0, "flag": false, "kind": "B" })).is_ok());
        assert!(check(json!({ "text": "a", "count": -5 })).is_ok());
        assert!(check(json!({ "text": "a", "count": u64::MAX })).is_ok());
    }

    #[test]
    fn missing_unknown_and_mistyped_arguments_are_refused_by_name() {
        assert_eq!(message(json!({})), "missing argument `text`");
        assert_eq!(message(json!({ "text": "a", "extra": 1 })), "unknown argument `extra`");
        assert_eq!(message(json!({ "text": 5 })), "`text` must be a string");
        assert_eq!(message(json!({ "text": "a", "count": "5" })), "`count` must be an integer");
        assert_eq!(message(json!({ "text": "a", "count": 1.5 })), "`count` must be an integer");
        assert_eq!(message(json!({ "text": "a", "flag": "true" })), "`flag` must be a boolean");
        assert_eq!(message(json!({ "text": "a", "kind": "C" })), "`kind` must be one of: A, B");
        // A value of the wrong type is a type error before it is a value error.
        assert_eq!(message(json!({ "text": "a", "kind": 1 })), "`kind` must be a string");
    }

    #[test]
    fn null_is_not_a_value() {
        assert_eq!(message(json!({ "text": null })), "`text` must be a string");
        assert_eq!(message(json!({ "text": "a", "count": null })), "`count` must be an integer");
    }

    #[test]
    fn case_matters_in_names_and_values() {
        assert_eq!(message(json!({ "Text": "a" })), "unknown argument `Text`");
        assert_eq!(message(json!({ "text": "a", "kind": "a" })), "`kind` must be one of: A, B");
    }

    #[test]
    fn an_odd_key_is_cut_and_cleaned_in_the_message() {
        let key = format!("bad\n{}", "k".repeat(500));
        let text = message(json!({ "text": "a", key: 1 }));
        assert!(!text.contains('\n') && text.len() < 100, "{text}");
    }

    #[test]
    fn the_table_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for route in ALL {
            assert!(seen.insert(route.name), "{} appears twice", route.name);
            let (noun, verb) = route.name.split_once('.').unwrap_or((route.name, "x"));
            for part in [noun, verb] {
                assert!(!part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase()), "bad route name {}", route.name);
            }
            assert!(!route.summary.is_empty() && route.summary.ends_with('.'), "{}", route.name);
            let mut names = std::collections::HashSet::new();
            for param in route.params {
                assert!(names.insert(param.name), "{}: {} twice", route.name, param.name);
                assert!(
                    param.name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                        && param.name.bytes().all(|b| b.is_ascii_alphanumeric()),
                    "{}: {} is not camelCase",
                    route.name,
                    param.name
                );
                assert!(!param.summary.is_empty(), "{}: {} has no summary", route.name, param.name);
                assert!(param.values.is_empty() || param.kind == Kind::String, "{}: {}", route.name, param.name);
            }
        }
    }

    #[test]
    fn a_route_describes_itself_for_the_schema() {
        let described = ROUTE.describe();
        assert_eq!(described["name"], "x.y");
        assert_eq!(described["scope"], "read");
        let params = described["params"].as_array().unwrap();
        assert_eq!(params.len(), 4);
        assert_eq!(params[0], json!({ "name": "text", "type": "string", "required": true, "summary": "A text." }));
        assert_eq!(params[3]["values"], json!(["A", "B"]));
        assert!(params[0].get("values").is_none(), "no empty list of values");
    }
}
