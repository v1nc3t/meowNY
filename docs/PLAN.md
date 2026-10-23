# meowNY: Build Plan (local-first)

Work top to bottom. Each phase ends with a "Done when" check. Details of finished work live in the code, the tests and the design notes (`docs/sync-design.md`, `docs/documents-design.md`, `docs/debts-design.md`).

**Size tags:** S = a few hours, M = a few days, L = a week or more (one person, part-time).

## Product goals

- **Local-first.** Standalone desktop app. No server, no accounts, no internet needed. Data in an encrypted SQLite file on your machine.
- **Easy to install.** Download, run. No Docker, no database to host.
- **Secure by design.** No network attack surface. Encrypted at rest, locked app, signed releases.
- **Phone copy that syncs.** Android keeps a full offline copy and syncs with the desktop on the home network, started by a QR code. No server of any kind.
- **CLI for AI agents.** Ships with the desktop app, client side only.
- **Documents with the numbers.** Receipts and invoices stored encrypted, linked to transactions; the phone gets the ones you choose.
- **Who owes whom.** Debts and shared bills in your own ledger. Friends do not need the app.

## Order and why

1. Foundations and security first: encrypted database, key handling and data model are painful to retrofit.
2. Core budgeting loop: categories, transactions, budgets.
3. Recurring, then charts.
4. Backup and export before anything that spreads data to other devices.
5. CLI, then phone sync, then documents, then debts. Packaging is a gate before sharing the app.

## What carries over from the server-based design

- Kept: React UI, Zod schemas, money and date logic, schema design (effective-dated budgets, `client_uuid`, history triggers, constraint tests).
- Replaced: Express, PostgreSQL, Better Auth, `server/`. Accounts, GDPR service, admin board and deployment phases are dropped.
- What remains of them: export your data, delete all your data, encrypted backup.

## Decisions already made

- App shell: Tauri (Rust, native OS keychain). Storage: SQLite with SQLCipher.
- One local profile; no user ids in the schema.
- Budgets are effective-dated; history by database triggers.
- Money is integer minor units or strings, never floats.
- Zod validation at every boundary (UI to core, sync).
- Every new screen from phase 4 on is mobile-first.
- Months, due dates and "this month" use the user's timezone setting.
- The CLI never opens the database. It talks to the running app through a local command API (`meowny schema` prints it).
- Every new desktop feature ships its CLI commands in the same change. The phone gets none.
- Program names: `meowny-desktop` (`src-tauri/`), `meowny` (`cli/`).
- Phone app: Tauri mobile (same `ui/` and Rust core), Android first, iOS later (phase 12). Sync is a direct home-network connection started by a QR code. No server, relay or remote reach.
- No telemetry. The only optional network traffic is update checks.
- 2026-10-16: bank connection through an aggregator and phone capture from notifications dropped. Nothing talks to a bank. Phases 9 and 10 are documents, and debts and splitting.

## Open decisions (decide when you reach the phase)

| Decision | Needed by | Notes |
|---|---|---|
| License | Phase 0 | Decided: AGPL-3.0-only |
| Query layer (Kysely or Drizzle) | Phase 0 | Moot: the core is Rust |
| Key handling: keychain only, or passphrase too | Phase 2 | Decided: keychain, optional passphrase |
| Chart library | Phase 5 | Decided: plain CSS and SVG |
| CLI transport and tokens | Phase 7 | Decided: loopback HTTP, hashed, scoped, expiring tokens |
| Phone app shape | Phase 8 | Decided 2026-10-15: Tauri mobile, Android first (a PWA cannot reach plain-http LAN, needs HTTPS for camera, storage can be evicted) |
| Sync transports | Phase 8 | Decided 2026-10-15: one only, direct home-network connection by QR code. No Tailscale, WebRTC, shared folder or relay |

---

## Phase 0: Setup and decisions (S)

- [x] Repo layout and TypeScript setup, oxlint, no separate formatter
- [x] Request validation with Zod
- [x] Logging that never prints personal data
- [x] Test setup: unit tests plus integration tests against a real database
- [x] CI that runs lint, tests and migrations from scratch
- [x] Tauri app shell that opens the React client in a window
- [x] SQLCipher database opens from Rust and from tests; query layer decided
- [x] Choose a license and add `LICENSE`
- [x] CI also runs a dependency audit and a secret scan
- [x] CI builds the app for Linux, macOS and Windows (unsigned)
- [x] Start a one-page data inventory (`docs/data-inventory.md`, rewritten for local-first)

**Done when:** `npm run tauri dev` opens a window, it reads a row from an encrypted SQLite file, and CI is green on all three platforms.

## Phase 1: Database foundation (M)

- [x] Port tables, indexes and foreign keys to SQLite migrations; drop user ids and auth tables (`src-tauri/migrations/0001_tables.sql`, `STRICT`)
- [x] Port CHECK constraints, partial unique indexes and triggers (`0001_tables.sql`, `0002_triggers.sql`; statistics rebuilt in phases 5 and 6)
- [x] Money as integer minor units in `*_minor` columns, bounded 0..999,999,999,999
- [x] Migration runner: applies on startup, refuses a newer database (`src-tauri/src/migrate.rs`, `user_version`)
- [x] Constraint tests in `src-tauri/src/schema_tests.rs`:
  - [x] a transaction cannot use a missing category
  - [x] a budget's type must match its category's type
  - [x] only one GLOBAL budget per type and month
  - [x] a category type cannot change after creation
  - [x] the same recurring occurrence cannot be paid twice
  - [x] the same `client_uuid` cannot create two transactions
  - [x] audit rows appear for insert, update and delete, and are skipped when only `updated_at` changed
- [x] Dev seed (`src-tauri/seeds/dev.sql`; `cargo run --example seed -- <db-path> <64-hex-key>`, `dev_vault` for the app's own data). Sixteen months, dates relative to today

**Done when:** a fresh database builds from migrations only and all constraint tests pass on SQLite.

## Phase 2: App security (M)

- [x] Database encrypted at rest; key from the OS keychain, optionally with a passphrase (`keys.rs`, `vault.rs`; HKDF of secret + Argon2id)
- [x] Recovery key shown once, replaceable in Settings; opens a separate wrapped copy of the database key
- [x] App lock on idle, minimise and close; key dropped on lock. Biometrics not done (needs a plugin per OS)
- [x] Tauri hardening: strict CSP, no remote content, one permission per command, no shell, external links in the system browser
- [x] No network unless a user-enabled feature needs it; `npm run check:outbound` with `scripts/outbound-allowlist.txt`
- [x] Safe logging: fixed error codes only, panic hook prints the location only
- [x] "Delete all my data": database, keys, backups; type DELETE
- [x] Settings: currency, locale, timezone (`ui/src/pages/SettingsPage.tsx`)
- [x] First-run screen: currency, lock method, data file location (`ui/src/vault/`)
- [x] Tests: wrong key cannot open the file, locked state holds no key, file has no plaintext

**Done when:** the data file is unreadable without the key, the app locks itself, and you can wipe everything.

## Phase 3: Core budgeting: categories, transactions, budgets (L)

- [x] React screens for categories, transactions and budgets
- [x] Typed command layer over Tauri `invoke` with Zod-checked answers (`ui/src/api/`); old HTTP client and auth pages removed
- [x] Categories CRUD; soft delete writes the NULL budget row (`categories.rs`)
- [x] Transactions CRUD with filters, pagination, `client_uuid` idempotency; amounts are strings (`transactions.rs`, `money.rs`)
- [x] Budgets: set or remove a GLOBAL or CATEGORY budget from a month, expense and income (`budgets.rs`); editing a month changes only that month; history table on the page
- [x] Budget overview: allocated vs unallocated, warning when over-allocated
- [x] Timezone-aware months and "today" (`clock.rs`), tested at month and year end
- [x] Tests: carry-over, removal via NULL, income budgets, idempotent create
- [x] Currency: pick at first run; change in Settings with a typed rate that converts every amount, or relabel only (`currency.rs`). Two decimals assumed for every currency
- [x] Language: seven languages from one dictionary each in `ui/src/i18n/`; tests check every key and placeholder
- [x] Speed and keyboard: WAL mode, date-and-id indexes, lazy pages, shortcuts and `?` help (`ui/src/keys/shortcuts.ts`)

**Done when:** you can record income and expenses, set monthly budgets and allocations, change them, and old months keep their old limits.

## Phase 4: Recurring items (M)

- [x] Recurring CRUD (`recurring.rs`); amount optional for varying bills (migration `0006`, shown as "varies")
- [x] Due date computed from `start_date`; month-end and leap-year tests
- [x] "Mark as paid" in one database transaction, with actual amount and date
- [x] Skip, pause, end; overdue and upcoming lists
- [x] Due dates and "overdue" use the timezone setting
- [x] Recurring page, mobile-first (`RecurringPage.tsx`, `/app/recurring`, `g r`)
- [x] Optional desktop notification for items due today (`useDueNotifications.ts`, on by default)
- [x] Tests: paying twice is rejected, planned vs actual stored, payment counts in its category

**Done when:** a monthly subscription shows up as due, one click turns it into a normal transaction, and the next due date is correct.

## Phase 5: Statistics and charts (M)

- [x] Budget vs actual for a month (`stats::budget_vs_actual`)
- [x] Category trend over a date range (`category_trend`, at most 120 months)
- [x] Dashboard, mobile-first (`DashboardPage.tsx`): totals, savings rate, year trend, spending donut, budget bars, recurring cards (overdue, due in 7 days, split with friends). Plain CSS and SVG
- [x] Extra stats: top categories, recurring commitments, planned vs paid
- [x] Money formatting by currency and locale (`money.ts`)
- [x] Tests: hand-calculated numbers, months with no budget return `budgeted: null`

**Done when:** the Food example works end to end: a budget of 400, transactions summing to 356.20, the chart shows both.

## Phase 6: Backup and export (M)

- [x] Encrypted backup: one file, own passphrase, restore with version check, optional scheduled backups
  - [x] Backup file and restore (`backup.rs`; ChaCha20-Poly1305 under Argon2id; older files migrated forward; restore needs the word RESTORE)
  - [x] Backup and restore screens in Settings and on the first-run screen
  - [x] Scheduled backups to a folder (`schedule.rs`, migration `0005`; newest 10 kept)
- [x] Export JSON and CSV with a selection (`export.rs`); CSV cells starting with `=`, `+`, `-`, `@` are neutralised
- [x] Excel export (`export/xlsx.rs`): real dates and numbers, no formulas, same safety as CSV
- [x] PDF reports (`report.rs`, `krilla`): totals, charts, budget vs actual, optional transaction list; local, no network
- [x] Export screen (`ExportPage.tsx`, `/app/export`, `g e`): format, period, categories, filters, parts, and a live preview for JSON, CSV, Excel and PDF
- [x] Restore test: back up, wipe, restore on a clean install, compare all rows
- [x] Export tests: Excel and PDF totals match the dashboard; formula-looking text neutralised; PDF without descriptions has none
  - [x] Excel totals match the dashboard
  - [x] PDF totals match the dashboard
  - [x] PDF without descriptions contains none
  - [x] Formula-looking description neutralised in Excel
- [x] Data inventory reviewed against the code and the files on disk

Later work on the export screen (2026-10-17 and 18): filters (type, amount, text, shared bills), parts, debts, due payments and history of debts in JSON, Excel and PDF, and the same choices in the CLI and API.

**Done when:** you can back up, wipe, restore, and export to JSON, CSV, Excel and PDF from one screen, choosing the period and the categories.

## Phase 7: CLI for AI agents (M)

The CLI is a client of the running desktop app. Desktop only. Decisions: loopback HTTP on `127.0.0.1` (random port in `api.json`), bearer tokens (`mny_` + 64 hex, stored as SHA-256), `Origin` refused, a locked app answers `locked`, one Rust route table drives schema, docs and CLI.

- [x] Design rule: thin client over the local command API; never opens the database. Schema comes from the Rust route table, not Zod. Restore, currency, lock settings, tokens, scheduled backup setup and delete-all stay app-only
- [x] Local command API: loopback only, off by default, token required
  - [x] The server (`api.rs`, `api/http.rs`, `api/routes.rs`); strict HTTP reader, fixed error codes
  - [x] On/off switch kept in `vault.json`, readable while locked
  - [x] Settings screen for the switch
- [x] Routes for every feature through phase 6
  - [x] Read routes (`api/read.rs`)
  - [x] Write routes (`api/write.rs`); write token needed; `clientUuid` makes create safe to repeat
  - [x] `export` and `backup` routes need a yes in a native dialog every time (`api/confirmed.rs`, `files.rs`, `prompt.rs`); files never overwritten
- [x] Package in `cli/` (one binary `meowny`, one dependency `serde_json`). Install with `cargo install --path cli`; on the PATH with the app is phase 11
  - [x] The package: commands come from `cli/schema.json` (`cargo run --example dump_schema > schema.json`); the desktop program was renamed `meowny-desktop`
- [x] Tokens: created in the app, revocable, hashed, scoped, expiring
  - [x] Token store (`tokens.rs`, migration `0007`); `verify` answers the same for unknown, revoked and expired
  - [x] Token screen in Settings (`CommandApiBlock.tsx`)
  - [x] A token without the scope gets 403 `forbidden`
  - [x] Export, delete-all and restore need the user at the app
  - [x] CLI token order: `--token-file`, `MEOWNY_TOKEN`, `MEOWNY_TOKEN_FILE`, saved file (`meowny login`); no `--token` flag
- [x] A locked app gives exit code 3 and no data
  - [x] API answers 423 `locked`
  - [x] CLI turns it into exit 3 with a message
- [x] Command groups `meowny <noun> <verb>` with `--help` at every level
  - [x] One command per read and write route, plus `login` and `logout`
  - [x] `export` and `backup` commands (`--out`, `--passphrase -`); no restore command
- [x] `--json` on every command; data on stdout, errors on stderr
- [x] Non-interactive: flags or stdin only; destructive commands need `--yes`; create sends a `client_uuid`
- [x] Exit codes: 0 ok, 1 error, 2 bad usage, 3 auth or locked, 4 not found
- [x] Money stays strings; dates are ISO 8601
- [x] `meowny schema` prints every command, flag and exit code as JSON
- [x] Docs: generated CLI reference site and `docs/CLI.md` (`docs/site/`, 70 commands, 68 routes, recorded from a real app by `cli/tests/docs.rs`; published by `pages.yml`)
- [x] Tests: each command against a real app; a test fails if a route has no command
- [x] Final audit of the phase: prompt lock fixed, direction marks refused in token names, panic counter, robustness tests, CI audit for `cli/`, Windows and macOS test fixes. Native Allow/Deny dialog still to try by hand once

**Done when:** a script or agent holding only a token can record an expense, set a budget, and read budget vs actual, all with `--json` and no prompts.

## Phase 8: Phone app (Android) and sync (L)

The phone keeps a full copy, works offline, and can change everything the desktop can. Both devices must be on the same network; every sync starts with a QR code the desktop shows. Android only (iOS is phase 12). Design: `docs/sync-design.md`.

Decided 2026-10-15:

- No server of any kind; away from home the phone works alone.
- Secure first, few dependencies second: existing crates plus `std` networking, two small new crates (QR encoder, barcode scanner plugin).
- Tauri mobile, not a PWA. CLI and command API stay desktop only.
- Phone vault: passphrase (Argon2), auto-lock, erase after 5 wrong passphrases; recover by pairing again.

- [x] Design doc first (`docs/sync-design.md`): QR layouts, handshake, key schedule, messages, stamps, merge rules, currency rule, vault, threats, tests
- [x] Spike: Android build on a real phone (Redmi Note 11, Android 13): encrypted database, screens, TCP to the desktop; camera scan not checked by me
- [x] Phone vault: passphrase, wrong-try counter kept outside the database, erase on the 5th (`vault.rs`, `keys.rs`)
  - [x] The vault side: `Policy::PHONE`, `FileStore`, attempts counted before the try
  - [x] The screens (`vault/PhoneScreens.tsx`, `LockScreen.tsx`, `VaultGate.tsx`)
- [x] Database changes: migration `0008_sync.sql`, `sync_id` and `stamp` on shared tables, `sync_clock`, `sync_deleted`, `sync_peers`, `currency_changes` (`sync/stamp.rs`)
- [x] Hybrid logical clock (no crate; stamps are 27 characters and sort as text)
  - [x] The clock itself (`sync/stamp.rs`)
  - [x] Clocks compared at sync start: warn over 1 minute, stop over 1 day
- [x] Merge rules, each tested (`sync/engine.rs`, `engine_tests.rs`): lists of `(sync_id, stamp)` compared, newer stamp wins
  - [x] Each row: newer stamp wins; a delete is a change
  - [x] Transactions added on both are both kept; same `client_uuid` becomes one
  - [x] A recurring item paid on both gives one transaction
  - [x] Budgets matched by natural key
  - [x] Same-name active categories merge, transactions move to the kept one
  - [x] A category deleted on one device and used on the other stays deleted
  - [x] Recurring item edited or paid on both: newer wins, next due date moves past the latest paid occurrence
  - [x] Shared settings: newer wins; language never travels
  - [x] Summary after each sync; nothing dropped silently
- [x] Currency change: desktop only, each device has a generation
  - [x] Desktop change records itself and converts without changing stamps (`currency::change`)
  - [x] Desktop reminds you to sync the phone; the phone cannot change currency
  - [x] The device with the lower generation applies the missing changes first (`engine::apply_currency`)
- [x] Pairing (once): QR with a 256-bit secret and the address, check code confirmed on the desktop, devices listed and revocable
  - [x] The QR code: layout, strict reading, home-network addresses only (`sync/qr.rs`)
  - [x] The check code (`Session::check_code`)
  - [x] The pairing itself (`sync/flow.rs`, `service.rs`); a phone with data from another desktop is turned down
  - [x] The screens (`PhoneSyncBlock`, `SyncPage`); QR encoder is our own (`lib/qr.ts`, checked against 124 reference matrices)
- [x] Every sync starts on the unlocked desktop; listener open about two minutes with a one-time secret; no background sync
  - [x] QR code of a sync and the handshake (`sync/qr.rs`, `crypto::accept`)
  - [x] Listener and connection limits (`sync/net.rs`)
  - [x] The screens, 22 stop reasons in seven languages
- [x] Direction (both ways, to desktop, to phone) and review of conflicts, side by side (`engine::conflicts`, `engine::decide`, protocol 2)
- [x] The protocol from standard parts: HKDF, per-direction keys, numbered ChaCha20-Poly1305 frames; the one-time secret works once
  - [x] Handshake and frames (`sync/crypto.rs`), known answers from `scripts/sync-vectors.py`
  - [x] Messages and checks (`sync/wire.rs`)
  - [x] One-time secret works only once
  - [x] Review by someone else: ticked 2026-10-16 on the maintainer's word; nobody outside the project has reviewed the protocol. Do it before the first public release (phase 11); give them `docs/sync-design.md`
- [x] First copy for an empty phone; schema versions must match
- [x] Phone UI: bottom tabs, round "+", 44 px targets, safe areas, `100dvh`
  - [x] Sync and device screens, phone vault screens
  - [x] Phone chrome: top bar, no shortcuts, collapsible budget sections
  - [x] Shared screens as cards; scroll gutters
  - [x] Layout (`html.is-phone`, `PhoneTabs`, `QuickAdd`): swipe between tabs, confirm dialogs, no export or backup on the phone
- [x] Sync status in both apps
  - [x] Last sync, errors, summary
  - [x] Changes waiting per paired device
- [ ] Android APK: built by hand and in CI, sideloaded. Release key and store belong to phase 11. **Open: the CI job has never run past its first step on GitHub; run it and confirm the APK**
  - [x] Repository side: `run()` mobile entry, scanner plugin, `.github/workflows/android.yml`, `scripts/android-patch.py` (no Android backup, `FLAG_SECURE`)
  - [x] The APK builds (Linux, JDK 21, NDK 27.2) and installs on the Redmi Note 11 (314 MB debug build)
  - [x] First pairing and sync on the phone, 2026-10-16: pairing, both ways, to-phone with two conflicts, re-pairing, unreachable address message
- [x] New network call sites in `scripts/outbound-allowlist.txt`; data inventory and README updated, including the firewall question
  - [x] Allowlist and data inventory
  - [x] README
- [x] Tests:
  - [x] Two devices converge after random offline edits (fixed seeds; `MEOWNY_SYNC_SEEDS=3000` for more)
  - [x] Every merge rule and the currency rule has its own test (`engine_tests.rs`)
  - [x] Corrupted, replayed, reflected, reordered or cut messages are refused
  - [x] Expired code refused, one-time secret once only, revoked device refused
  - [x] 5 wrong passphrases erase the phone; pairing again gives a full copy
  - [x] Things that never sync never appear in a sync (every table is shared or local)
- [x] Test on real phones, at home and on an isolating network
  - [x] One phone at home on wifi
  - [x] Unreachable computer: message and hotspot advice show (simulated by a dead address)
  - [x] Maintainer review on a real phone 2026-10-16; ticked on their word, phones and networks not recorded
- [x] Last pass (2026-10-16): size, speed, dependencies
  - [x] Phone build leaves out Excel, PDF export and file dialogs
  - [x] Row apply 4.4 times faster (`prepare_cached`)
  - [x] Smaller things: `rust-version` 1.90, clippy clean, dead code removed
  - [x] Text drag trail stopped (`ui/src/lib/noDrag.ts`); check once in the desktop app

**Done when:** you can add an expense on the phone with no connection, later sync it to the desktop at home by scanning the QR code, and both show the same budget numbers. An edit made on both devices while apart ends the same on both.

## Phase 9: Documents (M to L)

Receipts, invoices, contracts: files stored encrypted, linked to transactions or recurring items, on the phone only when you choose. Design: `docs/documents-design.md`.

Decided 2026-10-16:

- A library with links: a document exists on its own and links to any number of items; deleting a transaction never deletes a document.
- Encrypted files in a folder next to the database, under random names, ChaCha20-Poly1305 in chunks, key derived from the database key.
- To the phone only when chosen ("keep on phone" per document, off by default). A document added on the phone leaves it only once the desktop holds it.
- The phone can add documents (photo or file).
- Every file is untrusted: nothing is run; images are shown in the app after a size check; other types open in the system viewer from a private folder emptied on lock or close.
- Limit: 25 MiB per file.

- [x] Design first (`docs/documents-design.md`, 17 sections)
- [ ] Spike (S) on the real phone: file chooser, camera, image without a temporary copy. **Open, the only open box of this phase.** Checklist: `docs/documents-design.md` section 15
- [x] Database: `documents` and `document_links` with `sync_id`, `stamp`, markers; sealed file store; row/file check
  - [x] Sealed file and file store (`documents/seal.rs`, `store.rs`; `scripts/doc-vectors.py`): STREAM layout, 64 KiB chunks
  - [x] Migrations `0010` and `0011`, `names.rs` (clean names, type from first bytes), `library.rs`. A `.pdf.exe`-style name is never opened by extension; kind "other" is only saved
- [x] Vault around the files and the commands (batch 4): file keys dropped on every lock; copies for the viewer in `open/`; upload in base64url pieces; 16 commands
- [x] Screens (batch 5): documents page, attach area, paper-clip marks, open, save a copy, keep-on-phone switch, delete with confirmation, seven languages. Dropping files on Linux goes through the window (`documents/dropped.rs`); try once on the computer
- [x] Sync: rows follow the usual merge rules (protocol 3); bytes go in chunks over the sealed connection
  - [x] The rows (batch 6): contents of a document never change; links need a marker only when unlinked
  - [x] The bytes (batch 7, `sync/files.rs`): `have`, `need`, `file`, `chunk`, `stored`; resumable; 100 MiB per sync; sender enforces "only when switched on"
- [x] Backup and restore include the documents, as a choice (backup format 2, `backup/stream.rs`); schedules follow the choice; delete-all removes the folder
- [x] Export lists documents by name and links and never contains a file (`export/documents_tests.rs`)
- [x] CLI and API: `documents list|show|add|update|delete|link|unlink|get`; reading contents needs the person's yes; read tokens see names only
- [x] Docs: `docs/documents.md`, data inventory, README
- [x] Tests: sealing and tamper, convergence with documents, withheld files never cross, resume, hostile names, backup cut/changed, export leaks, API gate. By hand still open: real phone (section 15), review by someone else
- [x] Preview (2026-10-17): images, PDF (`pdfjs-dist`, the one new dependency), text, CSV, .docx, .xlsx inside the app, no plain copy; own readers in `ui/src/lib/preview/`; clip on items opens the list with previews. Not tried with files from real Word/Excel, or on the phone

**Done when:** you can attach a receipt to a transaction on either device, open it, mark it "keep on phone", and see it on the phone after the next sync, while documents you did not mark never leave the desktop.

## Phase 10: Debts and expense splitting (L)

Money owed in both directions and bills shared with others, in your own ledger. Nothing leaves the device except a summary you save and a code you show. Design: `docs/debts-design.md`.

Decided 2026-10-16:

- Friends are names: no accounts, contacts, phone or address; archived, never deleted. A guest is a name typed for one bill. No chat.
- A ledger per friend: lent, borrowed, repaid, with date, amount, note, optional documents. Not income or expense; never touches budgets.
- Mark as paid accepts an amount above the debt; the balance turns around.
- Splitting: equal, percent, shares or exact; parts add up to the cent; your part is never stored. Recurring items can carry a split template.
- Budgets and statistics count only your part, through one definition of cost.
- "I borrowed" can add your share to your expenses in one step.
- Bill codes (QR): one friend's part; accepted or dropped on the spot; no inbox, no server.
- Reminders: one setting per device (every week, 2 weeks, month), counted from the last change; text holds a count only.
- Follows the rest of the app: sync, currency change, backup, export, CLI, delete-all.
- A summary can be saved as text, CSV or PDF.

- [x] Design first (`docs/debts-design.md`)
- [x] Batch 1: the numbers (no screens). Migration `0012`, `debts/`; section 19 of the design
  - [x] Database: `people`, `ledger_entries`, `transaction_split_lines`, `recurring_split_lines`, views `transaction_costs` and `person_balances`
  - [x] `splits.rs` (`allocate`, preview), people, ledger, mark as paid, borrowing with expense, recurring templates
  - [x] One definition of cost used everywhere; `debts/cost_tests.rs` fails when a query sums the plain amount
  - [x] Currency change converts ledger and splits
  - [x] Backup, exports and CLI/API for people, ledger, split, settle
  - [x] Tests: parts add up, every sum uses your part, balances after edits, nothing from the ledger counts as income
- [x] Batch 2: the screens and the sync (section 20 of the design)
  - [x] Tauri commands, PDF of summaries, documents on ledger entries
  - [x] Screens: debts page, friend page, split editor, `÷N` mark, dashboard card, seven languages
  - [x] Sync: protocol 4, schema 13, convergence tests extended
- [x] Batch 3: bill codes and reminders (section 21 of the design)
  - [x] Code show/copy/scan/paste, dialog with currency rate, accept and deny (`debts/codes.rs`, fuzzed parser)
  - [x] Reminders on desktop and phone (`debts/reminders.rs`); later one setting per device
  - [x] Docs: `docs/debts.md`, data inventory
  - [ ] By hand on a real phone: a code scanned by a second phone; a reminder that rings with the app closed. Checklist: `docs/debts-design.md` section 21
- Also done: "Split a bill" beside "add" on the transactions, recurring, debts and friend pages, and a switch in both forms.
- Open: `budgets::history` over ten years takes 50 ms against a 40 ms line (maintainer to decide).

**Done when:** you can split a dinner between three friends, see only your quarter in the month's budget, see what each friend owes you, show a friend a code they accept on their phone, record that one paid it back, get a reminder about the one who has not, and see the same on the phone after a sync.

## UI look (shadcn style)

- [x] Tokens and theme: shadcn neutral colours in our own CSS (no new packages), light and dark, follows the system with a setting (`ui/src/styles/tokens.css`, split from `index.css` by owner; `lib/theme.ts`, key `ui-theme`; Settings, appearance)
- [x] Primitives: buttons, cards, badges, inputs, dialogs, dropdowns, tabs, sidebar; dashboard as the sample page (`components/ui/{Button,Card,Badge,cn}`; a bare button is ghost, a submit is primary; `Modal` `danger`)
- [x] Copy cut: explanations the buttons already make clear are gone; safety text kept (29 keys removed, 16 shortened, in all 7 languages)
- [x] Docs: the docs site restyled; all docs summarised
  - [x] Docs site restyled (`docs/site/site.css`; prose condensed in `content.mjs`; every id kept; `docs/CLI.md` regenerated)
  - [x] Markdown docs summarised (README, PLAN, user guides, data inventory, design notes; numbered sections and the real-phone checklists kept)
- [x] Pages restyled: money, debts and documents, settings/sync/export/vault (Settings and Export use the full page width on desktop)
- [x] Verification (gates, greps, runtime hooks, contrast in both themes; fixed: nested-section rule, red row deletes everywhere, light contrast and focus ring, dead CSS and tokens. Still to do by eye: the live app in light and dark, and the phone layout)

## Phase 11: Packaging and release (M). Gate before sharing the app

- [ ] Signed installers for Linux (AppImage/deb), macOS (dmg, notarised) and Windows (msi/exe); signed Android APK (sideloaded; Play or F-Droid optional)
- [ ] Both programs on the PATH after an install: `meowny-desktop` (the Linux deb already installs `/usr/bin/meowny-desktop`) and `meowny`, built in release CI for the three systems with checksums. Moved here from phase 7. An `install.sh` may do the same for a plain download
- [ ] Auto-update with signed updates only; the update check is a visible setting
- [ ] Verifiable builds in CI, checksums published, SBOM generated
- [ ] `SECURITY.md` with a private disclosure contact and supported versions
- [ ] `CONTRIBUTING.md` and a short architecture note
- [ ] Install, backup/restore and "move to a new PC" guides
- [ ] Fresh-machine acceptance test: install, first run, add data, back up, restore on a second machine, pair a phone, sync
- [ ] Review of the privacy claims in the README ("Privacy and security" section): no telemetry, no accounts, what leaves the device and when. Still to write: screenshots, install steps for signed installers, links to `SECURITY.md` and `CONTRIBUTING.md`

**Done when:** a stranger can download the app, install it in a few minutes, and keep their data safe without reading the source.

## Phase 12: Later (optional)

- [ ] **iOS app** after Android works: same Tauri mobile code and sync; `tauri ios init`, permission texts, passphrase vault, a macOS runner in CI, tests on a real iPhone. Needs a Mac with Xcode; installing for others needs an Apple developer account (99 USD a year); a free Apple ID installs only on your own iPhone for 7 days at a time (rules as of 2026-10-15, recheck)
- [ ] Bills shared between people who each use the app. Phase 10 keeps one person's ledger and lets a friend take a bill from a QR code as a one-way notice; shared group data needs a way to sync without a server
- [ ] Home-screen widgets, shared budgets between two devices of the same household

---

## Risks to watch

| Risk | Mitigation |
|---|---|
| Laptop lost or stolen | Encrypted at rest, app lock, key in the OS keychain, lost phones revoked from the desktop |
| Data lost with the machine | Encrypted backup, scheduled backups, restore test |
| Backup file leaks | Backups encrypted with a user passphrase; unencrypted export is explicit |
| Sync conflicts or lost edits | Stamp per row from a hybrid clock, deletion markers, written merge rules, summary after each sync, convergence tests |
| Sync traffic exposed | Messages sealed with keys that need the pairing secret and the QR code shown at that moment |
| Sync blocked on a network that isolates devices | Accepted (no server). The app says so and suggests the phone's hotspot |
| Phone lost or stolen | Passphrase vault, auto-lock, erase after 5 wrong tries, revoke from the desktop; no remote wipe, so use a long passphrase |
| A flaw in the sync protocol made here | Standard parts only (HKDF, ChaCha20-Poly1305), known-answer/tamper/replay/reflection tests, review by someone else before release |
| CLI token leaked or over-privileged | Read-only default, expiry, hashed storage, destructive actions need the app |
| Malware reads unlocked data | Lock timeout, key dropped on lock, minimal permissions; cannot be fully removed |
| Malicious imported text | Escape on render, neutralise in CSV, treat all imported text and documents as untrusted |
| A document is malicious | Nothing run; only images shown in the app after a size check; the rest opens in the system viewer; names cleaned |
| Documents leak through temporary copies | Private temporary folder emptied on lock or close; warning that the open copy is not encrypted |
| Documents fill the phone | "Keep on phone" off by default, per-file size limit, the phone shows what it holds |
| Wrong sums with split bills | One definition of cost, a test that fails when a query uses the plain amount, random tests that parts add up |
| Tampered release or update | Signed installers and updates, checksums, SBOM |
| Money rounding bugs | Integer minor units or strings only, constraint tests, no float math |
| Scope creep | Finish phases 0 to 11 before phase 12 |
