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
- **Deleting a link by cascade leaves a marker too** (deleting a transaction, a recurring item or a document removes its links through the foreign key, and the `DELETE` trigger of `document_links` fires for those rows), so the other device hears of them even if it never hears of the thing they were linked to.

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
- **Images in the app.** Only the four image types, after two checks: the size from the header of the format (width times height at most 64 megapixels, which stops a small file that expands to gigabytes) and the real signature. The bytes go to the page as a `Blob` and are drawn with an `<img>`, never as an SVG, never in a frame, never with a link. The CSP gets `blob:` added to `img-src` and nothing else, so a page cannot load anything from the network that way. (To confirm in the spike.)
- **Everything else that has a kind** (PDF, HTML, text, CSV, and an image that is too big to draw) is written to the open folder and handed to the system's own viewer (`tauri-plugin-opener`, called from Rust so the page cannot name a path). **The copy's extension comes from the kind the app found, never from the name** (`open_name`: `invoice.pdf.exe` found to be a PDF is opened as `invoice.pdf.pdf`, `holiday.bat` found to be a PNG as `holiday.png`). Otherwise "open" would run a program, since the system picks what to start by the extension and the extension is the one thing the sender chose. A file of the kind `application/octet-stream` is **not opened from the app at all**: it can only be saved as a copy, and what happens to it afterwards is the person's own doing. The screen says, before the first open of a session, that the copy is not encrypted while it is open and that an HTML file may load things from the internet when the system opens it.
- **The open folder** is `open/` in the data folder, owner-only, one sub-folder with a random name per open, holding the file under its `open_name()`. It is emptied when the app locks (idle lock, the lock button, lock on minimise or close), when it exits, when it starts (a crash leaves files behind), and on "delete all my data". On Windows a viewer that still holds the file open blocks its removal; the file is then removed at the next start, and the screen warns that closing the viewer first is better. A copy opened a second time is a new copy.
- **Saving a copy** (desktop only: the phone has no file dialogs) goes through the same dialog module as exports (`commands::ask`), is written owner-only, never over an existing file, and carries the same warning as an export: the copy is not encrypted.

## 7. Adding, linking, deleting

**Adding** (desktop and phone, the same Rust function):

1. The page lets the person pick a file with a plain file field (on the phone, with `capture` for the camera; section 15 checks what the web view does), reads it with `File.arrayBuffer()`, and sends the bytes to a command as a raw body (no JSON, no base64). A size over 25 MiB is refused in the page before reading and again in Rust. Drag and drop onto the documents page and the attach area does the same on the desktop. No file dialog plugin and no path ever reaches Rust, so there is no path to get wrong.
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

The rows travel as any rows do (section 10). The bytes travel after the rows, in the same sealed connection, as a new phase of the same conversation. The protocol number goes from 2 to 3, and an older app is told to update (the schema version already stops it: migration `0010` is schema 10).

**Who gets what.**

| Direction | Which files |
|---|---|
| Phone to desktop | Every file the desktop lacks and the phone holds. The desktop is the archive, so a document added on the phone, or a desktop restored from a backup without documents, is filled from the phone |
| Desktop to phone | Only files whose `keep_on_phone` is 1 after the rows have merged, and that the phone lacks |

The sender enforces the rule itself: a request from the phone for a file whose switch is off is answered with `error`, whatever the phone says. So a damaged or hostile phone cannot pull documents it was not given. A one-way sync (the direction chosen on the desktop) carries files only in that direction.

**The phase**, after `rows` are applied and before `done`, desktop first as everywhere else:

1. `have`: each side lists the `sync_id`s of the documents whose complete file it holds (400 to a message, the last marked).
2. `need`: each side lists what it wants from the other, as `[sync_id, offset]`: a document it has a row for, lacks a complete file of, that the other `have`s, and that the rules above allow. `offset` is where a stopped transfer left off (section 4: a multiple of 65,536, from the valid prefix of `name.part`), 0 otherwise.
3. For each wanted file, in order, the sender sends `file` (`sync_id`, `offset`, `size`) and then `chunk` messages (`sync_id`, `n`, `data` as base64url of up to 24 KiB, `last`). A message stays under the 40 KiB soft limit and the 64 KiB frame limit. Plaintext crosses only inside the sealed session; each device seals the file again under its own key, so nothing about a file's name or key on one device matters on the other.
4. The receiver seals each 64 KiB as it gathers it into `name.part`, hashes as it goes, and at the end compares the SHA-256 and the size with the row. On a match it renames the file into place. On a mismatch it removes the `.part` and counts the file as failed; the sync goes on with the next one and the summary shows the number. A wrong sequence (a chunk for a file not announced, a size over the row's, an offset not asked for) ends the sync as a bad message does today.
5. `stored`: the desktop tells the phone which documents it now holds. The phone drops a file when its switch is off **and** the desktop named it in a `have` or `stored` of this session. Never from a list of an earlier session. So a document added on the phone stays there until the desktop really has it.
6. The summary gains "documents sent", "documents received", "documents failed", "documents waiting" and "removed from the phone".

**Budget.** A session ends after 256 MiB in all today. A file is started only if all of it fits in what is left; the rest are "waiting" and the next sync, which compares `have` lists again, carries on from the valid prefix. A sync that stopped half way is repaired by the next one, as the rest of the design promises.

**Not done by the sync:** the open folder and the temporary files never cross; a document row with the switch off but a file the phone holds because it added it is not touched until the desktop confirms.

## 10. Merge rules for the rows

Rows follow the rules of the sync design (newer stamp wins as a whole row, a deletion is a change, the stamps of derived changes come from stamps and never from the clock). The new cases:

| Case | Rule |
|---|---|
| Document edited on both (renamed, date, switch) | The newer whole row wins. A conflict (both changed after their last sync, and they differ) goes to the review dialog like any other row; the facts shown are name, date, type and "keep on phone" |
| Document edited on one, deleted on the other | The newer wins (a deletion marker has a stamp). A document deleted for good removes its file on every device that applies the marker |
| Same link added on both | Matched by `(document, target)`, like a budget by its natural key: the newer stays, the other becomes a deletion marker with `after(stamp)` |
| Link removed on one, document or target deleted on the other | Removed. A link row that arrives for a document or a target that this device already knows as deleted is not applied; it gets a marker with `after(stamp)` so both end the same |
| Link added on one device for a transaction that the other deleted | Same: the link is dropped, the transaction stays deleted |
| Document unlinked and linked again | The old link's marker and the new link's row have different random ids; the new one is not touched by the old marker |
| Two documents with the same hash | Both kept (section 7) |

Applying: the order becomes settings, categories, recurring items, budgets, transactions, **documents, document links**, then deletion markers. A link refers to its document and its target by `sync_id`; a reference to an id that is unknown and not a known deletion stops the sync, as the sync design says for every row. The files named by a deleted document are removed after the transaction commits (the check of section 8 covers a crash in between).

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

**Delete all my data** and the phone's erase after 5 wrong passphrases remove `documents/` and `open/` in `remove_everything`, the one place that already removes the database. Without the key they could not be read anyway; removing them is for the disk, not the secrecy.

## 12. Export

JSON, CSV and Excel list the documents (name, date, type, size, hash, the names of what they are linked to) and never contain a file; the PDF report is unchanged. A limited export (chosen categories) lists only the documents linked to what is in it. The export page says that the files are not in an export, and points at the backup.

## 13. CLI and API

The command API stays on the loopback address and every route needs a token.

| Route | Token | What |
|---|---|---|
| `documents list`, `documents show` | read | Names, dates, types, sizes, hashes and links. Never contents. A read-only token can do nothing more with documents |
| `documents add <path>` | write | The CLI reads the file itself (it runs as the person, with the person's rights) and sends the bytes in the body of one request. The app never opens a path on a token's behalf. Needs a bigger body limit (today 1 MiB) for this one route, 25 MiB plus a little, still with a `Content-Length` and still read in pieces |
| `documents link`, `unlink`, `rename`, `delete`, `keep` (the phone switch) | write | The same rules as the screens |
| `documents get <id> --out <path>` | write | The app shows a dialog (the token's name, the document, its size, the exact file) and writes the file only on Allow, new file only and owner-only, through the module that exports already use. No answer in two minutes is a no |

The schema (`meowny schema`) and `docs/CLI.md` get the new commands in the same change as the code, as the project rule says.

## 14. The screens

- **Documents page** (sidebar on the desktop): a list with name, date, type, size and the number of links; a search box for the name; filters for type (images, PDFs, other) and for date (from, to); the total size of the library, and on the phone how much it holds; the count of documents with no file. Click a document: open (or show, for an image), the links with a button to go to each transaction, rename, change the date, the "keep on phone" switch, save a copy (desktop), delete. "Add file" and drag and drop. A phone shows the page from the top bar (the five tabs do not change) and puts the switch and the delete inside the document, not in the list.
- **Attach area** in the transaction and recurring dialogs (section 7).
- **A mark** (a paper-clip and the count) on a transaction or recurring item that has documents, in the lists, on both apps.
- All text in the seven languages, with the usual test that fails when a language lacks a key. Sizes are written with the number format of the settings. No plural forms: counts are "label: number" as in the sync summary.
- Mobile-first rule: tap targets of 44 px, inputs of 16 px, the safe-area insets.

## 15. The spike on a real phone

Before the screens, one small build on the Redmi Note 11, to settle what the web view can do without a plugin:

1. A file field with `accept="image/*,application/pdf"`: does Android's chooser open, and can the page read the chosen file with `arrayBuffer()`?
2. The same field with `capture="environment"`: does the camera open, does it need the `CAMERA` permission from the app (the barcode scanner plugin may already bring it; not checked), and is the photo readable?
3. Can a 25 MiB body be passed to a command as a raw request body without the app running out of memory?
4. Can an image be drawn from a `Blob` URL, and does the CSP need `blob:` (and nothing more)?
5. Can the system open a PDF from the app's folder (the opener plugin on Android may need a `FileProvider` that the generated project does not have)?

The answers go into this file. If 1 or 2 fail, a small plugin or a patch in `scripts/android-patch.py` takes their place, and that is a dependency decision to make with the maintainer. If 5 fails, the phone shows images in the app and says "open this on your computer" for the rest, which is acceptable for a first version.

## 16. Open points

- The spike (section 15).
- Whether the first backup dialog should default "include documents" on or off (section 11 proposes on under 100 MB).
- Thumbnails: none in the first version; a list of images that decrypts and draws each one would be slow. A later thumbnail would be a second sealed file per image.
- Windows: removal of an open copy while a viewer holds it (section 6) cannot be tested here; the behaviour is described, not seen.
- Where the phone's documents page lives (top bar button is proposed; it needs looking at on the phone).
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
