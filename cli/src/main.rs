//! `meowny`: the command line for the meowNY desktop app. It is a client of the running app's local
//! command API and never opens the data file. See `meowny --help`.

mod args;
mod catalog;
mod client;
mod error;
mod file;
mod help;
mod local;
mod output;
mod run;
#[cfg(test)]
mod robust_tests;

use std::io::{IsTerminal, Write};
use std::panic::UnwindSafe;
use std::process::ExitCode;

/// What a panic prints. A panic message can quote what the program was handling (an argument, a line of a
/// token file), so only where it happened is shown.
fn stopped(location: Option<(&str, u32)>) -> String {
    match location {
        Some((file, line)) => format!("error: meowny stopped unexpectedly ({file}:{line})"),
        None => "error: meowny stopped unexpectedly".to_string(),
    }
}

/// Runs `f`; a panic becomes exit code 1 instead of Rust's 101.
fn guarded(f: impl FnOnce() -> u8 + UnwindSafe) -> u8 {
    std::panic::catch_unwind(f).unwrap_or(1)
}

fn real_main() -> u8 {
    let args: Result<Vec<String>, _> = std::env::args_os().skip(1).map(|a| a.into_string()).collect();
    let Ok(args) = args else {
        eprintln!("error: arguments must be valid UTF-8");
        return 2;
    };
    let stdin = std::io::stdin();
    if args.iter().any(|a| a == "login") && stdin.is_terminal() {
        eprintln!("paste the token, then press Enter:");
    }
    let (mut input, mut out, mut err) = (stdin.lock(), std::io::stdout().lock(), std::io::stderr().lock());
    let env = |name: &str| std::env::var(name).ok();
    let code = run::run(&args, &env, &mut input, &mut out, &mut err);
    let _ = out.flush();
    code
}

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|info| eprintln!("{}", stopped(info.location().map(|l| (l.file(), l.line()))))));
    ExitCode::from(guarded(real_main))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_exit_1_and_the_message_is_not_printed() {
        assert_eq!(guarded(|| 7), 7);
        // The default hook would print this text; the real one prints only the place.
        std::panic::set_hook(Box::new(|_| {}));
        assert_eq!(guarded(|| panic!("secret mny_0123 in the message")), 1);
        let _ = std::panic::take_hook();
        let shown = stopped(Some(("src/args.rs", 42)));
        assert_eq!(shown, "error: meowny stopped unexpectedly (src/args.rs:42)");
        assert_eq!(stopped(None), "error: meowny stopped unexpectedly");
    }
}
