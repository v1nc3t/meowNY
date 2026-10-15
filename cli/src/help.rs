//! The text of `--help` at every level: the whole program, one noun (`transactions`), one command.

use crate::catalog::{Action, Catalog, Command, Param};
use crate::error::{CliError, EXIT_CODES};

pub const GLOBAL_FLAGS: &[(&str, &str)] = &[
    ("--json", "Print JSON: the data on standard output, an error as {code, message} on standard error."),
    ("--yes", "Go ahead with a command that deletes or ends something."),
    ("--token-file <path>", "Read the token from this file. It must not be readable by other users."),
    ("--data-dir <path>", "The app's data folder, where it writes its port. Default: the app's usual folder."),
    ("--port <number>", "Connect to this port instead of reading it from the app's data folder."),
    ("--timeout <seconds>", "How long to wait for the app. Default 30."),
    ("--app", "For schema: ask the running app for its own list of routes."),
    ("--help, -h", "Show help. Also: meowny help <command>."),
    ("--version, -V", "Show the version."),
];

pub const ENVIRONMENT: &[(&str, &str)] = &[
    ("MEOWNY_TOKEN", "The access token, made in the app (Settings, command line access)."),
    ("MEOWNY_TOKEN_FILE", "A file holding the token. Same as --token-file."),
    ("MEOWNY_DATA_DIR", "The app's data folder. Same as --data-dir."),
    ("MEOWNY_PORT", "The port to connect to. Same as --port."),
];

/// What a value looks like in a usage line.
fn hint(p: &Param) -> String {
    if !p.values.is_empty() {
        return p.values.join("|");
    }
    match p.kind.as_str() {
        "string" => "text",
        "integer" => "number",
        "money" => "amount",
        other => other,
    }
    .to_string()
}

pub fn usage_line(c: &Command) -> String {
    let mut line = format!("meowny {}", c.name());
    if let Some(id) = c.positional() {
        line.push_str(&format!(" <{}>", id.name));
    }
    for p in &c.params {
        if c.positional().is_some_and(|q| q.name == p.name) {
            continue;
        }
        let part = format!("--{} <{}>", p.flag, hint(p));
        line.push_str(&if p.required { format!(" {part}") } else { format!(" [{part}]") });
    }
    line
}

fn two_columns(rows: &[(String, String)]) -> String {
    let width = rows.iter().map(|(a, _)| a.chars().count()).max().unwrap_or(0);
    rows.iter().map(|(a, b)| format!("  {a:width$}  {b}\n")).collect()
}

fn command_help(c: &Command) -> String {
    let mut out = format!("meowny {}: {}\n\nusage: {}\n", c.name(), c.summary, usage_line(c));
    if !c.params.is_empty() {
        let mut rows = Vec::new();
        for p in &c.params {
            let mut text = p.summary.clone();
            if p.required {
                text.push_str(" Required.");
            }
            rows.push((format!("--{} <{}>", p.flag, hint(p)), text));
            if p.nullable {
                rows.push((format!("--clear-{}", p.flag), format!("Clears it. Leaving --{} out leaves it as it is.", p.flag)));
            }
        }
        out.push_str("\nflags:\n");
        out.push_str(&two_columns(&rows));
    }
    let mut notes: Vec<String> = Vec::new();
    if let Some(id) = c.positional() {
        notes.push(format!("The {0} can be given as a plain word, without --{0}.", id.flag));
    }
    match (c.action, c.scope.as_deref()) {
        (Action::Login, _) => notes.push("Reads one line from standard input, so the token never appears on a command line: echo \"$TOKEN\" | meowny login. It is saved where --token-file or MEOWNY_TOKEN_FILE says, otherwise in your config folder, readable only by you.".into()),
        (Action::Logout, _) => notes.push("Removes the saved token, from the same place login saves it.".into()),
        (Action::Schema, _) => notes.push("Prints every command, flag, exit code and error code as JSON, without contacting the app. With --app it asks the running app for its own list of routes instead.".into()),
        (_, Some("write")) => notes.push("Needs a token that can write.".into()),
        (_, Some("read")) => notes.push("Works with a read-only token.".into()),
        _ => {}
    }
    if c.destructive() {
        notes.push("This cannot be undone, so it needs --yes.".into());
    }
    if c.name() == "transactions create" {
        notes.push("A fresh --client-uuid is made if you do not give one, and is in the answer. To retry safely after an error or a timeout, send the same one again: the app returns the first transaction instead of making a second.".into());
    }
    notes.push("Every command also takes the global flags; see meowny --help.".into());
    out.push('\n');
    for note in notes {
        out.push_str(&format!("{note}\n"));
    }
    out
}

fn noun_help(catalog: &Catalog, noun: &str) -> String {
    let rows: Vec<(String, String)> = catalog.verbs(noun).iter().map(|c| (c.verb().to_string(), c.summary.clone())).collect();
    format!("meowny {noun}\n\ncommands:\n{}\nRun meowny {noun} <command> --help for the flags of one.\n", two_columns(&rows))
}

fn top_help(catalog: &Catalog) -> String {
    let mut rows: Vec<(String, String)> = Vec::new();
    for c in catalog.commands.iter().filter(|c| c.words.len() == 1) {
        rows.push((c.name(), c.summary.clone()));
    }
    for noun in catalog.nouns() {
        let verbs: Vec<&str> = catalog.verbs(noun).iter().map(|c| c.verb()).collect();
        rows.push((noun.to_string(), verbs.join(", ")));
    }
    let globals: Vec<(String, String)> = GLOBAL_FLAGS.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let env: Vec<(String, String)> = ENVIRONMENT.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let exits: Vec<(String, String)> = EXIT_CODES.iter().map(|(c, t)| (c.to_string(), t.to_string())).collect();
    format!(
        "meowny: the command line for the meowNY desktop app\n\n\
         usage: meowny <command> [flags]\n       meowny <command> --help\n\n\
         The app has to be running and unlocked, with \"command line access\" turned on in Settings.\n\
         Make a token there and give it to this program (see environment below, or: meowny login).\n\n\
         commands:\n{}\nglobal flags:\n{}\nenvironment:\n{}\nexit codes:\n{}",
        two_columns(&rows),
        two_columns(&globals),
        two_columns(&env),
        two_columns(&exits),
    )
}

/// The help text for these words: none for the whole program, a noun, or a command.
pub fn render(catalog: &Catalog, words: &[String]) -> Result<String, CliError> {
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    match refs.as_slice() {
        [] => Ok(top_help(catalog)),
        [one] => {
            if let Some(c) = catalog.find(&[one]) {
                Ok(command_help(c))
            } else if !catalog.verbs(one).is_empty() {
                Ok(noun_help(catalog, one))
            } else {
                Err(CliError::usage(format!("unknown command {one:?}; run meowny --help")))
            }
        }
        [noun, verb, ..] => catalog
            .find(&[noun, verb])
            .map(command_help)
            .ok_or_else(|| CliError::usage(format!("unknown command {:?}; run meowny {noun} --help", format!("{noun} {verb}")))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn help(words: &[&str]) -> String {
        render(&Catalog::embedded(), &words.iter().map(|w| w.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_usage_line_shows_required_flags_bare_and_optional_ones_in_brackets() {
        let catalog = Catalog::embedded();
        assert_eq!(
            usage_line(catalog.find(&["transactions", "create"]).unwrap()),
            "meowny transactions create --name <text> --amount <amount> --payment-date <date> --category-id <id> [--description <text>] [--client-uuid <uuid>]"
        );
        assert_eq!(usage_line(catalog.find(&["transactions", "delete"]).unwrap()), "meowny transactions delete <id>");
        assert_eq!(usage_line(catalog.find(&["categories", "list"]).unwrap()), "meowny categories list [--type <INCOME|EXPENSE>]");
        assert_eq!(usage_line(catalog.find(&["status"]).unwrap()), "meowny status");
    }

    #[test]
    fn the_top_level_names_every_command_and_the_ways_to_give_a_token() {
        let text = help(&[]);
        for word in ["status", "today", "schema", "login", "logout", "transactions", "list, create, update, delete", "--json", "MEOWNY_TOKEN", "meowny login", "exit codes", "Settings"] {
            assert!(text.contains(word), "{word}");
        }
        for (code, _) in EXIT_CODES {
            assert!(text.contains(&format!("  {code}  ")), "exit code {code}");
        }
        assert!(!text.contains("--token "), "there is no --token flag, on purpose");
    }

    #[test]
    fn a_noun_lists_its_commands_with_what_they_do() {
        let text = help(&["budgets"]);
        assert!(text.contains("overview") && text.contains("history") && text.contains("set") && text.contains("remove"));
        assert!(text.contains("meowny budgets <command> --help"));
    }

    #[test]
    fn a_command_shows_its_usage_flags_scope_and_warnings() {
        let text = help(&["transactions", "create"]);
        assert!(text.contains("usage: meowny transactions create --name <text>"));
        assert!(text.contains("--amount <amount>") && text.contains("Required."));
        assert!(text.contains("Needs a token that can write."));
        assert!(text.contains("--client-uuid") && text.contains("retry safely"));
        let del = help(&["transactions", "delete"]);
        assert!(del.contains("needs --yes") && del.contains("plain word"));
        assert!(help(&["transactions", "list"]).contains("Works with a read-only token."));
        let update = help(&["transactions", "update"]);
        assert!(update.contains("--clear-description") && update.contains("Clears it."));
        assert!(!update.contains("--clear-name"));
    }

    #[test]
    fn a_single_word_command_and_the_local_ones_have_help_too() {
        assert!(help(&["status"]).contains("usage: meowny status"));
        let login = help(&["login"]);
        assert!(login.contains("standard input") && login.contains("never appears on a command line"));
        assert!(help(&["logout"]).contains("Removes the saved token"));
        assert!(help(&["schema"]).contains("without contacting the app"));
    }

    #[test]
    fn help_for_something_unknown_is_a_usage_error() {
        let catalog = Catalog::embedded();
        for words in [vec!["nope"], vec!["transactions", "nope"], vec!["nope", "list"]] {
            let e = render(&catalog, &words.iter().map(|w| w.to_string()).collect::<Vec<_>>()).unwrap_err();
            assert_eq!(e.code, "usage", "{words:?}");
        }
    }

    #[test]
    fn every_command_has_help_that_mentions_every_one_of_its_flags() {
        let catalog = Catalog::embedded();
        for c in &catalog.commands {
            let text = render(&catalog, &c.words).unwrap();
            assert!(text.contains(&usage_line(c)), "{}", c.name());
            assert!(text.contains(&c.summary), "{}", c.name());
            for p in &c.params {
                assert!(text.contains(&format!("--{}", p.flag)) || c.positional().is_some_and(|q| q.name == p.name), "{} --{}", c.name(), p.flag);
                assert!(text.contains(&p.summary), "{} {}", c.name(), p.name);
            }
        }
    }
}
