<h1 align="center">meowNY</h1>

<p align="center">
  A local-first budget app for your computer and your phone.<br>
  Your data stays on your devices, encrypted. No server, no account, no telemetry.
</p>

<p align="center">
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/desktop.yml"><img alt="Desktop build" src="https://github.com/v1nc3t/meowNY/actions/workflows/desktop.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/android.yml"><img alt="Android build" src="https://github.com/v1nc3t/meowNY/actions/workflows/android.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/ios.yml"><img alt="iOS build" src="https://github.com/v1nc3t/meowNY/actions/workflows/ios.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/security.yml"><img alt="Security" src="https://github.com/v1nc3t/meowNY/actions/workflows/security.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
  <img alt="Platforms: Linux, macOS, Windows, Android, iOS" src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows%20%7C%20Android%20%7C%20iOS-lightgrey">
</p>

<p align="center">
  <a href="https://v1nc3t.github.io/meowNY/">Website</a> ·
  <a href="https://v1nc3t.github.io/meowNY/guide.html">Guide</a> ·
  <a href="https://v1nc3t.github.io/meowNY/cli.html">CLI reference</a> ·
  <a href="docs/PLAN.md">Roadmap</a>
</p>

## Contents

- [About](#about)
- [Features](#features)
- [Status](#status)
- [Getting started](#getting-started)
- [Command line](#command-line)
- [Privacy and security](#privacy-and-security)
- [Documentation](#documentation)
- [Development](#development)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Reporting a security problem](#reporting-a-security-problem)
- [License](#license)

## About

meowNY tracks income and expenses against monthly budgets. It is a [Tauri](https://tauri.app) app: a Rust core and a React UI, with all data in one encrypted SQLite file ([SQLCipher](https://www.zetetic.net/sqlcipher/)) on your machine. A phone app (Android and iPhone) keeps a full offline copy and syncs with the computer over your home wifi. A command line program, `meowny`, lets scripts and AI agents use the running app.

## Features

- **Budgeting:** categories, transactions, monthly budgets, recurring items (pay, skip, pause, end), statistics and charts. Seven languages, light and dark themes, keyboard shortcuts.
- **Documents:** receipts and invoices attached to transactions, stored encrypted, previewed inside the app (images, PDF, text, CSV, Word, Excel).
- **Debts and shared bills:** a ledger per friend, bills split four ways (budgets count only your part), QR bill codes a friend can scan, reminders.
- **Phone:** an Android and iPhone app with a full offline copy. It syncs straight to the computer after a QR code, with no server, and shows conflicts side by side.
- **Backup and export:** encrypted `.meowbak` backups, on a schedule if you like. Exports to JSON, CSV, Excel and PDF.
- **Command line:** `meowny` has a command per feature, `--json` output and stable exit codes, with tokens you make and revoke.

Each feature is explained in the [guide](https://v1nc3t.github.io/meowNY/guide.html).

## Status

**Pre-release (0.1.0).** The desktop app, command line and Android app are built, tested, and have been tried on a real phone (sync, documents, bill codes, reminders). There are no installers yet, so you build from source. Signed installers come with the first release ([phase 11](docs/PLAN.md)). The iPhone app builds in CI but has not been tried on a real iPhone yet. Nobody outside the project has reviewed the sync protocol yet; that review comes before the first release.

## Getting started

You need Node.js 22+, Rust 1.90+, [Tauri's system libraries](https://v2.tauri.app/start/prerequisites/) and a system keychain (macOS Keychain, Windows Credential Manager, GNOME Keyring or KWallet).

```bash
git clone https://github.com/v1nc3t/meowNY.git
cd meowNY
npm install
npm run tauri dev        # the first run compiles the Rust side (a few minutes)
```

The first launch asks for a currency and an optional passphrase, and shows your **recovery key once**: write it down. `npm run tauri build` makes an installer for your system (not signed yet).

**Android:** every change builds a debug APK in GitHub Actions (the **Android** workflow, artifact `meowny-android-debug`) that you can sideload. To build it yourself, see [docs/development.md](docs/development.md#android).

**iPhone:** every change builds an unsigned IPA (the **iOS** workflow, artifact `meowny-ios-unsigned`). Install it with [AltStore](https://altstore.io) or [SideStore](https://sidestore.io), which sign it with your own Apple ID. With a free Apple ID the app must be refreshed every 7 days; AltStore does that for you.

**Your data** lives in `~/.local/share/org.meowny.app/` (Linux), `~/Library/Application Support/org.meowny.app/` (macOS) or `%APPDATA%\org.meowny.app\` (Windows). The key is in your system keychain.

## Command line

`meowny` drives the running desktop app from a terminal. It never opens the data file: it talks to the app over `127.0.0.1`, which is off until you turn it on, and every request needs a token.

1. In the app: **settings › command line access**. Turn it on and make a token.
2. Install it: `cargo install --path cli`
3. Give it the token: `echo "$TOKEN" | meowny login`

```bash
meowny status
meowny transactions list --limit 5
meowny stats budget --month 2026-10 --json
meowny debts summary
```

Exports, backups and copies of documents also need your yes in the app, each time. Every command, flag and exit code, with real output: [CLI reference](https://v1nc3t.github.io/meowNY/cli.html) ([Markdown](docs/CLI.md)).

## Privacy and security

- **Local-first.** No server and no account, so no one else holds your data. No telemetry.
- **Encrypted at rest.** The key is in your OS keychain, optionally behind a passphrase. The app locks when idle and drops the key from memory.
- **No network by default.** A CI check (`npm run check:outbound`) lists every place in the code that can open a connection and fails when a new one appears. Phone sync stays on your home network.
- **Hardened window.** Strict content security policy, no remote content, no shell access from the web view.
- **Safe logs.** Logs and crash reports never contain amounts, descriptions, tokens or keys.

What is stored where, and who could see it: [docs/data-inventory.md](docs/data-inventory.md).

## Documentation

| Document | What it is |
|---|---|
| [Website](https://v1nc3t.github.io/meowNY/) | Landing page, the [guide](https://v1nc3t.github.io/meowNY/guide.html) and the [CLI reference](https://v1nc3t.github.io/meowNY/cli.html) |
| [docs/development.md](docs/development.md) | Build, Android, tests, project layout, demo data, the website, CI |
| [docs/PLAN.md](docs/PLAN.md) | Build plan and roadmap, phase by phase |
| [docs/data-inventory.md](docs/data-inventory.md) | What data the app holds, where, and which network calls exist |
| [docs/documents.md](docs/documents.md), [docs/debts.md](docs/debts.md) | Full guides to documents and to debts and shared bills |
| [docs/sync-design.md](docs/sync-design.md), [docs/documents-design.md](docs/documents-design.md), [docs/debts-design.md](docs/debts-design.md) | How sync, documents and debts are built |
| [docs/CLI.md](docs/CLI.md) | The CLI reference in one Markdown file |

## Development

```bash
cd src-tauri && cargo test                                # the Rust core
cd cli && cargo test                                      # the command line
npm run lint --workspace ui && npm test --workspace ui    # the UI
npm run check:docs                                        # the website and docs/CLI.md
npm run check:outbound                                    # fails on new network code
```

```text
ui/          React UI (Vite, TypeScript)
src-tauri/   the desktop app and Rust core: database, vault, sync, documents, debts, the command API
cli/         the meowny command line (Rust, one dependency)
docs/        documentation; docs/site/ builds the website
scripts/     whole-repo checks and helpers
```

Demo data, migrations, naming rules and CI: [docs/development.md](docs/development.md).

## Roadmap

| Phase | What | State |
|---|---|---|
| 0 to 5 | Encrypted database, app security, budgets, recurring items, statistics | Done |
| 6 | Encrypted backup and export | Done |
| 7 | Command line for scripts and AI agents | Done |
| 8 | Android app and sync with the desktop, no server | Done |
| 9 | Documents: encrypted receipts and invoices | Done |
| 10 | Debts and expense splitting | Done |
| 11 | Signed installers, updates, `SECURITY.md`, `CONTRIBUTING.md`, install guides | Next |
| 12 | iOS app (builds; App Store later), shared bills between app users, widgets | Started |

Details and open points: [docs/PLAN.md](docs/PLAN.md).

## Contributing

There is no `CONTRIBUTING.md` yet (phase 11). Until then:

1. Open an issue before a large change.
2. Run the checks above; CI runs the same ones. A bug fix comes with a test that fails without it.
3. Keep dependencies few, and say why in the pull request when you add one.
4. Keep the app local-first: a new network call must be on purpose, off until the user turns it on, and listed in `scripts/outbound-allowlist.txt`.
5. A new desktop feature ships its command line commands in the same change. After a route changes, regenerate `cli/schema.json` and the docs ([how](docs/development.md#the-website)).
6. Update [docs/PLAN.md](docs/PLAN.md) and [docs/data-inventory.md](docs/data-inventory.md) when the change affects them.

## Reporting a security problem

Do not post vulnerability details in a public issue. A `SECURITY.md` with a private contact comes with the first release. Until then, contact the maintainer privately through their GitHub profile, [@v1nc3t](https://github.com/v1nc3t).

## License

meowNY is free software under the [GNU Affero General Public License, version 3](LICENSE) (AGPL-3.0-only).
