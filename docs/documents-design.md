# Documents design (phase 9)

Status: built (batches 1 to 11). Decisions come from phase 9 of [PLAN.md](PLAN.md); this file says exactly how they work, so code can follow it and a reviewer can check it. Section 16 lists open points; the real-phone check (section 15) settles part of them.

## 1. What it has to do

- Keep the paper trail next to the numbers: a receipt, invoice, contract, warranty. A document is a file stored encrypted on the device, with a name, a date and links to the transactions and recurring items it belongs to.
- It exists on its own and can link to any number of transactions and recurring items. Deleting a transaction never deletes a document.
- The desktop holds every file. The phone holds only files marked "keep on phone" (off when added) and ones it added itself, until the desktop has them.
- Both devices can add, link, rename and delete. The phone adds from the camera or a file.
- Every file is untrusted: nothing is run, nothing is shown with scripts.
- No new Rust dependency: only `chacha20poly1305`, `hkdf`, `sha2`, `zeroize`, `getrandom` and `std`.

Not goals: editing a file, versions, OCR, thumbnails, folders or tags, sharing with another person, a total size limit for the library.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Document row | Name, date, type, size, SHA-256, "keep on phone" | Encrypted database (`documents`) |
| Link row | One document linked to one transaction or recurring item | Encrypted database (`document_links`) |
| Sealed file | The contents, encrypted in chunks | `documents/` next to the database, one file per document |
| Document key | Derived from the database key, in memory only while unlocked | Vault state, dropped by a lock |
| Open copy | A plain copy for the system's viewer | `open/` next to the database, removed on lock and close |

Files are not blobs in the database: ten years of receipts would make it large and slow; a rows-only backup stays small.

## 3. Keys and names

```
docs_root  = HKDF-SHA256(salt = none, ikm = database key, info = "meowny documents v1")          32 bytes
file_key   = HKDF-SHA256(salt = file salt (16, in the file header), ikm = docs_root,
                         info = "meowny doc file v1" || sync_id (16 bytes))                         32 bytes
file name  = hex(HKDF-Expand(docs_root, info = "meowny doc name v1" || sync_id), 16 bytes) + ".mnd"
```

- `docs_root` is made when the vault opens (`unlock`, `create`, `recover`) and lives in vault state next to the connection as a `Zeroizing` value. Locking drops it, so a locked app cannot read a file.
- A new random salt per file gives every file its own key, so the counter nonces of section 4 never repeat under a key.
- The document's sync id is in the key, so a sealed file moved onto another document's name does not open.
- The name is derived, not stored: no column can fall out of step, and the folder shows only random-looking names (still how many files and about how big each; written in the data inventory).
- The recovery key and a changed passphrase wrap the same database key, so `docs_root` does not change and no file is resealed. A backup restored into a new vault (new database key) reseals files as it reads them (section 11).

## 4. The sealed file

```
file    = header || chunk_0 || chunk_1 || ... || chunk_n
header  = "MEOWNYDF" (8) || format u16 = 1 (2) || chunk size u32 = 65536 (4) || salt (16)        30 bytes
chunk_i = ChaCha20-Poly1305(file_key, nonce_i, aad = header, plaintext_i)                         16 bytes more than the plaintext
nonce_i = last (1: 0x01 on the final chunk, else 0x00) || 0 0 0 || i (u64, big-endian)           12 bytes
```

STREAM layout. Every chunk but the last holds exactly 65,536 bytes; the last holds 1 to 65,536 (an empty file is one last chunk of 0 bytes). The chunk count comes from the file's length on disk, so the reader knows which chunk is last.

| Damage | Why it is caught |
|---|---|
| A changed byte anywhere | That chunk's tag fails (header is the aad, so a changed header fails every chunk) |
| A chunk cut off the end | The new last chunk was sealed without the flag |
| A chunk added at the end | The old last chunk was sealed with the flag |
| Chunks swapped or one dropped | The counter is in the nonce |
| Another document's file under this name | The sync id is in the key |

- A file whose last chunk is empty but which has other chunks is refused (the sealer never writes one).
- The reader checks plaintext size against the row (`size_bytes`) and SHA-256 against the row (`sha256`) before handing anything over. A failure is "damaged"; nothing is shown or sent.
- **Writing** is atomic: sealed bytes go to `name.tmp` in the same folder, are flushed, renamed over the final name (folder flushed on Unix). Files are owner-only. A file arriving from the other device is built as `name.part` (a valid prefix: all chunks sealed as non-final) and renamed only after the hash matches, which makes a transfer resumable (section 9).
- **Limits.** 25 MiB per file (`MAX_DOCUMENT_BYTES`, 400 chunks); a bigger one is refused with its own message. Rows with size 0 or over the limit are refused when they arrive from another device. A person adding an empty file is refused (nearly always a mistake), but the format handles it so a test can cover it.

## 5. The rows

Migration `0010_documents.sql`. Dates are `YYYY-MM-DD` text, stamps are the sync design's text, ids that cross devices are `sync_id`s.

```
documents (
  id, sync_id, stamp,
  name           text   1..120 characters, cleaned (section 6)
  media_type     text   one of a closed list (below)
  size_bytes     int    1..26214400
  sha256         text   64 lowercase hex
  document_date  text   a real date, the day of the paper (default: today in the person's time zone)
  keep_on_phone  int    0 or 1, default 0
  created_at, updated_at
)

document_links (
  id, sync_id, stamp,
  document_id               -> documents      ON DELETE CASCADE
  transaction_id            -> transactions   ON DELETE CASCADE, may be empty
  recurring_transaction_id  -> recurring_transactions  ON DELETE CASCADE, may be empty
  created_at
  exactly one of the two targets is set (CHECK)
  unique (document_id, transaction_id) and unique (document_id, recurring_transaction_id), each where set
)
```

- **Immutable contents.** `media_type`, `size_bytes`, `sha256` never change. To change a file, delete and add another. So one `sync_id` always means the same bytes on every device.
- **Media types** (closed list, decided from the first bytes, not the name): `image/jpeg`, `image/png`, `image/gif`, `image/webp`, `application/pdf`, `text/plain`, `text/html`, `text/csv`, `application/octet-stream` (everything else). Another value is refused.
- **Real foreign keys**, not a kind and an id: the database keeps links consistent. Phase 10 added a third target (a ledger entry) by rebuilding the table.
- **Audit** records add, rename, delete of a document and add, remove of a link, with the name and never the contents.
- `sync_deleted.kind` gets `documents` and `document_links`; the migration rebuilds `sync_deleted` (SQLite needs that to change a CHECK) and copies rows. `backup_schedule` gets `include_documents` (0 or 1, default 0). Backups carry both tables (section 11).
- Stamp triggers, the `applying` switch and delete markers follow `0008` and `0009`, with two differences: `sync_id` is a column default (`lower(hex(randomblob(16)))`, `NOT NULL`), and the `documents` update trigger fires only when `name`, `document_date` or `keep_on_phone` really changes.
- **Immutable by trigger.** A `BEFORE UPDATE` trigger refuses changes to `sync_id`, `media_type`, `size_bytes`, `sha256` of a document and to any column of a link (a hostile row would first put other contents under a known id). Setting a column to its current value is allowed.
- **A link marker means a person unlinked it, nothing else** (migration `0011`; `0010` made one for every link that left, which was wrong). A link removed by the foreign key because its parent was deleted needs none: the parent's marker says it. SQLite deletes the parent before it cascades, so the trigger tells the two apart by whether the parent is still there. `0011` also gave the `documents` `updated_at` trigger the same "no-op write is no edit" condition (found by the random sync test).

**Does this device hold the file?** Look at the folder: the name is derived, so "does `documents/<name>.mnd` exist" is the answer; no column. The page reads the folder once per list. A row with no file shows one of:

| State | Where | Meaning |
|---|---|---|
| Not on the phone | phone, switch off | Normal; the file is on the computer |
| Waiting for sync | phone with switch on, or either device after a stopped transfer | The next sync brings it |
| Missing | desktop | The folder lost it (restore without documents, deleted file). The phone may hold it; the next sync takes it back (section 9) |

## 6. Names and types: files are untrusted

- **Names** are cleaned on add or rename.
  - From a file (`clean_file_name`): the part after the last `/` or `\`; control and hidden characters removed (direction marks and overrides, zero-width, soft hyphen, Arabic letter mark); all white space becomes one plain space, runs folded; over 120 characters is cut keeping a short extension; empty or only dots becomes `document`.
  - From typing (`clean_title`): `/` or `\` becomes a hyphen; an empty result is refused.
  - A name in a row from another device must already be clean (`is_clean_name`) or the row is refused.
- **A copy on disk** (saved or open) also goes through `disk_name()`: Windows-forbidden characters (`<>:"|?*`) become `_`, trailing dots and spaces go, reserved names (`CON`, `NUL`, `COM1`, ...) get a leading underscore, kept short. The database keeps the typed name.
- **Type** comes from the first bytes (`%PDF-`, PNG/JPEG/GIF/WebP signatures, valid UTF-8 text with no NUL), with the extension used only to tell text from CSV and HTML. A `.jpg` that is not an image is `text/plain` if text, else `application/octet-stream`; never drawn. An image signature also needs a header giving width and height, or it is `application/octet-stream`.
- **Nothing is ever run.** The app does not execute, script-render, or follow anything in a file.
- **Images in the app.** Only the four image types, after checks: size from the header (width x height at most 64 megapixels) and the real signature; also 10 MiB or less (the page gets it as text, a third larger). Bytes go as base64url text; the page makes a `data:` address and draws an `<img>`, never SVG, never a frame, never a link. The content policy already allows `data:` images (a `Blob` would have needed `blob:`). Bigger images open in the system viewer like a PDF.
- **Everything else with a kind** (PDF, HTML, text, CSV, too-big images) is written to `open/` and handed to the system viewer (`tauri-plugin-opener`, called from Rust so the page cannot name a path).
  - **The copy's extension comes from the kind the app found, never the name** (`open_name`: `invoice.pdf.exe` found to be a PDF opens as `invoice.pdf.pdf`; `holiday.bat` found to be a PNG as `holiday.png`). Otherwise "open" would run a program, since the extension is the one thing the sender chose.
  - `application/octet-stream` is **not opened from the app at all**; it can only be saved as a copy.
  - Before the first open of a session the screen says the copy is not encrypted while open and that an HTML file may load things from the internet.
- **The open folder** `open/`: owner-only, one sub-folder with a random name per open, file under its `open_name()`. Emptied on lock (idle, lock button, minimise or close), on exit, on start (a crash leaves files), and on "delete all my data". On Windows a viewer holding the file blocks removal; it goes at the next start and the screen advises closing the viewer first. A second open is a new copy.
- **Saving a copy** (desktop only; the phone has no file dialogs) uses `commands::ask` like exports, owner-only, never over an existing file, with the export warning that it is not encrypted.

## 7. Adding, linking, deleting

**Adding** (same Rust function on desktop and phone):

1. The page picks a file with a plain file field (phone: `capture` for the camera; section 15 checks the web view) and reads it with `File.arrayBuffer()`, sending it to Rust **in pieces of text**:
   - `document_upload_begin` (name, date, links, byte count), then `document_upload_chunk` (base64url, 256 KiB of file each, at most 512 KiB of text, each starting where the last ended), then `document_upload_finish`.
   - Raw bytes would be simpler, but Tauri does not support them on Android, where photos come from.
   - Pieces are gathered **in memory only**; a half-sent file is never written in the clear. One upload at a time; a new one drops the old; a lock drops it.
   - A bad description (date, link, size) is refused at `begin`; an out-of-order piece or one past the announced size ends the upload. Over 25 MiB is refused in the page and again in Rust.
   - No file dialog plugin, no path ever comes from the page.
   - **Dropping files on the window** (desktop) is the one other way in, handled by the window, not the page: on Linux WebKitGTK gives a page an empty list for a file-manager drop. `dragDropEnabled` stays on; `documents/dropped.rs` keeps the dropped paths in the vault (at most 100 per drop, replaced by the next, forgotten on lock, never kept while locked), emits `documents-drag` (true/false) and `documents-dropped` (count), and the page calls `document_add_dropped` (no argument). Rust reads exactly those files (regular, 1 byte to 25 MiB; a folder, empty, gone or too large file is reported by name and the rest go on) and adds each with the library function, named after the file, dated today, attached to nothing. A page asking for a path it was not given gets nothing.
2. Rust cleans the name, decides the type, hashes, and inside one `with_conn` call writes the sealed file and inserts the row (and links). Nothing can look at the folder in between.
3. If a document with the same SHA-256 exists, nothing is stored: the answer says "already in the library" and the asked links go to the existing one. If this device lost that file, adding it again restores it. A duplicate made on two devices while apart is kept as two documents.

- If the file is written and the row insert fails, the file is removed. If the process dies between, the section 8 check removes the stray file.
- **Linking** adds a `document_links` row; an existing link is not an error. **Unlinking** deletes it and leaves a marker. Deleting a transaction or recurring item removes its links by foreign key; a document is never touched.
- **Deleting a document** asks for a confirmation naming it and its link count. The row goes first inside the transaction, the file after commit (a file without a row is cleaned by the check; a row without a file is the worse state and avoided).
- **The attach area** (transaction and recurring dialogs) lists linked documents (open, unlink), has "add file" (and "take photo" on the phone) and "link an existing document" (a search over the library). In a dialog that creates a transaction, files picked before saving are held in the page and added and linked after the transaction exists; if one fails, the transaction stays and the area names the file.
- **Errors** (`AppError`, three fixed codes, text in seven languages): `too_large` (API 413), `file_missing` (API 404), `file_damaged` (API 500 `internal`). A refused add leaves no row, link or file.

## 8. Rows and files that disagree

A check runs after every unlock, restore and sync:

- A file in `documents/` whose name no row derives is removed. `.tmp` files are removed; `.part` files are kept when their row exists, else removed.
- A row whose file has a length its size cannot give is reported damaged and left. The check reads no file (lengths only), so it stays fast; opening a file proves the rest.
- Only ours is touched: names of 32 lowercase hex characters with `.mnd`, `.tmp`, `.part`. Anything else (another name, a folder, a link) is left alone and counted.
- A row whose file is missing is counted, not repaired; the page says how many. On the phone only rows with the switch on count.
- Nothing logs a name or hash.

## 9. Sync

Rows travel as any rows (section 10). The bytes travel after the rows, in the same sealed connection (`src-tauri/src/sync/files.rs`). Protocol 2 became 3.

**Who gets what.**

| Direction | Which files |
|---|---|
| Phone to desktop | Every file the desktop lacks and the phone holds (the desktop is the archive; a phone-added document, or a desktop restored without documents, is filled from the phone) |
| Desktop to phone | Only files whose `keep_on_phone` is 1 after the rows merged and that the phone lacks |

- The sender enforces the rule: a request for a file whose switch is off, a document it does not know, a start beyond the end, or a file that no longer opens or matches its row is answered with a `file` message saying `skipped`. (Built as `skipped`, not `error`: an `error` ends the whole sync.)
- A one-way sync carries files only that way; a request against the direction ends the sync as a bad message.

**The phase**, after `done` (rows applied, lists compared or equal), desktop first. It also runs when no table differed.

1. `have`: each side lists `sync_id`s of documents whose complete file it holds (400 per message, last marked; empty list is one empty message).
2. `need`: each side lists `[sync_id, offset]` it wants: a document it has a row for, lacks a complete file of, that the other `have`s, and the rules allow (desktop wants every such file; phone only those with the switch on). `offset` is where a stopped transfer left off (a multiple of 65,536 from the valid prefix of `name.part`), else 0. Newest first. The desktop's `need` is sent before the phone makes its own.
3. For each wanted file, in order, the sender sends one `file` (`id`, `offset`, `size`, or `skipped`) then `chunk` messages (`data` base64url of up to 24 KiB, `last`). A chunk names nothing: order is the addressing. Desktop sends what the phone asked for, then the phone what the desktop asked for. Each device reseals under its own key.
4. The receiver seals each 64 KiB into `name.part`, hashing as it goes, and compares SHA-256 with the row at the end. Match: rename into place. Mismatch: remove `.part`, count as failed, go on. A `file` whose size or start is not what row and request say, a chunk that is not valid base64url, a file longer than its row, or a stray message ends the sync as a bad message. A skipped file, or one this device cannot write, counts as failed; asked again next sync.
5. `stored`: the desktop tells the phone which documents it holds whole among those the phone holds with switch off. A file that arrived a moment ago was checked on arrival; others are re-read and checked against the row. The phone drops a file when its switch is off **and** the desktop named it in a `stored` of this session. A sync only to the desktop sends no `stored` and takes nothing off the phone.
6. The summary gains "documents received", "sent", "could not be moved", "waiting for the next sync", "taken off this phone".

- **Budget.** A session ends after 256 MiB in all. Files may use 100 MiB of file bytes, both directions together: the desktop's `need` gets up to that, the phone gets what is left, each sender also stops there and answers `skipped` for what would go over. A file is asked for only if the rest of it fits; what does not fit waits (a smaller, older one after it still goes); the next sync resumes from the valid prefix.
- **A broken connection** (timeout, closed socket, the other device ending with an error) in this phase is not a failed sync: rows are in, the summary notes the break, the rest waits. Anything against the rules ends the sync. A lock mid-way stops with "locked".
- The open folder and temporary files never cross.

## 10. Merge rules for the rows

Rows follow the sync design (newer stamp wins as a whole row, a deletion is a change, derived stamps come from stamps, never the clock). New cases:

| Case | Rule |
|---|---|
| Document edited on both (name, date, switch) | Newer whole row wins. A real conflict goes to the review dialog (name, date, "keep on phone" shown) |
| Edited on one, deleted on the other | Newer wins (a marker has a stamp). If the edit wins, the document returns with the links the editing device holds. A deleted document removes its file on every device that applies the marker |
| Same link added on both | Matched by `(document, target)`: the newer stays (tie: lower id), the other becomes a marker. Both devices write the same marker (`loser_marker`) |
| A person unlinked it on one device | A marker newer than the link: the other deletes its copy |
| Parent (document, transaction, recurring item) deleted on one device while the other attaches a document | The parent's marker takes the link with it (foreign key, silently). A link row arriving for a parent known as deleted is **not taken and not an error**; the other device gets the parent's deletion in the same sync and drops the link. One sync ends them the same, with no link marker on either |
| Unlinked and linked again | Different random ids; the new link is untouched by the old marker |
| A document row with other size, kind or hash than held under that id | Refused; the sync stops with nothing applied |
| A link row with another document or target than held under that id | Refused the same way |
| Two documents with the same hash | Both kept (section 7) |

- Apply order: settings, categories, recurring items, budgets, transactions, **documents, document links**, then deletion markers (transactions, budgets, recurring items, **links, documents**). A link refers to document and target by `sync_id`; an unknown id that is not a known deletion stops the sync. Files of a deleted document are removed after commit (section 8 covers a crash between).
- Row checks on arrival: name rule of section 6, closed media type, size 1 to 26,214,400, 64 hex hash, a real date, switch 0 or 1, a link with exactly one target. A failing row ends the sync naming the table, never the content.
- `MAX_ROWS_PER_TABLE`, the 400-entry digest messages, table hashes and the conflict limit of 200 work unchanged.

## 11. Backup, restore and "Delete all my data"

**Backup.** "Include documents" is a choice in the backup dialog (the dialog shows the library size; on by default under 100 MB). Scheduled backups keep the choice in `backup_schedule`; the newest 10 are kept, and the dialog shows the space. A backup stays one encrypted file.

Format 1 (a single sealed JSON blob in memory) cannot carry gigabytes. Format 2 keeps the header and Argon2id stretching and replaces the single seal with a STREAM of 64 KiB chunks as in section 4. The plaintext is a list of records (one-byte tag, a length):

```
S  the snapshot (the JSON of today's backup)           once, first
F  a file: sync_id (16) || size (8) || sha256 (32) || the bytes     0 to many
E  the end                                              once, last
```

- A backup without documents has only `S` and `E`. Version 1 files still open.
- Writing streams to a temporary file next to the target and renames; reading streams, never the whole file in memory. The 512 MiB ceiling becomes per record (snapshot at most 512 MiB as before, a file at most 25 MiB). Truncation is caught by the last-chunk flag.

**Restore** (running vault or first run): the snapshot is checked first in the scratch database, then:

1. Each `F` record is hashed as read and sealed under the target vault's `docs_root` into `documents/`, named from its `sync_id` (rewriting an existing file is harmless: same id, same bytes).
2. The database is replaced in one transaction.
3. The section 8 check removes files no row names.

- Crash before step 2: extra files with no row (removed), old data intact. Crash after: old files with no row (removed).
- A backup without documents restores rows; documents show "missing" until the phone returns files it holds. A restore turns on `applying`, gives the device a new device id and no pairings.

**As built** (batch 8):

- Header is the old 54 bytes with format 2; its nonce field is random filler (each chunk has its own nonce: first byte is the last-chunk flag, last eight the chunk number, as in section 4); the whole header is authenticated with every chunk.
- The writer holds one chunk back until it knows whether more follows (an unreadable file is left out, so the total is unknown). The reader knows the last chunk from the file length.
- A wrong passphrase fails the first chunk (indistinguishable from a damaged start: `WrongCredentials`); a later failure is damage (`InvalidBackup`). An empty chunk, a short non-last chunk, and anything after the end record are refused.
- Rows (`S`) are held in memory (at most 512 MiB, read as they arrive so a lying length cannot ask for memory); version 1 still read whole with its ceiling; version 2 has none.
- *Writing:* the "include documents" box is in the manual dialog and the schedule form (kept in `backup_schedule.include_documents`), showing files and bytes held, on under 100 MB; the schedule form shows what the newest 10 can take (ten times the library). A manual backup asks where first, then reads rows. Files are read one at a time, each under a short vault hold, so the app stays usable and a lock mid-way ends the backup with nothing left (the temporary file is removed; an older file under the name is untouched). A document with no file here, or one not matching its row, is left out and counted, and the dialog says how many. The phone makes no backups. The CLI's `backup.create` still writes rows only; its approval says so.
- *Restoring:* rows are checked in the scratch database; `F` records are read and sealed under the target vault's key; only when the stream ended whole are rows replaced in one transaction; then the folder check runs whatever the outcome. A file is taken only if a row of the checked snapshot names it with the same size and hash, and only once (a hostile backup cannot fill the disk); every file is written from its first byte, never on a stopped transfer's leftovers; a failing file (cut stream, wrong hash) is removed at once; the vault holds its state for the whole restore. A new install whose restore fails removes what it made and is left not set up.

**Delete all my data** and the phone's erase after 5 wrong passphrases remove `documents/` and `open/` in `remove_everything`, the one place that already removes the database. Without the key they could not be read anyway; removal is for the disk.

## 12. Export

JSON, CSV and Excel list the documents (name, date, type, size, hash, names of what they link to) and never contain a file; the PDF report is unchanged. A limited export lists only documents linked to what it contains. The export page notes that files are not in an export and points at the backup (the screen note was later removed at the maintainer's request).

**As built** (batch 9). Export functions take only the database, never the folder or keys, so a file cannot get into one.

- *JSON:* a `documents` list (id as string, `name`, `date`, `type`, `sizeBytes`, `sha256`, `links`), oldest paper first; each link has `kind` (`transaction` or `recurring`), `id`, `name`, `date` (payment date; none for a recurring item). File `version` stays 1.
- *CSV:* a last column `documents` with the names attached to that transaction, in name order, separated by `; ` (a name with `; ` itself is ambiguous in one cell; JSON lists them singly). A document attached to nothing, or only to a recurring item, is not in a CSV.
- *Excel:* a sixth sheet `Documents` (name, a real date, type, size in bytes, hash, "Attached to": `Name (date)` for a transaction, `Name (recurring)` for a recurring item, joined by `; `) and a `Documents` column on the Transactions sheet. Every name goes through `neutralise`.
- *What a selection keeps:* no limits lists every document, attached or not. Any limit lists a document only if attached to a transaction in the export (date and category) or a recurring item that is (category only; dates do not limit recurring), and only those attachments, so a name hanging on something left out is not given away.
- *Screens:* the receipt has a documents line for JSON and Excel (a note for the CSV column); the preview counts what the JSON would list.
- *Command line:* the approval dialog says how many documents are listed (or that a CSV column names them) and that files are never in it.

## 13. CLI and API

The command API stays on loopback; every route needs a token.

| Route | Token | What |
|---|---|---|
| `documents list`, `documents show` | read | Names, dates, types, sizes, hashes, links. Never contents |
| `documents add <path>` | write | The CLI reads the file itself (as the person) and sends the bytes in one request body. The app never opens a path for a token. A bigger body limit (1 MiB today) for this one route: 25 MiB plus a little, still with `Content-Length`, read in pieces |
| `documents link`, `unlink`, `rename`, `delete`, `keep` | write | Same rules as the screens |
| `documents get <id> --out <path>` | write | A dialog (token name, document, size, exact file); written only on Allow, new file only, owner-only, through the module exports use. No answer in two minutes is a no |

The schema (`meowny schema`) and `docs/CLI.md` get the new commands in the same change.

**As built** (batch 10). Eight routes in `src-tauri/src/api/documents.rs`: `documents.list`, `documents.show` (read); `documents.add`, `documents.update` (name, date, phone switch together, at least one), `documents.delete`, `documents.link`, `documents.unlink`, `documents.get` (write; `get` also needs the person's yes). `rename` and `keep` are one `update`. Answers are flat records: `show` is the document with `links`, `add` is the document and `alreadyThere`.

- *Upload.* The file is the argument `data` of `documents.add`: base64url without padding, a new argument kind (`bytes`) only this route has. The CLI reads the file (`meowny documents add <file>`, 1 byte to 25 MiB, regular file), keeps the file's name unless `--name`, and encodes with its own small encoder (no dependency on the app). That route's body may be 35 MiB; every other route takes 1 MiB.
- **How the bigger body is let in:** after the request head and before a byte of the body, `admit_large` makes the first checks `handle` makes (host, no origin, POST, a route that takes a big body and not bigger than allowed, a known bearer token, a scope that allows the route). A would-be-refused caller is refused from the head alone, so 30 MiB is never read for a request with no token (tests send only the head). A locked app cannot check a token, so it lets no big body in. Over 1 MiB for any other route is 413.
- *Getting a file out.* `documents.get` needs a write token **and** the person's yes (a read token is refused before anything is asked). The dialog names the token, the document (name, kind, size, date), the exact file, and says the copy is not encrypted. The copy's name must end in the extension of the kind the app found (`.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, `.bin` for unknown), never one the caller picks (a name like `start.desktop` or `run.sh` would be a way to make a system run something). The file is created new, owner-only, in one step with the not-there check (`files::write_new`). A document with no file here answers `not_found` before asking; one whose file no longer opens is asked about, answers `internal` after the yes, writes nothing.
- *Errors:* existing codes only (`invalid_input`, `not_found`, `too_large`, `file_exists`, `forbidden`, `denied`, `locked`, `internal`).
- *Docs:* `cli/schema.json`, `docs/site/examples.json` and `docs/CLI.md` regenerated, with examples for add, list, show, change, attach, get, delete and the two common failures (an unreadable file, a name for a copy a system could run).

## 14. The screens

- **Documents page** (sidebar, `g o`; phone: an icon in the top bar): newest paper first, 50 per page, search by name, type choice (all, images, pdf files, other files; no date filter), total size (phone: what it holds), count of documents with no file, "add file" (phone: "take photo"), drag and drop on the computer. Missing files show a warning (phone: "waiting for the next sync", only for documents it was asked to keep), and damaged ones. A card has **details**, **save** (a copy; not on a phone) and **delete** (names the file).
- **Details** is one dialog: the picture if drawable, name, date (the day on the paper), "keep a copy on my phone" (saved with the rest; nothing is asked of the app when nothing changed), **about this file** (kind, size, where the file is when not here), **linked to** (each can be detached), and **open**. The first open of a session explains the plain copy; a file of unknown kind has no open button and says so. On a phone the switch and delete are inside the document.
- **The attach area** is in the transaction and recurring dialogs and, short, in the phone's quick add. On an existing item a file is stored and linked at once; on one being made, files wait and are sent after saving; if one fails the item is still saved and the notice names the file and reason. "Attach one from the library" is a search with an attach button per result.
- **The paper clip** with a number marks transactions, recurring items and debt entries that have documents. Counts come from one question per list page (`document_counts`), asked again after a dialog closes.
- All text in seven languages (48 texts), counts as "label: number" (no plural forms). Mobile-first: 44 px targets, 16 px inputs, safe-area insets.

**Preview** (2026-10-17, at the maintainer's request). A **preview** button on each card, before **details**, and in the details dialog, shows an image, PDF, text, CSV, Word or Excel file in a wide dialog.

- *One library, only for PDF.* No single library draws PDF, Word and Excel without sending the file to an internet viewer (rejected: it would upload papers). So **one new dependency, `pdfjs-dist`** (Apache-2.0, the `legacy` build), loaded only when a PDF is shown (490 kB chunk, 1.3 MB worker). Word and Excel are read by about 700 lines of the app's own code in `ui/src/lib/preview/`: `zip.ts` (directory at the end of the zip; stored and deflated parts through the web view's `DecompressionStream`), `xml.ts`, `docx.ts`, `xlsx.ts`, `csv.ts`.
- *Made for hostile files.* The zip reader never writes anything and finds parts by fixed name, never a path from the file. A part stops at 16 MiB when it comes out whatever the directory claims; 40 MiB for all parts of one file; 20,000 entries at most; no zip64, no encryption. The XML reader refuses any document type (no entity expansion), reads only the five named entities and number references, no namespaces, at most 2 million elements and depth 100. Text is cleaned of control, direction and zero-width characters and drawn as React text, never markup.
- *Rust gives bytes and kind.* `read_document_preview` (in `COMMANDS`, `build.rs`, the capability and the handler) answers `{ kind, mediaType, data }` with base64url `data`. Kind: `image`, `pdf`, `text`, `csv`, `docx`, `xlsx`: from the kind the app found, and for `docx`/`xlsx` from the name (their kind is "other", never opened) with a check that the bytes begin as a zip. HTML is never given. At most 16 MiB (images 10 MiB and 64 megapixels as `read_document_image`); else `invalid_input`. Contents are checked against the row like every read.
- *Content policy unchanged.* pdf.js runs from the app's files (`script-src 'self'`; worker served with a JavaScript type), no `eval`, given bytes and no address, `useWasm: false` (no `wasm-unsafe-eval`). A JPEG 2000 picture in a PDF is not drawn. Fonts a PDF carries load from memory; others are the system's. Driven in Firefox with the production policy as a header: the PDF draws, and the policy reports nothing from pdf.js (only `eval` once from `zod`, as before).
- *Look.* PDF: one canvas fitted to the dialog, buttons and page number under it, wheel turns pages (`PdfView`; wheel events less than 120 ms apart are one gesture). Word: a white sheet (`.paper`). Excel and CSV: a sheet with lettered columns, numbered rows and tabs under the grid (`.sheet-grid`, `.sheet-tabs`). Paper and grid are light on purpose.
- *Links, previews on items, paying with a document* (2026-10-17). The number on a card opens `DocumentLinksModal`; a choice goes to the item's page with `navigate(path, { state: { focus } })`. A transaction is `/app/transactions` (its month, filters cleared, 100 per page), a recurring item `/app/recurring`, a debt entry `/app/debts/<friend>`; so `LinkView` got `personId` (ledger entries only; left out of the others, so the API's `documents.show` is unchanged for them). Rows carry `data-item="kind:id"`; `useFocusRequest` reads and clears the request and `useJumpTo` scrolls to the row and outlines it for 2.5 s, giving up after 4 s. The paper clip and number on an item is `ClipPreview` (a `ClipMark` after the date or repeat text): pressing it lists the item's documents (always, even for one) with a preview button each and the name opening details. `AttachArea` has a preview button on each stored document. The recurring pay dialog uses `useAttachments` in waiting mode and, after `pay_recurring` answers with the new transaction, `commit`s the waiting files and chosen documents to it; a file that cannot attach is named and the payment stays.
- *Limits, said on screen.* Word: text, headings, list items (bullets), tables; not pictures, text boxes, headers, footers, notes, comments or look. Excel: the values the sheet last had (a formula's result), dates by date format; not colours, currency signs, merged cells, charts, hidden sheets; 1,000 rows and 40 columns per sheet.
- *Not seen yet:* the Android System WebView and WebKitGTK with pdf.js (both support module workers and `DecompressionStream`). Without `DecompressionStream`, Word and Excel show "could not be read", not a broken page.

## 15. The spike on a real phone

Before the screens, one small build on the Redmi Note 11, to settle what the web view can do without a plugin:

1. A file field with `accept="image/*,application/pdf"`: does Android's chooser open, and can the page read the chosen file with `arrayBuffer()`?
2. The same field with `capture="environment"`: does the camera open, does it need the `CAMERA` permission from the app (the barcode scanner plugin may already bring it; not checked), and is the photo readable?
3. Do pieces of 340 KiB of text pass through a command on Android without trouble, and how long does a 25 MiB file take to go in? (Raw bytes are known not to work there, so they are not tried.)
4. Is an image of a few megabytes, given as text and drawn from a `data:` address, shown without delay?
5. Can the system open a PDF from the app's folder (the opener plugin on Android may need a `FileProvider` that the generated project does not have)?

**As of batch 11 the screens and commands that depend on the answers are built**, so the spike is now a check of the finished app on the phone rather than of a sample build. Run it with a debug or release APK of this branch (`scripts/android-patch.py` and the Android build of phase 8 are unchanged) on a phone paired with a desktop, and write down for each line what happened:

1. Documents (top bar, the page icon) → **add file**: does Android's chooser open, and does a chosen PDF show up in the list with the right size and kind? Then a 5 MB photo, and a file over 25 MiB (it should be refused in the page).
2. **Take photo**: does the camera open (watch for a `CAMERA` permission question; the barcode scanner already asked for it for pairing), and is the photo stored and drawn in details?
3. The size of the pieces: add the largest file you have under 25 MiB and note how long "adding" says it takes (the page sends 256 KiB pieces as text). If it is slow, say how slow.
4. **Details** of a photo of a few megabytes: is it shown without a long wait? (It is drawn from a `data:` address.)
5. **Open** a PDF that is kept on the phone: does the system offer a viewer? (The opener plugin may need a file provider that the generated project does not have.) If it fails, the page shows the error and the plan is "open this on your computer" for everything but images.
6. A sync with **keep on phone** on for one document and off for another: the first arrives, the second stays "on the computer"; add a file on the phone with the switch off and sync: it should reach the desktop and leave the phone.
7. Lock the phone's app with a file open in a viewer, and check that `open/` is empty afterwards (it cannot be looked at without a debug build; the page shows no sign of a leftover).

The answers go into this file. If 1 or 2 fail, a small plugin or a patch in `scripts/android-patch.py` takes their place, and that is a dependency decision to make with the maintainer. If 5 fails, the phone shows images in the app and says "open this on your computer" for the rest, which is acceptable for a first version.

## 16. Open points

- The real-phone check (section 15).
- ~~Backup dialog default.~~ On while the library this device holds is under 100 MB, off above; changeable (section 11).
- Thumbnails: none. A later one would be a second sealed file per image.
- Windows: removal of an open copy while a viewer holds it (section 6) is described, not seen.
- ~~Where the phone's documents page lives.~~ In the top bar, as an icon; to be looked at on the real phone.
- A total size limit or warning for the phone (it shows what it holds; it does not stop).
- A review by someone else of the sealed format and the file transfer before release, like the sync (this document is what to hand over).

## 17. Tests

- **Sealed format**: known answers from `scripts/doc-vectors.py` (a second implementation reusing the self-checked ChaCha20-Poly1305 and HKDF of `sync-vectors.py`) for keys, names and files of small sizes (empty, 1 byte, one chunk, one chunk and a byte, three chunks); a changed byte in the header or any chunk, cuts at every chunk boundary and inside, an appended chunk, swapped chunks, another file's chunk, a file under another document's name: all refused; the largest file; round trips for every size 0 to 3 chunks and some random; the key dropped on lock.
- **Names and types**: hostile names (path parts, control and direction characters, long, reserved Windows names, only dots) for what is stored and for `disk_name()`; media type from bytes against lying extensions; an image whose header claims a huge size is not drawn.
- **Rows**: constraints for both tables (one target, uniqueness, foreign keys, size and hash checks), triggers (stamps, markers, audit), a deleted transaction removes its links and no document, a deleted document removes its links and file.
- **Agreement**: a file without a row removed, a row without a file counted, `.tmp` and old `.part` cleaned, quiet when all agree.
- **Sync, rows**: every section 10 case has a test; random document and link edits on two devices from a fixed seed converge; a second sync finds nothing.
- **Sync, bytes**: a switched-on document crosses, a switched-off one never does (a marker in the file, searched in everything that crossed); the phone's new document reaches the desktop and leaves the phone only after confirmation; a transfer stopped at every chunk resumes; a changed byte, wrong hash, out-of-order chunk or unasked offset is refused; a request for a file the sender may not give is answered `skipped`; the byte budget leaves the rest waiting.
- **Backup**: with and without documents; restore into a vault with another key opens every file; a cut or changed backup is refused leaving the app as it was; a version 1 backup still opens; a simulated crash between section 11 steps is repaired by the check.
- **Wipe and lock**: after a lock `open/` is empty; after "delete all my data" neither folder exists.
- **API**: a read token sees no contents; an upload over 25 MiB is refused before it is read; `get` writes only after Allow and never over a file.
- **Screens**: attach area and documents page looked at in a browser at desktop and phone width with commands faked; the i18n key test.
- **By hand**: the real phone, with a photo and a PDF, as in section 15, then the whole "done when" of the plan.
