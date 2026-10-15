# Sync design (phase 8)

Status: design agreed on 2026-10-15, nothing built yet. The decisions come from phase 8 of [PLAN.md](PLAN.md); this file says exactly how they work, so the code can follow it and a reviewer can check it. Section 15 lists what is still open.

## 1. What it has to do

- The phone (Android) is a full copy of the desktop's data, and both can change everything.
- Away from home the phone works alone. Back home, the two sync.
- Both devices are online at the same time on the same network. Every sync is started on the unlocked desktop, which shows a QR code that the phone scans.
- No server of any kind: no mailbox, relay, signalling or STUN service, and nothing opens a port on the router.
- Very secure first, few dependencies second. The sync uses only crates the app already has: `chacha20poly1305`, `hkdf`, `sha2`, `getrandom`, `zeroize` (and `argon2` for the phone passphrase), `serde_json` for messages, and `std::net` for the connection. The only new crates are a QR encoder (desktop) and a barcode scanner plugin (phone).

Not goals: sync over the internet, more than a few devices, merging text inside a field, syncing the CLI, API tokens or backup settings.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Device id | 16 random bytes, made once per device | In the database (`sync_clock`) |
| Pairing secret (PS) | 32 random bytes, made by the desktop when a phone is paired | In the encrypted database of both devices (`sync_peers`). Never in a backup, export, CLI answer or sync |
| One-time secret (OTS) | 32 random bytes, made for one sync | On the desktop screen (the QR code) and in memory until used. Never stored |
| Session id | 8 random bytes, made for one sync | In the QR code |

A sync needs two things together: the paired phone (PS) and the screen at that moment (OTS). A stolen phone alone cannot sync, and neither can someone who photographs an old QR code.

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
 close.  The listener closes and the OTS is gone.
```

"Direct" here means the phone dials the desktop's address on the home network, like opening a page on your own network. There is no hole punching and no discovery. That is why both devices must be on the same network, and why a network that isolates devices (many hotels and campuses) does not work.

## 4. The QR codes

Text form: `MNY1:` followed by the bytes in base64url (no padding). All numbers are big-endian.

| Bytes | Field |
|---|---|
| 1 | Format version, `1` |
| 1 | Kind: `1` = sync, `2` = pair |
| 8 | Session id |
| 32 | Secret: the OTS for a sync, the new PS for a pairing |
| 2 | Port |
| 1 | Number of addresses (1 to 4) |
| 5 or 17 each | Address: 1 byte family (`4` or `6`) and 4 or 16 bytes |

With one IPv4 address that is 50 bytes, so the code is small and easy to scan. The expiry is not in the code: the desktop enforces it with its own timer (2 minutes for a sync, 5 minutes for a pairing), so a wrong phone clock cannot break it.

The desktop picks its address by asking the system which address it would use to reach the local network (a UDP socket connected to a private address; nothing is sent), and the screen also shows it in words, so a person can see when the two devices are on different networks. A field lets the person type another address.

## 5. Pairing (once)

1. On the unlocked desktop: "Add phone". It makes a new PS, opens a listener (5 minutes, one connection) and shows a pairing QR code (kind 2) that carries the PS.
2. The phone scans it and connects. The handshake of section 6 runs with the PS as the only secret.
3. Both screens show a 6-digit check code, made from the handshake (`HKDF-Expand(prk, "check")`, first 4 bytes as a number, modulo 1,000,000). The desktop asks: "Does your phone show the same code?" Only a yes continues.
4. On yes, inside the encrypted channel the devices swap device ids and names, and each stores the other with the PS. The desktop then sends the full copy (section 8).

A phone that already holds data is not paired: it must be new or erased. Two sets of data, possibly in different currencies, cannot be merged safely, and the first copy simply sets the phone's currency, currency generation and shared settings to the desktop's.

The pairing QR code holds the PS in the clear, so it is the weak moment: anyone who photographs it during those minutes could try to connect first. That is why the code is single use, short lived, and needs the person to confirm the check code on the desktop while holding the real phone. If the real phone cannot connect, or shows a different code, the person says no and nothing is stored. The PS is not used for anything until the yes.

The messages of a pairing, in order, all inside the encrypted channel: the phone sends `hello` (its name and what it holds); both screens now have the check code; the desktop asks the person and, on a yes, sends `accept`, then its own `hello`, then `currency` (its whole record of currency changes); the phone takes the currency and the generation as they are, stores the desktop and its secret, and sends a new `hello` that says what it holds now; from there it is an ordinary sync. On a no, the desktop sends `error` with the code `cancelled` and closes. The phone waits up to five minutes for the answer; neither side stores anything before it.

To pair again (new phone, lost phone, or a phone that erased itself), remove the old device on the desktop and pair again. A new PS is made every time.

## 6. The connection and the handshake

The desktop listens with `std::net::TcpListener`, bound to the one address it showed (not all addresses), on a random port. The phone tries the addresses in the code, 3 seconds each.

Notation: `seal(key, nonce, aad, plaintext)` is ChaCha20-Poly1305. `||` joins bytes. `ikm` is `PS || OTS` for a sync and `PS` for a pairing. `kind` and `session_id` come from the QR code.

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

**Session keys**, from the same `prk`: `K_p2d = Expand(prk, "p2d")` (phone to desktop) and `K_d2p = Expand(prk, "d2p")`. Each key is used once per session, because `Np` and `Nd` are fresh and random, so a fixed zero nonce for M1 and M2 is never reused under the same key.

**Frames after the handshake:**

```
frame = length(4, = length of the sealed part) || seal(K_dir, nonce = 0x00000000 || counter(8), aad = dir(1) || counter(8), plaintext)
dir   = 0x01 phone to desktop, 0x02 desktop to phone
```

The counter starts at 0 for each direction and goes up by one per frame. The receiver expects exactly the next number. A frame with a wrong, repeated or skipped number, one that does not open, or one that is longer than 64 KiB plus the 16-byte tag ends the session. A frame sent back to its sender cannot open, because each direction has its own key and its own `dir` byte.

What the desktop does with a connection:

1. It reads 66 bytes within 3 seconds. Anything else, a wrong magic, or a kind that does not match the open window (a sync window takes only kind 1, a pairing window only kind 2) is closed with nothing sent.
2. In a sync window it tries each paired secret (a few at most); in a pairing window it uses the new PS it just made. If nothing opens M1, it closes with nothing sent, so a scanner sees an open port that never answers.
3. If one opens, that sync's OTS is burned at once, whatever happens next, and the desktop sends M2.
4. After 5 connections that sent a full M1 which did not open, or 30 connections in all, the listener closes.

What the phone does: it opens M2 with K2. If that fails, it stops and says "this is not your desktop, or the code is old". Then it expects the first frame.

Limits on a session: 10 minutes, 30 seconds without a message, 256 MiB in all.

## 7. What each secret is for

| An attacker has | Can they sync? | Can they read a recorded sync? |
|---|---|---|
| Nothing but the network | No | No, only random-looking bytes |
| A photo of the sync QR code (OTS) | No, they also need PS | No |
| The phone's PS but not the screen | No, they also need the OTS | No |
| PS now, a recording from the past | No | No: the OTS of the past session was never stored |
| PS and the QR code of a session that is still open | Once, in that window (and they take the place of the real phone) | Yes, that session |
| PS and a photo of an old QR code, plus a recording of that session | No, the code is spent | Yes, that session |

There is no Diffie-Hellman step, so there is no forward secrecy beyond the OTS. The last row is the honest limit. The secrets are 256 random bits, not passwords, so nothing can be guessed offline.

## 8. What is sent, and how the two sides end up equal

Messages are JSON objects with a `t` field, each sealed into one frame of at most 64 KiB, so a long list is cut into several messages and the last one of a table is marked `last`. Money is an integer in minor units, as in the database. The message types are `hello`, `currency`, `hashes`, `digest`, `rows`, `done` and `error`.

1. **`hello`** from both sides: device id and name, app version, schema version, protocol `1`, the clock (section 10) and the currency generation (section 11).
   - Different schema versions: the sync stops with "update the phone app" or "update the desktop app".
   - Clocks more than a minute apart: a warning. More than a day apart: the sync stops with "check the clock on the phone", because a clock far in the future would make its edits win forever.
2. **`currency`**: the desktop sends the phone any currency changes it has not seen. The phone applies them first and converts its own rows, so both sides are in the same currency before anything is compared (section 11).
3. **`hashes`**: each side sends one hash per table of its list of `(sync_id, stamp, deleted)`. A table with the same hash on both sides is the same on both and is left out of everything that follows. If no table differs, the sync is already done.
4. **`digest`**: for each table that differs, each side lists `[sync_id, stamp, deleted]` for every row and every deletion marker. About 30 bytes a row, so a few MB for ten years of transactions.
5. **`rows`**: both sides now know both lists, so each works out for itself what the other lacks or has older (a missing `sync_id`, or a lower stamp) and sends exactly that, with no request message. A deletion is sent as its marker.
6. The receiver collects everything, validates it (section 12), and applies it in one database transaction. It counts what it did for the summary: rows added, rows taken over from the other device, of those how many had also been changed here since the last sync (a change of yours that was replaced by a newer one there), rows deleted, categories merged, and duplicates removed. (A row that is newer here is never sent, so the other device is the one that counts it as replaced.)
7. **`done`**: each side sends a hash of its final list of `(table, sync_id, stamp, deleted)`. If they are equal the sync is finished and both show a summary. If not, both say "differences remain, sync again", and nothing is thrown away.

When both sides have a lot to send (the lists of a big database are megabytes), they must not both write at once, or neither would ever read. So the desktop always speaks first and the phone listens, and then the other way round: the lists, then the rows, then `done`. Each side works out what the other lacks from the lists, before it applies anything of what arrives.

This is a comparison of states, not a log of changes, so it needs no record of what was sent before. A sync that stops half way, a restored backup or a wiped phone is repaired by the next sync without special cases. Applying is all or nothing on each side; if one side commits and the other does not, the next sync closes the gap.

## 9. Database changes (migrations `0008` and `0009`)

| Change | Why |
|---|---|
| `sync_id` (random 128 bits, as 32 hex characters, unique) on `categories`, `budgets`, `recurring_transactions`, `transactions`. Existing rows get one in the migration | Integer ids would collide between devices. They stay as the local ids. `transactions.client_uuid` stays |
| `stamp` (text, section 10) on the same tables and on the shared part of `settings` | Which change is newer |
| `categories.merged_into` (a `sync_id`, may be empty) | Section 11, same-name categories |
| `sync_deleted(sync_id, kind, stamp)`, filled by `DELETE` triggers on `transactions`, `budgets` and `recurring_transactions` | Those rows are hard-deleted today, so the other device needs a marker. Markers are kept for good (about 50 bytes each) |
| `sync_clock(id = 1, device_id, last_stamp, applying)` | The device id, the clock state, and a flag that stops the triggers from replacing a stamp while a sync is applying rows |
| `sync_peers(device_id, name, secret, paired_at, last_sync_at)` | The paired devices and their PS. Left out of backups, exports, the CLI and sync, like `api_tokens` |
| `currency_changes(generation, from_currency, to_currency, rate, stamp)` and `settings.currency_generation` | Section 11 |
| The language stays in `settings`, but the stamp of `settings` changes only when `locale` or `timezone` change | The UI language is per device, and a change to it must not touch a shared setting |

Stamps are set by triggers, like `updated_at` is today, so no write path (the screens, the command API, recurring items) can forget one. The triggers read `sync_clock`. While `applying = 1` they leave stamps as they are; a sync, a restore and a currency conversion turn it on inside their own transaction. The update triggers watch only the columns that are content (`UPDATE OF`), so `updated_at`, the sync columns and the link from a transaction to its recurring item never make a stamp on their own. There is one exception that follows from the link: deleting a recurring item clears the link of its payments (`ON DELETE SET NULL`), which is a change to them, so they take the stamp of the deletion marker (one tick for both, made by a trigger before the delete). A device that learns of the deletion from a sync does the same, and a payment that arrives for an item that is already deleted takes the marker's stamp unless it is newer. Migration `0009` also makes the `updated_at` triggers stand aside while `applying` is on and stop counting the recurring link, because two devices that each stamp "now" into one row would never hold equal rows. Built: `migrations/0008_sync.sql`, `sync/stamp.rs` and the tests in `sync/foundation_tests.rs`.

Never synced: `audit_log`, `backup_schedule`, `api_tokens`, `sync_peers`, `sync_clock`, the language, the lock settings, and the currency field itself (it travels as a currency change).

## 10. Stamps and the clock

A stamp is text that sorts like the tuple (milliseconds, counter, device id): `{ms:013}-{counter:04x}-{first 8 hex characters of the device id}`. A larger stamp is newer. Eight characters are enough: a tie needs two changes in the same millisecond with the same counter and the same first eight characters, and a tie only decides which of two equal-looking changes wins.

- A new local stamp: `ms = max(wall clock, last ms)`. If that equals the last ms, the counter goes up by one, otherwise it is 0. If the counter would pass `ffff`, `ms` goes up by one.
- On receiving rows or a clock, the device sets its last stamp to the larger of its own and the received ones, so the next local stamp is newer than everything it has seen.
- On start, `last_stamp` is the larger of the saved one and the largest stamp in the data.

So a phone whose clock is a minute slow still produces stamps that are newer than anything it has already received.

## 11. Merge rules

For the same `sync_id` on both sides, the larger stamp wins as a whole row (a deletion marker has a stamp too, so the newer of an edit and a delete wins). The cases that need more:

| Case | Rule |
|---|---|
| Transaction added on both | Both are kept, they have different `sync_id`s. Two with the same `client_uuid` are the same entry: the newer stays, the other becomes a deletion marker |
| Recurring item paid on both | Both made a transaction for the same item and due date. The existing unique rule allows one: the newer stays, the other becomes a deletion marker |
| Budget for the same type, scope, category and month on both | Same: matched by that key, the newer stays, the other becomes a deletion marker |
| Category with the same type and name, both active, on both | The lower `sync_id` (byte order) is kept. The other is marked deleted with `merged_into` set to it, and the budgets, recurring items and transactions that point at it are moved to the kept one |
| Category deleted on one, used on the other | It stays deleted. The new transactions stay attached to it, as old ones of a deleted category already are |
| Recurring item edited or paid on both | The newer whole row wins. Then, if its next due date is not after the latest occurrence that has a transaction (`recurring_due_date`), the date moves forward by whole intervals, with the function the app uses after a payment, until it is. So an occurrence paid on the other device does not come due again. Known limit: an occurrence skipped on the losing side (a skip makes no transaction) can come due again, and the person can skip it again |
| Shared settings (locale, time zone) | The newer wins |

Stamps of the changes these rules make follow from the stamps involved, never from the clock, so both devices work out the same ones and a merge does not start another round of changes:

- The marker of a duplicate that lost, and the merged-away category, get `derive(a, b)`: the larger of the two stamps with the counter one step on and the device id `00000000`. If the other device has already deleted the loser for good (its marker is in the same batch and covers the row), the loser takes that marker, so both end with the same one.
- A row moved to the kept category gets `moved(x)`: its own time and counter with the device part `ffffffff`. That is newer than `x`, the other device moves its copy the same way, and two different versions of the row stay different and in order after the move. (A stamp made with `derive` would give them the same stamp whenever the merge is newer than both, and an edit would be lost.)

Rules for the order things are done in, which the random tests found:

- A category row that wants a name held by a category that the same batch is about to rename waits for it, so a chain of renames (one takes the name another just gave up) never looks like two categories with one name. A swap of two names puts one of them aside under a name no person can type until its own row arrives.
- The budgets of a merged-away category are moved once every row is in, from the final versions, because the two versions of a budget that clash may not both be here yet.
- Deletions of payments come before the deletion of their recurring item, so a payment deleted on the other device does not get a newer stamp from its item's deletion and survive.
- After everything is in, every active recurring item whose next due date is not after its latest payment moves on (not only the ones the sync touched), so both devices reach the same date.

**Currency.** Only the desktop can change the currency, and afterwards it tells the person to sync the phone. A change adds a row to `currency_changes` and raises the desktop's `currency_generation`; the desktop converts its own rows at once, as it does today. A conversion does not change any row's stamp. At the start of a sync (step 2 above), the side with the lower generation applies the missing changes in order and converts all of its own rows, including ones added since the last sync, with the same function and rounding. Only then are the lists compared. If a converted amount would leave the allowed range, the sync stops with an error and nothing changes.

## 12. Applying changes safely

Rows from the other device are untrusted input, even from a paired one, so a bug or a damaged phone cannot corrupt the desktop beyond what the app's own rules allow.

- Only the synced tables can be named. A row has exactly the known fields, no more.
- Every field passes the same checks as the screens and the command API: money range, dates, allowed words, lengths, the first-of-month rule for budgets, and so on. A row that fails is refused, and the sync stops with an error that names the table (never the content).
- Rows are applied with bound parameters, never built into SQL text.
- Order: shared settings, categories, recurring items, budgets, transactions, then deletion markers. References arrive as `sync_id`s and are turned into local ids; a reference to an unknown `sync_id` stops the sync.
- After applying, the database passes `foreign_key_check` and `integrity_check` before the transaction is committed.
- Nothing from the sync (keys, rows, amounts, names) is written to a log.

## 13. The phone

- **Vault** (`Policy::PHONE` in `vault.rs`): locked by a passphrase (Argon2, as on the desktop), with no OS keychain crate: the random secret that the keychain holds on the desktop is a file next to the data (`FileStore`), so the passphrase is the protection. The database key and the pairing secret live inside the encrypted vault. The app locks itself after a while.
- **Erase**: after 5 wrong passphrases the phone deletes its database, its keys and its pairing secret. The counter is kept outside the encrypted database, so closing the app does not reset it; a warning shows from the 3rd miss and says how many tries are left. A correct passphrase resets the counter.
- **After an erase**, or on a new phone, the person pairs again and the desktop sends a full copy. Changes made on the phone since its last sync are lost, and the warning says so.
- Against someone who copies the file off the phone and guesses offline, the erase does not help. Only the length of the passphrase and Argon2 protect it, so the passphrase screen pushes for a long one. With no server there is also no remote wipe.
- **After a backup restore** on either device, the device gets a new device id and its paired devices are removed, so it has to pair again and gets a full copy. This keeps the rules above free of special cases.

## 14. Threats and what stops them

| Threat | What stops it |
|---|---|
| Someone listening on the network | Everything after the first 66 bytes is sealed; those bytes are random-looking. They learn that a sync happened, when, between which addresses, and about how much was sent |
| Someone changing or replaying messages | Every frame is sealed and numbered. M1 and M2 are bound to each other and to fresh random values. A replayed M1 finds its OTS already burned |
| A message sent back to its sender | Each direction has its own key and a direction byte |
| A fake desktop | It cannot make M2 without PS and OTS, so the phone stops |
| A fake phone | It cannot make M1 without PS and OTS, and the desktop stays silent |
| A port scan or flood | One address, a short time, no answer without a valid M1, 5 bad handshakes or 30 connections close the listener. Someone on the network can still waste the window; the person just starts again |
| A lost or stolen phone | The passphrase vault, the lock, the erase after 5 misses, and removing the device on the desktop. The phone's copy of the data stays until it is erased |
| A photographed sync QR code | Useless without PS |
| A photographed pairing QR code | Single use, 5 minutes, and the person confirms the check code on the desktop |
| A bad row from a paired device | Section 12 |
| Another version of the app | The schema versions must match |
| A flaw in this design | It is a small protocol made here from standard parts. It gets test vectors, hostile-input tests and a review by someone else before release (section 16) |

Not protected: malware on the desktop or on an unlocked phone (they hold the data anyway); someone who has both the unlocked desktop and the phone; the metadata above; a sync window wasted by someone on the network.

## 15. Open points

Settle these while building, and write the answer here:

- Triggers for stamps need a closer look: the clock update must be one statement and must hold up with several writers in the app.
- Whether the digest for transactions needs per-month hashes, once there are real numbers on a phone.
- Which address to show on machines with several networks (first version: the system's choice, plus the field to type another).
- IPv6: first version uses IPv4 only; the QR format already allows both.
- Whether a deletion marker can ever be dropped. For now they stay for good.
- The wording of the screens: pairing, the check code, the sync summary, and the errors (never include row contents).
- What Android asks for: camera permission for scanning, and whether the newest Android versions ask for a local network permission.

## 16. Tests

- **Test vectors**: fixed `PS`, `OTS`, `session_id`, `Np`, `Nd` and fixed frames, with every derived key and every output byte written down. `scripts/sync-vectors.py` is a second implementation (standard library only, checked against the test vectors of RFC 8439 and RFC 5869) that prints them, and `sync/crypto.rs` and `sync/qr.rs` hold them in their tests. Another implementation, or the reviewer, can reproduce them.
- **Hostile input**, with a fixed-seed generator as elsewhere in the project: random and damaged M1, M2 and frames; a wrong counter, repeated, skipped and reflected frames; a changed byte anywhere; a wrong length; a frame over the limit; a slow sender. The session ends and nothing panics.
- **Secrets**: an OTS works only once; a QR code with the wrong PS never gets an answer; 5 bad handshakes close the listener; secrets are zeroized after use; no key or row appears in any log line.
- **Two devices, two in-memory databases, a real socket**: random edits on both sides from a fixed seed, then a sync. They hold the same rows, and a second sync changes nothing. Every rule of section 11 has its own case. Replaying the same sync changes nothing. A sync stopped half way (at every message) is repaired by the next one.
- **Currency**: a change on the desktop, then edits on the phone, then a sync: the same amounts on both. A change that would go out of range stops the sync with nothing changed.
- **Never synced**: the tables and fields of section 9 never appear in any message.
- **Phone**: 5 wrong passphrases erase it; the counter survives a restart; pairing again restores a full copy.
- **By hand**: a real Android phone at home, and on a network that isolates devices (it should fail with a clear message that mentions the phone's hotspot).
