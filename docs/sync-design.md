# Sync design (phase 8)

Status: built (phases 8 to 10). Decisions come from phase 8 of [PLAN.md](PLAN.md); this file says exactly how they work, so code can follow it and a reviewer can check it. Section 15 lists open points.

## 1. What it has to do

- The phone (Android) is a full copy of the desktop's data; both can change everything.
- Away from home the phone works alone. Back home, the two sync.
- Both devices are online together on the same network. Every sync starts on the unlocked desktop, which shows a QR code the phone scans.
- No server of any kind: no mailbox, relay, signalling or STUN, no router port.
- Very secure first, few dependencies second. Only crates the app already has (`chacha20poly1305`, `hkdf`, `sha2`, `getrandom`, `zeroize`, `argon2` for the phone passphrase), `serde_json` for messages, `std::net` for the connection. New crates: a QR encoder (desktop) and a barcode scanner plugin (phone).

Not goals: sync over the internet, more than a few devices, merging text inside a field, syncing the CLI, API tokens or backup settings.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Device id | 16 random bytes, once per device | Database (`sync_clock`) |
| Pairing secret (PS) | 32 random bytes, made by the desktop when pairing | Encrypted database of both devices (`sync_peers`). Never in a backup, export, CLI answer or sync |
| One-time secret (OTS) | 32 random bytes, for one sync | Desktop screen (QR code) and memory until used. Never stored |
| Session id | 8 random bytes, for one sync | QR code |

A sync needs the paired phone (PS) and the screen at that moment (OTS). A stolen phone alone cannot sync; neither can someone with an old QR photo.

## 3. One sync in one picture

```
Phone                                              Desktop (unlocked)
                                                   "Sync with phone": opens a listener on the
                                                   home network address, shows the QR code
scan the QR code
(session id, OTS, address, port)
 ── TCP connect ───────────────────────────────▶
 ── M1: hello, sealed with key K1 ─────────────▶   tries each paired secret; stays silent if none opens it
 ◀────────────── M2: reply, sealed with key K2 ──   only a desktop that knows PS and OTS can make this
 ═══ every later message is a numbered, sealed frame ═══
 hello: versions, clock, currency generation  ◀──▶
 digests: which rows each side has (id, stamp) ◀──▶
 rows: the rows the other side lacks or has older ◀──▶
 done: hash of the final state                ◀──▶
 files: the documents' files, if any move     ◀──▶   (protocol 3, `docs/documents-design.md` section 9)
 close.  The listener closes and the OTS is gone.
```

- "Direct" means the phone dials the desktop's home-network address. No hole punching, no discovery.
- So both devices must be on the same network; networks that isolate devices (many hotels, campuses) do not work.

## 4. The QR codes

Text form: `MNY1:` then the bytes in base64url (no padding). Numbers are big-endian.

| Bytes | Field |
|---|---|
| 1 | Format version, `1` |
| 1 | Kind: `1` = sync, `2` = pair |
| 8 | Session id |
| 32 | Secret: the OTS for a sync, the new PS for a pairing |
| 2 | Port |
| 1 | Number of addresses (1 to 4) |
| 5 or 17 each | Address: 1 byte family (`4` or `6`), then 4 or 16 bytes |

- One IPv4 address is 50 bytes: small, easy to scan.
- Expiry is not in the code. The desktop enforces it with its own timer (2 minutes sync, 5 minutes pairing), so a wrong phone clock cannot break it.
- The desktop asks the system which address reaches the local network (a UDP socket connected to a private address; nothing is sent). The screen shows it in words so the person can see mismatched networks. A field lets the person type another address.

## 5. Pairing (once)

1. Unlocked desktop: "Add phone". New PS, listener (5 minutes, one connection), pairing QR (kind 2) carrying the PS.
2. The phone scans and connects; the section 6 handshake runs with the PS as the only secret.
3. Both work out a 6-digit check code (`HKDF-Expand(prk, "check")`, first 4 bytes as a number, modulo 1,000,000). The desktop shows it; the person **types it on the phone**, which compares and only on a match sends `accept`. Typing makes the check real (no "yes" to tap). Three wrong tries end the pairing.
4. Inside the encrypted channel the devices swap ids and names; each stores the other with the PS. The desktop then sends the full copy (section 8).

Order: phone `hello` → check code on both → person types it → `accept` (after three misses, `error` `cancelled`) → desktop `hello` and `currency` → phone takes currency and generation as they are, stores the desktop, sends a new `hello` → only then does the desktop store the phone → ordinary sync. Both wait up to five minutes; nothing is stored before the digits are right.

- A phone that holds data is not paired: two data sets, possibly in different currencies, cannot be merged safely. The first copy sets the phone's currency, generation and shared settings to the desktop's.
- Exception: a phone already paired with this desktop (desktop forgot it, or new keys wanted) pairs again without erasure, keeping its data and taking only missed currency changes. A phone with data from another desktop is turned down after the desktop's hello; the desktop stores the phone only after the phone's answering hello, so a turned-down pairing leaves nothing.
- The pairing QR holds the PS in the clear, the weak moment. Defences: single use, short life, typed digits. A phone from another session gets a different code, so the pairing stops with nothing stored.
- To pair again (new, lost or self-erased phone): remove the old device on the desktop and pair again. A new PS is made each time.

## 6. The connection and the handshake

The desktop listens with `std::net::TcpListener`, bound to the one address it showed, on a random port. The phone tries the addresses in the code, 3 seconds each.

Notation: `seal(key, nonce, aad, plaintext)` is ChaCha20-Poly1305. `||` joins bytes. `ikm` is `PS || OTS` for a sync, `PS` for a pairing. `kind` and `session_id` come from the QR code.

**M1 (phone to desktop), exactly 66 bytes:**

```
header = "MNY1" || kind(1) || session_id(8) || Np(32)          Np = 32 random bytes from the phone
K1     = HKDF(salt = "MNY1" || kind || session_id || Np, ikm, info = "k1")
M1     = header || seal(K1, nonce = 12 zero bytes, aad = header, "hello")
```

**M2 (desktop to phone), exactly 53 bytes:**

```
Nd     = 32 random bytes from the desktop
prk    = HKDF-Extract(salt = "MNY1" || kind || session_id || Np || Nd, ikm)
K2     = HKDF-Expand(prk, "k2")
M2     = Nd || seal(K2, nonce = 12 zero bytes, aad = M1 || Nd, "reply")
```

**Session keys**, from the same `prk`: `K_p2d = Expand(prk, "p2d")` (phone to desktop), `K_d2p = Expand(prk, "d2p")`. Each key is used once per session (`Np`, `Nd` are fresh), so the zero nonce of M1 and M2 is never reused under a key.

**Frames after the handshake:**

```
frame = length(4, = length of the sealed part) || seal(K_dir, nonce = 0x00000000 || counter(8), aad = dir(1) || counter(8), plaintext)
dir   = 0x01 phone to desktop, 0x02 desktop to phone
```

- The counter starts at 0 per direction, +1 per frame. The receiver expects exactly the next number.
- A wrong, repeated or skipped number, a frame that does not open, or one longer than 64 KiB plus the 16-byte tag ends the session.
- A frame sent back to its sender cannot open: own key and own `dir` byte per direction.

Desktop, per connection:

1. Read 66 bytes within 3 seconds. Anything else, a wrong magic, or a kind not matching the open window (sync window: kind 1 only; pairing window: kind 2 only) is closed with nothing sent.
2. Sync window: try each paired secret. Pairing window: use the new PS. If nothing opens M1, close with nothing sent (a scanner sees a port that never answers).
3. If one opens, that sync's OTS is burned at once, whatever happens next; send M2.
4. After 5 connections that sent a full M1 which did not open, or 30 connections in all, the listener closes.

Phone: open M2 with K2. On failure stop and say "this is not your desktop, or the code is old". Then expect the first frame.

Session limits: 10 minutes, 30 seconds without a message, 256 MiB in all.

## 7. What each secret is for

| An attacker has | Can they sync? | Can they read a recorded sync? |
|---|---|---|
| Nothing but the network | No | No, only random-looking bytes |
| A photo of the sync QR code (OTS) | No, also needs PS | No |
| The phone's PS but not the screen | No, also needs the OTS | No |
| PS now, a recording from the past | No | No: that session's OTS was never stored |
| PS and the QR code of a session still open | Once, in that window (taking the real phone's place) | Yes, that session |
| PS and a photo of an old QR code, plus a recording of that session | No, the code is spent | Yes, that session |

- No Diffie-Hellman step, so no forward secrecy beyond the OTS. The last row is the honest limit.
- Secrets are 256 random bits, not passwords: nothing can be guessed offline.

## 8. What is sent, and how the two sides end up equal

Messages are JSON objects with a `t` field, each sealed in one frame of at most 64 KiB; long lists are cut into several messages, the last marked `last`. Money is an integer in minor units.

- Types: `hello`, `currency`, `hashes`, `digest`, `rows`, `done`, `error`.
- Protocol 3 adds `have`, `need`, `file`, `chunk`, `stored` (documents' files, step 8).
- Protocol 4 (phase 10) adds the tables `people` and `ledger_entries`; a shared bill's parts travel inside its transaction's row, a recurring item's template inside the item's row.

1. **`hello`** from both: device id and name, app version, schema version, protocol `4` (older apps are told to update), clock (section 10), currency generation (section 11), `since` (when this device last synced with the other), and in the desktop's, the direction (`mode`).
   - Different schema versions: stop with "update the phone app" or "update the desktop app".
   - Clocks over a minute apart: warning. Over a day: stop ("check the clock on the phone"); a far-future clock would make its edits win forever.
2. **`currency`**: the desktop sends the currency changes the phone has not seen. The phone applies them and converts its rows first (section 11).
3. **`hashes`**: one hash per table of each side's list of `(sync_id, stamp, deleted)`. Equal tables are left out. If none differs, the sync is done.
4. **`digest`**: for each differing table, `[sync_id, stamp, deleted]` for every row and marker (about 30 bytes a row).
5. **`rows`**: each side sends exactly what the other lacks or has older, with no request message. A deletion is sent as its marker.
   - **Direction.** The desktop chooses before showing the code: both ways, phone to computer, or computer to phone. In a one-way sync the receiver sends empty `rows` and refuses rows from the other; `done` is not compared; the summary says it went one way.
   - **Conflicts.** A row on both devices, different, and changed on both after their last sync (the later of the two `since` values; none if never met) is a conflict. The device taking the changes in reviews them (the desktop when both ways), up to 200 at a time; the rest go to the newer change and are counted. The other device sends its version and waits. The reviewer sees both side by side, with the differing fields, and chooses.
   - The choice is carried out as stamps only: the winner gets a fresh stamp, or the reviewer's own gets the oldest possible stamp so the other version is newer. The reviewer sends `reviewed`, both compare lists again from `hashes`. Giving up ends the sync with nothing changed. A change only one device made is never a conflict.
6. The receiver validates (section 12), applies in one database transaction, and counts for the summary: added, taken over (and how many were also changed here since the last sync), deleted, categories merged, duplicates removed.
7. **`done`**: each side sends a hash of its final list of `(table, sync_id, stamp, deleted)`. Equal: finished. Not: "differences remain, sync again"; nothing is thrown away.
8. **The documents' files** move after `done`, so a broken connection costs files, never the sync. Steps and rules: `docs/documents-design.md`, section 9. The sync reports them on their own lines; a break there is a note, not a failed sync.

- Both sides cannot write at once when lists are megabytes: the desktop speaks first, then the phone.
- This compares states, not a log of changes, so nothing records what was sent. A half-finished sync, a restored backup or a wiped phone is repaired by the next sync. Applying is all or nothing per side.

## 9. Database changes (migrations `0008`, `0009`, `0012` and `0013`)

| Change | Why |
|---|---|
| `sync_id` (random 128 bits, 32 hex characters, unique) on `categories`, `budgets`, `recurring_transactions`, `transactions`; existing rows get one | Integer ids would collide between devices; they stay as local ids. `transactions.client_uuid` stays |
| `stamp` (text, section 10) on the same tables and on the shared part of `settings` | Which change is newer |
| `categories.merged_into` (a `sync_id`, may be empty) | Section 11, same-name categories |
| `sync_deleted(sync_id, kind, stamp)`, filled by `DELETE` triggers on `transactions`, `budgets`, `recurring_transactions`, `documents`, `document_links` | Hard-deleted rows need a marker for the other device. Kept for good (about 50 bytes each). A link has one only when a person unlinked it |
| `documents`, `document_links` (migrations `0010`, `0011`; phase 9) | Rows with `sync_id` and `stamp`; files travel after the rows (`docs/documents-design.md`, section 9) |
| `people`, `ledger_entries` (migration `0012`; phase 10, `docs/debts-design.md`) | A friend (never deleted, so no marker) and a ledger entry (has a marker; `sync_deleted` accepts kind `ledger_entries`). Parts (`transaction_split_lines`) and template lines (`recurring_split_lines`) have no `sync_id`, stamp or marker: a change gives the parent a new stamp (trigger) and they travel in the parent's row. `document_links` has a third target, a ledger entry |
| Migration `0013` | A ledger entry loses its expense link when the expense is deleted (foreign key). Without a stamp, an entry could end linked on one device and not the other. So the entries of a deleted expense take the deletion marker's stamp (a trigger before the delete ticks the clock, one after makes the marker with the same stamp). A device learning of the deletion by sync does the same |
| `sync_clock(id = 1, device_id, last_stamp, applying)` | Device id, clock state, and a flag that stops triggers replacing a stamp while a sync applies rows |
| `sync_peers(device_id, name, secret, paired_at, last_sync_at)` | Paired devices and their PS. Left out of backups, exports, CLI and sync |
| `currency_changes(generation, from_currency, to_currency, rate, stamp)` and `settings.currency_generation` | Section 11 |
| Language stays in `settings`; the `settings` stamp changes only on `locale` or `timezone` | UI language is per device |

- Stamps come from triggers, so no write path can forget one. While `applying = 1` triggers leave stamps alone; a sync, restore or currency conversion sets it inside its own transaction.
- Update triggers watch only content columns (`UPDATE OF`), so `updated_at`, the sync columns and a transaction's link to its recurring item never make a stamp.
- Exception: deleting a recurring item clears its payments' link (`ON DELETE SET NULL`), a change to them, so they take the deletion marker's stamp (one tick for both). A device learning of it by sync does the same; a payment arriving for an already deleted item takes the marker's stamp unless newer.
- Migration `0009` makes the `updated_at` triggers stand aside while `applying` is on and ignore the recurring link.
- Built in `migrations/0008_sync.sql`, `sync/stamp.rs`, tests in `sync/foundation_tests.rs`.

Never synced: `audit_log`, `backup_schedule`, `api_tokens`, `sync_peers`, `sync_clock`, the language, the lock settings, and the currency field itself (it travels as a currency change).

## 10. Stamps and the clock

A stamp is text that sorts like (milliseconds, counter, device id): `{ms:013}-{counter:04x}-{first 8 hex characters of the device id}`. Larger is newer. A tie needs two changes in the same millisecond, same counter, same first eight characters; it only decides which of two equal-looking changes wins.

- New local stamp: `ms = max(wall clock, last ms)`. If equal to last ms, counter +1, else 0. If the counter would pass `ffff`, `ms` goes up by one.
- On receiving rows or a clock, the last stamp becomes the larger of own and received, so the next local stamp is newer than everything seen.
- On start, `last_stamp` is the larger of the saved one and the largest stamp in the data.
- So a phone with a slow clock still produces stamps newer than anything it received.

## 11. Merge rules

The larger stamp wins as a whole row (a marker has a stamp, so the newer of an edit and a delete wins). More cases:

| Case | Rule |
|---|---|
| Transaction added on both | Both kept. Same `client_uuid` is the same entry: newer stays, the other becomes a marker |
| Recurring item paid on both | The existing unique rule allows one transaction: newer stays, other becomes a marker |
| Budget for the same type, scope, category and month on both | Matched by that key; newer stays, other becomes a marker |
| Same-name, same-type active category on both | Lower `sync_id` kept. The other is marked deleted with `merged_into`; budgets, recurring items and transactions pointing at it move to the kept one |
| Category deleted on one, used on the other | Stays deleted; new transactions stay attached |
| Recurring item edited or paid on both | Newer whole row wins; then if its next due date is not after the latest occurrence with a transaction (`recurring_due_date`), the date moves forward by whole intervals until it is. Known limit: an occurrence skipped on the losing side can come due again |
| Shared settings (locale, time zone) | Newer wins |
| Friend edited on both | Newer whole row wins; a real conflict goes to the review dialog |
| Same-name active friend on both | Lower `sync_id` kept. The other is archived with `merged_into` (hidden, never listed or found, never comes back); entries, parts and template lines move to the kept one with `moved` stamps. Two parts for one friend on a bill become one with amounts added (template: weights added) |
| Entry added, edited or deleted on both | Both added kept; otherwise newer whole row wins. Entries apply after transactions, entry markers before transaction markers |
| Bill split on both, or amount on one and split on the other | Newer whole transaction wins, with its parts (replaced as a unit). `splits::check` runs before commit; a sync that would leave parts above a bill, a part on an income or a template without a payer ends with an error and changes nothing |
| Recurring item or its template edited on both | Newer whole row wins, with its template |
| Expense deleted on one, entry linked on the other | Entry stays, link cleared with the deletion's stamp (migration `0013`) |
| Documents and links | `docs/documents-design.md`, section 10 |

Stamps of changes these rules make come from the stamps involved, never the clock, so both devices work out the same ones:

- A duplicate that lost, and a merged-away category, get `derive(a, b)`: the larger stamp with the counter one step on and device id `00000000`. If the other device already deleted the loser (marker in the same batch), the loser takes that marker.
- A row moved to the kept category gets `moved(x)`: its own time and counter with device part `ffffffff`, newer than `x`; the other device moves its copy the same way.

Ordering rules the random tests found:

- A category wanting a name that one in the same batch is about to give up waits for it; a swap of two names puts one aside under a name no person can type until its own row arrives.
- Budgets of a merged-away category move once every row is in, from the final versions.
- The odd cents of a bill rescaled by a currency change go by position, and positions come from what devices share (friends by `sync_id`, then guests by name), never storage order.
- Payment deletions come before their recurring item's deletion.
- After everything is in, every active recurring item whose next due date is not after its latest payment moves on.

Known limit: a row a category merge moved has no device part (`moved`), so two different versions made in the same millisecond with the same counter, both moved by the merge, end with equal stamps and different content. A person cannot do this; a test loop can, so random tests keep the devices' edits in different milliseconds.

**Currency.** Only the desktop changes the currency, then tells the person to sync the phone.

- A change adds a row to `currency_changes`, raises `currency_generation`, and converts its rows at once. No stamp changes.
- At sync start (step 2), the side with the lower generation applies the missing changes in order and converts all its rows (including ones added since the last sync) with the same function and rounding. Only then are lists compared.
- If a converted amount would leave the allowed range, the sync stops with an error and nothing changes.

## 12. Applying changes safely

Rows from the other device are untrusted input, even from a paired one.

- Only synced tables can be named. A row has exactly the known fields.
- Every field passes the same checks as the screens and the API: money range, dates, allowed words, lengths, first-of-month for budgets. A failing row is refused and the sync stops with an error naming the table (never the content).
- Bound parameters only, never SQL text.
- Order: shared settings, categories, friends, recurring items, budgets, transactions (with parts), ledger entries, documents, document links, then deletion markers (ledger entries' before transactions'). References arrive as `sync_id`s and become local ids; an unknown `sync_id` stops the sync.
- After applying, `foreign_key_check` and `integrity_check` pass before commit.
- Nothing from a sync (keys, rows, amounts, names) is logged.

## 13. The phone

- **Vault** (`Policy::PHONE` in `vault.rs`): passphrase (Argon2) and no OS keychain crate. The keychain's random secret is a file next to the data (`FileStore`), so the passphrase is the protection. The app locks after a while.
- **Erase**: after 5 wrong passphrases the phone deletes its database, keys and pairing secret. The counter is kept outside the database; warning from the 3rd miss; a correct passphrase resets it.
- **After an erase**, or on a new phone: pair again, full copy. Changes made on the phone since its last sync are lost, and the warning says so.
- Against someone who copies the file and guesses offline, only passphrase length and Argon2 help, so the screen pushes for a long one. No server means no remote wipe.
- **After a backup restore** on either device: new device id, paired devices removed, pair again.
- **Phone screens.** The app asks once which app it is (`app_profile`, no unlock) and draws nothing before it knows.
  - First start: language and passphrase (twice; warning under 12 characters, minimum 8); says 5 wrong tries erase the phone; no recovery key. Currency is a placeholder (`EUR`) until the first pairing, which converts nothing.
  - Lock screen: warns from the 3rd miss; after the 5th shows "this phone was erased". "Forgot my passphrase" is "erase this phone" (type `DELETE`).
  - Settings have no command line, phone or recovery-key block and no currency change. The phone locks on return from background after "lock after". The sync screen opens first until paired.
- **Shell.** Top bar (name; sync, settings, lock), four swipeable bottom tabs (dashboard, transactions, recurring, categories; budgets are on the categories page), and a round "+" for quick add (amount first, recent categories as buttons, today's date). No export or backup block. Targets at least 44 px, inputs at least 16 px, safe areas respected.
- **Android hardening** (`scripts/android-patch.py`, run after `tauri android init`):
  - `allowBackup="false"`: Android's backup would put the encrypted data, `vault.json` and the secret file side by side in the person's Google account, making the erase pointless.
  - `FLAG_SECURE` on the window: no screen capture, blank recent-apps picture.
  - Camera only while scanning. Sync uses Rust sockets, not the web view; it needs the `INTERNET` permission every Tauri app has.
- **iOS hardening** (2026-10-23; built in CI, not yet tried on an iPhone):
  - The data folder is kept out of iCloud backups (`NSURLIsExcludedFromBackupKey`), set at every start (`lib.rs`, `keep_out_of_backups`); the app does not start without it.
  - `src-tauri/Info.ios.plist` gives the camera and local network texts. iOS asks once to allow the local network; the first connection may fail while it asks, so scan again.
  - iOS cannot block screenshots, and the opener plugin cannot hand a file to another app there, so documents are previewed in the app only.
- **Desktop screens.** All in the "phone" block of settings: paired phones (name, when paired, last sync, two-step remove), a warning per phone not synced since a currency change, this computer's name, "add a phone" and "sync with phone". While running: the QR code (address in words, countdown), the six digits, progress, then the summary or stop reason. Leaving the page closes the window. Screens only show `sync_status`, polled twice a second.

## 14. Threats and what stops them

| Threat | What stops it |
|---|---|
| Listening on the network | Everything after the first 66 bytes is sealed; those look random. They learn that a sync happened, when, between which addresses, and about how much |
| Changing or replaying messages | Every frame sealed and numbered. M1 and M2 bound to each other and to fresh randoms. A replayed M1 finds its OTS burned |
| A message sent back to its sender | Own key and direction byte per direction |
| A fake desktop | Cannot make M2 without PS and OTS; the phone stops |
| A fake phone | Cannot make M1 without PS and OTS; the desktop stays silent |
| Port scan or flood | One address, short time, no answer without a valid M1, 5 bad handshakes or 30 connections close the listener. Someone on the network can still waste the window; the person starts again |
| Lost or stolen phone | Passphrase vault, lock, erase after 5 misses, removing the device on the desktop. The copy stays until erased |
| Photographed sync QR code | Useless without PS |
| Photographed pairing QR code | Single use, 5 minutes, and the person types the check code into the real phone |
| A bad row from a paired device | Section 12 |
| Another app version | Schema versions must match |
| A flaw in this design | Small protocol from standard parts; test vectors, hostile-input tests and a review by someone else before release (section 16) |

Not protected: malware on the desktop or an unlocked phone; someone with both the unlocked desktop and the phone; the metadata above; a sync window wasted by someone on the network.

## 15. Open points

- Stamp triggers: the clock update must be one statement and hold up with several writers.
- Per-month hashes for the transactions digest, if real data gets much bigger. Measured with ten years of heavy use (60 000 transactions, `cargo run --release --example bench`): first copy 28 MB in 1.6 s in memory; a sync with nothing new 1.5 KB in 0.14 s; a sync with 500 changes 8.5 MB in 0.4 s. A phone is several times slower; wifi carries 8 MB in a second. So the whole list is sent for now; monthly hashes would be a protocol change.
- ~~Which address to show with several networks.~~ Settled: the system's choice, shown in words, plus an "another address…" field (`sync_start`, `sync_start_pairing`; only a home-network, non-loopback address is accepted).
- IPv6: first version uses IPv4 only; the QR format allows both.
- Whether a deletion marker can ever be dropped. For now they stay.
- ~~Screen wording.~~ Done in seven languages (`sync.*`, `phone.*` in `ui/src/i18n`); a summary is "label: number" lines; the 22 stop reasons each have one text.
- Android: the camera permission is asked by the plugin when a scan starts. Whether newer Android's "local network" permission (planned for Android 17) stops dialling a home address is for a real device to say. The Android 13+ notification permission is not asked for yet.

## 16. Tests

- **Test vectors:** fixed `PS`, `OTS`, `session_id`, `Np`, `Nd` and frames with every derived key and output byte. `scripts/sync-vectors.py` (standard library only, checked against RFC 8439 and RFC 5869 vectors) prints them; `sync/crypto.rs` and `sync/qr.rs` hold them in tests.
- **Hostile input** (fixed-seed generator): random and damaged M1, M2 and frames; wrong, repeated, skipped, reflected counters; a changed byte anywhere; wrong length; over-limit frame; slow sender. The session ends, nothing panics.
- **Secrets:** an OTS works once; a wrong-PS QR never gets an answer; 5 bad handshakes close the listener; secrets zeroized; no key or row in any log.
- **Two devices, in-memory databases, a real socket:** random edits on both from a fixed seed, then a sync; same rows, a second sync changes nothing. Every section 11 rule has its own case. A sync stopped at every message is repaired by the next. Replays change nothing.
- **Debts** (`sync::engine_tests::debts`, 16 tests, plus random tests that edit friends, entries, parts and templates): parts travel with their bill; templates with their item; same-name friends merge; entries and deletions; hostile rows per section 12 refused; the finished-state check stops a rule-breaking sync. A hunt of 3,000 plain and 2,000 currency seeds (`MEOWNY_SYNC_SEEDS=500 MEOWNY_SYNC_FROM=1 cargo test --lib sync::engine_tests::random`) found four faults, now fixed.
- **Currency:** a desktop change, phone edits, a sync: same amounts. An out-of-range change stops the sync with nothing changed.
- **Never synced:** the tables and fields of section 9 never appear in a message.
- **Phone:** 5 wrong passphrases erase; the counter survives a restart; pairing again restores a full copy.
- **Screens** (`ui/src/api/sync.test.ts`, `ui/src/lib/qr.test.ts`): every status the app sends is accepted and others refused; translated reasons match `sync/service.rs` exactly; the QR encoder equals a reference module for module in 124 cases (also read back by an independent decoder).
- **By hand:** a real Android phone at home, and on a network that isolates devices (should fail clearly, mentioning the hotspot). Done 2026-10-16 with one phone (Redmi Note 11, Android 13) and `src-tauri/examples/sync_desktop.rs`: pairing with typed digits (wrong and right), first copy, both-ways sync with review on the desktop, to-phone sync with review on the phone, re-pairing after the desktop forgot the phone, and an unanswered address (hotspot advice shows). Not checked: a really isolating network, other phone sizes, newer Android, scanning a real code with the camera. The maintainer then reviewed the app on a real phone (2026-10-16); details not written down.
