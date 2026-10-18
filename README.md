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

meowNY is a desktop app for tracking income and expenses against monthly budgets. A [Tauri](https://tauri.app) shell (Rust) shows a React UI and keeps everything in one encrypted SQLite file ([SQLCipher](https://www.zetetic.net/sqlcipher/)) on your own machine. It also ships a command line program, `meowny`, so scripts and AI agents can use the running app without opening the data file.

> **Status: pre-release (version 0.1.0).** The desktop app and the command line work, and are tested, but there are no installers yet: you build it from source (see [Getting started](#getting-started)). The phone app and its sync are built and being tested; documents, and debts and expense splitting are built (the camera for a bill code and the phone's reminders are not yet tried on a real phone); see the [roadmap](#roadmap).

## Table of contents

- [Features](#features)
- [Privacy and security](#privacy-and-security)
- [Getting started](#getting-started)
  - [Requirements](#requirements)
  - [Program names](#program-names)
  - [Run the app](#run-the-app)
  - [Build a bundle](#build-a-bundle)
  - [Build the Android app](#build-the-android-app)
  - [Where the data lives](#where-the-data-lives)
- [Usage](#usage)
  - [Keyboard shortcuts](#keyboard-shortcuts)
  - [Command line](#command-line)
- [Documentation](#documentation)
- [Project structure](#project-structure)
  - [Naming and layout](#naming-and-layout)
- [Development](#development)
  - [Tests and checks](#tests-and-checks)
  - [Database](#database)
  - [Demo data](#demo-data)
  - [Continuous integration](#continuous-integration)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Reporting a security problem](#reporting-a-security-problem)
- [License](#license)

## Features

**Budgeting**

- Categories, and transactions (income and expenses) with search and filters.
- Monthly budgets with an amount per category, on the **categories** page: each side (expenses, income) shows what is allocated, used and not used, and each category has its own budget, which you change with the name in one dialog. Budgets are dated, so changing a limit does not rewrite old months; the **budget history** below lists every month.
- Recurring items (subscriptions, rent and the like): see what is due or overdue, pay it with one click to make a normal transaction, skip it, pause it or end it. Optional notifications for items that are due.
- Statistics and charts: budget against spending, trends, a year view, top categories, upcoming commitments and planned payments.
- Money is kept as whole minor units (cents), never as floating point numbers. Months and due dates follow the time zone you choose.
- Seven languages: English, Deutsch, Español, Français, Italiano, Português and Română.
- Keyboard shortcuts for the main actions.

**Documents (phase 9: built; not yet tried on a real phone)**

- Attach receipts, invoices and contracts to transactions and recurring items, on the computer and on the phone (from its file chooser or its camera). A file is kept **encrypted** next to your database, up to 25 MiB each, and is readable only while the app is unlocked. [A guide](docs/documents.md).
- **Preview** shows an image, a PDF, text, a CSV, a Word (.docx) or an Excel (.xlsx) file inside the app, with no plain copy and nothing run (PDFs are drawn by PDF.js, the one library added for it; the Word and Excel readers are the app's own).
- Each document has a name, a date and the things it is attached to. The type is found from the file's first bytes, never from its name, and a file of no known type is never opened from the app, only saved.
- Opening a PDF or a large picture hands a plain copy to your system's viewer; the copy is removed when you lock the app or close it, and the screen says so first.
- **Keep on phone** is a switch on each document. A document you did not switch on never leaves the computer; one added on the phone goes to the computer and leaves the phone only once the computer has confirmed it holds a good copy.
- Backups can include the files (a choice, on by default under 100 MB), as one encrypted file written as it is read. Exports list the documents by name and what they are attached to and never contain a file.
- From a script: `meowny documents add|list|show|update|link|unlink|delete|get`. A read-only token sees names, never a file; a copy comes out only when you allow it in the app.

**Debts and shared bills (phase 10: built; the phone's camera and reminders not yet tried on a real phone)**

- A **friend is a name** and nothing else: no phone number, no address, no contacts imported. A friend is archived, never deleted. A **guest** is a name typed for one bill.
- A **ledger** for each friend: money you lent, money you borrowed, and repayments, each with a date, an amount and a note, and optionally a document (a loan note). "Mark as paid" takes the whole balance or an amount you type; an amount over the debt is accepted and the screen says first that the balance will turn around. None of it is income or expense, and none of it touches a budget.
- **Split a bill** between you and friends and guests: equally, by percent, by shares or by exact amounts, with every amount shown before it is saved. The transaction keeps what you paid; **budgets, statistics, reports and exports count only your part**, and the list shows it with a mark such as `÷4`. A recurring item can be split too, and each payment of it is.
- "I borrowed" can add your share to your expenses in the same step (someone else paid).
- **Who owes what** on the debts page and on the dashboard, with the **recurring payments that are overdue** (one for each missed occurrence, from the day after it fell due) and a **history of debts** (friends' lines, and the recurring payments that were overdue: paid late, or still overdue) on tabs of the same page; a friend's page can save the statement as text, CSV or PDF, after showing it in a preview. The app sends it to nobody. Friends can be renamed or taken off the list from the list itself.
- **Bill codes.** On a shared bill, **show code** gives a friend a QR code (and its text) with their part, the bill and your name, nothing else. A friend with the app scans it (phone) or pastes it, sees the details, and accepts or denies; accepting makes the debt and, if ticked, an expense of their part (in another currency, with the rate they type). Nothing is sent or stored.
- **Reminders** about debts, one setting for all of them (in **settings**, for each device): every week, every 2 weeks or every month (or never) after the day each friend's debt last changed, and again after as long until it is settled. They ring on the computer while meowNY is open and, on the phone, from the system with the app closed. The text holds a count, never a name or an amount.
- Friends, entries and parts sync with the phone like everything else. [A guide](docs/debts.md). From a script: `meowny people|ledger|debts|splits ...` (`debts due`, `debts history`, `debts reminders`), `meowny transactions split`.

**Safety of your data**

- The database is encrypted at rest. The key lives in your operating system's keychain, optionally combined with a passphrase.
- A recovery key, shown once, so a forgotten passphrase or a lost keychain entry can be recovered.
- The app locks itself when idle, and optionally when minimised or closed. Locking drops the key from memory.
- Encrypted backups to a single file (`.meowbak`), restore, and scheduled backups to a folder you pick.
- Export to JSON, CSV, Excel and PDF, for the period and categories you choose (documents are listed by name, never their files). For JSON, CSV and Excel you can also keep to income or expenses, to an amount range (what it cost you) or to transactions with some text in the name or description, and for JSON and Excel choose which parts go in (transactions, categories, budgets, recurring items, documents, debts, due payments, the history of debts, the change history). You can also keep to the bills that are shared with friends, or to those that are not. A PDF report can add a **list of debts** (who owes what on the last day of its months), a **list of due payments** (the recurring payments that are overdue) and the history of debts in those months. The due payments are a part of their own in a JSON or Excel file too. The dates narrow the budgets and recurring items too: those that were in force or running in the range. The export screen shows a preview of the file that follows your choices: the start of the text for JSON (with its fields coloured) and CSV (plain text), the sheets of an Excel file as a grid, and the pages of a PDF report. The command line takes the same choices (`meowny export json|csv|xlsx|pdf --help`). The files are not encrypted; make an encrypted backup to keep data safe.
- "Delete all my data" removes the database, its key and the app's own backups.

**Your phone (phase 8: the sync, the screens and the Android app are built and have been reviewed on a real phone)**

- The phone keeps a full copy of your data, works on its own when you are away, and can change everything the desktop can. Back home, the two sync.
- There is no server of any kind. The desktop shows a QR code, the phone scans it, and the phone connects straight to the desktop on your home network. Every sync starts that way, and the code works once and only for a couple of minutes. Both devices must be on the same wifi network (a network that stops devices from talking to each other, such as many hotels, does not work; connect the desktop to the phone's hotspot there).
- Pairing shows six digits on the desktop that you type on the phone, so the check cannot be skipped by tapping "yes". Everything that is sent is encrypted with keys made from the pairing secret and that one-time code.
- Each sync can go both ways, from the phone to the computer only, or from the computer to the phone only. If the same thing was changed on both devices since they last met, the device that takes the changes in shows both versions side by side and you choose which one is true.
- On the phone the data is locked by a passphrase, and the phone erases itself after 5 wrong ones. The data comes back by pairing again with the desktop.
- The currency is changed on the desktop only, and the desktop reminds you to sync the phone.
- The first time the desktop listens, Windows or macOS may ask whether meowNY may accept connections on the network. Allow it for private networks only; nothing outside your home network is ever involved.
- On the phone there is a bar at the top (sync, settings, lock), tabs at the bottom that you can also swipe between, and a round "+" on every page to add a transaction in a few taps: the amount first, then the category from the ones you used lately. There is no export and no backup file on the phone, because the desktop keeps those; and Android's own cloud backup of the app is turned off, so the data never leaves your phone except by syncing with your computer.

**For scripts and AI agents (desktop only)**

- A [command line program](#command-line), `meowny`, with a command for every feature, `--json` output and stable exit codes.
- A local command API behind it that is off until you turn it on. Access needs a token that you make in the app, which can be read-only, expires and can be revoked.
- Exports, backups and copies of documents started from a script need a yes from you, in a dialog in the app, each time.

## Privacy and security

- **Local-first.** There is no server and no account, so there is no one who could hold your data. Nothing is sent anywhere, and there is no telemetry.
- **No network by default.** A CI check (`npm run check:outbound`) lists every place in the code that can open a connection and fails when a new one appears. The only optional network traffic planned is update checks, which you turn on yourself. Syncing with your phone stays on your home network.
- **Hardened window.** A strict content security policy, no remote content, one permission per command, no shell access from the web view, and links open in the system browser.
- **Safe logs.** Logs and crash reports never contain amounts, descriptions, tokens or keys.
- **The command API is for you only.** It listens on `127.0.0.1` and nothing else, refuses requests from web pages, answers "locked" while the app is locked, and does not count as activity, so the idle lock still works. Restoring a backup, changing the currency and deleting everything cannot be done through it at all.

What is stored, where, and who could see it is written down in [docs/data-inventory.md](docs/data-inventory.md). It is a technical note, not a privacy policy.

## Getting started

### Requirements

- Node.js 22 or newer, and npm
- Rust (stable, 1.77 or newer) from <https://rustup.rs>
- Tauri's system libraries for your OS: <https://v2.tauri.app/start/prerequisites/>
  - Linux (Debian/Ubuntu): `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev build-essential pkg-config`
  - macOS: Xcode command line tools
  - Windows: Microsoft C++ Build Tools and WebView2
- A secret service for the key: macOS Keychain, Windows Credential Manager, or on Linux GNOME Keyring or KWallet running in your session. Without one, the first run fails with "key unavailable".

### Program names

The project has two programs, and they do not share a name, so both can be on your `PATH`:

| Program | Name | Source |
|---|---|---|
| Desktop app | `meowny-desktop` | `src-tauri/` |
| Command line | `meowny` | `cli/` |

The window and the installers are still called meowNY. Only the name of the program file differs. The data folder (`org.meowny.app`) is the same, so nothing moves.

### Run the app

From the repository root:

```bash
git clone git@github.com:v1nc3t/meowNY.git
cd meowNY
npm install
npm run tauri dev
```

The first run compiles the Rust side, which takes a few minutes. It starts the Vite dev server for the UI and opens a window. Edits to `ui/src` hot-reload; edits to `src-tauri/src` rebuild and restart the app.

On first launch the app asks for a currency and an optional passphrase, shows your recovery key, and creates the encrypted data file. Settings (currency, language, time zone, app lock, backup, command line access, delete all data) are at `/app/settings`.

`npm run dev --workspace ui` on its own opens the UI in a browser, but there is no backend there: every screen talks to the Rust side, so the UI only works in the desktop window.

### Build a bundle

```bash
npm run tauri build
```

This builds a release version of the app and the installer formats Tauri supports for your system. The result is not signed, and signed installers for all three systems are planned for the first release.

### Build the Android app

The phone app is the same UI and the same Rust core, packaged by Tauri mobile. It has been built, installed and tried on one phone at home (see [docs/PLAN.md](docs/PLAN.md), phase 8). The CI job `.github/workflows/android.yml` builds a debug-signed APK on every change to the app, and keeps it as a download; building it by hand needs:

- Java 21, the Android SDK, and the Android NDK 27 (`sdkmanager "ndk;27.2.12479018"`), with `ANDROID_HOME`, `NDK_HOME` and `ANDROID_NDK_HOME` set, and the NDK's `toolchains/llvm/prebuilt/<system>/bin` folder on your `PATH` (SQLCipher is built with its own OpenSSL, which needs the NDK compilers)
- The Rust target for the phone: `rustup target add aarch64-linux-android`

```bash
npx tauri android init                        # makes src-tauri/gen/android, which is not kept in git
python3 scripts/android-patch.py              # no Android backup of the app's folder, no screenshots of it
npx tauri android build --apk --target aarch64 --debug
adb install src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

To try the phone against a desktop without a window (dev only), `cargo run --example sync_desktop -- <folder> setup`, then `pair` or `sync both|to_desktop|to_phone`; it prints the code and the six digits. A build for testing leaves out the screenshot block and adds a field to paste the code in: `MEOWNY_ALLOW_SCREENSHOTS=1 python3 scripts/android-patch.py` and `VITE_MEOWNY_TEST_PASTE=1 npx tauri android build ...`. Never install that build for real use.

The phone build leaves out the Excel and PDF export and the file dialogs (the phone has no export or backup file), so it has fewer dependencies and is smaller than a computer build with the same code. `cargo run --release --example bench` times the usual work with ten years of data (dev only).

The APK is signed with a debug key, which is enough to install it by hand ("install unknown apps" on the phone, or `adb install`). A release key and a store listing belong to the release phase. Run `python3 scripts/android-patch.py` again after every `tauri android init`: it keeps Android's backup from copying the encrypted data and its secret to a Google account, and it makes the app's window secure.

### Where the data lives

| OS | Folder |
|---|---|
| Linux | `~/.local/share/org.meowny.app/` |
| macOS | `~/Library/Application Support/org.meowny.app/` |
| Windows | `%APPDATA%\org.meowny.app\` |

It holds `meowny.db` (encrypted; while the app runs there are also `meowny.db-wal` and `meowny.db-shm`, encrypted too) and `vault.json` (lock settings, the wrapped keys and the passphrase salt). Documents add a folder `documents/` (one sealed file per document, with names that say nothing about it) and, only while a file is open in another program, `open/` (plain copies for the system's viewer, removed when you lock the app or close it). While the command API is on, it also holds `api.json` (the API version and port, no secrets). The key is in the OS keychain under the service name `org.meowny.app`.

To start over in development, use Settings, delete all my data. That removes the files and the keychain entry together. Deleting the folder by hand leaves a stale keychain entry, which is harmless and gets replaced on the next first run.

## Usage

### Keyboard shortcuts

Press `?` in the app to see them all. Hovering or focusing a control shows its shortcut. Shortcuts are ignored while typing in a field and inside dialogs.

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

All shortcuts are defined in one table, `ui/src/keys/shortcuts.ts`.

### Command line

`meowny` drives the running desktop app from a terminal, for scripts and AI agents. It is a small standalone program (under 1 MB, one dependency, `serde_json`) in `cli/`. It never opens the data file: it talks to the app over a local connection (`127.0.0.1` only) that is off until you turn it on, and every request needs a token. The command line exists on the desktop only; the planned phone app has none.

**Set it up**

1. In the app, open Settings, then "command line access", and turn it on. Make a token: read-only unless a script has to change things, and choose how long it lasts. The secret (`mny_` and 64 hex characters) is shown once.
2. Build the program. This puts `meowny` in cargo's bin folder (`~/.cargo/bin`), which must be on your `PATH`:

   ```bash
   cargo install --path cli
   ```

3. Give it the token, either once with `login` (read from standard input and saved readable only by you), or in the environment. There is deliberately no `--token` flag, because other users can read command lines:

   ```bash
   echo "$TOKEN" | meowny login
   # or
   export MEOWNY_TOKEN=mny_...
   ```

4. Check that it works. The app has to be running and unlocked for every command:

   ```bash
   meowny status
   ```

The desktop app's program is `meowny-desktop` (see [Program names](#program-names)), so the two never clash on your `PATH`. The command line is still built on its own and not as part of the app.

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

- Commands are `meowny <noun> <verb>`, worked out from the app's own list of routes.
- Data goes to standard output (`--json` for JSON on one line, otherwise a plain table). Messages and errors go to standard error. With `--json` an error is `{"code": "...", "message": "..."}`.
- It never asks anything: every input is a flag or standard input. Commands that delete or end something need `--yes`. A new transaction gets a fresh `--client-uuid`, so a retry cannot record it twice.
- Money stays a string end to end, and dates are ISO 8601.
- The token is found in `--token-file`, then `MEOWNY_TOKEN`, then `MEOWNY_TOKEN_FILE`, then the file `login` saved (`~/.config/meowny/token` on Linux). A token file that other users can read is refused.
- `MEOWNY_DATA_DIR` or `--data-dir` says where the app keeps its data (where it writes its port); `MEOWNY_PORT` or `--port` skips that.
- Exports (`export json|csv|xlsx|pdf|debts`), backups (`backup create`) and copies of documents (`documents get`) are not done on a token's say-so. Each one is described in a dialog in the app (which token, what, the exact file) and is written only if you click Allow. The file must have the right ending, be in a folder that exists, and not exist yet: nothing is ever overwritten. Asking again right after a "no" is refused for 30 seconds.

| Exit code | Means |
|---|---|
| 0 | done |
| 1 | the app is not reachable, the answer was not understood, or another error |
| 2 | bad usage, or a value the app refused |
| 3 | no token, a wrong or expired token, a token that may not do this, the person said no in the app, or the app is locked |
| 4 | not found |

**Try it without the app.** `cargo run --example api_demo` in `src-tauri/` serves demo data and prints a read token, a write token and a ready command line. There, the questions an export or a backup asks are answered in that terminal (y or n) instead of in a dialog.

Every command, flag and exit code is documented, with real output, in the [CLI reference](https://v1nc3t.github.io/meowNY/) and in [docs/CLI.md](docs/CLI.md).

## Documentation

| Document | What it is |
|---|---|
| [CLI reference site](https://v1nc3t.github.io/meowNY/) | One static page in the style of an API reference: a card per command with its flags and a real example, the exit and error codes, a page for AI agents, and the HTTP API behind the command line |
| [docs/CLI.md](docs/CLI.md) | The same text in one Markdown file, easier to read in an editor or to give to an agent |
| [docs/PLAN.md](docs/PLAN.md) | The build plan and roadmap, phase by phase, with what is done |
| [docs/data-inventory.md](docs/data-inventory.md) | What data the app holds, where, which network calls exist, and who could see it |
| [docs/sync-design.md](docs/sync-design.md) | How the phone syncs with the desktop: the QR codes, the encryption, the merge rules, the screens (phase 8) |
| [docs/documents.md](docs/documents.md) | A guide to documents: adding, attaching, opening, keeping on the phone, backups, exports and scripts (phase 9) |
| [docs/debts.md](docs/debts.md) | A guide to debts and shared bills: friends, the ledger, splitting, bill codes, reminders, files to send (phase 10) |
| [docs/documents-design.md](docs/documents-design.md) | How documents are built: the sealed file format, keys, names and types, the sync of the bytes, the backup format, the screens and the checklist for the real phone |

The reference site is made from the program, so it cannot say something the program does not do. `cli/tests/docs.rs` runs one tour of every command and every way to fail against a real app, records what came out (`docs/site/commands.json`, `docs/site/examples.json`), and fails when a command, a flag, a message or an example changes without the docs being redone. The prose around those facts is `docs/site/content.mjs`; the generator stops with an error if the program has a command, a group or an error code that the prose does not cover. There are no scripts, fonts or images, and the page asks nothing from anywhere else.

The site is published by `.github/workflows/pages.yml` on every push to `main` that touches `docs/`. It needs the repository's Settings, Pages, Source set to "GitHub Actions" once.

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
│   │                  the command API (api/) and the access tokens (tokens.rs), the phone sync (sync/)
│   │                  and the documents (documents/: sealed files, names and types, the library)
│   ├── migrations/    Schema changes as numbered SQL files
│   ├── seeds/         Demo data
│   └── examples/      dev_vault, seed and api_demo
├── cli/               The `meowny` command line program (Rust, one dependency)
├── docs/              Everything written for people: PLAN.md, data-inventory.md, sync-design.md, documents.md, documents-design.md, debts.md, debts-design.md, CLI.md, postgres-schema.txt
│   └── site/          Generator of the CLI reference site and CLI.md (plain Node, no dependencies)
├── scripts/           Checks CI runs on the whole repo: check-outbound and its allowlist
└── .github/workflows/ ci.yml, desktop.yml, security.yml and pages.yml
```

### Naming and layout

Where things go:

- A top-level folder is one thing you build or run (`ui`, `src-tauri`, `cli`) or one kind of support file (`docs`, `scripts`, `.github`). `src-tauri` keeps its name because the Tauri tools look for it.
- Anything written for people goes in `docs/`. Code that makes a document goes next to it, in `docs/site/`. Checks that run over the whole repo go in `scripts/`.
- Nothing is named after a design that is gone. The old server and its Postgres setup are only in git history.

Inside `ui/src/`:

| Folder | Holds |
|---|---|
| `pages/` | One screen per route, `NamePage.tsx`. Helpers that only that page uses sit beside it (`dashboardMath.ts`) |
| `components/` | Pieces of UI that are not a whole screen: modals, dropdowns, the sidebar |
| `layouts/` | The frame around the pages |
| `hooks/` | React hooks, `useName.ts` |
| `lib/` | Plain TypeScript with no React: money, caches, list helpers |
| `api/` | The calls into the Rust side, one file per area. `invoke.ts` is the one place that calls Tauri |
| `vault/`, `i18n/`, `keys/` | A feature that keeps its own screens, logic and tests together |

File names:

| What | Rule | Example |
|---|---|---|
| Folders | Lowercase, one word where possible, `kebab-case` otherwise | `ui`, `src-tauri` |
| Rust files | `snake_case`. A module with children is `x.rs` plus a folder `x/`, never `mod.rs` (only `cli/tests/common/mod.rs`, which Cargo needs). The tests of `x` sit beside it in `x_tests.rs` | `recovery_file.rs`, `api/read_tests.rs` |
| Migrations | `NNNN_name.sql`, numbered, never edited once shipped | `0001_tables.sql` |
| React components and pages | `PascalCase.tsx`, one component per file | `DashboardPage.tsx` |
| Other TypeScript | `camelCase.ts`. A test sits beside its file as `name.test.ts` | `displayFormat.ts`, `money.test.ts` |
| Markdown and text | `kebab-case`. The few files people open first are UPPERCASE | `data-inventory.md`, `README.md`, `PLAN.md` |
| Workflows | Named for what they do, not for the host | `ci.yml`, `pages.yml` |

How the parts fit: the UI calls Rust commands through Tauri; the Rust side owns the database, the keys and every rule. The command line is a client of the running app, through the local command API, and has no other way to the data. The list of API routes is one table in Rust, and `meowny schema`, the command line's commands and the docs are all made from it.

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

Schema migrations are plain SQL in `src-tauri/migrations/`, embedded in the app and applied on startup. The version is stored in SQLite's `user_version`, and a database from a newer app version is refused. Add a change as the next numbered file and append it to `MIGRATIONS` in `src-tauri/src/migrate.rs`. Never edit a migration that has shipped.

Money is stored as integer minor units (`*_minor` columns). Constraint tests are in `src-tauri/src/schema_tests.rs`.

### Demo data

There are sixteen months of demo data, with dates relative to today (`src-tauri/seeds/dev.sql`): categories, budgets that change over time, recurring items that are paid, overdue, paused and ended, a deleted category and a few hundred transactions, including awkward text (accents, Romanian and Japanese letters, quotes, cells that look like formulas) for trying the exports.

To put it in the desktop app, quit the app and run:

```bash
cd src-tauri
cargo run --example dev_vault -- ~/.local/share/org.meowny.app
```

This **deletes the app's data** (the data file, its key in the OS keychain, the settings file and any backups in that folder, as "delete all my data" does), sets the app up again with the demo data and prints the new recovery key. The lock settings are kept. The data file has no passphrase unless you set `MEOWNY_DEV_PASSPHRASE` for the run. The folder above is where the app keeps its data on Linux. Run the command again to start over.

To try the command API for the command line on demo data, without the app or your own data:

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

### Continuous integration

| Workflow | What it does |
|---|---|
| `ci.yml` (CI) | Lints, builds and tests the UI, and checks the documentation, on every push and pull request |
| `desktop.yml` (Desktop) | Runs the outbound check and `cargo test` for the app and for `cli/`, then builds the app. Pull requests build on Linux only (about 2.5 minutes); pushes to `main` also build on macOS and Windows. You can run all three for any branch from the Actions tab with "Run workflow" |
| `security.yml` (Security) | `npm audit`, `cargo audit` for the app and for `cli/`, and a secret scan, on pushes to `main`, pull requests and weekly |
| `pages.yml` (Docs site) | Builds and publishes the CLI reference site |

## Roadmap

The full plan, with what is done in each phase, is in [docs/PLAN.md](docs/PLAN.md).

| Phase | What | State |
|---|---|---|
| 0 to 2 | Setup, encrypted database, app security | Done |
| 3 to 5 | Categories, transactions, budgets, recurring items, statistics and charts | Done |
| 6 | Encrypted backup and export | Done |
| 7 | Command line for scripts and AI agents (desktop only) | Done. Installing it together with the app is part of phase 11 |
| 8 | Android app that keeps a full offline copy and syncs with the desktop at home by scanning a QR code (iOS comes later) | In progress: the sync, the phone vault, the screens and the Android app are built and have been tried between a real phone and a desktop at home; reviewed on a real phone; the CI job has not run on GitHub yet, and nobody outside the project has gone through the protocol |
| 9 | Documents: attach receipts and invoices to transactions, stored encrypted, and choose which ones go to the phone | Built: sealed files, the sync of their bytes, backups and exports, the screens, the command line. Not yet tried on a real phone (the camera, the file chooser, opening a PDF there) |
| 10 | Debts and expense splitting: who owes you and whom you owe, and bills shared with other people, in your own ledger | Built: friends, the ledger, split bills (budgets and statistics count only your part), the screens, summaries as text, CSV or PDF, documents on entries, bill codes a friend scans or pastes, reminders, the command line and the sync with the phone. The camera for a bill code and the reminders that ring on a phone with the app closed are not yet tried on a real phone |
| 11 | Signed installers, updates, `SECURITY.md`, `CONTRIBUTING.md` and install guides | Planned |

## Contributing

Contributions are welcome. There is no `CONTRIBUTING.md` yet (it comes with phase 11), so until then:

1. Open an issue before a large change, so the idea can be discussed first.
2. Run the checks that apply to your change (see [Tests and checks](#tests-and-checks)). CI runs the same ones.
3. Add tests with the change. A bug fix comes with a test that fails without it.
4. Keep dependencies few. Add one only when there is no reasonable way without it, and say why in the pull request.
5. Keep the app local-first. A new network call must be on purpose, off until the user turns it on, and listed in `scripts/outbound-allowlist.txt`.
6. Every new desktop feature ships its command line commands in the same change. The phone app gets none.
7. After a route changes, regenerate `cli/schema.json` and the docs (see [Documentation](#documentation)). Never edit a migration that has shipped.
8. Update [docs/PLAN.md](docs/PLAN.md) and [docs/data-inventory.md](docs/data-inventory.md) when the change affects them.

## Reporting a security problem

Please do not post the details of a vulnerability in a public issue. A `SECURITY.md` with a private disclosure contact comes with the first release (phase 11). Until then, contact the maintainer privately through their GitHub profile, [@v1nc3t](https://github.com/v1nc3t).

## License

meowNY is free software under the [GNU Affero General Public License, version 3](LICENSE) (AGPL-3.0-only).
