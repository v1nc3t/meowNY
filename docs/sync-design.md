# Sync design

How the phone and the computer sync, directly, with no server. Section numbers are cited in the code; keep them.
User side: the [guide](https://v1nc3t.github.io/meowNY/guide.html#phone).

## 1. What it has to do

- The phone is a full copy; both devices can change everything, and the phone works alone away from home.
- Every sync starts on the unlocked desktop, which shows a QR code; both devices are on the same network.
- No server, relay, signalling or open router port. Few dependencies: the crypto crates the app already has, `std::net`.
- Not goals: sync over the internet, many devices, merging inside a field, syncing tokens or backup settings.

## 2. The pieces

| Name | What | Where |
|---|---|---|
| Device id | 16 random bytes per device | `sync_clock` |
| Pairing secret (PS) | 32 random bytes, made at pairing | Encrypted database of both (`sync_peers`); never in a backup, export, CLI answer or sync |
| One-time secret (OTS) | 32 random bytes per sync | The QR code and memory, never stored |
| Session id | 8 random bytes per sync | The QR code |

A sync needs both the PS (the paired phone) and the OTS (the screen right now).

## 3. One sync in one picture

```
Phone                                         Desktop (unlocked): listener on its home address, shows QR
scan QR (session id, OTS, address, port)
 ── M1: hello sealed with K1 ────────────▶   tries each PS; silent if none opens it
 ◀──────────── M2: reply sealed with K2 ──   only a desktop with PS and OTS can make it
 ═══ numbered sealed frames: hello, currency, hashes, digest, rows, done, then documents' files ═══
```

The phone dials the desktop's home-network address; networks that isolate devices do not work.

## 4. The QR codes

`MNY1:` then base64url (no padding), big-endian: version `1` (1 byte), kind (1: `1` sync, `2` pair), session id (8),
secret (32: OTS, or the new PS when pairing), port (2), address count (1, up to 4), each address as family `4`/`6` (1)
plus 4 or 16 bytes. Expiry is the desktop's own timer (2 minutes sync, 5 pairing), never the phone's clock. The
address is the system's choice, shown in words, with a field to type another.

## 5. Pairing (once)

1. Desktop "add a phone": new PS, a 5-minute listener for one connection, a kind-2 QR holding the PS.
2. The phone connects; the section 6 handshake runs with the PS alone.
3. Both derive a 6-digit check code (`HKDF-Expand(prk, "check")`, first 4 bytes mod 1,000,000). The desktop shows it,
   the person **types** it on the phone, which sends `accept` only on a match. Three misses end the pairing.
4. Ids and names are swapped; the desktop sends the full copy. Nothing is stored before the digits match.

Only an empty phone pairs (or one already paired with this desktop, which keeps its data). The PS is in the clear in
the pairing QR: single use, short life and typed digits defend it.

## 6. The connection and the handshake

TCP on the one shown address, random port. `seal` is ChaCha20-Poly1305; `ikm` is `PS || OTS` for a sync, `PS` for a
pairing.

```
M1 (66 bytes)  header = "MNY1" || kind(1) || session_id(8) || Np(32)
               K1 = HKDF(salt = "MNY1" || kind || session_id || Np, ikm, info = "k1")
               M1 = header || seal(K1, nonce = 12 zero bytes, aad = header, "hello")
M2 (53 bytes)  prk = HKDF-Extract(salt = "MNY1" || kind || session_id || Np || Nd, ikm);  K2 = Expand(prk, "k2")
               M2 = Nd || seal(K2, nonce = 12 zero bytes, aad = M1 || Nd, "reply")
Keys           K_p2d = Expand(prk, "p2d"), K_d2p = Expand(prk, "d2p")
Frames         length(4) || seal(K_dir, nonce = 0x00000000 || counter(8), aad = dir(1) || counter(8), plaintext)
               dir 0x01 phone to desktop, 0x02 back; counter from 0, exactly +1 each frame; at most 64 KiB
```

- The desktop reads 66 bytes within 3 seconds, closes silently on anything wrong, burns the OTS as soon as M1 opens,
  and closes the listener after 5 bad handshakes or 30 connections.
- A wrong, repeated or skipped counter, a frame that does not open, or a reflected frame ends the session.
- Limits: 10 minutes, 30 seconds of silence, 256 MiB.

## 7. What each secret is for

Network only, a photo of the QR, or the PS alone: no sync, nothing readable. PS plus the QR of a window still open:
one sync in that window. PS plus a photo of an old QR plus a recording of that session: that session can be read.
There is no Diffie-Hellman step, so that last case is the honest limit. Secrets are 256 random bits, not passwords.

## 8. What is sent, and how the two sides end up equal

JSON messages with a `t` field, one per frame, long lists split. Money in minor units.

1. `hello`: id, name, versions (schema must match; protocol 4), clock (over a day apart stops), currency generation,
   `since`, and the direction the desktop chose (both ways, to the computer, to the phone).
2. `currency`: changes the phone has not seen, applied before anything else (section 11).
3. `hashes` per table, then `digest` (`sync_id`, stamp, deleted) for tables that differ.
4. `rows`: each side sends what the other lacks or has older; deletions as markers. One-way syncs refuse the other side's rows.
5. **Conflicts:** a row changed on both since their last sync. The receiving side reviews up to 200 side by side;
   the choice is carried out with stamps only, then lists are compared again.
6. Apply in one transaction (section 12), count for the summary, then `done` compares a hash of the final state.
7. Documents' files move after `done` (`documents-design.md`, section 9).

States are compared, not change logs, so a broken or half sync is repaired by the next.

## 9. Database changes

Migrations `0008`, `0009`, `0012`, `0013`: a random `sync_id` and a `stamp` on every shared row (`categories`,
`budgets`, `recurring_transactions`, `transactions`, `people`, `ledger_entries`, `documents`, `document_links`, the
shared part of `settings`); `sync_deleted` markers made by delete triggers; `categories.merged_into`; `sync_clock`;
`sync_peers`; `currency_changes`. Stamps come from triggers, which stand aside while `sync_clock.applying` is on. A
bill's parts and a recurring item's template travel inside their parent's row. Never synced: `audit_log`,
`backup_schedule`, `api_tokens`, `sync_peers`, `sync_clock`, the language, lock settings.

## 10. Stamps and the clock

`{ms:013}-{counter:04x}-{first 8 hex of the device id}`; larger is newer. A new stamp uses `max(wall clock, last ms)`
and counts up within a millisecond; receiving rows moves the clock past everything seen, so a slow phone clock still
makes newer stamps.

## 11. Merge rules

The larger stamp wins the whole row; a deletion marker is a row. Duplicates are matched by meaning (the same
`client_uuid`, the same payment of a recurring item, the same budget key, the same category or friend name): the newer
or lower `sync_id` stays, the other becomes a marker or is merged with `merged_into` and its rows move. A recurring
item's next due date moves past its latest payment. A bill's parts are replaced with their bill. Stamps these rules
make are derived from the stamps involved (`derive`, `moved`), never the clock, so both devices agree. Only the desktop
changes the currency; the lower generation converts all its rows first, and an out-of-range result stops the sync.
Documents: `documents-design.md`, section 10.

## 12. Applying changes safely

Rows from the other device are untrusted. Only synced tables and known fields; the same checks as the screens and the
API; bound parameters only; a fixed order (settings, categories, friends, recurring, budgets, transactions, ledger,
documents, links, markers); unknown references stop the sync; `foreign_key_check` and `integrity_check` before commit.
Nothing from a sync is logged.

## 13. The phone

- Passphrase vault (Argon2), no OS keychain; 5 wrong passphrases erase the data, keys and PS (warnings from the 3rd);
  pair again to get a full copy back. No recovery key, no remote wipe.
- A backup restore on either device gives a new device id and removes the pairings.
- Android (`scripts/android-patch.py`): `allowBackup="false"` and `FLAG_SECURE`. iOS: data folder kept out of iCloud
  backups at every start; screenshots cannot be blocked; documents are previewed only.
- Desktop screens live in settings, phone: paired phones, add, sync, the QR with countdown, the six digits, the summary.

## 14. Threats and what stops them

Eavesdropping, tampering, replay and reflection: sealed, numbered, direction-bound frames and a burned OTS. A fake
desktop or phone cannot make M2 or M1. Port scans get no answer, and floods close the listener. A lost phone has the
vault, the erase, and removal on the desktop. A bad row from a paired device: section 12. Not protected: malware on an
unlocked device, the metadata (that a sync happened, when, roughly how much), a wasted sync window.

## 15. Open points

Per-month hashes if data grows a lot (ten years, 60,000 transactions: a first copy is 28 MB in 1.6 s, an idle sync
1.5 KB). IPv6 (the QR allows it). Whether markers can ever be dropped. Android 17's local network permission.

## 16. Tests

Fixed test vectors (`scripts/sync-vectors.py`, checked against RFC 8439 and 5869; held in `sync/crypto.rs`,
`sync/qr.rs`); hostile and damaged handshakes and frames; two in-memory devices over a real socket with random edits
from fixed seeds (`MEOWNY_SYNC_SEEDS`), every merge rule, and syncs cut at every message; currency changes; tables that
must never be sent. Tried by hand on a real Android phone (2026-10-16 and 2026-10-23).
