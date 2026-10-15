//! `meowny`: the command line for the meowNY desktop app. It is a client of the running app's local
//! command API and never opens the data file. See `meowny --help`.

mod args;
mod catalog;
mod client;
mod error;
mod help;
mod local;
mod output;
mod run;

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Result<Vec<String>, _> = std::env::args_os().skip(1).map(|a| a.into_string()).collect();
    let Ok(args) = args else {
        eprintln!("error: arguments must be valid UTF-8");
        return ExitCode::from(2);
    };
    let stdin = std::io::stdin();
    if args.iter().any(|a| a == "login") && stdin.is_terminal() {
        eprintln!("paste the token, then press Enter:");
    }
    let (mut input, mut out, mut err) = (stdin.lock(), std::io::stdout().lock(), std::io::stderr().lock());
    let env = |name: &str| std::env::var(name).ok();
    let code = run::run(&args, &env, &mut input, &mut out, &mut err);
    let _ = out.flush();
    ExitCode::from(code)
}
