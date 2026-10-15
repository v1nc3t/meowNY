# meowNY

Local-first expense tracker. A Tauri desktop app (Rust) shows a React client and keeps all data in an encrypted SQLite file (SQLCipher) on your own machine. No server, no accounts, no telemetry. See `assets/PLAN.md` for the roadmap and `assets/data-inventory.md` for what data lives where.

The desktop app has categories, transactions, budgets, recurring items, charts, encrypted backup and export (phases 0 to 6). A command line for scripts and AI agents (phase 7) is in progress, and phone sync and the bank connection are still to come; see the plan. The old Express/Postgres server design is retired and only exists in git history.

## Requirements

- Node.js 22 or newer and npm
- Rust (stable, 1.77 or newer) from https://rustup.rs
- Tauri's system libraries for your OS: https://v2.tauri.app/start/prerequisites/
  - Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`
  - macOS: Xcode command line tools
  - Windows: Microsoft C++ Build Tools and WebView2
- A secret service for the key: macOS Keychain, Windows Credential Manager, or on Linux GNOME Keyring / KWallet running in your session. Without one, first run fails with "key unavailable".

## Run the app

From the repo root:

```bash
npm install
npm run tauri dev
```

The first run compiles the Rust side (a few minutes). It starts the Vite dev server for the client and opens a window; edits to `client/src` hot-reload, edits to `src-tauri/src` rebuild and restart the app.

On first launch the app asks for a currency and an optional passphrase, then creates the encrypted data file. Settings (currency, locale, timezone, app lock, delete all data) are at `/app/settings`.

Running `npm run dev --workspace client` on its own opens the client in a browser, but there is no backend there: every screen talks to the Rust side, so it only works in the desktop window.

### Where the data lives

| OS | Folder |
|---|---|
| Linux | `~/.local/share/org.meowny.app/` |
| macOS | `~/Library/Application Support/org.meowny.app/` |
| Windows | `%APPDATA%\org.meowny.app\` |

It holds `meowny.db` (encrypted; while the app runs there are also `meowny.db-wal` and `meowny.db-shm`, encrypted too) and `vault.json` (lock settings, the wrapped keys and the passphrase salt). The key is in the OS keychain under the service name `org.meowny.app`.

To start over in development, use Settings, delete all my data. That removes the files and the keychain entry together. Deleting the folder by hand leaves a stale keychain entry, which is harmless and gets replaced on the next first run.

## Keyboard shortcuts

Press `?` in the app to see them all. Hovering or focusing a control shows its shortcut. Shortcuts are ignored while typing in a field and inside dialogs.

| Keys | Does |
|---|---|
| `g` then `d` `t` `c` `b` `s` | go to dashboard, transactions, categories, budgets, settings |
| `n` | add a new transaction or category |
| `/` | search |
| `[` and `]` | previous and next month (budgets) |
| `t` | jump to the current month |
| `j` and `k` | next and previous row in a list (Enter opens it) |
| `Ctrl+L` / `Cmd+L` | lock the app |
| `Ctrl+B` / `Cmd+B` | show or hide the sidebar |

All shortcuts are defined in one table, `client/src/keys/shortcuts.ts`.

## Command line

`meowny` drives the running desktop app from a terminal, for scripts and AI agents. It is a small standalone program (under 1 MB, one dependency, `serde_json`) in `cli/`. It never opens the data file: it talks to the app over a local connection (`127.0.0.1` only) that is off until you turn it on, and every request needs a token. The desktop app's own program is also called `meowny`, so this one is built on its own.

```bash
# 1. In the app: Settings, command line access. Turn it on and make a token
#    (read only, unless the script has to change things). It is shown once.
# 2. Build the program; this puts `meowny` in cargo's bin folder
cargo install --path cli
# 3. Give it the token: in the environment, or once with `login` (read from standard input, saved
#    readable only by you). There is deliberately no --token flag: command lines can be read by other users.
export MEOWNY_TOKEN=mny_...
echo "$TOKEN" | meowny login

meowny status
meowny transactions list --limit 5
meowny transactions create --name Bread --amount 2.40 --payment-date 2026-10-14 --category-id 5
meowny stats budget --month 2026-10 --json
meowny transactions delete 42 --yes
meowny --help        # and meowny <command> --help
meowny schema        # every command, flag and exit code as JSON, without the app
```

The commands are `meowny <noun> <verb>`, worked out from the app's own list of routes. Data goes to standard output (`--json` for JSON, otherwise a plain table), messages and errors to standard error. It never asks anything: every input is a flag or standard input, commands that delete or end something need `--yes`, and a new transaction gets a fresh `--client-uuid` so a retry cannot record it twice. The app has to be running and unlocked.

| Exit code | Means |
|---|---|
| 0 | done |
| 1 | the app is not reachable, the answer was not understood, or another error |
| 2 | bad usage, or a value the app refused |
| 3 | no token, a wrong or expired token, a token that may not do this, or the app is locked |
| 4 | not found |

With `--json` an error is `{"code": "...", "message": "..."}` on standard error. The token is found in `--token-file`, then `MEOWNY_TOKEN`, then `MEOWNY_TOKEN_FILE`, then the file `login` saved; a token file that other users can read is refused. `MEOWNY_DATA_DIR` or `--data-dir` says where the app keeps its data (where it writes its port), `MEOWNY_PORT` or `--port` skips that.

To try it without the app, `cargo run --example api_demo` in `src-tauri/` serves demo data and prints the tokens and a ready command line.

## Tests and checks

```bash
# Rust: migrations, constraints, vault, key handling
cd src-tauri && cargo test

# Timings of the main queries on 50,000 transactions (prints, does not assert)
cd src-tauri && cargo test bench -- --ignored --nocapture

# Client
npm run lint --workspace client
npm test --workspace client
npm run build --workspace client

# The command line: unit tests, and tests that run the real binary against the app's own server code
cd cli && cargo test
# After a route is added or changed, regenerate the list of routes the command line embeds (a test fails until you do)
cd cli && cargo run --example dump_schema > schema.json

# Lists every place that can make a network call and fails on new ones
npm run check:outbound
```

CI (`.github/workflows/desktop.yml`) runs the outbound check, `cargo test` for the app and for `cli/`, then builds the app. Pull requests build on Linux only (about 2.5 minutes); pushes to `main` also build on macOS and Windows, and you can run all three for any branch from the Actions tab with "Run workflow".

## Database

Schema migrations are plain SQL in `src-tauri/migrations/`, embedded in the app and applied on startup. The version is stored in SQLite's `user_version`, and a database from a newer app version is refused. Add a change as the next numbered file and append it to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit a migration that has shipped.

Money is stored as integer minor units (`*_minor` columns). Constraint tests are in `src-tauri/src/schema_tests.rs`.

### Demo data

Sixteen months of demo data, with dates relative to today (`src-tauri/seeds/dev.sql`): categories, budgets that change over time, recurring items that are paid, overdue, paused and ended, a deleted category and a few hundred transactions, including awkward text (accents, Romanian and Japanese letters, quotes, cells that look like formulas) for trying the exports.

To put it in the desktop app, quit the app and run:

```bash
cd src-tauri
cargo run --example dev_vault -- ~/.local/share/org.meowny.app
```

This **deletes the app's data** (the data file, its key in the OS keychain, the settings file and any backups in that folder, as "delete all my data" does), sets the app up again with the demo data and prints the new recovery key. The lock settings are kept. The data file has no passphrase unless you set `MEOWNY_DEV_PASSPHRASE` for the run. The folder above is where the app keeps its data on Linux. Run the command again to start over.

To try the command API for the CLI (phase 7) on demo data, without the app or your own data:

```bash
cd src-tauri
cargo run --example api_demo
```

It prints a read token, a write token and `curl` lines, and serves until Ctrl+C. In the desktop app, Settings, command line access, turns the API on (it is off by default) and makes the tokens.

To load the demo data into a separate file instead, which the desktop app does not open:

```bash
cargo run --example seed -- /path/to/dev.db <64-hex-character-key>
```

It refuses a database that already has data.
