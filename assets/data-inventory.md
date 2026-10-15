# Data inventory

What data this app holds, where it lives, which optional network calls exist, and who else could see it. Update this page in every phase. This is a technical note, not a privacy policy.

meowNY is local-first: no server, no accounts, no telemetry. All data stays on the user's own devices. Nobody operates a service, so there is no operator or controller holding anyone's data.

Status: end of phase 6. Built: the encrypted database and its migrations, key handling, app lock and "delete all my data", categories, transactions, budgets, recurring items, statistics, backup and restore (also on a schedule) and export to JSON, CSV, Excel and PDF. Started in phase 7: the storage for CLI access tokens and the local command API server (off by default; it has read routes, which show what the screens show except the lock settings, the backup passphrase and the tokens, and write routes for categories, transactions, budgets, recurring items and the number format, time zone and language, which need a write token (nothing reaches the currency, the lock settings, the tokens, backups, exports or "delete all my data"); Settings has its switch and the token screen, and there is no CLI yet). Not built yet: the rest of the command API and the CLI (7), phone sync (8), phone capture (9) and the bank connection (10); their rows below are plans. The old server-based design (Postgres, accounts, sessions, consents) is retired and is only in git history.

Last checked against the code (`src-tauri/src`, `client/src`) and the files the app leaves on disk. A search of the whole data folder (the database and its journal files, `vault.json` and the webview's cache and storage, over 4,500 files) for text that exists only in the demo data found it in none of the plain places.

## Data stored on the device

| Data | Purpose | Where | Protection | Phase |
|---|---|---|---|---|
| Categories, budgets, transactions, recurring items (names, amounts, descriptions, dates; a recurring item may have no fixed amount) | The budgeting product. Names and descriptions can reveal sensitive facts | Encrypted SQLite file `meowny.db` in the app's data folder (with `-wal` and `-shm` files while it is open) | SQLCipher, key from the OS keychain (and optional passphrase), app lock | 1 to 5, built |
| Change history (`audit_log`): every create, edit and delete with the old and new values | The user's own history | Same database | Same. Deleting a transaction or category does not remove what it said from the history; only "delete all my data" does. The history is in backups and in a JSON export with no limits, and not in a limited export | 1, built |
| Currency, number and date format, time zone, language | Display money and dates | Same database (`settings`) | Same | 2 and 3, built |
| Lock settings (idle time, lock on minimize, lock on close) and the notification choice | Lock the app, notify about due items | `vault.json` in the data folder, a plain file that can be read while the app is locked | It holds no financial data. It also says whether a passphrase is set and holds the public Argon2 salt and the two wrapped copies of the database key | 2 and 4, built |
| Command API switch | Whether the app listens for the CLI | `vault.json` (`api_enabled`), a plain file, so the app knows at start whether to listen while the database is locked | It holds no financial data. Off by default | 7, built |
| Command API port file (`api.json`: the API version and the port) | Tell the CLI where the app listens | The data folder, while the API is on | Owner-only on Unix. Holds no token and no data. Removed when the app exits or the API is turned off, and by "delete all my data"; a crash can leave it behind until the next start | 7, built |
| Database key | Unlock the database | Never stored as-is. Kept as two encrypted copies in `vault.json`: one opened by the OS keychain secret (and passphrase), one by the recovery key. The keychain (entry `database-key` of `org.meowny.app`) holds only the random secret. In memory only while unlocked; locking drops it | OS keychain, optional passphrase | 2, built |
| Recovery key | Get back in after a forgotten passphrase or a lost keychain entry | Not stored by the app. Shown once. It can be copied to the clipboard, which keeps it until something else is copied, or saved as a text file the user chooses (owner-only permissions on Unix) | The user's own safekeeping. Anyone with it and the data folder can open the data | 2, built |
| Scheduled backup setup (folder, interval, backup passphrase, last attempt) | Make encrypted backups on a schedule while the app is unlocked | Same database (`backup_schedule`), with no audit triggers | Same. The passphrase is never shown in the UI, never logged, and left out of backups and exports | 6, built |
| Backups (`.meowbak`): every table except the scheduled backup setup | Backup, moving to a new PC | A file the user chooses, or the folder picked for scheduled backups (`meowny-auto-<date>-<time>.meowbak`, the newest 10 kept) | Encrypted under a passphrase chosen for the backup (Argon2id and ChaCha20-Poly1305). The file's header shows only its format, its schema version and the key-stretching settings. Written owner-only on Unix, through a temporary file next to it | 6, built |
| Exports (JSON, CSV, Excel, PDF) | Reports, using the data elsewhere | A file the user chooses | Not encrypted, and the export screen says so. Written owner-only on Unix. Text that looks like a formula is written as text in CSV and Excel. A PDF can leave out descriptions. A selection leaves out the rest: a JSON limited to some categories has no other category names, no global budgets and no history | 6, built |
| Interface preferences: language (`ui-language`), sidebar state (`sidebar-collapsed`), date of the last due notification (`due-notified`) | Remember choices before the app is unlocked | The webview's local storage, in the data folder | Not encrypted. No financial data | 3 and 4, built |
| WebKit's own folders in the data folder (`WebKitCache`, `CacheStorage`, `storage`, `mediakeys`, `hsts-storage.sqlite`) | Hold the app's own pages and scripts. Data goes between the interface and the database as app commands, not as web requests | The data folder | Nothing from the database was found in them | built |
| CLI access tokens: a name, scope (read or write), expiry, last use and the SHA-256 of the secret | Let scripts and AI agents use the app through the command API (phase 7). Made in Settings, command line access | Same database (`api_tokens`), with no audit triggers | Only the hash is stored; the secret is shown once, when the token is made. Read-only unless the person picks write; expires in 1 to 365 days; revoking deletes the row. Left out of backups and exports, and a restore does not touch them | 7, storage built |
| Phone copy, device ids, sync keys, change log | Offline use on the phone, sync between devices | Encrypted database on the phone, sync keys in the OS keychain | End-to-end encrypted sync. Lost devices can be revoked from the desktop | 8, planned |
| Captured payment notifications (amount, merchant) | Phone capture into the pending list | Phone, then the pending list | Parsed on the device, never logged | 9, planned |
| Bank aggregator credentials and token | Optional bank import | OS keychain only | Never in the database, exports, CLI output or sync. Included in backups only when the backup is encrypted | 10, planned |
| Bank accounts and imported transactions | Optional bank import | Encrypted database | Same as other financial data. Goes through the pending list (phase 9) | 10, planned |

## What leaves the app without a network call

| What | Detail |
|---|---|
| Desktop notifications | Once a day while the app is unlocked, the names of recurring items that are due or overdue (a few names, then "…"). The operating system shows them and may keep a notification history. On by default; Settings, notifications, turns it off |
| The clipboard | The recovery key, or a new CLI token, only when "copy" is pressed. It stays there until something else is copied |
| Files | Backups, exports and the recovery key go where the user saves them. The app does not track the ones saved by hand and cannot remove them |
| Links | An https link opens in the system browser, only when it is clicked |

## Network calls

None today. The page content policy allows only the app itself and its own command channel, and `npm run check:outbound` in CI lists every place the code could make an outbound call and fails on a new one. Nothing leaves the device unless the user turns a feature on:

| Call | When | What is sent | To whom |
|---|---|---|---|
| Update check | Phase 11, a visible setting | App version and platform | The project's release host |
| Phone sync | Phase 8, after pairing | End-to-end encrypted changes | The other paired device, directly or through a transport the user picks (Tailscale, WebRTC, a synced folder, or an optional relay that sees only ciphertext) |
| Bank import | Phase 10, opt-in | Credentials and requests the user's own aggregator needs | The aggregator the user registered with. This is the only external processor, and the user chooses it |

## What the app accepts from the same computer

| What | Detail |
|---|---|
| Command API (phase 7) | Off by default; Settings, command line access, turns it on. When on, the app listens on `127.0.0.1` only, on a port the system picks, so nothing on the network can reach it. Every request needs a token made in the app. A web page in the user's browser cannot use it: a request with an `Origin` header or a `Host` that is not the loopback address and this port is refused, and the API sends no CORS headers. A locked app answers "locked" and no data. Calls do not count as activity, so the idle lock still works. The server logs nothing. A script that holds a token can read (or, with a write token, change) the data while the app is unlocked, which is why tokens are read-only by default, expire and can be revoked |

## "Delete all my data"

Removes the database and its journal files, `vault.json` (which holds the two wrapped copies of the key and the lock settings), any backups in the `backups` folder of the data folder (nothing writes there today), and the key in the OS keychain. Without that key, a leftover copy of the database file cannot be opened. The scheduled backup setup and the CLI tokens go with the database. The command API's port file (`api.json`) is removed too, and the API stops listening.

Leaves behind: backups and exports saved elsewhere, including the scheduled backups in the folder the user picked; the interface preferences in the webview's storage (language, sidebar state, the date of the last notification; no financial data) and WebKit's own folders; and whatever is on the clipboard or in the operating system's notification history.

## Logs

Logs and crash reports never contain amounts, descriptions, tokens or keys. Errors cross into the UI as fixed codes, database error text is dropped, and the panic hook prints only the source location. The development command `dev_vault` prints counts and the new recovery key of demo data; it is not part of the app.

## Not collected

Analytics, telemetry, advertising identifiers and remote error reports.
