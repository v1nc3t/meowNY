//! The commands that do not call a route: `login`, `logout`, and `schema` (which answers from the
//! catalog built into this program, so an agent can discover the commands without the app).

use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::args::Globals;
use crate::catalog::{Catalog, Command, API_VERSION};
use crate::client::{has_token_shape, saved_token_path, Env};
use crate::error::{CliError, ERROR_CODES, EXIT_CODES, exit_for};
use crate::help::{ENVIRONMENT, GLOBAL_FLAGS};

fn command_json(c: &Command) -> Value {
    let mut flags = Vec::new();
    for p in &c.params {
        flags.push(json!({
            "flag": format!("--{}", p.flag),
            "param": p.name,
            "type": p.kind,
            "required": p.required,
            "nullable": p.nullable,
            "values": p.values,
            "summary": p.summary,
        }));
        if p.nullable {
            flags.push(json!({ "flag": format!("--clear-{}", p.flag), "clears": p.name, "summary": format!("Clears {}.", p.name) }));
        }
    }
    json!({
        "command": c.name(),
        "route": c.route,
        "scope": c.scope,
        "summary": c.summary,
        "destructive": c.destructive(),
        "positional": c.positional().map(|p| p.name.clone()),
        "flags": flags,
    })
}

/// Every command with its flags, the global flags, the exit codes and the error codes, as JSON.
pub fn schema(catalog: &Catalog) -> Value {
    let exit_codes: serde_json::Map<String, Value> = EXIT_CODES.iter().map(|(c, t)| (c.to_string(), json!(t))).collect();
    let error_codes: serde_json::Map<String, Value> = ERROR_CODES.iter().map(|c| (c.to_string(), json!(exit_for(c)))).collect();
    json!({
        "cli": { "name": "meowny", "version": env!("CARGO_PKG_VERSION"), "api": API_VERSION },
        "commands": catalog.commands.iter().map(command_json).collect::<Vec<_>>(),
        "globalFlags": GLOBAL_FLAGS.iter().map(|(f, s)| json!({ "flag": f, "summary": s })).collect::<Vec<_>>(),
        "environment": ENVIRONMENT.iter().map(|(n, s)| json!({ "name": n, "summary": s })).collect::<Vec<_>>(),
        "exitCodes": exit_codes,
        "errorCodes": error_codes,
    })
}

/// Where `login` and `logout` keep the token: `--token-file`, `MEOWNY_TOKEN_FILE`, else the saved file.
fn token_path(env: Env, g: &Globals) -> Result<PathBuf, CliError> {
    if let Some(file) = &g.token_file {
        return Ok(PathBuf::from(file));
    }
    match env("MEOWNY_TOKEN_FILE").filter(|v| !v.is_empty()) {
        Some(file) => Ok(PathBuf::from(file)),
        None => saved_token_path(env),
    }
}

fn write_private(path: &PathBuf, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&tmp).and_then(|mut f| f.write_all(text.as_bytes()).and_then(|_| f.sync_all()));
    if let Err(e) = written.and_then(|_| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Saves the token on the first line of standard input. It is checked for its shape (not with the app)
/// and written readable only by its owner. The token is never shown.
pub fn login(env: Env, g: &Globals, stdin: &mut dyn BufRead) -> Result<PathBuf, CliError> {
    let mut line = String::new();
    // At most 4 KiB, so a wrong file piped in cannot fill memory.
    std::io::Read::take(&mut *stdin, 4096).read_line(&mut line).map_err(|_| CliError::new("bad_token", "the token is not text"))?;
    let token = line.trim();
    if !has_token_shape(token) {
        return Err(CliError::new("bad_token", "that is not a token (mny_ and 64 letters and digits); copy it again from the app"));
    }
    let path = token_path(env, g)?;
    write_private(&path, &format!("{token}\n")).map_err(|_| CliError::new("internal", format!("cannot save the token to {}", path.display())))?;
    Ok(path)
}

/// Removes the saved token. Returns where it was and whether there was one.
pub fn logout(env: Env, g: &Globals) -> Result<(PathBuf, bool), CliError> {
    let path = token_path(env, g)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok((path, true)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((path, false)),
        Err(_) => Err(CliError::new("internal", format!("cannot remove the token file {}", path.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const TOKEN: &str = "mny_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn the_schema_lists_every_command_flag_exit_code_and_error_code() {
        let catalog = Catalog::embedded();
        let s = schema(&catalog);
        assert_eq!(s["cli"], json!({ "name": "meowny", "version": env!("CARGO_PKG_VERSION"), "api": 1 }));
        assert_eq!(s["commands"].as_array().unwrap().len(), catalog.commands.len());
        assert_eq!(s["exitCodes"].as_object().unwrap().len(), 5);
        assert_eq!(s["errorCodes"]["locked"], 3);
        assert_eq!(s["errorCodes"]["not_found"], 4);
        assert_eq!(s["errorCodes"]["invalid_input"], 2);
        assert!(s["globalFlags"].as_array().unwrap().iter().any(|f| f["flag"] == "--json"));
        assert!(s["environment"].as_array().unwrap().iter().any(|f| f["name"] == "MEOWNY_TOKEN"));
        assert!(!s.to_string().contains("--token <"), "no flag takes the token itself");
    }

    #[test]
    fn a_command_in_the_schema_says_what_it_calls_and_takes() {
        let s = schema(&Catalog::embedded());
        let update = s["commands"].as_array().unwrap().iter().find(|c| c["command"] == "transactions update").unwrap();
        assert_eq!((update["route"].clone(), update["scope"].clone(), update["destructive"].clone()), (json!("transactions.update"), json!("write"), json!(false)));
        assert_eq!(update["positional"], "id");
        let flags: Vec<&str> = update["flags"].as_array().unwrap().iter().map(|f| f["flag"].as_str().unwrap()).collect();
        assert_eq!(flags, ["--id", "--name", "--amount", "--payment-date", "--category-id", "--description", "--clear-description"]);
        let delete = s["commands"].as_array().unwrap().iter().find(|c| c["command"] == "transactions delete").unwrap();
        assert_eq!(delete["destructive"], true);
        let login = s["commands"].as_array().unwrap().iter().find(|c| c["command"] == "login").unwrap();
        assert_eq!((login["route"].clone(), login["scope"].clone()), (Value::Null, Value::Null));
        let top = s["commands"].as_array().unwrap().iter().find(|c| c["command"] == "categories list").unwrap();
        assert_eq!(top["flags"][0]["values"], json!(["INCOME", "EXPENSE"]));
    }

    #[test]
    fn login_saves_the_token_privately_and_logout_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/cfg/token");
        let g = Globals { token_file: Some(path.to_str().unwrap().into()), ..Default::default() };
        let saved = login(&env(&[]), &g, &mut format!("  {TOKEN}  \n").as_bytes()).unwrap();
        assert_eq!(saved, path);
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{TOKEN}\n"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700, "the folders made for it are private");
        }
        assert!(!dir.path().join("nested/cfg/token.tmp").exists());
        // Logging in again replaces it.
        let other = format!("mny_{}", "ab".repeat(32));
        login(&env(&[]), &g, &mut format!("{other}\n").as_bytes()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{other}\n"));

        assert_eq!(logout(&env(&[]), &g).unwrap(), (path.clone(), true));
        assert!(!path.exists());
        assert_eq!(logout(&env(&[]), &g).unwrap(), (path, false), "logging out twice is fine");
    }

    #[test]
    fn login_refuses_what_is_not_a_token_and_never_repeats_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        let g = Globals { token_file: Some(path.to_str().unwrap().into()), ..Default::default() };
        for bad in ["", "\n", "hello world\n", "mny_short\n", &format!("{TOKEN}0\n"), "Bearer abc\n"] {
            let e = login(&env(&[]), &g, &mut bad.as_bytes()).unwrap_err();
            assert_eq!(e.code, "bad_token", "{bad:?}");
            assert!(!e.message.contains(bad.trim()) || bad.trim().is_empty(), "{}", e.message);
        }
        assert!(!path.exists(), "nothing was written");
        // Only the first line counts, so a pasted token followed by more text still works.
        login(&env(&[]), &g, &mut format!("{TOKEN}\nsecond line\n").as_bytes()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{TOKEN}\n"));
    }

    #[test]
    fn the_place_follows_the_flag_then_the_variable_then_the_config_folder() {
        let dir = tempfile::tempdir().unwrap();
        let by_var = dir.path().join("by-var");
        let e = env(&[("MEOWNY_TOKEN_FILE", by_var.to_str().unwrap())]);
        login(&e, &Globals::default(), &mut format!("{TOKEN}\n").as_bytes()).unwrap();
        assert!(by_var.exists());
        if cfg!(target_os = "linux") {
            let cfg = env(&[("XDG_CONFIG_HOME", dir.path().to_str().unwrap())]);
            login(&cfg, &Globals::default(), &mut format!("{TOKEN}\n").as_bytes()).unwrap();
            assert!(dir.path().join("meowny/token").exists());
        }
    }
}
