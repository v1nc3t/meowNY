<p align="center"><img src="ui/public/favicon.svg" width="96" height="96" alt=""></p>

<h1 align="center">meowNY</h1>

<p align="center">
  A local-first budget app for your computer and your phone.<br>
  Your data stays on your devices, encrypted. No server, no account, no telemetry.
</p>

<p align="center">
  <a href="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/v1nc3t/meowNY/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/v1nc3t/meowNY/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/v1nc3t/meowNY"></a>
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
  <a href="https://ko-fi.com/v1nc3t"><img alt="Support on Ko-fi" src="https://img.shields.io/badge/support-Ko--fi-ff5e5b?logo=ko-fi&logoColor=white"></a>
</p>

<p align="center">
  <a href="https://github.com/v1nc3t/meowNY/releases/latest">Download</a> ·
  <a href="https://v1nc3t.github.io/meowNY/guide.html">Guide</a> ·
  <a href="https://v1nc3t.github.io/meowNY/cli.html">Command line</a> ·
  <a href="https://v1nc3t.github.io/meowNY/develop.html">Develop</a> ·
  <a href="#support">Support</a>
</p>

## About

meowNY is a local-first budget app for your computer and your phone. It is a [Tauri](https://tauri.app) app: a Rust core and a React UI, with all data in one encrypted SQLite file on your machine.

## Features

- **Budgeting:** categories, monthly budgets, recurring items and statistics. Expense, income and transfer share one add dialog.
- **Accounts:** bank, savings, cash and credit card, with groups, other currencies, return rates, transfers and net worth.
- **Stashes:** money set aside in an account for a goal, or a term deposit at a rate.
- **Documents:** receipts and invoices attached to transactions, encrypted, previewed inside the app.
- **Debts and shared bills:** a ledger per friend, split bills, QR bill codes and reminders. Loans and mortgages, with the payoff month.
- **Phone:** an Android app with a full offline copy. It syncs straight to the computer over your home wifi.
- **Backup and export:** encrypted backups, on a schedule if you like. Exports to JSON, CSV, Excel and PDF.
- **Command line:** `meowny` lets scripts and AI agents use the running app.
- Seven languages, light and dark themes, keyboard shortcuts.

## Install

1. Download the build for your system from the [releases page](https://github.com/v1nc3t/meowNY/releases/latest): Linux (`.deb`, AppImage), Windows or Android.
2. Install it: [step by step](https://v1nc3t.github.io/meowNY/guide.html#install).
3. Open meowNY and press **get started**.

iPhone and Mac are not available yet: see [support](#support).

**Pre-release.** Tried on a real Android phone, not yet on a real iPhone. The desktop installers are not signed yet. Nobody outside the project has reviewed the sync protocol; that review comes before 1.0.

## Run from source

1. Install Node.js 22 or newer, Rust stable and the [Tauri system libraries](https://v2.tauri.app/start/prerequisites/).
2. Run `npm install`.
3. Run `npm run tauri dev`. Edits to `ui/src` reload the app.

To build installers, run the tests or add a feature: [develop](https://v1nc3t.github.io/meowNY/develop.html).

## Privacy and security

- No server, no account, no telemetry.
- The database is encrypted with a key kept in your system keychain. The app locks when idle.
- A CI check fails on any new network code.

What is stored where, and what ever leaves the device: [privacy](https://v1nc3t.github.io/meowNY/guide.html#privacy). To report a security problem, see [SECURITY.md](SECURITY.md). To help, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentation

- [Website](https://v1nc3t.github.io/meowNY/): the guide, the command line reference (also as [Markdown](https://v1nc3t.github.io/meowNY/CLI.md)) and the develop page.
- Design notes, for how it is built and how the screens are laid out: [sync](docs/sync-design.md), [documents](docs/documents-design.md), [debts](docs/debts-design.md), [accounts](docs/accounts-design.md), [stashes](docs/stash-design.md), [loans and deposits](docs/loans-deposits-design.md), [UI](docs/ui-design.md).

## Support

meowNY is free and stays free. The App Store and Google Play need paid developer accounts. To help put it there, [support it on Ko-fi](https://ko-fi.com/v1nc3t). Questions, bugs and ideas: [discussions](https://github.com/v1nc3t/meowNY/discussions).

## License

meowNY is free software under the [GNU Affero General Public License, version 3](LICENSE) (AGPL-3.0-only).
