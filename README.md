<h1 align="center">meowNY</h1>

<p align="center">
  A local-first expense tracker. Your data stays on your computer, encrypted, with no server, no account and no telemetry.
</p>

<p align="center">
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/desktop.yml"><img alt="Desktop build" src="https://github.com/v1nc3t/meowNY/actions/workflows/desktop.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/security.yml"><img alt="Security" src="https://github.com/v1nc3t/meowNY/actions/workflows/security.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
  <img alt="Platforms: Linux, macOS, Windows" src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey">
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#getting-started">Getting started</a> ·
  <a href="#command-line">Command line</a> ·
  <a href="https://v1nc3t.github.io/meowNY/">CLI reference</a> ·
  <a href="docs/PLAN.md">Roadmap</a>
</p>

---

meowNY tracks income and expenses against monthly budgets. A [Tauri](https://tauri.app) shell (Rust) shows a React UI and keeps everything in one encrypted SQLite file ([SQLCipher](https://www.zetetic.net/sqlcipher/)) on your machine. A command line program, `meowny`, lets scripts and AI agents use the running app without opening the data file.

> **Status: pre-release (0.1.0).** Desktop app and command line work and are tested. No installers yet: build from source. The phone app and its sync are built and being tested. Documents, and debts and expense splitting, are built; the camera for a bill code and the phone's reminders are not yet tried on a real phone. See the [roadmap](#roadmap).

## Table of contents

- [Features](#features)
- [Privacy and security](#privacy-and-security)
- [Getting started](#getting-started)
- [Usage](#usage)
- [Documentation](#documentation)
- [Project structure](#project-structure)
- [Development](#development)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Reporting a security problem](#reporting-a-security-problem)
- [License](#license)

## Features

**Budgeting**

- Categories; transactions (income and expenses) with search and filters.
- Monthly budgets per category. Budgets are dated: changing a limit does not rewrite old months.
- Recurring items: see what is due or overdue; pay, skip, pause or end each. Optional notifications.
- Statistics and charts: budget against spending, trends, year view, top categories, upcoming payments.
- Money is whole minor units (cents), never floats. Months and due dates follow your time zone.
- Seven languages: English, Deutsch, Español, Français, Italiano, Português, Română.
- Keyboard shortcuts for the main actions.

**Documents** ([guide](docs/documents.md); built, not yet tried on a real phone)

- Attach receipts, invoices and contracts to transactions and recurring items, on computer and phone (file chooser or camera).
- Files are kept **encrypted** next to the database, up to 25 MiB each, readable only while the app is unlocked.
- Preview of images, PDF, text, CSV, .docx and .xlsx inside the app: no plain copy, nothing run.
- The type comes from the file's first bytes, never its name. A file of unknown type is never opened, only saved.
- Opening a PDF or large picture hands a plain copy to the system viewer; it is removed on lock or close, and the screen says so first.
- **Keep on phone** is a switch per document. A document you did not switch on never leaves the computer.
- Backups can include the files (a choice). Exports list documents by name and never contain a file.
- Script: `meowny documents add|list|show|update|link|unlink|delete|get`. A copy comes out only when you allow it in the app.

**Debts and shared bills** ([guide](docs/debts.md); built, camera and phone reminders not yet tried on a real phone)

- A **friend is a name** and nothing else: no phone, address or contacts. Friends are archived, never deleted. A **guest** is a name typed for one bill.
- A **ledger** per friend: lent, borrowed, repaid. None of it is income or expense or touches a budget.
- **Split a bill**: equally, by percent, by shares or by exact amounts. Budgets, statistics, reports and exports count only your part.
- **Who owes what** on the debts page and the dashboard, with overdue recurring payments and a history of debts. A friend's statement can be saved as text, CSV or PDF.
- **Bill codes**: a QR code (and its text) with a friend's part, the bill and your name, nothing else. The friend scans or pastes it, then accepts or denies. Nothing is sent or stored.
- **Reminders**: one setting per device (weekly, every 2 weeks, monthly, never). The text holds a count, never a name or amount.
- Friends, entries and parts sync with the phone. Script: `meowny people|ledger|debts|splits ...`, `meowny transactions split`.

**Safety of your data**

- Database encrypted at rest. The key is in the OS keychain, optionally with a passphrase.
- A recovery key, shown once.
- Auto-lock when idle, and optionally on minimise or close. Locking drops the key from memory.
- Encrypted backups (`.meowbak`), restore, and scheduled backups to a folder.
- Export to JSON, CSV, Excel and PDF, with period, category and part choices and a preview. Export files are not encrypted; use a backup to keep data safe.
- "Delete all my data" removes the database, its key and the app's backups.

**Your phone** (sync, screens and Android app built; reviewed on a real phone)

- The phone keeps a full copy, works alone, and can change everything the desktop can. Back home, the two sync.
- No server. The desktop shows a QR code, the phone scans it and connects straight to the desktop on the home network. The code works once, for about two minutes. Both devices must be on the same wifi.
- Pairing shows six digits on the desktop that you type on the phone. Everything sent is encrypted with keys made from the pairing secret and that code.
- A sync can go both ways, phone to computer only, or computer to phone only. Conflicting edits are shown side by side and you choose.
- The phone is locked by a passphrase and erases itself after 5 wrong ones. Pair again to restore.
- The currency is changed on the desktop only.
- The first time the desktop listens, Windows or macOS may ask about network access. Allow private networks only.
- The phone has no export or backup file, and Android's cloud backup of the app is off.

**For scripts and AI agents (desktop only)**

- The [command line](#command-line) `meowny`: a command per feature, `--json` output, stable exit codes.
- A local command API behind it, off until you turn it on. Access needs a token you make in the app: read-only if you like, expiring, revocable.
- Exports, backups and document copies started from a script need a yes in an app dialog, each time.

## Privacy and security

- **Local-first.** No server and no account, so no one could hold your data. Nothing is sent anywhere; no telemetry.
- **No network by default.** A CI check (`npm run check:outbound`) lists every place in the code that can open a connection and fails when a new one appears. The only optional traffic planned is update checks, which you turn on yourself. Phone sync stays on your home network.
- **Hardened window.** Strict content security policy, no remote content, one permission per command, no shell access from the web view, links open in the system browser.
- **Safe logs.** Logs and crash reports never contain amounts, descriptions, tokens or keys.
- **The command API is for you only.** It listens on `127.0.0.1` only, refuses requests from web pages, answers "locked" while the app is locked, and does not count as activity (the idle lock still works). Restoring a backup, changing the currency and deleting everything cannot be done through it.

What is stored, where, and who could see it: [docs/data-inventory.md](docs/data-inventory.md). It is a technical note, not a privacy policy.

## Getting started

### Requirements

- Node.js 22 or newer, and npm
- Rust (stable, 1.77 or newer) from <https://rustup.rs>
- Tauri's system libraries: <https://v2.tauri.app/start/prerequisites/>
  - Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`
  - macOS: Xcode command line tools
  - Windows: Microsoft C++ Build Tools and WebView2
- A secret service for the key: macOS Keychain, Windows Credential Manager, or GNOME Keyring or KWallet on Linux. Without one, the first run fails with "key unavailable".

### Program names

| Program | Name | Source |
|---|---|---|
| Desktop app | `meowny-desktop` | `src-tauri/` |
| Command line | `meowny` | `cli/` |

The window and installers are still called meowNY. The data folder (`org.meowny.app`) is the same.

### Run the app

```bash
git clone git@github.com:v1nc3t/meowNY.git
cd meowNY
npm install
npm run tauri dev
```

- The first run compiles the Rust side (a few minutes). Edits to `ui/src` hot-reload; edits to `src-tauri/src` rebuild.
- First launch asks for a currency and an optional passphrase, shows your recovery key, and creates the data file. Settings are at `/app/settings`.
- `npm run dev --workspace ui` opens the UI in a browser, but without a backend it only works in the desktop window.

### Build a bundle

```bash
npm run tauri build
```

Builds a release app and the installer formats Tauri supports for your system. Not signed yet; signed installers are planned for the first release.

### Build the Android app

Same UI and Rust core, packaged by Tauri mobile. Tried on one phone at home (see [docs/PLAN.md](docs/PLAN.md), phase 8). CI (`.github/workflows/android.yml`) builds a debug-signed APK on every app change. To build by hand you need:

- Java 21, the Android SDK, and NDK 27 (`sdkmanager "ndk;27.2.12479018"`), with `ANDROID_HOME`, `NDK_HOME` and `ANDROID_NDK_HOME` set, and the NDK's `toolchains/llvm/prebuilt/<system>/bin` on your `PATH` (SQLCipher builds its own OpenSSL)
- The Rust target: `rustup target add aarch64-linux-android`

```bash
npx tauri android init                        # makes src-tauri/gen/android, which is not kept in git
python3 scripts/android-patch.py              # no Android backup of the app's folder, no screenshots of it
npx tauri android build --apk --target aarch64 --debug
adb install src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

- Run `scripts/android-patch.py` again after every `tauri android init`. It stops Android from copying the encrypted data and its secret to a Google account, and makes the window secure.
- The APK has a debug key: enough to install by hand. A release key and store listing belong to phase 11.
- The phone build leaves out Excel and PDF export and file dialogs.
- Dev only: `cargo run --example sync_desktop -- <folder> setup`, then `pair` or `sync both|to_desktop|to_phone`, tries the phone against a desktop without a window. `cargo run --release --example bench` times common work on ten years of data.
- Test build (adds a field to paste the code, allows screenshots): `MEOWNY_ALLOW_SCREENSHOTS=1 python3 scripts/android-patch.py` and `VITE_MEOWNY_TEST_PASTE=1 npx tauri android build ...`. Never install it for real use.

### Where the data lives

| OS | Folder |
|---|---|
| Linux | `~/.local/share/org.meowny.app/` |
| macOS | `~/Library/Application Support/org.meowny.app/` |
| Windows | `%APPDATA%\org.meowny.app\` |

- `meowny.db`: encrypted (with `-wal` and `-shm` while the app runs, also encrypted).
- `vault.json`: lock settings, wrapped keys, passphrase salt.
- `documents/`: one sealed file per document, names say nothing about it.
- `open/`: plain copies for the system viewer, only while a file is open; removed on lock or close.
- `api.json`: API version and port, no secrets, only while the command API is on.
- The key is in the OS keychain under the service name `org.meowny.app`.

To start over in development, use Settings, delete all my data. Deleting the folder by hand leaves a stale keychain entry, which is harmless and replaced on the next first run.

## Usage

### Keyboard shortcuts

Press `?` in the app to see them all. Shortcuts are ignored while typing and inside dialogs. They are defined in `ui/src/keys/shortcuts.ts`.

| Keys | Does |
|---|---|
| `g` then `d` `t` `c` `s` | go to dashboard, transactions, categories (and budgets), settings |
| `n` | add a new transaction or category |
| `/` | search |
| `[` and `]` | previous and next month (budgets) |
| `t` | jump to the current month |
| `j` and `k` | next and previous row in a list (Enter opens it) |
| `Ctrl+L` / `Cmd+L` | lock the app |
| `Ctrl+B` / `Cmd+B` | show or hide the sidebar |

### Command line

`meowny` drives the running desktop app from a terminal. It is a small standalone program (under 1 MB, one dependency) in `cli/`. It never opens the data file: it talks to the app over `127.0.0.1`, which is off until you turn it on, and every request needs a token. Desktop only.

1. In the app: Settings, "command line access", turn it on, make a token (read-only unless a script must change things; pick a lifetime). The secret (`mny_` and 64 hex characters) is shown once.
2. Build it. This puts `meowny` in `~/.cargo/bin`, which must be on your `PATH`:

   ```bash
   cargo install --path cli
   ```

3. Give it the token, once with `login` (read from standard input, saved readable only by you) or in the environment. There is no `--token` flag, because other users can read command lines:

   ```bash
   echo "$TOKEN" | meowny login
   # or
   export MEOWNY_TOKEN=mny_...
   ```

4. Check it. The app must be running and unlocked for every command:

   ```bash
   meowny status
   ```

**Examples**

```bash
meowny transactions list --limit 5
meowny transactions create --name Bread --amount 2.40 --payment-date 2026-10-14 --category-id 5
meowny stats budget --month 2026-10 --json
meowny transactions delete 42 --yes
meowny export csv --out ~/budget-2026.csv --from 2026-01-01
meowny documents add ~/Downloads/boiler-receipt.pdf --transaction-ids 42   # the program reads the file and sends it
meowny documents get 7 --out ~/boiler-receipt.pdf                          # you allow this in the app
meowny people create --name Anna                                            # a friend is a name and nothing else
meowny transactions split 42 --mode equal --people 1,2                     # you paid for dinner; budgets count only your part
meowny people settle 1 --amount 20.00                                       # Anna paid you back
meowny debts summary                                                        # who owes what
meowny debts due                                                            # recurring payments that are overdue
meowny export json --out ~/debts.json --sections people,due,ledger          # parts and limits as on the export screen
meowny backup create --out ~/meowny.meowbak --passphrase -   # the passphrase on standard input
meowny --help        # and meowny <command> --help
meowny schema        # every command, flag and exit code as JSON, without the app
```

**How it behaves**

- Commands are `meowny <noun> <verb>`, made from the app's own list of routes.
- Data goes to standard output (`--json` for one-line JSON, otherwise a table). Messages and errors go to standard error; with `--json` an error is `{"code": "...", "message": "..."}`.
- It never asks anything: input is flags or standard input. Commands that delete or end something need `--yes`. A new transaction gets a fresh `--client-uuid`, so a retry cannot record it twice.
- Money stays a string end to end; dates are ISO 8601.
- Token order: `--token-file`, `MEOWNY_TOKEN`, `MEOWNY_TOKEN_FILE`, the file `login` saved (`~/.config/meowny/token` on Linux). A token file other users can read is refused.
- `MEOWNY_DATA_DIR` or `--data-dir` says where the app keeps its data; `MEOWNY_PORT` or `--port` skips that.
- Exports (`export json|csv|xlsx|pdf|debts`), backups (`backup create`) and document copies (`documents get`) need a yes in an app dialog (which token, what, the exact file). The file must have the right ending, be in an existing folder, and not exist yet: nothing is overwritten. Asking again within 30 seconds of a "no" is refused.

| Exit code | Means |
|---|---|
| 0 | done |
| 1 | app not reachable, answer not understood, or another error |
| 2 | bad usage, or a value the app refused |
| 3 | no, wrong or expired token; token may not do this; you said no in the app; app is locked |
| 4 | not found |

**Try it without the app.** `cargo run --example api_demo` in `src-tauri/` serves demo data and prints a read token, a write token and a ready command line. Export and backup questions are answered in that terminal (y or n).

Every command, flag and exit code, with real output, is in the [CLI reference](https://v1nc3t.github.io/meowNY/) and [docs/CLI.md](docs/CLI.md).

## Documentation

| Document | What it is |
|---|---|
| [CLI reference site](https://v1nc3t.github.io/meowNY/) | A card per command with flags and a real example, exit and error codes, a page for AI agents, the HTTP API |
| [docs/CLI.md](docs/CLI.md) | The same text in one Markdown file |
| [docs/PLAN.md](docs/PLAN.md) | The build plan and roadmap, phase by phase |
| [docs/data-inventory.md](docs/data-inventory.md) | What data the app holds, where, which network calls exist, who could see it |
| [docs/sync-design.md](docs/sync-design.md) | How the phone syncs: QR codes, encryption, merge rules, screens (phase 8) |
| [docs/documents.md](docs/documents.md) | Guide to documents (phase 9) |
| [docs/debts.md](docs/debts.md) | Guide to debts and shared bills (phase 10) |
| [docs/documents-design.md](docs/documents-design.md) | How documents are built: sealed file format, keys, sync of bytes, backup format, real-phone checklist |

The reference site is made from the program. `cli/tests/docs.rs` runs a tour of every command and failure against a real app, records it (`docs/site/commands.json`, `docs/site/examples.json`), and fails when a command, flag, message or example changes without the docs being redone. The prose is `docs/site/content.mjs`. The site is published by `.github/workflows/pages.yml` on pushes to `main` that touch `docs/` (Settings, Pages, Source set to "GitHub Actions" once).

```bash
# After the commands change: record them again, then rebuild the page
cd cli && UPDATE_DOCS=1 cargo test --offline --test docs
npm run docs                  # writes docs/CLI.md and the site in docs/site/_site/ (open docs/site/_site/index.html)
npm run check:docs            # what CI runs: the generator's tests, and that docs/CLI.md is up to date
```

## Project structure

```text
meowNY/
├── ui/                React UI (Vite, TypeScript, Zod). Every screen talks to the Rust side
│   └── src/           pages, components, hooks, lib, api, and the vault, i18n and keys features
├── src-tauri/         The desktop app, `meowny-desktop` (Rust, Tauri 2)
│   ├── src/           vault and keys, database, budgets, recurring items, statistics, backup, export,
│   │                  the command API (api/), access tokens (tokens.rs), phone sync (sync/), documents (documents/)
│   ├── migrations/    Schema changes as numbered SQL files
│   ├── seeds/         Demo data
│   └── examples/      dev_vault, seed and api_demo
├── cli/               The `meowny` command line program (Rust, one dependency)
├── docs/              Everything written for people; site/ makes the CLI reference site and CLI.md (plain Node)
├── scripts/           Checks CI runs on the whole repo: check-outbound and its allowlist
└── .github/workflows/ ci.yml, desktop.yml, security.yml and pages.yml
```

### Naming and layout

- Top-level folders are one thing you build or run (`ui`, `src-tauri`, `cli`) or one kind of support file (`docs`, `scripts`, `.github`).
- Anything written for people goes in `docs/`; code that makes a document sits next to it in `docs/site/`; whole-repo checks go in `scripts/`.
- Nothing is named after a design that is gone.

| Folder in `ui/src/` | Holds |
|---|---|
| `pages/` | One screen per route, `NamePage.tsx`; helpers only that page uses sit beside it |
| `components/` | Pieces of UI that are not a whole screen |
| `layouts/` | The frame around the pages |
| `hooks/` | React hooks, `useName.ts` |
| `lib/` | Plain TypeScript with no React |
| `api/` | Calls into the Rust side, one file per area. `invoke.ts` is the one place that calls Tauri |
| `vault/`, `i18n/`, `keys/` | A feature with its own screens, logic and tests |

| File names | Rule | Example |
|---|---|---|
| Folders | Lowercase, `kebab-case` if needed | `ui`, `src-tauri` |
| Rust files | `snake_case`. A module with children is `x.rs` plus a folder `x/`, never `mod.rs` (except `cli/tests/common/mod.rs`). Tests of `x` sit in `x_tests.rs` | `recovery_file.rs` |
| Migrations | `NNNN_name.sql`, never edited once shipped | `0001_tables.sql` |
| React components and pages | `PascalCase.tsx`, one per file | `DashboardPage.tsx` |
| Other TypeScript | `camelCase.ts`; tests as `name.test.ts` | `money.test.ts` |
| Markdown and text | `kebab-case`; the first-open files are UPPERCASE | `data-inventory.md`, `README.md` |
| Workflows | Named for what they do | `ci.yml`, `pages.yml` |

How the parts fit: the UI calls Rust commands through Tauri; Rust owns the database, keys and every rule. The command line is a client of the running app through the local command API. The list of API routes is one table in Rust; `meowny schema`, the commands and the docs are made from it.

## Development

### Tests and checks

```bash
# Rust: migrations, constraints, vault, key handling, the command API
cd src-tauri && cargo test

# Timings of the main queries on 50,000 transactions (prints, does not assert)
cd src-tauri && cargo test bench -- --ignored --nocapture

# UI
npm run lint --workspace ui
npm test --workspace ui
npm run build --workspace ui

# The command line: unit tests, and tests that run the real binary against the app's own server code
# (the docs test and the login test assume Linux paths and run only there)
cd cli && cargo test
# After a route is added or changed, regenerate the list of routes the command line embeds (a test fails until you do)
cd cli && cargo run --example dump_schema > schema.json

# The documentation: the generator's tests, and that docs/CLI.md is what it makes
npm run check:docs

# Lists every place that can make a network call and fails on new ones
npm run check:outbound
```

### Database

- Migrations are plain SQL in `src-tauri/migrations/`, embedded and applied on startup. The version is SQLite's `user_version`; a database from a newer app is refused.
- Add a change as the next numbered file and append it to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit a shipped migration.
- Money is integer minor units (`*_minor` columns). Constraint tests: `src-tauri/src/schema_tests.rs`.

### Demo data

Sixteen months of demo data with dates relative to today (`src-tauri/seeds/dev.sql`): categories, changing budgets, recurring items (paid, overdue, paused, ended), a deleted category, a few hundred transactions, debts (four friends, a ledger, shared bills, a guest, overdue recurring payments), and awkward text (accents, Romanian and Japanese letters, quotes, formula-like cells) for the exports.

To put it in the desktop app, quit the app and run:

```bash
cd src-tauri
cargo run --example dev_vault -- ~/.local/share/org.meowny.app
```

This **deletes the app's data** (the data file, its key in the OS keychain, the settings file and any backups in that folder, as "delete all my data" does), sets the app up again with the demo data and prints the new recovery key. Lock settings are kept. The data file has no passphrase unless you set `MEOWNY_DEV_PASSPHRASE`. The folder above is the Linux data folder.

To try the command API on demo data, without the app or your own data:

```bash
cd src-tauri
cargo run --example api_demo
```

It prints a read token, a write token and `curl` lines, and serves until Ctrl+C. In the desktop app, Settings, command line access, turns the API on (off by default) and makes tokens.

To load the demo data into a separate file the desktop app does not open:

```bash
cargo run --example seed -- /path/to/dev.db <64-hex-character-key>
```

It refuses a database that already has data.

### Continuous integration

| Workflow | What it does |
|---|---|
| `ci.yml` | Lints, builds and tests the UI, checks the docs, on every push and pull request |
| `desktop.yml` | Outbound check and `cargo test` for the app and `cli/`, then builds the app. Pull requests build on Linux only; pushes to `main` also build on macOS and Windows. Can be run by hand for any branch |
| `security.yml` | `npm audit`, `cargo audit` for the app and `cli/`, and a secret scan, on pushes to `main`, pull requests and weekly |
| `pages.yml` | Builds and publishes the CLI reference site |

## Roadmap

Full plan: [docs/PLAN.md](docs/PLAN.md).

| Phase | What | State |
|---|---|---|
| 0 to 2 | Setup, encrypted database, app security | Done |
| 3 to 5 | Categories, transactions, budgets, recurring items, statistics and charts | Done |
| 6 | Encrypted backup and export | Done |
| 7 | Command line for scripts and AI agents (desktop only) | Done. Installing it with the app is phase 11 |
| 8 | Android app with a full offline copy, synced with the desktop at home by QR code (iOS later) | Built and tried between a real phone and a desktop. The CI job has not run on GitHub yet; nobody outside the project has reviewed the protocol |
| 9 | Documents: receipts and invoices attached to transactions, encrypted, chosen ones go to the phone | Built. Not yet tried on a real phone (camera, file chooser, opening a PDF) |
| 10 | Debts and expense splitting, in your own ledger | Built. The camera for a bill code and reminders on a phone with the app closed are not yet tried on a real phone |
| 11 | Signed installers, updates, `SECURITY.md`, `CONTRIBUTING.md`, install guides | Planned |

## Contributing

There is no `CONTRIBUTING.md` yet (phase 11). Until then:

1. Open an issue before a large change.
2. Run the checks that apply (see [Tests and checks](#tests-and-checks)). CI runs the same ones.
3. Add tests. A bug fix comes with a test that fails without it.
4. Keep dependencies few; say why in the pull request when you add one.
5. Keep the app local-first. A new network call must be on purpose, off until the user turns it on, and listed in `scripts/outbound-allowlist.txt`.
6. Every new desktop feature ships its command line commands in the same change. The phone app gets none.
7. After a route changes, regenerate `cli/schema.json` and the docs (see [Documentation](#documentation)). Never edit a shipped migration.
8. Update [docs/PLAN.md](docs/PLAN.md) and [docs/data-inventory.md](docs/data-inventory.md) when the change affects them.

## Reporting a security problem

Do not post vulnerability details in a public issue. A `SECURITY.md` with a private contact comes with the first release (phase 11). Until then, contact the maintainer privately through their GitHub profile, [@v1nc3t](https://github.com/v1nc3t).

## License

meowNY is free software under the [GNU Affero General Public License, version 3](LICENSE) (AGPL-3.0-only).
