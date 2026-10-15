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
4. **Backup and export before anything that spreads your data to other devices.**
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
- Request validation with Zod at every boundary (UI to core, CLI to core, sync).
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
- [x] Dev seed script with demo categories, budgets and transactions (`src-tauri/seeds/dev.sql`, loaded by `src/seed.rs`; run `cargo run --example seed -- <db-path> <64-hex-key>` from `src-tauri/`. Dates are relative to today. Later widened: sixteen months of history, awkward text for the exports, and `cargo run --example dev_vault -- <data-dir>`, which replaces the app's own data with it. The tests run the seed for ten different "today" dates, month ends and leap days included.)

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
- [x] Language and region: a language setting (separate from the number and date format and the timezone), chosen on the first-run screen together with format, timezone and currency, and changeable in Settings. All app text is translated (English, German, Spanish, French, Italian, Portuguese, Romanian) from one dictionary per language in `client/src/i18n/`; text the person types is never translated. Adding a language is one new file plus a line in `languages.ts`; tests check every language has every key and the same placeholders. The recovery-key text file is still English, and amounts keep two decimals for every currency.
- [x] Speed and keyboard: SQLite runs in WAL mode with a larger cache; the transaction list reads straight from date-and-id indexes (newest 50 of 50,000 rows in under a millisecond in a debug build, from about 10 ms; deep pages and month filters about 15 to 50 times faster); the history table comes from two queries instead of one per month. In the client every page loads on its own and starts loading on hover, number, date, currency and timezone formatters and option lists are built once, and search waits for a pause in typing. Keyboard shortcuts for navigation, adding, search, months and list rows, with hover hints and a `?` help dialog (`client/src/keys/`, table in `shortcuts.ts`).

**Done when:** you can record income and expenses, set monthly budgets and category allocations, change them, and old months keep their old limits, all inside the desktop app.

## Phase 4: Recurring items (M)

- [x] Recurring CRUD (income or expense, any category, interval unit + count, optional end date) (`src-tauri/src/recurring.rs`: create, update, delete, list. Exposed as the `*_recurring` commands and `client/src/api/recurring.ts`. Changing the schedule moves the next due date to the first new occurrence on or after the old one. Deleting an item keeps its paid transactions. Later: the amount is optional. A bill that changes every time (electricity, a phone bill) has no fixed amount (`amount_minor` is NULL, migration `0006_recurring_amount_optional.sql`); each payment then says what was paid, and nothing is stored as planned. Such a bill is listed but left out of the monthly commitments, and the dashboard shows it as "varies". In the form, leave the amount empty; in an edit, clearing it makes the item varying. The migration rebuilds the table, so the runner now turns foreign keys off while a migration runs and checks them before it commits (a test shows the payments stay linked, and the migration was run on a copy of real data first).)
- [x] Due-date function computed from `start_date` (not from the previous due date). **Unit-test month-end cases** (Jan 31, Feb 29, yearly on Feb 29). (`recurring::due_date`, `next_after`, `first_on_or_after`; tests cover Jan 31, leap February and yearly Feb 29.)
- [x] "Mark as paid" in one database transaction (schema Part 5-B), with optional actual amount and payment date (`recurring::pay`. The planned amount is kept on the transaction. Paying the last occurrence before the end date makes the item inactive.)
- [x] Skip, pause, end; overdue and upcoming lists (`skip`, `set_active`, `end`, `dues`. Resuming an item past its end date is refused. Upcoming defaults to the next 30 days, at most 366.)
- [x] Due dates and "overdue" are evaluated in the timezone setting (`pay`, `end` and `dues` take today's date as an argument; the commands in the next batch pass the date from `clock::today`.)
- [x] Frontend: recurring list with due/overdue state and a mark-as-paid action, built mobile-first (`client/src/pages/RecurringPage.tsx`, route `/app/recurring`, sidebar entry and `g r` shortcut. It reuses the transactions list layout, so it wraps on narrow screens. Status per row: overdue, due today, due on a date, paused or ended. Actions: mark as paid (amount and date prefilled), skip, pause or resume, edit, end, delete. All text is in the seven languages. Later restyled: a card for each item with the next date as an outlined badge (red when overdue, green when due today, dashed when paused or ended), "mark as paid", skip and pause right beside the date, and edit, end and delete underneath, quieter. A varying bill asks for the amount when it is marked as paid.)
- [x] Optional desktop notification for items due today (local only) (Once a day while the app is open and unlocked, one notification lists items due today or overdue; checked at start and every 30 minutes. A checkbox under Settings, notifications, turns it off; the choice is kept in the vault config file and is on by default. `client/src/useDueNotifications.ts`, `show_due_notification` in `commands.rs`. Nothing is shown while the app is closed or for items that fall due later the same day.)
- [x] Tests: paying twice is rejected, planned vs actual amount stored, paid transaction counts in its category's actual (`recurring.rs` tests.)

**Done when:** a monthly subscription shows up as due, one click turns it into a normal transaction, and the next due date is correct.

## Phase 5: Statistics and charts (M)

- [x] Command: budget vs actual for a month (categories + totals) (`budget_vs_actual` in `src-tauri/src/stats.rs`. Per category: budgeted (null without a budget), actual, remaining, percent used with one decimal; totals reuse the budget overview. A deleted category is listed only for months where it has transactions. Budget lookup is shared with the budget screens.)
- [x] Command: category trend over a date range (`category_trend`: one point per month, `budgeted` null where the category has no budget, at most 120 months.)
- [x] Dashboard (built mobile-first): budget vs actual per category (bars/progress), month totals for income and expense, savings rate, year trend (`client/src/pages/DashboardPage.tsx`: headline income, expense, savings and savings rate with change from last month; the year as income and expense lines with savings bars, hover to read a month, click to open it, year picked with ‹ year › and a current button; a spending donut; a bar per budget with its percent, over-budget in red; a recurring card. Plain CSS and SVG sized to their container. Chart titles link to transactions, budgets and recurring. Month steps with `[` `]` and `t`.)
- [x] Extra stats from the schema: top categories, committed recurring cost per month, planned vs paid for recurring items (Commands in `stats.rs`: `year_trend`, `top_categories`, `recurring_commitments`, `recurring_planned_vs_paid`. On the dashboard: top categories are the spending donut; the recurring card shows each item's monthly cost, what was paid this month and the difference from plan.)
- [x] Frontend formats money using the currency and locale setting (`formatMoney` and `formatPercent` in `money.ts`, used for every figure on the dashboard.)
- [x] Tests: numbers match hand-calculated examples, months with no budget return `budgeted: null` (Rust: `the_food_example_matches_hand_calculation`, `a_month_without_budgets_has_null_budgeted`. Client: `dashboardMath.test.ts` for percent used, axis scale and change from last month.)

**Done when:** the Food example works end to end: a budget of 400, transactions summing to 356.20, the chart shows both.

## Phase 6: Backup and export (M)

- [x] **Encrypted backup**: one click writes a single encrypted file (passphrase chosen by the user), plus restore with a version check. Optional scheduled backups to a folder the user picks.
  - [x] Backup file and restore with a version check (`src-tauri/src/backup.rs`, commands `create_backup`, `restore_backup`, `restore_backup_new_vault`. The file holds every row as JSON, sealed with ChaCha20-Poly1305 under an Argon2id key from the backup passphrase (8 characters or more, not the app's), so it does not depend on the vault's keys and opens on a clean install. The header carries the schema version of the app that wrote it: a newer one is refused, an older one is loaded into a scratch database of that version and migrated forward. Restoring checks every constraint in the scratch database first and then replaces all data in one transaction, so a damaged file changes nothing. Over an open vault it needs the word RESTORE typed. `vault.json` (lock settings, notification choice) is not part of a backup. A test fails when a migration adds a table that the backup does not list.)
  - [x] Backup and restore screens (Settings, backup block: a passphrase typed twice and "back up now" opens a save dialog; restoring asks for the backup passphrase and the word RESTORE, then opens a file dialog and reloads the app. The first-run screen has "restore from a backup" (`client/src/vault/RestoreFromBackup.tsx`), which sets an app passphrase for the new install and then shows its recovery key. Calls are `vault.createBackup`, `restoreBackup`, `restoreFromBackup`; new error code `invalid_backup` and new texts in all seven languages.)
  - [x] Scheduled backups to a folder the user picks (`src-tauri/src/schedule.rs`, migration `0005_backup_schedule.sql`, commands `get_backup_schedule`, `set_backup_schedule`, `clear_backup_schedule`. Settings, backup: a passphrase, daily, weekly or monthly, and a folder chosen in a dialog; the first backup is made at once so a folder that does not work is reported then. A timer checks every minute and backs up when due, only while the app is unlocked. A failed run is recorded, shown in Settings and tried again after an hour. Files are named `meowny-auto-<date>-<time>.meowbak`; the newest 10 are kept and only files with that exact name are deleted. The passphrase is stored in the encrypted database, in a table with no audit triggers; it is never sent to the UI and is left out of backups. Restoring does not change the schedule, and "delete all data" removes it but does not touch the backup files in the folder.)
- [x] **Export**: full JSON of all data, CSV of transactions. CSV safety: cells starting with `=`, `+`, `-` or `@` are prefixed with `'`. (`src-tauri/src/export.rs`, commands `export_json` and `export_csv`, client call in `client/src/api/dataExport.ts`; the screen is the export screen item below. Both take a selection: a date range for transactions and a set of categories. With categories chosen, the JSON leaves out other categories, global budgets and the change history, and a test checks that no other category name leaks. The full JSON has settings, categories, every budget row, recurring items, transactions and the change history; amounts and ids are strings. The CSV has `date,type,category,name,amount,currency,description`, amounts positive, a byte order mark for Excel and CRLF line ends. Cells starting with `=`, `+`, `-`, `@`, tab or carriage return get a `'` in front (`export::neutralise`, shared with the Excel export). The files are written readable by the owner only. The scheduled-backup passphrase is never exported.)
- [x] **Excel export (.xlsx)**: one workbook with sheets for transactions, categories, budgets, recurring items and a monthly summary. Dates are real dates, money cells are numbers with the currency format (converted from the stored strings only at write time). The same formula-cell safety as CSV applies to text cells. (`src-tauri/src/export/xlsx.rs`, command `export_xlsx`, written with the `rust_xlsxwriter` crate; the tests take the file apart with a small reader of their own (plus `flate2`, a dev dependency, to unpack it). Sheets: Transactions, Categories, Budgets, Recurring, Monthly summary (income, expense, savings and savings rate for every month from the first to the last, quiet months as zeros). No cell holds a formula. Cells follow the person's number and date format setting: `client/src/api/displayFormat.ts` reads the date order, the date separator and the symbol position from `Intl` and sends them as `DisplayFormat` (`src-tauri/src/display.rs`, shared with the PDF); the app turns them into Excel format codes with the separators and symbol as literals (`dd\.mm\.yyyy`, `#,##0.00" €"`). Digit grouping and the decimal mark are left to Excel, which takes them from the computer it runs on. The same selection as JSON and CSV limits every sheet. Dates before 1900 are written as text.)
- [x] **PDF reports**: a printable report for a chosen month or date range, with totals for income and expense, savings rate, budget vs actual per category, the charts from phase 5, and optionally the transaction list. Generated locally, with no network calls. Options: page size, include/hide descriptions (so a report can be shared without private details). (`src-tauri/src/report.rs`, `report/layout.rs`, `report/render.rs`, command `export_pdf`, written with the `krilla` crate, no network. The report covers whole months, from one month to another, optionally limited to some categories. Page 1: four tiles (income, expenses, savings, savings rate), income and expenses by month as bars, where the money went as a donut with a legend (top seven categories and the rest), then budget against actual per category as bars (over budget in red, categories without a budget marked), with the global budget on top. The transaction list is optional, repeats its headings on every page and shows descriptions only when asked. A4 or Letter, with page numbers. Numbers and dates follow the person's format (`DisplayFormat`, now also with decimal mark and group separator) and the fixed text comes from the app in the chosen language (`client/src/api/reportLabels.ts`, 8 new texts in seven languages). The charts are drawn directly in the page, not copied from the screen. For a range, budget against actual compares each category over the months in which it had a budget; for one month it is exactly what the budget screen shows. Text is set in DejaVu Sans and Bold, embedded as subsets (`src-tauri/fonts/`, with its license): Latin, Greek and Cyrillic are drawn, a character outside them, such as Chinese, shows as `?`. The file has a title and no author or creation date. Limits: 120 months and 20,000 listed transactions.)
- [x] Export screen lets you pick the format (JSON, CSV, Excel, PDF), the date range and the categories, and shows what the file will contain before saving. A file with sensitive data is not encrypted unless you choose the encrypted backup, and the screen says so. (`client/src/pages/ExportPage.tsx`, route `/app/export`, sidebar entry and `g e`; the choices are worked out in `exportPlan.ts` with tests. One format picker with a line about each; all dates or a from and to; all categories or a ticked list. For a PDF the dates widen to whole months and the screen says which, with a page size, a switch for the transaction list and a switch for descriptions (off by default). "What the file will contain" is a receipt on the right (it stays in view on a wide screen and comes last on a phone), filled from the new `export_preview` command: the number of transactions, the first and last date, the income and expense totals, and for JSON and Excel the categories, budget entries and recurring items, and for a full JSON the history. The format is four tiles, the categories are toggle chips grouped by expenses and income, and the export button shows the file extension. It follows the choice a moment after it changes and shows the same counts as the file; a choice that cannot be exported (end before start, no category ticked, a report over ten years) says why and disables the button. Dates, money and the language of the PDF come from the person's settings. The page ends with the plain warning that the file is not encrypted, with a link to the backup in Settings.)
- [x] Restore test: back up, wipe, restore on a clean install, compare all rows (`vault::tests::back_up_wipe_and_restore_on_a_clean_install_brings_back_every_row` compares every row of every table, including the change history, and checks the restored install has keys of its own; `backup::tests` cover wrong passphrase, tampered and truncated files, newer and older versions, and hostile content.)
- [x] Export tests: the Excel totals and the PDF totals match the dashboard for the same month; an Excel file with a formula-looking description is neutralised on export; the PDF with descriptions hidden contains none
  - [x] The Excel totals match the dashboard (`export::xlsx::tests::the_monthly_summary_matches_the_dashboard`)
  - [x] The PDF totals match the dashboard (`report::tests::the_pdf_totals_are_the_dashboard_totals` reads the text back out of the PDF file and compares it with `stats`)
  - [x] The PDF with descriptions hidden contains none (`report::tests::a_pdf_made_without_descriptions_contains_none`, on the text of the file and on its raw bytes; a PDF limited to some categories contains nothing about the others)
  - [x] A formula-looking description is neutralised in the Excel file (`text_that_looks_like_a_formula_is_written_as_text_and_comes_back_as_text`, which also checks that the file has no formula cells)
- [x] Review the data inventory against what the system really stores (`assets/data-inventory.md`, checked against the code and the files the app leaves on disk, and by searching the whole data folder for demo-only text, which turned up in no plain place. Corrections: the lock settings and the notification choice live in `vault.json`, not in the database. Added what was missing: the webview's stored preferences (language, sidebar state, date of the last notification), desktop notifications and the clipboard as places data goes, that the change history keeps what deleted rows said, what a backup and an export hold, and what "delete all my data" removes and leaves behind.)

**Done when:** you can back up, wipe, restore, and export to JSON, CSV, Excel and PDF from one screen, choosing the period and the categories.

## Phase 7: CLI for AI agents (M)

The whole app is usable from the terminal, so scripts and AI agents can drive it without the UI. The CLI is a client of the running desktop app. No server is introduced. The CLI and the command API belong to the desktop app only: the phone app (phases 8 and 9) has neither.

Decisions made at the start of the phase (the open "CLI transport and token mechanism" row above):

- **Transport:** HTTP/1.1 on `127.0.0.1` only, on a random port that the app writes to `api.json` in its data folder. Plain `std::net`, no new crates. A plain `curl` works, which helps agents and the docs. Off by default; the switch lives in `vault.json` so the app can answer "locked" while the database is closed.
- **Browser safety:** every request carries `Authorization: Bearer <token>`, the `Host` header must be a loopback name, and a request with an `Origin` header is refused, so a web page cannot talk to the API.
- **Tokens:** `mny_` and 64 hex characters, kept as a SHA-256 in the encrypted database. A locked app cannot check any token, so it answers "locked" (exit code 3) to everyone. API calls do not count as activity: the idle lock still fires, and a script cannot keep the app open.
- **Routes:** one table in Rust lists every route with its scope (read, write, or needs the user at the app), arguments and summary. It is the single source of `meowny schema`, of the docs and of the test that fails when a route has no CLI command. The core is Rust, so serde validates the input; the schema comes from this table and not from Zod.

- [ ] Design rule: the CLI is a thin client over the app's local command API, described by a schema generated from the Zod types. It never opens the database file. Validation, scoping and encryption stay in one place. Every feature through phase 6 gets a command; if the API lacks something, fix the API, not the CLI.
- [ ] Local command API: listens only on a local socket or loopback, off by default, enabled in settings. Requests must carry a token.
  - [x] The server (`src-tauri/src/api.rs`, `api/http.rs`, `api/routes.rs`; `std` only, no new crates). `127.0.0.1` on a port the system picks, written to `api.json` in the data folder (owner-only, removed on exit and by "delete all my data"; a crash's leftover is removed at the next start). One request per connection: `POST /v1/<route>` with a JSON object body and `Authorization: Bearer`; answers are `{"ok": true, "data": ...}` or `{"ok": false, "error": {"code", "message"}}` with a matching status (400 `invalid_input`, 401 `unauthorized`, 403 `forbidden`, 404 `not_found`, 405, 409 `name_exists` / `not_set_up`, 413 `too_large`, 423 `locked`, 500 `internal`). Checked in this order: `Host` must be `127.0.0.1:<port>` or `localhost:<port>`, any `Origin` is refused, POST only, a bearer token, then the route, the token's scope and the arguments against the route's declared parameters (unknown, missing, mistyped or not-allowed values are a 400 before the route runs). The HTTP reader is strict (no chunked bodies, no repeated `Host` / `Content-Length` / `Authorization`, no folded headers, head at most 16 KiB, body at most 1 MiB, the whole request within 10 seconds, at most 16 connections). A panic in a route is a 500, and error messages never hold database text or the token. Calls do not count as activity, so the idle lock still fires. Routes so far: `status` and `schema`. `cargo run --example api_demo` serves it on demo data for trying it with curl. `npm run check:outbound` now also lists listeners and `TcpStream` users; the API's are in the allowlist with reasons.
  - [x] On/off switch and start-up (`api::Host`: `set_enabled` saves the choice in `vault.json`, so it is readable while locked, and starts or stops the listener; it starts at launch if left on and stops on exit and on "delete all my data". A locked app that is left on answers `locked` to every token. Turning it on or off needs the unlocked app.)
  - [x] Settings screen for the switch (Settings, command line access; see the token screen below)
- [ ] Routes for every feature through phase 6, so the CLI has something to call
  - [x] Read routes: `today`, `settings.get`, `categories.list`, `transactions.list` (from, to, categoryId, q, limit, offset), `budgets.overview`, `budgets.history`, `recurring.list`, `recurring.dues`, `stats.budget`, `stats.trend`, `stats.year`, `stats.top`, `stats.commitments`, `stats.planned` (`src-tauri/src/api/read.rs`; each is a thin wrapper over the function the desktop screen calls, and a test compares every route with that function on the demo data. Money stays a string and ids stay strings; a test walks every answer and fails on any new JSON number that is not a count or a percentage. The lock settings, the backup passphrase and the token list are not exposed. New argument kinds in the schema, `date`, `month`, `year` and `id`, whose shape the server checks before the route runs (an odd value is a 400 that names the argument); whether the date or month really exists is still the route's call. A route's arguments are declared with `Param::new(...).required().one_of(...)`. Tests: `api/read_tests.rs` (wiring, filters, time zone and month end, bad values, nothing written, locked app) and, over real sockets, `api/tests.rs`.)
  - [x] Write routes: `settings.set` (number format, time zone, language; not the currency), `categories.create` / `rename` / `delete`, `transactions.create` / `update` / `delete`, `budgets.set` / `remove`, `recurring.create` / `update` / `delete` / `pay` / `skip` / `pause` / `resume` / `end` (`src-tauri/src/api/write.rs`. They need a write token; the scope is checked before the body is read, so a read token gets a 403 whatever it sends. Each calls the function the desktop screen calls, so every rule of the screens holds, and each change is in the change history like one made in the window. `transactions.create` with a `clientUuid` is safe to repeat: the same one twice returns the first transaction. Removing a budget is its own route instead of a null amount, and a budget is a category budget when `categoryId` is given and the global one of that side otherwise. As the app does, a month that has already started changes only that month. New argument kinds `money` ("12.50", two decimals, never a JSON number) and `uuid`, and `nullable` arguments: `null` clears a transaction's description, a recurring item's end date or description, and makes its amount "none"; leaving a field out leaves it alone. A test lists the write routes exactly, so a new one has to be added on purpose. Not reachable with any token: changing the currency, the lock settings, tokens, backups and restores, exports, the recovery key and "delete all my data". Tests: `api/write_tests.rs` (each route and its refusals, a refused write leaves the whole database as it was, every write is refused while locked and changes nothing, history rows), and over sockets in `api/tests.rs`: a read token is refused every write route, and the plan's "done when" with only a token (a category, a budget of 400, three payments of 120.50, 99.70 and 136.00, each sent twice, then 356.20 spent and 43.80 left).)
  - [ ] `export` and `backup` routes, which need a confirmation in the app
- [ ] Package in `cli/` (one binary, `meowny`), installed by the desktop installer and available standalone.
  - [x] The package (`cli/`, **Rust**, chosen with the maintainer on 2026-10-15 instead of the TypeScript the plan first said: no Node on the user's machine, one real standalone binary, and the tests can run it against the app's own server code. It is its own cargo package with one dependency, `serde_json` (the app already uses it; `meowny` links only libc, about 850 KB in release), and the app as a dev-dependency for the tests. Build it with `cargo install --path cli`. The desktop app's program is also called `meowny`, so the CLI is built on its own and not as part of the app. The commands are not written by hand: `cli/schema.json` is the app's own description of its routes (`cargo run --example dump_schema > schema.json`) and a test fails when it differs from the route table, so a route has a command by construction. Modules: `catalog.rs` (routes to commands and flags), `args.rs` (the command line to a JSON body), `client.rs` (find the app, the token, one HTTP request), `output.rs` (tables), `help.rs`, `local.rs` (`login`, `logout`, `schema`), `run.rs`. The argument checks of the plan's Zod item are the app's: the CLI only builds the JSON (numbers, the allowed words matched without regard to case) and the app says what is wrong with a value.)
  - [ ] Installed by the desktop installer (phase 11 decides how both programs get on the path)
- [ ] Tokens: created in the app, revocable, stored hashed, with scopes (read-only by default, write opt-in), an expiry and a last-used time. Passed by `MEOWNY_TOKEN` or stored in a `0600` file. Never printed or logged. `export`, `delete-all` and backup restore are not allowed with a token and need the user at the app.
  - [x] Token store: create, list, revoke, verify (`src-tauri/src/tokens.rs`, migration `0007_api_tokens.sql`. A token is `mny_` and 64 hex characters, shown once; the table keeps only its SHA-256. Scope is `read` unless `write` is picked (write also reads); expiry is 1 to 365 days, 30 by default; revoking deletes the row; use is stamped at most once a minute. The table is in the encrypted database with no audit triggers, is left out of backups and exports, and a restore does not touch it. `verify` gives the same answer for unknown, revoked, expired and malformed tokens. Nothing calls it yet. Tests: `tokens.rs`, constraints in `schema_tests.rs`, backup and export tests.)
  - [x] Token screen in Settings: create (name, scope, expiry), the secret shown once, list with last used, revoke (Settings has a "command line access" block, `client/src/components/CommandApiBlock.tsx`, with the on/off switch, a status line (off, listening on port N, or on but could not start), the token list and the make-token form. Commands: `get_api_status`, `set_api_enabled`, `list_api_tokens`, `create_api_token`, `revoke_api_token`, over `client/src/api/commandApi.ts` (answers checked with Zod; no new error codes). A name is trimmed and 1 to 50 characters; access is read only or read and write; lifetimes offered are 7, 30, 90 and 365 days (the app accepts 1 to 365). The new token's secret is shown once with copy and done; it is not kept in the page after done. Each row shows the access, the expiry and the last use in the person's format and time zone (`whenText`, moved to `api/locales.ts`); an expired token stays listed, muted, until it is revoked. 25 new texts, `cli.*`, in the seven languages, and a test that `MEOWNY_TOKEN` stays untranslated. Looked at in a browser against a mocked backend: switch on and off, the three status lines, make, the one-time secret, revoke, and a narrow window.)
  - [x] The API refuses a token without the scope a route needs (403 `forbidden`; a write token also reads)
  - [ ] `export`, `delete-all` and restore need the user at the app
  - [x] The CLI takes the token from `--token-file`, then `MEOWNY_TOKEN`, then `MEOWNY_TOKEN_FILE`, then the file `meowny login` saved (`~/.config/meowny/token`, or the platform's equivalent), and never prints or logs it (there is no `--token` flag on purpose, since a command line can be read by other users. A token file that other users can read is refused with `token_file_unsafe`. `login` reads one line from standard input and writes the file `0600` in a `0700` folder; `logout` removes it. A test runs every kind of success and failure with real tokens and checks that neither the token nor its hash appears in anything printed.)
- [ ] If the app is locked, the CLI gets a clear exit code and no data.
  - [x] The API answers 423 `locked` and no data (tested for a valid token, a wrong one and an unknown route; no token at all stays a 401)
  - [x] The CLI turns it into exit code 3, prints nothing on standard output, and says to unlock the app
- [ ] Command groups: `settings`, `categories`, `transactions`, `budgets`, `recurring`, `stats`, `export` (json, csv, xlsx, pdf), `backup`. Pattern `meowny <noun> <verb>`, with `--help` at every level.
  - [x] `status`, `today`, `settings` (get, set), `categories`, `transactions`, `budgets`, `recurring`, `stats`: one command per route, so 33 commands, plus `login`, `logout` and `schema`. A flag per argument (`paymentDate` is `--payment-date`), `--clear-<flag>` for what can be cleared, and the `id` as a plain word (`meowny transactions delete 42 --yes`). `--help`, `meowny help <command>` and `meowny <noun> --help` work at every level and without the app.
  - [ ] `export` and `backup` (with the routes, which need a confirmation in the app)
- [x] Agent-friendly output: `--json` on every command (stable shape, additive fields only), human table otherwise. Data on stdout, messages and errors on stderr. (`--json` is the app's JSON on one line; an error is `{code, message}` on standard error. The plain form is a table with the id and name first and the money columns in reading order, numbers right-aligned, text cut at 60 characters, a column of lists left out, and control characters in text turned into spaces. The app already refuses control characters in names and notes, so this is a second wall. A test lists the keys and types of every answer and fails if one is lost or changes type.)
- [x] Non-interactive by default: no prompts; every input is a flag or stdin. Destructive commands need `--yes`. Transaction create sends a `client_uuid`, so retries are safe. (A text flag given `-` reads its value from standard input. `delete`, `remove` and `end` commands fail with exit 2 and change nothing without `--yes`. `transactions create` makes a fresh version 4 UUID if `--client-uuid` is not given, from the operating system's random hash keys and the clock with no crate, and the answer carries it, so a script that never heard back runs the same command with that `--client-uuid` and gets the first transaction. Nothing reads standard input unless asked: a test leaves it open and checks the command still ends.)
- [x] Exit codes: 0 ok, 1 error, 2 bad usage, 3 auth failed or app locked, 4 not found. In `--json` mode errors are `{code, message}`. (`invalid_input` from the app and every usage mistake are 2; `no_token`, `bad_token`, `token_file_unsafe`, `unauthorized`, `forbidden` and `locked` are 3; `not_found` is 4; the rest, including `app_unavailable` and `timeout`, are 1. The table is in `meowny schema` and the help, and a test pins each code.)
- [x] Money stays strings end to end; dates are ISO 8601. (The CLI never turns an amount into a number: the only numbers it makes are for integer flags such as `--limit`.)
- [x] `meowny schema` prints all commands and arguments as JSON, so an agent can discover them. (Offline, from what the program was built with: every command with its route, scope, flags (type, required, nullable, allowed values), whether it needs `--yes`, plus the global flags, the environment variables, the exit codes and the error codes. `meowny schema --app` asks the running app for its own list of routes.)
- [ ] Docs: one `CLI.md` with an example per command, plus a short agent note (env var, `--json`, exit codes).
- [x] Tests: each command against a real app instance (happy path, validation error, bad token, locked app, `--json` shape snapshot), plus one test that fails if a command-API route is missing from the schema or has no CLI command. (79 unit tests and 24 in `cli/tests/cli.rs`, which run the real binary against the app's own server code with a real encrypted vault in a temporary folder: every read command on the demo data; the plan's "done when" with only a token (a budget of 400, three payments sent twice each, 356.20 spent and 43.80 left); every write command refused to a read token with the data untouched; bad, revoked and expired tokens; a locked app; the app not running or on another API version; usage errors that never reach the app; `--yes`; not found; the token files; `login` and `logout`; eight scripts at once. The route coverage test checks that the embedded schema equals the route table, that every route has a command with help naming all its flags, and that no command points at a missing route. CI runs them next to the app's tests.)
- [ ] From here on, every new desktop feature ships its CLI commands in the same phase. The phone app gets none.

**Done when:** a script or agent holding only a token can record an expense, set a budget, and read budget vs actual, all with `--json` and no prompts.

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

Adds transactions without opening the app.

- [ ] iOS: a documented Shortcuts automation that fires on each Wallet/Apple Pay transaction and sends amount and merchant to the app
- [ ] Android: a notification listener (opt-in permission) that reads bank and wallet notifications and extracts amount and merchant
- [ ] Captured items land as PENDING in a pending list, never straight in the books. (Phase 6 no longer builds a review queue, so this phase builds the small pending list: accept, edit, ignore, set a category.)
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
- [ ] Import job: fetch, deduplicate by provider transaction id, put into the pending list from phase 9. Token expiry or aggregator errors mark the connection `NEEDS_REAUTH` and notify the user.
- [ ] Currency: accounts in a currency other than yours store the original currency and amount; no silent conversion
- [ ] Imported merchant names and descriptions are untrusted text: escape on render, neutralise in CSV export
- [ ] Tables for connections, accounts and imported transactions; additive columns on `transactions`
- [ ] CLI: commands for connections, the pending list (list, accept, ignore) and disconnect. Credentials are never readable through the CLI.
- [ ] Docs: a step-by-step "get your key" guide for each supported aggregator, and a clear note that the aggregator is the only external service involved
- [ ] Update the data inventory (aggregator as an optional external processor chosen by the user)
- [ ] Tests: no duplicates on re-import, disconnect revokes and deletes the token, expired consent stops imports, credentials never appear in logs, exports, CLI output or unencrypted backups

**Done when:** a connected account brings new transactions into the pending list, accepted ones show up in budgets, and disconnecting revokes the token at the aggregator and removes it from the machine.

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

- [ ] More aggregators
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
