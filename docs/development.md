# Development

Everything needed to build, test and change meowNY. The [README](../README.md) is the short version.

## Contents

- [Requirements](#requirements)
- [Run and build the desktop app](#run-and-build-the-desktop-app)
- [Android](#android)
- [The data folder](#the-data-folder)
- [Project structure](#project-structure)
- [Tests and checks](#tests-and-checks)
- [Database](#database)
- [Demo data](#demo-data)
- [The website](#the-website)
- [Continuous integration](#continuous-integration)

## Requirements

- Node.js 22 or newer, and npm
- Rust (stable, 1.90 or newer for the app; 1.77 for `cli/`) from <https://rustup.rs>
- Tauri's system libraries: <https://v2.tauri.app/start/prerequisites/>
  - Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`
  - macOS: Xcode command line tools
  - Windows: Microsoft C++ Build Tools and WebView2
- A secret service for the key: macOS Keychain, Windows Credential Manager, or GNOME Keyring or KWallet on Linux. Without one, the first run fails with "key unavailable".

## Run and build the desktop app

| Program | Name | Source |
|---|---|---|
| Desktop app | `meowny-desktop` | `src-tauri/` |
| Command line | `meowny` | `cli/` |

The window and installers are called meowNY. The data folder is `org.meowny.app`.

```bash
npm install
npm run tauri dev      # run the app
npm run tauri build    # a release app and installers for this system (not signed yet)
```

- The first run compiles the Rust side (a few minutes). Edits to `ui/src` hot-reload; edits to `src-tauri/src` rebuild.
- First launch asks for a currency and an optional passphrase, shows the recovery key, and creates the data file.
- `npm run dev --workspace ui` opens the UI in a browser, but without a backend it only works in the desktop window.

## Android

Same UI and Rust core, packaged by Tauri mobile. CI (`.github/workflows/android.yml`) builds a debug-signed APK on every app change (artifact `meowny-android-debug`). By hand you need:

- Java 21, the Android SDK, NDK 27 (`sdkmanager "ndk;27.2.12479018"`), with `ANDROID_HOME`, `NDK_HOME` and `ANDROID_NDK_HOME` set and the NDK's `toolchains/llvm/prebuilt/<system>/bin` on your `PATH` (SQLCipher builds its own OpenSSL)
- The Rust target: `rustup target add aarch64-linux-android`

```bash
npx tauri android init                        # makes src-tauri/gen/android, which is not kept in git
python3 scripts/android-patch.py              # no Android backup of the app's folder, no screenshots of it
npx tauri android build --apk --target aarch64 --debug
adb install src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

- Run `scripts/android-patch.py` again after every `tauri android init`: it stops Android copying the encrypted data and its secret to a Google account.
- The APK has a debug key (enough to sideload). A release key and store listing are phase 11. The phone build leaves out Excel/PDF export and file dialogs.
- `cargo run --example sync_desktop -- <folder> setup`, then `pair` or `sync both|to_desktop|to_phone`, tries the phone against a desktop without a window. `cargo run --release --example bench` times common work on ten years of data.
- Test build (a field to paste the code, screenshots allowed): `MEOWNY_ALLOW_SCREENSHOTS=1 python3 scripts/android-patch.py` and `VITE_MEOWNY_TEST_PASTE=1 npx tauri android build ...`. Never install it for real use.

## The data folder

| OS | Folder |
|---|---|
| Linux | `~/.local/share/org.meowny.app/` |
| macOS | `~/Library/Application Support/org.meowny.app/` |
| Windows | `%APPDATA%\org.meowny.app\` |

- `meowny.db` (encrypted, plus `-wal` and `-shm` while running) and `vault.json` (lock settings, wrapped keys, passphrase salt).
- `documents/`: one sealed file per document; the names say nothing. `open/`: plain copies for the system viewer, only while open, removed on lock or close. `api.json`: API version and port (no secrets), only while the API is on.
- The key is in the OS keychain under `org.meowny.app`.
- To start over, use Settings, delete all my data. Deleting the folder by hand leaves a harmless stale keychain entry.

## Project structure

```text
meowNY/
├── ui/                React UI (Vite, TypeScript, Zod). Every screen talks to the Rust side
│   └── src/           pages, components, hooks, lib, api, and the vault, i18n and keys features
├── src-tauri/         The desktop app, `meowny-desktop` (Rust, Tauri 2)
│   ├── src/           vault and keys, database, budgets, recurring items, statistics, backup, export,
│   │                  the command API (api/), access tokens (tokens.rs), phone sync (sync/),
│   │                  documents (documents/), debts and splitting (debts/)
│   ├── migrations/    Schema changes as numbered SQL files
│   ├── seeds/         Demo data
│   └── examples/      dev_vault, seed, api_demo, sync_desktop and bench
├── cli/               The `meowny` command line program (Rust, one dependency)
├── docs/              Everything written for people; site/ builds the website and docs/CLI.md (plain Node)
├── scripts/           Whole-repo checks and helpers: check-outbound and its allowlist, android-patch, test vectors
└── .github/workflows/ ci.yml, desktop.yml, android.yml, security.yml and pages.yml
```

Naming and layout:

- Top-level folders: one thing you build or run (`ui`, `src-tauri`, `cli`) or one kind of support file (`docs`, `scripts`, `.github`). Anything written for people goes in `docs/`; code that makes a document sits in `docs/site/`; whole-repo checks go in `scripts/`.
- `ui/src/`: `pages/` (one `NamePage.tsx` per route), `components/`, `layouts/`, `hooks/` (`useName.ts`), `lib/` (plain TypeScript, no React), `api/` (calls into Rust; `invoke.ts` is the one place that calls Tauri), and `vault/`, `i18n/`, `keys/` (a feature with its own screens, logic and tests).
- Files: Rust `snake_case` (a module with children is `x.rs` plus `x/`, never `mod.rs`, except `cli/tests/common/mod.rs`; tests of `x` in `x_tests.rs`); migrations `NNNN_name.sql`, never edited once shipped; React `PascalCase.tsx`; other TypeScript `camelCase.ts` with `name.test.ts` beside it; Markdown `kebab-case` (first-open files UPPERCASE: `README.md`, `PLAN.md`); workflows named for what they do.
- The UI calls Rust commands through Tauri; Rust owns the database, keys and every rule. The command line is a client of the running app through the local command API. The route table is one table in Rust; `meowny schema`, the commands and the docs come from it.

## Tests and checks

```bash
# Rust: migrations, constraints, vault, key handling, sync, documents, debts, the command API
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

# The website: the generator's tests, and that docs/CLI.md is what it makes
npm run check:docs

# Lists every place that can make a network call and fails on new ones
npm run check:outbound
```

## Database

- Migrations are plain SQL in `src-tauri/migrations/`, embedded and applied on startup. The version is SQLite's `user_version`; a database from a newer app is refused.
- Add a change as the next numbered file and append it to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit a shipped migration.
- Money is integer minor units (`*_minor` columns). Constraint tests: `src-tauri/src/schema_tests.rs`.

## Demo data

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

It prints a read token, a write token and `curl` lines, and serves until Ctrl+C. Export and backup questions are answered in that terminal (y or n).

To load the demo data into a separate file the desktop app does not open:

```bash
cargo run --example seed -- /path/to/dev.db <64-hex-character-key>
```

It refuses a database that already has data.

## The website

`docs/site/` builds the website for GitHub Pages with plain Node and no dependencies: a landing page (`landing.mjs`), the guide (`guide.mjs`) and the command line reference (`content.mjs`), all in the same sidebar (`render.mjs`). The stylesheet is the app's `ui/src/styles/tokens.css` followed by `docs/site/site.css`, so the site has the app's colours, font and corners in light and dark.

The command line reference is made from the program. `cli/tests/docs.rs` runs a tour of every command and failure against a real app, records it (`docs/site/commands.json`, `docs/site/examples.json`), and fails when a command, flag, message or example changes without the docs being redone. The same text is written to `docs/CLI.md`.

```bash
# After the commands change: record them again, then rebuild the site
cd cli && UPDATE_DOCS=1 cargo test --offline --test docs
npm run docs                  # writes docs/CLI.md and the site in docs/site/_site/ (open docs/site/_site/index.html)
npm run check:docs            # what CI runs: the generator's tests, every page and link checked, docs/CLI.md up to date
```

Screenshots: put `dashboard.png`, `transactions.png`, `debts.png` and `phone.png` in `docs/site/images/` and rebuild. Until a file is there, the landing page shows an empty frame in its place.

The site is published by `.github/workflows/pages.yml` on pushes to `main` that touch `docs/`, `cli/schema.json` or the app's tokens (Settings, Pages, Source set to "GitHub Actions" once).

## Continuous integration

- `ci.yml`: lint, build and test the UI, and check the website, on every push and pull request.
- `desktop.yml`: outbound check and `cargo test` for the app and `cli/`, then build the app (Linux on pull requests; also macOS and Windows on `main`; can be run by hand for any branch).
- `android.yml`: builds the debug APK and uploads it (`meowny-android-debug`).
- `security.yml`: `npm audit`, `cargo audit` (app and `cli/`) and a secret scan, on `main`, pull requests and weekly.
- `pages.yml`: builds and publishes the website.
