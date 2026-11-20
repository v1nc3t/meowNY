//! Turns the command line into one command and the JSON body for its route.
//!
//! Global flags can stand anywhere: `--json`, `--yes`, `--app`, `--token-file`, `--data-dir`,
//! `--port`, `--timeout`, `--help` and `--version`. The command is the first one or two plain words
//! (`status`, `transactions list`). After it come that command's flags as `--name value` or
//! `--name=value`, and for a command that takes an `id`, the id as a plain word. A lone `-` as the
//! value of a text flag reads that value from standard input. `--` ends the flags.
//!
//! Nothing here checks what a value means: dates, amounts and ids are checked by the app, which is
//! the one place that knows. Only what is needed to build the JSON is done here: whole numbers
//! become numbers, and a value that must be one of a few words is matched without regard to case.

use std::collections::{HashMap, HashSet};
use std::io::BufRead;

use serde_json::{Map, Value};

use crate::catalog::{Catalog, Command, Param};
use crate::error::CliError;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Globals {
    /// Print JSON (data on standard output, errors as `{code, message}` on standard error).
    pub json: bool,
    /// The person means it: required for commands that delete or end something.
    pub yes: bool,
    /// For `schema`: ask the running app instead of answering from this program.
    pub app: bool,
    pub token_file: Option<String>,
    pub data_dir: Option<String>,
    pub port: Option<u16>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug)]
pub enum Parsed {
    /// Show help for these words (empty for the top level).
    Help(Vec<String>),
    Version,
    Run(Box<Invocation>),
}

#[derive(Debug)]
pub struct Invocation {
    pub globals: Globals,
    pub command: Command,
    /// The request body: the arguments of the route, camelCase.
    pub body: Map<String, Value>,
}

struct Tok {
    text: String,
    /// After `--`: a plain word whatever it looks like.
    literal: bool,
}

/// The shown form of something the person typed, so an odd argument cannot garble the message.
fn shown(text: &str) -> String {
    let cut: String = text.chars().take(40).collect();
    format!("{cut:?}")
}

fn flag_value(name: &str, inline: Option<String>, args: &[String], i: &mut usize) -> Result<String, CliError> {
    if let Some(v) = inline {
        return Ok(v);
    }
    let value = args.get(*i).cloned().ok_or_else(|| CliError::usage(format!("{name} needs a value")))?;
    *i += 1;
    Ok(value)
}

fn switch(name: &str, inline: Option<String>) -> Result<bool, CliError> {
    match inline.as_deref() {
        None | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => Err(CliError::usage(format!("{name} takes no value, or true or false"))),
    }
}

/// Pulls the global flags out wherever they are and returns the rest in order.
fn split(args: &[String]) -> Result<(Globals, bool, bool, Vec<Tok>), CliError> {
    let (mut g, mut help, mut version) = (Globals::default(), false, false);
    let mut rest = Vec::new();
    let (mut i, mut literal) = (0, false);
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if literal {
            rest.push(Tok { text: arg.clone(), literal: true });
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        match name {
            "-h" | "--help" => help = switch(name, inline)?,
            "-V" | "--version" => version = switch(name, inline)?,
            "--json" => g.json = switch(name, inline)?,
            "--yes" => g.yes = switch(name, inline)?,
            "--app" => g.app = switch(name, inline)?,
            "--token-file" => g.token_file = Some(flag_value(name, inline, args, &mut i)?),
            "--data-dir" => g.data_dir = Some(flag_value(name, inline, args, &mut i)?),
            "--port" => {
                let v = flag_value(name, inline, args, &mut i)?;
                g.port = Some(v.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| CliError::usage("--port must be a port number"))?);
            }
            "--timeout" => {
                let v = flag_value(name, inline, args, &mut i)?;
                g.timeout_secs = Some(v.parse::<u64>().ok().filter(|s| (1..=3600).contains(s)).ok_or_else(|| CliError::usage("--timeout must be a number of seconds, 1 to 3600"))?);
            }
            _ => rest.push(Tok { text: arg.clone(), literal: false }),
        }
    }
    Ok((g, help, version, rest))
}

fn plain(tok: &Tok) -> bool {
    tok.literal || !tok.text.starts_with('-') || tok.text == "-"
}

/// The command and how many words it took.
fn resolve<'a>(catalog: &'a Catalog, words: &[&str]) -> Result<(&'a Command, usize), CliError> {
    let first = words.first().ok_or_else(|| CliError::usage("give a command"))?;
    if let Some(command) = catalog.find(&[first]) {
        return Ok((command, 1));
    }
    let verbs = catalog.verbs(first);
    if verbs.is_empty() {
        return Err(CliError::usage(format!("unknown command {}; run meowny --help", shown(first))));
    }
    if let Some(command) = words.get(1).and_then(|second| catalog.find(&[first, second])) {
        return Ok((command, 2));
    }
    let names: Vec<&str> = verbs.iter().map(|c| c.verb()).collect();
    Err(CliError::usage(format!("{first} needs one of: {}", names.join(", "))))
}

/// The value of a text flag given as `-`: all of standard input, without the last line break.
fn read_stdin(stdin: &mut dyn BufRead, used: &mut bool) -> Result<String, CliError> {
    if std::mem::replace(used, true) {
        return Err(CliError::usage("only one value can be read from standard input"));
    }
    let mut text = String::new();
    stdin.read_to_string(&mut text).map_err(|_| CliError::usage("standard input is not text"))?;
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    Ok(text)
}

fn hint(p: &Param) -> String {
    if p.values.is_empty() { p.kind.clone() } else { p.values.join(", ") }
}

/// A path the app can use: absolute, with `.` and `..` worked out, because the app does not know where this
/// program was started.
fn absolute(flag: &str, raw: &str) -> Result<String, CliError> {
    use std::path::{Component, Path, PathBuf};
    let path = Path::new(raw);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(|_| CliError::usage(format!("cannot tell the current folder; give {flag} as an absolute path")))?.join(path)
    };
    let mut out = PathBuf::new();
    for part in full.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_str().map(str::to_string).ok_or_else(|| CliError::usage(format!("{flag} must be text")))
}

/// `file_name` receives the name of a file that was read, for the argument that carries one.
fn convert(p: &Param, raw: String, stdin: &mut dyn BufRead, stdin_used: &mut bool, file_name: &mut Option<String>) -> Result<Value, CliError> {
    let flag = format!("--{}", p.flag);
    match p.kind.as_str() {
        "path" => absolute(&flag, &raw).map(Value::String),
        "bytes" => {
            let (data, name) = crate::file::load(&flag, &raw)?;
            *file_name = Some(name);
            Ok(Value::String(data))
        }
        "ids" => {
            let ids: Vec<&str> = raw.split(',').map(str::trim).collect();
            if ids.iter().any(|id| id.is_empty()) {
                return Err(CliError::usage(format!("{flag} must be ids separated by commas, like 3,7,12")));
            }
            Ok(Value::Array(ids.into_iter().map(|id| Value::String(id.to_string())).collect()))
        }
        "integer" => raw
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| CliError::usage(format!("{flag} must be a whole number"))),
        "boolean" => match raw.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(CliError::usage(format!("{flag} must be true or false"))),
        },
        _ if !p.values.is_empty() => p
            .values
            .iter()
            .find(|v| v.eq_ignore_ascii_case(&raw))
            .map(|v| Value::String(v.clone()))
            .ok_or_else(|| CliError::usage(format!("{flag} must be one of: {}", hint(p)))),
        "string" if raw == "-" => read_stdin(stdin, stdin_used).map(Value::String),
        _ => Ok(Value::String(raw)),
    }
}

fn fill(command: &Command, tokens: &[Tok], stdin: &mut dyn BufRead) -> Result<Map<String, Value>, CliError> {
    // Every flag the command knows: one per argument, and `clear-` for those that can be cleared.
    let mut flags: HashMap<String, (&Param, bool)> = HashMap::new();
    for p in &command.params {
        flags.insert(p.flag.clone(), (p, false));
        if p.nullable {
            flags.insert(format!("clear-{}", p.flag), (p, true));
        }
    }
    let (mut body, mut seen, mut stdin_used) = (Map::new(), HashSet::new(), false);
    let mut file_name: Option<String> = None;
    let mut positionals: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        i += 1;
        if plain(tok) {
            positionals.push(&tok.text);
            continue;
        }
        let Some(flag) = tok.text.strip_prefix("--") else {
            return Err(CliError::usage(format!("unknown flag {}; flags start with two dashes", shown(&tok.text))));
        };
        let (name, inline) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        let Some(&(param, clear)) = flags.get(name) else {
            return Err(CliError::usage(format!("{} has no flag --{}; run meowny {} --help", command.name(), name.chars().take(40).collect::<String>(), command.name())));
        };
        if !seen.insert(param.name.clone()) {
            return Err(CliError::usage(format!("--{} was given more than once", param.flag)));
        }
        let value = if clear {
            if inline.is_some() {
                return Err(CliError::usage(format!("--{name} takes no value")));
            }
            Value::Null
        } else if param.kind == "boolean" && inline.is_none() {
            Value::Bool(true)
        } else {
            let raw = match inline {
                Some(v) => v,
                None => {
                    let next = tokens.get(i).ok_or_else(|| CliError::usage(format!("--{name} needs a value")))?;
                    i += 1;
                    next.text.clone()
                }
            };
            convert(param, raw, stdin, &mut stdin_used, &mut file_name)?
        };
        body.insert(param.name.clone(), value);
    }

    match (command.positional(), positionals.as_slice()) {
        (_, []) => {}
        (Some(p), [one]) => {
            if body.contains_key(&p.name) {
                return Err(CliError::usage(format!("the {0} was given twice: as a plain word and as --{0}", p.flag)));
            }
            body.insert(p.name.clone(), convert(p, one.to_string(), stdin, &mut stdin_used, &mut file_name)?);
        }
        // The first plain word is the id when there is one; whatever comes after it is the unexpected one.
        (Some(_), [_, extra, ..]) => return Err(CliError::usage(format!("unexpected argument {}", shown(extra)))),
        (None, [first, ..]) => return Err(CliError::usage(format!("unexpected argument {}", shown(first)))),
    }
    // a file that was read gives its name to the document, unless the person chose another
    if let Some(name) = file_name {
        body.entry("name".to_string()).or_insert(Value::String(name));
    }
    for p in command.params.iter().filter(|p| p.required) {
        if !body.contains_key(&p.name) {
            return Err(CliError::usage(if command.positional().is_some_and(|q| q.name == p.name) {
                format!("give the {0}: meowny {1} <{0}>", p.flag, command.name())
            } else {
                format!("missing --{}", p.flag)
            }));
        }
    }
    Ok(body)
}

pub fn parse(args: &[String], catalog: &Catalog, stdin: &mut dyn BufRead) -> Result<Parsed, CliError> {
    let (globals, help, version, rest) = split(args)?;
    let words: Vec<&str> = rest.iter().take_while(|t| plain(t) && !t.literal).take(3).map(|t| t.text.as_str()).collect();

    // `meowny help transactions create` is `meowny transactions create --help`.
    if help || words.first() == Some(&"help") {
        let skip = usize::from(words.first() == Some(&"help"));
        return Ok(Parsed::Help(words.iter().skip(skip).take(2).map(|w| w.to_string()).collect()));
    }
    if version {
        return Ok(Parsed::Version);
    }
    let (command, used) = resolve(catalog, &words[..words.len().min(2)])?;
    if command.destructive() && !globals.yes {
        return Err(CliError::usage(format!("{} cannot be undone; add --yes to go ahead", command.name())));
    }
    let body = fill(command, &rest[used..], stdin)?;
    Ok(Parsed::Run(Box::new(Invocation { globals, command: command.clone(), body })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    fn parsed(line: &str) -> Result<Parsed, CliError> {
        parse(&args(line), &Catalog::embedded(), &mut std::io::empty())
    }

    fn run(line: &str) -> Invocation {
        match parsed(line) {
            Ok(Parsed::Run(i)) => *i,
            other => panic!("{line}: {other:?}"),
        }
    }

    fn usage(line: &str) -> String {
        match parsed(line) {
            Err(e) => {
                assert_eq!(e.code, "usage", "{line}");
                e.message
            }
            other => panic!("{line} should be a usage error: {other:?}"),
        }
    }

    #[test]
    fn whole_numbers_are_numbers_and_the_rest_stay_text() {
        let i = run("transactions list --limit 10 --offset 0 --category-id 12");
        assert_eq!(Value::Object(i.body), json!({ "limit": 10, "offset": 0, "categoryId": "12" }));
        assert!(usage("transactions list --limit ten").contains("--limit must be a whole number"));
        assert!(usage("transactions list --limit 1.5").contains("whole number"));
        assert!(usage("transactions list --limit").contains("--limit needs a value"));
        // A negative number is a number; whether it is allowed is the app's business.
        assert_eq!(Value::Object(run("transactions list --offset -3").body), json!({ "offset": -3 }));
    }

    #[test]
    fn words_that_must_be_one_of_a_few_are_matched_without_regard_to_case() {
        assert_eq!(Value::Object(run("categories list --type expense").body), json!({ "type": "EXPENSE" }));
        assert_eq!(Value::Object(run("categories list --type Income").body), json!({ "type": "INCOME" }));
        assert!(usage("categories list --type both").contains("--type must be one of: INCOME, EXPENSE"));
    }

    #[test]
    fn a_span_and_a_list_of_ids_become_what_the_app_takes() {
        let line = "budgets set --type expense --month 2026-10 --amount-limit 5.00 --span forward";
        assert_eq!(run(line).body["span"], json!("FORWARD"));
        assert!(usage("budgets set --type expense --month 2026-10 --amount-limit 5.00 --span some").contains("--span must be one of: ONLY, FORWARD, ALL"));
        assert_eq!(Value::Object(run("categories reorder --ids 3,7,12").body), json!({ "ids": ["3", "7", "12"] }));
        assert_eq!(Value::Object(run("recurring skip 5 --due-date 2026-12-20").body), json!({ "id": "5", "dueDate": "2026-12-20" }));
    }

    #[test]
    fn a_negative_amount_is_a_value_after_a_flag_with_or_without_an_equals_sign() {
        // The word after a flag is always its value, so `-120.00` is not read as a flag; `=` is the form that cannot be misread.
        for line in ["accounts create --name Card --kind credit_card --opening-balance=-120.00", "accounts create --name Card --kind credit_card --opening-balance -120.00"] {
            assert_eq!(Value::Object(run(line).body), json!({ "name": "Card", "kind": "CREDIT_CARD", "openingBalance": "-120.00" }), "{line}");
        }
        assert_eq!(Value::Object(run("accounts update 2 --opening-balance=-0.01 --clear-number").body), json!({ "id": "2", "openingBalance": "-0.01", "number": null }));
        // a kind is matched without regard to case, like the type of a category
        for kind in ["savings", "Savings", "SAVINGS", "Credit_Card"] {
            let body = run(&format!("accounts create --name X --kind {kind}")).body;
            assert_eq!(body["kind"], kind.to_uppercase().as_str());
        }
        assert!(usage("accounts create --name X --kind bank").contains("--kind must be one of: CHECKING, SAVINGS, CASH, CREDIT_CARD"));
        // the ids come as plain words and the lists as they do for a category
        assert_eq!(Value::Object(run("accounts show 2 --from 2026-11-01 --to 2026-11-30").body), json!({ "id": "2", "from": "2026-11-01", "to": "2026-11-30" }));
        assert_eq!(Value::Object(run("accounts reorder --ids 1,3,2").body), json!({ "ids": ["1", "3", "2"] }));
        assert_eq!(
            Value::Object(run("transfers create --from-account-id 1 --to-account-id 2 --amount 200.00 --date 2026-11-03").body),
            json!({ "fromAccountId": "1", "toAccountId": "2", "amount": "200.00", "date": "2026-11-03" })
        );
        assert_eq!(Value::Object(run("transactions create --name X --amount 1.00 --payment-date 2026-10-14 --category-id 5 --account-id 3").body)["accountId"], "3");
    }

    #[test]
    fn the_details_of_an_account_the_breakdown_and_the_kind_of_a_list_become_what_the_app_takes() {
        // a negative virtual balance is a value after a flag, with or without an equals sign, as an opening balance is
        for line in ["accounts create --name Card --kind cash --virtual-balance=-50.00", "accounts create --name Card --kind cash --virtual-balance -50.00"] {
            assert_eq!(Value::Object(run(line).body), json!({ "name": "Card", "kind": "CASH", "virtualBalance": "-50.00" }), "{line}");
        }
        // the switch is on when given, and off or on with an equals sign; the word after it is not its value
        assert_eq!(Value::Object(run("accounts update 2 --include-in-net-worth").body), json!({ "id": "2", "includeInNetWorth": true }));
        assert_eq!(Value::Object(run("accounts update 2 --include-in-net-worth=false").body), json!({ "id": "2", "includeInNetWorth": false }));
        assert_eq!(Value::Object(run("accounts create --name X --kind cash --include-in-net-worth=true").body)["includeInNetWorth"], true);
        assert!(usage("accounts update 2 --include-in-net-worth false").contains("unexpected argument \"false\""));
        assert!(usage("accounts update 2 --include-in-net-worth=no").contains("--include-in-net-worth must be true or false"));
        // the three texts that can be cleared have a clear flag on update and none on create
        assert_eq!(Value::Object(run("accounts update 2 --clear-bic --clear-notes --clear-group").body), json!({ "id": "2", "bic": null, "notes": null, "group": null }));
        assert!(usage("accounts create --name X --kind cash --clear-bic").contains("no flag --clear-bic"));
        assert!(usage("accounts update 2 --clear-virtual-balance").contains("no flag --clear-virtual-balance"));
        assert!(usage("accounts update 2 --bic X --clear-bic").contains("--bic was given more than once"));
        assert_eq!(Value::Object(run("accounts create --name X --kind cash --bic DEUTDEFF --notes Hi --group Bank").body), json!({ "name": "X", "kind": "CASH", "bic": "DEUTDEFF", "notes": "Hi", "group": "Bank" }));
        // a breakdown: the id as a plain word or a flag, and the days
        assert_eq!(Value::Object(run("accounts breakdown 2 --from 2026-11-01 --to 2026-11-30").body), json!({ "id": "2", "from": "2026-11-01", "to": "2026-11-30" }));
        assert_eq!(Value::Object(run("accounts breakdown --id 2").body), json!({ "id": "2" }));
        assert!(usage("accounts breakdown").contains("give the id: meowny accounts breakdown <id>"));
        // the kind of a list is matched without regard to case, like the type of a category
        for kind in ["income", "Income", "INCOME"] {
            assert_eq!(Value::Object(run(&format!("transactions list --kind {kind}")).body), json!({ "kind": "INCOME" }), "{kind}");
        }
        assert_eq!(Value::Object(run("transactions list --kind=Expense --limit 3").body), json!({ "kind": "EXPENSE", "limit": 3 }));
        assert!(usage("transactions list --kind nonsense").contains("--kind must be one of: INCOME, EXPENSE"));
    }

    #[test]
    fn the_currency_the_rate_the_balance_and_the_second_amounts_become_what_the_app_takes() {
        // a new account in another currency: the code and the rate are text, and the app says what it takes
        assert_eq!(Value::Object(run("accounts create --name Lei --kind checking --currency ron --rate 5.00").body), json!({ "name": "Lei", "kind": "CHECKING", "currency": "ron", "rate": "5.00" }));
        assert_eq!(Value::Object(run("accounts update 3 --currency RON --rate=4.97").body), json!({ "id": "3", "currency": "RON", "rate": "4.97" }));
        // a balance may be negative: a value after a flag, with or without an equals sign, as an opening balance is
        for line in ["accounts update 3 --balance=-50.00", "accounts update 3 --balance -50.00"] {
            assert_eq!(Value::Object(run(line).body), json!({ "id": "3", "balance": "-50.00" }), "{line}");
        }
        assert_eq!(Value::Object(run("accounts update 3 --balance 1662.90").body), json!({ "id": "3", "balance": "1662.90" }));
        // given with an opening balance it still reaches the app, which refuses the two together
        assert_eq!(Value::Object(run("accounts update 3 --balance 1.00 --opening-balance=-2.00").body), json!({ "id": "3", "balance": "1.00", "openingBalance": "-2.00" }));
        // none of them can be cleared, and a new account has no balance to set
        for flag in ["currency", "rate", "balance"] {
            assert!(usage(&format!("accounts update 3 --clear-{flag}")).contains(&format!("no flag --clear-{flag}")), "{flag}");
        }
        assert!(usage("accounts create --name X --kind cash --balance 1.00").contains("no flag --balance"));
        assert!(usage("accounts update 3 --rate 5.00 --rate 4.00").contains("--rate was given more than once"));
        assert!(usage("accounts update 3 --balance").contains("--balance needs a value"));
        // the second amount of a transaction and of a transfer are amounts, each for its own command
        let create = "transactions create --name X --amount 4.00 --payment-date 2026-10-14 --category-id 5 --account-id 6";
        assert_eq!(Value::Object(run(&format!("{create} --native-amount 20.00")).body), json!({ "name": "X", "amount": "4.00", "paymentDate": "2026-10-14", "categoryId": "5", "accountId": "6", "nativeAmount": "20.00" }));
        assert_eq!(Value::Object(run("transactions update 7 --native-amount=20.00 --account-id 6").body), json!({ "id": "7", "nativeAmount": "20.00", "accountId": "6" }));
        let moved = "--from-account-id 2 --to-account-id 6 --amount 40.00 --date 2026-10-14";
        assert_eq!(Value::Object(run(&format!("transfers create {moved} --to-amount 200.00")).body), json!({ "fromAccountId": "2", "toAccountId": "6", "amount": "40.00", "date": "2026-10-14", "toAmount": "200.00" }));
        assert_eq!(Value::Object(run("transfers update 3 --to-amount 180.00").body), json!({ "id": "3", "toAmount": "180.00" }));
        assert!(usage(&format!("transfers create {moved} --native-amount 1.00")).contains("no flag --native-amount"));
        assert!(usage(&format!("{create} --to-amount 1.00")).contains("no flag --to-amount"));
        assert!(usage("transactions update 7 --clear-native-amount").contains("no flag --clear-native-amount"));
        assert!(usage("transfers update 3 --clear-to-amount").contains("no flag --clear-to-amount"));
        assert!(usage("transactions update 7 --native-amount 1.00 --native-amount 2.00").contains("--native-amount was given more than once"));
        assert!(usage("transfers update 3 --to-amount").contains("--to-amount needs a value"));
    }

    #[test]
    fn missing_unknown_and_repeated_flags_are_usage_errors() {
        assert!(usage("transactions create --name X").contains("missing --amount"));
        assert!(usage("categories create --name Pets").contains("missing --type"));
        assert!(usage("status --nope").contains("status has no flag --nope"));
        assert!(usage("status -x").contains("unknown flag \"-x\""));
        assert!(usage("transactions list --limit 1 --limit 2").contains("--limit was given more than once"));
        assert!(usage("transactions list --category-id 1 --category-id=1").contains("more than once"));
        assert!(usage("transactions list --categoryId 1").contains("no flag --categoryId"), "flags are kebab-case");
    }

    #[test]
    fn deleting_and_ending_need_yes() {
        for line in ["transactions delete 5", "categories delete 5", "recurring delete 5", "recurring end 5", "budgets remove --type EXPENSE --month 2026-10", "accounts delete 5", "transfers delete 5", "stashes delete 5", "stashes erase 5", "stashes erase 5 --delete-transfer"] {
            assert!(usage(line).contains("cannot be undone; add --yes"), "{line}");
            let with = format!("{line} --yes");
            assert!(run(&with).globals.yes, "{with}");
        }
        // Changing something is not deleting it.
        run("transactions update 5 --name X");
        run("recurring pause 5");
        run("accounts archive 5");
        run("accounts update 5 --name X");
        run("stashes archive 5");
        run("stashes add 5 --amount 10.00");
        run("stashes amend 5 --note X");
    }

    #[test]
    fn two_dashes_end_the_flags() {
        // After `--` everything is a plain word, even if it starts with dashes.
        let i = run("transactions update --name Rye -- 42");
        assert_eq!(Value::Object(i.body), json!({ "id": "42", "name": "Rye" }));
        let i = run("transactions delete --yes -- -5");
        assert_eq!(i.body["id"], "-5", "the app will refuse it, not this program");
        assert!(usage("status -- --json").contains("unexpected argument"));
    }

    #[test]
    fn a_value_that_looks_like_a_flag_is_still_a_value() {
        let i = run("transactions list --q --weird");
        assert_eq!(i.body["q"], "--weird");
        let i = run("transactions list --q --limit");
        assert_eq!(i.body["q"], "--limit");
    }

    #[test]
    fn global_flag_values_are_checked() {
        assert!(usage("status --port 0").contains("--port must be a port number"));
        assert!(usage("status --port 70000").contains("--port"));
        assert!(usage("status --port abc").contains("--port"));
        assert!(usage("status --timeout 0").contains("--timeout"));
        assert!(usage("status --timeout 9999").contains("--timeout"));
        assert!(usage("status --token-file").contains("--token-file needs a value"));
        assert!(usage("status --json=maybe").contains("--json takes no value"));
    }

    #[test]
    fn a_path_is_made_absolute_and_tidied_because_the_app_does_not_know_where_this_started() {
        let cwd = std::env::current_dir().unwrap();
        let out = |line: &str| run(line).body["out"].as_str().unwrap().to_string();
        let cwd_text = |p: std::path::PathBuf| p.to_str().unwrap().to_string();
        assert_eq!(out("export json --out out.json"), cwd_text(cwd.join("out.json")));
        assert_eq!(out("export json --out ./sub/./out.json"), cwd_text(cwd.join("sub").join("out.json")));
        assert_eq!(out("export json --out ../up.json"), cwd_text(cwd.parent().unwrap().join("up.json")));
        assert_eq!(out("export json --out=sub/../out.json"), cwd_text(cwd.join("out.json")));
        #[cfg(unix)]
        {
            assert_eq!(out("export json --out /data/out.json"), "/data/out.json", "an absolute path stays");
            assert_eq!(out("export json --out /a/b/../c/./out.json"), "/a/c/out.json");
            assert_eq!(out("export json --out /../../out.json"), "/out.json", "there is nothing above the root");
        }
    }

    #[test]
    fn the_backup_passphrase_can_come_from_standard_input() {
        let line = ["backup", "create", "--out", "/x/a.meowbak", "--passphrase", "-"].map(String::from);
        let mut stdin: &[u8] = b"correct horse battery\n";
        let Ok(Parsed::Run(i)) = parse(&line, &Catalog::embedded(), &mut stdin) else { panic!() };
        assert_eq!(i.body["passphrase"], "correct horse battery");
        assert!(!i.command.destructive(), "the person's yes is asked in the app, not with --yes");
    }

    #[test]
    fn an_odd_argument_is_shown_cut_and_quoted() {
        let long = "x".repeat(300);
        let message = usage(&format!("status {long}"));
        assert!(message.len() < 100, "{message}");
        assert!(message.contains('"'));
    }

    // ---- a file to send -------------------------------------------------------------------------------

    fn parsed_in(dir: &std::path::Path, line: &str) -> Result<Parsed, CliError> {
        let mut words: Vec<String> = line.split_whitespace().map(String::from).collect();
        for word in &mut words {
            *word = word.replace("DIR", dir.to_str().unwrap());
        }
        parse(&words, &Catalog::embedded(), &mut std::io::empty())
    }

    #[test]
    fn a_file_that_cannot_be_sent_is_a_usage_error_before_anything_is_asked() {
        let dir = tempfile::tempdir().unwrap();
        let message = |line: &str| match parsed_in(dir.path(), line) {
            Err(e) => (e.code, e.message),
            Ok(_) => panic!("{line} was accepted"),
        };
        let (code, text) = message("documents add DIR/missing.pdf");
        assert_eq!(code, "usage");
        assert!(text.contains("there is no such file"), "{text}");
        assert!(message("documents add DIR").1.contains("is not a file"));
        let (code, text) = message("documents add");
        assert_eq!((code.as_str(), text.as_str()), ("usage", "give the file: meowny documents add <file>"));
        std::fs::write(dir.path().join("a.pdf"), b"x").unwrap();
        assert!(message("documents add DIR/a.pdf --file DIR/a.pdf").1.contains("given twice"));
    }

}
