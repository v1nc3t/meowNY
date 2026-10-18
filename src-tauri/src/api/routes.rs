//! The route table. Every route the command API offers is one entry here: its name, the scope a
//! token needs, what it takes and what it does. The same table answers `schema`, so what the CLI
//! and the docs say about a route and what the server accepts cannot drift apart: the server
//! refuses any argument the entry does not list.
//!
//! Names are `noun.verb` (`transactions.create`), or one word for the API itself (`status`).

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::{confirmed, debts, documents, read, write, Approval, ApiError, API_VERSION};
use crate::clock;
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

/// What an argument is. `date`, `month`, `year` and `id` are JSON strings with a fixed shape that the server
/// checks before the route runs; whether the date really exists is the route's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    String,
    Integer,
    Boolean,
    /// "2026-10-14"
    Date,
    /// "2026-10"
    Month,
    /// "2026"
    Year,
    /// The id of a category, a transaction and so on, as the app lists it: digits, no leading zero.
    Id,
    /// An amount of money as text with two decimals, "12.50". Never a JSON number, so nothing rounds it.
    Money,
    /// "123e4567-e89b-12d3-a456-426614174000"
    Uuid,
    /// A list of ids, as a JSON array of strings.
    Ids,
    /// A file path as text: absolute, up to 400 characters, no control or direction-changing characters. The
    /// route says what else it must be (an extension, a folder that exists, a file that does not).
    Path,
    /// The bytes of a file, as base64url text (letters, digits, `-` and `_`, no padding). Only a route that takes a file has one,
    /// and its request may be bigger than the others (`body_limit`).
    Bytes,
}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `text` is `pattern` with a digit wherever the pattern has a `9`. Works on bytes, so odd text cannot panic.
fn fits(text: &str, pattern: &str) -> bool {
    text.len() == pattern.len()
        && text.bytes().zip(pattern.bytes()).all(|(t, p)| if p == b'9' { t.is_ascii_digit() } else { t == p })
}

/// Whole part of 1 to 10 digits (no leading zero unless it is just "0"), a point, exactly two decimals.
fn is_money(text: &str) -> bool {
    let Some((whole, fraction)) = text.split_once('.') else { return false };
    (1..=10).contains(&whole.len())
        && all_digits(whole)
        && !(whole.len() > 1 && whole.starts_with('0'))
        && fraction.len() == 2
        && all_digits(fraction)
}

pub(super) use crate::tokens::is_plain_text;

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text
            .bytes()
            .enumerate()
            .all(|(i, b)| if matches!(i, 8 | 13 | 18 | 23) { b == b'-' } else { b.is_ascii_hexdigit() })
}

fn is_id(text: &str) -> bool {
    (1..=18).contains(&text.len()) && !text.starts_with('0') && all_digits(text)
}

impl Kind {
    fn with_article(self) -> &'static str {
        match self {
            Kind::String => "a string",
            Kind::Integer => "an integer",
            Kind::Boolean => "a boolean",
            Kind::Date => "a date like 2026-10-14",
            Kind::Month => "a month like 2026-10",
            Kind::Year => "a year like 2026",
            Kind::Id => "an id as the app lists it",
            Kind::Money => "an amount as text with two decimals, like \"12.50\"",
            Kind::Uuid => "a UUID like 123e4567-e89b-12d3-a456-426614174000",
            Kind::Ids => "a list of ids as the app lists them",
            Kind::Path => "a file path",
            Kind::Bytes => "the bytes of a file as base64url text",
        }
    }

    fn matches(self, value: &Value) -> bool {
        match (self, value) {
            (Kind::String, Value::String(_)) => true,
            (Kind::Integer, v) => v.is_i64() || v.is_u64(),
            (Kind::Boolean, Value::Bool(_)) => true,
            (Kind::Date, Value::String(text)) => fits(text, "9999-99-99"),
            (Kind::Month, Value::String(text)) => fits(text, "9999-99"),
            (Kind::Year, Value::String(text)) => fits(text, "9999"),
            (Kind::Id, Value::String(text)) => is_id(text),
            (Kind::Money, Value::String(text)) => is_money(text),
            (Kind::Uuid, Value::String(text)) => is_uuid(text),
            (Kind::Ids, Value::Array(items)) => items.len() <= 500 && items.iter().all(|i| i.as_str().is_some_and(is_id)),
            (Kind::Path, Value::String(text)) => (1..=400).contains(&text.chars().count()) && is_plain_text(text),
            (Kind::Bytes, Value::String(text)) => !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            _ => false,
        }
    }
}

/// One argument, a key of the JSON object in the request body.
pub struct Param {
    /// camelCase, as in the body.
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    /// `null` is a value of its own that means "clear this" (or "none"). Without it, `null` is refused.
    pub nullable: bool,
    pub summary: &'static str,
    /// The only values a string may have. Empty means any.
    pub values: &'static [&'static str],
}

impl Param {
    pub const fn new(name: &'static str, kind: Kind, summary: &'static str) -> Self {
        Self { name, kind, required: false, nullable: false, summary, values: &[] }
    }

    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub const fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub const fn one_of(mut self, values: &'static [&'static str]) -> Self {
        self.values = values;
        self
    }
}

/// For a route that writes outside the app, or hands data out in bulk: builds what the person is shown,
/// from the request, before anything is done. It also checks the request, so a bad one never asks.
pub type Confirm = fn(&Ctx, &Map<String, Value>) -> Result<Approval, ApiError>;

pub struct Route {
    pub name: &'static str,
    /// The least scope a token needs.
    pub scope: Scope,
    pub summary: &'static str,
    pub params: &'static [Param],
    /// If set, the person has to allow each request in the app before the route runs. A token alone is not enough.
    pub confirm: Option<Confirm>,
    pub run: fn(&Ctx, Map<String, Value>) -> Result<Value, ApiError>,
}

impl Route {
    pub fn describe(&self) -> Value {
        let params: Vec<Value> = self
            .params
            .iter()
            .map(|p| {
                let mut out = json!({ "name": p.name, "type": p.kind, "required": p.required, "summary": p.summary });
                if p.nullable {
                    out["nullable"] = json!(true);
                }
                if !p.values.is_empty() {
                    out["values"] = json!(p.values);
                }
                out
            })
            .collect();
        let mut out = json!({ "name": self.name, "scope": self.scope, "summary": self.summary, "params": params });
        if self.confirm.is_some() {
            out["confirm"] = json!(true);
        }
        out
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
                Some(Value::Null) if param.nullable => {}
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

/// The request body as the route's own input type. A value that does not fit says what was wrong with it
/// (the message quotes only what the caller sent).
/// The most a request to this route may hold in its body. Every route takes at most 1 MiB, except the one that takes a file.
pub fn body_limit(route: &str) -> usize {
    match route {
        "documents.add" => super::http::MAX_UPLOAD_BODY,
        _ => super::http::MAX_BODY,
    }
}

pub fn parse<T: DeserializeOwned>(args: Map<String, Value>) -> Result<T, ApiError> {
    serde_json::from_value(Value::Object(args)).map_err(|e| ApiError::invalid(e.to_string()))
}

pub fn reply<T: Serialize>(value: T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| ApiError::INTERNAL)
}

/// Removes the id argument and returns it as a number, so what is left can be read as the other fields.
pub fn take_id(args: &mut Map<String, Value>, name: &str) -> Result<i64, ApiError> {
    let text = args.remove(name).and_then(|v| v.as_str().map(str::to_string)).ok_or(ApiError::INVALID_INPUT)?;
    Ok(crate::categories::parse_id(&text)?)
}

/// Today in the time zone set in the app, as "YYYY-MM-DD". Due dates and "overdue" follow it, never the computer's zone.
pub fn today(ctx: &Ctx) -> Result<String, ApiError> {
    let timezone = ctx.vault.settings()?.timezone;
    Ok(clock::today(ctx.now, &timezone)?.to_string())
}

pub const STATUS: Route = Route {
    name: "status",
    scope: Scope::Read,
    summary: "The app version, the API version and the scope of the token used.",
    params: &[],
    confirm: None,
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
    confirm: None,
    run: schema,
};

fn schema(ctx: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
    Ok(json!({ "api": API_VERSION, "routes": ctx.routes.iter().map(Route::describe).collect::<Vec<_>>() }))
}

/// Everything the app serves.
pub static ALL: &[Route] = &[
    STATUS,
    SCHEMA,
    read::TODAY,
    read::SETTINGS_GET,
    read::CATEGORIES_LIST,
    read::TRANSACTIONS_LIST,
    read::BUDGETS_OVERVIEW,
    read::BUDGETS_HISTORY,
    read::RECURRING_LIST,
    read::RECURRING_DUES,
    read::STATS_BUDGET,
    read::STATS_TREND,
    read::STATS_YEAR,
    read::STATS_TOP,
    read::STATS_COMMITMENTS,
    read::STATS_PLANNED,
    write::SETTINGS_SET,
    write::CATEGORIES_CREATE,
    write::CATEGORIES_RENAME,
    write::CATEGORIES_DELETE,
    write::TRANSACTIONS_CREATE,
    write::TRANSACTIONS_UPDATE,
    write::TRANSACTIONS_DELETE,
    write::BUDGETS_SET,
    write::BUDGETS_REMOVE,
    write::RECURRING_CREATE,
    write::RECURRING_UPDATE,
    write::RECURRING_DELETE,
    write::RECURRING_PAY,
    write::RECURRING_SKIP,
    write::RECURRING_PAUSE,
    write::RECURRING_RESUME,
    write::RECURRING_END,
    read::EXPORT_PREVIEW,
    read::BACKUP_SCHEDULE,
    confirmed::EXPORT_JSON,
    confirmed::EXPORT_CSV,
    confirmed::EXPORT_XLSX,
    confirmed::EXPORT_PDF,
    confirmed::BACKUP_CREATE,
    documents::LIST,
    documents::SHOW,
    documents::ADD,
    documents::UPDATE,
    documents::DELETE,
    documents::LINK,
    documents::UNLINK,
    documents::GET,
    debts::PEOPLE_LIST,
    debts::PEOPLE_SHOW,
    debts::DEBTS_SUMMARY,
    debts::DEBTS_DUE,
    debts::DEBTS_HISTORY,
    debts::DEBTS_REMINDERS,
    debts::SPLITS_PREVIEW,
    debts::PEOPLE_CREATE,
    debts::PEOPLE_UPDATE,
    debts::PEOPLE_ARCHIVE,
    debts::PEOPLE_UNARCHIVE,
    debts::PEOPLE_SETTLE,
    debts::LEDGER_ADD,
    debts::LEDGER_UPDATE,
    debts::LEDGER_DELETE,
    debts::TRANSACTIONS_SPLIT,
    debts::TRANSACTIONS_UNSPLIT,
    debts::RECURRING_SPLIT,
    debts::RECURRING_UNSPLIT,
    debts::EXPORT_DEBTS,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(_: &Ctx, _: Map<String, Value>) -> Result<Value, ApiError> {
        Ok(Value::Null)
    }

    const PARAMS: &[Param] = &[
        Param { name: "text", kind: Kind::String, required: true, nullable: false, summary: "A text.", values: &[] },
        Param { name: "count", kind: Kind::Integer, required: false, nullable: false, summary: "A number.", values: &[] },
        Param { name: "flag", kind: Kind::Boolean, required: false, nullable: false, summary: "A switch.", values: &[] },
        Param { name: "kind", kind: Kind::String, required: false, nullable: false, summary: "A or B.", values: &["A", "B"] },
    ];

    const ROUTE: Route = Route { name: "x.y", scope: Scope::Read, summary: "Test.", params: PARAMS, confirm: None, run: dummy };

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
    fn dates_months_years_and_ids_have_a_fixed_shape() {
        let accepts = |kind: Kind, text: &str| kind.matches(&json!(text));
        for ok in ["2026-10-14", "0000-00-00", "9999-99-99"] {
            assert!(accepts(Kind::Date, ok), "{ok}");
        }
        for bad in ["2026-10-1", "2026-1-01", "20261014", "2026/10/14", "2026-10-14 ", " 2026-10-14", "2026-10-14T00:00:00Z", "2026-10", "", "é026-10-14", "2026-10-1é", "２０２６-10-14"] {
            assert!(!accepts(Kind::Date, bad), "{bad:?}");
        }
        assert!(accepts(Kind::Month, "2026-10") && accepts(Kind::Month, "2026-99"), "whether the month exists is for the route");
        for bad in ["2026-1", "2026-100", "202610", "2026-10-14", "2026-1\n", "", "é026-10", "2026-1é"] {
            assert!(!accepts(Kind::Month, bad), "{bad:?}");
        }
        assert!(accepts(Kind::Year, "2026") && accepts(Kind::Year, "0000"));
        for bad in ["26", "20266", "abcd", "", "٢٠٢٦", "202é"] {
            assert!(!accepts(Kind::Year, bad), "{bad:?}");
        }
        for ok in ["1", "42", "999999999999999999"] {
            assert!(accepts(Kind::Id, ok), "{ok}");
        }
        for bad in ["0", "01", "-1", "+1", "1.0", "", " 1", "1 ", "1e3", "٣", &"1".repeat(19)] {
            assert!(!accepts(Kind::Id, bad), "{bad:?}");
        }
        // Only a string can be any of these.
        for kind in [Kind::Date, Kind::Month, Kind::Year, Kind::Id, Kind::String] {
            for value in [json!(20261014), json!(null), json!(true), json!(["2026"]), json!({})] {
                assert!(!kind.matches(&value), "{kind:?} accepted {value}");
            }
        }
    }

    #[test]
    fn the_kinds_have_the_names_a_client_reads_in_the_schema() {
        let names: Vec<Value> = [Kind::String, Kind::Integer, Kind::Boolean, Kind::Date, Kind::Month, Kind::Year, Kind::Id, Kind::Money, Kind::Uuid]
            .iter()
            .map(|k| json!(k))
            .collect();
        assert_eq!(Value::Array(names), json!(["string", "integer", "boolean", "date", "month", "year", "id", "money", "uuid"]));
    }

    #[test]
    fn money_is_text_with_two_decimals_and_never_a_number() {
        let accepts = |text: &str| Kind::Money.matches(&json!(text));
        for ok in ["0.00", "0.01", "12.50", "1000.00", "9999999999.99", "1234567890.00"] {
            assert!(accepts(ok), "{ok}");
        }
        for bad in [
            "", ".", "1", "1.", "1.5", "1.234", ".50", "01.00", "00.00", "-1.00", "+1.00", "1,00", "1 .00", " 1.00", "1.00 ", "1e3", "1.0e1",
            "12345678901.00", "１２.５０", "1.5٠", "1.00\n", "NaN", "Infinity", "0x10.00",
        ] {
            assert!(!accepts(bad), "{bad:?}");
        }
        for value in [json!(12.5), json!(12), json!(null), json!(true), json!(["12.50"])] {
            assert!(!Kind::Money.matches(&value), "{value}");
        }
    }

    #[test]
    fn a_uuid_has_five_groups_of_hex() {
        let accepts = |text: &str| Kind::Uuid.matches(&json!(text));
        assert!(accepts("123e4567-e89b-12d3-a456-426614174000") && accepts("123E4567-E89B-12D3-A456-426614174000"));
        for bad in [
            "", "123e4567e89b12d3a456426614174000", "123e4567-e89b-12d3-a456-42661417400", "123e4567-e89b-12d3-a456-4266141740000",
            "123e4567-e89b-12d3-a456-42661417400g", "123e4567_e89b_12d3_a456_426614174000", "{123e4567-e89b-12d3-a456-426614174000}",
            "123e4567-e89b-12d3-a456-42661417400é", " 123e4567-e89b-12d3-a456-426614174000",
        ] {
            assert!(!accepts(bad), "{bad:?}");
        }
    }

    #[test]
    fn null_is_accepted_only_where_a_parameter_says_so() {
        const ROUTE: Route = Route {
            name: "x.clear",
            scope: Scope::Write,
            summary: "Test.",
            params: &[
                Param::new("note", Kind::String, "A note, or null to clear it.").nullable(),
                Param::new("when", Kind::Date, "A date, or null for none.").nullable(),
                Param::new("plain", Kind::String, "A text."),
                Param::new("must", Kind::Integer, "A number.").required().nullable(),
            ],
            confirm: None,
            run: dummy,
        };
        let check = |body: Value| ROUTE.check(body.as_object().unwrap());
        assert!(check(json!({ "must": 1 })).is_ok());
        assert!(check(json!({ "must": null, "note": null, "when": null })).is_ok(), "null clears");
        assert!(check(json!({ "must": 1, "note": "a", "when": "2026-10-14" })).is_ok(), "and a value still works");
        assert_eq!(check(json!({ "note": null })).unwrap_err().message, "missing argument `must`", "a nullable argument can still be required");
        assert_eq!(check(json!({ "must": 1, "plain": null })).unwrap_err().message, "`plain` must be a string");
        // Null does not excuse a wrong value of the right key.
        assert_eq!(check(json!({ "must": 1, "when": "tomorrow" })).unwrap_err().message, "`when` must be a date like 2026-10-14");
        assert_eq!(check(json!({ "must": 1, "note": 5 })).unwrap_err().message, "`note` must be a string");
        // The schema says which arguments take null.
        let described = ROUTE.describe();
        let params = described["params"].as_array().unwrap();
        assert_eq!(params[0]["nullable"], true);
        assert_eq!(params[2].get("nullable"), None, "absent unless true");
    }

    #[test]
    fn a_parameter_is_built_in_steps_and_is_optional_until_said_otherwise() {
        const P: Param = Param::new("type", Kind::String, "A or B.");
        assert!(!P.required && P.values.is_empty());
        const Q: Param = P.required().one_of(&["A", "B"]);
        assert!(Q.required && Q.values == ["A", "B"]);
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
