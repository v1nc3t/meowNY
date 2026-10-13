# meowNY: Build Plan (local-first)

Companion to `expense_tracker_schema.txt` (schema v3, written for Postgres; it is ported to SQLite in phase 1). Work top to bottom. Each phase ends with a "Done when" check, so you know when to move on.

**Size tags:** S = a few hours, M = a few days, L = a week or more (rough, for one person working part-time).

## Product goals

- **Local-first.** A standalone desktop app. No server, no accounts, no internet needed. Data lives in an encrypted SQLite file on your own machine.
- **Open source and easy to install.** Download, run. No Docker, no database to host.
- **Secure by design.** No network attack surface. Encrypted at rest, locked app, signed releases.
- **Phone copy that syncs.** The phone keeps its own offline copy and syncs with the desktop, also on networks you do not own.
- **CLI for AI agents.** Ships with the desktop app, client side only.
- **Optional bank connection** with your own aggregator key. No project-run backend, ever.

## Order and why

1. **Foundations and security first**, because the encrypted database, key handling and the data model are painful to retrofit.
2. **Core budgeting loop next**: categories, transactions, budgets. This is the product.
3. **Recurring, then charts**: both build on the core.
4. **Backup, export and import before anything that spreads your data to other devices.** The review queue built here is reused by phone capture and bank import.
5. **CLI, then phone sync and capture, then bank connection.** Packaging and release is a gate before sharing the app with anyone else.

Phone sync (phase 8) can move ahead of the CLI (phase 7) if you mostly use the app on your phone.

## What carries over from the server-based design

Kept: the React client (`client/src`), Zod schemas, money and date logic, and the schema design (effective-dated budgets, `client_uuid` idempotency, history triggers, constraint tests). Replaced: Express, PostgreSQL, Better Auth and the `server/` package. The old accounts, GDPR-service, admin board and deployment phases are dropped. There is no operator and no one else's data to protect, so what remains of them is: export your data, delete all your data, and the encrypted backup.

## Decisions already made

- App shell: Tauri (small, Rust, native OS keychain access). React client reused.
- Storage: SQLite encrypted with SQLCipher. Schema stays in TypeScript migrations.
- Query layer: Kysely with a SQLite dialect (revisit if the Tauri bridge makes Drizzle simpler).
- One local profile. No user ids in the schema.
- Budgets are effective-dated; recurring items use categories; history via database triggers.
- Money is handled as strings/decimals (or integer minor units) in code, never floats.
- Request validation with Zod at every boundary (UI to core, CLI to core, sync, import).
- Mobile-first rule: every new screen from phase 4 on is built mobile-first, so the phone app (phase 8) can reuse it.
- Time: months, due dates and "this month" are computed in the user's timezone setting, never in UTC by accident.
- The CLI never opens the database file directly. It talks to the running app through a local command API, described by a schema it can print (`meowny schema`).
- No telemetry. The only optional network traffic is the bank aggregator (phase 10), update checks and sync transports the user turns on.

## Open decisions (decide when you reach the phase)

| Decision | Needed by | Notes |
|---|---|---|
| License (AGPL, MIT or Apache-2.0) | Phase 0 | Affects how forks and hosted copies can be used |
| Query layer: Kysely or Drizzle on SQLite | Phase 0 | Pick after a small spike with SQLCipher in Tauri |
| Key handling: OS keychain only, or passphrase as well | Phase 2 | Passphrase protects better if the machine is shared |
| Chart library | Phase 5 | Any that works with the frontend |
| CLI transport (local socket or loopback HTTP) and token mechanism | Phase 7 | Tokens must be revocable and stored hashed |
| Phone app shape: PWA with WASM SQLite, or Tauri mobile / Capacitor | Phase 8 | PWA storage can be evicted by iOS Safari; a native wrapper avoids that |
| Which sync transports, in which order | Phase 8 | See phase 8; start with LAN plus Tailscale |
| Bank aggregator | Phase 10 | Do a spike first |

---

## Phase 0: Setup and decisions (S)

- [x] Repo layout and TypeScript setup, oxlint, no separate formatter
- [x] Request validation with Zod
- [x] Logging that never prints personal data
- [x] Test setup: unit tests plus integration tests against a real database
- [x] CI that runs lint, tests and migrations from scratch
- [x] Tauri app shell that opens the existing React client in a window
- [x] SQLCipher database opens from Rust and from the test setup; decide the query layer (see open decisions)
- [x] Choose a license and add `LICENSE`
- [x] CI also runs a dependency audit and a secret scan
- [x] CI builds the app for Linux, macOS and Windows (unsigned at this stage)
- [x] Start a one-page **data inventory**: what data, where it is stored, which optional network calls exist (`assets/data-inventory.md` is rewritten for local-first)

**Done when:** `npm run tauri dev` opens a window, it reads a row from an encrypted SQLite file, and CI is green on all three platforms.

## Phase 1: Database foundation (M)

- [x] Port the tables, indexes and foreign keys from the schema file to SQLite migrations. Remove user ids, auth tables and server-only tables. (`src-tauri/migrations/0001_tables.sql`, plain SQL, `STRICT` tables.)
- [x] Port CHECK constraints, partial unique indexes and triggers (history, `updated_at`). Postgres functions for statistics and GDPR become queries or Rust/TypeScript code. (CHECKs and partial indexes are in `0001_tables.sql`, triggers in `0002_triggers.sql`. The statistics and GDPR functions are not ported here; they are rebuilt in phases 5 and 6.)
- [x] Money storage decision applied: decimal strings or integer minor units, with constraints on the column (Integer minor units in `*_minor` columns, bounded 0..999,999,999,999; amounts > 0, planned amounts and limits >= 0.)
- [x] Migration runner that applies migrations on startup and refuses to open a database from a newer app version (`src-tauri/src/migrate.rs`, version kept in `user_version`. Startup should call `db::open_and_migrate`; the app does not open the database yet because key handling is phase 2.)
- [x] Constraint tests (these are your safety net), in `src-tauri/src/schema_tests.rs`:
  - [x] a transaction cannot use a missing category
  - [x] a budget's type must match its category's type
  - [x] only one GLOBAL budget per type and month
  - [x] a category type cannot change after creation
  - [x] the same recurring occurrence cannot be paid twice
  - [x] the same `client_uuid` cannot create two transactions
  - [x] audit rows appear for insert, update and delete, and are skipped when only `updated_at` changed
- [x] Dev seed script with demo categories, budgets and transactions (`src-tauri/seeds/dev.sql`, loaded by `src/seed.rs`; run `cargo run --example seed -- <db-path> <64-hex-key>` from `src-tauri/`. Dates are relative to today.)

**Done when:** a fresh database builds from migrations only and all constraint tests pass on SQLite.

## Phase 2: App security (M)

- [x] Database encrypted at rest. The key comes from the OS keychain, optionally combined with a passphrase. (`src-tauri/src/keys.rs`, `vault.rs`: random secret in the OS keychain; with a passphrase the key is HKDF(secret + Argon2id(passphrase)).)
- [x] Recovery key: shown once at setup and replaceable in Settings. It opens a separate wrapped copy of the database key, so "forgot my passphrase" and a lost keychain entry are both recoverable if it was saved. (`src-tauri/src/keys.rs`, `vault.rs`; the database key is random and stored only as two wrapped copies, one for everyday unlock and one for the recovery key.)
- [x] App lock: lock on idle timeout and on window close/minimize (configurable), optional biometrics where the OS offers it. Locked means the key is dropped from memory. (Idle timeout, minimize and close, configurable; connection closed on lock. Biometrics are not done: they need a platform plugin per OS and are left for later.)
- [x] Tauri hardening: strict CSP, no remote content, minimal allowed commands, no shell access from the webview, external links open in the system browser (Strict CSP, one permission per command, navigation guard, https links via the opener plugin.)
- [x] No network access unless a feature the user turned on needs it. A test or CI check lists every outbound call site. (`npm run check:outbound` in CI, allowlist in `scripts/outbound-allowlist.txt`. The old client code that still calls the server API is listed there and goes in phase 3.)
- [x] Safe logging: logs and crash reports never contain amounts, descriptions or keys (Fixed error codes only, panic hook prints the location only.)
- [x] "Delete all my data" action: wipes the database, keys and backups the app created, with confirmation (Needs typing DELETE. Also removes the keychain secret, which makes any leftover copy of the file unreadable.)
- [x] Settings: currency, locale, timezone (carried over from the old account page) (`client/src/pages/SettingsPage.tsx`, route `/app/settings`. The old `AccountPage` is removed in phase 3.)
- [x] First-run screen: choose currency, set lock method, show where the data file lives (`client/src/vault/`.)
- [x] Tests: wrong key cannot open the file, locked state holds no key, the database file contains no plaintext (Rust tests in `vault.rs`, `keys.rs`, `commands.rs`, `lib.rs`.)

**Done when:** the data file is unreadable without the key, the app locks itself, and you can wipe everything.

## Phase 3: Core budgeting: categories, transactions, budgets (L)

- [x] Existing React screens: category management, transaction form and list, budget setup (built against the old HTTP API)
- [x] Replace `client/src/http.ts` and the `fetch` calls with a typed command layer over Tauri (`invoke`), one place for validation and errors (`client/src/api/`: answers are checked with Zod, failures are fixed codes. `http.ts`, the login/signup/account/landing/legal pages, `ProtectedRoute` and the `better-auth` client dependency are deleted; the app opens straight into `/app` behind the vault gate.)
- [x] Categories CRUD. Soft delete writes the NULL budget row (schema Part 5-A). (`src-tauri/src/categories.rs`; names trimmed, 1 to 50 characters, unique among active categories of a type; the NULL budget row uses the current month in the user's timezone.)
- [x] Transactions CRUD with filters (date range, category, text), pagination, and `client_uuid` idempotency. Amount rules: positive, two decimals, sent as strings. (`src-tauri/src/transactions.rs`, `money.rs`. Amounts cross the boundary as `"12.50"` strings and are stored as integer cents. Two decimals are assumed for every currency; JPY or BHD need a per-currency exponent later. Text search ignores case for ASCII letters only.)
- [x] Budgets: set / remove a GLOBAL or CATEGORY budget from a given month, for both EXPENSE and INCOME (`src-tauri/src/budgets.rs`. Month is `YYYY-MM` and applies from then on; a null limit removes the budget from that month and keeps history. Category budgets must match the category type and be active.) Editing a month changes only that month: a month that already started keeps what it showed (the next month gets its own row if it would otherwise follow the change), while future months carry the newest value forward. The budgets page has a monthly history table (month, expense budget, spent, income budget, earned).
- [x] Budget overview: allocated vs unallocated, with a warning when categories exceed the global budget (Per side: budgeted, allocated, unallocated, actual, remaining and an over-allocated flag, computed from the effective budgets.)
- [x] Month filters and "current month" use the timezone setting, with a test around midnight at month end (`clock.rs` computes today and the month in the timezone setting; the transactions and budgets pages use it through the `today` command. Tests cover midnight at month end and year end.)
- [x] Tests: carry-over (a March budget applies in June until changed), removal via NULL, income budgets, idempotent create (`budgets.rs`, `transactions.rs`, `categories.rs`, `clock.rs`.)
- [x] Currency: pick from a list at first run; change it later in Settings with a typed exchange rate that converts every stored amount (transactions, planned amounts, recurring items, budget limits) in one transaction, rounded half up to the cent, or relabel only. No rate is fetched; the rate and date go to the history. (`src-tauri/src/currency.rs`, `client/src/components/ChangeCurrencyModal.tsx`. Two decimals are still assumed for every currency, so JPY, KRW and similar show `.00`.)

**Done when:** you can record income and expenses, set monthly budgets and category allocations, change them, and old months keep their old limits, all inside the desktop app.

## Phase 4: Recurring items (M)

- [ ] Recurring CRUD (income or expense, any category, interval unit + count, optional end date)
- [ ] Due-date function computed from `start_date` (not from the previous due date). **Unit-test month-end cases** (Jan 31, Feb 29, yearly on Feb 29).
- [ ] "Mark as paid" in one database transaction (schema Part 5-B), with optional actual amount and payment date
- [ ] Skip, pause, end; overdue and upcoming lists
- [ ] Due dates and "overdue" are evaluated in the timezone setting
- [ ] Frontend: recurring list with due/overdue state and a mark-as-paid action, built mobile-first
- [ ] Optional desktop notification for items due today (local only)
- [ ] Tests: paying twice is rejected, planned vs actual amount stored, paid transaction counts in its category's actual

**Done when:** a monthly subscription shows up as due, one click turns it into a normal transaction, and the next due date is correct.

## Phase 5: Statistics and charts (M)

- [ ] Command: budget vs actual for a month (categories + totals)
- [ ] Command: category trend over a date range
- [ ] Dashboard (built mobile-first): budget vs actual per category (bars/progress), month totals for income and expense, savings rate, year trend
- [ ] Extra stats from the schema: top categories, committed recurring cost per month, planned vs paid for recurring items
- [ ] Frontend formats money using the currency and locale setting
- [ ] Tests: numbers match hand-calculated examples, months with no budget return `budgeted: null`

**Done when:** the Food example works end to end: a budget of 400, transactions summing to 356.20, the chart shows both.

## Phase 6: Backup, export and import (M)

- [ ] **Encrypted backup**: one click writes a single encrypted file (passphrase chosen by the user), plus restore with a version check. Optional scheduled backups to a folder the user picks.
- [ ] **Export**: full JSON of all data, CSV of transactions. CSV safety: cells starting with `=`, `+`, `-` or `@` are prefixed with `'`.
- [ ] **Excel export (.xlsx)**: one workbook with sheets for transactions, categories, budgets, recurring items and a monthly summary. Dates are real dates, money cells are numbers with the currency format (converted from the stored strings only at write time). The same formula-cell safety as CSV applies to text cells.
- [ ] **PDF reports**: a printable report for a chosen month or date range, with totals for income and expense, savings rate, budget vs actual per category, the charts from phase 5, and optionally the transaction list. Generated locally, with no network calls. Options: page size, include/hide descriptions (so a report can be shared without private details).
- [ ] Export screen lets you pick the format (JSON, CSV, Excel, PDF), the date range and the categories, and shows what the file will contain before saving. A file with sensitive data is not encrypted unless you choose the encrypted backup, and the screen says so.
- [ ] **Import**: CSV and Excel (.xlsx) first, then OFX and CAMT/MT940. A saved column mapping per bank, a dry-run preview, and neutralisation of formula cells (Excel formulas are read as text and never evaluated).
- [ ] **Review queue**: imported rows land as PENDING. The user can accept, edit, ignore, set a category, or match to a recurring item ("mark as paid?"). Category rules pre-fill (merchant text to category).
- [ ] Deduplicate imports by a hash of date, amount and normalised description, and by provider transaction id when one exists
- [ ] Restore test: back up, wipe, restore on a clean install, compare all rows
- [ ] Export tests: the Excel totals and the PDF totals match the dashboard for the same month; an Excel file with a formula-looking description is neutralised on export and not evaluated on import; the PDF with descriptions hidden contains none
- [ ] Review the data inventory against what the system really stores

**Done when:** you can back up, wipe, restore, export to JSON, CSV, Excel and PDF, and import a bank CSV or Excel file into a review queue without duplicates.

## Phase 7: CLI for AI agents (M)

The whole app is usable from the terminal, so scripts and AI agents can drive it without the UI. The CLI is a client of the running desktop app. No server is introduced.

- [ ] Design rule: the CLI is a thin client over the app's local command API, described by a schema generated from the Zod types. It never opens the database file. Validation, scoping and encryption stay in one place. Every feature through phase 6 gets a command; if the API lacks something, fix the API, not the CLI.
- [ ] Local command API: listens only on a local socket or loopback, off by default, enabled in settings. Requests must carry a token.
- [ ] Package in `cli/` (TypeScript, one binary, e.g. `meowny`), installed by the desktop installer and available standalone. Reuses the Zod schemas for argument validation.
- [ ] Tokens: created in the app, revocable, stored hashed, with scopes (read-only by default, write opt-in), an expiry and a last-used time. Passed by `MEOWNY_TOKEN` or stored in a `0600` file. Never printed or logged. `export`, `delete-all` and backup restore are not allowed with a token and need the user at the app.
- [ ] If the app is locked, the CLI gets a clear exit code and no data.
- [ ] Command groups: `settings`, `categories`, `transactions`, `budgets`, `recurring`, `stats`, `import`, `export` (json, csv, xlsx, pdf), `backup`. Pattern `meowny <noun> <verb>`, with `--help` at every level.
- [ ] Agent-friendly output: `--json` on every command (stable shape, additive fields only), human table otherwise. Data on stdout, messages and errors on stderr.
- [ ] Non-interactive by default: no prompts; every input is a flag or stdin. Destructive commands need `--yes`. Transaction create sends a `client_uuid`, so retries are safe.
- [ ] Exit codes: 0 ok, 1 error, 2 bad usage, 3 auth failed or app locked, 4 not found. In `--json` mode errors are `{code, message}`.
- [ ] Money stays strings end to end; dates are ISO 8601. Bulk transaction import from CSV/JSON on stdin goes through the review queue.
- [ ] `meowny schema` prints all commands and arguments as JSON, so an agent can discover them.
- [ ] Docs: one `CLI.md` with an example per command, plus a short agent note (env var, `--json`, exit codes).
- [ ] Tests: each command against a real app instance (happy path, validation error, bad token, locked app, `--json` shape snapshot), plus one test that fails if a command-API route is missing from the schema or has no CLI command.
- [ ] From here on, every new feature phase ships its CLI commands in the same phase.

**Done when:** a script or agent holding only a token can record an expense, set a budget, read budget vs actual and queue an import, all with `--json` and no prompts.

## Phase 8: Phone app and sync (L)

The phone keeps its own full copy and works offline. Sync is separate from the way the devices reach each other, so transports can be added one at a time. The copy is small (about 10 MB for ten years of data), so storage is not a concern.

- [ ] **Design step first** (a short doc): an append-only change log with device ids and per-record versions; merge rules (transactions by `client_uuid`, budgets by effective date, categories by id); conflict policy (last writer wins per field, deletes are tombstones); schema version negotiation; what a full re-sync means.
- [ ] Phone shape decided (see open decisions). If it is a PWA: request persistent storage, show a warning if it is refused, and rely on the desktop copy as the authoritative one, so storage eviction costs only a re-sync.
- [ ] Phone database encrypted at rest (WASM SQLite with an app-level key, or native SQLCipher) and an app lock.
- [ ] **Pairing**: scan a QR code shown by the desktop app. It carries a device id and a key for end-to-end encrypted sync. Devices can be listed and revoked.
- [ ] End-to-end encryption of all sync messages. Transports and relays only ever see ciphertext.
- [ ] Transports, added one at a time:
  - [ ] (a) Direct LAN, when both devices are on the same network
  - [ ] (b) Tailscale or WireGuard address, for networks you do not own. Guest, hotel and campus Wi-Fi often isolate devices, so direct LAN alone is not enough.
  - [ ] (c) WebRTC peer to peer with QR/short-code signalling (needs both devices online at once)
  - [ ] (d) Encrypted change files in a folder synced by Syncthing, Nextcloud, iCloud or similar (no network setup, works any time)
  - [ ] (e) Optional self-hostable dumb relay that stores only encrypted blobs
- [ ] Phone UI: mobile-first layouts for every screen, bottom tab bar, quick add (floating button, amount first with numeric keypad, recent categories first), tap targets of at least 44 px, safe-area insets, `100dvh`, inputs at least 16 px
- [ ] Sync status in both apps: last sync time, pending changes, errors
- [ ] Tests: two devices edit offline and converge; replaying the same changes is idempotent; a revoked device is rejected; a corrupted or tampered message is rejected
- [ ] Test on real phones (small and large) and on a network that isolates clients

**Done when:** you can add an expense on the phone with no connection, later sync it to the desktop over at least one transport that works away from home, and both show the same budget numbers.

## Phase 9: Phone capture (S to M)

Adds transactions without opening the app, and feeds the same review queue as imports.

- [ ] iOS: a documented Shortcuts automation that fires on each Wallet/Apple Pay transaction and sends amount and merchant to the app
- [ ] Android: a notification listener (opt-in permission) that reads bank and wallet notifications and extracts amount and merchant
- [ ] Captured items land as PENDING in the review queue, never straight in the books
- [ ] Parsing is local; notification text is untrusted and never executed or logged
- [ ] Docs: setup guide per platform, and what is not captured (transfers, direct debits, anything not paid with the phone)
- [ ] Tests: sample notifications parse correctly; duplicates from the same payment are merged

**Done when:** paying with the phone creates a pending item you can accept with one tap.

## Phase 10: Bank connection with your own aggregator key (L)

Read-only account information through an Open Banking aggregator, with credentials the user owns. The app talks to the aggregator directly. There is no project-run backend and no shared API key in the source. The user logs in at the bank or in the aggregator's hosted page; the app never sees bank usernames, passwords, MFA codes or card numbers. It holds only a scoped, read-only, revocable token. This feature is optional and off by default.

- [ ] **Spike first** (S): choose which aggregators to support for your country and bank(s). Candidates: Enable Banking (free personal tier in the EU), SimpleFIN Bridge (US and Canada, small yearly fee), others. GoCardless may have stopped accepting new signups. Verify current terms, cost, bank coverage, consent length and what a personal, non-commercial project needs before building.
- [ ] Settings screen to paste aggregator credentials. They are stored encrypted in the OS keychain, never in the plain database, never logged, and not included in exports, CLI output or sync. They are included in backups only when the backup is encrypted.
- [ ] Requests are signed locally on the user's machine. Only the desktop app talks to the aggregator (not the phone), and synced data arrives on the phone through normal sync.
- [ ] Connect flow: the app opens the aggregator's hosted bank-login page in the system browser; the redirect returns to the app through a loopback address (`127.0.0.1`) or a custom URI scheme. A one-time `state` value is checked on return. Bank credentials are never accepted in the app's own forms.
- [ ] Least scope: read-only accounts and transactions, no payment initiation
- [ ] Consent: show status and expiry, remind before it runs out (EU consent lasts roughly 90 to 180 days), and make renewal one click
- [ ] Disconnect: revokes the token at the aggregator first, then deletes it locally. "Delete all my data" does the same.
- [ ] Import job: fetch, deduplicate by provider transaction id, put into the review queue from phase 6. Token expiry or aggregator errors mark the connection `NEEDS_REAUTH` and notify the user.
- [ ] Currency: accounts in a currency other than yours store the original currency and amount; no silent conversion
- [ ] Imported merchant names and descriptions are untrusted text: escape on render, neutralise in CSV export
- [ ] Tables for connections, accounts and imported transactions; additive columns on `transactions`
- [ ] CLI: commands for connections, the review queue (list, accept, ignore) and disconnect. Credentials are never readable through the CLI.
- [ ] Docs: a step-by-step "get your key" guide for each supported aggregator, and a clear note that the aggregator is the only external service involved
- [ ] Update the data inventory (aggregator as an optional external processor chosen by the user)
- [ ] Tests: no duplicates on re-import, disconnect revokes and deletes the token, expired consent stops imports, credentials never appear in logs, exports, CLI output or unencrypted backups

**Done when:** a connected account imports new transactions into the review queue, accepted ones show up in budgets, and disconnecting revokes the token at the aggregator and removes it from the machine.

## Phase 11: Packaging and release (M). Gate before sharing the app

- [ ] Signed installers for Linux (AppImage/deb), macOS (dmg, notarised) and Windows (msi/exe); the phone app is built and installable
- [ ] Auto-update with signed updates only; the update check is a visible setting
- [ ] Reproducible or at least verifiable builds in CI, checksums published, SBOM generated
- [ ] `SECURITY.md` with a private disclosure contact and supported versions
- [ ] `CONTRIBUTING.md` and a short architecture note
- [ ] Install, backup/restore and "move to a new PC" guides
- [ ] Fresh-machine acceptance test: install, first run, add data, back up, restore on a second machine, pair a phone, sync
- [ ] Review of the privacy-related claims in the README: no telemetry, no accounts, what leaves the device and when

**Done when:** a stranger can download the app, install it in a few minutes, and keep their data safe without reading the source.

## Phase 12: Later (optional)

- [ ] More aggregators, and more import formats
- [ ] Expense splitting. It is hard without a server because both sides need the same group data. Options: peer-to-peer groups over the sync transports, or leave it out.
- [ ] Home-screen widgets, shared budgets between two devices of the same household

---

## Risks to watch

| Risk | Mitigation |
|---|---|
| Laptop lost or stolen | Database encrypted at rest, app lock, key in the OS keychain, lost phones can be revoked from the desktop |
| Data lost with the machine | One-click encrypted backup, scheduled backups, restore test in phase 6 |
| Backup file leaks | Backups are encrypted with a user passphrase; unencrypted export is explicit and warned |
| Sync conflicts or lost edits | Append-only change log, merge rules written before code, convergence tests |
| Sync traffic exposed | End-to-end encryption; transports and relays see only ciphertext |
| Direct LAN sync blocked on other networks | Transport-independent design, Tailscale/WireGuard and folder-based transports |
| iOS evicts PWA storage | Request persistent storage, desktop copy stays authoritative, consider a native wrapper |
| Aggregator credentials stolen | OS keychain only, read-only scope, revoke on disconnect, never in logs, exports or sync |
| Bank consent expires unnoticed | Expiry shown, reminders, one-click renewal, `NEEDS_REAUTH` state |
| CLI token leaked or over-privileged | Read-only default scope, expiry, hashed storage, destructive actions require the app |
| Malware on the machine reads unlocked data | Lock timeout, key dropped from memory on lock, minimal permissions; this cannot be fully removed |
| Malicious imported text | Escape on render, neutralise in CSV, treat all import and notification text as untrusted |
| Tampered release or update | Signed installers and updates, checksums, SBOM |
| Money rounding bugs | Strings/decimals or integer minor units only, constraint tests, no float math |
| Scope creep | Finish and release phases 0 to 11 before starting phase 12 |
