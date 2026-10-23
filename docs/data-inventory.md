# Data inventory

What data the app holds, where it lives, which optional network calls exist, and who else could see it. Update it in every phase. A technical note, not a privacy policy.

meowNY is local-first: no server, no accounts, no telemetry. All data stays on the user's own devices. Nobody operates a service, so there is no operator or controller holding anyone's data.

Status: phases 0 to 10 built (see `docs/PLAN.md`).

- Encrypted database and migrations, key handling, app lock, "delete all my data".
- Categories, transactions, budgets, recurring items, statistics.
- Backup and restore (also scheduled); export to JSON, CSV, Excel and PDF.
- Phase 7: CLI token storage, the local command API (off by default), Settings switch and token screen, the `meowny` program (desktop only) and its docs site.
  - Read routes show what the screens show, except the lock settings, the backup passphrase and the tokens.
  - Write routes (categories, transactions, budgets, recurring items, number format, time zone, language) need a write token.
  - Exports and backups are written only after the person allows each one in an app dialog.
  - Nothing reaches the currency, the lock settings, the tokens, restoring a backup or "delete all my data".
- Phase 9 documents: sealed files, sync of their bytes, backups with them, export lists, the command line, temporary copies for the system's viewer.
- Phase 10 debts (`docs/debts-design.md`): stored numbers, backup, exports, command line, screens, PDF of summaries and statements, documents on ledger entries, sync of friends, entries and parts, bill codes, reminders.
- Documents, the camera for a bill code and reminders on the phone were checked on a real phone by the maintainer (2026-10-23).
- The old server-based design (Postgres, accounts, sessions, consents) is retired and only in git history.

Last checked against the code (`src-tauri/src`, `ui/src`) and the files the app leaves on disk. A search of the whole data folder (database and journal files, `vault.json`, the webview's cache and storage; over 4,500 files) for text that exists only in the demo data found it in no plain place.

## Data stored on the device

| Data | Purpose | Where | Protection | Phase |
|---|---|---|---|---|
| Categories, budgets, transactions, recurring items (names, amounts, descriptions, dates; a recurring item may have no fixed amount) | The budgeting product. Names and descriptions can reveal sensitive facts | Encrypted SQLite file `meowny.db` in the data folder (with `-wal` and `-shm` while open) | SQLCipher, key from the OS keychain (and optional passphrase), app lock | 1 to 5, built |
| Change history (`audit_log`): every create, edit and delete with old and new values | The user's own history | Same database | Same. Deleting a transaction or category does not remove what it said from the history; only "delete all my data" does. In backups and in a JSON export with no limits; not in a limited export | 1, built |
| Currency, number and date format, time zone, language | Display money and dates | Same database (`settings`) | Same | 2 and 3, built |
| Lock settings (idle time, lock on minimize, lock on close) and the notification choice | Lock the app, notify about due items | `vault.json` in the data folder, a plain file readable while locked | No financial data. Also says whether a passphrase is set; holds the public Argon2 salt and the two wrapped copies of the database key | 2 and 4, built |
| Command API switch | Whether the app listens for the CLI | `vault.json` (`api_enabled`), plain, so the app knows at start whether to listen while the database is locked | No financial data. Off by default | 7, built |
| Files an export or backup makes for a token (JSON, CSV, Excel, PDF, `.meowbak`) | Let a script hand data to another program, or keep a backup | The path the token gave and the person saw in the dialog: absolute, in an existing folder, not there yet. An existing file is never replaced | Each request is described in a dialog (token, what, exact file, how many transactions) and written only on Allow. One question at a time; a "no" is remembered for the token for 30 seconds; no answer in two minutes is a no. Owner-only. Not encrypted, except the backup (locked with the passphrase the token gave; the dialog never shows it). Dialog text is English only | 7, built |
| Token saved by `meowny login` (and `MEOWNY_TOKEN`) | Let the CLI use the app without asking each time | A plain text file where the CLI runs: `~/.config/meowny/token` (Linux), `~/Library/Application Support/meowny/token` (macOS), `%APPDATA%\meowny\token` (Windows), or the file named by `--token-file` or `MEOWNY_TOKEN_FILE`. Created only by `login`; `logout` removes it | File permissions only (owner-only file and folder; the CLI refuses a file others can read). Whoever can read the person's files can use the token until it expires or is revoked in the app; read-only by default, 1 to 365 days. Not in backups or exports. "Delete all my data" does not remove it (outside the app's folder), but the app's token list goes, so it stops working | 7, built |
| Command API port file (`api.json`: API version and port) | Tell the CLI where the app listens | The data folder, while the API is on | Owner-only on Unix. No token, no data. Removed on exit, when the API is turned off, and by "delete all my data"; a crash can leave it until the next start | 7, built |
| Database key | Unlock the database | Never stored as-is. Two encrypted copies in `vault.json`: one opened by the OS keychain secret (and passphrase), one by the recovery key. The keychain (entry `database-key` of `org.meowny.app`) holds only the random secret. In memory only while unlocked; locking drops it | OS keychain, optional passphrase | 2, built |
| Recovery key | Get back in after a forgotten passphrase or lost keychain entry | Not stored by the app. Shown once. May be copied to the clipboard (stays until something else is copied) or saved as a text file the user chooses (owner-only on Unix) | The user's own safekeeping. Anyone with it and the data folder can open the data | 2, built |
| Scheduled backup setup (folder, interval, whether the documents' files are included, backup passphrase, last attempt) | Encrypted backups on a schedule while unlocked | Same database (`backup_schedule`), no audit triggers | Same. The passphrase is never shown in the UI, never logged, and left out of backups and exports | 6, built |
| Backups (`.meowbak`): every table except the scheduled backup setup, and the documents' files if the person chose to include them (the app's own backup; the command line's has rows only) | Backup, moving to a new PC | A file the user chooses, or the scheduled-backup folder (`meowny-auto-<date>-<time>.meowbak`, newest 10 kept) | Encrypted under a passphrase chosen for the backup (Argon2id, ChaCha20-Poly1305). Version 2: a stream of 64 KiB sealed chunks, so a big library is never held in memory and a cut, reordered or changed file is refused whole. The header shows only format, schema version and key-stretching settings. Owner-only on Unix, written through a temporary file next to it. A backup with documents is as big as the library, and so is each of the newest 10 a schedule keeps | 6, built; documents 9, built |
| Exports (JSON, CSV, Excel, PDF). JSON, CSV and Excel also list the documents by name, date, type, size, hash and attachments (never the files; a limited export lists only those attached to what it holds) | Reports, using the data elsewhere | A file the user chooses | Not encrypted (the guides and README say so; the export screen carries no warning, at the maintainer's request on 2026-10-17). Owner-only on Unix. Formula-looking text is written as text in CSV and Excel. A PDF can leave out descriptions. A selection leaves out the rest: a JSON limited to some categories has no other category names, no global budgets, no history. See "Export choices" below | 6, built; debts 10, built |
| Interface preferences: language (`ui-language`), sidebar state (`sidebar-collapsed`), date of the last due notification (`due-notified`), the name this device gives itself in the paired device's list (`device-name`). Debts (phase 10): the name given to friends on a bill code (`bill-sender-name`, asked at first run, changed in settings), the last expense category chosen for a code (`bill-category`), time of day of debt reminders (`debt-reminder-time`), days apart (`debt-reminder-every`: 0, 7, 14 or 30), the day of each friend's reminder this device has shown (`debt-reminded`: a friend's id and a date) and, on a phone, the ids of the notifications handed to the system (`debt-scheduled`) | Remember choices before the app is unlocked | The webview's local storage, in the data folder | Not encrypted. No financial data. The device name is sent to the paired device (inside the encrypted channel) and shown there | 3, 4 and 8, built |
| WebKit's own folders in the data folder (`WebKitCache`, `CacheStorage`, `storage`, `mediakeys`, `hsts-storage.sqlite`) | Hold the app's own pages and scripts. Data goes between the interface and the database as app commands, not web requests | The data folder | Nothing from the database was found in them | built |
| CLI access tokens: name, scope (read or write), expiry, last use, SHA-256 of the secret | Let scripts and AI agents use the command API (Settings, command line access) | Same database (`api_tokens`), no audit triggers | Only the hash is stored; the secret is shown once. Name is 1 to 50 characters with no control or hidden direction characters, so an export question cannot be made to read differently. Read-only unless write is picked; expires in 1 to 365 days; revoking deletes the row. Left out of backups and exports; a restore does not touch them | 7, built |
| Phone copy, device ids, sync keys, change log. In the encrypted database: a random sync id and a change stamp on every shared row, markers for deleted transactions and recurring items, this device's id and clock, the list of currency changes, and the table of paired devices with the secret each pairing made (filled when a phone is paired). A pairing secret is in the encrypted database only, never in a backup, export, the command line or a sync. Backups carry ids, stamps, markers and currency changes, not the device id or pairing secrets | Offline use on the phone, sync between devices | Encrypted database on the phone, locked by a passphrase; the desktop keeps the pairing secrets | Encrypted sync. Lost devices can be revoked from the desktop. The phone erases itself after 5 wrong passphrases | 8, built |
| Phone passphrase vault (phone only): the database key wrapped under the passphrase, a random secret in a file next to the data, the count of wrong passphrases in `vault.json` | Open the app on the phone, which has no keychain this app can use | The phone's data folder, outside the database so the count works while locked | The passphrase (Argon2) is the protection; the file is owner-only. 5 wrong passphrases in a row erase the database, keys and pairing secret; pairing again brings the data back (unsynced changes are lost). No recovery key: a forgotten passphrase means erasing the phone and pairing again. Screens warn from the 3rd wrong try. Android: the system backup of the app's folder is off (`allowBackup="false"`) and the window is secure (no screenshots, blank in recent apps). iOS: the data folder is kept out of iCloud backups at every start; screenshots cannot be blocked. The phone has no export or backup file | 8, built (the Android settings are applied when the APK is made) |
| Documents: the files (receipts, invoices, contracts) | The paper trail next to the transactions | One file per document in `documents/` next to the database (`<32 hex>.mnd`; `.part` for an unfinished transfer; `.tmp` while one is written). The name comes from the document's id by a key, so no column says where a file is; the folder shows only how many files and about how big | Sealed in 64 KiB chunks with ChaCha20-Poly1305, under a per-file key derived from the database key; the key exists only while unlocked, so a locked app cannot read a file. A cut, changed, moved or swapped file fails to open. Owner-only on Unix. At most 25 MiB each. Only documents marked "keep on phone" go to the phone (sealed again under the phone's key); never in the command line's output unless the person allows it in the app | 9, built |
| Documents: name, kind, size, date, SHA-256 of the contents, the "keep on phone" switch, and attachments (`documents`, `document_links`) | The list, search and attachments | The encrypted database, with a random id, a stamp and markers like other shared rows | Same as other financial data. The change history keeps the name, never the hash. Names are cleaned (last part of a path, no control or hidden characters). Shared with the phone by sync, in backups and in JSON, CSV and Excel exports (by name, never the file) | 9, built |
| Plain copies of documents for the system's viewer | Open a PDF or large picture in the program the system picks | `open/` in the data folder, one sub-folder with a random name per opening, owner-only | **Not encrypted** while it exists: the viewer has to read it. The copy gets the extension of the kind the app found, never the one in the name; a file of no known kind is never opened. Removed on lock, on close, on start (a crash leaves them) and on "delete all my data". The screen says so before the first open of a session | 9, built |
| A document's contents in the window while a preview is open | Show an image, PDF, text, CSV, Word or Excel file inside the app | The app window's memory only, until the preview closes. Nothing written to disk, no plain copy | Not encrypted in memory, like anything the window shows; only while unlocked (locking reloads the window). Read by the app's own code and by PDF.js, which gets bytes and no address: nothing is fetched, no script in a file runs. Text is cleaned of hidden and reversing characters | 9, built |
| People and debts: friends' names, a reminder date for each, ledger entries (loans, borrowings, repayments, with a note), the parts of split bills and the proportions of a recurring item's split (a friend's or a typed guest's name, and an amount or weight) | Track who owes whom; show only your part of a shared bill in the budgets | Encrypted database (`people`, `ledger_entries`, `transaction_split_lines`, `recurring_split_lines`) with random ids, stamps and markers like other shared rows. Your own part of a bill is not stored; it is worked out from the others | Same as other financial data. In backups (refused whole if a bill's parts add up to more than the bill) and in exports (see "Export choices"). Never counted as income or as a budget item. The change history keeps names and amounts. A summary or statement leaves only when the person saves it (a file the command line writes needs the person's yes in the app). **Synced with the phone** like other rows (`docs/sync-design.md`): friends and entries as rows of their own, a bill's parts inside its transaction's row, the template inside its item's, so both devices hold the same names, amounts and weights. A friend never leaves; an entry leaves a name-less deletion marker (id and stamp). A document can be attached to an entry; its file moves like any document's | 10, stored and synced |

### Export choices

- JSON, CSV and Excel can be limited by: a range of dates, categories, income or expenses only, an amount range (what it cost you), text in the name or description, bills shared with friends or not. A PDF takes only the dates and the categories.
- JSON and Excel also choose which parts go in; a part left out is not in the file or named in it.
- The dates also narrow the budgets (those in force in the range) and the recurring items (those running in it).
- Debts parts of a JSON or Excel file:
  - **debts**: the friends, each with what they owe.
  - **history of debts**: the ledger entries and the parts of shared bills (`debtHistory`, *Debt history* sheet). It also holds the recurring payments that were paid late (made after the day they fell due; one paid in time is not in it) and those overdue now.
  - **due payments**: recurring payments overdue as of today (active, expense categories, one per occurrence from the first unpaid one up to yesterday, since a payment is a debt only after its day, at most 24 per item). `duePayments` in JSON, a *Due payments* sheet. Narrowed by categories and kind, not by dates.
- With a limit, the ledger is only in when its part is chosen, narrowed by dates and not by categories; the bills' parts only for the bills the file holds; friends are named without balance or reminder.
- These are worked out from rows already stored; nothing is added to the database.
- A PDF report can add a list of debts (who owes what on the last day of its months), a list of due payments and the history of debts in its months. With some categories chosen, the bills of the others and the balance after each line are left out.

## What leaves the app without a network call

| What | Detail |
|---|---|
| Desktop notifications | Once a day while unlocked, the names of recurring items that are due or overdue (a few names, then "…"). The OS shows them and may keep a notification history. On by default; Settings, notifications, turns it off |
| The clipboard | The recovery key or a new CLI token, only when "copy" is pressed. Stays until something else is copied |
| Files | Backups, exports, copies of documents and the recovery key go where the user saves them. The app does not track ones saved by hand and cannot remove them. A copy of a document is not encrypted, and the save dialog says so |
| Links | An https link opens in the system browser, only when clicked |
| The camera (phone only) | Opened only while scanning a code (the desktop's pairing code or a friend's bill code), after the system's permission question. The picture is read for a QR code inside the camera plugin; the app keeps no picture, nothing is recorded or sent |
| A file with who owes what (phase 10) | The summary of all friends, one friend's statement or the ledger, as text, CSV or PDF (the ledger only as CSV), written where the person says; from the command line only after the person allows it in the app. The PDF is made on this computer, in the person's language from the screens and English from the command line. Names and amounts, not encrypted; the app sends it to nobody. Not on the phone |
| A bill code (phase 10) | A QR code or its text, shown or copied when the person asks: a friend's part of one bill (the sender's name as typed, the bill's name, date, amount, total and currency; never another person's name or part, and no id of the sender's database: its id is a hash). Not stored, not encrypted; whoever sees the screen or the copied text reads it, and the phone's window cannot be captured. The scanner and the paste box write nothing until Accept; Accept makes a friend, an expense and an entry that holds the code's id, so the same code is not taken twice |
| Notifications (phone) | The permission is asked the first time a reminder has a day to ring on. The app hands the system only the notification plugin's own text and time. Nothing else of the app goes to the notification system |
| Debt reminders (phase 10) | A local notification at a time of day chosen on each device, for every friend with a debt, one period (set on each device, in settings) after the day the debt last changed and again after each period. Fixed text ("meowNY: a debt reminder") and a count, never a name or amount. Changed 2026-10-17: it was a date per friend; a friend's `remind_on` and `remind_every_days` are still stored, synced and in the command API, and nothing rings from them. Computer: shown by the app while open and unlocked. Phone: the app hands the system the next four days with a reminder, which keeps them until they ring (also with the app closed); the OS may keep a history of those shown, and a debt settled elsewhere can still ring until the phone's app is opened |

## Network calls

None today. The page content policy allows only the app itself and its own command channel, and `npm run check:outbound` in CI lists every place the code could make an outbound call and fails on a new one. Nothing leaves the device unless the user turns a feature on:

| Call | When | What is sent | To whom |
|---|---|---|---|
| Update check | Phase 11, a visible setting | App version and platform | The project's release host |
| Phone sync | Phase 8, each time you start it on the desktop. The desktop listens for about two minutes (five for a pairing) on its home-network address and takes one connection that proves two secrets; everything else is closed without an answer | Changes, sealed with keys from the pairing secret and a one-time QR code. The pairing check code, each device's name, and the address and port in the QR code | The paired phone only, directly on the home network. No server, relay or other party. A QR code naming an address outside the home network is refused |

## What the app accepts from the same computer

| What | Detail |
|---|---|
| Command API (phase 7) | Off by default; Settings, command line access, turns it on. Listens on `127.0.0.1` only, on a port the system picks, so nothing on the network can reach it. Every request needs a token made in the app. A web page in the user's browser cannot use it: a request with an `Origin` header, or a `Host` that is not the loopback address and this port, is refused, and no CORS headers are sent. An export or backup is written only after the person allows it, each time, in an app dialog. A locked app answers "locked" and no data. Calls do not count as activity, so the idle lock still works. The server logs nothing. A script holding a token can read (with a write token, change) the data while unlocked, which is why tokens are read-only by default, expire and can be revoked |

Document routes:

- A read token can list and show documents (names, dates, types, sizes, hashes, attachments), never a file.
- A write token can add one, change, attach, detach and delete it. The caller reads the file itself and sends its bytes as base64url text in the body of one request, up to 35 MiB, only for the one route that takes a file, and only from a caller whose token and scope were checked from the request head before the body is read.
- A copy of a file comes out only after the person allows it in the app, as a new owner-only file whose name must end in the extension of the kind the app found, so a token cannot choose a name a system would run.

Restoring a backup, changing the currency, the lock settings, tokens, setting up a scheduled backup and "delete all my data" cannot be reached through the command API at all, with or without the person's yes. They are done in the app.

## "Delete all my data"

Removes:

- the database and its journal files
- the folder of the documents' files (`documents/`) and of plain copies (`open/`)
- `vault.json` (the two wrapped key copies and the lock settings)
- any backups in the `backups` folder of the data folder (nothing writes there today)
- the key in the OS keychain (without it, a leftover copy of the database file cannot be opened)
- the scheduled backup setup and the CLI tokens (they go with the database)
- the command API's port file (`api.json`); the API stops listening

Leaves behind:

- a token file saved by `meowny login` (it stops working because the tokens are in the database; `meowny logout` removes the file)
- backups and exports saved elsewhere, including scheduled backups in the folder the user picked
- interface preferences in the webview's storage (language, sidebar state, date of the last notification; no financial data) and WebKit's own folders
- whatever is on the clipboard or in the OS notification history

## Logs

Logs and crash reports never contain amounts, descriptions, tokens or keys. Errors cross into the UI as fixed codes, database error text is dropped, and the panic hook prints only the source location. The development command `dev_vault` prints counts and the new recovery key of demo data; it is not part of the app.

## Not collected

Analytics, telemetry, advertising identifiers and remote error reports.
