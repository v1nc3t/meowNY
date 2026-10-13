# Data inventory

What data this app holds, where it lives, which optional network calls exist, and who else could see it. Update this page in every phase. This is a technical note, not a privacy policy.

meowNY is local-first: no server, no accounts, no telemetry. All data stays on the user's own devices. Nobody operates a service, so there is no operator or controller holding anyone's data.

Status: phase 2. The encrypted database, its migrations, key handling, app lock and "delete all my data" exist (`src-tauri/src/{db,migrate,keys,vault}.rs`). The key is a random secret in the OS keychain, optionally combined with an Argon2id-stretched passphrase. Categories, transactions and the other tables exist in the schema but the screens that use them come in phases 3 to 5. The old server-based design (Postgres, accounts, sessions, consents) is retired and is only in git history.

## Data stored on the device

| Data | Purpose | Where | Protection | Planned in phase |
|---|---|---|---|---|
| Categories, budgets, transactions, recurring items | The budgeting product. Names and descriptions can reveal sensitive facts | Encrypted SQLite file on the desktop | SQLCipher, key from the OS keychain (and optional passphrase), app lock | 1 to 5 |
| History of creates, edits and deletes | The user's own change history | Same database (`audit_log`) | Same | 1 |
| Currency, locale, timezone, lock settings | Display money and dates, lock the app | Same database | Same | 2 |
| Database key | Unlock the database | Never stored as-is. Kept as two encrypted copies in `vault.json`: one opened by the OS keychain secret (and passphrase), one by the recovery key. The keychain holds only the random secret | OS keychain, optional passphrase | 2 |
| Recovery key | Get back in after a forgotten passphrase or a lost keychain entry | Not stored by the app. Shown once; the user keeps it | The user's own safekeeping. Anyone with it and the data folder can open the data | 2 |
| Backups and exports (JSON, CSV, Excel, PDF) | Backup, moving to a new PC, reports | A file the user chooses | Encrypted backups use a user passphrase. Exports are plain files and the screen says so | 6 |
| CLI tokens (hashed), scopes, expiry, last use | Let scripts and AI agents use the app | Same database | Stored hashed, revocable, read-only by default | 7 |
| Phone copy, device ids, sync keys, change log | Offline use on the phone, sync between devices | Encrypted database on the phone, sync keys in the OS keychain | End-to-end encrypted sync. Lost devices can be revoked from the desktop | 8 |
| Captured payment notifications (amount, merchant) | Phone capture into the review queue | Phone, then the review queue | Parsed on the device, never logged | 9 |
| Bank aggregator credentials and token | Optional bank import | OS keychain only | Never in the database, exports, CLI output or sync. Included in backups only when the backup is encrypted | 10 |
| Bank accounts and imported transactions | Optional bank import | Encrypted database | Same as other financial data. Goes through the review queue | 10 |

## Network calls

Nothing leaves the device unless the user turns a feature on.

| Call | When | What is sent | To whom |
|---|---|---|---|
| Update check | Phase 11, a visible setting | App version and platform | The project's release host |
| Phone sync | Phase 8, after pairing | End-to-end encrypted changes | The other paired device, directly or through a transport the user picks (Tailscale, WebRTC, a synced folder, or an optional relay that sees only ciphertext) |
| Bank import | Phase 10, opt-in | Credentials and requests the user's own aggregator needs | The aggregator the user registered with. This is the only external processor, and the user chooses it |

## Logs

Logs and crash reports never contain amounts, descriptions, tokens or keys. Errors cross into the UI as fixed codes, database error text is dropped, and the panic hook prints only the source location.

## Not collected

Analytics, telemetry, advertising identifiers and remote error reports.
