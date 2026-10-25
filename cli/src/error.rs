//! Every way a command can fail: a fixed code for programs, a message for people, and the exit code
//! the process ends with. The message never holds the token, and never text from the database.

use serde_json::{json, Value};

/// How the process ends. Scripts branch on these; they are listed in `meowny schema` and in the docs.
pub const EXIT_CODES: &[(u8, &str)] = &[
    (0, "success"),
    (1, "error: the app is not reachable, the answer was not understood, or the request failed for another reason"),
    (2, "bad usage: a missing or unknown flag, or a value the app refused"),
    (3, "the token is missing, wrong, expired or may not do this, the person said no in the app, or the app is locked"),
    (4, "not found"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    /// A fixed word such as `locked` or `invalid_input`. The app's own codes pass through unchanged.
    pub code: String,
    pub message: String,
}

impl CliError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new("usage", message)
    }

    pub fn exit(&self) -> u8 {
        exit_for(&self.code)
    }

    /// What `--json` prints on standard error.
    pub fn to_json(&self) -> Value {
        json!({ "code": self.code, "message": self.message })
    }
}

/// The exit code for an error code. A code this CLI does not know, say from a newer app, is a plain 1.
pub fn exit_for(code: &str) -> u8 {
    match code {
        "usage" | "invalid_input" => 2,
        "no_token" | "bad_token" | "token_file_unsafe" | "unauthorized" | "forbidden" | "denied" | "locked" => 3,
        "not_found" => 4,
        _ => 1,
    }
}

/// Every error code and the exit code it gets, for `meowny schema`.
pub const ERROR_CODES: &[&str] = &[
    "usage",
    "no_token",
    "bad_token",
    "token_file_unsafe",
    "app_unavailable",
    "timeout",
    "bad_answer",
    "invalid_input",
    "unauthorized",
    "forbidden",
    "denied",
    "not_found",
    "unknown_route",
    "method_not_allowed",
    "name_exists",
    "over_budget",
    "file_exists",
    "not_set_up",
    "too_large",
    "locked",
    "internal",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_error_code_ends_the_process_the_documented_way() {
        let expected = [
            ("usage", 2),
            ("invalid_input", 2),
            ("no_token", 3),
            ("bad_token", 3),
            ("token_file_unsafe", 3),
            ("unauthorized", 3),
            ("forbidden", 3),
            ("denied", 3),
            ("locked", 3),
            ("not_found", 4),
            ("unknown_route", 1),
            ("app_unavailable", 1),
            ("timeout", 1),
            ("bad_answer", 1),
            ("method_not_allowed", 1),
            ("name_exists", 1),
            ("over_budget", 1),
            ("file_exists", 1),
            ("not_set_up", 1),
            ("too_large", 1),
            ("internal", 1),
        ];
        for (code, exit) in expected {
            assert_eq!(exit_for(code), exit, "{code}");
            assert!(ERROR_CODES.contains(&code), "{code} is not listed");
        }
        assert_eq!(ERROR_CODES.len(), expected.len(), "a code was added without a test");
    }

    #[test]
    fn a_missing_record_is_4_but_a_missing_route_is_not() {
        // "Already deleted" and "your app is older than this program" must not look alike to a script.
        assert_eq!(exit_for("not_found"), 4);
        assert_eq!(exit_for("unknown_route"), 1);
    }

}
