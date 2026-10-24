<p align="center"><img src="ui/public/favicon.svg" width="96" height="96" alt=""></p>

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
</p>

<p align="center">
  <a href="https://github.com/v1nc3t/meowNY/releases/latest">Download</a> ·
  <a href="https://v1nc3t.github.io/meowNY/guide.html">Guide</a> ·
  <a href="https://v1nc3t.github.io/meowNY/cli.html">Command line</a> ·
  <a href="https://v1nc3t.github.io/meowNY/develop.html">Develop</a>
</p>

## About

meowNY tracks income and expenses against monthly budgets. It is a [Tauri](https://tauri.app) app: a Rust core and a React UI, with all data in one encrypted SQLite file on your machine. A phone app (Android and iPhone) keeps a full offline copy and syncs with the computer over your home wifi. A command line program, `meowny`, lets scripts and AI agents use the running app.

- **Budgeting:** categories, transactions, monthly budgets, recurring items, statistics. Seven languages, light and dark themes, keyboard shortcuts.
- **Documents:** receipts and invoices attached to transactions, encrypted, previewed inside the app.
- **Debts and shared bills:** a ledger per friend, split bills (budgets count only your part), QR bill codes, reminders.
- **Phone:** a full offline copy that syncs straight to the computer, with no server.
- **Backup and export:** encrypted backups, on a schedule if you like; exports to JSON, CSV, Excel and PDF.

Everything is explained on the [website](https://v1nc3t.github.io/meowNY/guide.html).

## Install

Linux (`.deb`, AppImage), Windows, Android and iPhone builds are on the [releases page](https://github.com/v1nc3t/meowNY/releases/latest). How to install and update each one: [the guide](https://v1nc3t.github.io/meowNY/guide.html#install). To build it yourself: [develop](https://v1nc3t.github.io/meowNY/develop.html).

**Pre-release.** Tried on a real Android phone, not yet on a real iPhone. The desktop installers are not signed yet. Nobody outside the project has reviewed the sync protocol; that review comes before 1.0.

## Privacy and security

No server, no account, no telemetry. The database is encrypted with a key kept in your system keychain, the app locks when idle, and a CI check fails on any new network code. What is stored where and what ever leaves the device: [privacy](https://v1nc3t.github.io/meowNY/guide.html#privacy).

To report a security problem, do not open a public issue: contact [@v1nc3t](https://github.com/v1nc3t) privately.

## Documentation

| Where | What |
|---|---|
| [Website](https://v1nc3t.github.io/meowNY/) | The guide, the command line reference (also as [Markdown](https://v1nc3t.github.io/meowNY/CLI.md)) and the develop page |
| [docs/sync-design.md](docs/sync-design.md), [docs/documents-design.md](docs/documents-design.md), [docs/debts-design.md](docs/debts-design.md) | How sync, documents and debts are built |

## License

meowNY is free software under the [GNU Affero General Public License, version 3](LICENSE) (AGPL-3.0-only).
