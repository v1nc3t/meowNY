# Documents design (phase 9)

Status: design written on 2026-10-16, nothing built yet. The decisions come from phase 9 of [PLAN.md](PLAN.md); this file says exactly how they work, so the code can follow it and a reviewer can check it. Section 16 lists what is still open, and the spike on a real phone (section 15) settles part of it.

## 1. What it has to do

- Keep the paper trail next to the numbers: a receipt, an invoice, a contract, a warranty. A document is a file, stored encrypted on the device, with a name, a date and links to the transactions and recurring items it belongs to.
- It exists on its own (a contract) and can be linked to any number of transactions and recurring items. Deleting a transaction never deletes a document.
- The desktop holds every file. The phone holds only the ones marked "keep on phone" (off when a document is added) and the ones it added itself, until the desktop has them.
- Both devices can add, link, rename and delete documents. The phone adds from the camera or a file.
- Every file is untrusted: nothing is run, nothing is shown with scripts.
- No new dependency. Everything below is built from the crates the app already has (`chacha20poly1305`, `hkdf`, `sha2`, `zeroize`, `getrandom`) and `std`.

Not goals: editing a file, versions of a file, OCR or reading the text of a receipt, thumbnails, folders or tags (the name, the date, the type and the links are the whole of the filing), sharing a document with another person, a total size limit for the library.

## 2. The pieces

| Name | What it is | Where it lives |
|---|---|---|
| Document row | Name, date, type, size, SHA-256 of the contents, "keep on phone" | Encrypted database (`documents`) |
| Link row | One document linked to one transaction or one recurring item | Encrypted database (`document_links`) |
| Sealed file | The contents, encrypted in chunks | `documents/` next to the database, one file per document |
| Document key | Derived from the database key, in memory only while unlocked | Vault state, dropped by a lock |
| Open copy | A plain copy of a file for the system's viewer | `open/` next to the database, removed on lock and close |

The files are not blobs in the database: ten years of receipts would make it large and slow, and a backup of the rows alone would stay small.

## 3. Keys and names

```
docs_root  = HKDF-SHA256(salt = none, ikm = database key, info = "meowny documents v1")          32 bytes
file_key   = HKDF-SHA256(salt = file salt (16, in the file header), ikm = docs_root,
                         info = "meowny doc file v1" || sync_id (16 bytes))                         32 bytes
file name  = hex(HKDF-Expand(docs_root, info = "meowny doc name v1" || sync_id), 16 bytes) + ".mnd"
```

- `docs_root` is made when the vault opens (`unlock`, `create`, `recover`, the three places that already hold the database key) and lives in the vault's state next to the connection, as a `Zeroizing` value. Locking drops it together with the connection, so a locked app cannot read a file. The database key itself is still not kept.
- A new random salt for every file means every file has its own key, so the counter nonces of section 4 can never repeat under one key.
- The sync id of the document is mixed into the key, so a sealed file moved onto another document's name does not open.
- The file name is derived, not stored: there is no column to fall out of step, and the folder shows only random-looking names. What it still shows is how many files there are and about how big each is. That is written in the data inventory.
- The recovery key and a changed passphrase wrap the same database key, so they do not change `docs_root` and no file needs sealing again. A backup restored into a new vault (new database key) seals the files again as it reads them (section 11).

## 4. The sealed file

```
file    = header || chunk_0 || chunk_1 || ... || chunk_n
header  = "MEOWNYDF" (8) || format u16 = 1 (2) || chunk size u32 = 65536 (4) || salt (16)        30 bytes
chunk_i = ChaCha20-Poly1305(file_key, nonce_i, aad = header, plaintext_i)                         16 bytes more than the plaintext
nonce_i = last (1: 0x01 on the final chunk, else 0x00) || 0 0 0 || i (u64, big-endian)           12 bytes
```

This is the usual STREAM layout. Every chunk except the last holds exactly 65,536 bytes; the last holds 1 to 65,536 (an empty file is one last chunk of 0 bytes). The number of chunks comes from the length of the file on disk, so the reader knows which chunk is last and opens it with the last flag:

| Damage | Why it is caught |
|---|---|
| A changed byte anywhere | The tag of that chunk fails (the header is the aad, so a changed header fails every chunk) |
| A chunk cut off the end | The new last chunk was sealed without the flag, so it does not open as the last one |
| A chunk added at the end | The old last chunk was sealed with the flag, so it does not open as a middle one |
| Chunks swapped or one dropped from the middle | The counter is in the nonce, so every later chunk fails |
| Another document's file under this name | The sync id is in the key |

A file whose last chunk is empty but which has other chunks is refused too: the sealer never writes one, so it can only be someone's attempt to extend a file. The reader also checks the plaintext size against the row (`size_bytes`) and the SHA-256 of what came out against the row (`sha256`) before it hands anything over. A file that fails any of these is reported as damaged and nothing from it is shown or sent.

**Writing** is atomic: the sealed bytes go to `name.tmp` in the same folder, are flushed to disk, and are renamed over the final name; on Unix the folder is flushed too. Files are created owner-only. A file arriving from the other device is first built as `name.part` (a valid prefix: its chunks are all sealed as non-final), and renamed only after the hash matches; that is what makes a transfer resumable (section 9).

**Limits.** 25 MiB per file (`MAX_DOCUMENT_BYTES`, 400 chunks). A bigger file is refused with its own message, before it is read when the size is known. Rows with a size of 0 or over the limit are refused when they arrive from the other device. An empty file is refused when a person adds one (it is nearly always a mistake), but the sealed format handles it, so a test can cover it.

## 5. The rows

Migration `0010_documents.sql`. Money is not involved. Dates are `YYYY-MM-DD` text, stamps are the text of the sync design, ids that cross devices are `sync_id`s.

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

- **Immutable contents.** `media_type`, `size_bytes` and `sha256` never change after the row is made. To change the file, delete the document and add another. So one `sync_id` always means the same bytes, on every device, and a hash in a row is a promise a receiver can check.
- **The media type is a closed list**, decided by looking at the first bytes of the file, not by trusting the name: `image/jpeg`, `image/png`, `image/gif`, `image/webp`, `application/pdf`, `text/plain`, `text/html`, `text/csv`, `application/octet-stream` (everything else). A row with another value is refused. The screens group them as images, PDFs and other.
- **Real foreign keys, not a kind and an id.** The database keeps links consistent by itself (deleting a transaction removes its links), and the constraint tests can see the rules. Phase 10 adds a third target (a ledger entry) by rebuilding the table, which SQLite needs for a changed CHECK anyway.
- **The audit history** records adding, renaming and deleting a document and adding and removing a link, with the name and never the contents, like every other table.
- `sync_deleted.kind` gets `documents` and `document_links`. Its CHECK has to change, which SQLite only does by making the table again, so the migration rebuilds `sync_deleted` and copies its rows.
- Backups carry both tables (section 11). `EXCLUDED_TABLES` does not change. The same migration adds `include_documents` (0 or 1, default 0) to `backup_schedule`.
- The stamp triggers, the `applying` switch and the delete markers are the ones of migrations `0008` and `0009`, made again for the two tables. Built differently from the older tables in two places: `sync_id` is a column default (`lower(hex(randomblob(16)))`, `NOT NULL`), so no trigger has to fill it in and a row without one cannot exist; and the update trigger of `documents` fires only when `name`, `document_date` or `keep_on_phone` really changes, so a write that gives a column the value it has makes no stamp. The `updated_at` trigger watches the content columns and not `updated_at` itself, and both stand aside while `applying` is on.
- **Immutable by trigger, not only by habit.** A `BEFORE UPDATE` trigger refuses any change to `sync_id`, `media_type`, `size_bytes` or `sha256` of a document, and to any column of a link: the first thing a damaged or hostile row from another device would do is put other contents under an id that already means something. Setting a column to the value it has is allowed.
- **A marker for a link means a person unlinked it, and nothing else** (migration `0011`; `0010` made one for every link that left, which turned out wrong). A link that the foreign key removes because its document, transaction or recurring item was deleted needs none: the parent's marker already says it, and a marker for a link that only this device knew would make the two devices disagree about a row the other one cannot know exists. SQLite deletes the parent before it cascades, so the trigger tells the two apart by whether the parent is still there. The `updated_at` trigger of `documents` had the same slip as the stamp one (it moved on a write that changed nothing, while the stamp did not) and got the same condition in `0011`; the random test of the sync found it.

**Finding out whether a device holds the file** is a look at the folder: the name is derived from the row, so "does `documents/<name>.mnd` exist" is the answer. There is no column for it. The documents page reads the folder once per list, not once per row. A row with no file shows one of three states:

| State | Where | Meaning |
|---|---|---|
| Not on the phone | phone, switch off | Normal. The file is on the computer |
| Waiting for sync | phone, switch on, or either device after a transfer stopped | The next sync brings it |
| Missing | desktop | The folder lost it (a restore of a backup without documents, a deleted file). The phone may still hold it, and the next sync takes it back (section 9) |

## 6. Names and types: files are untrusted

- **The name** is cleaned when a document is added or renamed. From a file (`clean_file_name`): the part after the last `/` or `\` only; control characters and the hidden ones (the direction marks and overrides the access-token names already refuse, plus zero-width characters, the soft hyphen and the Arabic letter mark) are removed; every kind of white space becomes one plain space, and runs are folded; a name over 120 characters is cut, keeping a short extension; a result that is empty or only dots becomes `document`. From a person typing a new name (`clean_title`): a `/` or `\` becomes a hyphen instead of cutting the name short, and a result with nothing in it is refused. A name that arrives in a row from the other device must already be clean (`is_clean_name`), or the row is refused, so two devices cannot end up with different names for one document. Every name in the database is plain text of one line.
- **A copy on disk** (a saved copy, or an open copy) gets one more step, `disk_name()`: the characters Windows forbids (`<>:"|?*`) become `_`, trailing dots and spaces go, a reserved Windows name (`CON`, `NUL`, `COM1`, ...) gets an underscore in front, and the name is kept short. The database keeps the name the person typed.
- **The type** comes from the first bytes (`%PDF-`, the PNG, JPEG, GIF and WebP signatures, valid UTF-8 text with no NUL bytes) with the extension used only to tell text from CSV and HTML. A file that says `.jpg` and is not an image is `text/plain` if it is text and `application/octet-stream` if not; it is never drawn. An image signature is not enough: the header must also give a width and a height, or the file is `application/octet-stream`.
- **Nothing is ever run.** The app does not execute, render with scripts, preview or follow anything in a file.
- **Images in the app.** Only the four image types, after two checks: the size from the header of the format (width times height at most 64 megapixels, which stops a small file that expands to gigabytes) and the real signature. The image also has to be 10 MiB or less (the page gets it as text, a third larger). The bytes go to the page as base64url text in the answer of the command, and the page makes a `data:` address of them and draws it with an `<img>`, never as an SVG, never in a frame, never with a link. The content policy already allows `data:` images, so nothing in it had to loosen (a `Blob` would have needed `blob:` added). A bigger or heavier image is opened by the system's viewer like a PDF.
- **Everything else that has a kind** (PDF, HTML, text, CSV, and an image that is too big to draw) is written to the open folder and handed to the system's own viewer (`tauri-plugin-opener`, called from Rust so the page cannot name a path). **The copy's extension comes from the kind the app found, never from the name** (`open_name`: `invoice.pdf.exe` found to be a PDF is opened as `invoice.pdf.pdf`, `holiday.bat` found to be a PNG as `holiday.png`). Otherwise "open" would run a program, since the system picks what to start by the extension and the extension is the one thing the sender chose. A file of the kind `application/octet-stream` is **not opened from the app at all**: it can only be saved as a copy, and what happens to it afterwards is the person's own doing. The screen says, before the first open of a session, that the copy is not encrypted while it is open and that an HTML file may load things from the internet when the system opens it.
- **The open folder** is `open/` in the data folder, owner-only, one sub-folder with a random name per open, holding the file under its `open_name()`. It is emptied when the app locks (idle lock, the lock button, lock on minimise or close), when it exits, when it starts (a crash leaves files behind), and on "delete all my data". On Windows a viewer that still holds the file open blocks its removal; the file is then removed at the next start, and the screen warns that closing the viewer first is better. A copy opened a second time is a new copy.
- **Saving a copy** (desktop only: the phone has no file dialogs) goes through the same dialog module as exports (`commands::ask`), is written owner-only, never over an existing file, and carries the same warning as an export: the copy is not encrypted.

## 7. Adding, linking, deleting

**Adding** (desktop and phone, the same Rust function):

1. The page lets the person pick a file with a plain file field (on the phone, with `capture` for the camera; section 15 checks what the web view does) and reads it with `File.arrayBuffer()`. It sends it to Rust **in pieces of text**: `document_upload_begin` says what to call it, which date, which links and how many bytes; then `document_upload_chunk` carries the pieces as base64url (256 KiB of file each, at most 512 KiB of text), each starting where the last ended; `document_upload_finish` stores it. A request with raw bytes would have been simpler, but Tauri does not support one on Android, and the phone is where photos come from; text works on both. The pieces are put together **in memory only**: a half-sent file is never written to the disk in the clear. There is one upload at a time, a new one drops the old, and locking the app drops it too. What is wrong with the description (a date, a link, the size) is said at `begin`, before a byte is sent; a piece out of order or past the announced size ends the upload. A size over 25 MiB is refused in the page before reading and again in Rust. Drag and drop onto the documents page and the attach area does the same on the desktop. No file dialog plugin and no path ever reaches Rust, so there is no path to get wrong.
2. Rust cleans the name, decides the type, hashes the bytes, and inside one `with_conn` call writes the sealed file and inserts the row (and the links, when it was asked to link). Because the file and the row are written in the same call, nothing else can look at the folder in between.
3. If a document with the same SHA-256 exists, nothing is stored: the answer says "already in the library" with that document, and the links asked for go to the existing one. If this device has lost the file of that document, adding the same file again puts it back. A duplicate made on two devices while apart is kept as two documents (like transactions added on both), because a rule that merged them would add a merge case for little.

If the file is written and the row insert fails, the file is removed. If the process dies between the two, the file has no row and the check of section 8 removes it.

**Linking** adds a `document_links` row; a link that already exists is not an error, it is the same link. **Unlinking** deletes the row and leaves a marker. A transaction or a recurring item that is deleted removes its links by the foreign key; a document is never touched.

**Deleting a document** asks for a confirmation that names it and says how many links it has. It deletes the row (the links go with it, by the foreign key) and then the sealed file. Order matters in a crash: the row goes first inside the transaction, the file after the commit; a file without a row is cleaned up by the check, a row without a file is the worse state and is the one avoided.

**The attach area** in the transaction and recurring dialogs lists the linked documents (open, unlink), has "add file" (and "take photo" on the phone), and "link an existing document" (a search box over the library). In the dialog that creates a transaction, files picked before saving are held in the page and added and linked after the transaction exists; if one fails, the transaction stays and the area says which file failed.

**Errors the screens get** (`AppError`, three new fixed codes, with a text in each of the seven languages): `too_large` (over 25 MiB; the API answers 413 as it does for a body that is too large), `file_missing` (this device has no file for the document; the API answers 404) and `file_damaged` (it did not open, or is not what its row says; the API answers 500 `internal`). A refused add leaves no row, no link and no file.

## 8. Rows and files that disagree

A check runs after every unlock, after a restore, and after a sync:

- A file in `documents/` whose name no row derives (leftovers, a document deleted on another device) is removed. The `.tmp` files are removed; `.part` files are kept when their row exists and removed when it does not.
- A row whose file has a length that its size cannot give (a cut or extended file) is reported as damaged and left where it is; the check reads no file, it only looks at lengths, so it stays fast with thousands of documents. Opening a file proves the rest.
- Only things that are ours are touched: names of 32 lowercase hex characters with the extensions `.mnd`, `.tmp` and `.part`. Anything else in the folder (another name, a folder, a link) is left alone and counted.
- A row whose file is missing is counted, not repaired, and the documents page says how many ("3 documents have no file on this computer"). On the phone only rows with the switch on count.
- Nothing is ever written to a log with a name or a hash in it.

## 9. Sync

The rows travel as any rows do (section 10). The bytes travel after the rows, in the same sealed connection, as a new phase of the same conversation (`src-tauri/src/sync/files.rs`). The protocol number goes from 2 to 3, and an older app is told to update (the schema version already stops it: migration `0010` is schema 10).

**Who gets what.**

| Direction | Which files |
|---|---|
| Phone to desktop | Every file the desktop lacks and the phone holds. The desktop is the archive, so a document added on the phone, or a desktop restored from a backup without documents, is filled from the phone |
| Desktop to phone | Only files whose `keep_on_phone` is 1 after the rows have merged, and that the phone lacks |

The sender enforces the rule itself: a request from the phone for a file whose switch is off is answered with a `file` message that says `skipped`, whatever the phone says. So a damaged or hostile phone cannot pull documents it was not given. (Built as a skipped file and not as an `error` message: an `error` ends the whole sync, and one refused file is not a reason for that.) A request for a file the desktop does not know, or for a start beyond the end of a file, or for a file that no longer opens or matches its row, is answered the same way. A one-way sync (the direction chosen on the desktop) carries files only in that direction, and a request that goes against the direction ends the sync as a bad message does.

**The phase**, after `done` (the rows are applied and the two lists have been compared, or found equal, so a connection that breaks here cannot undo anything), desktop first as everywhere else. It also runs when no table differed: a file that is missing is a reason to move bytes even when every row is equal.

1. `have`: each side lists the `sync_id`s of the documents whose complete file it holds (400 to a message, the last marked; an empty list is one empty message).
2. `need`: each side lists what it wants from the other, as `[sync_id, offset]`: a document it has a row for, lacks a complete file of, that the other `have`s, and that the rules above allow (the desktop wants every such file; the phone only those with the switch on). `offset` is where a stopped transfer left off (section 4: a multiple of 65,536, from the valid prefix of `name.part`), 0 otherwise. The newest documents are asked for first. The desktop's `need` is sent before the phone makes its own.
3. For each wanted file, in the order asked, the sender sends one `file` (`id`, `offset`, `size`, or `skipped`) and then `chunk` messages (`data` as base64url of up to 24 KiB, `last`). A chunk belongs to the file announced before it, so it names nothing: the order is the whole addressing. A message stays well under the 64 KiB frame limit. The desktop sends what the phone asked for, then the phone sends what the desktop asked for. Plaintext crosses only inside the sealed session; each device seals the file again under its own key, so nothing about a file's name or key on one device matters on the other.
4. The receiver seals each 64 KiB as it gathers it into `name.part`, hashes as it goes, and at the end compares the SHA-256 with the row. On a match it renames the file into place. On a mismatch it removes the `.part` and counts the file as failed; the sync goes on with the next one and the summary shows the number. A `file` whose size or start is not what the row and the request say, a chunk that is not valid base64url, a file longer than its row, and a message that does not belong here end the sync as a bad message does. A file the other side skips, or one this device cannot write (no room, a broken folder), is counted as failed and the next one goes on; the bytes are asked for again at the next sync.
5. `stored`: the desktop tells the phone which documents it holds whole, among those the phone holds a file of and whose switch is off. A file that arrived from the phone a moment ago was checked as it arrived; every other is read again and checked against its row, so a damaged copy on the desktop is never a reason for the phone to let go of its own. The phone drops a file when its switch is off **and** the desktop named it in a `stored` of this session. Never from a list of an earlier session. So a document added on the phone stays there until the desktop really has it. A sync that goes only to the desktop does not take anything off the phone, and sends no `stored`.
6. The summary gains "documents received", "documents sent", "documents that could not be moved", "documents waiting for the next sync" and "taken off this phone".

**Budget.** A session ends after 256 MiB in all (counting the rows, the framing and base64url's third on top of the bytes). Files may use 100 MiB of file bytes, both directions together: the desktop's `need` gets what it asks up to that, the phone's gets what is left after the desktop's, and each sender also stops there on its own and answers `skipped` for what would go over. A file is asked for only if the rest of it fits; what does not fit is "waiting" (a smaller, older file after it still goes), and the next sync carries on from the valid prefix. A sync that stopped half way is repaired by the next one, as the rest of the design promises.

**A broken connection** (a timeout, a closed or failed socket, or the other device ending with an error) during this phase is not a failed sync: the rows are in, the summary says that the connection broke, and what did not move waits. Anything that goes against the rules ends the sync. The app being locked mid-way stops it with the usual "locked".

**Not done by the sync:** the open folder and the temporary files never cross; a document row with the switch off but a file the phone holds because it added it is not touched until the desktop confirms.

## 10. Merge rules for the rows

Rows follow the rules of the sync design (newer stamp wins as a whole row, a deletion is a change, the stamps of derived changes come from stamps and never from the clock). The new cases:

| Case | Rule |
|---|---|
| Document edited on both (renamed, date, switch) | The newer whole row wins. A conflict (both changed after their last sync, and they differ) goes to the review dialog like any other row; the facts shown are name, date and "keep on phone" |
| Document edited on one, deleted on the other | The newer wins (a deletion marker has a stamp). If the edit wins, the document comes back, and with it the links the editing device still holds (they travel as rows). A document deleted for good removes its file on every device that applies the marker |
| Same link added on both | Matched by `(document, target)`, like a budget by its natural key: the newer stays (on a tie the lower id), the other becomes a deletion marker. Both devices find the duplicate when the other's row arrives, and write the same marker (`loser_marker`) |
| A person unlinked it on one device | A marker, newer than the link: the other device deletes its copy |
| Document, transaction or recurring item deleted on one device while the other attaches a document to it (or the link already existed) | The parent's marker takes the link with it (the foreign key does it, silently, on the device that applies the marker). A link row that arrives for a parent this device knows as deleted is **not taken and is not an error**; the other device, which still holds the link, is sent the deletion of the parent in the same sync and drops the link with it. So one sync ends them the same, with no marker for the link on either |
| Document unlinked and linked again | The old link's marker and the new link's row have different random ids; the new one is not touched by the old marker |
| A document row with other size, kind or hash than the one held under that id | Refused, and the sync stops with nothing applied: the contents of a document never change |
| A link row with another document or another target than the one held under that id | Refused, the same way |
| Two documents with the same hash | Both kept (section 7) |

Applying: the order becomes settings, categories, recurring items, budgets, transactions, **documents, document links**, then deletion markers (transactions, budgets, recurring items, **links, documents**: a link marker before the marker of its document, though either order ends the same). A link refers to its document and its target by `sync_id`; a reference to an id that is unknown and not a known deletion stops the sync, as the sync design says for every row. The files named by a deleted document are removed after the transaction commits (the check of section 8 covers a crash in between).

Row checks on arrival (the same as the screens' rules): the name rule of section 6, a closed media type, size 1 to 26,214,400, 64 hex characters of hash, a real date, a switch of 0 or 1, a link with exactly one target. A row that fails ends the sync with the table named and never the content.

`MAX_ROWS_PER_TABLE`, the 400-entry digest messages, the table hashes and the conflict limit of 200 work unchanged for the two new tables.

## 11. Backup, restore and "Delete all my data"

**Backup.** The person chooses "include documents" in the backup dialog (off by default would hide files from people who think they are covered; the dialog says the size of the library next to the box, and on by default when it is under 100 MB). Scheduled backups remember the choice in `backup_schedule` and the newest 10 are kept as now, so the dialog also shows the space this takes. A backup stays one encrypted file.

The current format (version 1) is a single sealed JSON blob held in memory, which cannot carry gigabytes. Version 2 keeps the header and the Argon2id key stretching, and replaces the single seal with a STREAM of 64 KiB chunks of the same kind as section 4. The plaintext stream is a list of records, each with a one-byte tag and a length:

```
S  the snapshot (the JSON of today's backup)           once, first
F  a file: sync_id (16) || size (8) || sha256 (32) || the bytes     0 to many
E  the end                                              once, last
```

A backup made without documents has only `S` and `E`. Version 1 files still open. Writing is streamed to a temporary file next to the target and renamed; reading is streamed from the file, never the whole of it in memory. The file-size ceiling of 512 MiB is replaced by a per-record ceiling (the snapshot at most 512 MiB as now, a file at most 25 MiB) and a total chosen by what the disk holds. The truncation of a stream is caught by the last-chunk flag, as for a file.

**Restore** (in a running vault, or at first run from a backup): the snapshot is checked first, in the scratch database, as today. Then:

1. Each `F` record is hashed as it is read and sealed under the target vault's `docs_root` into `documents/`, with the name derived from its `sync_id`. Because a `sync_id` always means the same bytes, writing a file that is already there is harmless.
2. The database is replaced in one transaction.
3. The check of section 8 removes the files that no row of the new data names.

The order is the crash-safe one: a crash before step 2 leaves extra files with no row (removed by the check) and the old data intact; a crash after it leaves old files with no row (removed). A backup without documents restores the rows, and the documents show "missing" until the phone returns the files it holds (section 9). A restore turns on `applying` as it does today and gives the device a new device id and no pairings.

**As built** (batch 8). The format is as described, with these details. The header is the old one (54 bytes) with format 2; its nonce field is random filler in version 2, because every chunk has a nonce of its own (the first byte is the last-chunk flag, the last eight the chunk number, as in section 4), and the whole header is authenticated with every chunk. The writer holds one chunk back until it knows whether more follows, because a file that cannot be read is left out and the total is not known in advance. The reader knows the last chunk from the length of the file. A wrong passphrase fails the first chunk, which cannot be told from a damaged start (`WrongCredentials`); a chunk that fails after the first opened was reached with the right key, so it is damage (`InvalidBackup`). An empty chunk, a chunk that is short and not last, and anything after the end record are refused. The rows (`S`) are held in memory (at most 512 MiB, read as they arrive so a length that lies cannot ask for memory the file does not hold); a version 1 file is still read whole and still has the 512 MiB ceiling; a version 2 file has none. An old backup (version 1) restores as before, with no files.

*Writing.* The person chooses "include documents" in the dialog of a manual backup and in the form of a scheduled one (the choice is kept in `backup_schedule.include_documents`); the box shows how many files and how many bytes this device holds, and starts on while that is under 100 MB. The scheduled form also says what the newest 10 backups can take (ten times the library). A manual backup asks where to save first, and reads the rows after that, so it is of the moment it is written. The files are read one at a time, each under a short hold of the vault, so the app stays usable and a lock in the middle ends the backup with nothing left (the temporary file is removed and an older file under the name is untouched). A document whose file is not here, or does not open and match its row, is left out and counted; the dialog says how many documents the backup holds and how many were left out. The phone makes no backups. The command line's `backup.create` still writes the rows only; its approval says so (documents on the command line are batch 10).

*Restoring.* As designed: the rows are checked in the scratch database first, then the `F` records are read from the stream and sealed into the folder under the target vault's own key, and only when the stream has ended whole are the rows replaced in one transaction; then the check of the folder runs, whatever the outcome. Details the design left open: a file is taken only if a row of the checked snapshot names it with the same size and hash, and only once (a hostile or damaged backup cannot fill the disk with files no row wants); every file is written from its first byte, never on top of what an earlier stopped transfer left; a file that fails (a cut stream, a wrong hash) is removed at once; the vault holds its state for the whole restore, so nothing else touches the data meanwhile. A restore as a new install that fails anywhere removes what it made and leaves the app not set up.

**Delete all my data** and the phone's erase after 5 wrong passphrases remove `documents/` and `open/` in `remove_everything`, the one place that already removes the database. Without the key they could not be read anyway; removing them is for the disk, not the secrecy.

## 12. Export

JSON, CSV and Excel list the documents (name, date, type, size, hash, the names of what they are linked to) and never contain a file; the PDF report is unchanged. A limited export (chosen categories) lists only the documents linked to what is in it. The export page says that the files are not in an export, and points at the backup.

**As built** (batch 9). The functions that write an export take only the database, never the folder or the keys, so a file cannot get into one: there is nothing to read it with. *JSON:* a `documents` list (id as a string, `name`, `date`, `type`, `sizeBytes`, `sha256`, `links`), oldest paper first; each link is `kind` (`transaction` or `recurring`), `id`, `name` and `date` (the payment date; none for a recurring item). The file's `version` stays 1: the list is an addition. *CSV:* the transactions file gets a last column, `documents`, with the names of the documents attached to that transaction, in the order of the names and separated by `; ` (a name that holds a `; ` itself cannot be told apart in one cell; the JSON lists them one by one). A CSV is only transactions, so a document attached to nothing, or only to a recurring item, is not in it. *Excel:* a sixth sheet, `Documents` (name, a real date, type, size in bytes, hash, and an "Attached to" cell: `Name (date)` for a transaction, `Name (recurring)` for a recurring item, separated by `; `), and a `Documents` column in the Transactions sheet. Every cell that holds a name goes through `neutralise`, as all text does. *What a selection keeps:* with no limits (no dates, all categories) every document is listed, attached or not. With any limit a document is listed only if it is attached to a transaction that is in the export (date and category) or to a recurring item that is (a category only: dates do not limit the recurring list), and only those attachments are listed, so the name of a document that hangs on something left out is not given away, even when it also hangs on something that is in. *The screens:* the receipt on the export page has a documents line for JSON and Excel and a note that they are listed by name, never the files (for the CSV, a note about the column); the preview counts what the JSON would list. *The command line:* the dialog that asks the person to allow an export says how many documents it lists (or, for a CSV, that a column names them), and that the files are never in it.

## 13. CLI and API

The command API stays on the loopback address and every route needs a token.

| Route | Token | What |
|---|---|---|
| `documents list`, `documents show` | read | Names, dates, types, sizes, hashes and links. Never contents. A read-only token can do nothing more with documents |
| `documents add <path>` | write | The CLI reads the file itself (it runs as the person, with the person's rights) and sends the bytes in the body of one request. The app never opens a path on a token's behalf. Needs a bigger body limit (today 1 MiB) for this one route, 25 MiB plus a little, still with a `Content-Length` and still read in pieces |
| `documents link`, `unlink`, `rename`, `delete`, `keep` (the phone switch) | write | The same rules as the screens |
| `documents get <id> --out <path>` | write | The app shows a dialog (the token's name, the document, its size, the exact file) and writes the file only on Allow, new file only and owner-only, through the module that exports already use. No answer in two minutes is a no |

The schema (`meowny schema`) and `docs/CLI.md` get the new commands in the same change as the code, as the project rule says.

**As built** (batch 10). Eight routes in `src-tauri/src/api/documents.rs`: `documents.list` and `documents.show` (read), and `documents.add`, `documents.update` (name, date and the phone switch together, at least one), `documents.delete`, `documents.link`, `documents.unlink` and `documents.get` (write; `get` also needs the person's yes). The design had `rename` and `keep` as commands of their own; they are one `update`, like `transactions.update`. Answers are flat records, as everywhere: `show` is the document with its `links`, `add` is the document and `alreadyThere`.

*The upload.* The file is the argument `data` of `documents.add`, base64url text without padding, a new kind of argument (`bytes`) that only this route has. The CLI reads the file itself (`meowny documents add <file>`, up to 25 MiB, a regular file, not empty), keeps it under the name of the file unless `--name` is given, and encodes it with a small encoder of its own (the CLI has no dependency on the app). The request body of that route may be up to 35 MiB; every other route still takes 1 MiB. **How the bigger body is let in:** after the head of a request is read and before a byte of its body, `admit_large` makes the first checks `handle` makes (the host, no origin, POST, the route is one that takes a big body and this one is not bigger than it allows, a bearer token that is known, and a scope that allows the route). A caller that would be refused is refused from the head alone, so a body of 30 MiB is never read for a request with no token (tests send only the head and see the answer at once); a locked app cannot check a token, so it does not let a big body in either; and a body over 1 MiB for any other route is a 413 as before.

*Getting a file out.* `documents.get` needs a write token **and** the person's yes (a read token is refused before anything is asked). The dialog names the token, the document (name, kind, size, date) and the exact file, and says the copy is not encrypted. The name of the copy must end in the extension of the kind of file the app found (`.pdf`, `.png`, `.jpg`, `.gif`, `.webp`, `.txt`, `.html`, `.csv`, and `.bin` for a file of no known kind), never one the caller picks, because the same module is what a token uses to write exports and a name like `start.desktop` or `run.sh` would be a way to make a system run something. The file is created new, owner-only, in one step with the check that it is not there (`files::write_new`), so nothing is replaced. A document with no file on this computer answers `not_found` before the person is asked; one whose file no longer opens is asked about, answered with `internal` after the yes, and writes nothing.

*Errors:* the existing codes only (`invalid_input`, `not_found`, `too_large`, `file_exists`, `forbidden`, `denied`, `locked`, `internal`). *Documentation:* `cli/schema.json`, the recorded examples (`docs/site/examples.json`) and `docs/CLI.md` were regenerated, and the examples include adding a file, listing, showing, changing, attaching, getting a copy and deleting, and the two ways it goes wrong most often (a file that cannot be read, a name for a copy that a system could run).

## 14. The screens

- **Documents page** (sidebar on the desktop): a list with name, date, type, size and the number of links; a search box for the name; a filter for type (images, PDFs, other); no filter by date (the list is newest paper first, and the search finds the rest); the total size of the library, and on the phone how much it holds; the count of documents with no file. Click a document: open (or show, for an image), the links with a button to go to each transaction, rename, change the date, the "keep on phone" switch, save a copy (desktop), delete. "Add file" and drag and drop. A phone shows the page from the top bar (the five tabs do not change) and puts the switch and the delete inside the document, not in the list.
- **Attach area** in the transaction and recurring dialogs (section 7).
- **A mark** (a paper-clip and the count) on a transaction or recurring item that has documents, in the lists, on both apps.
- All text in the seven languages, with the usual test that fails when a language lacks a key. Sizes are written with the number format of the settings. No plural forms: counts are "label: number" as in the sync summary.
- Mobile-first rule: tap targets of 44 px, inputs of 16 px, the safe-area insets.

**As built** (batch 5): the page is `documents` in the sidebar (shortcut `g o`) and, on the phone, a fourth icon in the top bar beside sync, settings and lock (the five tabs did not change). It lists newest paper first, 50 to a page, and has a search, a type choice (all, images, pdf files, other files; no date filter), "add file" (and "take photo" on a phone), drag and drop on a computer (the window now has `dragDropEnabled: false`, which Windows needs for the page to receive dropped files), a line with how many documents there are and how big (the phone adds what it holds), and one warning line each for files that are missing (on the phone: "waiting for the next sync", only the ones it was asked to keep) and damaged. A card has details and delete (with a question that names the file). **Details** is one dialog: the picture if the app may draw it, name, date, the "keep on phone" switch (all three saved with "save", and nothing is asked of the app when nothing changed), the kind and size, what it is linked to (each link can be detached), where the file is when it is not here, and open, save a copy (not on a phone) and delete. The first open of a session says what a plain copy is and asks; a file of an unknown kind has no open button and says so. **The attach area** is in the dialogs of a transaction and a recurring item and, in a short form (the buttons and the list), in the phone's quick add, where a photo of the receipt is most likely taken. On an item that exists a file is stored and linked at once; on one being made the files wait, and are sent after it is saved. If one fails the item is still saved and the notice names the file and the reason. "Attach one from the library" is a search with an attach button for each result. The paper clip with a number is on transactions and recurring items; it opens the item, where the documents are listed. The counts come from one question per page of the list (`document_counts`), and the page asks again after the dialog closes. 48 texts in all seven languages (counts are "label: number", so no language needs plural forms).

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

- The spike (section 15).
- ~~Whether the backup dialog should default "include documents" on or off.~~ On while the library this device holds is under 100 MB, off above it, as proposed; the person can change it either way (section 11, as built).
- Thumbnails: none in the first version; a list of images that decrypts and draws each one would be slow. A later thumbnail would be a second sealed file per image.
- Windows: removal of an open copy while a viewer holds it (section 6) cannot be tested here; the behaviour is described, not seen.
- ~~Where the phone's documents page lives.~~ In the top bar, as an icon. It fits at 390 px in a browser; to be looked at on the real phone.
- A total size limit or a warning for the phone (the phone shows what it holds; it does not stop).
- A review by someone else of the sealed format and the file transfer, before release, like the sync (the same document is the thing to hand over).

## 17. Tests

- **Sealed format**: known-answer vectors from `scripts/doc-vectors.py`, a second implementation that takes the ChaCha20-Poly1305 and HKDF written from RFC 8439 and RFC 5869 in `sync-vectors.py` (which checks them against the RFCs' own vectors) and builds the keys, the names and the files on top of them, that prints the keys and the bytes of small files (empty, 1 byte, exactly one chunk, one chunk and a byte, three chunks); a changed byte in the header or in any chunk, a cut file (at every chunk boundary and inside a chunk), an appended chunk, swapped chunks, a chunk of another file, a file under another document's name: all refused; the largest file; round trips for every size from 0 to 3 chunks and a few random ones; the key is dropped when the vault locks.
- **Names and types**: a list of hostile names (path parts, control and direction characters, long names, reserved Windows names, only dots), checked for what is stored and for `disk_name()`; the media type from bytes against lying extensions; an image whose header claims a huge size is not drawn.
- **Rows**: constraint tests for both tables (one target, uniqueness, foreign keys, the check on size and hash), the triggers (stamps, markers, audit), a deleted transaction removes its links and no document, a deleted document removes its links and its file.
- **Agreement**: a file without a row is removed, a row without a file is counted, `.tmp` and old `.part` files are cleaned, the check is quiet when everything agrees.
- **Sync, rows**: every case of section 10 has a test of its own; random edits of documents and links on two devices from a fixed seed converge (the existing test extended), and a second sync finds nothing.
- **Sync, bytes**: a document with the switch on crosses and one with it off never does (a marker in a file and a search of everything that crossed, as for the secrets); the phone's new document reaches the desktop and is dropped from the phone only after the desktop confirms; a transfer stopped at every chunk resumes from the valid prefix; a changed byte in a chunk, a wrong hash, a chunk out of order and an offset nobody asked for are refused; a request for a file the sender may not give is answered with an error; the byte budget leaves the rest waiting.
- **Backup**: with and without documents; a restore into a vault with another key opens every file; a cut or changed backup is refused and leaves the app as it was; a version 1 backup still opens; a crash simulated between the steps of section 11 is repaired by the check.
- **Wipe and lock**: after a lock the open folder is empty; after "delete all my data" neither folder exists.
- **API**: a read token sees no contents; an upload over 25 MiB is refused before it is read; `get` writes only after Allow and never over a file.
- **Screens**: the attach area and the documents page looked at in a browser at desktop and phone width with the commands faked; the i18n key test.
- **By hand**: the real phone, with a photo and a PDF, as in section 15 and then the whole of the "done when" of the plan.
